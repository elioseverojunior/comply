#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 COMPLY contributors
#
# SPDX-License-Identifier: MIT OR Apache-2.0

# PostToolUse hook (Write|Edit): keep edited Rust sources formatted,
# SPDX-annotated, and clippy-clean. Registered in .claude/settings.json.
#
# Legs 1 and 2 are silent and non-blocking -- a formatting or header hiccup must
# never interrupt the session. Leg 3 reports clippy failures back to the model as
# injected context; the edit itself already landed, so it does not block either.
set -uo pipefail

payload="$(cat)"
file="$(printf '%s' "$payload" | jq -r '.tool_response.filePath // .tool_input.file_path // empty' 2>/dev/null)"

[ -n "$file" ] || exit 0

case "$file" in
*.rs) ;;
*) exit 0 ;;
esac

[ -f "$file" ] || exit 0
# `comply annotate` writes THROUGH a symlink, which would corrupt the canonical
# license texts in LICENSES/. Never annotate one.
[ -L "$file" ] && exit 0

root="$(git -C "$(dirname "$file")" rev-parse --show-toplevel 2>/dev/null)" || exit 0
[ -n "$root" ] || exit 0
cd "$root" || exit 0

# `comply annotate` records into REUSE.toml whatever path string it is HANDED.
# The hook payload carries an absolute path, which would be written verbatim and
# could never match on another machine or in CI. Always pass repo-relative.
rel="${file#"$root"/}"

# Leg 1: format only the edited file. Standalone rustfmt defaults to edition
# 2015, which mis-parses this workspace's 2024 sources -- pass the edition.
rustfmt --edition 2024 "$rel" >/dev/null 2>&1 || true

# Leg 2: SPDX header. Idempotent -- a file that already carries the canonical
# 2-line block comes out byte-identical.
mise run comply:fix "$rel" >/dev/null 2>&1 || true

# Leg 3: workspace lint gate, matching `mise run clippy`.
if ! out="$(CARGO_TERM_COLOR=never cargo clippy --all-targets --all-features --message-format short -- -D warnings 2>&1)"; then
    msg="$(printf '%s\n' "$out" | grep -E 'error|warning' | head -40 | head -c 4000)"
    [ -n "$msg" ] || msg="$(printf '%s\n' "$out" | tail -40 | head -c 4000)"
    ctx="cargo clippy failed after editing ${file} (workspace is warnings-as-errors):
${msg}"
    jq -n --arg ctx "$ctx" '{
        systemMessage: "clippy failed after a Rust edit -- lint output was sent to Claude",
        hookSpecificOutput: {
            hookEventName: "PostToolUse",
            additionalContext: $ctx
        }
    }'
fi

exit 0
