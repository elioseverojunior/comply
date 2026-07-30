<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# comply -- AGENTS.md

## Project identity

- **comply**: REUSE compliance tool in pure Rust. Checks software projects
  for compliance with the [REUSE Specification](https://reuse.software/spec/).
  Native implementation (not a port of the Python reuse-tool), with full
  configuration compatibility.
- Virtual workspace with 5 crates under `crates/`: `comply` (core library),
  `comply-cli` (CLI binary), `comply-wasm` (WASM target), `comply-mcp`
  (MCP server), `comply-lsp` (LSP server).

## Toolchain

- **Stable Rust** pinned via `rust-toolchain.toml`.
- Warnings-as-errors enforced via `[workspace.lints]` in root `Cargo.toml` + `.cargo/config.toml`.
- `rustfmt.toml` configures import reordering: std -> external crates -> local.
- `.rumdl.toml` controls Markdown formatting.

## Workflow

- **TDD strictly**: write test -> see red -> implement -> green -> refactor. Never write production code first.
- **Commands**:

  ```sh
  cargo nextest run             # all tests (nextest)
  cargo nextest run <test_name>  # single test
  cargo clippy -- -D warnings   # Rust lint
  cargo sort                    # sort Cargo.toml dependencies
  cargo audit --deny warnings   # check security advisories
  cargo deny check              # check licenses, bans, sources
  cargo vet                     # supply chain vetting
  cargo machete                 # detect unused dependencies
  taplo format --check Cargo.toml crates/*/Cargo.toml  # TOML formatting check
  mise run markdownlint         # markdown lint
  mise run markdownlint:fix     # auto-fix markdown issues
  ```

- Order: `cargo sort` -> `taplo format Cargo.toml crates/*/Cargo.toml` -> `mise run markdownlint:fix` ->
  `cargo fmt` -> `cargo clippy -- -D warnings` -> `cargo audit --deny warnings` ->
  `cargo deny check` -> `cargo vet` -> `cargo machete` -> `cargo nextest run` ->
  `mise run coverage:llvm`.
- Plan file `docs/plan/IMPLEMENTATION.md` must be consulted/updated for architecture decisions.

## Worktrees

- Each feature branch gets its own worktree under `worktrees/` (ignored by git).

  ```sh
  git worktree add worktrees/<branch> <branch>
  ```

- **Reintegrate via rebase** to keep main linear:

  ```sh
  # from main worktree
  git pull --rebase
  git rebase main worktrees/<branch>
  git merge worktrees/<branch>   # fast-forward
  ```

- Never merge with `--no-ff` -- main must stay a straight line.

## Architecture / design

- REUSE Specification compatible: same REUSE.toml, same .reuse/dep5, same lint output.
- Security-first: use `thiserror` for errors in libraries, `anyhow` in binaries.
  Unsafe code is forbidden (`unsafe_code = "deny"`).
- Principles: TDD, KISS, DRY, YAGNI, TDA, SOLID.
- No comments unless explaining a non-obvious invariant.

## Session context (Jul 14-15, 2026) -- Legacy LLM work

The project was originally scoped as a llama.cpp LLM inference engine port
and later pivoted to REUSE compliance. The following session context
documents the old work for history, but all new development follows the
compliance architecture described in `docs/plan/IMPLEMENTATION.md` and
`docs/ARCHITECTURE.md`.

### Old LLM work (archived)

- **Download subcommand**: Added `comply-cli download` to fetch GGUF models from HuggingFace Hub.
- **Model caching**: Models saved to `~/.cache/huggingface/hub/` following HF conventions.
- **License creation**: MIT, Apache-2.0, CC-BY-3.0 license texts. REUSE.toml annotations.
- **SPDX headers**: Code files `MIT OR Apache-2.0`, docs `CC-BY-3.0+`.

## Session context (Jul 15-16, 2026) -- Phases 1 & 2 complete

### What was done

- **Project redirection**: comply is now a REUSE compliance tool, replacing
  the previous LLM inference engine scope.
- **Architecture planned**: 5-crate workspace: `comply` (core), `comply-cli`
  (CLI), `comply-wasm` (WASM), `comply-mcp` (MCP), `comply-lsp` (LSP).
- **IMPLEMENTATION.md rewritten**: Full architecture plan for REUSE compliance tool.
- **ARCHITECTURE.md rewritten**: Mermaid diagrams for dependency graph and
  surface crate designs.
- **Phase 1 (core library)**: SPDX parser, header detection, file discovery,
  license DB (SPDX 3.28 bundled), REUSE.toml/DEP5 parsing, SHA-256 hashing,
  report generation, auto-fix -- all implemented with TDD.
- **Phase 2 (CLI)**: All 5 subcommands implemented (init, format, lint,
  annotate, fix) with `.license` precedence and REUSE-IgnoreStart/End support.
- **145 tests passing**, Python `reuse lint` compatibility confirmed, GPG-signed
  commit `f2d59ae`.
- **Dockerfile**: Multi-stage with tini, non-root user, mold linker, OCI labels.
- **Mise tasks**: docker-build, comply lint/fix, audit, deny.

### Known limitations

- WASM, LSP, and MCP surface crates are scaffolded but not implemented.
- REUSE.toml and DEP5 mutual exclusivity not yet enforced (currently merged).
- `intelligent.rs` module (AI-assisted CLI) not yet implemented.

### URLs

- REUSE specification: <https://reuse.software/spec/>
- REUSE Python tool: <https://github.com/fsfe/reuse-tool>
- REUSE.toml format: <https://reuse.software/spec-3.3/>
- SPDX License List: <https://spdx.org/licenses/>
- Model Context Protocol: <https://modelcontextprotocol.io>

## License model

### Code: `MIT OR Apache-2.0`

Dual-licensing provides:

- **MIT**: simple permissive license, widely compatible, lets anyone use the code with minimal restrictions.
- **Apache-2.0**: MIT's permissions PLUS an explicit patent grant. Apache-2.0 Section 3 grants a patent license from contributors, which protects downstream users from patent litigation. This is the Rust ecosystem standard (`Cargo.toml` convention), and every crate published to crates.io inherits this model.

Both licenses are OSI-approved. Users may choose either. `LICENSE` states the
choice; `LICENSE-MIT` and `LICENSE-APACHE` contain the full texts.

### Docs: `CC-BY-3.0+`

Documentation files (`CONTRIBUTING.md`, `CII_BEST_PRACTICES.md`, etc.) use Creative Commons Attribution 3.0 Unported or any later version. Rationale:

- **Why not MIT/Apache-2.0 for docs?** MIT and Apache-2.0 are software licenses -- they cover "source code," not "creative works." While technically usable for docs, Creative Commons is the standard for documentation, and the OpenSSF CII Best Practices badge explicitly requires a **documentation license** for its "FLOSS license" criterion.
- **Why CC-BY-3.0?** It is the OSI-approved Creative Commons license with the simplest requirements: anyone can share/adapt the docs as long as they give attribution. No ShareAlike copyleft (which would prevent commercial reuse), no ND (which would prevent modification).
- **Why `+` (or any later version)?** Future-proofing -- downstream users can use CC-BY-4.0 or later if they prefer. The CII badge specifically recommends CC-BY-3.0+ or CC-BY-4.0.
- **What about CC-BY-4.0?** Also acceptable. 3.0+ was chosen to match the SPDX short identifier convention used by REUSE, and 3.0 is the version most commonly cited in badge documentation.

### Enforcement

- All files carry SPDX headers (inline or via `REUSE.toml` aggregate annotations).
- `mise run comply` in CI enforces compliance -- zero warnings required.
- Code files inherit `MIT OR Apache-2.0` from the `REUSE.toml` aggregate annotation.
- Doc files use inline `CC-BY-3.0+` SPDX license identifiers.
- License text files (`LICENSE-MIT`, `LICENSE-APACHE`, `CC-BY-3.0.txt`, `DCO.txt`) use explicit `REUSE.toml` annotations.

## Key files

| File | Purpose |
| --- | --- |
| `rust-toolchain.toml` | Pinned stable channel |
| `.cargo/config.toml` | Build/linker tuning, profile optimization |
| `rustfmt.toml` | Import reorder: std, external, local last |
| `deny.toml` | License/bans/sources policy (cargo-deny) |
| `.cargo/audit.toml` | Advisory severity thresholds |
| `supply-chain/` | cargo-vet supply chain audits |
| `.taplo.toml` | TOML formatting rules, schema validation disabled |
| `Cargo.toml` | Virtual workspace root (`[workspace]` only) |
| `crates/comply/Cargo.toml` | Core compliance library crate |
| `crates/comply-cli/Cargo.toml` | CLI binary crate |
| `crates/comply-wasm/Cargo.toml` | WASM target crate |
| `crates/comply-mcp/Cargo.toml` | MCP server crate |
| `crates/comply-lsp/Cargo.toml` | LSP server crate |
| `instructions.md` | Owner's vision: REUSE compliance tool |
| `docs/guidelines/CONTRIBUTION.md` | AI agent rules -- read before committing/pushing/PRing |
| `docs/plan/IMPLEMENTATION.md` | Architecture plan |
| `docs/ARCHITECTURE.md` | Detailed architecture with diagrams |
| `LICENSE` | Dual-license notice: states the MIT OR Apache-2.0 choice |
| `LICENSE-MIT` | MIT license text (for code) |
| `LICENSE-APACHE` | Apache-2.0 license text (for code) |
| `CC-BY-3.0.txt` | CC-BY-3.0 license text (for docs) |
| `REUSE.toml` | REUSE compliance manifest |

## Agent rules (from `docs/guidelines/CONTRIBUTION.md`)

- Never commit, push, or create PRs without explicit human approval.
- Never write PR descriptions, commit messages, or reviewer responses.
- Use `Assisted-by:` (not `Co-authored-by:`) if user asks you to commit.
- No unicode chars (`--`, `->`, `x`, `...`) -- use ASCII.
