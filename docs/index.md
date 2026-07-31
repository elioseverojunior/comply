---
layout: home

hero:
  name: comply
  text: REUSE compliance in pure Rust
  tagline: Every file in a project should say who owns it and how it may be used. comply checks that, and fixes what it can.
  actions:
    - theme: brand
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: CLI reference
      link: /reference/cli
    - theme: alt
      text: View on GitHub
      link: https://github.com/elioseverojunior/comply

features:
  - title: Agrees with the reference tool
    details: Checked against reuse 6.2.0 on a real repository -- 109 files seen by both, zero licensing disagreements. Where the two differ, the reason is recorded rather than discovered later.
    link: /PARITY
    linkText: Read the parity report
  - title: One binary, no runtime
    details: A single static-friendly executable. The SPDX licence list is compiled in, so lint works offline. No Python, no virtualenv, no interpreter to keep on the PATH.
  - title: Fixes, not just findings
    details: annotate writes headers, fix repairs a whole tree, format canonicalises what is already there. Uncommentable files get a .license companion instead of a corrupted first line.
    link: /reference/cli
    linkText: See the commands
  - title: Built to be trusted
    details: unsafe forbidden workspace-wide, warnings denied on every build, 100% line coverage enforced as a gate, and a fuzz target over the SPDX expression parser.
  - title: Fits an existing project
    details: Reads the REUSE.toml and .reuse/dep5 you already have. A [tool.comply] section adds ignore rules, traversal switches and custom comment syntaxes without breaking the spec.
    link: /guide/configuration
    linkText: Configure it
  - title: Made for pre-commit
    details: lint-file checks only the files that changed, so the cost scales with the diff rather than the repository. spdx emits a reproducible SPDX 2.1 bill of materials for review.
---

<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

<!-- rumdl-disable MD041 -->
<!-- A VitePress `layout: home` page has no H1: the title is rendered from the
     `hero.name` frontmatter above. Adding one would print a stray heading
     directly beneath the hero. -->

## Install

```sh
cargo install comply-cli
```

The binary is named `comply`.

## Check a project

```sh
comply lint
```

```text
REUSE lint report
113 total, 112 passed, 0 failed, 1 skipped
```

Exit status carries the verdict, so it drops straight into CI:

```sh
comply lint --quiet || echo "not compliant"
```

## Fix what is missing

```sh
comply init                      # create REUSE.toml and LICENSES/
comply download --all            # fetch every licence text the project references
comply annotate --license MIT --copyright "Your Name" src/main.rs
comply fix                       # repair the whole tree
```

Every command and flag is documented in the [CLI reference](/docs/reference/cli).
When something misbehaves, start with [troubleshooting](/docs/guide/troubleshooting).
