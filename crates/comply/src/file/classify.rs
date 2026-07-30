// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::Path;

use super::FileType;
use crate::config;

/// Check if the file extension is a known programming language source file.
fn is_source_extension(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "c"
            | "cpp"
            | "cc"
            | "cxx"
            | "h"
            | "hpp"
            | "hxx"
            | "java"
            | "kt"
            | "kts"
            | "py"
            | "pyx"
            | "pxd"
            | "pxi"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "go"
            | "rb"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "swift"
            | "scala"
            | "sc"
            | "zig"
            | "dart"
            | "cs"
            | "fs"
            | "fsx"
            | "fsi"
            | "erl"
            | "hrl"
            | "ex"
            | "exs"
            | "hs"
            | "lhs"
            | "lua"
            | "pl"
            | "pm"
            | "lisp"
            | "clj"
            | "cljs"
            | "cljc"
            | "pas"
            | "pp"
            | "r"
            | "rkt"
            | "ml"
            | "mli"
            | "v"
            | "vh"
            | "sv"
            | "vhd"
            | "vhdl"
    )
}

/// Check if the file name is a well-known source file without extension.
fn is_well_known_source_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "makefile"
            | "dockerfile"
            | "gemfile"
            | "rakefile"
            | "cargo.lock"
            | "build.gradle"
            | "settings.gradle"
            | "gradle.properties"
            | "pom.xml"
    )
}

/// Check if the file extension is a binary format.
fn is_binary_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "ico"
            | "svg"
            | "pdf"
            | "zip"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
            | "o"
            | "so"
            | "dll"
            | "dylib"
            | "exe"
            | "bin"
            | "class"
            | "jar"
            | "ttf"
            | "otf"
            | "woff"
            | "woff2"
            | "icns"
    )
}

/// Check if the path contains ignored directory components.
fn has_ignored_component(path: &Path) -> bool {
    path.components()
        .any(|c| crate::scope::IGNORED_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
}

/// Classify a file for REUSE analysis.
///
/// REUSE covers every file in the project, so the default is [`FileType::Source`]
/// and only three things fall outside it: paths under a VCS or build directory,
/// the license texts themselves, and formats with no comment syntax -- which are
/// [`FileType::Binary`], meaning their licensing belongs in a `.license` companion.
pub(super) fn classify(path: &Path, source_patterns: &[String]) -> FileType {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().to_string());

    if has_ignored_component(path) {
        return FileType::Ignored;
    }

    // An explicit `source-patterns` entry in REUSE.toml outranks every default
    // below, so a project can force a file to be treated as commentable source.
    let rel_path = path
        .strip_prefix(std::env::current_dir().unwrap_or_default())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    if source_patterns
        .iter()
        .any(|p| config::glob_match(p, &rel_path))
    {
        return FileType::Source;
    }

    let ext = name
        .split('.')
        .next_back()
        .unwrap_or("")
        .to_ascii_lowercase();

    if is_source_extension(&ext) || is_well_known_source_name(&name) {
        return FileType::Source;
    }

    if is_binary_extension(&ext) || is_uncommentable_text_extension(&ext) {
        return FileType::Binary;
    }

    if is_license_text_name(&name) {
        return FileType::Ignored;
    }

    FileType::Source
}

/// Text formats that carry no comment syntax comply can emit, so their
/// licensing has to live in a `.license` companion rather than a header.
fn is_uncommentable_text_extension(ext: &str) -> bool {
    matches!(ext, "json")
}

/// The project's own license texts, which carry no licensing of their own.
///
/// Matches `LICENSE`, `LICENCE`, `COPYING` and suffixed variants such as
/// `LICENSE-APACHE` or `COPYING.LESSER`, while leaving names that merely
/// mention licensing (`licensing.md`, `LICENSES.md`) covered.
fn is_license_text_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_lowercase();
    matches!(stem.as_str(), "license" | "licence" | "copying")
        || stem.starts_with("license-")
        || stem.starts_with("licence-")
}

/// Check if a path should be ignored for REUSE analysis.
#[must_use]
pub fn is_ignored(path: &Path) -> bool {
    path.components()
        .any(|c| crate::scope::is_pruned_dir(c.as_os_str().to_string_lossy().as_ref()))
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_rust_source() {
        let p = Path::new("src/lib.rs");
        assert_eq!(classify(p, &[]), FileType::Source);
    }

    #[test]
    fn a_custom_source_pattern_overrides_the_default_classification() {
        let p = Path::new("assets/logo.png");

        assert_eq!(
            classify(p, &[]),
            FileType::Binary,
            "a png needs a companion by default"
        );
        assert_eq!(
            classify(p, &["**/*.png".to_string()]),
            FileType::Source,
            "source-patterns from REUSE.toml force a file to be treated as source"
        );
        assert_eq!(
            classify(p, &["**/*.other".to_string()]),
            FileType::Binary,
            "a pattern that does not match changes nothing"
        );
    }

    #[test]
    fn anything_under_a_vcs_or_build_directory_is_ignored() {
        for p in ["target/debug/build.rs", "node_modules/pkg/index.js"] {
            assert_eq!(
                classify(Path::new(p), &[]),
                FileType::Ignored,
                "{p} carries no licensing obligation"
            );
        }
    }

    #[test]
    fn classify_python_source() {
        let p = Path::new("main.py");
        assert_eq!(classify(p, &[]), FileType::Source);
    }

    #[test]
    fn classify_png_binary() {
        let p = Path::new("image.png");
        assert_eq!(classify(p, &[]), FileType::Binary);
    }

    #[test]
    fn classify_jpg_binary() {
        let p = Path::new("photo.jpg");
        assert_eq!(classify(p, &[]), FileType::Binary);
    }

    /// REUSE has no by-extension exemption: documentation and configuration
    /// are Covered Files and need licensing information like anything else.
    /// Expectations here match the observed behaviour of `reuse` 6.2.0.
    #[test]
    fn documentation_and_configuration_are_covered() {
        for p in [
            "README.md",
            "CHANGELOG.md",
            "notes.txt",
            "Cargo.toml",
            "config.yaml",
            "settings.ini",
            "schema.sql",
            "page.html",
            ".gitignore",
            "Cargo.lock",
            "SECURITY",
            "NOTICE",
            "AUTHORS",
        ] {
            assert_eq!(classify(Path::new(p), &[]), FileType::Source, "{p}");
        }
    }

    #[test]
    fn formats_with_no_comment_syntax_need_a_companion() {
        assert_eq!(
            classify(Path::new("data.json"), &[]),
            FileType::Binary,
            "JSON cannot carry a comment, so licensing goes in a .license file"
        );
    }

    #[test]
    fn stylesheets_and_rst_carry_their_own_header() {
        for p in [
            "style.css",
            "theme.scss",
            "vars.sass",
            "mixins.less",
            "guide.rst",
        ] {
            assert_eq!(
                classify(Path::new(p), &[]),
                FileType::Source,
                "{p} has a comment syntax comply can write"
            );
        }
    }

    #[test]
    fn the_licence_texts_themselves_are_not_covered() {
        for p in [
            "LICENSE",
            "LICENSE-APACHE",
            "LICENSE-MIT",
            "LICENSE.txt",
            "LICENCE",
            "COPYING",
            "COPYING.LESSER",
        ] {
            assert_eq!(classify(Path::new(p), &[]), FileType::Ignored, "{p}");
        }
    }

    #[test]
    fn a_name_merely_mentioning_licensing_is_still_covered() {
        for p in ["licensing.md", "LICENSES.md", "copying-guide.txt"] {
            assert_eq!(classify(Path::new(p), &[]), FileType::Source, "{p}");
        }
    }

    #[test]
    fn classify_makefile() {
        let p = Path::new("Makefile");
        assert_eq!(classify(p, &[]), FileType::Source);
    }

    #[test]
    fn classify_dockerfile() {
        let p = Path::new("Dockerfile");
        assert_eq!(classify(p, &[]), FileType::Source);
    }

    #[test]
    fn classify_unknown_extension_is_covered() {
        let p = Path::new("somefile.xyz");
        assert_eq!(
            classify(p, &[]),
            FileType::Source,
            "an unrecognised file is still a Covered File"
        );
    }

    #[test]
    fn classify_so_binary() {
        let p = Path::new("libfoo.so");
        assert_eq!(classify(p, &[]), FileType::Binary);
    }

    #[test]
    fn classify_dll_binary() {
        let p = Path::new("libfoo.dll");
        assert_eq!(classify(p, &[]), FileType::Binary);
    }

    #[test]
    fn is_ignored_git_dir() {
        let p = Path::new("/project/.git/config");
        assert!(is_ignored(p));
    }

    #[test]
    fn is_ignored_node_modules() {
        let p = Path::new("/project/node_modules/foo/index.js");
        assert!(is_ignored(p));
    }

    #[test]
    fn is_ignored_target_dir() {
        let p = Path::new("/project/target/debug/foo");
        assert!(is_ignored(p));
    }

    #[test]
    fn is_not_ignored_src_dir() {
        let p = Path::new("/project/src/lib.rs");
        assert!(!is_ignored(p));
    }

    #[test]
    fn is_ignored_licenses_dir() {
        let p = Path::new("/project/LICENSES/MIT.txt");
        assert!(is_ignored(p));
    }

    #[test]
    fn is_ignored_reuse_dir() {
        let p = Path::new("/project/.reuse/dep5");
        assert!(is_ignored(p));
    }
}
// REUSE-IgnoreEnd
