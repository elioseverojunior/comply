// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::Result;
use comply::spdx;
use serde::Serialize;

/// Which of the bundled lists to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Licenses,
    Exceptions,
    Deprecated,
}

impl Kind {
    const fn from_flags(exceptions: bool, deprecated: bool) -> Self {
        match (exceptions, deprecated) {
            (true, _) => Self::Exceptions,
            (_, true) => Self::Deprecated,
            _ => Self::Licenses,
        }
    }

    const fn ids(self) -> &'static [&'static str] {
        match self {
            Self::Licenses => spdx::known_licenses(),
            Self::Exceptions => spdx::known_exceptions(),
            Self::Deprecated => spdx::deprecated_licenses(),
        }
    }
}

/// Every list at once, so a caller need not run the command three times.
#[derive(Serialize)]
struct AllLists {
    licenses: &'static [&'static str],
    exceptions: &'static [&'static str],
    deprecated: &'static [&'static str],
}

pub(crate) fn run(json: bool, exceptions: bool, deprecated: bool) -> Result<()> {
    if json {
        // JSON carries all three regardless of the selector flags: a machine
        // reader wants the whole list set in one call.
        let all = AllLists {
            licenses: spdx::known_licenses(),
            exceptions: spdx::known_exceptions(),
            deprecated: spdx::deprecated_licenses(),
        };
        println!("{}", serde_json::to_string_pretty(&all)?);
        return Ok(());
    }

    for id in Kind::from_flags(exceptions, deprecated).ids() {
        println!("{id}");
    }
    Ok(())
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    /// `from_flags` is a `const fn`, so literal arguments get folded at compile
    /// time and the match arms record no runtime coverage. `black_box` hides the
    /// values from the optimiser so the call actually executes.
    fn from_flags(exceptions: bool, deprecated: bool) -> Kind {
        Kind::from_flags(
            std::hint::black_box(exceptions),
            std::hint::black_box(deprecated),
        )
    }

    #[test]
    fn the_default_selection_is_licences() {
        assert_eq!(from_flags(false, false), Kind::Licenses);
    }

    #[test]
    fn deprecated_is_selected_on_its_own() {
        assert_eq!(from_flags(false, true), Kind::Deprecated);
    }

    #[test]
    fn exceptions_wins_over_deprecated_when_both_are_given() {
        // Both flags set is ambiguous rather than an error; exceptions is the
        // narrower list, so it is the more likely intent.
        assert_eq!(from_flags(true, true), Kind::Exceptions);
    }

    #[test]
    fn each_selection_reads_the_matching_bundled_list() {
        assert!(Kind::Licenses.ids().contains(&"MIT"));
        assert!(Kind::Exceptions.ids().contains(&"Classpath-exception-2.0"));
        assert!(Kind::Deprecated.ids().contains(&"GPL-3.0"));
        assert_eq!(Kind::Deprecated.ids(), spdx::deprecated_licenses());
    }

    #[test]
    fn every_mode_runs_without_error() {
        assert!(run(false, false, false).is_ok());
        assert!(run(false, true, false).is_ok());
        assert!(run(false, false, true).is_ok());
        assert!(run(true, false, false).is_ok());
    }
}
// REUSE-IgnoreEnd
