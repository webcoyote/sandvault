#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

import json
import os
import sys
import re
from pathlib import Path

# Add utils directory to path for imports
sys.path.insert(0, str(Path(__file__).parent))
from utils.logging import log_to_jsonl

# Check if external drive access is allowed (default: blocked)
ALLOW_EXTERNAL_DRIVES = os.environ.get('SANDVAULT_ALLOW_EXTERNAL_DRIVES', '0') == '1'

def is_dangerous_rm_command(command):
    """
    Comprehensive detection of dangerous rm commands.
    Matches various forms of rm -rf and similar destructive patterns.
    """
    # Normalize command by removing extra spaces and converting to lowercase
    normalized = ' '.join(command.lower().split())
    
    # Pattern 1: Standard rm -rf variations
    patterns = [
        r'\brm\s+.*-[a-z]*r[a-z]*f',  # rm -rf, rm -fr, rm -Rf, etc.
        r'\brm\s+.*-[a-z]*f[a-z]*r',  # rm -fr variations
        r'\brm\s+--recursive\s+--force',  # rm --recursive --force
        r'\brm\s+--force\s+--recursive',  # rm --force --recursive
        r'\brm\s+-r\s+.*-f',  # rm -r ... -f
        r'\brm\s+-f\s+.*-r',  # rm -f ... -r
    ]
    
    # Check for dangerous patterns
    for pattern in patterns:
        if re.search(pattern, normalized):
            return True
    
    # Pattern 2: Check for rm with recursive flag targeting dangerous paths
    dangerous_paths = [
        r'/',           # Root directory
        r'/\*',         # Root with wildcard
        r'~',           # Home directory
        r'~/',          # Home directory path
        r'\$HOME',      # Home environment variable
        r'\.\.',        # Parent directory references
        r'\*',          # Wildcards in general rm -rf context
        r'\.',          # Current directory
        r'\.\s*$',      # Current directory at end of command
    ]
    
    if re.search(r'\brm\s+.*-[a-z]*r', normalized):  # If rm has recursive flag
        for path in dangerous_paths:
            if re.search(path, normalized):
                return True
    
    return False

def is_external_drive_access(tool_name, tool_input):
    """
    Check if any tool is trying to access external drives in /Volumes.
    Returns True if the tool attempts to access /Volumes (excluding system volumes).
    """
    if ALLOW_EXTERNAL_DRIVES:
        return False

    # System volumes to exclude from blocking (these are internal)
    system_volumes = ['/Volumes/Macintosh HD', '/Volumes/Recovery']

    def is_blocked_volume_path(path):
        """Check if path is under /Volumes but not a system volume."""
        if not path:
            return False
        # Normalize the path
        try:
            normalized = os.path.normpath(path)
        except (TypeError, ValueError):
            return False

        # Check if it's under /Volumes
        if not normalized.startswith('/Volumes'):
            return False

        # Allow system volumes
        for sys_vol in system_volumes:
            if normalized == sys_vol or normalized.startswith(sys_vol + '/'):
                return False

        return True

    if tool_name in ['Read', 'Edit', 'MultiEdit', 'Write', 'Glob', 'Grep']:
        # Check file_path parameter
        file_path = tool_input.get('file_path', '')
        if is_blocked_volume_path(file_path):
            return True
        # Check path parameter (used by Glob/Grep)
        path = tool_input.get('path', '')
        if is_blocked_volume_path(path):
            return True

    elif tool_name == 'Bash':
        command = tool_input.get('command', '')
        # Check for /Volumes references in bash commands
        # Look for /Volumes/something but not /Volumes/Macintosh\ HD or /Volumes/Recovery
        volumes_pattern = r'/Volumes/(?!Macintosh[\\ ]HD|Recovery)[\w\-\.\s]+'
        if re.search(volumes_pattern, command):
            return True

    return False


def is_env_file_access(tool_name, tool_input):
    """
    Check if any tool is trying to access .env files containing sensitive data.
    """
    if tool_name in ['Read', 'Edit', 'MultiEdit', 'Write', 'Bash']:
        # Check file paths for file-based tools
        if tool_name in ['Read', 'Edit', 'MultiEdit', 'Write']:
            file_path = tool_input.get('file_path', '')
            if '.env' in file_path and not file_path.endswith('.env.sample'):
                return True
        
        # Check bash commands for .env file access
        elif tool_name == 'Bash':
            command = tool_input.get('command', '')
            # Pattern to detect .env file access (but allow .env.sample)
            env_patterns = [
                r'\b\.env\b(?!\.sample)',  # .env but not .env.sample
                r'cat\s+.*\.env\b(?!\.sample)',  # cat .env
                r'echo\s+.*>\s*\.env\b(?!\.sample)',  # echo > .env
                r'touch\s+.*\.env\b(?!\.sample)',  # touch .env
                r'cp\s+.*\.env\b(?!\.sample)',  # cp .env
                r'mv\s+.*\.env\b(?!\.sample)',  # mv .env
            ]
            
            for pattern in env_patterns:
                if re.search(pattern, command):
                    return True
    
    return False

def main():
    try:
        # Read JSON input from stdin
        input_data = json.load(sys.stdin)
        
        tool_name = input_data.get('tool_name', '')
        tool_input = input_data.get('tool_input', {})
        
        # Check for external drive access (blocks access to /Volumes by default)
        if is_external_drive_access(tool_name, tool_input):
            print("BLOCKED: Access to external drives in /Volumes is not allowed", file=sys.stderr)
            print("Set SANDVAULT_ALLOW_EXTERNAL_DRIVES=1 to enable access", file=sys.stderr)
            sys.exit(2)  # Exit code 2 blocks tool call and shows error to Claude

        # Check for .env file access (blocks access to sensitive environment files)
        if is_env_file_access(tool_name, tool_input):
            print("BLOCKED: Access to .env files containing sensitive data is prohibited", file=sys.stderr)
            print("Use .env.sample for template files instead", file=sys.stderr)
            sys.exit(2)  # Exit code 2 blocks tool call and shows error to Claude

        # Check for dangerous rm -rf commands
        if tool_name == 'Bash':
            command = tool_input.get('command', '')
            
            # Block rm -rf commands with comprehensive pattern matching
            if is_dangerous_rm_command(command):
                print("BLOCKED: Dangerous rm command detected and prevented", file=sys.stderr)
                sys.exit(2)  # Exit code 2 blocks tool call and shows error to Claude
        
        # Log the pre-tool use event using shared utility
        log_to_jsonl(input_data, 'pre_tool_use.jsonl')
        
        sys.exit(0)
        
    except json.JSONDecodeError:
        # Gracefully handle JSON decode errors
        sys.exit(0)
    except Exception:
        # Handle any other errors gracefully
        sys.exit(0)

if __name__ == '__main__':
    main()
