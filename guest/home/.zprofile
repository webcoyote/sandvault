# Ensure current directory is readable
[[ -r "$PWD" ]] || cd "$HOME"

# Load user configuration
[[ -f "$HOME/user/.zprofile" ]] && source "$HOME/user/.zprofile"
