// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeSet;
use std::fs;

use anyhow::Result;
use comply::config::{Config, Licensing};
use comply::file::{self, FileType};
use comply::report::{Report, Status};
use comply::{header, license, spdx};

use crate::shared::{load_config, print_json_report, resolve_root};

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
    let (header_copyrights, header_license) = header::detect(&content)
        .map_or_else(Default::default, |h| (h.copyrights, h.license_expression));
    let resolved =
        config.resolve_licensing(rel_path, &header_copyrights, header_license.as_deref());

    let status = if resolved.copyrights.is_empty() {
        Status::MissingCopyright
    } else if resolved.license.is_none() {
        Status::MissingLicense
    } else {
        Status::Ok
    };
    (status, resolved)
}

/// Returns `true` when the project is compliant.
///
/// The caller decides the process exit code; keeping `process::exit` out of
/// here is what makes the non-compliant path testable.
pub(crate) fn run(cmd: &crate::Command) -> Result<bool> {
    let (path, json) = match cmd {
        crate::Command::Lint { path, json } => (path.as_ref(), *json),
        _ => return Ok(true),
    };

    let root = resolve_root(path)?;
    let config = load_config(&root)?;

    let mut builder = Report::builder();
    let mut used_licenses = BTreeSet::new();

    for entry in file::walk(&root, &config.ignore, &config.source_patterns) {
        let entry = entry?;
        let rel_path = entry.relative_to(&root);

        match entry.file_type {
            FileType::Ignored => {
                builder = builder.add_skipped(&rel_path);
            }
            FileType::Binary => {
                // A file that cannot carry a header is licensed either by a
                // `.license` companion or by a REUSE.toml annotation; both are
                // valid declarations, so both have to be consulted.
                let companion = entry
                    .has_license_file
                    .then(|| fs::read_to_string(file::license_companion_path(&entry.path)).ok())
                    .flatten()
                    .and_then(|text| header::detect(&text));
                let (companion_copyrights, companion_license) = companion
                    .map_or_else(Default::default, |h| (h.copyrights, h.license_expression));
                let resolved = config.resolve_licensing(
                    &rel_path,
                    &companion_copyrights,
                    companion_license.as_deref(),
                );

                if resolved.copyrights.is_empty() || resolved.license.is_none() {
                    builder =
                        builder.add_result(&rel_path, Status::MissingLicenseFile, vec![], None);
                } else {
                    if let Some(expression) = &resolved.license
                        && let Ok(parsed) = spdx::parse(expression)
                    {
                        used_licenses.extend(parsed.identifiers());
                    }
                    builder = builder.add_ok_with_header(
                        &rel_path,
                        resolved.copyrights,
                        resolved.license,
                    );
                }
            }
            FileType::Source => {
                let (status, resolved) = lint_source(&entry, &rel_path, &config);

                // Every identifier the expression names -- including a `WITH`
                // exception -- needs its own file in LICENSES/.
                if let Some(expression) = &resolved.license
                    && let Ok(parsed) = spdx::parse(expression)
                {
                    used_licenses.extend(parsed.identifiers());
                }

                builder =
                    builder.add_result(&rel_path, status, resolved.copyrights, resolved.license);
            }
        }
    }

    let report = builder
        .licenses(license::audit(&root, &used_licenses)?)
        .build();

    if json {
        print_json_report(&report)?;
    } else {
        print!("{report}");
    }

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
        }
    }

    fn project() -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        (tmp, root)
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
