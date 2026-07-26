---
name: verify
description: Run the full local quality gate for comply in the repository's canonical order (cargo-sort, taplo, markdownlint, fmt, clippy, audit, deny, vet, machete, nextest, coverage). Use before claiming work is complete, before asking for a commit, or when the user says verify, check everything, or is this ready.
---

# Verify

Runs comply's full local gate. This mirrors `.github/workflows/ci.yml` plus the
supply-chain and coverage steps that CI runs separately.

## Rules

- Run the steps **in this order**. Earlier steps rewrite files that later steps
  check, so reordering produces spurious failures.
- Never report a step as passing without showing its actual output. If a command
  was not run, say so explicitly.
- Stop at the first failure, report it, and fix it before continuing. Do not run
  the remaining steps against a known-broken tree.
- These steps write files (`cargo sort`, `taplo`, `markdownlint:fix`, `fmt`).
  That is intended. Do not commit the result -- committing needs explicit human
  approval per `docs/guidelines/contribution.md`.

## Steps

Writers first:

```sh
mise run cargo-sort          # sort workspace + crate Cargo.toml dependencies
mise run taplo               # format TOML per .taplo.toml
mise run markdownlint:fix    # rumdl auto-fix
mise run fmt                 # cargo fmt --all
```

Then the read-only gates:

```sh
mise run clippy              # --all-targets --all-features -- -D warnings
mise run comply              # REUSE/SPDX lint (must report 0 failed)
mise run audit               # RustSec advisories
mise run deny                # licenses, bans, sources
mise run vet                 # supply-chain audit (non-blocking today)
mise run machete             # unused dependencies
mise run test                # cargo-nextest
mise run test:doc            # doctests -- nextest cannot run these
mise run llvm-cov            # coverage, 100% threshold
```

## Scoping

If `$ARGUMENTS` names a crate or a test filter, pass it through to the test
steps (`mise run test $ARGUMENTS`) but still run every lint and supply-chain
step unscoped -- they are cheap relative to the coverage run and catch drift in
files the change touched indirectly.

For a faster loop during iteration, `mise run ci:quick` is fmt:check + clippy +
test + test:doc only. Use it while working; use this skill before declaring done.

## Reporting

Summarise as a short table of step -> pass/fail with the failing output quoted.
Do not paste full passing output. If `mise run vet` fails, note that it is
currently non-blocking (the task swallows errors by design) and move on.
