---
name: sv
description: Clone the current repo into sandvault and launch Claude inside the sandbox to continue working on the current task. Use when the user wants to hand off work to a sandboxed Claude with deploy key access.
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

Write the task summary to `/Users/Shared/sv-$USER/handoff.md`. When `sv-clone` runs, it copies this file into the cloned repo as `CLAUDE.md` (after git clone but before launching Claude) and deletes the original. Claude discovers it naturally on startup. The file only exists as an untracked file in the clone — never committed, never in the source repo.

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

The launch command should play the animation first, then run sv-clone:

```
bash <skill-directory>/sv-vibes.sh && sv-clone <repo-path> -- claude
```

If `sv-clone` is not on PATH, fall back to `sv claude --clone <repo-path>`.

**For iTerm2:**

```bash
osascript <<'EOF'
tell application "iTerm2"
    activate
    set newWindow to (create window with default profile)
    tell current session of newWindow
        write text "bash <skill-directory>/sv-vibes.sh && sv-clone <repo-path> -- claude"
    end tell
end tell
EOF
```

**For Terminal.app:**

```bash
osascript -e 'tell application "Terminal" to activate' -e 'tell application "Terminal" to do script "bash <skill-directory>/sv-vibes.sh && sv-clone <repo-path> -- claude"'
```
