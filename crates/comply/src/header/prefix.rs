// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::str::FromStr;

use crate::error::Error;

/// How a copyright notice is introduced in a header.
///
/// REUSE 3.3 section 2.1.1 admits three tags -- the file tag, the snippet tag,
/// and no tag at all -- each combinable with a marker (`(C)`, the symbol, the
/// word `Copyright`, or a pair of them). The legal combinations are enumerated
/// rather than composed from a (tag, marker) pair, because the bare forms admit
/// only four of the six markers: a bare `(C) 2026 Acme` carries nothing that
/// identifies it as a copyright notice, so neither this nor the reference tool
/// will write one.
// REUSE-IgnoreStart
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CopyrightPrefix {
    /// `SPDX-FileCopyrightText: 2026 Acme`
    #[default]
    Spdx,
    /// `SPDX-FileCopyrightText: (C) 2026 Acme`
    SpdxC,
    /// `SPDX-FileCopyrightText: \u{a9} 2026 Acme`
    SpdxSymbol,
    /// `SPDX-FileCopyrightText: Copyright 2026 Acme`
    SpdxString,
    /// `SPDX-FileCopyrightText: Copyright (C) 2026 Acme`
    SpdxStringC,
    /// `SPDX-FileCopyrightText: Copyright \u{a9} 2026 Acme`
    SpdxStringSymbol,
    /// `SPDX-SnippetCopyrightText: 2026 Acme`
    Snippet,
    /// `SPDX-SnippetCopyrightText: (C) 2026 Acme`
    SnippetC,
    /// `SPDX-SnippetCopyrightText: \u{a9} 2026 Acme`
    SnippetSymbol,
    /// `SPDX-SnippetCopyrightText: Copyright 2026 Acme`
    SnippetString,
    /// `SPDX-SnippetCopyrightText: Copyright (C) 2026 Acme`
    SnippetStringC,
    /// `SPDX-SnippetCopyrightText: Copyright \u{a9} 2026 Acme`
    SnippetStringSymbol,
    /// `Copyright 2026 Acme`
    String,
    /// `Copyright (C) 2026 Acme`
    StringC,
    /// `Copyright \u{a9} 2026 Acme`
    StringSymbol,
    /// `\u{a9} 2026 Acme`
    Symbol,
}
// REUSE-IgnoreEnd

impl CopyrightPrefix {
    /// Every accepted spelling, in the reference tool's order.
    ///
    /// The CLI builds its `--copyright-prefix` value list from this, so the
    /// accepted set and the parser cannot drift apart.
    pub const VALUES: [&'static str; 16] = [
        "spdx",
        "spdx-c",
        "spdx-symbol",
        "spdx-string",
        "spdx-string-c",
        "spdx-string-symbol",
        "snippet",
        "snippet-c",
        "snippet-symbol",
        "snippet-string",
        "snippet-string-c",
        "snippet-string-symbol",
        "string",
        "string-c",
        "string-symbol",
        "symbol",
    ];

    /// The literal text a notice opens with, up to and including its separator.
    ///
    /// Emitted verbatim before the copyright text, so the whole rendering
    /// difference between the sixteen forms lives in this one table.
    // REUSE-IgnoreStart
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Spdx => "SPDX-FileCopyrightText: ",
            Self::SpdxC => "SPDX-FileCopyrightText: (C) ",
            Self::SpdxSymbol => "SPDX-FileCopyrightText: \u{a9} ",
            Self::SpdxString => "SPDX-FileCopyrightText: Copyright ",
            Self::SpdxStringC => "SPDX-FileCopyrightText: Copyright (C) ",
            Self::SpdxStringSymbol => "SPDX-FileCopyrightText: Copyright \u{a9} ",
            Self::Snippet => "SPDX-SnippetCopyrightText: ",
            Self::SnippetC => "SPDX-SnippetCopyrightText: (C) ",
            Self::SnippetSymbol => "SPDX-SnippetCopyrightText: \u{a9} ",
            Self::SnippetString => "SPDX-SnippetCopyrightText: Copyright ",
            Self::SnippetStringC => "SPDX-SnippetCopyrightText: Copyright (C) ",
            Self::SnippetStringSymbol => "SPDX-SnippetCopyrightText: Copyright \u{a9} ",
            Self::String => "Copyright ",
            Self::StringC => "Copyright (C) ",
            Self::StringSymbol => "Copyright \u{a9} ",
            Self::Symbol => "\u{a9} ",
        }
    }
    // REUSE-IgnoreEnd
}

/// A copyright notice: its text, and the form it is written in.
///
/// Pairing the two is what keeps the prefix from being dropped on the way to
/// the writer. The same bundling that [`super::ResolvedStyle`] does for comment
/// syntax, and for the same reason -- `annotate_file_with` is already at
/// clippy's argument ceiling, so a sixteen-way rendering choice cannot travel
/// as a parameter of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyrightNotice<'a> {
    text: &'a str,
    prefix: CopyrightPrefix,
}

impl<'a> CopyrightNotice<'a> {
    /// A notice in the default form, `SPDX-FileCopyrightText`.
    #[must_use]
    pub const fn new(text: &'a str) -> Self {
        Self {
            text,
            prefix: CopyrightPrefix::Spdx,
        }
    }

    /// The same notice, written in `prefix`'s form.
    #[must_use]
    pub const fn with_prefix(self, prefix: CopyrightPrefix) -> Self {
        Self { prefix, ..self }
    }

    /// The same notice with different text, keeping the form.
    #[must_use]
    pub const fn with_text(self, text: &'a str) -> Self {
        Self { text, ..self }
    }

    /// The notice text, without its prefix.
    #[must_use]
    pub const fn text(self) -> &'a str {
        self.text
    }

    /// The form the notice is written in.
    #[must_use]
    pub const fn prefix(self) -> CopyrightPrefix {
        self.prefix
    }
}

impl<'a> From<&'a str> for CopyrightNotice<'a> {
    fn from(text: &'a str) -> Self {
        Self::new(text)
    }
}

impl FromStr for CopyrightPrefix {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "spdx" => Ok(Self::Spdx),
            "spdx-c" => Ok(Self::SpdxC),
            "spdx-symbol" => Ok(Self::SpdxSymbol),
            "spdx-string" => Ok(Self::SpdxString),
            "spdx-string-c" => Ok(Self::SpdxStringC),
            "spdx-string-symbol" => Ok(Self::SpdxStringSymbol),
            "snippet" => Ok(Self::Snippet),
            "snippet-c" => Ok(Self::SnippetC),
            "snippet-symbol" => Ok(Self::SnippetSymbol),
            "snippet-string" => Ok(Self::SnippetString),
            "snippet-string-c" => Ok(Self::SnippetStringC),
            "snippet-string-symbol" => Ok(Self::SnippetStringSymbol),
            "string" => Ok(Self::String),
            "string-c" => Ok(Self::StringC),
            "string-symbol" => Ok(Self::StringSymbol),
            "symbol" => Ok(Self::Symbol),
            other => Err(Error::InvalidCopyrightPrefix(other.to_string())),
        }
    }
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    /// The rendering of every accepted value, captured from `reuse 6.2.0`:
    ///
    /// ```text
    /// reuse annotate -c "Jane Doe" -l MIT -y 2026 --copyright-prefix <value>
    /// ```
    const ORACLE: [(&str, &str); 16] = [
        ("spdx", "SPDX-FileCopyrightText: 2026 Jane Doe"),
        ("spdx-c", "SPDX-FileCopyrightText: (C) 2026 Jane Doe"),
        (
            "spdx-symbol",
            "SPDX-FileCopyrightText: \u{a9} 2026 Jane Doe",
        ),
        (
            "spdx-string",
            "SPDX-FileCopyrightText: Copyright 2026 Jane Doe",
        ),
        (
            "spdx-string-c",
            "SPDX-FileCopyrightText: Copyright (C) 2026 Jane Doe",
        ),
        (
            "spdx-string-symbol",
            "SPDX-FileCopyrightText: Copyright \u{a9} 2026 Jane Doe",
        ),
        ("snippet", "SPDX-SnippetCopyrightText: 2026 Jane Doe"),
        ("snippet-c", "SPDX-SnippetCopyrightText: (C) 2026 Jane Doe"),
        (
            "snippet-symbol",
            "SPDX-SnippetCopyrightText: \u{a9} 2026 Jane Doe",
        ),
        (
            "snippet-string",
            "SPDX-SnippetCopyrightText: Copyright 2026 Jane Doe",
        ),
        (
            "snippet-string-c",
            "SPDX-SnippetCopyrightText: Copyright (C) 2026 Jane Doe",
        ),
        (
            "snippet-string-symbol",
            "SPDX-SnippetCopyrightText: Copyright \u{a9} 2026 Jane Doe",
        ),
        ("string", "Copyright 2026 Jane Doe"),
        ("string-c", "Copyright (C) 2026 Jane Doe"),
        ("string-symbol", "Copyright \u{a9} 2026 Jane Doe"),
        ("symbol", "\u{a9} 2026 Jane Doe"),
    ];

    #[test]
    fn every_value_renders_what_the_reference_tool_renders() {
        for (value, expected) in ORACLE {
            let prefix: CopyrightPrefix = value.parse().unwrap();
            assert_eq!(
                format!("{}{}", prefix.tag(), "2026 Jane Doe"),
                expected,
                "`--copyright-prefix {value}` must match reuse byte for byte"
            );
        }
    }

    #[test]
    fn the_accepted_values_are_exactly_those_the_parser_takes() {
        assert_eq!(
            CopyrightPrefix::VALUES.len(),
            ORACLE.len(),
            "VALUES drives the CLI's accepted set; it must list every form"
        );
        for value in CopyrightPrefix::VALUES {
            assert!(
                value.parse::<CopyrightPrefix>().is_ok(),
                "`{value}` is offered by the CLI but the parser rejects it"
            );
        }
    }

    #[test]
    fn an_unknown_value_is_rejected_rather_than_ignored() {
        let err = "spdx-c-symbol".parse::<CopyrightPrefix>().unwrap_err();
        assert!(
            err.to_string().contains("spdx-c-symbol"),
            "the message must name the offending value, got: {err}"
        );
    }

    #[test]
    fn the_default_is_the_plain_file_tag() {
        assert_eq!(CopyrightPrefix::default(), CopyrightPrefix::Spdx);
    }
}
// REUSE-IgnoreEnd
