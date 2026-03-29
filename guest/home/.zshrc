# Load user configuration
if [[ -n "${SHARED_WORKSPACE:-}" && -f "$SHARED_WORKSPACE/user/.zshrc" ]]; then
    source "$SHARED_WORKSPACE/user/.zshrc"
fi

# Run specified application
if [[ "${COMMAND:-}" != "" ]]; then
    # Split COMMAND_ARGS on spaces while respecting quotes using zsh's (z) flag
    args=("${(z)COMMAND_ARGS}")
    exec "$COMMAND" "${args[@]}"
fi
