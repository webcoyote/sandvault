#!/usr/bin/env bash
# Build a sandbox user ("sandvault") for running commands
set -Eeuo pipefail
trap 'echo "${BASH_SOURCE[0]}: line $LINENO: $BASH_COMMAND: exitcode $?"' ERR

# MCP Proxy Client Connection Script
# Connects to MCP proxy server through UNIX socket

# Check for required argument
if [[ $# -eq 0 ]]; then
    echo "Usage: ${BASH_SOURCE[0]} <name>"
    echo "  <name> - MCP server name to connect to"
    exit 1
fi

NAME="$1"
SOCKET_DIR="/Users/Shared/sandvault-pat"
SOCKET_PATH="${SOCKET_DIR}/.mcp.sock.${NAME}"

# Check if socket exists
if [[ ! -S "${SOCKET_PATH}" ]]; then
    echo >&2 "Warning: Socket not found at ${SOCKET_PATH}"
    echo >&2 "Make sure the listener (listen.sh) is running first."
    exit 1
fi

# Check if mcp-proxy-tool is installed
if ! command -v mcp-proxy-tool &> /dev/null; then
    echo >&2 "Error: mcp-proxy-tool is not installed"
    echo >&2 "Install it first: npm install -g mcp-proxy-tool"
    exit 1
fi

exec mcp-proxy-tool -p "${SOCKET_PATH}"
