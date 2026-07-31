// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Browser bindings for `comply`.
//!
//! Every function here is a thin adapter: it converts JavaScript strings into
//! the shapes `comply` already takes and converts the results back. No
//! compliance logic lives in this crate, because `wasm32-unknown-unknown` code
//! never executes under the host test harness -- logic placed here would be
//! unreachable by the coverage gate, and untested by construction.
//!
//! `lint_project` from the plan is deliberately absent: project linting walks
//! the filesystem, which does not exist in a browser. Reaching it needs an
//! in-memory file source threaded through `scope` and `report`; that is its own
//! change, not a detail of these bindings.

use comply::config::Config;
use comply::header::{ResolvedStyle, detect_style};
use wasm_bindgen::prelude::wasm_bindgen;

/// Outcome of validating an SPDX expression.
///
/// A struct rather than a thrown exception: an invalid expression is an ordinary
/// answer for an editor validating as the user types, not an exceptional one.
#[wasm_bindgen(getter_with_clone)]
pub struct ValidationResult {
    /// Whether the expression is a well-formed SPDX expression.
    pub valid: bool,
    /// Why it was rejected; `undefined` in JavaScript when `valid` is true.
    pub error: Option<String>,
}

/// What a REUSE.toml turned out to contain.
///
/// Not the parsed `Config` itself: `Config` is not serialisable, and exposing it
/// would mean deriving serde on the core's public types purely to satisfy a
/// binding.
#[wasm_bindgen(getter_with_clone)]
pub struct ParsedConfig {
    /// How many annotations the manifest declares, after multi-path expansion.
    pub annotation_count: usize,
}

/// Validates an SPDX licence expression.
#[wasm_bindgen]
#[must_use]
pub fn validate_spdx(expression: &str) -> ValidationResult {
    match comply::spdx::validate(expression) {
        Ok(()) => ValidationResult {
            valid: true,
            error: None,
        },
        Err(error) => ValidationResult {
            valid: false,
            error: Some(error.to_string()),
        },
    }
}

/// Adds an SPDX header to file content, returning the annotated content.
///
/// `filename` selects the comment syntax and is required: content alone cannot
/// distinguish a Rust file from a shell script, so omitting it silently produced
/// `#` comments for `fn main() {}`. A browser has the name from `File.name`.
///
/// # Errors
///
/// Returns the message `comply` produced when `license` is not a valid SPDX
/// expression, or when `filename` has no comment syntax `comply` recognises.
#[wasm_bindgen]
pub fn annotate_file(
    filename: &str,
    source: &str,
    license: &str,
    copyright: &str,
) -> Result<String, String> {
    let style = ResolvedStyle::builtin(detect_style(filename));

    comply::header::annotate_file(source, copyright, license, Some(style), false)
        .map_err(|error| error.to_string())
}

/// Parses REUSE.toml content.
///
/// # Errors
///
/// Returns the message `comply` produced when the content is not a valid
/// manifest -- malformed TOML, or a missing `version` key.
#[wasm_bindgen]
pub fn parse_reuse_toml(content: &str) -> Result<ParsedConfig, String> {
    // "REUSE.toml" as the name: it only labels error messages, and the content
    // arrived without a filename.
    let config = Config::from_toml_str(content, "REUSE.toml").map_err(|error| error.to_string())?;

    Ok(ParsedConfig {
        annotation_count: config.annotations.len(),
    })
}

// The fixtures below carry SPDX tags as data. Without these markers comply
// reads them as declarations for this file and folds them into the header --
// it rewrote it to `MIT OR Apache-2.0 AND MIT`, then appended another `AND MIT`
// on the next run.
// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "version = 1\n\n[[annotations]]\npath = \"src/lib.rs\"\n\
                            SPDX-FileCopyrightText = \"Someone\"\n\
                            SPDX-License-Identifier = \"MIT\"\n";

    #[test]
    fn a_valid_expression_reports_no_error() {
        let result = validate_spdx("MIT OR Apache-2.0");

        assert!(result.valid);
        assert!(
            result.error.is_none(),
            "a valid expression carries no error"
        );
    }

    #[test]
    fn an_invalid_expression_reports_why() {
        let result = validate_spdx("MIT OR OR");

        assert!(!result.valid);
        assert!(
            result.error.is_some_and(|error| !error.is_empty()),
            "a rejection has to say what was wrong"
        );
    }

    #[test]
    fn annotating_takes_the_comment_style_from_the_filename() {
        let annotated = annotate_file("lib.rs", "fn main() {}\n", "MIT", "Someone")
            .expect("a .rs file takes a line comment");

        assert!(
            annotated.contains("// SPDX-License-Identifier: MIT"),
            "{annotated}"
        );
        assert!(annotated.contains("fn main() {}"), "content must survive");
    }

    #[test]
    fn annotating_rejects_an_unparseable_licence() {
        let error = annotate_file("lib.rs", "fn main() {}\n", "NOT AN EXPRESSION", "Someone")
            .expect_err("the expression is not valid SPDX");

        assert!(!error.is_empty(), "the caller needs a reason");
    }

    #[test]
    fn parsing_reports_the_annotation_count() {
        let parsed = parse_reuse_toml(MANIFEST).expect("a well-formed manifest parses");

        assert_eq!(parsed.annotation_count, 1);
    }

    #[test]
    fn parsing_surfaces_a_malformed_manifest() {
        // `let Err(..) else`, not `expect_err`: that needs `Debug` on the Ok
        // type, and a derive existing only for a test message is an impl the
        // coverage gate counts but never executes.
        let Err(error) = parse_reuse_toml("[[annotations]]\n") else {
            panic!("a manifest with no `version` key must be rejected");
        };

        assert!(error.contains("version"), "got {error}");
    }
}
// REUSE-IgnoreEnd
