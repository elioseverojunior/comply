// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What is and is not a covered file, declared once.
//!
//! Every rule that puts a path outside REUSE analysis lives here, as data.
//! [`crate::file::walk`] applies them while descending and
//! [`crate::file::is_excluded`] applies them to a path a caller named, so the
//! two cannot disagree about the same file.
//!
//! These are public so a sibling tool can hold the same opinion without
//! copying the lists. Copies drift: three of them existed inside this crate
//! alone, and the one `classify` used had silently fallen behind the one the
//! public predicate used, so a file in a virtualenv was expected to carry an
//! SPDX header.

/// Directory names whose contents are never project files.
///
/// Version-control metadata, build output and vendored dependencies. Nothing
/// beneath them is authored by the project, so they are pruned during traversal
/// rather than walked and then discarded -- on a real repository `.git` alone
/// is hundreds of objects.
pub const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "venv",
];

/// Directories the REUSE specification puts outside the covered set.
///
/// `LICENSES/` holds the licence texts themselves and `.reuse/` holds DEP5;
/// both describe coverage rather than being covered.
pub const REUSE_DIRS: &[&str] = &["LICENSES", ".reuse"];

/// Suffix marking a companion file that carries its neighbour's licensing.
///
/// REUSE 3.3 section 2.2: an uncommentable file is licensed by `<name>.license`
/// alongside it. The companion is not judged in its own right -- the verdict
/// belongs to the file it describes.
pub const LICENSE_COMPANION_SUFFIX: &str = ".license";

/// Accepted spellings of the REUSE manifest.
///
/// The manifest declares what covers what; it is not itself covered.
pub const REUSE_MANIFEST_NAMES: &[&str] = &["REUSE.toml", "reuse.toml", ".reuse.toml"];

/// Suffixes of SPDX documents, which describe a project rather than belong to it.
pub const SPDX_DOCUMENT_SUFFIXES: &[&str] = &[
    ".spdx.json",
    ".spdx.xml",
    ".spdx.yml",
    ".spdx.yaml",
    ".spdx.rdf",
];

/// Whether a single path component names a directory never worth descending.
#[must_use]
pub fn is_pruned_dir(name: &str) -> bool {
    IGNORED_DIRS.contains(&name) || REUSE_DIRS.contains(&name)
}

/// Whether any component of `relative` names a pruned directory.
///
/// [`is_pruned_dir`] judges a single component, which is all a walk needs: it
/// stops at the directory and never sees what is inside. A path that did not
/// come from a descent -- one a user named, or one git reported as tracked --
/// has no such guarantee, so its ancestors have to be checked explicitly.
///
/// Skipping this is what let `LICENSES/MIT.txt` back into the report: the walk
/// pruned the directory, git tracked the file, and the backfill re-added it
/// without ever asking what it sat inside.
#[must_use]
pub fn is_within_pruned_dir(relative: &std::path::Path) -> bool {
    relative
        .components()
        .any(|c| is_pruned_dir(c.as_os_str().to_string_lossy().as_ref()))
}

/// Whether a file name is a REUSE manifest.
#[must_use]
pub fn is_reuse_manifest(name: &str) -> bool {
    REUSE_MANIFEST_NAMES.contains(&name)
}

/// Whether a file name is an SPDX document.
///
/// Covers both the compound suffixes and a bare `.spdx` extension, which is
/// matched case-insensitively because the extension is not a code identifier.
#[must_use]
pub fn is_spdx_document(name: &str) -> bool {
    SPDX_DOCUMENT_SUFFIXES.iter().any(|s| name.ends_with(s))
        || std::path::Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("spdx"))
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pruned_dirs_span_both_lists_and_nothing_else() {
        assert!(is_pruned_dir(".git"));
        assert!(is_pruned_dir("target"));
        assert!(is_pruned_dir(".venv"));
        assert!(is_pruned_dir("LICENSES"));
        assert!(is_pruned_dir(".reuse"));

        assert!(!is_pruned_dir("src"));
        assert!(!is_pruned_dir("gitignore"));
    }

    #[test]
    fn a_manifest_is_recognised_by_each_accepted_spelling() {
        for name in REUSE_MANIFEST_NAMES {
            assert!(is_reuse_manifest(name), "{name}");
        }
        assert!(!is_reuse_manifest("REUSE.md"));
    }

    #[test]
    fn spdx_documents_include_the_bare_extension_in_any_case() {
        assert!(is_spdx_document("sbom.spdx.json"));
        assert!(is_spdx_document("sbom.spdx.rdf"));
        assert!(is_spdx_document("sbom.spdx"));
        assert!(is_spdx_document("sbom.SPDX"));

        assert!(!is_spdx_document("spdx.rs"));
        assert!(!is_spdx_document("notes.md"));
    }
}
// REUSE-IgnoreEnd
