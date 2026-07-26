// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod annotate;
mod generate;
mod parse;
mod style;

pub use annotate::{annotate_file, annotate_file_with, format_file};
pub use generate::{generate, generate_with};
pub use parse::detect;
pub use style::detect_style;

/// A parsed SPDX header from a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Copyright lines (each `SPDX-FileCopyrightText` value).
    pub copyrights: Vec<String>,
    /// The SPDX license expression string.
    pub license_expression: Option<String>,
    /// Contributor names (each `SPDX-FileContributor` value).
    pub contributors: Vec<String>,
}

/// Comment syntax for a file type.
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
    /// `<!-- -->` block comments (HTML, XML, etc.)
    Html,
    /// No comment syntax (binary, data files - uses `.license` companion)
    Uncommentable,
}
