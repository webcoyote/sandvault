## SandVault — Summary

**SandVault** (`sv`) is a macOS sandboxing tool that lets you safely run AI coding agents (Claude Code, Codex, Gemini) in an isolated environment without VM overhead.

### Core Implementation

A single **~1,300-line Bash script** (`sv`) that orchestrates several macOS isolation layers:

1. **User account isolation** — Creates a limited `sandvault-$USER` account with restricted group memberships and no access to the host user's home directory
2. **Filesystem restrictions via ACLs** — Sandboxed user can only write to `/Users/Shared/sv-$USER` and its own home; read-only access to system dirs
3. **`sandbox-exec` profile** — A macOS sandbox policy (deployed to `/var/sandvault/`) that denies file writes except to whitelisted paths and blocks access to `/Volumes`
4. **Passwordless sudo** — Sudoers rules allow instant `sudo --login --user=sandvault-$USER` switching without passwords

### Execution Flow

```
sv claude → validate/build sandbox → sudo -u sandvault-$USER → sandbox-exec -f profile → zsh → claude --dangerously-skip-permissions
```

An SSH mode (`-s`) is available as an alternative to `sudo`-based impersonation.

### Key Components

| Path | Purpose |
|------|---------|
| `sv` | Monolithic main script — build, run, uninstall |
| `guest/home/` | Template home directory copied to sandbox user |
| `guest/home/bin/{claude,codex,gemini}` | Wrappers that locate and run each AI agent with permissions-bypass flags |
| `scripts/tests` | Test suite for sandbox functionality |
| `scripts/validate` | ShellCheck linting |
| `.github/workflows/ci.yml` | CI on macOS runners |

### Build Process (first run)

Creates user/group → generates SSH keys → deploys sandbox profile → configures sudoers → sets up shared workspace with ACLs → syncs guest home template via rsync.

### Notable Design Choices

- **No VMs** — uses native macOS user/process isolation for near-zero overhead
- **Monolithic bash** — everything in one file, targeting macOS system bash (v3.2) for compatibility
- **Session tracking** — reference-counted sessions in `~/.local/state/sandvault/` for safe process cleanup
- **`--clone` support** — clones repos into the sandbox with automatic git remote wiring back to the original

## Why running as a separate user is not enough?

Running as a separate user provides **process and file permission isolation**, but it still leaves significant gaps on macOS:

### What user isolation gives you

- Can't read/write the host user's home directory
- Separate process space (can't `ptrace` or signal host processes)
- Separate keychain

### What user isolation does NOT prevent

1. **Writing anywhere the "staff" group can** — By default macOS adds users to the `staff` group, which has write access to shared locations like `/usr/local`. SandVault mitigates this by removing the sandbox user from `staff`.

2. **Reading mounted/network volumes** — A separate user can still browse `/Volumes`, accessing USB drives, NAS mounts, etc. The `sandbox-exec` profile blocks this with `(deny file-read* (subpath "/Volumes"))`.

3. **Writing to `/tmp` and other world-writable locations to influence host processes** — While unavoidable for basic functionality, the sandbox profile tightly controls which paths are writable.

4. **Unrestricted network access** — User isolation alone doesn't limit what hosts/ports a process can connect to. The sandbox profile could add network restrictions (though SandVault currently allows network access since AI agents need API connectivity).

5. **Accessing sensitive system resources** — Things like the camera, microphone, or accessibility APIs. `sandbox-exec` can deny these at the kernel level.

### In short

User isolation is the **coarse-grained** layer (who are you?), while `sandbox-exec` is the **fine-grained** layer (what can you do?). SandVault uses both because neither is sufficient alone — a restricted user can still do damage in places the filesystem permissions allow, and `sandbox-exec` alone wouldn't prevent reading the host user's files if running as that user.

## Commands internal implementation

---

## `sv build` (alias: `sv b`)

Does the full sandbox setup, then exits. All steps marked with `[REBUILD]` below:

### 1. Install tools (lines 254-270)
- Installs Homebrew if missing (`curl | bash`)
- `brew install claude-code`, `codex`, or `gemini-cli` depending on command

### 2. Create user & group (lines 643-737)
- `dscl . -create /Groups/sandvault-$USER` — creates macOS group
- Sets `PrimaryGroupID`, `RealName` on the group
- `dscl . -create /Users/sandvault-$USER` — creates macOS user
- Sets `UniqueID`, `PrimaryGroupID`, `RealName`, `NFSHomeDirectory` → `/Users/sandvault-$USER`, `UserShell` → `/bin/zsh`
- `dscl . -passwd` — sets a random password (via `openssl rand -base64 32`)
- `dscl . -create ... IsHidden 1` — hides user from login screen
- `dseditgroup -o edit -d sandvault-$USER staff` — removes sandbox user from `staff` group
- `dscl . -delete /Groups/staff GroupMembers <GeneratedUID>` — removes UUID-based membership too
- `dseditgroup -o edit -a $USER sandvault-$USER` — adds host user to sandbox group

### 3. Configure SSH access (lines 750-765)
- If `com.apple.access_ssh` group exists, adds sandbox user to it via `dseditgroup`
- `ssh-keygen -t ed25519` — generates keypair at `~/.ssh/id_ed25519_sandvault`
- Writes public key into `guest/home/.ssh/authorized_keys`

### 4. Create shared workspace (lines 771-788)
- `mkdir -p /Users/Shared/sv-$USER`
- `sudo chown -R $USER:sandvault-$USER` on it
- `chmod 0770` on root
- `sudo find ... | xargs chmod -h +a "group:sandvault-$USER allow ..."` — sets ACLs recursively (read, write, append, delete, search, list, file/directory inherit)
- Writes `SANDVAULT-README.md`

### 5. Configure passwordless sudo (lines 794-876)
- Creates `/var/sandvault/buildhome-sandvault-$USER` script (root-owned, mode 0554) that:
  - `mkdir -p /Users/sandvault-$USER`, `chown`, `chmod 0750`
  - `rsync --checksum --recursive --perms --times` from `guest/home/.` to sandbox home
  - `chown` all synced files to sandbox user
- Creates `/etc/sudoers.d/50-nopasswd-for-sandvault-$USER` allowing:
  - `$USER` → run `/bin/zsh`, `/usr/bin/env`, `/usr/bin/true` as sandbox user (NOPASSWD)
  - `$USER` → run the buildhome script as root (NOPASSWD)
  - `$USER` → run `launchctl bootout user/<uid>` and `pkill -9 -u sandvault-$USER` as root (NOPASSWD)
- Validates via `visudo -c` before atomic `mv` into place

### 6. Create sandbox-exec profile (lines 882-940)
- Writes `/var/sandvault/sandbox-sandvault-$USER.sb` (root-owned, mode 0444)
- Profile rules:
  - `(allow default)` + `(deny file-write* (subpath "/"))` — deny all writes by default
  - `(deny file-read* (subpath "/Volumes"))` + `(allow file-read* (subpath "/Volumes/Macintosh HD"))` — block external drives
  - Allow writes to: sandbox home, shared workspace, `/tmp`, `/private/tmp`, `/var/folders`, `/dev`
  - Allow `process-info*`, `sysctl-read`, `process*`
  - Allow `/bin/ps` with `no-sandbox`

### 7. Configure git (lines 956-971)
- Copies host's `user.name` and `user.email` into `guest/home/.gitconfig`
- Sets `safe.directory = /Users/Shared/sv-$USER/*`

### 8. Sync home directory (lines 977-982)
- Runs the buildhome script via sudo: `sudo /var/sandvault/buildhome-sandvault-$USER`

### 9. Write install marker (lines 988-998)
- `mkdir -p ~/.config/codeofhonor/sandvault`
- Writes current date to `~/.config/codeofhonor/sandvault/install`

---

## `sv claude` / `sv codex` / `sv gemini` / `sv shell`

All four run the full build sequence above (if install marker is missing or `--rebuild`), then:

### Session registration (lines 1141-1145)
- Creates `~/.local/state/sandvault/sandvault.count`
- Uses `lockf` to atomically increment session count
- Registers EXIT trap to decrement and clean up on exit

### Launch via sudo (default mode, lines 1248-1296)
- Verifies passwordless sudo: `sudo --non-interactive --user=sandvault-$USER /usr/bin/true`
- Launches: `sudo --login --set-home --user=sandvault-$USER /usr/bin/env -i HOME=... USER=... SHELL=/bin/zsh TERM=... COMMAND=... PATH=/usr/bin:/bin:/usr/sbin:/sbin sandbox-exec -f <profile> /bin/zsh -c "export TMPDIR=$(mktemp -d); cd ~; exec /bin/zsh --login"`
- Environment passed: `COMMAND`, `COMMAND_ARGS`, `INITIAL_DIR`, `SV_SESSION_ID`, `SV_VERBOSE`
- Inside the sandbox zsh, `guest/home/.zshrc` (not shown here but synced) launches the actual `claude`/`codex`/`gemini` binary

### Launch via SSH (`--ssh` mode, lines 1178-1247)
- Checks SSH connectivity: `ssh -o BatchMode=yes -o ConnectTimeout=2 ... exit 0`
- If fails, checks for auth failure vs local network permission issue (offers to open System Settings)
- Launches: `ssh -q -t -o StrictHostKeyChecking=no -i ~/.ssh/id_ed25519_sandvault sandvault-$USER@$HOSTNAME /usr/bin/env -i ... sandbox-exec -f <profile> /bin/zsh -c '...'`

### Session cleanup on exit (lines 294-354)
- Atomically decrements session count via `lockf`
- If last session:
  - `sudo launchctl bootout user/<uid>` — terminates all sandbox user processes
  - `sudo pkill -9 -u sandvault-$USER` — force-kills any remaining processes

---

## `sv --clone URL|PATH <command>` (lines 1004-1116)

Before launching the agent/shell:

- If PATH is a local directory:
  - Checks if sandbox user can read it directly
  - If yes: `git clone --no-hardlinks <local-path> /Users/sandvault-$USER/repositories/<name>` (as sandbox user)
  - If no: `git clone --mirror --no-hardlinks` to a temp dir in shared workspace, `chmod -R a+rX`, then sandbox user clones from there
- If URL is remote:
  - Same mirror-then-clone approach through shared workspace temp dir
- Sets `origin` remote on sandbox clone to the source URL
- If cloned from local repo, adds `sandvault` remote on the local repo pointing to the sandbox clone

---

## `sv uninstall` (alias: `sv u`) (lines 382-427)

1. `launchctl bootout user/<uid>` + `pkill -9 -u sandvault-$USER` — kill all sandbox processes
2. `rm -rf ~/.config/codeofhonor/sandvault/install` — remove install marker
3. `sudo rm -rf /etc/sudoers.d/50-nopasswd-for-sandvault-$USER` — remove sudoers file
4. `sudo rm -rf /var/sandvault/buildhome-sandvault-$USER` — remove home-sync script
5. `sudo rm -rf /var/sandvault/sandbox-sandvault-$USER.sb` — remove sandbox profile
6. `sudo chown -R $USER:$(id -gn) /Users/Shared/sv-$USER` + `chmod 0700` — restore shared folder ownership, remove ACLs
7. `dseditgroup -o edit -d $USER sandvault-$USER` — remove host user from sandbox group
8. `dseditgroup -o edit -d sandvault-$USER com.apple.access_ssh` — remove SSH access
9. `dscl . -delete /Users/sandvault-$USER` — delete macOS user
10. `dscl . -delete /Groups/sandvault-$USER` — delete macOS group
11. `sudo rm -rf /Users/sandvault-$USER` — delete sandbox home directory
12. `rm -rf ~/.ssh/id_ed25519_sandvault{,.pub}` — delete SSH keypair
13. Removes `SANDVAULT-README.md`, `rmdir` shared workspace (keeps if non-empty)
