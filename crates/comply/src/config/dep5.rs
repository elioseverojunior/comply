// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::path::Path;

use super::{Annotation, Config, Precedence};
use crate::error::Error;

/// A parsed `.reuse/dep5` document: its header plus its `Files:` paragraphs.
///
/// `from_dep5` only needs the paragraphs, but converting to REUSE.toml has to
/// carry the header across too, so both come from one parse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dep5Document {
    /// `Upstream-Name`, written as `SPDX-PackageName`.
    pub package_name: Option<String>,
    /// `Upstream-Contact`, written as `SPDX-PackageSupplier`.
    pub package_supplier: Option<String>,
    /// `Source`, written as `SPDX-PackageDownloadLocation`.
    pub package_download_location: Option<String>,
    /// One entry per `Files:` paragraph.
    pub annotations: Vec<Annotation>,
}

/// Accumulates one `Files:` paragraph while the parser walks the document.
#[derive(Default)]
struct Paragraph {
    files: Option<String>,
    copyright: Vec<String>,
    license: Option<String>,
}

impl Paragraph {
    /// A paragraph only becomes an annotation once it has both a path and a
    /// licence; anything less is an incomplete stanza and is dropped.
    fn take_annotation(&mut self) -> Option<Annotation> {
        let path = self.files.take()?;
        let license = self.license.take()?;
        Some(Annotation {
            path,
            copyright: std::mem::take(&mut self.copyright),
            license,
            precedence: Precedence::Closest,
        })
    }
}

/// Which field a continuation line belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Copyright,
    Other,
}

impl Config {
    /// Parse DEP5 format (`.reuse/dep5`).
    #[allow(clippy::missing_errors_doc)]
    pub fn from_dep5(path: &Path) -> Result<Vec<Annotation>, Error> {
        Ok(Self::dep5_document(path)?.annotations)
    }

    /// Parse `.reuse/dep5` in full, header included.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the file cannot be read.
    pub fn dep5_document(path: &Path) -> Result<Dep5Document, Error> {
        let content = fs::read_to_string(path).map_err(Error::Io)?;
        let mut doc = Dep5Document::default();
        let mut para = Paragraph::default();
        // DEP5 folds long values onto indented continuation lines, so the field
        // a bare line belongs to is whichever one opened the run.
        let mut continuing = Field::Other;

        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // An indented line with no `Key:` continues the previous field.
            let is_continuation = raw.starts_with(char::is_whitespace) && !line.contains(": ");
            if is_continuation {
                if continuing == Field::Copyright {
                    para.copyright.push(line.to_string());
                }
                continue;
            }

            if let Some(files) = line.strip_prefix("Files:") {
                if let Some(annotation) = para.take_annotation() {
                    doc.annotations.push(annotation);
                }
                para.files = Some(files.trim().to_string());
                continuing = Field::Other;
            } else if let Some(copyright) = line.strip_prefix("Copyright:") {
                para.copyright.push(copyright.trim().to_string());
                continuing = Field::Copyright;
            } else if let Some(license) = line.strip_prefix("License:") {
                para.license = Some(license.trim().to_string());
                continuing = Field::Other;
            } else if let Some(name) = line.strip_prefix("Upstream-Name:") {
                doc.package_name = Some(name.trim().to_string());
                continuing = Field::Other;
            } else if let Some(contact) = line.strip_prefix("Upstream-Contact:") {
                doc.package_supplier = Some(contact.trim().to_string());
                continuing = Field::Other;
            } else if let Some(source) = line.strip_prefix("Source:") {
                doc.package_download_location = Some(source.trim().to_string());
                continuing = Field::Other;
            } else {
                continuing = Field::Other;
            }
        }

        if let Some(annotation) = para.take_annotation() {
            doc.annotations.push(annotation);
        }

        Ok(doc)
    }
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_folded_copyright_keeps_every_continuation_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dep5");
        // DEP5 folds long values onto indented lines. Dropping them silently
        // lost every holder after the first.
        fs::write(
            &path,
            "Files: docs/*\nCopyright: 2026 Jane Doe\n           2025 Old Author\nLicense: CC-BY-4.0\n",
        )
        .unwrap();

        let annotations = Config::from_dep5(&path).unwrap();

        assert_eq!(annotations.len(), 1);
        assert_eq!(
            annotations[0].copyright,
            vec!["2026 Jane Doe".to_string(), "2025 Old Author".to_string()]
        );
    }

    #[test]
    fn the_header_fields_are_captured_for_conversion() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dep5");
        fs::write(
            &path,
            concat!(
                "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n",
                "Upstream-Name: example\n",
                "Upstream-Contact: Jane <jane@example.com>\n",
                "Source: https://example.com\n",
                "\n",
                "Files: src/*\n",
                "Copyright: 2026 Jane Doe\n",
                "License: MIT\n",
            ),
        )
        .unwrap();

        let doc = Config::dep5_document(&path).unwrap();

        assert_eq!(doc.package_name.as_deref(), Some("example"));
        assert_eq!(
            doc.package_supplier.as_deref(),
            Some("Jane <jane@example.com>")
        );
        assert_eq!(
            doc.package_download_location.as_deref(),
            Some("https://example.com")
        );
        assert_eq!(doc.annotations.len(), 1);
        assert_eq!(doc.annotations[0].path, "src/*");
    }

    #[test]
    fn a_stanza_without_a_licence_is_dropped_rather_than_half_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dep5");
        fs::write(
            &path,
            "Files: a/*\nCopyright: A\n\nFiles: b/*\nCopyright: B\nLicense: MIT\n",
        )
        .unwrap();

        let annotations = Config::from_dep5(&path).unwrap();

        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].path, "b/*");
    }

    #[test]
    fn parse_dep5_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dep5");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r"Files: *
Copyright: 2026 Project
License: MIT

Files: src/**
Copyright: 2026 Author
License: Apache-2.0
"
        )
        .unwrap();

        let annos = Config::from_dep5(&path).unwrap();
        assert_eq!(annos.len(), 2);
        assert_eq!(annos[0].path, "*");
        assert_eq!(annos[0].copyright, vec!["2026 Project"]);
        assert_eq!(annos[0].license, "MIT");
        assert_eq!(annos[1].path, "src/**");
    }

    #[test]
    fn parse_dep5_skips_comments_and_blanks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dep5");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(
            f,
            r"# This is a comment

Files: *
Copyright: 2026 Project
License: MIT
"
        )
        .unwrap();

        let annos = Config::from_dep5(&path).unwrap();
        assert_eq!(annos.len(), 1);
    }

    #[test]
    /// REUSE 3.3 makes the two formats mutually exclusive, so there is no
    /// "preference" to express -- a project carrying both is misconfigured.
    fn load_rejects_reuse_toml_and_dep5_together() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = fs::File::create(dir.path().join("REUSE.toml")).unwrap();
        writeln!(
            f,
            r#"version = 1

[[annotation]]
path = "src/**"
SPDX-FileCopyrightText = ["2026 Project"]
SPDX-License-Identifier = "MIT"
"#
        )
        .unwrap();

        let dep5_dir = dir.path().join(".reuse");
        fs::create_dir_all(&dep5_dir).unwrap();
        let mut f = fs::File::create(dep5_dir.join("dep5")).unwrap();
        writeln!(
            f,
            r"Files: *
Copyright: 2026 Dep5
License: Apache-2.0
"
        )
        .unwrap();

        let err = Config::load(dir.path()).unwrap_err().to_string();

        assert!(err.contains("dep5"), "got: {err}");
    }

    #[test]
    fn load_dep5_only_when_no_reuse_toml() {
        let dir = tempfile::tempdir().unwrap();
        let dep5_dir = dir.path().join(".reuse");
        fs::create_dir_all(&dep5_dir).unwrap();
        let mut f = fs::File::create(dep5_dir.join("dep5")).unwrap();
        writeln!(
            f,
            r"Files: *
Copyright: 2026 Project
License: MIT
"
        )
        .unwrap();

        let config = Config::load(dir.path()).unwrap();
        assert_eq!(config.annotations.len(), 1);
        assert_eq!(config.annotations[0].license, "MIT");
    }

    #[test]
    fn every_reuse_toml_spelling_conflicts_with_dep5() {
        for name in &[".reuse.toml", "reuse.toml"] {
            let dir = tempfile::tempdir().unwrap();
            let mut f = fs::File::create(dir.path().join(name)).unwrap();
            writeln!(
                f,
                r#"version = 1

[[annotation]]
path = "{}/*"
SPDX-FileCopyrightText = ["2026 FromToml"]
SPDX-License-Identifier = "GPL-3.0-only"
"#,
                name.trim_start_matches('.')
            )
            .unwrap();

            let dep5_dir = dir.path().join(".reuse");
            fs::create_dir_all(&dep5_dir).unwrap();
            let mut f = fs::File::create(dep5_dir.join("dep5")).unwrap();
            writeln!(
                f,
                r"Files: *
Copyright: 2026 Dep5
License: MIT
"
            )
            .unwrap();

            let err = Config::load(dir.path()).unwrap_err().to_string();

            assert!(err.contains("dep5"), "for {name}, got: {err}");
        }
    }
}
// REUSE-IgnoreEnd
