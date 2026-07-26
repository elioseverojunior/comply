// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::path::Path;

use super::{Annotation, Config, Precedence};
use crate::error::Error;

impl Config {
    /// Parse DEP5 format (`.reuse/dep5`).
    #[allow(clippy::missing_errors_doc)]
    pub fn from_dep5(path: &Path) -> Result<Vec<Annotation>, Error> {
        let content = fs::read_to_string(path).map_err(Error::Io)?;
        let mut annotations = vec![];

        let mut current_files: Option<String> = None;
        let mut current_copyright: Vec<String> = vec![];
        let mut current_license: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(files) = line.strip_prefix("Files: ") {
                // Save previous annotation if complete
                if let (Some(path), Some(license)) = (current_files.take(), current_license.take())
                {
                    annotations.push(Annotation {
                        path,
                        copyright: std::mem::take(&mut current_copyright),
                        license,
                        precedence: Precedence::Closest,
                    });
                }
                current_files = Some(files.trim().to_string());
            } else if let Some(copyright) = line.strip_prefix("Copyright: ") {
                current_copyright.push(copyright.trim().to_string());
            } else if let Some(license) = line.strip_prefix("License: ") {
                current_license = Some(license.trim().to_string());
            }
        }

        // Flush last annotation
        if let (Some(path), Some(license)) = (current_files, current_license) {
            annotations.push(Annotation {
                path,
                copyright: current_copyright,
                license,
                precedence: Precedence::Closest,
            });
        }

        Ok(annotations)
    }
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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
