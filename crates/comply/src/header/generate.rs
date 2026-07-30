// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Write;

use super::CommentStyle;
use super::parse::YEAR_PREFIX_RE;
use super::prefix::CopyrightNotice;
use super::style::ResolvedStyle;
use crate::error::Error;

/// Generate an SPDX header block for a file.
#[allow(clippy::missing_errors_doc)]
pub fn generate(
    copyright: &str,
    license_expression: &str,
    style: CommentStyle,
) -> Result<String, Error> {
    generate_with(
        copyright,
        license_expression,
        ResolvedStyle::builtin(style),
        &[],
        false,
    )
}

/// Generate an SPDX header block with optional contributors.
///
/// `copyright` takes a bare `&str` for the default `SPDX-` `FileCopyrightText`
/// form, or a [`CopyrightNotice`] to choose one of the other fifteen.
#[allow(clippy::missing_errors_doc)]
pub fn generate_with<'a>(
    copyright: impl Into<CopyrightNotice<'a>>,
    license_expression: &str,
    style: ResolvedStyle<'_>,
    contributors: &[String],
    exclude_year: bool,
) -> Result<String, Error> {
    crate::spdx::parse(license_expression)?;

    let notice = copyright.into();
    let copyright_text = if exclude_year {
        YEAR_PREFIX_RE.replace(notice.text(), "").to_string()
    } else {
        notice.text().to_string()
    };

    let (prefix, suffix) = style.delimiters();
    let lead = style.lead();
    let is_block = style.is_block();

    let mut out = String::new();
    if is_block {
        let _ = writeln!(out, "{prefix}");
    }
    let _ = writeln!(out, "{lead}{}{copyright_text}", notice.prefix().tag());
    // REUSE-IgnoreStart
    for c in contributors {
        let _ = writeln!(out, "{lead}SPDX-FileContributor: {c}");
    }
    // The reference tool always separates the copyright block from the licence,
    // with the lead stripped of its trailing space. Matching it byte-for-byte is
    // what stops the two tools rewriting each other's headers.
    let _ = writeln!(out, "{}", lead.trim_end());
    let _ = writeln!(out, "{lead}SPDX-License-Identifier: {license_expression}");
    // Block delimiters are always non-empty; line styles have no suffix. The
    // closer aligns under the lead's `*`, so it inherits the lead's indent.
    if !suffix.is_empty() {
        let indent = &lead[..lead.len() - lead.trim_start().len()];
        let _ = writeln!(out, "{indent}{suffix}");
    }

    Ok(out)
}
// REUSE-IgnoreEnd

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CommentStyleRule;

    fn rule(open: &str, line: &str, close: &str) -> CommentStyleRule {
        CommentStyleRule {
            extensions: vec!["vue".to_string()],
            filenames: vec![],
            open: open.to_string(),
            line: line.to_string(),
            close: close.to_string(),
        }
    }

    #[test]
    fn a_configured_line_style_renders_with_its_own_prefix() {
        let rules = [rule("@@", "", "")];

        let out = generate_with(
            "2026 Acme",
            "MIT",
            ResolvedStyle::new(CommentStyle::Custom(0), &rules),
            &[],
            false,
        )
        .expect("a resolved style must render");

        assert_eq!(
            out,
            "@@ SPDX-FileCopyrightText: 2026 Acme\n@@\n@@ SPDX-License-Identifier: MIT\n"
        );
    }

    #[test]
    fn a_configured_block_style_brackets_the_header() {
        let rules = [rule("<%--", " * ", "--%>")];

        let out = generate_with(
            "2026 Acme",
            "MIT",
            ResolvedStyle::new(CommentStyle::Custom(0), &rules),
            &[],
            false,
        )
        .expect("a resolved block style must render");

        // The closer aligns under the body's indent, exactly as `/* * */` does.
        assert_eq!(
            out,
            "<%--\n * SPDX-FileCopyrightText: 2026 Acme\n *\n * SPDX-License-Identifier: MIT\n --%>\n"
        );
    }

    #[test]
    fn a_stale_index_falls_back_rather_than_panicking() {
        // The manifest shrank after the style was resolved. Rendering with the
        // built-in fallback beats indexing out of bounds mid-run.
        let out = generate_with(
            "2026 Acme",
            "MIT",
            ResolvedStyle::new(CommentStyle::Custom(9), &[]),
            &[],
            false,
        )
        .expect("a stale index must not panic");

        assert!(out.contains("SPDX-License-Identifier: MIT"));
    }

    #[test]
    fn every_builtin_still_renders_its_markers() {
        // The guard above must not catch anything else: each built-in has to
        // keep producing a header that starts with its own opener.
        for (style, opener) in [
            (CommentStyle::Slash, "//"),
            (CommentStyle::Hash, "#"),
            (CommentStyle::Html, "<!--"),
            (CommentStyle::SlashStar, "/*"),
            (CommentStyle::AsteriskParen, "(*"),
        ] {
            let out = generate_with(
                "2026 Acme",
                "MIT",
                ResolvedStyle::builtin(style),
                &[],
                false,
            )
            .unwrap_or_else(|e| panic!("{style:?} must render: {e}"));
            assert!(out.starts_with(opener), "{style:?} -> {out}");
        }
    }

    /// Byte-for-byte expectations captured from `reuse` 6.2.0 running
    /// `annotate --license MIT --copyright Acme` on a file of each type.
    /// Matching it exactly is the point: a project running both tools must not
    /// see them rewrite each other's headers.
    #[test]
    fn every_style_matches_the_reference_tool_byte_for_byte() {
        let cases = [
            (
                CommentStyle::Slash,
                "// SPDX-FileCopyrightText: 2026 Acme\n//\n// SPDX-License-Identifier: MIT\n",
            ),
            (
                CommentStyle::Hash,
                "# SPDX-FileCopyrightText: 2026 Acme\n#\n# SPDX-License-Identifier: MIT\n",
            ),
            (
                CommentStyle::Dash,
                "-- SPDX-FileCopyrightText: 2026 Acme\n--\n-- SPDX-License-Identifier: MIT\n",
            ),
            (
                CommentStyle::Semicolon,
                ";;; SPDX-FileCopyrightText: 2026 Acme\n;;;\n;;; SPDX-License-Identifier: MIT\n",
            ),
            (
                CommentStyle::Percent,
                "% SPDX-FileCopyrightText: 2026 Acme\n%\n% SPDX-License-Identifier: MIT\n",
            ),
            (
                CommentStyle::AsteriskParen,
                "(*\n * SPDX-FileCopyrightText: 2026 Acme\n *\n * SPDX-License-Identifier: MIT\n *)\n",
            ),
            (
                CommentStyle::SlashStar,
                "/*\n * SPDX-FileCopyrightText: 2026 Acme\n *\n * SPDX-License-Identifier: MIT\n */\n",
            ),
            (
                CommentStyle::DotDot,
                ".. SPDX-FileCopyrightText: 2026 Acme\n..\n.. SPDX-License-Identifier: MIT\n",
            ),
            (
                CommentStyle::Html,
                "<!--\nSPDX-FileCopyrightText: 2026 Acme\n\nSPDX-License-Identifier: MIT\n-->\n",
            ),
        ];

        for (style, expected) in cases {
            let result = generate("2026 Acme", "MIT", style).unwrap();
            assert_eq!(result, expected, "for {style:?}");
        }
    }

    /// A `.license` companion is the one thing `Uncommentable` renders, and
    /// `reuse` 6.2.0 writes its lines bare. An empty opener still contributes
    /// the lead's trailing space, so every line would start with one.
    #[test]
    fn an_uncommentable_style_renders_the_bare_companion_form() {
        let out = generate("2026 Acme", "MIT", CommentStyle::Uncommentable).unwrap();

        assert_eq!(
            out,
            "SPDX-FileCopyrightText: 2026 Acme\n\nSPDX-License-Identifier: MIT\n"
        );
    }

    #[test]
    fn generate_invalid_expression_fails() {
        let result = generate(
            "2026 Acme Inc",
            "NOT A VALID EXPRESSION",
            CommentStyle::Slash,
        );
        assert!(result.is_err());
    }

    #[test]
    fn generate_with_contributors() {
        let result = generate_with(
            "2026 Acme Inc",
            "MIT",
            ResolvedStyle::builtin(CommentStyle::Slash),
            &["John Doe".to_string(), "Jane Doe".to_string()],
            false,
        )
        .unwrap();
        assert!(result.contains("// SPDX-FileCopyrightText: 2026 Acme Inc"));
        assert!(result.contains("// SPDX-FileContributor: John Doe"));
        assert!(result.contains("// SPDX-FileContributor: Jane Doe"));
        assert!(result.contains("// SPDX-License-Identifier: MIT"));
    }

    #[test]
    fn contributors_sit_above_the_separator() {
        let result = generate_with(
            "2026 Acme",
            "MIT",
            ResolvedStyle::builtin(CommentStyle::Html),
            &["John Doe".to_string()],
            false,
        )
        .unwrap();

        assert_eq!(
            result,
            "<!--\n\
             SPDX-FileCopyrightText: 2026 Acme\n\
             SPDX-FileContributor: John Doe\n\
             \n\
             SPDX-License-Identifier: MIT\n\
             -->\n"
        );
    }

    /// Generation and detection are separate tables; nothing in the type system
    /// keeps them in step, so this asserts that every style comply can WRITE it
    /// can also READ back.
    #[test]
    fn every_generated_header_can_be_read_back() {
        for style in [
            CommentStyle::Slash,
            CommentStyle::Hash,
            CommentStyle::Dash,
            CommentStyle::Semicolon,
            CommentStyle::Percent,
            CommentStyle::AsteriskParen,
            CommentStyle::SlashStar,
            CommentStyle::DotDot,
            CommentStyle::Html,
        ] {
            let written = generate("2026 Acme Inc", "MIT", style).unwrap();
            let read = super::super::detect(&written)
                .unwrap_or_else(|| panic!("{style:?} produced an unreadable header:\n{written}"));

            assert_eq!(read.copyrights, ["2026 Acme Inc"], "{style:?}");
            assert_eq!(read.license_expression.as_deref(), Some("MIT"), "{style:?}");
        }
    }

    #[test]
    fn generate_with_exclude_year() {
        let result = generate_with(
            "2026 Acme Inc",
            "MIT",
            ResolvedStyle::builtin(CommentStyle::Slash),
            &[],
            true,
        )
        .unwrap();
        assert!(result.contains("// SPDX-FileCopyrightText: Acme Inc"));
        assert!(!result.contains("2026 Acme Inc"));
    }
}
// REUSE-IgnoreEnd
