# Load user configuration
if [[ -n "${SHARED_WORKSPACE:-}" && -f "$SHARED_WORKSPACE/user/.zlogin" ]]; then
    source "$SHARED_WORKSPACE/user/.zlogin"
fi
