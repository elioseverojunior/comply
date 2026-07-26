---
name: worktree
description: Create a git worktree for feature work in this repository, or reintegrate a finished branch while keeping main linear. Triggered by the user only.
disable-model-invocation: true
---

# Worktree

comply does feature work in a git worktree per branch, and keeps `main` a
straight line. `$ARGUMENTS` is the branch name.

## Create

```sh
git worktree add .worktrees/<branch> -b <branch>    # new branch
git worktree add .worktrees/<branch> <branch>       # existing branch
```

`.worktrees/` is gitignored (as are `worktrees/`, `.worktree/`, `worktree/`).
Confirm with `git worktree list` afterwards and report the path.

A fresh worktree needs `mise trust` before mise will run tasks in it.

## Reintegrate

Rebase, then fast-forward. Never `--no-ff`: `main` must stay linear.

```sh
# from the main worktree
git pull --rebase
git rebase main .worktrees/<branch>
git merge .worktrees/<branch>          # must fast-forward
```

If `git merge` reports anything other than a fast-forward, stop and report it --
that means the rebase did not take, and forcing it would create a merge commit.

## Remove

```sh
git worktree remove .worktrees/<branch>
git worktree prune
```

Refuse to remove a worktree with uncommitted changes unless the user explicitly
confirms losing them. Show `git -C .worktrees/<branch> status --short` first.

## Boundaries

`git worktree`, `git rebase`, and a fast-forward `git merge` are local history
operations and are in scope here. Publishing is not:

- Do NOT `git push` under any circumstances.
- Do NOT `gh pr create`, and do NOT write a PR description. Per
  `docs/guidelines/contribution.md` this is non-overridable and the penalty is a
  contributor ban.
- Committing needs explicit per-action human approval, with `git commit -S` and
  an `Assisted-by:` trailer. Never `Co-authored-by:`.

Before rebasing, verify the branch is clean and the tests pass -- use the
`verify` skill. Rebasing a broken tree just moves the breakage.
