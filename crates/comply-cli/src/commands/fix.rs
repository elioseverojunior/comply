// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::shared::{load_config, resolve_root};
use anyhow::{Context, Result};
use comply::fix;

#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub(crate) fn run(path: Option<String>, dry_run: bool) -> Result<()> {
    let root = resolve_root(path.as_ref())?;
    let config = load_config(&root)?;

    let report =
        fix::fix_project(&root, &config, dry_run).context("failed to fix compliance issues")?;

    for (path, action) in &report.actions {
        let action_str = match action {
            fix::FixAction::Unchanged => "unchanged",
            fix::FixAction::HeaderAdded => "header added",
            fix::FixAction::HeaderReformatted => "header reformatted",
            fix::FixAction::LicenseFileCreated => "license file created",
            fix::FixAction::BinaryLicenseMissing => {
                println!("FAILED {path}: binary license missing (needs manual fix)");
                continue;
            }
            fix::FixAction::Failed(reason) => {
                println!("FAILED {path}: {reason}");
                continue;
            }
        };
        println!("{action_str:40} {path}");
    }

    let total = report.total;
    let fixed = report.fixed;
    let unchanged = report.unchanged;
    let failed = report.failed;
    println!("\nSummary: {total} files, {fixed} fixed, {unchanged} unchanged, {failed} failed");

    Ok(())
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

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
    fn adds_a_missing_header() {
        let (_tmp, root, arg) = project();
        let file = root.join("src/a.rs");
        fs::write(&file, "fn a() {}\n").unwrap();

        run(Some(arg), false).unwrap();

        let after = fs::read_to_string(&file).unwrap();
        assert!(
            after.contains("SPDX-License-Identifier: MIT"),
            "fix should write the licence from REUSE.toml, got:\n{after}"
        );
        assert!(
            after.contains("SPDX-FileCopyrightText: 2026 Test Author"),
            "fix should write the copyright from REUSE.toml, got:\n{after}"
        );
        assert!(after.contains("fn a() {}"), "body must survive:\n{after}");
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
    fn creates_a_license_companion_for_a_binary() {
        let (_tmp, root, arg) = project();
        fs::write(root.join("logo.png"), [0x89, b'P', b'N', b'G']).unwrap();

        run(Some(arg), false).unwrap();

        let companion = root.join("logo.png.license");
        assert!(
            companion.exists(),
            "a binary should get a .license companion"
        );
        assert!(
            fs::read_to_string(&companion)
                .unwrap()
                .contains("SPDX-License-Identifier: MIT")
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
