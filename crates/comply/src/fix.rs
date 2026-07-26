// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::path::Path;

use crate::config::Config;
use crate::error::Error;
use crate::file::{self, FileType};

/// Result of fixing a file
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixAction {
    /// File was already compliant
    Unchanged,
    /// Added SPDX header to file
    HeaderAdded,
    /// Reformatted existing SPDX header
    HeaderReformatted,
    /// Created missing license companion file
    LicenseFileCreated,
    /// File is binary but missing license file
    BinaryLicenseMissing,
    /// File could not be fixed (error details)
    Failed(String),
}

/// Result of fixing a project
#[derive(Debug, Clone, Default)]
pub struct FixReport {
    pub actions: Vec<(String, FixAction)>,
    pub total: usize,
    pub fixed: usize,
    pub unchanged: usize,
    pub failed: usize,
}

impl FixReport {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            actions: Vec::new(),
            total: 0,
            fixed: 0,
            unchanged: 0,
            failed: 0,
        }
    }

    pub fn add(&mut self, path: String, action: FixAction) {
        self.total += 1;
        match &action {
            FixAction::Unchanged => self.unchanged += 1,
            FixAction::Failed(_) => self.failed += 1,
            _ => self.fixed += 1,
        }
        self.actions.push((path, action));
    }

    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.failed == 0 && self.fixed == 0
    }
}

/// Fix a project's REUSE compliance issues
#[allow(clippy::missing_errors_doc)]
pub fn fix_project(root: &Path, config: &Config, dry_run: bool) -> Result<FixReport, Error> {
    let mut report = FixReport::new();

    for entry in file::walk(root, &config.ignore, &config.source_patterns) {
        let entry = entry?;
        let rel_path = entry.relative_to(root);

        let action = match entry.file_type {
            FileType::Source => fix_source_file(&entry.path, &rel_path, config, dry_run)?,
            FileType::Binary => fix_binary_file(&entry.path, &rel_path, config, dry_run)?,
            FileType::Ignored => FixAction::Unchanged,
        };

        report.add(rel_path, action);
    }

    Ok(report)
}

/// Resolve copyright and license text for a file from the config.
///
/// `rel_path` must be relative to the project root: REUSE annotation globs are
/// written against that form, so an absolute path matches nothing but `**`.
/// Returns `None` when neither an annotation nor the defaults supply both
/// halves, since a header needs a copyright *and* a licence.
fn resolve_annotation(config: &Config, rel_path: &str) -> Option<(String, String)> {
    let resolved = config.resolve_licensing(rel_path, &[], None);
    if let Some(license) = resolved.license
        && !resolved.copyrights.is_empty()
    {
        return Some((resolved.copyrights.join("\n"), license));
    }

    if !config.default_copyright.is_empty()
        && let Some(license) = &config.default_license
    {
        return Some((config.default_copyright.join("\n"), license.clone()));
    }

    None
}

/// Fix a source code file. `rel_path` is relative to the project root.
fn fix_source_file(
    path: &Path,
    rel_path: &str,
    config: &Config,
    dry_run: bool,
) -> Result<FixAction, Error> {
    // Per REUSE spec §7.2, .license companion takes precedence over file content
    let (content, is_license_content) = if file::has_license_companion(path) {
        let license_path = file::license_companion_path(path);
        (fs::read_to_string(license_path)?, true)
    } else {
        (fs::read_to_string(path)?, false)
    };

    // Check if file already has a valid header
    if let Some(header) = crate::header::detect(&content) {
        let has_copyright = !header.copyrights.is_empty();
        let has_license = header.license_expression.is_some();

        if has_copyright && has_license {
            // Header is complete, just reformat if needed
            let path_str = path.to_str().unwrap_or("");
            let style = crate::header::detect_style(path_str);
            let formatted = crate::header::format_file(&content, Some(style))?;

            if formatted == content {
                return Ok(FixAction::Unchanged);
            }

            if !dry_run {
                if is_license_content {
                    fs::write(file::license_companion_path(path), formatted)?;
                } else {
                    fs::write(path, formatted)?;
                }
            }
            return Ok(FixAction::HeaderReformatted);
        }
    }

    // File missing header or incomplete header - add one using config annotations or defaults
    let Some((copyright, license)) = resolve_annotation(config, rel_path) else {
        return Ok(FixAction::Failed(
            "No SPDX header and no matching config annotation or defaults".to_string(),
        ));
    };

    let path_str = path.to_str().unwrap_or("");
    let style = crate::header::detect_style(path_str);
    let new_content =
        crate::header::annotate_file(&content, &copyright, &license, Some(style), true)?;

    if !dry_run {
        if is_license_content {
            fs::write(file::license_companion_path(path), new_content)?;
        } else {
            fs::write(path, new_content)?;
        }
    }
    Ok(FixAction::HeaderAdded)
}

/// Fix a binary file (ensure companion license file exists).
/// `rel_path` is relative to the project root.
fn fix_binary_file(
    path: &Path,
    rel_path: &str,
    config: &Config,
    dry_run: bool,
) -> Result<FixAction, Error> {
    let license_path = path.with_extension(
        path.extension()
            .and_then(|e| e.to_str())
            .map_or_else(|| "license".to_string(), |e| format!("{e}.license")),
    );

    if license_path.exists() {
        return Ok(FixAction::Unchanged);
    }

    let Some((copyright, license)) = resolve_annotation(config, rel_path) else {
        return Ok(FixAction::BinaryLicenseMissing);
    };

    // REUSE-IgnoreStart
    let license_text =
        format!("SPDX-FileCopyrightText: {copyright}\nSPDX-License-Identifier: {license}\n");
    // REUSE-IgnoreEnd

    if !dry_run {
        fs::write(&license_path, license_text)?;
    }
    Ok(FixAction::LicenseFileCreated)
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Annotation, Config, Precedence};
    use std::fs;
    use tempfile::TempDir;

    const MALFORMED_HEADER: &str = concat!(
        "//SPDX-FileCopyrightText:2025 Acme\n",
        "//SPDX-License-Identifier:MIT\n"
    );

    /// A source file plus its `.license` companion, which per REUSE 7.2 is what
    /// actually declares the licensing.
    fn source_with_companion(companion: &str) -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(&path, "fn main() {}\n").unwrap();
        fs::write(file::license_companion_path(&path), companion).unwrap();
        (tmp, path)
    }

    fn companion_of(path: &Path) -> String {
        fs::read_to_string(file::license_companion_path(path)).unwrap()
    }

    /// Every fixture below puts its file directly under the temp root, so the
    /// project-relative path the fixers expect is just the file name.
    fn rel(path: &Path) -> String {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into()
    }

    fn config_annotating(glob: &str) -> Config {
        Config {
            annotations: vec![Annotation {
                path: glob.to_string(),
                copyright: vec!["2026 Acme".to_string()],
                license: "MIT".to_string(),
                precedence: Precedence::Closest,
            }],
            ..Config::builder().build()
        }
    }

    #[test]
    fn an_annotation_glob_matches_a_source_file_by_repo_relative_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();

        let report = fix_project(root, &config_annotating("src/**"), false).unwrap();

        let actions = format!("{:?}", report.actions);
        assert_eq!(
            report.failed, 0,
            "`src/**` must match `src/a.rs`, not the absolute path; got {actions}"
        );
        let content = fs::read_to_string(root.join("src/a.rs")).unwrap();
        assert!(
            content.contains("SPDX-License-Identifier: MIT"),
            "got:\n{content}"
        );
    }

    #[test]
    fn an_annotation_glob_matches_a_binary_by_repo_relative_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(root.join("assets/logo.png"), b"\x89PNG").unwrap();

        let report = fix_project(root, &config_annotating("assets/**"), false).unwrap();

        let actions = format!("{:?}", report.actions);
        assert_eq!(
            report.failed, 0,
            "`assets/**` must match `assets/logo.png`, not the bare file name; got {actions}"
        );
        assert!(
            root.join("assets/logo.png.license").exists(),
            "the companion should have been written"
        );
    }

    #[test]
    fn a_companion_is_reformatted_in_place_of_the_source_file() {
        let (_tmp, path) = source_with_companion(MALFORMED_HEADER);

        let action =
            fix_source_file(&path, &rel(&path), &Config::builder().build(), false).unwrap();

        assert_eq!(action, FixAction::HeaderReformatted);
        let companion = companion_of(&path);
        assert!(
            companion.starts_with("// SPDX-FileCopyrightText: "),
            "got:\n{companion}"
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "fn main() {}\n",
            "the source file itself must not be rewritten"
        );
    }

    #[test]
    fn a_dry_run_reports_the_reformat_without_writing_it() {
        let (_tmp, path) = source_with_companion(MALFORMED_HEADER);

        let action = fix_source_file(&path, &rel(&path), &Config::builder().build(), true).unwrap();

        assert_eq!(action, FixAction::HeaderReformatted);
        assert_eq!(
            companion_of(&path),
            MALFORMED_HEADER,
            "--dry-run must leave the companion byte-identical"
        );
    }

    #[test]
    fn a_companion_with_no_header_gets_one_from_the_defaults() {
        let (_tmp, path) = source_with_companion("nothing declared here\n");
        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Test Corp")
            .build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();

        assert_eq!(action, FixAction::HeaderAdded);
        let companion = companion_of(&path);
        assert!(
            companion.contains("SPDX-License-Identifier: MIT"),
            "got:\n{companion}"
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "fn main() {}\n",
            "the header belongs in the companion, not the source"
        );
    }

    #[test]
    fn an_annotation_missing_its_licence_is_not_enough_to_fix_a_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(&path, "fn main() {}\n").unwrap();
        let config = Config {
            annotations: vec![Annotation {
                path: "**".to_string(),
                copyright: vec!["2026 Acme".to_string()],
                license: String::new(),
                precedence: Precedence::Closest,
            }],
            ..Config::builder().build()
        };

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();

        assert!(
            matches!(action, FixAction::Failed(_)),
            "a copyright with no licence cannot produce a header, got: {action:?}"
        );
    }

    #[test]
    fn a_default_copyright_without_a_default_licence_is_not_enough() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(&path, "fn main() {}\n").unwrap();
        let config = Config::builder().add_default_copyright("2026 Acme").build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();

        assert!(matches!(action, FixAction::Failed(_)), "got: {action:?}");
    }

    #[test]
    fn a_header_missing_its_licence_is_completed_from_the_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(
            &path,
            "// SPDX-FileCopyrightText: 2025 Acme\n\nfn main() {}\n",
        )
        .unwrap();
        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Test Corp")
            .build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();

        assert_eq!(
            action,
            FixAction::HeaderAdded,
            "a copyright without a licence is an incomplete header, not a complete one"
        );
        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("SPDX-License-Identifier: MIT"),
            "got:\n{content}"
        );
    }

    #[test]
    fn a_binary_with_no_extension_still_gets_a_companion() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("payload");
        fs::write(&path, b"\x00\x01\x02").unwrap();
        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Acme")
            .build();

        let action = fix_binary_file(&path, &rel(&path), &config, false).unwrap();

        assert_eq!(action, FixAction::LicenseFileCreated);
        assert!(
            tmp.path().join("payload.license").exists(),
            "an extensionless file falls back to a plain `.license` suffix"
        );
    }

    #[test]
    fn a_dry_run_does_not_write_a_binary_companion() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("logo.png");
        fs::write(&path, b"\x89PNG").unwrap();
        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Acme")
            .build();

        let action = fix_binary_file(&path, &rel(&path), &config, true).unwrap();

        assert_eq!(action, FixAction::LicenseFileCreated);
        assert!(
            !tmp.path().join("logo.png.license").exists(),
            "--dry-run reports the companion it would create, but creates nothing"
        );
    }

    #[test]
    fn a_binary_with_resolvable_licensing_gets_a_companion_written() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("logo.png");
        fs::write(&path, b"\x89PNG").unwrap();
        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Acme")
            .build();

        let action = fix_binary_file(&path, &rel(&path), &config, false).unwrap();

        assert_eq!(action, FixAction::LicenseFileCreated);
        assert!(
            fs::read_to_string(tmp.path().join("logo.png.license"))
                .unwrap()
                .contains("SPDX-License-Identifier: MIT")
        );
    }

    #[test]
    fn test_fix_source_file_adds_header() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(&path, "fn main() { println!(\"hello\"); }\n").unwrap();

        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Test Corp")
            .build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();
        assert_eq!(action, FixAction::HeaderAdded);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("// SPDX-FileCopyrightText:"));
        assert!(content.contains("SPDX-License-Identifier:"));
    }

    #[test]
    fn test_fix_source_file_reformats_header() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        // Malformed header
        fs::write(
            &path,
            "//SPDX-FileCopyrightText:2025 Acme\n//SPDX-License-Identifier:MIT\n\nfn main() {}\n",
        )
        .unwrap();

        let config = Config::builder().build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();
        assert_eq!(action, FixAction::HeaderReformatted);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("// SPDX-FileCopyrightText: "));
        assert!(content.contains("// SPDX-License-Identifier: MIT\n"));
    }

    #[test]
    fn test_fix_source_file_unchanged_when_compliant() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        // Already compliant AND already in canonical form -- note the `//`
        // separator, without which this would be reported as needing a reformat.
        fs::write(
            &path,
            "// SPDX-FileCopyrightText: 2025 Acme\n//\n// SPDX-License-Identifier: MIT\n\nfn main() {}\n",
        )
        .unwrap();

        let config = Config::builder().build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();
        assert_eq!(action, FixAction::Unchanged);
    }

    #[test]
    fn test_fix_binary_creates_license_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("binary.bin");
        fs::write(&path, b"binary content").unwrap();

        // Create config with annotation for this file
        let _config = Config::builder().build();
        // Note: config doesn't have annotations by default in tests
        // This test would need a proper config with annotations

        let action =
            fix_binary_file(&path, &rel(&path), &Config::builder().build(), false).unwrap();
        // Without annotation, should return BinaryLicenseMissing
        assert_eq!(action, FixAction::BinaryLicenseMissing);
    }

    #[test]
    fn test_fix_binary_unchanged_when_companion_exists() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("binary.bin");
        fs::write(&path, b"binary content").unwrap();

        // Create companion license file
        let license_path = path.with_extension("bin.license");
        fs::write(
            &license_path,
            "SPDX-FileCopyrightText: 2025 Acme\nSPDX-License-Identifier: MIT\n",
        )
        .unwrap();

        let action =
            fix_binary_file(&path, &rel(&path), &Config::builder().build(), false).unwrap();
        assert_eq!(action, FixAction::Unchanged);
    }

    #[test]
    fn test_fix_project_integration() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create a source file without header
        fs::write(root.join("main.rs"), "fn main() { println!(\"hello\"); }\n").unwrap();

        // Create a binary without license
        fs::write(root.join("binary.bin"), b"binary").unwrap();

        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Test Corp")
            .build();
        let report = fix_project(root, &config, false).unwrap();

        assert_eq!(report.total, 2); // main.rs + binary.bin
        assert!(report.fixed >= 1);
    }

    #[test]
    fn test_fix_report_is_clean() {
        // Clean report - no fixes needed
        let mut report = FixReport::new();
        report.add("file1.rs".to_string(), FixAction::Unchanged);
        report.add("file2.rs".to_string(), FixAction::Unchanged);
        assert!(report.is_clean());

        // Not clean - has fixed files
        let mut report2 = FixReport::new();
        report2.add("file1.rs".to_string(), FixAction::HeaderAdded);
        assert!(!report2.is_clean());

        // Not clean - has failed files
        let mut report3 = FixReport::new();
        report3.add(
            "file1.rs".to_string(),
            FixAction::Failed("error".to_string()),
        );
        assert!(!report3.is_clean());
    }
}
// REUSE-IgnoreEnd
