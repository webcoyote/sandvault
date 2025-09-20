# Ensure current directory is readable
[[ -r "$PWD" ]] || cd "$HOME"

# Load user configuration
[[ -f "$HOME/user/.zshenv" ]] && source "$HOME/user/.zshenv"

# Sentinel to indicate application is running inside a vault
export SANDVAULT=true
