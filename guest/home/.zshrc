# Load user configuration
if [[ -n "${SHARED_WORKSPACE:-}" && -f "$SHARED_WORKSPACE/user/.zshrc" ]]; then
    source "$SHARED_WORKSPACE/user/.zshrc"
fi
