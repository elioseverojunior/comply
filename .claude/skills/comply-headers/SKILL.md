---
name: comply-headers
description: Check and fix REUSE/SPDX license headers in this repository using the comply binary. Use when reuse or comply lint fails, when adding a new source file that needs an SPDX header, or when the user asks about SPDX headers, REUSE compliance, or licensing annotations.
---

# comply headers

This project checks its own compliance with the `comply` binary it builds. The
binary must be on `PATH` -- `mise run build:setup` builds release and copies it
to `~/.local/bin/`.

## Check

```sh
mise run comply              # defaults to `comply lint`
mise run comply lint --json  # machine-readable
```

A healthy report ends with `0 failed`. Skipped files are expected: most files
are covered by the `**` aggregate annotation in `REUSE.toml` rather than an
inline header.

## Fix

```sh
mise run comply:fix crates/comply/src/foo.rs   # explicit paths, always
mise run comply:fix:all                        # every tracked *.rs file
```

Paths are **required** on `comply:fix` by design. The `**` aggregate annotation
in `REUSE.toml` already makes every file compliant, so inline headers are an
opt-in convention for `.rs` source files -- not a blanket operation.

The task writes a canonical 2-line block and is idempotent, so re-running it
produces no diff:

```rust
// SPDX-FileCopyrightText: 2026 COMPLY contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
```

## Hard constraints

- **Never annotate a symlink.** `LICENSE`, `LICENSE-APACHE`, and
  `CC-BY-3.0.txt` are symlinks into `LICENSES/`. `comply annotate` would write
  through the link and corrupt the canonical license text, or drop a stray
  `.license` sidecar. The `comply:fix` task filters symlinks out; do not bypass
  it by calling `comply annotate` directly.
- **Do not pass a year to `--copyright`.** comply prepends the current year
  itself, so `--copyright "2026 X"` yields `2026 2026 X`.
- Markdown and config files get **no** inline header -- the `REUSE.toml`
  aggregate covers them. Adding one is noise and will not match the repo.
- License model: code is `MIT OR Apache-2.0`, docs are `CC-BY-3.0+`. Files
  needing something else get an explicit `[[annotations]]` entry in `REUSE.toml`
  with `precedence = "override"`, not a hand-written header.

## Editing REUSE.toml

The `precedence` field matters. `aggregate` yields to an inline header if one
exists; `override` wins over it. The existing `override` block lists specific
generated and CLI files -- extend that list rather than adding a new block.
