# Security review, 2026-11-24

Scope: every shipped script (`sv`, `sv-clone`, `sv-agentsview-setup`,
`helpers/`, `guest/home/`, `skills/`, `.github/`), read against
`docs/review-guidelines.md`. Reviewed at commit 2f36047, version 1.40.0,
sandbox profile version 4.

Method: static reading only. Nothing here was exploit-tested. Items marked
**verify** rest on an assumption that needs a host-side check before the
severity is trusted. Line numbers refer to the commit above.

Section references (§1, rule 4, ...) are to `docs/review-guidelines.md`.

## Summary

| # | Severity | Area | Problem |
|---|---|---|---|
| 1 | High | `sv` | Host writes setup scripts into the workspace |
| 2 | High | `sv-agentsview-setup` | All state I/O is host I/O in the workspace; consent is sandbox-writable |
| 3 | High | `sv-clone` | Host `git clone` and config writes run in the workspace |
| 4 | High | `sv-clone` | Deploy-key target read from sandbox-writable config |
| 5 | High | `sv` | Root passes over the workspace; unchecked new-profile workspace |
| 6 | High (verify) | `--ios` | Sandbox can install and launch apps on a host-side simulator |
| 7 | Medium | setup | `sv` run from a checkout inside the workspace |
| 8 | Medium | `sv` | Session plumbing leaks host authority (tty, ssh config, host key) |
| 9 | Medium | `sv` | sandbox-exec layer is optional for the sandbox user |
| 10 | Medium | `sv` | Root build-home script written in place |
| 11 | Medium | `sv` | Sandbox profile is allow-default |
| 12 | Medium | `sv` | SSH mode passes values unquoted through the remote shell |
| 13 | Medium | docs, skill | README and `/sv` skill contradict the guidelines |
| 14 | Low | `sv` | ID-allocation lock in `/tmp` |
| 15 | Low | `sv`, bridge | iOS scratch dir handled by path |
| 16 | Low | `sv-clone` | Unvalidated repository name, jq interpolation, discarded errors |
| 17 | Low | `sv` | `uninstall --all` sweeps directories that may be the user's |
| 18 | Low | skill | Bash 4 syntax in `launch-in-terminal.sh` |
| 19 | Low | install, CI | Supply-chain exposure |

## High

### 1. Host writes setup scripts into the workspace

`sv:2022-2066`

On rebuild the host user runs:

- `mkdir -p "$SV_PRIVATE_DIR/setup"`
- `cat > "$SV_PRIVATE_DIR/setup/gitconfig"` and `chmod +x`
- `cat > "$SV_PRIVATE_DIR/setup/claude-json"` and `chmod +x`

`$SV_PRIVATE_DIR` is sandbox-writable. This is the class fixed in d90f267 and
e30012a for the README: a link planted at `setup` or at either file name
redirects the host's truncate, write, and chmod to a path the host can write.
It fires on the next rebuild, which also happens whenever the install marker
is missing.

Fix (§1 rule 2): do not write there as the host. Either add the scripts to the
payload delivered by the existing `sudo -u "$SANDVAULT_USER" tar` step in the
build-home script, or pipe them on stdin to a shell running as the sandbox
user, with `rm -f` of the destination first. The git identity values are the
only host-derived content.

Test: plant a symlink at `setup/gitconfig` pointing at a fixture file and
assert the fixture is unchanged after a rebuild.

### 2. `sv-agentsview-setup` does all its state I/O in the workspace

`sv-agentsview-setup:43-49, 94-111, 117-121, 128, 145, 357-393, 438-445, 472`

Running as the host user, the script:

- creates `_sandvault/setup` and `_sandvault/sessions` with `mkdir -p`
- writes `setup/agentsview-export` with `cat >` and `chmod +x`
- writes `setup/agentsview-export.state` with `echo ... >`
- appends to `setup/agentsview-declined.keys` with `>>`
- reads both state files with `cat` and `grep` (a planted FIFO blocks)
- replaces links under `sessions/` with `rm -f` and `ln -s`

Three separate problems:

1. Every write, read, and chmod is a §1 rule-1 violation, same mechanism as
   item 1.
2. The opt-in record and the decline list are consent decisions stored where
   the sandbox can edit them (§2: "anything that grants access ... lives in
   host-only config"). The sandbox can set the state to `enabled` or delete a
   recorded decline. The interactive diff prompt for `config.toml` still
   applies, but the symlink install and resync then run without the first-run
   prompt.
3. The host's agentsview is configured to scan `_sandvault/sessions/<agent>`,
   which are links in a sandbox-writable directory. The sandbox can repoint
   them, so a host tool follows sandbox-chosen paths and parses
   sandbox-chosen content.

Fix: move the state file and decline list to
`~/.config/codeofhonor/sandvault`. Generate the sandbox-side ACL script
through the sandbox user. For the scan path, prefer a location the sandbox
cannot rename (for example pointing agentsview directly at the sandbox home
paths, which the sandbox owns but cannot turn into a link to a host path
without that being visible as its own home content), and document that the
mirrored data is untrusted input to agentsview.

### 3. Host `git clone` and config writes run in the workspace

`sv-clone:255-261, 377-382`

- `mkdir -p "$(dirname "$REPO_DIR")"` and `git clone ... "$REPO_DIR"` run as
  the host. `repos/<name>` inherits the sandbox-group ACEs as soon as it is
  created, so for the duration of the clone the sandbox can write inside the
  repository the host's git is operating in, including its hooks directory
  and config. §1 rule 7 already bans host transport commands for this reason;
  the clone destination is the remaining case.
- `repos` or `repos/<name>` can be pre-created as a link, sending the clone
  elsewhere.
- `git_repo remote set-url` (`:260`, `:378`) and
  `git_repo config core.sshCommand` (`:380`) run on the host afterwards.
  `.git` may be a gitfile or link, and `.git/config` may be a link; git
  resolves links when it locks and rewrites a config file. `git_safe`
  neutralizes command-valued keys but does not address where the write lands.

Fix (§1 rule 2): reuse the pattern the fetch step already uses. The host
clones or bundles into a host-only temp directory, makes the bundle readable
to the sandbox group, and the sandbox user runs `git clone`, `remote set-url`,
and the `core.sshCommand` config inside the workspace.

Test: a background sandbox process that adds a hook as soon as
`repos/<name>/.git/hooks` exists, with a host-side marker asserting the hook
never ran as the host.

### 4. Deploy-key target read from sandbox-writable config

`sv-clone:207-211, 286-312, 389-418`

When the source argument is a local directory, `REPOSITORY_SOURCE_URL` comes
from `git remote get-url origin` in that directory. If the directory is under
`$SHARED_WORKSPACE` (the comment at `:70-75` anticipates this), the sandbox
controls that value, including through `insteadOf` rewriting (f256d47). With
`-k` or `-w` the host then uploads a deploy key, whose private half the
sandbox holds, to whichever `owner/repo` that URL names, using the host's
`gh` credentials.

This is the bf754f9 lesson reached by the local-path route: "never derive
security decisions from sandbox-writable config".

Fix: when the local source resolves under `$SHARED_WORKSPACE` (or is writable
by the sandbox group), refuse `-k`/`-w` unless the URL is given explicitly as
an argument. Independently, print the target `owner/repo` and access level
and ask for confirmation before any upload.

### 5. Root passes over the workspace; unchecked new-profile workspace

`sv:762-764, 775, 787-791, 795-800, 958-960, 1621-1622`

- `uninstall_all` uses `sudo chmod -RN`, `sudo chown -R`, and
  `sudo chmod -R u=rwX,go=` (`:958-960`). §1 rule 4 says `find ... -exec`
  with `-h`, not `-R`. `configure_shared_folder_permssions` also uses
  `sudo chown -f -R` (`:762`, `:795`).
- The ACL pass (`:787-791`) adds sandbox-group rights to every inode in the
  tree, and the chown pass changes its owner. **Verify:** whether the sandbox
  user can hard-link a file it does not own into the workspace. If it can,
  the ACE and ownership change apply to that inode at its original location
  as well. §4 already notes that hardlinks carry modes; the same reasoning
  applies to ACLs and ownership.
- A new profile's workspace path is never validated. `mkdir -p
  "$SHARED_WORKSPACE"` (`:1621`) succeeds on an existing directory or a link
  to one, and `sudo /bin/chmod 0770 "$SHARED_WORKSPACE"` (`:764`) follows
  links. `/Users/Shared` is sticky, so an existing sandbox cannot replace a
  claimed name, but it can pre-create the name of a profile that does not
  exist yet. `write_shared_workspace_readme` and everything after it then
  operate on the wrong directory.

Fix:

- Before any root operation, require `$SHARED_WORKSPACE` to be a real
  directory (not a link) owned by `$HOST_USER`; abort otherwise. Create it
  with `mkdir` (no `-p`) and treat "already exists" as a case to validate.
- Use `chmod -h` on the workspace root.
- Replace the `-R` forms with the `find ... -exec ... -h` form used at
  `:775`.
- Restrict the file branch of the ACL and chown passes to `-links 1`, and
  warn about anything skipped.

Test: plant a hard link and a symlink in a fixture workspace and assert the
link targets' owner, mode, and ACL are unchanged after a rebuild and after
`uninstall --all`.

### 6. `--ios`: sandbox can install and launch apps on a host-side simulator

`helpers/sv-ios-bridge:72, 185-199, 416-418, 436-460, 577-613`

The bridge runs as the host user and lets the sandbox install an app bundle
it built, launch it, and drive it.

- **Verify:** simulator apps run as processes of the host user and, as far
  as I know, are not confined by sandvault's user boundary or profile. If
  that holds, `--ios` grants the sandbox code execution as the host user, and
  the README should state that plainly (§7: "say exactly what trust a flag
  grants"). Check from the host side: launch a test app that writes a marker
  outside its container and see who owns it.
- The bridge has no authentication. Any local process, including other
  profiles' sandboxes and other users, can connect to the port. Requests need
  no particular content type, so a page in the host's browser can also reach
  it.
- `/describe` passes the `x` and `y` query values to `iosef` as option values
  without validation (`:416-418`); a value beginning with `-` may be read as
  an option. `/type` has the same shape for `--text`.
- `/install` accepts any path under `/Users/Shared/` (`:72`), which includes
  other profiles' and other users' workspaces. The realpath check
  (`:185-199`) is resolved once and then used by path, so it is still
  check-then-use (§1 rule 5) despite the comment at `:586-588`.

Fix:

- Generate a per-session token in `sv`, pass it to the bridge and to the
  sandbox environment, and require it as a header. Reject requests with an
  unexpected `Host`.
- Validate `x`/`y` with `validate_coord`, and pass free text after `--` or
  with `--text=<value>`.
- Narrow the install prefix to this profile's `$SHARED_WORKSPACE`, and copy
  the bundle into the host-only `PRIVATE_DIR` without following links before
  installing the copy.
- Document the trust `--ios` grants once verified.

## Medium

### 7. `sv` run from a checkout inside the workspace

At review time the host process list showed
`/bin/bash /Users/Shared/sv-pat/personal/sandbox/sandvault/sv cl` running as
the host user. That checkout is inside `$SHARED_WORKSPACE`, so the sandbox
user can edit `sv`, `helpers/*`, and `guest/home/*`, all of which the host
then executes or packages, including the steps that call `sudo`. The
root-run build-home script also embeds `$WORKSPACE/guest/home`.

This is a setup issue rather than a code bug, but the code can catch it.

Fix: at startup, when not nested, warn or refuse if `$WORKSPACE` is under
`/Users/Shared/sv-*`, under `/Users/sandvault-*`, or is writable by any
`sandvault-*` group. Use an installed copy (Homebrew, or a checkout in the
host home) for real sessions.

### 8. Session plumbing leaks host authority

`sv:1750-1759, 2343-2400, 2417`

- Sudo mode runs the session on the host's controlling terminal. The
  sandbox process and the host's shell share one tty for the life of the
  session and after it. Add `Defaults:$HOST_USER use_pty` (or a
  `Defaults>$SANDVAULT_USER use_pty`) to the sudoers drop-in so the session
  gets its own pty.
- SSH mode invokes `ssh` with the user's own configuration. A `Host *` block
  with `ForwardAgent yes`, port forwards, or `SendEnv` applies to the sandbox
  connection. Add `-F /dev/null -a -x -o IdentitiesOnly=yes
  -o ClearAllForwardings=yes`.
- `StrictHostKeyChecking=no` with `UserKnownHostsFile=/dev/null` accepts any
  server on 127.0.0.1:22. Build a known-hosts file in a host-only temp dir
  from `/etc/ssh/ssh_host_*_key.pub` and use `StrictHostKeyChecking=yes`.

### 9. sandbox-exec layer is optional for the sandbox user

`sv:1526-1527, 1580-1589`, `TODO.md` item 3

The sandbox-exec profile applies only to processes started through `sv`. Any
other way for the sandbox user to obtain a session is outside it.

- `sudo dscl . -passwd "/Users/$SANDVAULT_USER" "$RANDOM_PASS"` puts the
  account password in argv on every rebuild (§2: "keep secrets out of argv").
  `/bin/ps` is exempt from the profile, and sessions may be running during a
  rebuild.
- Every rebuild adds the sandbox user to `com.apple.access_ssh` when that
  group exists, whether or not `--ssh` is used.
- The sandbox user's `~/.ssh/authorized_keys` is in its own writable home.
- `TODO.md` already records that port 22 is reachable from the sandbox.

Fix:

- Make SSH group membership conditional on `--ssh`, and remove it on a
  rebuild without `--ssh`.
- Keep the password off the command line, or avoid a usable password.
- Consider denying outbound connections to local port 22 in the profile, and
  an `sshd` `Match User` block (`ForceCommand` through `sandbox-exec`) for
  installs that do use SSH mode.

### 10. Root build-home script written in place

`sv:1738-1743`

`echo ... | sudo tee "$SUDOERS_BUILD_HOME_SCRIPT_NAME"` truncates and rewrites
a script that other sessions run as root through a NOPASSWD rule. A
concurrent `sv` can execute a partial file. The sudoers file (`:1763-1777`)
and the profile (`:1979-1993`) already do this correctly.

Fix: `sudo mktemp` beside the target, write, `chmod 0554`, `bash -n`, then
`sudo /bin/mv -f` (§2, §5).

### 11. Sandbox profile is allow-default

`sv:1815-1969`

- The profile starts with `(allow default)` and subtracts. §3 says default
  deny with each allow justified by a failing probe; that discipline is
  applied to `mach-lookup` only.
- Network access is unrestricted, so host services on localhost, other
  sessions' browser debugging ports, and other profiles' iOS bridges are
  reachable.
- File reads are denied only for `/Users`, `/Volumes`, and
  `/Library/Keychains`. Anything else world-readable is in reach.
- The `com.apple.cfprefsd` and `com.apple.ocspd` allows are annotated "kept
  as insurance" and "not measured", which contradicts "entries that are not
  justified by a failing probe get removed" (997fd09).

Fix: remove or measure the two unmeasured mach names. Decide and document a
position on localhost network access; at minimum deny port 22 and consider
allow-listing outbound local ports. Treat a move to default-deny for file
reads as the longer-term direction (TODO items 1 and 2).

### 12. SSH mode passes values unquoted through the remote shell

`sv:2380-2400`

In SSH mode the `env -i` arguments are joined and re-parsed by the remote
login shell. `ZSH_COMMAND` is quoted for this and `GIT_CONFIG_VALUE_0` is
escaped, but `INITIAL_DIR`, `TERM`, `COLORTERM`, the locale variables,
`TERMINFO`, and `SV_IOS_SIMULATOR_ENDPOINT` are not.

- A working directory containing a space breaks the session.
- Shell metacharacters in a directory name are interpreted by the sandbox
  user's login shell before `sandbox-exec` is reached. Directory names in the
  workspace are sandbox-chosen.

I did not find an SSH-mode test with a spaced or unusual working directory
(§6, 74c3dfa).

Fix: build the remote command with `quote_zsh_args` for every element, not
just the zsh script. Add spaced and non-ASCII `INITIAL_DIR` tests in both
modes.

### 13. README and `/sv` skill contradict the guidelines

- `README.md:181` tells the user to run `git fetch sandvault` in the host
  repository. `sv-clone:501-508` adds that remote. §1 rule 7: no transport
  commands against a sandbox-writable repo on the host, ever. Document a safe
  route instead (for example the sandbox produces a bundle and the host
  fetches from a host-owned copy of it).
- `sv-clone:256` clones from the source without `git_safe`, and the source
  may be a workspace path.
- `skills/sandvault/sv/SKILL.md` step 2 has the host-side agent write the
  handoff file to a path inside the workspace. That is a host write by path
  into sandbox-writable territory (§1 rule 1). The example path
  (`/Users/Shared/sv-jesse/handoff-sandvault.md`) also disagrees with the
  instruction (`.../tmp/handoff-<repo>.md`). Pass the briefing on stdin or as
  the prompt argument, or stage and `mv -fh` as the README write does.
- `skills/sandvault/sv/scripts/launch-in-terminal.sh` embeds the command
  string into AppleScript and `sh -lc` strings; a repository path containing
  quotes changes what the new terminal runs.

## Low

### 14. ID-allocation lock in `/tmp`

`sv:1452-1475`

- `/tmp/sandvault-id-alloc.lock` can be pre-created by any user, including a
  sandbox user, which blocks builds for 30 seconds and then aborts.
- If the recorded PID is not signalable, the script treats the lock as stale,
  `rm -rf` fails on a directory it does not own in a sticky dir (141dc0d),
  and `continue` loops with no sleep.
- `acquire` replaces the EXIT trap and `release` clears it, discarding any
  trap set earlier.

Fix: put the lock in `/var/sandvault` (root-owned, created with `sudo
mkdir`), and sleep on every retry path.

### 15. iOS scratch dir handled by path

`sv:588-592, 662-663`, `helpers/sv-ios-bridge:647-649`

The host runs `mkdir -p "$SV_PRIVATE_DIR/tmp"`, `mktemp -d` beneath it,
`os.makedirs(scratch_dir)`, and later `rm -rf "$IOS_BRIDGE_SCRATCH_DIR"`, all
by path through sandbox-writable parents. The bridge's own writes go through
a directory fd, which is right; creation and removal do not.

Fix: have the sandbox user create and remove the scratch directory, or open
the parent with `O_DIRECTORY|O_NOFOLLOW` and use `dir_fd` for both.

### 16. `sv-clone` input handling

- `REPOSITORY_NAME` (`:211-226`) is not validated. `..`, `.`, whitespace, and
  quotes all pass and flow into `REPO_DIR`, `DEPLOY_KEY_PRIV`, and the
  single-quoted `core.sshCommand` value (`:381`). Require
  `^[A-Za-z0-9._-]+$` and reject `.` and `..`.
- The deploy-key title is interpolated into a jq program (`:391`). Compare in
  the shell instead, or pass it through the environment.
- `gh` calls discard stderr (`:392, 398-401, 418`), hiding the reason for a
  failure (§6).
- `printf %q` is used for paths (`:345-346`); §6 notes it mangles non-ASCII.

### 17. `uninstall --all` sweeps directories that may be the user's

`sv:1001-1005, 1019-1023, 1026`

Pass 2 removes every subdirectory of `authorized_keys.d/` and of
`~/.local/state/sandvault/`, whether or not it corresponds to a profile this
tool created, including user-supplied public keys (§5: "never auto-delete
something that could be the user's"). `sudo rmdir "$(dirname
"$INSTALL_PRODUCT/x")"` at `:1026` runs `rmdir` on a host config directory as
root for no evident reason.

### 18. Bash 4 syntax in the skill launcher

`skills/sandvault/sv/scripts/launch-in-terminal.sh` uses `${1,,}`, which §6
rules out. It runs under `env bash`, so it fails on a machine with only
`/bin/bash` 3.2.

### 19. Supply-chain exposure

- `sv:329` installs Homebrew with `curl | bash` from the `HEAD` of the
  installer repository, as the host user.
- `sv:359-376` runs freshly installed agent binaries (`--help`, `--version`)
  as the host user to clear quarantine. Those binaries come from third-party
  taps (`anomalyco/tap`, `lightpanda-io/browser`).
- `.github/workflows/*.yml` pin actions by tag rather than commit SHA,
  including `mislav/bump-homebrew-formula-action@v4.2`, which receives
  `HOMEBREW_GITHUB_API_TOKEN`. The `tag-version` job checks out with that
  token and leaves it persisted in the checkout.

## What holds up

These paths follow the guidelines and nothing was found in them:

- README delivery: stage in `/Users/Shared`, set mode and ACL, `mv -fh`.
- Deploy-key install over stdin as the sandbox user; public-key read-back
  with strict validation; upload only from the host-owned copy.
- Fetch of an existing clone inside the sandbox, with a host-built bundle.
- Home sync as host-read, sandbox-write `tar`.
- Sudoers and sandbox profile: root temp file, validate, rename.
- `authorized_keys` generation in host-only config, with the private-key
  rejection.
- Bridge screenshot publishing through a directory fd.
- Guest wrappers: explicit binary paths, re-entry guard, banners that match
  the code path.

## Suggested order

1. Item 7 today: it is a setup change and removes the widest exposure.
2. Items 1, 2, 3: the class the guidelines exist to prevent, each with a
   structural fix already used elsewhere in the repo.
3. Items 4 and 5, with planted-attack tests.
4. Verify item 6 from the host side, then document or restrict `--ios`.
5. Items 8-13, then the low items.
