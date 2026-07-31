<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# CLI reference

Every flag below was exercised against comply 0.1.0; the outputs shown are real
runs, not transcribed help text.

```text
Usage: comply <COMMAND>

Commands:
  init                Initialize a project for REUSE compliance
  format              Format SPDX headers consistently
  lint                Check project for REUSE compliance
  lint-file           Check individual files for REUSE compliance
  annotate            Add or update SPDX headers
  fix                 Auto-fix compliance issues
  download            Fetch licence texts into LICENSES/
  spdx                Generate an SPDX 2.1 bill of materials for the project
  convert-dep5        Convert `.reuse/dep5` into an equivalent REUSE.toml
  supported-licenses  List the SPDX identifiers this build recognises
```

## Exit codes

| Code | Meaning                                   |
| ---- | ----------------------------------------- |
| `0`  | The check passed, or the write succeeded. |
| `1`  | Non-compliant, or the operation failed.   |

Checking commands (`lint`, `lint-file`, `format --check`) carry the verdict in
the exit code, so they work as CI gates with no output parsing.

::: warning Reading the exit code through a pipe
`comply lint | tail -3; echo $?` reports `tail`'s status, not comply's. Capture
first, or test the command directly:

```sh
comply lint > report.txt; echo "$?"
comply lint --quiet && echo compliant
```

:::

## lint

Check a whole project. Aliases: `l`, `check`, `chk`, `ck`.

| Flag            | Effect                                                |
| --------------- | ----------------------------------------------------- |
| `[PATH]`        | Project directory. Defaults to the current directory. |
| `-p`, `--plain` | Human-readable report. The default.                   |
| `-l`, `--lines` | One finding per line, prefixed with the file path.    |
| `-j`, `--json`  | Machine-readable report.                              |
| `-q`, `--quiet` | No output at all; only the exit code.                 |

```sh
comply lint
```

```text
REUSE lint report
113 total, 112 passed, 0 failed, 1 skipped
```

`--lines` is the format editors and CI annotations parse:

```sh
comply lint --lines
```

```text
/path/to/main.rs: missing license 'Apache-2.0'
/path/to/main.rs: missing license 'MIT'
```

`--json` gives a summary plus a per-file array:

```json
{
  "summary": { "total": 3, "passed": 1, "failed": 0, "skipped": 2 },
  "files": [
    {
      "path": "main.rs",
      "status": "OK",
      "copyrights": ["2026 Your Name"],
      "license": "MIT OR Apache-2.0"
    }
  ]
}
```

**Only `lint` makes the project-wide findings** -- licence texts referenced but
missing from `LICENSES/`, and licence files nothing uses. `lint-file` cannot see
them, because it never looks at the whole tree.

`skipped` counts files outside REUSE analysis: licence texts, files matched by
`[tool.comply] ignore`. They are neither judged nor required to carry anything.

## lint-file

Check only the files named. Aliases: `lf`, `check-file`, `chkf`, `ckf`.

| Flag                  | Effect                                                     |
| --------------------- | ---------------------------------------------------------- |
| `[FILES]...`          | Files to check.                                            |
| `-p`, `--path <PATH>` | Project directory, for finding `REUSE.toml`.               |
| `-l`, `--lines`       | One finding per line. **The default here**, unlike `lint`. |
| `-q`, `--quiet`       | No output; exit code only.                                 |

```sh
comply lint-file src/main.rs src/lib.rs
```

The pre-commit shape: cost scales with the change rather than the repository.
Keep a full `comply lint` in CI for the findings this cannot make.

A file passes if `REUSE.toml` covers it, even with no header of its own -- that
is the point of an aggregate annotation, not a false negative.

## annotate

Add or update headers on named files. Aliases: `a`, `note`.

| Flag                            | Effect                                                                                                                                       |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `[FILES]...`                    | Files to annotate.                                                                                                                           |
| `-p`, `--path <PATH>`           | Project directory.                                                                                                                           |
| `-l`, `--license <LICENSE>`     | SPDX expression. Optional when `REUSE.toml` supplies one.                                                                                    |
| `-c`, `--copyright <COPYRIGHT>` | Copyright notice. See the year note below.                                                                                                   |
| `--holder <HOLDER>`             | Copyright holder, `"Name <email>"`. Repeatable.                                                                                              |
| `--year <YEAR>`                 | Year or range: `2020`, `"2017, 2019-2024"`.                                                                                                  |
| `--exclude-year`                | Write no year at all.                                                                                                                        |
| `--contributor <CONTRIBUTOR>`   | Adds `SPDX-FileContributor`. Repeatable.                                                                                                     |
| `-r`, `--recursive`             | Annotate everything under the given paths.                                                                                                   |
| `--force`                       | Replace an existing header, or an existing `.license` companion. Without it, a file that already has either is left alone.                   |
| `--merge-copyrights`            | Collapse duplicate copyright lines into one. Needs `--force` to touch an existing header.                                                    |
| `--force-dot-license`           | Write a `.license` companion for a name comply does not recognise. A format that genuinely has no comment syntax gets one without this flag. |
| `--skip-unrecognised`           | Skip files whose comment syntax comply does not know, instead of warning.                                                                    |
| `--update-reuse-toml <BOOL>`    | Also write an annotation into `REUSE.toml`. Default `false`.                                                                                 |
| `--precedence <PRECEDENCE>`     | Precedence for that annotation: `closest`, `aggregate`, `override`. Default `override`.                                                      |
| `--copyright-prefix <PREFIX>`   | Form the notice is written in. Default `spdx`. See [Copyright forms](#copyright-forms).                                                      |

```sh
comply annotate --license MIT --copyright "Acme" src/main.rs
comply annotate --license MIT --holder "Ann <ann@example.com>" --year 2020 src/lib.rs
comply annotate --license MIT --copyright "Acme" --contributor Ann --contributor Bo src/x.rs
comply annotate --recursive --license MIT src/
```

### Copyright forms

`--copyright-prefix` selects how the notice is introduced. All sixteen values
of the REUSE specification are supported, and each is byte-identical to what
`reuse 6.2.0` writes.

| Value                   | Line written                                            |
| ----------------------- | ------------------------------------------------------- |
| `spdx` (default)        | `SPDX-FileCopyrightText: 2026 Acme`                     |
| `spdx-c`                | `SPDX-FileCopyrightText: (C) 2026 Acme`                 |
| `spdx-symbol`           | `SPDX-FileCopyrightText: &copy; 2026 Acme`              |
| `spdx-string`           | `SPDX-FileCopyrightText: Copyright 2026 Acme`           |
| `spdx-string-c`         | `SPDX-FileCopyrightText: Copyright (C) 2026 Acme`       |
| `spdx-string-symbol`    | `SPDX-FileCopyrightText: Copyright &copy; 2026 Acme`    |
| `snippet`               | `SPDX-SnippetCopyrightText: 2026 Acme`                  |
| `snippet-c`             | `SPDX-SnippetCopyrightText: (C) 2026 Acme`              |
| `snippet-symbol`        | `SPDX-SnippetCopyrightText: &copy; 2026 Acme`           |
| `snippet-string`        | `SPDX-SnippetCopyrightText: Copyright 2026 Acme`        |
| `snippet-string-c`      | `SPDX-SnippetCopyrightText: Copyright (C) 2026 Acme`    |
| `snippet-string-symbol` | `SPDX-SnippetCopyrightText: Copyright &copy; 2026 Acme` |
| `string`                | `Copyright 2026 Acme`                                   |
| `string-c`              | `Copyright (C) 2026 Acme`                               |
| `string-symbol`         | `Copyright &copy; 2026 Acme`                            |
| `symbol`                | `&copy; 2026 Acme`                                      |

The `-symbol` forms write U+00A9, the copyright sign. There is deliberately no
bare `(C)` form: with no tag and no leading `Copyright`, nothing marks the line
as a copyright notice, so neither comply nor the reference tool will write one.

::: warning The prefix stops at the header
`--copyright-prefix` is a rendering choice for inline headers. With
`--update-reuse-toml`, the annotation keeps the bare notice -- its
`SPDX-FileCopyrightText` key already names the field.
:::

### The year is added for you

comply prepends a year to `--copyright` and `--holder`: the value of `--year`
when given, otherwise the current year.

| Command                          | Result                        |
| -------------------------------- | ----------------------------- |
| `--copyright "Acme"`             | `2026 Acme`                   |
| `--copyright "Acme" --year 2020` | `2020 Acme`                   |
| `--holder "Acme <a@b>"`          | `2026 Acme <a@b>`             |
| `--copyright "2020 Acme"`        | `2026 2020 Acme` -- see below |

::: warning Do not put a year in `--copyright`
The year is prepended unconditionally, so a value that already starts with one
is doubled -- `--copyright "2020 Acme"` gives `2026 2020 Acme`. Pass the bare
name and let `--year` supply the date.

This matches `reuse 6.2.0` exactly, which does the same doubling. It is a
compatibility choice, not a defect. `--exclude-year` strips the year back off,
and `--holder` is unaffected: it is designed to take a name, so prefixing a
year there is correct.
:::

### Existing headers are kept

Without `--force`, a file that already carries a header is returned unchanged.
This is why `--merge-copyrights` looks inert on its own -- it has nothing to
rewrite until `--force` lets it.

```sh
comply annotate --license MIT --copyright Acme --merge-copyrights --force dup.rs
```

```text
// SPDX-FileCopyrightText: 2020 Acme      <- was two identical lines
//
// SPDX-License-Identifier: MIT
```

### Files that cannot carry a comment

A format with no comment syntax -- a PNG, a font, an SVG, a `.json` -- gets a
`.license` companion beside it rather than a header, with no flag at all:

```sh
comply annotate --license MIT --copyright Acme logo.png
```

```text
Updated (as .license): logo.png
```

`logo.png.license` is written and the file itself is not touched. A recursive
run reaches these files too, so `comply annotate --recursive .` companions
every such file in the tree.

The companion's lines carry no comment prefix at all -- they are bare tags,
which is what `comply fix` writes and what the reference tool writes:

```text
SPDX-FileCopyrightText: 2026 Acme

SPDX-License-Identifier: MIT
```

`--force-dot-license` is for the other case: a _name_ no built-in table claims,
which comply refuses rather than guessing a syntax for. The flag forces a
companion there instead.

```sh
comply annotate --license MIT --copyright Acme --force-dot-license notes.txt
```

A companion already on disk is left where it is and reported, because it is
somebody's licensing declaration and replacing it drops their copyright where
`lint` will not notice:

```text
Unchanged (as .license): logo.png
```

`--force` is what replaces one, exactly as it is what replaces an inline
header. `--force-dot-license` on its own does not.

::: warning Never annotate a symlink
Writing through a link edits its target. If `LICENSE-MIT` points into
`LICENSES/`, annotating it rewrites the canonical licence text every other file
cites. Put such paths in `[tool.comply] ignore`.
:::

Headers go **below** a shebang, an XML declaration, or YAML frontmatter, never
above -- prepending there breaks the file instead of annotating it.

## fix

Repair everything the configuration can already answer for. Aliases: `f`, `fx`.

| Flag        | Effect                                   |
| ----------- | ---------------------------------------- |
| `[PATH]`    | Project directory.                       |
| `--dry-run` | Report what would change; write nothing. |

```sh
comply fix --dry-run
```

```text
header reformatted                       src/lib.rs

Summary: 13 files, 2 fixed, 11 unchanged, 0 failed
```

`fix` never guesses. A file with no header and no matching annotation is
reported as failed, not invented for. A file comply cannot parse is reported and
the rest of the tree is still repaired -- one bad file does not abort the run.

## format

Canonicalise headers that already exist. Alias: `fmt`.

| Flag                    | Effect                                                      |
| ----------------------- | ----------------------------------------------------------- |
| `-p`, `--path <PATH>`   | Project directory.                                          |
| `-f`, `--files <FILES>` | Specific files. Defaults to all source files.               |
| `--check`               | Report without writing. Exits `1` if anything would change. |

```sh
comply format --check      # CI gate; exit 1 when unformatted
comply format              # rewrite in place
```

```text
Would reformat: src/lib.rs
2 file(s) would be reformatted
```

`format` never invents licensing. A header with a copyright but no licence is
left alone and reported by `lint`, because writing `NOASSERTION` would assert a
licence the project never chose.

## init

| Flag                        | Effect                                            |
| --------------------------- | ------------------------------------------------- |
| `[PATH]`                    | Project directory.                                |
| `-l`, `--license <LICENSE>` | Default `MIT OR Apache-2.0`.                      |
| `--holder <HOLDER>`         | `"Name <email>"`. Repeatable.                     |
| `--config-name <NAME>`      | `REUSE.toml`, `reuse.toml` or `.reuse.toml`.      |
| `--force`                   | Overwrite an existing manifest and licence files. |

```sh
comply init --license "MIT OR Apache-2.0" --holder "Ann <ann@example.com>"
```

Writes `REUSE.toml` with a `**` aggregate annotation and a starter
`[tool.comply] ignore`, plus root licence files.

::: tip init alone does not make a project compliant
It does not populate `LICENSES/`, so the next `lint` reports the licence texts
as missing and exits `1`. Follow it with `comply download --all`.
:::

## download

Fetch licence texts into `LICENSES/`.

| Flag                      | Effect                                             |
| ------------------------- | -------------------------------------------------- |
| `[IDS]...`                | SPDX identifiers to fetch.                         |
| `--all`                   | Fetch exactly what `lint` reports missing.         |
| `-p`, `--path <PATH>`     | Project directory.                                 |
| `-o`, `--output <OUTPUT>` | Target directory. Defaults to `LICENSES/`.         |
| `--source <SOURCE>`       | URL or path template; `{}` becomes the identifier. |
| `--force`                 | Overwrite a licence file that already exists.      |

```sh
comply download MIT Apache-2.0
comply download --all
comply download --source ./licence-cache/{}.txt MIT     # no network
```

```text
MIT: written to /path/to/LICENSES/MIT.txt
```

Transfers are delegated to `curl` rather than linking an HTTP/TLS stack -- a
supply-chain choice, since the project carries no `cargo-vet` exemptions and a
TLS client would add a large subtree needing fresh audits for a feature used
once per licence. `curl` must therefore be on `PATH`; `--source` with a local
template removes even that requirement.

## spdx

Emit an SPDX 2.1 tag-value bill of materials.

| Flag                            | Effect                             |
| ------------------------------- | ---------------------------------- |
| `[PATH]`                        | Project directory.                 |
| `-o`, `--output <OUTPUT>`       | File to write. Defaults to stdout. |
| `--creator-person <NAME>`       | Person signing off.                |
| `--creator-organization <NAME>` | Organization signing off.          |

```sh
comply spdx --output comply.spdx --creator-person "Ann"
```

Three deliberate differences from `reuse spdx`:

- Checksums carry `SHA256` alongside the spec-required `SHA1`.
- `DocumentNamespace` is derived from the file set rather than a random UUID, so
  an unchanged tree yields a byte-identical document that can be diffed.
- Skipped files are omitted rather than listed as `NOASSERTION`. The file count
  equals the number `lint` reports checked.

## convert-dep5

| Flag        | Effect                                        |
| ----------- | --------------------------------------------- |
| `[PATH]`    | Project directory.                            |
| `--dry-run` | Print the `REUSE.toml` that would be written. |

Refuses to overwrite an existing `REUSE.toml`, which may hold annotations the
conversion knows nothing about.

## supported-licenses

| Flag           | Effect                                           |
| -------------- | ------------------------------------------------ |
| `--json`       | Output as JSON.                                  |
| `--exceptions` | List licence exceptions instead of licences.     |
| `--deprecated` | List deprecated identifiers instead of licences. |

```sh
comply supported-licenses | grep -i apache
comply supported-licenses --deprecated
```

The list is compiled into the binary, so this works offline and reports exactly
what this build accepts.

## version

| Flag                                 | Effect                                                  |
| ------------------------------------ | ------------------------------------------------------- |
| `--full`                             | Include build provenance and the full gitversion stamp. |
| `-o`, `--output <plain\|json\|toml>` | Output format. Defaults to `plain`.                     |

```sh
comply version                 # v0.0.1-1
comply version --full          # provenance + the gitversion stamp
comply version --full -o json  # machine-readable
```

The bare form prints `v<SemVer>` resolved from the GitVersion stamp baked in at
build time, falling back to the package version when gitversion was unavailable
on the build machine.

`--full` adds the commit (suffixed `-dirty` if the tree had local changes), the
build timestamp, the `rustc` that compiled it, and the target triple, followed by
every field of the stamp. `MajorMinor` is synthesized the same way
`mise run version:json` does it, since raw gitversion does not emit that key.

`-o toml` drops stamp entries whose value is null, because TOML has no null. The
JSON form keeps them, and keeps numbers as numbers.

The build timestamp honours `SOURCE_DATE_EPOCH`, so two builds of identical
source report the same value.
