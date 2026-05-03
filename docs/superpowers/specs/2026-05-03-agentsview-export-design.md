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

**Session-dir preparation.** On every sandbox launch (when agentsview export
is enabled), ensure each agent's sandbox subdirectory exists and apply
group-read permissions (`chmod -R g+rX`) so the host user can read through
the symlink. Idempotent. Implemented as a setup-merge script under
`$SHARED_WORKSPACE/setup/agentsview-export`, mirroring the existing
`gitconfig` and `claude-json` pattern.

### Host-side (in `sv setup`)

**Agentsview detection.** Check `command -v agentsview` *and* the existence
of `$HOME/.agentsview/`. If neither is present, skip the entire feature and
never mention it.

**One-time opt-in prompt.** Shown once during `sv setup` when agentsview is
detected and no prior choice is recorded. The prompt:

> Detected agentsview on this machine. Mirror sandvault session data so it
> appears in agentsview's dashboard, search, and cost tracking? This adds
> read-only paths to your agentsview config and makes sandbox session
> directories group-readable to your user. [y/N]

The choice is persisted in `$SHARED_WORKSPACE/setup/agentsview-export.state`
(values: `enabled`, `disabled`). Re-running `sv setup --rebuild` re-prompts
only if the state file is missing.

**Symlink installer.** On opt-in:

1. `mkdir -p /Users/Shared/sv-$USER/sessions`
2. For each agent in the path map, ensure the target subdirectory exists
   inside the sandbox user's home (so the symlink is never dangling), then
   create the symlink. Both steps idempotent.

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

The writer is a small, self-contained Python 3 script invoked by `sv` (Python
3 is part of macOS's command-line tools and already required transitively by
sandvault's existing dependencies). TOML parsing/serialization uses the
stdlib `tomllib` (read) and a minimal hand-written serializer for the array
update — we only ever rewrite four top-level array keys, so we don't need a
full round-trip writer that preserves comments/formatting. The script reads
the file, applies the four updates, and writes back. If the file contains
TOML constructs we can't safely round-trip (rare for a config file), the
script aborts with a clear message and tells the user the manual edit
needed.

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
