<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# comply-cli

Command-line interface for comply -- check and enforce the
[REUSE Specification](https://reuse.software/spec/) for software licensing.

Installs a single binary named `comply`.

```sh
cargo install comply-cli
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

## License

MIT OR Apache-2.0
