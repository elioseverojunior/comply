<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# comply

Check and enforce the [REUSE Specification](https://reuse.software/spec/) for
software licensing -- a command-line tool and the library behind it.

comply is a native Rust implementation, not a port and not a wrapper. It reads
only the project tree it is pointed at: no network, no Python runtime, no
system packages. A single static binary that starts in milliseconds, and the
same crate is usable as a library so other tooling can reach the same verdict
without shelling out.

- **Drop-in for the Python `reuse` tool.** Same command names, same
  `REUSE.toml`, same `.reuse/dep5`, same lint output shape.
- **Fast enough to run per-commit.** No interpreter start-up, no import cost.
- **Library first.** The CLI is a thin layer over a public API; anything the
  binary can do, a program can do.
- **The SPDX License List is bundled** (3.28), so identifier validation works
  offline and produces the same answer on every machine.

## Install

```sh
cargo install comply
```

Releases do not yet carry prebuilt binaries, so `cargo binstall comply` works
but falls back to compiling from source.

From a git checkout:

```sh
git clone https://github.com/elioseverojunior/comply
cd comply
cargo install --path crates/comply
```

All three install one binary, `comply`. Confirm it and see how it was built:

```sh
comply --version
comply version --full --output json
```

## Quick start

```sh
comply init my-project          # REUSE.toml + LICENSES/
cd my-project
comply download MIT             # fetch the license text
comply annotate --license MIT --copyright "Acme Inc" --recursive src/
comply lint                     # exit 0 when compliant
```

## Migrating from the Python `reuse` tool

Command names and arguments line up, so most invocations work unchanged --
substitute the binary and keep the rest:

| Python `reuse` | comply | Notes |
| --- | --- | --- |
| `reuse lint` | `comply lint` | Same `--json`, `--plain`, `--lines`, `--quiet` |
| `reuse lint-file FILE` | `comply lint-file FILE` | Same per-file verdicts |
| `reuse annotate ...` | `comply annotate ...` | Same flags; 16 `--copyright-prefix` styles |
| `reuse download ID` | `comply download ID` | `--all`, `--source`, `--output`, `--force` |
| `reuse spdx` | `comply spdx` | SPDX 2.1 bill of materials |
| `reuse supported-licenses` | `comply supported-licenses` | Adds `--json`, `--exceptions`, `--deprecated` |
| `reuse convert-dep5` | `comply convert-dep5` | Adds `--dry-run` |
| `reuse init` | `comply init` | Adds `--config-name`, `--holder` |
| -- | `comply format` | No equivalent: normalises existing headers |
| -- | `comply fix` | No equivalent: repairs a project in one pass |
| -- | `comply version` | Build provenance as plain text, JSON or TOML |

What is genuinely different:

- **`format` and `fix` are new.** `format` normalises header ordering and
  comment style (`--check` makes it a CI gate); `fix` adds missing headers,
  license files and `.license` companions in one pass.
- **Everything is offline except `download`.** Identifier validation uses the
  bundled list, so `lint` never touches the network.
- **Exit codes are the contract**: `0` compliant, `1` non-compliant. `--quiet`
  suppresses output entirely and leaves only the exit code, which is the form
  to use in a hook.
- **Known limitation**: when a project has both `REUSE.toml` and
  `.reuse/dep5`, comply merges them rather than rejecting the combination. The
  specification treats them as mutually exclusive. Convert with
  `comply convert-dep5` and delete the dep5 file.

## Commands

Every command takes `--help`. Aliases are shown in brackets.

| Command | Aliases | Purpose |
| --- | --- | --- |
| `init` | | Initialize a project for REUSE compliance |
| `format` | `fmt` | Normalize SPDX headers |
| `lint` | `l`, `check`, `chk`, `ck` | Check a project for compliance |
| `lint-file` | `lf`, `check-file`, `chkf`, `ckf` | Check individual files |
| `annotate` | `a`, `note` | Add or update SPDX headers |
| `fix` | `f`, `fx` | Auto-fix compliance issues |
| `download` | | Fetch license texts into `LICENSES/` |
| `spdx` | | Generate an SPDX 2.1 bill of materials |
| `convert-dep5` | | Convert `.reuse/dep5` into a `REUSE.toml` |
| `supported-licenses` | | List the identifiers this build recognizes |
| `version` | | Show the version and build provenance |

### `init`

```sh
comply init /path/to/project
comply init --license MIT --holder "Acme Inc <legal@acme.example>" .
comply init --config-name reuse.toml --force .
```

Creates a `REUSE.toml` manifest and a `LICENSES/` directory.

| Flag | Meaning |
| --- | --- |
| `-l`, `--license <SPDX>` | Default license expression. Defaults to `MIT OR Apache-2.0` |
| `--holder <NAME <EMAIL>>` | Copyright holder, repeatable |
| `--config-name <NAME>` | `REUSE.toml`, `reuse.toml` or `.reuse.toml` |
| `--force` | Overwrite an existing manifest and license files |

### `lint`

```sh
comply lint                     # current directory
comply lint /path/to/project
comply lint --json | jq '.files[] | select(.status != "Ok")'
comply lint --quiet && echo compliant
```

| Flag | Meaning |
| --- | --- |
| `-p`, `--plain` | Human-readable summary (default) |
| `-j`, `--json` | Machine-readable, for pipelines |
| `-l`, `--lines` | One `path:line: message` per finding, for editors |
| `-q`, `--quiet` | No output; the exit code is the verdict |

Exit codes: `0` compliant, `1` non-compliant.

### `lint-file`

```sh
comply lint-file src/main.rs src/lib.rs
comply lint-file --path /path/to/project src/main.rs
```

Judges only the files named, which makes it the right command for a
`pre-commit` hook over staged paths. Output is `--lines` by default. Paths
outside the covered set are skipped rather than reported as failures, so
passing a whole staged list is safe.

### `annotate`

```sh
comply annotate --license MIT --copyright "Acme Inc" src/main.rs

# Recurse, merging duplicate copyright lines
comply annotate -l Apache-2.0 -c "Acme Inc" --recursive --merge-copyrights src/

# Year ranges, and a companion file instead of an inline header
comply annotate -l MIT -c "Acme Inc" --year "2017, 2019-2024" logo.png \
  --force-dot-license
```

`--license` and `--copyright` are optional when `REUSE.toml` already supplies
them.

| Flag | Meaning |
| --- | --- |
| `-l`, `--license <SPDX>` | License expression to write |
| `-c`, `--copyright <TEXT>` | Copyright notice |
| `--year <YEAR>` | Year or range: `2017-2019`, `2017, 2019-2024` |
| `--exclude-year` | Omit the year entirely |
| `--holder <NAME <EMAIL>>` | Copyright holder, repeatable |
| `--contributor <NAME>` | Contributor, repeatable |
| `-r`, `--recursive` | Descend into the paths given |
| `--merge-copyrights` | Collapse identical notices into one line |
| `--copyright-prefix <STYLE>` | Notice form; 16 styles, see below |
| `--force-dot-license` | Write a `.license` companion, never an inline header |
| `--force` | Overwrite an existing header |
| `--skip-unrecognised` | Pass over files whose comment syntax is unknown |
| `--update-reuse-toml <BOOL>` | Also add an override annotation to `REUSE.toml` |
| `--precedence <MODE>` | `closest`, `aggregate` or `override` |

The `--copyright-prefix` styles are `spdx`, `spdx-c`, `spdx-symbol`,
`spdx-string`, `spdx-string-c`, `spdx-string-symbol`, `snippet`, `snippet-c`,
`snippet-symbol`, `snippet-string`, `snippet-string-c`,
`snippet-string-symbol`, `string`, `string-c`, `string-symbol` and `symbol`.
The default, `spdx`, writes `SPDX-FileCopyrightText: 2026 Acme Inc`.

`--update-reuse-toml` defaults to false on purpose: an inline header already
declares the license, and an `override` annotation in `REUSE.toml` would
supersede it.

### `format`

```sh
comply format --path /path/to/project
comply format --files src/main.rs --files src/lib.rs
comply format --check
```

Normalizes header ordering, comment style and whitespace. `--check` reports
without writing, which is the CI form. Note that `format` takes `--path`; it
has no positional argument.

### `fix`

```sh
comply fix --dry-run /path/to/project    # report only
comply fix /path/to/project
```

Adds missing headers, missing license files and missing `.license` companions.
Always worth a `--dry-run` first: it rewrites source files in place.

### `download`

```sh
comply download MIT Apache-2.0
comply download --all                  # every license the project references
comply download --all --force          # overwrite existing files
comply download MIT --output vendor/licenses
comply download MIT --source 'https://example.test/licenses/{}.txt'
```

Writes license texts into `LICENSES/`, or into `--output`. `--source` takes a
URL or path template in which `{}` is replaced with the identifier, which is
how an air-gapped build points at an internal mirror.

### `spdx`

```sh
comply spdx                                    # bill of materials to stdout
comply spdx --output sbom.spdx
comply spdx --creator-organization "Acme Inc"
comply spdx --creator-person "A. Maintainer"
```

### `convert-dep5`

```sh
comply convert-dep5 --dry-run    # print the REUSE.toml that would be written
comply convert-dep5              # write it
```

### `supported-licenses`

```sh
comply supported-licenses
comply supported-licenses --json
comply supported-licenses --exceptions
comply supported-licenses --deprecated
```

### `version`

```sh
comply version
comply version --full                  # build provenance
comply version --output json
comply version --output toml
```

## Configuration

comply reads its own settings from a `[tool.comply]` table in `REUSE.toml`.
Everything is optional; omitting the table gives the defaults below.

```toml
[tool.comply]
# Glob patterns comply never looks at, on top of the built-in exclusions
# (.git, target, node_modules, LICENSES/, .reuse/, and so on).
ignore = ["**/target/**", "**/*.lock"]

[tool.comply.walk]
# Each switch overrides one traversal default. Anything left out keeps the
# default shown here.
hidden       = false  # true skips dotfiles
git-ignore   = true   # apply .gitignore files
git-global   = true   # apply the user's global excludes file
git-exclude  = true   # apply .git/info/exclude
require-git  = false  # true applies gitignore rules only inside a repository
follow-links = false  # true follows symlinks
```

Gitignore rules are layered in git's own precedence order, lowest first:
`core.excludesFile`, then `.git/info/exclude`, then every `.gitignore` from the
project root down to the file's own directory. Because they share one matcher,
a `!negation` in the repository's `.gitignore` overrides the global excludes
file, exactly as it does for git.

### Excluding a region from scanning

A file that quotes SPDX tags -- documentation, tests, this README -- would
otherwise have those tags read as its own licensing. Guard the region, per
REUSE 3.3 section 2.4:

```text
REUSE-IgnoreStart
... text containing SPDX tags ...
REUSE-IgnoreEnd
```

## Continuous integration

`lint` is the gate; `--quiet` keeps the log clean and lets the exit code speak.

```yaml
- name: REUSE compliance
  run: |
    cargo install comply
    comply lint --quiet
```

As a pre-commit hook over staged files only:

```sh
git diff --cached --name-only --diff-filter=ACM | xargs -r comply lint-file --quiet
```

To fail a build on formatting drift without rewriting anything:

```sh
comply format --check
```

## Library

The same crate is the library. Depend on it with `default-features = false` to
drop clap, anyhow and chrono and keep only the compliance engine:

```toml
[dependencies]
comply = { version = "0.1", default-features = false }
```

| Module | Description |
| --- | --- |
| `config` | `REUSE.toml` and `.reuse/dep5` parsing, annotation matching |
| `error` | Typed error enum built with `thiserror` |
| `file` | Discovery, classification, `.license` companions, exclusion |
| `fix` | Auto-repair of missing headers and license files |
| `hash` | SHA-256 integrity hashing |
| `header` | SPDX header detection, parsing, generation, annotation |
| `license` | Bundled SPDX License List 3.28, project license audit |
| `report` | Lint reports and SPDX bill-of-materials generation |
| `reuse_toml` | `REUSE.toml` serialization |
| `scope` | Single source of truth for which paths are covered |
| `spdx` | SPDX expression parser and validator |
| `version` | Build provenance, rendered as plain text, JSON or TOML |

Validate an SPDX expression:

```rust
comply::spdx::validate("MIT OR Apache-2.0")?;
assert!(comply::spdx::validate("MIT OR").is_err());
# Ok::<(), comply::error::Error>(())
```

Parse one and pull out the identifiers it mentions:

```rust
let expr = comply::spdx::parse("Apache-2.0 WITH LLVM-exception OR MIT")?;
let ids = expr.identifiers();

assert!(ids.contains("MIT"));
assert!(ids.contains("Apache-2.0"));
# Ok::<(), comply::error::Error>(())
```

Read SPDX tags out of file contents:

<!-- The sample below contains real SPDX tags inside a Rust string. Without
     these guards a scanner reads them as this file's own licensing and tries
     to parse `MIT\n";` as an expression. REUSE 3.3 section 2.4. -->
<!-- REUSE-IgnoreStart -->

```rust
use comply::header;

let source = "// SPDX-FileCopyrightText: 2026 Acme Inc\n\
              // SPDX-License-Identifier: MIT\n";
let found = header::detect(source).expect("both tags are present");

assert_eq!(found.license_expression.as_deref(), Some("MIT"));
assert_eq!(found.copyrights, ["2026 Acme Inc"]);
```

<!-- REUSE-IgnoreEnd -->

Check an identifier against the bundled license list:

```rust
let db = comply::license::standard_db();

assert!(db.is_known("Apache-2.0"));
assert!(!db.is_known("Definitely-Not-A-License"));
```

Walk a project and build a report:

```rust,no_run
use comply::config::Config;
use comply::{file, report::Report};

let root = std::path::Path::new("/path/to/project");
let config = Config::builder().build();
let mut builder = Report::builder();

for entry in file::walk(root, &config.ignore, &config.source_patterns) {
    builder = builder.add_ok(entry?.path.display().to_string());
}

let report = builder.build();
println!("{} files in scope", report.iter().count());
# Ok::<(), comply::error::Error>(())
```

Load a project's configuration, then repair it:

```rust,no_run
use comply::config::Config;
use comply::fix::fix_project;

let root = std::path::Path::new("/path/to/project");
let config = Config::load(root)?;

let report = fix_project(root, &config, true)?;   // true = dry run
println!("{} of {} files would change", report.fixed, report.total);
# Ok::<(), comply::error::Error>(())
```

### Sharing the covered-path definition

`scope` is public so a sibling tool can agree with comply about what counts as
a project file without copying the lists -- copies drift, and a stale copy
means a file in a virtualenv gets asked for an SPDX header:

```rust
use comply::scope::{self, IGNORED_DIRS};

assert!(IGNORED_DIRS.contains(&".git"));
assert!(scope::is_pruned_dir("target"));
assert!(!scope::is_pruned_dir("src"));
```

## Features

| Feature | Default | Description |
| --- | --- | --- |
| `cli` | yes | The `comply` binary and its clap/anyhow/chrono dependencies |
| `license-db-network` | no | Fetch the license list at runtime (not yet implemented) |

The SPDX License List is always bundled; it is not a feature. Switching it off
could only produce a library unable to validate an identifier against anything.

## Minimum supported Rust version

1.95. Raising it is a minor-version change.

## Contributing

Issues and pull requests are welcome at
<https://github.com/elioseverojunior/comply>. Read `docs/contribution.md`
first: it sets out what the project expects, including its rules on
AI-assisted contributions.

## License

MIT OR Apache-2.0, at your option.
