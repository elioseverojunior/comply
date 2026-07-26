// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::config;
use crate::error::Error;

mod classify;

use classify::classify;
pub use classify::is_ignored;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// Source file that can have inline SPDX headers.
    Source,
    /// Binary file that needs a `.license` companion.
    Binary,
    /// Build artifact, dependency, or ignored file.
    Ignored,
}

/// A discovered file with metadata.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Classification.
    pub file_type: FileType,
    /// Whether a `.license` companion file exists for this file.
    pub has_license_file: bool,
}

impl FileEntry {
    /// Return the file's path relative to a root directory.
    ///
    /// If the path is not under `root`, the full path is returned.
    #[must_use]
    pub fn relative_to(&self, root: &Path) -> String {
        self.path
            .strip_prefix(root)
            .unwrap_or(&self.path)
            .to_string_lossy()
            .to_string()
    }
}

/// Walk a directory and discover all files for REUSE analysis.
///
/// Respects `.gitignore`, respects custom ignore patterns from `[tool.comply]`
/// in `REUSE.toml`, and classifies each file as source, binary, or ignored.
/// The `source_patterns` parameter allows extending the default source file
/// classification with custom glob patterns from REUSE.toml.
pub fn walk(
    root: &Path,
    ignore_patterns: &[String],
    source_patterns: &[String],
) -> impl Iterator<Item = Result<FileEntry, Error>> {
    let ignore_regexes: Vec<regex::Regex> = ignore_patterns
        .iter()
        .filter_map(|p| {
            let re_str = config::glob_to_regex(p);
            regex::Regex::new(&re_str).ok()
        })
        .collect();

    let root_owned = root.to_path_buf();
    let mut builder = WalkBuilder::new(root);
    builder.hidden(false);
    builder.git_ignore(true);
    builder.git_global(true);
    builder.git_exclude(true);
    builder.require_git(false);
    builder.follow_links(false);

    builder.filter_entry(move |entry| {
        let path = entry.path();
        let file_name = path
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().to_string());

        // Skip symlinks
        if entry.file_type().is_some_and(|t| t.is_symlink()) {
            return false;
        }

        // Skip .license files (they are companions, processed via parent)
        if file_name.ends_with(".license") {
            return false;
        }

        // Skip REUSE config files
        if matches!(
            file_name.as_str(),
            "REUSE.toml" | "reuse.toml" | ".reuse.toml"
        ) {
            return false;
        }

        // Skip LICENSES/ and .reuse/ directories entirely
        if entry.file_type().is_some_and(|t| t.is_dir()) {
            if matches!(file_name.as_str(), "LICENSES" | ".reuse") {
                return false;
            }
        }

        // Skip SPDX document files
        if file_name.ends_with(".spdx.json")
            || file_name.ends_with(".spdx.xml")
            || file_name.ends_with(".spdx.yml")
            || file_name.ends_with(".spdx.yaml")
            || file_name.ends_with(".spdx.rdf")
            || Path::new(&file_name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("spdx"))
        {
            return false;
        }

        // Apply custom ignore patterns from [tool.comply]
        let rel = path
            .strip_prefix(&root_owned)
            .unwrap_or(path)
            .to_string_lossy();
        !ignore_regexes.iter().any(|re| re.is_match(&rel))
    });

    let walker = builder.build();

    walker.filter_map(|entry| match entry {
        Ok(e) if e.file_type().is_some_and(|t| t.is_file()) => {
            let path = e.into_path();

            // Skip zero-byte files
            if std::fs::metadata(&path).is_ok_and(|m| m.len() == 0) {
                return None;
            }

            let file_type = classify(&path, source_patterns);
            let has_license_file = has_license_companion(&path);
            Some(Ok(FileEntry {
                path,
                file_type,
                has_license_file,
            }))
        }
        Ok(_) => None, // skip directories
        Err(e) => Some(Err(Error::Io(std::io::Error::other(e.to_string())))),
    })
}

/// Check whether a file has a `.license` companion file.
#[must_use]
pub fn has_license_companion(path: &Path) -> bool {
    let license_path = license_companion_path(path);
    license_path.exists() && license_path.is_file()
}

/// Find the `.license` companion file for a given file.
#[must_use]
pub fn license_companion_path(path: &Path) -> PathBuf {
    let mut s = path.to_path_buf().into_os_string();
    s.push(".license");
    PathBuf::from(s)
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn companion_path_is_correct() {
        let p = Path::new("binary.bin");
        assert_eq!(
            license_companion_path(p),
            PathBuf::from("binary.bin.license")
        );
    }

    #[test]
    fn walk_skips_licenses_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("LICENSES")).unwrap();
        std::fs::write(dir.path().join("LICENSES/MIT.txt"), "MIT license text").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_to(dir.path()), "src/main.rs");
    }

    #[test]
    fn walk_skips_reuse_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".reuse")).unwrap();
        std::fs::write(
            dir.path().join(".reuse/dep5"),
            "Files: *\nCopyright: 2026\nLicense: MIT\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }

    #[test]
    fn walk_skips_reuse_toml_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("REUSE.toml"), "version = 1\n").unwrap();
        std::fs::write(dir.path().join("reuse.toml"), "version = 1\n").unwrap();
        std::fs::write(dir.path().join(".reuse.toml"), "version = 1\n").unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }

    #[test]
    fn walk_skips_license_companion_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("binary.bin"), [0u8; 4]).unwrap();
        std::fs::write(
            dir.path().join("binary.bin.license"),
            "SPDX-FileCopyrightText: 2026\nSPDX-License-Identifier: MIT\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        // binary.bin should be included, but .license companion should be skipped
        let names: Vec<String> = files.iter().map(|f| f.relative_to(dir.path())).collect();
        assert!(names.contains(&"binary.bin".to_string()));
        assert!(names.contains(&"main.rs".to_string()));
        assert!(!names.contains(&"binary.bin.license".to_string()));
    }

    #[test]
    fn walk_skips_zero_byte_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("empty.txt"), "").unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }

    #[test]
    fn walk_skips_spdx_docs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sbom.spdx.json"), "{}").unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }

    #[test]
    fn walk_honours_custom_ignore_patterns() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("vendor")).unwrap();
        std::fs::write(dir.path().join("vendor/dep.rs"), "fn dep() {}").unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &["vendor/**".to_string()], &[])
            .filter_map(Result::ok)
            .collect();

        assert_eq!(files.len(), 1, "the vendored file should be filtered out");
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }

    #[test]
    fn walk_surfaces_a_traversal_error_rather_than_swallowing_it() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such-directory");

        let errors = walk(&missing, &[], &[]).filter(Result::is_err).count();

        assert_eq!(
            errors, 1,
            "a root that cannot be read must not look like an empty project"
        );
    }

    #[cfg(unix)]
    #[test]
    fn walk_does_not_follow_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        std::os::unix::fs::symlink(dir.path().join("main.rs"), dir.path().join("alias.rs"))
            .unwrap();

        let files: Vec<FileEntry> = walk(dir.path(), &[], &[]).filter_map(Result::ok).collect();

        assert_eq!(
            files.len(),
            1,
            "a symlink would double-count the file it points at"
        );
        assert_eq!(files[0].relative_to(dir.path()), "main.rs");
    }
}
// REUSE-IgnoreEnd
