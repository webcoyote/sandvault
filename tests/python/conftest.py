"""Pytest fixtures for SandVault tests."""
import sys
from pathlib import Path

import pytest

# Add hooks directory to path for imports
HOOKS_DIR = Path(__file__).parent.parent.parent / "guest" / "home" / "user" / ".claude" / "hooks"
sys.path.insert(0, str(HOOKS_DIR))


@pytest.fixture
def hooks_path():
    """Return the path to the hooks directory."""
    return HOOKS_DIR


@pytest.fixture
def temp_log_dir(tmp_path):
    """Create a temporary log directory."""
    log_dir = tmp_path / ".logs" / "claude" / "hooks" / "test-project" / "main"
    log_dir.mkdir(parents=True, exist_ok=True)
    return log_dir
