# Implementation plan: agentsview export

Companion to `2026-05-03-agentsview-export-design.md`.

## Decomposition

Five units, two of which can run in parallel as independent Sonnet workers.
Sequential parts stay on the main thread.

| # | Unit | Worker | Depends on |
|---|------|--------|------------|
| 1 | `helpers/agentsview-config.py` — TOML config writer (with vendored `tomli_w.py`) | Sonnet | — |
| 2 | `helpers/agentsview-paths.sh` — sourceable path map (single source of truth) | inline | — |
| 3 | `sv` integration — detection, pre-flight, prompt, symlink installer, config writer invocation | inline | 1, 2 |
| 4 | `sv` integration — sandbox-side setup-merge script | inline | 2 |
| 5 | `tests/test-agentsview-export.sh` — bats-style test using existing test harness | Sonnet | 1, 3, 4 |

Units 1 and 5 are large and self-contained — perfect for Sonnet workers.
Units 2, 3, 4 are tightly coupled edits to `sv` and one small new sourceable
file; doing them inline keeps the diff coherent.

## Step-by-step

### Step A — Path map (inline, ~10 min)

Create `helpers/agentsview-paths.sh` exporting:

```bash
# Agent → (sandbox subdir, mirror link name) tuples for agentsview export.
# Single source of truth for both host-side and sandbox-side scripts.
AGENTSVIEW_AGENTS=(claude codex opencode gemini)
AGENTSVIEW_SUBDIR_claude=".claude/projects"
AGENTSVIEW_SUBDIR_codex=".codex/sessions"
AGENTSVIEW_SUBDIR_opencode=".local/share/opencode"
AGENTSVIEW_SUBDIR_gemini=".gemini"
AGENTSVIEW_TOMLKEY_claude="claude_project_dirs"
AGENTSVIEW_TOMLKEY_codex="codex_sessions_dirs"
AGENTSVIEW_TOMLKEY_opencode="opencode_dirs"
AGENTSVIEW_TOMLKEY_gemini="gemini_dirs"
AGENTSVIEW_DEFAULT_claude=".claude/projects"   # joined with $HOME
AGENTSVIEW_DEFAULT_codex=".codex/sessions"
AGENTSVIEW_DEFAULT_opencode=".local/share/opencode"
AGENTSVIEW_DEFAULT_gemini=".gemini"
```

(Subdir and default are identical here, but they're conceptually distinct
— subdir is a sandbox path, default is a host path. Keep separate to avoid
future confusion.)

### Step B — TOML config writer (Sonnet worker)

Dispatch a Sonnet subagent to produce two files:

- `helpers/tomli_w.py`: vendored copy of `tomli_w` v1.2.0 from
  https://github.com/hukkin/tomli-w (MIT, ~200 lines, single file). Add a
  header comment: vendored from upstream, MIT-licensed, full license
  preserved.
- `helpers/agentsview-config.py`: stdin/CLI-driven script that:
  - Takes args: `--config-path PATH --home PATH --agent KEY=VAL ...`
    (e.g. `--agent claude_project_dirs=/Users/Shared/sv-jesse/sessions/claude`)
  - Reads the config file with `tomllib` (or starts from `{}` if absent).
  - For each `--agent KEY=PATH` arg: ensure `KEY` is a list; ensure `PATH`
    and the host default for that key are present; preserve existing
    entries.
  - Defaults map (matching the design): claude → `~/.claude/projects`,
    codex → `~/.codex/sessions`, opencode → `~/.local/share/opencode`,
    gemini → `~/.gemini`. Use `--home` to compute, don't read env.
  - Modes: `--diff` prints unified diff of new vs existing TOML and exits.
    `--write` writes the new TOML atomically with `0600` perms.
  - Exit nonzero with a clear stderr message on parse error.

Worker prompt template at end of plan.

### Step C — sv integration: host side (inline)

Edit `sv` to add, after the existing config-merge script section
(line ~1437):

1. Source `helpers/agentsview-paths.sh`.
2. New function `agentsview_detect`: returns 0 if `command -v agentsview`
   succeeds OR `[[ -d "$HOME/.agentsview" ]]`.
3. New function `agentsview_contamination_check`: for each agent's subdir,
   if it exists, `stat -f "%Su"` it; if owner != `$SANDVAULT_USER`, print
   the spec's pre-flight error and return 1.
4. New function `agentsview_install_symlinks`: `mkdir -p
   $SHARED_WORKSPACE/sessions`; for each agent, create the symlink (skip
   if already correct, error if exists pointing elsewhere).
5. New function `agentsview_update_config`: invoke
   `helpers/agentsview-config.py --diff`, show to user, prompt yes/no, run
   with `--write` on yes.
6. New function `agentsview_setup`: orchestrates 2→3→prompt→4→5,
   persisting state to `$SHARED_WORKSPACE/setup/agentsview-export.state`
   (`enabled` or `disabled`). Skip silently if state file exists; only
   prompt when it's missing.
7. Call `agentsview_setup` from the `REBUILD` branch, after the existing
   config-merge script writes (so the sandbox-side merge script is in
   place before the symlinks are followed).

### Step D — sv integration: sandbox side (inline)

In the existing config-merge writer block (around line 1407), add a third
heredoc creating `$SHARED_WORKSPACE/setup/agentsview-export`:

```bash
cat > "$SHARED_WORKSPACE/setup/agentsview-export" << SETUP_EOF
#!/bin/bash
set -Eeuo pipefail
# Only act if host opted in
if [[ ! -f "$SHARED_WORKSPACE/setup/agentsview-export.state" ]]; then
    exit 0
fi
if [[ "\$(cat "$SHARED_WORKSPACE/setup/agentsview-export.state")" != "enabled" ]]; then
    exit 0
fi
RIGHTS="group:$SANDVAULT_GROUP allow read,readattr,readextattr,readsecurity,search,list,file_inherit,directory_inherit"
for subdir in $(printf '%s ' "${AGENTSVIEW_AGENTS[@]/#/}" \
    | xargs -n1 -I{} eval echo "\$AGENTSVIEW_SUBDIR_{}"); do
    full="\$HOME/\$subdir"
    mkdir -p "\$full"
    chmod +a "\$RIGHTS" "\$full" 2>/dev/null || true
done
SETUP_EOF
chmod +x "$SHARED_WORKSPACE/setup/agentsview-export"
```

(The exact templating of the agent list will need care — heredoc + array
expansion is fiddly. Inline-expand to a fixed list of four `mkdir`/`chmod`
pairs in the heredoc body for clarity. Path map is still
single-sourced via `helpers/agentsview-paths.sh` at the *generator* level.)

The `configure` script that runs on sandbox startup must invoke this new
script. Find where existing setup scripts run (grep for `gitconfig` and
`claude-json` invocations) and add `agentsview-export`.

### Step E — Tests (Sonnet worker)

Look at existing tests under `tests/` for conventions, then add
`tests/test-agentsview-export.sh` covering:

- `agentsview-config.py` unit tests using `python3 -m pytest` style or
  plain assertions in a tmp dir: missing file, empty file, file with
  prior agent dirs, file with user customizations, file with unrelated
  keys preserved.
- Path map sanity: every agent has matching subdir + tomlkey + default.
- Symlink installer behavior with mocked `$SHARED_WORKSPACE` and sandbox
  home in a tmp dir (no real sandbox user needed).

Worker prompt template at end of plan.

### Step F — Manual verification (inline)

After tests pass:
1. Run `sv setup --rebuild` on this machine.
2. Verify the contamination pre-flight catches the existing
   `/Users/sandvault-jesse/.claude` ownership.
3. Document the manual `sudo chown` step in the test plan section of the
   PR body (don't fix it for the user).

### Step G — PR (inline)

Open a draft PR per Jesse's defaults:
- Title: `Mirror sandbox sessions to host agentsview`
- Body uses the standard structure (Motivation, Summary, Test plan).
- `--draft` flag; no Claude attribution.

## Order

A → (B + Step E start in parallel) → C → D → finish E → F → G

(B and Step E need agentsview-config.py to test against — start E once B's
prompt is dispatched but expect to wait for B's deliverable.)

## Worker prompts

### B — TOML config writer

```
You are implementing a small Python helper for the sandvault project. Read
the design spec at
/Users/jesse/GitHub/sandvault/docs/superpowers/specs/2026-05-03-agentsview-export-design.md
sections "Agentsview config writer" and "Edge cases" for full context.

Two deliverables:

1. /Users/jesse/GitHub/sandvault/helpers/tomli_w.py — vendored copy of
   tomli_w v1.2.0 from https://github.com/hukkin/tomli-w/blob/master/src/tomli_w/_writer.py
   (and __init__.py contents). Single .py file. Preserve the MIT license
   header at the top. Add a one-line "Vendored from upstream by sandvault"
   comment.

2. /Users/jesse/GitHub/sandvault/helpers/agentsview-config.py — Python 3
   script with this CLI:

     agentsview-config.py --config-path PATH --home PATH \
         [--diff | --write] \
         --agent KEY=MIRROR_PATH [--agent KEY=MIRROR_PATH ...]

   Behavior:
   - Imports tomllib (stdlib, Python 3.11+) and tomli_w (sibling file).
   - Reads --config-path TOML; if missing, starts from empty dict.
   - For each --agent KEY=MIRROR_PATH:
     * KEY must be one of: claude_project_dirs, codex_sessions_dirs,
       opencode_dirs, gemini_dirs (else exit 2 with message).
     * Compute the default host path for that KEY, joined with --home:
         claude_project_dirs → <home>/.claude/projects
         codex_sessions_dirs → <home>/.codex/sessions
         opencode_dirs       → <home>/.local/share/opencode
         gemini_dirs         → <home>/.gemini
     * Get the existing list under KEY (or empty list if absent).
     * If empty, set to [default_path, mirror_path].
     * If non-empty, append default_path if absent, then append mirror_path
       if absent. Preserve order of existing entries.
   - --diff: serialize updated dict with tomli_w.dumps; print unified diff
     vs existing file (or empty string if absent); exit 0.
   - --write: serialize and write atomically (write to PATH.tmp, fchmod
     0600, os.replace). Create parent dir with 0700 if missing. Exit 0.
   - On TOML parse error: write a clear error to stderr including the
     file path and exception message; exit 3.

   Add a --self-test mode that runs in-process tests against a tmp dir
   and exits 0 on success. Cover: missing file, empty file, file with
   one key already containing the mirror path (no change), file with
   user-added entries (preserve), file with unrelated top-level keys
   (preserve), atomic write doesn't truncate on failure.

Both files must be Python 3.11-compatible (no walrus-only-3.12 features,
no 3.12+ syntax). No external pip dependencies — only stdlib + the
vendored tomli_w sibling.

Run `python3 helpers/agentsview-config.py --self-test` and confirm it
exits 0 before reporting done.

Report deliverable file paths and confirm self-test passed.
```

### E — Tests

```
You are adding tests for the agentsview export feature in sandvault.
Read the design spec
/Users/jesse/GitHub/sandvault/docs/superpowers/specs/2026-05-03-agentsview-export-design.md
and the implementation plan
/Users/jesse/GitHub/sandvault/docs/superpowers/specs/2026-05-03-agentsview-export-plan.md
for context.

The TOML writer at /Users/jesse/GitHub/sandvault/helpers/agentsview-config.py
already has a --self-test mode covering its own logic. Your tests cover
the integration: path map consistency and the symlink-installer logic.

First, examine /Users/jesse/GitHub/sandvault/tests/ to understand the
existing test conventions (file naming, test runner, how tests are
invoked from CI/Makefile). Match those conventions exactly.

Then add tests covering:

1. helpers/agentsview-paths.sh: every agent in AGENTSVIEW_AGENTS has all
   four associated keys defined (subdir, tomlkey, default).

2. The symlink installer logic in sv (you may need to extract it into a
   small testable function if it isn't already). Tests in a tmp dir,
   mocking $SHARED_WORKSPACE and the sandbox home:
   - First run: creates all four symlinks pointing to the right targets.
   - Re-run: idempotent (no errors, links unchanged).
   - Conflict: existing regular file at the path → reports error,
     skips, continues with others.

3. End-to-end smoke test (skippable in CI without root): invoke
   agentsview-config.py with realistic --agent args against a tmp
   ~/.agentsview/config.toml; verify the resulting file parses cleanly
   with tomllib and contains expected entries.

Do not add tests that require an actual sandvault-$USER on the test
machine. Mock the path layout in a tmp dir.

Run the existing test suite plus your new tests and confirm both pass
before reporting done.

Report: test file paths, test runner command, output of running the
relevant new tests, and any conventions you adapted.
```
