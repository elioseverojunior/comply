// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{Context, Result};
use comply::config::Config;
use comply::file::{self, FileType};
use comply::reuse_toml;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

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

pub(crate) fn resolve_root(path: Option<&String>) -> Result<PathBuf> {
    let root = path.map_or_else(
        || Path::new(".").to_path_buf(),
        |p| Path::new(p.as_str()).to_path_buf(),
    );
    let root = root
        .canonicalize()
        .context("failed to resolve project root path")?;

    // A file here used to be accepted silently and then treated as the project
    // root: the walk yielded that one file under an empty relative path, and
    // REUSE.toml was looked for *inside* it, so nothing matched and the result
    // was "FAILED : No SPDX header and no matching config annotation" with no
    // filename at all. Say what was wrong and which command takes files.
    if !root.is_dir() {
        anyhow::bail!(
            "{} is a file, but this command takes a project directory.\n\
             To act on individual files use `comply lint-file <FILES>`, \
             `comply annotate --path <DIR> <FILES>` or \
             `comply format --path <DIR> --files <FILE>`.",
            root.display()
        );
    }

    Ok(root)
}

/// Filter caller-supplied paths down to the ones a command may act on.
///
/// Explicit paths never go through [`comply::file::walk`], and so bypass every
/// scope rule it applies. Unfiltered, `comply annotate .git/config` wrote an
/// SPDX header straight into the repository's own configuration; a gitignored
/// or `[tool.comply] ignore`-matched file was rewritten just as readily.
///
/// Returns the paths to act on. Anything declined is reported on stderr rather
/// than dropped silently, so a caller who named a file explicitly is told why
/// nothing happened to it.
pub(crate) fn permitted_targets(root: &Path, files: &[String], config: &Config) -> Vec<String> {
    files
        .iter()
        .filter(|f| {
            let absolute = if Path::new(f.as_str()).is_absolute() {
                PathBuf::from(f.as_str())
            } else {
                root.join(f.as_str())
            };
            if file::is_excluded(root, &absolute, &config.ignore) {
                eprintln!("skipping {f}: outside the files comply covers");
                return false;
            }
            true
        })
        .map(|f| to_repo_relative(root, f))
        .collect()
}

pub(crate) fn walk_source_files(root: &Path, config: &Config) -> Result<Vec<String>> {
    let mut targets = Vec::new();
    for entry in file::walk_with(root, &config.ignore, &config.source_patterns, config.walk) {
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
    use tempfile::TempDir;

    #[test]
    fn a_named_path_outside_the_covered_set_is_declined() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::write(root.join("REUSE.toml"), "version = 1\n").unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), "[core]\n").unwrap();
        fs::write(root.join("real.rs"), "fn r() {}\n").unwrap();
        let config = load_config(&root).unwrap();

        let targets = permitted_targets(
            &root,
            &[".git/config".to_string(), "real.rs".to_string()],
            &config,
        );

        // Unfiltered, `annotate` wrote an SPDX header into the repository's own
        // configuration file.
        assert_eq!(targets, vec!["real.rs".to_string()]);
    }

    #[test]
    fn a_file_where_a_project_directory_belongs_is_refused() {
        // Passing a file was accepted and then treated as the project root:
        // the walk yielded it under an empty relative path and REUSE.toml was
        // sought inside it, producing "FAILED : No SPDX header ..." with no
        // filename. The error now names the file and the right command.
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("convert.rs");
        fs::write(&file, "fn a() {}\n").unwrap();
        let arg = file.to_str().unwrap().to_string();

        let err = resolve_root(Some(&arg)).unwrap_err();
        let msg = err.to_string();

        assert!(msg.contains("is a file"), "got: {msg}");
        assert!(
            msg.contains("lint-file"),
            "must point at the right command: {msg}"
        );
    }

    #[test]
    fn a_directory_still_resolves() {
        let tmp = TempDir::new().unwrap();
        let arg = tmp.path().to_str().unwrap().to_string();

        assert!(resolve_root(Some(&arg)).is_ok());
    }

    #[test]
    fn to_repo_relative_strips_the_root_prefix() {
        let root = Path::new("/work/project");

        let rel = to_repo_relative(root, "/work/project/crates/comply/src/hash.rs");

        assert_eq!(rel, "crates/comply/src/hash.rs");
    }
}
