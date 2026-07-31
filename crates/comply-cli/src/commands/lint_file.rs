// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Lint an explicit list of files rather than a whole project.
//!
//! This is the pre-commit shape: a hook passes only the staged paths, so the
//! cost is proportional to the change instead of to the repository.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use comply::config::Config;
use comply::file::{self, FileEntry};
use comply::license::{self, LicenseAudit};
use comply::report::Report;

use crate::commands::lint::lint_entry;
use crate::output::{self, Format};
use crate::shared::{load_config, resolve_root};

/// Build the entry for one named path, classifying it exactly as a walk would.
///
/// `walk` cannot be reused here: it discovers files, and the whole point is to
/// judge the paths the caller named -- including ones a walk would skip.
fn entry_for(path: &Path, root: &Path) -> Result<FileEntry> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let absolute = absolute
        .canonicalize()
        .with_context(|| format!("cannot read {}", path.display()))?;

    Ok(FileEntry {
        file_type: file::classify_path(&absolute, &[]),
        has_license_file: file::license_companion_path(&absolute).exists(),
        path: absolute,
    })
}

/// Lint the named files, returning the report.
pub(crate) fn lint_files(root: &Path, config: &Config, files: &[String]) -> Result<Report> {
    let mut builder = Report::builder();
    let mut used_licenses = BTreeSet::new();

    for name in files {
        let entry = entry_for(&PathBuf::from(name), root)?;
        let rel_path = entry.relative_to(root);

        // The same scope rules `lint` gets from its walk. Without this a named
        // path bypassed them entirely, so a gitignored file or one matched by
        // `[tool.comply] ignore` was judged here and skipped there -- and the
        // pre-commit hook, which passes staged paths, failed on files the
        // project had explicitly excluded.
        if file::is_excluded(root, &entry.path, &config.ignore) {
            builder = builder.add_skipped(&rel_path);
            continue;
        }

        builder = lint_entry(builder, &entry, &rel_path, config, &mut used_licenses);
    }

    // Only `missing` survives the audit here. The other findings -- unused
    // licences, ones without an extension, bad or deprecated identifiers -- are
    // properties of the whole project and cannot be judged from the handful of
    // files the caller named: every licence the other files use would read as
    // unused. Verified against reuse 6.2.0, where `lint-file` on a compliant
    // file exits 0 in a project that has an unused licence, while `lint` on the
    // same project exits 1.
    let project = license::audit(root, &used_licenses)?;
    let named_only = LicenseAudit {
        missing: project.missing,
        ..LicenseAudit::default()
    };

    Ok(builder.licenses(named_only).build())
}

/// Returns `true` when every named file is compliant.
pub(crate) fn run(cmd: &crate::Command) -> Result<bool> {
    let (files, path, format) = match cmd {
        crate::Command::LintFile {
            files,
            path,
            lines,
            quiet,
        } => (
            files,
            path.as_ref(),
            // `lint-file` defaults to `--lines`, unlike `lint`; matches reuse 6.2.0.
            Format::resolve(
                output::Flags {
                    quiet: *quiet,
                    lines: *lines,
                    ..output::Flags::default()
                },
                Format::Lines,
            ),
        ),
        _ => return Ok(true),
    };

    if files.is_empty() {
        anyhow::bail!("no files given; pass at least one path to lint");
    }

    let root = resolve_root(path)?;
    let config = load_config(&root)?;
    let report = lint_files(&root, &config, files)?;

    output::emit(&report, &root, format)?;

    Ok(report.is_compliant())
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use comply::report::Status;
    use std::fs;
    use tempfile::TempDir;

    const HEADER: &str = concat!(
        "// SPDX-FileCopyrightText: 2026 Test Author\n",
        "// SPDX-License-Identifier: MIT\n"
    );

    fn project() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("LICENSES")).unwrap();
        fs::write(root.join("LICENSES/MIT.txt"), "MIT text").unwrap();
        (tmp, root)
    }

    fn cmd(root: &Path, files: &[&str], lines: bool, quiet: bool) -> crate::Command {
        crate::Command::LintFile {
            files: files.iter().map(ToString::to_string).collect(),
            path: Some(root.to_str().unwrap().to_string()),
            lines,
            quiet,
        }
    }

    #[test]
    fn another_subcommand_is_a_no_op_rather_than_a_panic() {
        // `run` takes the whole Command so main's dispatch stays uniform; a
        // mismatched variant must simply pass.
        let cmd = crate::Command::Fix {
            path: None,
            dry_run: true,
        };

        assert!(run(&cmd).unwrap());
    }

    #[test]
    fn a_compliant_file_passes() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), format!("{HEADER}fn a() {{}}\n")).unwrap();

        assert!(run(&cmd(&root, &["src/a.rs"], false, true)).unwrap());
    }

    #[test]
    fn a_licence_the_project_does_not_use_elsewhere_does_not_fail_this_file() {
        let (_tmp, root) = project();
        // A second licence file nothing in the named set references. Judged over
        // the whole project that is an "unused licence" finding, but a caller
        // linting one file cannot be told about the other files' licences --
        // reuse 6.2.0 exits 0 here too.
        fs::write(root.join("LICENSES/Apache-2.0.txt"), "Apache text").unwrap();
        fs::write(root.join("src/a.rs"), format!("{HEADER}fn a() {{}}\n")).unwrap();

        assert!(
            run(&cmd(&root, &["src/a.rs"], false, true)).unwrap(),
            "an unused licence elsewhere in the project must not fail this file"
        );
    }

    #[test]
    fn a_licence_this_file_names_but_licenses_lacks_still_fails() {
        let (_tmp, root) = project();
        fs::write(
            root.join("src/a.rs"),
            "// SPDX-FileCopyrightText: 2026 T\n// SPDX-License-Identifier: GPL-3.0-only\nfn a() {}\n",
        )
        .unwrap();

        assert!(
            !run(&cmd(&root, &["src/a.rs"], false, true)).unwrap(),
            "the missing-licence finding is per-file and must survive"
        );
    }

    #[test]
    fn an_excluded_path_is_skipped_rather_than_judged() {
        let (_tmp, root) = project();
        fs::write(root.join(".gitignore"), "generated.rs\n").unwrap();
        fs::write(root.join("generated.rs"), "fn g() {}\n").unwrap();

        let report = lint_files(
            &root,
            &load_config(&root).unwrap(),
            &["generated.rs".to_string()],
        )
        .unwrap();

        // `lint` would never have discovered it; naming it explicitly must not
        // produce a different verdict.
        assert_eq!(
            report.get("generated.rs").map(|r| r.status.clone()),
            Some(Status::Skipped)
        );
        assert!(report.is_compliant());
    }

    #[test]
    fn a_file_without_a_header_fails() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();

        assert!(!run(&cmd(&root, &["src/a.rs"], false, true)).unwrap());
    }

    #[test]
    fn only_the_named_files_are_judged() {
        let (_tmp, root) = project();
        fs::write(root.join("src/good.rs"), format!("{HEADER}fn g() {{}}\n")).unwrap();
        fs::write(root.join("src/bad.rs"), "fn b() {}\n").unwrap();

        // The whole project is non-compliant, but the named file is fine --
        // that difference is the entire point of the subcommand.
        assert!(run(&cmd(&root, &["src/good.rs"], false, true)).unwrap());

        let report = lint_files(
            &root,
            &load_config(&root).unwrap(),
            &["src/good.rs".to_string()],
        )
        .unwrap();
        assert_eq!(report.total(), 1);
    }

    #[test]
    fn an_absolute_path_is_accepted_and_reported_repo_relative() {
        let (_tmp, root) = project();
        let file = root.join("src/a.rs");
        fs::write(&file, "fn a() {}\n").unwrap();

        let report = lint_files(
            &root,
            &load_config(&root).unwrap(),
            &[file.to_str().unwrap().to_string()],
        )
        .unwrap();

        assert!(report.get("src/a.rs").is_some(), "got {report:?}");
    }

    #[test]
    fn a_path_that_does_not_exist_is_an_error_naming_it() {
        let (_tmp, root) = project();

        let err = run(&cmd(&root, &["src/nope.rs"], false, true)).unwrap_err();

        assert!(format!("{err:#}").contains("src/nope.rs"), "got: {err:#}");
    }

    #[test]
    fn no_files_is_an_error_rather_than_a_silent_pass() {
        let (_tmp, root) = project();

        let err = run(&cmd(&root, &[], false, true)).unwrap_err();

        assert!(
            format!("{err:#}").contains("no files given"),
            "got: {err:#}"
        );
    }

    #[test]
    fn a_binary_with_a_license_companion_passes() {
        let (_tmp, root) = project();
        fs::write(root.join("logo.png"), [0x89, b'P', b'N', b'G']).unwrap();
        fs::write(root.join("logo.png.license"), HEADER).unwrap();

        assert!(run(&cmd(&root, &["logo.png"], false, true)).unwrap());
    }

    #[test]
    fn the_lines_format_runs() {
        let (_tmp, root) = project();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();

        assert!(!run(&cmd(&root, &["src/a.rs"], true, false)).unwrap());
    }
}
// REUSE-IgnoreEnd
