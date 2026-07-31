// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `comply version` -- the version, and optionally how the binary was built.

use anyhow::Result;
use clap::ValueEnum;
use comply::version::{BuildInfo, Format};

/// The `-o/--output` choices.
///
/// A CLI-side mirror of [`Format`] rather than a `ValueEnum` derive on the
/// library type: that would put an argument parser in a dependency of every
/// surface crate, including the WASM one that has no command line at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Output {
    /// Human-readable lines.
    Plain,
    /// A JSON object.
    Json,
    /// A TOML table.
    Toml,
}

impl Output {
    // Deliberately not a `const fn`: literal arguments to one get folded at
    // compile time, so the arms record no runtime coverage. `output.rs` carries
    // the same note for the same reason.
    #[allow(clippy::missing_const_for_fn)]
    fn format(self) -> Format {
        match self {
            Self::Plain => Format::Plain,
            Self::Json => Format::Json,
            Self::Toml => Format::Toml,
        }
    }
}

pub(crate) fn run(full: bool, output: Output) -> Result<()> {
    println!("{}", BuildInfo::current().render(output.format(), full)?);
    Ok(())
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_choice_maps_to_its_library_format() {
        assert_eq!(Output::Plain.format(), Format::Plain);
        assert_eq!(Output::Json.format(), Format::Json);
        assert_eq!(Output::Toml.format(), Format::Toml);
    }

    #[test]
    fn every_combination_of_format_and_detail_runs() {
        for output in [Output::Plain, Output::Json, Output::Toml] {
            assert!(run(false, output).is_ok(), "{output:?} short");
            assert!(run(true, output).is_ok(), "{output:?} full");
        }
    }
}
// REUSE-IgnoreEnd
