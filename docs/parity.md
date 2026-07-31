<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Parity report -- comply against the reuse tool

Differential check: comply 0.1.0 vs reuse 6.2.0.

Both tools run over the comply repository itself. The question is not whether
each passes, but whether they say the same thing about the same files.

## Verdict

**Full agreement on `lint`.** 109 files seen by both, zero disagreements. comply
sees a strict subset of reuse's file set, and the only difference is its own
`[tool.comply] ignore` config.

`annotate` is where the two still part company. Nothing recorded below changes a
lint verdict -- neither the comment-style tables nor the uncommentable tables are
consulted by `lint` -- but they decide what `annotate` writes, and they are not
identical. See "Uncommentable formats" below.

## Where they agree

| Measure                | Result |
| ---------------------- | ------ |
| Files seen by both     | 109    |
| Licence disagreements  | 0      |
| Unused licences (both) | 0      |
| Files only comply sees | 0      |
| Exit codes             | 0 / 0  |

Both report the same three licences in use -- `Apache-2.0`, `CC-BY-3.0`, `MIT`
-- and neither finds an unused or missing licence text. Comparing the SPDX
identifier set per file across all 109 shared files yields no mismatch.

The `CC-BY-3.0` agreement is recent. comply previously reported it unused
because `DCO.txt`, its only carrier, sits in `[tool.comply] ignore` and was
dropped from licence accounting along with the walk. That is now fixed; see
`collect_declared_by_ignored` in `commands/lint.rs`.

## Where they differ

| Class                  | Files                   | Verdict                                                                                                                       |
| ---------------------- | ----------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| By design              | 7 -- seen only by reuse | comply honours `[tool.comply] ignore`; reuse has no such config.                                                              |
| Representation         | all shared files        | Same identifiers, different `LicenseInfoInFile` shape.                                                                        |
| Comment styles         | `annotate` only         | comply claims 19 extensions reuse rejects, and picks a different syntax for `.c`, `.h` and `.ini`. Lint output is unaffected. |
| Uncommentable formats  | `annotate` only         | Same contract, different tables. comply writes a companion for 20 formats reuse refuses and refuses 44 reuse writes one for.  |
| Existing companion     | `annotate` only         | reuse merges the old notice with the new; comply preserves and reports, and `--force` replaces. Open gap.                     |
| Companion line endings | `annotate` only         | Identical on binaries. reuse mirrors CRLF for text-decodable files such as `.json`; comply always writes LF.                  |
| `fix` vs `annotate`    | comply-internal         | Closed. Both now call `header::generate`, so they emit the same bytes -- see below.                                           |

## Detail

### By design: seven files only reuse sees

```text
.actrc                      instructions.md
Cargo.lock                  mise.lock
DCO.txt                     supply-chain/imports.lock
fuzz/Cargo.lock
```

Every one is an entry in `[tool.comply] ignore`: `.actrc`, `instructions.md`,
`DCO.txt`, and the `**/*.lock` glob. `ignore` is a comply extension with no
counterpart in the REUSE specification, so reuse walks them and comply does
not. Expected, and the divergence disappears if the config is removed.

### Fixed: three files only comply saw

```text
LICENSES/Apache-2.0.txt
LICENSES/CC-BY-3.0.txt
LICENSES/MIT.txt
```

The first read of this was wrong. It looked like symlink resolution -- the root
holds three links into `LICENSES/` -- but `out_of_scope` already rejects
symlinks. The cause was elsewhere.

The walk prunes `LICENSES/` as a _directory_ and never descends, so the texts
inside it never reach the seen-set. git tracks them all the same, and comply
backfills tracked files the traversal dropped, because a gitignore rule must
not hide a file git considers part of the project.

That backfill calls `out_of_scope` with `is_dir: false`, and the directory rule
is guarded by exactly that flag:

```rust
if is_dir && is_pruned_dir(&file_name) { return true; }   // directories only

.filter(|path| !out_of_scope(path, root, false, ...))     // backfill: is_dir = false
```

So the one check that would have caught it was skipped, and the file came back.
The sibling predicate `is_excluded` already had the component-wise check, with
a comment naming this very hazard -- "a named path was never descended into, so
its ancestors have to be checked here". The backfill has the same property and
did not use it.

The visible damage: comply asserted `LicenseInfoInFile: MIT OR Apache-2.0` for
`LICENSES/MIT.txt` via the `**` aggregate, claiming the MIT licence text is
itself covered by the project's dual licence.

Fixed by lifting the ancestor check into `scope::is_within_pruned_dir`, so the
walk-time and named-path rules share one definition instead of two, and
applying it in the backfill.

### Comment styles: a superset, and two disagreements

The extension and filename tables in `resources/comment-styles.toml` were
captured from `reuse annotate` 6.2.0 and then extended. They are not identical
to it. Every claim below was measured against reuse 6.2.0's
`EXTENSION_COMMENT_STYLE_MAP` and confirmed by running `reuse annotate` on a
file of each type.

comply claims 19 extensions reuse does not recognise at all:

```text
ada cfg cmd conf cts dockerfile edn elm htm hxx
lhs makefile mts pas pm pp psd1 xhtml xslt
```

and six filenames: `GNUmakefile`, `Vagrantfile`, `Brewfile`, `Podfile`,
`.profile`, `.bash_profile`. A further sixteen extensions comply calls
uncommentable are also unknown to reuse; those are counted in the
"Uncommentable formats" table below rather than here.

None of these can make comply write a _wrong_ header. reuse answers `None` and
refuses the file, so the divergence is between "annotated" and "refused", not
between two syntaxes. `.mts`/`.cts` are the case that prompted the work --
refusing to annotate a TypeScript file because of its module suffix is a gap in
that tool rather than a rule of the specification -- and `.cmd` is the same
argument for a Windows script whose interpreter treats it exactly as `.bat`.
Parity is owed on lint output, which never consults these tables.

Two extensions both tools claim get a different syntax:

| Extension  | reuse 6.2.0         | comply    |
| ---------- | ------------------- | --------- |
| `.c`, `.h` | `/*` `*` `*/` block | `//` line |
| `.ini`     | `;`                 | `#`       |

reuse maps `.c` and `.h` to `CCommentStyle`, a bracketed block, while every
other C-family extension it knows (`.cc`, `.cpp`, `.cxx`, `.hpp`) gets `//`.
comply uses `//` throughout. Both are valid C and neither tool rewrites the
other's header, but a tree annotated by both ends up carrying two shapes.
`.ini` is the same class of thing: reuse writes `;`, comply writes `#`, and
common INI parsers read both as comments.

Neither is being changed to match. They are recorded here so the claim that
these tables agree with reuse is not made a third time.

Fortran (`.f90`, `.f95`, `.f03`, `.f08`, `!` line comments) and `.bat`
(`REM` line comments) are not part of the superset -- reuse already
annotates both, and comply's tables previously did not, which was a gap
closed rather than a divergence introduced. `.svg` was a third disagreement
until recently; see "Uncommentable formats" below.

More important is what the two now agree on. comply used to answer `Hash` for
any extension no table claimed, so `.mts` was annotated `#` and the TypeScript
stopped parsing; `.tsx`, `.jsx`, `.mjs`, `.cjs`, `.vue`, and `.lua`
had the same defect. reuse refuses in exactly that case:

```text
Error: The following files do not have a recognised file extension.
Please use '--style', '--force-dot-license', '--fallback-dot-license',
or '--skip-unrecognised'
```

comply now resolves an unclaimed name to `CommentStyle::Uncommentable`, which
routes into the branch it already had for binaries -- warn, or honour
`--skip-unrecognised` / `--force-dot-license`. A guess is never written.

The escape hatch differs. reuse takes `--style` per invocation; comply takes a
`[[tool.comply.comment-styles]]` rule in the manifest, checked before the
built-ins, so the declaration is committed once rather than repeated on every
command. `hk.pkl` and `.gitmessage` in this repository are declared that way --
neither tool's built-ins claim them.

### Uncommentable formats: same contract, different tables

reuse splits what comply had merged. `UncommentableCommentStyle` marks a format
that _has_ no comment syntax -- 36 extensions and 14 filenames, `.png` and
`.json` among them. Such a file gets a `.license` companion with no flag at all,
and `--skip-unrecognised` does not reach it, because it was never unrecognised.
An extension no table claims is a different thing entirely and is refused:

```text
Error: The following files do not have a recognised file extension.
Please use '--style', '--force-dot-license', '--fallback-dot-license',
or '--skip-unrecognised'
```

comply resolves both to `CommentStyle::Uncommentable`, so `annotate` warned for
both and wrote a companion for neither without `--force-dot-license`. A walked
binary fared worse: the target loop kept only `FileType::Source`, so it was
dropped before the branch could see it and the run said nothing at all.

The distinction reuse draws already existed here, one layer down -- `classify`
in `file/classify.rs` calls exactly these formats `FileType::Binary`. `annotate`
now consults it, which restores the contract without a second style variant.
The companion carries no comment prefix on its lines:

```text
SPDX-FileCopyrightText: 2026 X

SPDX-License-Identifier: MIT
```

On a genuine binary that is byte-for-byte what reuse writes, verified against a
valid PNG. It is not byte-for-byte everywhere; see "Line endings" below.

What remains is table membership, not mechanism:

| Direction                             | Count | Examples                                                      |
| ------------------------------------- | ----- | ------------------------------------------------------------- |
| Companion in comply, refused by reuse | 20    | `.zip`, `.so`, `.exe`, `.ttf`, `.gz`, `.class`                |
| Companion in reuse, refused by comply | 44    | `.csv`, `.webp`, `.xlsx`, `.ipynb`, `Cargo.lock`, `yarn.lock` |
| Companion in both                     | 6     | `.png`, `.jpg`, `.jpeg`, `.pdf`, `.json`, `.svg`              |

comply's list is the better call in the first row -- an object file or a zip
archive plainly cannot carry a comment, and refusing to annotate one helps
nobody. The second row is a genuine gap, and closing it means editing
`is_binary_extension`, which also moves those files out of `lint`'s
inline-header expectation. That is a wider change than `annotate` and has not
been made.

`.svg` used to be the one entry in the third row the two treated differently:
reuse wrote a companion, comply wrote an XML comment into the file. SVG does
support a comment, but the inline writer prepends at byte 0, and an exported SVG
opens with `<?xml version="1.0"?>`, which XML requires to be the first thing in
the document. The result was a file no parser accepts:

```text
logo.svg:7: parser error : XML declaration allowed only at the start of the
document
```

`lint` passed on it either way, so nothing downstream caught it. `.svg` now
lives in the `[uncommentable]` table, which is what reuse answers and what
`classify` already said, so the file is left alone and gets a companion.

`.xml`, `.xhtml`, `.xsl` and `.xslt` are annotated in place by both tools, and
the same defect reached them: comply prepended above the declaration, reuse did
not. `preamble_lines` now treats `<?xml` on line 1 the way it already treated
`#!` and YAML frontmatter, so the two agree byte for byte:

```text
<?xml version="1.0"?>

<!--
SPDX-FileCopyrightText: 2026 X

SPDX-License-Identifier: MIT
-->

<root/>
```

### An existing companion is preserved, not merged

Both tools leave the annotated file alone and act on the companion. They do not
agree on what to do when one is already there. Given a `logo.png.license`
reading `2019 Original Author` / `GPL-3.0-only`, and
`annotate --copyright X --license MIT --year 2026`:

```text
reuse:   SPDX-FileCopyrightText: 2019 Original Author
         SPDX-FileCopyrightText: 2026 X

         SPDX-License-Identifier: GPL-3.0-only
         SPDX-License-Identifier: MIT          <- merged

comply:  Unchanged (as .license): logo.png     <- preserved, nothing written
```

comply preserves and reports; `--force` replaces. That is the rule its inline
path has always applied to a file that already carries a header, and `fix`
applies the stronger form of it -- an existing companion is `Unchanged` there
with no escape hatch at all. A third policy for the same artefact would be worse
than the divergence.

Merging is the fuller answer and reuse's is the better output, but comply has no
merge anywhere in it: unioning two SPDX expressions and de-duplicating copyright
lines is a subsystem, not an edit, and it is not being invented under a bug fix.
Open gap.

The severity is asymmetric, which is why the conservative side was taken. A
missed merge is visible -- the run says `Unchanged` and the old notice is still
there to read. The other direction erases a copyright holder's declaration
silently, and `lint` passes on the result, so nothing downstream catches it.

### Line endings: mirrored for text, `os.linesep` for binaries

reuse picks the companion's newline from the annotated file, but only when that
file decodes as text (`cli/annotate.py`):

```python
encoding = detect_encoding(chunk)
newline = detect_newline(chunk, encoding=encoding) if encoding is not None else os.linesep
```

A genuine binary returns `None` from `detect_encoding`, so `detect_newline` is
never reached and the companion gets `os.linesep`. A real PNG signature does
contain `\r\n`, but it never reaches the sniffer -- an 8-byte stub that decodes
as text does, which is how this was first mis-read in the other direction.

comply always writes LF. Measured:

| Case                                 | Result                           |
| ------------------------------------ | -------------------------------- |
| Valid PNG                            | identical (both LF on POSIX)     |
| CRLF `.json`                         | differs -- comply LF, reuse CRLF |
| `--force-dot-license` over CRLF text | differs -- comply LF, reuse CRLF |

`.json` is in comply's `FileType::Binary` set
(`is_uncommentable_text_extension`), so the middle row sits on the default
no-flag path rather than in a corner. On Windows the first row would differ too,
since `os.linesep` is CRLF there.

### `fix` and `annotate` now write the same companion

Closed. `fix_binary_file` (`crates/comply/src/fix.rs`) now calls
`header::generate` instead of hand-rolling the two tags with `format!` --
the same generator `annotate`'s Uncommentable branch reaches through
`generate_with`. The missing blank line between the tags is gone, and there
is now exactly one construction of companion content in the codebase.

Verified against reuse 6.2.0 with a genuine PNG (valid IHDR/IDAT/IEND, not a
short stub -- reuse's encoding detection reads a short file as text and mirrors
its newline convention, which would have hidden the CRLF question rather than
answering it): `reuse annotate`, `comply fix`, and `comply annotate` all wrote
the identical 61-byte companion:

```text
SPDX-FileCopyrightText: 2026 X

SPDX-License-Identifier: MIT
```

`crates/comply-cli/src/commands/fix.rs`'s
`fix_and_annotate_write_byte_identical_companions` test pins the two commands
to each other so this cannot drift again.

### Representation: one expression, or one line per identifier

For a file licensed `MIT OR Apache-2.0`, the two tools write the same fact
differently:

```text
comply:  LicenseInfoInFile: MIT OR Apache-2.0

reuse:   LicenseInfoInFile: Apache-2.0
         LicenseInfoInFile: Apache-2.0
         LicenseInfoInFile: MIT
         LicenseInfoInFile: MIT
```

The identifier sets are identical, so no information differs. Worth noting on
both sides: `LicenseInfoInFile` is a repeatable field whose values are
individual licence identifiers, which favours reuse's shape -- but reuse emits
each identifier twice, which is its own quirk. Neither is checked by any gate
today.

## Deliberate divergences, unchanged

These are recorded in `docs/plan/IMPLEMENTATION.md` so they are not mistaken
for drift:

- **Checksums.** comply emits `SHA256` alongside the spec-required `SHA1`;
  SHA-1 alone is no longer fit for integrity claims.
- **Document namespace.** Derived from the file set rather than a random UUID,
  so an unchanged tree yields a byte-identical document and two SBOMs can be
  diffed in review.
- **Skipped files.** Omitted from the SBOM rather than listed as `NOASSERTION`,
  which would assert "unknown" about a file the manifest covers. The SBOM's
  file count now equals the number `lint` reports checked.

## Method

```sh
comply lint  &&  comply spdx      # comply 0.1.0, built from the workspace
reuse  lint  &&  reuse  spdx      # reuse 6.2.0, via mise x pipx:reuse@6.2.0
```

File sets extracted from each SBOM's `FileName:` records and compared with
`comm`; per-file licensing compared as SPDX identifier sets, which normalises
away the representation difference above.

reuse is not installed, invoked or checked by any task, workflow or hook in
this project. comply is the tool of record; this comparison is a one-off
differential check, run on request to validate comply against the
specification rather than to gate anything.

## Run data

Generated from a live run over the comply repository.

- comply `lint`: 110 total, 109 passed, 0 failed, 1 skipped, exit 0.
- reuse `lint`: 116 / 116 files with copyright and licence information, exit 0.

The 7-file gap is comply's `ignore` config; every file both tools see agrees on
licensing.
