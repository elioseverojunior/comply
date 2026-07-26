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
| `download` | (Legacy - only for backward compat) |

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

comply implements all three methods identically to the Python reuse-tool,
ensuring that `comply lint` and `reuse lint` produce the same results for
the same project.

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
- **Compatibility tests**: Run `reuse lint` (Python) and `comply lint` (Rust)
  on same fixtures, compare output.

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

## REUSE configuration compatibility

comply reads and writes:

- **`REUSE.toml`** (preferred, TOML format) -- same structure as Python reuse-tool
- **`.reuse/dep5`** (DEP5 format) -- same structure as Python reuse-tool
- **Inline SPDX headers** -- same regex patterns

comply generates identical output to the Python tool for:

- `lint` output (text format, same messages)
- `spdx` BOM output (JSON format)
- Header annotation format

This ensures projects can switch between `reuse` and `comply` seamlessly.

## Backlog -- needs analysis before implementation

### User-defined comment markers in REUSE.toml

Comment syntax is currently a closed set: `CommentStyle` has nine variants and
`detect_style` maps extensions onto them in `crates/comply/src/header/style.rs`.
A project using a format comply does not know gets `Hash` by default, or a
`.license` companion if the extension is listed as uncommentable -- there is no
way to teach it a new syntax without editing the crate.

The proposal is to let a project declare markers itself, e.g.

```toml
[[comment-styles]]
extensions = ["vue", "svelte"]
open  = "<!--"     # emitted once, on its own line; omit for line styles
line  = ""         # prefix on each header line
close = "-->"      # emitted once; omit for line styles
```

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

Until then the closed set stands, extended in-crate: `SlashStar` (`/* */`, for
`css`/`scss`/`sass`/`less`) and `DotDot` (`..`, for `rst`) were added this cycle,
leaving `json` as the only text format routed to a `.license` companion.
