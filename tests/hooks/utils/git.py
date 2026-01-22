#!/usr/bin/env python3
"""
Shared Git utilities for Claude hooks.
Provides common Git operations and information retrieval.
"""

import subprocess
from typing import Optional, Tuple


def get_git_branch() -> Optional[str]:
    """
    Get the current git branch name.
    
    Returns:
        Branch name or None if not in a git repository
    """
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            capture_output=True,
            text=True,
            timeout=2
        )
        if result.returncode == 0:
            return result.stdout.strip()
    except (subprocess.TimeoutExpired, subprocess.SubprocessError, FileNotFoundError):
        pass
    return None


def get_git_status() -> Tuple[Optional[str], Optional[int]]:
    """
    Get current git status information.
    
    Returns:
        Tuple of (branch_name, uncommitted_changes_count)
    """
    try:
        # Get current branch
        branch = get_git_branch()
        
        # Get uncommitted changes count
        status_result = subprocess.run(
            ['git', 'status', '--porcelain'],
            capture_output=True,
            text=True,
            timeout=5
        )
        if status_result.returncode == 0:
            changes = status_result.stdout.strip().split('\n') if status_result.stdout.strip() else []
            uncommitted_count = len(changes)
        else:
            uncommitted_count = None
        
        return branch, uncommitted_count
    except Exception:
        return None, None


def get_recent_github_issues(limit: int = 5) -> Optional[str]:
    """
    Get recent GitHub issues if gh CLI is available.
    
    Args:
        limit: Maximum number of issues to retrieve
    
    Returns:
        Formatted string of recent issues or None
    """
    try:
        # Check if gh is available
        gh_check = subprocess.run(['which', 'gh'], capture_output=True)
        if gh_check.returncode != 0:
            return None
        
        # Get recent open issues
        result = subprocess.run(
            ['gh', 'issue', 'list', '--limit', str(limit), '--state', 'open'],
            capture_output=True,
            text=True,
            timeout=10
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip()
    except Exception:
        pass
    return None
