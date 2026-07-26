// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{Context, Result};
use comply::config::Config;
use comply::file::{self, FileType};
use comply::reuse_toml;
use serde::Serialize;
use std::fs;
use std::path::Path;

pub(crate) fn write_reuse_toml_override(
    root: &Path,
    files: &[String],
    license: &str,
    copyright: &str,
    precedence: &str,
) -> Result<()> {
    let reuse_toml_path = root.join("REUSE.toml");
    if !reuse_toml_path.exists() {
        return Ok(()); // No REUSE.toml to update
    }

    let content = fs::read_to_string(&reuse_toml_path).context("failed to read REUSE.toml")?;

    // Merges into a block that already carries the same copyright, license and
    // precedence, so repeated runs cannot pile up duplicate blocks.
    let updated = reuse_toml::upsert_annotation(&content, files, license, copyright, precedence)
        .context("failed to merge the annotation into REUSE.toml")?;

    if updated == content {
        return Ok(());
    }

    fs::write(&reuse_toml_path, &updated).context("failed to write REUSE.toml")?;
    println!(
        "Updated REUSE.toml with override annotation for: {}",
        files.join(", ")
    );
    Ok(())
}

/// Normalise a caller-supplied path into a repo-relative string.
///
/// REUSE annotation paths must be relative to the project root; an absolute one
/// would never match on another machine or in CI.
pub(crate) fn to_repo_relative(root: &Path, input: &str) -> String {
    let prefix = format!("{}/", root.display());
    input.strip_prefix(&prefix).unwrap_or(input).to_string()
}

/// Load the project's REUSE configuration.
///
/// A broken or spec-violating manifest is an error, not an empty config:
/// falling back silently would report "missing copyright" on every file and
/// point the reader at entirely the wrong problem.
/// No extra context is wrapped around the error: the underlying message
/// already names the offending file, and an outer "failed to load the
/// configuration" would only bury it.
pub(crate) fn load_config(root: &Path) -> Result<Config> {
    Ok(Config::load(root)?)
}

pub(crate) fn resolve_root(path: Option<&String>) -> Result<std::path::PathBuf> {
    let root = path.map_or_else(
        || Path::new(".").to_path_buf(),
        |p| Path::new(p.as_str()).to_path_buf(),
    );
    root.canonicalize()
        .context("failed to resolve project root path")
}

pub(crate) fn walk_source_files(root: &Path, config: &Config) -> Result<Vec<String>> {
    let mut targets = Vec::new();
    for entry in file::walk(root, &config.ignore, &config.source_patterns) {
        let entry = entry?;
        if entry.file_type == FileType::Source {
            let rel_path = entry.relative_to(root);
            targets.push(rel_path);
        }
    }
    Ok(targets)
}

pub(crate) fn print_json_report(report: &comply::report::Report) -> Result<()> {
    #[derive(Serialize)]
    struct JsonReport {
        summary: JsonSummary,
        files: Vec<JsonFileResult>,
    }

    #[derive(Serialize)]
    struct JsonSummary {
        total: usize,
        passed: usize,
        failed: usize,
        skipped: usize,
    }

    #[derive(Serialize)]
    struct JsonFileResult {
        path: String,
        status: String,
        copyrights: Vec<String>,
        license: Option<String>,
    }

    let json = JsonReport {
        summary: JsonSummary {
            total: report.total(),
            passed: report.passed(),
            failed: report.failed(),
            skipped: report.skipped(),
        },
        files: report
            .iter()
            .map(|r| JsonFileResult {
                path: r.path.clone(),
                status: r.status.to_string(),
                copyrights: r.copyrights.clone(),
                license: r.license.clone(),
            })
            .collect(),
    };
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_repo_relative_strips_the_root_prefix() {
        let root = Path::new("/work/project");

        let rel = to_repo_relative(root, "/work/project/crates/comply/src/hash.rs");

        assert_eq!(rel, "crates/comply/src/hash.rs");
    }
}
