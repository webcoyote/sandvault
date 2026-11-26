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
#          agent NAME NATIVE SCRIPT N PATH1..PATHN [ARGS...]: the first of
#          the N paths that is a program, with ARGS. When none is and NATIVE
#          is 1, SCRIPT (the text of the agent's installer) is run first.
#          probe N PATH1..PATHN: exit 0 when one of the paths is a program,
#          1 when none is. Starts nothing.
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
# Deploy keys: when "sv2 deploy-key" has made any, git uses the ssh that
# picks the right one per repository. Set here, in the environment, and not
# in ~/.gitconfig, which is the user's. (It takes precedence over a
# core.sshCommand set there.)
###############################################################################
if [[ -x "$HOME/.ssh/sv2-deploy-keys/ssh" ]]; then
    GIT_SSH_COMMAND="$HOME/.ssh/sv2-deploy-keys/ssh"
    export GIT_SSH_COMMAND
fi

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
# Agents
###############################################################################
# take_paths N PATH...: move the N paths into PATHS. Sets TAKEN to the number
# of arguments used, for the caller to shift.
PATHS=()
TAKEN=0
take_paths() {
    local n="${1:-}"
    if [[ ! "$n" =~ ^[1-9]$ ]] || [[ $# -le "$n" ]]; then
        warn "launcher: bad list of paths"
        exit 2
    fi
    PATHS=("${@:2:n}")
    TAKEN=$((n + 1))
}

# Set PROGRAM to the first of PATHS that is a program, or to nothing. Paths
# are used as given and never looked up: a name on PATH could be anything.
PROGRAM=""
find_program() {
    local candidate
    PROGRAM=""
    for candidate in "${PATHS[@]}"; do
        if [[ "$candidate" == /* && -f "$candidate" && -x "$candidate" ]]; then
            PROGRAM="$candidate"
            return 0
        fi
    done
}

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
    probe)
        take_paths "$@"
        find_program
        if [[ -z "$PROGRAM" ]]; then
            exit 1
        fi
        exit 0
        ;;
    agent)
        if [[ $# -lt 5 ]]; then
            warn "launcher: agent needs NAME NATIVE SCRIPT N PATH..."
            exit 2
        fi
        NAME="$1"
        NATIVE="$2"
        SCRIPT="$3"
        shift 3
        take_paths "$@"
        shift "$TAKEN"
        find_program
        if [[ -z "$PROGRAM" ]]; then
            if [[ "$NATIVE" != "1" || -z "$SCRIPT" ]]; then
                warn "$NAME is not installed in this sandbox; looked for: ${PATHS[*]}"
                exit 127
            fi
            warn "installing $NAME in the sandbox home, with its own installer"
            # The installer's standard input is the terminal: it may ask.
            /bin/bash -c "$SCRIPT" "sv2-install-$NAME" && st=0 || st=$?
            if [[ "$st" -ne 0 ]]; then
                warn "the installer of $NAME failed (exit status $st)"
                exit "$st"
            fi
            find_program
            if [[ -z "$PROGRAM" ]]; then
                warn "the installer of $NAME finished, but none of these exists: ${PATHS[*]}"
                exit 127
            fi
        fi
        warn "running $PROGRAM as ${USER:-the sandbox user}"
        # shellcheck disable=SC2016 # expanded by the zsh this starts
        exec /bin/zsh -lc 'exec -- "$@"' sv2 "$PROGRAM" "$@"
        ;;
    *)
        warn "launcher: unknown kind '$KIND'"
        exit 2
        ;;
esac
