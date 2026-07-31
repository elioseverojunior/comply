// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::CommentStyle;
use super::generate::generate_with;
use super::parse::{
    detect, detect_style_from_content, detect_style_from_header, is_comment_line,
    is_comment_line_in, is_plain_copyright_notice, leading_comment_marker, strip_comment_prefix,
};
use super::prefix::CopyrightNotice;
use super::style::{ResolvedStyle, block_delimiters, configured_prefixes};
use crate::config::CommentStyleRule;
use crate::error::Error;

/// Annotate a file with an SPDX header.
///
/// If the file already has an SPDX header and `force` is false, the existing header is kept.
/// If `force` is true, any existing SPDX header is replaced.
/// If the file has no header, a new one is prepended.
///
/// Returns an error if the file is uncommentable (binary).
#[allow(clippy::missing_errors_doc)]
pub fn annotate_file<'a>(
    content: &str,
    copyright: impl Into<CopyrightNotice<'a>>,
    license_expression: &str,
    style_override: Option<ResolvedStyle<'_>>,
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
///
/// `copyright` takes a bare `&str` for the default `SPDX-` `FileCopyrightText`
/// form, or a [`CopyrightNotice`] to choose one of the other fifteen.
#[allow(clippy::missing_errors_doc)]
pub fn annotate_file_with<'a>(
    content: &str,
    copyright: impl Into<CopyrightNotice<'a>>,
    license_expression: &str,
    style_override: Option<ResolvedStyle<'_>>,
    force: bool,
    contributors: &[String],
    exclude_year: bool,
) -> Result<String, Error> {
    crate::spdx::parse(license_expression)?;

    let notice = copyright.into();

    let style = style_override
        .unwrap_or_else(|| ResolvedStyle::builtin(detect_style_from_content(content)));

    if style.style() == CommentStyle::Uncommentable {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "File has no known comment syntax; declare one in \
             [[tool.comply.comment-styles]] or use a .license companion",
        )));
    }

    // Not `detect`: that stays permissive so `lint` can still read a bare-tag
    // `.license` companion and still flag stray tags. What matters here is
    // whether there is a header this may *rewrite*, which is the comment-block
    // rule `header_span` applies. Otherwise a TOML template whose value happens
    // to hold SPDX tags counted as a header, and the file was skipped as
    // "already annotated" when it needs a real one on top.
    let lines: Vec<&str> = content.lines().collect();
    let (span_start, span_end) = header_span(&lines, style.rules());

    if span_start != span_end {
        if !force {
            return Ok(content.to_string());
        }
        return replace_header_with(
            content,
            notice,
            license_expression,
            style,
            contributors,
            exclude_year,
        );
    }

    generate_with(
        notice,
        license_expression,
        style,
        contributors,
        exclude_year,
    )
    .map(|header| insert_header(content, &header))
}

/// Number of leading lines a new header must go below.
///
/// A shebang is only a shebang on line 1, an XML declaration is only a
/// declaration on line 1, and YAML frontmatter is only frontmatter when `---`
/// opens the file. Prepending above any of them does not annotate the file, it
/// breaks it: the kernel stops seeing the interpreter, XML rejects the document
/// outright, and the frontmatter degrades into a setext heading that a Markdown
/// autoformatter will then rewrite in place.
fn preamble_lines(lines: &[&str]) -> usize {
    if lines
        .first()
        .is_some_and(|l| l.starts_with("#!") || l.starts_with("<?xml"))
    {
        return 1;
    }

    // A closing delimiter is what separates frontmatter from a document that
    // merely opens with a horizontal rule.
    if lines.first().is_some_and(|l| l.trim_end() == "---") {
        if let Some(offset) = lines.iter().skip(1).position(|l| l.trim_end() == "---") {
            return offset + 2;
        }
    }

    0
}

/// Put `header` at the top of `content`, below any load-bearing preamble.
///
/// The no-preamble case stays a plain prepend so nothing about existing files
/// shifts; the preamble case follows the same layout `replace_header_with`
/// emits, so `format --check` sees one canonical shape either way.
fn insert_header(content: &str, header: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let preamble = preamble_lines(&lines);
    if preamble == 0 {
        return format!("{header}\n{content}");
    }

    let mut result = String::new();
    for line in &lines[..preamble] {
        result.push_str(line);
        result.push('\n');
    }
    result.push('\n');

    result.push_str(header);
    result.push('\n');

    for line in lines[preamble..].iter().skip_while(|l| l.trim().is_empty()) {
        result.push_str(line);
        result.push('\n');
    }
    result
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
pub fn format_file(
    content: &str,
    style_override: Option<ResolvedStyle<'_>>,
) -> Result<String, Error> {
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
    let style =
        style_override.unwrap_or_else(|| ResolvedStyle::builtin(detect_style_from_header(content)));

    if style.style() == CommentStyle::Uncommentable {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "File has no known comment syntax; declare one in \
             [[tool.comply.comment-styles]] or use a .license companion",
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
    style: ResolvedStyle<'_>,
) -> Result<String, Error> {
    replace_header_with(
        content,
        CopyrightNotice::new(copyright),
        license_expression,
        style,
        &[],
        false,
    )
}

/// Replace an existing SPDX header with optional contributors and year exclusion.
/// Whether a line declares licensing -- an SPDX tag or a plain copyright notice.
///
/// The tag names are matched in full, colon included. A bare `SPDX-` prefix
/// would also match prose such as `# SPDX-annotated, and clippy-clean`, and
/// treating that as a tag drags the surrounding comment into the header.
fn carries_licensing(line: &str, prefixes: &[&str]) -> bool {
    // The snippet tag is here because `--copyright-prefix snippet*` writes one.
    // A form this can write and not recognise is a form every `--force` run
    // stacks another header on top of.
    // REUSE-IgnoreStart
    const TAGS: [&str; 4] = [
        "SPDX-FileCopyrightText:",
        "SPDX-SnippetCopyrightText:",
        "SPDX-FileContributor:",
        "SPDX-License-Identifier:",
    ];
    // REUSE-IgnoreEnd
    let trimmed = line.trim_start();
    let bare = prefixes
        .iter()
        .find_map(|prefix| trimmed.strip_prefix(prefix))
        .unwrap_or(line);
    let stripped = strip_comment_prefix(bare);
    TAGS.iter().any(|tag| stripped.starts_with(tag)) || is_plain_copyright_notice(&stripped)
}

/// For each line, whether it sits inside a comment.
///
/// Not just a per-line prefix test: a block comment puts its delimiter on one
/// line and the tags *bare* inside it, which is the ordinary shape of a
/// Markdown or CSS header --
/// `<!--` / `SPDX-FileCopyrightText: ...` / `-->`. Judging those interior lines
/// by prefix alone read every such header as data and offered to add a second
/// one above it.
fn comment_mask(lines: &[&str], blocks: &[(&str, &str)], prefixes: &[&str]) -> Vec<bool> {
    let mut mask = Vec::with_capacity(lines.len());
    let mut open = false;
    for line in lines {
        let trimmed = line.trim();
        let opens = blocks.iter().any(|(o, _)| trimmed.starts_with(o));
        let closes = blocks.iter().any(|(_, c)| trimmed.ends_with(c));

        let configured_line = prefixes.iter().any(|p| trimmed.starts_with(p));

        mask.push(open || opens || configured_line || is_comment_line(line));

        if opens && !closes {
            open = true;
        } else if closes {
            open = false;
        }
    }
    mask
}

/// Closing delimiter of a block comment, on its own line.
fn is_block_closer(line: &str, blocks: &[(&str, &str)]) -> bool {
    // Derived from the same table as the openers. The hardcoded
    // `"*/" | "*)" | "-->"` this replaces was the fourth copy of the built-in
    // list: a configured closer fell through it, so `header_span` ended above
    // the closer and every `fix` run left another orphaned one behind.
    let trimmed = line.trim();
    blocks.iter().any(|(_, close)| trimmed == *close)
}

/// The half-open span of the SPDX header block within `lines`.
///
/// The block is the contiguous run of comment lines carrying SPDX tags. A blank
/// line ends that run -- which is how a file keeps a preamble above its header
/// -- a shebang is never part of it, and nothing after the last tag (beyond a
/// block's closing delimiter) belongs to it.
fn header_span(lines: &[&str], rules: &[CommentStyleRule]) -> (usize, usize) {
    // A header is a *comment* block. Bare tags are data -- a template string, a
    // test fixture, a documentation sample -- and rewriting them destroys what
    // the author wrote: `annotate` shredded a TOML `header = """..."""` value
    // whose contents happened to be SPDX tags. The exception is a `.license`
    // companion, which is nothing but bare tags by design, so it is recognised
    // by the whole file being licensing lines rather than by its name.
    // Derived from the style table rather than restated here. The hardcoded
    // `["<!--", "/*", "(*"]` this replaces was a second copy that nothing kept
    // in step with the first: a style gaining a block delimiter would have been
    // written correctly and then not recognised on the way back in.
    let blocks = block_delimiters(rules);
    let prefixes = configured_prefixes(rules);
    let in_comment = comment_mask(lines, &blocks, &prefixes);
    let sidecar = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .all(|l| carries_licensing(l, &prefixes));

    let Some(first) = lines
        .iter()
        .enumerate()
        .position(|(i, l)| carries_licensing(l, &prefixes) && (sidecar || in_comment[i]))
    else {
        return (0, 0);
    };

    // Only a block's opening delimiter is taken from above the first tag.
    // The reference tool also swallows any comment directly abutting the
    // header; comply deliberately does not, because that silently deletes
    // things like Cargo's "automatically @generated" marker.
    let mut start = first;
    if start > 0
        && blocks
            .iter()
            .any(|(open, _)| lines[start - 1].trim() == *open)
    {
        start -= 1;
    }

    // A header sits at the TOP. Tags appearing after real code belong to
    // something else -- a bundled dependency's notice inside a build artefact --
    // and replacing that span deletes a third-party licence declaration, which
    // is the one edit a REUSE tool must never make. Reporting no header instead
    // leaves the notice alone and puts this project's header above it.
    //
    // Comments and blank lines above are a preamble, not code, so they do not
    // disqualify. Neither does a shebang or an XML declaration, which are not
    // comments in every syntax that permits them -- `preamble_lines` already
    // names that set for the insertion path, so it is reused rather than
    // restated.
    let preamble = preamble_lines(lines);
    if !sidecar
        && lines[..start]
            .iter()
            .enumerate()
            .any(|(i, l)| i >= preamble && !l.trim().is_empty() && !in_comment[i])
    {
        return (0, 0);
    }

    // Filler is judged in the syntax THIS header is written in, not in the
    // union of every syntax comply knows. The union read `#include` in a C
    // file, `%matplotlib` in a Python file and a Markdown `* bullet` as part of
    // the header, and `--force` replaces the span wholesale. The marker comes
    // from the header rather than from the style being written, because a
    // header some other tool wrote in another syntax still has to be replaced
    // whole -- scoping to the target style orphans half of it instead.
    let mut markers: Vec<&str> = leading_comment_marker(lines[first])
        .into_iter()
        .chain(prefixes.iter().copied())
        .collect();
    markers.dedup();

    // Only a tag advances the end; separators between tags are swept up on the
    // way, but a comment trailing the block is left where it is.
    let mut last = first;
    for (i, line) in lines.iter().enumerate().skip(first) {
        if carries_licensing(line, &prefixes) {
            last = i;
        } else if !(line.trim().is_empty() || is_comment_line_in(line, &markers)) {
            break;
        }
    }

    let mut end = last + 1;
    if lines.get(end).is_some_and(|l| is_block_closer(l, &blocks)) {
        end += 1;
    }
    (start, end)
}

fn replace_header_with(
    content: &str,
    notice: CopyrightNotice<'_>,
    license_expression: &str,
    style: ResolvedStyle<'_>,
    contributors: &[String],
    exclude_year: bool,
) -> Result<String, Error> {
    let lines: Vec<&str> = content.lines().collect();
    let (start, end) = header_span(&lines, style.rules());

    let new_header = generate_with(
        notice,
        license_expression,
        style,
        contributors,
        exclude_year,
    )?;

    let mut result = String::new();

    // A header the reference tool wrote sits INSIDE the frontmatter, as YAML
    // comments. Reformatting it in place would emit this file's comment syntax
    // there -- `<!-- -->` for Markdown -- which YAML does not recognise, so the
    // block stops parsing. Lift it out to where comply puts a header instead.
    let frontmatter = preamble_lines(&lines);
    if frontmatter > 0 && start < frontmatter {
        let kept: Vec<&str> = lines[..frontmatter]
            .iter()
            .enumerate()
            .filter(|(i, _)| !(start..end).contains(i))
            .map(|(_, line)| *line)
            .collect();

        // The opening delimiter, then the block with any blank the removal left
        // behind dropped -- otherwise the frontmatter opens on an empty line.
        for line in kept.iter().take(1) {
            result.push_str(line);
            result.push('\n');
        }
        for line in kept.iter().skip(1).skip_while(|l| l.trim().is_empty()) {
            result.push_str(line);
            result.push('\n');
        }
        result.push('\n');
        result.push_str(&new_header);
        result.push('\n');
        for line in lines[frontmatter..]
            .iter()
            .skip_while(|l| l.trim().is_empty())
        {
            result.push_str(line);
            result.push('\n');
        }
        return Ok(result);
    }

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
    use crate::header::CopyrightPrefix;

    #[test]
    fn a_licence_notice_below_real_code_is_not_the_files_header() {
        // A bundled dependency's notice, 17k lines into a dist/index.js.
        // Treating the first SPDX-bearing comment ANYWHERE as the header made
        // `--force` delete smol-toml's BSD-3-Clause declaration and write the
        // project's own licence over it -- removing a third-party attribution,
        // which is the one thing a REUSE tool must never do.
        let content = concat!(
            "import { createHash } from \"node:crypto\";\n",
            "\n",
            "/*!\n",
            " * Copyright (c) Vendor et al., All rights reserved.\n",
            " * SPDX-License-Identifier: BSD-3-Clause\n",
            " */\n",
        );
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(
            header_span(&lines, &[]),
            (0, 0),
            "a vendored notice must be left untouched"
        );
    }

    #[test]
    fn a_header_under_a_comment_preamble_is_still_the_files_header() {
        // The guard above must not reject a legitimate header that sits below
        // a generated-file marker or any other comment preamble.
        let content = concat!(
            "// @generated by something\n",
            "\n",
            "// SPDX-FileCopyrightText: Acme\n",
            "// SPDX-License-Identifier: MIT\n",
            "\n",
            "fn main() {}\n",
        );
        let lines: Vec<&str> = content.lines().collect();

        let (start, end) = header_span(&lines, &[]);
        assert!(start < end, "a real header must still be found");
        assert!(lines[start].contains("SPDX-FileCopyrightText"), "{start}");
    }

    fn configured(open: &str, line: &str, close: &str) -> Vec<CommentStyleRule> {
        vec![CommentStyleRule {
            extensions: vec!["vue".to_string()],
            filenames: vec![],
            open: open.to_string(),
            line: line.to_string(),
            close: close.to_string(),
        }]
    }

    /// Writing and reading are separate tables; nothing in the type system
    /// keeps them in step. A configured style comply can WRITE it must also
    /// recognise, or the second run treats its own header as absent and
    /// prepends another.
    #[test]
    fn a_configured_style_round_trips_through_annotate() {
        for (open, line, close) in [("@@", "", ""), ("<%--", " * ", "--%>")] {
            let rules = configured(open, line, close);
            let style = ResolvedStyle::new(CommentStyle::Custom(0), &rules);

            let once = annotate_file(
                "<template></template>\n",
                "2026 Acme",
                "MIT",
                Some(style),
                false,
            )
            .unwrap_or_else(|e| panic!("{open} must annotate: {e}"));

            assert!(once.starts_with(open), "{open}: header missing -> {once}");

            let twice = annotate_file(&once, "2026 Acme", "MIT", Some(style), false)
                .unwrap_or_else(|e| panic!("{open} must re-read: {e}"));

            assert_eq!(twice, once, "{open}: its own header was not recognised");
        }
    }

    /// The two built-ins added for Fortran and Windows batch. `is_comment_line`
    /// and `strip_comment_prefix` have to know `!` and `REM`, or `header_span`
    /// cannot find the header this just wrote and a second `annotate --force`
    /// run stacks a second one above it.
    #[test]
    fn a_bang_or_rem_header_round_trips_through_annotate() {
        for style in [CommentStyle::Bang, CommentStyle::Rem] {
            let once = annotate_file(
                "program hello\nend program hello\n",
                "2026 Acme",
                "MIT",
                Some(ResolvedStyle::builtin(style)),
                false,
            )
            .unwrap_or_else(|e| panic!("{style:?} must annotate: {e}"));

            let twice = annotate_file(
                &once,
                "2026 Acme",
                "MIT",
                Some(ResolvedStyle::builtin(style)),
                true,
            )
            .unwrap_or_else(|e| panic!("{style:?} must re-read: {e}"));

            assert_eq!(twice, once, "{style:?}: its own header was not recognised");
            assert_eq!(
                twice.matches("SPDX-License-Identifier").count(),
                1,
                "{style:?}: a second header was stacked -> {twice}"
            );
        }
    }

    /// The same round trip for every other built-in. Both predicates now read
    /// one marker table, which closed a gap the two lists had already drifted
    /// into: `..` was missing from the one `comment_mask` consults, so an
    /// reStructuredText header comply had just written did not read back as a
    /// comment at all and the next run stacked a second one above it.
    #[test]
    fn every_line_style_header_round_trips_through_annotate() {
        for style in [
            CommentStyle::Slash,
            CommentStyle::Hash,
            CommentStyle::Dash,
            CommentStyle::Semicolon,
            CommentStyle::Percent,
            CommentStyle::DotDot,
            CommentStyle::SlashStar,
            CommentStyle::AsteriskParen,
            CommentStyle::Html,
        ] {
            let once = annotate_file(
                "Title\n=====\n\ntext\n",
                "2026 Acme",
                "MIT",
                Some(ResolvedStyle::builtin(style)),
                false,
            )
            .unwrap_or_else(|e| panic!("{style:?} must annotate: {e}"));

            let twice = annotate_file(
                &once,
                "2026 Acme",
                "MIT",
                Some(ResolvedStyle::builtin(style)),
                true,
            )
            .unwrap_or_else(|e| panic!("{style:?} must re-read: {e}"));

            assert_eq!(twice, once, "{style:?}: its own header was not recognised");
            assert_eq!(
                twice.matches("SPDX-License-Identifier").count(),
                1,
                "{style:?}: a second header was stacked -> {twice}"
            );
        }
    }

    /// An XML declaration is a declaration only on line 1. A comment above it
    /// is not a comment but a fatal parse error, so the header has to go below
    /// -- which is where reuse 6.2.0 puts it, and the same rule `#!` and `---`
    /// already get. `.svg` used to reach this path; `.xml` and `.xhtml` still
    /// do, and `pom.xml` classifies as source, so this is an ordinary file.
    #[test]
    fn a_header_goes_below_an_xml_declaration() {
        const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
        let style = ResolvedStyle::builtin(CommentStyle::Html);

        let annotated = annotate_file(
            &format!("{DECLARATION}<root/>\n"),
            "2026 X",
            "MIT",
            Some(style),
            false,
        )
        .expect("must annotate");

        assert!(
            annotated.starts_with(DECLARATION),
            "the declaration must stay on line 1 -> {annotated}"
        );

        let again = annotate_file(&annotated, "2026 X", "MIT", Some(style), true)
            .expect("must rewrite its own header");

        assert_eq!(again, annotated, "rewriting moved the declaration");
    }

    /// `is_comment_line` is not scoped to a file type -- `header_span` asks it
    /// which lines may sit *between* two tags, whatever the file is -- so a
    /// bare `!` or `REM` prefix made ordinary source lines part of the header,
    /// and `--force` replaces a span wholesale. `!` opens every Markdown image
    /// and `REM` prefixes identifiers such as `REMOTE` in every language.
    #[test]
    fn force_keeps_a_line_that_merely_starts_with_bang_or_rem() {
        let cases = [
            (
                concat!(
                    "# SPDX-FileCopyrightText: 2019 Original\n",
                    "#\n",
                    "# SPDX-License-Identifier: GPL-3.0-only\n",
                    "REMOTE_URL=https://example.com\n",
                    "# SPDX-FileContributor: Someone Else\n",
                    "echo hi\n"
                ),
                "REMOTE_URL=https://example.com",
            ),
            (
                concat!(
                    "# SPDX-FileCopyrightText: 2019 Original\n",
                    "#\n",
                    "# SPDX-License-Identifier: GPL-3.0-only\n",
                    "!important = 1\n",
                    "# SPDX-FileContributor: Someone Else\n",
                    "print(\"hi\")\n"
                ),
                "!important = 1",
            ),
        ];

        for (content, survivor) in cases {
            let forced = annotate_file(
                content,
                "2026 X",
                "MIT",
                Some(ResolvedStyle::builtin(CommentStyle::Hash)),
                true,
            )
            .expect("must annotate");

            assert!(
                forced.contains(survivor),
                "{survivor} was deleted -> {forced}"
            );
        }
    }

    /// The same hazard as `!`/`REM`, for every other marker. `is_comment_line`
    /// tests the union of every comment syntax comply knows, whatever the file
    /// is, so a line that merely opens with ANOTHER language's marker counted
    /// as header filler and `--force` replaced the span wholesale. Each row is
    /// a real line of the language its style names.
    #[test]
    fn force_keeps_a_source_line_that_opens_with_a_foreign_marker() {
        let cases = [
            (
                CommentStyle::Html,
                concat!(
                    "<!-- SPDX-FileCopyrightText: 2019 Original -->\n",
                    "\n",
                    "* a bullet item\n",
                    "\n",
                    "<!-- SPDX-License-Identifier: GPL-3.0-only -->\n",
                    "\n",
                    "Prose.\n"
                ),
                "* a bullet item",
            ),
            (
                CommentStyle::Slash,
                concat!(
                    "// SPDX-FileCopyrightText: 2019 Original\n",
                    "\n",
                    "#include <stdio.h>\n",
                    "\n",
                    "// SPDX-License-Identifier: GPL-3.0-only\n",
                    "\n",
                    "int main(void) { return 0; }\n"
                ),
                "#include <stdio.h>",
            ),
            (
                CommentStyle::Hash,
                concat!(
                    "# SPDX-FileCopyrightText: 2019 Original\n",
                    "\n",
                    "---\n",
                    "\n",
                    "# SPDX-License-Identifier: GPL-3.0-only\n",
                    "\n",
                    "key: value\n"
                ),
                "---",
            ),
            (
                CommentStyle::Slash,
                concat!(
                    "// SPDX-FileCopyrightText: 2019 Original\n",
                    "\n",
                    ";(function(){})()\n",
                    "\n",
                    "// SPDX-License-Identifier: GPL-3.0-only\n",
                    "\n",
                    "console.log(1)\n"
                ),
                ";(function(){})()",
            ),
            (
                CommentStyle::Hash,
                concat!(
                    "# SPDX-FileCopyrightText: 2019 Original\n",
                    "\n",
                    "%matplotlib inline\n",
                    "\n",
                    "# SPDX-License-Identifier: GPL-3.0-only\n",
                    "\n",
                    "x = 1\n"
                ),
                "%matplotlib inline",
            ),
            (
                CommentStyle::Hash,
                concat!(
                    "# SPDX-FileCopyrightText: 2019 Original\n",
                    "\n",
                    "--flag=value\n",
                    "\n",
                    "# SPDX-License-Identifier: GPL-3.0-only\n",
                    "\n",
                    "echo hi\n"
                ),
                "--flag=value",
            ),
        ];

        let mut deleted = Vec::new();
        for (style, content, survivor) in cases {
            let forced = annotate_file(
                content,
                "2026 X",
                "MIT",
                Some(ResolvedStyle::builtin(style)),
                true,
            )
            .unwrap_or_else(|e| panic!("{style:?} must annotate: {e}"));

            if !forced.contains(survivor) {
                deleted.push(survivor);
            }
        }

        assert!(deleted.is_empty(), "deleted by --force: {deleted:?}");
    }

    /// The opposite pull on the same span. Scoping it to the style being
    /// WRITTEN is the wrong scope: a `.rs` file whose header was written with
    /// `#` still has to be located and replaced whole. Narrowed to `//`, the
    /// span stops at the copyright line and the rest of the old header is left
    /// orphaned below the new one -- or, if the tag line itself stops matching,
    /// the file gains a second header above the first.
    #[test]
    fn a_header_written_in_another_syntax_is_still_replaced_whole() {
        let content = concat!(
            "# SPDX-FileCopyrightText: 2001 Old\n",
            "#\n",
            "# SPDX-License-Identifier: MIT\n",
            "\n",
            "fn main() {}\n"
        );

        let out = format_file(content, Some(ResolvedStyle::builtin(CommentStyle::Slash)))
            .expect("a header in another syntax is still a header");

        assert_eq!(
            out.matches("SPDX-License-Identifier").count(),
            1,
            "exactly one header -- more means the old one survived:\n{out}"
        );
        assert!(
            out.starts_with("// SPDX-FileCopyrightText: 2001 Old\n"),
            "the header must be rewritten in place, not stacked:\n{out}"
        );
        assert!(
            !out.contains("# SPDX"),
            "no part of the old header may survive:\n{out}"
        );
        assert!(out.contains("fn main() {}"), "body must survive:\n{out}");
    }

    /// `format` regenerates the header and replaces the span. If the span ends
    /// above a configured closer, the old closer survives below the new header
    /// and every run leaves another one behind -- the file grows without bound.
    #[test]
    fn formatting_a_configured_block_header_is_idempotent() {
        let rules = configured("<%--", " * ", "--%>");
        let style = ResolvedStyle::new(CommentStyle::Custom(0), &rules);
        let annotated = annotate_file(
            "<template></template>\n",
            "2026 Acme",
            "MIT",
            Some(style),
            false,
        )
        .expect("must annotate");

        let formatted = format_file(&annotated, Some(style)).expect("must format");

        assert_eq!(formatted, annotated, "format left the old closer behind");
        assert_eq!(
            formatted.matches("--%>").count(),
            1,
            "exactly one closer: {formatted}"
        );
    }

    /// The separator between the copyright and the licence carries no tag and
    /// is not a built-in comment, so a span that ended at the first such line
    /// would cover only half a configured header -- and `--force` would then
    /// rewrite the top of it and leave the licence line orphaned below.
    #[test]
    fn a_configured_line_style_span_covers_the_whole_header() {
        let rules = configured("@@", "", "");
        let content =
            "@@ SPDX-FileCopyrightText: 2026 Acme\n@@\n@@ SPDX-License-Identifier: MIT\n\nbody\n";
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(header_span(&lines, &rules), (0, 3));
    }

    /// The rules ride along on the style, so `header_span` sees a configured
    /// block even when the file it scans uses one comply has no built-in for.
    #[test]
    fn a_configured_block_header_is_not_mistaken_for_bare_tags() {
        let rules = configured("<%--", " * ", "--%>");
        let content = "<%--\n * SPDX-FileCopyrightText: 2026 Acme\n *\n * SPDX-License-Identifier: MIT\n --%>\nbody\n";
        let lines: Vec<&str> = content.lines().collect();

        let (start, end) = header_span(&lines, &rules);

        assert_eq!(
            start, 0,
            "the opener must be taken from above the first tag"
        );
        assert!(end > start, "a configured block must register as a header");
    }

    /// Some first lines are load-bearing: the kernel only honours `#!` on line
    /// 1, and YAML frontmatter is only frontmatter when `---` opens the file.
    /// A header prepended above either breaks the file rather than annotating
    /// it. Verified against `reuse` 6.2.0, which also inserts below both.
    /// A header is the file's own SPDX comment block. Tags that are not in a
    /// comment are data -- a template string, a fixture, a doc sample -- and
    /// rewriting them destroys whatever the author meant. `reuse` 6.2.0
    /// prepends a real header in that case and leaves the text alone.
    mod header_must_be_a_comment {
        use super::*;

        const TEMPLATE: &str = concat!(
            "header = \"\"\"\n",
            "SPDX-FileCopyrightText: ignorefile contributors\n",
            "\n",
            "SPDX-License-Identifier: MIT OR Apache-2.0\"\"\"\n",
            "name = \"Ignored\"\n"
        );

        #[test]
        fn bare_tags_in_a_string_are_not_a_header_to_replace() {
            // These lines carry SPDX tags but have no `#`, so in TOML they are
            // string data. Treating them as the header made `annotate` rewrite
            // the span and shred the value.
            let out = annotate_file(
                TEMPLATE,
                "Acme",
                "MIT",
                Some(ResolvedStyle::builtin(CommentStyle::Hash)),
                true,
            )
            .expect("annotating must succeed");

            assert!(
                out.contains("SPDX-FileCopyrightText: ignorefile contributors"),
                "the template string must survive verbatim, got:\n{out}"
            );
            assert!(out.contains("name = \"Ignored\""), "got:\n{out}");
            assert!(
                out.starts_with("# SPDX-FileCopyrightText:"),
                "a real header belongs on top instead, got:\n{out}"
            );
        }

        #[test]
        fn a_commented_header_is_still_replaced_on_force() {
            // The guard must not stop `--force` rewriting a genuine header.
            let content = "# SPDX-FileCopyrightText: 2025 Old\n#\n# SPDX-License-Identifier: Apache-2.0\n\nx = 1\n";

            let out = annotate_file(
                content,
                "2026 New",
                "MIT",
                Some(ResolvedStyle::builtin(CommentStyle::Hash)),
                true,
            )
            .expect("force must still replace");

            assert!(out.contains("2026 New"), "got:\n{out}");
            assert!(
                !out.contains("2025 Old"),
                "the old header must go, got:\n{out}"
            );
        }

        #[test]
        fn a_style_override_still_finds_a_block_header_it_does_not_match() {
            // Characterisation, guarding the rewrite of `header_span` onto
            // `comment_delimiters`. The block here is `<!-- ... -->` while the
            // override asks for `//`. If the scan ever locates the existing
            // header using the style being WRITTEN rather than the file's own,
            // this header becomes invisible and a second one gets prepended
            // above it. Nothing else covers that, and `format --check` is a CI
            // gate, so the failure would land as churn on every annotated file.
            let content = concat!(
                "<!--\n",
                "SPDX-FileCopyrightText: 2026 Acme\n",
                "\n",
                "SPDX-License-Identifier: MIT\n",
                "-->\n",
                "\n",
                "# Doc\n"
            );

            let out = format_file(content, Some(ResolvedStyle::builtin(CommentStyle::Slash)))
                .expect("an override must still locate the block");

            assert_eq!(
                out.matches("SPDX-License-Identifier").count(),
                1,
                "exactly one header -- a second means the first was not found:\n{out}"
            );
            assert!(out.contains("# Doc"), "body must survive:\n{out}");
        }

        #[test]
        fn a_license_companion_of_bare_tags_is_still_a_header() {
            // A `.license` sidecar carries bare tags with no comment prefix --
            // that is how the reference tool writes them and how comply reads
            // them. The comment rule must not break that.
            let companion = "SPDX-FileCopyrightText: 2026 Acme\nSPDX-License-Identifier: MIT\n";

            let detected = detect(companion).expect("a companion is a header");

            assert_eq!(detected.license_expression.as_deref(), Some("MIT"));
            assert_eq!(detected.copyrights, ["2026 Acme"]);
        }
    }

    mod preamble {
        use super::*;

        #[test]
        fn a_new_header_goes_below_yaml_frontmatter() {
            let content = "---\nname: doc\ncreated_by: someone\n---\n\n# Title\n\ntext\n";
            let out = annotate_file(
                content,
                "Acme",
                "MIT",
                Some(ResolvedStyle::builtin(CommentStyle::Html)),
                false,
            )
            .unwrap();

            assert!(
                out.starts_with("---\nname: doc\ncreated_by: someone\n---\n"),
                "frontmatter has to keep line 1, got:\n{out}"
            );
            assert!(out.contains("SPDX-License-Identifier: MIT"), "got:\n{out}");
            assert!(out.trim_end().ends_with("text"), "body kept, got:\n{out}");
        }

        #[test]
        fn a_new_header_goes_below_a_shebang() {
            let content = "#!/bin/sh\necho hi\n";
            let out = annotate_file(
                content,
                "Acme",
                "MIT",
                Some(ResolvedStyle::builtin(CommentStyle::Hash)),
                false,
            )
            .unwrap();

            assert!(
                out.starts_with("#!/bin/sh\n"),
                "a shebang below line 1 is not a shebang, got:\n{out}"
            );
            assert!(out.contains("SPDX-License-Identifier: MIT"), "got:\n{out}");
        }

        #[test]
        fn a_header_inside_frontmatter_is_lifted_out_not_rewritten_in_place() {
            // reuse writes the tags inside the frontmatter as YAML comments.
            // Reformatting them where they lie emits this file's comment style
            // -- `<!-- -->` for Markdown -- inside the YAML block, which is not
            // a YAML comment, so the frontmatter stops parsing entirely.
            let content = concat!(
                "---\n",
                "# SPDX-FileCopyrightText: 2026 Acme\n",
                "#\n",
                "# SPDX-License-Identifier: MIT\n",
                "\n",
                "name: doc\n",
                "---\n",
                "\n",
                "# Title\n"
            );
            let out =
                format_file(content, Some(ResolvedStyle::builtin(CommentStyle::Html))).unwrap();

            assert!(
                out.starts_with("---\nname: doc\n---\n"),
                "the frontmatter block must come out intact, got:\n{out}"
            );
            assert!(
                out.contains("SPDX-License-Identifier: MIT"),
                "the licensing must survive, got:\n{out}"
            );

            // Nothing of the header may remain between the delimiters.
            let block = out
                .strip_prefix("---\n")
                .and_then(|rest| rest.split_once("\n---\n"))
                .map(|(block, _)| block.to_string())
                .unwrap_or_default();
            assert!(
                !block.contains("SPDX") && !block.contains("<!--"),
                "frontmatter still holds header text: {block:?}"
            );
        }

        #[test]
        fn a_lone_dash_rule_is_not_frontmatter() {
            // `---` opening a file is frontmatter only if a closing `---`
            // follows. A document that merely starts with a horizontal rule
            // still takes the header on line 1.
            let content = "---\n\nJust a rule, no closing delimiter.\n";
            let out = annotate_file(
                content,
                "Acme",
                "MIT",
                Some(ResolvedStyle::builtin(CommentStyle::Html)),
                false,
            )
            .unwrap();

            assert!(out.starts_with("<!--"), "got:\n{out}");
        }

        #[test]
        fn frontmatter_not_at_the_start_is_left_alone() {
            let content = "# Title\n\n---\nname: not-frontmatter\n---\n";
            let out = annotate_file(
                content,
                "Acme",
                "MIT",
                Some(ResolvedStyle::builtin(CommentStyle::Html)),
                false,
            )
            .unwrap();

            assert!(out.starts_with("<!--"), "got:\n{out}");
        }
    }

    /// The header block is the contiguous run of comment lines carrying SPDX
    /// tags. A blank line ends it, a shebang is never part of it, and nothing
    /// past it is touched. Behaviours verified against `reuse` 6.2.0.
    mod header_boundary {
        use super::*;

        const HDR: &str = "# SPDX-FileCopyrightText: 2001 Old\n#\n# SPDX-License-Identifier: MIT\n";

        #[test]
        fn a_file_carrying_no_licensing_at_all_has_an_empty_span() {
            // The empty span is what makes `replace_header_with` insert at the
            // top instead of overwriting the first lines of an unrelated file.
            let lines = ["fn main() {}", "// just a comment", "x = 1"];

            assert_eq!(header_span(&lines, &[]), (0, 0));
        }

        #[test]
        fn a_shebang_survives_and_keeps_line_one() {
            let content = format!("#!/bin/sh\n{HDR}\necho hi\n");

            let out =
                format_file(&content, Some(ResolvedStyle::builtin(CommentStyle::Hash))).unwrap();

            assert!(
                out.starts_with("#!/bin/sh\n"),
                "the shebang must stay executable, got:\n{out}"
            );
            assert!(out.contains("echo hi"), "body must survive:\n{out}");
        }

        #[test]
        fn a_preamble_separated_by_a_blank_line_is_preserved() {
            let content = format!("# This file is @generated.\n\n{HDR}\nx = 1\n");

            let out =
                format_file(&content, Some(ResolvedStyle::builtin(CommentStyle::Hash))).unwrap();

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

            let out =
                format_file(&content, Some(ResolvedStyle::builtin(CommentStyle::Hash))).unwrap();

            assert!(out.contains("# adjacent note"), "got:\n{out}");
        }

        #[test]
        fn prose_mentioning_an_spdx_tag_is_not_treated_as_one() {
            // `SPDX-annotated` is not a tag; reading it as one drags the whole
            // surrounding comment into the header and deletes it.
            let content =
                format!("{HDR}\n# keeps sources SPDX-annotated and clippy-clean.\nx = 1\n");

            let out =
                format_file(&content, Some(ResolvedStyle::builtin(CommentStyle::Hash))).unwrap();

            assert!(
                out.contains("SPDX-annotated and clippy-clean"),
                "got:\n{out}"
            );
        }

        #[test]
        fn a_comment_after_the_header_is_left_alone() {
            let content = format!("{HDR}\n# a note about the code below\nx = 1\n");

            let out =
                format_file(&content, Some(ResolvedStyle::builtin(CommentStyle::Hash))).unwrap();

            assert!(
                out.contains("# a note about the code below"),
                "nothing past the header block may be consumed, got:\n{out}"
            );
        }

        #[test]
        fn a_block_comment_header_is_replaced_whole() {
            let content = "/*\n * SPDX-FileCopyrightText: 2001 Old\n *\n * SPDX-License-Identifier: MIT\n */\n\nbody {}\n";

            let out = format_file(
                content,
                Some(ResolvedStyle::builtin(CommentStyle::SlashStar)),
            )
            .unwrap();

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

            let out =
                format_file(content, Some(ResolvedStyle::builtin(CommentStyle::Hash))).unwrap();

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
            Some(ResolvedStyle::builtin(CommentStyle::Slash)),
            false,
            &[],
            false,
        );

        assert!(result.is_err());
    }

    #[test]
    fn reformatting_a_header_whose_expression_is_unparseable_leaves_it_alone() {
        // This used to return Err. `header::detect` now keeps an unparseable
        // tag out of the expression, so formatting no longer trips over it --
        // and the file comes back untouched rather than the error aborting the
        // caller. `lint` is what reports the bad tag; `format` is not the place
        // to fail on it, and rewriting the line would risk destroying whatever
        // the author actually meant.
        let content = concat!(
            "// SPDX-FileCopyrightText: 2026 Acme\n",
            "// SPDX-License-Identifier: NOT A VALID EXPRESSION\n",
            "\n",
            "fn main() {}\n"
        );

        let out = format_file(content, Some(ResolvedStyle::builtin(CommentStyle::Slash)))
            .expect("an unparseable tag is reported by lint, not fatal here");

        assert!(
            out.contains("NOT A VALID EXPRESSION"),
            "the author's text must survive verbatim, got:\n{out}"
        );
        assert!(out.contains("fn main() {}"), "body must survive:\n{out}");
    }

    #[test]
    fn formatting_into_an_uncommentable_style_fails() {
        let content = concat!(
            "// SPDX-FileCopyrightText: 2026 Acme\n",
            "// SPDX-License-Identifier: MIT\n",
            "\n",
            "fn main() {}\n"
        );

        let err = format_file(
            content,
            Some(ResolvedStyle::builtin(CommentStyle::Uncommentable)),
        )
        .expect_err("a binary format cannot hold a comment");

        assert!(
            err.to_string().contains("no known comment syntax"),
            "got: {err}"
        );
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
        let result =
            format_file(content, Some(ResolvedStyle::builtin(CommentStyle::Slash))).unwrap();
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
            Some(ResolvedStyle::builtin(CommentStyle::Slash)),
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
            Some(ResolvedStyle::builtin(CommentStyle::Hash)),
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
            Some(ResolvedStyle::builtin(CommentStyle::Html)),
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
            Some(ResolvedStyle::builtin(CommentStyle::Uncommentable)),
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
            Some(ResolvedStyle::builtin(CommentStyle::Slash)),
            false,
            &["John Doe".to_string()],
            false,
        )
        .unwrap();
        assert!(result.contains("// SPDX-FileCopyrightText: 2026 Acme Inc"));
        assert!(result.contains("// SPDX-FileContributor: John Doe"));
        assert!(result.contains("// SPDX-License-Identifier: MIT"));
    }

    #[test]
    fn every_prefix_form_is_written_in_the_form_it_names() {
        for value in CopyrightPrefix::VALUES {
            let prefix: CopyrightPrefix = value.parse().unwrap();
            let out = annotate_file(
                "fn main() {}\n",
                CopyrightNotice::new("2026 Acme").with_prefix(prefix),
                "MIT",
                Some(ResolvedStyle::builtin(CommentStyle::Slash)),
                false,
            )
            .unwrap();

            assert!(
                out.contains(&format!("// {}2026 Acme", prefix.tag())),
                "`--copyright-prefix {value}` did not render its own form:\n{out}"
            );
        }
    }

    /// A form comply can write, it must also recognise on the way back in.
    ///
    /// This is the failure the comment-style work already hit once: a value was
    /// written correctly and then not read back, so each run stacked another
    /// header on the file. The bare and snippet forms carry no
    /// `SPDX-` `FileCopyrightText` tag, which is exactly what `header_span`
    /// looks for.
    #[test]
    fn a_header_in_any_prefix_form_is_replaced_rather_than_duplicated() {
        for value in CopyrightPrefix::VALUES {
            let prefix: CopyrightPrefix = value.parse().unwrap();
            let notice = CopyrightNotice::new("2020 First").with_prefix(prefix);
            let style = Some(ResolvedStyle::builtin(CommentStyle::Slash));

            let once = annotate_file("fn main() {}\n", notice, "MIT", style, false).unwrap();
            let twice =
                annotate_file(&once, notice.with_text("2021 Second"), "MIT", style, true).unwrap();

            assert_eq!(
                twice.matches("SPDX-License-Identifier").count(),
                1,
                "`--copyright-prefix {value}` stacked a second header:\n{twice}"
            );
            assert!(
                twice.contains("2021 Second") && !twice.contains("2020 First"),
                "`--copyright-prefix {value}` did not replace the old notice:\n{twice}"
            );
        }
    }
}
// REUSE-IgnoreEnd
