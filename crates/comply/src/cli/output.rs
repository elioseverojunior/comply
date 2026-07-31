// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Report output shapes, matching `reuse` 6.2.0's `--quiet/--json/--plain/--lines`.

use std::path::Path;

use anyhow::Result;
use comply::report::{Report, Status};
use comply::spdx;

use crate::cli::shared::print_json_report;

/// How a lint report is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Format {
    /// Human-readable summary. `reuse`'s default for `lint`.
    #[default]
    Plain,
    /// Machine-readable.
    Json,
    /// One `path: problem` per line. `reuse`'s default for `lint-file`.
    Lines,
    /// Nothing at all; the exit code carries the verdict.
    Quiet,
}

/// The output flags as a command supplies them.
///
/// A struct rather than four `bool` parameters: the order of four positional
/// bools is impossible to read at a call site, and easy to get wrong.
#[derive(Debug, Clone, Copy, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors the four independent output flags on the clap variants"
)]
pub(crate) struct Flags {
    pub(crate) quiet: bool,
    pub(crate) json: bool,
    pub(crate) plain: bool,
    pub(crate) lines: bool,
}

impl Format {
    /// Resolve the mutually exclusive output flags, most specific first.
    ///
    /// `default` differs per command: `lint` is `Plain`, `lint-file` is `Lines`,
    /// matching the reference tool.
    // Deliberately not `const fn`, against clippy's advice: both callers pass
    // runtime flags, so const bought nothing, and it let the compiler fold the
    // tests' calls away at compile time. Whether it did depended on
    // optimisation settings, so these branches measured as covered locally and
    // uncovered on CI -- a coverage gate that moves with the target is worse
    // than a missing const.
    #[allow(clippy::missing_const_for_fn)]
    pub(crate) fn resolve(flags: Flags, default: Self) -> Self {
        // `--quiet` wins: asking for silence and a format at once means silence.
        if flags.quiet {
            Self::Quiet
        } else if flags.json {
            Self::Json
        } else if flags.lines {
            Self::Lines
        } else if flags.plain {
            Self::Plain
        } else {
            default
        }
    }
}

pub(crate) fn emit(report: &Report, root: &Path, format: Format) -> Result<()> {
    match format {
        Format::Quiet => Ok(()),
        Format::Json => print_json_report(report),
        Format::Plain => {
            print!("{report}");
            Ok(())
        }
        Format::Lines => {
            for line in lines_of(report, root) {
                println!("{line}");
            }
            Ok(())
        }
    }
}

/// The `--lines` rendering: absolute path, then one problem per line.
///
/// Two problems on one file produce two lines, and the licence identifier is
/// reported before the copyright notice -- both verified against `reuse` 6.2.0.
fn lines_of(report: &Report, root: &Path) -> Vec<String> {
    let missing: Vec<&str> = report
        .licenses()
        .missing
        .iter()
        .map(String::as_str)
        .collect();
    let mut out = Vec::new();

    for result in report.iter() {
        let shown = root.join(&result.path).display().to_string();

        match &result.status {
            Status::Ok | Status::Skipped => {}
            Status::ReadError => out.push(format!("{shown}: read error")),
            // An expression that does not parse declares nothing, so the
            // reference tool reports the malformed text AND the resulting
            // absence of an identifier. Without the first line this status
            // printed nothing at all: the exit code said 1 and the output was
            // empty, which is how the pre-commit hook failed without a reason.
            Status::InvalidExpression(expression) => {
                out.push(format!(
                    "{shown}: invalid SPDX License Expression '{expression}'"
                ));
                out.push(format!("{shown}: no license identifier"));
            }
            _ => {
                if result.license.is_none() {
                    out.push(format!("{shown}: no license identifier"));
                }
                if result.copyrights.is_empty() {
                    out.push(format!("{shown}: no copyright notice"));
                }
            }
        }

        // A licence the file names but `LICENSES/` does not carry. Derived from
        // the file's own expression so the report points at the file that has
        // to change, not just at the identifier.
        if let Some(expression) = &result.license
            && let Ok(parsed) = spdx::parse(expression)
        {
            for id in parsed.identifiers() {
                if missing.contains(&id.as_str()) {
                    out.push(format!("{shown}: missing license '{id}'"));
                }
            }
        }
    }
    out
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use comply::license::LicenseAudit;

    fn root() -> &'static Path {
        Path::new("/project")
    }

    #[test]
    fn quiet_beats_every_other_flag() {
        let all = Flags {
            quiet: true,
            json: true,
            plain: true,
            lines: true,
        };
        assert_eq!(Format::resolve(all, Format::Plain), Format::Quiet);
    }

    #[test]
    fn each_flag_selects_its_own_format() {
        let json = Flags {
            json: true,
            ..Flags::default()
        };
        let lines = Flags {
            lines: true,
            ..Flags::default()
        };
        let plain = Flags {
            plain: true,
            ..Flags::default()
        };
        assert_eq!(Format::resolve(json, Format::Plain), Format::Json);
        assert_eq!(Format::resolve(lines, Format::Plain), Format::Lines);
        assert_eq!(Format::resolve(plain, Format::Lines), Format::Plain);
    }

    #[test]
    fn with_no_flags_each_command_keeps_its_own_default() {
        assert_eq!(
            Format::resolve(Flags::default(), Format::Plain),
            Format::Plain
        );
        assert_eq!(
            Format::resolve(Flags::default(), Format::Lines),
            Format::Lines
        );
    }

    #[test]
    fn an_invalid_expression_is_named_in_the_line_output() {
        // Without this the status was set and the exit code was 1, but
        // `--lines` printed nothing -- and `--lines` is `lint-file`'s default,
        // which the pre-commit hook uses. The hook failed with no reason given.
        // Both lines and their order verified against `reuse` 6.2.0.
        let report = Report::builder()
            .add_result(
                "bad.rs",
                Status::InvalidExpression("MIT AND AND GPL".to_string()),
                vec!["2026 Acme".to_string()],
                Some("MIT AND AND GPL".to_string()),
            )
            .build();

        assert_eq!(
            lines_of(&report, root()),
            vec![
                "/project/bad.rs: invalid SPDX License Expression 'MIT AND AND GPL'".to_string(),
                "/project/bad.rs: no license identifier".to_string(),
            ]
        );
    }

    #[test]
    fn a_file_missing_both_declarations_produces_two_lines_licence_first() {
        let report = Report::builder()
            .add_result("bad.py", Status::MissingCopyright, vec![], None)
            .build();

        assert_eq!(
            lines_of(&report, root()),
            vec![
                "/project/bad.py: no license identifier".to_string(),
                "/project/bad.py: no copyright notice".to_string(),
            ]
        );
    }

    #[test]
    fn a_referenced_licence_with_no_file_is_attributed_to_the_file_naming_it() {
        let report = Report::builder()
            .add_ok_with_header(
                "nolicfile.py",
                vec!["2026 A".to_string()],
                Some("Apache-2.0".to_string()),
            )
            .licenses(LicenseAudit {
                missing: vec!["Apache-2.0".to_string()],
                ..LicenseAudit::default()
            })
            .build();

        assert_eq!(
            lines_of(&report, root()),
            vec!["/project/nolicfile.py: missing license 'Apache-2.0'".to_string()]
        );
    }

    #[test]
    fn compliant_and_skipped_files_produce_no_lines() {
        let report = Report::builder()
            .add_ok_with_header("a.rs", vec!["2026 A".to_string()], Some("MIT".to_string()))
            .add_skipped("LICENSE")
            .build();

        assert!(lines_of(&report, root()).is_empty());
    }

    #[test]
    fn a_read_error_says_so_rather_than_blaming_the_header() {
        let report = Report::builder()
            .add_result("locked.rs", Status::ReadError, vec![], None)
            .build();

        assert_eq!(
            lines_of(&report, root()),
            vec!["/project/locked.rs: read error".to_string()]
        );
    }

    #[test]
    fn every_format_emits_without_error() {
        let report = Report::builder()
            .add_result("bad.py", Status::MissingCopyright, vec![], None)
            .build();

        for format in [Format::Plain, Format::Json, Format::Lines, Format::Quiet] {
            assert!(emit(&report, root(), format).is_ok(), "{format:?}");
        }
    }
}
// REUSE-IgnoreEnd
