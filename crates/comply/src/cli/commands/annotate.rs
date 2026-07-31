// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;

use anyhow::{Context, Result};
use chrono::Datelike;
use comply::file::{self, FileType};
use comply::header;

use crate::cli::shared::{
    load_config, permitted_targets, resolve_root, to_repo_relative, write_reuse_toml_override,
};

/// The copyright the flags alone specify, or `None` when they specify none.
///
/// One resolver for both the header and the REUSE.toml annotation. They were
/// built separately and had drifted -- the annotation left the year off
/// `--copyright` and ignored `--year` for `--holder` -- and because the
/// annotation is written with `precedence = "override"`, the manifest's version
/// is the one that counts.
fn copyright_from_flags(copyright: Option<&str>, holder: &[String], year: &str) -> Option<String> {
    if holder.is_empty() {
        return copyright.map(|c| format!("{year} {c}"));
    }

    Some(
        holder
            .iter()
            .map(|h| format!("{year} {h}"))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// Whether a walked file is a candidate for annotation.
///
/// A binary belongs here as much as a source file does: its licensing goes in a
/// `.license` companion rather than a header, but it still needs one. Keeping
/// only [`FileType::Source`] dropped every binary before the companion branch
/// could see it, so a recursive run neither wrote a companion nor said why.
const fn is_annotatable(file_type: FileType) -> bool {
    matches!(file_type, FileType::Source | FileType::Binary)
}

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
        copyright_prefix,
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

    let prefix: header::CopyrightPrefix = copyright_prefix.parse()?;

    let current_year = format!("{}", chrono::Utc::now().year());
    let year = year.as_deref().unwrap_or(&current_year);

    // Pre-compute final license/copyright for REUSE.toml update
    let final_license = license.as_deref().unwrap_or("").to_string();
    let final_copyright =
        copyright_from_flags(copyright.as_deref(), holder, year).unwrap_or_default();

    // Determine target files
    let target_files: Vec<String> = if !files.is_empty() && !*recursive {
        permitted_targets(&root, files, &config)
    } else {
        let mut targets = Vec::new();
        if *recursive && !files.is_empty() {
            // Treat each file path as a directory root for recursive walk
            for f in files {
                let dir_path = root.join(f);
                if dir_path.is_dir() {
                    for entry in file::walk_with(
                        &dir_path,
                        &config.ignore,
                        &config.source_patterns,
                        config.walk,
                    ) {
                        let entry = entry?;
                        if is_annotatable(entry.file_type) {
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
            for entry in
                file::walk_with(&root, &config.ignore, &config.source_patterns, config.walk)
            {
                let entry = entry?;
                if is_annotatable(entry.file_type) {
                    let rel_path = entry.relative_to(&root);
                    targets.push(rel_path);
                }
            }
        }
        targets
    };

    // Every target goes through the scope filter, however it was derived. Two
    // routes above bypassed it and wrote SPDX headers into `LICENSES/`,
    // corrupting the canonical licence texts the whole project cites:
    //
    //   annotate --recursive LICENSES              walks with a pruned
    //     directory AS the root, and the prune matches directory names during
    //     descent, so nothing above the root is ever examined.
    //   annotate --recursive LICENSES/MIT.txt      the non-directory branch
    //     pushed the path straight through, unfiltered.
    let target_files = permitted_targets(&root, &target_files, &config);

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

        let resolved_copyright = copyright_from_flags(copyright.as_deref(), holder, year)
            .unwrap_or_else(|| from_config.copyrights.join(", "));

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

        let style = header::ResolvedStyle::detect(rel_path, &config.comment_styles);

        if style.style() == header::CommentStyle::Uncommentable {
            // One style name covers two situations the reference tool keeps
            // apart: a format that HAS no comment syntax, and a name no table
            // claims. `FileType::Binary` is the first of those, and it gets its
            // companion unasked -- `--skip-unrecognised` does not reach it,
            // because it was never unrecognised. The second still has to be
            // declared in `[[tool.comply.comment-styles]]` or forced.
            let uncommentable_format =
                file::classify_path(&full_path, &config.source_patterns) == FileType::Binary;

            if *force_dot_license || uncommentable_format {
                // A companion already on disk is somebody's licensing
                // declaration, and rewriting it drops their copyright notice
                // where nothing downstream will notice -- `lint` passes on the
                // result either way. The inline path below refuses that without
                // `--force`, and `fix` leaves an existing companion alone
                // entirely; this is the same rule rather than a third one.
                if !*force && file::has_license_companion(&full_path) {
                    println!("Unchanged (as .license): {rel_path}");
                    continue;
                }

                let license_path = file::license_companion_path(&full_path);
                let license_content = header::generate_with(
                    header::CopyrightNotice::new(&resolved_copyright).with_prefix(prefix),
                    &resolved_license,
                    header::ResolvedStyle::builtin(header::CommentStyle::Uncommentable),
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
                "Warning: {rel_path} has no known comment syntax. \
                 Declare one in [[tool.comply.comment-styles]], \
                 or use --force-dot-license / --skip-unrecognised"
            );
            continue;
        }

        let content =
            fs::read_to_string(&full_path).with_context(|| format!("failed to read {rel_path}"))?;

        let new_content = header::annotate_file_with(
            &content,
            header::CopyrightNotice::new(&resolved_copyright).with_prefix(prefix),
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
        copyright_prefix: String,
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
                copyright_prefix: "spdx".to_string(),
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

        fn copyright_prefix(mut self, prefix: &str) -> Self {
            self.copyright_prefix = prefix.to_string();
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
                copyright_prefix: self.copyright_prefix,
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
    fn a_failed_reuse_toml_update_surfaces_instead_of_passing_silently() {
        // An annotation block matching on licence, copyright and precedence but
        // carrying no `path` key -- `path` is `#[serde(default)]`, so this is
        // valid config. The merge then finds the block but has no `path` value
        // to rewrite, and that error must reach the caller.
        let (_tmp, root) = project(Some(concat!(
            "version = 1\n",
            "\n",
            "[[annotations]]\n",
            "SPDX-FileCopyrightText = \"1999 Test Author\"\n",
            "SPDX-License-Identifier = \"MIT\"\n",
            "precedence = \"override\"\n",
        )));
        write(&root, "src/b.rs", "fn b() {}\n");

        let err = Annotate::at(&root)
            .files(&["src/b.rs"])
            .license("MIT")
            .copyright("Test Author")
            .year("1999")
            .update_reuse_toml()
            .apply()
            .unwrap_err();

        assert!(
            format!("{err:#}").contains("path"),
            "the error should say the `path` value could not be located, got: {err:#}"
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
    fn recursive_never_writes_into_the_licenses_directory() {
        // `LICENSES/` holds the canonical licence texts the whole project
        // cites. Two routes wrote SPDX headers into them: naming the directory
        // made it the walk ROOT, and the prune only matches directory names
        // encountered during descent; naming a file inside it skipped the
        // scope filter entirely. Both corrupted the licence texts on disk.
        for arg in ["LICENSES", "LICENSES/Apache-2.0.txt"] {
            let (_tmp, root) = project(Some("version = 1\n"));
            fs::create_dir_all(root.join("LICENSES")).unwrap();
            let licence = root.join("LICENSES/Apache-2.0.txt");
            fs::write(&licence, "                    Apache License\n").unwrap();

            Annotate::at(&root)
                .files(&[arg])
                .recursive()
                .apply()
                .unwrap();

            assert_eq!(
                fs::read_to_string(&licence).unwrap(),
                "                    Apache License\n",
                "`--recursive {arg}` must not touch a licence text"
            );
        }
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

    /// `--copyright-prefix` was declared on the clap variant but dropped by the
    /// `..` in this module's destructure, so every value was accepted and none
    /// had any effect.
    #[test]
    fn the_copyright_prefix_reaches_the_header() {
        let (_tmp, root) = project(None);
        let file = write(&root, "src/a.rs", "fn a() {}\n");

        Annotate::at(&root)
            .files(&["src/a.rs"])
            .license("MIT")
            .copyright("Acme")
            .year("2026")
            .copyright_prefix("string-c")
            .apply()
            .unwrap();

        assert!(
            fs::read_to_string(&file)
                .unwrap()
                .contains("// Copyright (C) 2026 Acme"),
            "--copyright-prefix must choose the form the notice is written in"
        );
    }

    /// The prefix is how a notice is rendered in a comment. A REUSE.toml
    /// annotation names the field in its key, so the value stays bare.
    #[test]
    fn the_copyright_prefix_stops_at_the_header() {
        let (_tmp, root) = project(Some("version = 1\n"));
        write(&root, "src/a.rs", "fn a() {}\n");

        Annotate::at(&root)
            .files(&["src/a.rs"])
            .license("MIT")
            .copyright("Acme")
            .year("2026")
            .copyright_prefix("symbol")
            .update_reuse_toml()
            .apply()
            .unwrap();

        let manifest = fs::read_to_string(root.join("REUSE.toml")).unwrap();
        assert!(
            manifest.contains("2026 Acme") && !manifest.contains('\u{a9}'),
            "the manifest keeps the bare notice:\n{manifest}"
        );
    }

    /// The annotation is written with `precedence = "override"`, so a manifest
    /// that disagrees with the header does not merely look untidy -- it wins,
    /// and the header's copyright is discarded. Both were built from the flags
    /// by separate code that had drifted: the manifest's copy left the year off
    /// `--copyright` entirely, and ignored `--year` for `--holder`.
    #[test]
    fn the_header_and_the_manifest_agree_on_the_copyright() {
        for annotate in [
            Annotate::at(Path::new(".")).copyright("Acme"),
            Annotate::at(Path::new(".")).holder("Acme"),
        ] {
            let (_tmp, root) = project(Some("version = 1\n"));
            let file = write(&root, "src/a.rs", "fn a() {}\n");

            Annotate {
                path: root.to_str().unwrap().to_string(),
                ..annotate
            }
            .files(&["src/a.rs"])
            .license("MIT")
            .year("1999")
            .update_reuse_toml()
            .apply()
            .unwrap();

            assert!(
                fs::read_to_string(&file).unwrap().contains("1999 Acme"),
                "the header should carry the year that was asked for"
            );
            assert!(
                fs::read_to_string(root.join("REUSE.toml"))
                    .unwrap()
                    .contains("\"1999 Acme\""),
                "an overriding annotation that drops the year revokes it"
            );
        }
    }

    #[test]
    fn an_unknown_copyright_prefix_is_an_error_not_a_silent_default() {
        let (_tmp, root) = project(None);
        let file = write(&root, "src/a.rs", "fn a() {}\n");

        let err = Annotate::at(&root)
            .files(&["src/a.rs"])
            .license("MIT")
            .copyright("Acme")
            .copyright_prefix("spdx-c-symbol")
            .apply()
            .unwrap_err();

        assert!(
            err.to_string().contains("spdx-c-symbol"),
            "the message must name the offending value, got: {err}"
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "fn a() {}\n",
            "a rejected run must not have written a header first"
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

    /// Byte-for-byte capture of `reuse` 6.2.0 writing a `.license` companion:
    /// the lines carry no comment prefix at all.
    const COMPANION: &str = concat!(
        "SPDX-FileCopyrightText: 2026 X\n",
        "\n",
        "SPDX-License-Identifier: MIT\n"
    );

    /// A format with no comment syntax and a name no table claims both resolve
    /// to `CommentStyle::Uncommentable`, but reuse 6.2.0 treats them
    /// differently: the first gets its companion unasked, the second is
    /// refused. comply warned for both.
    #[test]
    fn a_known_uncommentable_format_gets_a_companion_with_no_flag() {
        let (_tmp, root) = project(None);
        let file = write(&root, "logo.png", "pretend this is binary\n");

        Annotate::at(&root)
            .files(&["logo.png"])
            .license("MIT")
            .copyright("X")
            .year("2026")
            .apply()
            .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("logo.png.license")).unwrap(),
            COMPANION
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "pretend this is binary\n",
            "an uncommentable file must never be rewritten in place"
        );
    }

    /// `.svg` is `FileType::Binary` in `classify.rs` but used to answer `Html`
    /// in the style table, so it missed the companion branch and had an XML
    /// comment written at byte 0 -- above the declaration, which is a fatal
    /// parse error rather than a comment. reuse 6.2.0 writes a companion and
    /// leaves the file alone.
    #[test]
    fn an_svg_keeps_its_xml_declaration_on_the_first_line() {
        const SVG: &str = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n"
        );

        let (_tmp, root) = project(None);
        let file = write(&root, "logo.svg", SVG);

        Annotate::at(&root)
            .license("MIT")
            .copyright("X")
            .year("2026")
            .apply()
            .unwrap();

        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            SVG,
            "an SVG must not be rewritten in place"
        );
        assert_eq!(
            fs::read_to_string(root.join("logo.svg.license")).unwrap(),
            COMPANION
        );
    }

    /// `.png` classifies as `FileType::Binary`, and both target-collection
    /// branches kept only `FileType::Source`. A walked binary was therefore
    /// dropped before the companion branch could see it -- no companion, and no
    /// warning either.
    #[test]
    fn a_walked_binary_reaches_the_companion_branch() {
        for recursive in [false, true] {
            let (_tmp, root) = project(None);
            write(&root, "src/logo.png", "pretend this is binary\n");

            let annotate = Annotate::at(&root).license("MIT").copyright("X");
            let annotate = if recursive {
                annotate.files(&["src"]).recursive()
            } else {
                annotate
            };
            annotate.apply().unwrap();

            assert!(
                root.join("src/logo.png.license").exists(),
                "a walked binary must not vanish (recursive: {recursive})"
            );
        }
    }

    /// The companion write used to sit behind `--force-dot-license`, where
    /// clobbering is what the flag promises. Putting it on the default path for
    /// every binary made a plain `annotate --recursive .` erase a copyright
    /// holder's own declaration, and `lint` passes afterwards, so nothing
    /// downstream catches it. The inline path already refuses this.
    const PRIOR_COMPANION: &str = concat!(
        "SPDX-FileCopyrightText: 2019 Original Author\n",
        "\n",
        "SPDX-License-Identifier: GPL-3.0-only\n"
    );

    #[test]
    fn an_existing_companion_survives_a_run_without_force() {
        let (_tmp, root) = project(None);
        write(&root, "logo.png", "pretend this is binary\n");
        write(&root, "logo.png.license", PRIOR_COMPANION);

        Annotate::at(&root)
            .license("MIT")
            .copyright("X")
            .year("2026")
            .apply()
            .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("logo.png.license")).unwrap(),
            PRIOR_COMPANION,
            "a companion already on disk is somebody's licensing declaration"
        );
    }

    #[test]
    fn force_replaces_an_existing_companion() {
        let (_tmp, root) = project(None);
        write(&root, "logo.png", "pretend this is binary\n");
        write(&root, "logo.png.license", PRIOR_COMPANION);

        Annotate::at(&root)
            .files(&["logo.png"])
            .license("MIT")
            .copyright("X")
            .year("2026")
            .force()
            .apply()
            .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("logo.png.license")).unwrap(),
            COMPANION,
            "--force is the escape hatch the inline path already offers"
        );
    }

    #[test]
    fn an_unrecognised_name_warns_rather_than_gaining_a_companion() {
        let (_tmp, root) = project(None);
        let file = write(&root, "notes.txt", "just text\n");

        Annotate::at(&root)
            .files(&["notes.txt"])
            .license("MIT")
            .copyright("Test Author")
            .apply()
            .unwrap();

        assert!(!root.join("notes.txt.license").exists());
        assert_eq!(fs::read_to_string(&file).unwrap(), "just text\n");
    }

    #[test]
    fn force_dot_license_writes_a_companion_for_an_unrecognised_name() {
        let (_tmp, root) = project(None);
        write(&root, "notes.txt", "just text\n");

        Annotate::at(&root)
            .files(&["notes.txt"])
            .license("MIT")
            .copyright("X")
            .year("2026")
            .force_dot_license()
            .apply()
            .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("notes.txt.license")).unwrap(),
            COMPANION
        );
    }

    #[test]
    fn skip_unrecognised_passes_over_an_unrecognised_name() {
        let (_tmp, root) = project(None);
        write(&root, "notes.txt", "just text\n");

        Annotate::at(&root)
            .files(&["notes.txt"])
            .license("MIT")
            .copyright("Test Author")
            .skip_unrecognised()
            .apply()
            .unwrap();

        assert!(!root.join("notes.txt.license").exists());
    }

    /// `--skip-unrecognised` is about names no table claims. reuse 6.2.0 still
    /// writes the companion for a known-uncommentable format when it is given,
    /// because that format was never unrecognised.
    #[test]
    fn skip_unrecognised_does_not_skip_a_known_uncommentable_format() {
        let (_tmp, root) = project(None);
        write(&root, "logo.png", "pretend this is binary\n");

        Annotate::at(&root)
            .files(&["logo.png"])
            .license("MIT")
            .copyright("Test Author")
            .skip_unrecognised()
            .apply()
            .unwrap();

        assert!(root.join("logo.png.license").exists());
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
