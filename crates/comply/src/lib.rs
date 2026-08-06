// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

// The README is the crate's front page on docs.rs, and including it here makes
// every example in it a doctest. Two of the previous examples named APIs that
// had never existed; nothing caught them because a ```rust fence in a README
// the compiler never reads is prose.
//
// `../README.md` is `crates/comply/README.md`, NOT the workspace one:
// include_str! resolves against the file that writes it, and that file is in
// `src/`. The distinction matters because the two are different documents --
// the workspace README is the GitHub landing page and runs no doctests, this
// one is the docs.rs front page and every fence in it is a test. rustdoc then
// reports failures against the unnormalised `crates/comply/src/../README.md`,
// which reads like the workspace file and is not.
#![doc = include_str!("../README.md")]

pub mod config;
pub mod error;
pub mod file;
pub mod fix;
pub mod hash;
pub mod header;
pub mod license;
pub mod report;
pub mod reuse_toml;
pub mod scope;
pub mod spdx;
pub mod version;
