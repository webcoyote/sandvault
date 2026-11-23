# Sandvault profiles

**Status:** Implemented
**Date:** 2026-11-22
**Issue:** #328

## Motivation

Sandvault gives each host user exactly one sandbox account,
`sandvault-$USER`. Users who keep separate work and personal identities want
agents in one area to have no access to the other's credentials: agent
subscription logins (Claude stores its OAuth login per macOS user, so
`CLAUDE_CONFIG_DIR` does not separate them), GitHub tokens and deploy keys,
and repositories.

The existing user boundary already provides this isolation; it is just
limited to one sandbox per host user. Profiles lift that limit.

## Goal

`sv --profile work cl` runs the agent as a separate macOS user,
`sandvault-$USER-work`, with its own home, Keychain, shared workspace
(`/Users/Shared/sv-$USER-work`), SSH keys and deploy keys. Profiles cannot
read each other's files. With no profile selected, behavior and every
name are exactly as today — no migration.

## Non-goals

- Per-repository sandboxes (this mechanism could support them later).
- Copying settings or state between profiles.
- Multiple agent logins inside a single sandbox account.

## Design

### 1. Selecting a profile

- `--profile NAME` flag, else `SV_PROFILE` environment variable, else the
  default (no profile). The flag wins over the environment.
- `NAME` must match `^[a-z0-9]{1,16}$`. This keeps derived names
  unambiguous and short.
- The derived account name `sandvault-$USER-$PROFILE` must also be at most
  64 characters; `sv` rejects the profile otherwise. macOS itself allows
  255-byte login names (`MAXLOGNAME`), so the real limit is Unix socket
  paths, which macOS caps at 103 bytes (`sun_path[104]`). The longest
  socket sandvault knows of is herdr's
  `/Users/<account>/.config/herdr/herdr-client.sock`, 39 bytes plus the
  account name, so 64 leaves exactly 103. With a 16-character profile this
  allows host user names up to 37 characters.
- Parsed before the resource constants are defined, because nearly all of
  them depend on it.

### 2. Derived names

One suffix, `SV_SUFFIX` (`""` for the default, `"-$PROFILE"` otherwise),
feeds every per-sandbox name:

| Constant | Default (unchanged) | Profile `work` |
|---|---|---|
| `SANDVAULT_USER`, `SANDVAULT_GROUP` | `sandvault-$USER` | `sandvault-$USER-work` |
| `SHARED_WORKSPACE` | `/Users/Shared/sv-$USER` | `/Users/Shared/sv-$USER-work` |
| `SUDOERS_FILE`, `SUDOERS_BUILD_HOME_SCRIPT_NAME`, `SANDBOX_PROFILE` | derived from `SANDVAULT_USER` | follow automatically |
| `SSH_KEYFILE_PRIV` / `_PUB` | `~/.ssh/id_ed25519_sandvault` | `~/.ssh/id_ed25519_sandvault-work` |
| `AUTHORIZED_KEYS_DIR` | `$INSTALL_PRODUCT/authorized_keys.d/` | `$INSTALL_PRODUCT/authorized_keys.d/work/` |
| `GUEST_AUTHORIZED_KEYS` | `$INSTALL_PRODUCT/authorized_keys` | `$INSTALL_PRODUCT/authorized_keys-work` |
| `INSTALL_MARKER` | `$INSTALL_PRODUCT/install` | `$INSTALL_PRODUCT/install-work` |
| `SESSION_DIR` | `~/.local/state/sandvault` | `~/.local/state/sandvault/work` |

Each profile has its own `authorized_keys.d`, so an extra key granted to one
profile does not get into the others. The default profile's
`authorized_keys.d/` will now contain profile subdirectories, so the
authorized-keys builder must skip directories instead of rejecting them as
invalid keys.

### 3. Host-user discovery inside the sandbox

Inside a sandbox, `$USER` is `sandvault-pat-work`. That name cannot be split
reliably: it could be host user `pat` with profile `work`, or host user
`pat-work` with no profile (`-`, `.` and `_` are all valid in macOS short
names).

`sv` passes the identity in explicitly. It adds `SV_HOST_USER=$HOST_USER` and
`SV_PROFILE=$PROFILE` to `EXTRA_ENV`, which already reaches the sandbox on
both the ssh and sudo launch paths, alongside `SV_SESSION_ID`.

`sv`, `sv-clone`, `sv-agentsview-setup` and `scripts/tests` resolve the
identity like this:

1. If `SV_HOST_USER` is set, use it together with `SV_PROFILE`.
2. Otherwise, fall back to today's logic (strip the `sandvault-` prefix,
   no profile).

**Trust:** the sandbox can change its own environment, but these values are
only used to build paths. A forged value points at paths that the sandbox
profile and Unix permissions already deny. It can only break the forging
session, the same as the current prefix-stripping logic.

**Known gap:** a profile sandbox entered without going through `sv` (a
direct `ssh` with a key from `authorized_keys.d`, a manual `sudo -u`)
lacks the variables and falls back to the wrong host user. This is
documented rather than handled; only users who add their own keys can hit
it. If it turns out to matter, `sv` can also write the identity into the
guest home at build time.

### 4. Ownership: discovery and collision guard (host side)

The account directory already records ownership: at build, `sv` adds
`$HOST_USER` to the sandbox's group (`dseditgroup -a "$HOST_USER"`), and
only root can change group membership. No new record file is needed.

**Owned sandboxes** of `$HOST_USER` are the accounts named
`sandvault-$HOST_USER` or `sandvault-$HOST_USER-*` whose group of the same
name lists `$HOST_USER` as a member. Once the owner is known, the profile
name is the account name with the `sandvault-$HOST_USER-` prefix removed,
which is unambiguous.

**Collision guard:** before building `sandvault-X`, if that account already
exists and its group does not list `$HOST_USER`, abort. This covers both
directions: `pat` building profile `work` cannot take over the default
sandbox of host user `pat-work`, and vice versa. Existing installs already
have the membership, so nothing needs migrating.

**Accepted limitation:** an admin who manually adds `pat` to another user's
sandvault group makes that sandbox look like `pat`'s.

### 5. Commands

- `sv --profile work <agent|shell|build>`: as today, against the profile's
  sandbox.
- `sv profiles list`: lists the owned sandboxes (section 4), marking the
  default.
- `sv --profile work uninstall`: removes only that profile.
- `sv uninstall`: removes the default sandbox, then lists any profiles that
  remain.
- `sv uninstall --all`: removes every owned sandbox, then sweeps orphaned
  per-profile host files (keys, `authorized_keys.d/<p>/`, markers, session
  dirs) found by name in the host user's own config and state directories.

Uninstall keeps the existing rule of leaving a shared workspace in place if
it is not empty. The current teardown (the user-session kill and
`uninstall`) is refactored to run once per profile.

### 6. Other scripts and docs

- `scripts/kill-sandvault`: accepts `--profile`.
- `sv-clone`: passes the profile through to `sv`.
- The README, the `/sv` skill docs (`skills/sandvault/sv/SKILL.md`) and the
  CHANGELOG are updated.

## Implementation steps

Each step leaves `./scripts/validate` passing.

1. Add a shared identity helper (env var, else prefix-strip fallback) and
   switch all four scripts to it. No behavior change.
2. Parse `--profile` / `SV_PROFILE`, validate the name, and add the suffix
   to the constants. Pass `SV_HOST_USER` / `SV_PROFILE` through
   `EXTRA_ENV`.
3. Add the ownership check, the collision guard and `sv profiles list`.
4. Add per-profile SSH keys and `authorized_keys.d`, including the
   directory-skip fix.
5. Make uninstall per-profile, then add `--all` with the orphan sweep.
6. Update `kill-sandvault`, `sv-clone`, the docs and the CHANGELOG.

## Tests

Each test protects a specific guarantee:

- **Default names are unchanged.** Every derived constant matches today's
  value when no profile is set, so existing installs keep working.
- **Invalid profile names are rejected** (`Work`, `a-b`, `../x`, 17
  characters). Each would produce an ambiguous or unsafe account name or
  path.
- **Flag beats environment.** `--profile a` with `SV_PROFILE=b` selects
  `a`, so a stale environment variable cannot silently redirect a run.
- **Identity resolution.** With `SV_HOST_USER=pat SV_PROFILE=work`, the
  scripts resolve to `pat`'s `work` paths, not to host user `pat-work`.
  Without the variables, they fall back to today's result.
- **Ownership filter.** A `sandvault-pat-x` whose group lists `pat-x` (not
  `pat`) is excluded from `pat`'s `profiles list` and `uninstall --all`.
- **Collision guard.** Building over an existing account that `$HOST_USER`
  does not own aborts without modifying that account.
- **Cross-profile isolation.** Inside the `work` sandbox, reading
  `/Users/Shared/sv-$USER` and `/Users/sandvault-$USER` fails. This is the
  property users asked for.
- **`uninstall --all`.** Removes every owned profile, keeps non-empty
  workspaces, and leaves other users' sandboxes untouched.

Tests that create accounts, sudoers entries or ACLs need sudo on the host
and cannot run from inside a sandvault session.
