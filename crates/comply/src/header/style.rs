// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::CommentStyle;
use crate::config::CommentStyleRule;

/// The one source of truth for a built-in style's delimiters and the
/// extensions/filenames that map to it. See that file for the schema.
const COMMENT_STYLES_TOML: &str = include_str!("../resources/comment-styles.toml");

/// Which form a style's delimiters take.
///
/// Naming this removes an old inference (a non-empty `close` meant "block"),
/// which could not say a style meant both a line and a block form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StyleKind {
    Line,
    Block,
    None,
}

/// One `comment-styles.toml` table: the delimiters and file-matching rules
/// for a single built-in [`CommentStyle`].
///
/// Unknown keys are rejected rather than ignored: a mistyped `extensions`
/// deserialized to a table claiming nothing at all, and the first sign of it
/// was a file silently refused as having no known comment syntax.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStyleEntry {
    kind: StyleKind,
    #[serde(default)]
    open: String,
    #[serde(default)]
    close: String,
    /// What leads each interior line of a block style's header.
    #[serde(default)]
    line: String,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    filenames: Vec<String>,
}

impl RawStyleEntry {
    /// `kind` must agree with which delimiters are actually set, which is the
    /// entire reason it exists rather than being inferred.
    fn validate(&self, key: &str) -> Result<(), String> {
        // Only [`StyleKind::Block`] reads `line`; accepting it elsewhere puts a
        // value in the file that nothing consults, which is the drift `kind`
        // was named to remove.
        if self.kind != StyleKind::Block && !self.line.is_empty() {
            return Err(format!("{key}: only kind \"block\" reads line"));
        }

        match self.kind {
            StyleKind::Line if !self.close.is_empty() => {
                Err(format!("{key}: kind \"line\" must leave close empty"))
            }
            StyleKind::Block if self.open.is_empty() || self.close.is_empty() => {
                Err(format!("{key}: kind \"block\" needs both open and close"))
            }
            StyleKind::None if !self.open.is_empty() || !self.close.is_empty() => {
                Err(format!("{key}: kind \"none\" must leave delimiters empty"))
            }
            StyleKind::Line | StyleKind::Block | StyleKind::None => Ok(()),
        }
    }
}

/// Map a resource table key to the [`CommentStyle`] it defines.
///
/// This direction (str -> enum) cannot be checked for exhaustiveness by the
/// compiler the way [`assert_style_is_listed`] checks the other one;
/// `every_builtin_style_has_a_resource_entry` is its runtime counterpart.
fn key_to_style(key: &str) -> Option<CommentStyle> {
    Some(match key {
        "slash" => CommentStyle::Slash,
        "hash" => CommentStyle::Hash,
        "dash" => CommentStyle::Dash,
        "semicolon" => CommentStyle::Semicolon,
        "percent" => CommentStyle::Percent,
        "asterisk-paren" => CommentStyle::AsteriskParen,
        "slash-star" => CommentStyle::SlashStar,
        "dot-dot" => CommentStyle::DotDot,
        "bang" => CommentStyle::Bang,
        "rem" => CommentStyle::Rem,
        "html" => CommentStyle::Html,
        "uncommentable" => CommentStyle::Uncommentable,
        _ => return None,
    })
}

/// Parse and validate the embedded resource text.
///
/// Kept separate from [`registry`] so a malformed file fails a test
/// (`the_embedded_file_parses_and_validates` and its malformed-input
/// siblings) rather than panicking at runtime for a user.
fn parse_registry(text: &str) -> Result<Vec<(CommentStyle, RawStyleEntry)>, String> {
    let raw: BTreeMap<String, RawStyleEntry> =
        toml::from_str(text).map_err(|error| error.to_string())?;

    let mut entries = Vec::with_capacity(raw.len());
    for (key, mut entry) in raw {
        let style =
            key_to_style(&key).ok_or_else(|| format!("unknown comment style key: {key}"))?;
        entry.validate(&key)?;
        // `filename_style` compares against an already-lowercased basename;
        // lowercasing once here keeps that comparison a plain `==`.
        for filename in &mut entry.filenames {
            *filename = filename.to_lowercase();
        }
        entries.push((style, entry));
    }
    Ok(entries)
}

static REGISTRY: OnceLock<Vec<(CommentStyle, RawStyleEntry)>> = OnceLock::new();

/// Parse the embedded resource, panicking with the parse error on failure.
///
/// Split out from [`registry`] so `a_malformed_registry_panics_rather_than_silently_degrading`
/// can exercise this branch directly with deliberately invalid text; the real
/// embedded file never takes it, since `the_embedded_file_parses_and_validates`
/// proves it valid.
fn load_registry(text: &str) -> Vec<(CommentStyle, RawStyleEntry)> {
    parse_registry(text).unwrap_or_else(|error| {
        panic!("crates/comply/src/resources/comment-styles.toml is invalid: {error}")
    })
}

/// The parsed, validated embedded resource.
///
/// `REGISTRY` is itself `static`, so `get_or_init` hands back `&'static` data
/// -- which is what lets [`comment_delimiters`] keep returning
/// `(&'static str, &'static str)` despite no longer being a `const fn`.
fn registry() -> &'static [(CommentStyle, RawStyleEntry)] {
    REGISTRY.get_or_init(|| load_registry(COMMENT_STYLES_TOML))
}

/// The resource table defining `style`, or `None` for a style no table claims.
///
/// `Custom`'s delimiters live in the manifest, which this module cannot see.
/// Callers that support configured styles go through [`ResolvedStyle`];
/// reaching here with one means a caller did not, and the empty answer surfaces
/// that as a header never written rather than as a wrong one that is.
fn style_entry(style: CommentStyle) -> Option<&'static RawStyleEntry> {
    registry()
        .iter()
        .find(|(entry_style, _)| *entry_style == style)
        .map(|(_, entry)| entry)
}

/// Return the comment prefix and suffix for a style.
pub(super) fn comment_delimiters(style: CommentStyle) -> (&'static str, &'static str) {
    style_entry(style).map_or(("", ""), |entry| {
        (entry.open.as_str(), entry.close.as_str())
    })
}

/// Whether a built-in brackets its header, as its table declares.
fn builtin_is_block(style: CommentStyle) -> bool {
    style_entry(style).is_some_and(|entry| entry.kind == StyleKind::Block)
}

/// What leads each body line of a built-in header.
///
/// A line style repeats its opener; a block style repeats the `line` prefix its
/// table declares, which is empty for HTML and `" * "` for the C family. Both
/// come out of the resource file, so a new style is a data edit.
fn builtin_lead(style: CommentStyle) -> String {
    style_entry(style).map_or_else(String::new, |entry| match entry.kind {
        StyleKind::Line => format!("{} ", entry.open),
        // `Uncommentable` renders only as a `.license` companion, whose lines
        // carry no prefix, and declares no `line` -- so it shares this arm.
        StyleKind::Block | StyleKind::None => entry.line.clone(),
    })
}

/// A [`CommentStyle`] together with the rules needed to resolve it.
///
/// A bare [`CommentStyle::Custom`] is only an index; the delimiters it points
/// at live in `[[tool.comply.comment-styles]]`. Pairing the two makes an
/// unresolved style unrepresentable, so rendering can no longer be handed one
/// it cannot write -- what used to be a runtime refusal is now a type error.
///
/// `Copy` for the same reason [`CommentStyle::Custom`] holds an index rather
/// than owned strings: this is threaded through the whole header path, and a
/// clone at every hop would be noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedStyle<'a> {
    style: CommentStyle,
    rules: &'a [CommentStyleRule],
}

impl<'a> ResolvedStyle<'a> {
    /// A built-in style, which needs no rules to resolve.
    #[must_use]
    pub const fn builtin(style: CommentStyle) -> Self {
        Self { style, rules: &[] }
    }

    /// Pair `style` with the rules it may index into.
    #[must_use]
    pub const fn new(style: CommentStyle, rules: &'a [CommentStyleRule]) -> Self {
        Self { style, rules }
    }

    /// Detect the style for `path`, keeping the rules that decided it.
    #[must_use]
    pub fn detect(path: &str, rules: &'a [CommentStyleRule]) -> Self {
        Self::new(detect_style_with(path, rules), rules)
    }

    /// The style this resolves.
    #[must_use]
    pub const fn style(self) -> CommentStyle {
        self.style
    }

    /// The rules carried alongside, for callers that must recognise any
    /// configured style rather than just this one.
    #[must_use]
    pub const fn rules(self) -> &'a [CommentStyleRule] {
        self.rules
    }

    /// The rule this indexes, if it is a configured style that still exists.
    ///
    /// An index past the end means the manifest changed after the style was
    /// resolved. That yields `None` and the built-in fallback rather than a
    /// panic: a stale index is a reason to write no header, not to abort a run.
    fn rule(self) -> Option<&'a CommentStyleRule> {
        match self.style {
            CommentStyle::Custom(index) => self.rules.get(index as usize),
            _ => None,
        }
    }

    /// Opening and closing delimiters. `close` empty means a line style.
    #[must_use]
    pub fn delimiters(self) -> (&'a str, &'a str) {
        self.rule().map_or_else(
            || comment_delimiters(self.style),
            |rule| (rule.open.as_str(), rule.close.as_str()),
        )
    }

    /// Whether the header is bracketed, with the delimiters on their own lines.
    ///
    /// A built-in says so in its `kind`; a configured rule has no such key, so
    /// there the non-empty close is still what declares the intent.
    #[must_use]
    pub fn is_block(self) -> bool {
        self.rule().map_or_else(
            || builtin_is_block(self.style),
            |rule| !rule.close.is_empty(),
        )
    }

    /// What leads each line of the header body.
    ///
    /// A block style repeats its `line` prefix (`" * "` for C, nothing for
    /// HTML); a line style repeats its opener. The reference tool's exact
    /// spacing is what keeps the two tools from rewriting each other.
    #[must_use]
    pub fn lead(self) -> String {
        self.rule().map_or_else(
            || builtin_lead(self.style),
            |rule| {
                if rule.close.is_empty() {
                    format!("{} ", rule.open)
                } else {
                    rule.line.clone()
                }
            },
        )
    }
}

/// Every built-in style. `Custom` is absent by nature -- it is not a fixed
/// variant but one entry per configured rule, so it cannot be enumerated here.
///
/// A plain array is not checked for exhaustiveness by the compiler, which is
/// what [`assert_style_is_listed`] is for.
const BUILTIN_STYLES: [CommentStyle; 12] = [
    CommentStyle::Slash,
    CommentStyle::Hash,
    CommentStyle::Dash,
    CommentStyle::Semicolon,
    CommentStyle::Percent,
    CommentStyle::AsteriskParen,
    CommentStyle::SlashStar,
    CommentStyle::DotDot,
    CommentStyle::Bang,
    CommentStyle::Rem,
    CommentStyle::Html,
    CommentStyle::Uncommentable,
];

/// Adding a `CommentStyle` variant makes this match non-exhaustive and breaks
/// the build -- deliberately, since nothing else would point the author at
/// [`BUILTIN_STYLES`], which they must extend too.
const fn assert_style_is_listed(style: CommentStyle) {
    match style {
        CommentStyle::Slash
        | CommentStyle::Hash
        | CommentStyle::Dash
        | CommentStyle::Semicolon
        | CommentStyle::Percent
        | CommentStyle::AsteriskParen
        | CommentStyle::SlashStar
        | CommentStyle::DotDot
        | CommentStyle::Bang
        | CommentStyle::Rem
        | CommentStyle::Html
        | CommentStyle::Uncommentable
        | CommentStyle::Custom(_) => (),
    }
}

/// The `(open, close)` pairs of every style that brackets its header.
///
/// Derived from [`comment_delimiters`] rather than restated, so a style that
/// gains or changes a delimiter is picked up automatically. `header_span` used
/// to carry its own copy -- three hardcoded openers -- which could drift from
/// the real table without anything noticing. Configured block styles are
/// appended, so a header written in one is recognised as a header.
pub(super) fn block_delimiters<'a>(rules: &'a [CommentStyleRule]) -> Vec<(&'a str, &'a str)> {
    BUILTIN_STYLES
        .iter()
        .copied()
        .inspect(|style| assert_style_is_listed(*style))
        .map(comment_delimiters)
        // `'static` delimiters and borrowed ones share one iterator only once
        // the built-ins are weakened to the shorter lifetime.
        .map(|(open, close)| -> (&'a str, &'a str) { (open, close) })
        .chain(
            rules
                .iter()
                .map(|rule| (rule.open.as_str(), rule.close.as_str())),
        )
        .filter(|(open, close)| !open.is_empty() && !close.is_empty())
        .collect()
}

/// Every prefix a configured style can put in front of a header line.
///
/// Both the opener and the per-line prefix, because a header line may carry
/// either. [`block_delimiters`] cannot serve here -- it filters on a non-empty
/// close, so line styles drop out of it -- and `is_comment_line` and
/// `strip_comment_prefix` only know the built-ins. Without these a style comply
/// just wrote is not recognised on the way back in, and a second run prepends
/// a second header above its own.
///
/// Longest first, so a prefix that is another's extension strips whole.
pub(super) fn configured_prefixes(rules: &[CommentStyleRule]) -> Vec<&str> {
    let mut prefixes: Vec<&str> = rules
        .iter()
        .flat_map(|rule| [rule.open.as_str(), rule.line.as_str()])
        .filter(|prefix| !prefix.trim().is_empty())
        .collect();
    prefixes.sort_unstable_by_key(|prefix| core::cmp::Reverse(prefix.len()));
    prefixes.dedup();
    prefixes
}

/// Detect the comment style for a file, consulting configured rules first.
///
/// A `[[tool.comply.comment-styles]]` rule wins over the built-in for the same
/// key, which is what lets a project retarget an extension without editing the
/// crate. Everything it does not claim keeps its built-in.
///
/// An exact `filenames` match beats an `extensions` match: a rule naming
/// `Jenkinsfile` is more specific than one claiming every extensionless file,
/// and the specific intent should not lose to the general one.
#[must_use]
pub fn detect_style_with(path: &str, rules: &[CommentStyleRule]) -> CommentStyle {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e).to_lowercase();

    let matched = rules
        .iter()
        .position(|r| r.filenames.iter().any(|f| f == name))
        .or_else(|| {
            (!ext.is_empty())
                .then(|| {
                    rules
                        .iter()
                        .position(|r| r.extensions.iter().any(|e| e.to_lowercase() == ext))
                })
                .flatten()
        });

    // `u16` is the index width `CommentStyle::Custom` carries, so a table
    // longer than that cannot be addressed. Falling back to the built-in beats
    // truncating into a wrong rule.
    matched
        .and_then(|i| u16::try_from(i).ok())
        .map_or_else(|| detect_style(path), CommentStyle::Custom)
}

/// The final path component, for either separator.
fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Detect the comment style for a file based on its name.
///
/// An exact name is consulted before the extension tables: `CMakeLists.txt` is
/// a build script rather than the unrecognised `.txt` its suffix suggests, and
/// `.gitignore` has no extension at all -- the leading dot is part of the name.
///
/// Anything no table claims is [`CommentStyle::Uncommentable`], not a guess.
/// Callers already treat that as "cannot write a header here" and offer
/// `--skip-unrecognised` or a `.license` companion, which is the reference
/// tool's contract. The previous `#` default silently wrote shell comments into
/// whatever it had not been taught -- `//` languages included.
#[must_use]
pub fn detect_style(path: &str) -> CommentStyle {
    let name = basename(path).to_lowercase();

    if let Some(style) = filename_style(&name) {
        return style;
    }

    let Some((_, ext)) = name.rsplit_once('.') else {
        return CommentStyle::Uncommentable;
    };

    extension_style(ext).unwrap_or(CommentStyle::Uncommentable)
}

/// The style for an exact (lowercased) file name, or `None` if none claims it.
fn filename_style(name: &str) -> Option<CommentStyle> {
    registry()
        .iter()
        .find(|(_, entry)| entry.filenames.iter().any(|f| f == name))
        .map(|(style, _)| *style)
}

/// The style for a (lowercased) extension, or `None` if none claims it.
fn extension_style(ext: &str) -> Option<CommentStyle> {
    registry()
        .iter()
        .find(|(_, entry)| entry.extensions.iter().any(|e| e == ext))
        .map(|(style, _)| *style)
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_python_file() {
        assert_eq!(detect_style("main.py"), CommentStyle::Hash);
    }

    #[test]
    fn style_html_file() {
        assert_eq!(detect_style("index.html"), CommentStyle::Html);
    }

    #[test]
    fn style_binary_file() {
        assert_eq!(detect_style("image.png"), CommentStyle::Uncommentable);
    }

    /// `.svg` used to sit in the `html` table while `file/classify.rs` called
    /// it `FileType::Binary`. That disagreement routed it past the companion
    /// branch and into the inline writer, which prepends at byte 0 -- above the
    /// `<?xml ...?>` declaration, where XML forbids anything at all. reuse
    /// 6.2.0 answers `UncommentableCommentStyle` here.
    #[test]
    fn an_svg_is_uncommentable_rather_than_html() {
        assert_eq!(detect_style("logo.svg"), CommentStyle::Uncommentable);
    }

    #[test]
    fn an_unknown_extension_is_refused_rather_than_guessed() {
        // `#` used to be the catch-all. That turned "no built-in claims this"
        // into a header written in the wrong syntax -- `.mts` got `#` and the
        // TypeScript stopped parsing. `Uncommentable` routes the same case into
        // the warn / --skip-unrecognised / --force-dot-license branch the CLI
        // already has for binaries, which is what the reference tool does.
        // `.cmd` used to live in this list; it now resolves to `Rem` alongside
        // `.bat`, see `a_deliberate_superset_of_reuse_gets_bang_and_rem`.
        for f in ["somefile.xyz", "app.astro", "theme.styl"] {
            assert_eq!(detect_style(f), CommentStyle::Uncommentable, "{f}");
        }
    }

    #[test]
    fn the_javascript_family_uses_slashes_whatever_the_module_suffix() {
        // The bug that prompted all of this: `ts` was listed, its ESM and
        // CommonJS spellings were not, so they fell through to the `#` default.
        for f in [
            "a.mts", "a.cts", "a.tsx", "a.jsx", "a.mjs", "a.cjs", "a.ts", "a.js",
        ] {
            assert_eq!(detect_style(f), CommentStyle::Slash, "{f}");
        }
    }

    #[test]
    fn an_extensionless_file_is_matched_by_name() {
        // `Dockerfile` and `Makefile` used to work only because the whole path
        // was searched for a dot and the miss landed on the `#` default. With
        // that default gone they need a table of their own.
        for f in [
            "Dockerfile",
            "Makefile",
            "Containerfile",
            "Gemfile",
            "Rakefile",
        ] {
            assert_eq!(detect_style(f), CommentStyle::Hash, "{f}");
        }
        assert_eq!(detect_style("Jenkinsfile"), CommentStyle::Slash);
    }

    #[test]
    fn a_dotfile_is_matched_by_name_not_by_a_phantom_extension() {
        // `.gitignore` has no extension: the leading dot is part of the name.
        // Splitting on it yields "gitignore", which no table claims.
        for f in [
            ".gitignore",
            ".gitattributes",
            ".editorconfig",
            ".dockerignore",
            ".yamllint",
            ".bashrc",
            ".zshrc",
        ] {
            assert_eq!(detect_style(f), CommentStyle::Hash, "{f}");
        }
    }

    #[test]
    fn a_name_match_reads_the_basename_not_the_whole_path() {
        // `detect_style` used to split the entire path on '.', so a dotted
        // directory decided the style: `.github/CODEOWNERS` resolved through
        // "github/codeowners". Every path below names a file the tables know.
        assert_eq!(detect_style(".github/CODEOWNERS"), CommentStyle::Hash);
        assert_eq!(
            detect_style("docs/.vitepress/config.mts"),
            CommentStyle::Slash
        );
        assert_eq!(detect_style("a.b/Makefile"), CommentStyle::Hash);
        assert_eq!(detect_style(r"win\path\main.rs"), CommentStyle::Slash);
    }

    #[test]
    fn a_name_with_no_extension_and_no_entry_is_refused() {
        // There is nothing left to consult once the name table misses and there
        // is no suffix to look up. These are the files a project is most likely
        // to point `annotate` at by accident, and `#` was the old answer.
        for f in ["LICENSE", "README", "CHANGELOG", "AUTHORS"] {
            assert_eq!(detect_style(f), CommentStyle::Uncommentable, "{f}");
        }
    }

    #[test]
    fn an_exact_name_beats_the_extension_tables() {
        // `CMakeLists.txt` is a build script, not the unrecognised `.txt` its
        // suffix suggests.
        assert_eq!(detect_style("CMakeLists.txt"), CommentStyle::Hash);
        assert_eq!(detect_style("notes.txt"), CommentStyle::Uncommentable);
    }

    #[test]
    fn the_extension_tables_agree_with_the_reference_tool() {
        // Captured from `reuse annotate` 6.2.0. Divergence here is what makes
        // the two tools rewrite each other's headers.
        let cases = [
            ("a.php", CommentStyle::Slash),
            ("a.proto", CommentStyle::Slash),
            ("a.gradle", CommentStyle::Slash),
            ("a.groovy", CommentStyle::Slash),
            ("a.kts", CommentStyle::Slash),
            ("a.tf", CommentStyle::Hash),
            ("a.hcl", CommentStyle::Hash),
            ("a.ps1", CommentStyle::Hash),
            ("a.psm1", CommentStyle::Hash),
            ("a.ex", CommentStyle::Hash),
            ("a.exs", CommentStyle::Hash),
            ("a.nix", CommentStyle::Hash),
            ("a.nim", CommentStyle::Hash),
            ("a.jl", CommentStyle::Hash),
            ("a.cmake", CommentStyle::Hash),
            ("a.bzl", CommentStyle::Hash),
            ("a.lua", CommentStyle::Dash),
            ("a.hs", CommentStyle::Dash),
            ("a.cabal", CommentStyle::Dash),
            ("a.rkt", CommentStyle::Semicolon),
            ("a.scm", CommentStyle::Semicolon),
            ("a.asm", CommentStyle::Semicolon),
            ("a.erl", CommentStyle::Percent),
            ("a.m", CommentStyle::Percent),
            ("a.vue", CommentStyle::Html),
            ("a.svelte", CommentStyle::Html),
        ];

        for (f, expected) in cases {
            assert_eq!(detect_style(f), expected, "{f}");
        }
    }

    #[test]
    fn a_configured_rule_still_claims_an_otherwise_unrecognised_extension() {
        // The escape hatch for anything the built-ins refuse: without this,
        // `Uncommentable` as the default would be a dead end rather than a
        // prompt to declare the style.
        let rules = [rule(&["astro"], &[], "//")];

        assert_eq!(
            detect_style_with("app.astro", &rules),
            CommentStyle::Custom(0)
        );
        assert_eq!(detect_style("app.astro"), CommentStyle::Uncommentable);
    }

    #[test]
    fn style_sql_file() {
        assert_eq!(detect_style("schema.sql"), CommentStyle::Dash);
    }

    #[test]
    fn style_clojure_file() {
        assert_eq!(detect_style("core.clj"), CommentStyle::Semicolon);
    }

    #[test]
    fn style_latex_file() {
        assert_eq!(detect_style("paper.tex"), CommentStyle::Percent);
    }

    #[test]
    fn style_ocaml_file() {
        assert_eq!(detect_style("lib.ml"), CommentStyle::AsteriskParen);
    }

    #[test]
    fn style_stylesheets_use_slash_star() {
        for f in ["main.css", "theme.scss", "vars.sass", "mixins.less"] {
            assert_eq!(detect_style(f), CommentStyle::SlashStar, "{f}");
        }
    }

    #[test]
    fn style_restructuredtext_uses_dot_dot() {
        assert_eq!(detect_style("guide.rst"), CommentStyle::DotDot);
    }

    #[test]
    fn style_fortran_uses_bang() {
        for f in ["a.f90", "a.f95", "a.f03", "a.f08"] {
            assert_eq!(detect_style(f), CommentStyle::Bang, "{f}");
        }
    }

    /// `.bat` matches `reuse` 6.2.0; `.cmd` is the deliberate superset -- see
    /// `docs/parity.md`, the same treatment `.mts`/`.cts` get on `slash`.
    #[test]
    fn a_deliberate_superset_of_reuse_gets_bang_and_rem() {
        for f in ["a.bat", "a.cmd"] {
            assert_eq!(detect_style(f), CommentStyle::Rem, "{f}");
        }
    }

    fn block_rule(open: &str, line: &str, close: &str) -> CommentStyleRule {
        CommentStyleRule {
            extensions: vec!["vue".to_string()],
            filenames: vec![],
            open: open.to_string(),
            line: line.to_string(),
            close: close.to_string(),
        }
    }

    #[test]
    fn a_configured_style_reads_its_delimiters_out_of_the_rules() {
        let rules = [block_rule("<%--", "  ", "--%>")];
        let resolved = ResolvedStyle::new(CommentStyle::Custom(0), &rules);

        assert_eq!(resolved.delimiters(), ("<%--", "--%>"));
        assert!(resolved.is_block());
        assert_eq!(resolved.lead(), "  ");
    }

    #[test]
    fn a_configured_line_style_leads_with_its_opener() {
        let rules = [block_rule("@@", "", "")];
        let resolved = ResolvedStyle::new(CommentStyle::Custom(0), &rules);

        assert_eq!(resolved.delimiters(), ("@@", ""));
        assert!(!resolved.is_block());
        assert_eq!(resolved.lead(), "@@ ");
    }

    #[test]
    fn an_index_past_the_end_yields_nothing_rather_than_a_guess() {
        // The manifest changed after the style was resolved. Writing a header
        // with guessed markers would be worse than writing none, and a panic
        // here would abort a run over a file that merely needs skipping.
        assert_eq!(
            ResolvedStyle::new(CommentStyle::Custom(7), &[]).delimiters(),
            ("", "")
        );
    }

    #[test]
    fn equality_compares_the_rules_a_custom_index_points_into() {
        // `Custom(0)` names a position in a table, not a syntax. Two styles
        // carrying different tables are different styles despite the equal
        // index, so a comparison that stopped at the index would call a `<!--`
        // file and a `#` file the same and write the wrong header into one.
        let html = [block_rule("<!--", "", "-->")];
        let hash = [block_rule("#", "", "")];

        assert_eq!(
            ResolvedStyle::new(CommentStyle::Custom(0), &html),
            ResolvedStyle::new(CommentStyle::Custom(0), &html)
        );
        assert_ne!(
            ResolvedStyle::new(CommentStyle::Custom(0), &html),
            ResolvedStyle::new(CommentStyle::Custom(0), &hash)
        );
    }

    #[test]
    fn the_debug_form_names_the_syntax_an_index_alone_cannot() {
        // A failing assertion that printed only `Custom(0)` would say nothing
        // about which syntax resolved, so the rules have to travel with it.
        let rules = [block_rule("<%--", "  ", "--%>")];

        let shown = format!("{:?}", ResolvedStyle::new(CommentStyle::Custom(0), &rules));

        assert!(shown.contains("<%--"), "got: {shown}");
    }

    #[test]
    fn builtins_answer_the_same_with_rules_as_without() {
        // The rules are consulted only for `Custom`; a built-in must not change
        // meaning because a project happens to configure a style.
        let rules = [block_rule("<%--", "  ", "--%>")];

        for style in BUILTIN_STYLES {
            assert_eq!(
                ResolvedStyle::new(style, &rules).delimiters(),
                comment_delimiters(style),
                "{style:?} must ignore the rules"
            );
            assert_eq!(
                ResolvedStyle::new(style, &rules).lead(),
                ResolvedStyle::builtin(style).lead(),
                "{style:?} lead must not depend on the rules"
            );
        }
    }

    #[test]
    fn a_configured_block_style_is_recognised_as_one() {
        // `comment_mask` reads this list to tell a header from bare tags. A
        // configured block style absent from it would be written and then not
        // read back.
        let rules = [block_rule("<%--", "  ", "--%>")];

        assert!(
            block_delimiters(&rules).contains(&("<%--", "--%>")),
            "configured block styles must join the built-ins"
        );
    }

    fn rule(exts: &[&str], files: &[&str], open: &str) -> CommentStyleRule {
        CommentStyleRule {
            extensions: exts.iter().map(|s| (*s).to_string()).collect(),
            filenames: files.iter().map(|s| (*s).to_string()).collect(),
            open: open.to_string(),
            line: String::new(),
            close: String::new(),
        }
    }

    #[test]
    fn a_configured_extension_wins_over_the_builtin() {
        // `md` is `Html` built in. A project that wants `//` there must get it,
        // or the config is decoration.
        let rules = [rule(&["md"], &[], "//")];

        assert_eq!(
            detect_style_with("notes.md", &rules),
            CommentStyle::Custom(0)
        );
        assert_eq!(
            detect_style("notes.md"),
            CommentStyle::Html,
            "builtin intact"
        );
    }

    #[test]
    fn anything_unclaimed_keeps_its_builtin() {
        let rules = [rule(&["vue"], &[], "<!--")];

        assert_eq!(detect_style_with("main.rs", &rules), CommentStyle::Slash);
    }

    #[test]
    fn an_exact_filename_beats_an_extension_rule() {
        // The specific intent must not lose to the general one, whichever order
        // they happen to appear in.
        let rules = [rule(&["txt"], &[], "#"), rule(&[], &["NOTICE.txt"], "//")];

        assert_eq!(
            detect_style_with("NOTICE.txt", &rules),
            CommentStyle::Custom(1)
        );
        assert_eq!(
            detect_style_with("other.txt", &rules),
            CommentStyle::Custom(0)
        );
    }

    #[test]
    fn matching_is_case_insensitive_on_the_extension() {
        let rules = [rule(&["VUE"], &[], "<!--")];

        assert_eq!(
            detect_style_with("App.vue", &rules),
            CommentStyle::Custom(0)
        );
    }

    #[test]
    fn no_rules_is_exactly_the_builtin_behaviour() {
        for f in ["a.rs", "b.py", "c.md", "d.png", "unknown.xyz"] {
            assert_eq!(detect_style_with(f, &[]), detect_style(f), "{f}");
        }
    }

    #[test]
    fn block_delimiters_are_derived_not_restated() {
        // `header_span` used to hardcode this set. Deriving it means a style
        // that gains a block delimiter is recognised on the way back in, not
        // just written correctly on the way out.
        let mut pairs = block_delimiters(&[]);
        pairs.sort_unstable();

        assert_eq!(pairs, vec![("(*", "*)"), ("/*", "*/"), ("<!--", "-->")]);
    }

    #[test]
    fn only_bracketing_styles_appear() {
        // Line styles have an empty close and must not leak in -- treating `//`
        // as opening a block would swallow the rest of the file.
        for (open, close) in block_delimiters(&[]) {
            assert!(!open.is_empty() && !close.is_empty(), "{open:?}/{close:?}");
        }
    }

    #[test]
    fn every_builtin_style_has_a_resource_entry() {
        // The data-side analogue of `assert_style_is_listed`: a style listed
        // in `BUILTIN_STYLES` but missing from `comment-styles.toml` would
        // silently fall back to empty delimiters instead of failing to build.
        for style in BUILTIN_STYLES {
            assert!(
                registry().iter().any(|(s, _)| *s == style),
                "{style:?} is missing from comment-styles.toml"
            );
        }
    }

    /// Precedence used to be an explicit cascade in `extension_style`
    /// (uncommentable, then slash, then hash, ...). It is now "the first entry
    /// the registry yields", which is alphabetical by table name -- so a name
    /// listed twice silently goes to whichever table sorts first, and the loser
    /// is a style a project asked for and did not get.
    #[test]
    fn no_name_is_claimed_by_two_tables() {
        let mut extensions: BTreeMap<&str, CommentStyle> = BTreeMap::new();
        let mut filenames: BTreeMap<&str, CommentStyle> = BTreeMap::new();

        for (style, entry) in registry() {
            for ext in &entry.extensions {
                let prior = extensions.insert(ext.as_str(), *style);
                assert!(prior.is_none(), "{ext}: {prior:?} and {style:?}");
            }
            for name in &entry.filenames {
                let prior = filenames.insert(name.as_str(), *style);
                assert!(prior.is_none(), "{name}: {prior:?} and {style:?}");
            }
        }
    }

    /// `detect_style` splits the basename on its last dot, so the extension it
    /// looks up is never more than one component. An entry carrying a dot can
    /// therefore match nothing at all, and only misleads the next reader.
    #[test]
    fn no_extension_entry_carries_a_dot() {
        assert_eq!(detect_style("archive.tar.gz"), CommentStyle::Uncommentable);
        for (style, entry) in registry() {
            for ext in &entry.extensions {
                assert!(!ext.contains('.'), "{style:?} lists an unreachable {ext}");
            }
        }
    }

    #[test]
    fn every_resource_key_resolves_to_a_known_style() {
        // `parse_registry` rejects an unrecognised key outright (see
        // `an_unknown_key_is_rejected`), so a registry that parsed at all
        // means every key already resolved; this pins the count down too.
        assert_eq!(registry().len(), BUILTIN_STYLES.len());
    }

    #[test]
    fn the_embedded_file_parses_and_validates() {
        assert!(parse_registry(COMMENT_STYLES_TOML).is_ok());
    }

    #[test]
    fn an_unknown_key_is_rejected() {
        let bad = "[not-a-style]\nkind = \"line\"\nopen = \"@\"\n";
        assert!(parse_registry(bad).is_err());
    }

    #[test]
    fn a_line_style_with_a_close_is_rejected() {
        let bad = "[slash]\nkind = \"line\"\nopen = \"//\"\nclose = \"x\"\n";
        assert!(parse_registry(bad).is_err());
    }

    #[test]
    fn a_block_style_missing_open_is_rejected() {
        let bad = "[html]\nkind = \"block\"\nclose = \"-->\"\n";
        assert!(parse_registry(bad).is_err());
    }

    #[test]
    fn a_block_style_missing_close_is_rejected() {
        let bad = "[html]\nkind = \"block\"\nopen = \"<!--\"\n";
        assert!(parse_registry(bad).is_err());
    }

    #[test]
    fn a_none_style_with_delimiters_is_rejected() {
        let bad = "[uncommentable]\nkind = \"none\"\nopen = \"x\"\n";
        assert!(parse_registry(bad).is_err());
    }

    #[test]
    fn a_table_missing_kind_is_rejected() {
        let bad = "[slash]\nopen = \"//\"\n";
        assert!(parse_registry(bad).is_err());
    }

    /// `line` used to be absent from `RawStyleEntry`, and serde drops what it
    /// does not know, so the value in the file was inert while `lead()`
    /// hardcoded `" * "`. Reading it is what makes a new block style a data
    /// edit rather than a Rust edit.
    #[test]
    fn a_block_style_declares_its_inner_prefix_in_the_file() {
        let text = "[slash-star]\nkind = \"block\"\nopen = \"/*\"\nclose = \"*/\"\nline = \"|\"\n";
        let parsed = parse_registry(text).expect("a block style may declare its lead");

        assert_eq!(parsed[0].1.line, "|");
    }

    /// Only a block style reads `line`. Accepting it elsewhere reintroduces
    /// exactly the silence this table was meant to remove.
    #[test]
    fn a_line_prefix_outside_a_block_style_is_rejected() {
        for bad in [
            "[slash]\nkind = \"line\"\nopen = \"//\"\nline = \"|\"\n",
            "[uncommentable]\nkind = \"none\"\nline = \"|\"\n",
        ] {
            assert!(parse_registry(bad).is_err(), "{bad}");
        }
    }

    /// A mistyped key used to deserialize to a table with no extensions and no
    /// error, which silently unclaims every file the table was there to claim.
    #[test]
    fn a_mistyped_key_is_rejected_rather_than_emptying_the_table() {
        let bad = "[slash]\nkind = \"line\"\nopen = \"//\"\nextenions = [\"rs\"]\n";
        assert!(parse_registry(bad).is_err());
    }

    #[test]
    fn malformed_toml_syntax_is_rejected() {
        let bad = "not even toml {{{";
        assert!(parse_registry(bad).is_err());
    }

    #[test]
    #[should_panic(expected = "comment-styles.toml is invalid")]
    fn a_malformed_registry_panics_rather_than_silently_degrading() {
        // The embedded file itself is proven valid by
        // `the_embedded_file_parses_and_validates`, so this branch never
        // fires for a user -- only for a contributor who broke the file,
        // which is exactly when a build-time panic beats a runtime one.
        load_registry("not even toml {{{");
    }

    #[test]
    fn every_style_has_delimiters() {
        assert_eq!(comment_delimiters(CommentStyle::Slash), ("//", ""));
        assert_eq!(comment_delimiters(CommentStyle::Hash), ("#", ""));
        assert_eq!(comment_delimiters(CommentStyle::Dash), ("--", ""));
        assert_eq!(comment_delimiters(CommentStyle::Semicolon), (";;;", ""));
        assert_eq!(comment_delimiters(CommentStyle::Percent), ("%", ""));
        assert_eq!(
            comment_delimiters(CommentStyle::AsteriskParen),
            ("(*", "*)")
        );
        assert_eq!(comment_delimiters(CommentStyle::SlashStar), ("/*", "*/"));
        assert_eq!(comment_delimiters(CommentStyle::DotDot), ("..", ""));
        assert_eq!(comment_delimiters(CommentStyle::Bang), ("!", ""));
        assert_eq!(comment_delimiters(CommentStyle::Rem), ("REM", ""));
        assert_eq!(comment_delimiters(CommentStyle::Html), ("<!--", "-->"));
        assert_eq!(
            comment_delimiters(CommentStyle::Uncommentable),
            ("", ""),
            "an uncommentable file has no delimiters to wrap a header in"
        );
    }

    /// Captured from `reuse annotate` 6.2.0: `o.f90` and `p.bat` each got a
    /// header whose first line is exactly this text. Byte-for-byte is what
    /// stops the two tools rewriting each other's headers.
    #[test]
    fn bang_and_rem_render_bytes_matching_the_reference_tool() {
        let cases = [
            (
                CommentStyle::Bang,
                "! SPDX-FileCopyrightText: 2026 X\n!\n! SPDX-License-Identifier: MIT\n",
            ),
            (
                CommentStyle::Rem,
                "REM SPDX-FileCopyrightText: 2026 X\nREM\nREM SPDX-License-Identifier: MIT\n",
            ),
        ];

        for (style, expected) in cases {
            let result = crate::header::generate("2026 X", "MIT", style).unwrap();
            assert_eq!(result, expected, "{style:?}");
        }
    }
}
// REUSE-IgnoreEnd
