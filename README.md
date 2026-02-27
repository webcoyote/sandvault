# SandVault

**Run Claude Code, OpenAI Codex, Google Gemini and shell commands safely in a sandboxed macOS user account**

SandVault (sv) manages a limited user account to sandbox shell commands and AI agents, providing a lightweight alternative to application isolation using virtual machines.

TL;DR:

1. To run `xcodebuild` or `swift` see [Sandboxing xcodebuild and swift](#Sandboxing-xcodebuild-and-swift) for details.
2. To run other sandboxed applications inside sandvault, use the `-x` option. See [Sandboxing other apps](#Sandboxing-other-apps) for details.
3. It's not possible to run GUI applications from within the sandbox; see [Running GUI Applications](#Running-GUI-Applications) for details.


## Features

- **AI ready** - Includes Claude Code, OpenAI Codex, Google Gemini
- **Fast context switching** - No VM overhead; instant user switching
- **Passwordless** - switch accounts without a prompt (after setup)
- **Shared workspace** - joint access to `/Users/Shared/sv-$USER`
- **Clean uninstall** - Complete removal with `sv uninstall`


## Security Model

SandVault has limited access to your computer:

- Cannot access your home directory
- Runs with standard user privileges
- Cannot modify system files
- Has no access to mounted drives

```
- writable:  /Users/Shared/sv-$USER         -- only accessible by you & sandvault-$USER
- writable:  /Users/sandvault-$USER         -- sandvault's home directory
- readable:  /usr, /bin, /etc, /opt         -- system directories
- no access: /Users/*                       -- other user directories

- writable:  /Volumes/Macintosh HD          -- accessible as per file permissions
- no access: /Volumes/*                     -- cannot access mounted/remote/network drives
```


## Installation

Install via Homebrew:

```bash
brew install sandvault
```

Install via git:

```bash
# Clone the repository
  git clone https://github.com/webcoyote/sandvault

# Option 1: add the sandvault directory to your path
  export PATH="$PATH:/path/to/where/you/cloned/sandvault"

# Option 2: add to your shell configuration for easy access
  echo >> ~/.zshrc  'alias sv="/path/to/where/you/cloned/sandvault/sv"'
  echo >> ~/.bashrc 'alias sv="/path/to/where/you/cloned/sandvault/sv"'
```


## Quick Start

```bash
# Run Claude Code in the sandbox
# shortcut: sv cl
  sv claude

# Run OpenAI Codex in the sandbox
# shortcut: sv co
  sv codex

# Run Google Gemini in the sandbox
# shortcut: sv g
  sv gemini

# Run command shell in the sandbox
# shortcut: sv s
  sv shell
```


## Connect via SSH

The default mode for sandvault runs commands as a limited user (basically `sudo -u sandbox-$USER COMMAND`). Sandvault also configures the limited sandvault account so that you can run commands via SSH (basically `ssh sandbox-$USER@$HOSTNAME`), and everything works the same. Use the `-s` or `--ssh` option to use SSH mode with `sv`, or use `tmux` or `screen` (for users so inclined).

```bash
# Run using impersonation
# sv COMMAND
  sv codex

# Run using ssh
# sv -s/--ssh COMMAND
  sv --ssh gemini
```


## Advanced Commands

```bash
# Run shell command in sandvault and exit
# Usage:
#  sv shell [PATH] -- [SHELL_COMMAND]
# Example:
  sv shell /Users -- pwd      # output: /Users


# Run AI agent with optional arguments
# Usage:
#   sv <agent> [PATH] [-- AGENT_ARGUMENTS]
# Example:
  sv gemini -- --continue


# Clone local/remote Git repository into /Users/sandvault-$USER/repositories/<git-repository> and open there
# Usage:
#   sv <agent|shell> --clone URL_OR_LOCAL_PATH [-- AGENT_OR_SHELL_ARGS]
# Examples:
  sv codex --clone https://github.com/webcoyote/sandvault.git
  sv codex -c ~/src/my-app
  sv shell --clone https://github.com/webcoyote/sandvault.git
  sv shell -c ../my-app

Use a full or relative path with a directory name for local clones.

For local Git repositories, sandvault also wires remotes:

- Your local Git repository gets/updates remote `sandvault` -> `/Users/sandvault-$USER/repositories/<git-repository>`
- This lets you run `git fetch sandvault` from the original local Git repository to pull commits made in the sandvault Git repository.


# Send input via stdin
# Usage:
#   <producer> | sv shell [PATH] [-- SHELL_COMMAND]
# Examples:
  echo "pwd ; exit" | sv shell /Users       # output: /Users

  echo ABC | sv shell -- tr 'A-Z' 'a-z'     # output: abc

  cat PROMPT.md | sv gemini
```


## Maintenance Commands

```bash
# Build sandvault but do not run a command
  sv build
  sv b


# Rebuild sandvault, including updating all file permissions and ACLs in the shared volume
  sv build --rebuild
  sv b -r

# Uninstall sandvault (does not delete files in the shared volume)
  sv uninstall


# Misc commands
  sv --version
  sv --help
```


## Nested sandboxes

In addition to running in a different macOS user account, sandvault also runs applications using macOS `sandbox-exec`, which further limits what resources are accessible.

Some applications, like `swift`, already run inside a sandbox. Because macOS does not support nested (i.e. recursive) sandboxes, these applications fail to run.

Read on for solutions.


### Sandboxing xcodebuild and swift

For `swift` (and `xcodebuild`, which runs `swift`), you can set the following variables in your build scripts to run inside sandvault:

For `swift`:

```bash
ARGS=()

# Disable sandboxing when running inside sandvault to avoid nested sandbox-exec
if [[ -n "${SV_SESSION_ID:-}" ]]; then
    ARGS+=(--disable-sandbox)
fi

swift build "${ARGS[@]}" "$@"
```

For `xcodebuild`:

```bash
ARGS=()

# Disable sandboxing when running inside sandvault to avoid nested sandbox-exec
if [[ -n "${SV_SESSION_ID:-}" ]]; then
    export SWIFTPM_DISABLE_SANDBOX=1
    export SWIFT_BUILD_USE_SANDBOX=0
    ARGS+=("-IDEPackageSupportDisableManifestSandbox=1")
    ARGS+=("-IDEPackageSupportDisablePackageSandbox=1")
    # shellcheck disable=SC2016 # Expressions don't expand in single quotes # that is intentional
    ARGS+=('OTHER_SWIFT_FLAGS=$(inherited) -disable-sandbox')
fi

xcodebuild \
    build \
    "${ARGS[@]}" \
    ...
```


### Sandboxing other apps

If the app you intend to run does not support disabling it use of sandbox-exec like `xcodebuild` and `swift` you can run andvault without utilizing `sandbox-exec`:

```bash
# Disable use of sandbox-exec (app still runs as sandvault user) using -x / --no-sandbox
  sv -x           claude
  sv --no-sandbox codex
  sv --no-sandbox shell $HOME/projects/my-app -- xcodebuild ...
```

Disabling `sandbox-exec` has the following security implications:

- No protection against reading/writing removable drives (`/Volumes/...`)
- No protection against writing files with `o+w` (`0002`) file permissions

```bash
# To find all files on your computer that are "world writable" (perms: `o+w` / 0002)
# run this command from your account (not in sandvault):
  find / \
       -path "/Users/sandvault-$USER" -prune \
    -o -path "/Users/sv-$USER" -prune \
    -o -perm -o=w -print 2>/dev/null
```


## Troubleshooting

If your sandbox is misbehaving you can fix it with a rebuild or uninstall/reinstall. They're both safe and will not delete files in the shared sandbox folder.

```bash
# Force rebuild
sv --rebuild build


# Uninstall then reinstall
sv uninstall
sv build
```


### Fix the security popup

![macOS security keychain login dialog](https://webcoyote.github.io/images/shared/sandvault/security-keychain.jpg)

If you see a security popup above, it may be because files in the shared sandvault directory don't have the correct ACLs, which occurs when another user's files are copied into the sandvault shared directory (`/Users/Shared/sv-$USER`). This can be corrected by running the rebuild command `sv --rebuild build`, or adding the rebuild flag to any command, e.g. `sv -r shell`. This only needs to be done once.


## Custom Configuration

SandVault supports custom configuration; see [`./guest/home/README.md`](./guest/home/README.md).


## Running GUI Applications

TL;DR: Sorry, macOS security limitations prevent this from working.

It would be great to be able to run GUI applications (e.g. browsers, Claude Desktop) in the sandbox account to limit their access to main account resources.

The issue seems to be that an application cannot report to a WindowServer that's owned by a different user.

Internet posts suggest it's possible using `sudo su` and `sudo launchctl bsexec`, but those answers are from long ago and it seems likely that Apple improvements to macOS security have closed those doors.

In the event you do find a solution, send a PR please :)


## Why SandVault?

After exploring Docker containers, Podman, sandbox-exec, and virtualization, I needed something that:

- Works natively on macOS without virtualization overhead
- Provides meaningful isolation without too much complexity
- Runs Claude Code with `--dangerously-skip-permissions`
- Runs OpenAI Codex with `--dangerously-bypass-approvals-and-sandbox`
- Runs Google Gemini with `--yolo`
- Maintains a clean separation between trusted and untrusted code

SandVault uses macOS's Unix heritage and user account system to create a simple but effective sandbox.


# Alternatives

- [ClodPod](https://github.com/webcoyote/clodpod) runs Claude Code inside a macOS virtual machine.
- [Chamber](https://github.com/cirruslabs/chamber) is a proof-of-concept app for running Claude Code inside a macOS virtual machine.
- [Claude Code Sandbox](https://github.com/textcortex/claude-code-sandbox) runs Claude Code in a Docker container (Linux)


# License

Apache License, Version 2.0

SandVault Copyright © 2026 Patrick Wyatt

See [LICENSE.md](LICENSE.md) for details.


# Contributors

We welcome contributions and bug reports.

See [CONTRIBUTORS.md](CONTRIBUTORS.md) for the list of contributors to this project.


# Thanks to

This project builds on the great works of other open-source authors:

- [Claude](https://www.anthropic.com/claude) - AI coding assistant
- [Codex](https://openai.com/codex/) - AI coding assistant
- [Homebrew](https://brew.sh): 🍺 The missing package manager for macOS (or Linux)
- [Shellcheck](https://www.shellcheck.net): finds bugs in your shell scripts
- [uv](https://docs.astral.sh/uv/): An extremely fast Python package and project manager, written in Rust
- [Claude Code Hooks Mastery](https://github.com/disler/claude-code-hooks-mastery): Quickly master how to use Claude Code hooks to add deterministic (or non-deterministic) control over Claude Code's behavior
- [StatusLine](https://gist.github.com/dhkts1/55709b1925b94aec55083dd1da9d8f39): project status information for Claude Code

... as well as GNU, BSD, Linux, Git, Sqlite, Node, Python, netcat, jq, and more. "We stand upon the shoulders of giants."
