# sandvault2 (`sv2`)

`sv2` runs AI agents in a separate, sandboxed macOS user account. It is the
rewrite of sandvault (`sv`, described in [README.md](README.md)) in Rust.

**Status: in development.** It is built from this repository and is not yet
packaged for Homebrew. `sv` and `sv2` can be installed side by side; they
share nothing. The design, and what is and is not finished, is in
[docs/SANDVAULT2.md](docs/SANDVAULT2.md).


## What is different from `sv`

- **No password after the install.** `sudo` is needed to install, to
  uninstall, and to allow or deny a user. Creating, entering and removing
  sandboxes needs none, and works for users who are not administrators.
- **Profiles.** Each profile is a sandbox of its own: its own macOS account,
  home directory and workspace.
- **`sv2` does not work inside the sandbox's directories.** Neither `sv2` nor
  its root helper reads, writes or runs tools in a workspace or a sandbox
  home; the sandbox account does that itself. Most of the ways out of the
  `sv` sandbox that were found and fixed were of that kind.
- **The sandbox profile denies by default**, and every session has it. There
  is no `--no-sandbox`.
- **Agents are definitions, not code.** An agent is added with a small file,
  without changing `sv2`.
- **Removed:** SSH mode, `sv-clone` and the `/sv` skill, the agentsview
  setup, nested `sv`, and `SANDVAULT_ARGS`. See "Features removed" in the
  design document for what replaces each.


## Build and install

You need Rust ([rustup.rs](https://rustup.rs)) and macOS.

```bash
scripts/build                     # builds sv2/target/release/sv2 and sv2-helper
sv2/target/release/sv2 install    # asks for your password, once
```

`install` copies the helper to `/Library/PrivilegedHelperTools`, writes a
sudoers file that lets members of the `sv2-users` group call it, adds you to
that group, and keeps sandbox accounts from logging in over SSH. Run it
again after you build a newer `sv2`: `sv2` says so when the installed helper
is out of date.

To let another user of the Mac have sandboxes: `sv2 user allow USER`.

A build from a checkout does not find the agent definitions on its own if
the checkout is somewhere a sandbox can write (such as `/Users/Shared`).
`scripts/sv2-dev-agents` copies them to `~/.config/sandvault2/agents`; read
its header first.


## Use

```bash
sv2 shell                         # a shell in the default sandbox
sv2 claude                        # an agent; installed with Homebrew if missing
sv2 --native claude               # the agent's own install, in the sandbox home
sv2 -p work claude                # in the profile "work"
sv2 --lightpanda claude           # with a headless browser in the sandbox
sv2 shell ls -la                  # one command, then back
sv2 herdr                         # many agents in one terminal; reattaches
```

herdr's server, and every pane in it, outlives the `sv2` that started it,
inside the sandbox. So it refuses `--chrome` and `--lightpanda`: the browser
stops with the session, and the panes would keep its address.

`sv2 --help` lists every command and the agents it knows. Names can be
shortened: `sv2 s` is `sv2 shell`.

- The first run creates the `default` profile. Others are made with
  `sv2 profile add NAME`, listed with `sv2 profile list`, and removed with
  `sv2 profile remove NAME`.
- A session starts in the profile's workspace,
  `/Users/Shared/sv2/YOU/PROFILE`, or in your current directory if that is
  inside it. You and the sandbox can both read and write everything there.
  It is the place to put what the agent should work on.
- The sandbox account is `sv2-YOU-PROFILE`, with its home in `/Users`.
- Options (`-p`, `--native`, `--lightpanda`, `--chrome`) come before the agent's name. Everything
  after it goes to the agent unchanged.


## What a sandbox can and cannot do

- It can write in its workspace, its own home and the temporary
  directories, and nowhere else.
- It cannot read your home directory.
- It has the network, unrestricted but for this Mac's own SSH port: 22, and
  another one if `/etc/services` or sshd's configuration names it. An agent
  that is talked into it can send anything it can read to anywhere.
- It can connect to every other port of this Mac, because its own servers
  are there. A service of yours that listens on this Mac and asks for no
  password is open to it.
- It cannot connect to this Mac's SSH port, and sshd refuses a login as a
  sandbox account from anywhere. It cannot use your pasteboard.
- It runs on a terminal of its own, so it cannot type into yours.

The sandbox is a separate account under a `sandbox-exec` profile. It is not
a virtual machine. One way around the profile is known and not yet closed:
see open item 7, "launchd as a route around the profile", in the design
document.


## Your configuration in the sandbox

Put the files you want in every sandbox home (`.zshrc`, `.gitconfig`,
a `bin` directory) into:

```
~/.config/sandvault2/home/
```

They are copied into a profile's sandbox home when you next run `sv2` in
that profile, and again whenever the directory has changed. `sv2` says so
when it copies, and `sv2 doctor` shows which profiles are up to date.

- Symbolic links are followed: a link to a file kept elsewhere arrives as
  the file.
- A file of the same name in the sandbox home is replaced. Nothing is ever
  deleted there: a file you take out of the directory stays in the sandbox
  homes it was copied to.
- A change made inside the sandbox to a copied file lasts until your own
  copy changes.
- **Do not put secrets there.** Every sandbox of yours gets what is in it.
- Paths in those files that point into your own home will not work in the
  sandbox, which cannot read it. A `.gitconfig` that names a hooks
  directory is one example: keep the hooks in the directory too, and name
  them by their place in the sandbox home.
- A `.zprofile` of yours replaces the one `sv2` writes, which puts Homebrew
  and `~/.local/bin` ahead of the system's directories. Include that
  yourself if you want it.


## Git and GitHub from a sandbox

A sandbox has none of your credentials. Two commands give it some:

```bash
sv2 deploy-key OWNER/REPO           # a key for one repository, read-only
sv2 deploy-key OWNER/REPO --write   # the same, with push
sv2 gh-token                        # a read-only token for gh
```

`deploy-key` makes the key inside the sandbox and prints the command that
adds its public half to the repository; you run that yourself. See
[docs/DEPLOY-KEYS.md](docs/DEPLOY-KEYS.md).


## A browser in the sandbox

```bash
sv2 --lightpanda claude    # Lightpanda
sv2 --chrome claude        # Chrome, headless
```

The browser runs inside the sandbox, as the sandbox account, for as long as
the session does. A page it loads reaches what the agent reaches and nothing
else. The session finds it at `$SV_BROWSER_ENDPOINT`
(`http://127.0.0.1:PORT`), which Playwright's `connectOverCDP` and
Puppeteer's `connect` take.

An agent whose definition has a `system_prompt` (claude, pi, muse) is told
in its system prompt that the browser is there and how to attach to it.
Others are not; `sv2` says which when the session starts.

Lightpanda is installed with Homebrew when it is missing
(`lightpanda-io/browser/lightpanda`, Apple silicon only).

`--chrome` starts one of two programs:

- **Playwright's headless shell** of Chromium. The first `--chrome` in a
  profile installs it in the sandbox home with
  `npx playwright install --only-shell chromium` (about ten seconds). `npx` comes with node, which `sv2` installs with
  Homebrew when it is missing. Later sessions use what is there.

- **Google Chrome** from `/Applications`, when that install is not possible
  or fails. It has a limit in a sandbox: **it ends when one of its windows closes.** It opens real windows
  even when headless, and a sandbox account has no screen. Opening pages,
  navigating, and closing a tab of the first window are fine. Closing the
  last page of a browser context, or the context, ends the browser for the
  rest of the session. `sv2` says so when it starts this one.

Also:

- **Attach, do not launch.** A tool that starts Google Chrome itself
  (Playwright's `launch` with `channel: 'chrome'`, and its MCP server by
  default) loses it at the first page. Give it the endpoint, or have it use
  Playwright's own `chromium`, which is the headless shell when headless.
- Chrome runs with `--no-sandbox`: it cannot set up its own sandbox inside
  this one. A page that breaks out of Chrome has the sandbox account's
  access, which is what the agent has already.


## Your own agent

```bash
sv2 agent new NAME                  # a definition from a template
sv2 agent new claude --from claude  # a copy of a built-in, to change
```

Definitions live in `~/.config/sandvault2/agents/NAME/agent.toml` and take
precedence over the built-ins of the same name. The template shows every
key. `sv2` refuses a definition kept where a sandbox could change it.
A program that needs more than the keys say can have a `wrapper`: a shell
script beside `agent.toml` that runs in the sandbox, is given the program
and its arguments, and runs it (`sv2/agents/herdr/run.sh` is one).


## When something is wrong

```bash
sv2 doctor
```

It changes nothing and needs no password. Each line is `ok`, `note` or
`PROBLEM`, and a problem says what to run.

**"would like to administer your computer".** When `sv2` removes a profile,
macOS may ask whether your terminal may administer your computer. Answer
"Allow". If nobody answers within a minute, `sv2` gives up and says so; run
the command again. If it was refused once, macOS does not ask again until
you run:

```bash
tccutil reset SystemPolicySysAdminFiles
```

**Files in the workspace that you or the sandbox cannot use.** Files moved
into a workspace from elsewhere lack the permissions that files created
there get, and so do files an agent moves in from its own home.
`sv2 repair PATH` puts them right.

**An error message ends in two odd words in brackets**, such as
`(loitering pelican)`. They identify the place in the code that raised the
error. Include them in a bug report.


## Troubleshooting

### git cannot reach a repository after `sv2 deploy-key`

See [docs/DEPLOY-KEYS.md](docs/DEPLOY-KEYS.md): how a deploy key is chosen for
each repository in an sv2 sandbox, what is stored where, and a table of
symptoms and causes.


## Uninstall

```bash
sv2 profile remove NAME           # for each profile; keeps a workspace with files in it
sv2 uninstall                     # asks for your password
```

`sv2 uninstall` refuses while profiles exist and names them.
`sv2 uninstall --all-profiles` removes every profile of every user as well.


## Not there yet

Homebrew packaging; the iOS simulator bridge; per-profile capabilities.
The list, with what has
and has not been verified, is under "Remaining work" in
[docs/SANDVAULT2.md](docs/SANDVAULT2.md).
