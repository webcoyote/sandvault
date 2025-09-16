# Ensure current directory is readable
[[ -r "$PWD" ]] || cd "$HOME"

# Load user configuration
[[ -f "$HOME/user/.zlogin" ]] && source "$HOME/user/.zlogin"
