#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

import json
import os
import sys
from pathlib import Path

# Add utils directory to path for imports
sys.path.insert(0, str(Path(__file__).parent))
from utils.logging import log_to_jsonl

def main():
    try:
        # Read JSON input from stdin
        input_data = json.load(sys.stdin)
        
        # Log the post-tool use event using shared utility
        log_to_jsonl(input_data, 'post_tool_use.jsonl')
        
        sys.exit(0)
        
    except json.JSONDecodeError:
        # Handle JSON decode errors gracefully
        sys.exit(0)
    except Exception:
        # Exit cleanly on any other error
        sys.exit(0)

if __name__ == '__main__':
    main()
