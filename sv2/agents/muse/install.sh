#!/bin/bash
# Muse Code's own installer. Runs inside the sandbox, as the sandbox user,
# and installs into the sandbox home (~/.local/bin/muse).
#
# MUSE_INSTALL_DIR is the installer's default, given so that the program
# lands where agent.toml looks for it whatever the installer's defaults
# become. MUSE_NO_MODIFY_PATH keeps it from editing .zshrc and .zprofile,
# which are the user's: ~/.local/bin is on PATH in every session already.
set -euo pipefail
/usr/bin/curl -fsSL https://dev.meta.ai/install.sh \
    | MUSE_INSTALL_DIR="$HOME/.local/bin" MUSE_NO_MODIFY_PATH=1 /bin/bash
