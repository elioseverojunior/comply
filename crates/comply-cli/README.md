# comply-cli

Command-line interface for the comply REUSE compliance tool -- check and
enforce the [REUSE Specification](https://reuse.software/spec/) for software
licensing.

## Commands

### `init`

Initialize a project for REUSE compliance:

```sh
comply init /path/to/project
```

Creates `.reuse/` directory with `REUSE.toml` manifest and `LICENSES/`
directory for license texts.

### `lint`

Check a project for REUSE compliance:

```sh
# Check the current directory
comply lint

# Check a specific project
comply lint /path/to/project

# JSON output
comply lint --json

# SPDX Bill of Materials output
comply lint --spdx
```

Exit codes: 0 = compliant, 1 = non-compliant.

### `annotate`

Add or update SPDX headers on files:

```sh
comply annotate --license MIT --copyright "2026 Acme Inc" src/main.rs

# Annotate multiple files
comply annotate --license Apache-2.0 --copyright "2026 Acme Inc" src/*.rs

# Use a different comment style
comply annotate --style reuse --license MIT --copyright "2026 Acme Inc" src/main.rs
```

### `format`

Format all SPDX headers consistently:

```sh
comply format /path/to/project
```

Normalizes header ordering, line endings, comment style, and whitespace.

### `fix`

Auto-fix common compliance issues:

```sh
# Dry run (show what would be fixed)
comply fix --dry-run /path/to/project

# Apply fixes
comply fix /path/to/project
```

Fixes missing headers, invalid SPDX expressions, missing license files,
incorrect DEP5 coverage, and missing `.license` companion files.

## Typical workflow

```sh
# 1. Initialize a project
comply init my-project

# 2. Add license files
cp LICENSE.txt my-project/LICENSES/MIT.txt

# 3. Annotate source files
comply annotate --license MIT --copyright "2026 Acme Inc" my-project/src/*.rs

# 4. Check compliance
comply lint my-project
```

## License

MIT OR Apache-2.0
