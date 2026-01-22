#!/usr/bin/env bash
# Test version output
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=tests/bash/helpers.sh
source "$SCRIPT_DIR/helpers.sh"

echo "=== Version Tests ==="

# Test --version flag
output=$("$SV_SCRIPT" --version 2>&1 || true)
assert_contains "$output" "version" "--version outputs version string"
assert_contains "$output" "1." "--version contains major version"

# Test version format (X.Y.Z)
version_pattern='[0-9]+\.[0-9]+\.[0-9]+'
if [[ "$output" =~ $version_pattern ]]; then
    assert_equals "1" "1" "Version follows semantic versioning format"
else
    assert_equals "X.Y.Z format" "$output" "Version follows semantic versioning format"
fi

print_summary
