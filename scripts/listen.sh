#!/usr/bin/env bash
# Build a sandbox user ("sandvault") for running commands
set -Eeuo pipefail
trap 'echo "${BASH_SOURCE[0]}: line $LINENO: $BASH_COMMAND: exitcode $?"' ERR
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# MCP Proxy Client Connection Script
# Listens as MCP proxy server through UNIX socket

# Check for required argument
if [[ $# -eq 0 ]]; then
    echo "Usage: ${BASH_SOURCE[0]} <name>"
    echo "  <name> - MCP script name to run (e.g., localmcp)"
    exit 1
fi

NAME="$1"
SOCKET_DIR="/Users/Shared/sandvault-pat"
SOCKET_PATH="${SOCKET_DIR}/.mcp.sock.${NAME}"

# Create directory if it doesn't exist
if [[ ! -d "${SOCKET_DIR}" ]]; then
    mkdir -p "${SOCKET_DIR}"
fi

# Clean up old socket if it exists
if [[ -S "${SOCKET_PATH}" ]]; then
    rm -f "${SOCKET_PATH}"
fi

# Check if mcp-proxy-tool is installed
if ! command -v mcp-proxy-tool &> /dev/null; then
    echo >&2 "Error: mcp-proxy-tool is not installed"
    echo >&2 "Install it first: npm install -g mcp-proxy-tool"
    exit 1
fi

# Function to cleanup on exit
cleanup() {
    if [[ -S "${SOCKET_PATH}" ]]; then
        rm -f "${SOCKET_PATH}"
    fi
}

# Set up trap for cleanup
trap cleanup SIGINT SIGTERM EXIT

# Use socat to create UNIX socket and forward to mcp-proxy-tool
# This allows mcp-proxy-tool to handle the MCP protocol while socat manages the socket
#socat -v UNIX-LISTEN:"${SOCKET_PATH}",fork,reuseaddr EXEC:"mcp-proxy-tool -c npx -a ios-simulator-mcp"

# This version doesn't keep the socket open
#socat -v UNIX-LISTEN:"${SOCKET_PATH}",fork,reuseaddr EXEC:"npx -a ios-simulator-mcp"

#socat -v UNIX-LISTEN:"${SOCKET_PATH}",fork,reuseaddr EXEC:"mcp-proxy-tool -c ./${NAME}"

# This version doesn't keep the socket open
socat -v UNIX-LISTEN:"${SOCKET_PATH}",fork,reuseaddr EXEC:"./${NAME}"
