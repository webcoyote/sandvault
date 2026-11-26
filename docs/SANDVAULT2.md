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
- `sv2` and the helper compare artifact versions, not the application
  version (see "Artifact versions" under "Milestone 1").
- The helper is not setuid, because Homebrew cannot install a setuid binary.
- One sudoers line:
  `%sv2-users ALL=(root) NOPASSWD: /Library/PrivilegedHelperTools/sv2-helper call *`.
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
- `sv2 repair PATH` only fixes ownership and permissions, and only inside
  the caller's own workspace tree; any other path is an error. It runs as the
  host user, never as root. Because it is a host operation on
  sandbox-writable paths, it is an exception to the rule and is held to §1:
  it walks the tree without following links, changes each entry through an
  open file descriptor rather than by path, and skips (with a warning, and a
  failing exit status) a file that has more than one hard link and anything
  that belongs to another account. See "`sv2 repair`" under "Milestone 1" for
  what was built.
- **Decided (2026-11-25):** the same pass runs by itself, as the host user,
  after `sv2 profile add` and `sv2 profile remove`, and it makes the host
  user the owner of what the sandbox account owned. A kept workspace is the
  host user's own files after a remove, and works with the new account after
  an add.
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
- **Decided:** the first milestone ported v1's profile unchanged
  (allow-default with its deny list). It is default deny since 2026-11-26;
  see "Default deny" under "Milestone 1".
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

This is built; see "SSH lockdown" under "Milestone 1".

Other routes to audit before release: cron, `at`, launchd agents, Screen
Sharing.

## Command line

```
sv2 install                       # asks for sudo, once per machine
sv2 uninstall                     # asks for sudo; refuses while profiles exist
sv2 uninstall --all-profiles      # asks for sudo; removes every profile too
sv2 user allow USER               # asks for sudo
sv2 user deny USER                # asks for sudo
sv2 profile add NAME
sv2 profile remove NAME
sv2 profile list
sv2 run [OPTIONS] NAME [ARGS...]
sv2 [OPTIONS] NAME [ARGS...]      # shorthand for sv2 run
sv2 repair PATH
sv2 doctor
sv2 agent new NAME [--from AGENT]
sv2 deploy-key [--profile NAME] OWNER/REPO [--write]
sv2 gh-token [--profile NAME] [login | status | logout]
```

- `sv2 user allow USER` and `sv2 user deny USER` add a user to, or remove one
  from, the `sv2-users` group named in the sudoers line. `sv2 install`
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

**Decided (2026-11-25):** names may be shortened. A subcommand, a verb after
`user` or `profile`, and an agent name are each accepted as any prefix that
fits exactly one name: `sv2 s` and `sv2 sh` are `sv2 shell`, and `sv2 p l` is
`sv2 profile list`.

- A whole name is always itself, even when a longer name starts with it.
- A prefix that fits several names runs nothing, and the error lists them
  (`sv2 r`: `run`, `repair`). The first word is matched against subcommands
  and agents together, so a prefix shared by one of each is ambiguous too.
  A subcommand or agent added later can therefore turn a prefix that worked
  into an error, but never into a different command.
- `uninstall` and `profile remove` delete things without asking, and must be
  written in full: `sv2 un` and `sv2 profile r NAME` are refused.
- After `run` or an option only an agent can follow, so only agent names are
  matched there.
- Options and profile names are not shortened.

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
  guest scripts, as the host user and passes it to the session on the
  command line, as it does the guest launcher (see "Guest launcher" under
  "Milestone 1"). Root never opens it, and the sandbox cannot edit it.
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
file per sandvault profile. Each of these carries a version (see "Artifact
versions" under "Milestone 1") and has an upgrade path, and each is written
to a temporary file, validated, then renamed into place (§2, §5). Upgrading the base profile means re-rendering every profile
file listed in the registry.

There are no generated root scripts (finding 10) and no per-launch setup as
root. The guest side has no installed copy to upgrade: the launcher travels
on the command line of every session (see "Guest launcher" under
"Milestone 1").

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
`user allow|deny`, `profile add|remove|list`, `sv2 run shell`,
`sv2 repair`, and the SSH lockdown, with v1's sandbox profile ported
unchanged.

| Crate | Binary | Runs as | Dependencies |
|---|---|---|---|
| `crates/common` | (library) | both | none |
| `crates/helper` | `sv2-helper` | root | `libc` |
| `crates/cli` | `sv2` | host user | `libc`, `serde`, `toml` |

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
  sandbox left in `/tmp` are never owned by a later sandbox. Reuse would go
  wrong in two ways: a later sandbox, perhaps another user's, could read
  what the earlier one left behind; and an earlier sandbox could leave files
  for a later one to trust, such as a poisoned cache.
  **Decided (2026-11-25):** `uninstall` therefore leaves the registry in
  place, holding no profiles and the `next-id` it reached, without comment; a
  later `install` carries on from it. IDs run from 600 to 2^31-1, the
  largest value that is positive as a signed 32-bit number (the limit was
  60000 at first, for no measured reason). There is no wraparound: if the
  range were used up, `profile add` would fail with a message that says
  why. Nothing reclaims IDs. An admin who accepts that old files may change hands
  starts again at 600 by deleting the registry after `sv2 uninstall`.
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
  directory; root never walks it. macOS refuses that rmdir (EPERM) while the
  account exists, and for a moment after its record is deleted: measured on
  2026-11-25, 3 of 8 removals were refused and each succeeded a second
  later. The helper retries a refused rmdir for up to 15 seconds. Why the
  refusal outlasts the record, and whether it is the same privacy
  permission as open item 10, is not known.
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
- **`sv2 repair` (implemented after the rest of the milestone).** It lives in
  `crates/cli/src/repair.rs` and does not use the helper.
  - *Ownership.* An entry owned by this profile's sandbox account, or by a
    UID with no account (a removed sandbox), becomes the host user's. This
    needs no root: the inherited ACL entry gives the host user the right to
    take ownership (measured). Set-user-ID and set-group-ID bits are cleared
    before and after the change. Afterwards the sandbox cannot set them: the
    kernel lets only a file's owner do that, whatever the ACL says
    (measured). The sandbox can still `chmod +x` such a file through its ACL
    entry. Entries of any other existing account are skipped. Other mode
    bits and the group are not changed.
  - *ACL.* Each file and directory gets the ACL the kernel would have given
    it by inheritance, replacing whatever ACL it had. That ACL is computed
    from the workspace directory's own, which the helper writes and the
    sandbox cannot change, so the rights are listed in one place
    (`ops::workspace_aces`). The workspace directory itself is not changed.
  - *When it runs.* `sv2 repair PATH` does both steps under PATH. `sv2`
    also runs it over the whole workspace after the helper returns from
    `profile-add` (both steps) and from `profile-remove` (ownership only: the
    ACLs then name an account that is gone and a UID that is never used
    again). The first-use creation of `default` counts as an add. A problem
    in the automatic pass is reported and does not fail the add or remove.
  - *PATH.* Checked as text first (inside `sv2/<caller>/<profile>`, no `..`),
    then opened one component at a time without following links. A link
    named by PATH or on the way to it is an error; links found during the
    walk are left alone.
  - *Limits.* An entry whose ACL the sandbox removed cannot be taken over or
    repaired from the host (it can be deleted); it is reported. The
    automatic pass visits every entry, so `profile add` on a very large kept
    workspace takes as long as that walk; this has not been measured.
- **SSH lockdown (implemented after the rest of the milestone).**
  - *Group.* `install` creates `sv2-sandboxes` (RealName
    `sandvault2 sandbox accounts`; a group of that name it did not create is
    refused). Its ID comes from the registry's allocator, so it is never an
    ID a removed sandbox had. `profile-add` puts the account in the group
    right after creating it and fails if the group is missing;
    `profile-remove` takes the account out before deleting it, because the
    group lists members by name and a later profile gets the same name.
    Sessions carry the group. It owns no file and appears in no ACL.
  - *Drop-in.* `/etc/ssh/sshd_config.d/000-sandvault2.conf`, root-owned,
    mode 0644. It holds a global `DenyGroups sv2-sandboxes` and a
    `Match Group sv2-sandboxes` block that turns off public-key, password,
    keyboard-interactive and host-based authentication, sets
    `AuthorizedKeysFile none`, turns off forwarding and the terminal, and
    forces `/usr/bin/false`. Measured against real logins with a private
    sshd (OpenSSH 10.5, macOS 26.7): a key that is accepted without the file
    is refused with it, refused with `DenyGroups` alone, and runs no command
    with `ForceCommand` alone. The name sorts first because the first `Match`
    block to set a keyword wins. The block ends where the file ends
    (measured, and a unit test), so macOS's `100-macos.conf` after it is
    still global.
  - *Validation.* The file is checked with `sshd -G -f` before it is renamed
    into place. `-G` needs no host keys; `sshd -t` fails on a Mac that never
    had Remote Login on, because it has none. `-G` is a recent option
    (measured on OpenSSH 10.5 only); on an sshd without it `install` fails
    with sshd's usage message. The
    temp file's name starts with a dot, which the `Include` glob skips. After
    the rename `install` runs `sshd -G` on the system configuration and fails
    unless it parses and lists the group under `DenyGroups`: a main
    configuration without the `Include` line, or one that was already
    broken, stops the install. The drop-in is left in place in that case.
  - *Launch.* `launch` refuses to start a session when the drop-in is
    missing or stale (see "Artifact versions" below).
  - *Upgrade.* `install` adds every `active` account in the registry to the
    group. (The application version went to 0.1.1 for this, which was the
    only way then to make an old install notice.)
  - *Uninstall* removes the drop-in and the group after the accounts.
  - *Limits.* The helper does not check that sshd sees an account as a
    member (the integration suite does, from the host side, with
    `sshd -T -C user=ACCOUNT`); nothing re-checks the drop-in after install
    except its version line at launch; and an admin's own `Match` block that
    sets `DenyGroups` replaces the global list for the logins it matches.
    `sv2 doctor` asks sshd about each of the caller's accounts.
- **Artifact versions (implemented after the rest of the milestone).** What
  `sv2 install` puts on a machine changes only when `sv2 install` is run
  again, with sudo. Whether that is needed no longer follows from the
  application version.
  - *Numbers.* `crates/common/src/lib.rs` has one for the helper itself
    (`HELPER_VERSION`) and one for each file the helper writes
    (`SUDOERS_VERSION`, `SSHD_DROPIN_VERSION`, `SANDBOX_PROFILE_VERSION`).
    Bumping any of them makes every installed machine ask for
    `sv2 install`; a release that bumps none needs no reinstall.
  - *Comparison.* `sv2` passes all four as one word
    (`helper=5,sudoers=2,sshd=1,profile=3`) where it used to pass the
    application version, and the helper refuses a word that is not its own,
    with exit status 3. `sv2 --version` prints the word on a second line.
    The numbers reach a machine only inside a new helper, because the
    helper holds the text of every file, so one comparison covers them all.
  - *In the files.* Each file carries its number in a comment line
    (`# sv2-sudoers-version: N`, `# sv2-sshd-version: N`,
    `;; sv2-profile-version: N`). Before `launch` and `profile-add` the
    helper reads the sudoers file and the drop-in back, and `launch` also
    reads the profile's sandbox file; a missing file, another number, or no
    number is refused with the command that fixes it (`sv2 install`, or
    `sv2 profile add NAME` for the sandbox file). This catches an install
    that replaced the helper and then stopped. `profile-remove` and
    `profile-list` do not check, so a stale machine can still be cleaned up.
  - *Forgetting to bump.* A unit test pins the text of each generated file
    to its number and fails when one changes without the other. Nothing
    does that for `HELPER_VERSION`: a change to the helper's code that
    installed machines need must be bumped by hand.
  - *Not covered.* The registry has its own format number in its header.
    The two groups and the directories have none; `install` checks and
    repairs them each time it runs.
- **Agent definitions (2026-11-26).** Built as designed under "Agents",
  in `crates/cli/src/agent.rs`, with one built-in, `sv2/agents/claude`.
  What the build decided or changed:
  - *Schema 1* has `binary`, `install.host.brew` (one `formula` or one
    `cask`; Claude Code is a cask), `install.guest.script`, and under `run`:
    `bypass_flag`, `no_inject`, `inject_last` (subcommands that take the
    flag after their arguments: pi), `env` (variables set for the agent:
    opencode is told by a variable what others are told by a flag) and
    `grants` (a few words on what the flag and variables allow, which the
    banner repeats; a unit test requires it of every built-in that has
    either). The last three came with the definitions that needed them,
    before any release, so the schema number did not change. `system_prompt` and `wrapper` from the design's
    example are not in it: nothing uses them until browser and iOS support
    exist. A key sv2 does not know is an error, so adding them is a schema
    change and not a silent difference between versions.
  - *What a definition may say* is checked when it is read: binaries start
    with `~/`, `{brew}/` or `/` and are plain paths; a package is a plain
    name, or `user/repository/name` from a tap (opencode); a variable is
    upper case and none of those sv2 sets or forwards; the flag is one long
    option; the script is a `.sh`
    file in the definition's directory, at most 64 KB.
  - *Who can write it.* Before a definition is read its resolved directory
    is refused if it is in a workspace or a sandbox home, or if any
    directory on the way to it, or any file in it, is writable by everyone
    (sticky or not, so `/tmp` and `/Users/Shared` are out), belongs to a
    sandbox account, is writable by a sandbox group, or has an access list
    that lets a sandbox account, a sandbox group or `everyone` write. A
    symbolic link in the directory is refused too. Sandbox accounts and
    groups are recognised by name, v2's (`sv2-`) and v1's (`sandvault-`):
    a v1 sandbox is as hostile as a v2 one. A checkout in a v1 workspace is
    therefore refused as a source of built-ins; the integration suite runs
    agents from a copy of the build in host-only scratch, and
    `scripts/sv2-dev-agents` copies the checkout's definitions into
    `~/.config/sandvault2/agents` for a development build to use.
  - *Where the agent is.* `sv2` cannot look into the sandbox home, so when
    no candidate outside it exists it asks the sandbox (the launcher's
    `probe`, one extra launch) before it decides to install. A sandbox that
    lies gets the package the definition names installed, or a session that
    finds no program.
  - *Install.* Without `--native`: `brew install --formula|--cask NAME`, as
    the host user, from `/`, shown before it runs, when the agent is
    nowhere. With `--native`: only candidates in the sandbox home count,
    and the launcher runs the definition's script when none exists.
  - *The flag.* `sv2` puts `bypass_flag` before the user's arguments unless
    the first is listed in `no_inject`; the banner says which happened.
    Claude Code 2.1.326 accepts its flag before every subcommand (checked
    against the real program), so its list is empty.
  - *The launcher* gets the agent as arguments: name, whether to install,
    the script's text, a counted list of paths, then the agent's arguments.
  - *Measured on 2026-11-26:* Homebrew's `claude` runs in a session;
    `--native claude` installed and ran 2.1.332 in a throwaway profile, and
    the next run without the option used it.
- **Quarantine (2026-11-26; open item 1).** Homebrew marks what a cask
  downloads with `com.apple.quarantine`, and Gatekeeper judges such a
  program when it starts. v1 ran each freshly installed program once as the
  host user to get past that.
  - *Measured* on macOS 26.7, in a session, with fresh copies of programs
    and the attribute set by hand to what Homebrew writes (`0181;...`):
    Claude Code, which Apple has notarized, starts, on its first run and
    with the attribute untouched afterwards. A program that is not
    notarized is killed as it starts (exit status 137; `syspolicyd` logs
    "Terminating process due to Gatekeeper rejection") unless the
    attribute's flags carry `0x40`, approved by the user, or the attribute
    is absent. The flags `0001`, `0081`, `0101`, `0181`, `0201` and `0281`
    were all killed; `0041` ran. Running the program on the host first did
    not help, and did not change the attribute.
  - *Decided.* sv2 does nothing for a notarized program, because nothing is
    needed. For one that is not, it does not run it on the host and does not
    remove the attribute: that is Gatekeeper's verdict on something
    downloaded, and overriding it is the user's decision. What sv2 does is
    say so at the one moment it knows the program is new, right after its
    own `brew install`: that macOS will stop the agent, why, and the
    `xattr -d` command that approves it.
  - *How it knows.* The attribute is read with `getxattr`; notarization
    with `codesign --verify -R=notarized`, which reads the signature and
    runs nothing (0.2 s for Claude Code's 200 MB, which is why it is not
    done on every launch).
  - *Limits.* A program that was already installed and is not approved
    gets no such message; it is killed with "Killed: 9" and nothing else.
    `sv2 doctor` checks every definition's program and says so. The
    rule is measured on one macOS version with Gatekeeper on.
- **Guest launcher (2026-11-25; most of open item 5).** What a session needs
  done before its program starts is done by `sv2/guest/launch.sh`, a bash
  script run as the sandbox user under the sandbox profile.
  - *How it gets there.* Its text is compiled into `sv2` and passed on the
    command line: `/bin/bash -c TEXT sv2-launch DIR KIND ARGS...`. There is
    no payload unpacked into the sandbox home and so no version stamp: every
    session runs the launcher of the `sv2` that started it, and one session
    cannot change it for the next. The design first had an unpacked payload;
    sudo passes only standard input, output and error, and standard input
    has to stay the terminal. The cost: sudo logs the command line, so the
    script (under 4 KB; a unit test caps it at 16) is in the log once per
    session. Nothing secret may ever go this way (§2).
  - *What it does.* Sets `PATH` (the account's `~/.local/bin` and `~/bin`,
    then Homebrew, then the system), writes the startup files below, changes
    to DIR, and starts the program: for KIND `shell` a login zsh, or ARGS as
    a command in a login zsh's environment. ARGS stay positional parameters
    from `sv2` to the program and are never parsed as shell text.
  - *Startup files.* A login zsh runs `/etc/zprofile`, whose `path_helper`
    puts the system directories first again, so the order has to be restored
    from a file zsh reads later. The launcher writes `~/.zprofile` (Homebrew
    and the bin directories) and an empty `~/.zshrc`, each only when nothing
    has that name. From then on the file is the user's: `sv2` never rewrites
    it, so a later change to the text does not reach an existing home.
- **Holding the lock (2026-11-25).** The registry lock is shared by every
  user, and the caller controls a good deal about the helper that holds it.
  Three things keep one caller from making everyone else wait:
  - *Output.* Nothing is written to standard output while a verb runs;
    lines are queued and written after the lock is released. The caller
    chooses what standard output is, and a pipe that is already full blocks
    the writer for as long as the caller likes. Measured against the helper
    before this: `profile-list` into a full pipe blocked with the lock held,
    and another command waited behind it. The integration suite plants this.
  - *Suspension.* The helper ignores the terminal's stop signals (`^Z`, and
    background reads and writes). The launch restores them just before the
    session starts, or `^Z` would do nothing inside it. SIGSTOP cannot be
    ignored, but only root can send it to the helper.
  - *Waiting.* A verb waits at most 30 seconds for the lock, says after 3
    what it is waiting for, and then fails naming the holder (its PID, user
    and verb, which an exclusive holder writes into the lock file). This is
    also what a caller sees while another user's `profile remove` sits
    behind the privacy prompt of open item 10.
- **Killing an account's processes (2026-11-25).** `profile remove` signals
  every process of the account from a child that has become the account
  (`kill(-1, ...)`: the kernel does the matching).
  - A pass of SIGKILL reaches what exists when it starts. Measured: against
    400 processes that fork and exit without pause, 20 passes a tenth of a
    second apart left 391 and the removal failed; against 80 it succeeded.
  - So the child sends SIGSTOP first, 16 times, and SIGKILL after. A stopped
    process cannot fork and frees no process slot. Measured: one pass of
    SIGSTOP stopped all of a storm of 73.
  - On macOS `kill(-1, sig)` signals the caller too (measured). The child is
    stopped by its own SIGSTOP; the helper, as root, continues it, a bounded
    number of times, so an account that stops the child itself cannot keep
    the helper waiting.
  - The helper repeats the whole thing until no process is left, for at
    most 10 seconds; then the removal fails and can be run again.
- **A taken workspace root (2026-11-25).** `/Users/Shared` is writable by
  everyone, so another user can create `sv2` there first. `install` replaces
  what holds nobody's data, an empty directory or a symbolic link, and
  refuses anything else with the owner's name and the command that moves it
  away. It never opens or follows what it finds. A user who keeps putting a
  directory with a file in it there can still block `install`; the message
  names them.
- **Repository credentials (2026-11-26).** Built as decided under
  "Repository credentials", in `crates/cli/src/credentials.rs`.
  - *`sv2 deploy-key OWNER/REPO [--write]`.* The sandbox user makes an
    ed25519 key in its home (`guest/deploy-key.sh`, passed on the command
    line like the launcher) and prints the public key. `sv2` accepts that
    answer only if it is exactly one ed25519 public key on one line, drops
    its comment, and writes its own copy to
    `~/.local/state/sandvault2/deploy-keys/PROFILE/`. It then prints
    `gh repo deploy-key add FILE --repo OWNER/REPO --title ...`, with
    `--allow-write` only when `--write` was given, for the user to run. The
    repository is the one the user typed; the file is one the sandbox
    cannot write (§1 rule 6). `sv2` adds nothing to GitHub itself.
  - *In the sandbox,* everything is in `~/.ssh/sv2-deploy-keys/`: the key
    pairs, GitHub's ed25519 host key, pinned (an agent cannot answer ssh's
    question, and should not accept whatever answers), and an `ssh` that
    chooses the key (`guest/git-ssh.sh`). git tells ssh the exact
    repository; that program offers the repository's own key and no other,
    and is plain ssh for everything else. The guest launcher sets
    `GIT_SSH_COMMAND` to it when it exists. Nothing is written to
    `~/.gitconfig` or `~/.ssh/config`, which are the user's and which a
    later feature will copy in from the host. `docs/DEPLOY-KEYS.md`
    describes all of it for a user.
  - *Dropped on the way:* `url.insteadOf` rules to an ssh alias. They match
    the start of an address, so a key for `acme/api` caught `acme/api-docs`
    too, and they lived in the two files above.
  - *`sv2 gh-token [login|status|logout]`* is v1's `sv-gh-token`: it opens
    GitHub's fine-grained token form filled in with read-only permissions,
    reads the token without echo (or from a pipe), refuses classic and app
    tokens, and hands it to the sandbox's `gh auth login --with-token` on
    standard input, never on a command line.
  - `profile remove` forgets the host's copies of that profile's keys.
  - *Measured:* with a key that GitHub does not know, in a session:
    git reached GitHub through the key ("using the deploy key", then
    "Permission denied (publickey)"), and a public repository of the same
    owner still cloned over `https://`.
  - *Not run:* a key actually added to a repository and used to clone and
    push; a real token through `gh-token login`.
- **Doctor (2026-11-26).** `sv2 doctor` looks and reports; it changes
  nothing, asks for no password, and runs as the host user
  (`crates/cli/src/doctor.rs`). Each line is `ok`, `note` or `PROBLEM`, a
  problem says what to run, and the exit status is 1 when there is one.
  - *The installation:* the helper is there, answers, and matches this
    `sv2`; the caller is in `sv2-users`.
  - *The caller's profiles:* one left `adding` or `removing` is a problem,
    with the command that finishes it; an active one whose account is gone
    is too. Other users' profiles are theirs: the helper lists only the
    caller's.
  - *sshd:* the drop-in is there, and for each of the caller's accounts
    `sshd -T -C user=ACCOUNT` (with a throwaway host key, as the host user)
    reports all four refusals. This is the check the SSH lockdown's limits
    asked for: it catches a main configuration without the `Include`, and
    an administrator's own `Match` block that replaces `DenyGroups`.
  - *Agents:* every definition is loaded as it would be to run, so one
    that is refused or does not parse is a problem; a name that is also a
    command gets a note; an installed program that is quarantined and not
    notarized is a problem, with the `xattr` command (see "Quarantine");
    one that is not installed gets a note.
  - *Not looked at:* whether a session actually starts; the sandbox
    profile files; other users.
- **Default deny (2026-11-26; finding 11).** The base profile starts with
  `(deny default)`. Profile version 2; `sv2 install` re-renders every
  profile.
  - *Probes.* `sv2/tests/profile-probes.sh` is what a session must be able
    to do (about 40 things: files, processes, terminals, the network, git,
    a C compile, python, node, keychains, a file watcher, tmux, claude) and
    must not (other homes, keychains, raw disks, the pasteboard,
    AppleScript, opening an application, DiskArbitration, launchd jobs,
    the screen). Each allow in `base.sb` names the probe that fails
    without it. The probes run in a real session in the integration suite,
    and under `sandbox-exec` as the current user in a unit test of the
    helper (`profile_probes`), which needs no root and is how the profile
    is worked on.
  - *What is allowed*, each measured by taking it out: reading files,
    the network, signals, pseudo-terminals, ioctl on `/dev`, POSIX shared
    memory and semaphores, `sysctl` reads, starting programs, `/bin/ps`
    outside the sandbox, the writable places as before, and five Mach
    names: `com.apple.system.` (user and group names),
    `TrustEvaluationAgent` (TLS for curl, git, python), `trustd.agent`
    (TLS for Go programs), `SecurityServer` (keychains), `FSEvents` (file
    watchers).
  - *Removed*, because no probe failed without them: the two Mach names
    the review called unmeasured, `com.apple.cfprefsd` (`defaults` works
    from the file) and `com.apple.ocspd`; `com.apple.dnssd`,
    `com.apple.mDNSResponder`, `com.apple.networkd` and
    `com.apple.SystemConfiguration` (names resolve and connections are
    made without them); `process-info*`; the denies that default deny
    made redundant (all writes, mounting); and what only the full browser
    applications need (`com.apple.coreservices.launchservicesd`, the Chrome
    and Crashpad prefixes), which come back with browser support and a
    probe of their own.
  - *A consequence:* a session can no longer push keystrokes into a
    terminal's input queue at all. The kernel asks the sandbox for
    `hid-control` before `TIOCSTI`, and nothing allows it (seen in the
    first installed run: the ioctl fails with "Operation not permitted").
    The session's own pty is still there as the second defence.
  - *Headless Chromium (2026-11-26, profile version 3).* v1 gained two
    Mach name prefixes on `main` (285dd0e) so that Playwright's Chromium
    builds start. Under default deny the same builds need more: the
    browser also has to register its rendezvous name, which
    `(allow default)` had let through unseen. Measured with Playwright's
    `chromium_headless_shell-1255`, started with `--no-sandbox` as
    Playwright starts it: with lookup and registration of
    `org.chromium.Chromium.MachPortRendezvousServer.` a page loads, 6 times
    of 6; without it the browser stops at `bootstrap_check_in`. That is the
    one rule added, with a probe that the unit test runs when this user
    has that build. The full applications, Google Chrome and Chrome for
    Testing, are not allowed for yet: with their own rendezvous prefixes,
    `org.chromium.crashpad.`, `com.apple.coreservices.launchservicesd` and
    the IOKit clients `IOSurfaceRootUserClient` and `AGXDeviceUserClient`
    they loaded a page only about half the time. That is for browser
    support to finish, measured in a real session.
  - *Not narrowed, by decision:* file reads are still everything but
    `/Users`, `/Volumes`, `/Library/Keychains` and raw disks, and the
    network is still everything, localhost and port 22 included. Both
    were deferred, with per-profile capabilities (open item 8).
  - *How it was measured.* As the current user under `sandbox-exec`, in
    the host user's launchd domain, with a scratch home. The probes
    themselves were run in a real session under the old profile and all
    passed there. The new profile in a real session waits for
    `sv2 install`.
- **Uninstall and profiles (2026-11-26).** `sv2 uninstall` used to remove
  every profile of every user, with the sandbox homes and what the agents
  kept in them (logins, native installs), behind nothing but the sudo
  password. It now refuses while any profile exists, in whatever state,
  and lists them with their owners. They go by each user's own
  `profile remove`, or all at once with `sv2 uninstall --all-profiles`,
  which is not shortened. The helper decides (it alone can list every
  user's profiles), so the password is asked for before the refusal is
  shown.
- **Installing from a path (2026-11-25): not a gap.** `install` copies the
  helper to its root-owned place by opening its own path again, so whoever
  can write the directory it was run from (the Homebrew prefix, a checkout)
  could swap the file between sudo's exec and the copy, and have a helper
  of their choosing installed. This is not defended against. The same
  person could replace the file before sudo runs it, with no race to win,
  and be root at once with the password just typed: the attack starts on
  the far side of the door it claims to open (Raymond Chen's "other side of
  this airtight hatchway"). What does help is outside this code: install
  from a place only a trusted admin can write, and review a helper build
  first (see "Development").
- **Allowed-users group.** It is `sv2-users`; `install` refuses a group of
  that name it did not create (it checks the group's RealName). It was
  `sandvault-users` at first, which is also what v1 names the sandbox group
  of a host user called `users`. The rename bumped the helper and sudoers
  versions to 2 but has no upgrade path: the one machine that had sv2 was
  uninstalled first.

## Remaining work

What milestone 1 left undone, untested, or knowingly imperfect. Remove an item
when it is fixed. Design questions still to be answered are under "Open
items" below.

### Priority order

**Decided 2026-11-25, brought up to date 2026-11-26.** The sections below
group items by kind; this is the order to do them in. (This section was
dropped from the document by mistake in commit 9a52da3 and put back.)

1. Done: the helper's group rename, `next-id` kept across `uninstall`, and
   the helper's security gaps that do not concern launchd.
2. Done: the guest-side launcher, agent definitions (five built-ins),
   quarantine handling, and `sv2 agent new`.
3. Done: the safer `uninstall`, default deny, `sv2 doctor`, repository
   credentials, and the verification that needed no push.
4. Next, in this order: browser support (Playwright's headless Chromium
   already runs; the full applications do not); the iOS bridge; Homebrew
   packaging.
5. Deferred until asked for: the planted `~/Library/LaunchAgents`
   measurement (open item 7); per-profile capabilities (open item 8); a
   position on localhost and port 22; CI on a runner.
6. Last, the daemon: the measurement in open item 3, the move of account
   changes into it, and with it what `sv2` does about the "administer your
   computer" permission (open item 10).

### Not yet verified

`sv2/tests/root-checks` makes the checks that need an administrator:
uninstall with and without `--all-profiles`, install over a name somebody
else took, and account IDs across a reinstall. It deletes every profile on
the Mac, so it is run by hand: `sv2/tests/root-checks sv2/target/release
--delete-every-profile`. It was run on the development Mac on 2026-11-26
and passed (33 of 33), all three "taken name" cases included. That run
still had a section for very large account IDs, since removed from the
script: an account with ID 2147483000 was created, entered (in its own
launchd domain) and removed, and the integration suite passed with IDs
there (137 of 137).

- [ ] **CI steps are unverified.** `.github/workflows/ci.yml` has "Install
  sandvault2" and "SandVault2 Tests"; neither has run on a runner, because
  the branch has not been pushed. The first also turns Remote Login on
  (`launchctl load -w .../ssh.plist`) so that the suite's SSH login attempt
  runs; whether that works on a runner is unknown. Read against the code on
  2026-11-26: the step now installs `shellcheck` and `tmux`, which the
  tests need and fail without under `SV_TESTS_FAIL_ON_SKIP`.
- [ ] **The SSH lockdown is partly verified.** `sv2 install` with it, the
  integration suite, and `sv2 uninstall` (root-checks: the drop-in and the
  group go, and sshd's configuration still parses) have run. Still
  unverified: whether adding to or removing from the group raises the
  "administer your computer" prompt (open item 10); and, on a Mac where
  Remote Login is open to all users, that the suite's login attempt
  succeeds against a build without the drop-in. On the development Mac
  Remote Login is limited to listed users, so that attempt is refused
  either way.
- [ ] **Parts of the kill and workspace-root changes are untested.** The
  integration suite covers the full pipe, the fork storm, stop signals in
  a session, and (since 2026-11-26) `^Z` typed at a helper that holds the
  lock, with a control that the same keystrokes suspend a session. Not
  covered: an account that stops the kill child on purpose (the path that
  continues it runs on every removal, because on macOS the child stops
  itself); `install` over a `sv2` directory that really belongs to
  another user (root-checks makes the directory as the user who runs it,
  which exercises the same code). Seen while writing the `^Z` check and not pursued: `^Z` typed
  while sudo is still starting, before the helper runs, can leave that one
  sudo waiting for good. Nothing is locked at that point.
- [ ] **The integration suite fails now and then for a known reason.** In
  9 runs in a row on 2026-11-26, one failed: `dscl . -delete` of an account
  waited on the "administer your computer" permission (open item 10) for
  more than a minute, with the helper holding the lock, so that removal
  and the next one failed. Which removal meets it is chance. The `^Z`
  check's watchdog prints the processes when it happens and says so.
- [ ] **Parts of agent definitions have not run.** Unit tests, the
  integration suite (126 of 126, 2026-11-26, with a test agent) and the two
  measurements with `claude` cover the rest. Not run: the Homebrew install
  itself (`brew install` from `sv2`), since `claude` was already installed;
  a Mac without Homebrew or with a prefix the user cannot write; a user
  definition in `~/.config/sandvault2/agents` on a real machine (unit tests
  cover the precedence); built-ins found in an installed `share/` directory
  under a real Homebrew prefix, which does not exist until sv2 is packaged.
- [ ] **The quarantine message has not been seen for real.** The rule
  behind it was measured by hand and is unit-tested, also against real
  files and the real `codesign`. Not run: `sv2` installing a cask that is
  not notarized, which is the only way the message appears.
- [ ] **Profile version 3 has not run in a session.** The probes pass under
  it as the current user, the headless browser included. In a real session
  that probe runs only if Playwright's browsers are installed in the
  sandbox home (`npx playwright install chromium` in a session).
- [ ] **`use_pty` has not been tested against its absence.** The terminal
  checks passed before the `use_pty` line existed, because this machine's sudo
  (1.9.17) defaults to it. Nothing has run on a sudo older than 1.9.14, or
  with `Defaults !use_pty` set elsewhere, to show the line is what holds.
- [ ] **The workspace ACL checks have not been mutation-checked.** The
  integration cases for the workspace directory (sandbox cannot chmod, chown,
  re-ACL, delete or rename it) and for inheritance (no execute bit, host
  access to sandbox-private entries) pass, but no build with a weakened ACL
  has been installed to confirm they fail.

- [ ] **Account changes need a macOS privacy permission.** Changing an
  existing account's home directory, and deleting an account, make
  `opendirectoryd` ask TCC for `kTCCServiceSystemPolicySysAdminFiles`
  ("would like to administer your computer"). The permission belongs to the
  app the command was started from (the terminal), not to `sv2`, the helper
  or root. While the prompt waits, the `dscl` call hangs (about 290 seconds,
  then `eUndefinedError`); if the answer is no, it fails at once with
  `eDSPermissionError`. This affects `profile remove` and `uninstall`. A
  repeated `profile add` no longer asks: the helper does not write an
  attribute that already has the wanted value. The helper stays consistent (the verb can
  be run again), but it holds the registry lock for the whole wait, has no
  timeout of its own on `dscl`, and passes the raw `dscl` error to the user
  with no hint about the cause. See open item 10.

### Security gaps

- [ ] **launchd as a route around the profile** (open item 7). The direct
  routes are refused (measured, not yet a test). Still open: a plist the
  sandbox plants in `~/Library/LaunchAgents` may load when the account's
  launchd domain is next created, and would run unconfined as the sandbox
  user.

### Functional gaps

None recorded.

### Features not in milestone 1

- [ ] remove "--all-profiles" uninstall as a capability; too dangerous
- [ ] the path search list for each agent includes both native and homebrew paths. That creates a problem where the binary could be out-of-date because the wrong one is being executed. Create separate entries for native vs. homebrew.
- [ ] Shorten usage: doesn't need to be exhaustive (e.g. remove "refuses while..."), no newlines for agents, repair PATH shorter help, single-line help strings for commands, shorter explanation of shortening
- [x] `sv2 doctor`; see "Doctor" under "Milestone 1".
- [x] The guest-side launcher (open item 5); see "Guest launcher" under
  "Milestone 1". There is no unpacked payload.
- [x] Agent definitions: format, `install.sh`, Homebrew and native install,
  `--native`; see "Agent definitions" under "Milestone 1". Built-ins:
  `claude`, `codex`, `gemini`, `opencode`, `pi`. Not yet: `muse` and
  `herdr` from v1; `system_prompt` and `wrapper`.
- [x] `sv2 agent new NAME [--from AGENT]` creates a definition in
  `~/.config/sandvault2/agents`: from a template that shows every key, the
  optional ones commented out, or as a copy of an existing definition (a
  built-in, to change it). It never replaces a definition that is there.
- [x] Quarantine handling for installed agent binaries (open item 1); see
  "Quarantine" under "Milestone 1".
- [x] Default deny; see "Default deny" under "Milestone 1".
- [ ] Per-profile capabilities (open item 8), a position on localhost and
  port 22, and a narrower rule for reading files. All deferred on
  2026-11-26.
- [x] Repository credentials: `sv2 deploy-key` and `sv2 gh-token`; see
  "Repository credentials" under "Milestone 1".
- [ ] Browser support. Playwright's headless Chromium already runs in a
  session; Google Chrome and Chrome for Testing do not (see "Default
  deny" under "Milestone 1" for what was measured).
- [ ] iOS bridge, and the simulator trust question (open item 2).
- [ ] Homebrew packaging of `sv2` itself, and upgrade handling when `sv2` and
  the installed helper differ in version.
- [ ] Move from the sudoers helper to a launchd daemon (open item 3). Done
  last: see "Priority order".

## Open items

1. **Quarantine.** Decided on 2026-11-26; see "Quarantine" under
   "Milestone 1". sv2 neither runs a fresh program on the host, as v1 did,
   nor removes the attribute.
2. **Simulator trust.** Confirm from the host side whether simulator apps run
   unconfined as the host user, then document what `--ios` grants.
3. **Daemon follow-up.** Scope and timing of the move from the sudoers helper
   to a launchd daemon.

   **Noted 2026-11-25; scheduled last, see "Priority order":** the daemon is the likely fix for the
   "administer your computer" prompt (open item 10). Under sudo, macOS
   charges the helper's account changes to the terminal app the user ran
   `sv2` from, prompts for it again every few minutes, and blocks the helper
   for about 290 seconds when nobody answers. A daemon started by launchd has
   no such app behind it. Before building it, measure with a throwaway
   LaunchDaemon that deletes a test account: is the request granted, refused,
   or still shown to someone? If it is granted, move at least account
   creation and deletion into the daemon; `launch` can stay on sudo if that
   keeps the session's terminal handling simple.
4. **Terminal.** Decided in milestone 1; see "Milestone 1".
5. **Guest-side launcher.** Decided: a script compiled into `sv2` and
   passed on the command line, which gets an agent's data and install
   script as arguments; see "Guest launcher" and "Agent definitions" under
   "Milestone 1". It was not to be the helper after it drops privileges:
   that would put definition parsing into the binary that runs as root.
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
9. **What `sv2 repair` should do.** Partly decided on 2026-11-25: it runs
   automatically after `profile add` and `profile remove`, and it makes the
   host user the owner (see "Workspace permissions" and "`sv2 repair`" under
   "Milestone 1"). Still to go over:
   - whether it replaces an entry's ACL, as built, or adds to it;
   - whether it should also fix mode bits (a kept file of mode 000 stays
     that way) or the group;
   - whether PATH is required, or `sv2 repair` alone means a whole profile;
   - what to do with entries it cannot take over because the sandbox removed
     their ACL: leave and report them, as built, or have the sandbox account
     put the ACL back before the account is deleted;
   - the cost of the automatic pass on a very large workspace, and whether
     `profile add` of a profile that is already active should skip it.
10. **The "administer your computer" permission.** `profile remove`, a
    repeated `profile add` and `uninstall` depend on a TCC permission held by
    the user's terminal app (see "Account changes need a macOS privacy
    permission" under "Remaining work"). Decide what sv2 does about it.

    What was observed on 2026-11-25, macOS 26.7, with `sv2` run from a
    terminal app (cmux):
    - Until 19:25 every request for `kTCCServiceSystemPolicySysAdminFiles`
      was granted without a prompt. `tccd` checks
      `kTCCServiceSystemPolicyAllFiles` (Full Disk Access) first.
    - At 19:26:14 `tccd` logged `AUTHREQ_PROMPTING` for the terminal app.
      With the prompt unanswered, `ODRecordSetValue` (re-setting
      `NFSHomeDirectory`) ran 283 s and `ODRecordDelete` 294 s before
      `opendirectoryd` forced `ODErrorPluginOperationTimeout`; `dscl` printed
      `DS Error: -14987 (eUndefinedError)` and exited with status 139. A
      later delete went through, and runs passed again until 20:02.
    - At 20:02 the same requests were answered at once with `authValue=0,
      authReason=2` (denied, set by the user); `dscl` printed
      `DS Error: -14120 (eDSPermissionError)` and exited with status 40.
    - `opendirectoryd` did not restart. Creating an account, and reads, were
      never affected.

    To find out and decide:
    - why requests were granted silently before 19:26 and prompted after;
    - exactly which operations ask: the home directory attribute and record
      deletion were seen; check `IsHidden`, group membership and the rest;
    - done: the helper no longer re-sets an attribute that already has the
      right value, which spares a repeated `profile add`. Deleting an account
      still asks, and that cannot be skipped;
    - whether a launchd daemon is asked at all, since it has no responsible
      app. This is now the main reason to do open item 3;
    - what the user is told: detect `eDSPermissionError` and the long wait,
      name the permission and where to grant it, and document it in the
      README. v1 creates and deletes accounts the same way, so check whether
      it has the same dependency;
    - a timeout on `dscl` shorter than the daemon's, and whether to release
      the registry lock while waiting.
