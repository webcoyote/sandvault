#!/usr/bin/env bash
set -Eeuo pipefail

# Launch a command in a new terminal window, dispatching by terminal app.
# Usage: launch-in-terminal.sh "<command string>"
#
# Terminal selection (in priority order):
#   1. $SV_TERMINAL env var, if set (e.g. SV_TERMINAL=iTerm.app)
#   2. Auto-detected parent terminal via find-terminal-app.sh
#   3. Terminal.app fallback
#
# $SV_TERMINAL accepts either the bundle name ("iTerm.app") or a short
# alias ("iterm", "terminal", "ghostty", "wezterm", "kitty", "alacritty",
# "cmux"), case-insensitive.

if [[ $# -ne 1 || -z "$1" ]]; then
    echo "Usage: $0 <command>" >&2
    exit 2
fi

cmd="$1"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

normalize_terminal() {
    # Map a user-supplied terminal name to a canonical bundle name.
    local raw="${1,,}"
    raw="${raw%.app}"
    case "$raw" in
        terminal|apple) echo "Terminal.app" ;;
        iterm|iterm2)   echo "iTerm.app" ;;
        ghostty)        echo "Ghostty.app" ;;
        wezterm)        echo "WezTerm.app" ;;
        kitty)          echo "kitty.app" ;;
        alacritty)      echo "Alacritty.app" ;;
        cmux)           echo "cmux.app" ;;
        *)              echo "$1" ;;
    esac
}

if [[ -n "${SV_TERMINAL:-}" ]]; then
    app="$(normalize_terminal "$SV_TERMINAL")"
else
    app="$("$script_dir/find-terminal-app.sh" 2>/dev/null || true)"
fi

# Run $cmd, then drop the user into a fresh login shell so the window
# stays open after the command exits. Used by terminals that close the
# window on command exit (Ghostty, kitty, Alacritty, WezTerm).
cmd_keep_open="$cmd; exec \"\$SHELL\" -l"

launch_terminal_app() {
    # AppleScript injects keystrokes into a new window. Escape backslashes
    # and double quotes for AppleScript string literal embedding.
    local escaped=${cmd//\\/\\\\}
    escaped=${escaped//\"/\\\"}
    osascript \
        -e 'tell application "Terminal" to activate' \
        -e "tell application \"Terminal\" to do script \"$escaped\""
}

launch_iterm() {
    local escaped=${cmd//\\/\\\\}
    escaped=${escaped//\"/\\\"}
    osascript <<EOF
tell application "iTerm"
    activate
    set newWindow to (create window with default profile)
    tell current session of newWindow
        write text "$escaped"
    end tell
end tell
EOF
}

launch_ghostty() {
    # On macOS, ghostty must be launched via `open -na`. Ghostty composes
    # `login -flp <user> bash --noprofile --norc -c "exec -l <command>"`,
    # so <command> is shell-parsed. Pass our shell explicitly with -lc so
    # the wrapped command survives as a single quoted argument to it.
    local quoted
    quoted=$(printf '%q' "$cmd_keep_open")
    open -na Ghostty.app --args --command="$SHELL -lc $quoted"
}

launch_wezterm() {
    # `wezterm cli spawn` only works when a wezterm-mux server is already
    # running — probe with `cli list` first. If it fails (cold start) or
    # the CLI isn't installed, cold-start the GUI via `open -na`.
    if command -v wezterm >/dev/null 2>&1 && \
       wezterm cli list >/dev/null 2>&1; then
        wezterm cli spawn --new-window -- "$SHELL" -lc "$cmd_keep_open"
    else
        open -na WezTerm --args start -- "$SHELL" -lc "$cmd_keep_open"
    fi
}

launch_kitty() {
    # `kitty @ launch` only works when a kitty instance is already running
    # with allow_remote_control = yes. Without one, it hangs trying to
    # connect to a missing socket. Skip the fast path entirely unless
    # kitty is running — the cold-start `open -na` path is reliable.
    if command -v kitty >/dev/null 2>&1 && \
       pgrep -x kitty >/dev/null 2>&1 && \
       kitty @ ls >/dev/null 2>&1; then
        kitty @ launch --type=os-window "$SHELL" -lc "$cmd_keep_open"
        return 0
    fi
    open -na kitty --args "$SHELL" -lc "$cmd_keep_open"
}

launch_alacritty() {
    if command -v alacritty >/dev/null 2>&1; then
        alacritty -e "$SHELL" -lc "$cmd_keep_open" &
    else
        open -na Alacritty --args -e "$SHELL" -lc "$cmd_keep_open"
    fi
}

launch_cmux() {
    # cmux opens a new workspace with a starting command. The CLI talks to
    # the running cmux app over its Unix socket, so the app must be open.
    if ! command -v cmux >/dev/null 2>&1; then
        echo "cmux CLI not found on PATH" >&2
        return 1
    fi
    # Best-effort cwd: if the command starts with `sv-clone <path>`, pull
    # that path out so cmux opens the workspace there. Otherwise fall back
    # to $PWD.
    local cwd="$PWD"
    if [[ "$cmd" =~ ^sv-clone[[:space:]]+([^[:space:]]+) ]]; then
        cwd="${BASH_REMATCH[1]}"
    fi
    cmux new-workspace --cwd "$cwd" --command "$cmd"
}

case "${app:-}" in
    Terminal.app)           launch_terminal_app ;;
    iTerm.app|iTerm2.app)   launch_iterm ;;
    Ghostty.app)            launch_ghostty ;;
    WezTerm.app)            launch_wezterm ;;
    kitty.app)              launch_kitty ;;
    Alacritty.app)          launch_alacritty ;;
    cmux.app)               launch_cmux ;;
    *)
        echo "Unknown or undetected terminal '${app:-<none>}', falling back to Terminal.app" >&2
        launch_terminal_app
        ;;
esac
