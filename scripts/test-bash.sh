#!/usr/bin/env bash
# Run all bash tests
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Running Bash Tests"
echo "=================="

FAILED=0

for test_file in "$PROJECT_ROOT/tests/bash"/test_*.sh; do
    if [[ -f "$test_file" ]]; then
        echo ""
        echo "--- Running: $(basename "$test_file") ---"
        if ! bash "$test_file"; then
            FAILED=1
        fi
    fi
done

echo ""
if [[ $FAILED -eq 0 ]]; then
    echo "All bash tests passed!"
    exit 0
else
    echo "Some bash tests failed!"
    exit 1
fi
