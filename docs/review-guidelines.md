# Review guidelines

Read this before finalizing a commit. It is distilled from the bug classes
that recur in this repository's history; the short hashes point at the commit
where each lesson was paid for (`git show <hash>` has the full story).

## The three actors

Every line of code runs as, or on behalf of, one of these. Know which.

| Actor | Who | Trust |
|---|---|---|
| **Host user** | The person running `sv`. Owns `~`, `~/.ssh`, `~/.config`, the install tree. Can `sudo`. | Trusted. Everything the sandbox exists to protect. |
| **Sandbox user** | `sandvault-$USER[-profile]`, running agents in bypass-permissions mode. | Treat as hostile. Assume it runs arbitrary code, concurrently with `sv`, at all times. |
| **Adversary** | Whoever steers the agent (prompt injection, malicious repo, bad dependency). | Has everything the sandbox user has, and is patient: it can plant something now that fires on the *next* `sv` run. |

A fourth party matters for robustness: **other host users** on the same Mac
(shared Homebrew install, their own sandboxes, `/tmp`) and **other concurrent
`sv` sessions** of the same user.

The escape pattern is almost always a *confused deputy*: the host user (or
root) performs an ordinary file or git operation on a path the sandbox user
controls, and the sandbox redirects it.

## Where the boundary is

Sandbox-writable (never trust a path, its contents, type, owner, or mode):

- `$SHARED_WORKSPACE` (`/Users/Shared/sv-$USER…`) and everything under it,
  including `_sandvault/` (`$SV_PRIVATE_DIR`: `.ssh`, `tmp`, `sessions`,
  `setup`), `user/`, and `repos/`.
- `/Users/sandvault-$USER…` (the sandbox home).
- World-writable dirs (`/tmp`, `/Users/Shared`): sticky, so the sandbox cannot
  replace files it does not own, but it can pre-create names.

Host-only (safe to stage and keep state in): `mktemp -d` in the host's
`$TMPDIR`, `~/.config/codeofhonor/sandvault`, `~/.local/state/sandvault`,
`~/.ssh`. Root-only: `/var/sandvault`, `/etc/sudoers.d`.

Inside the workspace **ownership and mode bits are not defenses**:

- New entries inherit ACEs granting the sandbox group
  `add_file`/`delete_child`/`writesecurity`, so `mkdir` + `chmod 0700` by the
  host does not make a host-only directory (be18874 → 06697d9).
- `delete_child` lets the sandbox rename or move host-owned entries, so
  "it's host-owned, therefore git/ssh/we can trust it" is false. Git's
  `safe.directory` check does not fire on host-owned repos (e8d60d4), and the
  sandbox can assemble a host-owned `.git` from repurposed directories.

## 1. Host or root touching sandbox-writable paths

The largest and most serious class. Each of these was a real escape:

| Operation | What the sandbox did | Commit |
|---|---|---|
| `> "$WORKSPACE/README"` | symlink → host truncates any file it can write | d90f267, e30012a |
| `ssh-keygen -f` into `_sandvault/.ssh` | `.pub` symlink → overwrite `~/.ssh/authorized_keys` | 8551ded |
| `chdir` into shared scratch, tool writes `./.iosef` | symlinked state dir | be18874, 06697d9 |
| root `chown` after sync, no `-h` | swap path for symlink → own any system file | e87cd79 |
| root `rsync` from a user-writable tree | symlink/hardlink/dir swap → read root-only files | 76d9cd6 |
| `mktemp` + `sudo tee` | swap temp path → redirect root write | 3393bd3 |
| host `git fetch` in the clone | `.git/config` keys run as commands | e8d60d4, bf754f9 |

Rules:

1. **Prefer not touching it at all.** The host should not open, read, write,
   `chmod`, `chown`, `cd` into, or run tools inside a sandbox-writable
   directory. A path-only existence test (`[[ -e ]]`, `[[ -f ]]`) is fine;
   anything that opens is not (a `cat` can also block on a planted FIFO).
2. **Move the operation across the boundary instead of hardening it.** Have
   the *sandbox user* do writes into sandbox territory (deploy key install,
   `git fetch`, home dir extraction via `sudo -u "$SANDVAULT_USER" tar`), and
   have the host produce inputs in a host-only temp dir. A poisoned config or
   planted link gains nothing when the process following it is already the
   sandbox user.
3. **If the host must deliver a file:** stage it outside the workspace on the
   same volume, set mode and ACL on the staged file, then a single
   `rename(2)`: `/bin/mv -fh` (`-h` so a planted symlink-to-directory is
   replaced, not entered). In Python hold a directory fd opened once with
   `O_DIRECTORY|O_NOFOLLOW` and use `dir_fd=` / `O_EXCL|O_NOFOLLOW`
   (`helpers/sv-ios-bridge`). Before a write the sandbox does itself, `rm -f`
   the destination so a link is replaced, not followed.
4. **Never run as root what a user can do.** Read as the host user, write as
   the sandbox user. When root must touch a user-writable tree, never follow
   links: `chown -h`, `chmod -h`, `find … -exec` (not `-R`, and not a
   list-then-act pair with a window between them).
5. **Check-then-use is a race.** Anything validated by path can be swapped
   before it is used. Validate the thing you hold (fd, host-owned copy), not
   the path you got it from.
6. **Data read back from the sandbox is attacker input.** Read it through the
   sandbox as text, validate strictly (the deploy pubkey must match one
   `ssh-ed25519` line), and only ever act on the host-owned copy. Never derive
   security decisions from sandbox-writable config: the repo URL the deploy
   key is uploaded to comes from the host's argument, not from the clone's
   `remote.origin.url` (bf754f9).
7. **Tools that execute their config are code execution.** Host-side git in
   any possibly-sandbox-writable repo goes through `git_safe`/`git_repo`
   (`sv-clone`), and that denylist is known-incomplete: no transport commands
   (`fetch`, `pull`, `push`, `clone` from it) on the host, ever. Ask the same
   question of every new tool the host runs in the workspace (npm, make,
   direnv, editors, anything with rc files or hooks).

A warning from history: the *first* fix in this class was wrong three times
(chmod under inherited ACLs; temp file beside the target; a `-c` denylist).
Each was replaced by a structural fix. If your fix enumerates bad cases, look
for the version that removes the access instead.

## 2. Secrets and authority flow one way

- Nothing private moves host → sandbox by accident. `ssh-keygen -l` accepts a
  private key and reports its public half, so `authorized_keys.d` rejects
  anything containing `PRIVATE KEY` outright (2ead910).
- Anything that grants access *to* the sandbox or *from* it lives in host-only
  config, so the sandbox cannot grant itself access (2ead910, 78a2afa).
- Least privilege by default, opt-in to more: deploy keys are opt-in and
  read-only unless `-w` (c1f4661, 33d7937). The agent runs with permissions
  bypassed, so every credential in reach is a credential it will use.
- Keep secrets out of argv and logs (deploy key goes over stdin, 8551ded).
- Creating directories for an agent can weaken them: `mkdir -p` makes parents
  0755, and a tool that only sets 0700 at creation never fixes it. Know what
  credential files live beside a path before pre-creating or ACLing it
  (pi `auth.json`, fcee172).
- Sudoers stays minimal and exact (e287826). Root-run scripts and policy files
  are root-owned, non-writable, and validated *before* being renamed into
  place (`visudo -c`, `sandbox-exec -f … true`): a bad file that installs
  sticks (36712f4, 48ddc84).

## 3. Sandbox profile

- Default deny; every allow names the tool that needs it and the symptom
  without it, **measured as the sandbox user under the rendered profile**
  (997fd09). Entries that are not justified by a failing probe get removed.
- Reachability of a host-user service is an escape, not a convenience:
  unrestricted mach-lookup let `open` launch apps as the host user (6bd77c4).
  When allowing a mach service ask whether it is a root system daemon or a
  per-user host agent.
- POSIX permissions are not the only layer: deny explicitly what happens to be
  world-readable (`/Library/Keychains`, raw disks, bpf, `/Volumes`, other
  users' homes) (ccb9196, b6e668e).
- Helpers that process untrusted content run inside the sandbox (browser,
  516f40a). A long-lived server keeps the sandbox state of whoever started it,
  so `--no-sandbox` is refused where that would leak forward (herdr, 4f68015).
- Changing the profile requires bumping `SV_SANDBOX_PROFILE_VERSION`, or
  installed machines keep the old one (2840017).
- The profile is an unquoted heredoc: backticks and `$` in comments execute.
- Verify escapes **from the host side** (did a host-owned process or file
  appear?), not from what the sandbox reports.

## 4. Permissions, ACLs, and other people's machines

- ACLs: `search`/`execute` on a file is the execute bit. Directories and files
  get different ACEs; inheritance flags matter (0352b82, 16655f7, 47a6ef0).
  An empty path element in a generated list resolves to the parent, e.g.
  ACLing all of `$HOME` (fcee172): abort on empty elements.
- Do not assume umask 022 (49ae69f), that the install tree is writable by or
  owned by this user (shared Homebrew, 78a2afa), that usernames lack dots
  (`sudoers.d` skips dotted filenames, dc0047c), that `$HOSTNAME` resolves
  (0a20ccf), or that the Homebrew path is version-stable (use `opt/`, not
  `Cellar/`; be0cadb, 30ae928).
- Sticky directories cut both ways: the host cannot delete sandbox-created
  files in `/tmp` (141dc0d). Temp dirs must be created by the user who will
  own them (b316311) and be unique per session (9ec090f).
- Hardlinks and copies carry source modes (`--no-hardlinks`, f1959b1).
  `chown` before `chmod` when widening access (5cf177b).
- Use absolute paths to system tools whose GNU versions differ
  (`/usr/bin/stat`, `/bin/ln`, `/bin/chmod`, `/bin/mv`, `/usr/bin/tar`);
  contributors have coreutils first on `PATH` (fd1bde5, 040716d).

## 5. Concurrency and lifecycle

- Several `sv` sessions, profiles, and host users run at once. Per-session
  cleanup touches only that session's PIDs and directories; user-wide
  `pkill`/`launchctl bootout` is for `uninstall` only (47abda1, 60e321c,
  14974f6).
- Shared files are written temp + rename with a unique temp name, never in
  place and never via a fixed temp name (7b356e2, 78a2afa). Allocation from
  shared namespaces (UID/GID) takes a lock (5b5ce23).
- `exec` discards EXIT traps: cleanup registered before an `exec` never runs
  (7b9dcd1; leaked browsers and simulators for many releases). Clean up
  explicitly before `exec`, or supervise a child.
- A backgrounded command in a non-interactive script gets stdin from
  `/dev/null`; pass `<&0` (f67d6cb).
- Generated artifacts (build-home script, sudoers, profile, guest dotfiles)
  persist on installed machines. Every change to one needs an upgrade story:
  a version bump, a rewrite-on-mismatch, or a stated "`sv build` required",
  plus cleanup of the legacy name/shape. Never auto-delete something that
  could be the user's (2782cd1).

## 6. Bash

Scripts must run on `/bin/bash` 3.2 and Homebrew bash under
`set -Eeuo pipefail` with an ERR trap.

- Capturing an exit status: `cmd && st=0 || st=$?`. `if ! x=$(cmd)` and
  `if cmd; then …; fi` both lose it (85a23c1).
- `set -E` carries the ERR trap into `$(…)`; clear it (`trap - ERR;`) inside
  substitutions whose non-zero exit is expected (da42046).
- Never merge stderr into stdout that is then parsed (`2>&1` turned error
  text into config keys, d0ed125). Never discard stderr that explains a
  failure (`2>/dev/null` hid sudo, simctl, and rsync errors: a957ce5, bc6f114,
  09cbbb1).
- Exit codes are a protocol: one meaning per code, and "non-zero with empty
  output" is an error, not "nothing to do" (31b6d84, 605689a).
- Optional features must not abort core setup: guard with `|| warn`
  (f5a8eb2, 5dfbdbe). Conversely, do not swallow failures of core steps.
- Quoting across the boundary: arguments pass through sudo, a login zsh, ssh,
  and sandbox-exec. Use `quote_zsh_args`; `printf %q` emits byte escapes that
  break non-ASCII (0bbe650); `$` in args is expanded by the login shell, so
  send scripts on stdin (8551ded). Test with spaces and unicode, in both
  direct and `--ssh` modes (74c3dfa).
- Empty arrays under `set -u` on 3.2: `"${a[@]+"${a[@]}"}"` (71e8a50). No
  `readlink -f`, no associative arrays, no `${var,,}`.
- Facts that bit: `git remote get-url` applies `insteadOf` (read the raw
  config; f256d47); `.git` may be a file (eb45d79); quoted `~` is not expanded
  (cbe7a06); `xargs` with empty input (724ca44); rsync trailing slashes.

## 7. Agent wrappers (`guest/home/bin/*`)

- A wrapper lives in `~/bin`, first on `PATH`. Resolve the real binary by
  explicit path, never `command -v`; installers that probe `PATH` re-enter the
  wrapper, so also set a re-entry sentinel (2a62b62, fcee172).
- Injected flags must not break subcommands. Check where each CLI accepts
  global flags against the real binary's `--help`, not from memory
  (ff0b5b0 → 38d419f → 51e2a42 → 5ffc40e: four fixes for one feature).
- Say exactly what trust a flag grants, in the banner and README, and keep
  the banner true per code path (29575f3).
- Installers must not edit rc files sandvault regenerates, or drop directories
  with side effects (`~/node_modules`, 12aad44). Diagnostics go to stderr
  (dc4dbdc); exit codes propagate (44b8520).

## 8. Tests

- **Tests run with real privileges on a real machine.** They have deleted a
  contributor's checkout (d4630fd) and rewritten the repo's own git config
  (f2612aa, 45388b3). Use `mktemp -d` fixtures and throwaway repos; never
  point a test at the current checkout, `$SHARED_WORKSPACE/repos`, or the
  user's config; no backup-and-restore of shared state (eb5d572).
- A security fix ships with a test that plants the attack (symlink to a file,
  symlink to a directory, poisoned config) and **fails against the previous
  code**.
- A test must be able to fail for the reason it exists. If several paths
  produce the same exit code, assert something only the intended path does;
  mutation-check new guards by reverting the fix (d0ed125, 605689a, 38d419f).
- Stubs emit the real tool's output shape and run the script's own parsing; a
  stub that reconstructs the expected answer hides the bug (1a8a160).
- No dependence on remote content or the network unless that is the thing
  under test (4890a43, 63cc229). A skipped suite must not read as a pass
  (888709d). New test files must be wired into `scripts/validate` (31b6d84).
- Isolate from the developer's environment (`SANDVAULT_ARGS`,
  `GIT_CONFIG_GLOBAL`, PATH shims) and bound waits by wall-clock (89f1b1c).

## Before you commit

1. For every file operation added or changed: **who runs it, and who can
   write the path, its parents, and its contents?** If the answers differ in
   privilege, section 1 applies.
2. Does anything read from the sandbox feed a host-side decision, command
   line, or upload?
3. Does it hold with a second session, a second profile, a second host user,
   and a kill -9 halfway through?
4. Does it hold on bash 3.2, umask 077, GNU coreutils on `PATH`, a Homebrew
   install owned by someone else, and an already-installed older version?
5. Is there a test that fails without the change, and is it safe to run on a
   contributor's machine?
6. Does the commit message state what was verified and what was not?
7. Run `./scripts/validate` and report the result.
