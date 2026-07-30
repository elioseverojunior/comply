<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# comply -- Architecture Plan

## Overview

comply is a **REUSE compliance tool** written in pure Rust. It is a new
implementation (not a port of the Python [fsfe/reuse-tool](https://github.com/fsfe/reuse-tool)),
but it maintains full configuration compatibility with the REUSE specification
(REUSE.toml / DEP5 format).

The goal is to provide a fast, secure, and multi-platform REUSE compliance
toolkit that works everywhere: CLI, browser (WASM), IDE (LSP), and AI-assisted
coding (MCP).

## Guiding principles

- **TDD**: Test first, implement, refactor. Never production code before tests.
- **Security-first**: No unwrap/expect on external input; `thiserror` in libraries,
  `anyhow` in binaries. Unsafe code forbidden workspace-wide.
- **REUSE Specification compliant**: Output and configuration are interchangeable
  with the Python reuse-tool. Same REUSE.toml format, same lint rules.
- **KISS, DRY, YAGNI, TDA, SOLID**.

## Crate architecture

The project is a virtual workspace with 5 crates under `crates/`:

```text
Cargo.toml                     # Virtual workspace root ([workspace] only)
crates/
  comply/                        # Core library (REUSE compliance engine)
    src/
      lib.rs
      spdx.rs                    # SPDX expression parsing and validation
      header.rs                  # SPDX header detection, parsing, generation
      license.rs                 # License detection, known license DB
      file.rs                    # File discovery, classification, .license files
      config.rs                  # REUSE.toml / DEP5 parsing
      report.rs                  # Lint reports, BOM generation
      hash.rs                    # Integrity hashing (SHA256)
      error.rs                   # Error types
  comply-cli/                    # CLI binary
    src/
      main.rs                    # Entrypoint + subcommand dispatch
      init.rs                    # `comply init` - initialize project
      format.rs                  # `comply format` - format SPDX headers
      lint.rs                    # `comply lint` - check compliance
      annotate.rs                # `comply annotate` - add/update headers
      fix.rs                     # `comply fix` - auto-fix issues
      intelligent.rs             # AI-assisted compliance management
  comply-wasm/                   # WASM target (browser)
    src/
      lib.rs                     # wasm-bindgen API surface
      tree.rs                    # File tree analysis
      report.rs                  # Report generation
  comply-mcp/                    # MCP (Model Context Protocol) server
    src/
      main.rs                    # MCP server entrypoint
      tools.rs                   # Tool definitions
      server.rs                  # MCP protocol handling
  comply-lsp/                    # Language Server Protocol
    src/
      main.rs                    # LSP server entrypoint
      backend.rs                 # Document analysis backend
      handlers.rs                # LSP notification/request handlers
      diagnostics.rs             # Diagnostic generation
      code_actions.rs            # Code action providers
```

## Crate dependency graph

```text
comply-lsp  comply-mcp  comply-wasm  comply-cli
     \          |          /         /
      \         |         /         /
       comply (core library)
```

`comply` is the sole library dependency for all other crates. Each surface
crate (CLI, WASM, MCP, LSP) depends only on `comply` + its transport-layer
dependencies.

## Layer architecture

```text
+------------------------------------------------------+
|  Surface layers                                       |
|  +----------+  +----------+  +----------+  +--------+ |
|  | comply-  |  | comply-  |  | comply-  |  |comply- | |
|  | cli      |  | wasm     |  | mcp      |  | lsp    | |
|  +----------+  +----------+  +----------+  +--------+ |
+------------------------------------------------------+
|  Core library (comply)                                 |
|  +--------+ +--------+ +--------+ +--------+ +------+ |
|  | spdx   | | header | | license| | file   | |config| |
|  +--------+ +--------+ +--------+ +--------+ +------+ |
|  +--------+ +--------+                               |
|  | report | | hash   |                               |
|  +--------+ +--------+                               |
+------------------------------------------------------+
```

## Module design

### `comply` (core library)

| Module | Responsibility |
|--------|---------------|
| `spdx` | Parse and validate SPDX expressions (`MIT`, `Apache-2.0`, `MIT OR Apache-2.0`, `LicenseRef-*`). Validate license IDs against the SPDX License List. |
| `header` | Detect SPDX headers in files (first N lines regex scan). Parse `SPDX-FileCopyrightText` and `SPDX-License-Identifier` lines. Generate correctly formatted headers. |
| `license` | License detection from text (approximate matching). Bundled SPDX License List data for known licenses. Custom license support via `.license` files. |
| `file` | File discovery (recursive directory walk). File classification (source, binary, build artifact, ignored). `.license` companion file handling. Gitignore-aware. |
| `config` | Parse `REUSE.toml` (TOML-based manifest) and `.reuse/dep5` (DEP5 format). Both formats per REUSE spec. |
| `report` | Generate lint reports (compliant vs noncompliant files). Bill of Materials (BOM) generation. Machine-readable output (JSON, SPDX). |
| `hash` | SHA256 file hashing for integrity checks. |
| `error` | Typed error enum using `thiserror`. |

### `comply-cli` (CLI binary)

Subcommands mirror the Python reuse-tool for compatibility:

| Subcommand | Description |
|------------|-------------|
| `init` | Create `.reuse/` directory with initial `REUSE.toml` and `LICENSES/` structure. |
| `format` | Normalize SPDX headers across all files (consistent formatting, ordering, line endings). |
| `lint` | Check the entire project for REUSE compliance. Exit 0 if compliant, non-zero with report. |
| `annotate` | Add or update SPDX headers on specified files. Supports `--copyright`, `--license`, `--year`, etc. |
| `fix` | Auto-fix common compliance issues (missing headers, wrong format, missing licenses). |
| `lint-file` | Check only the named files. The pre-commit shape: cost is proportional to the change, not the repository. Defaults to `--lines`, matching the reference tool. |
| `download` | Fetch licence texts into `LICENSES/`. `--all` fetches exactly what `lint` reports missing, which is what makes it the remedy for that finding rather than a separate chore. |
| `spdx` | Emit an SPDX 2.1 tag-value bill of materials. |
| `convert-dep5` | Convert `.reuse/dep5` into an equivalent `REUSE.toml` and remove the dep5, so the project stops declaring licensing two ways. |
| `supported-licenses` | List the bundled SPDX identifiers (727 licences, 34 exceptions, 32 deprecated). |

Output flags on `lint` and `lint-file` mirror `reuse` 6.2.0: `--quiet`, `--json`,
`--plain`, `--lines`. Precedence is resolved in one place (`output::Format`), and
the default differs per command -- `plain` for `lint`, `lines` for `lint-file`.

Three deliberate divergences from the reference tool, all recorded here so they
are not mistaken for drift:

- `spdx` checksums with `SHA256`, not `SHA1`. Both are valid SPDX; SHA-1 is no
  longer fit for integrity claims.
- `spdx` derives `DocumentNamespace` from the file set rather than a random
  UUID, so an unchanged tree yields a byte-identical document and two SBOMs can
  be diffed in review.
- `convert-dep5` refuses to overwrite an existing `REUSE.toml` instead of
  replacing it, because that file may hold annotations the conversion knows
  nothing about.

The git-tracked backfill has to judge ancestors that a walk would never reach.
`out_of_scope` prunes a directory only when handed the directory itself, which
is all a descent needs -- it stops there and never sees the contents. The
backfill starts from `git ls-files`, so its paths skipped that descent
entirely: `LICENSES/MIT.txt` was pruned from the walk, tracked by git, absent
from `seen`, and re-added with `is_dir: false`, which bypasses the one rule
that would have caught it. `is_excluded` already had the component-wise check
and a comment explaining precisely this hazard; the backfill did not use it.
`scope::is_within_pruned_dir` now holds that rule once and both call it.

The visible damage was in the SBOM, which asserted that `LICENSES/MIT.txt` is
licensed `MIT OR Apache-2.0` through the `**` aggregate -- claiming the MIT
licence text is covered by the project's dual licence.

`spdx` describes only the files `lint` checks: a `Status::Skipped` file is left
out entirely rather than listed as `NOASSERTION`. It has no licensing to state,
and an entry would assert "unknown" about a file the manifest may well cover --
the `**` aggregate covers `LICENSE`, yet the entry said otherwise. This surfaced
only when `LICENSE` stopped being a symlink into `LICENSES/` and became a
regular dual-licence notice, since the symlinks were already excluded. The
invariant to hold on to is that the SBOM's file count equals the number `lint`
reports checked.

`download` shells out to `curl` rather than linking an HTTP/TLS stack. That is a
supply-chain choice: the project carries zero `cargo-vet` exemptions and audits
every dependency through imported audit sets, and a TLS client would add a large
subtree needing fresh audits for a feature used once per licence. `--source`
takes a URL or path template, so the tests never touch the network.

The `intelligent.rs` module provides AI-assisted features:

- Smart file classification using ML heuristics
- Automatic license detection from project context
- Batch annotation with conflict detection
- Integration with comply-mcp for agent assistance

### `comply-wasm` (WASM binary)

Exposes core compliance functions to browser JavaScript:

- `lint_project(files: &[FileEntry]) -> LintReport` -- check compliance of
  uploaded project files
- `annotate_file(source: &str, license: &str, copyright: &str) -> String` --
  add SPDX header to file content
- `validate_spdx(expression: &str) -> ValidationResult` -- validate an SPDX
  expression
- `parse_reuse_toml(content: &str) -> Result<Config, Error>` -- parse
  REUSE.toml in-browser

Target: `wasm32-unknown-unknown`. No filesystem access (all data supplied
from JS). Uses `wasm-bindgen` for the JS bridge.

### `comply-mcp` (MCP server)

Implements the [Model Context Protocol](https://modelcontextprotocol.io) to
expose compliance tools to AI coding assistants (Claude, Copilot, etc.).

Resources:

- `reuse://project/{path}/lint` -- lint results for a project
- `reuse://project/{path}/files` -- file classification tree
- `reuse://project/{path}/bom` -- bill of materials

Tools:

- `lint_project(path: str)` -- run REUSE lint on a project
- `annotate_file(path: str, license: str, copyright: str, year: str)` --
  annotate a single file
- `init_project(path: str)` -- initialize REUSE structure
- `check_spdx(expression: str)` -- validate an SPDX expression
- `list_licenses()` -- list known SPDX licenses

Prompts:

- `compliance_status` -- summary of a project's REUSE compliance
- `fix_plan` -- step-by-step fix plan for compliance issues

### `comply-lsp` (LSP server)

Implements the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
to provide real-time compliance feedback in editors (VS Code, Neovim, Emacs,
etc.).

Capabilities:

- **Diagnostics**: flag files missing SPDX headers, invalid SPDX expressions,
  missing license files, incorrect DEP5 coverage
- **Code actions**:
  - `Add SPDX header` (inserts a compliant header)
  - `Fix SPDX expression` (corrects syntax)
  - `Annotate with license` (adds license identifier)
  - `Create .license file` (for binary/uncommentable files)
- **Completion**: suggest SPDX license IDs when typing the `SPDX-License-Identifier` tag
- **Hover**: show license details when hovering over an SPDX expression
- **Code lens**: show compliance status at the top of each file

Transport: stdio (default), TCP (optional).

Protocol version: 3.17 (LSP 3.17).

## REUSE Specification compatibility

The REUSE specification (version 3.x) defines three methods for declaring
licenses:

1. **SPDX headers** -- inline `SPDX-License-Identifier` and
   `SPDX-FileCopyrightText` in source files
2. **`.license` files** -- companion files for binary/uncommentable files
3. **DEP5 / REUSE.toml** -- bulk declarations in `.reuse/dep5` or `REUSE.toml`

comply implements all three methods identically to the Python reuse-tool, so
the two agree on the same project. That parity is a design constraint, not a
runtime dependency: the reference tool is not installed, invoked or checked by
any task, workflow or hook. Divergences are settled by writing a test here, not
by shelling out to it.

Key compatibility points:

- `REUSE.toml` structure is identical (TOML format, same keys)
- `.reuse/dep5` format is identical (DEP5 format, same keys)
- SPDX header regex patterns match the Python tool's behavior
- `.license` file naming and content conventions match exactly
- Exit codes match: 0 = compliant, 1 = non-compliant

## Implementation phases

### Phase 1 -- Core library (DONE)

- `comply::spdx` -- SPDX expression parser and validator
- `comply::header` -- header detector and generator
- `comply::file` -- file discovery and classification
- `comply::error` -- error types
- Known license DB (SPDX License List 3.28 bundled)

### Phase 2 -- CLI (DONE)

- `comply-cli lint` -- full lint implementation
- `comply-cli annotate` -- annotation subcommand
- `comply-cli init` -- project initialization
- `REUSE.toml` / DEP5 parsing support
- `.license` file support

### Phase 3 -- Format and Fix (DONE)

- `comply-cli format` -- header formatting
- `comply-cli fix` -- auto-fix implementation
- `comply::report` -- report generation (text, JSON, SPDX BOM)
- `comply::hash` -- integrity hashing
- `comply::license` -- license detection from text

### Phase 3b -- Command-surface parity with reuse 6.2.0 (DONE)

- `comply-cli lint-file` -- lint named files only
- `comply-cli download` -- fetch licence texts, `--all` driven by the lint audit
- `comply-cli spdx` -- SPDX 2.1 tag-value SBOM
- `comply-cli convert-dep5` -- DEP5 to REUSE.toml migration
- `comply-cli supported-licenses` -- expose the bundled SPDX lists
- `comply-cli output` -- `--quiet/--json/--plain/--lines` for both lint commands
- `comply::config::Dep5Document` -- DEP5 header fields and folded continuation
  lines, which the previous parser dropped
- `comply::reuse_toml::render_dep5_as_toml` -- writer verified byte-identical to
  `reuse convert-dep5`
- `comply::spdx::{known_licenses, known_exceptions, deprecated_licenses}` --
  enumeration of the bundled lists
- `comply::file::classify_path` -- classify one path as a walk would

### Phase 4 -- WASM (pending)

- `comply-wasm` -- WASM compilation target
- JS API design
- Browser demo / playground

### Phase 5 -- LSP (pending)

- `comply-lsp` -- LSP server
- Diagnostics for missing/invalid headers
- Code actions for compliance fixes
- VS Code extension (separate repo or bundled)

### Phase 6 -- MCP (pending)

- `comply-mcp` -- MCP server
- Tool definitions for AI assistants
- Resource providers for project state

## Feature flags

```toml
# comply (core)
[features]
default = ["license-db-bundled"]
license-db-bundled = []   # Bundle SPDX License List (~500KB)
license-db-network = []   # Fetch license list at runtime

# comply-cli
[features]
default = []
intelligent = []          # AI-assisted features (requires comply-mcp)
```

## Testing strategy

- **Unit tests**: Inline `#[cfg(test)] mod tests` in every module.
- **Integration tests**: End-to-end `comply lint` on fixture projects.
- **Fixtures**: Small test projects with known compliance states (compliant,
  missing headers, invalid SPDX, etc.).
- **Round-trip tests**: Parse REUSE.toml -> generate -> re-parse -> compare.
- **Compatibility tests**: parity with the reference tool is pinned by tests in
  this repository -- expectations captured from it and asserted against comply,
  such as the byte-for-byte header cases in `header/generate.rs`. The reference
  tool is not run as part of any gate.

### Coverage: one gate, one exclusion list

Two engines run, but only one decides.

- **tarpaulin is the gate of record.** `ci.yml`'s Coverage job passes or fails
  the build; `tarpaulin.toml` holds the threshold rationale and `exclude-files`.
- **llvm-cov runs against a ratchet.** `mise.yml` holds it to a threshold it can
  actually meet (currently 97; today's floor is regions 98.93, functions 98.00,
  lines 99.74), so that job goes red on a real regression rather than on every
  run. It cannot be held to 100: the shortfall is `?` error edges and assert
  branches that no test can execute. Raise the ratchet as the numbers improve.
- **Exclusions are declared once.** The `llvm-cov` task derives its
  `--ignore-filename-regex` from `tarpaulin.toml`'s `exclude-files` at run time.
  It used to keep a hand-copied regex, so the two could drift and the verdict
  would depend on which engine ran.

Two lines needed source changes rather than tests to become measurable, both
because tarpaulin attributes a line it cannot observe being executed:

- a `const fn` whose test passed literal arguments was folded at compile time --
  the test now hides them behind `std::hint::black_box`
- a branch keyword on its own line inside a `filter_map` closure -- the code is
  now a single `then_some` expression with no `if`/`else` statement

## Key design decisions

| Decision | Rationale |
| -------- | --------- |
| REUSE spec compatible | Drop-in replacement for Python reuse-tool; same config, same output |
| Native Rust, not a port | Full safety guarantees, no Python dependency, cross-compilation to WASM |
| Multi-surface architecture | One core library powers CLI, WASM, LSP, and MCP |
| Unsafe code forbidden | `unsafe_code = "deny"` workspace-wide |
| Stable Rust | `rust-toolchain.toml` pins stable, edition 2024 |
| Bundled SPDX License List | Offline-first: works without network |
| TDD strictly | Every feature starts with a failing test |

### An unknown extension is refused, not guessed

`detect_style` used to end in `CommentStyle::Hash` for anything its tables did
not claim. That made "no built-in knows this format" indistinguishable from
"this format uses `#`", and the wrong one was written to disk: `.mts` got shell
comments in a TypeScript file, as did `.tsx`, `.jsx`, `.mjs`, `.cjs`, `.vue`,
`.lua`, and `.bat`. `--force` then made it stick.

Unclaimed names now resolve to `CommentStyle::Uncommentable`. Nothing new was
needed downstream: `commands/annotate.rs` and `commands/format.rs` already
branch on that variant to warn, honour `--skip-unrecognised`, or write a
`.license` companion, which is the contract reuse 6.2.0 offers for the same
case. The variant's meaning widens from "binary" to "no known comment syntax",
and the messages say so.

Three consequences worth stating:

- **Lint is untouched.** `commands/lint.rs` and `report.rs` never call
  `detect_style`; header parsing is permissive by design so an existing header
  is read whatever comply would have written. The compliance gate is unchanged.
- **A name table was required.** With the `#` catch-all gone, `Dockerfile`,
  `Makefile`, `CODEOWNERS`, and dotfiles like `.gitignore` had nothing to fall
  back on, so `filename_style` now maps exact names. It is consulted before the
  extension tables, because `CMakeLists.txt` is a build script rather than the
  unrecognised `.txt` its suffix suggests.
- **The extension was being read from the wrong string.** `detect_style` split
  the whole path on `.`, so `.github/CODEOWNERS` resolved through
  `"github/codeowners"`. It now takes the basename, as `detect_style_with`
  always did.

The escape hatch is `[[tool.comply.comment-styles]]`, which `detect_style_with`
already consults ahead of the built-ins. `Uncommentable` is therefore a prompt
to declare the syntax, not a dead end.

### `[tool.comply] ignore` carries two intents

Most entries mean *not part of the project* (`**/target/**`, `**/*.lock`). A few
mean *part of the project, but nothing may write to it*: `DCO.txt`, whose own
text forbids modification, and the `crates/*/LICENSE-*` symlinks, which
`annotate` would write straight through into the canonical licence texts.

The walk cannot tell the two apart, so it dropped the second kind entirely --
including its declaration. `DCO.txt` is the only carrier of `CC-BY-3.0`, so the
licence it declares at `REUSE.toml:14` never reached `used_licenses` and
`license::audit` called the text unused. The reference tool, which has no such
config, correctly reported it used; comply failed CI over a licence plainly in
use.

`collect_declared_by_ignored` closes the gap: an annotation whose file exists
but is excluded contributes its identifiers to the used set, without the file
being judged or counted. The existence check is what keeps this honest -- a
stale annotation naming a deleted path would otherwise keep a dead licence text
alive forever, which is exactly what the unused check exists to catch. It also
confines the rule to literal paths, since a glob such as `crates/*/LICENSE-*`
names no file on disk.

The fuller fix is to split the config so write-protection stops overloading
scope. That is a user-facing schema change and is deliberately not done here;
this narrows the damage without moving the schema.

## REUSE configuration compatibility

comply reads and writes:

- **`REUSE.toml`** (preferred, TOML format) -- same structure as Python reuse-tool
- **`.reuse/dep5`** (DEP5 format) -- same structure as Python reuse-tool
- **Inline SPDX headers** -- same regex patterns

comply generates identical output to the Python tool for:

- `lint` output (text format, same messages)
- `lint --lines` and `lint-file` (verified byte-identical on shared fixtures)
- `convert-dep5` output (verified byte-identical)
- Header annotation format (all nine shared comment styles)

`spdx` is tag-value rather than JSON, and diverges deliberately on checksum
algorithm and namespace derivation -- see the CLI section above.

This ensures projects can switch between `reuse` and `comply` seamlessly.

## Backlog -- needs analysis before implementation

### User-defined comment markers in REUSE.toml

Comment syntax is currently a closed set: `CommentStyle` has nine variants and
`detect_style` maps extensions onto them in `crates/comply/src/header/style.rs`.
A project using a format comply does not know is refused rather than annotated
(see "An unknown extension is refused, not guessed") -- there is no way to teach
it a new syntax without editing the crate.

The proposal is to let a project declare markers itself. Settled shape, an array
of tables so one rule can cover every language that shares a syntax:

```toml
[[tool.comply.comment-styles]]
extensions = ["vue", "svelte", "astro"]
open  = "<!--"     # emitted once, on its own line; omit for line styles
line  = ""         # prefix on each header line; omit when there is none
close = "-->"      # emitted once; omit for line styles

[[tool.comply.comment-styles]]
filenames = ["Jenkinsfile", "Vagrantfile"]
open  = "#"        # a line style sets `open` only
```

`extensions` and `filenames` are both optional and may appear together; a rule
with neither is a configuration error rather than a silent no-op. Grouping is
the point -- eleven extensions share `#` in the built-in table today, and a
table per key would mean eleven near-identical blocks.

Two spelling notes. An inline `{ ... }` table cannot span lines in TOML, so the
obvious-looking nested-map form does not parse. And the doubled brackets matter:
`[[...]]` is an array of tables, `[...]` a single one, so a second `[...]` block
of the same name is a duplicate-key error.

Grouping costs one thing the keyed form gave free: with a unique key per rule,
two rules could not claim the same extension. Here they can, so the loader needs
a stated tiebreak, and there is no precedent in the file to borrow -- `ignore`
resolves by `.any()`, a union in which order carries no meaning, so nothing
about it implies an answer here. The tiebreak has to be chosen and written down.
Last rule wins is the conventional pick; rejecting the manifest outright is the
safer one, since a duplicated extension is far more likely a mistake than an
intent.

**Settled: reject.** `reject_contested_keys` in `config/reuse_toml.rs` refuses a
manifest where two rules claim the same extension or filename, naming the key.
Rejecting means the order of a `Vec` never decides anything, so
`detect_style_with`'s first-match lookup needs no documented precedence -- at
most one rule can ever match. Extensions compare lowercased, matching how
lookup normalises them, so `VUE` and `vue` collide rather than shadow. Filenames
compare exactly, since a filesystem may distinguish them. The two are separate
namespaces: a `makefile` extension does not contest a `makefile` filename.

### How a configured style reaches the code that writes it

`CommentStyle::Custom(u16)` is only an index; the delimiters live in the
manifest. Carrying them in the enum would drop `Copy` and ripple through the
~114 places a style is passed, so the index stays and something else has to
supply the table.

`ResolvedStyle<'a>` is that something: a `Copy` pair of the style and the rules
it may index into, built by `ResolvedStyle::detect(path, rules)` at the four
entry points that know both (`fix.rs` twice, `commands/format.rs`,
`commands/annotate.rs`). It **replaces** the `CommentStyle` parameter rather
than joining it, which matters because `annotate_file_with` already sits at
clippy's seven-argument ceiling -- an eighth parameter would not build under
`-D warnings`. It also retires the runtime refusal `generate_with` used to
carry: an unresolved style no longer type-checks, so the failure moved from a
returned `Error` to a compile error.

The reach turned out to be wider than rendering. A configured style has to be
recognised on the way back in, and four separate places had each hardcoded the
built-in list:

| Site | What it decides | Was |
| --- | --- | --- |
| `generate_with` | how the header is written | `comment_delimiters` |
| `comment_mask` | which lines are inside a comment | `["<!--", "/*", "(*"]` |
| `carries_licensing` | whether a line's tag is readable | `strip_comment_prefix` |
| `is_block_closer` | where the header span ends | `"*/" \| "*)" \| "-->"` |

All four now derive from the same rules, via `block_delimiters(rules)` and
`configured_prefixes(rules)`. Missing any one of them is not a cosmetic
failure: with the closer alone left hardcoded, `header_span` ended above a
configured closer, so each `comply fix` left another orphaned `--%>` behind and
the file grew without bound. Two round-trip tests pin it -- annotate-twice and
format-after-annotate must both be no-ops, for a line style and a block style.

Open questions, all of which need answering before this is worth building:

1. **Is it in the spec?** REUSE 3.3 defines no such key. Adding one makes the
   file non-portable: `reuse` would ignore it and annotate the same files
   differently, so the two tools would fight over the same headers. A
   `[tool.comply]` subtable (already used for `ignore`) keeps it clearly a
   comply extension rather than a claimed spec feature.
2. **Generation and detection are two tables.** `generate_with` renders headers;
   `strip_comment_prefix` and `detect_comment_style_from_spdx_line` read them
   back. Both would have to be driven from the same config, or comply will write
   headers it cannot read -- the exact defect that made `(*` headers undetectable
   until it was fixed in this cycle.
3. **The blank separator line.** `reuse` emits a separator between the copyright
   block and the licence line for *every* style (`//`, `#`, `.. `, ` * `);
   comply emits one only when contributors are present. This is a pre-existing,
   global divergence: `comply format --check` reports "would reformat" on any
   header `reuse` wrote, for all styles, not just the new ones. Configurable
   markers would have to decide whether the separator is part of the style, and
   whether comply's canonical form should change to match -- which would churn
   every annotated file in every comply-using project.
4. **Precedence.** Does a user-defined style override a built-in mapping for the
   same extension, or only fill gaps?

Decisions taken (2026-07-27), narrowing the above:

- **Q1, placement.** `[tool.comply.comment-styles]`, not a top-level table.
  Top-level keys in REUSE.toml belong to the spec, and comply *silently ignores*
  unknown ones -- a top-level `[CommentStyles]` would parse and then do nothing,
  which is the worst outcome of the three.
- **Q2, one source for both directions.** Confirmed as the hard requirement.
  There is now a third consumer: `header_span` in `header/annotate.rs` decides
  whether a line sits inside a comment, and it currently hardcodes
  `["<!--", "/*", "(*"]` -- a second copy of what `comment_delimiters` already
  knows. That list is the first thing this feature should delete.
- **Q2a, where a user-defined style lives.** The blocker found while attempting
  the refactor, and the reason it was reverted rather than finished. Lookup is
  two hops, `extension -> CommentStyle -> (open, close)`, and **both ends are
  closed**: `CommentStyle` is a fixed ten-variant enum, and `comment_delimiters`
  returns `(&'static str, &'static str)`. A configured style has no variant to
  map to, and its markers are read from a file at runtime so they cannot be
  `&'static`. Merging is therefore not "repoint an extension at a different
  variant" -- there is no variant to point at.

  Decided: add a **custom variant** carrying owned delimiters, rather than
  dissolving the enum into config-threaded data. Smaller blast radius, and the
  enum stays a useful name for the built-ins. The cost is that every exhaustive
  `match` on `CommentStyle` must handle it; `comment_delimiters` cannot stay
  `const fn` returning `&'static str`, so callers move to an owned or borrowed
  pair.

  A first attempt at the refactor added `ALL_STYLES` plus a deliberately
  exhaustive `assert_style_is_listed` guard, so that adding a variant breaks the
  build and points the author at the list. That guard is worth reinstating when
  the work resumes -- an array of variants is otherwise unchecked.
- **Q4, precedence.** A user rule *replaces* the built-in for that exact key
  rather than layering under it, so overriding one extension cannot silently
  inherit half of the old definition. Built-ins remain the base set and are
  still what `init` and `convert-dep5` write into a generated REUSE.toml.

Q3 (the blank separator line) is unchanged and remains the blocker: it is a
global divergence from `reuse`, not specific to configurable markers.

Implementation note, learned the hard way this cycle: `header_span` must decide
comment-ness from the file's **own** style, via `detect_style_from_header`, not
from the style being written. `replace_header_with` can receive an override
(`--style`, or `format --check` forcing one), and using that to locate the
existing block means `--style slash` on a Markdown file stops recognising its
`<!--` header. Pin that case with a test before touching the scan.

Until then the closed set stands, extended in-crate: `SlashStar` (`/* */`, for
`css`/`scss`/`sass`/`less`) and `DotDot` (`..`, for `rst`) were added this cycle,
leaving `json` as the only text format routed to a `.license` companion.

### Force-include list -- `[tool.comply] add`

`ignore` can only subtract. A project that excludes a directory wholesale has no
way to bring one file back, short of rewriting the pattern:

```toml
[tool.comply]
ignore = [".claude/**/*.md"]
add    = [".claude/skills/verify/SKILL.md"]   # this one back in
```

The design question is not the syntax, it is **where the override stops**.

`add` overrides the *preference* layers only -- `ignore`, gitignore, and
`[tool.comply.walk]`. It must never override the *safety* layer in `scope.rs`:
`.git/`, `LICENSES/`, `.reuse/`, symlinks, and `.license` companions.

That split is not fastidiousness. Both of this cycle's file-corruption incidents
were writes the safety layer is there to stop, and neither was prevented by
`ignore`:

- `annotate --recursive LICENSES` walked with a pruned directory *as the root*,
  and the prune matches directory names during descent, so nothing above the
  root was examined. It wrote an SPDX header into the canonical Apache-2.0 text.
- Before that, an unfiltered explicit path wrote a header into `.git/config`.

An `add` that overrode everything would re-open both, and `add = ["**"]` is the
first thing a frustrated user reaches for. A caller who genuinely needs to write
inside `LICENSES/` can still name the path explicitly to `annotate`, which is a
deliberate act rather than a glob that quietly matches more than intended.

Open question: whether `add` re-includes a path for *reading* (lint judges it)
or also for *writing* (annotate/fix may rewrite it). Reading is clearly safe;
writing is the half that needs the safety floor above.
