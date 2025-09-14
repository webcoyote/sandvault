#!/usr/bin/env bash
# Build a sandbox user ("sandvault") for running commands
set -Eeuo pipefail
trap 'echo "${BASH_SOURCE[0]}: line $LINENO: $BASH_COMMAND: exitcode $?"' ERR


###############################################################################
# Functions
###############################################################################
trace () {
    [[ "${VERBOSE_LEVEL:-0}" -lt 2 ]] || echo >&2 -e "🔬 \033[90m$*\033[0m"
}
debug () {
    [[ "${VERBOSE_LEVEL:-0}" -lt 1 ]] || echo >&2 -e "🔍 \033[36m$*\033[0m"
}
info () {
    echo >&2 -e "ℹ️ \033[36m$*\033[0m"
}
warn () {
    echo >&2 -e "⚠️ \033[33m$*\033[0m"
}
error () {
    echo >&2 -e "❌ \033[31m$*\033[0m"
}
abort () {
    error "$*"
    exit 1
}


###############################################################################
# Create keychain to avoid error dialog
# error message: "a keychain cannot be found to store ..."
#
# This occurs when connecting using "sudo --user=sandvault ..."
# but not when using "ssh sandvault@..."
#
# TODO: I guess the sv script could call this configure script directly
# with the password the first time instead of it being called from .zshrc
###############################################################################
# Explicitly specify the path to avoid confusion with the host user;
# - security dump-keychain                 => host user
# - security dump-keychain $LOGIN_KEYCHAIN => sandvault user
LOGIN_KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
if [[ ! -f "$LOGIN_KEYCHAIN" ]]; then
    warn "Creating keychain without password"
    security create-keychain -p '' "$LOGIN_KEYCHAIN"
fi
security unlock-keychain -p '' "$LOGIN_KEYCHAIN"


###############################################################################
# Install claude
###############################################################################
if [[ ! -x "$HOME/node_modules/.bin/claude" ]]; then
    cd "$HOME"
    npm install --silent @anthropic-ai/claude-code@latest
fi
