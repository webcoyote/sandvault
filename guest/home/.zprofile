# Ensure current directory is readable
if [[ -r "${INITIAL_DIR:-}" ]]; then
    cd "$INITIAL_DIR"
elif [[ -r "${SHARED_WORKSPACE:-}" ]]; then
    cd "$SHARED_WORKSPACE"
elif [[ ! -r "$PWD" ]]; then
    cd "$HOME"
fi

# Load user configuration
if [[ -n "${SHARED_WORKSPACE:-}" && -f "$SHARED_WORKSPACE/user/.zprofile" ]]; then
    source "$SHARED_WORKSPACE/user/.zprofile"
fi

# Setup Homebrew PATH when user configuration has not already done so
if [[ -z "${HOMEBREW_PREFIX:-}" || "${PATH%%:"${HOMEBREW_PREFIX}"/sbin*}" != "${HOMEBREW_PREFIX}/bin" ]]; then
    case "$(uname -m)" in
        arm64)
            [[ -x /opt/homebrew/bin/brew ]] && eval "$(/opt/homebrew/bin/brew shellenv)"
            ;;
        x86_64)
            [[ -x /usr/local/bin/brew ]] && eval "$(/usr/local/bin/brew shellenv)"
            ;;
        *)
            echo >&2 "sv: error: unsupported architecture: $(uname -m)"
            exit 1
            ;;
    esac
fi

# Add sandvault and user bin directories; user directories take priority
for dir in "$HOME/.local/bin" "$HOME/bin" "$SHARED_WORKSPACE/user/.local/bin" "$SHARED_WORKSPACE/user/bin"; do
    [[ -d "$dir" ]] && path=("$dir" ${path:#$dir})
done
export PATH
