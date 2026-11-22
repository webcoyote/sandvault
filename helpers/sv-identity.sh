# shellcheck shell=bash
# Shared identity resolution. Source from a script with set -Eeuo pipefail
# already in effect. Sets HOST_USER (and optionally SV_PROFILE later).
#
# Resolution order:
#   1. SV_HOST_USER environment variable (set by sv when entering the sandbox)
#   2. Strip the "sandvault-" prefix from $USER (today's logic)

# shellcheck disable=SC2034 # HOST_USER is used by the sourcing script
if [[ -n "${SV_HOST_USER:-}" ]]; then
    HOST_USER="$SV_HOST_USER"
elif [[ "$USER" == sandvault-* ]]; then
    HOST_USER="${USER#sandvault-}"
else
    HOST_USER="$USER"
fi
