# Deploy keys in an sv2 sandbox

How `sv2 deploy-key` gives a sandbox access to one GitHub repository, what it
puts in the sandbox, and what to check when git cannot reach a repository.

## What a deploy key is, and why it needs care

A deploy key is an ssh key that GitHub accepts for exactly one repository,
read-only unless you allow it to write. It is the narrowest credential you can
hand an agent: one repository, and nothing else you can reach.

The awkward part is having more than one. When ssh connects, GitHub takes the
first key it recognises. If that key belongs to a different repository,
GitHub answers "Permission denied", even though a key that would work is
sitting next to it. So something has to choose the key per repository.

## Using it

On the host:

```
sv2 deploy-key acme/api            # read-only
sv2 deploy-key acme/api --write    # may also push
sv2 deploy-key -p work acme/api    # in the profile "work"
```

`sv2` prints a `gh repo deploy-key add ...` command. Run it yourself: it names
the repository and says whether the key may write, and `sv2` adds nothing to
GitHub on its own. Then, in a new session:

```
git clone git@github.com:acme/api.git
```

Use the `git@github.com:` form of the address. An `https://` address never
uses a deploy key.

## What happens, step by step

1. The sandbox user makes an ed25519 key pair in its own home. The private
   key never leaves the sandbox.
2. `sv2` reads back the public key. It accepts the answer only if it is
   exactly one ed25519 public key on one line, drops the comment, and keeps
   its own copy in `~/.local/state/sandvault2/deploy-keys/PROFILE/` on the
   host. The command it prints adds that copy, which the sandbox cannot
   change.
3. The repository in the command is the one you typed. The sandbox is never
   asked which repository the key is for.

## What is in the sandbox

Everything is in one directory of the sandbox home, `~/.ssh/sv2-deploy-keys/`:

| File | What it is |
|---|---|
| `OWNER--REPO`, `OWNER--REPO.pub` | The key pair, named in lower case |
| `known_hosts` | GitHub's ssh host key, pinned |
| `ssh` | The ssh that git uses; see below |

Nothing is written to the sandbox's `~/.gitconfig` or `~/.ssh/config`. Those
are yours, and may be replaced wholesale (by a copy from the host, for
instance) without breaking deploy keys. Whatever replaces a sandbox home's
files should leave `~/.ssh/sv2-deploy-keys/` alone.

Your own git and ssh configuration on the host are never touched.

## How the key is chosen

At the start of every session, if `~/.ssh/sv2-deploy-keys/ssh` exists, the
session's environment gets:

```
GIT_SSH_COMMAND=/Users/sv2-USER-PROFILE/.ssh/sv2-deploy-keys/ssh
```

git runs that program in place of `ssh`, and tells it which repository it
wants. The program then does one of two things:

- **The sandbox has a key for exactly that repository** (compared without
  regard to capitals or a trailing `.git`): it runs ssh with that key and no
  other, checks GitHub's host key against the pinned one, and says so on
  standard error: `sv2: using the deploy key for acme/api`.
- **Anything else** (another repository, another host, a use of ssh that is
  not git's): it runs plain `ssh` with the same arguments.

So a key for `acme/api` is not offered for `acme/api-docs`, and a repository
without a key behaves as if deploy keys did not exist.

### What was tried first, and why it was dropped

The first version told git to rewrite the repository's addresses
(`url.<alias>.insteadOf`) to an ssh alias that carried the key. git matches
those rules against the start of an address, so a key for `acme/api` also
caught `acme/api-docs` and `acme/api2`, which then failed with "Permission
denied" although they were public. The rules also sent the repository's own
`https://` address to the key, and missed an address written with other
capitals. They lived in `~/.gitconfig` and `~/.ssh/config`, which a copied-in
configuration would have overwritten. Choosing the key where the exact
repository is known has none of these problems.

## Things that can surprise you

- **A session that was already running** when you made the first key does
  not have `GIT_SSH_COMMAND`. Start a new one. (Later keys are picked up at
  once: the program looks the key up each time.)
- **`GIT_SSH_COMMAND` wins over `core.sshCommand`.** If your `.gitconfig`
  sets `core.sshCommand`, git ignores it inside a sandbox that has deploy
  keys. A `core.sshCommand` set in one repository's own configuration is
  ignored the same way.
- **Something that sets `GIT_SSH_COMMAND` itself**, or unsets it, bypasses
  the key.
- **Only git goes through it.** `ssh git@github.com` typed by hand does not
  use a deploy key.
- **Tools with their own git** do not run git's ssh at all. Cargo's built-in
  client is one; `CARGO_NET_GIT_FETCH_WITH_CLI=true` makes it use `git`.
- **`https://` addresses** of a private repository fail as they would
  without a key. Use `git@github.com:OWNER/REPO.git`.
- **The sandbox can change or delete these files.** They are in its home.
  That can break its own access and nothing else.

## When git cannot reach a repository

Inside the sandbox:

```
echo "$GIT_SSH_COMMAND"                  # should name .../sv2-deploy-keys/ssh
ls ~/.ssh/sv2-deploy-keys                # the key for owner--repo, in lower case
GIT_TRACE=1 git ls-remote git@github.com:OWNER/REPO.git
```

| What you see | What it means |
|---|---|
| No `sv2: using the deploy key for ...` line | No key for that repository, an `https://` address, or `GIT_SSH_COMMAND` is not set (an old session) |
| The line, then `Permission denied (publickey)` | The key is not added to the repository on GitHub, or was removed there. Run `sv2 deploy-key OWNER/REPO` again and run the command it prints |
| The line, then a push is refused | The key was added read-only. Run `sv2 deploy-key OWNER/REPO --write`, and replace the key on GitHub |
| `Host key verification failed` with the line | GitHub's host key is not the pinned one: stop, and find out why |
| `Host key verification failed` without the line | Plain ssh, to a host the sandbox has not seen before. No deploy key is involved |

To remove a key: delete it in the repository's settings on GitHub ("Deploy
keys"), and delete the two key files in the sandbox. Removing the profile
removes its keys from the sandbox and `sv2`'s copies on the host; the entries
on GitHub stay until you delete them there.
