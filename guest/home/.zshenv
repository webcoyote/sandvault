# Setup Homebrew PATH
case "$(uname -m)" in
    arm64)
        if [[ -x /opt/homebrew/bin/brew ]]; then
            eval "$(/opt/homebrew/bin/brew shellenv)"
        fi
        ;;
    x86_64)
        if [[ -x /usr/local/bin/brew ]]; then
            eval "$(/usr/local/bin/brew shellenv)"
        fi
        ;;
    *)
        echo >&2 "sv: error: unsupported architecture: $(uname -m)"
        exit 1
        ;;
esac
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
