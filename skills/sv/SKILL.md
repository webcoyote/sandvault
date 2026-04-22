---
name: sv
description: Clone the current repo into sandvault and launch Claude inside the sandbox to continue working on the current task. Use when the user wants to hand off work to a sandboxed Claude with deploy key access.
---

# Sandvault Handoff

Hand off the current task to a sandboxed Claude running inside sandvault — or check on a running session and pull results back.

## Modes

Parse the user's invocation to determine the mode:

- `/sv` or `/sv handoff` — **Handoff mode** (default): summarize, write handoff, launch sandbox
- `/sv status` — **Status mode**: check on active sandbox sessions
- `/sv pull` — **Pull mode**: fetch results back from sandbox

---

## Handoff Mode

### Step 1: Pre-flight checks

Before anything else, validate the environment and offer to fix problems:

```bash
# Is sandvault installed?
command -v sv >/dev/null 2>&1
# Is sv-clone available?
command -v sv-clone >/dev/null 2>&1
```

If `sv` is not found, tell the user: "sandvault is not installed. Install with `brew install sandvault` or clone from GitHub."

Then gather environment info:

```bash
REPO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null)
REPO_NAME=$(basename "$REPO_ROOT")
BRANCH=$(git branch --show-current)
HOST_USER=$(whoami)
SHARED_WS="/Users/Shared/sv-${HOST_USER}"
DEPLOY_KEY_DIR="${SHARED_WS}/.ssh/deploy_${REPO_NAME}"
HAS_GH=$(command -v gh >/dev/null 2>&1 && echo yes || echo no)
```

Check and report:

| Check | Pass | Fail |
|-------|------|------|
| sandvault installed | Continue | Suggest install |
| Deploy key for repo | Show "deploy key: ready" | Warn: "No deploy key for ${REPO_NAME}. The sandbox won't be able to push. Run `sv deploy-key ${REPO_NAME}` to create one." |
| `gh` CLI available | Continue | Note: "gh CLI not found — PR creation won't work in sandbox" |
| Shared workspace exists | Continue | Will be created by sv-clone |

If any critical check fails (sandvault not installed), stop and help the user fix it. For non-critical issues (no deploy key, no gh), warn but continue if the user wants.

### Step 2: Detect task type

Infer the task type from the conversation context:

- **bug-fix**: user is debugging, fixing errors, or mentions a bug/issue
- **feature**: user is building something new, adding functionality
- **review**: user wants code reviewed, or mentions a PR to review
- **experiment**: user is exploring, prototyping, or says "try" / "experiment"
- **general**: default if none of the above clearly apply

The task type determines the handoff template structure (see Step 4).

### Step 3: Gather rich context

Collect context automatically to include in the handoff. Run these in parallel:

```bash
# Uncommitted changes (so sandbox Claude knows what was in-progress)
git diff HEAD 2>/dev/null | head -200

# Recent commit log for context
git log --oneline -10

# Current PR info if one exists
gh pr view --json title,body,url,reviews,comments 2>/dev/null

# Check for existing CLAUDE.md in the repo
cat CLAUDE.md 2>/dev/null
```

### Step 4: Write the handoff file

Write the task summary to `/Users/Shared/sv-$USER/handoff.md`. When `sv-clone` runs, it copies this file into the cloned repo as `CLAUDE.md` (after git clone but before launching Claude) and deletes the original. Claude discovers it naturally on startup. The file only exists as an untracked file in the clone — never committed, never in the source repo.

#### Handoff file structure

Always include these sections, but adapt content based on task type:

```markdown
# Task Handoff

## Context
<!-- What the goal is, approach, decisions made, branch name -->
<!-- Include relevant conversation context -->

## Current State
<!-- Include git diff summary if there are uncommitted changes -->
<!-- Include recent commit log for branch context -->
<!-- Include PR info if relevant -->

## Setup
<!-- Environment setup commands — be specific (see Step 4a) -->

## What to do
<!-- Direct, actionable instruction -->
<!-- Adapted by task type (see templates below) -->

## When you're done
When your work is complete:
1. Commit and push your changes to the branch
2. Write a brief report to `/Users/Shared/sv-HOST_USER/reports/REPO_NAME-TIMESTAMP.md` with:
   - What you did (summary)
   - What worked / what didn't
   - Any follow-up items
   - The branch name and commit SHA
3. If appropriate, create a draft PR with `gh pr create --draft`

## Original CLAUDE.md
<!-- If the source repo had a CLAUDE.md, include its contents here -->
<!-- so sandbox Claude inherits the project's conventions -->
```

Replace `HOST_USER` with the actual host username and `TIMESTAMP` with the current ISO timestamp.

#### Task-type templates

**Bug fix** — include in "What to do":
```
You are fixing a bug. Here's what we know:
- Error/symptom: <description>
- Reproduction: <steps if known>
- Suspected cause: <if discussed>

Fix the bug, add a test that would have caught it, and verify the fix.
```

**Feature** — include in "What to do":
```
You are building a new feature:
- Goal: <what it should do>
- Acceptance criteria: <if discussed>
- Design decisions: <any decisions already made>

Implement the feature, add tests, and update any relevant documentation.
```

**Review** — include in "What to do":
```
You are reviewing code:
- PR: <url if available>
- Focus areas: <what to look for>

Review the code for correctness, security, performance, and style.
Leave your findings in the report file.
```

**Experiment** — include in "What to do":
```
You are running an experiment:
- Hypothesis: <what we're testing>
- Approach: <suggested approach>

Try it out. Document what works and what doesn't in your report.
Feel free to explore — this is a sandbox, nothing here can break production.
```

**General** — use the standard format with a clear "What to do" instruction.

#### Step 4a: Smart environment setup

Detect the project's package manager and generate exact setup commands:

```bash
# Check what exists in the repo root
ls -1 pyproject.toml requirements.txt Pipfile Gemfile package.json \
     package-lock.json yarn.lock pnpm-lock.yaml Cargo.toml go.mod \
     Makefile .tool-versions .nvmrc .python-version .ruby-version \
     composer.json mix.exs build.gradle pom.xml 2>/dev/null
```

Generate setup instructions based on what's found:

| Found | Setup command |
|-------|--------------|
| `pyproject.toml` | `python -m venv .venv && source .venv/bin/activate && pip install -e '.[dev]'` |
| `requirements.txt` (no pyproject) | `python -m venv .venv && source .venv/bin/activate && pip install -r requirements.txt` |
| `Pipfile` | `pipenv install --dev` |
| `pnpm-lock.yaml` | `pnpm install` |
| `yarn.lock` | `yarn install` |
| `package-lock.json` | `npm install` |
| `package.json` (no lockfile) | `npm install` |
| `Cargo.toml` | `cargo build` |
| `go.mod` | `go mod download` |
| `Gemfile` | `bundle install` |
| `composer.json` | `composer install` |
| `Makefile` | Note: `make` may be needed — check Makefile targets |
| `.tool-versions` | `asdf install` |
| `.nvmrc` | `nvm install && nvm use` |
| `.python-version` | `pyenv install $(cat .python-version) && pyenv local $(cat .python-version)` |

Include ALL matching setup commands in the Setup section, in dependency order (version managers first, then package managers).

### Step 5: Register the session

Track this handoff for status checking later. Write session info to `/Users/Shared/sv-$USER/sessions/`:

```bash
mkdir -p "/Users/Shared/sv-${USER}/sessions"
mkdir -p "/Users/Shared/sv-${USER}/reports"
```

Write a session file at `/Users/Shared/sv-$USER/sessions/REPO_NAME-TIMESTAMP.json`:
```json
{
  "repo": "REPO_NAME",
  "branch": "BRANCH",
  "task_type": "TYPE",
  "task_summary": "Brief one-line summary",
  "started_at": "ISO_TIMESTAMP",
  "handoff_file": "/Users/Shared/sv-$USER/handoff.md",
  "status": "active"
}
```

### Step 6: Ask the user for confirmation

Show the user a summary:
- The repo that will be cloned
- The detected task type
- A brief summary of the task being handed off
- Any warnings from pre-flight checks
- If there are other active sessions for this repo, note them

### Step 7: Launch in a new terminal window

Set environment variables for the animation, then launch:

```bash
export SV_REPO_NAME="$REPO_NAME"
export SV_BRANCH="$BRANCH"
export SV_TASK_TYPE="$TASK_TYPE"
export SV_DEPLOY_KEY="$HAS_DEPLOY_KEY"  # "yes" or "no"
export SV_TASK_SUMMARY="$TASK_SUMMARY"  # one-line summary
```

**Detect the default terminal:**

```bash
defaults read com.apple.LaunchServices/com.apple.launchservices.secure LSHandlers 2>/dev/null | grep -A1 "LSHandlerRoleShell" | grep -o '"[^"]*"' | tr -d '"'
```

Look for `com.googlecode.iterm2` (iTerm2) or `com.apple.Terminal` (Terminal.app). If detection fails, default to Terminal.app.

The launch command should play the animation first, then run sv-clone:

```
SV_REPO_NAME="..." SV_BRANCH="..." SV_TASK_TYPE="..." SV_DEPLOY_KEY="..." SV_TASK_SUMMARY="..." bash <skill-directory>/sv-vibes.sh && sv-clone <repo-path> -- claude
```

If `sv-clone` is not on PATH, fall back to `sv claude --clone <repo-path>`.

**For iTerm2:**

```bash
osascript <<'EOF'
tell application "iTerm2"
    activate
    set newWindow to (create window with default profile)
    tell current session of newWindow
        write text "SV_REPO_NAME='...' SV_BRANCH='...' SV_TASK_TYPE='...' SV_DEPLOY_KEY='...' SV_TASK_SUMMARY='...' bash <skill-directory>/sv-vibes.sh && sv-clone <repo-path> -- claude"
    end tell
end tell
EOF
```

**For Terminal.app:**

```bash
osascript -e 'tell application "Terminal" to activate' -e 'tell application "Terminal" to do script "SV_REPO_NAME='"'"'...'"'"' SV_BRANCH='"'"'...'"'"' SV_TASK_TYPE='"'"'...'"'"' SV_DEPLOY_KEY='"'"'...'"'"' SV_TASK_SUMMARY='"'"'...'"'"' bash <skill-directory>/sv-vibes.sh && sv-clone <repo-path> -- claude"'
```

---

## Status Mode (`/sv status`)

Check on active sandbox sessions:

1. List session files in `/Users/Shared/sv-$USER/sessions/`
2. For each active session, check:
   - Does a report exist in `/Users/Shared/sv-$USER/reports/` matching the repo+timestamp?
   - Can we see the sandbox repo? (`/Users/Shared/sv-$USER/$REPO_NAME/`)
   - What's the latest commit in the sandbox repo? (`su -l sandvault-$USER -c "cd /Users/Shared/sv-$USER/$REPO_NAME && git log --oneline -3"`)
3. Show a summary table:

```
Session          | Task Type | Started    | Status
─────────────────┼───────────┼────────────┼────────
myrepo (main)    | feature   | 10min ago  | active (3 new commits)
myrepo (fix/bug) | bug-fix   | 2hr ago    | done — report ready
```

If a report exists, offer to show it.

---

## Pull Mode (`/sv pull`)

Fetch results back from a sandbox session:

1. List completed sessions (those with reports)
2. If multiple, ask which one to pull
3. Run:
   ```bash
   # Fetch from the sandvault remote
   git fetch sandvault 2>/dev/null || git remote add sandvault "/Users/Shared/sv-${USER}/${REPO_NAME}" && git fetch sandvault
   ```
4. Show:
   - The report contents from `/Users/Shared/sv-$USER/reports/REPO_NAME-*.md`
   - A `git log sandvault/BRANCH --oneline` summary
   - A `git diff HEAD...sandvault/BRANCH --stat` summary
5. Offer to:
   - Merge: `git merge sandvault/BRANCH`
   - Cherry-pick specific commits
   - Just review the diff
6. Mark the session as "pulled" in the session file
