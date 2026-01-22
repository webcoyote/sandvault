#!/usr/bin/env bash
# Run all Python tests
set -Eeuo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TESTS_DIR="$PROJECT_ROOT/tests"

echo "Running Python Tests"
echo "===================="

# Run from tests directory so pytest finds pytest.ini
cd "$TESTS_DIR"

# Set PYTHONPATH to include the hooks directory for imports
export PYTHONPATH="$TESTS_DIR/hooks${PYTHONPATH:+:$PYTHONPATH}"

# Keep cache and venv inside tests directory
export PYTEST_CACHE_DIR="$TESTS_DIR/.pytest_cache"
export PYTHONPYCACHEPREFIX="$TESTS_DIR/.pycache"

# Use uv to run pytest (handles virtual environment automatically)
# This is consistent with how the project already runs Python scripts
if command -v uv &> /dev/null; then
    uv run --with pytest --directory "$TESTS_DIR" pytest python/ -v --tb=short
else
    # Fallback: use python -m pytest directly
    # In CI, pytest should be pre-installed via the workflow
    # Locally, user needs to have pytest installed or use uv
    python -m pytest python/ -v --tb=short
fi
