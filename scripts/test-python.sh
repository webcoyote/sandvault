#!/usr/bin/env bash
# Run all Python tests
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Running Python Tests"
echo "===================="

cd "$PROJECT_ROOT"

# Set PYTHONPATH to include the hooks directory for imports
export PYTHONPATH="$PROJECT_ROOT/guest/home/user/.claude/hooks${PYTHONPATH:+:$PYTHONPATH}"

# Use uv to run pytest (handles virtual environment automatically)
# This is consistent with how the project already runs Python scripts
if command -v uv &> /dev/null; then
    uv run --with pytest pytest tests/python/ -v --tb=short
else
    # Fallback: use python -m pytest to ensure correct environment
    # Use pythonLocation from setup-python action if available, otherwise fall back to PYTHON env var
    if [[ -n "${pythonLocation:-}" ]]; then
        PYTHON_CMD="${pythonLocation}/bin/python"
    else
        PYTHON_CMD="${PYTHON:-python}"
    fi
    if $PYTHON_CMD -c "import pytest" &> /dev/null; then
        $PYTHON_CMD -m pytest tests/python/ -v --tb=short
    else
        echo "Creating virtual environment..."
        $PYTHON_CMD -m venv .venv
        # shellcheck disable=SC1091  # .venv created at runtime
        source .venv/bin/activate
        pip install pytest
        $PYTHON_CMD -m pytest tests/python/ -v --tb=short
    fi
fi
