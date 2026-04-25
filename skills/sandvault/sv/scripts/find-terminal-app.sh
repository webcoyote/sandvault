#!/usr/bin/env bash
set -Eeuo pipefail

# Walk up the process tree to find the first ancestor running from /Applications
pid=$$
while [[ "$pid" -gt 1 ]]; do
    pid=$(ps -o ppid= -p "$pid" | tr -d ' ')
    exe=$(ps -o command= -p "$pid" 2>/dev/null) || break
    if [[ "$exe" == /Applications/* ]]; then
        # Extract the .app bundle name without the /Applications/ prefix
        app=$(echo "$exe" | sed -n 's|^/Applications/\([^/]*\.app\).*|\1|p')
        echo "${app:-$exe}"
        exit 0
    fi
done

echo "No parent application found in /Applications" >&2
exit 1
