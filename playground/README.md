# Playground


## Sandbox testbed

`sandbox-testbed` is a minimal sandbox runner for experimenting with `sandbox-exec` policies.

Usage examples:

- Interactive shell: `./sandbox-testbed`
- Run a command: `./sandbox-testbed -- echo hello`


## Requirements

- A sandbox user named `sandvault-$USER` created via `./sv build`.
- Passwordless sudo for that sandbox user (also set up by `./sv build`).
