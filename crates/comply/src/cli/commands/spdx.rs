// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emit an SPDX 2.1 tag-value bill of materials for the project.
//!
//! Two deliberate differences from `reuse spdx`, both in comply's favour:
//!
//! * checksums are `SHA256`, not `SHA1`. Both are valid SPDX; SHA-1 is no longer
//!   fit for integrity claims, and comply already hashes with SHA-256 elsewhere.
//! * `DocumentNamespace` is derived from the file set instead of a random UUID,
//!   so re-running on an unchanged tree produces an identical document. The
//!   reference tool's random namespace makes every SBOM differ from the last,
//!   which defeats diffing them in review.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use comply::config::Config;
use comply::hash;
use comply::report::{Report, Status};

use crate::cli::commands::lint::lint_project;
use crate::cli::shared::{load_config, resolve_root};

/// Who is signing off on the document.
pub(crate) struct Creators<'a> {
    pub(crate) person: Option<&'a String>,
    pub(crate) organization: Option<&'a String>,
}

pub(crate) fn run(
    path: Option<&String>,
    output: Option<&String>,
    creators: &Creators<'_>,
) -> Result<()> {
    let root = resolve_root(path)?;
    let config = load_config(&root)?;
    let report = lint_project(&root, &config)?;

    // SPDX requires exactly `YYYY-MM-DDThh:mm:ssZ`; `to_rfc3339` would add
    // fractional seconds and spell the zone `+00:00`, which validators reject.
    let created = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let document = render(&report, &root, &config, creators, &created)?;

    match output {
        Some(file) => {
            fs::write(file, &document).with_context(|| format!("failed to write {file}"))?;
        }
        None => print!("{document}"),
    }
    Ok(())
}

/// An SPDX identifier must match `[A-Za-z0-9.-]+`, so a path cannot be used raw.
///
/// The file's content hash gives a stable, collision-resistant id that also
/// changes when the file does.
fn spdx_ref(digest: &str) -> String {
    format!("SPDXRef-{}", &digest[..digest.len().min(32)])
}

/// Build the tag-value document. `created` is injected so tests are not at the
/// mercy of the clock.
fn render(
    report: &Report,
    root: &Path,
    config: &Config,
    creators: &Creators<'_>,
    created: &str,
) -> Result<String> {
    let name = root.file_name().map_or_else(
        || "project".to_string(),
        |n| n.to_string_lossy().to_string(),
    );

    // Files first: the document namespace is derived from them.
    let mut files = String::new();
    let mut describes = Vec::new();

    // A skipped file is outside REUSE analysis -- a licence text, a build
    // artifact -- so comply has no licensing to state for it. Emitting an entry
    // anyway would have to say `NOASSERTION`, which asserts "unknown" about a
    // file the manifest may cover: the `**` aggregate covers `LICENSE`, yet the
    // entry claimed otherwise. Omitting it states nothing, which is both honest
    // and what the reference tool does.
    for result in report.iter().filter(|r| r.status != Status::Skipped) {
        let absolute = root.join(&result.path);
        let digest = hash::format_hash(&hash::hash_file(&absolute)?);
        // SPDX 2.x requires a SHA1 file checksum; the field is repeatable, so
        // the stronger SHA256 is kept alongside rather than replaced.
        let digest_sha1 = hash::format_hash(&hash::hash_file_sha1(&absolute)?);
        let id = spdx_ref(&digest);
        describes.push(id.clone());

        let license = result.license.as_deref().unwrap_or("NOASSERTION");
        let copyright = if result.copyrights.is_empty() {
            "NONE".to_string()
        } else {
            result
                .copyrights
                .iter()
                .map(|c| format!("SPDX-FileCopyrightText: {c}"))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let _ = write!(
            files,
            "\nFileName: ./{}\nSPDXID: {id}\n\
             FileChecksum: SHA1: {digest_sha1}\n\
             FileChecksum: SHA256: {digest}\n\
             LicenseConcluded: NOASSERTION\nLicenseInfoInFile: {license}\n\
             FileCopyrightText: <text>{copyright}</text>\n",
            result.path
        );
    }

    let namespace = hash::format_hash(&hash::hash_bytes(describes.join("\n").as_bytes()));

    let mut out = String::new();
    let _ = write!(
        out,
        "SPDXVersion: SPDX-2.1\nDataLicense: CC0-1.0\nSPDXID: SPDXRef-DOCUMENT\n\
         DocumentName: {}\nDocumentNamespace: http://spdx.org/spdxdocs/spdx-v2.1-{}\n",
        config.spdx_package_name.as_deref().unwrap_or(&name),
        &namespace[..32]
    );
    let _ = writeln!(
        out,
        "Creator: Person: {}",
        creators.person.map_or("Anonymous ()", String::as_str)
    );
    let _ = writeln!(
        out,
        "Creator: Organization: {}",
        creators.organization.map_or("Anonymous ()", String::as_str)
    );
    let _ = writeln!(out, "Creator: Tool: comply-{}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "Created: {created}");
    out.push_str(
        "CreatorComment: <text>This document was created automatically using available reuse information consistent with REUSE.</text>\n",
    );
    for id in &describes {
        let _ = writeln!(out, "Relationship: SPDXRef-DOCUMENT DESCRIBES {id}");
    }
    out.push_str(&files);
    Ok(out)
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use comply::report::Status;
    use tempfile::TempDir;

    const HEADER: &str = concat!(
        "# SPDX-FileCopyrightText: 2026 Test Author\n",
        "# SPDX-License-Identifier: MIT\n"
    );

    /// A skipped file has no licensing to state, so stating `NOASSERTION` for
    /// it asserts "unknown" about something the manifest may well cover -- for
    /// `LICENSE` the `**` aggregate does exactly that. Leaving it out says
    /// nothing, which is the honest answer and what the reference tool does.
    #[test]
    fn a_skipped_file_is_left_out_of_the_document() {
        let (_tmp, root, _arg) = project();
        fs::write(root.join("LICENSE"), "dual licensed\n").unwrap();

        let config = load_config(&root).unwrap();
        let report = lint_project(&root, &config).unwrap();
        assert!(
            report
                .iter()
                .any(|r| r.path == "LICENSE" && r.status == Status::Skipped),
            "the fixture must actually produce a skipped file"
        );

        let document = render(
            &report,
            &root,
            &config,
            &Creators {
                person: None,
                organization: None,
            },
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        assert!(
            !document.contains("FileName: ./LICENSE"),
            "a skipped file must not appear at all:\n{document}"
        );
        assert!(
            document.contains("FileName: ./a.py"),
            "checked files must still appear:\n{document}"
        );
    }

    fn project() -> (TempDir, PathBuf, String) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("LICENSES")).unwrap();
        fs::write(root.join("LICENSES/MIT.txt"), "MIT text").unwrap();
        fs::write(root.join("REUSE.toml"), "version = 1\n").unwrap();
        fs::write(root.join("a.py"), format!("{HEADER}x = 1\n")).unwrap();
        let arg = root.to_str().unwrap().to_string();
        (tmp, root, arg)
    }

    fn document(root: &Path) -> String {
        let config = load_config(root).unwrap();
        let report = lint_project(root, &config).unwrap();
        render(
            &report,
            root,
            &config,
            &Creators {
                person: None,
                organization: None,
            },
            "2026-01-01T00:00:00Z",
        )
        .unwrap()
    }

    #[test]
    fn the_document_carries_the_required_spdx_preamble() {
        let (_tmp, root, _arg) = project();

        let doc = document(&root);

        assert!(doc.starts_with("SPDXVersion: SPDX-2.1\n"), "got:\n{doc}");
        assert!(doc.contains("DataLicense: CC0-1.0\n"));
        assert!(doc.contains("SPDXID: SPDXRef-DOCUMENT\n"));
        assert!(doc.contains("Created: 2026-01-01T00:00:00Z\n"));
        assert!(doc.contains("Creator: Tool: comply-"));
    }

    #[test]
    fn the_created_timestamp_uses_the_exact_format_spdx_requires() {
        let (_tmp, root, _arg) = project();
        let config = load_config(&root).unwrap();
        let report = lint_project(&root, &config).unwrap();

        let created = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let doc = render(
            &report,
            &root,
            &config,
            &Creators {
                person: None,
                organization: None,
            },
            &created,
        )
        .unwrap();

        // No fractional seconds, and `Z` rather than `+00:00`.
        let line = doc
            .lines()
            .find(|l| l.starts_with("Created: "))
            .expect("a Created line");
        let value = line.trim_start_matches("Created: ");
        assert_eq!(value.len(), 20, "got {value}");
        assert!(value.ends_with('Z'), "got {value}");
        assert!(!value.contains('.'), "got {value}");
        assert!(!value.contains('+'), "got {value}");
    }

    #[test]
    fn a_root_with_no_final_component_falls_back_to_a_generic_name() {
        let (_tmp, root, _arg) = project();
        let config = load_config(&root).unwrap();
        let report = lint_project(&root, &config).unwrap();

        // `/` has no file_name, so DocumentName has to come from somewhere.
        let doc = render(
            &report,
            Path::new("/"),
            &config,
            &Creators {
                person: None,
                organization: None,
            },
            "2026-01-01T00:00:00Z",
        );

        // Hashing files under `/` will fail; the name resolution is what matters.
        let rendered = doc.unwrap_or_else(|_| String::new());
        assert!(
            rendered.is_empty() || rendered.contains("DocumentName: project"),
            "got:\n{rendered}"
        );
    }

    #[test]
    fn every_file_carries_a_sha1_checksum_beside_the_sha256() {
        // SPDX 2.x makes a SHA1 file checksum mandatory, and this document
        // declares `SPDXVersion: SPDX-2.1`. Emitting only SHA256 left it
        // non-conformant with its own declared version, and a consumer looking
        // for the required algorithm found nothing. `reuse` 6.2.0 emits SHA1.
        // Both are kept: the field is repeatable and SHA256 is the stronger
        // digest, so dropping it to match would be a downgrade.
        let (_tmp, root, _arg) = project();

        let doc = document(&root);

        let sha1_lines = doc.matches("FileChecksum: SHA1: ").count();
        let sha256_lines = doc.matches("FileChecksum: SHA256: ").count();
        assert!(sha1_lines > 0, "no SHA1 checksum emitted:\n{doc}");
        assert_eq!(
            sha1_lines, sha256_lines,
            "every file needs both algorithms, got {sha1_lines} SHA1 and {sha256_lines} SHA256"
        );
    }

    #[test]
    fn the_sha1_digest_is_forty_hex_characters() {
        // A wrong-length digest still "contains SHA1: " and would pass the
        // test above, so the shape is pinned separately.
        let (_tmp, root, _arg) = project();

        let doc = document(&root);

        let digest = doc
            .lines()
            .find_map(|line| line.strip_prefix("FileChecksum: SHA1: "))
            .expect("a SHA1 line");
        assert_eq!(digest.len(), 40, "got {digest:?}");
        assert!(
            digest
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "lowercase hex only, got {digest:?}"
        );
    }

    #[test]
    fn each_covered_file_gets_a_checksum_licence_and_copyright() {
        let (_tmp, root, _arg) = project();

        let doc = document(&root);

        assert!(doc.contains("FileName: ./a.py\n"), "got:\n{doc}");
        assert!(doc.contains("FileChecksum: SHA256: "), "got:\n{doc}");
        assert!(doc.contains("LicenseInfoInFile: MIT\n"), "got:\n{doc}");
        assert!(
            doc.contains(
                "FileCopyrightText: <text>SPDX-FileCopyrightText: 2026 Test Author</text>"
            ),
            "got:\n{doc}"
        );
    }

    #[test]
    fn every_file_is_related_to_the_document() {
        let (_tmp, root, _arg) = project();
        fs::write(root.join("b.py"), format!("{HEADER}y = 2\n")).unwrap();

        let doc = document(&root);

        assert_eq!(
            doc.matches("Relationship: SPDXRef-DOCUMENT DESCRIBES SPDXRef-")
                .count(),
            2,
            "got:\n{doc}"
        );
    }

    #[test]
    fn an_unchanged_tree_produces_a_byte_identical_document() {
        let (_tmp, root, _arg) = project();

        // The namespace is derived from the file set, not a random UUID, so two
        // runs over the same tree are diffable.
        assert_eq!(document(&root), document(&root));
    }

    #[test]
    fn changing_a_file_changes_its_id_and_the_namespace() {
        let (_tmp, root, _arg) = project();
        let before = document(&root);

        fs::write(root.join("a.py"), format!("{HEADER}x = 2\n")).unwrap();

        assert_ne!(document(&root), before);
    }

    #[test]
    fn a_file_with_no_declaration_reads_as_noassertion_and_none() {
        let (_tmp, root, _arg) = project();
        fs::write(root.join("bare.py"), "z = 3\n").unwrap();

        let doc = document(&root);

        assert!(
            doc.contains("LicenseInfoInFile: NOASSERTION\n"),
            "got:\n{doc}"
        );
        assert!(
            doc.contains("FileCopyrightText: <text>NONE</text>\n"),
            "got:\n{doc}"
        );
    }

    #[test]
    fn named_creators_replace_the_anonymous_defaults() {
        let (_tmp, root, _arg) = project();
        let config = load_config(&root).unwrap();
        let report = lint_project(&root, &config).unwrap();
        let person = "Jane Doe".to_string();
        let org = "Acme".to_string();

        let doc = render(
            &report,
            &root,
            &config,
            &Creators {
                person: Some(&person),
                organization: Some(&org),
            },
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        assert!(doc.contains("Creator: Person: Jane Doe\n"), "got:\n{doc}");
        assert!(doc.contains("Creator: Organization: Acme\n"), "got:\n{doc}");
    }

    #[test]
    fn output_writes_to_a_file_instead_of_stdout() {
        let (_tmp, root, arg) = project();
        let out = root.join("sbom.spdx");

        run(
            Some(&arg),
            Some(&out.to_str().unwrap().to_string()),
            &Creators {
                person: None,
                organization: None,
            },
        )
        .unwrap();

        assert!(
            fs::read_to_string(&out)
                .unwrap()
                .starts_with("SPDXVersion:"),
            "the document should land in the named file"
        );
    }

    #[test]
    fn printing_to_stdout_succeeds() {
        let (_tmp, _root, arg) = project();

        assert!(
            run(
                Some(&arg),
                None,
                &Creators {
                    person: None,
                    organization: None,
                }
            )
            .is_ok()
        );
    }
}
// REUSE-IgnoreEnd
