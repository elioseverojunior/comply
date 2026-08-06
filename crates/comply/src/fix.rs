// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Writing missing licensing information back to a project.

use std::fs;
use std::path::Path;

use crate::config::Config;
use crate::error::Error;
use crate::file::{self, FileType};

/// Result of fixing a file
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixAction {
    /// File was already compliant
    Unchanged,
    /// Added SPDX header to file
    HeaderAdded,
    /// Reformatted existing SPDX header
    HeaderReformatted,
    /// Created missing license companion file
    LicenseFileCreated,
    /// File is binary but missing license file
    BinaryLicenseMissing,
    /// File could not be fixed (error details)
    Failed(String),
}

/// Result of fixing a project
#[derive(Debug, Clone, Default)]
pub struct FixReport {
    /// What happened to each file, in the order the walk reached it.
    pub actions: Vec<(String, FixAction)>,
    /// Files considered, whether or not they changed.
    pub total: usize,
    /// Files that gained licensing information.
    pub fixed: usize,
    /// Files already compliant, left untouched.
    pub unchanged: usize,
    /// Files that could not be fixed; each carries its reason in `actions`.
    pub failed: usize,
}

impl FixReport {
    /// An empty report, before any file has been considered.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            actions: Vec::new(),
            total: 0,
            fixed: 0,
            unchanged: 0,
            failed: 0,
        }
    }

    /// Record what happened to one file and fold it into the counts.
    ///
    /// Every action other than `Unchanged` and `Failed` counts as fixed, so a
    /// new variant is treated as a change unless it is added to the match.
    pub fn add(&mut self, path: String, action: FixAction) {
        self.total += 1;
        match &action {
            FixAction::Unchanged => self.unchanged += 1,
            FixAction::Failed(_) => self.failed += 1,
            _ => self.fixed += 1,
        }
        self.actions.push((path, action));
    }

    /// Whether the run neither changed nor failed anything.
    ///
    /// False after a successful fix, not just after a failure -- a caller
    /// wanting "did this project already comply" gets that only from a run
    /// that had nothing to do.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.failed == 0 && self.fixed == 0
    }
}

/// Fix a project's REUSE compliance issues
#[allow(clippy::missing_errors_doc)]
pub fn fix_project(root: &Path, config: &Config, dry_run: bool) -> Result<FixReport, Error> {
    let mut report = FixReport::new();

    for entry in file::walk_with(root, &config.ignore, &config.source_patterns, config.walk) {
        let entry = entry?;
        let rel_path = entry.relative_to(root);

        let action = match entry.file_type {
            FileType::Source => fix_source_file(&entry.path, &rel_path, config, dry_run)?,
            FileType::Binary => fix_binary_file(&entry.path, &rel_path, config, dry_run)?,
            FileType::Ignored => FixAction::Unchanged,
        };

        report.add(rel_path, action);
    }

    Ok(report)
}

/// Copyright and licence from the `[default]` section, when it supplies both.
///
/// Annotations are not consulted here: a file one covers is already compliant,
/// so both callers return before reaching this. `resolve_licensing`
/// deliberately ignores the defaults, which makes them the only thing that can
/// turn a genuinely unlicensed file into a compliant one.
///
/// `None` when either half is missing -- a header needs a copyright *and* a
/// licence, and half of one is not worth writing.
fn resolve_defaults(config: &Config) -> Option<(String, String)> {
    if !config.default_copyright.is_empty()
        && let Some(license) = &config.default_license
    {
        return Some((config.default_copyright.join("\n"), license.clone()));
    }

    None
}

/// Fix a source code file. `rel_path` is relative to the project root.
fn fix_source_file(
    path: &Path,
    rel_path: &str,
    config: &Config,
    dry_run: bool,
) -> Result<FixAction, Error> {
    // Per REUSE spec section 7.2, a .license companion takes precedence over
    // the file's own content.
    let (content, is_license_content) = if file::has_license_companion(path) {
        let license_path = file::license_companion_path(path);
        (fs::read_to_string(license_path)?, true)
    } else {
        (fs::read_to_string(path)?, false)
    };

    // Check if file already has a valid header
    if let Some(header) = crate::header::detect(&content) {
        let has_copyright = !header.copyrights.is_empty();
        let has_license = header.license_expression.is_some();

        if has_copyright && has_license {
            // Header is complete, just reformat if needed
            let path_str = path.to_str().unwrap_or("");
            let style = crate::header::ResolvedStyle::detect(path_str, &config.comment_styles);
            // Not `?`: a header comply cannot parse is a fault in one file,
            // not a reason to abandon the run. Propagating it aborted
            // `fix_project` entirely -- one malformed expression anywhere in a
            // tree meant nothing was fixed anywhere.
            let formatted = match crate::header::format_file(&content, Some(style)) {
                Ok(formatted) => formatted,
                Err(e) => return Ok(FixAction::Failed(e.to_string())),
            };

            if formatted == content {
                return Ok(FixAction::Unchanged);
            }

            if !dry_run {
                if is_license_content {
                    fs::write(file::license_companion_path(path), formatted)?;
                } else {
                    fs::write(path, formatted)?;
                }
            }
            return Ok(FixAction::HeaderReformatted);
        }
    }

    // An annotation covering this file already declares its licensing, so it is
    // compliant and `lint` passes it. Writing a header in as well is a blanket
    // rewrite of files nobody named -- exactly what REUSE.toml's `**` aggregate
    // exists to avoid -- and it made `fix` claim work on a tree `lint` reported
    // clean. `annotate` is the opt-in writer when a per-file header is wanted.
    let annotated = config.resolve_licensing(rel_path, &[], None);
    if annotated.license.is_some() && !annotated.copyrights.is_empty() {
        return Ok(FixAction::Unchanged);
    }

    // Nothing covers it. The defaults are the last thing that can supply a
    // header; without them there is no licensing to write and fix must say so.
    let Some((copyright, license)) = resolve_defaults(config) else {
        return Ok(FixAction::Failed(
            "No SPDX header and no matching config annotation or defaults".to_string(),
        ));
    };

    let path_str = path.to_str().unwrap_or("");
    let style = crate::header::ResolvedStyle::detect(path_str, &config.comment_styles);
    let new_content = match crate::header::annotate_file(
        &content,
        copyright.as_str(),
        &license,
        Some(style),
        true,
    ) {
        Ok(new_content) => new_content,
        Err(e) => return Ok(FixAction::Failed(e.to_string())),
    };

    if !dry_run {
        if is_license_content {
            fs::write(file::license_companion_path(path), new_content)?;
        } else {
            fs::write(path, new_content)?;
        }
    }
    Ok(FixAction::HeaderAdded)
}

/// Fix a binary file (ensure companion license file exists).
/// `rel_path` is relative to the project root.
fn fix_binary_file(
    path: &Path,
    rel_path: &str,
    config: &Config,
    dry_run: bool,
) -> Result<FixAction, Error> {
    if file::has_license_companion(path) {
        return Ok(FixAction::Unchanged);
    }

    // As for source files: an annotation covering this one already declares its
    // licensing, so `lint` passes it and there is no companion to write. Only
    // the defaults, which `resolve_licensing` deliberately does not consult,
    // can turn a genuinely unlicensed binary into a compliant one.
    let annotated = config.resolve_licensing(rel_path, &[], None);
    if annotated.license.is_some() && !annotated.copyrights.is_empty() {
        return Ok(FixAction::Unchanged);
    }

    let Some((copyright, license)) = resolve_defaults(config) else {
        return Ok(FixAction::BinaryLicenseMissing);
    };

    // Same generator `annotate`'s Uncommentable branch calls, so the two
    // commands cannot drift back onto separate hand-rolled bytes.
    let license_text = match crate::header::generate(
        &copyright,
        &license,
        crate::header::CommentStyle::Uncommentable,
    ) {
        Ok(license_text) => license_text,
        Err(e) => return Ok(FixAction::Failed(e.to_string())),
    };

    if !dry_run {
        fs::write(file::license_companion_path(path), license_text)?;
    }
    Ok(FixAction::LicenseFileCreated)
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Annotation, Config, Precedence};
    use std::fs;
    use tempfile::TempDir;

    const MALFORMED_HEADER: &str = concat!(
        "//SPDX-FileCopyrightText:2025 Acme\n",
        "//SPDX-License-Identifier:MIT\n"
    );

    /// A source file plus its `.license` companion, which per REUSE 7.2 is what
    /// actually declares the licensing.
    fn source_with_companion(companion: &str) -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(&path, "fn main() {}\n").unwrap();
        fs::write(file::license_companion_path(&path), companion).unwrap();
        (tmp, path)
    }

    fn companion_of(path: &Path) -> String {
        fs::read_to_string(file::license_companion_path(path)).unwrap()
    }

    /// Every fixture below puts its file directly under the temp root, so the
    /// project-relative path the fixers expect is just the file name.
    fn rel(path: &Path) -> String {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into()
    }

    fn config_annotating(glob: &str) -> Config {
        Config {
            annotations: vec![Annotation {
                path: glob.to_string(),
                copyright: vec!["2026 Acme".to_string()],
                license: "MIT".to_string(),
                precedence: Precedence::Closest,
            }],
            ..Config::builder().build()
        }
    }

    #[test]
    fn an_annotation_glob_matches_a_source_file_by_repo_relative_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();

        let report = fix_project(root, &config_annotating("src/**"), false).unwrap();

        let actions = format!("{:?}", report.actions);
        assert_eq!(
            report.failed, 0,
            "`src/**` must match `src/a.rs`, not the absolute path; got {actions}"
        );
        assert_eq!(
            fs::read_to_string(root.join("src/a.rs")).unwrap(),
            "fn a() {}\n",
            "the annotation licenses the file, so fix has nothing to write"
        );

        // The negative half: a glob that matches nothing leaves the file
        // uncovered, and then fix has no licensing to fall back on. Without
        // this the assertion above would also pass if globs never matched.
        let missed = fix_project(root, &config_annotating("other/**"), false).unwrap();
        assert_eq!(
            missed.failed, 1,
            "an unmatched glob must leave the file unlicensed; got {:?}",
            missed.actions
        );
    }

    #[test]
    fn a_licence_text_file_is_walked_but_left_untouched() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // `LICENSE` classifies as Ignored: it is a licence text, not a covered
        // file, so `fix` must record it as Unchanged rather than write a header
        // into the licence itself.
        fs::write(root.join("LICENSE"), "MIT License\n").unwrap();

        let report = fix_project(root, &config_annotating("**"), false).unwrap();

        assert_eq!(
            report.actions,
            vec![("LICENSE".to_string(), FixAction::Unchanged)]
        );
        assert_eq!(
            fs::read_to_string(root.join("LICENSE")).unwrap(),
            "MIT License\n"
        );
    }

    #[test]
    fn an_annotation_glob_matches_a_binary_by_repo_relative_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(root.join("assets/logo.png"), b"\x89PNG").unwrap();

        let report = fix_project(root, &config_annotating("assets/**"), false).unwrap();

        let actions = format!("{:?}", report.actions);
        assert_eq!(
            report.failed, 0,
            "`assets/**` must match `assets/logo.png`, not the bare file name; got {actions}"
        );
        assert!(
            !root.join("assets/logo.png.license").exists(),
            "the annotation licenses the binary, so no companion is needed"
        );

        // The negative half: an unmatched glob leaves the binary uncovered, and
        // then a companion is the only thing that could license it.
        let missed = fix_project(root, &config_annotating("other/**"), false).unwrap();
        assert!(
            format!("{:?}", missed.actions).contains("BinaryLicenseMissing"),
            "an unmatched glob must leave the binary unlicensed; got {:?}",
            missed.actions
        );
    }

    #[test]
    fn a_companion_is_reformatted_in_place_of_the_source_file() {
        let (_tmp, path) = source_with_companion(MALFORMED_HEADER);

        let action =
            fix_source_file(&path, &rel(&path), &Config::builder().build(), false).unwrap();

        assert_eq!(action, FixAction::HeaderReformatted);
        let companion = companion_of(&path);
        assert!(
            companion.starts_with("// SPDX-FileCopyrightText: "),
            "got:\n{companion}"
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "fn main() {}\n",
            "the source file itself must not be rewritten"
        );
    }

    #[test]
    fn a_dry_run_reports_the_reformat_without_writing_it() {
        let (_tmp, path) = source_with_companion(MALFORMED_HEADER);

        let action = fix_source_file(&path, &rel(&path), &Config::builder().build(), true).unwrap();

        assert_eq!(action, FixAction::HeaderReformatted);
        assert_eq!(
            companion_of(&path),
            MALFORMED_HEADER,
            "--dry-run must leave the companion byte-identical"
        );
    }

    #[test]
    fn a_companion_with_no_header_gets_one_from_the_defaults() {
        let (_tmp, path) = source_with_companion("nothing declared here\n");
        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Test Corp")
            .build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();

        assert_eq!(action, FixAction::HeaderAdded);
        let companion = companion_of(&path);
        assert!(
            companion.contains("SPDX-License-Identifier: MIT"),
            "got:\n{companion}"
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "fn main() {}\n",
            "the header belongs in the companion, not the source"
        );
    }

    #[test]
    fn an_annotation_missing_its_licence_is_not_enough_to_fix_a_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(&path, "fn main() {}\n").unwrap();
        let config = Config {
            annotations: vec![Annotation {
                path: "**".to_string(),
                copyright: vec!["2026 Acme".to_string()],
                license: String::new(),
                precedence: Precedence::Closest,
            }],
            ..Config::builder().build()
        };

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();

        assert!(
            matches!(action, FixAction::Failed(_)),
            "a copyright with no licence cannot produce a header, got: {action:?}"
        );
    }

    #[test]
    fn a_default_copyright_without_a_default_licence_is_not_enough() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(&path, "fn main() {}\n").unwrap();
        let config = Config::builder().add_default_copyright("2026 Acme").build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();

        assert!(matches!(action, FixAction::Failed(_)), "got: {action:?}");
    }

    #[test]
    fn a_header_missing_its_licence_is_completed_from_the_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(
            &path,
            "// SPDX-FileCopyrightText: 2025 Acme\n\nfn main() {}\n",
        )
        .unwrap();
        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Test Corp")
            .build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();

        assert_eq!(
            action,
            FixAction::HeaderAdded,
            "a copyright without a licence is an incomplete header, not a complete one"
        );
        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("SPDX-License-Identifier: MIT"),
            "got:\n{content}"
        );
    }

    #[test]
    fn a_binary_with_no_extension_still_gets_a_companion() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("payload");
        fs::write(&path, b"\x00\x01\x02").unwrap();
        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Acme")
            .build();

        let action = fix_binary_file(&path, &rel(&path), &config, false).unwrap();

        assert_eq!(action, FixAction::LicenseFileCreated);
        assert!(
            tmp.path().join("payload.license").exists(),
            "an extensionless file falls back to a plain `.license` suffix"
        );
    }

    #[test]
    fn a_dry_run_does_not_write_a_binary_companion() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("logo.png");
        fs::write(&path, b"\x89PNG").unwrap();
        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Acme")
            .build();

        let action = fix_binary_file(&path, &rel(&path), &config, true).unwrap();

        assert_eq!(action, FixAction::LicenseFileCreated);
        assert!(
            !tmp.path().join("logo.png.license").exists(),
            "--dry-run reports the companion it would create, but creates nothing"
        );
    }

    #[test]
    fn a_binary_companion_matches_the_shared_generator_byte_for_byte() {
        // `fix_binary_file` used to hand-roll this string with `format!`,
        // omitting the blank line `header::generate` inserts between the two
        // tags -- the same generator `annotate`'s Uncommentable branch calls.
        // Pinning to its exact output is what keeps the two commands from
        // drifting back onto separate implementations.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("logo.png");
        fs::write(&path, b"\x89PNG").unwrap();
        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Acme")
            .build();

        let action = fix_binary_file(&path, &rel(&path), &config, false).unwrap();

        assert_eq!(action, FixAction::LicenseFileCreated);
        let expected = crate::header::generate(
            "2026 Acme",
            "MIT",
            crate::header::CommentStyle::Uncommentable,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("logo.png.license")).unwrap(),
            expected,
            "fix must write exactly what the shared generator produces"
        );
    }

    #[test]
    fn a_binary_whose_default_licence_cannot_be_rendered_fails_the_file_not_the_run() {
        // The other half of the guard `fix_source_file` already has: routing
        // through `header::generate` means a malformed default expression is
        // now caught here too, rather than being written into the companion
        // unvalidated.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("logo.png");
        fs::write(&path, b"\x89PNG").unwrap();
        let config = Config::builder()
            .default_license("MIT AND AND GPL-3.0-only")
            .add_default_copyright("2026 Acme")
            .build();

        let action = fix_binary_file(&path, &rel(&path), &config, false).unwrap();

        assert!(matches!(action, FixAction::Failed(_)), "got {action:?}");
        assert!(
            !tmp.path().join("logo.png.license").exists(),
            "nothing may be written when the licence cannot be rendered"
        );
    }

    #[test]
    fn a_binary_with_resolvable_licensing_gets_a_companion_written() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("logo.png");
        fs::write(&path, b"\x89PNG").unwrap();
        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Acme")
            .build();

        let action = fix_binary_file(&path, &rel(&path), &config, false).unwrap();

        assert_eq!(action, FixAction::LicenseFileCreated);
        assert!(
            fs::read_to_string(tmp.path().join("logo.png.license"))
                .unwrap()
                .contains("SPDX-License-Identifier: MIT")
        );
    }

    #[test]
    fn a_file_whose_header_cannot_be_parsed_is_reported_not_fatal() {
        // A malformed expression made `format_file` return Err, which `?`
        // propagated all the way out of `fix_project`: one bad file aborted the
        // entire run with "failed to fix compliance issues" and nothing was
        // fixed anywhere. It has to be reported like any other unfixable file.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("bad.rs");
        fs::write(
            &path,
            "// SPDX-FileCopyrightText: 2026 Acme\n\
             // SPDX-License-Identifier: MIT AND AND GPL-3.0-only\n\nfn a() {}\n",
        )
        .unwrap();

        let action = fix_source_file(&path, &rel(&path), &Config::builder().build(), false)
            .expect("a bad header must not abort the run");

        assert!(matches!(action, FixAction::Failed(_)), "got {action:?}");
        assert!(
            fs::read_to_string(&path).unwrap().contains("fn a() {}"),
            "the file must be left alone"
        );
    }

    #[test]
    fn a_source_file_that_cannot_hold_a_comment_is_reported_not_fatal() {
        // `source-patterns` can force an uncommentable extension to be treated
        // as source. Reformatting then asks for a comment style that does not
        // exist, and that Err used to propagate out of `fix_project` and abort
        // the whole run rather than failing this one file.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("logo.png");
        fs::write(&path, b"\x89PNG").unwrap();
        fs::write(
            tmp.path().join("logo.png.license"),
            "SPDX-FileCopyrightText: 2026 Acme\nSPDX-License-Identifier: MIT\n",
        )
        .unwrap();
        let config = Config::builder().add_source_pattern("*.png").build();

        let action = fix_source_file(&path, &rel(&path), &config, false)
            .expect("an uncommentable source must not abort the run");

        assert!(matches!(action, FixAction::Failed(_)), "got {action:?}");
    }

    #[test]
    fn a_malformed_default_licence_fails_the_file_not_the_run() {
        // The other half of the same guard: here the bad expression comes from
        // the config's defaults rather than the file's own header, so it is
        // `annotate_file` that rejects it while writing a new header.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("plain.rs");
        fs::write(&path, "fn a() {}\n").unwrap();
        let config = Config::builder()
            .default_license("MIT AND AND GPL-3.0-only")
            .add_default_copyright("2026 Acme")
            .build();

        let action = fix_source_file(&path, &rel(&path), &config, false)
            .expect("a bad default must not abort the run");

        assert!(matches!(action, FixAction::Failed(_)), "got {action:?}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "fn a() {}\n",
            "nothing may be written when the licence cannot be rendered"
        );
    }

    #[test]
    fn one_unfixable_file_does_not_stop_the_others() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(
            root.join("bad.rs"),
            "// SPDX-FileCopyrightText: 2026 Acme\n\
             // SPDX-License-Identifier: MIT AND AND GPL-3.0-only\n\nfn a() {}\n",
        )
        .unwrap();
        // Its own valid header, so the bad default below cannot touch it --
        // otherwise both files fail and the test proves nothing about one
        // failure not stopping the rest.
        fs::write(
            root.join("good.rs"),
            "// SPDX-FileCopyrightText: 2026 Acme\n\
             //\n\
             // SPDX-License-Identifier: MIT\n\nfn b() {}\n",
        )
        .unwrap();

        // A malformed *default* is the failure fix cannot route around: a bad
        // tag in the file body is now filtered out by `header::detect`, so it
        // no longer reaches the writer at all.
        let config = Config::builder()
            .default_license("MIT AND AND GPL-3.0-only")
            .add_default_copyright("2026 Acme")
            .build();
        let report =
            fix_project(root, &config, false).expect("the run must complete despite the bad file");

        assert_eq!(report.total, 2, "both files reach the report");
        assert_eq!(report.failed, 1, "exactly the bad one failed");
    }

    #[test]
    fn a_file_an_annotation_already_licenses_is_left_alone() {
        // An annotation covering the file makes it compliant: `lint` passes it
        // with no inline header. Writing one in as well is the blanket rewrite
        // REUSE.toml's `**` aggregate exists to avoid, and it made `fix` report
        // work on a tree `lint` called clean. `annotate` is the opt-in writer
        // for per-file headers.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(&path, "fn main() {}\n").unwrap();

        let config = Config::builder()
            .add_annotation(Annotation {
                path: "**".to_string(),
                copyright: vec!["2026 Acme".to_string()],
                license: "MIT".to_string(),
                precedence: Precedence::Aggregate,
            })
            .build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();

        assert_eq!(action, FixAction::Unchanged);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "fn main() {}\n",
            "an already-compliant file must come back byte-identical"
        );
    }

    #[test]
    fn a_file_nothing_licenses_still_reports_a_failure() {
        // The counterpart: no annotation, no defaults, no header. `fix` cannot
        // invent licensing, so it has to say so rather than pass silently.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(&path, "fn main() {}\n").unwrap();

        let action =
            fix_source_file(&path, &rel(&path), &Config::builder().build(), false).unwrap();

        assert!(matches!(action, FixAction::Failed(_)), "got: {action:?}");
    }

    #[test]
    fn test_fix_source_file_adds_header() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        fs::write(&path, "fn main() { println!(\"hello\"); }\n").unwrap();

        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Test Corp")
            .build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();
        assert_eq!(action, FixAction::HeaderAdded);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("// SPDX-FileCopyrightText:"));
        assert!(content.contains("SPDX-License-Identifier:"));
    }

    #[test]
    fn test_fix_source_file_reformats_header() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        // Malformed header
        fs::write(
            &path,
            "//SPDX-FileCopyrightText:2025 Acme\n//SPDX-License-Identifier:MIT\n\nfn main() {}\n",
        )
        .unwrap();

        let config = Config::builder().build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();
        assert_eq!(action, FixAction::HeaderReformatted);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("// SPDX-FileCopyrightText: "));
        assert!(content.contains("// SPDX-License-Identifier: MIT\n"));
    }

    #[test]
    fn test_fix_source_file_unchanged_when_compliant() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("main.rs");
        // Already compliant AND already in canonical form -- note the `//`
        // separator, without which this would be reported as needing a reformat.
        fs::write(
            &path,
            "// SPDX-FileCopyrightText: 2025 Acme\n//\n// SPDX-License-Identifier: MIT\n\nfn main() {}\n",
        )
        .unwrap();

        let config = Config::builder().build();

        let action = fix_source_file(&path, &rel(&path), &config, false).unwrap();
        assert_eq!(action, FixAction::Unchanged);
    }

    #[test]
    fn test_fix_binary_creates_license_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("binary.bin");
        fs::write(&path, b"binary content").unwrap();

        // Create config with annotation for this file
        let _config = Config::builder().build();
        // Note: config doesn't have annotations by default in tests
        // This test would need a proper config with annotations

        let action =
            fix_binary_file(&path, &rel(&path), &Config::builder().build(), false).unwrap();
        // Without annotation, should return BinaryLicenseMissing
        assert_eq!(action, FixAction::BinaryLicenseMissing);
    }

    #[test]
    fn test_fix_binary_unchanged_when_companion_exists() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("binary.bin");
        fs::write(&path, b"binary content").unwrap();

        // Create companion license file
        let license_path = path.with_extension("bin.license");
        fs::write(
            &license_path,
            "SPDX-FileCopyrightText: 2025 Acme\nSPDX-License-Identifier: MIT\n",
        )
        .unwrap();

        let action =
            fix_binary_file(&path, &rel(&path), &Config::builder().build(), false).unwrap();
        assert_eq!(action, FixAction::Unchanged);
    }

    #[test]
    fn test_fix_project_integration() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create a source file without header
        fs::write(root.join("main.rs"), "fn main() { println!(\"hello\"); }\n").unwrap();

        // Create a binary without license
        fs::write(root.join("binary.bin"), b"binary").unwrap();

        let config = Config::builder()
            .default_license("MIT")
            .add_default_copyright("2026 Test Corp")
            .build();
        let report = fix_project(root, &config, false).unwrap();

        assert_eq!(report.total, 2); // main.rs + binary.bin
        assert!(report.fixed >= 1);
    }

    #[test]
    fn test_fix_report_is_clean() {
        // Clean report - no fixes needed
        let mut report = FixReport::new();
        report.add("file1.rs".to_string(), FixAction::Unchanged);
        report.add("file2.rs".to_string(), FixAction::Unchanged);
        assert!(report.is_clean());

        // Not clean - has fixed files
        let mut report2 = FixReport::new();
        report2.add("file1.rs".to_string(), FixAction::HeaderAdded);
        assert!(!report2.is_clean());

        // Not clean - has failed files
        let mut report3 = FixReport::new();
        report3.add(
            "file1.rs".to_string(),
            FixAction::Failed("error".to_string()),
        );
        assert!(!report3.is_clean());
    }
}
// REUSE-IgnoreEnd
