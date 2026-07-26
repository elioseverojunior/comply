// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::Result;
use clap::Parser;

mod commands;
mod shared;

use commands::{annotate, fix, format, init, lint};

#[derive(Parser)]
#[command(version, about = "comply -- REUSE compliance tool")]
enum Command {
    /// Initialize a project for REUSE compliance
    Init {
        /// Path to project directory
        path: Option<String>,
        /// Overwrite existing REUSE.toml and license files
        #[arg(long)]
        force: bool,
        /// Config file name (default: REUSE.toml)
        #[arg(long, value_parser = ["REUSE.toml", "reuse.toml", ".reuse.toml"], default_value = "REUSE.toml")]
        config_name: String,
        /// SPDX license identifier (default: MIT OR Apache-2.0)
        #[arg(short, long, default_value = "MIT OR Apache-2.0")]
        license: String,
        /// Copyright holder "Name <email>" (repeatable)
        #[arg(long)]
        holder: Vec<String>,
    },
    /// Format SPDX headers consistently
    Format {
        /// Path to project directory
        #[arg(short, long)]
        path: Option<String>,
        /// Files to format (default: all source files)
        #[arg(short, long)]
        files: Vec<String>,
        /// Check if files are formatted without making changes
        #[arg(long)]
        check: bool,
    },
    /// Check project for REUSE compliance
    Lint {
        /// Path to project directory
        path: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Add or update SPDX headers
    Annotate {
        /// Files to annotate
        files: Vec<String>,
        /// Path to project directory
        #[arg(short, long)]
        path: Option<String>,
        /// SPDX license identifier (optional if REUSE.toml provides it)
        #[arg(short, long)]
        license: Option<String>,
        /// Copyright notice (optional if REUSE.toml provides it)
        #[arg(short, long)]
        copyright: Option<String>,
        /// Copyright year (supports ranges: "2017-2019", "2017, 2019-2024")
        #[arg(long)]
        year: Option<String>,
        /// Contributor name (repeatable)
        #[arg(long)]
        contributor: Vec<String>,
        /// Copyright holder "Name <email>" (repeatable)
        #[arg(long)]
        holder: Vec<String>,
        /// Annotate all files recursively under specified paths
        #[arg(short, long)]
        recursive: bool,
        /// Exclude year from copyright notice
        #[arg(long)]
        exclude_year: bool,
        /// Merge identical copyrights
        #[arg(long)]
        merge_copyrights: bool,
        /// Copyright prefix style: spdx (default), spdx-c, spdx-symbol, string, symbol
        #[arg(long)]
        copyright_prefix: Option<String>,
        /// Force .license companion file instead of inline header
        #[arg(long)]
        force_dot_license: bool,
        /// Overwrite existing headers
        #[arg(long)]
        force: bool,
        /// Also add an override annotation for these files to REUSE.toml
        /// (default: false -- an inline header already declares the license, and
        /// an `override` annotation would supersede it)
        #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
        update_reuse_toml: bool,
        /// Precedence for config override: closest, aggregate, override
        #[arg(long, value_parser = ["closest", "aggregate", "override"], default_value = "override")]
        precedence: String,
        /// Skip files with unrecognised comment styles
        #[arg(long)]
        skip_unrecognised: bool,
    },
    /// Auto-fix compliance issues
    Fix {
        /// Path to project directory
        path: Option<String>,
        /// Show what would be fixed without making changes
        #[arg(long)]
        dry_run: bool,
    },
}

/// Commands that gate report success as a bool; a failure exits 1 with no
/// extra output, matching the historical `process::exit(1)` behaviour.
fn exit_on_failure(ok: bool) {
    if !ok {
        std::process::exit(1);
    }
}

fn main() -> Result<()> {
    let cmd = Command::parse();
    match &cmd {
        Command::Init { .. } => init::run(&cmd),
        Command::Format { path, files, check } => {
            exit_on_failure(format::run(path.as_ref(), files, *check)?);
            Ok(())
        }
        Command::Lint { .. } => {
            exit_on_failure(lint::run(&cmd)?);
            Ok(())
        }
        Command::Annotate { .. } => annotate::run(&cmd),
        Command::Fix { path, dry_run } => fix::run(path.clone(), *dry_run),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotate_does_not_update_reuse_toml_by_default() {
        let cmd = Command::parse_from(["comply", "annotate", "src/lib.rs"]);

        let Command::Annotate {
            update_reuse_toml, ..
        } = cmd
        else {
            panic!("expected the annotate subcommand");
        };

        assert!(
            !update_reuse_toml,
            "REUSE.toml must only be rewritten when explicitly requested"
        );
    }
}
