<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# comply

Check and enforce the [REUSE Specification](https://reuse.software/spec/) for
software licensing -- a command-line tool and the library behind it. A native
implementation rather than a port of the Python reuse-tool, reading only the
project tree it is pointed at.

```sh
cargo install comply
```

That installs a single binary, `comply`. The same crate is the library: depend
on it with `default-features = false` to drop clap, anyhow and chrono and keep
only the compliance engine.

```toml
[dependencies]
comply = { version = "0.0.1", default-features = false }
```

## Commands

| Command | Purpose |
|---------|---------|
| `init` | Initialize a project for REUSE compliance |
| `format` | Format SPDX headers consistently |
| `lint` | Check a project for REUSE compliance |
| `lint-file` | Check individual files for REUSE compliance |
| `annotate` | Add or update SPDX headers |
| `fix` | Auto-fix compliance issues |
| `download` | Fetch license texts into `LICENSES/` |
| `spdx` | Generate an SPDX 2.1 bill of materials |
| `convert-dep5` | Convert `.reuse/dep5` into an equivalent REUSE.toml |
| `supported-licenses` | List the SPDX identifiers this build recognizes |

### `init`

```sh
comply init /path/to/project
comply init --license MIT --holder "Acme Inc <legal@acme.example>" .
```

Creates a `REUSE.toml` manifest and a `LICENSES/` directory. `--config-name`
selects the manifest spelling (`REUSE.toml`, `reuse.toml`, `.reuse.toml`) and
`--force` overwrites an existing one.

### `lint`

```sh
comply lint                    # current directory
comply lint /path/to/project
```

Output format is chosen with `--plain` (default), `--json`, `--lines`, or
`--quiet`. Exit codes: 0 = compliant, 1 = non-compliant.

### `lint-file`

```sh
comply lint-file src/main.rs src/lib.rs
comply lint-file --path /path/to/project src/main.rs
```

Judges only the files named. Output is `--lines` by default, or `--quiet` to
rely on the exit code alone. Paths outside the covered set are skipped rather
than reported as failures.

### `annotate`

```sh
comply annotate --license MIT --copyright "Acme Inc" src/main.rs

# Recurse, merging duplicate copyright lines
comply annotate -l Apache-2.0 -c "Acme Inc" --recursive --merge-copyrights src/

# Year ranges, and a companion file instead of an inline header
comply annotate -l MIT -c "Acme Inc" --year "2017, 2019-2024" logo.png \
  --force-dot-license
```

`--license` and `--copyright` are optional when REUSE.toml already supplies
them. `--copyright-prefix` selects the notice style (`spdx`, `spdx-c`,
`spdx-symbol`, `string`, `symbol`), and `--skip-unrecognised` passes over files
whose comment syntax comply does not know.

### `format`

```sh
comply format --path /path/to/project
comply format --files src/main.rs --files src/lib.rs
comply format --check
```

Normalizes header ordering, comment style, and whitespace. `--check` reports
without writing, for use as a CI gate. Note that `format` takes `--path`; it
has no positional argument.

### `fix`

```sh
comply fix --dry-run /path/to/project
comply fix /path/to/project
```

Fixes missing headers, missing license files, and missing `.license`
companions.

### `download`

```sh
comply download MIT Apache-2.0
comply download --all               # every license the project references
comply download --all --force       # overwrite existing files
```

Writes license texts into `LICENSES/`, or into `--output`. `--source` takes a
URL or path template in which `{}` is replaced with the identifier, which is
also how the test suite stays offline.

### `spdx`

```sh
comply spdx                             # bill of materials to stdout
comply spdx --output sbom.spdx
comply spdx --creator-organization "Acme Inc"
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

## Configuration

comply reads its own settings from a `[tool.comply]` table in `REUSE.toml`.
Everything here is optional.

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
project root down to the file's own directory. Because they share one matcher, a
`!negation` in the repository's `.gitignore` overrides the global excludes file,
exactly as it does for git.

## Typical workflow

```sh
comply init my-project
cd my-project
comply download MIT
comply annotate --license MIT --copyright "Acme Inc" --recursive src/
comply lint
```

## Library

| Module | Description |
|--------|-------------|
| `config` | REUSE.toml and `.reuse/dep5` parsing, annotation matching |
| `error` | Typed error enum built with `thiserror` |
| `file` | Discovery, classification, `.license` companions, exclusion |
| `fix` | Auto-repair of missing headers and license files |
| `hash` | SHA-256 integrity hashing |
| `header` | SPDX header detection, parsing, generation, annotation |
| `license` | Bundled SPDX License List 3.28, project license audit |
| `report` | Lint reports and SPDX bill-of-materials generation |
| `reuse_toml` | REUSE.toml serialization |
| `scope` | Single source of truth for which paths are covered |
| `spdx` | SPDX expression parser and validator |

Validate an SPDX expression:

```rust
comply::spdx::validate("MIT OR Apache-2.0")?;
assert!(comply::spdx::validate("MIT OR").is_err());
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
|---------|---------|-------------|
| `cli` | yes | The `comply` binary and its clap/anyhow/chrono dependencies |
| `license-db-network` | no | Fetch the license list at runtime (not yet implemented) |

The SPDX License List is always bundled; it is not a feature. Switching it off
could only produce a library unable to validate an identifier against anything.

## License

MIT OR Apache-2.0
