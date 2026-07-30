// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzz `comply::spdx::parse`.
//!
//! The SPDX expression parser is the natural first target: it is total,
//! side-effect free, and takes attacker-shaped input -- every `REUSE.toml`
//! annotation and every `SPDX-License-Identifier` header in a scanned project
//! reaches it. A panic here is reachable by handing comply a crafted file, so
//! any input that is not a clean `Ok`/`Err` is a bug.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Non-UTF-8 is rejected before the parser ever sees it, so feeding it here
    // would only measure the decoder.
    let Ok(expression) = std::str::from_utf8(data) else {
        return;
    };

    // The contract is total: every input yields Ok or Err, never a panic.
    if let Ok(parsed) = comply::spdx::parse(expression) {
        // Re-parsing what we rendered has to succeed, otherwise the parser and
        // its Display disagree and a round-trip through REUSE.toml would lose
        // or corrupt an expression.
        let rendered = parsed.to_string();
        assert!(
            comply::spdx::parse(&rendered).is_ok(),
            "rendered expression no longer parses: {expression:?} -> {rendered:?}"
        );
    }
});
