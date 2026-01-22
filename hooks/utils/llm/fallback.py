#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

import os
import sys
import random
import re
from pathlib import Path

# Add parent directories to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent.parent))
from utils.git import get_git_branch


def generate_completion_message():
    """
    Generate a fallback completion message using random selection.
    
    Returns:
        str: A randomly selected completion message
    """
    
    # Get project and branch information
    project_name = os.path.basename(os.getcwd())
    branch = get_git_branch()
    
    # Clean up names for better readability
    project_name = re.sub(r'[-_]', ' ', project_name)
    branch = re.sub(r'[-_]', ' ', branch) if branch else ""
    branch_text = f" on branch {branch}" if branch and branch != "main" else ""
    
    # Random completion messages
    messages = [
        f"{project_name}{branch_text} is done!",
        f"{project_name}{branch_text} finished!",
        f"Completed {project_name}{branch_text}!",
        f"{project_name}{branch_text} is ready!",
    ]
    
    return random.choice(messages)


def main():
    """Command line interface for testing."""
    if len(sys.argv) > 1:
        if sys.argv[1] == "--completion":
            message = generate_completion_message()
            print(message)
        else:
            # For general prompts, return nothing (not supported)
            sys.exit(1)
    else:
        print("Usage: ./fallback.py --completion")


if __name__ == "__main__":
    main()
