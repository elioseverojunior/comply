// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Editing `REUSE.toml` annotations in place.

use crate::error::Error;

/// Merge an annotation for `paths` into `content`, returning the new content.
///
/// A block whose copyright, license and precedence all match is extended with
/// any missing paths. When every path is already covered the content is
/// returned unchanged, so repeated runs are idempotent. Only when no block
/// matches is a new one appended.
///
/// # Errors
///
/// Returns [`Error::Serde`] when `content` is not valid TOML, or when a
/// matching block was found but its `path` assignment could not be located in
/// the raw text.
pub fn upsert_annotation(
    content: &str,
    paths: &[String],
    license: &str,
    copyright: &str,
    precedence: &str,
) -> Result<String, Error> {
    let doc: toml::Value = toml::from_str(content)
        .map_err(|e| Error::Serde(format!("failed to parse REUSE.toml: {e}")))?;

    let blocks = doc
        .get("annotations")
        .and_then(toml::Value::as_array)
        .map_or_else(Vec::new, Clone::clone);

    let matched = blocks.iter().enumerate().find(|(_, b)| {
        field(b, "SPDX-License-Identifier") == Some(license)
            && field(b, "SPDX-FileCopyrightText") == Some(copyright)
            && field(b, "precedence") == Some(precedence)
    });

    let Some((index, block)) = matched else {
        return Ok(append_block(content, paths, license, copyright, precedence));
    };

    let existing = block_paths(block);
    let mut merged = existing.clone();
    for p in paths {
        if !merged.contains(p) {
            merged.push(p.clone());
        }
    }
    // Already covered: leave the file byte-identical so re-runs are no-ops.
    if merged == existing {
        return Ok(content.to_string());
    }

    replace_path_value(content, index, &merged)
}

/// Key order matches what taplo produces, so appending causes no format churn.
fn append_block(
    content: &str,
    paths: &[String],
    license: &str,
    copyright: &str,
    precedence: &str,
) -> String {
    format!(
        "{}\n\n[[annotations]]\nSPDX-FileCopyrightText = \"{copyright}\"\nSPDX-License-Identifier = \"{license}\"\npath = {}\nprecedence = \"{precedence}\"\n",
        content.trim_end(),
        format_path_array(paths)
    )
}

fn field<'a>(block: &'a toml::Value, key: &str) -> Option<&'a str> {
    block.get(key).and_then(toml::Value::as_str)
}

fn block_paths(block: &toml::Value) -> Vec<String> {
    match block.get("path") {
        Some(toml::Value::String(s)) => vec![s.clone()],
        Some(toml::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(ToString::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// Byte span of the `index`-th `[[annotations]]` block, header line included.
fn block_span(content: &str, index: usize) -> Option<(usize, usize)> {
    let mut seen = 0usize;
    let mut start: Option<usize> = None;
    let mut offset = 0usize;

    for line in content.split_inclusive('\n') {
        let trimmed = line.trim();
        if start.is_none() {
            if trimmed == "[[annotations]]" {
                if seen == index {
                    start = Some(offset);
                } else {
                    seen += 1;
                }
            }
        } else if trimmed.starts_with('[') {
            return start.map(|s| (s, offset));
        }
        offset += line.len();
    }

    start.map(|s| (s, content.len()))
}

/// Span of the value assigned to `path`, relative to the start of `block`.
fn path_value_span(block: &str) -> Option<(usize, usize)> {
    let mut offset = 0usize;
    for line in block.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("path")
            && let Some(eq) = line.find('=')
        {
            let indent = line[eq + 1..].len() - line[eq + 1..].trim_start().len();
            let value_start = offset + eq + 1 + indent;
            let end = value_end(&block[value_start..])?;
            return Some((value_start, value_start + end));
        }
        offset += line.len();
    }
    None
}

/// Length of the TOML value starting at the front of `s` (array or string).
fn value_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    match bytes.first()? {
        b'[' => {
            let mut in_string = false;
            let mut depth = 0usize;
            bytes
                .iter()
                .position(|&c| match c {
                    b'"' => {
                        in_string = !in_string;
                        false
                    }
                    b'[' if !in_string => {
                        depth += 1;
                        false
                    }
                    b']' if !in_string => {
                        depth -= 1;
                        depth == 0
                    }
                    _ => false,
                })
                .map(|i| i + 1)
        }
        b'"' => bytes.iter().skip(1).position(|&c| c == b'"').map(|p| p + 2),
        _ => None,
    }
}

/// Multi-line array in the layout taplo produces for this repo.
fn format_path_array(paths: &[String]) -> String {
    let mut out = String::from("[\n");
    for p in paths {
        out.push_str("  \"");
        out.push_str(p);
        out.push_str("\",\n");
    }
    out.push(']');
    out
}

fn replace_path_value(content: &str, index: usize, paths: &[String]) -> Result<String, Error> {
    let (start, end) = block_span(content, index)
        .ok_or_else(|| Error::Serde("could not locate the annotation block".to_string()))?;
    let block = content
        .get(start..end)
        .ok_or_else(|| Error::Serde("annotation block span out of range".to_string()))?;
    let (value_start, value_end) = path_value_span(block)
        .ok_or_else(|| Error::Serde("could not locate the `path` value".to_string()))?;

    let mut out = String::with_capacity(content.len() + 64);
    out.push_str(content.get(..start + value_start).unwrap_or_default());
    out.push_str(&format_path_array(paths));
    out.push_str(content.get(start + value_end..).unwrap_or_default());
    Ok(out)
}

/// DEP5's `*` is recursive, REUSE.toml's is not, so a lone `*` becomes `**`.
///
/// An existing `**` is left alone. Matches `reuse convert-dep5` 6.2.0, which
/// rewrites textually: `*.c` becomes `**.c`.
fn widen_globs(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 2);
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '*' {
            out.push(c);
            continue;
        }
        let mut stars = 1;
        while chars.peek() == Some(&'*') {
            chars.next();
            stars += 1;
        }
        // A single star widens; two or more already recurse.
        if stars == 1 {
            out.push_str("**");
        } else {
            for _ in 0..stars {
                out.push('*');
            }
        }
    }
    out
}

/// Render a TOML string or array-of-strings value.
fn toml_value(values: &[String]) -> String {
    if let [single] = values {
        return format!("\"{single}\"");
    }
    let joined = values
        .iter()
        .map(|v| format!("\"{v}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{joined}]")
}

/// Render a parsed `.reuse/dep5` document as an equivalent REUSE.toml.
///
/// Field order, `precedence = "aggregate"` and the string-vs-array choice all
/// match `reuse convert-dep5` 6.2.0 so a converted project produces no diff
/// against the reference tool.
#[must_use]
pub fn render_dep5_as_toml(doc: &crate::config::Dep5Document) -> String {
    use std::fmt::Write as _;

    let mut out = String::from("version = 1\n");
    if let Some(name) = &doc.package_name {
        let _ = writeln!(out, "SPDX-PackageName = \"{name}\"");
    }
    if let Some(supplier) = &doc.package_supplier {
        let _ = writeln!(out, "SPDX-PackageSupplier = \"{supplier}\"");
    }
    if let Some(location) = &doc.package_download_location {
        let _ = writeln!(out, "SPDX-PackageDownloadLocation = \"{location}\"");
    }

    for annotation in &doc.annotations {
        // DEP5 allows several whitespace-separated patterns in one `Files:`.
        let paths: Vec<String> = annotation
            .path
            .split_whitespace()
            .map(widen_globs)
            .collect();

        out.push_str("\n[[annotations]]\n");
        let _ = writeln!(out, "path = {}", toml_value(&paths));
        out.push_str("precedence = \"aggregate\"\n");
        let _ = writeln!(
            out,
            "SPDX-FileCopyrightText = {}",
            toml_value(&annotation.copyright)
        );
        let _ = writeln!(out, "SPDX-License-Identifier = \"{}\"", annotation.license);
    }
    out
}

#[cfg(test)]
mod tests {
    mod dep5_conversion {
        use super::*;
        use crate::config::Config;

        /// The expected output is `reuse convert-dep5` 6.2.0's, byte for byte.
        #[test]
        fn a_document_renders_exactly_as_the_reference_tool_writes_it() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("dep5");
            std::fs::write(
                &path,
                concat!(
                    "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n",
                    "Upstream-Name: example\n",
                    "Upstream-Contact: Jane <jane@example.com>\n",
                    "Source: https://example.com\n",
                    "\n",
                    "Files: src/*\n",
                    "Copyright: 2026 Jane Doe\n",
                    "License: MIT\n",
                    "\n",
                    "Files: docs/*\n",
                    "Copyright: 2026 Jane Doe\n",
                    "           2025 Old Author\n",
                    "License: CC-BY-4.0\n",
                ),
            )
            .unwrap();

            let doc = Config::dep5_document(&path).unwrap();

            assert_eq!(
                render_dep5_as_toml(&doc),
                concat!(
                    "version = 1\n",
                    "SPDX-PackageName = \"example\"\n",
                    "SPDX-PackageSupplier = \"Jane <jane@example.com>\"\n",
                    "SPDX-PackageDownloadLocation = \"https://example.com\"\n",
                    "\n[[annotations]]\n",
                    "path = \"src/**\"\n",
                    "precedence = \"aggregate\"\n",
                    "SPDX-FileCopyrightText = \"2026 Jane Doe\"\n",
                    "SPDX-License-Identifier = \"MIT\"\n",
                    "\n[[annotations]]\n",
                    "path = \"docs/**\"\n",
                    "precedence = \"aggregate\"\n",
                    "SPDX-FileCopyrightText = [\"2026 Jane Doe\", \"2025 Old Author\"]\n",
                    "SPDX-License-Identifier = \"CC-BY-4.0\"\n",
                )
            );
        }

        #[test]
        fn several_patterns_in_one_files_stanza_become_an_array() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("dep5");
            std::fs::write(&path, "Files: a/* b/*\nCopyright: 2026 C\nLicense: MIT\n").unwrap();

            let rendered = render_dep5_as_toml(&Config::dep5_document(&path).unwrap());

            assert!(
                rendered.contains("path = [\"a/**\", \"b/**\"]"),
                "got:\n{rendered}"
            );
        }

        #[test]
        fn a_lone_star_widens_but_an_existing_double_star_is_left_alone() {
            assert_eq!(widen_globs("*"), "**");
            assert_eq!(widen_globs("src/*"), "src/**");
            assert_eq!(widen_globs("*.c"), "**.c");
            assert_eq!(widen_globs("docs/*.md"), "docs/**.md");
            assert_eq!(widen_globs("a/**"), "a/**");
            assert_eq!(widen_globs("foo.c"), "foo.c");
        }

        #[test]
        fn a_document_with_no_header_starts_straight_at_the_version() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("dep5");
            std::fs::write(&path, "Files: *\nCopyright: A\nLicense: MIT\n").unwrap();

            let rendered = render_dep5_as_toml(&Config::dep5_document(&path).unwrap());

            assert!(
                rendered.starts_with("version = 1\n\n[[annotations]]"),
                "got:\n{rendered}"
            );
        }
    }

    use super::*;

    // REUSE-IgnoreStart
    const ONE_BLOCK: &str = r#"version = 1

[[annotations]]
SPDX-FileCopyrightText = "COMPLY contributors"
SPDX-License-Identifier = "MIT"
path = [
  "src/a.rs",
]
precedence = "override"
"#;

    #[test]
    fn merges_a_new_path_into_an_existing_matching_block() {
        let out = upsert_annotation(
            ONE_BLOCK,
            &["src/b.rs".to_string()],
            "MIT",
            "COMPLY contributors",
            "override",
        )
        .unwrap();

        assert_eq!(
            out.matches("[[annotations]]").count(),
            1,
            "must not create a second block, got:\n{out}"
        );
        assert!(out.contains("src/a.rs"), "existing path kept:\n{out}");
        assert!(out.contains("src/b.rs"), "new path merged in:\n{out}");
    }

    #[test]
    fn appends_a_new_block_when_no_existing_block_matches() {
        let out = upsert_annotation(
            ONE_BLOCK,
            &["src/b.rs".to_string()],
            "Apache-2.0",
            "COMPLY contributors",
            "override",
        )
        .unwrap();

        assert_eq!(
            out.matches("[[annotations]]").count(),
            2,
            "a non-matching annotation needs its own block, got:\n{out}"
        );
        assert!(out.contains("Apache-2.0"), "new license recorded:\n{out}");
        assert!(out.contains("src/b.rs"), "new path recorded:\n{out}");
    }

    /// Deliberately an INLINE array: regenerating it would reflow to
    /// multi-line, so this fixture detects a missing already-covered guard.
    const INLINE_BLOCK: &str = r#"version = 1

[[annotations]]
SPDX-FileCopyrightText = "COMPLY contributors"
SPDX-License-Identifier = "MIT"
path = ["src/a.rs"]
precedence = "override"
"#;

    /// `path` may legally be a bare string rather than an array.
    const STRING_PATH_BLOCK: &str = r#"version = 1

[[annotations]]
SPDX-FileCopyrightText = "COMPLY contributors"
SPDX-License-Identifier = "MIT"
path = "src/a.rs"
precedence = "override"
"#;

    #[test]
    fn merges_into_a_block_whose_path_is_a_bare_string() {
        let out = upsert_annotation(
            STRING_PATH_BLOCK,
            &["src/b.rs".to_string()],
            "MIT",
            "COMPLY contributors",
            "override",
        )
        .unwrap();

        assert_eq!(out.matches("[[annotations]]").count(), 1, "got:\n{out}");
        assert!(out.contains("src/a.rs"), "existing path kept:\n{out}");
        assert!(out.contains("src/b.rs"), "new path merged in:\n{out}");
    }

    const TWO_BLOCKS: &str = r#"version = 1

[[annotations]]
SPDX-FileCopyrightText = "COMPLY contributors"
SPDX-License-Identifier = "Apache-2.0"
path = [
  "first.rs",
]
precedence = "override"

[[annotations]]
SPDX-FileCopyrightText = "COMPLY contributors"
SPDX-License-Identifier = "MIT"
path = [
  "second.rs",
]
precedence = "override"
"#;

    #[test]
    fn merges_into_the_first_of_several_blocks_without_disturbing_the_rest() {
        let out = upsert_annotation(
            TWO_BLOCKS,
            &["added.rs".to_string()],
            "Apache-2.0",
            "COMPLY contributors",
            "override",
        )
        .unwrap();

        assert_eq!(out.matches("[[annotations]]").count(), 2, "got:\n{out}");
        assert!(out.contains("first.rs") && out.contains("added.rs"));
        assert!(
            out.contains("second.rs"),
            "the later block survives:\n{out}"
        );
    }

    #[test]
    fn merges_into_a_later_block() {
        let out = upsert_annotation(
            TWO_BLOCKS,
            &["added.rs".to_string()],
            "MIT",
            "COMPLY contributors",
            "override",
        )
        .unwrap();

        assert_eq!(out.matches("[[annotations]]").count(), 2, "got:\n{out}");
        assert!(out.contains("second.rs") && out.contains("added.rs"));
    }

    fn block_with_path(path_line: &str) -> String {
        format!(
            concat!(
                "version = 1\n\n",
                "[[annotations]]\n",
                "SPDX-FileCopyrightText = \"COMPLY contributors\"\n",
                "SPDX-License-Identifier = \"MIT\"\n",
                "{}\n",
                "precedence = \"override\"\n"
            ),
            path_line
        )
    }

    fn upsert_b_into(content: &str) -> Result<String, Error> {
        upsert_annotation(
            content,
            &["src/b.rs".to_string()],
            "MIT",
            "COMPLY contributors",
            "override",
        )
    }

    #[test]
    fn a_scalar_path_value_is_rejected_rather_than_overwritten() {
        let result = upsert_b_into(&block_with_path("path = 42"));

        assert!(
            result.is_err(),
            "a `path` that is neither a string nor an array has no value span to replace"
        );
    }

    /// Documents CURRENT behaviour: a nested array yields no readable paths, so
    /// the merge treats the block as empty and the nested entry is dropped.
    #[test]
    fn a_nested_path_array_is_flattened_and_its_entries_lost() {
        let out = upsert_b_into(&block_with_path("path = [[\"nested.rs\"]]")).unwrap();

        assert!(out.contains("src/b.rs"), "got:\n{out}");
        assert!(!out.contains("nested.rs"), "got:\n{out}");
    }

    #[test]
    fn a_block_with_no_path_key_is_rejected() {
        let content = concat!(
            "version = 1\n\n",
            "[[annotations]]\n",
            "SPDX-FileCopyrightText = \"COMPLY contributors\"\n",
            "SPDX-License-Identifier = \"MIT\"\n",
            "precedence = \"override\"\n"
        );

        let result = upsert_annotation(
            content,
            &["src/b.rs".to_string()],
            "MIT",
            "COMPLY contributors",
            "override",
        );

        assert!(result.is_err(), "there is no `path` value to extend");
    }

    #[test]
    fn leaves_content_byte_identical_when_the_path_is_already_covered() {
        let out = upsert_annotation(
            INLINE_BLOCK,
            &["src/a.rs".to_string()],
            "MIT",
            "COMPLY contributors",
            "override",
        )
        .unwrap();

        assert_eq!(out, INLINE_BLOCK, "re-running must be a byte-level no-op");
    }
    // REUSE-IgnoreEnd
}
