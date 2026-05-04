# shellcheck shell=bash
# Logging helpers shared by sv and sv-agentsview-setup. Source from a script
# with `set -Eeuo pipefail` already in effect.

[[ "${SV_VERBOSE:-0}" =~ ^[0-9]+$ ]] && SV_VERBOSE="${SV_VERBOSE:-0}" || SV_VERBOSE=1

trace () {
    [[ "$SV_VERBOSE" -lt 2 ]] || echo >&2 -e "🔬 \033[90m$*\033[0m"
}
debug () {
    [[ "$SV_VERBOSE" -lt 1 ]] || echo >&2 -e "🔍 \033[36m$*\033[0m"
}
info () {
    echo >&2 -e "ℹ️  \033[36m$*\033[0m"
}
warn () {
    echo >&2 -e "⚠️  \033[33m$*\033[0m"
}
error () {
    echo >&2 -e "❌ \033[31m$*\033[0m"
}
abort () {
    error "$*"
    exit 1
}
