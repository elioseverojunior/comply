<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

# comply-wasm

Browser bindings for [`comply`](../comply), a REUSE compliance tool written in
Rust.

This crate is a thin adapter layer. It converts JavaScript strings into the
shapes `comply` already accepts and converts the results back -- no compliance
logic lives here. Code compiled for `wasm32-unknown-unknown` never executes
under the host test harness, so logic placed in this crate would be counted by
the coverage gate and never run.

## API

### `validate_spdx(expression) -> ValidationResult`

Validates an SPDX licence expression. Returns `{ valid, error }` rather than
throwing: an invalid expression is an ordinary answer for an editor validating as
the user types, not an exceptional one.

```js
import init, { validate_spdx } from "./comply_wasm.js";

await init();

validate_spdx("MIT OR Apache-2.0"); // { valid: true,  error: undefined }
validate_spdx("MIT OR OR"); // { valid: false, error: "..." }
```

### `annotate_file(filename, source, license, copyright) -> string`

Adds an SPDX header to file content and returns the annotated content. Throws a
string when `license` is not a valid SPDX expression, or when `filename` has no
comment syntax `comply` recognises.

`filename` selects the comment syntax and is required. Content alone cannot
distinguish a Rust file from a shell script, so omitting it yields `#` comments
for `fn main() {}`. In a browser the name comes from `File.name`.

```js
annotate_file("lib.rs", "fn main() {}\n", "MIT", "Someone");
// "// SPDX-FileCopyrightText: Someone\n//\n// SPDX-License-Identifier: MIT\n\nfn main() {}\n"
```

### `parse_reuse_toml(content) -> ParsedConfig`

Parses REUSE.toml content, returning `{ annotation_count }`. Throws a string when
the content is not a valid manifest -- malformed TOML, or a missing `version`
key.

A summary rather than the parsed `Config`: `Config` derives no `Serialize`, and
exposing it would mean adding serde to the core's public types purely to satisfy
a binding.

```js
parse_reuse_toml('version = 1\n\n[[annotations]]\npath = "src/lib.rs"\n...');
// { annotation_count: 1 }
```

## Not implemented

`lint_project` is deferred. Linting discovers files by walking the filesystem,
which a browser does not have. Reaching it needs an in-memory file source
threaded through `comply`'s `scope` and `report` modules -- a change to fully
covered core code, so it belongs on its own rather than inside these bindings.

## Building

```sh
cargo build -p comply-wasm --target wasm32-unknown-unknown
```

The `wasm32-unknown-unknown` target must be installed
(`rustup target add wasm32-unknown-unknown`). `crate-type` is
`["cdylib", "rlib"]`: `cdylib` is what `wasm-bindgen` consumes, and `rlib` is
what lets `cargo test` link the crate on the host.

There is no `wasm-pack` or npm packaging step yet -- `wasm-bindgen-test` needs a
browser runner CI does not currently provision.

## Testing

Tests run natively, against the same functions JavaScript calls:

```sh
cargo nextest run -p comply-wasm
```

## Licence

`MIT OR Apache-2.0`, matching the workspace. See [`LICENSE`](../../LICENSE).
