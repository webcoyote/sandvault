# Sandvault 2 design

Status: milestone 1 (shell sessions) is implemented in `sv2/`; see
"Milestone 1" near the end for what exists and what it decided. Everything
else is design. Decisions marked **Decided** were made on 2026-11-24. Open
items are listed at the end.

Section references such as §1 are to `docs/review-guidelines.md`. "Finding N"
refers to `docs/security-review-2026-11-24.md`.

## Goals

1. Rewrite in Rust as a single compiled binary plus one small root helper.
2. Need `sudo` only for install, uninstall, and allowing or denying a user. Creating and
   deleting sandboxes needs no password, and works for users without admin
   rights.
3. Make account and workspace names impossible to confuse between users.
4. Remove features that put the host user or root to work inside
   sandbox-writable directories.
5. Add an AI agent without changing sandvault itself.

## The rule the design follows

Almost every escape in v1's history is a confused deputy: the host user or
root performs an ordinary file or git operation on a path the sandbox
controls. The first fix in that class was wrong three times, and each time the
fix that held was the one that removed the access (§1).

So v2 starts from this rule:

> The host user and root never read, write, or run tools inside the workspace
> or the sandbox home. The sandbox user does every operation there.

The rule binds sandvault's own code (`sv2` and the helper), not the person:
the host user still edits files in the workspace by hand.

There are two stated exceptions: `sv2 repair` (see "Workspace permissions")
and the iOS bridge (see "iOS bridge").

Of the 19 findings in the security review, about 12 no longer have a code
path under this rule and the feature removals below.

## Trust model

| Actor | Trust |
|---|---|
| Root helper | The only code that runs as root. Trusts nothing it is passed. |
| Admin who runs `sv2 install` | Trusted. Decides who may create sandboxes. |
| Host user | Trusted with their own sandboxes. **Not** trusted by root or by other users. |
| Sandbox user | Hostile. Runs arbitrary code, concurrently, at all times. |
| Adversary | Steers the agent; can plant something now that fires on a later run. |


## Language and structure

Rust, split in two:

- **`sv2`**, the command-line tool. Runs as the host user. Ordinary
  dependencies are fine.
- **`sv2-helper`**, the root helper. As close to zero dependencies as
  possible, because every crate in it runs as root. Commit `Cargo.lock`,
  vendor or audit its dependency tree, and pin CI actions by commit SHA.

What Rust buys:

- All of §6 (bash 3.2 idioms, lost exit codes) goes away.
- Arguments are passed as a vector from `sv2` to the agent. No shell re-parses
  them, which removes the quoting class (finding 12).
- File operations can be done relative to a directory handle without following
  links, which bash cannot do.
- A launch drops from roughly 60–90 processes to a handful.

What Rust does not buy: the confused-deputy class. That is closed by the rule
above, not by the language. Concurrency is not a benefit here either; the work
is a few system calls.

User and group creation still shells out to `dscl` and `dseditgroup`. That
happens only when a profile is added.

## Privileged helper

**Decided:** a root-owned helper invoked through one sudoers line for v2. A
launchd daemon is a planned follow-up.

- The helper is installed outside the Homebrew prefix, which an admin user can
  write. `sv2 install` is run without `sudo` and refuses to run as root. It
  runs `sudo <prefix>/sv2-helper install` itself, which asks for the
  password; the helper copies its own executable to a root-owned path and
  writes the other root-owned artifacts. `sv2` itself never runs as root.
- `sv2` and the helper check each other's version.
- The helper is not setuid, because Homebrew cannot install a setuid binary.
- One sudoers line:
  `%sandvault-users ALL=(root) NOPASSWD: /Library/PrivilegedHelperTools/sv2-helper call *`.
  There is no per-user sudoers file. The helper's first argument is a mode.
  `call` (the profile verbs and `launch`) is what the line covers. `install`,
  `uninstall`, `user-allow` and `user-deny` are in `admin` mode, which the
  line does not match, so sudo itself asks for a password and requires general
  sudo rights. The helper cannot tell whether a password was typed; sudo can.
  The same file sets `use_pty` for the helper, so every session runs on a
  terminal of its own (see "Milestone 1").
- The helper's interface is designed as a wire protocol so it can move behind
  a daemon without redesign:
  - narrow verbs only: `profile-add NAME`, `profile-remove NAME`,
    `profile-list`, `launch NAME -- ARGV`, `user-allow USER`,
    `user-deny USER`, `install`, `uninstall`;
  - no path arguments;
  - the caller is identified by `SUDO_UID`. Under sudo the helper's real UID
    is 0, so this is the only identity available; sudo sets the variable
    itself and the caller cannot override it. The helper refuses to run
    unless its real UID is 0 and `SUDO_UID` is set, rejects a `SUDO_UID` that
    is a sandbox account, and takes identity from no argument and no other
    variable;
  - root never opens a host-owned file. The guest payload is embedded in the
    binary.
- A per-user limit on the number of profiles, since each one is a real
  account.
- UID allocation and the registry are locked in a root-owned directory, not
  `/tmp` (finding 14).
- It is written with setuid-level care: the caller controls its working
  directory, resource limits, and timing, and can kill it halfway through.
  Every operation must be safe to interrupt and repeat.

Why the daemon comes later: it is the stronger design for an untrusted caller
(the caller can only send it messages), but it needs a terminal proxy, a
versioned protocol, and a root socket that sandboxed processes must be kept
away from.

## Session launch

**Decided:** every session runs in the sandbox user's own launchd domain. This
is the approach from the `use-launchd` branch (commit f9d2924).

Background: `sudo -u` alone keeps the caller's Mach bootstrap, so a v1 session
looked up per-user services in the host user's login session and reached the
host user's agents. `launchctl asuser <uid>`, run as root, moves the session
into `user/<sandbox uid>`. Services the profile allows, such as
`com.apple.cfprefsd.agent` and `com.apple.trustd.agent`, are then the sandbox
user's own.

The launch sequence in v2:

1. `sv2` runs `sudo sv2-helper launch NAME -- ARGV`.
2. The helper looks up the profile and its UID in the registry.
3. The helper runs `/bin/launchctl asuser <uid>` on a second stage of itself.
4. The second stage sets groups and UID, clears the environment, applies the
   sandbox profile, and executes the command.

Consequences:

- The sandbox profile is always applied. There is no `--no-sandbox`.
- No inner `sudo --login`, so no login shell re-parses arguments or sources
  `.zshenv` and `.zprofile` twice.
- No per-user launcher script and no launcher version stamp; the UID comes
  from the registry.
- Go programs verify TLS through the sandbox user's own trust agent, so the
  `SSL_CERT_FILE` workaround is not needed.
- The later daemon needs the same `asuser` step, since its own domain has no
  per-user agents.

Acceptance test, carried over from the branch: inside a session,
`launchctl manageruid` equals `id -u`.

### Terminal multiplexers

tmux and screen work without SSH, in two ways:

- **On the host.** Run `sv2` inside a host tmux pane. To tmux this is an
  ordinary program in a terminal; nothing is needed.
- **Inside the sandbox.** Run `sv2 run shell tmux new -A -s main`, then detach,
  close the terminal, and reattach later with the same command. The server
  runs as the sandbox user, under the sandbox profile, in the sandbox user's
  launchd domain.

The second way needs one thing from the launch: **detached processes may
outlive `sv2 run`.** The helper does not kill the session's process group when
the command exits. This grants nothing new: the trust model already assumes
the sandbox runs code at all times, and the sandbox profile is inherited
across `fork`, so a tmux server stays confined. In exchange, `profile-remove`
kills every process of the sandbox UID before it deletes the account.

The sandbox profile must allow tmux's socket directory
(`/private/tmp/tmux-<uid>`), measured the same way as other allow rules (§3).
The pasteboard service is denied, so copying to the host clipboard from inside
tmux needs OSC 52 (`set -g set-clipboard on`), which goes through the host
terminal.

## Names and ownership

**Decided:** username plus a mandatory profile. The default profile is named
`default`.

The v1 collision exists because the profile is optional: `pat` + `work` and
`pat-work` + nothing produce the same string. Profiles match `^[a-z0-9]{1,16}$`
and so never contain a dash. With a profile always present, the last dash
separates user from profile unambiguously.

- **Account:** `sv2-<user>-<profile>`, for example `sv2-pat-default` and
  `sv2-pat-work`. The `sv2-` prefix keeps v2 accounts distinct from v1's
  `sandvault-<user>-<profile>` accounts, so both versions can be installed
  side by side and v2 never adopts or modifies a v1 account.
- **Workspace:** `/Users/Shared/sv2/<user>/<profile>/`. The directories `sv2/`
  and `sv2/<user>/` are root-owned and not world-writable, so no sandbox can
  pre-create the name of a future profile (finding 5).
- **Ownership is recorded, not parsed.** A root-owned registry, keyed by host
  UID, lists each user's profiles. The helper reads the registry and never
  derives ownership from a name. This replaces v1's collision guard, its
  listing by regular expression, and "the host user is in the sandbox group"
  as the ownership record.
- **Host usernames** are validated against an explicit character set and a
  length limit, and are never placed unescaped into a pattern or a filename.

## Workspace permissions

- Root never operates inside the workspace. v1's recursive `chown` and ACL
  passes are removed (finding 5).
- New files and directories get access through inherited ACL entries, as
  today.
- Files moved in from elsewhere keep their old permissions. The host user owns
  them and can fix them without root: `sv2 repair PATH` does this explicitly.
- `sv2 repair PATH` only fixes permissions, and only inside the caller's own
  workspace tree; any other path is an error. It runs as the host user, never
  as root. Because it is a host operation on sandbox-writable paths, it is an
  exception to the rule and is held to §1: it walks the tree without following
  links, changes each entry through an open file descriptor rather than by
  path, and skips (with a warning) anything the host user does not own or
  that has more than one hard link.
- `sv2 profile remove` never deletes the user's files. The helper removes the
  workspace directory only if it is empty. Otherwise it leaves it, says so,
  and a later `profile add` of the same name reuses it.
- Nothing managed by sandvault lives in the workspace. v1's `_sandvault/`
  directory (setup scripts, state files, scratch space) is gone (finding 1).
  State that grants access lives in host-only or root-only locations (§2).

## Sandbox profile

- Each sandvault profile has its own `sandbox-exec` profile file, so profiles
  can later be given different capabilities, as defined by the host user.
- The files are root-owned and live outside the workspace and the sandbox
  home, for example `/Library/Application Support/sandvault2/profiles/<user>/<profile>.sb`.
  The helper writes one on `profile-add` and deletes it on `profile-remove`.
  The sandbox user cannot edit its own file.
- Root never opens a host-owned file, so `sv2` sends the host user's
  capability choices to the helper on stdin. The helper validates them and
  renders the file from the embedded base profile. The input format is an open
  item.
- **Decided:** the first milestone ports v1's profile unchanged
  (allow-default with its deny list). The default-deny work below follows
  once sessions run, and is required before release.
- Default deny (finding 11). Each allow rule is written in the source next to
  the tool that needs it and the symptom without it, measured as the sandbox
  user under the rendered profile (§3). The probes are kept as tests.
- The Mach service allow list stays short, and each entry is re-measured in
  the sandbox user's own domain.
- A stated position on localhost networking. At minimum, deny local port 22.

## Closing other routes out of the sandbox

The profile only confines processes that sandvault starts. If Remote Login is
on for all users, the sandbox user can connect to its own account, because
`authorized_keys` is in its own writable home.

`sv2 install` therefore:

- puts all sandbox accounts in one shared group and installs an sshd drop-in
  that refuses that group (no keys, no passwords, forced command
  `/usr/bin/false`);
- gives sandbox accounts no usable password;
- never adds them to `com.apple.access_ssh`.

Other routes to audit before release: cron, `at`, launchd agents, Screen
Sharing.

## Command line

```
sv2 install                       # asks for sudo, once per machine
sv2 uninstall                     # asks for sudo
sv2 user allow USER               # asks for sudo
sv2 user deny USER                # asks for sudo
sv2 profile add NAME
sv2 profile remove NAME
sv2 profile list
sv2 run [OPTIONS] NAME [ARGS...]
sv2 [OPTIONS] NAME [ARGS...]      # shorthand for sv2 run
sv2 repair PATH
sv2 doctor
```

- `sv2 user allow USER` and `sv2 user deny USER` add a user to, or remove one
  from, the `sandvault-users` group named in the sudoers line. `sv2 install`
  adds the admin who runs it.
- Options come before `NAME`. Everything after `NAME` is passed to the agent
  unchanged; no `--` is needed.
- `sv2 run` creates the `default` profile on first use.
- `sv2 run --profile work claude` selects a profile; `-p` is the short form.
- One uninstall path, which removes only what the
  registry lists (finding 17).

**Decided:** a bare agent name is accepted as shorthand. `sv2 claude` is
`sv2 run claude`, and `sv2 --profile work claude` is
`sv2 run --profile work claude`.

- Subcommands always win. An agent whose name matches a subcommand is
  reachable only through `sv2 run NAME`, and `sv2 doctor` warns about it. This
  way a subcommand added later cannot be shadowed by a user's agent, and a
  user's agent cannot change what `sv2 install` or `sv2 uninstall` does.
- A first argument that is neither a subcommand nor a known agent is an error
  ("unknown command or agent"). `sv2` never searches `PATH` for it.

## Agents

`NAME` in `sv2 run NAME` selects an agent definition. Adding an agent means
adding one definition; `sv2` does not change.

- **Where definitions live:**
  - Built-ins are installed with `sv2` in `<prefix>/share/sandvault2/agents/`.
    `sv2` finds this directory from its own resolved executable path
    (`../share/sandvault2/agents`), never from the current directory or
    `PATH`. Built-ins can then be added or fixed without rebuilding the binary.
  - User definitions live in host-only `~/.config/sandvault2/agents/`.
  - `shell` is embedded in the binary, so `sv2 run shell` works even when the
    agents directory is missing or broken.
  - A development build with no `share/` directory falls back to the
    `agents/` directory of the source tree it was built from.
- **Precedence:** a user definition with the same name as a built-in replaces
  it as a whole directory, not file by file. The session banner names the
  directory that was used.
- **Definition directories must not be writable by the sandbox.** A definition
  names the Homebrew package that `sv2` installs as the host user, and
  Homebrew formulae run arbitrary code, so a definition the sandbox can edit
  gives the sandbox code execution as the host user. Before reading a
  directory, `sv2` refuses it if it lies under a workspace or a sandbox home,
  or if any directory on its path is writable by a sandbox account
  (world-writable, or writable through a group or ACL entry that includes
  one). A directory owned by another host user is accepted: for a non-admin
  user the Homebrew prefix belongs to an admin, and that admin can already
  replace `sv2` itself. Ownership alone is not enough inside the workspace,
  because the sandbox can move host-owned directories into place. This applies to the development fallback
  too: a checkout inside the workspace is refused (finding 7). The rule covers
  the whole definition directory, including guest scripts, so there is one
  rule rather than one per file.
- **Versioning:** each definition carries a schema version that `sv2` checks.
  `sv2 doctor` reports missing or incompatible built-ins.
- **How they reach the sandbox:** `sv2` reads the definition, including its
  guest scripts, as the host user and passes it to the session over a pipe.
  Root never opens it, and the sandbox cannot edit it.
- **Names** match `^[a-z0-9][a-z0-9-]*$` and are looked up only in the places
  above, never on `PATH`.
- `shell` is a built-in definition, embedded in the binary (see above).
- §7 still applies: resolve the binary by explicit path, check injected flags
  against the real `--help`, and state in the banner what trust a flag grants.

### Definition format

**Decided:** one directory per agent, holding an `agent.toml` for data and
separate shell scripts for steps that run inside the sandbox.

```
agents/claude/
  agent.toml
  install.sh      # guest native install, runs as the sandbox user
```

A definition does three things, and how much code each may contain depends on
which user runs it:

1. **Host install is data only.** It runs as the host user, outside the
   sandbox. A host-side script would make every definition arbitrary host
   code, and could reach into the workspace and reopen the confused-deputy
   class. The definition names a package; `sv2` builds the argument vector
   itself (for example `brew install --formula claude-code`), validates the
   name, shows what it will run, and passes it with no shell (finding 12).
   A new method, such as MacPorts, is a new key plus reviewed code in `sv2`.
2. **Guest install may be a script.** It runs as the sandbox user, under the
   sandbox profile, into a home the sandbox already controls, so arbitrary
   code there grants nothing new. Scripts are separate `.sh` files, not
   strings inside TOML, so they need no escaping and are checked by
   shellcheck like the rest of the repository.
3. **Run is mostly data,** with an optional guest-side wrapper script. Binary
   location, permission-bypass flag, system prompt injection, and the
   subcommands that must not receive injected flags are data, because `sv2`
   needs them for the banner, for checking flags against `--help` (§7), and
   for `sv2 doctor`. An agent that does not fit can add a wrapper script,
   which runs inside the sandbox.

Example (values are illustrative):

```toml
schema = 1
binary = ["~/.local/bin/claude", "{brew}/bin/claude"]

[install.host]          # data only; sv2 builds the argv
brew = { formula = "claude-code" }

[install.guest]
script = "install.sh"

[run]
bypass_flag = "--dangerously-skip-permissions"
system_prompt = { flag = "--append-system-prompt" }
no_inject = ["mcp", "config", "update"]
# wrapper = "run.sh"    # optional, runs inside the sandbox
```

Why not the alternatives:

- **Bash scripts only:** v1's wrappers are about 80% identical, and the shared
  part drifted. Host install would become host code, and `sv2` could not
  inspect a definition for the banner or `doctor`.
- **JSON:** no comments and no multi-line strings. Each flag and install step
  should carry a comment saying why it is there.
- **YAML:** implicit typing and indentation errors, and nothing gained over
  TOML. Rust's `toml` crate reads TOML directly through `serde`.

### Installing agents

**Decided:** Homebrew by default; native install with
`sv2 run --native NAME`.

- Homebrew runs as the host user from `sv2`, never from the root helper.
- `sv2` requires an existing Homebrew and does not install it. If Homebrew is
  missing, or its prefix is not writable (non-admin users), the error points
  to `--native`.
- A native install runs the agent's own installer inside the sandbox, into the
  sandbox home.
- A native binary already present in the sandbox home is preferred, so
  `--native` is needed only the first time.

## Features removed

| Removed | Reason | Replacement |
|---|---|---|
| SSH mode (`--ssh`, `authorized_keys.d`) | Findings 8, 9, 12; the account password; half the test matrix. Its one isolation benefit (own login context) now applies to every session. | None needed for tmux or screen: see "Terminal multiplexers". Inbound SSH from other machines (v1's `authorized_keys.d`) is out of scope for v2. |
| `sv-clone` and the `/sv` skill | Findings 3, 4, 13, 16, 18: host git and host writes in the workspace | Deploy-key script, below |
| `sv-agentsview-setup` | Finding 2; about 1,600 lines including vendored code | None |
| `--no-sandbox` | The profile is mandatory | None |
| Nested `sv` | Trusted an environment variable to detect nesting | None |
| `SANDVAULT_ARGS` | Second argument parser with different quoting | Shell alias |
| `uninstall --all`, `--fix-permissions` | Root passes over the workspace | `sv2 uninstall`, `sv2 repair` |
| Installing Homebrew via `curl \| bash` | Finding 19 | Require Homebrew or use `--native` |
| Host-written files in the workspace | Findings 1, 15 | Embedded guest payload |

Browser support (`--chrome`, `--lightpanda`) stays. It already runs inside the
sandbox.

## Repository credentials

**Decided:** `sv-clone` is replaced by a script that runs inside the sandbox.

- The script generates a deploy key in the sandbox and prints the
  `gh repo deploy-key add` command. The user runs that command on the host and
  sees exactly which repository and access level it names.
- The sandbox clones and pushes. The host gets results by pulling from GitHub.
- The host never runs git against the workspace (§1 rule 7).

`sv-gh-token` carries over. It already follows the rule: the token is sent on
stdin to a script running as the sandbox user, and the host writes nothing
into the workspace.

## iOS bridge

**Decided:** redesign in v2 rather than drop.

The bridge is a server running as the host user that lets the sandbox install
and launch simulator apps (finding 6). It is the one exception to the rule at
the top of this document, so it is opt-in and held to these requirements:

- a per-session token on every request, and a `Host` header check;
- installs limited to this profile's workspace;
- the app bundle is copied into a host-only directory without following links,
  and only the copy is installed;
- the scratch directory is created and removed by the sandbox user, or
  accessed through a directory handle (finding 15);
- every argument is validated and passed so it cannot be read as an option;
- the bridge runs as the host user under `sv2`, never from the root helper;
- the README states exactly what trust `--ios` grants.

## Installed artifacts and upgrades

v2 installs four things as root: the helper, one sudoers line, the sshd
drop-in, and the registry. The helper also writes one `sandbox-exec` profile
file per sandvault profile. Each of these carries a version and has an upgrade
path, and each is written to a temporary file, validated, then renamed into
place (§2, §5). Upgrading the base profile means re-rendering every profile
file listed in the registry.

There are no generated root scripts (finding 10) and no per-launch setup. The
guest side unpacks the embedded payload only when its version stamp changes.

## Tests

- Naming, profile rendering, registry handling, and argument parsing are unit
  tests that need no privileges.
- The planted-attack tests in v1's `scripts/tests` are the acceptance suite.
  They and the review guidelines are the real specification.
- §8 still applies to integration tests: throwaway fixtures only, a test that
  plants the attack for every security fix, and verification from the host
  side.

## Development

A checkout inside the workspace lets a sandboxed agent edit the source of the
root helper (finding 7). A helper build must be reviewed, and built from a
checkout the sandbox cannot write, before `sv2 install`.

## Milestone 1

Implemented in `sv2/` (a Cargo workspace): `sv2 install`, `uninstall`,
`user allow|deny`, `profile add|remove|list`, and `sv2 run shell`, with v1's
sandbox profile ported unchanged.

| Crate | Binary | Runs as | Dependencies |
|---|---|---|---|
| `crates/common` | (library) | both | none |
| `crates/helper` | `sv2-helper` | root | `libc` |
| `crates/cli` | `sv2` | host user | `libc` |

Build and test: `scripts/tests-sv2` (also run by `scripts/validate`). Unit
tests need no privileges. `sv2/tests/integration` creates a throwaway profile
and needs this build installed (`sv2/target/release/sv2 install`); without
that it reports itself SKIPPED, and fails under `SV_TESTS_FAIL_ON_SKIP=1`.

Decisions this milestone made:

- **Helper modes.** See the sudoers line under "Privileged helper".
- **Registry format (open item 6).** A line-oriented text file, documented at
  the top of `crates/helper/src/registry.rs`. Each entry has a state:
  `adding`, `active`, or `removing`. The state is written before the first
  system change and every later step is idempotent, so a killed
  `profile-add` or `profile-remove` is finished by running it again.
  `next-id` only grows: an account ID is never reused, so files a removed
  sandbox left in `/tmp` are never owned by a later sandbox. Two limits:
  `uninstall` deletes the registry, so a reinstall starts again at 600; and
  nothing reclaims IDs, so the range (600-60000) can be used up by repeated
  add and remove.
- **Launch (part of open item 5).** The helper takes no environment and no
  path from its caller. A session gets a fixed environment (`HOME`, `USER`,
  `LOGNAME`, `SHELL`, a system `PATH`, `TMPDIR`, `SV2_HOST_USER`,
  `SV2_PROFILE`, `SV2_WORKSPACE`). Whatever else `sv2` wants (terminal
  variables, the starting directory) it puts in the command line, as
  `/usr/bin/env NAME=value ... /bin/zsh -c 'cd ...'`, which runs as the
  sandbox user. The command must be an absolute path: it follows
  `sandbox-exec -f FILE`, where a leading `-p` would replace the profile.
- **Workspace access.** ACL entries name the sandbox user and the host user
  directly, so the host user joins no group. The entry on the workspace
  directory itself omits `delete`, `writesecurity` and `chown`: the sandbox
  cannot remove, move, or re-permission the workspace. Inherited entries for
  files omit `search` (the execute bit), as in v1.
- **Sandbox home.** Mode 0700, owned by the sandbox user. On
  `profile remove` the sandbox user empties it and root removes the empty
  directory; root never walks it.
- **Removal and launch.** `launch` holds the registry lock (shared) until its
  process has become the sandbox user; `profile-remove` takes it exclusively
  before it kills the account's processes.
- **Terminal (open item 4).** A session does not share the host's terminal.
  sudo runs the helper on a new pty and relays input, output and window size.
  On a shared terminal the sandbox could queue keystrokes with `TIOCSTI` for
  the host shell to run after the session ends; the integration suite plants
  that attack, with a control that shows it works when the terminal is
  shared. sudo 1.9.14 and later do this by default; the sudoers file says
  `Defaults!<helper> use_pty` so it does not depend on the sudo version or on
  another sudoers file turning the default off. tmux needs nothing further:
  the base profile allows writes under `/private/tmp`, where its socket
  directory is.
- **Allowed-users group.** v1 would name the sandbox group of a host user
  called `users` exactly `sandvault-users`. `install` therefore refuses a
  group of that name it did not create (it checks the group's RealName).

## Remaining work

What milestone 1 left undone, untested, or knowingly imperfect. Remove an item
when it is fixed. Design questions still to be answered are under "Open
items" below.

### Not yet verified

Do these before building anything else on milestone 1.

- [ ] **CI steps are unverified.** `.github/workflows/ci.yml` gained "Install
  sandvault2" and "SandVault2 Tests"; neither has run on a runner.
- [ ] **`use_pty` has not been tested against its absence.** The terminal
  checks passed before the `use_pty` line existed, because this machine's sudo
  (1.9.17) defaults to it. Nothing has run on a sudo older than 1.9.14, or
  with `Defaults !use_pty` set elsewhere, to show the line is what holds.
- [ ] **The workspace ACL checks have not been mutation-checked.** The
  integration cases for the workspace directory (sandbox cannot chmod, chown,
  re-ACL, delete or rename it) and for inheritance (no execute bit, host
  access to sandbox-private entries) pass, but no build with a weakened ACL
  has been installed to confirm they fail.

### Security gaps

- [ ] **SSH to self leaves the sandbox.** With Remote Login enabled for all
  users, a sandbox account can `ssh localhost` and get a shell without the
  `sandbox-exec` profile. Fix: the sshd drop-in and shared sandbox group
  ("Closing other routes out of the sandbox").
- [ ] **launchd as a route around the profile** (open item 7). The direct
  routes are refused (measured, not yet a test). Still open: a plist the
  sandbox plants in `~/Library/LaunchAgents` may load when the account's
  launchd domain is next created, and would run unconfined as the sandbox
  user.
- [ ] **`install` reads its own executable by path.** The helper copies itself
  to `/Library/PrivilegedHelperTools` by reopening the path. Whoever can write
  the directory it was run from (the Homebrew prefix) can swap the file
  between sudo's exec and the copy. Fix: copy from an already-open descriptor,
  or have `sv2` pass the file on a descriptor and verify a hash.
- [ ] **A suspended helper blocks everyone.** An allowed user can stop the
  helper (^Z) while it holds the exclusive registry lock; every other user's
  `sv2` command then waits forever. Fix: ignore job-control stop signals while
  the lock is held, and/or give waiters a timeout with a message naming the
  holder.
- [ ] **Fork storm against `profile remove`.** `kill_all_processes_of` loops
  `kill(-1, SIGKILL)` as the sandbox user; a process forking fast enough can
  outlast the loop's bound and removal fails (safely, state stays `removing`).
  Fix: take away the account's ability to run first (e.g. `launchctl bootout`
  plus a per-user process limit), then kill.
- [ ] **A pre-created `/Users/Shared/sv2` blocks install.** `/Users/Shared` is
  world-writable, so any local user can create the directory first; `install`
  refuses a directory root does not own, and an admin must delete it by hand.
  Fix: decide whether `install` may replace an empty one, or move the root
  somewhere users cannot write.

### Functional gaps

- [ ] **Kept workspaces are unreadable after re-add.** `profile remove` keeps a
  non-empty workspace. A later `profile add` reuses the directory, but the new
  account has a new ID and the old files' ACLs name the deleted one. Needs
  `sv2 repair` (below).
- [ ] **Account IDs.** `uninstall` deletes the registry, so a reinstall starts
  numbering at 600 again and a new sandbox can own files an old one left in
  `/tmp`. Nothing reclaims IDs, so 600-60000 can be used up by repeated add
  and remove. Fix: keep `next-id` somewhere `uninstall` does not delete, and
  decide on wraparound.
- [ ] **Group name `sandvault-users`.** v1 gives a host user named `users` a
  sandbox group of exactly this name. `install` refuses a group it did not
  create, so the two cannot be confused, but they also cannot coexist.
  Renaming to `sv2-users` removes the overlap; do it before anyone installs.
- [ ] **No guest payload.** Sessions get Homebrew on `PATH` from `sv2`, and the
  sandbox user's shell creates an empty `~/.zshrc`. The embedded payload
  (shell setup, `install.sh` runner) is not written.

### Features not in milestone 1

In rough dependency order.

- [ ] `sv2 repair` (re-apply ACLs and ownership; finish half-done add/remove).
  The name is reserved and reports "not implemented yet".
- [ ] `sv2 doctor`. Reserved the same way.
- [ ] sshd drop-in and the shared sandbox group (closes the SSH gap above).
- [ ] Embedded guest payload and the guest-side launcher (open item 5).
- [ ] Agent definitions: format, `install.sh`, Homebrew and native install,
  `--native`.
- [ ] Quarantine handling for installed agent binaries (open item 1).
- [ ] Default deny, and per-profile capabilities (open item 8).
- [ ] Repository credentials.
- [ ] Browser support.
- [ ] iOS bridge, and the simulator trust question (open item 2).
- [ ] Homebrew packaging of `sv2` itself, and upgrade handling when `sv2` and
  the installed helper differ in version.
- [ ] Move from the sudoers helper to a launchd daemon (open item 3).

## Open items

1. **Quarantine.** v1 clears quarantine by running freshly installed agent
   binaries as the host user. Find a way that does not execute them (remove
   the attribute, or install without quarantine).
2. **Simulator trust.** Confirm from the host side whether simulator apps run
   unconfined as the host user, then document what `--ios` grants.
3. **Daemon follow-up.** Scope and timing of the move from the sudoers helper
   to a launchd daemon.
4. **Terminal.** Decided in milestone 1; see "Milestone 1".
5. **Guest-side launcher.** Which component, running as the sandbox user,
   resolves the agent binary, runs `install.sh`, and builds the final
   argument vector: the helper after it drops privileges, or a script in the
   embedded payload. Also how the agent definition, initial directory, and
   terminal variables reach it, given that sudo passes only stdin, stdout,
   and stderr. Milestone 1 answers the last part for `shell` (see
   "Milestone 1"); agent definitions are still open.
6. **Registry format.** Decided in milestone 1; see "Milestone 1".
7. **launchd as a route around the profile.** Measure whether a session can
   start a job in its own launchd domain (`launchctl bootstrap` or `submit`,
   or a plist in `~/Library/LaunchAgents`). Such a job would run as the
   sandbox user without the `sandbox-exec` profile.

   Measured on macOS 26.7 from inside a session: `launchctl submit`,
   `bootstrap user/<uid>`, `bootstrap gui/<uid>` and `load` all fail, and no
   job ran. Not measured: whether a planted `~/Library/LaunchAgents` plist
   (with `LimitLoadToSessionType` `Background`, the domain's session type)
   loads when the domain is created. The domain outlives the session, because
   launchd starts system agents in it (`distnoted`) that keep running, so the
   test needs `launchctl bootout user/<uid>` as root, or a reboot, before the
   next session. Those system agents also run as the sandbox user outside the
   profile; they are Apple's code, reachable only through what the profile
   allows.
8. **Per-profile capabilities.** The format the host user uses to define a
   profile's capabilities: a fixed set of named capabilities the helper maps
   to rules it already contains, or raw SBPL rules added to the base profile.
   Named capabilities keep root's parser small and every rule reviewed. Raw
   SBPL is more flexible, but the helper then has to parse and validate it.
