# comply

REUSE compliance tool in pure Rust -- check and enforce the
[REUSE Specification](https://reuse.software/spec/) for software licensing.

[![MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
![Rust Stable](https://img.shields.io/badge/rust-stable-purple)
![Unsafe Forbidden](https://img.shields.io/badge/unsafe-forbidden-red)
![Status: Prototype](https://img.shields.io/badge/status-prototype-yellow)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/elioseverojunior/comply/badge)](https://securityscorecards.dev/viewer/?uri=github.com/elioseverojunior/comply)

## Overview

comply is a native Rust implementation of the
[REUSE specification](https://reuse.software/spec/) -- a standard for
declaring licenses in software projects. It is compatible with the Python
[fsfe/reuse-tool](https://github.com/fsfe/reuse-tool) configuration format
(REUSE.toml, DEP5) but runs faster, has no Python dependency, and compiles
to WASM for browser use.

### Why comply?

- **Zero unsafe code** -- `unsafe_code = "deny"` workspace-wide. Memory
  safety by construction.
- **Native Rust** -- no Python runtime dependency. Single static binary.
- **WASM target** -- run compliance checks in the browser.
- **LSP support** -- real-time compliance feedback in VS Code, Neovim, Emacs.
- **MCP support** -- AI-assisted compliance via the Model Context Protocol.

### How it differs from the Python reuse-tool

| Aspect | Python reuse-tool | comply |
|--------|-------------------|--------|
| Language | Python 3 | Rust |
| Runtime | Requires Python | Single binary |
| WASM | No | Yes (wasm32-unknown-unknown) |
| LSP | No | Yes (LSP 3.17) |
| MCP | No | Yes |
| Speed | Slow on large repos | Fast (compiled) |
| Config compatible | -- | Same REUSE.toml/DEP5 |

## Project Status

| Phase | Focus | Timeline | Status |
|-------|-------|----------|--------|
| 0 | Workspace, CI/toolchain, project setup | 2026 Q3 | Done |
| 1 | Core library (SPDX, headers, file discovery, license DB) | 2026 Q3 | Done |
| 2 | CLI (init, format, lint, annotate, fix) | 2026 Q3 | Done |
| 3 | WASM, LSP, MCP surfaces | 2026 Q4+ | Pending |

## Architecture

```text
comply (core library -- SPDX parsing, header detection, file classification)
    |
+---+-------+--------+--------+
|   |       |        |        |
cli wasm   mcp      lsp
(init,    (browser) (AI     (IDE)
 format,           assistant)
 lint,
 annotate,
 fix)
```

Five crates under `crates/`:

| Crate | Role |
|-------|------|
| `comply` | Core library: SPDX parsing, header detection, license DB, REUSE.toml/DEP5 parsing |
| `comply-cli` | CLI binary: init, format, lint, annotate, fix |
| `comply-wasm` | WASM binary for browser-based compliance checking |
| `comply-mcp` | MCP server for AI-assisted compliance |
| `comply-lsp` | LSP server for IDE compliance feedback |

## Getting Started

### Prerequisites

- [mise](https://mise.jdx.dev) -- provisions the Rust toolchain and all dev tools
- Rust stable (installed automatically by mise via `rust-toolchain.toml`)
- Node 24+ (for Markdown lint toolchain)

### Setup

```sh
git clone https://github.com/elioseverojunior/comply.git
cd comply
mise trust
mise run setup
```

### Build

```sh
mise run build          # or: cargo build --workspace
```

### Test

```sh
mise run test           # cargo nextest run (primary test runner)
mise run test-doc       # cargo test --doc (doctests)
```

### Use

```sh
# Check a project for REUSE compliance
cargo run -p comply-cli -- lint /path/to/project

# Initialize REUSE structure in a project
cargo run -p comply-cli -- init /path/to/project

# Annotate a file with SPDX header
cargo run -p comply-cli -- annotate --license MIT --copyright "2026 Acme Inc" src/main.rs

# Format all SPDX headers consistently
cargo run -p comply-cli -- format /path/to/project
```

### Full Quality Gate

```sh
mise run ci-quick       # fmt-check + clippy + tests + doctests (~1 min)
```

Or run every check manually in the prescribed order:

```sh
cargo sort --workspace --check \
  && taplo format --check Cargo.toml crates/*/Cargo.toml \
  && mise run markdownlint \
  && cargo fmt --check \
  && cargo clippy -- -D warnings \
  && cargo audit --deny warnings \
  && cargo deny check \
  && cargo vet \
  && cargo machete \
  && cargo nextest run
```

## Tooling

All dev tools are provisioned by mise and pinned in `mise.toml` for reproducible builds across machines.

| Tool | Purpose |
|------|---------|
| `cargo nextest` | Test runner (faster than `cargo test`) |
| `cargo clippy` | Lint with `-D warnings` |
| `cargo fmt` | Rust formatting |
| `cargo sort` | Cargo.toml dependency sorting |
| `taplo` | TOML formatting |
| `cargo audit` | Security advisory scanning |
| `cargo deny` | License, ban, and source policy enforcement |
| `cargo vet` | Supply-chain audit |
| `cargo machete` | Unused dependency detection |
| `cargo tarpaulin` | Code coverage |
| `cargo criterion` | Benchmarking |
| `cargo flamegraph` | Performance profiling |
| `cargo fuzz` | Fuzz testing |
| `cargo mutants` | Mutation testing |
| `gitleaks` | Secret scanning |
| `hk` | Git hook manager (Rust) |
| `git-cliff` | Changelog generation |
| `rumdl` | Markdown lint |
| `yamllint-rs` | YAML lint (Rust) |
| `actionlint` | GitHub Actions workflow lint |
| `reuse` | SPDX/REUSE compliance (Python tool, for compat testing) |

## Key Design Decisions

- **No `unsafe`** -- forbidden workspace-wide. Safety without unsafe blocks.
- **REUSE spec compatible** -- same REUSE.toml, same lint output as Python reuse-tool.
- **Multi-surface** -- one core library powers CLI, WASM, LSP, and MCP.
- **Virtual workspace** -- each crate independently versioned, faster compilation.
- **TDD** -- all code is written test-first.
- **Stable Rust** -- edition 2024, pinned toolchain.

## License

Dual-licensed under **MIT OR Apache-2.0**. See `LICENSE` and `LICENSE-APACHE`.

Documentation is licensed under **CC-BY-3.0+**. See `CC-BY-3.0.txt`.

All source files carry SPDX headers:

```text
SPDX-FileCopyrightText: 2026 COMPLY contributors
SPDX-License-Identifier: MIT OR Apache-2.0
```

## Contributing

See `docs/guidelines/contribution.md` and `AGENTS.md` for AI agent rules and workflow.
