#!/bin/bash
# herdr's own installer. Runs inside the sandbox, as the sandbox user, and
# installs into the sandbox home (~/.local/bin/herdr).
#
# HERDR_INSTALL_DIR is the installer's default, given so that the program
# lands where agent.toml looks for it whatever the installer's defaults
# become. The installer checks the download's SHA-256 and only prints a
# hint about PATH; it does not edit .zshrc or .zprofile.
set -euo pipefail
/usr/bin/curl -fsSL https://herdr.dev/install.sh \
    | HERDR_INSTALL_DIR="$HOME/.local/bin" /bin/sh
