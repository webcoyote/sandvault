# Agentsview export from sandbox sessions

**Status:** Approved (design)
**Date:** 2026-05-03

## Motivation

Sandvault runs Claude Code, Codex, OpenCode, and Gemini inside a separate
macOS user account (`sandvault-$USER`). Their session JSONL files are written
under that user's home directory (`/Users/sandvault-$USER/.claude/projects/`,
etc.) — invisible to the host user's installation of agentsview, which
auto-discovers sessions only under the *host* user's `~/.<agent>` paths.

The result: a user who does most of their AI coding inside sandvault sees
nothing in agentsview's dashboards, search, or cost tracking, even though
agentsview already supports every agent sandvault runs. This spec closes that
gap with the lightest possible change to both projects.

## Goal

When the host user has agentsview installed, expose the sandboxed agents'
session directories to host-side agentsview through the existing shared
workspace, so the same JSONL files are read live by agentsview's watcher,
parser, and SSE pipeline — no copy, no daemon, no schema changes.

## Non-goals

- Mirroring sessions for users without agentsview installed.
- Making sandbox session data visible to anyone other than the host user.
- Pushing sessions to a remote agentsview instance.
- Modifying agentsview itself.
- Per-agent toggles (mirror Claude but not Codex). All four are mirrored or
  none are.

## Architecture

Two-side, host-driven feature:

1. **Sandbox side** (`sandvault-$USER` home): each agent's session subdirectory
   is made group-readable so the `sandvault-$USER` group (which the host user
   is a member of) can read JSONL files in place.

2. **Shared workspace** (`/Users/Shared/sv-$USER/sessions/`): one symlink per
   agent points into the sandbox user's session subdirectory. Created once at
   opt-in time during `sv setup`. This is the stable, predictable path
   agentsview reads from.

3. **Host side** (the user running `sv`): during `sv setup`, sandvault detects
   agentsview, prompts once, and on opt-in: (a) creates the symlinks, (b)
   updates the host user's agentsview `config.toml` to add the mirror paths
   to each agent's directory list, (c) records the choice so subsequent
   setups don't re-prompt.

Data flow is read-only one-way: agent writes JSONL inside the sandbox → host
agentsview reads through the symlink → live SSE updates work because it is
the same inode, not a copy.

```
sandbox writes JSONL
  /Users/sandvault-$USER/.claude/projects/<proj>/<sess>.jsonl
                              │
                              │  (group-readable; host user in sandvault group)
                              ▼
shared workspace (symlink)
  /Users/Shared/sv-$USER/sessions/claude
    → /Users/sandvault-$USER/.claude/projects
                              │
                              ▼
host agentsview reads through symlink
  ~/.agentsview/config.toml:
    claude_project_dirs = [
      "/Users/<host>/.claude/projects",            # default (preserved)
      "/Users/Shared/sv-<host>/sessions/claude",   # added by sandvault
    ]
```

## Components

### Sandbox-side (in `sv`)

**Per-agent path map.** A single source of truth used by both the sandbox-side
preparer and the host-side symlink installer:

| Agent    | Sandbox subdirectory      | Shared link name |
| -------- | ------------------------- | ---------------- |
| claude   | `.claude/projects`        | `claude`         |
| codex    | `.codex/sessions`         | `codex`          |
| opencode | `.local/share/opencode`   | `opencode`       |
| gemini   | `.gemini`                 | `gemini`         |

These match agentsview's `parser.Registry` `DefaultDirs` exactly (verified at
`agentsview/agentsview-public/internal/parser/types.go`).

**Session-dir preparation.** Implemented as a setup-merge script under
`$SHARED_WORKSPACE/setup/agentsview-export`, mirroring the existing
`gitconfig` and `claude-json` pattern. Runs as the sandbox user during
sandbox startup. For each agent in the path map:

1. `mkdir -p $HOME/<subdir>` (creates with sandbox user's umask).
2. Apply ACL granting the `sandvault-$USER` group read/list/search rights
   *with `file_inherit,directory_inherit`*, so any new JSONL files agents
   create automatically inherit group-read regardless of umask. Use the same
   `+a` ACL approach already used at line 700 of `sv` for the shared
   workspace, but with a read-only rights string:
   ```
   AGENTSVIEW_RIGHTS="group:$SANDVAULT_GROUP allow read,readattr,readextattr,readsecurity,search,list,file_inherit,directory_inherit"
   chmod +a "$AGENTSVIEW_RIGHTS" "$HOME/<subdir>"
   ```
3. Idempotent: `mkdir -p` is safe; macOS `chmod +a` deduplicates identical
   ACEs.

**Contamination pre-flight check (host side, before opt-in).** Before
showing the prompt, for each agent's subdir, `stat` the owner if it
exists. If any subdir is owned by something other than `sandvault-$USER`
(typical cause: an accidental host-side `claude` run with `HOME` pointing
at the sandbox home), abort with a clear message:

> Cannot enable agentsview export: /Users/sandvault-$USER/.claude is owned
> by <user>:<group> (expected sandvault-$USER:sandvault-$USER). This
> usually means an earlier agent run had HOME set to the sandbox home.
> Fix with:
>     sudo chown -R sandvault-$USER:sandvault-$USER /Users/sandvault-$USER/.claude
> then re-run `sv setup`.

The pre-flight runs unconditionally before the opt-in prompt. Sandvault
does not chown for the user — the user decides whether the dir contains
data they want to keep before fixing ownership.

This handles the umask question cleanly: ACLs with inherit flags are
applied at the parent dir, and macOS HFS+/APFS propagates them to all
children (including new files), so live tailing works for sessions started
*after* opt-in. Existing files written before opt-in retain their original
mode; if those happen to be `0600` and not owned by the host user, they
won't be visible. The first new session after opt-in will be visible.

### Host-side (in `sv setup`)

**Agentsview detection.** Check `command -v agentsview` *and* the existence
of `$HOME/.agentsview/`. If neither is present, skip the entire feature and
never mention it.

**One-time opt-in prompt.** Shown once during `sv setup` when agentsview is
detected and no prior choice is recorded. The prompt:

> Detected agentsview on this machine. Mirror sandvault session data so it
> appears in agentsview's dashboard, search, and cost tracking?
>
> This will:
>   - add read-only symlinks under /Users/Shared/sv-$USER/sessions/
>   - apply read-only ACLs to sandbox agent session dirs
>   - add four scan paths to ~/.agentsview/config.toml (with diff confirmation)
>   - rewrite your agentsview config without preserving comments
>
> Sandvault won't auto-track new agents agentsview adds in future versions
> (you'd re-run `sv setup --rebuild` to refresh). [y/N]

The choice is persisted in `$SHARED_WORKSPACE/setup/agentsview-export.state`
(values: `enabled`, `disabled`). Re-running `sv setup --rebuild` re-prompts
only if the state file is missing.

**Symlink installer.** On opt-in:

1. `mkdir -p /Users/Shared/sv-$USER/sessions`
2. For each agent in the path map, create a symlink
   `/Users/Shared/sv-$USER/sessions/<agent>` →
   `/Users/sandvault-$USER/<subdir>`. Idempotent (skip if already correct;
   error if exists and points elsewhere).

The host installer does *not* create the symlink target dirs. The sandbox
user owns its home and creates those dirs itself via the setup-merge
script during the next sandbox startup. Until that happens the symlinks
dangle — verified safe: agentsview's `os.ReadDir` (parser/discovery.go:168)
returns empty without error on missing dirs, and `Watcher.WatchShallow`
(sync/watcher.go:90) returns false without crashing. Empty dashboards are
the user-visible result, which is correct (no sessions to show yet).

**Agentsview config writer.** On opt-in, update
`$HOME/.agentsview/config.toml`:

- If the file does not exist, create `$HOME/.agentsview/` with `0700` perms
  and write a minimal `config.toml` containing only the four agent-dir keys,
  each set to `[default_path, mirror_path]`. Default path is the host user's
  `~/.<agent-default>` (matches agentsview's `DefaultDirs`).
- If the file exists:
  - For each agent, read the existing array under its config key
    (`claude_project_dirs`, `codex_sessions_dirs`, `opencode_dirs`,
    `gemini_dirs`). If the key is absent, the array effectively contains
    agentsview's default path — preserve it explicitly when writing back, so
    we don't *replace* defaults with only the mirror path.
  - Append the mirror path if not already present.
  - Write back with `0600` perms, preserving the rest of the file unchanged.

The writer is a small Python 3 script invoked by `sv`. Macos 14+ ships
Python 3.11+ (so `tomllib` for read is available in stdlib). For write,
sandvault vendors `tomli_w` (single-file, MIT-licensed, ~200 lines, pure
Python — no install step) at `helpers/tomli_w.py`. The script:

1. Reads `~/.agentsview/config.toml` with `tomllib.load`.
2. For each of the four agent dir keys: if absent, set to
   `[default_path, mirror_path]`; if present, append mirror_path if not
   already in the list.
3. Serializes the entire merged dict back with `tomli_w.dumps` and writes
   atomically (write to `.tmp`, `os.replace`).
4. Round-tripping discards comments and original formatting on the *whole
   file* — acceptable because the file is config, not a hand-tuned
   document; documented in the prompt text the user sees.

Diff is shown to the user before writing; user must press `y` to confirm.

### What does NOT change

- No changes to agentsview itself. Existing config schema, existing
  auto-discovery, existing watcher.
- No new long-running processes. No file watcher, no copy daemon.
- No new sandbox-exec profile changes. The sandbox already has write access
  to the sandbox user's home, and the host already has read access through
  the existing group + ACL configuration.

## Edge cases

| Case | Behavior |
| --- | --- |
| Agentsview not installed | Skip entire feature; no prompt, no config touch. |
| Agentsview installed, never run (no `~/.agentsview/`) | Create dir + minimal `config.toml`. |
| User declines | Persist `disabled`. Don't re-prompt. Re-running `sv setup --rebuild` after deleting the state file re-prompts. |
| User opts in then uninstalls agentsview | Symlinks remain (harmless). Not auto-cleaned. |
| User has manually customized an agent's dir array | Read existing array, append mirror path only if absent. Never remove user entries. |
| Sandbox not yet provisioned for an agent (e.g., `sv codex` never run) | Symlink target dir is created empty during opt-in, so the symlink is never dangling. Agentsview sees an empty dir until first use. |
| Symlink creation fails (permission, name conflict) | Print clear error, skip that agent, continue. Don't roll back others. |
| Agentsview config write fails (parse error, permission) | Abort the writer step with a clear message. Symlinks remain (they're harmless without the config update); user can fix the config and re-run `sv setup --rebuild`. |
| Multiple host users on the same machine | Each has their own `sv-$USER` shared workspace and their own sandbox user; no collision. Agentsview is per-user. |
| `sv uninstall` | Remove symlinks, remove state file, leave agentsview's `config.toml` alone (the user may have other tools depending on those entries; revert is one manual edit). |

## Testing

Sandvault's test suite is bash-based, under `tests/`. Match that pattern.

**Unit tests** (`tests/agentsview-export.bats` or equivalent):

1. Path map returns expected `(agent, subdir, link)` triples.
2. Detection function returns `0` (true) only when both `agentsview` is on
   PATH (mocked via `PATH` manipulation) *and* `$HOME/.agentsview/` exists.
3. Config writer, given a temp HOME and various initial states:
   - No file → creates dir + minimal config with all four keys.
   - File with no agent keys → adds all four keys, each `[default, mirror]`.
   - File with one agent key already containing mirror path → no change to
     that key.
   - File with one agent key containing user customizations → appends mirror
     path, preserves user entries.
   - File with unrelated keys (e.g., `host`, `port`) → unrelated keys
     unchanged after write.
4. Symlink installer:
   - Creates symlinks pointing to the right targets.
   - Re-running is idempotent (no error, no duplicate links).
   - Conflict (existing non-symlink at the path) → reports error, skips,
     continues with others.

**Integration test** (manual, documented in spec):

1. Fresh machine with sandvault and agentsview installed.
2. Run `sv setup`, accept the agentsview prompt.
3. Verify `/Users/Shared/sv-$USER/sessions/{claude,codex,opencode,gemini}`
   are symlinks.
4. Verify `~/.agentsview/config.toml` contains mirror paths for each agent.
5. Run `sv claude` with a recorded prompt; exit.
6. Run `agentsview usage daily --agent claude` and confirm the sandboxed
   session's tokens appear.

The existing test suite must keep passing. New tests should not require an
actual sandbox user — mock the path layout in a tmp dir.

## Out of scope (explicitly deferred)

- Per-agent opt-in. All four or none.
- A `sv config` CLI for flipping `agentsview-export` after initial setup. For
  now, deletion of the state file + `sv setup --rebuild` is the documented
  re-prompt path.
- Auto-cleanup of mirror paths from agentsview config on `sv uninstall`.
  Documented as a manual one-line edit.
- Native auto-discovery in agentsview (option C from brainstorming). Could
  be revisited later as a simplification, but the host-writes-config
  approach works without coordination across two repos.

## Open questions for implementation

None. Schema and integration points are confirmed against the agentsview
source in `agentsview/agentsview-public/internal/{config,parser}/`.
