// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Write;

use super::CommentStyle;
use super::parse::YEAR_PREFIX_RE;
use super::style::comment_delimiters;
use crate::error::Error;

/// Generate an SPDX header block for a file.
#[allow(clippy::missing_errors_doc)]
pub fn generate(
    copyright: &str,
    license_expression: &str,
    style: CommentStyle,
) -> Result<String, Error> {
    generate_with(copyright, license_expression, style, &[], false)
}

/// Generate an SPDX header block with optional contributors.
#[allow(clippy::missing_errors_doc)]
pub fn generate_with(
    copyright: &str,
    license_expression: &str,
    style: CommentStyle,
    contributors: &[String],
    exclude_year: bool,
) -> Result<String, Error> {
    crate::spdx::parse(license_expression)?;

    let copyright_text = if exclude_year {
        YEAR_PREFIX_RE.replace(copyright, "").to_string()
    } else {
        copyright.to_string()
    };

    let (prefix, suffix) = comment_delimiters(style);

    // The styles differ only in what leads each line, and whether the delimiter
    // sits on its own line. Block styles (`<!--`, `(*`, `/*`) open and close on
    // their own lines; line styles repeat the prefix on every line.
    let (lead, is_block) = match style {
        CommentStyle::Html => (String::new(), true),
        CommentStyle::AsteriskParen | CommentStyle::SlashStar => (" * ".to_string(), true),
        _ => (format!("{prefix} "), false),
    };

    let mut out = String::new();
    if is_block {
        let _ = writeln!(out, "{prefix}");
    }
    // REUSE-IgnoreStart
    let _ = writeln!(out, "{lead}SPDX-FileCopyrightText: {copyright_text}");
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
            CommentStyle::Slash,
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
            CommentStyle::Html,
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
        let result = generate_with("2026 Acme Inc", "MIT", CommentStyle::Slash, &[], true).unwrap();
        assert!(result.contains("// SPDX-FileCopyrightText: Acme Inc"));
        assert!(!result.contains("2026 Acme Inc"));
    }
}
// REUSE-IgnoreEnd
