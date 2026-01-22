#!/usr/bin/env python3
"""
Shared logging utilities for Claude hooks.
Provides consistent JSONL logging functionality across all hooks.
"""

import json
import os
import sys
from pathlib import Path
from typing import Any, Dict, Optional

# Add parent directories to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))
from utils.git import get_git_branch

def ensure_log_directory() -> Path:
    """
    Ensure the log directory exists.

    Returns:
        Path object for the log directory
    """

    # Get project and branch information
    project_name = os.path.basename(os.getcwd())
    branch = get_git_branch()
    log_path = Path.home() / ".logs" / "claude" / "hooks" / project_name / branch
    log_path.mkdir(parents=True, exist_ok=True)
    return log_path

def log_to_jsonl(
    data: Dict[str, Any], 
    log_filename: str
) -> None:
    """
    Append data as a single line to a JSONL file.
    
    Args:
        data: Dictionary to log
        log_filename: Name of the log file (e.g., 'user_prompt_submit.jsonl')
    """
    log_path = ensure_log_directory()
    log_file = log_path / log_filename
    
    with open(log_file, 'a') as f:
        json.dump(data, f)
        f.write('\n')

def copy_jsonl_file(
    source_path: str,
    dest_path: str
) -> None:
    """
    Copy a JSONL file to another location.

    Args:
        source_path: Path to the source JSONL file
        dest_path: Path for the destination JSONL file
    """
    log_path = ensure_log_directory()
    dest_file = log_path / dest_path

    with open(source_path, 'r') as src_file:
        content = src_file.read()
    with open(dest_file, 'w') as dest_file:
        dest_file.write(content)
