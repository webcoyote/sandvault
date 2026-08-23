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

###############################################################################
# Test 7: agentsview_resync_config with a missing mirror, non-interactive,
# reaches the non-interactive skip guard -- NOT some other exit-0 path.
# This is the regression codex caught in iter 2 (capture bug returned 1) and
# the one codex+claude-code re-raised in iter 3: "exit 0 + no-write +
# no-decline" is true for every exit-0 path (all-up-to-date, all-declined,
# missing-script, skip-guard), so it doesn't prove the pending branch ran.
#
# Observe the branch directly: run with SV_VERBOSE=2 (enables `trace`) and
# capture stderr, then assert the skip guard's trace message names the
# missing key. Only the pending-skip path emits "config changes pending ... 
# non-interactive; skipping" with the key in it. A bug where --check reports
# clean (no keys) reaches a different branch; a missing script returns early
# with a different trace; the all-up-to-date path has no missing mirror.
###############################################################################
cat > "$CFG" <<EOF
claude_project_dirs = ["$HOME/.claude/projects", "$SV_PRIVATE_DIR/sessions/claude"]
codex_sessions_dirs = ["$HOME/.codex/sessions", "$SV_PRIVATE_DIR/sessions/codex"]
opencode_dirs = ["$HOME/.local/share/opencode", "$SV_PRIVATE_DIR/sessions/opencode"]
gemini_dirs = ["$HOME/.gemini", "$SV_PRIVATE_DIR/sessions/gemini"]
pi_dirs = ["$HOME/.pi/agent/sessions"]
EOF
rm -f "$DECLINED"
before=$(cat "$CFG")
st=0; log=$(SV_VERBOSE=2 agentsview_resync_config </dev/null 2>&1) || st=$?
[[ "$st" -eq 0 ]] \
    || fail "pending+non-interactive should skip (exit 0), got exit $st"
# The skip guard's trace must name pi_dirs -- proves --check reported it
# missing AND the pending branch ran (not all-up-to-date, not missing-script).
[[ "$log" == *"config changes pending"*"pi_dirs"*"non-interactive; skipping"* ]] \
    || fail "pending-skip trace not found; got: $log"
# ... and it did not write or decline.
[[ "$(cat "$CFG")" == "$before" ]] \
    || fail "pending+non-interactive should not write the config"
[[ ! -e "$DECLINED" ]] \
    || fail "pending+non-interactive should not record a decline (file exists)"
pass "resync: pending+non-interactive reaches skip guard (observed via trace)"
set +e

###############################################################################
# Test 8: --check exit 1 with empty stdout (a protocol violation, e.g. an
# import-time error in the vendored shim) is a hard error that surfaces the
# stderr diagnostic -- not silent success, and not a generic message that
# hides the cause. Uses the AGENTSVIEW_CONFIG_SCRIPT seam to point resync at a
# stub that exits 1 with empty stdout and a known stderr line.
###############################################################################
cat > "$CFG" <<EOF
claude_project_dirs = ["$HOME/.claude/projects", "$SV_PRIVATE_DIR/sessions/claude"]
EOF
rm -f "$DECLINED"
stub="$TMP_HOME/stub.py"
cat > "$stub" <<'PY'
import sys
sys.stderr.write("boom: import-time traceback marker\n")
sys.exit(1)  # exit 1, no stdout keys
PY
before=$(cat "$CFG")
st=0; out=$( trap - ERR; AGENTSVIEW_CONFIG_SCRIPT="$stub" agentsview_resync_config </dev/null 2>&1 ) || st=$?
[[ "$st" -eq 1 ]] \
    || fail "empty-keys protocol violation should return 1, got $st"
[[ "$out" == *"reported pending but listed no keys"* ]] \
    || fail "empty-keys error message missing; got: $out"
[[ "$out" == *"boom: import-time traceback marker"* ]] \
    || fail "empty-keys error should surface the stderr diagnostic; got: $out"
# And it did not silently write or decline.
[[ "$(cat "$CFG")" == "$before" ]] || fail "empty-keys error should not write"
[[ ! -e "$DECLINED" ]] || fail "empty-keys error should not record a decline"
pass "resync: empty-keys protocol violation errors with the stderr diagnostic"
set +e

###############################################################################
# Test 9: the empty-keys error when --check violates protocol with NO stderr
# (a bare sys.exit(1), empty stdout, empty stderr). Must still error (return
# 1) but must NOT emit a bare noisy diagnostic line -- only the message.
###############################################################################
cat > "$CFG" <<EOF
claude_project_dirs = ["$HOME/.claude/projects", "$SV_PRIVATE_DIR/sessions/claude"]
EOF
rm -f "$DECLINED"
stub2="$TMP_HOME/stub2.py"
cat > "$stub2" <<'PY'
import sys
sys.exit(1)  # exit 1, no stdout, no stderr
PY
before=$(cat "$CFG")
st=0; out=$( trap - ERR; AGENTSVIEW_CONFIG_SCRIPT="$stub2" agentsview_resync_config </dev/null 2>&1 ) || st=$?
[[ "$st" -eq 1 ]] \
    || fail "empty-keys (no stderr) should return 1, got $st"
[[ "$out" == *"reported pending but listed no keys"* ]] \
    || fail "empty-keys error message missing; got: $out"
# No bare diagnostic line: the only error line is the message itself.
err_lines=$(printf '%s\n' "$out" | grep -c '❌')
[[ "$err_lines" -eq 1 ]] \
    || fail "empty-keys (no stderr) should emit exactly one error line, got $err_lines: $out"
[[ "$(cat "$CFG")" == "$before" ]] || fail "empty-keys error should not write"
pass "resync: empty-keys with no stderr errors without a bare diagnostic line"
set +e

###############################################################################
# Test 10: the AGENTSVIEW_CONFIG_SCRIPT seam — when the override points at a
# nonexistent path, resync skips (return 0) and the skip trace names the
# *overridden* path, not the installed default (which exists in this tree, so
# the old hardcoded message would name a present, irrelevant file).
###############################################################################
cat > "$CFG" <<EOF
claude_project_dirs = ["$HOME/.claude/projects", "$SV_PRIVATE_DIR/sessions/claude"]
EOF
rm -f "$DECLINED"
fake="$TMP_HOME/does-not-exist.py"
st=0; log=$(SV_VERBOSE=2 AGENTSVIEW_CONFIG_SCRIPT="$fake" agentsview_resync_config </dev/null 2>&1) || st=$?
[[ "$st" -eq 0 ]] \
    || fail "missing override script should skip (exit 0), got $st"
[[ "$log" == *"$fake not installed; skipping config resync"* ]] \
    || fail "skip trace should name the overridden path '$fake'; got: $log"
# And it must NOT name the default path (which exists here) -- the bug was that
# it silently named a present, irrelevant file.
[[ "$log" != *"helpers/agentsview-config.py not installed"* ]] \
    || fail "skip trace should not name the default path; got: $log"
pass "resync: not-installed trace names the overridden script path"
set +e

echo
echo "All integration tests passed."
