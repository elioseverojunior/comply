---
name: precommit
description: Run comply's hk-managed pre-commit hooks and interpret the results. Use when the user asks to run pre-commit, run the hooks, check staged files, or when a git commit was rejected by a hook.
---

# Pre-commit

comply uses [hk](https://github.com/jdx/hk) (configured in `hk.pkl`), not the
Python `pre-commit` tool. Every hk step delegates to a mise task, so a hook
failure is reproducible by running that task directly.

## Commands

```sh
mise run pre-commit          # hk run pre-commit -- staged files only
mise run pre-commit:all      # hk run pre-commit --all-files -- whole tree
```

The `pre-commit` hook runs with `fix = true` and `stash = "git"`: hk stashes
unstaged work, applies auto-fixes to the staged set, then restores. If a run is
interrupted, check `git stash list` before retrying.

## What the hooks cover

Auto-fixing steps (trailing whitespace, end-of-file, mixed line endings,
`cargo-sort`, `taplo`, `rumdl`, `comply:fix`) rewrite files in place. Re-stage
what they changed.

Check-only steps that you must fix by hand:

- `gitleaks` -- a secret was detected. Never "fix" this by adding an allowlist
  entry without asking the user; the finding may be real.
- `actionlint` / `yamllint-rs` -- workflow and YAML errors.
- `fmt-check`, `clippy` -- run `mise run fmt` then `mise run clippy`.
- `comply` -- REUSE lint. Use the `comply-headers` skill.
- `commit-config:check` -- `committed.toml`, `cliff.toml`, or `.gitmessage` are
  stale relative to `commit-types.toml`. Fix with `mise run commit-config`; never
  hand-edit the derived files.
- `audit`, `deny` -- dependency advisories and policy. These fire on any
  `Cargo.toml` / `Cargo.lock` change.

The `commit-msg` hook runs `committed` against `committed.toml`. A rejected
commit message is a conventional-commit format problem -- check
`commit-types.toml` for the allowed types.

## Boundaries

Running the hooks is fine. Committing is not: do not run `git commit` unless the
user has explicitly approved that specific commit, and never write the commit
message for them (`docs/guidelines/contribution.md`).

If the hooks are not installed at all, `mise run setup` provisions the tools and
runs `hk install --global`.
