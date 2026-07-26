// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::CommentStyle;

/// Return the comment prefix and suffix for a style.
pub(super) const fn comment_delimiters(style: CommentStyle) -> (&'static str, &'static str) {
    match style {
        CommentStyle::Slash => ("//", ""),
        CommentStyle::Hash => ("#", ""),
        CommentStyle::Dash => ("--", ""),
        // `;;;` is the Lisp-family convention for a file-level comment, and is
        // what the reference tool emits.
        CommentStyle::Semicolon => (";;;", ""),
        CommentStyle::Percent => ("%", ""),
        CommentStyle::AsteriskParen => ("(*", "*)"),
        CommentStyle::SlashStar => ("/*", "*/"),
        CommentStyle::DotDot => ("..", ""),
        CommentStyle::Html => ("<!--", "-->"),
        CommentStyle::Uncommentable => ("", ""),
    }
}

/// Detect the comment style for a file based on its extension.
#[must_use]
pub fn detect_style(path: &str) -> CommentStyle {
    let lower = path.to_lowercase();

    // Extract extension (without dot)
    let ext = lower.split('.').next_back().unwrap_or("").to_string();

    // Uncommentable binary formats
    if is_uncommentable_extension(&ext) {
        return CommentStyle::Uncommentable;
    }

    // Slash-style comments (//)
    if is_slash_comment_extension(&ext) {
        return CommentStyle::Slash;
    }

    // Hash-style comments (#)
    if is_hash_comment_extension(&ext) {
        return CommentStyle::Hash;
    }

    // Dash-style comments (--)
    if is_dash_comment_extension(&ext) {
        return CommentStyle::Dash;
    }

    // Semicolon-style comments (;)
    if is_semicolon_comment_extension(&ext) {
        return CommentStyle::Semicolon;
    }

    // Percent-style comments (%)
    if is_percent_comment_extension(&ext) {
        return CommentStyle::Percent;
    }

    // HTML-style comments (<!-- -->)
    if is_html_comment_extension(&ext) {
        return CommentStyle::Html;
    }

    // Stylesheet block comments (/* */)
    if is_slash_star_extension(&ext) {
        return CommentStyle::SlashStar;
    }

    // reStructuredText comments (..)
    if ext == "rst" {
        return CommentStyle::DotDot;
    }

    // AsteriskParen comments (ML-like)
    if is_asterisk_paren_extension(&ext) {
        return CommentStyle::AsteriskParen;
    }

    // Default to hash for unknown text-like files
    CommentStyle::Hash
}

/// Check if the extension corresponds to a slash-style comment language (//).
fn is_slash_comment_extension(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "java"
            | "kt"
            | "js"
            | "ts"
            | "go"
            | "swift"
            | "scala"
            | "zig"
            | "dart"
            | "cs"
            | "cc"
            | "cxx"
            | "hxx"
    )
}

/// Check if the extension corresponds to a hash-style comment language (#).
fn is_hash_comment_extension(ext: &str) -> bool {
    matches!(
        ext,
        "py" | "sh"
            | "bash"
            | "zsh"
            | "yaml"
            | "yml"
            | "toml"
            | "cfg"
            | "ini"
            | "rb"
            | "pl"
            | "pm"
            | "r"
            | "dockerfile"
            | "makefile"
    )
}

/// Check if the extension corresponds to a dash-style comment language (--).
fn is_dash_comment_extension(ext: &str) -> bool {
    matches!(ext, "sql" | "ada")
}

/// Check if the extension corresponds to a semicolon-style comment language (;).
fn is_semicolon_comment_extension(ext: &str) -> bool {
    matches!(ext, "lisp" | "clj" | "cljs")
}

/// Check if the extension corresponds to a percent-style comment language (%).
fn is_percent_comment_extension(ext: &str) -> bool {
    matches!(ext, "tex" | "sty" | "cls")
}

/// Check if the extension corresponds to HTML-style comments (<!-- -->).
fn is_html_comment_extension(ext: &str) -> bool {
    // Markdown belongs here: the reference tool wraps its headers in `<!-- -->`
    // too, and a `#` prefix would render as a heading.
    matches!(
        ext,
        "html" | "htm" | "xml" | "xhtml" | "svg" | "md" | "markdown"
    )
}

/// Check if the extension corresponds to stylesheet block comments (`/* */`).
fn is_slash_star_extension(ext: &str) -> bool {
    matches!(ext, "css" | "scss" | "sass" | "less")
}

/// Check if the extension corresponds to ML-style comments ((* *)).
fn is_asterisk_paren_extension(ext: &str) -> bool {
    matches!(ext, "ml" | "mli" | "pas" | "pp")
}

/// Check if the extension is a binary/uncommentable format.
fn is_uncommentable_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "ico"
            | "pdf"
            | "zip"
            | "tar"
            | "gz"
            | "tar.gz"
            | "o"
            | "so"
            | "dll"
            | "exe"
            | "bin"
            | "class"
            | "jar"
            | "ttf"
            | "otf"
            | "woff"
            | "woff2"
    )
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

    #[test]
    fn style_unknown_defaults_to_hash() {
        assert_eq!(detect_style("somefile.xyz"), CommentStyle::Hash);
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
        assert_eq!(comment_delimiters(CommentStyle::Html), ("<!--", "-->"));
        assert_eq!(
            comment_delimiters(CommentStyle::Uncommentable),
            ("", ""),
            "an uncommentable file has no delimiters to wrap a header in"
        );
    }
}
// REUSE-IgnoreEnd
