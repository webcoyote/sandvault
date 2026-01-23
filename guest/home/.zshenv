# Add Homebrew bin directories
[[ -d "/opt/homebrew/bin" ]] && path=("/opt/homebrew/bin" $path)
[[ -d "/opt/homebrew/sbin" ]] && path=("/opt/homebrew/sbin" $path)
export PATH

# Perform sandvault setup once per session
if [[ -n "${SV_SESSION_ID:-}" ]]; then
    sv_session_lock="/tmp/sandvault-configure-$SV_SESSION_ID"
    if [[ ! -e "$sv_session_lock" ]]; then
        : > "$sv_session_lock"

        #echo "CONFIGURING"
        "$HOME/configure"
    fi
fi


# Load user configuration
[[ -f "$HOME/user/.zshenv" ]] && source "$HOME/user/.zshenv"
