// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reading, generating and writing SPDX headers.

mod annotate;
mod generate;
mod parse;
mod prefix;
mod style;

pub use annotate::{annotate_file, annotate_file_with, format_file};
pub use generate::{generate, generate_with};
pub use parse::detect;
pub use prefix::{CopyrightNotice, CopyrightPrefix};
pub use style::{ResolvedStyle, detect_style, detect_style_with};

/// A parsed SPDX header from a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Copyright lines (each `SPDX-FileCopyrightText` value).
    pub copyrights: Vec<String>,
    /// The SPDX license expression string, built only from tags that parse.
    pub license_expression: Option<String>,
    /// Tag values that did not parse as SPDX expressions.
    ///
    /// Kept apart from [`Self::license_expression`] rather than merged into it:
    /// a fragment picked up from a test fixture or a documentation sample would
    /// otherwise make the whole file unlicensed, discarding the real header it
    /// carries. Reported on its own, which is what the reference tool does.
    pub invalid_expressions: Vec<String>,
    /// Contributor names (each `SPDX-FileContributor` value).
    pub contributors: Vec<String>,
}

/// Comment syntax for a file type.
///
/// [`Self::Custom`] holds an *index* rather than the delimiters themselves,
/// which is what keeps this `Copy`. Owning the strings would drop `Copy` and
/// ripple through every one of the ~114 places a style is passed around; an
/// index leaves all of them untouched, and only the delimiter lookup needs to
/// see the table it points into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStyle {
    /// `//` line comments (Rust, C++, etc.)
    Slash,
    /// `#` line comments (Python, shell, etc.)
    Hash,
    /// `--` line comments (SQL, Ada, etc.)
    Dash,
    /// `;` line comments (Lisp, etc.)
    Semicolon,
    /// `%` line comments (TeX, etc.)
    Percent,
    /// `(* *)` block comments (Pascal, etc.)
    AsteriskParen,
    /// `/* */` block comments with a leading `*` per line (CSS, Sass, Less)
    SlashStar,
    /// `..` line comments (reStructuredText)
    DotDot,
    /// `!` line comments (Fortran)
    Bang,
    /// `REM` line comments (Windows batch)
    Rem,
    /// `<!-- -->` block comments (HTML, XML, etc.)
    Html,
    /// No comment syntax (binary, data files - uses `.license` companion)
    Uncommentable,
    /// A style declared in `[[tool.comply.comment-styles]]`, identified by its
    /// position in that table.
    ///
    /// The delimiters live in the table, not here, so resolving one needs the
    /// config -- see `comment_delimiters_in`. Unresolvable indices are treated
    /// as having no delimiters rather than panicking: a stale index means the
    /// manifest changed under us, which is a reason to annotate nothing, not to
    /// abort the run.
    Custom(u16),
}
