#!/usr/bin/env bash
# Run Python linters
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Running Python Linters"
echo "======================"

FAILED=0

# Only lint the tests directory for now
# The hooks directory has pre-existing linting issues that need separate cleanup
PYTHON_DIRS=(
    "$PROJECT_ROOT/tests/python"
)

# Use uv to run ruff (handles installation automatically)
if command -v uv &> /dev/null; then
    echo "Running ruff via uv..."
    for dir in "${PYTHON_DIRS[@]}"; do
        if [[ -d "$dir" ]]; then
            echo "  Checking: $dir"
            if ! uv run --with ruff ruff check "$dir" --config "$PROJECT_ROOT/tests/ruff.toml"; then
                FAILED=1
            fi
        fi
    done
elif command -v ruff &> /dev/null; then
    echo "Running ruff..."
    for dir in "${PYTHON_DIRS[@]}"; do
        if [[ -d "$dir" ]]; then
            echo "  Checking: $dir"
            if ! ruff check "$dir" --config "$PROJECT_ROOT/tests/ruff.toml"; then
                FAILED=1
            fi
        fi
    done
else
    echo "Neither uv nor ruff found. Install uv or ruff."
    exit 1
fi

echo ""
if [[ $FAILED -eq 0 ]]; then
    echo "All Python files passed linting!"
    exit 0
else
    echo "Some Python files failed linting!"
    exit 1
fi
