// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Convert `.reuse/dep5` into an equivalent `REUSE.toml`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use comply::config::Config;
use comply::reuse_toml;

use crate::shared::resolve_root;

/// Where DEP5 lives, relative to the project root.
const DEP5: &str = ".reuse/dep5";

pub(crate) fn run(path: Option<&String>, dry_run: bool) -> Result<()> {
    let root = resolve_root(path)?;
    let dep5 = root.join(DEP5);
    let reuse_toml_path = root.join("REUSE.toml");

    if !dep5.exists() {
        bail!("no {DEP5} to convert");
    }
    // REUSE 3.3 forbids a project carrying both, and the existing REUSE.toml may
    // hold annotations this conversion knows nothing about. The reference tool
    // overwrites; refusing is the safer default when data is at stake.
    if reuse_toml_path.exists() {
        bail!(
            "REUSE.toml already exists; remove it first, or merge {DEP5} by hand -- \
             a project must not declare licensing both ways"
        );
    }

    let document =
        Config::dep5_document(&dep5).with_context(|| format!("failed to read {DEP5}"))?;
    let rendered = reuse_toml::render_dep5_as_toml(&document);

    if dry_run {
        print!("{rendered}");
        return Ok(());
    }

    fs::write(&reuse_toml_path, &rendered).context("failed to write REUSE.toml")?;
    // The whole point is to migrate: leaving dep5 behind would leave the project
    // declaring licensing twice, which its own lint then rejects.
    fs::remove_file(&dep5).with_context(|| format!("failed to remove {DEP5}"))?;
    prune_empty_reuse_dir(&root);

    println!(
        "Converted {DEP5} into REUSE.toml ({} annotation(s))",
        document.annotations.len()
    );
    Ok(())
}

/// Remove `.reuse/` once dep5 was its only occupant, so no empty directory is
/// left behind. A directory that still holds anything is left alone.
fn prune_empty_reuse_dir(root: &Path) {
    let dir = root.join(".reuse");
    if dir
        .read_dir()
        .is_ok_and(|mut entries| entries.next().is_none())
    {
        let _ = fs::remove_dir(&dir);
    }
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    const DEP5_BODY: &str = concat!(
        "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n",
        "Upstream-Name: example\n",
        "\n",
        "Files: src/*\n",
        "Copyright: 2026 Jane Doe\n",
        "License: MIT\n",
    );

    fn project(dep5: Option<&str>) -> (TempDir, PathBuf, String) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        if let Some(body) = dep5 {
            fs::create_dir_all(root.join(".reuse")).unwrap();
            fs::write(root.join(DEP5), body).unwrap();
        }
        let arg = root.to_str().unwrap().to_string();
        (tmp, root, arg)
    }

    #[test]
    fn a_dep5_project_becomes_a_reuse_toml_project() {
        let (_tmp, root, arg) = project(Some(DEP5_BODY));

        run(Some(&arg), false).unwrap();

        let written = fs::read_to_string(root.join("REUSE.toml")).unwrap();
        assert!(written.starts_with("version = 1\n"), "got:\n{written}");
        assert!(written.contains("SPDX-PackageName = \"example\""));
        assert!(written.contains("path = \"src/**\""));
        assert!(
            !root.join(DEP5).exists(),
            "dep5 must go, or the project declares licensing twice"
        );
        assert!(
            !root.join(".reuse").exists(),
            "an emptied .reuse/ should not be left behind"
        );
    }

    #[test]
    fn a_dry_run_prints_without_touching_either_file() {
        let (_tmp, root, arg) = project(Some(DEP5_BODY));

        run(Some(&arg), true).unwrap();

        assert!(root.join(DEP5).exists(), "--dry-run must not delete dep5");
        assert!(
            !root.join("REUSE.toml").exists(),
            "--dry-run must not write REUSE.toml"
        );
    }

    #[test]
    fn a_project_with_no_dep5_says_so() {
        let (_tmp, _root, arg) = project(None);

        let err = run(Some(&arg), false).unwrap_err();

        assert!(format!("{err:#}").contains(DEP5), "got: {err:#}");
    }

    #[test]
    fn an_existing_reuse_toml_is_refused_rather_than_overwritten() {
        let (_tmp, root, arg) = project(Some(DEP5_BODY));
        fs::write(root.join("REUSE.toml"), "version = 1\n# hand-written\n").unwrap();

        let err = run(Some(&arg), false).unwrap_err();

        assert!(
            format!("{err:#}").contains("already exists"),
            "got: {err:#}"
        );
        assert!(
            fs::read_to_string(root.join("REUSE.toml"))
                .unwrap()
                .contains("hand-written"),
            "the existing file must survive untouched"
        );
    }

    #[test]
    fn a_reuse_dir_holding_other_files_is_kept() {
        let (_tmp, root, arg) = project(Some(DEP5_BODY));
        fs::write(root.join(".reuse/templates"), "x").unwrap();

        run(Some(&arg), false).unwrap();

        assert!(
            root.join(".reuse").exists(),
            ".reuse/ still has contents, so it must stay"
        );
    }
}
// REUSE-IgnoreEnd
