---
name: sv
description: This skill should be used when the user invokes `/sv` or asks to "hand this off to sandvault", "continue in the sandbox", "sandvault this task", or to clone the current repo into a sandboxed Claude session with per-repo deploy-key access. Writes a task briefing to the sandvault shared workspace and launches `sv-clone` in a new terminal window, pointing the sandboxed Claude at the briefing as its first prompt.
---

# Sandvault Handoff

Hand off the current task to a sandboxed Claude running inside sandvault.

## When to use

The user invokes `/sv` when they want Claude to continue the current work inside a sandvault sandbox.

## Steps

### 1. Summarize the task

Write a clear, actionable summary of what the user wants done. Include:
- What the goal is
- What approach to take (if already discussed)
- What files are involved
- Any decisions already made in this conversation
- The current branch name and any relevant context

### 2. Write the handoff file

Write the task briefing to `/Users/Shared/sv-$USER/handoff-<repo>.md`,
where `$USER` is the host user and `<repo>` is the source repo's
basename (e.g. `/Users/Shared/sv-jesse/handoff-sandvault.md`). This is
the sandvault shared workspace — readable from inside the sandbox via
the mounted `/Users/Shared` tree, so the sandboxed Claude can read it
at startup. The briefing is **not** copied into the clone; it lives in
the shared workspace and the sandboxed Claude is pointed at it by path
(see step 4). Nothing is written into the source repo itself.

The handoff file should include:
- A "# Task Handoff" heading
- Task context, approach, decisions, branch, relevant files
- A "## Setup" section: note that gitignored build artifacts (`.venv/`, `node_modules/`, build dirs) won't be in the clone. Check for `requirements.txt`, `pyproject.toml`, `package.json` etc. and instruct the sandboxed Claude to set up the environment first.
- A "## What to do" section with a direct instruction

### 3. Ask the user for confirmation

Show the user:
- The repo that will be cloned
- A brief summary of the task being handed off

### 4. Launch in a new terminal window

Detect the user's default terminal and launch `sv-clone` in a new window.

**Detect the default terminal:**

```bash
defaults read com.apple.LaunchServices/com.apple.launchservices.secure LSHandlers 2>/dev/null | grep -A1 "LSHandlerRoleShell" | grep -o '"[^"]*"' | tr -d '"'
```

Look for `com.googlecode.iterm2` (iTerm2) or `com.apple.Terminal` (Terminal.app). If detection fails, default to Terminal.app.

The launch command runs `sv-clone`, passing the handoff path to the
sandboxed Claude as its initial prompt via the `--` separator
pass-through (`sv-clone` → `sv` → sandbox zshrc → `claude`):

```
sv-clone <repo-path> -- claude -- "Read /Users/Shared/sv-<host-user>/handoff-<repo>.md and continue the task described there."
```

Substitute `<host-user>` (the host user's login — same `$USER` used
when writing the handoff file) and `<repo>` (the source repo's
basename) literally into the prompt string.

If `sv-clone` is not on PATH, the installed sandvault is out of date — tell the user to upgrade (e.g. `brew upgrade sandvault`) and retry. The old `sv --clone` flag has been removed in favor of the standalone `sv-clone` script.

**For iTerm2:**

```bash
osascript <<'EOF'
tell application "iTerm2"
    activate
    set newWindow to (create window with default profile)
    tell current session of newWindow
        write text "sv-clone <repo-path> -- claude -- \"Read /Users/Shared/sv-<host-user>/handoff-<repo>.md and continue the task described there.\""
    end tell
end tell
EOF
```

**For Terminal.app:**

```bash
osascript -e 'tell application "Terminal" to activate' -e 'tell application "Terminal" to do script "sv-clone <repo-path> -- claude -- \"Read /Users/Shared/sv-<host-user>/handoff-<repo>.md and continue the task described there.\""'
```
