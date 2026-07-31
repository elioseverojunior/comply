// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

// The README is the crate's front page on docs.rs, and including it here makes
// every example in it a doctest. Two of the previous examples named APIs that
// had never existed; nothing caught them because a ```rust fence in a README
// the compiler never reads is prose.
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
