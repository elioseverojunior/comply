# comply

Core REUSE compliance library -- SPDX parsing, header detection, file
classification, license detection, and REUSE.toml/DEP5 parsing.

## Modules

| Module | Description |
|--------|-------------|
| `spdx` | SPDX expression parser and validator |
| `header` | SPDX header detection, parsing, and generation |
| `license` | License database with bundled SPDX License List 3.28 |
| `file` | File discovery, classification, `.license` companion files |
| `config` | REUSE.toml / DEP5 parsing |
| `report` | Lint reports and Bill of Materials generation |
| `hash` | SHA-256 integrity hashing |
| `error` | Typed error enum using `thiserror` |

## Usage

```rust
use comply::spdx::validate;
use comply::header::{detect, generate};
use comply::license::standard_db;
use comply::config::Config;
use comply::file::walk;

// Validate an SPDX expression
assert!(validate("MIT OR Apache-2.0").is_ok());

// Check a project for REUSE compliance
let report = comply::report::Report::new("/path/to/project")?
    .build()
    .generate()?;
```

## License

MIT OR Apache-2.0
