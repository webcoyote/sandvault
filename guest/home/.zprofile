# Ensure current directory is readable
if [[ -r "${INITIAL_DIR:-}" ]]; then
    cd "$INITIAL_DIR"
elif [[ -r "${SHARED_WORKSPACE:-}" ]]; then
    cd "$SHARED_WORKSPACE"
elif [[ ! -r "$PWD" ]]; then
    cd "$HOME"
fi

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

# Add sandvault and user bin directories; user directories take priority
path=("$HOME/user/bin" "$HOME/user/.local/bin" "$HOME/bin" "$HOME/.local/bin" $path)
export PATH

# Load user configuration
[[ -f "$HOME/user/.zprofile" ]] && source "$HOME/user/.zprofile"
