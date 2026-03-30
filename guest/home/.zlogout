# Load user configuration
if [[ -n "${SHARED_WORKSPACE:-}" && -f "$SHARED_WORKSPACE/user/.zlogout" ]]; then
    source "$SHARED_WORKSPACE/user/.zlogout"
fi
