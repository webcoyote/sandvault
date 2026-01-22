#!/usr/bin/env bash
# Run complete test suite
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Running Complete Test Suite"
echo "==========================="
echo ""

FAILED=0

# Run bash tests
if ! "$SCRIPT_DIR/test-bash.sh"; then
    FAILED=1
fi

echo ""

# Run Python tests
if ! "$SCRIPT_DIR/test-python.sh"; then
    FAILED=1
fi

echo ""
echo "==========================="
if [[ $FAILED -eq 0 ]]; then
    echo "All tests passed!"
    exit 0
else
    echo "Some tests failed!"
    exit 1
fi
