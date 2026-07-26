// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;

use anyhow::{Context, Result};
use comply::header;

use crate::shared::{load_config, resolve_root, walk_source_files};

/// Returns `true` when nothing needs reformatting.
///
/// The caller decides the process exit code; keeping `process::exit` out of
/// here is what makes the would-reformat path testable.
pub(crate) fn run(path: Option<&String>, files: &[String], check: bool) -> Result<bool> {
    let root = resolve_root(path)?;
    let config = load_config(&root)?;

    let target_files: Vec<String> = if files.is_empty() {
        walk_source_files(&root, &config)?
    } else {
        files.to_vec()
    };

    let mut changed = 0;
    let mut unchanged = 0;

    for rel_path in &target_files {
        let full_path = root.join(rel_path);

        if !full_path.exists() {
            eprintln!("Warning: {rel_path} does not exist, skipping");
            continue;
        }

        let content =
            fs::read_to_string(&full_path).with_context(|| format!("failed to read {rel_path}"))?;

        let style = header::detect_style(rel_path);

        if style == header::CommentStyle::Uncommentable {
            if check {
                eprintln!("Warning: {rel_path} is not commentable, skipping");
            }
            continue;
        }

        let new_content = header::format_file(&content, Some(style))?;

        if new_content == content {
            unchanged += 1;
            if !check {
                println!("Unchanged: {rel_path}");
            }
        } else if check {
            println!("Would reformat: {rel_path}");
            changed += 1;
        } else {
            fs::write(&full_path, new_content)
                .with_context(|| format!("failed to write {rel_path}"))?;
            println!("Updated: {rel_path}");
            changed += 1;
        }
    }

    if check {
        if changed > 0 {
            eprintln!("{changed} file(s) would be reformatted");
            return Ok(false);
        }
        println!("All files are correctly formatted");
    } else {
        println!("Reformatted {changed} file(s), {unchanged} unchanged");
    }

    Ok(true)
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Canonical for a `.rs` file: slash comments.
    const CANONICAL: &str = concat!(
        "// SPDX-FileCopyrightText: 2026 Test Author\n",
        "//\n",
        "// SPDX-License-Identifier: MIT\n",
        "\n"
    );
    /// Same header in the wrong comment style for a `.rs` file.
    const WRONG_STYLE: &str = concat!(
        "# SPDX-FileCopyrightText: 2026 Test Author\n",
        "# SPDX-License-Identifier: MIT\n"
    );

    fn project() -> (TempDir, PathBuf, String) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        let root_arg = root.to_str().unwrap().to_string();
        (tmp, root, root_arg)
    }

    #[test]
    fn check_passes_when_every_file_is_already_canonical() {
        let (_tmp, root, arg) = project();
        fs::write(root.join("src/a.rs"), format!("{CANONICAL}fn a() {{}}\n")).unwrap();

        assert!(run(Some(&arg), &[], true).unwrap());
    }

    #[test]
    fn check_fails_when_a_file_would_be_reformatted() {
        let (_tmp, root, arg) = project();
        fs::write(root.join("src/a.rs"), format!("{WRONG_STYLE}fn a() {{}}\n")).unwrap();

        assert!(
            !run(Some(&arg), &[], true).unwrap(),
            "a header in the wrong comment style must be reported as needing reformatting"
        );
    }

    #[test]
    fn check_mode_does_not_write() {
        let (_tmp, root, arg) = project();
        let file = root.join("src/a.rs");
        let original = format!("{WRONG_STYLE}fn a() {{}}\n");
        fs::write(&file, &original).unwrap();

        run(Some(&arg), &[], true).unwrap();

        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            original,
            "--check must never modify a file"
        );
    }

    #[test]
    fn write_mode_rewrites_the_header_into_the_canonical_style() {
        let (_tmp, root, arg) = project();
        let file = root.join("src/a.rs");
        fs::write(&file, format!("{WRONG_STYLE}fn a() {{}}\n")).unwrap();

        assert!(run(Some(&arg), &[], false).unwrap());

        let after = fs::read_to_string(&file).unwrap();
        assert!(
            after.contains("// SPDX-License-Identifier: MIT"),
            "header should be rewritten with slash comments, got:\n{after}"
        );
        assert!(after.contains("fn a() {}"), "body must survive:\n{after}");
    }

    #[test]
    fn write_mode_is_idempotent() {
        let (_tmp, root, arg) = project();
        let file = root.join("src/a.rs");
        fs::write(&file, format!("{CANONICAL}fn a() {{}}\n")).unwrap();
        let before = fs::read_to_string(&file).unwrap();

        run(Some(&arg), &[], false).unwrap();

        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            before,
            "an already-canonical file must come out byte-identical"
        );
    }

    #[test]
    fn an_explicit_file_list_is_honoured_in_write_mode() {
        let (_tmp, root, arg) = project();
        fs::write(root.join("src/a.rs"), format!("{WRONG_STYLE}fn a() {{}}\n")).unwrap();
        fs::write(root.join("src/b.rs"), format!("{WRONG_STYLE}fn b() {{}}\n")).unwrap();

        run(Some(&arg), &["src/a.rs".to_string()], false).unwrap();

        assert!(
            fs::read_to_string(root.join("src/a.rs"))
                .unwrap()
                .contains("// SPDX-License-Identifier"),
            "the listed file is reformatted"
        );
        assert!(
            fs::read_to_string(root.join("src/b.rs"))
                .unwrap()
                .contains("# SPDX-License-Identifier"),
            "a file outside the list is left alone"
        );
    }

    #[test]
    fn a_missing_file_is_skipped_rather_than_failing() {
        let (_tmp, _root, arg) = project();

        assert!(
            run(Some(&arg), &["src/does-not-exist.rs".to_string()], false).unwrap(),
            "a missing path warns and continues"
        );
    }

    #[test]
    fn check_honours_an_explicit_file_list() {
        let (_tmp, root, arg) = project();
        fs::write(
            root.join("src/listed.rs"),
            format!("{CANONICAL}fn a() {{}}\n"),
        )
        .unwrap();
        fs::write(
            root.join("src/unlisted.rs"),
            format!("{WRONG_STYLE}fn b() {{}}\n"),
        )
        .unwrap();

        assert!(
            run(Some(&arg), &["src/listed.rs".to_string()], true).unwrap(),
            "--check must look only at the files it was given, like --write does"
        );
    }

    #[test]
    fn an_explicitly_listed_uncommentable_file_is_passed_over() {
        let (_tmp, root, arg) = project();
        // Valid UTF-8, so the read succeeds and the style check is what decides.
        fs::write(root.join("logo.png"), "not really a png\n").unwrap();

        assert!(run(Some(&arg), &["logo.png".to_string()], false).unwrap());
        assert_eq!(
            fs::read_to_string(root.join("logo.png")).unwrap(),
            "not really a png\n",
            "a file with no comment syntax must be left alone"
        );
    }

    #[test]
    fn check_warns_about_an_uncommentable_file_without_failing() {
        let (_tmp, root, arg) = project();
        // Valid UTF-8, so the read succeeds and the style check is what decides.
        fs::write(root.join("logo.png"), "not really a png\n").unwrap();

        assert!(
            run(Some(&arg), &["logo.png".to_string()], true).unwrap(),
            "an uncommentable file is warned about, not counted as needing reformatting"
        );
    }

    /// Documents CURRENT behaviour, which is arguably wrong: the file is read
    /// (and fails as non-UTF-8) before the `Uncommentable` branch is reached,
    /// so an explicitly listed binary errors instead of being skipped. Moving
    /// the style check above the read would make this a clean skip.
    #[test]
    fn an_explicitly_listed_binary_currently_errors_instead_of_skipping() {
        let (_tmp, root, arg) = project();
        fs::write(root.join("logo.png"), [0x89, b'P', b'N', b'G']).unwrap();

        assert!(run(Some(&arg), &["logo.png".to_string()], false).is_err());
    }
}
// REUSE-IgnoreEnd
