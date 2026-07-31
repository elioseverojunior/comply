// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::cli::shared::{load_config, resolve_root};
use anyhow::{Context, Result};
use comply::fix;

#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub(crate) fn run(path: Option<String>, dry_run: bool) -> Result<()> {
    let root = resolve_root(path.as_ref())?;
    let config = load_config(&root)?;

    let report =
        fix::fix_project(&root, &config, dry_run).context("failed to fix compliance issues")?;

    for (path, action) in &report.actions {
        if let Some(line) = describe(path, action) {
            println!("{line}");
        }
    }

    let total = report.total;
    let fixed = report.fixed;
    let unchanged = report.unchanged;
    let failed = report.failed;
    println!("\nSummary: {total} files, {fixed} fixed, {unchanged} unchanged, {failed} failed");

    Ok(())
}

/// The line to print for one action, or `None` when there is nothing to say.
///
/// A file comply left alone is not news. On this repository that was 101 of 108
/// lines, which buried the seven that mattered; the summary still counts them.
fn describe(path: &str, action: &fix::FixAction) -> Option<String> {
    let label = match action {
        fix::FixAction::Unchanged => return None,
        fix::FixAction::HeaderAdded => "header added",
        fix::FixAction::HeaderReformatted => "header reformatted",
        fix::FixAction::LicenseFileCreated => "license file created",
        fix::FixAction::BinaryLicenseMissing => {
            return Some(format!(
                "FAILED {path}: binary license missing (needs manual fix)"
            ));
        }
        fix::FixAction::Failed(reason) => return Some(format!("FAILED {path}: {reason}")),
    };
    Some(format!("{label:40} {path}"))
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn a_file_comply_left_alone_gets_no_line() {
        assert_eq!(describe("src/lib.rs", &fix::FixAction::Unchanged), None);
    }

    #[test]
    fn everything_comply_touched_or_could_not_touch_gets_a_line() {
        for action in [
            fix::FixAction::HeaderAdded,
            fix::FixAction::HeaderReformatted,
            fix::FixAction::LicenseFileCreated,
            fix::FixAction::BinaryLicenseMissing,
            fix::FixAction::Failed("unreadable".to_owned()),
        ] {
            let line = describe("src/lib.rs", &action);
            assert!(line.is_some(), "{action:?} must be reported");
            assert!(
                line.unwrap_or_default().contains("src/lib.rs"),
                "{action:?} must name the file"
            );
        }
    }

    #[test]
    fn a_failure_line_carries_the_reason() {
        let line =
            describe("a.bin", &fix::FixAction::Failed("no header".to_owned())).unwrap_or_default();
        assert!(line.starts_with("FAILED"), "got {line}");
        assert!(line.contains("no header"), "got {line}");
    }

    const REUSE_TOML: &str = concat!(
        "version = 1\n\n",
        "[[annotations]]\n",
        "SPDX-FileCopyrightText = \"2026 Test Author\"\n",
        "SPDX-License-Identifier = \"MIT\"\n",
        "path = [\"**\"]\n",
        "precedence = \"aggregate\"\n"
    );

    /// A project whose REUSE.toml supplies the copyright/licence that `fix`
    /// needs in order to know what header to write.
    fn project() -> (TempDir, PathBuf, String) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("REUSE.toml"), REUSE_TOML).unwrap();
        let arg = root.to_str().unwrap().to_string();
        (tmp, root, arg)
    }

    #[test]
    fn a_file_the_manifest_already_licenses_is_left_alone() {
        // The fixture's REUSE.toml carries a `**` aggregate, so `src/a.rs` is
        // compliant the moment it exists and `lint` passes it. `fix` writing a
        // header into it anyway is a blanket rewrite of a file nobody named --
        // and it reported work on a tree lint called clean. `annotate` is the
        // opt-in writer when a per-file header is actually wanted.
        let (_tmp, root, arg) = project();
        let file = root.join("src/a.rs");
        fs::write(&file, "fn a() {}\n").unwrap();

        run(Some(arg), false).unwrap();

        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "fn a() {}\n",
            "an already-licensed file must come back byte-identical"
        );
    }

    #[test]
    fn dry_run_leaves_the_file_untouched() {
        let (_tmp, root, arg) = project();
        let file = root.join("src/a.rs");
        fs::write(&file, "fn a() {}\n").unwrap();

        run(Some(arg), true).unwrap();

        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "fn a() {}\n",
            "--dry-run must not write anything"
        );
    }

    #[test]
    fn is_idempotent_on_an_already_compliant_file() {
        let (_tmp, root, arg) = project();
        let file = root.join("src/a.rs");
        fs::write(&file, "fn a() {}\n").unwrap();

        run(Some(arg.clone()), false).unwrap();
        let first = fs::read_to_string(&file).unwrap();
        run(Some(arg), false).unwrap();

        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            first,
            "a second fix run must be a no-op"
        );
    }

    #[test]
    fn a_binary_the_manifest_already_licenses_gets_no_companion() {
        // The fixture's `**` aggregate declares this binary's licensing, so it
        // is compliant and `lint` passes it -- a companion would restate what
        // the manifest already says. The write path, which only the defaults
        // can trigger, is covered by `fix::tests` in the core crate.
        let (_tmp, root, arg) = project();
        fs::write(root.join("logo.png"), [0x89, b'P', b'N', b'G']).unwrap();

        run(Some(arg), false).unwrap();

        assert!(
            !root.join("logo.png.license").exists(),
            "an aggregate-licensed binary needs no companion"
        );
    }

    #[test]
    fn fix_and_annotate_write_byte_identical_companions() {
        // The deliverable for closing the `fix` vs `annotate` companion drift
        // recorded in docs/parity.md: both commands must produce the exact
        // same bytes for the same copyright and licence, so a project running
        // both in sequence does not see them churn each other's file.
        let fix_tmp = TempDir::new().unwrap();
        let fix_root = fix_tmp.path().canonicalize().unwrap();
        fs::write(
            fix_root.join("REUSE.toml"),
            concat!(
                "version = 1\n\n",
                "[default]\n",
                "SPDX-FileCopyrightText = [\"2026 Acme\"]\n",
                "SPDX-License-Identifier = \"MIT\"\n",
            ),
        )
        .unwrap();
        fs::write(fix_root.join("logo.png"), [0x89, b'P', b'N', b'G']).unwrap();

        run(Some(fix_root.to_str().unwrap().to_string()), false).unwrap();
        let fix_companion = fs::read_to_string(fix_root.join("logo.png.license")).unwrap();

        let annotate_tmp = TempDir::new().unwrap();
        let annotate_root = annotate_tmp.path().canonicalize().unwrap();
        fs::write(annotate_root.join("logo.png"), [0x89, b'P', b'N', b'G']).unwrap();

        crate::cli::commands::annotate::run(&crate::Command::Annotate {
            files: vec![],
            path: Some(annotate_root.to_str().unwrap().to_string()),
            license: Some("MIT".to_string()),
            copyright: Some("Acme".to_string()),
            year: Some("2026".to_string()),
            contributor: vec![],
            holder: vec![],
            recursive: false,
            exclude_year: false,
            merge_copyrights: false,
            copyright_prefix: "spdx".to_string(),
            force_dot_license: false,
            force: false,
            update_reuse_toml: false,
            precedence: "override".to_string(),
            skip_unrecognised: false,
        })
        .unwrap();
        let annotate_companion =
            fs::read_to_string(annotate_root.join("logo.png.license")).unwrap();

        assert_eq!(
            fix_companion, annotate_companion,
            "fix and annotate must write byte-identical companions"
        );
    }

    #[test]
    fn reports_reformatted_failed_and_unfixable_binary_in_one_pass() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        // No annotations and no defaults: a file without a usable header has
        // nothing to fall back on, which is what drives the failure branches.
        fs::write(root.join("REUSE.toml"), "version = 1\n").unwrap();

        fs::write(
            root.join("src/reformat.rs"),
            concat!(
                "//SPDX-FileCopyrightText:2025 Acme\n",
                "//SPDX-License-Identifier:MIT\n",
                "\n",
                "fn a() {}\n"
            ),
        )
        .unwrap();
        fs::write(root.join("src/bare.rs"), "fn b() {}\n").unwrap();
        fs::write(root.join("logo.png"), [0x89, b'P', b'N', b'G']).unwrap();

        run(Some(root.to_str().unwrap().to_string()), false).unwrap();

        assert!(
            fs::read_to_string(root.join("src/reformat.rs"))
                .unwrap()
                .starts_with("// SPDX-FileCopyrightText: "),
            "a complete but malformed header is reformatted in place"
        );
        assert_eq!(
            fs::read_to_string(root.join("src/bare.rs")).unwrap(),
            "fn b() {}\n",
            "with no annotation to draw on the file is reported failed, not rewritten"
        );
        assert!(
            !root.join("logo.png.license").exists(),
            "a binary with no annotation is reported, not invented"
        );
    }

    #[test]
    fn succeeds_on_an_empty_project() {
        let (_tmp, _root, arg) = project();

        assert!(run(Some(arg), false).is_ok());
    }

    #[test]
    fn defaults_to_the_current_directory_when_no_path_is_given() {
        // `None` resolves to ".", so this exercises the default-path branch
        // against the real repo, which is already compliant.
        assert!(run(None, true).is_ok());
    }
}
// REUSE-IgnoreEnd
