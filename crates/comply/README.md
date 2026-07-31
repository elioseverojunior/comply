<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# comply

Core [REUSE Specification](https://reuse.software/spec/) compliance library --
SPDX expression parsing, header detection, file classification, license
auditing, and REUSE.toml/DEP5 parsing.

This crate is the engine behind the `comply` binary shipped by
[comply-cli](https://crates.io/crates/comply-cli). It is a native
implementation rather than a port of the Python reuse-tool, and reads only the
project tree it is pointed at.

## Modules

| Module | Description |
|--------|-------------|
| `config` | REUSE.toml and `.reuse/dep5` parsing, annotation matching |
| `error` | Typed error enum built with `thiserror` |
| `file` | Discovery, classification, `.license` companions, exclusion |
| `fix` | Auto-repair of missing headers and license files |
| `hash` | SHA-256 integrity hashing |
| `header` | SPDX header detection, parsing, generation, annotation |
| `license` | Bundled SPDX License List 3.28, project license audit |
| `report` | Lint reports and SPDX bill-of-materials generation |
| `reuse_toml` | REUSE.toml serialization |
| `scope` | Single source of truth for which paths are covered |
| `spdx` | SPDX expression parser and validator |

## Usage

Validate an SPDX expression:

```rust
comply::spdx::validate("MIT OR Apache-2.0")?;
assert!(comply::spdx::validate("MIT OR").is_err());
# Ok::<(), comply::error::Error>(())
```

Read SPDX tags out of file contents:

<!-- The sample below contains real SPDX tags inside a Rust string. Without
     these guards a scanner reads them as this file's own licensing and tries
     to parse `MIT\n";` as an expression. REUSE 3.3 section 2.4. -->
<!-- REUSE-IgnoreStart -->

```rust
use comply::header;

let source = "// SPDX-FileCopyrightText: 2026 Acme Inc\n\
              // SPDX-License-Identifier: MIT\n";
let found = header::detect(source).expect("both tags are present");

assert_eq!(found.license_expression.as_deref(), Some("MIT"));
assert_eq!(found.copyrights, ["2026 Acme Inc"]);
```

<!-- REUSE-IgnoreEnd -->

Check an identifier against the bundled license list:

```rust
let db = comply::license::standard_db();

assert!(db.is_known("Apache-2.0"));
assert!(!db.is_known("Definitely-Not-A-License"));
```

Walk a project and build a report:

```rust,no_run
use comply::config::Config;
use comply::{file, report::Report};

let root = std::path::Path::new("/path/to/project");
let config = Config::builder().build();
let mut builder = Report::builder();

for entry in file::walk(root, &config.ignore, &config.source_patterns) {
    builder = builder.add_ok(entry?.path.display().to_string());
}

let report = builder.build();
println!("{} files in scope", report.iter().count());
# Ok::<(), comply::error::Error>(())
```

## Sharing the covered-path definition

`scope` is public so a sibling tool can agree with comply about what counts as
a project file without copying the lists -- copies drift, and a stale copy
means a file in a virtualenv gets asked for an SPDX header:

```rust
use comply::scope::{self, IGNORED_DIRS};

assert!(IGNORED_DIRS.contains(&".git"));
assert!(scope::is_pruned_dir("target"));
assert!(!scope::is_pruned_dir("src"));
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `license-db-bundled` | yes | Bundle the SPDX License List (727+ identifiers) |
| `license-db-network` | no | Fetch the license list at runtime (not yet implemented) |

## License

MIT OR Apache-2.0
