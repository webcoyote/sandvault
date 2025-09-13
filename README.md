# SandVault

**Run Claude Code safely in a sandboxed macOS user account**

Sandvault creates an isolated user account ("sandvault") with restricted permissions for running Claude Code and other tools with reduced system access. This provides a lightweight alternative to VMs while maintaining security through macOS's built-in user isolation.


## Quick Start

```bash
# Clone the repository
git clone https://github.com/webcoyote/sandvault
cd sandvault

# Run Claude in the sandbox
./sv claude

# Or just get a shell
./sv shell
```

Sandvault creates the `$HOME/sandvault` directory, and only has access to that directory (apart from it's own home directory).


## Installation

Add to your shell configuration for easy access:

```bash
# In ~/.zshrc or ~/.bashrc
alias sv="$HOME/path/to/where/you/cloned/sandvault/sv"
```

Then use:

- `sv` or `sv shell` - Open a sandboxed shell
- `sv claude [PATH]` - Run Claude Code in the sandbox
- `sv uninstall` - Remove the sandvault user and configuration


## How It Works

Sandvault creates a separate macOS user account with:

- Limited filesystem access
- Isolated environment from your main user
- Shared workspace at `~/sandvault` for project files
- Passwordless sudo switching (no password prompts)
- Pre-configured development tools

The sandboxed user can only access:

- The shared workspace (`~/sandvault`) (read/write)
- Its own home directory (`/Users/sandvault`) (read/write)
- System binaries and tools (readonly)


## Features

- **Fast context switching** - No VM overhead, instant user switching
- **Shared workspace** - Easy file exchange through `~/sandvault`
- **Development ready** - Includes Node.js, Python, uv, and Homebrew
- **SSH support** - Connect via SSH with `sv --ssh`
- **Clean uninstall** - Complete removal with `sv uninstall`


## Why Sandvault?

After exploring Docker containers, Podman, sandbox-exec, and virtualization, I needed something that:

- Works natively on macOS without virtualization overhead
- Provides meaningful isolation without too much complexity
- Allows running tools like Claude Code with `--dangerously-skip-permissions`
- Maintains a clean separation between trusted and untrusted code

Sandvault uses macOS's Unix heritage and user account system to create a simple but effective sandbox.


## Requirements

- macOS (Darwin)
- Admin privileges (for initial setup only)
- Homebrew (will be installed if missing)


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

# SSH mode
sv --ssh ...          # Connect via SSH instead of sudo

# Management
sv --rebuild ...      # Force rebuild configuration
sv uninstall          # Remove sandvault user and files
sv --version          # Show version
sv --help             # Show help
```


## Security Model

The sandvault user:

- Cannot access your main user's files (except the shared workspace)
- Runs with standard user privileges
- Cannot modify system files
- Has its own isolated home directory

This provides defense in depth when running untrusted code or experimenting with new tools.


# Alternatives

- [ClodPod](https://github.com/webcoyote/clodpod) runs Claude Code inside a macOS virtual machine.
- [Chamber](https://github.com/cirruslabs/chamber) is a proof-of-concept app for running Claude Code inside a macOS virtual machine.


# License

Apache License, Version 2.0

Sandvault Copyright © 2025 Patrick Wyatt

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
