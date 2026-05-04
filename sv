#!/bin/bash
# Build a sandbox user ("sandvault") for running commands
set -Eeuo pipefail
trap 'echo "${BASH_SOURCE[0]}: line $LINENO: $BASH_COMMAND: exitcode $?"' ERR

# perform "readlink -f", which is not supported in macOS system bash
SOURCE="${BASH_SOURCE[0]}"
while [[ -L "$SOURCE" ]]; do
    SOURCE_DIR="$(cd -P "$(dirname "$SOURCE")" && pwd -P)"
    SOURCE="$(readlink "$SOURCE")"
    [[ "$SOURCE" = /* ]] || SOURCE="$SOURCE_DIR/$SOURCE"
done
WORKSPACE="$(cd -P "$(dirname "$SOURCE")" && pwd -P)"

# If running from a Homebrew Cellar (e.g. /opt/homebrew/Cellar/sandvault/1.2.3),
# use the stable opt/ symlink instead so generated scripts don't break on upgrade.
if [[ "$WORKSPACE" =~ ^(.*)/homebrew/Cellar/([^/]+)/[^/]+$ ]]; then
    WORKSPACE="${BASH_REMATCH[1]}/homebrew/opt/${BASH_REMATCH[2]}"
fi
readonly WORKSPACE


###############################################################################
# Functions
###############################################################################
[[ "${SV_VERBOSE:-0}" =~ ^[0-9]+$ ]] && SV_VERBOSE="${SV_VERBOSE:-0}" || SV_VERBOSE=1
trace () {
    [[ "$SV_VERBOSE" -lt 2 ]] || echo >&2 -e "🔬 \033[90m$*\033[0m"
}
debug () {
    [[ "$SV_VERBOSE" -lt 1 ]] || echo >&2 -e "🔍 \033[36m$*\033[0m"
}
info () {
    echo >&2 -e "ℹ️  \033[36m$*\033[0m"
}
warn () {
    echo >&2 -e "⚠️  \033[33m$*\033[0m"
}
error () {
    echo >&2 -e "❌ \033[31m$*\033[0m"
}
abort () {
    error "$*"
    exit 1
}
# heredoc MESSAGE << EOF
#    your favorite text here
# EOF
heredoc(){ IFS=$'\n' read -r -d '' "${1}" || true; }

git_config_set_if_changed() {
    local file="$1"
    local key="$2"
    local value="$3"
    local values=()

    while IFS= read -r line; do
        values+=("$line")
    done < <(git config -f "$file" --get-all "$key" 2>/dev/null || true)
    if [[ ${#values[@]} -eq 1 && "${values[0]}" == "$value" ]]; then
        return 0
    fi

    git config set -f "$file" "$key" "$value"
}

git_config_require_value() {
    local file="$1"
    local key="$2"
    local value="$3"
    local values=()

    while IFS= read -r line; do
        values+=("$line")
    done < <(git config -f "$file" --get-all "$key" 2>/dev/null || true)
    if [[ ${#values[@]} -eq 1 && "${values[0]}" == "$value" ]]; then
        return 0
    fi

    abort "--no-build set: git config $key in $file would change"
}

configure_ssh_access() {
    local guest_authorized_keys
    local ssh_public_key
    local ssh_key_count
    local tmp_authorized_keys

    if [[ ! -f "$SSH_KEYFILE_PRIV" || ! -f "$SSH_KEYFILE_PUB" ]]; then
        if [[ "$NO_BUILD" == "true" ]]; then
            abort "$SANDVAULT_USER SSH keypair is missing but --no-build flag set"
        fi
        trace "Creating SSH key files..."
        mkdir -p "$SSH_DIR"
        /bin/chmod 0700 "$SSH_DIR"
        ssh-keygen -t ed25519 \
            -f "$SSH_KEYFILE_PRIV" \
            -N "" \
            -q \
            -C "${HOST_USER}-to-sandvault@${HOSTNAME}"
    fi

    # Add HOST_USER SSH public key to SANDVAULT_USER authorized_keys
    guest_authorized_keys="$WORKSPACE/guest/home/.ssh/authorized_keys"
    ssh_public_key="$(<"$SSH_KEYFILE_PUB")"
    ssh_key_count="$(grep -Fxc "$ssh_public_key" "$guest_authorized_keys" 2>/dev/null || true)"
    if [[ "$ssh_key_count" -ne 1 ]]; then
        if [[ "$NO_BUILD" == "true" ]]; then
            abort "$SANDVAULT_USER authorized_keys would change but --no-build flag is set"
        fi
        trace "Configuring remote SSH access"
        mkdir -p "$(dirname "$guest_authorized_keys")"
        /bin/chmod 0700 "$(dirname "$guest_authorized_keys")"
        touch "$guest_authorized_keys"
        tmp_authorized_keys="$(mktemp)"
        grep -Fvx "$ssh_public_key" "$guest_authorized_keys" > "$tmp_authorized_keys" || true
        printf '%s\n' "$ssh_public_key" >> "$tmp_authorized_keys"
        /bin/chmod 0600 "$tmp_authorized_keys"
        mv -f "$tmp_authorized_keys" "$guest_authorized_keys"
    fi
}


###############################################################################
# Preconditions
###############################################################################
if [[ $OSTYPE != 'darwin'* ]]; then
    abort "ERROR: this script is for Mac OSX"
fi

if [[ $EUID -eq 0 ]]; then
    abort "ERROR: this script should not be run as root"
fi


###############################################################################
# Resources
###############################################################################
readonly VERSION="1.16.0"

# Re-entrancy detection: if SV_SESSION_ID is already set, we're already in sandvault.
NESTED=false
if [[ -n "${SV_SESSION_ID:-}" ]]; then
    NESTED=true
else
    SV_SESSION_ID="$(/usr/bin/uuidgen)"
fi
readonly NESTED
readonly SV_SESSION_ID

# Each user on the computer can have their own sandvault.
# Inside sandvault, USER will be sandvault-<name>, where name is the host-users's name.
if [[ "$USER" == sandvault-* ]]; then
    HOST_USER="${USER#sandvault-}"
else
    HOST_USER="$USER"
fi
readonly HOST_USER
readonly SANDVAULT_USER="sandvault-$HOST_USER"
readonly SANDVAULT_GROUP="sandvault-$HOST_USER"
readonly SHARED_WORKSPACE="/Users/Shared/sv-$HOST_USER"
readonly SANDVAULT_DIR_RIGHTS="group:$SANDVAULT_GROUP allow read,write,append,delete,delete_child,readattr,writeattr,readextattr,writeextattr,readsecurity,writesecurity,chown,search,list,file_inherit,directory_inherit"
readonly SANDVAULT_FILE_RIGHTS="group:$SANDVAULT_GROUP allow read,write,append,delete,readattr,writeattr,readextattr,writeextattr,readsecurity,writesecurity,chown,file_inherit,directory_inherit"

# Create sudoers.d file for passwordless sudo to sandvault user
readonly SUDOERS_FILE="/etc/sudoers.d/50-nopasswd-for-$SANDVAULT_USER"
readonly SUDOERS_BUILD_HOME_SCRIPT_NAME="/var/sandvault/buildhome-$SANDVAULT_USER"

# Installation marker file
readonly INSTALL_ORG="$HOME/.config/codeofhonor"
readonly INSTALL_PRODUCT="$INSTALL_ORG/sandvault"
readonly INSTALL_MARKER="$INSTALL_PRODUCT/install"

# Session tracking for safe multi-instance cleanup
readonly SESSION_DIR="$HOME/.local/state/sandvault"
readonly SESSION_FILE="$SESSION_DIR/sandvault.count"

# Chrome browser state (per-instance using session ID)
readonly CHROME_LOG_FILE="$SESSION_DIR/chrome-$SV_SESSION_ID.log"
readonly CHROME_DATA_DIR="$SESSION_DIR/chrome-data-$SV_SESSION_ID"
CHROME_PID=""
CHROME_PORT=""

# iOS Simulator bridge state (per-instance using session ID)
readonly IOS_BRIDGE_LOG_FILE="$SESSION_DIR/ios-bridge-$SV_SESSION_ID.log"
readonly IOS_SIM_DEVICE_NAME="sandvault-$SV_SESSION_ID"
IOS_BRIDGE_PID=""
IOS_BRIDGE_PORT=""
IOS_BRIDGE_SCRATCH_DIR=""
IOS_SIM_UDID=""

readonly SSH_DIR="$HOME/.ssh"
readonly SSH_KEYFILE_PRIV="$SSH_DIR/id_ed25519_sandvault"
readonly SSH_KEYFILE_PUB="$SSH_KEYFILE_PRIV.pub"

# Sandbox profile to restrict /Volumes access (external drives)
# Stored in /var/sandvault/ so sandvault user cannot modify it
readonly SANDBOX_PROFILE="/var/sandvault/sandbox-$SANDVAULT_USER.sb"


###############################################################################
# Functions
###############################################################################
show_version() {
    echo "$(basename "${BASH_SOURCE[0]}") version $VERSION"
    exit 0
}

show_endpoint() {
    if [[ -z "${SV_BROWSER_ENDPOINT:-}" ]]; then
        echo >&2 "No browser available. Start sandvault with --browser flag:"
        echo >&2 "  sv --browser shell"
        exit 1
    fi

    if ! /usr/bin/curl -sf "$SV_BROWSER_ENDPOINT/json/version" > /dev/null 2>&1; then
        echo >&2 "Browser endpoint $SV_BROWSER_ENDPOINT is not responding."
        echo >&2 "Chrome may have crashed. Exit and restart with: sv --browser <command>"
        exit 1
    fi

    echo "$SV_BROWSER_ENDPOINT"
    exit 0
}

brew_shellenv() {
    case "$(uname -m)" in
        arm64)
            if [[ -x /opt/homebrew/bin/brew ]]; then
                eval "$(/opt/homebrew/bin/brew shellenv)"
                return 0
            fi
            ;;
        x86_64)
            if [[ -x /usr/local/bin/brew ]]; then
                eval "$(/usr/local/bin/brew shellenv)"
                return 0
            fi
            ;;
        *)
            return 1
            ;;
    esac
}

ensure_brew() {
    # shellcheck disable=SC2310 # brew_shellenv intentionally used in condition
    if brew_shellenv; then
        return 0
    fi
    if [[ "$NESTED" == "true" ]]; then
        abort "sandvault user cannot install Homebrew; run as $HOST_USER instead"
    fi
    if [[ "$NO_BUILD" == "true" ]]; then
        abort "Missing Homebrew; refusing to install because --no-build flag set"
    fi
    debug "Installing Homebrew..."
    env bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    # shellcheck disable=SC2310 # brew_shellenv intentionally used in || condition
    brew_shellenv || abort "Homebrew install failed."
}

ensure_brew_tool() {
    local tool="$1"
    local cli_name="${2:-$tool}"
    # shellcheck disable=SC2310 # brew_shellenv intentionally used in || condition
    brew_shellenv || true

    local brew_bin
    brew_bin="$(brew --prefix)/bin/$cli_name"

    if [[ ! -x "$brew_bin" ]]; then
        if [[ "$NESTED" == "true" ]]; then
            abort "sandvault user cannot install $tool; run as $HOST_USER instead"
        fi
        if [[ "$NO_BUILD" == "true" ]]; then
            abort "Missing $cli_name; refusing to install because --no-build flag set"
        fi
        ensure_brew
        debug "Installing $tool with Homebrew..."
        if [[ "$SV_VERBOSE" -lt 3 ]]; then
            brew install --quiet "$tool"
        else
            brew install "$tool"
        fi
    fi

    if [[ "$NESTED" == "false" && -x "$brew_bin" ]] \
        && /usr/bin/xattr -p com.apple.quarantine "$brew_bin" &>/dev/null; then
        debug "Warming up $cli_name outside sandvault..."
        if ! "$brew_bin" --help &>/dev/null; then
            abort "$cli_name is quarantined and failed to warm up. Run '$brew_bin --help' once as $HOST_USER and try again."
        fi
    fi

    # Fix homebrew symlink permissions only when explicitly requested.
    # sv doesn't own these symlinks (homebrew creates them), so only
    # modify them with --fix-permissions.
    if [[ "$FIX_PERMISSIONS" == "true" && -L "$brew_bin" ]]; then
        debug "Fixing symlink permissions: $brew_bin"
        /bin/chmod -h 0755 "$brew_bin"
    fi

    # Warn if the homebrew bin directory itself has restrictive permissions,
    # which can happen when homebrew was installed/used under a restrictive umask.
    local brew_bin_dir
    brew_bin_dir="$(brew --prefix)/bin"
    if [[ -d "$brew_bin_dir" ]]; then
        local dir_perms
        dir_perms=$(/usr/bin/stat -f "%Lp" "$brew_bin_dir")
        if [[ "$((8#$dir_perms & 8#0005))" -eq 0 ]]; then
            warn "Homebrew bin directory ($brew_bin_dir) has restrictive permissions ($dir_perms). Run: sudo chmod -R o+rX $(brew --prefix)"
        fi
    fi

    if command -v "$cli_name" &>/dev/null; then
        return 0
    fi
    warn "Homebrew installed $tool, but no '$cli_name' CLI was found in PATH. Will use \$HOME/node_modules/bin/$cli_name if present."
    return 0
}

install_tools () {
    if [[ "$NATIVE_INSTALL" == "true" ]]; then
        # Native install is handled inside the sandbox by guest/home/bin/* scripts;
        # ensure node is available for npm-based tools (codex, gemini).
        case "${COMMAND:-}" in
            codex|gemini)
                ensure_brew_tool "node" "node"
                ;;
            *)
                # node installation not required
                ;;
        esac
        return 0
    fi

    # Install homebrew tools only when the user invokes them.
    case "${COMMAND:-}" in
        claude)
            ensure_brew_tool "claude-code" "claude"
            ;;
        codex)
            ensure_brew_tool "codex" "codex"
            ;;
        opencode)
            ensure_brew_tool "anomalyco/tap/opencode" "opencode"
            ;;
        gemini)
            ensure_brew_tool "gemini-cli" "gemini"
            ;;
        *)
            # No tool installation needed for other commands
            ;;
    esac
}

# Ensure host-side dependencies for --ios are present: uv (via
# Homebrew) and iosef (via `uv tool install`). The bridge runs as the host
# user, so both tools must be available in the host user's PATH.
install_ios_deps() {
    # uv via Homebrew.
    ensure_brew_tool "uv" "uv"

    # iosef via `uv tool install`. `uv tool install` is idempotent, so we
    # only call it when iosef is not already on PATH.
    if command -v iosef &>/dev/null; then
        return 0
    fi

    # Already-installed uv tools live in `uv tool dir`/bin. Check there so
    # we don't spuriously re-install if PATH just doesn't include it yet.
    # The bridge itself falls back to `uv tool run iosef` so bridge
    # functionality doesn't depend on PATH; this is only a convenience note
    # for host-side direct use.
    local uv_bin_dir
    if uv_bin_dir=$(uv tool dir --bin 2>/dev/null) && [[ -x "$uv_bin_dir/iosef" ]]; then
        debug "iosef installed at $uv_bin_dir/iosef (not on PATH; bridge uses 'uv tool run iosef'). For direct host-side use, add \"$uv_bin_dir\" to PATH or run: uv tool update-shell"
        return 0
    fi

    if [[ "$NESTED" == "true" ]]; then
        abort "sandvault user cannot install iosef; run as $HOST_USER instead"
    fi
    if [[ "$NO_BUILD" == "true" ]]; then
        abort "Missing iosef; refusing to install because --no-build flag set"
    fi

    debug "Installing iosef via uv..."
    if ! uv tool install iosef >/dev/null 2>&1; then
        abort "Failed to install iosef via uv. Try: uv tool install iosef"
    fi

    if ! command -v iosef &>/dev/null; then
        uv_bin_dir=$(uv tool dir --bin 2>/dev/null || echo "")
        if [[ -n "$uv_bin_dir" && -x "$uv_bin_dir/iosef" ]]; then
            debug "iosef installed at $uv_bin_dir/iosef (not on PATH; bridge uses 'uv tool run iosef'). For direct host-side use, add \"$uv_bin_dir\" to PATH or run: uv tool update-shell"
        else
            abort "iosef install appeared to succeed but the binary is not available."
        fi
    fi
}

force_cleanup_sandvault_processes() {
    local cleanup_mode="${1:-session-exit}"
    if [[ "$NESTED" == "true" ]]; then
        return 0
    fi

    if [[ "$cleanup_mode" != "force-all" ]]; then
        trace "Skipping user-wide cleanup on ordinary session exit"
        return 0
    fi

    # Stop host-side Chrome if running
    stop_chrome

    # Stop host-side iOS simulator bridge if running
    stop_ios_simulator

    # Try to bootout the user session (this terminates all processes)
    trace "Terminating $SANDVAULT_USER user session..."
    local sandvault_uid
    if sandvault_uid=$(dscl . -read "/Users/$SANDVAULT_USER" UniqueID 2>/dev/null | awk '{print $2}') ; then
        sudo launchctl bootout "user/$sandvault_uid" 2>/dev/null || true
        sleep 0.2
    fi

    # Final forceful cleanup only if needed
    if pgrep -u "$SANDVAULT_USER" >/dev/null 2>&1; then
        trace "Final cleanup of remaining processes..."
        sudo pkill -9 -u "$SANDVAULT_USER" 2>/dev/null || true
    fi

    # Final check
    if pgrep -u "$SANDVAULT_USER" >/dev/null 2>&1; then
        warn "Some $SANDVAULT_USER processes may still be running (likely system daemons)"
    fi
}

kill_chrome_pid() {
    local pid="$1"
    if kill -0 "$pid" 2>/dev/null; then
        debug "Stopping Chrome (PID $pid)..."
        kill "$pid" 2>/dev/null || true
        local i
        for (( i=0; i<20; i++ )); do
            kill -0 "$pid" 2>/dev/null || { wait "$pid" 2>/dev/null || true; return 0; }
            sleep 0.1
        done
        trace "Force-killing Chrome (PID $pid)..."
        kill -9 "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    fi
}

stop_chrome() {
    if [[ -n "$CHROME_PID" ]]; then
        kill_chrome_pid "$CHROME_PID"
        CHROME_PID=""
    fi
    rm -f "$CHROME_LOG_FILE"
    rm -rf "$CHROME_DATA_DIR"
}

start_chrome() {
    mkdir -p "$SESSION_DIR"

    # Locate Chrome binary
    local chrome_bin=""
    if [[ -x "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" ]]; then
        chrome_bin="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    elif [[ -x "/Applications/Chromium.app/Contents/MacOS/Chromium" ]]; then
        chrome_bin="/Applications/Chromium.app/Contents/MacOS/Chromium"
    else
        abort "Chrome or Chromium not found. Install Google Chrome to use --browser."
    fi

    mkdir -p "$CHROME_DATA_DIR"
    rm -f "$CHROME_LOG_FILE"
    rm -f "$CHROME_DATA_DIR/SingletonLock" "$CHROME_DATA_DIR/SingletonCookie" "$CHROME_DATA_DIR/SingletonSocket"

    # Launch Chrome headless with dynamic port allocation
    debug "Starting headless Chrome..."
    "$chrome_bin" \
        --headless \
        --no-sandbox \
        --disable-gpu \
        --remote-debugging-port=0 \
        --remote-debugging-address=127.0.0.1 \
        --user-data-dir="$CHROME_DATA_DIR" \
        --no-first-run \
        --no-default-browser-check \
        --disable-extensions \
        --disable-background-networking \
        > "$CHROME_LOG_FILE" 2>&1 &
    CHROME_PID=$!

    # Wait for Chrome to report its debugging port (up to 15 seconds).
    # The window is generous because CI runners under load (parallel iOS
    # simulator boot, Xcode jobs) can take several seconds to start Chrome.
    local port=""
    local i
    for (( i=0; i<150; i++ )); do
        if ! kill -0 "$CHROME_PID" 2>/dev/null; then
            CHROME_PID=""
            abort "Chrome exited unexpectedly. Check $CHROME_LOG_FILE for details."
        fi
        port=$(sed -n 's|.*DevTools listening on ws://127\.0\.0\.1:\([0-9]*\)/.*|\1|p' "$CHROME_LOG_FILE" 2>/dev/null | head -1)
        if [[ -n "$port" ]]; then
            break
        fi
        sleep 0.1
    done

    if [[ -z "$port" ]]; then
        kill_chrome_pid "$CHROME_PID"
        CHROME_PID=""
        abort "Chrome did not report a debugging port within 15 seconds. Check $CHROME_LOG_FILE."
    fi

    CHROME_PORT="$port"
    debug "Chrome started (PID $CHROME_PID, port $CHROME_PORT)"
}

kill_bridge_pid() {
    local pid="$1"
    if kill -0 "$pid" 2>/dev/null; then
        debug "Stopping iOS bridge (PID $pid)..."
        kill "$pid" 2>/dev/null || true
        local i
        for (( i=0; i<20; i++ )); do
            kill -0 "$pid" 2>/dev/null || { wait "$pid" 2>/dev/null || true; return 0; }
            sleep 0.1
        done
        trace "Force-killing iOS bridge (PID $pid)..."
        kill -9 "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    fi
}

stop_ios_simulator() {
    if [[ -n "$IOS_BRIDGE_PID" ]]; then
        kill_bridge_pid "$IOS_BRIDGE_PID"
        IOS_BRIDGE_PID=""
    fi
    if [[ -n "$IOS_SIM_UDID" ]]; then
        debug "Shutting down iOS simulator $IOS_SIM_UDID..."
        /usr/bin/xcrun simctl shutdown "$IOS_SIM_UDID" >/dev/null 2>&1 || true
        /usr/bin/xcrun simctl delete "$IOS_SIM_UDID" >/dev/null 2>&1 || true
        IOS_SIM_UDID=""
    fi
    rm -f "$IOS_BRIDGE_LOG_FILE"
    if [[ -n "$IOS_BRIDGE_SCRATCH_DIR" ]]; then
        rm -rf "$IOS_BRIDGE_SCRATCH_DIR"
        IOS_BRIDGE_SCRATCH_DIR=""
    fi
}

# Find the newest available iPhone device type identifier and iOS runtime
# identifier. Prints "<device_type>\t<runtime>" on stdout. Returns non-zero
# if no usable pair is found.
ios_pick_device_and_runtime() {
    /usr/bin/xcrun simctl list -j devicetypes runtimes 2>/dev/null \
        | "$WORKSPACE/helpers/sv-ios-pick-device"
}

start_ios_simulator() {
    mkdir -p "$SESSION_DIR"

    if [[ ! -x /usr/bin/xcrun ]]; then
        abort "xcrun not found. Install Xcode or the Command Line Tools."
    fi

    # Select device type and runtime.
    local device_type runtime pair
    # shellcheck disable=SC2310 # ios_pick_device_and_runtime intentionally used in condition
    if ! pair=$(ios_pick_device_and_runtime); then
        abort "No iOS simulator runtime available. Install one via Xcode → Settings → Platforms."
    fi
    device_type="${pair%$'\t'*}"
    runtime="${pair#*$'\t'}"
    debug "Selected device=$device_type runtime=$runtime"

    # Create a fresh scratch simulator for this session.
    if ! IOS_SIM_UDID=$(/usr/bin/xcrun simctl create "$IOS_SIM_DEVICE_NAME" "$device_type" "$runtime" 2>/dev/null); then
        abort "Failed to create iOS simulator (device=$device_type runtime=$runtime)"
    fi
    debug "Created simulator $IOS_SIM_DEVICE_NAME UDID=$IOS_SIM_UDID"

    # Begin booting. Don't wait for bootstatus here — the bridge polls
    # readiness in a background thread and rejects non-/ready endpoints
    # with HTTP 503 until the simulator finishes booting (30-90s).
    # Sandbox sessions can start interacting immediately and poll
    # /ready until it returns 200.
    if ! /usr/bin/xcrun simctl boot "$IOS_SIM_UDID" >/dev/null 2>&1; then
        stop_ios_simulator
        abort "Failed to boot iOS simulator $IOS_SIM_UDID"
    fi
    debug "iOS simulator boot started in background; bridge will report readiness."

    if [[ "$USE_IOS_SIMULATOR_GUI" == "true" ]]; then
        debug "Opening Simulator.app to display the device..."
        /usr/bin/open -a Simulator >/dev/null 2>&1 || \
            warn "Failed to open Simulator.app; the device is still running headless."
    fi

    # Launch the HTTP bridge.
    # Place the scratch directory under $SHARED_WORKSPACE/tmp so that
    # screenshots and other bridge artifacts are accessible to both the
    # host user and the sandboxed user.
    mkdir -p "$SHARED_WORKSPACE/tmp"
    IOS_BRIDGE_SCRATCH_DIR="$(mktemp -d "$SHARED_WORKSPACE/tmp/sv-ios-bridge.XXXXXX")"
    rm -f "$IOS_BRIDGE_LOG_FILE"
    debug "Starting iOS bridge..."
    "$WORKSPACE/helpers/sv-ios-bridge" --udid "$IOS_SIM_UDID" \
        --host 127.0.0.1 --port 0 \
        --scratch-dir "$IOS_BRIDGE_SCRATCH_DIR" \
        > "$IOS_BRIDGE_LOG_FILE" 2>&1 &
    IOS_BRIDGE_PID=$!

    # Wait for the bridge to report its port (up to 15 seconds). Generous
    # because CI runners under load can be slow to start Python processes.
    local port=""
    local i
    for (( i=0; i<150; i++ )); do
        if ! kill -0 "$IOS_BRIDGE_PID" 2>/dev/null; then
            IOS_BRIDGE_PID=""
            stop_ios_simulator
            abort "iOS bridge exited unexpectedly. Check $IOS_BRIDGE_LOG_FILE for details."
        fi
        port=$(sed -n 's|.*Bridge listening on http://127\.0\.0\.1:\([0-9]*\).*|\1|p' "$IOS_BRIDGE_LOG_FILE" 2>/dev/null | head -1)
        if [[ -n "$port" ]]; then
            break
        fi
        sleep 0.1
    done

    if [[ -z "$port" ]]; then
        stop_ios_simulator
        abort "iOS bridge did not report a port within 15 seconds. Check $IOS_BRIDGE_LOG_FILE."
    fi

    IOS_BRIDGE_PORT="$port"
    debug "iOS bridge started (PID $IOS_BRIDGE_PID, port $IOS_BRIDGE_PORT)"
}

register_session() {
    mkdir -p "$SESSION_DIR"
    local new_count
    # shellcheck disable=SC2016 # Single quotes intentional - variables expand in inner bash
    new_count=$(/usr/bin/lockf "$SESSION_FILE.lock" /bin/bash -c '
        session_file=$1
        count=$(cat "$session_file" 2>/dev/null || echo 0)
        [[ "$count" =~ ^[0-9]+$ ]] || count=0
        new_count=$((count + 1))
        echo "$new_count" > "$session_file"
        echo "$new_count"
    ' bash "$SESSION_FILE")
    trace "Session registered (count: $new_count)"
}

unregister_session() {
    # Per-session cleanup
    [[ "$USE_BROWSER" == "true" ]] && stop_chrome
    [[ "$USE_IOS_SIMULATOR" == "true" ]] && stop_ios_simulator
    rm -f "$SHARED_WORKSPACE/tmp/sv-session-$SV_SESSION_ID" 2>/dev/null || true

    mkdir -p "$SESSION_DIR"
    local prev_count
    local new_count
    # shellcheck disable=SC2016 # Single quotes intentional - variables expand in inner bash
    read -r prev_count new_count < <(/usr/bin/lockf "$SESSION_FILE.lock" /bin/bash -c '
        session_file=$1
        count=$(cat "$session_file" 2>/dev/null || echo 1)
        [[ "$count" =~ ^[0-9]+$ ]] || count=1
        new_count=$((count - 1))
        echo "$new_count" > "$session_file"
        echo "$count $new_count"
    ' bash "$SESSION_FILE")
    trace "Session unregistered (count: $new_count)"
    if [[ "$prev_count" -le 1 ]]; then
        trace "Last session exited; skipping user-wide sandvault cleanup"
        force_cleanup_sandvault_processes
    else
        trace "Other sessions still active; skipping cleanup"
    fi
}

configure_shared_folder_permssions() {
    local enable="$1"

    # Grant write access to shared workspace for sandvault group. We want
    # to modify files and symbolic links, not what symbolic links point to.
    # Use `find | xargs chmod -h` instead of `chmod -R -h` because the latter
    # causes: "chmod: the -R and -h options may not be specified together"
    if [[ "$enable" == "true" ]]; then
        # Make workspace accessible to $HOST_USER and $SANDVAULT_GROUP only
        trace "Configuring $SHARED_WORKSPACE: set owner to $HOST_USER:$SANDVAULT_GROUP"
        sudo /usr/sbin/chown -f -R "$HOST_USER:$SANDVAULT_GROUP" "$SHARED_WORKSPACE"
        trace "Configuring $SHARED_WORKSPACE permissions..."
        sudo /bin/chmod 0770 "$SHARED_WORKSPACE"
        # Apply directory ACL (with search/list) to directories only,
        # and file ACL (without search/list) to files only, so that
        # files don't inherit the execute bit from the search permission.
        # Single find pass to avoid walking the tree twice.
        trace "Configuring $SHARED_WORKSPACE: add directory and file ACLs"
        sudo find "$SHARED_WORKSPACE" \
            \( -type d -exec /bin/chmod -h +a "$SANDVAULT_DIR_RIGHTS" {} + \) \
            -o \
            \( ! -type d -exec /bin/chmod -h +a "$SANDVAULT_FILE_RIGHTS" {} + \)
    else
        # Make workspace accessible to $HOST_USER only
        trace "Configuring $SHARED_WORKSPACE: restoring owner to $HOST_USER:$(id -gn)"
        sudo /usr/sbin/chown -f -R "$HOST_USER:$(id -gn)" "$SHARED_WORKSPACE"
        trace "Configuring $SHARED_WORKSPACE permissions..."
        sudo /bin/chmod 0700 "$SHARED_WORKSPACE"
        # Remove all ACL entries for the sandvault group. Apply the
        # directory ACE only to directories and the file ACE only to
        # files, mirroring the `enable=true` branch above. Also remove
        # the legacy combined ACE (from before the dir/file split),
        # which was historically applied to every entry. Each -a removal
        # is a no-op (fails silently) if the ACE doesn't exist. Single
        # find pass to avoid walking the tree twice.
        trace "Configuring $SHARED_WORKSPACE: remove $SANDVAULT_GROUP ACLs"
        local legacy_ace="group:$SANDVAULT_GROUP allow read,write,append,delete,delete_child,readattr,writeattr,readextattr,writeextattr,readsecurity,writesecurity,chown,search,list,file_inherit,directory_inherit"
        sudo find "$SHARED_WORKSPACE" \
            \( -type d -exec /bin/chmod -h -a "$SANDVAULT_DIR_RIGHTS" {} + \
                       -exec /bin/chmod -h -a "$legacy_ace" {} + \) \
            -o \
            \( ! -type d -exec /bin/chmod -h -a "$SANDVAULT_FILE_RIGHTS" {} + \
                         -exec /bin/chmod -h -a "$legacy_ace" {} + \) \
            2>/dev/null || true
    fi
}

###############################################################################
# Agentsview export
#
# Mirrors sandbox session data into the host user's agentsview installation.
# See docs/superpowers/specs/2026-05-03-agentsview-export-design.md.
###############################################################################

# Source the path map (single source of truth for agent → subdir/key/default)
# shellcheck source=helpers/agentsview-paths.sh
source "$WORKSPACE/helpers/agentsview-paths.sh"

readonly AGENTSVIEW_STATE_FILE="$SHARED_WORKSPACE/setup/agentsview-export.state"
readonly AGENTSVIEW_HOST_CONFIG="$HOME/.agentsview/config.toml"

# Detect host-side agentsview presence (binary on PATH or data dir present).
agentsview_detect() {
    command -v agentsview &>/dev/null && return 0
    [[ -d "$HOME/.agentsview" ]] && return 0
    return 1
}

# Pre-flight: verify each (existing) sandbox agent dir is owned by the
# sandbox user. Returns 0 if all clean, 1 if contamination detected.
# Prints the offending dir + remediation to stderr on failure.
agentsview_contamination_check() {
    local agent subdir full owner
    local clean=0
    for agent in "${AGENTSVIEW_AGENTS[@]}"; do
        subdir="$(agentsview_field SUBDIR "$agent")"
        full="/Users/$SANDVAULT_USER/$subdir"
        [[ -e "$full" ]] || continue
        owner="$(/usr/bin/stat -f "%Su" "$full" 2>/dev/null || echo unknown)"
        if [[ "$owner" != "$SANDVAULT_USER" ]]; then
            error "Cannot enable agentsview export: $full is owned by $owner (expected $SANDVAULT_USER)."
            error "  This usually means an earlier agent run had HOME pointing at the sandbox home."
            error "  Fix with:"
            error "    sudo chown -R $SANDVAULT_USER:$SANDVAULT_GROUP $full"
            error "  then re-run \`sv setup\`."
            clean=1
        fi
    done
    return "$clean"
}

# Create the four mirror symlinks under $SHARED_WORKSPACE/sessions/.
# Idempotent. Logs and continues on individual failures (warn for tolerated
# skips like a non-symlink at the path; error for unexpected `ln` failures).
agentsview_install_symlinks() {
    local agent subdir link target current
    mkdir -p "$SHARED_WORKSPACE/sessions"
    for agent in "${AGENTSVIEW_AGENTS[@]}"; do
        subdir="$(agentsview_field SUBDIR "$agent")"
        link="$SHARED_WORKSPACE/sessions/$agent"
        target="/Users/$SANDVAULT_USER/$subdir"
        if [[ -L "$link" ]]; then
            current="$(readlink "$link")"
            if [[ "$current" == "$target" ]]; then
                trace "agentsview symlink ok: $link"
                continue
            fi
            warn "agentsview symlink $link points to $current (expected $target); replacing"
            rm -f "$link"
        elif [[ -e "$link" ]]; then
            warn "agentsview: $link exists and is not a symlink; skipping"
            continue
        fi
        /bin/ln -s "$target" "$link" || error "agentsview: failed to create symlink $link"
    done
}

# Update the host user's agentsview config.toml. Shows a diff and prompts.
# Returns 0 on success (or no-op), 1 on failure.
agentsview_update_config() {
    local script="$WORKSPACE/helpers/agentsview-config.py"
    local agent_args=()
    local agent tomlkey link
    for agent in "${AGENTSVIEW_AGENTS[@]}"; do
        tomlkey="$(agentsview_field TOMLKEY "$agent")"
        link="$SHARED_WORKSPACE/sessions/$agent"
        agent_args+=(--agent "$tomlkey=$link")
    done

    local diff_out
    if ! diff_out="$(/usr/bin/python3 "$script" \
        --config-path "$AGENTSVIEW_HOST_CONFIG" \
        --home "$HOME" \
        --diff \
        "${agent_args[@]}" 2>&1)"; then
        error "agentsview: failed to compute config diff:"
        error "$diff_out"
        return 1
    fi

    if [[ -z "$diff_out" ]]; then
        debug "agentsview: $AGENTSVIEW_HOST_CONFIG already up to date"
        return 0
    fi

    info ""
    info "Proposed changes to $AGENTSVIEW_HOST_CONFIG:"
    info ""
    printf '%s\n' "$diff_out"
    info ""
    info "Note: writing this file does not preserve comments on managed keys."
    printf 'Apply these changes? [y/N] '
    local reply
    read -r reply
    if [[ "$reply" != "y" && "$reply" != "Y" ]]; then
        info "agentsview: skipped config update"
        return 0
    fi

    if ! /usr/bin/python3 "$script" \
        --config-path "$AGENTSVIEW_HOST_CONFIG" \
        --home "$HOME" \
        --write \
        "${agent_args[@]}"; then
        error "agentsview: failed to write $AGENTSVIEW_HOST_CONFIG"
        return 1
    fi
    info "agentsview: updated $AGENTSVIEW_HOST_CONFIG"
}

# Orchestrate the full opt-in flow. Idempotent: skips silently if state
# file already records a choice.
agentsview_setup() {
    if [[ -f "$AGENTSVIEW_STATE_FILE" ]]; then
        trace "agentsview: choice already recorded ($(cat "$AGENTSVIEW_STATE_FILE")); skipping prompt"
        return 0
    fi
    if [[ ! -t 0 ]]; then
        trace "agentsview: non-interactive; skipping prompt"
        return 0
    fi
    if [[ ! -f "$WORKSPACE/helpers/agentsview-config.py" ]]; then
        trace "agentsview: helpers/agentsview-config.py not installed; skipping"
        return 0
    fi
    if ! agentsview_detect; then
        trace "agentsview: not detected; skipping"
        return 0
    fi
    if ! agentsview_contamination_check; then
        # Pre-flight already printed remediation. Don't persist a choice;
        # user re-runs after fixing.
        return 0
    fi

    info ""
    info "Detected agentsview on this machine. Mirror sandvault session data so it"
    info "appears in agentsview's dashboard, search, and cost tracking?"
    info ""
    info "This will:"
    info "  - add read-only symlinks under $SHARED_WORKSPACE/sessions/"
    info "  - apply read-only ACLs to sandbox agent session dirs"
    info "  - add four scan paths to $AGENTSVIEW_HOST_CONFIG (with diff confirmation)"
    info "  - rewrite your agentsview config without preserving comments"
    info ""
    info "Sandvault won't auto-track new agents agentsview adds in future versions"
    info "(re-run \`sv setup --rebuild\` after deleting $AGENTSVIEW_STATE_FILE to refresh)."
    printf 'Enable agentsview export? [y/N] '
    local reply
    read -r reply
    mkdir -p "$SHARED_WORKSPACE/setup"
    if [[ "$reply" != "y" && "$reply" != "Y" ]]; then
        echo disabled > "$AGENTSVIEW_STATE_FILE"
        info "agentsview: export disabled"
        return 0
    fi

    echo enabled > "$AGENTSVIEW_STATE_FILE"
    agentsview_install_symlinks
    agentsview_update_config
}


uninstall() {
    info "Uninstalling..."
    force_cleanup_sandvault_processes force-all
    rm -rf "$SESSION_DIR"/chrome-data-* "$SESSION_DIR"/chrome-*.log
    rm -f "$SESSION_DIR"/ios-bridge-*.log
    rm -f /tmp/sandvault-configure-* 2>/dev/null || true

    # Remove the install marker file first; it's a sentinel for "everything is complete".
    # By removing it first we force a rebuild if the user wants to run this again.
    rm -rf "$INSTALL_MARKER"
    rmdir "$INSTALL_PRODUCT" &>/dev/null || true
    rmdir "$INSTALL_ORG" &>/dev/null || true

    # Remove the sudoers file
    sudo rm -rf "$SUDOERS_FILE"

    # Remove build home script and sandbox profile
    sudo rm -rf "$SUDOERS_BUILD_HOME_SCRIPT_NAME"
    sudo rm -rf "$SANDBOX_PROFILE"
    sudo rmdir "$(dirname "$SUDOERS_BUILD_HOME_SCRIPT_NAME")" 2>/dev/null || true

    # Remove shared folder ACLS
    debug "Configuring shared workspace permissions..."
    configure_shared_folder_permssions false

    # Remove host user from sandvault group
    debug "Removing user and group..."
    sudo dseditgroup -o edit -d "$HOST_USER" -t user "$SANDVAULT_GROUP" 2>/dev/null || true

    # Remove sandvault user from SSH group BEFORE deleting the user
    sudo dseditgroup -o edit -d "$SANDVAULT_USER" -t user com.apple.access_ssh 2>/dev/null || true

    # Now delete the user and group
    sudo dscl . -delete "/Users/$SANDVAULT_USER" &>/dev/null || true
    sudo dscl . -delete "/Groups/$SANDVAULT_GROUP" &>/dev/null || true
    sudo rm -rf "/Users/$SANDVAULT_USER"

    # Cleanup SSH
    rm -rf "$SSH_KEYFILE_PRIV" "$SSH_KEYFILE_PUB"

    # Remove shared workspace
    # Do not remove $SHARED_WORKSPACE/user
    rmdir "$SHARED_WORKSPACE/tmp" 2>/dev/null || true

    rm -f "$SHARED_WORKSPACE/setup/gitconfig"
    rm -f "$SHARED_WORKSPACE/setup/claude-json"
    rm -f "$SHARED_WORKSPACE/setup/agentsview-export"
    rm -f "$AGENTSVIEW_STATE_FILE"
    rmdir "$SHARED_WORKSPACE/setup" 2>/dev/null || true

    # Remove agentsview mirror symlinks (host installs in setup; harmless to leave)
    if [[ -d "$SHARED_WORKSPACE/sessions" ]]; then
        for _agent in "${AGENTSVIEW_AGENTS[@]}"; do
            rm -f "$SHARED_WORKSPACE/sessions/$_agent"
        done
        rmdir "$SHARED_WORKSPACE/sessions" 2>/dev/null || true
    fi

    rm -f "$SHARED_WORKSPACE/SANDVAULT-README.md"
    rmdir "$SHARED_WORKSPACE" 2>/dev/null || true

    if [[ -d "$SHARED_WORKSPACE" ]]; then
        info "Keeping $SHARED_WORKSPACE directory (it is not empty)"
    else
        debug "Removed $SHARED_WORKSPACE (it was empty)"
    fi
}


###############################################################################
# Parse command line
###############################################################################
REBUILD=false
NO_BUILD=false
USE_SANDBOX=true
USE_BROWSER=false
USE_IOS_SIMULATOR=false
USE_IOS_SIMULATOR_GUI=false
FIX_PERMISSIONS=false
NATIVE_INSTALL=false
MODE=shell
COMMAND_ARGS=()
INITIAL_DIR=""
show_help() {
    echo "SandVault $VERSION by Patrick Wyatt <pat@codeofhonor.com>"
    echo "Project home page: https://github.com/webcoyote/sandvault"
    echo
    echo "SandVault (sv) manages a limited user account to sandbox shell commands and AI agents,"
    echo "providing a lightweight alternative to application isolation using virtual machines."
    echo
    echo "Usage: sv [options] command [-- args ...]"
    echo ""
    echo "Options:"
    echo "  -s, --ssh            Connect via SSH [default: use account impersonation]"
    echo "  -r, --rebuild        Rebuild configuration and file permissions/ACLs"
    echo "  -v, --verbose        Enable verbose output"
    echo "  -vv / -vvv           More verbose / even more verbose"
    echo "  -h, --help           Show this help message"
    echo "  -n, --no-build       Refuse to make any sandbox changes; error if changes are needed"
    echo "  -x, --no-sandbox     Disable sandbox-exec restrictions"
    echo "  -b, --browser        Launch headless Chrome and pass endpoint into sandbox"
    echo "  -e, --endpoint       Show Chrome endpoint URL (requires --browser session)"
    echo "  -i, --ios            Boot iOS Simulator and expose HTTP bridge into sandbox"
    echo "  -I, --ios-gui        Also show the Simulator.app window (implies --ios)"
    echo "  -c, --clone URL|PATH (removed — use sv-clone instead)"
    echo "  -N, --native-install Use native installers instead of Homebrew for AI tools"
    echo "  --fix-permissions    Fix umask and file permissions [standalone or with build]"
    echo "  --version            Show version information"
    echo ""
    echo "Commands:"
    echo "  cl, claude [PATH]    Open Claude Code in sandvault"
    echo "  co, codex  [PATH]    Open OpenAI Codex in sandvault"
    echo "  o,  opencode [PATH]  Open OpenCode in sandvault"
    echo "  g,  gemini [PATH]    Open Google Gemini in sandvault"
    echo "  s, shell   [PATH]    Open shell in sandvault"
    echo "  b, build             Build sandvault"
    echo "  u, uninstall         Remove sandvault; keep shared files"
    echo ""
    echo "Arguments after -- are passed to the command (claude, codex, opencode, gemini, shell)"
    echo ""
    echo "Environment:"
    echo "  SANDVAULT_ARGS       Default arguments (prepended to command line)"
    echo "                       Example: export SANDVAULT_ARGS=\"--verbose --ssh --browser\""
    exit 0
}

# Prepend arguments from SV_ARGS environment variable
if [[ -n "${SANDVAULT_ARGS:-}" ]]; then
    # Use xargs to parse shell-style quoting without eval
    sv_args_array=()
    while IFS= read -r arg; do
        sv_args_array+=("$arg")
    done < <(xargs -n1 printf '%s\n' <<< "$SANDVAULT_ARGS")
    set -- "${sv_args_array[@]}" "$@"
fi

# Parse optional arguments
NEW_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --)
            # Everything after -- goes to COMMAND_ARGS
            shift
            while [[ $# -gt 0 ]]; do
                COMMAND_ARGS+=("$1")
                shift
            done
            break
            ;;
        -s|--ssh)
            MODE=ssh
            shift
            ;;
        -r|--rebuild)
            REBUILD=true
            shift
            ;;
        -v|--verbose)
            ((SV_VERBOSE++)) || true
            shift
            ;;
        -vv)
            ((SV_VERBOSE+=2)) || true
            shift
            ;;
        -vvv)
            ((SV_VERBOSE+=3)) || true
            shift
            ;;
        -n|--no-build)
            NO_BUILD=true
            shift
            ;;
        -x|--no-sandbox)
            USE_SANDBOX=false
            shift
            ;;
        -b|--browser)
            USE_BROWSER=true
            shift
            ;;
        -i|--ios)
            USE_IOS_SIMULATOR=true
            shift
            ;;
        -I|--ios-gui)
            USE_IOS_SIMULATOR=true
            USE_IOS_SIMULATOR_GUI=true
            shift
            ;;
        --fix-permissions)
            FIX_PERMISSIONS=true
            shift
            ;;
        -N|--native-install)
            NATIVE_INSTALL=true
            shift
            ;;
        -c|--clone)
            abort "--clone has been removed from sv. Use sv-clone instead:\n  sv-clone <URL|PATH> [-- sv-args ...]"
            ;;
        -h|--help)
            show_help
            ;;
        -e|--endpoint)
            show_endpoint
            ;;
        --version)
            show_version
            ;;
        -*)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
        *)
            NEW_ARGS+=("$1")
            shift
            ;;
    esac
done
set -- "${NEW_ARGS[@]:-}"

# Parse fixed arguments
case "${1:-}" in
    cl|claude)
        COMMAND=claude
        INITIAL_DIR="${2:-}"
        ;;
    co|codex)
        COMMAND=codex
        INITIAL_DIR="${2:-}"
        ;;
    o|opencode)
        COMMAND=opencode
        INITIAL_DIR="${2:-}"
        ;;
    g|gemini)
        COMMAND=gemini
        INITIAL_DIR="${2:-}"
        ;;
    s|shell)
        COMMAND=
        INITIAL_DIR="${2:-}"
        ;;
    b|build)
        COMMAND=build
        ;;
    u|uninstall)
        if [[ "$NESTED" == "true" ]]; then
            abort "sandvault running recursively; cannot uninstall"
        fi
        if [[ "$NO_BUILD" == "true" ]]; then
            abort "--no-build set: refusing to uninstall"
        fi
        uninstall
        exit 0
        ;;
    *)
        if [[ "$FIX_PERMISSIONS" == "true" ]]; then
            # --fix-permissions can run standalone (implies build)
            COMMAND=build
        else
            show_help
        fi
        ;;
esac
readonly COMMAND

if [[ "$FIX_PERMISSIONS" == "true" && "$COMMAND" != "build" ]]; then
    abort "--fix-permissions can only be used standalone or with build"
fi

# Resolve symlinks to get the real path
INITIAL_DIR="$(cd "${INITIAL_DIR:-"${PWD}"}" 2>/dev/null && pwd -P || echo "$INITIAL_DIR")"
readonly INITIAL_DIR


###############################################################################
# Determine whether configuration is valid
###############################################################################
# INSTALL_MARKER is stored in HOST_USER home directory so SANDVAULT_USER
# cannot access it to check for completed installation.
# TODO: perhaps we should store it in the shared directory instead?
if [[ "$NESTED" == "false" ]]; then
    if [[ ! -f "$INSTALL_MARKER" ]]; then
        # Install marker not present so installation is incomplete
        REBUILD=true

        # Since this is an initial install, provide more feedback
        SV_VERBOSE=$(( SV_VERBOSE > 1 ? SV_VERBOSE : 1 ))
    fi
fi

if [[ "$NO_BUILD" == "true" ]]; then
    if [[ "$COMMAND" == "build" && "$FIX_PERMISSIONS" != "true" ]]; then
        abort "refusing sandvault build command with --no-build flag"
    fi
    if [[ "$REBUILD" == "true" ]]; then
        abort "sandvault is not installed (run without --no-build flag)"
    fi
fi

if [[ "$NESTED" == "true" ]]; then
    if [[ "$REBUILD" == "true" ]]; then
        abort "Cannot rebuild sandvault inside sandvault (sudo unavailable)"
    fi

    # Cannot build because sudo is not available to SANDBOX_USER
    NO_BUILD=true

    # MacOS does not allow nested sandboxes
    USE_SANDBOX=false
fi

readonly NO_BUILD
readonly REBUILD
readonly USE_SANDBOX
readonly FIX_PERMISSIONS

###############################################################################
# Umask check
###############################################################################
# A restrictive umask (e.g. 077) causes permission failures: /var/sandvault/
# becomes inaccessible to the sandvault user, homebrew symlinks get owner-only
# permissions, etc. Detect and warn; override only with --fix-permissions.
ORIGINAL_UMASK="$(umask)"
if [[ "$((8#$ORIGINAL_UMASK & 8#0044))" -ne 0 ]]; then
    if [[ "$FIX_PERMISSIONS" == "true" ]]; then
        info "Umask is $ORIGINAL_UMASK (restrictive); overriding to 022 for this session"
        umask 022
    else
        warn "Host umask is $ORIGINAL_UMASK (expected 022 or less restrictive). Permission errors may occur. Re-run with --fix-permissions to correct."
    fi
elif [[ "$FIX_PERMISSIONS" == "true" ]]; then
    debug "Umask is $ORIGINAL_UMASK (ok, no override needed)"
fi


###############################################################################
# Setup
###############################################################################
if [[ "$REBUILD" == "true" ]]; then
    info "Installing sandvault..."
    sudo "-p Password required to create sandvault: " true
fi

install_tools
if [[ "$USE_IOS_SIMULATOR" == "true" ]]; then
    install_ios_deps
fi


###############################################################################
# Create sandvault user and group
###############################################################################

# Pick the first free integer ID at or above SV_MIN_ID, scanning the
# union of existing user UIDs and group GIDs. Avoids three failure
# modes of the old "max(UniqueID)+1" approach:
#   1) collisions with reserved/system IDs that sit above the current max
#   2) cross-collisions where a group GID equals a user UID (or vice versa)
#   3) gaps left by deleted accounts being reused without coordination
# SV_MIN_ID defaults to 600 to stay above macOS service accounts (which
# cluster below 500) and the typical first local user (501).
SV_MIN_ID="${SV_MIN_ID:-600}"
next_free_id() {
    local taken
    taken=$( {
        dscl . -list /Users UniqueID
        dscl . -list /Groups PrimaryGroupID
    } | awk '{print $2}' | sort -un)
    awk -v min="$SV_MIN_ID" '
        BEGIN { next_id = min; printed = 0 }
        {
            if ($1 < next_id) next
            if ($1 == next_id) { next_id++; next }
            print next_id; printed = 1; exit
        }
        END { if (!printed) print next_id }
    ' <<< "$taken"
}

# Coarse advisory lock so concurrent `sv build` invocations on the same
# Mac do not race on ID allocation. mkdir is atomic on POSIX, so the
# first creator wins. The lock holder writes its PID; if a stale lock
# is found (PID no longer running) we steal it.
SV_ID_LOCK_DIR="/tmp/sandvault-id-alloc.lock"
acquire_id_alloc_lock() {
    local waited=0
    while ! mkdir "$SV_ID_LOCK_DIR" 2>/dev/null; do
        local holder=""
        [[ -r "$SV_ID_LOCK_DIR/pid" ]] && holder=$(cat "$SV_ID_LOCK_DIR/pid" 2>/dev/null || true)
        if [[ -n "$holder" ]] && ! kill -0 "$holder" 2>/dev/null; then
            warn "Removing stale ID-allocation lock from PID $holder"
            rm -rf "$SV_ID_LOCK_DIR"
            continue
        fi
        if (( waited >= 30 )); then
            abort "Timed out waiting for ID-allocation lock at $SV_ID_LOCK_DIR (held by PID ${holder:-unknown})"
        fi
        sleep 1
        waited=$((waited + 1))
    done
    echo "$$" > "$SV_ID_LOCK_DIR/pid"
    trap 'rm -rf "$SV_ID_LOCK_DIR"' EXIT
}
release_id_alloc_lock() {
    rm -rf "$SV_ID_LOCK_DIR"
    trap - EXIT
}

if [[ "$REBUILD" == "true" ]]; then
    debug "Creating $SANDVAULT_USER user and $SANDVAULT_GROUP group..."

    acquire_id_alloc_lock

    # Check if group exists, create if needed
    if ! dscl . -read "/Groups/$SANDVAULT_GROUP" &>/dev/null 2>&1; then
        trace "Creating $SANDVAULT_GROUP group..."
        sudo dscl . -create "/Groups/$SANDVAULT_GROUP"
        GROUP_ID=$(next_free_id)
    else
        trace "Group $SANDVAULT_GROUP already exists"
        GROUP_ID=$(dscl . -read "/Groups/$SANDVAULT_GROUP" PrimaryGroupID 2>/dev/null | awk '{print $2}')
    fi

    # Ensure group has all required properties (idempotent)
    if [[ -z "${GROUP_ID:-}" ]]; then
        # Group exists but has no PrimaryGroupID, find next available
        GROUP_ID=$(next_free_id)
    fi
    trace "Configuring $SANDVAULT_GROUP group properties (GID=$GROUP_ID)..."
    sudo dscl . -create "/Groups/$SANDVAULT_GROUP" PrimaryGroupID "$GROUP_ID"
    sudo dscl . -create "/Groups/$SANDVAULT_GROUP" RealName "$SANDVAULT_GROUP Group"

    # Check if user exists, create if needed
    if ! dscl . -read "/Users/$SANDVAULT_USER" &>/dev/null 2>&1; then
        trace "Creating $SANDVAULT_USER user..."
        sudo dscl . -create "/Users/$SANDVAULT_USER"
        USER_ID=$(next_free_id)
    else
        trace "User $SANDVAULT_USER already exists"
        USER_ID=$(dscl . -read "/Users/$SANDVAULT_USER" UniqueID 2>/dev/null | awk '{print $2}')
    fi

    # Ensure user has all required properties (idempotent)
    trace "Configuring $SANDVAULT_USER user properties (UID=$USER_ID)..."
    if [[ -z "${USER_ID:-}" ]]; then
        # User exists but has no UniqueID, find next available
        USER_ID=$(next_free_id)
    fi
    sudo dscl . -create "/Users/$SANDVAULT_USER" UniqueID "$USER_ID"
    sudo dscl . -create "/Users/$SANDVAULT_USER" PrimaryGroupID "$GROUP_ID"
    sudo dscl . -create "/Users/$SANDVAULT_USER" RealName "$SANDVAULT_USER User"
    sudo dscl . -create "/Users/$SANDVAULT_USER" NFSHomeDirectory "/Users/$SANDVAULT_USER"
    sudo dscl . -create "/Users/$SANDVAULT_USER" UserShell "/bin/zsh"

    # Set a random password for the user (password required for SSH on macOS)
    # We'll use key-based auth so the password won't actually be used.
    RANDOM_PASS=$(openssl rand -base64 32)
    sudo dscl . -passwd "/Users/$SANDVAULT_USER" "$RANDOM_PASS"
    sudo dscl . -create "/Users/$SANDVAULT_USER" IsHidden 1  # Hide from login window

    # DANGEROUS: allow login as sandvault user
    #sudo dscl . -create "/Users/$SANDVAULT_USER" IsHidden 0
    #sudo dscl . -passwd "/Users/$SANDVAULT_USER" "sandvault"

    # Remove sandvault user from "staff" group so it doesn't have access to most files.
    # On macOS this may require removing both username and GeneratedUID group entries.
    trace "Removing $SANDVAULT_USER from staff group..."
    SANDVAULT_GENERATED_UID="$(
        dscl . -read "/Users/$SANDVAULT_USER" \
	    GeneratedUID 2>/dev/null \
	    | awk '/^GeneratedUID: +/ {print $2;}' \
	    || true)"
    sudo dseditgroup -o edit -d "$SANDVAULT_USER" -t user staff 2>/dev/null || true
    if [[ -n "$SANDVAULT_GENERATED_UID" ]]; then
        sudo dscl . -delete "/Groups/staff" GroupMembers "$SANDVAULT_GENERATED_UID" 2>/dev/null || true
    fi

    sudo dscl . -delete "/Groups/staff" GroupMembership "$SANDVAULT_USER" 2>/dev/null || true
    if sudo dscl . -read "/Groups/staff" GroupMembership 2>/dev/null | grep -Eq "(^|[[:space:]])$SANDVAULT_USER($|[[:space:]])"; then
        abort "Failed to remove $SANDVAULT_USER user entry from staff group"
    fi
    if [[ -n "$SANDVAULT_GENERATED_UID" ]] && \
       sudo dscl . -read "/Groups/staff" GroupMembers 2>/dev/null | grep -Eq "(^|[[:space:]])$SANDVAULT_GENERATED_UID($|[[:space:]])"
    then
        abort "Failed to remove $SANDVAULT_USER GeneratedUID entry from staff group"
    fi

    # Add sandvault user to the sandvault group
    # PrimaryGroupID alone is insufficient for ACL and dseditgroup membership checks
    trace "Adding $SANDVAULT_USER to $SANDVAULT_GROUP group..."
    sudo dseditgroup -o edit -a "$SANDVAULT_USER" -t user "$SANDVAULT_GROUP"

    # Add host user to the sandvault group
    trace "Adding $HOST_USER to $SANDVAULT_GROUP group..."
    sudo dseditgroup -o edit -a "$HOST_USER" -t user "$SANDVAULT_GROUP"

    release_id_alloc_lock
fi


###############################################################################
# Manage SSH access
###############################################################################
if [[ "$MODE" == "ssh" && "$NESTED" == "true" ]]; then
    info "SSH not implemented for nested mode; falling back to shell"
    MODE="default"
fi

# REBUILD mode: always configure SSH
# SSH mode: check configuration available
if [[ "$REBUILD" == "true" || "$MODE" == "ssh" ]]; then
    if dscl . -read /Groups/com.apple.access_ssh &>/dev/null; then
        # Remote Login is enabled with specific users/groups; ensure sandvault user can SSH
        if ! dseditgroup -o checkmember -m "$SANDVAULT_USER" com.apple.access_ssh &>/dev/null; then
            if [[ "$NO_BUILD" == "true" ]]; then
                abort "cannot add $SANDVAULT_USER to remote access because --no-build flag set"
            fi
            trace "Adding $SANDVAULT_USER to com.apple.access_ssh group"
            # do not use sudo dscl; it creates duplicate entries
            sudo dseditgroup -o edit -a "$SANDVAULT_USER" -t user com.apple.access_ssh
        else
            trace "SSH access: $SANDVAULT_USER is already in com.apple.access_ssh group"
        fi
    elif pgrep -x sshd &>/dev/null; then
        # Remote Login is enabled for "All users" — sshd is running but
        # com.apple.access_ssh group does not exist. No group membership
        # needed since all users are already allowed.
        trace "SSH access: Remote Login enabled for all users (no group membership needed)"
    elif [[ "$MODE" == "ssh" ]]; then
        # Remote Login is disabled and SSH mode requested
        abort "Remote Login via SSH is not enabled. Enable it in System Settings → General → Sharing → Remote Login"
    else
        trace "SSH access: Remote Login is not enabled (skipping, not in SSH mode)"
    fi
fi

# SSH smoke test
#if [[ "$COMMAND" == "build" || "$REBUILD" == "true" ]]; then
#    if ssh -n -o BatchMode=yes -o ConnectTimeout=2 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -i "$SSH_KEYFILE_PRIV" "$SANDVAULT_USER@$HOSTNAME" true 2>/dev/null; then
#        trace "SSH smoke test: $SANDVAULT_USER@$HOSTNAME connected successfully"
#    else
#        warn "SSH smoke test failed: $SANDVAULT_USER@$HOSTNAME could not connect. SSH mode may not work."
#    fi
#fi


###############################################################################
# Create shared workspace directory
###############################################################################
if [[ "$REBUILD" == "true" ]]; then
    debug "Creating shared workspace at $SHARED_WORKSPACE..."
    mkdir -p "$SHARED_WORKSPACE"
    configure_shared_folder_permssions true

    # Create a README in the shared workspace
    cat > "$SHARED_WORKSPACE/SANDVAULT-README.md" << EOF
    # sandvault workspace for '$HOST_USER'
    # (autogenerated file; do not edit)

    This directory is shared with '$SANDVAULT_USER' user.
    The sandvault user has full read/write access here.

    ## To switch to sandvault:

        sv shell
EOF
fi


###############################################################################
# Configure passwordless sudo to switch to sandvault user
###############################################################################
if [[ "$REBUILD" == "true" ]]; then
    if [[ ! -d "$WORKSPACE/guest/home" ]]; then
        abort "ERROR: '$WORKSPACE/guest/home' directory not found"
    fi
    debug "Configuring passwordless access to $SANDVAULT_USER..."

heredoc SUDOERS_BUILD_HOME_SCRIPT_CONTENTS << EOF
#!/bin/bash
set -Eeuo pipefail
trap 'echo "\${BASH_SOURCE[0]}: line \$LINENO: \$BASH_COMMAND: exitcode \$?"' ERR

# Verify preconditions
if [[ ! -d "$WORKSPACE/guest/home" ]]; then
    echo >&2 "ERROR: '$WORKSPACE/guest/home' directory not found"
    exit 1
fi

# Copy files to home directory
sudo mkdir -p "/Users/$SANDVAULT_USER"
sudo chown "$SANDVAULT_USER:$SANDVAULT_GROUP" "/Users/$SANDVAULT_USER"
sudo /bin/chmod 0750 "/Users/$SANDVAULT_USER"

# Copy files preserving permissions for contents only
# Use the system rsync to avoid Homebrew dependencies.
# Perform rsync from within the destination directory so that the paths
# passed through xargs to chown are correct.
#
# Collect changed files first, then chown only if rsync reported changes.
# macOS xargs lacks --no-run-if-empty so we guard against empty input.
#
# Use OSX chown, not GNU chown, because I've tested that it works
cd "/Users/$SANDVAULT_USER/"
_changed=\$(/usr/bin/rsync \
    --itemize-changes \
    --out-format="%n" \
    --links \
    --copy-unsafe-links \
    --checksum \
    --recursive \
    --perms \
    --times \
    "$WORKSPACE/guest/home/." \
    ".")
if [[ -n "\$_changed" ]]; then
    echo "\$_changed" | tr '\n' '\0' \
        | xargs -0 sudo /usr/sbin/chown "$SANDVAULT_USER:$SANDVAULT_GROUP"
fi
EOF
    sudo mkdir -p "$(dirname "$SUDOERS_BUILD_HOME_SCRIPT_NAME")"
    trace "Setting $(dirname "$SUDOERS_BUILD_HOME_SCRIPT_NAME") to 0755 (world-traversable)"
    sudo /bin/chmod 0755 "$(dirname "$SUDOERS_BUILD_HOME_SCRIPT_NAME")"
    # shellcheck disable=SC2154 # SUDOERS_BUILD_HOME_SCRIPT_CONTENTS is referenced but not assigned (yes it is)
    echo "$SUDOERS_BUILD_HOME_SCRIPT_CONTENTS" | sudo tee "$SUDOERS_BUILD_HOME_SCRIPT_NAME" > /dev/null
    sudo /bin/chmod 0554 "$SUDOERS_BUILD_HOME_SCRIPT_NAME"

    # Get the sandvault user's UID
    SANDVAULT_UID=$(dscl . -read "/Users/$SANDVAULT_USER" UniqueID 2>/dev/null | awk '{print $2}')

heredoc SUDOERS_CONTENT << EOF
# Allow $HOST_USER to run these commands as $SANDVAULT_USER without password
$HOST_USER ALL=($SANDVAULT_USER) NOPASSWD: /bin/zsh
$HOST_USER ALL=($SANDVAULT_USER) NOPASSWD: /usr/bin/env
$HOST_USER ALL=($SANDVAULT_USER) NOPASSWD: /usr/bin/true

# Allow $HOST_USER to run $SUDOERS_BUILD_HOME_SCRIPT_NAME to sync home directory
$HOST_USER ALL=(root) NOPASSWD: $SUDOERS_BUILD_HOME_SCRIPT_NAME

# Allow $HOST_USER to kill $SANDVAULT_USER processes without password
$HOST_USER ALL=(root) NOPASSWD: /bin/launchctl bootout user/$SANDVAULT_UID
$HOST_USER ALL=(root) NOPASSWD: /usr/bin/pkill -9 -u $SANDVAULT_USER
EOF

    # Write to a root-owned temp file, validate, then atomically move into place.
    SUDOERS_TMP="$(sudo /usr/bin/mktemp "$(dirname "$SUDOERS_FILE")/.sudoers.XXXXXXXX")"
    # shellcheck disable=SC2154 # SUDOERS_CONTENT is referenced but not assigned (yes it is)
    echo "$SUDOERS_CONTENT" | sudo tee "$SUDOERS_TMP" > /dev/null
    sudo /bin/chmod 0444 "$SUDOERS_TMP"

    if sudo visudo -c -f "$SUDOERS_TMP" &>/dev/null; then
        sudo /bin/mv -f "$SUDOERS_TMP" "$SUDOERS_FILE"
    else
        error "Failed to create valid sudoers file"
        sudo rm -f "$SUDOERS_TMP"
        abort "Sudoers configuration failed"
    fi
fi


###############################################################################
# Configure sandbox-exec
###############################################################################
if [[ "$NESTED" == "true" ]]; then
    : # sandbox-exec already configured
else
    if [[ "$NO_BUILD" == "true" && ! -f "$SANDBOX_PROFILE" ]]; then
        abort "Sandbox profile is missing; cannot rebuild because --no-build set"
    fi
    if [[ "$REBUILD" == "true" || ! -f "$SANDBOX_PROFILE" ]]; then
        debug "Configuring passwordless access to $SANDVAULT_USER..."

        # Create sandbox profile to restrict /Volumes access, which prevents
        # sandvault user from modifying removable drives. Issue discovered by
        # by Github user redLocomotive.
        #
        # The profile file is owned by root so sandvault user cannot modify it.
        debug "Creating sandbox profile..."
heredoc SANDBOX_PROFILE_CONTENT << EOF
;; Sandbox profile for sandvault
(version 1)
(allow default)

;; restrict writes to everything
(deny file-write*
    (subpath "/"))

;; restrict reads to Volumes to prevent access to removable disks,
;; but ensure the main disk is readable via Volumes.
(deny file-read*
    (subpath "/Volumes"))
(allow file-read*
    (subpath "/Volumes/Macintosh HD"))

;; Allow writes to sandvault home, shared workspace, temporary directories.
;; Allow writes to devices, which are protected by unix permissions
(allow file-write*
    (subpath "$SHARED_WORKSPACE")
    (subpath "/Users/$SANDVAULT_USER")
    (subpath "/tmp")
    (subpath "/private/tmp")
    (subpath "/var/folders")
    (subpath "/private/var/folders")
    (subpath "/dev"))

;; allow basic process info queries
(allow process-info*)
(allow sysctl-read)

;; allow process exec/fork (ps may require process info access)
(allow process*)

;; allow /bin/ps (setuid) to run without sandbox restrictions
(allow process-exec
    (literal "/bin/ps")
    (with no-sandbox))
EOF
        # shellcheck disable=SC2154
        echo "$SANDBOX_PROFILE_CONTENT" | sudo tee "$SANDBOX_PROFILE" > /dev/null
        sudo /bin/chmod 0444 "$SANDBOX_PROFILE"
    fi
fi


###############################################################################
# Create passwordless SSH key with permission to remotely login to guest
###############################################################################
if [[ "$NESTED" == "true" ]]; then
    : # SSH already configured
else
    configure_ssh_access
fi


###############################################################################
# Write config merge scripts to shared workspace
#
# These scripts run as the sandbox user during session startup (called by
# configure). They merge required settings into existing config files rather
# than overwriting them, so agent/tool modifications persist across sessions.
###############################################################################
if [[ "$NESTED" == "true" ]]; then
    : # Already configured
elif [[ "$REBUILD" == "true" ]]; then
    trace "Writing config merge scripts..."
    GIT_USER_NAME=$(git config --global --get user.name 2>/dev/null || echo "")
    GIT_USER_EMAIL=$(git config --global --get user.email 2>/dev/null || echo "")

    mkdir -p "$SHARED_WORKSPACE/setup"

    # .gitconfig: seed if missing, preserving user overrides
    cat > "$SHARED_WORKSPACE/setup/gitconfig" << SETUP_EOF
#!/bin/bash
set -Eeuo pipefail
if [[ ! -f "\$HOME/.gitconfig" ]]; then
    git config -f "\$HOME/.gitconfig" user.name "$GIT_USER_NAME"
    git config -f "\$HOME/.gitconfig" user.email "$GIT_USER_EMAIL"
    git config -f "\$HOME/.gitconfig" safe.directory "$SHARED_WORKSPACE/*"
fi
SETUP_EOF
    chmod +x "$SHARED_WORKSPACE/setup/gitconfig"

    # .claude.json: seed if missing (onboarding flags only matter on first run)
    cat > "$SHARED_WORKSPACE/setup/claude-json" << 'SETUP_EOF'
#!/bin/bash
set -Eeuo pipefail
if [[ ! -f "$HOME/.claude.json" ]]; then
    cat > "$HOME/.claude.json" << 'JSON_EOF'
{
  "hasCompletedOnboarding": true,
  "bypassPermissionsModeAccepted": true,
  "tipsHistory": {
    "new-user-warmup": 1
  }
}
JSON_EOF
fi
SETUP_EOF
    chmod +x "$SHARED_WORKSPACE/setup/claude-json"

    # agentsview-export: ensure each agent's session subdir exists with an
    # ACL granting the sandvault group read+inherit, so JSONL files written
    # by sandbox agents are readable by the host user through the symlinks
    # under $SHARED_WORKSPACE/sessions/. Acts as a no-op unless the host
    # has opted in (state file enabled).
    cat > "$SHARED_WORKSPACE/setup/agentsview-export" << SETUP_EOF
#!/bin/bash
set -Eeuo pipefail
STATE_FILE="$AGENTSVIEW_STATE_FILE"
if [[ ! -f "\$STATE_FILE" ]] || [[ "\$(cat "\$STATE_FILE")" != "enabled" ]]; then
    exit 0
fi
RIGHTS="group:$SANDVAULT_GROUP allow read,readattr,readextattr,readsecurity,search,list,file_inherit,directory_inherit"
for subdir in "$AGENTSVIEW_SUBDIR_claude" "$AGENTSVIEW_SUBDIR_codex" "$AGENTSVIEW_SUBDIR_opencode" "$AGENTSVIEW_SUBDIR_gemini"; do
    full="\$HOME/\$subdir"
    mkdir -p "\$full"
    /bin/chmod +a "\$RIGHTS" "\$full" 2>/dev/null || true
done
SETUP_EOF
    chmod +x "$SHARED_WORKSPACE/setup/agentsview-export"

    # Ask the host user about agentsview export (idempotent; prompts only once)
    agentsview_setup
fi


###############################################################################
# Copy guest/home/. to sandvault $HOME
###############################################################################
if [[ "$NESTED" == "true" ]]; then
    : # Home already configured
elif [[ "$NO_BUILD" == "false" ]]; then
    debug "Configure $SANDVAULT_USER home directory..."
    sudo "$SUDOERS_BUILD_HOME_SCRIPT_NAME"
fi


###############################################################################
# Mark installation as complete
###############################################################################
if [[ "$NESTED" == "true" ]]; then
    : # Marker already configured
elif [[ "$NO_BUILD" == "true" ]]; then
    if [[ ! -f "$INSTALL_MARKER" ]]; then
        abort "--no-build set: install marker is missing"
    fi
else
    debug "Creating installation marker..."
    mkdir -p "$(dirname "$INSTALL_MARKER")"
    date > "$INSTALL_MARKER"
fi


###############################################################################
# Locate user configuration directory for configure script
###############################################################################
# Place custom configuration files in $SHARED_WORKSPACE/user/ — this directory
# is accessible to both $HOST_USER and $SANDVAULT_USER.
if [[ -d "$WORKSPACE/guest/home/user" ]]; then
    abort "Storing user configuration in 'guest/home/user/' is no longer supported. Move it to the shared workspace instead:\n\n  /usr/bin/rsync --archive --remove-source-files '$WORKSPACE/guest/home/user/' '$SHARED_WORKSPACE/user/' && rmdir '$WORKSPACE/guest/home/user'"
fi


###############################################################################
# Fix permissions (runs with --fix-permissions regardless of --rebuild)
###############################################################################
if [[ "$FIX_PERMISSIONS" == "true" ]]; then
    # /var/sandvault/ must be world-traversable for sandvault user to read sandbox profile
    SV_DIR="$(dirname "$SUDOERS_BUILD_HOME_SCRIPT_NAME")"
    if [[ -d "$SV_DIR" ]]; then
        sv_dir_perms=$(/usr/bin/stat -f "%Lp" "$SV_DIR")
        if [[ "$sv_dir_perms" != "755" ]]; then
            debug "Fixing $SV_DIR permissions: $sv_dir_perms -> 0755"
            sudo /bin/chmod 0755 "$SV_DIR"
        else
            debug "$SV_DIR permissions ok (0755)"
        fi
    fi

    # Fix homebrew symlinks for any installed tools
    # shellcheck disable=SC2310 # brew_shellenv intentionally used in condition
    if brew_shellenv 2>/dev/null; then
        for tool_cli in claude codex opencode gemini; do
            brew_link="$(brew --prefix)/bin/$tool_cli"
            if [[ -L "$brew_link" ]]; then
                link_perms=$(/usr/bin/stat -f "%Lp" "$brew_link")
                if [[ "$((8#$link_perms & 8#0005))" -eq 0 ]]; then
                    debug "Fixing symlink permissions: $brew_link ($link_perms -> 0755)"
                    /bin/chmod -h 0755 "$brew_link"
                else
                    debug "Symlink permissions ok: $brew_link ($link_perms)"
                fi
            fi
        done

        # Check homebrew bin directory
        brew_bin_dir="$(brew --prefix)/bin"
        if [[ -d "$brew_bin_dir" ]]; then
            dir_perms=$(/usr/bin/stat -f "%Lp" "$brew_bin_dir")
            if [[ "$((8#$dir_perms & 8#0005))" -eq 0 ]]; then
                warn "Homebrew bin directory ($brew_bin_dir) has restrictive permissions ($dir_perms). Run: sudo chmod -R o+rX $(brew --prefix)"
            else
                debug "Homebrew bin directory permissions ok ($dir_perms)"
            fi
        fi
    fi

    debug "Permissions check complete"
fi

# Install the Claude Code /sv skill when Claude Code is present. The skill
# lives under the sandvault namespace (~/.claude/skills/sandvault) so it
# won't collide with any other skill named "sv". Use /bin/ln explicitly —
# GNU coreutils `ln` on PATH (e.g. via Homebrew) has incompatible flag
# handling that has bitten this project before.
SV_SKILL_SOURCE="$WORKSPACE/skills/sandvault/sv"
SV_SKILL_DEST="$HOME/.claude/skills/sandvault-sv"
if [[ ! -L "$SV_SKILL_DEST" ]]; then
    mkdir -p "$(dirname "$SV_SKILL_DEST")"
    /bin/ln -sfn "$SV_SKILL_SOURCE" "$SV_SKILL_DEST"
    debug "Installed /sv skill symlink at $SV_SKILL_DEST"
fi

if [[ "$COMMAND" == "build" ]]; then
    exit 0
fi


###############################################################################
# Verify permissions before running
###############################################################################
SV_DIR="$(dirname "$SANDBOX_PROFILE")"
if [[ -d "$SV_DIR" ]]; then
    sv_dir_perms=$(/usr/bin/stat -f "%Lp" "$SV_DIR")
    if [[ "$((8#$sv_dir_perms & 8#0005))" -eq 0 ]]; then
        warn "$SV_DIR has restrictive permissions ($sv_dir_perms). Run: sv --fix-permissions"
    fi
fi
if [[ -f "$SANDBOX_PROFILE" && ! -r "$SANDBOX_PROFILE" ]]; then
    warn "Cannot read sandbox profile ($SANDBOX_PROFILE). Run: sv --fix-permissions"
fi


# kitty doesn't set this properly :(
TERM_PROGRAM="${TERM_PROGRAM:-e.g. ghostty, kitty, iTerm, WezTerm}"
heredoc LOCAL_NETWORK_ERROR << EOF
\n
ERROR: unable to connect to $HOSTNAME.

Your terminal app ($TERM_PROGRAM)
has not been granted "Local Network" access rights,
which are required to SSH to the Virtual Machine.

- Open "System Settings.app"
- Navigate to "Privacy & Security"
- Select "Local Network"
- Grant access to your terminal application
\n
EOF

# Register this session and set up trap to unregister on exit
if [[ "$NESTED" == "false" ]]; then
    sv_exit_code=0
    register_session
    trap 'sv_exit_code=$?; set +e; unregister_session; exit $sv_exit_code' EXIT
    if [[ "$USE_BROWSER" == "true" ]]; then
        start_chrome
    fi
    if [[ "$USE_IOS_SIMULATOR" == "true" ]]; then
        start_ios_simulator
    fi
else
    if [[ "$USE_BROWSER" == "true" && -z "${SV_BROWSER_ENDPOINT:-}" ]]; then
        # Nested session with --browser: Chrome cannot be launched inside the
        # sandbox, so the parent session must already have started it.
        abort "--browser requires Chrome, but the parent sandvault session was not started with --browser"
    fi
    if [[ "$USE_IOS_SIMULATOR" == "true" && -z "${SV_IOS_SIMULATOR_ENDPOINT:-}" ]]; then
        # Nested session with --ios: the simulator runs on the host,
        # so the parent session must already have started it.
        abort "--ios requires a simulator, but the parent sandvault session was not started with --ios"
    fi
fi

# TMPDIR: claude (and perhaps other AI agents) creates temporary directories in locations
# that are shared, e.g. /tmp/claude and /private/tmp/claude, which doesn't work when there
# are multiple users running the agent on the same computer.
# Fix: set TMPDIR after the shell has started running.
#
# Source login files explicitly in a single zsh -c invocation rather than spawning
# an intermediate "zsh --login". This avoids double-sourcing .zshenv and ensures
# piped stdin passes through to the final process (an interactive login shell would
# consume stdin as commands).
ZSH_COMMAND="export TMPDIR=\$(mktemp -d); cd ~; source ~/.zshenv; source ~/.zprofile; source ~/.zshrc"

# Prepare command args as a single string
COMMAND_ARGS_STR=""
SHELL_COMMAND_MODE=false
if [[ ${#COMMAND_ARGS[@]} -gt 0 ]]; then
    printf -v COMMAND_ARGS_STR '%q ' "${COMMAND_ARGS[@]}"

    # When the user requests running shell (instead of an AI agent) then convert
    # COMMAND_ARGS to a command that will be run by the shell.
    #
    # Example: sv shell -- echo foo
    # Runs:    source ~/.zshenv; source ~/.zprofile; source ~/.zshrc; echo foo
    if [[ "$COMMAND" == "" ]]; then
        ZSH_COMMAND="$ZSH_COMMAND; ${COMMAND_ARGS_STR}"
        COMMAND_ARGS_STR=""
        SHELL_COMMAND_MODE=true
    fi
fi

# When COMMAND is set, .zshrc will exec it — no need to append anything.
# When running a shell command (sv shell -- echo foo), it was already appended above.
# For interactive shells (sv shell with no args), drop into a real interactive zsh
# only when stdin is a TTY. When stdin is piped (e.g. `echo cmd | sv s`), exec a
# non-interactive zsh that reads commands from stdin without printing prompts or
# triggering interactive-only hooks like direnv.
# Use -i (not --login) to avoid re-sourcing the login files.
if [[ -z "$COMMAND" && "$SHELL_COMMAND_MODE" == "false" ]]; then
    if [[ -t 0 ]]; then
        ZSH_COMMAND="$ZSH_COMMAND; exec /bin/zsh -i"
    else
        ZSH_COMMAND="$ZSH_COMMAND; exec /bin/zsh"
    fi
fi

SANDBOX_EXEC=()
if [[ "$USE_SANDBOX" == "true" ]]; then
    SANDBOX_EXEC=(/usr/bin/sandbox-exec -f "$SANDBOX_PROFILE")
else
    debug "Sandbox disabled: running without sandbox-exec restrictions"
fi

# Extra environment variables (e.g. browser CDP endpoint, iOS bridge
# endpoint, native install flag). These are expanded into the "/usr/bin/env
# -i ..." argv below so they survive sudo/SSH and the env scrubber, which
# is how nested `sv --browser` / `sv --ios` invocations inherit
# SV_BROWSER_ENDPOINT / SV_IOS_SIMULATOR_ENDPOINT from the parent session.
EXTRA_ENV=()
if [[ "$NATIVE_INSTALL" == "true" ]]; then
    EXTRA_ENV+=("SV_NATIVE_INSTALL=true")
fi
if [[ "$USE_BROWSER" == "true" ]]; then
    if [[ -n "${SV_BROWSER_ENDPOINT:-}" ]]; then
        EXTRA_ENV+=("SV_BROWSER_ENDPOINT=$SV_BROWSER_ENDPOINT")
    elif [[ -n "$CHROME_PORT" ]]; then
        EXTRA_ENV+=("SV_BROWSER_ENDPOINT=http://127.0.0.1:$CHROME_PORT")
    else
        abort "Chrome port not available. Chrome may have failed to start."
    fi
fi
if [[ "$USE_IOS_SIMULATOR" == "true" ]]; then
    if [[ -n "${SV_IOS_SIMULATOR_ENDPOINT:-}" ]]; then
        EXTRA_ENV+=("SV_IOS_SIMULATOR_ENDPOINT=$SV_IOS_SIMULATOR_ENDPOINT")
    elif [[ -n "$IOS_BRIDGE_PORT" ]]; then
        EXTRA_ENV+=("SV_IOS_SIMULATOR_ENDPOINT=http://127.0.0.1:$IOS_BRIDGE_PORT")
    else
        abort "iOS bridge port not available. Bridge may have failed to start."
    fi
fi
if [[ -n "${COLORTERM:-}" ]]; then
    EXTRA_ENV+=("COLORTERM=$COLORTERM")
fi

if [[ "$MODE" == "ssh" ]]; then
    # Only allocate a TTY for interactive shells.
    SSH_TTY_OPT="-t"
    if [[ ! -t 0 || "$SHELL_COMMAND_MODE" == "true" ]]; then
        SSH_TTY_OPT="-T"
    fi

    # Escape single quotes for a remote shell context.
    ZSH_COMMAND_SSH=$(printf '%s' "$ZSH_COMMAND" | sed "s/'/'\"'\"'/g")
    ZSH_COMMAND_SSH="'$ZSH_COMMAND_SSH'"

    trace "Checking SSH connectivity"
    if ! ssh_check_output=$(ssh \
        -o BatchMode=yes \
        -o ConnectTimeout=2 \
        -o ConnectionAttempts=1 \
        -o LogLevel=ERROR \
        -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null \
        -n \
        -i "$SSH_KEYFILE_PRIV" \
        "$SANDVAULT_USER@$HOSTNAME" \
        exit 0 2>&1)
    then
        if echo "$ssh_check_output" | /usr/bin/grep -qiE "permission denied|authentication failed"; then
            error "SSH authentication failed for $SANDVAULT_USER@$HOSTNAME."
            error "Verify your SSH key is installed and authorized on the host."
        else
            # shellcheck disable=SC2154 # LOCAL_NETWORK_ERROR is referenced but not assigned (yes it is)
            error "$LOCAL_NETWORK_ERROR"
            debug "$ssh_check_output"
            read -n 1 -s -r -p "Press any key to open System Settings"
            open "/System/Library/PreferencePanes/Security.prefPane"
        fi
        exit 1
    fi

    debug "SSH $SANDVAULT_USER@$HOSTNAME"

    # SSH requires TWO layers of shell parsing: local shell → SSH → remote shell → /bin/zsh
    # The extra single quotes protect the command through SSH's remote shell parsing.
    # Without them, the remote shell would word-split the command, causing incorrect execution.
    # Example: "'export TMPDIR=...'" becomes a single arg after local expansion, then the remote
    # shell strips the outer quotes, passing 'export TMPDIR=...' correctly to /bin/zsh -c
    exec ssh \
        -q \
        "$SSH_TTY_OPT" \
        -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null \
        -i "$SSH_KEYFILE_PRIV" \
        "$SANDVAULT_USER@$HOSTNAME" \
        /usr/bin/env -i \
            "HOME=/Users/$SANDVAULT_USER" \
            "USER=$SANDVAULT_USER" \
            "SHELL=/bin/zsh" \
            "TERM=${TERM:-}" \
            "COMMAND=$COMMAND" \
            "COMMAND_ARGS=$COMMAND_ARGS_STR" \
            "INITIAL_DIR=$INITIAL_DIR" \
            "SHARED_WORKSPACE=$SHARED_WORKSPACE" \
            "SV_SESSION_ID=$SV_SESSION_ID" \
            "SV_VERBOSE=$SV_VERBOSE" \
            "VERBOSE=${VERBOSE:-}" \
            "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
            "${EXTRA_ENV[@]+"${EXTRA_ENV[@]}"}" \
            "${SANDBOX_EXEC[@]+"${SANDBOX_EXEC[@]}"}" \
            /bin/zsh -c "$ZSH_COMMAND_SSH"
else

    LAUNCHER=()
    if [[ "$NESTED" == "true" ]]; then
        : # No launcher required
    else
        # Verify passwordless sudo is working
        trace "Checking passwordless sudo"
        if ! sudo --non-interactive --user="$SANDVAULT_USER" /usr/bin/true ; then
            error "Passwordless sudo to $SANDVAULT_USER user is not configured correctly."
            error "Please run: ${BASH_SOURCE[0]} build --rebuild"
            exit 1
        fi

        # Launch using sudo
        LAUNCHER+=("sudo" "--login" "--set-home" "--user=$SANDVAULT_USER")
    fi

    # Launch interactive shell as sandvault user
    # Use sudo with -H to set HOME correctly
    # Use env to ensure the environment is cleared, otherwise PATH carries over
    # Use sandbox-exec to restrict access to external drives
    debug "Shell $SANDVAULT_USER@$HOSTNAME"

    # sudo requires only ONE layer of shell parsing: local shell → /bin/zsh
    # Simple double quotes "$ZSH_COMMAND" are sufficient because sudo passes arguments
    # directly to the command without an intermediate shell parsing layer.
    # This is different from SSH (see above) which requires extra quoting.
    exec "${LAUNCHER[@]+"${LAUNCHER[@]}"}" \
        /usr/bin/env -i \
            "HOME=/Users/$SANDVAULT_USER" \
            "USER=$SANDVAULT_USER" \
            "SHELL=/bin/zsh" \
            "TERM=${TERM:-}" \
            "COMMAND=$COMMAND" \
            "COMMAND_ARGS=$COMMAND_ARGS_STR" \
            "INITIAL_DIR=$INITIAL_DIR" \
            "SHARED_WORKSPACE=$SHARED_WORKSPACE" \
            "SV_SESSION_ID=$SV_SESSION_ID" \
            "SV_VERBOSE=$SV_VERBOSE" \
            "VERBOSE=${VERBOSE:-}" \
            "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
            "${EXTRA_ENV[@]+"${EXTRA_ENV[@]}"}" \
            "${SANDBOX_EXEC[@]+"${SANDBOX_EXEC[@]}"}" \
            /bin/zsh -c "$ZSH_COMMAND"
fi
