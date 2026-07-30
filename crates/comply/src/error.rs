// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    /// SPDX expression parsing or validation error.
    #[error("SPDX error: {0}")]
    Spdx(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Regex error.
    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),

    /// Serde (de)serialization error.
    #[error("serialization error: {0}")]
    Serde(String),

    /// An unrecognised `--copyright-prefix` value.
    #[error("unknown copyright prefix: {0}")]
    InvalidCopyrightPrefix(String),

    /// Not yet implemented.
    #[error("not yet implemented")]
    NotImplemented,
}
