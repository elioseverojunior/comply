<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Getting started

comply checks that every file in a project declares its copyright and licence,
following the [REUSE Specification](https://reuse.software/spec/). It is a new
implementation in Rust rather than a port of the Python `reuse` tool, and it
reads the same configuration.

## Install

```sh
cargo install comply-cli
```

The crate is `comply-cli`; the binary it installs is `comply`.

To build from a checkout:

```sh
git clone https://github.com/elioseverojunior/comply
cd comply
cargo build --release
./target/release/comply --version
```

comply needs `git` to see which files a repository tracks, and `curl` for
`comply download`. Nothing else: the SPDX licence list is compiled into the
binary, so checking works offline.

## The shape of a compliant project

REUSE asks two questions of every file: who holds the copyright, and under what
licence is it published. A file can answer in three ways.

1. **An SPDX header** in the file itself.

   ```rust
   // SPDX-FileCopyrightText: 2026 Your Name
   //
   // SPDX-License-Identifier: MIT OR Apache-2.0
   ```

2. **A `.license` companion** beside it, for files that cannot carry a comment
   -- a PNG, a binary fixture. `logo.png` is answered by `logo.png.license`.

3. **An annotation in `REUSE.toml`**, which covers many files at once. This is
   how most projects license their documentation and configuration without
   putting a header in every file.

Every licence a project uses must also exist as a text file in `LICENSES/`.

## First run

```sh
comply lint
```

```text
REUSE lint report
113 total, 112 passed, 0 failed, 1 skipped
```

The exit status carries the verdict -- `0` when compliant -- so it works as a CI
gate directly:

```sh
comply lint --quiet
```

If files are missing information, `--lines` prints one finding per line, which
is the format editors and CI annotations understand:

```sh
comply lint --lines
```

```text
src/main.rs: missing copyright
src/main.rs: missing license
```

## Making a project compliant

Start from nothing:

```sh
comply init --license "MIT OR Apache-2.0" --holder "Your Name <you@example.com>"
```

That writes a `REUSE.toml` and creates `LICENSES/`. Then fetch the licence texts
the project references but does not yet have:

```sh
comply download --all
```

Then add headers. For specific files:

```sh
comply annotate --license MIT --copyright "2026 Your Name" src/main.rs src/lib.rs
```

Or repair everything the configuration can already answer for:

```sh
comply fix --dry-run   # show what would change
comply fix             # apply it
```

`fix` only writes where it has an answer. A file with no header and no matching
annotation is reported, not guessed at.

## In pre-commit

Checking the whole project on every commit means walking every file to judge a
handful. `lint-file` scales with the change instead:

```sh
comply lint-file src/main.rs src/lib.rs
```

Wire it into a hook so it receives the staged files. Keep a full `comply lint`
in CI, because the project-wide findings -- unused licences, missing licence
texts -- are the ones `lint-file` cannot make.

## Producing a bill of materials

```sh
comply spdx --output comply.spdx
```

An SPDX 2.1 tag-value document listing every checked file with its licence and
checksums. The document namespace is derived from the file set rather than
randomly generated, so an unchanged tree produces a byte-identical document and
two runs can be diffed in review.

## Next

- [Configuration](/docs/guide/configuration) -- `REUSE.toml` and the `[tool.comply]`
  extensions.
- [CLI reference](/docs/reference/cli) -- every command and flag.
- [Parity report](/PARITY) -- how comply compares against `reuse` 6.2.0.
