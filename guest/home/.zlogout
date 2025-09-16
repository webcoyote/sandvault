# Ensure current directory is readable
[[ -r "$PWD" ]] || cd "$HOME"

# Load user configuration
[[ -f "$HOME/user/.zlogout" ]] && source "$HOME/user/.zlogout"
