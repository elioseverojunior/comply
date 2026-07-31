<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# comply Architecture

## Overview

comply is a **REUSE compliance tool** written in pure Rust. It checks and
enforces the [REUSE Specification](https://reuse.software/spec/) for
declaring licenses in software projects.

Unlike the Python [fsfe/reuse-tool](https://github.com/fsfe/reuse-tool),
comply is a native Rust implementation -- no runtime dependency on Python,
faster execution, and compilable to WASM for browser use.

## Guiding principles

- **TDD**: Test first, implement, refactor.
- **Security-first**: `thiserror` in libraries, `anyhow` in binaries, no
  unwrap/expect on external input.
- **No unsafe code**: The `unsafe_code = "deny"` lint is set workspace-wide.
  Zero `unsafe` blocks, functions, or traits are permitted.
- **REUSE-compatible**: Configuration and output are interchangeable with the
  Python reuse-tool.
- **KISS, DRY, YAGNI, TDA, SOLID**.
- **Stable Rust**: Edition 2024, pinned via `rust-toolchain.toml`.

---

## Workspace structure

The root `Cargo.toml` is a virtual workspace. Five crates live under
`crates/`:

```mermaid
mindmap
  root((comply workspace))
    comply
      Core library
      SPDX parsing
      Header detection
      License detection
      REUSE.toml / DEP5 parsing
    comply-cli
      CLI binary
      Subcommands: init, format, lint, annotate, fix
      Intelligent management
    comply-wasm
      Browser WASM target
      wasm-bindgen API
      In-browser linting
    comply-mcp
      MCP server
      AI assistant integration
      Tool definitions
    comply-lsp
      LSP server
      IDE diagnostics
      Code actions
      Real-time compliance
```

---

## Crate dependency graph

```mermaid
flowchart BT
    cli["comply-cli"]
    wasm["comply-wasm"]
    mcp["comply-mcp"]
    lsp["comply-lsp"]
    core["comply (core)"]

    cli --> core
    wasm --> core
    mcp --> core
    lsp --> core
```

`comply` is the sole leaf crate (excluding standard dependencies) -- every
surface crate depends on it and nothing else.

---

## Core library design

```mermaid
flowchart LR
    subgraph CORE["comply (core)"]
        spdx["spdx -- SPDX expression validation"]
        header["header -- header detection & generation"]
        license["license -- license DB & detection"]
        file["file -- discovery & classification"]
        config["config -- REUSE.toml / DEP5 parser"]
        report["report -- lint reports & BOM"]
        hash["hash -- SHA256 integrity"]
        error["error -- thiserror types"]
    end

    file --> config
    file --> header
    file --> license
    header --> spdx
    header --> license
    report --> header
    report --> file
    report --> license
```

All modules depend on `error` for typed errors.

---

## Surface crate designs

### comply-cli

```mermaid
flowchart LR
    main["main.rs -- dispatch"]
    init["init -- project scaffolding"]
    lint["lint -- compliance check"]
    annotate["annotate -- SPDX header editing"]
    format["format -- header normalization"]
    fix["fix -- auto-repair"]
    intelligent["intelligent -- AI-assisted"]

    main --> init
    main --> lint
    main --> annotate
    main --> format
    main --> fix
    main --> intelligent
    intelligent --> mcp["comply-mcp client"]
```

### comply-wasm

```mermaid
flowchart LR
    js["JS caller"]
    lib["lib.rs -- wasm-bindgen exports"]
    lint_wasm["lint_project()"]
    annotate_wasm["annotate_file()"]
    validate_wasm["validate_spdx()"]
    config_wasm["parse_reuse_toml()"]

    js --> lib
    lib --> lint_wasm
    lib --> annotate_wasm
    lib --> validate_wasm
    lib --> config_wasm
    lint_wasm --> core["comply core"]
    annotate_wasm --> core
    validate_wasm --> core
    config_wasm --> core
```

### comply-mcp

```mermaid
flowchart LR
    ai["AI Assistant\n(Claude, Copilot)"]
    mcp_server["comply-mcp MCP Server"]
    tools["Tool definitions"]
    resources["Resource providers"]
    prompts["Prompt templates"]

    ai -->|"MCP protocol (stdio)"| mcp_server
    mcp_server --> tools
    mcp_server --> resources
    mcp_server --> prompts
    tools --> core["comply core"]
    resources --> core
    prompts --> core
```

### comply-lsp

```mermaid
flowchart LR
    editor["VS Code / Neovim / Emacs"]
    lsp_server["comply-lsp LSP Server"]
    backend["Document analysis backend"]
    diagnostics["Diagnostic publisher"]
    actions["Code action provider"]
    completions["Completion provider"]

    editor -->|"LSP (stdio)"| lsp_server
    lsp_server --> backend
    lsp_server --> diagnostics
    lsp_server --> actions
    lsp_server --> completions
    backend --> core["comply core"]
    diagnostics --> core
    actions --> core
    completions --> core
```

---

## External dependencies

| Dependency         | Used by                                  | Purpose                           |
| ------------------ | ---------------------------------------- | --------------------------------- |
| `serde` 1          | `comply`                                 | REUSE.toml / DEP5 deserialization |
| `serde_json` 1     | `comply`                                 | JSON report output                |
| `toml` 0.8         | `comply`                                 | REUSE.toml parsing                |
| `thiserror` 2      | `comply`                                 | Typed error enums                 |
| `sha2` 0.10        | `comply`                                 | SHA256 hashing                    |
| `walkdir` 2        | `comply`                                 | Recursive file discovery          |
| `ignore` 0.4       | `comply`                                 | Gitignore-aware file filtering    |
| `regex` 1          | `comply`                                 | SPDX header regex scanning        |
| `clap` 4           | `comply-cli`                             | CLI argument parsing              |
| `wasm-bindgen` 0.2 | `comply-wasm`                            | JS/WASM bridge                    |
| `mcp-sdk` 0.x      | `comply-mcp`                             | MCP protocol implementation       |
| `tower-lsp` 0.x    | `comply-lsp`                             | LSP framework                     |
| `tokio` 1          | `comply-cli`, `comply-mcp`, `comply-lsp` | Async runtime                     |

---

## External API

### CLI (comply-cli)

```text
Usage: comply <COMMAND>

Commands:
  init       Initialize a project for REUSE compliance
  format     Format SPDX headers consistently
  lint       Check project for REUSE compliance
  annotate   Add or update SPDX headers
  fix        Auto-fix compliance issues
  help       Print this message or the help for the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### WASM (comply-wasm)

```rust
// wasm-bindgen exports
pub fn lint_project(files: JsValue) -> JsValue;        // FileEntry[] -> LintReport
pub fn annotate_file(source: &str, license: &str,      // source content -> annotated content
    copyright: &str) -> String;
pub fn validate_spdx(expression: &str) -> JsValue;     // expression -> ValidationResult
pub fn parse_reuse_toml(content: &str) -> JsValue;     // TOML content -> ReuseConfig
```

### MCP (comply-mcp)

Tools exposed via MCP:

- `lint_project` -- run compliance check on a project directory
- `annotate_file` -- annotate a file with SPDX header
- `init_project` -- create REUSE project structure
- `check_spdx` -- validate SPDX expression
- `list_licenses` -- list all known SPDX licenses

### LSP (comply-lsp)

Capabilities advertised to editor:

- `TextDocumentSyncKind::Incremental`
- `Diagnostics` -- compliance issues as diagnostics
- `CodeAction` -- fix compliance issues
- `CompletionItem` -- SPDX license ID completion
- `HoverProvider` -- license details on hover

---

## Key design decisions

| Decision                  | Rationale                                 |
| ------------------------- | ----------------------------------------- |
| REUSE spec compatible     | Drop-in replacement for Python reuse-tool |
| Native Rust               | No Python dependency, faster, WASM target |
| Multi-surface             | One core powers CLI + WASM + LSP + MCP    |
| Unsafe code forbidden     | `unsafe_code = "deny"` workspace-wide     |
| Stable Rust               | Edition 2024, pinned toolchain            |
| Bundled SPDX License List | Works offline, no network dependency      |
| TDD strictly              | Every feature starts with a failing test  |

---

## Configuration

All shared metadata lives in the root `Cargo.toml`:

- `[workspace.package]` -- edition, license, version
- `[workspace.lints.rust]` -- lint policy (warnings-as-errors)
- `[workspace.dependencies]` -- shared external and internal dependencies

Build profiles and target-specific linker flags are in `.cargo/config.toml`.
Rust toolchain is pinned via `rust-toolchain.toml` (stable).

---

## Testing strategy

- **Unit tests**: Inline `#[cfg(test)] mod tests` in every module.
- **Integration tests**: Full `comply lint` on fixture projects in `tests/`.
- **Fixtures**: `tests/fixtures/` with known compliance states (compliant,
  missing headers, invalid SPDX, mixed license).
- **Round-trip tests**: Serialize -> deserialize -> compare for REUSE.toml.
- **Compat tests**: Compare output with Python `reuse` tool on same fixtures.
