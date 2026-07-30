---
name: instructions.md
created_by: <elioseverojunior@gmail.com>
---

# Instructions

I want to build a new project called `comply` in pure Rust. It must be a
REUSE compliance tool that checks and enforces the
[REUSE Specification](https://reuse.software/spec/) for software licensing.

## Project Scope

comply is NOT a port of the Python
[fsfe/reuse-tool](https://github.com/fsfe/reuse-tool/). It is a new, native
Rust implementation. However, it must maintain full configuration
compatibility -- same REUSE.toml format, same DEP5 format, same lint output.

The project has multiple surfaces:

- **comply** (core library) -- SPDX parsing, header detection, file
  classification, license detection, REUSE.toml/DEP5 parsing
- **comply-cli** -- CLI with subcommands: init, format, lint, annotate, fix,
  plus intelligent management features
- **comply-wasm** -- WASM target for browser-based compliance checking
- **comply-mcp** -- MCP (Model Context Protocol) integration for AI-assisted
  compliance
- **comply-lsp** -- LSP (Language Server Protocol) for IDE real-time
  compliance feedback

## Rust SDK Requirements

- Must use Rust (stable) that allows new features (edition 2024).
- We must focus on Security-first and Performance.

## AI Requirements

You must write the needed agents, skill, prompts and other configs including
AI-agentics. I want AI agnostic configurations that can be reused by Claude,
Codex, Copilot, Bedrock and so on. I want the AI to be able to do fully
autonomous work. Check the
[docs/guidelines/CONTRIBUTION.md](docs/guidelines/CONTRIBUTION.md).

## Coding Specifications

Ensure we use TDD before writing any code line.
Ensure the development principles:

1. TDD (If we need to change existing code without TDD, first write the
   test using TDD and ensure the test is working and then start to
   convert/refactor the context we intended to modify, always doing TDD
   implementation loop till green)
2. KISS (Keep It Simple, Stupid)
3. DRY (Don't Repeat Yourself)
4. YAGNI (You Aren't Gonna Need It)
5. TDA (Tell Don't Ask)
6. SOLID (Use the SOLID Principles that make sense to the project).

Write the plan into [docs/plan/IMPLEMENTATION.md](docs/plan/IMPLEMENTATION.md).
