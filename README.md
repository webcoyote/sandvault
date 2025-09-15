# SandVault

**Run Claude Code safely in a sandboxed macOS user account**

SandVault creates an isolated user account ("sandvault-$USER") with restricted permissions for running Claude Code and other tools with limited system access. This provides a lightweight alternative to VMs while maintaining security through macOS's built-in user isolation.


## Features

- **Development ready** - Includes Claude Code, Node.js, Python, uv, and Homebrew
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

# Run Claude in the sandbox
sv claude

# Or a shell
sv shell
```

SandVault has limited access to your computer:

```
- writable:  /Users/Shared/sandvault-$USER  -- only accessible by you & sandvault-$USER
- writable:  /Users/sandvault-$USER         -- sandvault's home directory
- readable:  /usr, /bin, /etc, /opt         -- system directories
- no access: /Users/*                       -- other user directories
```


## Custom Claude Code Configuration

SandVault supports deploying your own custom Claude Code configuration, like hooks, agents, and plugins.

1. Copy `./guest/home/.env.sample` to `./guest/home/.env` and edit the `CLAUDE_CONFIG_REPO` variable to your Git repository containing your Claude Code configuration files
2. Run `sv c --rebuild` to copy your configuration (only needs to be done once)

Your repository will be cloned to `/Users/sandvault-$USER/.claude/` during setup.


## Why SandVault?

After exploring Docker containers, Podman, sandbox-exec, and virtualization, I needed something that:

- Works natively on macOS without virtualization overhead
- Provides meaningful isolation without too much complexity
- Allows running tools like Claude Code with `--dangerously-skip-permissions`
- Maintains a clean separation between trusted and untrusted code

SandVault uses macOS's Unix heritage and user account system to create a simple but effective sandbox.


## Commands

```bash
# Shell mode (default)
sv                    # Open sandboxed shell
sv shell [PATH]       # Open shell at specific path
sv s [PATH]           # Short alias

# Claude mode
sv claude [PATH]      # Run Claude Code
sv c [PATH]           # Short alias
sv run [PATH]         # Alternative alias
sv r [PATH]           # Yet another alias

# SSH mode
sv --ssh ...          # Connect via SSH instead of sudo

# Management
sv uninstall          # Remove sandvault user and files
sv --rebuild ...      # Force rebuild configuration
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
- [Homebrew](https://brew.sh): 🍺 The missing package manager for macOS (or Linux)
- [Shellcheck](https://www.shellcheck.net): finds bugs in your shell scripts
- [uv](https://docs.astral.sh/uv/): An extremely fast Python package and project manager, written in Rust
- [Claude Code Hooks Mastery](https://github.com/disler/claude-code-hooks-mastery): Quickly master how to use Claude Code hooks to add deterministic (or non-deterministic) control over Claude Code's behavior
- [StatusLine](https://gist.github.com/dhkts1/55709b1925b94aec55083dd1da9d8f39): project status information for Claude Code

... as well as GNU, BSD, Linux, Git, Sqlite, Node, Python, netcat, jq, and more. "We stand upon the shoulders of giants."
