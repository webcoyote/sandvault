# Ensure current directory is readable
if [[ -r "${INITIAL_DIR:-}" ]]; then
    cd "$INITIAL_DIR"
elif [[ -r "${SHARED_WORKSPACE:-}" ]]; then
    cd "$SHARED_WORKSPACE"
elif [[ ! -r "$PWD" ]]; then
    cd "$HOME"
fi

# Add sandvault and user bin directories
[[ -d "$HOME/bin" ]] && path=("$HOME/bin" $path)
[[ -d "$HOME/.local/bin" ]] && path=("$HOME/.local/bin" $path)
[[ -d "$HOME/user/bin" ]] && path=("$HOME/user/bin" $path)
[[ -d "$HOME/user/.local/bin" ]] && path=("$HOME/user/.local/bin" $path)
export PATH

# Load user configuration
[[ -f "$HOME/user/.zprofile" ]] && source "$HOME/user/.zprofile"
