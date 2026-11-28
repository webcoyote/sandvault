# Blocking sandbox → host SSH

Goal: the host user can connect to the sandbox account, and the sandbox user
cannot connect to this machine. Host → sandbox keeps working exactly as it
does today; sandbox → anything-on-this-Mac over SSH stops.

Status: proposal. Nothing here is implemented. It follows from items 8 and 9
of `docs/security-review-2026-11-24.md` and is tracked as TASK-0003.

## Why this matters

Two different logins are in play, and they fail differently.

| Login | What it would give the sandbox | State today |
|---|---|---|
| Sandbox → **host account** | Everything the sandbox exists to protect | No route found in current code: it needs a key in the host's `~/.ssh/authorized_keys` or the host's password. Was possible before 8551ded. |
| Sandbox → **its own account** | A session outside `sandbox-exec`: no profile, so the `/Users`, `/Volumes`, and Keychains read denies and the mach-lookup restrictions no longer apply | Open when Remote Login is on. The account's `authorized_keys` is in its own writable home, and `sv` adds the account to `com.apple.access_ssh` on every rebuild. |

The second row is the practical problem. The first is closed, but any future
bug that lets the sandbox write one line into the host's `authorized_keys`
turns into a full login, so reducing reachability is worth doing as well.

## Constraint: sshd cannot see who is connecting

sshd knows the account being logged into, the credential offered, and the
source address. It does not know which local user opened the socket. So
"block the sandbox user from connecting" has to be expressed as one of:

- what the **target account** will accept (sshd configuration), or
- which **uid may open the connection** (packet filter), or
- what a **sandboxed process** may connect to (sandbox profile).

The layers below use each of these. They are listed in the order to
implement them.

## Layer 1: lock down what the sandbox account accepts

A root-owned sshd drop-in, for example
`/etc/ssh/sshd_config.d/100-sandvault.conf`:

```
Match User sandvault-*
    AuthorizedKeysFile /var/sandvault/authorized_keys-%u
    PasswordAuthentication no
    KbdInteractiveAuthentication no
    AllowAgentForwarding no
    AllowTcpForwarding no
    X11Forwarding no
    PermitTTY yes
```

What each part does:

- **`AuthorizedKeysFile` outside the sandbox home.** Today the file is
  `/Users/sandvault-*/.ssh/authorized_keys`, which the sandbox user can
  edit. Under `/var/sandvault` (root-owned, like the sandbox profile and the
  build-home script) only keys the host installed are accepted: the host's
  `id_ed25519_sandvault` and whatever is in `authorized_keys.d`.
- **No password or keyboard-interactive logins.** The account password stops
  being an SSH credential, which removes the SSH consequence of the
  password-in-argv issue (`sv:1527`).
- **No forwarding.** A sandbox session cannot use the connection to reach
  the host's agent or open tunnels. This complements the client-side flags
  in item 8 of the review.

Optional, stricter:

```
    ForceCommand /var/sandvault/ssh-session
```

where `ssh-session` is a root-owned wrapper that runs
`$SSH_ORIGINAL_COMMAND` under `sandbox-exec -f /var/sandvault/sandbox-%u.sb`.
Every SSH session is then sandboxed regardless of what the client asked for.
`sv --ssh --no-sandbox` would stop working unless the wrapper carries a
deliberate, host-controlled exception, so decide that before adopting it.

### Implementation notes

- `configure_ssh_access` already generates the file in host-only config
  (`$GUEST_AUTHORIZED_KEYS`). The build-home script currently extracts it
  into the sandbox home as the sandbox user; it would instead install it to
  `/var/sandvault/authorized_keys-$SANDVAULT_USER` as root. The source is
  host-owned and host-written, so root reading it is the same trust as the
  sudoers file. Write to a root temp file beside the target, then rename.
- sshd requires the keys file and its parents to be root-owned (or owned by
  the target user) and not group- or world-writable. Mode 0644 in
  `/var/sandvault` (0755 root) satisfies that; confirm with `StrictModes`
  left on.
- Write the drop-in to a root temp file, validate with `sshd -t` (the temp
  file must be reachable by the main config's `Include`, or validate with
  `sshd -t -f` against a config that includes it), then rename. A drop-in
  that fails to parse can stop sshd from starting, so validation before the
  rename is not optional (§2: "a bad file that installs sticks").
- Confirm the installed macOS includes `sshd_config.d/*.conf` from
  `/etc/ssh/sshd_config` before relying on it; abort with a clear message if
  the `Include` line is missing rather than editing the main config.
- The `Match User` pattern covers all profiles. If per-user scoping is
  wanted on a shared Mac, write one drop-in per host user with an exact
  `Match User` list.
- Remove the stale `~/.ssh/authorized_keys` from the sandbox home on
  upgrade. It becomes inert once the drop-in is active, but leaving it is
  confusing.
- `sv uninstall` removes the keys file; the drop-in is removed when the last
  profile for this host user is uninstalled.
- Upgrade story (§5): the drop-in and the relocated keys file are generated
  artifacts. Bump a version marker so an installed machine rewrites them, and
  state that the change needs `sv --rebuild build`.

### Tests

- As the sandbox user, append a new public key to
  `~/.ssh/authorized_keys`, then attempt `ssh` to the sandbox account with
  that key: must be refused. Must succeed against the previous code.
- Password login to the sandbox account: must be refused.
- `sv --ssh shell -- true` from the host: must still succeed.
- Verify from the host side (§3): no process owned by the sandbox user
  exists outside the profile after the refused attempts.

## Layer 2: SSH group membership only when `--ssh` is used

`sv:1580-1589` adds the sandbox user to `com.apple.access_ssh` on every
rebuild, whether or not SSH mode is ever used.

Change:

- Add the membership only when `MODE == ssh`.
- On a rebuild without `--ssh`, remove it.

On machines where Remote Login is limited to specific users, sshd then
refuses the sandbox account outright. It does nothing where Remote Login is
set to "All users", which is why Layer 1 is still needed.

This is the smallest change and has no new generated artifacts. It can ship
first.

Test: after `sv --rebuild build` without `--ssh`,
`dseditgroup -o checkmember -m sandvault-$USER com.apple.access_ssh` reports
not a member; after `sv --ssh --rebuild build` it reports a member.

## Layer 3: block the connection by uid (optional)

A packet-filter rule that drops loopback connections to the SSH port when
they originate from the sandbox uid. This is the most literal form of
"the sandbox cannot connect to this machine": it applies to every process
owned by that uid, inside or outside `sandbox-exec`, and it protects the
host account's sshd as well as the sandbox account's.

Sketch, in a dedicated anchor:

```
block drop out quick on lo0 proto tcp from any to any port 22 user <sandbox-uid>
```

Costs:

- Needs a pf anchor referenced from `/etc/pf.conf`, pf enabled, and the
  anchor reloaded at boot. That is a larger and more fragile install
  footprint than anything `sv` does today.
- OS updates can reset `/etc/pf.conf`.
- Must be checked against connections to the machine's own non-loopback
  addresses, which are also delivered locally.
- A non-default sshd port needs the rule to follow it.

Treat as optional hardening for users who want it, not a default.

The same mechanism could later restrict the sandbox uid's access to other
localhost services (item 11 of the review), which the sandbox profile does
not limit today.

## Not sufficient alone: a sandbox-profile rule

The profile could deny outbound connections to local port 22. Reasons not to
rely on it:

- It binds only processes started under the profile. Anything the sandbox
  user gets running outside it is unaffected, and that is the case this
  proposal is about.
- Measured on 2026-11-27 for sv2 (macOS 26.7.3): a rule on
  `localhost:22` covers every address of the machine, its LAN address
  included, and no other host's. See "Local port 22" in
  `docs/SANDVAULT2.md`.
- Denying port 22 to all destinations would break git over SSH to remote
  hosts.

Reasonable as defense in depth after Layer 1, measured as the sandbox user
under the rendered profile, with a `SV_SANDBOX_PROFILE_VERSION` bump.

## Related client-side hardening

Separate from the goal above but in the same code path (`sv:2343-2400`):

- Run `ssh` with `-F /dev/null -a -x -o IdentitiesOnly=yes
  -o ClearAllForwardings=yes` so the user's own `~/.ssh/config` cannot
  forward an agent or ports into the sandbox session.
- Pin the host key: build a known-hosts file in a host-only temp directory
  from `/etc/ssh/ssh_host_*_key.pub` and use `StrictHostKeyChecking=yes`
  instead of `no` with `/dev/null`.

## If SSH mode is not needed

Turning Remote Login off in System Settings removes both rows of the table
at the top, whatever else is or is not implemented. Worth saying in the
README next to the `--ssh` documentation.

## Suggested order

1. Layer 2 (small, no new artifacts).
2. Layer 1 without `ForceCommand`: relocate the keys file and add the
   drop-in, with the tests above.
3. Client-side flags and host-key pinning.
4. Decide on `ForceCommand` and what `--ssh --no-sandbox` should mean.
5. Layer 3 and the profile rule as optional hardening.
