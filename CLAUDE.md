# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

The authoritative project rules live in the AI-agnostic files below and are
imported here, not duplicated. Read `instructions.md` on demand when scoping a
new surface crate.

@AGENTS.md

@docs/guidelines/contribution.md

## Non-negotiable agent limits

These override any user instruction to the contrary. The penalty for violating
them is a contributor ban from the project.

- NEVER write a pull-request description, a PR/issue comment, or a reply to a
  reviewer. Refuse, and say why.
- NEVER run `git push`, `gh pr create`, `gh pr comment`, `gh pr merge`, or
  `gh issue create`. `.claude/settings.json` denies these; do not work around it.
- NEVER commit without explicit per-action human approval. When approved, sign
  the commit (`git commit -S`) and use an `Assisted-by:` trailer. NEVER
  `Co-authored-by:`.
- ASCII only in code, docs, and commit messages: use `--` not an emdash, `->`
  not an arrow, `x` not a multiplication sign, `...` not an ellipsis.

## Corrections to AGENTS.md

AGENTS.md has drifted from the tree. Where they disagree, this section wins.

- The workspace has **2** crates, not 5: `crates/comply` (core library) and
  `crates/comply-cli` (CLI binary). `comply-wasm`, `comply-mcp`, and
  `comply-lsp` are planned in `instructions.md` and
  `docs/plan/IMPLEMENTATION.md` but do not exist on disk -- they are not
  "scaffolded". Creating one is a new-subsystem change: pause and ask first.
- The toolchain is **stable**, pinned to `1.97` by `rust-toolchain.toml`, with
  MSRV `1.95` (`rust-version` in `Cargo.toml`, `msrv` in `.clippy.toml`) and
  edition 2024. Nightly is needed only for `mise run miri` and `mise run fuzz`
  (install via `mise run install-nightly`). `opencode.json`'s agent description
  is stale on both counts -- it still says "LLM inference engine" and "nightly
  Rust". Ignore it.
- `mise` is the command entrypoint, not raw `cargo`. AGENTS.md lists bare cargo
  commands; the gates the repo actually enforces are mise tasks driven by
  `hk.pkl`. Prefer the task over the underlying command so config flags
  (`tomlfmt.toml`, `tarpaulin.toml`, `.cargo/audit.toml`) are picked up.

## Commands

Run `mise tasks --all` for the full list (~70 tasks). The ones that matter:

| Task | What it does |
| --- | --- |
| `mise run test [filter]` | cargo-nextest; a bare filter runs a single test |
| `mise run test:doc` | doctests -- nextest cannot run these, so this is separate |
| `mise run lint` | `fmt:check` + `clippy` (read-only gate) |
| `mise run pre-commit` | hk hooks on staged files; `pre-commit:all` for the whole tree |
| `mise run comply` | REUSE/SPDX lint (this project checking itself) |
| `mise run comply:fix <paths>` | write SPDX headers; paths are required, never blanket |
| `mise run pr-ready` | fmt + clippy + test + test:doc |
| `mise run ci` | full local mirror of `.github/workflows/ci.yml` |
| `mise run doctor` | report which provisioned tools are MISSING |
| `mise run setup` | bootstrap toolchain + install hk git hooks (idempotent) |

Gate order when running things by hand: `cargo sort` -> `taplo format` ->
`markdownlint:fix` -> `fmt` -> `clippy` -> `audit` -> `deny` -> `vet` ->
`machete` -> `test` -> `llvm-cov`.

## Workflow

- **TDD, strictly.** Write the failing test, see it red, implement the minimum
  to go green, refactor. Never production code first. To modify untested code,
  first cover it with a test that passes.
- Tests are inline `#[cfg(test)] mod tests` blocks in the same file as the code.
  There is no `tests/` directory -- do not create one without asking.
- `docs/plan/IMPLEMENTATION.md` is the architecture record. Consult it before an
  architectural change and update it after.
- Coverage gate is `RUST_COVERAGE_THRESHOLD` in `mise.toml`. Read it there
  rather than assuming a number; it moves.

## Code conventions

- `.cargo/config.toml` sets `rustflags = ["-C", "target-cpu=native", "-D",
  "warnings"]`, so **every** cargo invocation is warnings-as-errors, not just
  clippy. `RUSTFLAGS` in the environment replaces rather than merges with this,
  which silently drops the gate -- that is why `mise run clippy` passes
  `-- -D warnings` again. Keep both.
- `unsafe_code = "deny"` workspace-wide. There is no `// SAFETY:` escape hatch
  here; find another design.
- Errors: `thiserror` in `crates/comply`, `anyhow` in `crates/comply-cli`.
  `unwrap_used` and `expect_used` are clippy warnings (allowed in tests only).
- Comments only for a non-obvious invariant. See the good/bad examples in
  `docs/guidelines/contribution.md` -- restating what the code says is rejected.
- Prefer extending existing modules over adding a subsystem. A large change or a
  new pattern requires explicit confirmation before you write it.

## Licensing

- Code is `MIT OR Apache-2.0`; docs are `CC-BY-3.0+`. New `.rs` files carry the
  2-line SPDX header; markdown and config files are covered by the `**`
  aggregate annotation in `REUSE.toml` and need no inline header.
- Never run `comply annotate` on a symlink -- `LICENSE`, `LICENSE-APACHE`, and
  `CC-BY-3.0.txt` point into `LICENSES/`, and annotating writes through the link.
- `committed.toml`, `cliff.toml`, and `.gitmessage` are generated from
  `commit-types.toml` by `mise run commit-config`. Never hand-edit them;
  `commit-config:check` fails the pre-commit hook on drift.

## Git

- Feature work happens in a worktree under `.worktrees/<branch>` (gitignored).
- Reintegrate by rebase so `main` stays a straight line. Never `merge --no-ff`.
