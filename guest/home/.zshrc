# Ensure current directory is readable
[[ -r "$PWD" ]] || cd "$HOME"

# Set custom prompt
export PROMPT="%F{magenta}%n %F{blue}%~%f %# "

# Perform sandvault setup
"$HOME/configure"

# sandvault bin directories
export PATH="$HOME/bin:$HOME/.local/bin:$PATH"

# user bin directories
if [[ -d "$HOME/user/.local/bin" ]]; then
    export PATH="$HOME/user/.local/bin:$PATH"
fi
if [[ -d "$HOME/user/bin" ]]; then
    export PATH="$HOME/user/bin:$PATH"
fi

# Load user configuration
[[ -f "$HOME/user/.zshrc" ]] && source "$HOME/user/.zshrc"

# Set directory as requested
if [[ -r "${INITIAL_DIR:-}" ]]; then
    cd "$INITIAL_DIR"
elif [[ -r "${SHARED_WORKSPACE:-}" ]]; then
    cd "$SHARED_WORKSPACE"
fi

# Run specified application
if [[ "${COMMAND:-}" != "" ]]; then
    # Split COMMAND_ARGS on spaces while respecting quotes using zsh's (z) flag
    args=("${(z)COMMAND_ARGS}")
    exec "$COMMAND" "${args[@]}"
fi
