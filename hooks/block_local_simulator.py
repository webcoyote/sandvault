#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# ///

# The sandvault user should not directly launch Simulator.app or called "xcrun simctl ..."
# because they'll run inside the sandvault account, on the sandvault desktop, and therefore
# not be visible to the user. This hook blocks those commands from running so that Claude
# will be forced to use the ios-simulator-mcp connected through an MCP proxy (via Unix pipe)
# that's running in the logged-in user's account.

import sys
import json
import os

def main():
    # Read the tool call from stdin
    tool_call = json.loads(sys.stdin.read())

    # Only block commands if SANDVAULT environment variable is defined
    if not os.environ.get("SANDVAULT"):
        # Allow all tool calls if SANDVAULT is not defined
        print(json.dumps({"action": "allow"}))
        return

    # Check if it's a Bash tool call
    if tool_call.get("tool") == "Bash":
        command = tool_call.get("parameters", {}).get("command", "")
        commandLower = command.lower()

        # Block any attempt to use "open -a" for Simulator
        if "open -a" in commandLower and "simulator" in commandLower:
            print(json.dumps({
                "action": "block",
                "message": "Use mcp__ios-simulator__open_simulator instead of 'open -a Simulator'"
            }))
            return

        # Block xcrun simctl commands
        if "xcrun simctl" in commandLower:
            print(json.dumps({
                "action": "block",
                "message": "Use mcp__ios-simulator__* instead of xcrun simctl commands"
            }))
            return

    # Allow all other tool calls
    print(json.dumps({"action": "allow"}))

if __name__ == "__main__":
    main()
