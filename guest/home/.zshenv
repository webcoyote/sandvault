# Perform sandvault setup once per session
if [[ -n "${SV_SESSION_ID:-}" ]]; then
    # Use $SHARED_WORKSPACE/_sandvault/tmp for the lock file so the host user
    # can delete it during session cleanup. /tmp has the sticky bit set, which
    # prevents users from deleting files they don't own.
    sv_session_dir="${SHARED_WORKSPACE:-}/_sandvault/tmp"
    mkdir -p "$sv_session_dir"
    sv_session_lock="$sv_session_dir/sv-session-$SV_SESSION_ID"
    if [[ ! -e "$sv_session_lock" ]]; then
        : > "$sv_session_lock"

        #echo "CONFIGURING"
        "$HOME/configure"
    fi
fi


# Load user configuration
if [[ -n "${SHARED_WORKSPACE:-}" && -f "$SHARED_WORKSPACE/user/.zshenv" ]]; then
    source "$SHARED_WORKSPACE/user/.zshenv"
fi
