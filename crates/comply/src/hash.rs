// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::io::Read;
use std::path::Path;

use sha1::Sha1;
use sha2::{Digest, Sha256};

use crate::error::Error;

/// Compute the SHA-256 digest of a file's contents.
#[allow(clippy::missing_errors_doc)]
pub fn hash_file(path: &Path) -> Result<Vec<u8>, Error> {
    stream(path, Sha256::new())
}

/// Compute the SHA-1 digest of a file's contents.
///
/// Needed only because SPDX 2.x makes a SHA1 file checksum mandatory, so a
/// document declaring `SPDX-2.1` has to carry one. It is not used for
/// integrity anywhere in comply -- SHA-1 is collision-broken and
/// [`hash_file`] remains the digest of record.
#[allow(clippy::missing_errors_doc)]
pub fn hash_file_sha1(path: &Path) -> Result<Vec<u8>, Error> {
    stream(path, Sha1::new())
}

/// Feed a file through a hasher in fixed-size chunks.
///
/// Shared so the two digests cannot drift in how they read: a whole-file read
/// would also hold an arbitrarily large file in memory.
fn stream<D: Digest>(path: &Path, mut hasher: D) -> Result<Vec<u8>, Error> {
    let mut file = fs::File::open(path)?;
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_vec())
}

/// Compute the SHA-256 digest of a byte slice.
#[must_use]
pub fn hash_bytes(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Format a hash digest as a lowercase hex string.
#[must_use]
pub fn format_hash(digest: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn hash_bytes_consistent() {
        let a = hash_bytes(b"hello");
        let b = hash_bytes(b"hello");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_bytes_different() {
        let a = hash_bytes(b"hello");
        let b = hash_bytes(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn hash_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "test content").unwrap();
        drop(f);

        let digest = hash_file(&path).unwrap();
        assert_eq!(digest.len(), 32); // SHA-256 is 32 bytes
    }

    #[test]
    fn hash_format_hex() {
        let digest = hash_bytes(b"test");
        let hex = format_hash(&digest);
        assert_eq!(hex.len(), 64);
    }
}
