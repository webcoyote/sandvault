#!/bin/bash
# herdr's wrapper: runs inside the sandbox, as the sandbox user, in place of
# herdr. $1 is the herdr that sv2 found; the rest are its arguments.
#
# The server herdr starts outlives this session, and its panes keep the
# environment of the session that started it. A browser of sv2's is
# stopped when its session ends, so a pane would be left with an
# SV_BROWSER_ENDPOINT where nothing listens (and a session that only
# attaches gives its browser to no pane). v1 refused --chrome with herdr
# for the same reason. Run an agent with a browser in a session of its own.
set -euo pipefail
if [[ -n "${SV_BROWSER_ENDPOINT:-}" ]]; then
    echo >&2 "sv2: herdr cannot run with --chrome or --lightpanda: its server outlives this session, and its panes would keep the address of a browser that stops when the session ends. Run herdr without one, and an agent that needs a browser in a session of its own (sv2 --lightpanda claude)."
    exit 2
fi
exec "$@"
