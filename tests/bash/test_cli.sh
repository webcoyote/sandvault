#!/usr/bin/env bash
# Test CLI argument parsing
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=tests/bash/helpers.sh
source "$SCRIPT_DIR/helpers.sh"

echo "=== CLI Argument Parsing Tests ==="

# Test --help flag
output=$("$SV_SCRIPT" --help 2>&1 || true)
assert_contains "$output" "Usage:" "--help shows usage"
assert_contains "$output" "claude" "--help mentions claude command"
assert_contains "$output" "shell" "--help mentions shell command"
assert_contains "$output" "uninstall" "--help mentions uninstall command"
assert_contains "$output" "build" "--help mentions build command"

# Test -h flag (short form)
output=$("$SV_SCRIPT" -h 2>&1 || true)
assert_contains "$output" "Usage:" "-h shows usage"

# Test command aliases are documented
output=$("$SV_SCRIPT" --help 2>&1)
assert_contains "$output" "cl" "Help shows claude alias"
assert_contains "$output" "codex" "Help shows codex command"
assert_contains "$output" "gemini" "Help shows gemini command"

print_summary
