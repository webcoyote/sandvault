#!/usr/bin/env bash
# Run all Python tests
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Running Python Tests"
echo "===================="

cd "$PROJECT_ROOT"

# Set PYTHONPATH to include the hooks directory for imports
export PYTHONPATH="$PROJECT_ROOT/hooks${PYTHONPATH:+:$PYTHONPATH}"

# Use uv to run pytest (handles virtual environment automatically)
# This is consistent with how the project already runs Python scripts
if command -v uv &> /dev/null; then
    uv run --with pytest pytest tests/python/ -v --tb=short
else
    # Fallback: use python -m pytest directly
    # In CI, pytest should be pre-installed via the workflow
    # Locally, user needs to have pytest installed or use uv
    python -m pytest tests/python/ -v --tb=short
fi
