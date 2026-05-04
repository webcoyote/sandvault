# Load user configuration
if [[ -n "${SHARED_WORKSPACE:-}" && -f "$SHARED_WORKSPACE/user/.zshenv" ]]; then
    source "$SHARED_WORKSPACE/user/.zshenv"
fi
