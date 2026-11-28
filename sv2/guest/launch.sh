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
#          agent NAME NATIVE SCRIPT WRAPPER N PATH1..PATHN [ARGS...]: the
#          first of the N paths that is a program, with ARGS. When none is
#          and NATIVE is 1, SCRIPT (the text of the agent's installer) is
#          run first. When WRAPPER (the text of the definition's wrapper) is
#          not empty, it runs instead, given the program and ARGS, and is
#          expected to run them.
#          unpack: the tar archive on standard input, into the home. sv2
#          sends the user's own configuration this way.
#          browser NAME PROGRAM KIND...: start the browser PROGRAM (NAME is
#          chrome or lightpanda) in the sandbox, put where it listens in
#          SV_BROWSER_ENDPOINT, and go on as KIND. The browser is stopped
#          when the session's process ends. For chrome, a headless shell
#          of Playwright's in the home is used in place of PROGRAM.
#          ios XCRUN KIND...: the session was started with --ios, so its
#          sandbox profile lets it reach the simulator's services. Create
#          an iOS simulator with XCRUN's simctl, boot it, put its
#          identifier in SV_IOS_SIMULATOR_UDID, and go on as KIND. It is
#          shut down and deleted when the session's process ends. After
#          browser, when both are given.
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
# Browser
###############################################################################
# start_browser NAME PROGRAM: start it headless, listening on a port of its
# own choosing on localhost, and export SV_BROWSER_ENDPOINT. It runs as the
# sandbox user under the sandbox profile like everything else here, so a
# page it loads reaches what the session reaches and no more.
# Set HEADLESS_SHELL to the newest of Playwright's "headless shell" builds
# of Chromium in this home, or to nothing. It is what "npx playwright
# install chromium" leaves there, in a directory named for its revision.
HEADLESS_SHELL=""
find_headless_shell() {
    local candidate revision best=-1
    HEADLESS_SHELL=""
    for candidate in "$HOME"/Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-*/chrome-headless-shell; do
        revision="${candidate#"$HOME"/Library/Caches/ms-playwright/chromium_headless_shell-}"
        revision="${revision%%/*}"
        if [[ "$revision" =~ ^[0-9]{1,9}$ && -f "$candidate" && -x "$candidate" ]] \
            && [[ "$revision" -gt "$best" ]]; then
            best="$revision"
            HEADLESS_SHELL="$candidate"
        fi
    done
}

start_browser() {
    local name="$1" program="$2" dir log pid port="" pattern tries=0 what="$1"
    # Chrome ends when one of its windows closes: it opens real ones even
    # when headless, and a sandbox account has no screen. The headless
    # shell has no windows, so it is used when this home has one.
    if [[ "$name" == "chrome" ]]; then
        find_headless_shell
        # SV2_NO_BROWSER_INSTALL: sv2 passes it on from its own environment,
        # for tests, which must not download a browser.
        if [[ -z "$HEADLESS_SHELL" && -z "${SV2_NO_BROWSER_INSTALL:-}" \
            && -n "$BREW" && -x "$BREW/bin/npx" ]]; then
            # Once for a sandbox home. It runs here, as the sandbox user,
            # like an agent's own installer; what it downloads stays in
            # this home.
            warn "installing Playwright's headless shell of Chromium in the sandbox home (once): npx playwright install --only-shell chromium"
            "$BREW/bin/npx" --yes playwright install --only-shell chromium < /dev/null >&2 \
                || warn "that install failed"
            find_headless_shell
        fi
        if [[ -n "$HEADLESS_SHELL" ]]; then
            program="$HEADLESS_SHELL"
            what="chrome (Playwright's headless shell)"
        elif [[ -f "$program" && -x "$program" ]]; then
            warn "using Google Chrome, which in a sandbox ends when a page's window closes (the last page of a browser context). The build without that limit is Playwright's headless shell, which could not be installed"
        else
            warn "--chrome needs Playwright's headless shell, which could not be installed (it takes Homebrew's node), or Google Chrome in /Applications; neither is there"
            exit 127
        fi
    fi
    if [[ "$program" != /* || ! -f "$program" || ! -x "$program" ]]; then
        warn "the browser $name is not a program: $program"
        exit 127
    fi
    dir="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/sv2-browser.XXXXXX")"
    log="$dir/log"
    case "$name" in
        chrome)
            # --no-sandbox: Chrome cannot set up its own sandbox inside
            # this one, which is what confines it. The --disable-background
            # flags are what Playwright and Puppeteer pass themselves.
            "$program" --headless --no-sandbox --disable-gpu \
                --remote-debugging-port=0 --remote-debugging-address=127.0.0.1 \
                --user-data-dir="$dir/profile" --no-first-run \
                --no-default-browser-check --disable-extensions \
                --disable-background-networking \
                --disable-background-timer-throttling \
                --disable-backgrounding-occluded-windows \
                --disable-renderer-backgrounding \
                > "$log" 2>&1 < /dev/null &
            pattern='s|.*DevTools listening on ws://127\.0\.0\.1:\([0-9]*\)/.*|\1|p'
            ;;
        lightpanda)
            "$program" serve --host 127.0.0.1 --port 0 --log-level info \
                > "$log" 2>&1 < /dev/null &
            pattern='s|.*"server running" address=127\.0\.0\.1:\([0-9]*\).*|\1|p'
            ;;
        *)
            warn "launcher: unknown browser '$name'"
            exit 2
            ;;
    esac
    pid=$!
    # Not a job of this shell: bash would report its death in its own words.
    disown "$pid"
    # Up to 15 seconds for it to say where it listens.
    while [[ "$tries" -lt 150 ]]; do
        port="$(/usr/bin/sed -n "$pattern" "$log" 2>/dev/null | /usr/bin/head -1)"
        if [[ -n "$port" ]] || ! kill -0 "$pid" 2>/dev/null; then
            break
        fi
        /bin/sleep 0.1
        tries=$((tries + 1))
    done
    if [[ -z "$port" ]]; then
        kill "$pid" 2>/dev/null || true
        if [[ -s "$log" ]]; then
            warn "the browser $what did not start; the last it said:"
            /usr/bin/tail -5 "$log" >&2 || true
        else
            warn "the browser $what did not start, and said nothing"
        fi
        /bin/rm -rf "$dir"
        exit 1
    fi
    # The session's program replaces this shell and keeps its process ID, so
    # a trap set here would never run. Something else has to see it end.
    (
        trap '' HUP
        while kill -0 "$$" 2>/dev/null; do /bin/sleep 1; done
        kill "$pid" 2>/dev/null
        /bin/rm -rf "$dir"
    ) > /dev/null 2>&1 < /dev/null &
    SV_BROWSER_ENDPOINT="http://127.0.0.1:$port"
    export SV_BROWSER_ENDPOINT
    warn "$what listens at $SV_BROWSER_ENDPOINT for this session"
}

###############################################################################
# iOS simulator
###############################################################################
# start_ios XCRUN: everything here is the sandbox user's own. The simulator,
# and every app in it, is started by this account's launchd and so runs
# outside the sandbox profile, though inside the account.
start_ios() {
    local xcrun="$1" runtime udid="" kind pid old key prefs
    if [[ "$xcrun" != /* || ! -x "$xcrun" ]]; then
        warn "--ios: $xcrun is not a program"
        exit 127
    fi
    # iosef taps and reads the screen. Once for a sandbox home, like the
    # headless shell. SV2_NO_IOSEF_INSTALL: for tests.
    if [[ ! -x "$HOME/.local/bin/iosef" && -z "${SV2_NO_IOSEF_INSTALL:-}" \
        && -n "$BREW" && -x "$BREW/bin/uv" ]]; then
        warn "installing iosef in the sandbox home (once): uv tool install iosef"
        "$BREW/bin/uv" tool install iosef < /dev/null >&2 \
            || warn "that install failed; without iosef there are no taps and no accessibility tree"
    fi
    # Simulators of sessions that were killed before they deleted theirs.
    while read -r pid old; do
        kill -0 "$pid" 2>/dev/null || "$xcrun" simctl delete "$old" > /dev/null 2>&1 || true
    done < <("$xcrun" simctl list devices 2>/dev/null \
        | /usr/bin/sed -n 's/^ *sv2-ios-\([0-9]*\) (\([0-9A-F-]*\)) .*/\1 \2/p')
    # The newest iOS runtime, and the newest iPhone it takes.
    runtime="$("$xcrun" simctl list runtimes 2>/dev/null \
        | /usr/bin/sed -n 's/^iOS \([0-9]*\)\.\([0-9]*\).* - \(com\.apple\.CoreSimulator\.SimRuntime\.iOS-[0-9-]*\)$/\1 \2 \3/p' \
        | /usr/bin/sort -k1,1n -k2,2n | /usr/bin/tail -1 | /usr/bin/cut -d' ' -f3)"
    if [[ -z "$runtime" ]]; then
        warn "--ios: no iOS simulator runtime was found. Install one in Xcode (Settings, Components)"
        exit 1
    fi
    while read -r _ kind; do
        udid="$("$xcrun" simctl create "sv2-ios-$$" "$kind" "$runtime" 2>/dev/null)" && break
        udid=""
    done < <("$xcrun" simctl list devicetypes 2>/dev/null \
        | /usr/bin/sed -n 's/^iPhone \([0-9][0-9]*\).* (\(com\.apple\.CoreSimulator\.SimDeviceType\.[A-Za-z0-9-]*\))$/\1 \2/p' \
        | /usr/bin/sort -k1,1nr -k2,2r)
    if [[ ! "$udid" =~ ^[0-9A-F-]{36}$ ]]; then
        warn "--ios: could not create a simulator for $runtime"
        exit 1
    fi
    # Accessibility on, in the device's own settings, before it boots.
    # iosef otherwise turns it on at the first describe and restarts a
    # service of the simulator's to do so; an install or a launch made at
    # that moment fails ("server died"), and apps already running need a
    # relaunch.
    prefs="$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Library/Preferences"
    /bin/mkdir -p "$prefs" 2>/dev/null || true
    for key in AccessibilityEnabled ApplicationAccessibilityEnabled; do
        /usr/bin/defaults write "$prefs/com.apple.Accessibility" "$key" -bool true 2>/dev/null || true
    done
    # It boots while the session starts.
    "$xcrun" simctl boot "$udid" >&2 || warn "--ios: the simulator did not start to boot"
    # Wait for the session's process to end, as for a browser.
    (
        trap '' HUP
        while kill -0 "$$" 2>/dev/null; do /bin/sleep 1; done
        "$xcrun" simctl shutdown "$udid"
        "$xcrun" simctl delete "$udid"
    ) > /dev/null 2>&1 < /dev/null &
    SV_IOS_SIMULATOR_UDID="$udid"
    export SV_IOS_SIMULATOR_UDID
    warn "the simulator $udid is booting for this session"
}

###############################################################################
# Start
###############################################################################
# The helper has already put the session in the workspace or the home.
cd -- "$DIR" 2>/dev/null || true

if [[ "$KIND" == "browser" ]]; then
    if [[ $# -lt 3 ]]; then
        warn "launcher: browser needs NAME PROGRAM KIND"
        exit 2
    fi
    start_browser "$1" "$2"
    KIND="$3"
    shift 3
fi

if [[ "$KIND" == "ios" ]]; then
    if [[ $# -lt 2 ]]; then
        warn "launcher: ios needs XCRUN KIND"
        exit 2
    fi
    start_ios "$1"
    KIND="$2"
    shift 2
fi

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
    agent)
        if [[ $# -lt 6 ]]; then
            warn "launcher: agent needs NAME NATIVE SCRIPT WRAPPER N PATH..."
            exit 2
        fi
        NAME="$1"
        NATIVE="$2"
        SCRIPT="$3"
        WRAPPER="$4"
        shift 4
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
        if [[ -n "$WRAPPER" ]]; then
            warn "running $PROGRAM as ${USER:-the sandbox user}, through the wrapper of $NAME"
            # In the login shell's environment, like the program itself.
            # shellcheck disable=SC2016 # expanded by the zsh this starts
            exec /bin/zsh -lc 'exec -- "$@"' sv2 \
                /bin/bash -c "$WRAPPER" "sv2-wrapper-$NAME" "$PROGRAM" "$@"
        fi
        warn "running $PROGRAM as ${USER:-the sandbox user}"
        # shellcheck disable=SC2016 # expanded by the zsh this starts
        exec /bin/zsh -lc 'exec -- "$@"' sv2 "$PROGRAM" "$@"
        ;;
    unpack)
        # No login shell: the startup files may be what is being replaced.
        exec /usr/bin/tar -x -f - -C "$HOME"
        ;;
    *)
        warn "launcher: unknown kind '$KIND'"
        exit 2
        ;;
esac
