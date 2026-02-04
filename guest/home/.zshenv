# Perform sandvault setup once per session
if [[ -n "${SV_SESSION_ID:-}" ]]; then
    sv_session_lock="/tmp/sandvault-configure-$SV_SESSION_ID"
    trap 'rm -f "/tmp/sandvault-configure-$SV_SESSION_ID" 2>/dev/null || true' EXIT
    if [[ ! -e "$sv_session_lock" ]]; then
        : > "$sv_session_lock"

        #echo "CONFIGURING"
        "$HOME/configure"
    fi
fi


# Load user configuration
[[ -f "$HOME/user/.zshenv" ]] && source "$HOME/user/.zshenv"
