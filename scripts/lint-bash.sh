#!/usr/bin/env bash
# Run shellcheck on all bash files
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Running ShellCheck"
echo "=================="

# Check if shellcheck is available
if ! command -v shellcheck &> /dev/null; then
    echo "shellcheck not found. Install with: brew install shellcheck"
    exit 1
fi

FAILED=0
SHELLCHECK_OPTS="--rcfile=$PROJECT_ROOT/tests/.shellcheckrc"

# Lint main sv script
echo "Checking: sv"
if ! shellcheck "$SHELLCHECK_OPTS" "$PROJECT_ROOT/sv"; then
    FAILED=1
fi

# Lint scripts directory (only bash scripts)
for script in "$PROJECT_ROOT/scripts"/*; do
    if [[ -f "$script" ]] && head -1 "$script" | grep -q "bash"; then
        echo "Checking: scripts/$(basename "$script")"
        if ! shellcheck "$SHELLCHECK_OPTS" "$script"; then
            FAILED=1
        fi
    fi
done

# Lint guest home bin scripts
for script in "$PROJECT_ROOT/guest/home/bin"/*; do
    if [[ -f "$script" ]] && head -1 "$script" | grep -q "bash"; then
        echo "Checking: guest/home/bin/$(basename "$script")"
        if ! shellcheck "$SHELLCHECK_OPTS" "$script"; then
            FAILED=1
        fi
    fi
done

# Lint test scripts
for script in "$PROJECT_ROOT/tests/bash"/*.sh; do
    if [[ -f "$script" ]]; then
        echo "Checking: tests/bash/$(basename "$script")"
        if ! shellcheck "$SHELLCHECK_OPTS" "$script"; then
            FAILED=1
        fi
    fi
done

echo ""
if [[ $FAILED -eq 0 ]]; then
    echo "All shell scripts passed shellcheck!"
    exit 0
else
    echo "Some scripts failed shellcheck!"
    exit 1
fi
