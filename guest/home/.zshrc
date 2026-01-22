# Load user configuration
[[ -f "$HOME/user/.zshrc" ]] && source "$HOME/user/.zshrc"

# Run specified application
if [[ "${COMMAND:-}" != "" ]]; then
    # Split COMMAND_ARGS on spaces while respecting quotes using zsh's (z) flag
    args=("${(z)COMMAND_ARGS}")
    exec "$COMMAND" "${args[@]}"
fi
