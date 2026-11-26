#!/bin/bash
# The sv2 guest launcher: the first thing a session runs.
#
# It runs as the sandbox user, under the sandbox profile, with the fixed
# environment the helper sets plus what sv2 forwards (TERM, locale). sv2
# holds this file's text and passes it on the command line:
#
#   /bin/bash -c "$(this file)" sv2-launch DIR KIND [ARGS...]
#
# Nothing of it is stored in the sandbox home, so there is no copy there to
# go stale or to be changed by one session for the next.
#
#   DIR    where the session starts
#   KIND   shell: a login zsh, or ARGS as a command in a login zsh's
#          environment
#
# It must run on /bin/bash 3.2. ARGS are never parsed as shell text.
set -euo pipefail

warn() {
    echo >&2 "sv2: $*"
}

if [[ $# -lt 2 ]]; then
    warn "launcher: expected DIR KIND [ARGS...]"
    exit 2
fi
DIR="$1"
KIND="$2"
shift 2

###############################################################################
# PATH: the user's own bin directories, then Homebrew, then the system.
###############################################################################
BREW=""
for candidate in /opt/homebrew /usr/local; do
    if [[ -x "$candidate/bin/brew" ]]; then
        BREW="$candidate"
        break
    fi
done
PATH="$HOME/.local/bin:$HOME/bin${BREW:+:$BREW/bin:$BREW/sbin}:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
export PATH

###############################################################################
# Shell startup files, written once.
#
# A login zsh runs /etc/zprofile, whose path_helper puts the system
# directories first again, so the order above has to be restored from a file
# zsh reads after that. Each file is written only when nothing has that name,
# and is the user's from then on: sv2 never rewrites it, and a later change
# to the text below does not reach a home that already has the file.
###############################################################################
# seed NAME: write standard input to ~/NAME unless something is there.
seed() {
    local file="$HOME/$1"
    if [[ -e "$file" || -L "$file" ]]; then
        return 0
    fi
    # noclobber: lose the race to another session rather than overwrite.
    (set -o noclobber && /bin/cat > "$file") 2>/dev/null \
        || warn "could not write $file"
}

seed .zprofile <<'ZPROFILE'
# Written once by sandvault2 because this file did not exist. It is yours:
# sandvault2 will not change it again.

# Homebrew, then your own bin directories, ahead of the system's. (macOS
# puts the system directories first in every login shell.)
for _sv2_dir in /opt/homebrew /usr/local; do
    if [[ -x "$_sv2_dir/bin/brew" ]]; then
        eval "$("$_sv2_dir/bin/brew" shellenv)"
        break
    fi
done
unset _sv2_dir
path=("$HOME/.local/bin" "$HOME/bin" $path)
typeset -U path
export PATH
ZPROFILE

# An empty one is enough to keep zsh from offering to write it.
seed .zshrc <<'ZSHRC'
ZSHRC

###############################################################################
# Start
###############################################################################
# The helper has already put the session in the workspace or the home.
cd -- "$DIR" 2>/dev/null || true

case "$KIND" in
    shell)
        if [[ $# -eq 0 ]]; then
            # Interactive on a terminal; with piped input it reads commands
            # from the pipe.
            exec /bin/zsh -l
        fi
        # The login shell reads the startup files once, for the environment,
        # and becomes the command.
        # shellcheck disable=SC2016 # expanded by the zsh this starts
        exec /bin/zsh -lc 'exec -- "$@"' sv2 "$@"
        ;;
    *)
        warn "launcher: unknown kind '$KIND'"
        exit 2
        ;;
esac
