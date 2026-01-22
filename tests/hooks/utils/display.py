#!/usr/bin/env python3
"""
Display menu bar notification in OSX
"""

import os
import sys
import subprocess
from pathlib import Path
from typing import Optional

# Add parent directories to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))
from utils.git import get_git_branch


def display_task_complete(
    suppress_output: bool = True
) -> bool:
    # Get project and branch information
    project_name = os.path.basename(os.getcwd())
    branch = get_git_branch()

    # Prepare subprocess arguments
    args = ["osascript", "-e", f"display notification \"{branch} complete\" with title \"{project_name}\""]

    kwargs = {}
    if suppress_output:
        kwargs["capture_output"] = True

    result = subprocess.run(args, **kwargs)
    return result.returncode == 0


def main():
    """Command line interface for testing."""
    display_task_complete()

if __name__ == "__main__":
    main()
