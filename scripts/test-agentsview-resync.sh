#!/bin/bash
# Integration test for the agentsview re-sync + per-key decline flow.
# Sources the REAL sv-agentsview-setup (functions only) and drives the REAL
# agentsview-config.py, with HOME/USER pointed at throwaway paths under
# /Users/Shared so the readonly SV_PRIVATE_DIR resolves somewhere we can
# create and clean up.
#
# The sourced script sets `set -Eeuo pipefail` and an ERR trap, and its
# functions toggle set -e internally -- which leaks into this shell. So we
# keep errexit off after sourcing and capture expected-failure statuses with
# `cmd || st=$?`, which also runs in a conditional context (no abort, ERR
# trap suppressed).
# shellcheck disable=SC2154  # SV_PRIVATE_DIR, AGENTSVIEW_*, agentsview_field come from sourcing sv-agentsview-setup + agentsview-paths.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PUB="$SCRIPT_DIR/.."

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "PASS: $1"; }

# Unique fake host user so SHARED_WORKSPACE=/Users/Shared/sv-<fake> is ours.
FAKE_USER="avtest$$"
export USER="$FAKE_USER"
TMP_HOME="/tmp/avtest.home.$$"
mkdir -p "$TMP_HOME/.agentsview"
export HOME="$TMP_HOME"

cleanup() { rm -rf "/Users/Shared/sv-$FAKE_USER" "$TMP_HOME"; }
trap cleanup EXIT

# Source with --help so the entry-point case runs print_usage (harmless) and
# returns, leaving every function defined and the readonly path vars set.
# shellcheck source=/dev/null
source "$PUB/sv-agentsview-setup" --help >/dev/null
set +e   # the sourced script turned errexit on; we drive it ourselves

# Sanity: new function present, old one gone.
declare -F agentsview_resync_config >/dev/null || fail "agentsview_resync_config not defined"
if declare -F agentsview_update_config >/dev/null; then fail "old agentsview_update_config still present"; fi
declare -F agentsview_record_decline >/dev/null || fail "agentsview_record_decline not defined"

CFG="$AGENTSVIEW_HOST_CONFIG"
DECLINED="$AGENTSVIEW_DECLINED_FILE"
PY="$PUB/helpers/agentsview-config.py"

mkdir -p "$(dirname "$AGENTSVIEW_STATE_FILE")"
echo enabled > "$AGENTSVIEW_STATE_FILE"

# Capture a command's exit status without aborting (errexit may be on or off).
status_of() { local st; "$@" >/dev/null 2>&1 || st=$?; echo "${st:-0}"; }

###############################################################################
# Test 1: resync with every mirror already present -> no-op (check exits 0)
###############################################################################
cat > "$CFG" <<EOF
claude_project_dirs = ["$HOME/.claude/projects", "$SV_PRIVATE_DIR/sessions/claude"]
codex_sessions_dirs = ["$HOME/.codex/sessions", "$SV_PRIVATE_DIR/sessions/codex"]
opencode_dirs = ["$HOME/.local/share/opencode", "$SV_PRIVATE_DIR/sessions/opencode"]
gemini_dirs = ["$HOME/.gemini", "$SV_PRIVATE_DIR/sessions/gemini"]
pi_dirs = ["$HOME/.pi/agent/sessions", "$SV_PRIVATE_DIR/sessions/pi"]
EOF
agentsview_resync_config </dev/null
[[ $? -eq 0 ]] || fail "resync (all present) should return 0"
pass "resync: all mirrors present is a no-op"
set +e

###############################################################################
# Test 2: check -> write -> check idempotence (the core of Bug 3).
###############################################################################
cat > "$CFG" <<EOF
claude_project_dirs = ["$HOME/.claude/projects"]
EOF
st=$(status_of /usr/bin/python3 "$PY" --config-path "$CFG" --home "$HOME" --check \
    --agent claude_project_dirs="$SV_PRIVATE_DIR/sessions/claude")
[[ "$st" -eq 1 ]] || fail "check should exit 1 (pending), got $st"
/usr/bin/python3 "$PY" --config-path "$CFG" --home "$HOME" --write \
    --agent claude_project_dirs="$SV_PRIVATE_DIR/sessions/claude" >/dev/null
st=$(status_of /usr/bin/python3 "$PY" --config-path "$CFG" --home "$HOME" --check \
    --agent claude_project_dirs="$SV_PRIVATE_DIR/sessions/claude")
[[ "$st" -eq 0 ]] || fail "after write, check should exit 0, got $st"
pass "resync: check->write->check is idempotent"
set +e

###############################################################################
# Test 3: decline records pending keys permanently (de-duped); resync skips.
###############################################################################
cat > "$CFG" <<EOF
claude_project_dirs = ["$HOME/.claude/projects"]
pi_dirs = ["$HOME/.pi/agent/sessions"]
EOF
rm -f "$DECLINED"
pending=()
for agent in "${AGENTSVIEW_AGENTS[@]}"; do
    pending+=( "$(agentsview_field TOMLKEY "$agent")" )
done
agentsview_record_decline "${pending[@]}"
count=$(agentsview_declined_keys | grep -c .)
[[ "$count" -eq 5 ]] || fail "declined file should have 5 keys, got $count"
agentsview_record_decline "${pending[@]}"   # de-dup
count=$(agentsview_declined_keys | grep -c .)
[[ "$count" -eq 5 ]] || fail "decline not de-duplicated, got $count"
agentsview_resync_config </dev/null
[[ $? -eq 0 ]] || fail "resync (all declined) should return 0"
pass "resync: all-declined is a no-op (decline de-duped)"
set +e

###############################################################################
# Test 4: scalar managed key -> resync errors (return 1), file untouched.
###############################################################################
cat > "$CFG" <<'EOF'
claude_project_dirs = "/some/scalar/path"
EOF
rm -f "$DECLINED"
st=0; agentsview_resync_config </dev/null || st=$?
[[ "$st" -eq 1 ]] || fail "resync on scalar config should return 1, got $st"
grep -q '^claude_project_dirs = "/some/scalar/path"$' "$CFG" \
    || fail "scalar config was rewritten (should be untouched): $(cat "$CFG")"
pass "resync: scalar config errors cleanly, file untouched"
set +e

###############################################################################
# Test 5: partial decline — decline pi only; resync filters it out so check
# is clean even though pi's mirror is missing.
###############################################################################
cat > "$CFG" <<EOF
claude_project_dirs = ["$HOME/.claude/projects", "$SV_PRIVATE_DIR/sessions/claude"]
codex_sessions_dirs = ["$HOME/.codex/sessions", "$SV_PRIVATE_DIR/sessions/codex"]
opencode_dirs = ["$HOME/.local/share/opencode", "$SV_PRIVATE_DIR/sessions/opencode"]
gemini_dirs = ["$HOME/.gemini", "$SV_PRIVATE_DIR/sessions/gemini"]
pi_dirs = ["$HOME/.pi/agent/sessions"]
EOF
rm -f "$DECLINED"
agentsview_record_decline pi_dirs
declined="$(agentsview_declined_keys)"
agent_args=()
for agent in "${AGENTSVIEW_AGENTS[@]}"; do
    tk="$(agentsview_field TOMLKEY "$agent")"
    printf '%s\n' "$declined" | grep -Fxq -- "$tk" && continue
    agent_args+=(--agent "$tk=$SV_PRIVATE_DIR/sessions/$agent")
done
st=$(status_of /usr/bin/python3 "$PY" --config-path "$CFG" --home "$HOME" --check \
    "${agent_args[@]}")
[[ "$st" -eq 0 ]] || fail "with pi declined, check should be 0 (others present), got $st"
pass "resync: declined pi is filtered out (no false pending)"
set +e

###############################################################################
# Test 6: decline records ONLY the actually-missing key, not every
# non-declined agent. 4 agents present + pi missing; assert --check reports
# only pi_dirs, and recording exactly that set leaves the others out. The
# decline-at-prompt path itself is tty-only (the ! -t 0 guard skips
# non-interactive), so this tests the logic resync uses on decline: parse
# --check's missing-key output and record only those.
###############################################################################
cat > "$CFG" <<EOF
claude_project_dirs = ["$HOME/.claude/projects", "$SV_PRIVATE_DIR/sessions/claude"]
codex_sessions_dirs = ["$HOME/.codex/sessions", "$SV_PRIVATE_DIR/sessions/codex"]
opencode_dirs = ["$HOME/.local/share/opencode", "$SV_PRIVATE_DIR/sessions/opencode"]
gemini_dirs = ["$HOME/.gemini", "$SV_PRIVATE_DIR/sessions/gemini"]
pi_dirs = ["$HOME/.pi/agent/sessions"]
EOF
rm -f "$DECLINED"
missing_out=$(/usr/bin/python3 "$PY" --config-path "$CFG" --home "$HOME" --check \
    --agent claude_project_dirs="$SV_PRIVATE_DIR/sessions/claude" \
    --agent codex_sessions_dirs="$SV_PRIVATE_DIR/sessions/codex" \
    --agent opencode_dirs="$SV_PRIVATE_DIR/sessions/opencode" \
    --agent gemini_dirs="$SV_PRIVATE_DIR/sessions/gemini" \
    --agent pi_dirs="$SV_PRIVATE_DIR/sessions/pi" 2>/dev/null || true)
trimmed=$(printf '%s\n' "$missing_out" | sed '/^$/d')
[[ "$trimmed" == "pi_dirs" ]] \
    || fail "--check should report only pi_dirs missing, got: '$trimmed'"
while IFS= read -r mk; do
    [[ -n "$mk" ]] && agentsview_record_decline "$mk"
done <<< "$missing_out"
count=$(agentsview_declined_keys | grep -c .)
[[ "$count" -eq 1 ]] || fail "decline list should have 1 key (pi_dirs), got $count"
agentsview_declined_keys | grep -Fxq -- pi_dirs \
    || fail "decline list should contain pi_dirs"
for other in claude_project_dirs codex_sessions_dirs opencode_dirs gemini_dirs; do
    if agentsview_declined_keys | grep -Fxq -- "$other"; then
        fail "decline list should NOT contain $other (M1 over-record regression)"
    fi
done
pass "resync: decline records only the missing key (not all agents)"
set +e

echo
echo "All integration tests passed."
