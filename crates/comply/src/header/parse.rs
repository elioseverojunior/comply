// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::LazyLock;

use regex::Regex;

use super::{CommentStyle, Header};

/// Matches a copyright tag, whether it covers the file or a snippet within it.
///
/// REUSE 3.3 requires a snippet's notices to be prefixed
/// `SPDX-SnippetCopyrightText`, and the reference tool counts them towards the
/// file's own copyright rather than scoping them to the snippet's region.
static COPYRIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"SPDX-(?:File|Snippet)CopyrightText:\s*(.*)").expect("invalid copyright regex")
});

// REUSE-IgnoreStart
/// Regex pattern to match `SPDX-FileContributor:` lines.
static CONTRIBUTOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"SPDX-FileContributor:\s*(.*)").expect("invalid contributor regex")
});
// REUSE-IgnoreEnd

/// Regex pattern to strip year prefix from copyright strings.
pub(super) static YEAR_PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"^\d{4}(?:[-,]\s*\d{4}|\s*-\s*Present)*\s+").expect("invalid year prefix regex")
});

// REUSE-IgnoreStart
/// Regex pattern to match `SPDX-License-Identifier:` lines.
///
/// The capture excludes `"` deliberately. No SPDX expression contains one, and
/// running to end-of-line swallowed the closing delimiter of a TOML multi-line
/// string that merely mentioned the tag -- `Apache-2.0"""` then parsed as an
/// identifier and propagated into every file the tool rewrote afterwards.
pub(super) static LICENSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r#"SPDX-License-Identifier:\s*([^"]+)"#).expect("invalid license regex")
});
// REUSE-IgnoreEnd

/// Whether a line is a Copyright Notice in one of the non-SPDX forms REUSE
/// permits (spec 3.3 section 2.1.1).
///
/// A bare `(C)` does not qualify -- the word `Copyright` or the `\u{a9}` symbol
/// has to lead -- which matches the reference implementation.
pub(super) fn is_plain_copyright_notice(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('\u{a9}') || trimmed.starts_with("Copyright")
}

/// Detect the SPDX licensing declared anywhere in `content`, respecting the
/// span markers `REUSE-` `IgnoreStart` and `REUSE-` `IgnoreEnd`.
///
/// The whole file is read. REUSE puts no line limit on where a declaration may
/// appear, and a window would silently miss one -- reporting a covered file as
/// unlicensed. Callers that care about the header's *position* (rather than its
/// existence) locate the block themselves.
pub fn detect(content: &str) -> Option<Header> {
    let mut copyrights = Vec::new();
    let mut licenses: Vec<String> = Vec::new();
    let mut contributors = Vec::new();
    let mut ignore = false;
    let mut fenced = false;

    for line in content.lines() {
        if is_fence_delimiter(line) {
            fenced = !fenced;
            continue;
        }
        // Everything inside a fence is inert, span markers included: a project
        // documenting `REUSE-` `IgnoreStart` in an example must not have that
        // example switch the real scanner on or off.
        if fenced {
            continue;
        }

        let stripped = strip_comment_prefix(line);

        // Handle start / end span markers
        if stripped.contains(concat!("REUSE", "-IgnoreStart")) {
            ignore = true;
            continue;
        }
        if stripped.contains(concat!("REUSE", "-IgnoreEnd")) {
            ignore = false;
            continue;
        }

        if ignore {
            continue;
        }

        if let Some(caps) = COPYRIGHT_RE.captures(&stripped) {
            let text = caps.get(1).map_or("", |m| m.as_str()).trim();
            if !text.is_empty() {
                copyrights.push(text.to_string());
            }
        } else if is_plain_copyright_notice(&stripped) {
            copyrights.push(stripped.trim().to_string());
        }
        // Every declaration applies, not just the last one: a file carrying
        // both its own licence and a snippet's is under both.
        if let Some(caps) = LICENSE_RE.captures(&stripped) {
            let expr = caps.get(1).map_or("", |m| m.as_str()).trim();
            if !expr.is_empty() && !licenses.iter().any(|l| l == expr) {
                licenses.push(expr.to_string());
            }
        }
        if let Some(caps) = CONTRIBUTOR_RE.captures(&stripped) {
            let text = caps.get(1).map_or("", |m| m.as_str()).trim();
            if !text.is_empty() {
                contributors.push(text.to_string());
            }
        }
    }

    if copyrights.is_empty() && licenses.is_empty() && contributors.is_empty() {
        return None;
    }

    // Several declarations mean the file is under all of them, which is what
    // `AND` says in an SPDX expression -- but only the ones that actually parse
    // are joined. A file whose body happens to contain the tag text (a test
    // fixture, a documentation sample) would otherwise drag that fragment into
    // the expression and make the whole file unlicensed, losing the real header
    // it carries. The reference tool reports such a fragment on its own and
    // still knows the file's licence; keeping them apart is what allows that.
    let (valid, invalid_expressions): (Vec<String>, Vec<String>) = licenses
        .into_iter()
        .partition(|expr| crate::spdx::validate(expr).is_ok());
    let license_expression = (!valid.is_empty()).then(|| valid.join(" AND "));

    Some(Header {
        copyrights,
        license_expression,
        invalid_expressions,
        contributors,
    })
}

// REUSE-IgnoreStart

/// Detect comment style from the existing header in the content.
pub(super) fn detect_style_from_header(content: &str) -> CommentStyle {
    // First check if content is wrapped in HTML comments (<!-- ... -->)
    if is_html_wrapped(content) {
        return CommentStyle::Html;
    }

    // Check the first non-empty, non-SPDX line to determine comment style
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(style) = detect_comment_style_from_spdx_line(trimmed) {
            return style;
        }
    }

    // Default fallback: check shebang
    detect_style_from_shebang(content)
}

/// Check if content is wrapped in HTML comments (<!-- ... -->).
fn is_html_wrapped(content: &str) -> bool {
    let trimmed = content.trim();
    trimmed.starts_with("<!--") && trimmed.contains("-->")
}

/// Try to detect comment style from an SPDX line.
fn detect_comment_style_from_spdx_line(line: &str) -> Option<CommentStyle> {
    let trimmed = line.trim();
    let stripped = strip_comment_prefix(trimmed);

    if stripped.starts_with("SPDX-") {
        if trimmed.starts_with("//") {
            Some(CommentStyle::Slash)
        } else if trimmed.starts_with('#') {
            Some(CommentStyle::Hash)
        } else if trimmed.starts_with("--") {
            Some(CommentStyle::Dash)
        } else if trimmed.starts_with(';') {
            Some(CommentStyle::Semicolon)
        } else if trimmed.starts_with('%') {
            Some(CommentStyle::Percent)
        } else if trimmed.starts_with("..") {
            Some(CommentStyle::DotDot)
        } else if trimmed.starts_with("/*") || trimmed.starts_with('*') {
            // Either the opening delimiter or a continuation line of a
            // `/* ... */` block, whose lines lead with `*`.
            Some(CommentStyle::SlashStar)
        } else if trimmed.starts_with("(*") {
            Some(CommentStyle::AsteriskParen)
        } else if trimmed.starts_with("<!--") {
            Some(CommentStyle::Html)
        } else {
            None
        }
    } else {
        None
    }
}

/// Detect comment style from shebang line.
const fn detect_style_from_shebang(_content: &str) -> CommentStyle {
    CommentStyle::Hash
}

/// Detect comment style from file content (shebang, modelines, etc.)
pub(super) const fn detect_style_from_content(content: &str) -> CommentStyle {
    detect_style_from_shebang(content)
}

/// Whether a line opens or closes a Markdown fenced block.
///
/// The raw line is tested rather than the comment-stripped one. A Rust doc
/// example opens with ``/// ``` ``, which is prose inside a comment, not a
/// fence -- so this rule stays confined to formats that actually use fences.
fn is_fence_delimiter(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// Every comment marker the built-in styles use, longest first so a marker that
/// extends another is reported whole (`;;;` over `;`, `<!--` over nothing).
const COMMENT_MARKERS: [&str; 14] = [
    "<!--", ";;;", "REM", ";;", "//", "/*", "(*", "--", "..", "#", ";", "%", "*", "!",
];

/// Markers that lead a comment only when a space or the line end follows.
///
/// `!` opens every Markdown image and `REM` prefixes identifiers such as
/// `REMOTE`, so a bare prefix test swallows `!important` and `REMOTE_URL=`.
/// The generator only ever writes these two followed by a space or nothing.
fn needs_a_delimiter(marker: &str) -> bool {
    matches!(marker, "!" | "REM")
}

/// Whether `trimmed` opens with `marker` used as a comment.
fn marker_leads(trimmed: &str, marker: &str) -> bool {
    trimmed
        .strip_prefix(marker)
        .is_some_and(|rest| !needs_a_delimiter(marker) || rest.is_empty() || rest.starts_with(' '))
}

/// The comment marker `line` opens with, if any.
///
/// This is what scopes a header span to the syntax the header is actually
/// written in. Judging a line by the union of every marker instead reads
/// `#include` in a C file, `%matplotlib` in a Python file and a Markdown
/// `* bullet` as header filler, and `--force` replaces a span wholesale.
pub(super) fn leading_comment_marker(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    COMMENT_MARKERS
        .iter()
        .copied()
        .find(|marker| marker_leads(trimmed, marker))
}

/// Check if a line is a comment line, in any syntax comply knows.
///
/// Deliberately permissive: `comment_mask` uses it to tell a header from bare
/// data, and a file some other tool annotated carries whichever syntax that
/// tool chose. Narrowing this would make comply blind to headers it currently
/// reads. Deciding how far a header *extends* is the scoped question, and
/// [`is_comment_line_in`] answers that one.
pub(super) fn is_comment_line(line: &str) -> bool {
    leading_comment_marker(line).is_some()
}

/// Whether `line` is a comment in the syntax `markers` names, and no other.
pub(super) fn is_comment_line_in(line: &str, markers: &[&str]) -> bool {
    let trimmed = line.trim();
    markers.iter().any(|marker| marker_leads(trimmed, marker))
}
pub(super) fn strip_comment_prefix(line: &str) -> String {
    let trimmed = line.trim();
    // Longest first: `;;; ` must win over `; `, or a Lisp file-level comment
    // strips to `;; SPDX-...` and stops looking like a header at all.
    let patterns = [
        "// ", "//", "# ", "#", "-- ", "--", ";;; ", ";;;", ";; ", ";;", "; ", ";", "% ", "%",
        ".. ", "..", "* ", "*", "! ", "!", "REM ", "REM",
    ];

    for pat in &patterns {
        if let Some(stripped) = trimmed.strip_prefix(pat) {
            return stripped.trim().to_string();
        }
    }

    // Block comments, whose closing delimiter would otherwise be parsed as part
    // of the licence expression on a single-line header.
    for (open, close) in [("<!--", "-->"), ("(*", "*)"), ("/*", "*/")] {
        if let Some(stripped) = trimmed.strip_prefix(open) {
            let s = stripped.strip_suffix(close).unwrap_or(stripped);
            return s.trim().to_string();
        }
    }

    trimmed.to_string()
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    /// REUSE 3.3 permits a plain notice as well as the SPDX tag. Accepted
    /// forms verified against `reuse` 6.2.0.
    #[test]
    fn a_plain_copyright_notice_is_recognised() {
        for notice in [
            "Copyright 2026 Acme",
            "Copyright (C) 2026 Acme",
            "Copyright (c) 2026 Acme",
            "Copyright \u{a9} 2026 Acme",
            "\u{a9} 2026 Acme",
        ] {
            let header = detect(&format!("# {notice}\n# SPDX-License-Identifier: MIT\n"))
                .unwrap_or_else(|| panic!("{notice:?} should be detected"));

            assert_eq!(header.copyrights, [notice], "for {notice:?}");
        }
    }

    #[test]
    fn an_identifier_stops_at_a_quote_rather_than_running_to_end_of_line() {
        // A TOML multi-line string that mentions the tag closes on the same
        // line: `SPDX-License-Identifier: MIT OR Apache-2.0"""`. Capturing to
        // end-of-line swallowed the delimiter, `Apache-2.0"""` parsed as an
        // identifier, and every later rewrite carried it forward.
        let header = detect("# SPDX-License-Identifier: MIT OR Apache-2.0\"\"\"\n")
            .expect("the licence line still forms a header");

        assert_eq!(
            header.license_expression.as_deref(),
            Some("MIT OR Apache-2.0")
        );
    }

    #[test]
    fn a_bare_parenthesised_c_is_not_a_copyright_notice() {
        let header = detect("# (C) 2026 Acme\n# SPDX-License-Identifier: MIT\n")
            .expect("the licence line still forms a header");

        assert!(
            header.copyrights.is_empty(),
            "the reference tool does not accept a bare `(C)`"
        );
    }

    /// The spec puts no line limit on where a declaration may appear, and the
    /// reference tool reads the whole file. A window would silently miss a
    /// licence -- reporting a covered file as unlicensed.
    #[test]
    fn a_declaration_far_down_the_file_is_still_found() {
        let mut content = "// filler\n".repeat(200);
        content.push_str("// SPDX-FileCopyrightText: 2026 Acme\n");
        content.push_str("//\n");
        content.push_str("// SPDX-License-Identifier: MIT\n");

        let header = detect(&content).expect("a tag anywhere in the file is a declaration");

        assert_eq!(header.copyrights, ["2026 Acme"]);
        assert_eq!(header.license_expression.as_deref(), Some("MIT"));
    }

    #[test]
    fn an_ignore_block_still_hides_a_tag_far_down_the_file() {
        let mut content = "// filler\n".repeat(200);
        // Split so this file's own text never carries a contiguous marker --
        // an `IgnoreEnd` written literally here would close the block this test
        // lives inside, exposing every SPDX literal below it.
        content.push_str(concat!("// REUSE", "-IgnoreStart\n"));
        content.push_str("// SPDX-License-Identifier: MIT\n");
        content.push_str(concat!("// REUSE", "-IgnoreEnd\n"));

        assert!(
            detect(&content).is_none(),
            "reading further must not defeat the ignore markers"
        );
    }

    /// REUSE 3.3 sections 2.1.3/2.1.4. The reference tool does not scope a
    /// snippet to a region: its copyright counts as the file's, and its licence
    /// joins the file's own. Expectations verified against `reuse` 6.2.0.
    mod snippets {
        use super::*;

        #[test]
        fn a_snippet_copyright_counts_as_the_files_copyright() {
            let content = concat!(
                "# SPDX-SnippetBegin\n",
                "# SPDX-SnippetCopyrightText: 2026 Snip\n",
                "# SPDX-License-Identifier: MIT\n",
                "# SPDX-SnippetEnd\n",
                "x = 1\n"
            );

            let header = detect(content).expect("a snippet declares licensing too");

            assert_eq!(header.copyrights, ["2026 Snip"]);
            assert_eq!(header.license_expression.as_deref(), Some("MIT"));
        }

        #[test]
        fn a_file_header_and_a_snippet_both_apply() {
            let content = concat!(
                "# SPDX-FileCopyrightText: 2026 File\n",
                "#\n",
                "# SPDX-License-Identifier: Apache-2.0\n",
                "\n",
                "# SPDX-SnippetBegin\n",
                "# SPDX-SnippetCopyrightText: 2026 Snip\n",
                "# SPDX-License-Identifier: GPL-3.0-only\n",
                "# SPDX-SnippetEnd\n",
                "x = 1\n"
            );

            let header = detect(content).unwrap();

            assert_eq!(header.copyrights, ["2026 File", "2026 Snip"]);
            assert_eq!(
                header.license_expression.as_deref(),
                Some("Apache-2.0 AND GPL-3.0-only"),
                "a file under both licences is the conjunction of them"
            );
        }

        #[test]
        fn the_same_licence_declared_twice_is_named_once() {
            let content = concat!(
                "# SPDX-FileCopyrightText: 2026 A\n",
                "# SPDX-License-Identifier: MIT\n",
                "\n",
                "# SPDX-SnippetBegin\n",
                "# SPDX-License-Identifier: MIT\n",
                "# SPDX-SnippetEnd\n",
                "x = 1\n"
            );

            let header = detect(content).unwrap();

            assert_eq!(header.license_expression.as_deref(), Some("MIT"));
        }

        #[test]
        fn the_snippet_markers_themselves_declare_nothing() {
            let content = "# SPDX-SnippetBegin\n# SPDX-SnippetEnd\nx = 1\n";

            assert!(
                detect(content).is_none(),
                "opening a snippet is not a licensing declaration"
            );
        }
    }

    /// A fenced block quotes an example; a tag inside one is being shown, not
    /// declared. This is a deliberate divergence -- `reuse` 6.2.0 reads them as
    /// real declarations, which is what lets `format` rewrite a project's own
    /// documentation into a licence header.
    mod fenced_blocks {
        use super::*;

        #[test]
        fn a_tag_inside_a_fence_declares_nothing() {
            let content = concat!(
                "# Docs\n",
                "\n",
                "```\n",
                "# SPDX-FileCopyrightText: 2026 Example\n",
                "# SPDX-License-Identifier: Apache-2.0\n",
                "```\n"
            );

            assert!(detect(content).is_none(), "the example is quoted, not used");
        }

        #[test]
        fn a_real_header_outside_the_fence_still_counts() {
            let content = concat!(
                "<!--\n",
                "SPDX-FileCopyrightText: 2026 Real\n",
                "\n",
                "SPDX-License-Identifier: MIT\n",
                "-->\n",
                "\n",
                "```toml\n",
                "SPDX-License-Identifier = \"Apache-2.0\"\n",
                "```\n"
            );

            let header = detect(content).expect("the file's own header is outside the fence");

            assert_eq!(header.copyrights, ["2026 Real"]);
            assert_eq!(
                header.license_expression.as_deref(),
                Some("MIT"),
                "the fenced Apache-2.0 is an example, not a second licence"
            );
        }

        #[test]
        fn a_tilde_fence_works_too() {
            let content = "~~~\n# SPDX-License-Identifier: Apache-2.0\n~~~\n";

            assert!(detect(content).is_none());
        }

        #[test]
        fn declarations_resume_after_the_fence_closes() {
            let content = concat!(
                "```\n",
                "# SPDX-License-Identifier: Apache-2.0\n",
                "```\n",
                "\n",
                "<!-- SPDX-FileCopyrightText: 2026 Real -->\n",
                "<!-- SPDX-License-Identifier: MIT -->\n"
            );

            let header = detect(content).expect("the tags below the fence are real");

            assert_eq!(header.license_expression.as_deref(), Some("MIT"));
        }

        #[test]
        fn a_fence_inside_a_comment_is_not_a_fence() {
            // Rust doc examples open with `/// ``` `, which is prose in a
            // comment rather than a Markdown fence -- .rs behaviour is unchanged.
            let content = concat!(
                "/// ```\n",
                "/// SPDX-License-Identifier: MIT\n",
                "/// ```\n",
                "// SPDX-FileCopyrightText: 2026 Real\n"
            );

            let header = detect(content).expect("nothing here is fenced");

            assert_eq!(header.license_expression.as_deref(), Some("MIT"));
        }
    }

    #[test]
    fn a_copyright_tag_with_no_text_is_not_recorded() {
        let header = detect("// SPDX-FileCopyrightText:\n// SPDX-License-Identifier: MIT\n")
            .expect("the licence alone still makes a header");

        assert!(
            header.copyrights.is_empty(),
            "a bare tag declares nobody, got: {:?}",
            header.copyrights
        );
    }

    #[test]
    fn a_contributor_tag_with_no_text_is_not_recorded() {
        let header = detect("// SPDX-FileContributor:\n// SPDX-License-Identifier: MIT\n")
            .expect("the licence alone still makes a header");

        assert!(header.contributors.is_empty());
    }

    #[test]
    fn style_detection_steps_over_blank_and_non_spdx_lines() {
        let content = "\n\nfn main() {}\n// SPDX-License-Identifier: MIT\n";

        assert_eq!(detect_style_from_header(content), CommentStyle::Slash);
    }

    #[test]
    fn style_detection_covers_every_line_comment_syntax() {
        for (content, expected) in [
            ("-- SPDX-License-Identifier: MIT\n", CommentStyle::Dash),
            ("; SPDX-License-Identifier: MIT\n", CommentStyle::Semicolon),
            ("% SPDX-License-Identifier: MIT\n", CommentStyle::Percent),
        ] {
            assert_eq!(
                detect_style_from_header(content),
                expected,
                "for {content:?}"
            );
        }
    }

    #[test]
    fn an_ml_style_block_comment_is_detected() {
        assert_eq!(
            detect_style_from_header("(* SPDX-License-Identifier: MIT *)\n"),
            CommentStyle::AsteriskParen
        );
    }

    #[test]
    fn an_ml_style_block_comment_does_not_leak_its_closing_delimiter() {
        let header = detect("(* SPDX-License-Identifier: MIT *)\n")
            .expect("the licence line should be recognised");

        assert_eq!(
            header.license_expression.as_deref(),
            Some("MIT"),
            "`*)` must be stripped, not parsed as part of the expression"
        );
    }

    #[test]
    fn a_stylesheet_block_comment_is_detected() {
        let content = concat!(
            "/*\n",
            " * SPDX-FileCopyrightText: 2026 Acme\n",
            " * SPDX-License-Identifier: MIT\n",
            "*/\n",
            "\n",
            "body {}\n"
        );

        let header = detect(content).expect("a stylesheet header should be detected");

        assert_eq!(header.copyrights, ["2026 Acme"]);
        assert_eq!(header.license_expression.as_deref(), Some("MIT"));
        assert_eq!(detect_style_from_header(content), CommentStyle::SlashStar);
    }

    #[test]
    fn a_restructuredtext_comment_is_detected() {
        let content = concat!(
            ".. SPDX-FileCopyrightText: 2026 Acme\n",
            ".. SPDX-License-Identifier: MIT\n",
            "\n",
            "Title\n"
        );

        let header = detect(content).expect("an rst header should be detected");

        assert_eq!(header.copyrights, ["2026 Acme"]);
        assert_eq!(header.license_expression.as_deref(), Some("MIT"));
        assert_eq!(detect_style_from_header(content), CommentStyle::DotDot);
    }

    #[test]
    fn a_single_line_stylesheet_comment_does_not_leak_its_closing_delimiter() {
        let header = detect("/* SPDX-License-Identifier: MIT */\n")
            .expect("the licence line should be recognised");

        assert_eq!(
            header.license_expression.as_deref(),
            Some("MIT"),
            "`*/` must be stripped, not parsed as part of the expression"
        );
    }

    #[test]
    fn an_unclosed_html_comment_is_still_detected_per_line() {
        // `is_html_wrapped` needs a closing `-->`; without one it is the
        // per-line check that has to recognise the style.
        assert_eq!(
            detect_style_from_header("<!-- SPDX-License-Identifier: MIT\n"),
            CommentStyle::Html
        );
    }

    #[test]
    fn an_spdx_line_with_no_comment_marker_falls_back_to_hash() {
        assert_eq!(
            detect_style_from_header("SPDX-License-Identifier: MIT\n"),
            CommentStyle::Hash
        );
    }

    #[test]
    fn detect_empty_content() {
        assert!(detect("").is_none());
    }

    #[test]
    fn detect_no_header() {
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        assert!(detect(content).is_none());
    }

    #[test]
    fn detect_full_header_comment_style() {
        let content = "// SPDX-FileCopyrightText: 2026 Acme Inc\n\
                        // SPDX-License-Identifier: MIT\n\
                        \n\
                        fn main() {}";
        let header = detect(content).unwrap();
        assert_eq!(header.copyrights, vec!["2026 Acme Inc"]);
        assert_eq!(header.license_expression, Some("MIT".into()));
    }

    #[test]
    fn detect_full_header_hash_style() {
        let content = "# SPDX-FileCopyrightText: 2026 Acme Inc\n\
                        # SPDX-License-Identifier: Apache-2.0\n\
                        \n\
                        import os";
        let header = detect(content).unwrap();
        assert_eq!(header.copyrights, vec!["2026 Acme Inc"]);
        assert_eq!(header.license_expression, Some("Apache-2.0".into()));
    }

    #[test]
    fn detect_header_html_style() {
        let content = "<!--\n\
                        SPDX-FileCopyrightText: 2026 Acme Inc\n\
                        SPDX-License-Identifier: MIT\n\
                        -->\n\
                        <html>";
        let header = detect(content).unwrap();
        assert_eq!(header.copyrights, vec!["2026 Acme Inc"]);
        assert_eq!(header.license_expression, Some("MIT".into()));
    }

    #[test]
    fn detect_multiple_copyrights() {
        let content = "# SPDX-FileCopyrightText: 2026 Acme Inc\n\
                        # SPDX-FileCopyrightText: 2026 Bob Corp\n\
                        # SPDX-License-Identifier: MIT OR Apache-2.0\n";
        let header = detect(content).unwrap();
        assert_eq!(header.copyrights, vec!["2026 Acme Inc", "2026 Bob Corp"]);
        assert_eq!(header.license_expression, Some("MIT OR Apache-2.0".into()));
    }

    #[test]
    fn detect_copyright_only() {
        let content = "// SPDX-FileCopyrightText: 2026 Acme Inc\n";
        let header = detect(content).unwrap();
        assert_eq!(header.copyrights, vec!["2026 Acme Inc"]);
        assert!(header.license_expression.is_none());
    }

    #[test]
    fn detect_license_only() {
        let content = "// SPDX-License-Identifier: MIT\n";
        let header = detect(content).unwrap();
        assert!(header.copyrights.is_empty());
        assert_eq!(header.license_expression, Some("MIT".into()));
    }

    #[test]
    fn a_declaration_is_found_regardless_of_how_deep_it_sits() {
        let mut content = "// some comment\n".repeat(64);
        content.push_str("// SPDX-FileCopyrightText: 2026 Acme Inc\n");

        let header = detect(&content).expect("depth does not make a declaration invisible");

        assert_eq!(header.copyrights, ["2026 Acme Inc"]);
    }

    #[test]
    fn detect_contributor() {
        let content = "// SPDX-FileCopyrightText: 2026 Acme Inc\n\
                        // SPDX-FileContributor: John Doe\n\
                        // SPDX-License-Identifier: MIT\n";
        let header = detect(content).unwrap();
        assert_eq!(header.contributors, vec!["John Doe"]);
    }

    #[test]
    fn strip_slash_comment() {
        assert_eq!(
            strip_comment_prefix("// SPDX-FileCopyrightText: 2026 Acme Inc"),
            "SPDX-FileCopyrightText: 2026 Acme Inc"
        );
    }

    #[test]
    fn strip_hash_comment() {
        assert_eq!(
            strip_comment_prefix("# SPDX-License-Identifier: MIT"),
            "SPDX-License-Identifier: MIT"
        );
    }

    #[test]
    fn strip_html_comment() {
        assert_eq!(
            strip_comment_prefix("<!-- SPDX-FileCopyrightText: 2026 Acme Inc -->"),
            "SPDX-FileCopyrightText: 2026 Acme Inc"
        );
    }

    #[test]
    fn strip_dash_comment() {
        assert_eq!(
            strip_comment_prefix("-- SPDX-License-Identifier: MIT"),
            "SPDX-License-Identifier: MIT"
        );
    }

    #[test]
    fn strip_no_comment() {
        assert_eq!(
            strip_comment_prefix("SPDX-FileCopyrightText: 2026 Acme Inc"),
            "SPDX-FileCopyrightText: 2026 Acme Inc"
        );
    }

    #[test]
    fn strip_bang_comment() {
        assert_eq!(
            strip_comment_prefix("! SPDX-FileCopyrightText: 2026 Acme Inc"),
            "SPDX-FileCopyrightText: 2026 Acme Inc"
        );
    }

    #[test]
    fn strip_rem_comment() {
        assert_eq!(
            strip_comment_prefix("REM SPDX-License-Identifier: MIT"),
            "SPDX-License-Identifier: MIT"
        );
    }

    #[test]
    fn is_comment_line_recognises_bang_and_rem() {
        assert!(is_comment_line("! a Fortran comment"));
        assert!(is_comment_line("REM a batch comment"));
        assert!(!is_comment_line("x = 1"));
    }

    /// This predicate is not scoped to a file type: `header_span` asks it which
    /// lines may sit between two SPDX tags, whatever the file is, and every
    /// line it covers is replaced by `annotate --force`. A bare `!` or `REM`
    /// therefore reached ordinary source, and the generator only ever writes
    /// the two followed by a space or the end of the line.
    #[test]
    fn bang_and_rem_lead_a_comment_only_before_a_space_or_a_line_end() {
        for line in ["!", "! ", "! SPDX-License-Identifier: MIT", "REM", "REM x"] {
            assert!(is_comment_line(line), "{line:?}");
        }
        for line in [
            "!important = 1",
            "![badge](https://img.example/badge.svg)",
            "REMOTE_URL=https://example.com",
            "REMOVE = 1",
        ] {
            assert!(!is_comment_line(line), "{line:?}");
        }
    }

    /// The marker a line opens with is what scopes a header span. Reporting a
    /// shorter marker that the real one merely extends would widen the span
    /// back out: `;;;` reported as `;` lets any `;`-leading source line in.
    #[test]
    fn a_lines_marker_is_the_longest_one_it_opens_with() {
        for (line, expected) in [
            (";;; a file-level Lisp comment", Some(";;;")),
            (";; a section comment", Some(";;")),
            ("; an ini comment", Some(";")),
            ("<!-- markup -->", Some("<!--")),
            ("/* a C block */", Some("/*")),
            ("(* an ML block *)", Some("(*")),
            ("  // indented source", Some("//")),
            (".. an rst comment", Some("..")),
            ("REM a batch comment", Some("REM")),
            ("! a Fortran comment", Some("!")),
            ("REMOTE_URL=https://example.com", None),
            ("!important = 1", None),
            ("x = 1", None),
        ] {
            assert_eq!(leading_comment_marker(line), expected, "for {line:?}");
        }
    }

    /// The scoped predicate is what keeps `--force` from deleting a source
    /// line that merely opens with another language's marker.
    #[test]
    fn a_scoped_comment_test_admits_no_other_language() {
        assert!(is_comment_line_in("// a Rust comment", &["//"]));
        assert!(is_comment_line_in("#", &["#"]));
        assert!(is_comment_line_in("@@ a configured comment", &["@@"]));
        assert!(is_comment_line_in("!", &["!"]));

        for (line, markers) in [
            ("#include <stdio.h>", &["//"][..]),
            (";(function(){})()", &["//"][..]),
            ("%matplotlib inline", &["#"][..]),
            ("--flag=value", &["#"][..]),
            ("---", &["#"][..]),
            ("* a bullet item", &["<!--"][..]),
            ("!important = 1", &["!"][..]),
            ("anything at all", &[][..]),
        ] {
            assert!(!is_comment_line_in(line, markers), "for {line:?}");
        }
    }

    /// A header comply writes for these two styles must be readable back, or
    /// a second `annotate --force` run stacks a second header above its own.
    #[test]
    fn a_bang_or_rem_header_round_trips_through_detect() {
        for content in [
            "! SPDX-FileCopyrightText: 2026 Acme\n!\n! SPDX-License-Identifier: MIT\n",
            "REM SPDX-FileCopyrightText: 2026 Acme\nREM\nREM SPDX-License-Identifier: MIT\n",
        ] {
            let header = detect(content).expect("a bang/rem header should be detected");

            assert_eq!(header.copyrights, ["2026 Acme"], "for {content:?}");
            assert_eq!(
                header.license_expression.as_deref(),
                Some("MIT"),
                "for {content:?}"
            );
        }
    }

    fn ign_start() -> String {
        let a = "REUSE";
        let b = "-IgnoreStart";
        format!("{a}{b}")
    }

    fn ign_end() -> String {
        let a = "REUSE";
        let b = "-IgnoreEnd";
        format!("{a}{b}")
    }

    fn ignore_block(text: &str) -> String {
        format!(
            "// {start}\n{text}// {end}\n",
            start = ign_start(),
            text = text,
            end = ign_end()
        )
    }

    #[test]
    fn detect_ignores_content_between_markers() {
        let inner = "// SPDX-FileCopyrightText: 2026 Ignored\n\
                      // SPDX-License-Identifier: GPL-3.0-only\n";
        let content = format!(
            "// SPDX-FileCopyrightText: 2026 Before\n\
             // SPDX-License-Identifier: MIT\n\
             {}\
             // SPDX-FileCopyrightText: 2026 After\n",
            ignore_block(inner)
        );
        let header = detect(&content).unwrap();
        assert!(header.copyrights.contains(&"2026 Before".to_string()));
        assert!(header.copyrights.contains(&"2026 After".to_string()));
        assert!(!header.copyrights.contains(&"2026 Ignored".to_string()));
        assert_eq!(header.license_expression, Some("MIT".to_string()));
    }

    #[test]
    fn detect_no_start_marker() {
        let content = format!(
            "// {end}\n\
             // SPDX-FileCopyrightText: 2026 Author\n\
             // SPDX-License-Identifier: MIT\n",
            end = ign_end()
        );
        let header = detect(&content);
        assert!(header.is_some());
        assert_eq!(header.unwrap().copyrights, vec!["2026 Author"]);
    }

    #[test]
    fn detect_ignore_without_end_ignores_to_eof() {
        let content = format!(
            "// {start}\n\
             // SPDX-FileCopyrightText: 2026 Should Not Appear\n\
             // SPDX-License-Identifier: BSD-3-Clause\n",
            start = ign_start()
        );
        let header = detect(&content);
        assert!(header.is_none());
    }

    #[test]
    fn detect_markers_work_with_hash_comments() {
        let inner = "# SPDX-License-Identifier: Apache-2.0\n";
        let content = format!(
            "# SPDX-FileCopyrightText: 2026 Visible\n\
             # {start}\n\
             {inner}\
             # {end}\n\
             # SPDX-FileCopyrightText: 2026 Also Visible\n",
            start = ign_start(),
            inner = inner,
            end = ign_end()
        );
        let header = detect(&content).unwrap();
        assert_eq!(header.copyrights, vec!["2026 Visible", "2026 Also Visible"]);
        assert!(header.license_expression.is_none());
    }
}
// REUSE-IgnoreEnd
