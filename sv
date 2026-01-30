#!/usr/bin/env bash
# Build a sandbox user ("sandvault") for running commands
set -Eeuo pipefail
trap 'echo "${BASH_SOURCE[0]}: line $LINENO: $BASH_COMMAND: exitcode $?"' ERR
SCRIPT_DIR="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}" 2>/dev/null || echo "${BASH_SOURCE[0]}")")" && pwd)"


###############################################################################
# Functions
###############################################################################
[[ "${VERBOSE:-0}" =~ ^[0-9]+$ ]] && VERBOSE="${VERBOSE:-0}" || VERBOSE=1
trace () {
    [[ "$VERBOSE" -lt 2 ]] || echo >&2 -e "🔬 \033[90m$*\033[0m"
}
debug () {
    [[ "$VERBOSE" -lt 1 ]] || echo >&2 -e "🔍 \033[36m$*\033[0m"
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
readonly VERSION="1.1.13"

# Each user on the computer can have their own sandvault
readonly SANDVAULT_USER="sandvault-$USER"
readonly SANDVAULT_GROUP="sandvault-$USER"
readonly SHARED_WORKSPACE="/Users/Shared/sv-$USER"
readonly SANDVAULT_RIGHTS="group:$SANDVAULT_GROUP allow read,write,append,delete,delete_child,readattr,writeattr,readextattr,writeextattr,readsecurity,writesecurity,chown,file_inherit,directory_inherit"

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
    if command -v "$cli_name" &>/dev/null; then
        return 0
    fi
    ensure_brew
    debug "Installing $tool with Homebrew..."
    if [[ "$VERBOSE" -lt 3 ]]; then
        brew install --quiet "$tool"
    else
        brew install "$tool"
    fi
    if command -v "$cli_name" &>/dev/null; then
        return 0
    fi
    warn "Homebrew installed $tool, but no '$cli_name' CLI was found in PATH. Will use \$HOME/node_modules/bin/$cli_name if present."
    return 0
}

install_tools () {
    # Install homebrew tools only when the user invokes them.
    case "${COMMAND:-}" in
        claude)
            ensure_brew_tool "claude-code" "claude"
            ;;
        codex)
            ensure_brew_tool "codex" "codex"
            ;;
        gemini)
            ensure_brew_tool "gemini-cli" "gemini"
            ;;
        *)
            # No tool installation needed for other commands
            ;;
    esac
}

resolve_workspace() {
    if [[ -d "$SCRIPT_DIR/.git" ]] || [[ -f "$SCRIPT_DIR/.git" ]] ; then
        # Development mode: running from a git repo
        if [[ -d "$SCRIPT_DIR/guest/home" ]]; then
            echo "$SCRIPT_DIR"
            return 0
        fi
    else
        # Homebrew opt symlink
        local candidate
        candidate="$(brew --prefix sandvault)"
        if [[ -d "$candidate/guest/home" ]]; then
            echo "$candidate"
            return 0
        fi
    fi

    abort "Unable to find sandvault guest/home directory"
}

force_cleanup_sandvault_processes() {
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
        trace "Last session exited; cleaning up sandvault processes"
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
    if [[ "$enable" != "false" ]]; then
        # Make workspace accessible to $USER and $SANDVAULT_GROUP only
        trace "Configuring $SHARED_WORKSPACE permissions..."
        sudo /bin/chmod 0770 "$SHARED_WORKSPACE"
        trace "Configuring $SHARED_WORKSPACE: set owner to $USER:$SANDVAULT_GROUP"
        sudo /usr/sbin/chown -f -R "$USER:$SANDVAULT_GROUP" "$SHARED_WORKSPACE"
        trace "Configuring $SHARED_WORKSPACE: add $SANDVAULT_RIGHTS (recursively)"
        sudo find "$SHARED_WORKSPACE" -print0 | xargs -0 sudo /bin/chmod -h +a "$SANDVAULT_RIGHTS"
    else
        # Make workspace accessible to $USER only
        trace "Configuring $SHARED_WORKSPACE permissions..."
        sudo /bin/chmod 0700 "$SHARED_WORKSPACE"
        trace "Configuring $SHARED_WORKSPACE: restoring owner to $USER:$(id -gn)"
        sudo /usr/sbin/chown -f -R "$USER:$(id -gn)" "$SHARED_WORKSPACE"
        trace "Configuring $SHARED_WORKSPACE: remove $SANDVAULT_RIGHTS (recursively)"
        sudo find "$SHARED_WORKSPACE" -print0 2>/dev/null | xargs -0 sudo /bin/chmod -h -a "$SANDVAULT_RIGHTS" 2>/dev/null || true
    fi
}

uninstall() {
    debug "Uninstalling..."
    force_cleanup_sandvault_processes

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

    # Remove current user from sandvault group
    debug "Removing user and group..."
    sudo dseditgroup -o edit -d "$USER" -t user "$SANDVAULT_GROUP" 2>/dev/null || true

    # Remove sandvault user from SSH group BEFORE deleting the user
    sudo dseditgroup -o edit -d "$SANDVAULT_USER" -t user com.apple.access_ssh 2>/dev/null || true

    # Now delete the user and group
    sudo dscl . -delete "/Users/$SANDVAULT_USER" &>/dev/null || true
    sudo dscl . -delete "/Groups/$SANDVAULT_GROUP" &>/dev/null || true
    sudo rm -rf "/Users/$SANDVAULT_USER"

    # Cleanup SSH
    rm -rf "$SSH_KEYFILE_PRIV" "$SSH_KEYFILE_PUB"

    # Remove shared workspace
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
MODE=shell
COMMAND_ARGS=()

show_help() {
    appname=$(basename "${BASH_SOURCE[0]}")
    echo "Usage: $appname [options] command [-- args...]"
    echo ""
    echo "Options:"
    echo "  -s, --ssh            Use SSH to connect"
    echo "  -r, --rebuild        Rebuild all files & configuration"
    echo "  -v, --verbose        Enable verbose output (repeat for more verbosity)"
    echo "  -vv                  Set verbosity level 2"
    echo "  -vvv                 Set verbosity level 3"
    echo "  -h, --help           Show this help message"
    echo "  --version            Show version information"
    echo ""
    echo "Commands:"
    echo "  cl, claude [PATH]    Open Claude Code in sandvault"
    echo "  co, codex  [PATH]    Open OpenAI Codex in sandvault"
    echo "  g,  gemini [PATH]    Open Google Gemini in sandvault"
    echo "  s, shell   [PATH]    Open shell in sandvault"
    echo "  b, build             Build sandvault"
    echo "  u, uninstall         Remove sandvault; keep shared files"
    echo ""
    echo "Arguments after -- are passed to command (claude, gemini, codex)"
    exit 0
}

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
            ((VERBOSE++)) || true
            shift
            ;;
        -vv)
            ((VERBOSE+=2)) || true
            shift
            ;;
        -vvv)
            ((VERBOSE+=3)) || true
            shift
            ;;
        -h|--help)
            show_help
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
        uninstall
        exit 0
        ;;
    *)
        show_help
        ;;
esac
INITIAL_DIR="${INITIAL_DIR:-$PWD}"

# Resolve symlinks to get the real path
INITIAL_DIR="$(cd "$INITIAL_DIR" 2>/dev/null && pwd -P || echo "$INITIAL_DIR")"


###############################################################################
# Setup
###############################################################################
install_tools

WORKSPACE="$(resolve_workspace)"
readonly WORKSPACE


###############################################################################
# Determine whether configuration is already complete
###############################################################################
if [[ ! -f "$INSTALL_MARKER" ]]; then
    # Since this is a full rebuild, provide more feedback
    VERBOSE=$(( VERBOSE > 1 ? VERBOSE : 1 ))
    REBUILD=true
fi

if [[ "$REBUILD" != "false" ]]; then
    info "Installing sandvault..."
    sudo "-p Password required to create sandvault: " true
fi


###############################################################################
# Create sandvault user and group
###############################################################################
if [[ "$REBUILD" != "false" ]]; then
    debug "Creating $SANDVAULT_USER user and $SANDVAULT_GROUP group..."

    # Check if group exists, create if needed
    if ! dscl . -read "/Groups/$SANDVAULT_GROUP" &>/dev/null 2>&1; then
        trace "Creating $SANDVAULT_GROUP group..."

        # Find next available UID/GID starting from 501
        NEXT_UID=$(dscl . -list /Users UniqueID | awk '{print $2}' | sort -n | tail -1)
        NEXT_UID=$((NEXT_UID + 1))

        # Create group
        sudo dscl . -create "/Groups/$SANDVAULT_GROUP"
        GROUP_ID=$NEXT_UID
    else
        trace "Group $SANDVAULT_GROUP already exists"
        GROUP_ID=$(dscl . -read "/Groups/$SANDVAULT_GROUP" PrimaryGroupID 2>/dev/null | awk '{print $2}')
    fi

    # Ensure group has all required properties (idempotent)
    if [[ -z "${GROUP_ID:-}" ]]; then
        # Group exists but has no PrimaryGroupID, find next available
        NEXT_UID=$(dscl . -list /Users UniqueID | awk '{print $2}' | sort -n | tail -1)
        GROUP_ID=$((NEXT_UID + 1))
    fi
    trace "Configuring $SANDVAULT_GROUP group properties..."
    sudo dscl . -create "/Groups/$SANDVAULT_GROUP" PrimaryGroupID "$GROUP_ID"
    sudo dscl . -create "/Groups/$SANDVAULT_GROUP" RealName "$SANDVAULT_GROUP Group"

    # Check if user exists, create if needed
    if ! dscl . -read "/Users/$SANDVAULT_USER" &>/dev/null 2>&1; then
        trace "Creating $SANDVAULT_USER user..."

        # Find next available UID
        NEXT_UID=$(dscl . -list /Users UniqueID | awk '{print $2}' | sort -n | tail -1)
        NEXT_UID=$((NEXT_UID + 1))

        # Create user
        sudo dscl . -create "/Users/$SANDVAULT_USER"
        USER_ID=$NEXT_UID
    else
        trace "User $SANDVAULT_USER already exists"
        USER_ID=$(dscl . -read "/Users/$SANDVAULT_USER" UniqueID 2>/dev/null | awk '{print $2}')
    fi

    # Ensure user has all required properties (idempotent)
    trace "Configuring $SANDVAULT_USER user properties..."
    if [[ -z "${USER_ID:-}" ]]; then
        # User exists but has no UniqueID, find next available
        NEXT_UID=$(dscl . -list /Users UniqueID | awk '{print $2}' | sort -n | tail -1)
        USER_ID=$((NEXT_UID + 1))
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

    # Remove sandvault user from "staff" group so it doesn't have access to most files
    sudo dseditgroup -o edit -d "$SANDVAULT_USER" -t user staff 2>/dev/null || true

    # Add current user to the sandvault group
    trace "Adding $USER to $SANDVAULT_GROUP group..."
    sudo dseditgroup -o edit -a "$USER" -t user "$SANDVAULT_GROUP"
fi


###############################################################################
# Manage SSH access
###############################################################################
# REBUILD mode: always configure
# SSH mode: must configure (core functionality)
if [[ "$REBUILD" != "false" ]] || [[ "$MODE" == "ssh" ]]; then
    if dscl . -read /Groups/com.apple.access_ssh &>/dev/null; then
        # Remote Login is enabled; ensure sandvault user can SSH
        if ! dseditgroup -o checkmember -m "$SANDVAULT_USER" com.apple.access_ssh &>/dev/null; then
            # do not use sudo dscl; it creates duplicate entries
            sudo dseditgroup -o edit -a "$SANDVAULT_USER" -t user com.apple.access_ssh
        fi
    elif [[ "$MODE" == "ssh" ]]; then
        # Remote Login is disabled and SSH mode requested
        abort "Remote Login via SSH is not enabled. Enable it in System Settings → General → Sharing → Remote Login"
    fi
    # else: Remote Login disabled but not using SSH mode; skip silently
fi


###############################################################################
# Create shared workspace directory
###############################################################################
if [[ "$REBUILD" != "false" ]]; then

    debug "Creating shared workspace at $SHARED_WORKSPACE..."
    mkdir -p "$SHARED_WORKSPACE"
    configure_shared_folder_permssions true

    # Create a README in the shared workspace
    cat > "$SHARED_WORKSPACE/SANDVAULT-README.md" << EOF
    # sandvault workspace for '$USER'
    # (autogenerated file; do not edit)

    This directory is shared with '$SANDVAULT_USER' user.
    The sandvault user has full read/write access here.

    ## To switch to sandvault:

        "${BASH_SOURCE[0]} shell"

    ## Or create an alias in your $HOME/.zshrc or $HOME/.bashrc

        alias sv="${BASH_SOURCE[0]}"

        then run "sv shell"
EOF
fi


###############################################################################
# Configure passwordless sudo to switch to sandvault user
###############################################################################
if [[ "$REBUILD" != "false" ]]; then
    debug "Configuring passwordless access to $SANDVAULT_USER..."

heredoc SUDOERS_BUILD_HOME_SCRIPT_CONTENTS << EOF
#!/usr/bin/env bash
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
# Use tr to convert newlines to null terminators to pass to xargs -0
#
# In the event that no files need to be synchronized, send ":" to xargs to avoid error
#
# Use OSX chown, not GNU chown, because I've tested that it works
cd "/Users/$SANDVAULT_USER/"
/usr/bin/rsync \
    --itemize-changes \
    --out-format="%n" \
    --links \
    --checksum \
    --recursive \
    --perms \
    --times \
    "$WORKSPACE/guest/home/." \
    "." \
    | (tr '\n' '\0' || echo :) \
    | xargs -0 sudo /usr/sbin/chown "$SANDVAULT_USER:$SANDVAULT_GROUP"
EOF
    sudo mkdir -p "$(dirname "$SUDOERS_BUILD_HOME_SCRIPT_NAME")"
    # shellcheck disable=SC2154 # SUDOERS_BUILD_HOME_SCRIPT_CONTENTS is referenced but not assigned (yes it is)
    echo "$SUDOERS_BUILD_HOME_SCRIPT_CONTENTS" | sudo tee "$SUDOERS_BUILD_HOME_SCRIPT_NAME" > /dev/null
    sudo /bin/chmod 0550 "$SUDOERS_BUILD_HOME_SCRIPT_NAME"

    # Get the sandvault user's UID
    SANDVAULT_UID=$(dscl . -read "/Users/$SANDVAULT_USER" UniqueID 2>/dev/null | awk '{print $2}')

heredoc SUDOERS_CONTENT << EOF
# Allow $USER to run these commands as $SANDVAULT_USER without password
$USER ALL=($SANDVAULT_USER) NOPASSWD: /bin/zsh
$USER ALL=($SANDVAULT_USER) NOPASSWD: /usr/bin/env
$USER ALL=($SANDVAULT_USER) NOPASSWD: /usr/bin/true

# Allow $USER to run $SUDOERS_BUILD_HOME_SCRIPT_NAME to sync home directory
$USER ALL=(root) NOPASSWD: $SUDOERS_BUILD_HOME_SCRIPT_NAME

# Allow $USER to kill $SANDVAULT_USER processes without password
$USER ALL=(root) NOPASSWD: /bin/launchctl bootout user/$SANDVAULT_UID
$USER ALL=(root) NOPASSWD: /usr/bin/pkill -9 -u $SANDVAULT_USER
EOF

    # Write to a root-owned temp file, validate, then atomically move into place.
    SUDOERS_TMP="$(sudo /usr/bin/mktemp "$(dirname "$SUDOERS_FILE")/.sudoers.XXXXXXXX")"
    # shellcheck disable=SC2154 # SUDOERS_CONTENT is referenced but not assigned (yes it is)
    echo "$SUDOERS_CONTENT" | sudo tee "$SUDOERS_TMP" > /dev/null
    sudo /bin/chmod 0440 "$SUDOERS_TMP"

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
if [[ "$REBUILD" != "false" ]] || [[ ! -f "$SANDBOX_PROFILE" ]]; then
    debug "Configuring passwordless access to $SANDVAULT_USER..."

    # Create sandbox profile to restrict /Volumes access, which prevents
    # sandvault user from modifying removable drives. Issue discovered by
    # by Github user redLocomotive.
    #
    # The profile file is owned by root so sandvault user cannot modify it.
    debug "Creating sandbox profile..."
heredoc SANDBOX_PROFILE_CONTENT << 'EOF'
;; Sandbox profile for sandvault - restricts access to external drives
(version 1)
(allow default)
(deny file-read* (subpath "/Volumes"))
(deny file-write* (subpath "/Volumes"))
(allow file-read* (subpath "/Volumes/Macintosh HD"))
(allow file-write* (subpath "/Volumes/Macintosh HD"))
EOF
    # shellcheck disable=SC2154
    echo "$SANDBOX_PROFILE_CONTENT" | sudo tee "$SANDBOX_PROFILE" > /dev/null
    sudo /bin/chmod 0444 "$SANDBOX_PROFILE"
fi


###############################################################################
# Create passwordless SSH key with permission to remotely login to guest
###############################################################################
if [[ ! -f "$SSH_KEYFILE_PRIV" ]] || [[ ! -f "$SSH_KEYFILE_PUB" ]]; then
    trace "Creating SSH key files..."
    mkdir -p "$SSH_DIR"
    /bin/chmod 0700 "$SSH_DIR"
    ssh-keygen -t ed25519 \
        -f "$SSH_KEYFILE_PRIV" \
        -N "" \
        -q \
        -C "${USER}-to-sandvault@${HOSTNAME}"
fi

# Add SSH public key to host's authorized_keys
trace "Configuring remote SSH access"
GUEST_AUTHORIZED_KEYS="$WORKSPACE/guest/home/.ssh/authorized_keys"
mkdir -p "$(dirname "$GUEST_AUTHORIZED_KEYS")"
/bin/chmod 0700 "$(dirname "$GUEST_AUTHORIZED_KEYS")"
cp "$SSH_KEYFILE_PUB" "$GUEST_AUTHORIZED_KEYS"
/bin/chmod 0600 "$GUEST_AUTHORIZED_KEYS"


###############################################################################
# Configure git
###############################################################################
trace "Configuring git..."

# Get git config from host
GIT_USER_NAME=$(git config --global --get user.name 2>/dev/null || echo "")
GIT_USER_EMAIL=$(git config --global --get user.email 2>/dev/null || echo "")
git config set -f "$WORKSPACE/guest/home/.gitconfig" user.name "$GIT_USER_NAME"
git config set -f "$WORKSPACE/guest/home/.gitconfig" user.email "$GIT_USER_EMAIL"
git config set -f "$WORKSPACE/guest/home/.gitconfig" safe.directory "$SHARED_WORKSPACE/*"


###############################################################################
# Copy guest/home/. to sandvault $HOME
###############################################################################
debug "Configure $SANDVAULT_USER home directory..."
sudo "$SUDOERS_BUILD_HOME_SCRIPT_NAME"


###############################################################################
# Mark installation as complete
###############################################################################
debug "Creating installation marker..."
mkdir -p "$(dirname "$INSTALL_MARKER")"
date > "$INSTALL_MARKER"


###############################################################################
# Run the application
###############################################################################
if [[ "$COMMAND" == "build" ]]; then
    exit 0
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
sv_exit_code=0
register_session
trap 'sv_exit_code=$?; set +e; unregister_session; exit $sv_exit_code' EXIT

# TMPDIR: claude (and perhaps other AI agents) creates temporary directories in locations
# that are shared, e.g. /tmp/claude and /private/tmp/claude, which doesn't work when there
# are multiple users running the agent on the same computer.
# Fix: set TMPDIR after the shell has started running.
ZSH_COMMAND="export TMPDIR=\$(mktemp -d); cd ~; exec /bin/zsh --login"

# Prepare command args as a single string
COMMAND_ARGS_STR=""
SHELL_COMMAND_MODE=false
if [[ ${#COMMAND_ARGS[@]} -gt 0 ]]; then
    printf -v COMMAND_ARGS_STR '%q ' "${COMMAND_ARGS[@]}"

    # When the user requests running shell (instead of an AI agent) then convert
    # COMMAND_ARGS to a command that will be run by the shell.
    #
    # Example: sv shell -- echo foo
    # Runs:    exec /bin/zsh --login -c 'echo foo'
    if [[ "$COMMAND" == "" ]]; then
        ZSH_COMMAND="$ZSH_COMMAND -c '${COMMAND_ARGS_STR}'"
        COMMAND_ARGS_STR=""
        SHELL_COMMAND_MODE=true
    fi
fi

# Unique session id for this invocation (used by shell init scripts)
SV_SESSION_ID="${SV_SESSION_ID:-$(/usr/bin/uuidgen)}"

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
    if ssh \
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
            "VERBOSE=$VERBOSE" \
            "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
            /usr/bin/sandbox-exec -f "$SANDBOX_PROFILE" \
                /bin/zsh -c "$ZSH_COMMAND_SSH"
    then
        :
    else
        exit $?
    fi
else
    # First verify that passwordless sudo is working
    trace "Checking passwordless sudo"
    if ! sudo --non-interactive --user="$SANDVAULT_USER" /usr/bin/true ; then
        error "Passwordless sudo to $SANDVAULT_USER user is not configured correctly."
        error "Please run: ${BASH_SOURCE[0]} build --rebuild"
        exit 1
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
    if sudo \
        --login \
        --set-home \
        --user="$SANDVAULT_USER" \
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
            "VERBOSE=$VERBOSE" \
            "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
            /usr/bin/sandbox-exec -f "$SANDBOX_PROFILE" \
                /bin/zsh -c "$ZSH_COMMAND"
    then
        :
    else
        exit $?
    fi
fi
