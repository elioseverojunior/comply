// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;

use anyhow::{Context, Result};
use chrono::Datelike;
use comply::file::{self, FileType};
use comply::header;

use crate::shared::{load_config, resolve_root, to_repo_relative, write_reuse_toml_override};

#[allow(clippy::too_many_lines)]
pub(crate) fn run(cmd: &crate::Command) -> Result<()> {
    let crate::Command::Annotate {
        files,
        path,
        license,
        copyright,
        year,
        contributor,
        holder,
        recursive,
        exclude_year,
        force_dot_license,
        force,
        update_reuse_toml,
        precedence,
        skip_unrecognised,
        ..
    } = cmd
    else {
        return Ok(());
    };

    let root = resolve_root(path.as_ref())?;
    let config = load_config(&root)?;

    let current_year = format!("{}", chrono::Utc::now().year());

    // Pre-compute final license/copyright for REUSE.toml update
    let final_license = license.as_deref().unwrap_or("").to_string();
    let final_copyright = if holder.is_empty() {
        copyright.as_deref().unwrap_or("").to_string()
    } else {
        holder
            .iter()
            .map(|h| format!("{current_year} {h}"))
            .collect::<Vec<_>>()
            .join(", ")
    };

    // Determine target files
    let target_files: Vec<String> = if !files.is_empty() && !*recursive {
        files.iter().map(|f| to_repo_relative(&root, f)).collect()
    } else {
        let mut targets = Vec::new();
        if *recursive && !files.is_empty() {
            // Treat each file path as a directory root for recursive walk
            for f in files {
                let dir_path = root.join(f);
                if dir_path.is_dir() {
                    for entry in file::walk(&dir_path, &config.ignore, &config.source_patterns) {
                        let entry = entry?;
                        if entry.file_type == FileType::Source {
                            let rel = entry.relative_to(&root);
                            targets.push(rel);
                        }
                    }
                } else {
                    targets.push(to_repo_relative(&root, f));
                }
            }
        } else {
            // Walk all source files from root
            for entry in file::walk(&root, &config.ignore, &config.source_patterns) {
                let entry = entry?;
                if entry.file_type == FileType::Source {
                    let rel_path = entry.relative_to(&root);
                    targets.push(rel_path);
                }
            }
        }
        targets
    };

    for rel_path in &target_files {
        let full_path = root.join(rel_path);

        if !full_path.exists() {
            eprintln!("Warning: {rel_path} does not exist, skipping");
            continue;
        }

        // What REUSE.toml alone says about this file: the flags below override
        // it, so the file's own header is deliberately not fed in here.
        let from_config = config.resolve_licensing(rel_path, &[], None);

        let resolved_license = license.clone().or(from_config.license).unwrap_or_default();

        let resolved_copyright = if holder.is_empty() {
            copyright.clone().map_or_else(
                || from_config.copyrights.join(", "),
                |c| {
                    let year_str = year.as_deref().unwrap_or(&current_year);
                    format!("{year_str} {c}")
                },
            )
        } else {
            // Build copyright from --holder arguments
            let year_str = year.as_deref().unwrap_or(&current_year);
            holder
                .iter()
                .map(|h| format!("{year_str} {h}"))
                .collect::<Vec<_>>()
                .join(", ")
        };

        if resolved_license.is_empty() && resolved_copyright.is_empty() {
            eprintln!(
                "Warning: {rel_path} has no license or copyright info. Use --license/--copyright or add to REUSE.toml"
            );
            continue;
        }

        if resolved_license.is_empty() {
            eprintln!("Warning: {rel_path} has no license. Use --license or add to REUSE.toml");
            continue;
        }

        let style = header::detect_style(rel_path);

        if style == header::CommentStyle::Uncommentable {
            if *force_dot_license {
                // Write .license companion file
                let license_path = file::license_companion_path(&full_path);
                let license_content = header::generate_with(
                    &resolved_copyright,
                    &resolved_license,
                    header::CommentStyle::Hash,
                    contributor,
                    *exclude_year,
                )?;
                fs::write(&license_path, license_content)
                    .with_context(|| format!("failed to write .license for {rel_path}"))?;
                println!("Updated (as .license): {rel_path}");
                continue;
            }
            if *skip_unrecognised {
                println!("Skipped (unrecognised): {rel_path}");
                continue;
            }
            eprintln!(
                "Warning: {rel_path} is not commentable. Use --force-dot-license to create a .license file"
            );
            continue;
        }

        let content =
            fs::read_to_string(&full_path).with_context(|| format!("failed to read {rel_path}"))?;

        let new_content = header::annotate_file_with(
            &content,
            &resolved_copyright,
            &resolved_license,
            Some(style),
            *force,
            contributor,
            *exclude_year,
        )?;

        if new_content == content {
            println!("Unchanged: {rel_path}");
        } else {
            fs::write(&full_path, new_content)
                .with_context(|| format!("failed to write {rel_path}"))?;
            println!("Updated: {rel_path}");
        }
    }

    if *update_reuse_toml {
        write_reuse_toml_override(
            &root,
            &target_files,
            &final_license,
            &final_copyright,
            precedence,
        )?;
    }

    Ok(())
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    /// A REUSE.toml that licenses everything under `src/`, so a test can check
    /// the no-flags path where licensing comes entirely from config.
    const SRC_ANNOTATION: &str = concat!(
        "version = 1\n",
        "[[annotation]]\n",
        "path = \"src/**\"\n",
        "SPDX-FileCopyrightText = [\"2026 Test Author\"]\n",
        "SPDX-License-Identifier = \"MIT\"\n"
    );

    /// Fluent builder for an `Annotate` command. `run` reads fourteen fields,
    /// so a positional constructor makes every call site an unreadable row of
    /// bare `false`s; here a test names only the flags it actually varies.
    #[expect(
        clippy::struct_excessive_bools,
        reason = "mirrors the flags on the clap variant one-for-one"
    )]
    struct Annotate {
        files: Vec<String>,
        path: String,
        license: Option<String>,
        copyright: Option<String>,
        year: Option<String>,
        contributor: Vec<String>,
        holder: Vec<String>,
        recursive: bool,
        exclude_year: bool,
        force_dot_license: bool,
        force: bool,
        update_reuse_toml: bool,
        skip_unrecognised: bool,
    }

    impl Annotate {
        fn at(root: &Path) -> Self {
            Self {
                files: vec![],
                path: root.to_str().unwrap().to_string(),
                license: None,
                copyright: None,
                year: None,
                contributor: vec![],
                holder: vec![],
                recursive: false,
                exclude_year: false,
                force_dot_license: false,
                force: false,
                update_reuse_toml: false,
                skip_unrecognised: false,
            }
        }

        fn files(mut self, files: &[&str]) -> Self {
            self.files = files.iter().map(ToString::to_string).collect();
            self
        }

        fn license(mut self, license: &str) -> Self {
            self.license = Some(license.to_string());
            self
        }

        fn copyright(mut self, copyright: &str) -> Self {
            self.copyright = Some(copyright.to_string());
            self
        }

        fn year(mut self, year: &str) -> Self {
            self.year = Some(year.to_string());
            self
        }

        fn contributor(mut self, contributor: &str) -> Self {
            self.contributor.push(contributor.to_string());
            self
        }

        fn holder(mut self, holder: &str) -> Self {
            self.holder.push(holder.to_string());
            self
        }

        fn recursive(mut self) -> Self {
            self.recursive = true;
            self
        }

        fn exclude_year(mut self) -> Self {
            self.exclude_year = true;
            self
        }

        fn force_dot_license(mut self) -> Self {
            self.force_dot_license = true;
            self
        }

        fn force(mut self) -> Self {
            self.force = true;
            self
        }

        fn update_reuse_toml(mut self) -> Self {
            self.update_reuse_toml = true;
            self
        }

        fn skip_unrecognised(mut self) -> Self {
            self.skip_unrecognised = true;
            self
        }

        fn build(self) -> crate::Command {
            crate::Command::Annotate {
                files: self.files,
                path: Some(self.path),
                license: self.license,
                copyright: self.copyright,
                year: self.year,
                contributor: self.contributor,
                holder: self.holder,
                recursive: self.recursive,
                exclude_year: self.exclude_year,
                merge_copyrights: false,
                copyright_prefix: None,
                force_dot_license: self.force_dot_license,
                force: self.force,
                update_reuse_toml: self.update_reuse_toml,
                precedence: "override".to_string(),
                skip_unrecognised: self.skip_unrecognised,
            }
        }

        fn apply(self) -> Result<()> {
            run(&self.build())
        }
    }

    /// A project root plus, optionally, a REUSE.toml. `canonicalize` matters:
    /// `resolve_root` canonicalises too, and on macOS `/var` -> `/private/var`
    /// would otherwise defeat the repo-relative path stripping.
    fn project(reuse_toml: Option<&str>) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        if let Some(body) = reuse_toml {
            fs::write(root.join("REUSE.toml"), body).unwrap();
        }
        (tmp, root)
    }

    fn write(root: &Path, rel: &str, body: &str) -> PathBuf {
        let path = root.join(rel);
        fs::write(&path, body).unwrap();
        path
    }

    fn current_year() -> String {
        format!("{}", chrono::Utc::now().year())
    }

    #[test]
    fn a_non_annotate_command_is_a_no_op() {
        let cmd = crate::Command::Fix {
            path: None,
            dry_run: true,
        };

        assert!(run(&cmd).is_ok());
    }

    #[test]
    fn an_annotation_supplies_the_licence_when_no_flag_is_given() {
        let (_tmp, root) = project(Some(SRC_ANNOTATION));
        let file = write(&root, "src/test.py", "print('hello')\n");

        Annotate::at(&root).files(&["src/test.py"]).apply().unwrap();

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("# SPDX-FileCopyrightText: 2026 Test Author"));
        assert!(content.contains("# SPDX-License-Identifier: MIT"));
        assert!(content.contains("print('hello')"), "body must survive");
    }

    #[test]
    fn annotate_records_a_repo_relative_path_in_reuse_toml() {
        let (_tmp, root) = project(Some("version = 1\n"));
        write(&root, "src/test.rs", "fn main() {}\n");
        let absolute_input = root.join("src/test.rs").to_str().unwrap().to_string();

        Annotate::at(&root)
            .files(&[&absolute_input])
            .license("MIT")
            .copyright("Test Author")
            .update_reuse_toml()
            .apply()
            .unwrap();

        let written = fs::read_to_string(root.join("REUSE.toml")).unwrap();
        assert!(
            !written.contains(&absolute_input),
            "REUSE.toml must not record an absolute path, got:\n{written}"
        );
        assert!(
            written.contains("\"src/test.rs\""),
            "REUSE.toml should record the repo-relative path, got:\n{written}"
        );
    }

    #[test]
    fn annotate_recursive_records_a_repo_relative_path_for_a_file_argument() {
        let (_tmp, root) = project(Some("version = 1\n"));
        write(&root, "src/test.rs", "fn main() {}\n");
        let absolute_input = root.join("src/test.rs").to_str().unwrap().to_string();

        Annotate::at(&root)
            .files(&[&absolute_input])
            .recursive()
            .license("MIT")
            .copyright("Test Author")
            .update_reuse_toml()
            .apply()
            .unwrap();

        let written = fs::read_to_string(root.join("REUSE.toml")).unwrap();
        assert!(
            !written.contains(&absolute_input),
            "recursive mode must not record an absolute path, got:\n{written}"
        );
    }

    #[test]
    fn recursive_walks_every_source_file_under_a_directory_argument() {
        let (_tmp, root) = project(None);
        write(&root, "src/a.rs", "fn a() {}\n");
        write(&root, "src/b.rs", "fn b() {}\n");

        Annotate::at(&root)
            .files(&["src"])
            .recursive()
            .license("MIT")
            .copyright("Test Author")
            .apply()
            .unwrap();

        for rel in ["src/a.rs", "src/b.rs"] {
            let content = fs::read_to_string(root.join(rel)).unwrap();
            assert!(
                content.contains("SPDX-License-Identifier: MIT"),
                "{rel} should have been annotated, got:\n{content}"
            );
        }
    }

    #[test]
    fn with_no_file_arguments_every_source_file_under_the_root_is_annotated() {
        let (_tmp, root) = project(None);
        write(&root, "src/a.rs", "fn a() {}\n");
        write(&root, "src/b.rs", "fn b() {}\n");

        Annotate::at(&root)
            .license("MIT")
            .copyright("Test Author")
            .apply()
            .unwrap();

        for rel in ["src/a.rs", "src/b.rs"] {
            let content = fs::read_to_string(root.join(rel)).unwrap();
            assert!(
                content.contains("SPDX-License-Identifier: MIT"),
                "{rel} should have been annotated, got:\n{content}"
            );
        }
    }

    #[test]
    fn a_missing_file_is_skipped_rather_than_failing() {
        let (_tmp, root) = project(None);

        assert!(
            Annotate::at(&root)
                .files(&["src/ghost.rs"])
                .license("MIT")
                .copyright("Test Author")
                .apply()
                .is_ok(),
            "a path that does not exist warns and continues"
        );
    }

    #[test]
    fn holder_builds_the_copyright_and_reaches_reuse_toml() {
        let (_tmp, root) = project(Some("version = 1\n"));
        let file = write(&root, "src/a.rs", "fn a() {}\n");

        Annotate::at(&root)
            .files(&["src/a.rs"])
            .license("MIT")
            .holder("Ada Lovelace")
            .update_reuse_toml()
            .apply()
            .unwrap();

        let expected = format!("{} Ada Lovelace", current_year());
        assert!(
            fs::read_to_string(&file).unwrap().contains(&expected),
            "--holder should become a year-prefixed copyright in the header"
        );
        assert!(
            fs::read_to_string(root.join("REUSE.toml"))
                .unwrap()
                .contains(&expected),
            "the same copyright should reach REUSE.toml"
        );
    }

    #[test]
    fn an_explicit_year_replaces_the_current_one() {
        let (_tmp, root) = project(None);
        let file = write(&root, "src/a.rs", "fn a() {}\n");

        Annotate::at(&root)
            .files(&["src/a.rs"])
            .license("MIT")
            .copyright("Test Author")
            .year("1999")
            .apply()
            .unwrap();

        assert!(
            fs::read_to_string(&file)
                .unwrap()
                .contains("1999 Test Author"),
            "--year should override the current year"
        );
    }

    #[test]
    fn exclude_year_drops_the_year_from_the_notice() {
        let (_tmp, root) = project(None);
        let file = write(&root, "src/a.rs", "fn a() {}\n");

        Annotate::at(&root)
            .files(&["src/a.rs"])
            .license("MIT")
            .copyright("Test Author")
            .year("1999")
            .exclude_year()
            .apply()
            .unwrap();

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("Test Author"), "got:\n{content}");
        assert!(
            !content.contains("1999"),
            "year should be stripped:\n{content}"
        );
    }

    #[test]
    fn a_contributor_is_recorded_alongside_the_copyright() {
        let (_tmp, root) = project(None);
        let file = write(&root, "src/a.rs", "fn a() {}\n");

        Annotate::at(&root)
            .files(&["src/a.rs"])
            .license("MIT")
            .copyright("Test Author")
            .contributor("Grace Hopper")
            .apply()
            .unwrap();

        assert!(
            fs::read_to_string(&file)
                .unwrap()
                .contains("SPDX-FileContributor: Grace Hopper")
        );
    }

    #[test]
    fn force_overwrites_an_existing_header() {
        let (_tmp, root) = project(None);
        let file = write(
            &root,
            "src/a.rs",
            concat!(
                "// SPDX-FileCopyrightText: 2001 Old Owner\n",
                "// SPDX-License-Identifier: GPL-3.0-only\n",
                "\n",
                "fn a() {}\n"
            ),
        );

        Annotate::at(&root)
            .files(&["src/a.rs"])
            .license("MIT")
            .copyright("New Owner")
            .force()
            .apply()
            .unwrap();

        let content = fs::read_to_string(&file).unwrap();
        assert!(
            content.contains("SPDX-License-Identifier: MIT"),
            "--force should replace the old licence, got:\n{content}"
        );
    }

    #[test]
    fn an_annotation_without_a_licence_leaves_the_file_untouched() {
        let (_tmp, root) = project(Some(concat!(
            "version = 1\n",
            "[[annotations]]\n",
            "path = \"src/**\"\n",
            "SPDX-FileCopyrightText = \"2026 Test Author\"\n"
        )));
        let file = write(&root, "src/a.rs", "fn a() {}\n");

        Annotate::at(&root).files(&["src/a.rs"]).apply().unwrap();

        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "fn a() {}\n",
            "a copyright with no licence is not enough to write a header"
        );
    }

    #[test]
    fn an_annotation_without_a_copyright_still_writes_the_licence() {
        let (_tmp, root) = project(Some(concat!(
            "version = 1\n",
            "[[annotations]]\n",
            "path = \"src/**\"\n",
            "SPDX-License-Identifier = \"MIT\"\n"
        )));
        let file = write(&root, "src/a.rs", "fn a() {}\n");

        Annotate::at(&root).files(&["src/a.rs"]).apply().unwrap();

        assert!(
            fs::read_to_string(&file)
                .unwrap()
                .contains("SPDX-License-Identifier: MIT")
        );
    }

    #[test]
    fn a_file_with_no_licensing_information_anywhere_is_skipped() {
        let (_tmp, root) = project(None);
        let file = write(&root, "src/a.rs", "fn a() {}\n");

        Annotate::at(&root).files(&["src/a.rs"]).apply().unwrap();

        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "fn a() {}\n",
            "with neither flags nor REUSE.toml there is nothing to write"
        );
    }

    #[test]
    fn an_uncommentable_file_gets_a_companion_with_force_dot_license() {
        let (_tmp, root) = project(None);
        write(&root, "logo.png", "pretend this is binary\n");

        Annotate::at(&root)
            .files(&["logo.png"])
            .license("MIT")
            .copyright("Test Author")
            .force_dot_license()
            .apply()
            .unwrap();

        let companion = fs::read_to_string(root.join("logo.png.license")).unwrap();
        assert!(
            companion.contains("# SPDX-License-Identifier: MIT"),
            "the companion uses hash comments, got:\n{companion}"
        );
    }

    #[test]
    fn an_uncommentable_file_is_passed_over_with_skip_unrecognised() {
        let (_tmp, root) = project(None);
        write(&root, "logo.png", "pretend this is binary\n");

        Annotate::at(&root)
            .files(&["logo.png"])
            .license("MIT")
            .copyright("Test Author")
            .skip_unrecognised()
            .apply()
            .unwrap();

        assert!(!root.join("logo.png.license").exists());
    }

    #[test]
    fn an_uncommentable_file_warns_when_neither_flag_is_given() {
        let (_tmp, root) = project(None);
        let file = write(&root, "logo.png", "pretend this is binary\n");

        Annotate::at(&root)
            .files(&["logo.png"])
            .license("MIT")
            .copyright("Test Author")
            .apply()
            .unwrap();

        assert!(!root.join("logo.png.license").exists());
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "pretend this is binary\n",
            "an uncommentable file must never be rewritten in place"
        );
    }

    #[test]
    fn update_reuse_toml_is_a_no_op_when_the_project_has_none() {
        let (_tmp, root) = project(None);
        write(&root, "src/a.rs", "fn a() {}\n");

        Annotate::at(&root)
            .files(&["src/a.rs"])
            .license("MIT")
            .copyright("Test Author")
            .update_reuse_toml()
            .apply()
            .unwrap();

        assert!(
            !root.join("REUSE.toml").exists(),
            "--update-reuse-toml must not conjure a REUSE.toml that was not there"
        );
    }

    #[test]
    fn an_invalid_licence_expression_fails_when_annotating_in_place() {
        let (_tmp, root) = project(None);
        write(&root, "src/a.rs", "fn a() {}\n");

        assert!(
            Annotate::at(&root)
                .files(&["src/a.rs"])
                .license("NOT A VALID EXPRESSION")
                .copyright("Test Author")
                .apply()
                .is_err(),
            "an unparseable SPDX expression must not be written to a file"
        );
    }

    #[test]
    fn an_invalid_licence_expression_fails_for_a_dot_license_companion() {
        let (_tmp, root) = project(None);
        write(&root, "logo.png", "pretend this is binary\n");

        assert!(
            Annotate::at(&root)
                .files(&["logo.png"])
                .license("NOT A VALID EXPRESSION")
                .copyright("Test Author")
                .force_dot_license()
                .apply()
                .is_err(),
            "the companion path validates the expression too"
        );
    }

    #[test]
    fn a_malformed_reuse_toml_fails_the_update() {
        let (_tmp, root) = project(Some("this is = = not toml\n"));
        write(&root, "src/a.rs", "fn a() {}\n");

        assert!(
            Annotate::at(&root)
                .files(&["src/a.rs"])
                .license("MIT")
                .copyright("Test Author")
                .update_reuse_toml()
                .apply()
                .is_err(),
            "a REUSE.toml that cannot be parsed must surface as an error"
        );
    }

    #[test]
    fn running_annotate_twice_does_not_duplicate_the_annotation_block() {
        let (_tmp, root) = project(Some("version = 1\n"));
        write(&root, "src/test.rs", "fn main() {}\n");

        let annotate_once = || {
            Annotate::at(&root)
                .files(&["src/test.rs"])
                .license("MIT")
                .copyright("Test Author")
                .update_reuse_toml()
                .apply()
                .unwrap();
        };

        annotate_once();
        annotate_once();

        let written = fs::read_to_string(root.join("REUSE.toml")).unwrap();
        assert_eq!(
            written.matches("[[annotations]]").count(),
            1,
            "the second run must merge, not append, got:\n{written}"
        );
    }
}
// REUSE-IgnoreEnd
