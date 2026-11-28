#!/bin/bash
# The ssh that git uses in a sandbox that has deploy keys.
#
# A deploy key belongs to one repository, and GitHub takes the first key it
# knows: offered the key of another repository, it answers "Permission
# denied". So the key has to be chosen per repository. git tells ssh which
# repository it wants, in the command it asks the far side to run:
#
#   ssh [options] git@github.com "git-upload-pack 'OWNER/REPO.git'"
#
# This script reads that, and when the sandbox has a key for exactly that
# repository it offers that key and no other, and checks GitHub's host key
# against the pinned one. For anything else (another repository, another
# host, a use of ssh that is not git's) it is plain ssh, unchanged.
#
# "sv2 deploy-key" installs it as ~/.ssh/sv2-deploy-keys/ssh, and the guest
# launcher points git at it with GIT_SSH_COMMAND at the start of a session.
# Nothing is written to ~/.gitconfig or ~/.ssh/config: those stay the
# user's. See docs/DEPLOY-KEYS.md.
#
# SV2_DEPLOY_KEY_DRY_RUN=1 prints the ssh command line instead of running
# it, one word per line, for the tests.
set -euo pipefail

DIR="$HOME/.ssh/sv2-deploy-keys"
SSH=/usr/bin/ssh

run() {
    if [[ "${SV2_DEPLOY_KEY_DRY_RUN:-}" == "1" ]]; then
        printf '%s\n' "$@"
        exit 0
    fi
    exec "$@"
}

# git puts the host and the command last. Fewer than two words is not that.
if [[ $# -lt 2 ]]; then
    run "$SSH" "$@"
fi
HOST="${*: -2:1}"
COMMAND="${*: -1}"

REPO=""
case "$HOST" in
    git@github.com | github.com)
        # The service, a space, the path in single quotes. A path that
        # begins with a slash is the ssh:// form of the address.
        if [[ "$COMMAND" =~ ^git-(upload-pack|receive-pack|upload-archive)\ \'/?([A-Za-z0-9-]+)/([A-Za-z0-9._-]+)\'$ ]]; then
            OWNER="${BASH_REMATCH[2]}"
            NAME="${BASH_REMATCH[3]%.git}"
            # GitHub does not tell capitals apart; the key files are named
            # in lower case.
            REPO="$(printf '%s--%s' "$OWNER" "$NAME" | /usr/bin/tr '[:upper:]' '[:lower:]')"
        fi
        ;;
    *) ;;
esac

if [[ -n "$REPO" && -f "$DIR/$REPO" ]]; then
    echo >&2 "sv2: using the deploy key for ${REPO/--//}"
    run "$SSH" -i "$DIR/$REPO" -o IdentitiesOnly=yes \
        -o UserKnownHostsFile="$DIR/known_hosts" -o StrictHostKeyChecking=yes "$@"
fi
run "$SSH" "$@"
