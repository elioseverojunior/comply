<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# Configuration

comply reads the configuration the REUSE Specification defines, and adds a
`[tool.comply]` section for things the spec does not cover. A project that only
uses the spec keys stays portable: the Python `reuse` tool reads the same file
and reaches the same conclusions.

## REUSE.toml

The manifest lives at the project root. Accepted names are `REUSE.toml`,
`reuse.toml` and `.reuse.toml`.

```toml
version = 1
SPDX-PackageName = "comply"
SPDX-PackageSupplier = "COMPLY contributors"
SPDX-PackageDownloadLocation = "https://github.com/elioseverojunior/comply"

[[annotations]]
path = ["**"]
precedence = "aggregate"
SPDX-FileCopyrightText = "COMPLY contributors"
SPDX-License-Identifier = "MIT OR Apache-2.0"
```

Each `[[annotations]]` block answers for the paths its `path` globs match, so a
project can license a whole tree without putting a header in every file.

### Precedence

When both a file header and an annotation apply, `precedence` decides which
wins:

| Value       | Meaning                                                                                                |
| ----------- | ------------------------------------------------------------------------------------------------------ |
| `aggregate` | The annotation supplies what the file does not state itself. A file header still wins where it exists. |
| `closest`   | The annotation wins over the file's own header.                                                        |
| `override`  | The annotation wins over everything, including a `.license` companion.                                 |

`aggregate` is the usual choice. Reach for `override` only when a file's own
header is wrong and cannot be edited -- vendored code, generated output.

### .reuse/dep5

The older DEP5 format is read as well. To move off it:

```sh
comply convert-dep5 --dry-run   # print the REUSE.toml it would write
comply convert-dep5             # write it and remove the dep5
```

Conversion refuses to overwrite an existing `REUSE.toml`, because that file may
hold annotations the conversion knows nothing about.

## [tool.comply]

Everything below is a comply extension. `reuse` ignores it, which means these
keys change what comply checks without changing what the spec says.

### ignore

Glob patterns for paths outside REUSE analysis.

```toml
[tool.comply]
ignore = [
  "context.md",
  "**/*.lock",
  "**/target/**",
  "vendor/**",
]
```

Use it for files that are genuinely not part of the project, and for files that
must never be written to. Annotating a symlink, for instance, writes through the
link and corrupts the file it points at.

::: warning
`ignore` removes a path from the walk entirely. A licence that only an ignored
file declares still counts as used -- comply reads the annotation covering it --
but the file itself is neither checked nor reported.
:::

### walk

Traversal switches. Each mirrors a rule comply applies by default.

```toml
[tool.comply.walk]
hidden = true          # descend into dot-directories
git-ignore = true      # honour .gitignore
git-global = true      # honour the user's global gitignore
git-exclude = true     # honour .git/info/exclude
require-git = false    # apply gitignore rules outside a repository
follow-links = false   # follow symlinks
```

Files git tracks are checked even when a gitignore rule would hide them, because
git does not apply those rules to tracked files either.

### comment-styles

comply knows the comment syntax of the common file types. A project working in a
language it does not know can declare one:

```toml
[[tool.comply.comment-styles]]
extensions = ["vue", "svelte"]
open = "<!--"
line = ""
close = "-->"

[[tool.comply.comment-styles]]
filenames = ["Jenkinsfile"]
open = "//"
```

| Key          | Meaning                                                   |
| ------------ | --------------------------------------------------------- |
| `extensions` | Extensions this rule claims, without the leading dot.     |
| `filenames`  | Exact filenames, for files with no extension.             |
| `open`       | Opening delimiter, or the whole prefix for a line style.  |
| `line`       | Prefix repeated on each line inside a block. Often empty. |
| `close`      | Closing delimiter. Empty means a line style.              |

One rule covers every extension sharing a syntax, so `["vue", "svelte"]` is one
block rather than two. A rule must claim at least one `extensions` or
`filenames` entry.

An exact `filenames` match beats an `extensions` match. Two rules claiming the
same key are refused when the manifest is parsed, rather than resolved by
position:

```text
Error: two [[tool.comply.comment-styles]] rules claim the extension `vue`;
remove one, or lookup order would silently decide
```

Extensions are compared case-insensitively, so `VUE` and `vue` collide.
Filenames are compared exactly, because a filesystem may distinguish them. The
two are separate namespaces -- a `makefile` extension does not contest a
`makefile` filename.

For a block style, `line` is what leads each line of the header body:

```toml
[[tool.comply.comment-styles]]
extensions = ["jsp"]
open = "<%--"
line = " * "
close = "--%>"
```

produces

```text
<%--
 * SPDX-FileCopyrightText: 2026 Your Name
 *
 * SPDX-License-Identifier: MIT
 --%>
```

### Defaults

comply's built-in styles are not replaced by this table -- a configured rule
wins for the keys it claims, and everything else keeps its built-in. Nothing
needs re-declaring to keep working.
