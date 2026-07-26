// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::CommentStyle;
use super::generate::generate_with;
use super::parse::{
    detect, detect_style_from_content, detect_style_from_header, is_comment_line,
    is_plain_copyright_notice, strip_comment_prefix,
};
use crate::error::Error;

/// Annotate a file with an SPDX header.
///
/// If the file already has an SPDX header and `force` is false, the existing header is kept.
/// If `force` is true, any existing SPDX header is replaced.
/// If the file has no header, a new one is prepended.
///
/// Returns an error if the file is uncommentable (binary).
#[allow(clippy::missing_errors_doc)]
pub fn annotate_file(
    content: &str,
    copyright: &str,
    license_expression: &str,
    style_override: Option<CommentStyle>,
    force: bool,
) -> Result<String, Error> {
    annotate_file_with(
        content,
        copyright,
        license_expression,
        style_override,
        force,
        &[],
        false,
    )
}

/// Annotate a file with optional contributors and year exclusion.
#[allow(clippy::missing_errors_doc)]
pub fn annotate_file_with(
    content: &str,
    copyright: &str,
    license_expression: &str,
    style_override: Option<CommentStyle>,
    force: bool,
    contributors: &[String],
    exclude_year: bool,
) -> Result<String, Error> {
    crate::spdx::parse(license_expression)?;

    let style = style_override.unwrap_or_else(|| detect_style_from_content(content));

    if style == CommentStyle::Uncommentable {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "File is not commentable (binary)",
        )));
    }

    let existing_header = detect(content);

    if let Some(_existing) = existing_header {
        if !force {
            return Ok(content.to_string());
        }
        return replace_header_with(
            content,
            copyright,
            license_expression,
            style,
            contributors,
            exclude_year,
        );
    }

    generate_with(
        copyright,
        license_expression,
        style,
        contributors,
        exclude_year,
    )
    .map(|header| format!("{header}\n{content}"))
}

/// Format a file's SPDX header to canonical style.
///
/// If the file has an SPDX header, it is reformatted to canonical style.
/// Format an SPDX header in a file to canonical format.
///
/// If the file has no header, the content is returned unchanged.
///
/// The `style_override` parameter can force a specific comment style.
/// # Panics
///
/// Panics if `existing_header` is `None` (should be impossible after the `is_none()` check).
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::unwrap_used)]
pub fn format_file(content: &str, style_override: Option<CommentStyle>) -> Result<String, Error> {
    // Detect if there's an existing SPDX header
    let existing_header = detect(content);

    if existing_header.is_none() {
        // No header, return content unchanged
        return Ok(content.to_string());
    }

    // Safe because we checked `is_none()` above
    let existing = existing_header.as_ref().unwrap();

    // Extract copyright and license from existing header
    // Preserve original copyright format (first copyright line)
    let copyright = existing.copyrights.first().cloned().unwrap_or_default();

    // A header with no licence has nothing to canonicalise, and fabricating
    // `NOASSERTION` would assert a licence the project never chose -- writing a
    // claim into the file rather than reporting the gap. `lint` names it.
    let Some(license) = existing.license_expression.clone() else {
        return Ok(content.to_string());
    };

    // Determine target style
    let style = style_override.unwrap_or_else(|| {
        // Try to detect style from the existing header's comment syntax
        detect_style_from_header(content)
    });

    if style == CommentStyle::Uncommentable {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "File is not commentable (binary)",
        )));
    }

    // Replace the header with canonical formatting
    replace_header(content, &copyright, &license, style)
}

/// Replace an existing SPDX header in the content.
fn replace_header(
    content: &str,
    copyright: &str,
    license_expression: &str,
    style: CommentStyle,
) -> Result<String, Error> {
    replace_header_with(content, copyright, license_expression, style, &[], false)
}

/// Replace an existing SPDX header with optional contributors and year exclusion.
/// Whether a line declares licensing -- an SPDX tag or a plain copyright notice.
///
/// The tag names are matched in full, colon included. A bare `SPDX-` prefix
/// would also match prose such as `# SPDX-annotated, and clippy-clean`, and
/// treating that as a tag drags the surrounding comment into the header.
fn carries_licensing(line: &str) -> bool {
    // REUSE-IgnoreStart
    const TAGS: [&str; 3] = [
        "SPDX-FileCopyrightText:",
        "SPDX-FileContributor:",
        "SPDX-License-Identifier:",
    ];
    // REUSE-IgnoreEnd
    let stripped = strip_comment_prefix(line);
    TAGS.iter().any(|tag| stripped.starts_with(tag)) || is_plain_copyright_notice(&stripped)
}

/// Closing delimiter of a block comment, on its own line.
fn is_block_closer(line: &str) -> bool {
    matches!(line.trim(), "*/" | "*)" | "-->")
}

/// The half-open span of the SPDX header block within `lines`.
///
/// The block is the contiguous run of comment lines carrying SPDX tags. A blank
/// line ends that run -- which is how a file keeps a preamble above its header
/// -- a shebang is never part of it, and nothing after the last tag (beyond a
/// block's closing delimiter) belongs to it.
fn header_span(lines: &[&str]) -> (usize, usize) {
    let Some(first) = lines.iter().position(|l| carries_licensing(l)) else {
        return (0, 0);
    };

    // Only a block's opening delimiter is taken from above the first tag.
    // The reference tool also swallows any comment directly abutting the
    // header; comply deliberately does not, because that silently deletes
    // things like Cargo's "automatically @generated" marker.
    let mut start = first;
    if start > 0 && matches!(lines[start - 1].trim(), "/*" | "(*" | "<!--") {
        start -= 1;
    }

    // Only a tag advances the end; separators between tags are swept up on the
    // way, but a comment trailing the block is left where it is.
    let mut last = first;
    for (i, line) in lines.iter().enumerate().skip(first) {
        if carries_licensing(line) {
            last = i;
        } else if !(line.trim().is_empty() || is_comment_line(line)) {
            break;
        }
    }

    let mut end = last + 1;
    if lines.get(end).is_some_and(|l| is_block_closer(l)) {
        end += 1;
    }
    (start, end)
}

fn replace_header_with(
    content: &str,
    copyright: &str,
    license_expression: &str,
    style: CommentStyle,
    contributors: &[String],
    exclude_year: bool,
) -> Result<String, Error> {
    let lines: Vec<&str> = content.lines().collect();
    let (start, end) = header_span(&lines);

    let new_header = generate_with(
        copyright,
        license_expression,
        style,
        contributors,
        exclude_year,
    )?;

    let mut result = String::new();

    // Whatever sits above the header -- a shebang, or a preamble a blank line
    // has separated from it -- is not ours to rewrite. Trailing blanks are
    // dropped so exactly one separates it from the header.
    let preamble = lines[..start]
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map_or(0, |i| i + 1);
    for line in &lines[..preamble] {
        result.push_str(line);
        result.push('\n');
    }
    if preamble > 0 {
        result.push('\n');
    }

    // `generate_with` writes whole lines, so the header already ends in a
    // newline; this adds the blank line after it.
    result.push_str(&new_header);
    result.push('\n');

    for line in lines[end..].iter().skip_while(|l| l.trim().is_empty()) {
        result.push_str(line);
        result.push('\n');
    }

    Ok(result)
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    /// The header block is the contiguous run of comment lines carrying SPDX
    /// tags. A blank line ends it, a shebang is never part of it, and nothing
    /// past it is touched. Behaviours verified against `reuse` 6.2.0.
    mod header_boundary {
        use super::*;

        const HDR: &str = "# SPDX-FileCopyrightText: 2001 Old\n#\n# SPDX-License-Identifier: MIT\n";

        #[test]
        fn a_shebang_survives_and_keeps_line_one() {
            let content = format!("#!/bin/sh\n{HDR}\necho hi\n");

            let out = format_file(&content, Some(CommentStyle::Hash)).unwrap();

            assert!(
                out.starts_with("#!/bin/sh\n"),
                "the shebang must stay executable, got:\n{out}"
            );
            assert!(out.contains("echo hi"), "body must survive:\n{out}");
        }

        #[test]
        fn a_preamble_separated_by_a_blank_line_is_preserved() {
            let content = format!("# This file is @generated.\n\n{HDR}\nx = 1\n");

            let out = format_file(&content, Some(CommentStyle::Hash)).unwrap();

            assert!(
                out.starts_with("# This file is @generated.\n"),
                "a blank line separates a preamble from the header, got:\n{out}"
            );
        }

        /// A deliberate divergence: `reuse` treats a comment abutting the header
        /// as part of it and deletes it. comply keeps it, because that rule
        /// silently destroys things like Cargo's "@generated" marker.
        #[test]
        fn a_comment_adjacent_to_the_header_is_still_preserved() {
            let content = format!("# adjacent note\n{HDR}\nx = 1\n");

            let out = format_file(&content, Some(CommentStyle::Hash)).unwrap();

            assert!(out.contains("# adjacent note"), "got:\n{out}");
        }

        #[test]
        fn prose_mentioning_an_spdx_tag_is_not_treated_as_one() {
            // `SPDX-annotated` is not a tag; reading it as one drags the whole
            // surrounding comment into the header and deletes it.
            let content =
                format!("{HDR}\n# keeps sources SPDX-annotated and clippy-clean.\nx = 1\n");

            let out = format_file(&content, Some(CommentStyle::Hash)).unwrap();

            assert!(
                out.contains("SPDX-annotated and clippy-clean"),
                "got:\n{out}"
            );
        }

        #[test]
        fn a_comment_after_the_header_is_left_alone() {
            let content = format!("{HDR}\n# a note about the code below\nx = 1\n");

            let out = format_file(&content, Some(CommentStyle::Hash)).unwrap();

            assert!(
                out.contains("# a note about the code below"),
                "nothing past the header block may be consumed, got:\n{out}"
            );
        }

        #[test]
        fn a_block_comment_header_is_replaced_whole() {
            let content = "/*\n * SPDX-FileCopyrightText: 2001 Old\n *\n * SPDX-License-Identifier: MIT\n */\n\nbody {}\n";

            let out = format_file(content, Some(CommentStyle::SlashStar)).unwrap();

            assert_eq!(
                out.matches("/*").count(),
                1,
                "the old delimiters must not be left behind, got:\n{out}"
            );
            assert!(out.contains("body {}"), "got:\n{out}");
        }

        #[test]
        fn a_header_with_no_licence_is_left_untouched() {
            // Fabricating `NOASSERTION` would write a licence the project never
            // chose; leaving it alone lets `lint` report the real problem.
            let content = "# SPDX-FileCopyrightText: 2001 Old\n\nx = 1\n";

            let out = format_file(content, Some(CommentStyle::Hash)).unwrap();

            assert!(!out.contains("NOASSERTION"), "got:\n{out}");
            assert_eq!(out, content, "nothing to canonicalise, so nothing changes");
        }
    }

    #[test]
    fn annotating_with_an_unparseable_expression_fails() {
        let result = annotate_file_with(
            "fn main() {}\n",
            "2026 Acme",
            "NOT A VALID EXPRESSION",
            Some(CommentStyle::Slash),
            false,
            &[],
            false,
        );

        assert!(result.is_err());
    }

    #[test]
    fn reformatting_a_header_whose_expression_is_unparseable_fails() {
        let content = concat!(
            "// SPDX-FileCopyrightText: 2026 Acme\n",
            "// SPDX-License-Identifier: NOT A VALID EXPRESSION\n",
            "\n",
            "fn main() {}\n"
        );

        assert!(format_file(content, Some(CommentStyle::Slash)).is_err());
    }

    #[test]
    fn formatting_into_an_uncommentable_style_fails() {
        let content = concat!(
            "// SPDX-FileCopyrightText: 2026 Acme\n",
            "// SPDX-License-Identifier: MIT\n",
            "\n",
            "fn main() {}\n"
        );

        let err = format_file(content, Some(CommentStyle::Uncommentable))
            .expect_err("a binary format cannot hold a comment");

        assert!(err.to_string().contains("not commentable"), "got: {err}");
    }

    #[test]
    fn format_file_normalizes_slash_style() {
        let content = "// SPDX-FileCopyrightText: 2026 Acme Inc\n\
                        // SPDX-License-Identifier: MIT\n\
                        \n\
                        fn main() {}";
        let result = format_file(content, None).unwrap();
        // Should have consistent formatting
        assert!(result.starts_with("// SPDX-FileCopyrightText: 2026 Acme Inc\n"));
        assert!(result.contains("// SPDX-License-Identifier: MIT\n"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn format_file_normalizes_hash_style() {
        let content = "# SPDX-FileCopyrightText: 2026 Acme Inc\n\
                        # SPDX-License-Identifier: MIT\n\
                        \n\
                        def main():\n    pass";
        let result = format_file(content, None).unwrap();
        assert!(result.starts_with("# SPDX-FileCopyrightText: 2026 Acme Inc\n"));
        assert!(result.contains("# SPDX-License-Identifier: MIT\n"));
        assert!(result.contains("def main()"));
    }

    #[test]
    fn format_file_normalizes_html_style() {
        let content = "<!--\n\
                        SPDX-FileCopyrightText: 2026 Acme Inc\n\
                        SPDX-License-Identifier: MIT\n\
                        -->\n\
                        <html>";
        let result = format_file(content, None).unwrap();
        assert!(result.starts_with("<!--\n"));
        assert!(result.contains("SPDX-FileCopyrightText: 2026 Acme Inc\n"));
        assert!(result.contains("SPDX-License-Identifier: MIT\n"));
        assert!(result.contains("-->\n"));
        assert!(result.contains("<html>"));
    }

    #[test]
    fn format_file_converts_to_canonical_style() {
        // Header with mixed/inconsistent formatting gets normalized
        let content =
            "//SPDX-FileCopyrightText:2026 Acme Inc\n//SPDX-License-Identifier:MIT\n\nfn main() {}";
        let result = format_file(content, None).unwrap();
        assert!(result.starts_with("// SPDX-FileCopyrightText: 2026 Acme Inc\n"));
        assert!(result.contains("// SPDX-License-Identifier: MIT\n"));
    }

    #[test]
    fn format_file_preserves_no_header() {
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        let result = format_file(content, None).unwrap();
        // No header detected, content unchanged
        assert_eq!(result, content);
    }

    #[test]
    fn format_file_respects_style_override() {
        // File has hash-style header, but we want slash style
        let content = "# SPDX-FileCopyrightText: 2026 Acme Inc\n\
                        # SPDX-License-Identifier: MIT\n\
                        \n\
                        def main():\n    pass";
        let result = format_file(content, Some(CommentStyle::Slash)).unwrap();
        assert!(result.starts_with("// SPDX-FileCopyrightText: 2026 Acme Inc\n"));
        assert!(result.contains("// SPDX-License-Identifier: MIT\n"));
    }

    #[test]
    fn annotate_new_file_slash_style() {
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        let result = annotate_file(
            content,
            "2026 Acme Inc",
            "MIT",
            Some(CommentStyle::Slash),
            false,
        )
        .unwrap();
        assert!(result.starts_with("// SPDX-FileCopyrightText: 2026 Acme Inc"));
        assert!(result.contains("// SPDX-License-Identifier: MIT"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn annotate_new_file_hash_style() {
        let content = "def main():\n    print('hello')\n";
        let result = annotate_file(
            content,
            "2026 Acme Inc",
            "MIT",
            Some(CommentStyle::Hash),
            false,
        )
        .unwrap();
        assert!(result.starts_with("# SPDX-FileCopyrightText: 2026 Acme Inc"));
        assert!(result.contains("# SPDX-License-Identifier: MIT"));
        assert!(result.contains("def main()"));
    }

    #[test]
    fn annotate_new_file_html_style() {
        let content = "<html>\n<body>Hello</body>\n</html>\n";
        let result = annotate_file(
            content,
            "2026 Acme Inc",
            "MIT",
            Some(CommentStyle::Html),
            false,
        )
        .unwrap();
        assert!(result.starts_with("<!--"));
        assert!(result.contains("SPDX-FileCopyrightText: 2026 Acme Inc"));
        assert!(result.contains("SPDX-License-Identifier: MIT"));
        assert!(result.contains("<html>"));
    }

    #[test]
    fn annotate_existing_header_replaced_when_force() {
        let content = "// SPDX-FileCopyrightText: 2025 Old Corp\n\
                        // SPDX-License-Identifier: Apache-2.0\n\
                        \n\
                        fn main() {}";
        let result = annotate_file(
            content,
            "2026 New Corp",
            "MIT",
            None,
            true, // force = true
        )
        .unwrap();
        // Should have new copyright/license
        assert!(result.contains("2026 New Corp"));
        assert!(result.contains("MIT"));
        // Should NOT have old copyright/license
        assert!(!result.contains("2025 Old Corp"));
        assert!(!result.contains("Apache-2.0"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn annotate_existing_header_kept_when_no_force() {
        let content = "// SPDX-FileCopyrightText: 2025 Old Corp\n\
                        // SPDX-License-Identifier: Apache-2.0\n\
                        \n\
                        fn main() {}";
        let result = annotate_file(
            content,
            "2026 New Corp",
            "MIT",
            None,
            false, // force = false
        )
        .unwrap();
        // Should keep original
        assert!(result.contains("2025 Old Corp"));
        assert!(result.contains("Apache-2.0"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn annotate_uncommentable_file_returns_error() {
        // For uncommentable files, annotate_file should return an error
        // We can test this by using an empty string but specifying uncommentable style
        let result = annotate_file(
            "",
            "2026 Acme Inc",
            "MIT",
            Some(CommentStyle::Uncommentable),
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn annotate_file_with_contributors() {
        let content = "fn main() {}\n";
        let result = annotate_file_with(
            content,
            "2026 Acme Inc",
            "MIT",
            Some(CommentStyle::Slash),
            false,
            &["John Doe".to_string()],
            false,
        )
        .unwrap();
        assert!(result.contains("// SPDX-FileCopyrightText: 2026 Acme Inc"));
        assert!(result.contains("// SPDX-FileContributor: John Doe"));
        assert!(result.contains("// SPDX-License-Identifier: MIT"));
    }
}
// REUSE-IgnoreEnd
