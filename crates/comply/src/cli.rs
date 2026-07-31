// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

// Reachable from `main.rs` only. `lib.rs` never declares this module, so the
// library a downstream crate links against carries none of it -- which is what
// lets `default-features = false` drop clap and anyhow entirely.
pub(crate) mod commands;
pub(crate) mod output;
pub(crate) mod shared;
