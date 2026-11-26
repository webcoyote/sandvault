#!/bin/bash
# Makes a deploy key for one GitHub repository, in the sandbox home.
#
# Runs inside the sandbox, as the sandbox user. sv2 holds this file's text
# and passes it on the command line:
#
#   /bin/bash -c "$(this file)" sv2-deploy-key OWNER REPO WRAPPER
#
# WRAPPER is the text of guest/git-ssh.sh, the ssh that chooses the key.
#
# Standard output is the public key and nothing else: sv2 reads it back and
# treats it as untrusted. Everything said to the user goes to standard error.
#
# The private key never leaves the sandbox home. Whether the key may write to
# the repository is decided on GitHub, when the user adds the public key.
#
# Everything is kept in one directory, ~/.ssh/sv2-deploy-keys. Nothing is
# written to ~/.gitconfig or ~/.ssh/config: those are the user's, and may be
# replaced wholesale. See docs/DEPLOY-KEYS.md.
set -euo pipefail
umask 077

if [[ $# -ne 3 ]]; then
    echo >&2 "sv2: deploy-key: expected OWNER REPO WRAPPER"
    exit 2
fi
OWNER="$1"
REPO="$2"
WRAPPER="$3"
DIR="$HOME/.ssh/sv2-deploy-keys"
# In lower case: GitHub does not tell capitals apart, and the wrapper looks
# the key up by the name git asks for.
KEY="$DIR/$(printf '%s--%s' "$OWNER" "$REPO" | /usr/bin/tr '[:upper:]' '[:lower:]')"

/bin/mkdir -p "$DIR"
/bin/chmod 700 "$HOME/.ssh" "$DIR"

if [[ ! -f "$KEY" || ! -f "$KEY.pub" ]]; then
    /bin/rm -f "$KEY" "$KEY.pub"
    /usr/bin/ssh-keygen -q -t ed25519 -N '' -C "sv2 deploy key for $OWNER/$REPO" -f "$KEY" >&2
fi

# GitHub's host key, pinned: an agent cannot answer ssh's question about a
# host it has not seen, and should not be asked to trust whatever answers.
# https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/githubs-ssh-key-fingerprints
/bin/cat > "$DIR/known_hosts" <<'KNOWN'
github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl
KNOWN

# The wrapper, written afresh each time so that it is the one this sv2 has.
# The guest launcher finds it here and sets GIT_SSH_COMMAND to it, so it
# takes effect in sessions started from now on.
TMP="$(/usr/bin/mktemp "$DIR/.ssh.XXXXXX")"
printf '%s' "$WRAPPER" > "$TMP"
/bin/chmod 700 "$TMP"
/bin/mv -f "$TMP" "$DIR/ssh"

/bin/cat "$KEY.pub"
