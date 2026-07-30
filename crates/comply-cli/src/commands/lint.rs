// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeSet;
use std::fs;

use std::path::Path;

use anyhow::Result;
use comply::config::{Config, Licensing, Precedence};
use comply::file::{self, FileType};
use comply::report::{Report, ReportBuilder, Status};
use comply::{header, license, spdx};

use crate::output::{self, Format};
use crate::shared::{load_config, resolve_root};

/// Decide one source file's status and the licensing that applies to it.
///
/// Per REUSE 7.2 a `.license` companion supersedes the file's own content, so
/// that is what gets read when one exists.
fn lint_source(entry: &file::FileEntry, rel_path: &str, config: &Config) -> (Status, Licensing) {
    let content = if entry.has_license_file {
        fs::read_to_string(file::license_companion_path(&entry.path))
    } else {
        fs::read_to_string(&entry.path)
    };

    let content = match content {
        Ok(content) => content,
        // `InvalidData` means the bytes are simply not text -- a binary the
        // classifier did not recognise, which needs a `.license` companion.
        // Anything else (permissions, I/O) is a genuine read error.
        Err(e) if e.kind() != std::io::ErrorKind::InvalidData => {
            return (Status::ReadError, Licensing::default());
        }
        Err(_) if entry.has_license_file => return (Status::Ok, Licensing::default()),
        Err(_) => return (Status::MissingLicenseFile, Licensing::default()),
    };

    // A REUSE.toml annotation is an equally valid way to declare licensing,
    // and its precedence decides how it combines with the file's own header.
    let detected = header::detect(&content);
    let invalid = detected
        .as_ref()
        .and_then(|h| h.invalid_expressions.first().cloned());
    let (header_copyrights, header_license) =
        detected.map_or_else(Default::default, |h| (h.copyrights, h.license_expression));
    let resolved =
        config.resolve_licensing(rel_path, &header_copyrights, header_license.as_deref());

    // A tag that did not parse is reported on its own terms, with the file's
    // real licensing left intact in `resolved`. Merging it into the expression
    // instead made one stray fragment -- from a test fixture or a doc sample --
    // read as though the whole file were unlicensed.
    //
    // Unless an `override` annotation supersedes the file's own header, in
    // which case what the file says is irrelevant by definition and reporting
    // it would fail a project the reference tool passes.
    if let Some(expression) = invalid
        && !supersedes_file_header(config, rel_path)
    {
        return (Status::InvalidExpression(expression), resolved);
    }

    let status = classify_licensing(&resolved);
    (status, resolved)
}

/// Fold one discovered file into the report, recording the licences it names.
///
/// Shared with `lint-file`, which walks an explicit list instead of the project.
pub(crate) fn lint_entry(
    builder: ReportBuilder,
    entry: &file::FileEntry,
    rel_path: &str,
    config: &Config,
    used_licenses: &mut BTreeSet<String>,
) -> ReportBuilder {
    match entry.file_type {
        FileType::Ignored => builder.add_skipped(rel_path),
        FileType::Binary => {
            // A file that cannot carry a header is licensed either by a
            // `.license` companion or by a REUSE.toml annotation; both are
            // valid declarations, so both have to be consulted.
            let companion = entry
                .has_license_file
                .then(|| fs::read_to_string(file::license_companion_path(&entry.path)).ok())
                .flatten()
                .and_then(|text| header::detect(&text));
            let (companion_copyrights, companion_license) =
                companion.map_or_else(Default::default, |h| (h.copyrights, h.license_expression));
            let resolved = config.resolve_licensing(
                rel_path,
                &companion_copyrights,
                companion_license.as_deref(),
            );

            if resolved.copyrights.is_empty() || resolved.license.is_none() {
                builder.add_result(rel_path, Status::MissingLicenseFile, vec![], None)
            } else {
                collect_identifiers(resolved.license.as_deref(), used_licenses);
                builder.add_ok_with_header(rel_path, resolved.copyrights, resolved.license)
            }
        }
        FileType::Source => {
            let (status, resolved) = lint_source(entry, rel_path, config);
            collect_identifiers(resolved.license.as_deref(), used_licenses);
            builder.add_result(rel_path, status, resolved.copyrights, resolved.license)
        }
    }
}

/// Every identifier an expression names -- including a `WITH` exception -- needs
/// its own file in `LICENSES/`, so all of them are collected.
/// Whether a REUSE.toml annotation replaces this file's own header outright.
///
/// `match_annotations` returns its matches in precedence order, so the last one
/// is the winner. Only `override` discards the file's header; `aggregate` and
/// `closest` both still let it contribute.
fn supersedes_file_header(config: &Config, rel_path: &str) -> bool {
    config
        .match_annotations(rel_path)
        .last()
        .is_some_and(|a| a.precedence == Precedence::Override)
}

/// The verdict for a file whose licensing has already been resolved.
///
/// An expression that does not parse is not a declaration: accepting one
/// reported compliance that was not there, while `reuse` 6.2.0 listed the same
/// file under INVALID SPDX LICENSE EXPRESSIONS and exited 1. Checked last, so a
/// file missing both a notice and a licence still reports the more basic fault.
fn classify_licensing(resolved: &Licensing) -> Status {
    if resolved.copyrights.is_empty() {
        return Status::MissingCopyright;
    }
    let Some(expression) = resolved.license.as_deref() else {
        return Status::MissingLicense;
    };
    if spdx::validate(expression).is_err() {
        return Status::InvalidExpression(expression.to_string());
    }
    Status::Ok
}

fn collect_identifiers(expression: Option<&str>, used_licenses: &mut BTreeSet<String>) {
    if let Some(expression) = expression
        && let Ok(parsed) = spdx::parse(expression)
    {
        used_licenses.extend(parsed.identifiers());
    }
}

/// Count the licences an ignored file is nonetheless declared to carry.
///
/// `ignore` carries two intents that the walk cannot tell apart. Most entries
/// mean "not part of the project"; a few mean "part of the project, but nothing
/// may write to it" -- `DCO.txt`, whose own text forbids modification, and the
/// symlinks into `LICENSES/`, which `annotate` would write straight through.
/// Dropping the second kind from the walk also dropped its declaration, so the
/// licence it was the only user of was reported unused and failed CI over a
/// licence plainly in use.
///
/// Only a declaration whose file actually exists counts. A stale entry naming a
/// deleted path would otherwise keep a dead licence text alive forever, which
/// is the very thing the unused check is for. That also confines this to
/// literal paths: a glob such as `crates/*/LICENSE-*` does not name a file on
/// disk, so it is skipped rather than guessed at.
fn collect_declared_by_ignored(root: &Path, config: &Config, used: &mut BTreeSet<String>) {
    for annotation in &config.annotations {
        let path = root.join(&annotation.path);
        if path.exists() && file::is_excluded(root, &path, &config.ignore) {
            collect_identifiers(Some(&annotation.license), used);
        }
    }
}

/// Lint every covered file under `root`.
pub(crate) fn lint_project(root: &Path, config: &Config) -> Result<Report> {
    let mut builder = Report::builder();
    let mut used_licenses = BTreeSet::new();

    for entry in file::walk_with(root, &config.ignore, &config.source_patterns, config.walk) {
        let entry = entry?;
        let rel_path = entry.relative_to(root);
        builder = lint_entry(builder, &entry, &rel_path, config, &mut used_licenses);
    }

    collect_declared_by_ignored(root, config, &mut used_licenses);

    Ok(builder
        .licenses(license::audit(root, &used_licenses)?)
        .build())
}

/// Returns `true` when the project is compliant.
///
/// The caller decides the process exit code; keeping `process::exit` out of
/// here is what makes the non-compliant path testable.
pub(crate) fn run(cmd: &crate::Command) -> Result<bool> {
    let (path, format) = match cmd {
        crate::Command::Lint {
            path,
            json,
            plain,
            lines,
            quiet,
        } => (
            path.as_ref(),
            Format::resolve(
                output::Flags {
                    quiet: *quiet,
                    json: *json,
                    plain: *plain,
                    lines: *lines,
                },
                Format::Plain,
            ),
        ),
        _ => return Ok(true),
    };

    let root = resolve_root(path)?;
    let config = load_config(&root)?;
    let report = lint_project(&root, &config)?;

    output::emit(&report, &root, format)?;

    Ok(report.is_compliant())
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    const HEADER: &str = concat!(
        "// SPDX-FileCopyrightText: 2026 Test Author\n",
        "// SPDX-License-Identifier: MIT\n"
    );

    fn lint_cmd(root: &Path, json: bool) -> crate::Command {
        crate::Command::Lint {
            path: Some(root.to_str().unwrap().to_string()),
            json,
            plain: false,
            lines: false,
            quiet: false,
        }
    }

    fn project() -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        (tmp, root)
    }

    /// A declared expression that does not parse is not a declaration. comply
    /// used to accept it: `reuse` 6.2.0 reported the same tree under "INVALID
    /// SPDX LICENSE EXPRESSIONS" and exited 1 while comply exited 0, which is
    /// a compliance tool certifying compliance that is not there.
    mod invalid_expressions {
        use super::*;

        fn source_with(expression: &str) -> (TempDir, std::path::PathBuf) {
            let (tmp, root) = project();
            fs::write(root.join("REUSE.toml"), "version = 1\n").unwrap();
            fs::write(
                root.join("src/bad.rs"),
                format!(
                    "// SPDX-FileCopyrightText: 2026 Acme\n\
                     // SPDX-License-Identifier: {expression}\n\nfn a() {{}}\n"
                ),
            )
            .unwrap();
            (tmp, root)
        }

        #[test]
        fn a_malformed_expression_fails_the_file() {
            let (_tmp, root) = source_with("MIT AND AND GPL-3.0-only");
            let report = lint_project(&root, &Config::builder().build()).unwrap();

            let result = report.get("src/bad.rs").expect("file was linted");
            assert!(
                matches!(result.status, Status::InvalidExpression(_)),
                "got {:?}",
                result.status
            );
            assert_eq!(report.failed(), 1, "the file has to count as failed");
        }

        #[test]
        fn the_reported_status_names_the_offending_expression() {
            let (_tmp, root) = source_with("MIT AND AND GPL-3.0-only");
            let report = lint_project(&root, &Config::builder().build()).unwrap();

            let Some(Status::InvalidExpression(expression)) =
                report.get("src/bad.rs").map(|r| r.status.clone())
            else {
                panic!("expected an InvalidExpression status");
            };
            assert_eq!(
                expression, "MIT AND AND GPL-3.0-only",
                "the message has to quote what was rejected"
            );
        }

        #[test]
        fn a_malformed_expression_in_the_manifest_is_caught_too() {
            // The file's own bad tags are filtered by `header::detect` now, so
            // this is the remaining route to an invalid resolved expression:
            // the annotation itself declares one.
            let (_tmp, root) = project();
            fs::write(
                root.join("REUSE.toml"),
                "version = 1\n\n[[annotations]]\n\
                 SPDX-FileCopyrightText = \"2026 Acme\"\n\
                 SPDX-License-Identifier = \"MIT AND AND GPL-3.0-only\"\n\
                 path = [\"**\"]\nprecedence = \"aggregate\"\n",
            )
            .unwrap();
            fs::write(root.join("src/plain.rs"), "fn a() {}\n").unwrap();

            let config = load_config(&root).unwrap();
            let report = lint_project(&root, &config).unwrap();

            let result = report.get("src/plain.rs").expect("linted");
            assert!(
                matches!(result.status, Status::InvalidExpression(_)),
                "got {:?}",
                result.status
            );
        }

        #[test]
        fn a_well_formed_expression_still_passes() {
            // The guard: this must reject malformed input without rejecting
            // legitimate compound expressions.
            let (_tmp, root) = source_with("MIT OR (Apache-2.0 AND MIT)");
            let report = lint_project(&root, &Config::builder().build()).unwrap();

            assert_eq!(
                report.get("src/bad.rs").map(|r| r.status.clone()),
                Some(Status::Ok),
                "a valid compound expression must not be flagged"
            );
        }

        #[test]
        fn an_invalid_expression_does_not_enter_the_used_licence_set() {
            // Otherwise the audit would chase a `LICENSES/` file for a licence
            // that was never validly named, reporting a second, phantom fault.
            let (_tmp, root) = source_with("MIT AND AND GPL-3.0-only");
            let report = lint_project(&root, &Config::builder().build()).unwrap();

            assert!(
                report.licenses().missing.is_empty(),
                "no phantom missing licence, got {:?}",
                report.licenses().missing
            );
        }
    }

    #[test]
    fn a_licence_text_at_the_root_is_skipped_not_judged() {
        let (_tmp, root) = project();
        // `COPYING` classifies as a licence text: it carries a licence rather
        // than needing one. Pruned directories never reach `lint_entry`, so this
        // is the remaining way an Ignored entry gets there.
        fs::write(root.join("COPYING"), "GPL text\n").unwrap();

        let report = lint_project(&root, &Config::builder().build()).unwrap();

        assert_eq!(
            report.get("COPYING").map(|r| r.status.clone()),
            Some(Status::Skipped)
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_a_read_error_not_a_missing_licence() {
        let (_tmp, root) = project();
        // Anything other than `InvalidData` is a genuine read failure and must
        // not be reported as "needs a .license companion", which would send the
        // reader looking for the wrong fix. A vanished path gives NotFound.
        let entry = file::FileEntry {
            path: root.join("src/gone.rs"),
            file_type: FileType::Source,
            has_license_file: false,
        };

        let (status, licensing) = lint_source(&entry, "src/gone.rs", &Config::builder().build());

        assert_eq!(status, Status::ReadError);
        assert_eq!(licensing, Licensing::default());
    }

    #[test]
    fn reports_compliant_when_every_source_file_has_a_header() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), format!("{HEADER}fn a() {{}}\n")).unwrap();
        license_files(&root, &["MIT.txt"]);

        assert!(run(&lint_cmd(&root, false)).unwrap());
    }

    #[test]
    fn reports_non_compliant_when_a_header_is_missing() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();

        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "a source file with no SPDX header must fail the lint"
        );
    }

    #[test]
    fn reports_non_compliant_when_the_licence_is_missing() {
        let (_tmp, root) = project();
        fs::write(
            root.join("src/a.rs"),
            "// SPDX-FileCopyrightText: 2026 Test Author\nfn a() {}\n",
        )
        .unwrap();

        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "copyright without a licence identifier is not compliant"
        );
    }

    #[test]
    fn falls_back_to_a_reuse_toml_annotation_when_the_file_has_no_header() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        fs::write(
            root.join("REUSE.toml"),
            concat!(
                "version = 1\n\n",
                "[[annotations]]\n",
                "SPDX-FileCopyrightText = \"2026 Test Author\"\n",
                "SPDX-License-Identifier = \"MIT\"\n",
                "path = [\"src/**\"]\n",
                "precedence = \"aggregate\"\n"
            ),
        )
        .unwrap();
        license_files(&root, &["MIT.txt"]);

        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "an annotation covering the file makes it compliant without an inline header"
        );
    }

    #[test]
    fn a_binary_file_needs_a_license_companion() {
        let (_tmp, root) = project();
        fs::write(root.join("logo.png"), [0x89, b'P', b'N', b'G', 0x0d]).unwrap();

        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "a binary with no .license companion is not compliant"
        );

        fs::write(root.join("logo.png.license"), HEADER).unwrap();
        license_files(&root, &["MIT.txt"]);
        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "adding the .license companion makes it compliant"
        );
    }

    #[test]
    fn json_output_reports_the_same_verdict() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), format!("{HEADER}fn a() {{}}\n")).unwrap();
        license_files(&root, &["MIT.txt"]);

        assert!(run(&lint_cmd(&root, true)).unwrap());
    }

    /// Writes a REUSE.toml whose single annotation covers `src/**`.
    fn annotate_src(root: &Path, keys: &str, precedence: &str) {
        fs::write(
            root.join("REUSE.toml"),
            format!(
                "version = 1\n\n[[annotations]]\n{keys}\npath = [\"src/**\"]\nprecedence = \"{precedence}\"\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn an_override_annotation_replaces_the_header_wholesale() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), format!("{HEADER}fn a() {{}}\n")).unwrap();
        // Supplies a licence but no copyright. Per REUSE 3.3 an `override`
        // annotation replaces the header outright, so the file is left with no
        // copyright at all -- and is therefore not compliant.
        annotate_src(
            &root,
            "SPDX-License-Identifier = \"Apache-2.0\"",
            "override",
        );

        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "override must discard the header's copyright, not fall back to it"
        );
    }

    #[test]
    fn a_partial_header_is_completed_by_the_annotation() {
        let (_tmp, root) = project();
        fs::write(
            root.join("src/a.rs"),
            "// SPDX-FileCopyrightText: 2026 Test Author\n\nfn a() {}\n",
        )
        .unwrap();
        annotate_src(&root, "SPDX-License-Identifier = \"MIT\"", "closest");
        license_files(&root, &["MIT.txt"]);

        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "closest falls back field by field, so the annotation supplies the missing licence"
        );
    }

    /// Creates `LICENSES/<name>` for each entry, so a fixture can declare
    /// exactly which license files the project ships.
    fn license_files(root: &Path, names: &[&str]) {
        fs::create_dir_all(root.join("LICENSES")).unwrap();
        for name in names {
            fs::write(root.join("LICENSES").join(name), "license text").unwrap();
        }
    }

    #[test]
    fn a_licence_used_without_a_file_in_licenses_fails_the_lint() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), format!("{HEADER}fn a() {{}}\n")).unwrap();

        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "MIT is referenced but LICENSES/ has no file for it"
        );

        license_files(&root, &["MIT.txt"]);
        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "adding the license file makes the project compliant"
        );
    }

    #[test]
    fn a_licence_file_nothing_references_fails_the_lint() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), format!("{HEADER}fn a() {{}}\n")).unwrap();
        license_files(&root, &["MIT.txt", "GPL-3.0-only.txt"]);

        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "a project must not ship a license nothing is under"
        );
    }

    #[test]
    fn a_licence_file_without_an_extension_fails_the_lint() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), format!("{HEADER}fn a() {{}}\n")).unwrap();
        license_files(&root, &["MIT"]);

        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "the License File name must carry an extension"
        );
    }

    #[test]
    fn both_halves_of_a_composite_expression_need_their_own_file() {
        let (_tmp, root) = project();
        fs::write(
            root.join("src/a.rs"),
            "// SPDX-FileCopyrightText: 2026 Test Author\n// SPDX-License-Identifier: MIT OR Apache-2.0\n\nfn a() {}\n",
        )
        .unwrap();
        license_files(&root, &["MIT.txt"]);

        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "Apache-2.0 is referenced by the expression and needs its own file"
        );

        fs::write(root.join("LICENSES/Apache-2.0.txt"), "text").unwrap();
        assert!(run(&lint_cmd(&root, false)).unwrap());
    }

    #[test]
    fn an_ignored_files_declaration_still_counts_its_licence_as_used() {
        // `ignore` carries two intents. `DCO.txt` is there so nothing WRITES to
        // it -- its own text forbids modification -- not because it is outside
        // the project. Dropping it from the walk also dropped its declaration,
        // so the licence it is the only user of was reported unused and CI
        // failed on a licence that is plainly in use.
        let (_tmp, root) = project();
        fs::write(root.join("DCO.txt"), "Developer Certificate of Origin\n").unwrap();
        fs::write(
            root.join("REUSE.toml"),
            "version = 1\n\n\
             [[annotations]]\n\
             SPDX-FileCopyrightText = \"2026 A\"\n\
             SPDX-License-Identifier = \"MIT\"\n\
             path = [\"src/**\"]\n\
             precedence = \"closest\"\n\n\
             [[annotations]]\n\
             SPDX-License-Identifier = \"CC-BY-3.0\"\n\
             path = [\"DCO.txt\"]\n\
             precedence = \"aggregate\"\n\n\
             [tool.comply]\n\
             ignore = [\"DCO.txt\"]\n",
        )
        .unwrap();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        license_files(&root, &["MIT.txt", "CC-BY-3.0.txt"]);

        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "a licence declared for an ignored file is used, not unused"
        );
    }

    #[test]
    fn a_licence_declared_for_a_path_that_does_not_exist_stays_unused() {
        // The narrowing that keeps the fix honest: a declaration only counts
        // when the file it names is actually there. Otherwise a stale entry in
        // REUSE.toml would keep a dead licence text alive forever.
        let (_tmp, root) = project();
        fs::write(
            root.join("REUSE.toml"),
            "version = 1\n\n\
             [[annotations]]\n\
             SPDX-FileCopyrightText = \"2026 A\"\n\
             SPDX-License-Identifier = \"MIT\"\n\
             path = [\"src/**\"]\n\
             precedence = \"closest\"\n\n\
             [[annotations]]\n\
             SPDX-License-Identifier = \"CC-BY-3.0\"\n\
             path = [\"GONE.txt\"]\n\
             precedence = \"aggregate\"\n\n\
             [tool.comply]\n\
             ignore = [\"GONE.txt\"]\n",
        )
        .unwrap();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        license_files(&root, &["MIT.txt", "CC-BY-3.0.txt"]);

        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "nothing carries CC-BY-3.0, so its text is genuinely unused"
        );
    }

    #[test]
    fn a_licence_reaching_a_file_only_through_an_annotation_counts_as_used() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        annotate_src(
            &root,
            "SPDX-FileCopyrightText = \"2026 A\"\nSPDX-License-Identifier = \"MIT\"",
            "closest",
        );
        license_files(&root, &["MIT.txt"]);

        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "a licence declared only in REUSE.toml is still used, not unused"
        );
    }

    #[test]
    fn an_uncommentable_file_can_be_declared_by_an_annotation_alone() {
        let (_tmp, root) = project();
        // JSON has no comment syntax, so it cannot carry a header -- but a
        // REUSE.toml annotation is an equally valid way to license it.
        fs::write(root.join("src/data.json"), "{\"a\":1}\n").unwrap();
        annotate_src(
            &root,
            "SPDX-FileCopyrightText = \"2026 A\"\nSPDX-License-Identifier = \"MIT\"",
            "closest",
        );
        license_files(&root, &["MIT.txt"]);

        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "an annotation covers a file that has nowhere to put a header"
        );
    }

    #[test]
    fn a_companion_licence_counts_as_used() {
        let (_tmp, root) = project();
        fs::write(root.join("src/data.json"), "{\"a\":1}\n").unwrap();
        fs::write(root.join("src/data.json.license"), HEADER).unwrap();
        license_files(&root, &["MIT.txt"]);

        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "the MIT declared by the companion is in use, so MIT.txt is not unused"
        );
    }

    #[test]
    fn an_or_later_licence_is_satisfied_by_the_base_licence_file() {
        let (_tmp, root) = project();
        fs::write(
            root.join("src/a.rs"),
            "// SPDX-FileCopyrightText: 2026 A\n// SPDX-License-Identifier: CC-BY-3.0+\n\nfn a() {}\n",
        )
        .unwrap();
        license_files(&root, &["CC-BY-3.0.txt"]);

        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "`CC-BY-3.0+` is satisfied by LICENSES/CC-BY-3.0.txt"
        );
    }

    #[test]
    fn a_broken_manifest_is_reported_as_such_not_as_missing_copyrights() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), format!("{HEADER}fn a() {{}}\n")).unwrap();
        fs::write(root.join("REUSE.toml"), "this is = = not toml\n").unwrap();

        let err = run(&lint_cmd(&root, false))
            .expect_err("a manifest that cannot be parsed is an error, not a verdict")
            .to_string()
            .to_lowercase();

        assert!(err.contains("reuse.toml"), "got: {err}");
    }

    #[test]
    fn a_file_under_an_ignored_directory_is_skipped_not_failed() {
        let (_tmp, root) = project();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target/generated.rs"), "fn g() {}\n").unwrap();

        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "build output carries no licensing obligation"
        );
    }

    #[test]
    fn a_license_companion_is_read_in_place_of_the_source_file() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        fs::write(root.join("src/a.rs.license"), HEADER).unwrap();
        license_files(&root, &["MIT.txt"]);

        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "per REUSE 7.2 the companion supersedes the file's own content"
        );
    }

    #[test]
    fn unreadable_content_falls_back_to_whether_a_companion_exists() {
        let (_tmp, root) = project();
        // Source extension, but bytes that are not valid UTF-8.
        fs::write(root.join("src/withcompanion.rs"), [0xff, 0xfe, 0x00]).unwrap();
        fs::write(
            root.join("src/withcompanion.rs.license"),
            [0xff, 0xfe, 0x00],
        )
        .unwrap();

        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "a companion is taken as the declaration even when it cannot be parsed"
        );

        fs::write(root.join("src/bare.rs"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "unreadable content with no companion has nothing declaring its licence"
        );
    }

    #[test]
    fn an_annotation_missing_its_licence_does_not_satisfy_the_lint() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        fs::write(
            root.join("REUSE.toml"),
            concat!(
                "version = 1\n\n",
                "[[annotations]]\n",
                "SPDX-FileCopyrightText = \"2026 Test Author\"\n",
                "path = [\"src/**\"]\n",
                "precedence = \"aggregate\"\n"
            ),
        )
        .unwrap();

        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "an annotation needs both a copyright and a licence to count"
        );
    }

    #[test]
    fn a_contributor_only_header_still_consults_the_annotation() {
        let (_tmp, root) = project();
        fs::write(
            root.join("src/a.rs"),
            "// SPDX-FileContributor: Grace Hopper\n\nfn a() {}\n",
        )
        .unwrap();
        fs::write(
            root.join("REUSE.toml"),
            concat!(
                "version = 1\n\n",
                "[[annotations]]\n",
                "SPDX-FileCopyrightText = \"2026 Test Author\"\n",
                "SPDX-License-Identifier = \"MIT\"\n",
                "path = [\"src/**\"]\n",
                "precedence = \"aggregate\"\n"
            ),
        )
        .unwrap();
        license_files(&root, &["MIT.txt"]);

        assert!(
            run(&lint_cmd(&root, false)).unwrap(),
            "a header carrying only a contributor declares nothing, so the annotation decides"
        );
    }

    #[test]
    fn a_licence_with_no_copyright_is_reported_missing_copyright() {
        let (_tmp, root) = project();
        fs::write(
            root.join("src/a.rs"),
            "// SPDX-License-Identifier: MIT\n\nfn a() {}\n",
        )
        .unwrap();

        assert!(
            !run(&lint_cmd(&root, false)).unwrap(),
            "a licence alone is not compliant -- REUSE requires a copyright too"
        );
    }

    #[test]
    fn a_non_lint_command_is_a_no_op() {
        assert!(
            run(&crate::Command::Fix {
                path: None,
                dry_run: true
            })
            .unwrap()
        );
    }
}
// REUSE-IgnoreEnd
