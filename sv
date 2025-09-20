#!/usr/bin/env bash
# Build a sandbox user ("sandvault") for running commands
set -Eeuo pipefail
trap 'echo "${BASH_SOURCE[0]}: line $LINENO: $BASH_COMMAND: exitcode $?"' ERR
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE="$SCRIPT_DIR"


###############################################################################
# Functions
###############################################################################
trace () {
    [[ "${VERBOSE_LEVEL:-0}" -lt 2 ]] || echo >&2 -e "🔬 \033[90m$*\033[0m"
}
debug () {
    [[ "${VERBOSE_LEVEL:-0}" -lt 1 ]] || echo >&2 -e "🔍 \033[36m$*\033[0m"
}
info () {
    echo >&2 -e "ℹ️ \033[36m$*\033[0m"
}
warn () {
    echo >&2 -e "⚠️ \033[33m$*\033[0m"
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
VERSION="1.0.5"

# Each user on the computer can have their own sandvault
SANDVAULT_USER="sandvault-$USER"
SANDVAULT_GROUP="sandvault-$USER"
SHARED_WORKSPACE="/Users/Shared/$SANDVAULT_USER"

# Create sudoers.d file for passwordless sudo to sandvault user
SUDOERS_FILE="/etc/sudoers.d/50-nopasswd-for-$SANDVAULT_USER"

# Installation marker file
INSTALL_ORG="$HOME/.config/codeofhonor"
INSTALL_PRODUCT="$INSTALL_ORG/sandvault"
INSTALL_MARKER="$INSTALL_PRODUCT/install"

SSH_DIR="$HOME/.ssh"
SSH_KEYFILE_PRIV="$SSH_DIR/id_ed25519_sandvault"
SSH_KEYFILE_PUB="$SSH_KEYFILE_PRIV.pub"


###############################################################################
# Functions
###############################################################################
install_tools () {
    # Install brew
    if ! command -v brew &> /dev/null ; then
        debug "Installing brew..."
        /usr/bin/env bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    fi

    debug "Installing tools..."
    local TOOLS=()
    TOOLS+=("git")      # version control
    TOOLS+=("git-lfs")  # large files
    TOOLS+=("netcat")   # test network connectivity
    TOOLS+=("node")     # install claude with npm
    TOOLS+=("python")   # python used for claude hooks
    TOOLS+=("uv")       # run python scripts with uv

    for tool in "${TOOLS[@]}"; do
        if ! command -v "$(basename "$tool")" &>/dev/null ; then
            trace "Installing $tool..."
            if [[ "${VERBOSE_LEVEL:-0}" -lt 3 ]]; then
                brew install --quiet "$tool"
            else
                brew install "$tool"
            fi
        fi
    done
}

show_version() {
    echo "$(basename "${BASH_SOURCE[0]}") version $VERSION"
    exit 0
}

configure_shared_folder_permssions() {
    local enable="$1"

    # Set the owner to $USER on both enable and disable so
    # files owned by sandvault do not get orphaned by uninstall
    trace "Configuring $SHARED_WORKSPACE permissions..."
    sudo /bin/chmod 0700 "$SHARED_WORKSPACE"
    trace "Configuring $SHARED_WORKSPACE owner and group..."
    sudo /usr/sbin/chown -f -R "$USER:$(id -gn)" "$SHARED_WORKSPACE"

    # Grant write access to shared workspace for sandvault group. We want
    # to modify files and symbolic links, not what symbolic links point to.
    # Use `find | xargs chmod -h` instead of `chmod -R -h` because the latter
    # causes: "chmod: the -R and -h options may not be specified together"
    local rights="group:$SANDVAULT_GROUP allow read,write,execute,append,delete,delete_child,readattr,writeattr,readextattr,writeextattr,readsecurity,writesecurity,chown,file_inherit,directory_inherit"
    if [[ "$enable" != "false" ]]; then
        trace "Configuring $SHARED_WORKSPACE: add $rights (recursively)"
        sudo find "$SHARED_WORKSPACE" -print0 | xargs -0 sudo /bin/chmod -h +a "$rights"
    else
        trace "Configuring $SHARED_WORKSPACE: remove $rights (recursively)"
        sudo find "$SHARED_WORKSPACE" -print0 2>/dev/null | xargs -0 sudo /bin/chmod -h -a "$rights" 2>/dev/null || true
    fi
}

uninstall() {
    debug "Uninstalling..."

    # Remove the install marker file first; it's a sentinel for "everything is complete".
    # By removing it first we force a rebuild if the user wants to run this again.
    rm -rf "$INSTALL_MARKER"
    rmdir "$INSTALL_PRODUCT" &>/dev/null || true
    rmdir "$INSTALL_ORG" &>/dev/null || true

    # Remove the sudoers file
    sudo rm -rf "$SUDOERS_FILE"

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
VERBOSE_LEVEL="${VERBOSE_LEVEL:-0}"
REBUILD=false
MODE=shell

show_help() {
    appname=$(basename "${BASH_SOURCE[0]}")
    echo "Usage: $appname [options] command"
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
    echo "  c, claude [PATH]     Open Claude Code in sandvault"
    echo "  s, shell  [PATH]     Open shell in sandvault"
    echo "  b, build             Build sandvault"
    echo "  u, uninstall         Remove user & files (but not this repo)"
    exit 0
}

# Parse optional arguments
NEW_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        -s|--ssh)
            MODE=ssh
            shift
            ;;
        -r|--rebuild)
            REBUILD=true
            shift
            ;;
        -v|--verbose)
            ((VERBOSE_LEVEL++)) || true
            shift
            ;;
        -vv)
            ((VERBOSE_LEVEL+=2)) || true
            shift
            ;;
        -vvv)
            ((VERBOSE_LEVEL+=3)) || true
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
set -- "${NEW_ARGS[@]}"

# Parse fixed arguments
case "${1:-}" in
    c|claude)
        COMMAND=claude
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


###############################################################################
# Setup
###############################################################################
install_tools


###############################################################################
# Determine whether configuration is already complete
###############################################################################
if [[ ! -f "$INSTALL_MARKER" ]]; then
    # Since this is a full rebuild, provide more feedback
    VERBOSE_LEVEL=$(( VERBOSE_LEVEL > 1 ? VERBOSE_LEVEL : 1 ))
    info "Installing sandvault..."
    REBUILD=true
fi

if [[ "$REBUILD" != "false" ]]; then
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

    # Add to SSH access group (required for SSH login)
    # do not use sudo dscl; it creates duplicate entries
    sudo dseditgroup -o edit -a "$SANDVAULT_USER" -t user com.apple.access_ssh

    # Add current user to the sandvault group
    trace "Adding $USER to $SANDVAULT_GROUP group..."
    sudo dseditgroup -o edit -a "$USER" -t user "$SANDVAULT_GROUP"
fi


###############################################################################
# Create passwordless SSH key with permission to remotely login to guest
###############################################################################
if [[ "$REBUILD" != "false" ]]; then
    if [[ ! -f "$SSH_KEYFILE_PRIV" ]] || [[ ! -f "$SSH_KEYFILE_PUB" ]]; then
        trace "Creating SSH key files..."
        mkdir -p "$SSH_DIR"
        /bin/chmod 0700 "$SSH_DIR"
        ssh-keygen -t ed25519 \
            -f "$SSH_KEYFILE_PRIV" \
            -N "" \
            -q \
            -C "sandvault-${USER}@${HOSTNAME}"
    fi
fi


###############################################################################
# Configure settings
###############################################################################
if [[ "$REBUILD" != "false" ]]; then
    debug "Configuring sandvault dotfiles..."

    # Get git config from host
    GIT_USER_NAME=$(git config --global --get user.name 2>/dev/null || echo "")
    GIT_USER_EMAIL=$(git config --global --get user.email 2>/dev/null || echo "")
    git config set -f "$WORKSPACE/guest/home/.gitconfig" user.name "$GIT_USER_NAME"
    git config set -f "$WORKSPACE/guest/home/.gitconfig" user.email "$GIT_USER_EMAIL"
    git config set -f "$WORKSPACE/guest/home/.gitconfig" safe.directory "$SHARED_WORKSPACE/*"

    # Add SSH public key to host's authorized_keys
    GUEST_AUTHORIZED_KEYS="$WORKSPACE/guest/home/.ssh/authorized_keys"
    mkdir -p "$(dirname "$GUEST_AUTHORIZED_KEYS")"
    cp "$SSH_KEYFILE_PUB" "$GUEST_AUTHORIZED_KEYS"
    /bin/chmod 0600 "$GUEST_AUTHORIZED_KEYS"
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
# Configure sandvault user
###############################################################################
if [[ "$REBUILD" != "false" ]]; then
    debug "Configure $SANDVAULT_USER home directory..."

    # Copy files to home directory
    sudo mkdir -p "/Users/$SANDVAULT_USER"
    sudo cp -rf "$WORKSPACE/guest/home/." "/Users/$SANDVAULT_USER/"

    # Make sandvault the owner of the files
    sudo chown -R "$SANDVAULT_USER:$SANDVAULT_GROUP" "/Users/$SANDVAULT_USER" 2>/dev/null || true

    # Fixup file permissions
    sudo /bin/chmod 0755 "/Users/$SANDVAULT_USER"
    sudo /bin/chmod 0700 "/Users/$SANDVAULT_USER/.ssh"
    if [[ -f "/Users/$SANDVAULT_USER/authorized_keys" ]]; then
        sudo /bin/chmod 0600 "/Users/$SANDVAULT_USER/authorized_keys"
    fi
    if [[ -f "/Users/$SANDVAULT_USER/.ssh/id_ed25519" ]]; then
        sudo /bin/chmod 0600 "/Users/$SANDVAULT_USER/.ssh/id_ed25519"
    fi
    if [[ -f "/Users/$SANDVAULT_USER/.ssh/id_ed25519.pub" ]]; then
        sudo /bin/chmod 0644 "/Users/$SANDVAULT_USER/.ssh/id_ed25519.pub"
    fi
fi


###############################################################################
# Configure passwordless sudo to switch to sandvault user
###############################################################################
if [[ "$REBUILD" != "false" ]]; then
    debug "Configuring passwordless access to $SANDVAULT_USER..."

    # Get the sandvault user's UID
    SANDVAULT_UID=$(dscl . -read "/Users/$SANDVAULT_USER" UniqueID 2>/dev/null | awk '{print $2}')

heredoc SUDOERS_CONTENT << EOF
# Allow '$USER' to sudo to $SANDVAULT_USER without password and run any command as that user
$USER ALL=($SANDVAULT_USER) NOPASSWD: ALL
# Allow '$USER' to kill $SANDVAULT_USER processes without password
$USER ALL=(root) NOPASSWD: /bin/launchctl bootout user/$SANDVAULT_UID
$USER ALL=(root) NOPASSWD: /usr/bin/pkill -9 -u $SANDVAULT_USER
EOF

    echo "$SUDOERS_CONTENT" | sudo tee "$SUDOERS_FILE" > /dev/null
    sudo /bin/chmod 0440 "$SUDOERS_FILE"

    # Validate the sudoers file
    if ! sudo visudo -c -f "$SUDOERS_FILE" &>/dev/null; then
        error "Failed to create valid sudoers file"
        sudo rm -f "$SUDOERS_FILE"
        abort "Sudoers configuration failed"
    fi
fi


###############################################################################
# Mark installation as complete
###############################################################################
if [[ "$REBUILD" != "false" ]]; then
    debug "Creating installation marker..."
    mkdir -p "$(dirname "$INSTALL_MARKER")"
    date > "$INSTALL_MARKER"
fi


###############################################################################
# Cleanup function for sandvault processes
###############################################################################
cleanup_sandvault_processes() {
    # Exit if other sandvault sessions are active
    local session_count
    # shellcheck disable=SC2009 # Consider using pgrep instead of grepping ps output
    session_count=$(ps -u "$SANDVAULT_USER" -o command | grep -c "/bin/zsh --login" || true)
    if [[ "${session_count:-0}" -ne 0 ]]; then
        trace "$session_count $SANDVAULT_USER sessions still active; skipping cleanup"
        return 0
    fi

    # We're the last session, safe to cleanup all sandvault processes
    # Try to bootout the user session (this terminates all processes)
    trace "Terminating $SANDVAULT_USER user session..."
    local sandvault_uid
    sandvault_uid=$(dscl . -read "/Users/$SANDVAULT_USER" UniqueID 2>/dev/null | awk '{print $2}')
    sudo launchctl bootout "user/$sandvault_uid" 2>/dev/null || true

    # Brief wait for cleanup
    sleep 0.2

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

# Set up trap to cleanup processes on exit
trap 'cleanup_sandvault_processes' EXIT

if [[ "$MODE" == "ssh" ]]; then
    trace "Checking SSH connectivity"
    if ! nc -z "$HOSTNAME" 22 ; then
        error "$LOCAL_NETWORK_ERROR"
        read -n 1 -s -r -p "Press any key to open System Settings"
        open "/System/Library/PreferencePanes/Security.prefPane"
    fi

    debug "SSH $SANDVAULT_USER@$HOSTNAME"
    ssh \
        -q \
        -t \
        -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null \
        -i "$SSH_KEYFILE_PRIV" \
        "$SANDVAULT_USER@$HOSTNAME" \
        /usr/bin/env \
            "COMMAND=$COMMAND" \
            "INITIAL_DIR=$INITIAL_DIR" \
            "SHARED_WORKSPACE=$SHARED_WORKSPACE" \
            "VERBOSE_LEVEL=${VERBOSE_LEVEL:-0}" \
            /bin/zsh --login || true
else
    # First verify that passwordless sudo is working
    trace "Checking passwordless sudo"
    if ! sudo --non-interactive --user="$SANDVAULT_USER" true 2>/dev/null; then
        error "Passwordless sudo to $SANDVAULT_USER user is not configured correctly."
        error "Please run: ${BASH_SOURCE[0]} build --rebuild"
        exit 1
    fi

    # Launch interactive shell as sandvault user
    # Use sudo with -H to set HOME correctly
    # Use env to ensure the environment is cleared, otherwise PATH carries over
    debug "Shell $SANDVAULT_USER@$HOSTNAME"
    sudo \
        --login \
        --set-home \
        --user="$SANDVAULT_USER" \
        env -i \
            "HOME=/Users/$SANDVAULT_USER" \
            "USER=$SANDVAULT_USER" \
            "SHELL=/bin/zsh" \
            "TERM=${TERM:-}" \
            "COMMAND=$COMMAND" \
            "INITIAL_DIR=$INITIAL_DIR" \
            "SHARED_WORKSPACE=$SHARED_WORKSPACE" \
            "VERBOSE_LEVEL=${VERBOSE_LEVEL:-0}" \
            /bin/zsh -c "cd ~ ; exec /bin/zsh --login" || true
fi
