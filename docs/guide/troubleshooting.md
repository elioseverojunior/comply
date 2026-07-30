<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Troubleshooting

Symptoms seen in practice, and what each one actually means.

## Exit codes

### `$?` is 0 but the output says files failed

The exit code came from the last command in a pipeline, not from comply.

```sh
comply lint | tail -3; echo $?     # reports tail's status -- always 0
```

Capture the output, or test the command directly:

```sh
comply lint > report.txt; echo "$?"
comply lint --quiet && echo compliant
```

This bites `format --check` most often, whose entire purpose is the exit code.

## Lint results

### `Missing licenses: MIT, Apache-2.0` right after `comply init`

`init` writes the manifest but does not populate `LICENSES/`. Every licence a
project references needs a text file there.

```sh
comply download --all
```

`--all` fetches exactly what `lint` reported missing.

### `Unused licenses: <ID>`

A file exists in `LICENSES/` that nothing declares. Either something should be
using it, or the text is left over from a licence you have dropped.

A licence declared only through an annotation still counts as used, including
for a file listed in `[tool.comply] ignore` -- comply reads the annotation
covering it. If a licence still reports unused, nothing in the manifest names
it.

### A file is reported as `skipped`, not checked

It is outside REUSE analysis. That is one of:

- a licence text (`LICENSES/`, root `LICENSE*` files),
- the manifest itself,
- an SPDX document,
- a `.license` companion, judged through the file it describes,
- a path matched by `[tool.comply] ignore`,
- a symlink -- never followed, because the target is covered in its own right.

Skipped files are counted in the total but require nothing.

### `lint-file` passes a file with no header

Correct, if `REUSE.toml` covers it. An aggregate annotation is a real
declaration; requiring a header as well would defeat the point of having one.

### `lint` finds problems `lint-file` did not

By design. Unused licences and missing licence texts are project-wide findings
that need the whole tree. Run `lint-file` in a pre-commit hook and `lint` in CI.

## Annotating

### The copyright year appears twice

```text
// SPDX-FileCopyrightText: 2026 2026 Acme
```

comply prepends a year to `--copyright` unconditionally, so a value that already
carries one is doubled. Pass a bare name:

```sh
comply annotate --license MIT --copyright "Acme" --year 2026 file.rs
```

`reuse` behaves identically, so this is a compatibility choice rather than a
bug. `--exclude-year` strips the year back off. See
[The year is added for you](/reference/cli#the-year-is-added-for-you).

### The notice is written with the wrong marker

`--copyright-prefix` chooses among the sixteen forms the specification allows:

```sh
comply annotate --license MIT --copyright Acme --copyright-prefix string-c file.rs
```

```text
// Copyright (C) 2026 Acme
```

The full table is in [Copyright forms](/reference/cli#copyright-forms). The
prefix applies to the header only -- with `--update-reuse-toml`, the annotation
keeps the bare notice.

### `annotate` reports success but the file is unchanged

The file already had a header. Without `--force`, comply keeps it rather than
overwriting a declaration someone made deliberately.

```sh
comply annotate --license MIT --copyright Acme --force file.rs
```

This is also why `--merge-copyrights` can look inert -- it has nothing to
rewrite until `--force` lets it.

### A licence text got overwritten with an SPDX header

`annotate` was pointed at a symlink. Writing through a link edits its target, so
annotating a root `LICENSE-MIT` that points into `LICENSES/` rewrites the
canonical text.

Recover with git, then add the path to `[tool.comply] ignore`:

```toml
[tool.comply]
ignore = ["LICENSE-*", "crates/*/LICENSE-*"]
```

`--recursive` over a directory holding such links is the usual way this happens.

### A TOML or Markdown file was mangled

Older versions treated any line carrying an SPDX tag as a header, including tags
inside a string literal or a fenced code block, and rewrote around them. comply
now requires a header to be *in a comment*.

If a file legitimately contains SPDX-shaped text that must not be touched, guard
it:

```text
REUSE-IgnoreStart
... text containing SPDX tags ...
REUSE-IgnoreEnd
```

Do not nest those guards. An inner `REUSE-IgnoreEnd` closes the outer block and
silently unguards the remainder of the file.

### A header landed above the shebang

Fixed. Headers are inserted below a shebang or YAML frontmatter, because
prepending there stops the kernel finding the interpreter and turns frontmatter
into a setext heading.

## Configuration

### A `[[tool.comply.comment-styles]]` rule is refused

```text
Error: two [[tool.comply.comment-styles]] rules claim the extension `vue`;
remove one, or lookup order would silently decide
```

Two rules claim the same key. This is refused at parse time rather than
resolved by position, so the order of the table never becomes an invisible
precedence rule. Extensions compare case-insensitively; filenames compare
exactly.

### A configured comment style is written but not recognised on the next run

That was a defect where a style comply wrote was not matched on the way back in,
so a second run stacked a second header. Fixed: rendering, comment detection,
tag stripping and span detection all derive from the same rule table.

If you still see it, check the rule declares the same delimiters the file
actually uses -- a `close` that does not match means comply sees a line style
where the file has a block.

### Files under an ignored directory still appear

`ignore` takes globs, so a directory needs `dir/**` rather than `dir`.

## Environment

### `could not run curl; install it or pass --source with a local path`

`download` shells out to `curl`, which is not on `PATH`. Either install it, or
avoid the network entirely:

```sh
comply download --source ./licence-cache/{}.txt MIT
```

This also appears in containers: an image that runs comply's test suite needs
`curl`, and a minimal base image will not have it.

### Results differ between a workstation and CI

Two usual causes.

**Coverage attribution.** tarpaulin attributes closure and const-fn bodies
differently per platform, so a macOS run can report a different figure from the
Linux run CI uses -- and the difference is not always in your favour: a tree CI
measured at 99.77% reported 100% locally. There is no local reproduction any
more; `mise run docker:test` claimed to be one and was not, reporting green on
the exact tree CI failed, so it was removed rather than left to be trusted.
Reproduce with `act -W .github/workflows/tests/act-ci.yml`, which does agree
with CI.

**gitignore state.** comply honours gitignore rules but never drops a file git
tracks, since git does not apply those rules to tracked files either. A file
ignored locally but committed is still checked.
