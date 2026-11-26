#!/bin/bash
# pi's installer, through npm. Runs inside the sandbox, as the sandbox
# user, and installs into the sandbox home (~/.local/bin/pi).
#
# The prefix is ~/.local and not the home itself: a directory named
# node_modules in the home makes Bun stop installing packages on demand for
# everything below it (webcoyote/sandvault#169).
set -euo pipefail
if ! command -v npm >/dev/null; then
    echo >&2 "pi: npm was not found in the sandbox. Install Node.js on this Mac (brew install node) and try again."
    exit 1
fi
npm install --prefix "$HOME/.local" -g @earendil-works/pi-coding-agent
