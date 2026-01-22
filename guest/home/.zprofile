# Ensure current directory is readable
if [[ -r "${INITIAL_DIR:-}" ]]; then
    cd "$INITIAL_DIR"
elif [[ -r "${SHARED_WORKSPACE:-}" ]]; then
    cd "$SHARED_WORKSPACE"
elif [[ ! -r "$PWD" ]]; then
    cd "$HOME"
fi

# Load user configuration
[[ -f "$HOME/user/.zprofile" ]] && source "$HOME/user/.zprofile"
