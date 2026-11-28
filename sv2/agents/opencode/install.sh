#!/bin/bash
# opencode's own installer. Runs inside the sandbox, as the sandbox user, and
# installs into the sandbox home (~/.opencode/bin/opencode).
#
# --no-modify-path: the installer would otherwise append to the shell's
# startup files, which are the user's.
set -euo pipefail
/usr/bin/curl -fsSL https://opencode.ai/install | /bin/bash -s -- --no-modify-path
