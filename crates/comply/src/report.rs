// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use crate::license::LicenseAudit;

/// The compliance status for a single file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// File has a valid SPDX header (or has a `.license` companion).
    Ok,
    /// File is missing copyright information.
    MissingCopyright,
    /// File is missing license information.
    MissingLicense,
    /// File has an invalid SPDX expression.
    InvalidExpression(String),
    /// File is binary with no `.license` companion.
    MissingLicenseFile,
    /// The file exists but could not be read at all -- distinct from a file
    /// whose bytes simply are not text, which needs a `.license` companion.
    ReadError,
    /// File is ignored (not checked).
    Skipped,
}

/// A lint result for a single file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResult {
    /// The relative path of the file.
    pub path: String,
    /// Compliance status.
    pub status: Status,
    /// Detected copyright lines (if any).
    pub copyrights: Vec<String>,
    /// Detected license expression (if any).
    pub license: Option<String>,
}

/// A complete lint report for one or more files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Per-file results keyed by relative path.
    results: BTreeMap<String, FileResult>,
    /// Total counters.
    total: usize,
    passed: usize,
    failed: usize,
    skipped: usize,
    /// How `LICENSES/` lines up with the licenses the files reference.
    licenses: LicenseAudit,
}

impl Report {
    /// Create a new `ReportBuilder`.
    #[must_use]
    pub const fn builder() -> ReportBuilder {
        ReportBuilder::new()
    }

    /// Iterate over all file results.
    pub fn iter(&self) -> impl Iterator<Item = &FileResult> {
        self.results.values()
    }

    /// The total number of files checked.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.total
    }

    /// The number of compliant files.
    #[must_use]
    pub const fn passed(&self) -> usize {
        self.passed
    }

    /// The number of non-compliant files.
    #[must_use]
    pub const fn failed(&self) -> usize {
        self.failed
    }

    /// The number of skipped files.
    #[must_use]
    pub const fn skipped(&self) -> usize {
        self.skipped
    }

    /// Whether all files are compliant.
    #[must_use]
    pub const fn is_compliant(&self) -> bool {
        self.failed == 0 && self.licenses.is_clean()
    }

    /// The `LICENSES/` directory findings for this project.
    #[must_use]
    pub const fn licenses(&self) -> &LicenseAudit {
        &self.licenses
    }

    /// Get the result for a specific relative path.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&FileResult> {
        self.results.get(path)
    }

    /// Get all results for files with a specific status.
    #[must_use]
    pub fn filter_by_status(&self, status: &Status) -> Vec<&FileResult> {
        self.results
            .values()
            .filter(|r| r.status == *status)
            .collect()
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            total,
            passed,
            failed,
            skipped,
            ..
        } = self;
        let summary = format!("{total} total, {passed} passed, {failed} failed, {skipped} skipped");
        writeln!(f, "REUSE lint report")?;
        writeln!(f, "{summary}")?;
        if self.failed > 0 {
            writeln!(f)?;
            writeln!(f, "Files with issues:")?;
            for result in self.results.values() {
                if result.status != Status::Ok && result.status != Status::Skipped {
                    writeln!(f, "  {}: {}", result.path, result.status)?;
                }
            }
        }
        for (label, ids) in [
            ("Bad licenses", &self.licenses.bad),
            ("Deprecated licenses", &self.licenses.deprecated),
            ("Missing licenses", &self.licenses.missing),
            ("Unused licenses", &self.licenses.unused),
            (
                "Licenses without file extension",
                &self.licenses.without_extension,
            ),
        ] {
            if !ids.is_empty() {
                writeln!(f)?;
                writeln!(f, "{label}: {}", ids.join(", "))?;
            }
        }
        Ok(())
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ok => write!(f, "OK"),
            Self::MissingCopyright => write!(f, "missing copyright"),
            Self::MissingLicense => write!(f, "missing license"),
            Self::InvalidExpression(e) => write!(f, "invalid expression: {e}"),
            Self::MissingLicenseFile => write!(f, "missing .license file"),
            Self::ReadError => write!(f, "read error"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

/// A fluent builder for constructing `Report` instances.
#[derive(Debug, Default)]
pub struct ReportBuilder {
    results: BTreeMap<String, FileResult>,
    licenses: LicenseAudit,
}

impl ReportBuilder {
    const fn new() -> Self {
        Self {
            results: BTreeMap::new(),
            // `LicenseAudit::default()` is not const, so the empty vectors are
            // spelled out to keep `Report::builder()` usable in const context.
            licenses: LicenseAudit {
                missing: Vec::new(),
                unused: Vec::new(),
                without_extension: Vec::new(),
                bad: Vec::new(),
                deprecated: Vec::new(),
            },
        }
    }

    /// Record the project's `LICENSES/` directory findings.
    ///
    /// Returns `self` for chaining.
    #[must_use]
    pub fn licenses(mut self, audit: LicenseAudit) -> Self {
        self.licenses = audit;
        self
    }

    /// Add a file result to the report.
    ///
    /// Returns `self` for chaining.
    #[must_use]
    pub fn add_file(mut self, result: FileResult) -> Self {
        self.results.insert(result.path.clone(), result);
        self
    }

    /// Convenience: add a result from its components.
    #[must_use]
    pub fn add_result(
        self,
        path: impl Into<String>,
        status: Status,
        copyrights: Vec<String>,
        license: Option<String>,
    ) -> Self {
        self.add_file(FileResult {
            path: path.into(),
            status,
            copyrights,
            license,
        })
    }

    /// Add an OK result for a source file that has a valid header.
    #[must_use]
    pub fn add_ok(self, path: impl Into<String>) -> Self {
        self.add_result(path, Status::Ok, vec![], None)
    }

    /// Add an OK result with detected header data.
    #[must_use]
    pub fn add_ok_with_header(
        self,
        path: impl Into<String>,
        copyrights: Vec<String>,
        license: Option<String>,
    ) -> Self {
        self.add_result(path, Status::Ok, copyrights, license)
    }

    /// Add a skipped result.
    #[must_use]
    pub fn add_skipped(self, path: impl Into<String>) -> Self {
        self.add_result(path, Status::Skipped, vec![], None)
    }

    /// Consume the builder and produce a `Report`.
    #[must_use]
    pub fn build(self) -> Report {
        let total = self.results.len();
        let passed = self
            .results
            .values()
            .filter(|r| r.status == Status::Ok)
            .count();
        let failed = self
            .results
            .values()
            .filter(|r| r.status != Status::Ok && r.status != Status::Skipped)
            .count();
        let skipped = self
            .results
            .values()
            .filter(|r| r.status == Status::Skipped)
            .count();

        Report {
            results: self.results,
            total,
            passed,
            failed,
            skipped,
            licenses: self.licenses,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_has_a_rendering() {
        assert_eq!(Status::Ok.to_string(), "OK");
        assert_eq!(Status::MissingCopyright.to_string(), "missing copyright");
        assert_eq!(Status::MissingLicense.to_string(), "missing license");
        assert_eq!(
            Status::InvalidExpression("MIT AND".to_string()).to_string(),
            "invalid expression: MIT AND"
        );
        assert_eq!(
            Status::MissingLicenseFile.to_string(),
            "missing .license file"
        );
        assert_eq!(Status::ReadError.to_string(), "read error");
        assert_eq!(Status::Skipped.to_string(), "skipped");
    }

    #[test]
    fn the_licence_audit_is_readable_back_off_the_report() {
        let audit = LicenseAudit {
            missing: vec!["MIT".to_string()],
            ..LicenseAudit::default()
        };
        let r = Report::builder().licenses(audit).build();

        assert_eq!(r.licenses().missing, vec!["MIT".to_string()]);
        // A missing licence file is a compliance failure even with no bad files.
        assert!(!r.is_compliant());
    }

    #[test]
    fn empty_report() {
        let r = Report::builder().build();
        assert!(r.is_compliant());
        assert_eq!(r.total(), 0);
    }

    #[test]
    fn single_ok_file() {
        let r = Report::builder().add_ok("src/lib.rs").build();
        assert!(r.is_compliant());
        assert_eq!(r.total(), 1);
        assert_eq!(r.passed(), 1);
    }

    #[test]
    fn single_failed_file() {
        let r = Report::builder()
            .add_result("src/main.rs", Status::MissingLicense, vec![], None)
            .build();
        assert!(!r.is_compliant());
        assert_eq!(r.total(), 1);
        assert_eq!(r.failed(), 1);
    }

    #[test]
    fn mixed_results() {
        let r = Report::builder()
            .add_ok("ok.rs")
            .add_result("bad.rs", Status::MissingCopyright, vec![], None)
            .add_skipped("ignored.bin")
            .build();
        assert_eq!(r.total(), 3);
        assert_eq!(r.passed(), 1);
        assert_eq!(r.failed(), 1);
        assert_eq!(r.skipped(), 1);
        assert!(!r.is_compliant());
    }

    #[test]
    fn filter_by_status() {
        let r = Report::builder()
            .add_ok("a.rs")
            .add_result("b.rs", Status::MissingLicense, vec![], None)
            .build();
        let failed = r.filter_by_status(&Status::MissingLicense);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].path, "b.rs");
    }

    #[test]
    fn get_result() {
        let r = Report::builder().add_ok("a.rs").build();
        assert!(r.get("a.rs").is_some());
        assert!(r.get("b.rs").is_none());
    }

    #[test]
    fn display_report() {
        let r = Report::builder()
            .add_ok("ok.rs")
            .add_result("bad.rs", Status::MissingLicense, vec![], None)
            .build();
        let display = format!("{r}");
        assert!(display.contains("REUSE lint report"));
        assert!(display.contains("1 failed"));
        assert!(display.contains("bad.rs"));
    }

    #[test]
    fn builder_chain() {
        let r = Report::builder()
            .add_ok("a.rs")
            .add_ok_with_header("b.rs", vec!["2026 Author".into()], Some("MIT".into()))
            .add_skipped("c.bin")
            .build();
        assert_eq!(r.total(), 3);
        assert_eq!(r.passed(), 2);
        assert_eq!(r.skipped(), 1);
    }

    #[test]
    fn iter_report() {
        let r = Report::builder()
            .add_ok("a.rs")
            .add_result("b.rs", Status::MissingLicense, vec![], None)
            .build();
        let items: Vec<_> = r.iter().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].path, "a.rs");
        assert_eq!(items[1].path, "b.rs");
    }
}
