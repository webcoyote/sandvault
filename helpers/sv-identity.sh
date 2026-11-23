# shellcheck shell=bash
# Shared identity resolution. Source from a script with set -Eeuo pipefail
# already in effect. Sets HOST_USER, SV_PROFILE, and SV_SUFFIX.
#
# HOST_USER resolution:
#   1. SV_HOST_USER environment variable (set by sv when entering the sandbox)
#   2. Strip the "sandvault-" prefix from $USER (today's logic)
#
# SV_PROFILE: taken from the environment if already set (e.g. inside a
# sandbox where sv passed it via EXTRA_ENV).  On the host side, sv itself
# parses --profile / SV_PROFILE before sourcing this file and sets
# SV_PROFILE before the source, so we just pick it up.
#
# SV_SUFFIX: "" when no profile, "-$SV_PROFILE" otherwise.  Every
# per-sandbox name is built from HOST_USER + SV_SUFFIX.

# shellcheck disable=SC2034 # variables are used by the sourcing script
if [[ -n "${SV_HOST_USER:-}" ]]; then
    HOST_USER="$SV_HOST_USER"
elif [[ "$USER" == sandvault-* ]]; then
    HOST_USER="${USER#sandvault-}"
else
    HOST_USER="$USER"
fi

# SV_PROFILE may already be set by the caller (sv's --profile parse) or
# by the environment (inside a sandbox).  Default to empty.
SV_PROFILE="${SV_PROFILE:-}"

# shellcheck disable=SC2034
if [[ -n "$SV_PROFILE" ]]; then
    SV_SUFFIX="-$SV_PROFILE"
else
    SV_SUFFIX=""
fi
