<div align="center" id="sandvault">
<a href="https://github.com/webcoyote/sandvault" title="sandvault">
  <img src="./assets/icon.jpg" alt="Sandvault Banner" width="128">
</a>
</div>

---

# SandVault

**Run Claude Code and OpenAI Codex safely in a sandboxed macOS user account**

SandVault creates an isolated user account ("sandvault-$USER") with restricted permissions for running AI tools with limited system access. This provides a lightweight alternative to VMs while maintaining security through macOS's built-in user isolation.


## Features

- **Development ready** - Includes Claude Code, OpenAI Codex, Google Gemini, Node.js, Python, uv, and Homebrew
- **Shared workspace** - joint access to `/Users/Shared/sandvault-$USER`
- **Fast context switching** - No VM overhead, instant user switching
- **Passwordless** - switch accounts or use SSH without a prompt (after setup)
- **Clean uninstall** - Complete removal with `sv uninstall`


## Quick Start

```bash
# Clone the repository
git clone https://github.com/webcoyote/sandvault
cd sandvault

# Add to your shell configuration for easy access:
echo >> ~/.zshrc  'alias sv="/path/to/where/you/cloned/sandvault/sv"'
echo >> ~/.bashrc 'alias sv="/path/to/where/you/cloned/sandvault/sv"'

# Run Claude Code in the sandbox
# shortcut: sv cl
sv claude

# Run OpenAI Codex in the sandbox
# shortcut: sv co
sv codex

# Run Google Gemini in the sandbox
# shortcut: sv g
sv gemini

# Or a shell
# shortcut: sv s
sv shell
```

SandVault has limited access to your computer:

```
- writable:  /Users/Shared/sandvault-$USER  -- only accessible by you & sandvault-$USER
- writable:  /Users/sandvault-$USER         -- sandvault's home directory
- readable:  /usr, /bin, /etc, /opt         -- system directories
- no access: /Users/*                       -- other user directories
```


## Custom Configuration

SandVault supports custom configuration; see `./guest/home/README.md`.


## Why SandVault?

After exploring Docker containers, Podman, sandbox-exec, and virtualization, I needed something that:

- Works natively on macOS without virtualization overhead
- Provides meaningful isolation without too much complexity
- Runs Claude Code with `--dangerously-skip-permissions`
- Runs OpenAI Codex with `--dangerously-bypass-approvals-and-sandbox`
- Runs Google Gemini
- Maintains a clean separation between trusted and untrusted code

SandVault uses macOS's Unix heritage and user account system to create a simple but effective sandbox.


## Commands

```bash
# Open shell (zsh) in sandvault
# shortcut: sv s
sv shell [PATH]

# Open Claude Code in sandvault
# shortcut: sv cl
sv claude [PATH]

# Open OpenAI Codex in sandvault
# shortcut: sv co
sv codex [PATH]

# Open Google Gemini in sandvault
# shortcut: sv g
sv gemini [PATH]

# Build sandvault
# shortcut: sv b
sv build

# SSH mode
sv --ssh ...          # Connect via SSH instead of sudo

# Management
sv uninstall          # Remove sandvault (but keep any files in shared directory)
sv --rebuild ...      # Force rebuild
sv --version          # Show version
sv --help             # Show help
```


## Security Model

The sandvault user:

- Cannot access your home directory
- Runs with standard user privileges
- Cannot modify system files
- Has its own isolated home directory

This provides defense in depth when running untrusted code or experimenting with new tools.


# Alternatives

- [ClodPod](https://github.com/webcoyote/clodpod) runs Claude Code inside a macOS virtual machine.
- [Chamber](https://github.com/cirruslabs/chamber) is a proof-of-concept app for running Claude Code inside a macOS virtual machine.
- [Claude Code Sandbox](https://github.com/textcortex/claude-code-sandbox) runs Claude Code in a Docker container (Linux)


# License

Apache License, Version 2.0

SandVault Copyright © 2025 Patrick Wyatt

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
