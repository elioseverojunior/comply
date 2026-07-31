// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::Result;
use clap::Parser;

mod cli;

use comply::version::LONG_VERSION;

use cli::commands::{
    annotate, convert_dep5, download, fix, format, init, lint, lint_file, spdx, supported_licenses,
    version,
};

#[derive(Parser)]
#[command(
    name = "comply",
    version,
    long_version = LONG_VERSION,
    about = "comply -- REUSE compliance tool"
)]
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
        /// Copyright holder `Name <email>` (repeatable)
        #[arg(long)]
        holder: Vec<String>,
    },
    /// Format SPDX headers consistently
    #[command(visible_aliases = ["fmt"])]
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
    #[command(visible_aliases = ["l", "check", "chk", "ck"])]
    Lint {
        /// Path to project directory
        path: Option<String>,
        /// Format output as JSON
        #[arg(short, long)]
        json: bool,
        /// Format output as plain text (default)
        #[arg(short, long)]
        plain: bool,
        /// Format output as errors per line
        #[arg(short, long)]
        lines: bool,
        /// Prevent output; the exit code carries the verdict
        #[arg(short, long)]
        quiet: bool,
    },
    /// Check individual files for REUSE compliance
    #[command(visible_aliases = ["lf", "check-file", "chkf", "ckf"])]
    LintFile {
        /// Files to check
        files: Vec<String>,
        /// Path to project directory
        #[arg(short, long)]
        path: Option<String>,
        /// Format output as errors per line (default)
        #[arg(short, long)]
        lines: bool,
        /// Prevent output; the exit code carries the verdict
        #[arg(short, long)]
        quiet: bool,
    },
    /// Add or update SPDX headers
    #[command(visible_aliases = ["a", "note"])]
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
        /// Copyright holder `Name <email>` (repeatable)
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
        /// Form the copyright notice is written in
        #[arg(
            long,
            value_parser = comply::header::CopyrightPrefix::VALUES,
            default_value = "spdx"
        )]
        copyright_prefix: String,
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
    // `f` belongs here rather than to `format`, which keeps only `fmt`.
    #[command(visible_aliases = ["f", "fx"])]
    Fix {
        /// Path to project directory
        path: Option<String>,
        /// Show what would be fixed without making changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Fetch licence texts into LICENSES/
    Download {
        /// SPDX identifiers to fetch
        ids: Vec<String>,
        /// Path to project directory
        #[arg(short, long)]
        path: Option<String>,
        /// Fetch every licence the project references but has no file for
        #[arg(long)]
        all: bool,
        /// Directory to write into (default: LICENSES/)
        #[arg(short, long)]
        output: Option<String>,
        /// URL or path template; `{}` is replaced with the identifier
        #[arg(long)]
        source: Option<String>,
        /// Overwrite a licence file that already exists
        #[arg(long)]
        force: bool,
    },
    /// Generate an SPDX 2.1 bill of materials for the project
    Spdx {
        /// Path to project directory
        path: Option<String>,
        /// File to write to (default: stdout)
        #[arg(short, long)]
        output: Option<String>,
        /// Name of the person signing off on the report
        #[arg(long)]
        creator_person: Option<String>,
        /// Name of the organization signing off on the report
        #[arg(long)]
        creator_organization: Option<String>,
    },
    /// Convert `.reuse/dep5` into an equivalent REUSE.toml
    ConvertDep5 {
        /// Path to project directory
        path: Option<String>,
        /// Print the REUSE.toml that would be written, changing nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// List the SPDX identifiers this build recognises
    SupportedLicenses {
        /// Output every list as JSON
        #[arg(long)]
        json: bool,
        /// List license exceptions instead of licenses
        #[arg(long)]
        exceptions: bool,
        /// List deprecated identifiers instead of licenses
        #[arg(long)]
        deprecated: bool,
    },
    /// Show the version, and optionally how this binary was built
    Version {
        /// Include build provenance and the full gitversion stamp
        #[arg(long)]
        full: bool,
        /// Output format
        #[arg(short, long, value_enum, default_value = "plain")]
        output: version::Output,
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
        Command::LintFile { .. } => {
            exit_on_failure(lint_file::run(&cmd)?);
            Ok(())
        }
        Command::Annotate { .. } => annotate::run(&cmd),
        Command::Fix { path, dry_run } => fix::run(path.clone(), *dry_run),
        Command::Download {
            ids,
            path,
            all,
            output,
            source,
            force,
        } => download::run(
            path.as_ref(),
            &download::Request {
                ids,
                all: *all,
                output: output.as_ref(),
                source: source.as_ref(),
                force: *force,
            },
        ),
        Command::Spdx {
            path,
            output,
            creator_person,
            creator_organization,
        } => spdx::run(
            path.as_ref(),
            output.as_ref(),
            &spdx::Creators {
                person: creator_person.as_ref(),
                organization: creator_organization.as_ref(),
            },
        ),
        Command::ConvertDep5 { path, dry_run } => convert_dep5::run(path.as_ref(), *dry_run),
        Command::SupportedLicenses {
            json,
            exceptions,
            deprecated,
        } => supported_licenses::run(*json, *exceptions, *deprecated),
        Command::Version { full, output } => version::run(*full, *output),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn the_long_version_reports_build_provenance() {
        assert!(
            LONG_VERSION.starts_with(env!("CARGO_PKG_VERSION")),
            "the release version has to lead: {LONG_VERSION}"
        );
        for label in ["commit:", "built:", "rustc:", "target:"] {
            assert!(
                LONG_VERSION.contains(label),
                "no {label} line: {LONG_VERSION}"
            );
        }
    }

    #[test]
    fn the_short_version_names_the_command_the_user_typed() {
        let rendered = Command::command().render_version();
        assert!(rendered.starts_with("comply "), "got {rendered:?}");
    }

    #[test]
    fn the_subcommand_aliases_reach_their_subcommand() {
        // `try_parse_from`, not `parse_from`: clap exits the process on an
        // unrecognised subcommand, which would kill the test binary instead of
        // failing this one test.
        let parsed = Command::try_parse_from(["comply", "fmt"]);
        assert!(
            matches!(parsed, Ok(Command::Format { .. })),
            "fmt did not reach format"
        );
        for alias in ["l", "check", "chk", "ck"] {
            let parsed = Command::try_parse_from(["comply", alias]);
            assert!(
                matches!(parsed, Ok(Command::Lint { .. })),
                "{alias} did not reach lint"
            );
        }
        for alias in ["a", "note"] {
            let parsed = Command::try_parse_from(["comply", alias, "src/lib.rs"]);
            assert!(
                matches!(parsed, Ok(Command::Annotate { .. })),
                "{alias} did not reach annotate"
            );
        }
        for alias in ["lf", "check-file", "chkf", "ckf"] {
            let parsed = Command::try_parse_from(["comply", alias, "src/lib.rs"]);
            assert!(
                matches!(parsed, Ok(Command::LintFile { .. })),
                "{alias} did not reach lint-file"
            );
        }
        for alias in ["f", "fx"] {
            let parsed = Command::try_parse_from(["comply", alias]);
            assert!(
                matches!(parsed, Ok(Command::Fix { .. })),
                "{alias} did not reach fix"
            );
        }
    }

    #[test]
    fn no_alias_shadows_another_subcommand() {
        // clap resolves an exact match before an alias, so an alias colliding
        // with a real subcommand name is silently unreachable rather than an
        // error. `l` for lint and `lf` for lint-file are one keystroke apart.
        let cmd = Command::command();
        let names: Vec<String> = cmd
            .get_subcommands()
            .map(|s| s.get_name().to_owned())
            .collect();
        for sub in cmd.get_subcommands() {
            for alias in sub.get_all_aliases() {
                assert!(
                    !names.iter().any(|n| n == alias),
                    "{alias} is both an alias of {} and a subcommand",
                    sub.get_name()
                );
            }
        }
    }

    #[test]
    fn the_aliases_are_listed_in_help() {
        // Hidden aliases are undiscoverable: a user reads `--help` to find out
        // that `fmt` exists at all.
        let help = Command::command().render_long_help().to_string();
        for alias in ["fmt", "check", "note"] {
            assert!(help.contains(alias), "help never mentions {alias}: {help}");
        }
    }

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
