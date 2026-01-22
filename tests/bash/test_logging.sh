#!/usr/bin/env bash
# Test logging functions
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=tests/bash/helpers.sh
source "$SCRIPT_DIR/helpers.sh"

echo "=== Logging Function Tests ==="

# Create a temporary file to source functions
TEMP_FUNCS=$(mktemp)
trap 'rm -f "$TEMP_FUNCS"' EXIT

# Extract logging functions from sv script (lines 12-31)
sed -n '12,31p' "$SV_SCRIPT" > "$TEMP_FUNCS"
# shellcheck disable=SC1090  # Dynamic source of extracted functions
source "$TEMP_FUNCS"

# Test info function (always outputs)
VERBOSE=0
output=$(info "test message" 2>&1)
assert_contains "$output" "test message" "info outputs message"

# Test warn function
output=$(warn "warning message" 2>&1)
assert_contains "$output" "warning message" "warn outputs message"

# Test error function
output=$(error "error message" 2>&1)
assert_contains "$output" "error message" "error outputs message"

# Test debug function with VERBOSE=0 (should not output)
VERBOSE=0
output=$(debug "debug message" 2>&1)
assert_equals "" "$output" "debug with VERBOSE=0 produces no output"

# Test debug function with VERBOSE=1 (should output)
VERBOSE=1
output=$(debug "debug message" 2>&1)
assert_contains "$output" "debug message" "debug with VERBOSE=1 outputs message"

# Test trace function with VERBOSE=1 (should not output)
VERBOSE=1
output=$(trace "trace message" 2>&1)
assert_equals "" "$output" "trace with VERBOSE=1 produces no output"

# Test trace function with VERBOSE=2 (should output)
# shellcheck disable=SC2034  # VERBOSE is used by the sourced trace function
VERBOSE=2
output=$(trace "trace message" 2>&1)
assert_contains "$output" "trace message" "trace with VERBOSE=2 outputs message"

print_summary
