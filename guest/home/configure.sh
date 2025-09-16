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
    debug "Creating keychain without password"
    security create-keychain -p '' "$LOGIN_KEYCHAIN"
fi

# Unlock the keychain to avoid password dialogs
security unlock-keychain -p '' "$LOGIN_KEYCHAIN"


###############################################################################
# Load environment vars
###############################################################################
# shellcheck disable=SC1091 # file does not exist
[[ -f "$HOME/.env" ]] && set -a && source "$HOME/.env" && set +a


###############################################################################
# Install Claude Code configuration
###############################################################################
if [[ -n "${CLAUDE_CONFIG_REPO:-}" ]]; then
    CLAUDE_CONFIG_DIR="$HOME/.claude"

    if [[ -d "$CLAUDE_CONFIG_DIR/.git" ]]; then
        debug "Updating Claude Code configuration from $CLAUDE_CONFIG_REPO"
        cd "$CLAUDE_CONFIG_DIR"
        git pull --quiet
    else
        debug "Cloning Claude Code configuration from $CLAUDE_CONFIG_REPO"
        rm -rf "$CLAUDE_CONFIG_DIR"
        git clone --quiet "$CLAUDE_CONFIG_REPO" "$CLAUDE_CONFIG_DIR"
    fi
fi


###############################################################################
# Install claude
###############################################################################
if [[ ! -x "$HOME/node_modules/.bin/claude" ]]; then
    cd "$HOME"
    npm install --silent @anthropic-ai/claude-code@latest
fi


###############################################################################
# Xcode
###############################################################################
# It's really useful to be able to run XCode inside sandvault, especially with
# worktrees to allow multiple agents to work on the code at the same time.
#
# However, the default configuration for Xcode stores build files (e.g. object
# files) in ~/Library/Developer/Xcode/DerivedData, which is inconvenient because:
#
# - these files are only accessible to sandvault-$USER, not $USER
# - running multiple agents on separate branches causes file-sharing conflicts;
#   who knows what kind of frankenstein-app will get built when two branches
#   are compiled into the same folder.
#
# The settings below build the Xcode application into the current directory,
# e.g. $REPO/.DerivedData
#
# You'll need to account for this in your .gitignore file. Ta!
#
# See https://github.com/webcoyote/sandvault/blob/main/scripts/worktree for a
# simple implementation of worktrees.
defaults write com.apple.dt.Xcode DerivedDataLocationStyle Custom
defaults write com.apple.dt.Xcode IDECustomDerivedDataLocation ".DerivedData"
