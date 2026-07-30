// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Version and build provenance, shared by every crate that reports a version.
//!
//! The values come from `build.rs`, which sits in this crate so that one capture
//! serves every surface rather than each binary carrying its own build script.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::Error;

/// Build provenance for a `--version` long form, assembled at compile time.
///
/// `concat!` so it costs nothing at runtime: every part is a literal that
/// `build.rs` handed to rustc. It lives here rather than in a binary because
/// `rustc-env` reaches only the package whose build script set it, so a const in
/// any other crate could not see these values.
///
/// This reports `CARGO_PKG_VERSION`, not the `GitVersion` `SemVer`: the stamp
/// needs parsing, which no const can do. [`BuildInfo::short`] is the
/// stamp-derived answer.
pub const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\ncommit:  ",
    env!("GIT_COMMIT"),
    "\nbuilt:   ",
    env!("BUILD_TIMESTAMP"),
    "\nrustc:   ",
    env!("RUSTC_VERSION"),
    "\ntarget:  ",
    env!("TARGET"),
);

/// How version information is rendered.
///
/// No `Default`: the caller's argument parser supplies the default, and a
/// derived one here would be an impl nothing calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable lines.
    Plain,
    /// A JSON object.
    Json,
    /// A TOML table.
    Toml,
}

/// The build provenance of a compiled binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildInfo {
    /// Release version, `SemVer` from the `GitVersion` stamp where there is one.
    pub version: String,
    /// Short commit hash, suffixed `-dirty` when the tree had local changes.
    pub commit: String,
    /// Build time, honouring `SOURCE_DATE_EPOCH`.
    pub built: String,
    /// The `rustc` that compiled it.
    pub rustc: String,
    /// The target triple it was compiled for.
    pub target: String,
    /// The `GitVersion` stamp, empty when gitversion was unavailable at build time.
    pub gitversion: Map<String, Value>,
}

impl BuildInfo {
    /// The provenance of the running binary.
    #[must_use]
    pub fn current() -> Self {
        Self::from_parts(
            env!("CARGO_PKG_VERSION"),
            env!("GIT_COMMIT"),
            env!("BUILD_TIMESTAMP"),
            env!("RUSTC_VERSION"),
            env!("TARGET"),
            env!("GITVERSION_JSON"),
        )
    }

    /// Assemble from captured parts, deriving the version from `stamp`.
    #[must_use]
    pub fn from_parts(
        package_version: &str,
        commit: &str,
        built: &str,
        rustc: &str,
        target: &str,
        stamp: &str,
    ) -> Self {
        let gitversion = parse_stamp(stamp);
        let version = gitversion
            .get("SemVer")
            .and_then(Value::as_str)
            .unwrap_or(package_version)
            .to_owned();

        Self {
            version,
            commit: commit.to_owned(),
            built: built.to_owned(),
            rustc: rustc.to_owned(),
            target: target.to_owned(),
            gitversion,
        }
    }

    /// The bare version line, e.g. `v0.1.0-1`.
    #[must_use]
    pub fn short(&self) -> String {
        format!("v{}", self.version)
    }

    /// Render, either the short line or the full provenance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Serde`] if the value cannot be serialized.
    pub fn render(&self, format: Format, full: bool) -> Result<String, Error> {
        if !full {
            return Ok(match format {
                Format::Plain => self.short(),
                Format::Json => format!("{{\n  \"version\": \"{}\"\n}}", self.short()),
                Format::Toml => format!("version = \"{}\"", self.short()),
            });
        }

        match format {
            Format::Plain => Ok(self.plain()),
            Format::Json => {
                serde_json::to_string_pretty(self).map_err(|e| Error::Serde(e.to_string()))
            }
            // TOML has no null, so a stamp key with one cannot be represented.
            // Dropping those keys keeps `-o toml` usable; refusing to render
            // would make it useless on every real gitversion stamp.
            Format::Toml => {
                let mut sanitized = self.clone();
                sanitized.gitversion.retain(|_, value| !value.is_null());
                toml::to_string(&sanitized).map_err(|e| Error::Serde(e.to_string()))
            }
        }
    }

    /// The human-readable full form, laid out like `--version` already was.
    fn plain(&self) -> String {
        let mut out = format!(
            "{}\ncommit:  {}\nbuilt:   {}\nrustc:   {}\ntarget:  {}",
            self.short(),
            self.commit,
            self.built,
            self.rustc,
            self.target
        );

        if !self.gitversion.is_empty() {
            let stamp: Vec<String> = self
                .gitversion
                .iter()
                .map(|(key, value)| format!("  {key}: {}", scalar(value)))
                .collect();

            out.push_str("\ngitversion:\n");
            out.push_str(&stamp.join("\n"));
        }
        out
    }
}

/// Parse the baked stamp, adding the key `version:json` synthesizes in jq.
///
/// A stamp that will not parse is treated as absent: the binary still has to
/// report a truthful version, and the package version is the other source.
fn parse_stamp(stamp: &str) -> Map<String, Value> {
    let mut stamp: Map<String, Value> = serde_json::from_str(stamp).unwrap_or_default();

    if let (Some(major), Some(minor)) = (stamp.get("Major"), stamp.get("Minor")) {
        let combined = Value::from(format!("{major}.{minor}"));
        stamp.insert("MajorMinor".to_owned(), combined);
    }
    stamp
}

/// A stamp value as a bare scalar -- a string without its JSON quotes.
fn scalar(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), ToOwned::to_owned)
}

// REUSE-IgnoreStart
#[cfg(test)]
mod tests {
    use super::*;

    /// A trimmed `GitVersion` stamp, including the `null` that TOML cannot hold.
    const STAMP: &str = r#"{"Major":0,"Minor":1,"Patch":0,"SemVer":"0.1.0-1",
        "MajorMinorPatch":"0.1.0","BranchName":"main","BuildMetaData":null}"#;

    fn info(stamp: &str) -> BuildInfo {
        BuildInfo::from_parts(
            "9.9.9",
            "abc123-dirty",
            "2026-07-29 00:00:00 UTC",
            "rustc 1.97.1",
            "x86_64-apple-darwin",
            stamp,
        )
    }

    #[test]
    fn the_running_binarys_provenance_comes_from_the_build_script() {
        // The only path that reads build.rs's output, so nothing else would
        // catch a `rustc-env` key being renamed or dropped.
        let built = BuildInfo::current();

        assert!(!built.commit.is_empty(), "no commit captured");
        assert!(!built.target.is_empty(), "no target captured");
        assert!(built.short().starts_with('v'), "got {}", built.short());
    }

    #[test]
    fn the_short_form_prefixes_the_semver_from_the_stamp() {
        assert_eq!(info(STAMP).short(), "v0.1.0-1");
    }

    #[test]
    fn an_absent_stamp_falls_back_to_the_package_version() {
        // A build on a machine without gitversion still has to report something
        // truthful, and the package version is the only other source.
        assert_eq!(info("{}").short(), "v9.9.9");
    }

    #[test]
    fn a_malformed_stamp_is_treated_as_absent_rather_than_failing() {
        assert_eq!(info("not json at all").short(), "v9.9.9");
    }

    #[test]
    fn major_minor_is_synthesized_the_way_the_mise_task_does() {
        // `version:json` adds MajorMinor in jq; raw gitversion has no such key,
        // so anything reading the stamp has to supply it or lose the field.
        let built = info(STAMP);
        assert_eq!(
            built.gitversion.get("MajorMinor").and_then(Value::as_str),
            Some("0.1")
        );
    }

    #[test]
    fn without_full_every_format_yields_only_the_short_line() {
        for format in [Format::Plain, Format::Json, Format::Toml] {
            let out = info(STAMP).render(format, false).unwrap();
            assert!(out.contains("v0.1.0-1"), "{format:?}: {out}");
            assert!(
                !out.contains("rustc"),
                "{format:?} leaked provenance: {out}"
            );
        }
    }

    #[test]
    fn the_full_plain_form_names_every_provenance_field() {
        let out = info(STAMP).render(Format::Plain, true).unwrap();

        for expected in [
            "v0.1.0-1",
            "abc123-dirty",
            "2026-07-29 00:00:00 UTC",
            "rustc 1.97.1",
            "x86_64-apple-darwin",
            "BranchName",
        ] {
            assert!(out.contains(expected), "missing {expected} in: {out}");
        }
    }

    #[test]
    fn the_full_plain_form_omits_the_stamp_section_when_there_is_no_stamp() {
        // A bare `gitversion:` heading with nothing under it reads as a stamp
        // that failed to render, rather than one that was never captured.
        let out = info("{}").render(Format::Plain, true).unwrap();

        assert!(out.contains("v9.9.9"), "{out}");
        assert!(!out.contains("gitversion:"), "empty heading shown: {out}");
    }

    #[test]
    fn the_same_parts_always_describe_the_same_build() {
        // The struct is a value: two reads of one binary's provenance must not
        // differ, or `--full -o json` would be unstable between invocations.
        assert_eq!(info(STAMP), info(STAMP));
        assert!(format!("{:?}", info(STAMP)).contains("BuildInfo"));
    }

    #[test]
    fn the_full_json_form_keeps_the_stamp_types() {
        let out = info(STAMP).render(Format::Json, true).unwrap();
        let parsed: Value = serde_json::from_str(&out).unwrap();

        assert_eq!(parsed["version"], Value::from("0.1.0-1"));
        // A number stays a number: stringifying the whole stamp would make the
        // JSON output lossy against `mise run version:json`.
        assert_eq!(parsed["gitversion"]["Major"], Value::from(0));
    }

    #[test]
    fn the_full_toml_form_drops_entries_toml_cannot_represent() {
        // TOML has no null. Dropping those keys is the only representable
        // choice; failing the render over them would make `-o toml` useless.
        let out = info(STAMP).render(Format::Toml, true).unwrap();

        assert!(out.contains("BranchName"), "{out}");
        assert!(!out.contains("BuildMetaData"), "null survived: {out}");
    }
}
// REUSE-IgnoreEnd
