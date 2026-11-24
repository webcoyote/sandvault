# Sandvault workspace for __HOST_USER__

This directory is a shared workspace between your macOS account (__HOST_USER__)
and the sandbox account (__SANDBOX_USER__), and both accounts can read and write it.

Sandvault runs shell commands and coding agents (Claude Code, Codex, OpenCode,
Gemini, pi, Muse, herdr) as __SANDBOX_USER__, a separate limited macOS account.

- **Host account**     — __HOST_USER__, your login
- **Sandbox account**  — __SANDBOX_USER__, runs the agents
- **Sandbox home**     — `__SANDBOX_HOME__`, not readable from your account
- **Shared workspace** — `__WORKSPACE__`, this directory
- **Managed state**    — `./_sandvault/` written by sv, leave it alone

This workspace belongs to __PROFILE_NOTE__. A profile is a separate sandbox
with its own home directory, workspace, SSH keys, and Keychain, and profiles
cannot read one another: run sv --profile NAME with any command to use one, or
sv profiles list to see every sandbox you own.

## Enter the sandbox

    sv shell        a shell as the sandbox account
    sv claude       Claude Code (see `sv --help` for other agents supported)

## Shell configuration

__SANDBOX_USER__ sources `./user/.zshrc` and `./user/.zprofile` from this workspace at
login, so tool configuration belongs there, not in the sandbox home directory.
