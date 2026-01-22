#!/usr/bin/env bash
# Test helper functions for bash tests
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# shellcheck disable=SC2034  # Used by test files that source this
SV_SCRIPT="$PROJECT_ROOT/sv"

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

assert_equals() {
    local expected="$1"
    local actual="$2"
    local message="${3:-}"
    ((TESTS_RUN++)) || true
    if [[ "$expected" == "$actual" ]]; then
        ((TESTS_PASSED++)) || true
        echo -e "${GREEN}PASS${NC}: $message"
    else
        ((TESTS_FAILED++)) || true
        echo -e "${RED}FAIL${NC}: $message"
        echo "  Expected: $expected"
        echo "  Actual:   $actual"
    fi
}

assert_contains() {
    local haystack="$1"
    local needle="$2"
    local message="${3:-}"
    ((TESTS_RUN++)) || true
    if [[ "$haystack" == *"$needle"* ]]; then
        ((TESTS_PASSED++)) || true
        echo -e "${GREEN}PASS${NC}: $message"
    else
        ((TESTS_FAILED++)) || true
        echo -e "${RED}FAIL${NC}: $message"
        echo "  String does not contain: $needle"
    fi
}

assert_not_contains() {
    local haystack="$1"
    local needle="$2"
    local message="${3:-}"
    ((TESTS_RUN++)) || true
    if [[ "$haystack" != *"$needle"* ]]; then
        ((TESTS_PASSED++)) || true
        echo -e "${GREEN}PASS${NC}: $message"
    else
        ((TESTS_FAILED++)) || true
        echo -e "${RED}FAIL${NC}: $message"
        echo "  String unexpectedly contains: $needle"
    fi
}

assert_exit_code() {
    local expected="$1"
    local actual="$2"
    local message="${3:-}"
    ((TESTS_RUN++)) || true
    if [[ "$expected" -eq "$actual" ]]; then
        ((TESTS_PASSED++)) || true
        echo -e "${GREEN}PASS${NC}: $message"
    else
        ((TESTS_FAILED++)) || true
        echo -e "${RED}FAIL${NC}: $message"
        echo "  Expected exit code: $expected"
        echo "  Actual exit code:   $actual"
    fi
}

print_summary() {
    echo ""
    echo "================================"
    echo "Tests run: $TESTS_RUN"
    echo -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
    echo -e "Failed: ${RED}$TESTS_FAILED${NC}"
    echo "================================"
    [[ $TESTS_FAILED -eq 0 ]]
}
