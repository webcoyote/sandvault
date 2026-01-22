#!/usr/bin/env bash
# Run all linters
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Running All Linters"
echo "==================="
echo ""

FAILED=0

if ! "$SCRIPT_DIR/lint-bash.sh"; then
    FAILED=1
fi

echo ""

if ! "$SCRIPT_DIR/lint-python.sh"; then
    FAILED=1
fi

echo ""
echo "==================="
if [[ $FAILED -eq 0 ]]; then
    echo "All linting passed!"
    exit 0
else
    echo "Some linting failed!"
    exit 1
fi
