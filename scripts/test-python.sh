#!/usr/bin/env bash
# Run all Python tests
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Running Python Tests"
echo "===================="

cd "$PROJECT_ROOT"

# Use uv to run pytest (handles virtual environment automatically)
# This is consistent with how the project already runs Python scripts
if command -v uv &> /dev/null; then
    uv run --with pytest pytest tests/python/ -v --tb=short
else
    # Fallback: use python -m pytest to ensure correct environment
    if python3 -c "import pytest" &> /dev/null; then
        python3 -m pytest tests/python/ -v --tb=short
    else
        echo "Creating virtual environment..."
        python3 -m venv .venv
        # shellcheck disable=SC1091  # .venv created at runtime
        source .venv/bin/activate
        pip install pytest
        python3 -m pytest tests/python/ -v --tb=short
    fi
fi
