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
- Virtual workspace with 2 crates under `crates/`: `comply` (the compliance
  engine plus the `comply` binary, the latter behind the default-on `cli`
  feature) and `comply-wasm` (browser bindings). A library-only consumer opts
  down with `default-features = false`.
- `comply-cli` was merged into `comply` so that `cargo install comply` -- the
  command a user actually types, the binary having always been named `comply` --
  installs the tool. It appears in `context.md` and in git history.
- `comply-mcp` and `comply-lsp` appear in `context.md`

## Toolchain

- **Stable Rust** pinned via `rust-toolchain.toml`.
- Warnings-as-errors enforced via `[workspace.lints]` in root `Cargo.toml` + `.cargo/config.toml`.
- `rustfmt.toml` configures import reordering: std -> external crates -> local.
- `.rumdl.toml` controls Markdown formatting.

## Workflow

- **TDD strictly**: write test -> see red -> implement -> green -> refactor. Never write production code first.
- **Commands -- mise is the entrypoint, not raw cargo.** Each task carries the config flags
  the bare command would miss (`tarpaulin.toml`, `.cargo/audit.toml`,
  `tomlfmt.toml`), so a hand-run `cargo` invocation gates on something narrower
  than CI does. `mise tasks --all` lists them all.

  ```sh
  mise run coverage:tarpaulin   # THE test gate: tests + doctests + coverage
  mise run lint                 # fmt:check + clippy (read-only)
  mise run pre-commit           # hk hooks on staged files
  mise run comply               # REUSE/SPDX lint (this project on itself)
  mise run pr:ready             # fmt + headers + version/lock drift + clippy + gate
  mise run ci                   # full local mirror of .github/workflows/cicd.yml
  ```

- Order when running gates by hand: `cargo:sort` -> `taplo:fmt` ->
  `markdownlint:fix` -> `cargo:fmt` -> `cargo:clippy` -> `cargo:audit` ->
  `cargo:deny` -> `cargo:vet` -> `cargo:machete` -> `coverage:tarpaulin`.
- Plan file `docs/plan/IMPLEMENTATION.md` must be consulted/updated for architecture decisions.

## Worktrees

- Each feature branch gets its own worktree under `.worktrees/` (ignored by git).

  ```sh
  git worktree add .worktrees/<branch> <branch>
  ```

- **Reintegrate via rebase** to keep main linear:

  ```sh
  # from main worktree
  git pull --rebase
  git rebase main .worktrees/<branch>
  git merge .worktrees/<branch>   # fast-forward
  ```

- Never merge with `--no-ff` -- main must stay a straight line.

## Architecture / design

- REUSE Specification compatible: same REUSE.toml, same .reuse/dep5, same lint output.
- Security-first: use `thiserror` for errors in libraries, `anyhow` in binaries.
  Unsafe code is forbidden (`unsafe_code = "deny"`).
- Principles: TDD, KISS, DRY, YAGNI, TDA, SOLID.
- No comments unless explaining a non-obvious invariant.

## Status

Core library and CLI are implemented and covered. `comply-wasm` carries string
adapters over `comply` and no compliance logic, by design -- `wasm32-unknown-unknown`
never runs under the host test harness, so logic placed there would be counted by
the coverage gate and never executed.

Two dated "session context" sections used to sit here, describing an earlier
scope this project no longer has. They were where `comply-mcp`, `comply-lsp` and
`intelligent.rs` came from -- names that sent readers looking for crates and
modules that do not exist. Point-in-time progress belongs in git history and in
`docs/plan/IMPLEMENTATION.md`, which is the architecture record.

### Known limitations

- REUSE.toml and DEP5 mutual exclusivity not yet enforced (currently merged).

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
| `crates/comply/Cargo.toml` | Compliance library + `comply` binary (`cli` feature) |
| `crates/comply-wasm/Cargo.toml` | Browser bindings crate (no compliance logic) |
| `context.md` | Owner's vision: REUSE compliance tool |
| `docs/contribution.md` | AI agent rules -- read before committing/pushing/PRing |
| `docs/plan/IMPLEMENTATION.md` | Architecture plan |
| `docs/architecture.md` | Detailed architecture with diagrams |
| `LICENSE` | Dual-license notice: states the MIT OR Apache-2.0 choice |
| `LICENSE-MIT` | MIT license text (for code) |
| `LICENSE-APACHE` | Apache-2.0 license text (for code) |
| `CC-BY-3.0.txt` | CC-BY-3.0 license text (for docs) |
| `REUSE.toml` | REUSE compliance manifest |

## Agent rules (from `docs/contribution.md`)

- Never commit, push, or create PRs without explicit human approval.
- Never write PR descriptions, commit messages, or reviewer responses.
- Never add an assistant attribution trailer to a commit: not `Co-authored-by:`,
  and not `Assisted-by:` either. Sign with `git commit -S` and let the message
  end at its own body.
- No unicode chars (`--`, `->`, `x`, `...`) -- use ASCII.
