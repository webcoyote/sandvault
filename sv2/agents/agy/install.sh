#!/bin/bash
# The Antigravity CLI's own installer. Runs inside the sandbox, as the
# sandbox user, and installs into the sandbox home (~/.local/bin/agy).
#
# The installer checks the download's SHA-512 against Google's manifest,
# then runs "agy install", which appends a PATH line to ~/.zshrc and
# ~/.zprofile even when ~/.local/bin is on PATH already. Those files are
# the user's, and the installer has no option to pass --skip-path on. So
# it runs with HOME set to a scratch directory, where those edits and its
# staging files land and are then removed, and with --dir naming the real
# destination. ~/.local/bin is on PATH in every session already.
set -euo pipefail
dest="$HOME/.local/bin"
/bin/mkdir -p "$HOME/.cache"
scratch="$(/usr/bin/mktemp -d "$HOME/.cache/agy-install.XXXXXX")"
trap '/bin/rm -rf "$scratch"' EXIT
/usr/bin/curl -fsSL -o "$scratch/install.sh" https://antigravity.google/cli/install.sh
HOME="$scratch" /bin/bash "$scratch/install.sh" --dir "$dest"
# The installer's last line names the scratch HOME as where agy went.
echo "agy: installed in $dest"
