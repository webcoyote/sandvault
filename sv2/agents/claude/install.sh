#!/bin/bash
# Claude Code's own installer. Runs inside the sandbox, as the sandbox user,
# and installs into the sandbox home (~/.local/bin/claude).
set -euo pipefail
/usr/bin/curl -fsSL https://claude.ai/install.sh | /bin/bash
