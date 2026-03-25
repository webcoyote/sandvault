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

host_cmd() {
    local display="" use_eval=false
    while true; do
        case "${1:-}" in
            --eval) use_eval=true; shift ;;
            --msg)  display="$2"; shift 2 ;;
            *) break ;;
        esac
    done
    if [[ -z "$display" ]]; then
        display="$*"
    fi
    if [[ "${QUIET:-false}" != "true" ]]; then
        echo >&2 "  ▸ $display"
    fi
    if [[ "${DRYRUN:-false}" == "true" ]]; then
        return 0
    fi
    if [[ "${AUTO_YES:-false}" != "true" && "${QUIET:-false}" != "true" ]]; then
        local choice
        read -rn1 -p "    (y)es / (a)ll / (s)kip / (q)uit [y]: " choice </dev/tty
        echo >&2
        case "${choice:-y}" in
            y|Y) ;;
            a|A) AUTO_YES=true ;;
            s|S) return 0 ;;
            q|Q) exit 1 ;;
            *) return 0 ;;
        esac
    fi
    if [[ "$use_eval" == "true" ]]; then
        if [[ "${QUIET:-false}" == "true" ]]; then
            eval "$@" >/dev/null 2>&1
        else
            eval "$@"
        fi
    else
        if [[ "${QUIET:-false}" == "true" ]]; then
            "$@" >/dev/null 2>&1
        else
            "$@"
        fi
    fi
}

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
        host_cmd mkdir -p "$SSH_DIR"
        host_cmd /bin/chmod 0700 "$SSH_DIR"
        host_cmd ssh-keygen -t ed25519 \
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
readonly VERSION="1.1.28"

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
readonly HOST_SHELL="${SHELL:-/bin/zsh}"

readonly SANDVAULT_RIGHTS="group:$SANDVAULT_GROUP allow read,write,append,delete,delete_child,readattr,writeattr,readextattr,writeextattr,readsecurity,writesecurity,chown,search,list,file_inherit,directory_inherit"

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
    if [[ "$NESTED" == "true" ]]; then
        abort "sandvault user cannot install Homebrew; run as $HOST_USER instead"
    fi
    if [[ "$NO_BUILD" == "true" ]]; then
        abort "Missing Homebrew; refusing to install because --no-build flag set"
    fi
    debug "Installing Homebrew..."
    host_cmd --eval 'env bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"'
    # shellcheck disable=SC2310 # brew_shellenv intentionally used in || condition
    brew_shellenv || abort "Homebrew install failed."
}

ensure_brew_tool() {
    local tool="$1"
    local cli_name="${2:-$tool}"

    # check if CLI already available in PATH
    if which "$cli_name" &>/dev/null; then
        debug "$tool CLI '$cli_name' already available in PATH; skipping Homebrew installation"
        return 0
    fi

    # shellcheck disable=SC2310 # brew_shellenv intentionally used in || condition
    brew_shellenv || true

    if [[ -x "$(brew --prefix)/bin/$cli_name" ]]; then
        return 0
    fi
    if [[ "$NESTED" == "true" ]]; then
        abort "sandvault user cannot install $tool; run as $HOST_USER instead"
    fi
    if [[ "$NO_BUILD" == "true" ]]; then
        abort "Missing $cli_name; refusing to install because --no-build flag set"
    fi
    ensure_brew
    debug "Installing $tool with Homebrew..."
    if [[ "$SV_VERBOSE" -lt 3 ]]; then
        host_cmd brew install --quiet "$tool"
    else
        host_cmd brew install "$tool"
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

init_sandbox_run_for_repository() {
    SANDBOX_RUN=()
    if [[ "$NESTED" == "false" ]]; then
        SANDBOX_RUN+=("sudo" "--non-interactive" "--user=$SANDVAULT_USER")
    fi
    SANDBOX_RUN+=(
        "/usr/bin/env" "-i"
        "HOME=/Users/$SANDVAULT_USER"
        "USER=$SANDVAULT_USER"
        "SHELL=$HOST_SHELL"
        "PATH=/usr/bin:/bin:/usr/sbin:/sbin"
    )
}

sandbox_repository_git() {
    "${SANDBOX_RUN[@]}" git -C "$INITIAL_DIR" "$@"
}

local_repository_git() {
    git -C "$LOCAL_REPOSITORY" "$@"
}

force_cleanup_sandvault_processes() {
    if [[ "$NESTED" == "true" ]]; then
        return 0
    fi

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
    if [[ "$enable" == "true" ]]; then
        # Make workspace accessible to $HOST_USER and $SANDVAULT_GROUP only
        host_cmd sudo /usr/sbin/chown -f -R "$HOST_USER:$SANDVAULT_GROUP" "/Users/$SANDVAULT_USER"
        host_cmd sudo /bin/chmod 0770 "/Users/$SANDVAULT_USER"
        host_cmd --eval "sudo find '/Users/$SANDVAULT_USER' -print0 | xargs -0 sudo /bin/chmod -h +a '$SANDVAULT_RIGHTS'"
    else
        # Make workspace accessible to $HOST_USER only
        host_cmd sudo /usr/sbin/chown -f -R "$HOST_USER:$(id -gn)" "/Users/$SANDVAULT_USER"
        host_cmd sudo /bin/chmod 0700 "/Users/$SANDVAULT_USER"
        host_cmd --eval "sudo find '/Users/$SANDVAULT_USER' -print0 2>/dev/null | xargs -0 sudo /bin/chmod -h -a '$SANDVAULT_RIGHTS' 2>/dev/null || true"
    fi
}

uninstall() {
    debug "Uninstalling..."
    host_cmd --msg "Terminate $SANDVAULT_USER processes (launchctl bootout + pkill)" \
        force_cleanup_sandvault_processes

    # Remove the install marker file first; it's a sentinel for "everything is complete".
    # By removing it first we force a rebuild if the user wants to run this again.
    host_cmd rm -rf "$INSTALL_MARKER"
    host_cmd rmdir "$INSTALL_PRODUCT" || true
    host_cmd rmdir "$INSTALL_ORG" || true

    # Remove the sudoers file
    host_cmd sudo rm -rf "$SUDOERS_FILE"

    # Remove build home script and sandbox profile
    host_cmd sudo rm -rf "$SUDOERS_BUILD_HOME_SCRIPT_NAME"
    host_cmd sudo rm -rf "$SANDBOX_PROFILE"
    host_cmd sudo rmdir "$(dirname "$SUDOERS_BUILD_HOME_SCRIPT_NAME")" || true

    # Remove shared folder ACLs from sandbox home
    debug "Removing shared workspace permissions..."
    configure_shared_folder_permssions false

    # Remove host user from sandvault group
    debug "Removing user and group..."
    host_cmd sudo dseditgroup -o edit -d "$HOST_USER" -t user "$SANDVAULT_GROUP" || true

    # Remove sandvault user from SSH group BEFORE deleting the user
    host_cmd sudo dseditgroup -o edit -d "$SANDVAULT_USER" -t user com.apple.access_ssh || true

    # Now delete the user and group
    host_cmd sudo dscl . -delete "/Users/$SANDVAULT_USER" || true
    host_cmd sudo dscl . -delete "/Groups/$SANDVAULT_GROUP" || true
    host_cmd sudo rm -rf "/Users/$SANDVAULT_USER"

    # Cleanup SSH
    host_cmd rm -rf "$SSH_KEYFILE_PRIV" "$SSH_KEYFILE_PUB"

    # Sandbox home is already removed above (sudo rm -rf /Users/$SANDVAULT_USER)
}


###############################################################################
# Parse command line
###############################################################################
REBUILD=false
NO_BUILD=false
USE_SANDBOX=true
AUTO_YES=false
QUIET=false
DRYRUN=false
MODE=shell
COMMAND_ARGS=()
INITIAL_DIR=""
CLONE_REPOSITORY=""

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
    echo "  -y, --yes            Auto-confirm all host commands (still echoed)"
    echo "  -q, --quiet          Suppress all host command output (implies -y)"
    echo "  --dryrun             Show what would be done without executing anything"
    echo "  -c, --clone URL|PATH Clone Git repository into sandvault home and open there"
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
    echo "Arguments after -- are passed to the command (claude, gemini, codex, shell)"
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
        -y|--yes)
            AUTO_YES=true
            shift
            ;;
        -q|--quiet)
            QUIET=true
            AUTO_YES=true
            shift
            ;;
        --dryrun)
            DRYRUN=true
            REBUILD=true
            shift
            ;;
        -c|--clone)
            if [[ $# -lt 2 ]]; then
                abort "Missing argument for $1"
            fi
            CLONE_REPOSITORY="$2"
            shift 2
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
        show_help
        ;;
esac
readonly COMMAND
readonly CLONE_REPOSITORY

if [[ -z "$CLONE_REPOSITORY" ]]; then
    # Resolve symlinks to get the real path
    INITIAL_DIR="$(cd "${INITIAL_DIR:-"${PWD}"}" 2>/dev/null && pwd -P || echo "$INITIAL_DIR")"
    readonly INITIAL_DIR
elif [[ -n "$INITIAL_DIR" ]]; then
    # --clone wants to set INITIAL_DIRECTORY
    abort "Cannot use [PATH] and --clone together; choose one"
fi


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
    if [[ "$COMMAND" == "build" ]]; then
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
readonly QUIET
readonly DRYRUN


###############################################################################
# Setup
###############################################################################
if [[ "$REBUILD" == "true" ]]; then
    info "Installing sandvault..."
    sudo "-p Password required to create sandvault: " true
fi

install_tools


###############################################################################
# Create sandvault user and group
###############################################################################
if [[ "$REBUILD" == "true" ]]; then
    debug "Creating $SANDVAULT_USER user and $SANDVAULT_GROUP group..."

    # Check if group exists, create if needed
    if ! dscl . -read "/Groups/$SANDVAULT_GROUP" &>/dev/null 2>&1; then
        trace "Creating $SANDVAULT_GROUP group..."

        # Find next available UID/GID starting from 501
        NEXT_UID=$(dscl . -list /Users UniqueID | awk '{print $2}' | sort -n | tail -1)
        NEXT_UID=$((NEXT_UID + 1))

        # Create group
        host_cmd sudo dscl . -create "/Groups/$SANDVAULT_GROUP"
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
    host_cmd sudo dscl . -create "/Groups/$SANDVAULT_GROUP" PrimaryGroupID "$GROUP_ID"
    host_cmd sudo dscl . -create "/Groups/$SANDVAULT_GROUP" RealName "$SANDVAULT_GROUP Group"

    # Check if user exists, create if needed
    if ! dscl . -read "/Users/$SANDVAULT_USER" &>/dev/null 2>&1; then
        trace "Creating $SANDVAULT_USER user..."

        # Find next available UID
        NEXT_UID=$(dscl . -list /Users UniqueID | awk '{print $2}' | sort -n | tail -1)
        NEXT_UID=$((NEXT_UID + 1))

        # Create user
        host_cmd sudo dscl . -create "/Users/$SANDVAULT_USER"
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
    host_cmd sudo dscl . -create "/Users/$SANDVAULT_USER" UniqueID "$USER_ID"
    host_cmd sudo dscl . -create "/Users/$SANDVAULT_USER" PrimaryGroupID "$GROUP_ID"
    host_cmd sudo dscl . -create "/Users/$SANDVAULT_USER" RealName "$SANDVAULT_USER User"
    host_cmd sudo dscl . -create "/Users/$SANDVAULT_USER" NFSHomeDirectory "/Users/$SANDVAULT_USER"
    host_cmd sudo dscl . -create "/Users/$SANDVAULT_USER" UserShell "$HOST_SHELL"

    # Set a random password for the user (password required for SSH on macOS)
    # We'll use key-based auth so the password won't actually be used.
    RANDOM_PASS=$(openssl rand -base64 32)
    host_cmd --msg "sudo dscl . -passwd /Users/$SANDVAULT_USER <random>" \
        sudo dscl . -passwd "/Users/$SANDVAULT_USER" "$RANDOM_PASS"
    host_cmd sudo dscl . -create "/Users/$SANDVAULT_USER" IsHidden 1

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
    host_cmd --eval "sudo dseditgroup -o edit -d '$SANDVAULT_USER' -t user staff 2>/dev/null || true"
    if [[ -n "$SANDVAULT_GENERATED_UID" ]]; then
        host_cmd --eval "sudo dscl . -delete /Groups/staff GroupMembers '$SANDVAULT_GENERATED_UID' 2>/dev/null || true"
    fi

    host_cmd --eval "sudo dscl . -delete /Groups/staff GroupMembership '$SANDVAULT_USER' 2>/dev/null || true"
    if sudo dscl . -read "/Groups/staff" GroupMembership 2>/dev/null | grep -Eq "(^|[[:space:]])$SANDVAULT_USER($|[[:space:]])"; then
        abort "Failed to remove $SANDVAULT_USER user entry from staff group"
    fi
    if [[ -n "$SANDVAULT_GENERATED_UID" ]] && \
       sudo dscl . -read "/Groups/staff" GroupMembers 2>/dev/null | grep -Eq "(^|[[:space:]])$SANDVAULT_GENERATED_UID($|[[:space:]])"
    then
        abort "Failed to remove $SANDVAULT_USER GeneratedUID entry from staff group"
    fi

    # Add host user to the sandvault group
    trace "Adding $HOST_USER to $SANDVAULT_GROUP group..."
    host_cmd sudo dseditgroup -o edit -a "$HOST_USER" -t user "$SANDVAULT_GROUP"
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
        # Remote Login is enabled; ensure sandvault user can SSH
        if ! dseditgroup -o checkmember -m "$SANDVAULT_USER" com.apple.access_ssh &>/dev/null; then
            if [[ "$NO_BUILD" == "true" ]]; then
                abort "cannot add $SANDVAULT_USER to remote access because --no-build flag set"
            fi
            # do not use sudo dscl; it creates duplicate entries
            host_cmd sudo dseditgroup -o edit -a "$SANDVAULT_USER" -t user com.apple.access_ssh
        fi
    elif [[ "$MODE" == "ssh" ]]; then
        # Remote Login is disabled and SSH mode requested
        abort "Remote Login via SSH is not enabled. Enable it in System Settings → General → Sharing → Remote Login"
    fi
    # else: Remote Login disabled but not using SSH mode; skip silently
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

# Copy files to home directory (--copy-unsafe-links resolves symlinks pointing outside source)
/usr/bin/rsync -a --copy-unsafe-links "$WORKSPACE/guest/home/." "/Users/$SANDVAULT_USER/."
sudo /usr/sbin/chown -R "$SANDVAULT_USER:$SANDVAULT_GROUP" "/Users/$SANDVAULT_USER"
EOF
    host_cmd sudo mkdir -p "$(dirname "$SUDOERS_BUILD_HOME_SCRIPT_NAME")"
    # shellcheck disable=SC2154 # SUDOERS_BUILD_HOME_SCRIPT_CONTENTS is referenced but not assigned (yes it is)
    host_cmd --eval "echo \"\$SUDOERS_BUILD_HOME_SCRIPT_CONTENTS\" | sudo tee '$SUDOERS_BUILD_HOME_SCRIPT_NAME' > /dev/null"
    host_cmd sudo /bin/chmod 0554 "$SUDOERS_BUILD_HOME_SCRIPT_NAME"

    # Get the sandvault user's UID
    SANDVAULT_UID=$(dscl . -read "/Users/$SANDVAULT_USER" UniqueID 2>/dev/null | awk '{print $2}')

heredoc SUDOERS_CONTENT << EOF
# Allow $HOST_USER to run these commands as $SANDVAULT_USER without password
$HOST_USER ALL=($SANDVAULT_USER) NOPASSWD: $HOST_SHELL
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
    host_cmd --msg "Write sudoers content to $SUDOERS_TMP" \
        --eval "echo \"\$SUDOERS_CONTENT\" | sudo tee '$SUDOERS_TMP' > /dev/null"
    host_cmd sudo /bin/chmod 0444 "$SUDOERS_TMP"

    if sudo visudo -c -f "$SUDOERS_TMP" &>/dev/null; then
        host_cmd sudo /bin/mv -f "$SUDOERS_TMP" "$SUDOERS_FILE"
    else
        error "Failed to create valid sudoers file"
        host_cmd sudo rm -f "$SUDOERS_TMP"
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

;; Allow writes to sandvault home, temporary directories.
;; Allow writes to devices, which are protected by unix permissions
(allow file-write*
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
        host_cmd --msg "Write sandbox profile to $SANDBOX_PROFILE" \
            --eval "echo \"\$SANDBOX_PROFILE_CONTENT\" | sudo tee '$SANDBOX_PROFILE' > /dev/null"
        host_cmd sudo /bin/chmod 0444 "$SANDBOX_PROFILE"
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
# Configure git (handled after credential/config copy below)
###############################################################################


###############################################################################
# Copy guest/home/. to sandvault $HOME
###############################################################################
if [[ "$NESTED" == "true" ]]; then
    : # Home already configured
elif [[ "$NO_BUILD" == "false" ]]; then
    debug "Configure $SANDVAULT_USER home directory..."
    host_cmd sudo "$SUDOERS_BUILD_HOME_SCRIPT_NAME"
fi

###############################################################################
# Configure permissions on sandbox home
###############################################################################
if [[ "$REBUILD" == "true" ]]; then
    configure_shared_folder_permssions true
fi


###############################################################################
# Copy host credentials, configs, and shell init scripts to sandbox
###############################################################################
if [[ "$NESTED" == "true" ]]; then
    : # Already configured
elif [[ "$REBUILD" == "true" ]]; then
    _SV_STAGING="/Users/$SANDVAULT_USER/.sv-staging"

    # Run a command as the sandbox user from a safe working directory
    _sandbox_sh() {
        (cd / && sudo -H --non-interactive --user="$SANDVAULT_USER" "$HOST_SHELL" -c "$@")
    }

    # Copy a single file from host to sandbox home via staging
    _copy_to_sandbox() {
        local src="$1" dst="$2" mode="${3:-600}"
        [[ -f "$src" ]] || return 0
        mkdir -p "$_SV_STAGING"
        cp "$src" "$_SV_STAGING/.tmp"
        chmod 644 "$_SV_STAGING/.tmp"
        _sandbox_sh "mkdir -p \"\$(dirname ~/'$dst')\" && cp '$_SV_STAGING/.tmp' ~/'$dst' && chmod $mode ~/'$dst'"
        rm -f "$_SV_STAGING/.tmp"
    }

    # Copy a directory from host to sandbox home via staging
    _copy_dir_to_sandbox() {
        local src="$1" dst="$2"
        [[ -d "$src" ]] || return 0
        mkdir -p "$_SV_STAGING/dir"
        /usr/bin/rsync --quiet --archive "$src/" "$_SV_STAGING/dir/"
        chmod -R a+rX "$_SV_STAGING/dir"
        _sandbox_sh "mkdir -p ~/'$dst' && /usr/bin/rsync --quiet --archive '$_SV_STAGING/dir/' ~/'$dst/'"
        rm -rf "$_SV_STAGING/dir"
    }

    # Merge oauthAccount from host .claude.json into sandbox .claude.json
    _merge_claude_oauth() {
        [[ -f "$HOME/.claude.json" ]] || return 0
        mkdir -p "$_SV_STAGING"
        # Read sandbox's .claude.json via sudo
        _sandbox_sh "cat ~/.claude.json 2>/dev/null" > "$_SV_STAGING/.tmp-guest" || return 0
        # Merge oauthAccount on host side
        /usr/bin/python3 -c "
import json, sys
host = json.load(open(sys.argv[1]))
guest = json.load(open(sys.argv[2]))
if 'oauthAccount' in host:
    guest['oauthAccount'] = host['oauthAccount']
    json.dump(guest, open(sys.argv[2], 'w'), indent=2)
" "$HOME/.claude.json" "$_SV_STAGING/.tmp-guest" 2>/dev/null || return 0
        # Copy merged file back
        chmod 644 "$_SV_STAGING/.tmp-guest"
        _sandbox_sh "cp '$_SV_STAGING/.tmp-guest' ~/.claude.json && chmod 600 ~/.claude.json"
        rm -f "$_SV_STAGING/.tmp-guest"
    }

    debug "Copying host credentials and configs to sandbox..."

    # Agent credentials
    host_cmd --msg "Copy ~/.claude/.credentials.json to sandbox" \
        _copy_to_sandbox "$HOME/.claude/.credentials.json" ".claude/.credentials.json"
    host_cmd --msg "Merge oauthAccount from ~/.claude.json into sandbox" \
        _merge_claude_oauth
    host_cmd --msg "Copy ~/.config/gh/hosts.yml to sandbox" \
        _copy_to_sandbox "$HOME/.config/gh/hosts.yml" ".config/gh/hosts.yml"
    host_cmd --msg "Copy ~/.config/gemini/ to sandbox" \
        _copy_dir_to_sandbox "$HOME/.config/gemini" ".config/gemini"

    # Host configs
    for f in .gitconfig .tmux.conf .vimrc .wgetrc .curlrc .inputrc; do
        host_cmd --msg "Copy ~/$f to sandbox" \
            _copy_to_sandbox "$HOME/$f" "$f" 644
    done

    # Patch .gitconfig: ensure safe.directory is set
    _patch_sandbox_gitconfig() {
        _sandbox_sh "git config --global safe.directory '/Users/$SANDVAULT_USER/*'"
    }
    host_cmd --msg "Set safe.directory=/Users/$SANDVAULT_USER/* in sandbox .gitconfig" \
        _patch_sandbox_gitconfig

    # Shell init scripts
    for f in .bashrc .bash_profile .bash_login .bash_logout .profile \
             .zshrc .zshenv .zprofile .zlogin .zlogout; do
        host_cmd --msg "Copy ~/$f to sandbox" \
            _copy_to_sandbox "$HOME/$f" "$f" 644
    done

    rm -rf "$_SV_STAGING"
fi


###############################################################################
# Write API keys to sandbox home (sourced by configure script)
# Done here (not in buildhome script) so keys are always current
###############################################################################
if [[ "$NESTED" == "false" && "$DRYRUN" != "true" ]]; then
    _write_env_key() {
        local file="/Users/$SANDVAULT_USER/.sv-env/$1"
        mkdir -p "/Users/$SANDVAULT_USER/.sv-env"
        printf 'export %s=%q\n' "$1" "$2" > "$file"
        chmod 600 "$file"
    }
    [[ -n "${OPENAI_API_KEY:-}" ]]    && _write_env_key OPENAI_API_KEY "$OPENAI_API_KEY"
    [[ -n "${ANTHROPIC_API_KEY:-}" ]] && _write_env_key ANTHROPIC_API_KEY "$ANTHROPIC_API_KEY"
    [[ -n "${GOOGLE_API_KEY:-}" ]]    && _write_env_key GOOGLE_API_KEY "$GOOGLE_API_KEY"
    [[ -n "${GEMINI_API_KEY:-}" ]]    && _write_env_key GEMINI_API_KEY "$GEMINI_API_KEY"
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
    host_cmd mkdir -p "$(dirname "$INSTALL_MARKER")"
    host_cmd --msg "date > $INSTALL_MARKER" bash -c "date > '$INSTALL_MARKER'"
fi


###############################################################################
# Run the application
###############################################################################
if [[ -n "$CLONE_REPOSITORY" ]]; then
    CLONE_SUPPORTED_COMMANDS=(shell claude codex gemini)
    for supported_command in "${CLONE_SUPPORTED_COMMANDS[@]}"; do
        if [[ "${COMMAND:-shell}" == "$supported_command" ]]; then
            break
        fi
    done
    if [[ "${COMMAND:-shell}" != "${supported_command:-}" ]]; then
        abort "--clone is only supported with: ${CLONE_SUPPORTED_COMMANDS[*]}"
    fi

    case "$(basename "${CLONE_REPOSITORY%/}")" in
        ""|/)
            abort "--clone path must include a directory name"
            ;;
        *)
            :
            ;;
    esac

    init_sandbox_run_for_repository

    if [[ -d "$CLONE_REPOSITORY" ]]; then
        LOCAL_REPOSITORY="$(cd "$CLONE_REPOSITORY" && pwd -P)"
        REPOSITORY_SOURCE_URL="$(local_repository_git remote get-url origin)"
        REPOSITORY_CLONE_SOURCE="$LOCAL_REPOSITORY"
        REPOSITORY_NAME="$(basename "$LOCAL_REPOSITORY")"
    else
        REPOSITORY_SOURCE_URL="$CLONE_REPOSITORY"
        REPOSITORY_CLONE_SOURCE="$REPOSITORY_SOURCE_URL"
        REPOSITORY_NAME="${REPOSITORY_SOURCE_URL%/}"
    fi

    [[ "$REPOSITORY_NAME" == *"://"* ]] && REPOSITORY_NAME="${REPOSITORY_NAME##*/}"
    if [[ "$REPOSITORY_NAME" == *:* ]]; then
        REPOSITORY_NAME="${REPOSITORY_NAME##*:}"
    fi
    REPOSITORY_NAME="${REPOSITORY_NAME##*/}"
    REPOSITORY_NAME="${REPOSITORY_NAME%.git}"
    if [[ -z "$REPOSITORY_NAME" ]]; then
        abort "Could not determine repository name from --clone argument"
    fi

    INITIAL_DIR="/Users/$SANDVAULT_USER/repositories/$REPOSITORY_NAME"

    USE_DIRECT_LOCAL_CLONE=false
    LOCAL_GIT_SAFE_DIRECTORY_ARGS=()
    if [[ -n "${LOCAL_REPOSITORY:-}" ]]; then
        LOCAL_GIT_SAFE_DIRECTORY_ARGS=(
            -c "safe.directory=$LOCAL_REPOSITORY"
            -c "safe.directory=$LOCAL_REPOSITORY/.git"
        )
        # Ensure sandvault user can read repository metadata before direct clone/fetch
        if "${SANDBOX_RUN[@]}" test -r "$LOCAL_REPOSITORY" \
            && "${SANDBOX_RUN[@]}" git \
                "${LOCAL_GIT_SAFE_DIRECTORY_ARGS[@]}" \
                -C "$LOCAL_REPOSITORY" rev-parse --git-dir &>/dev/null; then
            USE_DIRECT_LOCAL_CLONE=true
        fi
    fi
    # Clone into a directory writable by user and readable by sandvault-user
    (
        # Use directory that both $USER and sandvault-$USER find valid
        cd "/Users/$SANDVAULT_USER"
        "${SANDBOX_RUN[@]}" mkdir -p "$(dirname "$INITIAL_DIR")"

        if [[ "$USE_DIRECT_LOCAL_CLONE" == "true" ]]; then
            if ! "${SANDBOX_RUN[@]}" test -d "$INITIAL_DIR/.git"; then
                "${SANDBOX_RUN[@]}" git \
                    "${LOCAL_GIT_SAFE_DIRECTORY_ARGS[@]}" \
                    clone --no-hardlinks "$LOCAL_REPOSITORY" "$INITIAL_DIR"
            else
                "${SANDBOX_RUN[@]}" git \
                    "${LOCAL_GIT_SAFE_DIRECTORY_ARGS[@]}" \
                    -C "$INITIAL_DIR" fetch "$LOCAL_REPOSITORY"
            fi
        else
            mkdir -p "/Users/$SANDVAULT_USER/tmp"
            HOST_SOURCE_DIR="$(mktemp -d "/Users/$SANDVAULT_USER/tmp/sv-clone-$REPOSITORY_NAME.XXXXXX")"
            trap '
                cd "/Users/$SANDVAULT_USER"
                "${SANDBOX_RUN[@]}" git config --global --unset-all --fixed-value safe.directory "$HOST_SOURCE_DIR" || true
                "${SANDBOX_RUN[@]}" git config --global --unset-all --fixed-value safe.directory "$HOST_SOURCE_DIR/.git" || true
                rm -rf "$HOST_SOURCE_DIR"
            ' EXIT
            "${SANDBOX_RUN[@]}" git config --global --add safe.directory "$HOST_SOURCE_DIR"
            "${SANDBOX_RUN[@]}" git config --global --add safe.directory "$HOST_SOURCE_DIR/.git"

            # Clone the repo in a way that sandvault-user has access to all files (--no-hardlinks)
            git clone --mirror --no-hardlinks "$REPOSITORY_CLONE_SOURCE" "$HOST_SOURCE_DIR"
            chmod -R a+rX "$HOST_SOURCE_DIR"
            if ! "${SANDBOX_RUN[@]}" test -d "$INITIAL_DIR/.git"; then
                "${SANDBOX_RUN[@]}" git clone "$HOST_SOURCE_DIR" "$INITIAL_DIR"
            else
                "${SANDBOX_RUN[@]}" git -C "$INITIAL_DIR" fetch "$HOST_SOURCE_DIR"
            fi
        fi
    )

    if "${SANDBOX_RUN[@]}" git -C "$INITIAL_DIR" remote get-url origin &>/dev/null; then
        sandbox_repository_git remote set-url origin "$REPOSITORY_SOURCE_URL"
    else
        sandbox_repository_git remote add origin "$REPOSITORY_SOURCE_URL"
    fi

    if [[ -n "${LOCAL_REPOSITORY:-}" ]]; then
        if git -C "$LOCAL_REPOSITORY" remote get-url sandvault &>/dev/null; then
            local_repository_git remote set-url sandvault "$INITIAL_DIR"
        else
            local_repository_git remote add sandvault "$INITIAL_DIR"
        fi
    fi
fi
readonly INITIAL_DIR

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
if [[ "$NESTED" == "false" ]]; then
    sv_exit_code=0
    register_session
    trap 'sv_exit_code=$?; set +e; unregister_session; exit $sv_exit_code' EXIT
fi

# TMPDIR: claude (and perhaps other AI agents) creates temporary directories in locations
# that are shared, e.g. /tmp/claude and /private/tmp/claude, which doesn't work when there
# are multiple users running the agent on the same computer.
# Fix: set TMPDIR after the shell has started running.
SHELL_COMMAND="export TMPDIR=\$(mktemp -d); cd ~; exec $HOST_SHELL --login"

# Prepare command args as a single string
COMMAND_ARGS_STR=""
SHELL_COMMAND_MODE=false
if [[ ${#COMMAND_ARGS[@]} -gt 0 ]]; then
    printf -v COMMAND_ARGS_STR '%q ' "${COMMAND_ARGS[@]}"

    # When the user requests running shell (instead of an AI agent) then convert
    # COMMAND_ARGS to a command that will be run by the shell.
    #
    # Example: sv shell -- echo foo
    # Runs:    exec $HOST_SHELL --login -c 'echo foo'
    if [[ "$COMMAND" == "" ]]; then
        SHELL_COMMAND="$SHELL_COMMAND -c '${COMMAND_ARGS_STR}'"
        COMMAND_ARGS_STR=""
        SHELL_COMMAND_MODE=true
    fi
fi

SANDBOX_EXEC=()
if [[ "$USE_SANDBOX" == "true" ]]; then
    SANDBOX_EXEC=(/usr/bin/sandbox-exec -f "$SANDBOX_PROFILE")
else
    debug "Sandbox disabled: running without sandbox-exec restrictions"
fi

if [[ "$MODE" == "ssh" ]]; then
    # Only allocate a TTY for interactive shells.
    SSH_TTY_OPT="-t"
    if [[ ! -t 0 || "$SHELL_COMMAND_MODE" == "true" ]]; then
        SSH_TTY_OPT="-T"
    fi

    # Escape single quotes for a remote shell context.
    SHELL_COMMAND_SSH=$(printf '%s' "$SHELL_COMMAND" | sed "s/'/'\"'\"'/g")
    SHELL_COMMAND_SSH="'$SHELL_COMMAND_SSH'"

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

    # SSH requires TWO layers of shell parsing: local shell → SSH → remote shell → $HOST_SHELL
    # The extra single quotes protect the command through SSH's remote shell parsing.
    # Without them, the remote shell would word-split the command, causing incorrect execution.
    # Example: "'export TMPDIR=...'" becomes a single arg after local expansion, then the remote
    # shell strips the outer quotes, passing 'export TMPDIR=...' correctly to $HOST_SHELL -c
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
            "SHELL=$HOST_SHELL" \
            "TERM=${TERM:-}" \
            "COMMAND=$COMMAND" \
            "COMMAND_ARGS=$COMMAND_ARGS_STR" \
            "INITIAL_DIR=$INITIAL_DIR" \
            "SV_SESSION_ID=$SV_SESSION_ID" \
            "SV_VERBOSE=$SV_VERBOSE" \
            "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
            "${SANDBOX_EXEC[@]+"${SANDBOX_EXEC[@]}"}" \
            $HOST_SHELL -c "$SHELL_COMMAND_SSH"
    then
        :
    else
        exit $?
    fi
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

    # sudo requires only ONE layer of shell parsing: local shell → $HOST_SHELL
    # Simple double quotes "$SHELL_COMMAND" are sufficient because sudo passes arguments
    # directly to the command without an intermediate shell parsing layer.
    # This is different from SSH (see above) which requires extra quoting.
    if "${LAUNCHER[@]+"${LAUNCHER[@]}"}" \
        /usr/bin/env -i \
            "HOME=/Users/$SANDVAULT_USER" \
            "USER=$SANDVAULT_USER" \
            "SHELL=$HOST_SHELL" \
            "TERM=${TERM:-}" \
            "COMMAND=$COMMAND" \
            "COMMAND_ARGS=$COMMAND_ARGS_STR" \
            "INITIAL_DIR=$INITIAL_DIR" \
            "SV_SESSION_ID=$SV_SESSION_ID" \
            "SV_VERBOSE=$SV_VERBOSE" \
            "PATH=/usr/bin:/bin:/usr/sbin:/sbin" \
            "${SANDBOX_EXEC[@]+"${SANDBOX_EXEC[@]}"}" \
            $HOST_SHELL -c "$SHELL_COMMAND"
    then
        :
    else
        exit $?
    fi
fi
