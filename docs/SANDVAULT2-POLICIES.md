# Sandvault 2 policies

Status: decided on 2026-11-27; nothing here is built. The work, and what is
still to be decided or measured, is tracked in the backlog: TASK-0005 and its
subtasks.
This answers open item 8 of `docs/SANDVAULT2.md`, which is the design this
one belongs to: the trust model, the helper and the rule that root never
opens a host-owned file are described there. Section references such as §1
are to `docs/review-guidelines.md`.

## What it is for

Each sandvault profile has its own `sandbox-exec` file, written by the root
helper, but every profile gets the same rules. A policy lets the host user
choose the rules of a profile: take the default, pick another that ships
with sv2, or write their own.

## Words

- A **rule** is one `allow` or `deny`.
- A **policy** is a named, ordered list of rules and of includes of other
  policies, and nothing else.
- A **profile** is a sandvault profile only: the account, its home and its
  workspace. A profile has one policy. The file `sandbox-exec` reads is the
  profile's rendered policy.

Why these words. `sandbox-exec` calls its whole file a profile, which
sandvault already uses for something else; its manual has no word for one
line, and Apple's own files say "rule" in their comments. "Permission" is
taken by file permissions and the privacy prompts, "entitlement" by code
signing, and "parameter" by `sandbox-exec -D`, which the helper uses for
`--ios`.

## Four ways to get a policy

All four end in the same kind of text.

1. **The default.** `sv2 profile add NAME` uses the policy named `default`.
2. **Another built-in policy,** by name: `sv2 profile add NAME --policy
   offline`.
3. **The wizard.** `sv2 policy new NAME` asks questions and writes an
   ordinary file, filled in from a template.
4. **By hand.** A file in `~/.config/sandvault2/policies/`.

A policy of the user's own:

```scheme
;; ~/.config/sandvault2/policies/reference.sb
(include "default")
(allow file-read* (subpath "/Users/pat/reference"))
```

## The format: rules only

A file holds three forms, with `;` comments:

```scheme
(include "NAME")
(allow OPERATION... FILTER...)
(deny OPERATION... FILTER...)
```

The filters are `literal`, `subpath`, `prefix`, `global-name`,
`global-name-prefix`, `remote` and `local`. A path may start with `$HOME` or
`$WORKSPACE`, which the helper fills in as it fills in `base.sb` today.

The helper refuses everything else:

- `define`, `if`, `import` and `regex`;
- a modifier such as `with no-sandbox`;
- the operation `default`;
- a path that is not absolute;
- a string with a quote, a backslash or a control character in it.

### Why not the whole `sandbox-exec` language

It is a Scheme. Apple's files hold rules, and also variables, functions,
conditions and commands that load other files.

- **The helper could not know what a file grants** except by loading it.
- **Nothing the helper appends would hold.** Measured: a file that says
  `(define (deny . args) #t)` loads, and a `deny` after it denies nothing.
- **A sandbox could write its own policy.** `import` reads another file at
  every session start; if the sandbox can write that file, it controls its
  own rules.
- **Root would run a program the host user wrote.** The helper tests a file
  by loading it, today as root, and root does not trust the host user.

What is given up: a rule from one of Apple's profiles that uses a variable,
a condition or a pattern does not paste in unchanged. A form can be added to
the format later; taking one away would break files people have written.

### What was measured

As the current user under `sandbox-exec`, on macOS 26.7.3 (25G342), on
2026-11-27. Each rule was added to a small default-deny file.

| Rule | Result |
|---|---|
| `(prefix "/private/etc")` | loads |
| two operations in one rule | loads |
| `(subpath (param "SV2_HOME"))` | loads |
| `global-name` and `global-name-prefix` | load |
| `(remote tcp "localhost:22")`, `(remote tcp "*:443")` | load |
| `(remote tcp "github.com:443")` | refused: "host must be * or localhost" |
| an unknown operation, an unknown filter | refused: "unbound variable" |
| a relative path | loads |
| `(allow default)` after `(deny default)` | loads |
| a `deny` after `(define (deny . args) #t)` | loads, and denies nothing |

What follows:

- The helper must refuse `(allow default)` and a relative path itself,
  because `sandbox-exec` accepts them.
- The helper need not know every operation and filter by name: an unknown
  one does not load, and a policy that does not load is never put in place.
- The network can be narrowed by port and to this Mac, not to a site. There
  is no "GitHub only" policy.

### Where the parser is

In `sv2-common`, so that `sv2` reports the line and the helper decides. It
is small and has no dependencies, as everything in the helper must be.

The built-in policies are the helper's own text and are not held to the
format: they keep the pattern for raw disks, the condition on `--ios`, and
`/bin/ps` outside the sandbox.

## What the helper writes around every policy

**First:** `(version 1)` and `(deny default)`. A policy starts from nothing
by construction.

**Last:** the denies no policy may undo. They are last because the last
matching rule wins.

- this Mac's SSH ports;
- `/Library/Keychains`, but for `System.keychain`;
- raw disks and `bpf`;
- the four Mach names `base.sb` keeps denied (DiskArbitration, the two
  NetAuth agents, `appleeventsd`).

A rule of the user's that allows `/Library` whole therefore does not open
the keychains.

`/Users` and `/Volumes` are not in this part. Opening a directory of the
host user's to a sandbox is what a policy is for, and the account's file
permissions still apply to it.

## Order

An include is replaced by the rules it names, where it stands, and the last
matching rule wins. `(deny network*)` after `(include "default")` closes the
network; before it, it does nothing.

## Built-in policies

`base.sb` is cut into small policies. Each allow still names the probe in
`tests/profile-probes.sh` that fails without it (§3).

| Policy | What it allows |
|---|---|
| `core` | processes, signals, terminals, `sysctl` reads, shared memory, priority |
| `home-and-workspace` | reading and writing the sandbox home, the workspace and the temporary directories |
| `read-all` | reading files as today: everything but `/Users` and `/Volumes` |
| `read-system` | the narrower reading; not yet defined |
| `network` | the network |
| `tls` | the trust services curl, git, python and Go programs verify through |
| `keychain` | the sandbox account's own keychains |
| `file-watching` | FSEvents |
| `chromium` | Playwright's headless Chromium |
| `chrome` | Google Chrome, for `sv2 --chrome` |
| `ios-simulator` | CoreSimulator, for `sv2 --ios` |

The ones a user picks are lists of includes:

| Policy | What it is |
|---|---|
| `default` | renders the rules of today |
| `offline` | `default` without the network |
| `strict` | `default` with `read-system` in place of `read-all` |

Three, because each is run through the probes with its own expectations.

The names are an interface. A stored policy that includes a name the helper
no longer has does not render, and its profile does not start until a
policy is set again.

## Where the text is, and how it reaches root

1. The built-in policies are compiled into `sv2-common`: `sv2` shows them,
   and the helper renders from its own copy.
2. The user's policies are files in `~/.config/sandvault2/policies/`, next
   to `agents/`. No sandbox can write there (§2).
3. `sv2` reads the file, replaces includes of the user's own policies (to a
   fixed depth, no cycles), and leaves includes of built-in ones as names.
4. `sv2` sends the text and the policy's name on stdin of a new helper
   verb. No path is passed, and root opens no host-owned file.
5. The helper parses the text and refuses what it does not recognise.
6. The helper keeps the text as sent in a root-owned file beside the
   rendered one, renders, load-tests, and renames into place as it does
   now. The load test runs as an account without privileges and with a
   time limit, no longer as root.

At `install` the helper renders every profile again from what it kept, so a
profile on `default` gets the new `default`. A profile from before policies
has nothing kept and is on `default`.

A sandbox account cannot change its policy: the source is in the host
user's configuration, the kept copy and the rendered file are root's, the
helper refuses a caller that is a sandbox account, and the format has no
form that reads a file when a session starts.

## When a policy takes effect

- **At `sv2 profile set`, not when a file is saved.** `sv2 run` and `sv2
  doctor` compare the file with what the helper kept, which everyone can
  read, and say when they differ. No sandbox is widened by a save.
- **For sessions started afterwards.** A process keeps the rules it started
  under, and a tmux server outlives its session. `profile set` says which
  processes of the account are still running and offers to end them.

`--ios` stays a switch of the session. It needs a policy that includes
`ios-simulator`; `launch` refuses it on any other.

## Commands

```
sv2 profile add NAME [--policy POLICY]  set the profile's policy at creation
sv2 profile set NAME --policy POLICY    change it later
sv2 profile list                        also shows each profile's policy
sv2 policy list                         built-in and yours, and which profiles use each
sv2 policy show NAME [--expanded]       the text, or what the helper would render
sv2 policy new NAME [--from POLICY]     the wizard: asks, then writes an ordinary file
sv2 policy edit NAME                    edit a copy, check it, then replace the file
sv2 policy check NAME                   parse it and load it under sandbox-exec
sv2 policy rename OLD NEW               yours only; refuses while in use
sv2 policy remove NAME                  yours only; refuses while in use
```

- **`profile set` takes `--policy`,** as `profile add` does, so that `set`
  can change something else later.
- **`remove`, not `delete`,** as in `profile remove`. Like `profile remove`
  it must be written in full.
- **`policy remove` and `policy rename` refuse a policy in use,** and name
  what uses it: a profile that has it, or another policy that includes it.
  Removing one in use would leave a profile on a kept copy with no file to
  edit; renaming one would need a helper call for each profile, which would
  also apply edits not yet applied.
- **`policy edit` works as `visudo` does.** The editor gets a copy, the
  copy is checked, and only a copy that passes replaces the file. It
  changes no profile, and prints the `profile set` command for each profile
  that has the policy. On a built-in policy it is refused, with `sv2 policy
  new NAME --from BUILTIN` as the way.
- **`policy new --from` writes a list of includes,** not of rules, so the
  small policies in it still follow the helper.
- **`sv2 p` is ambiguous already** (`profile`, `pi`); `policy` adds a
  third.
