# Changelog

All notable user-facing changes to SandVault are documented in this file.

## [1.8.0] - 2026-04-14

### Fixed
- Fix OpenCode permission bypass so sandbox restrictions are properly enforced ([#111](https://github.com/webcoyote/sandvault/pull/111)) — thanks @MikeMcQuaid!

### Thanks to 2 contributors!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@webcoyote](https://github.com/webcoyote)

## [1.7.0] - 2026-04-13

### Fixed
- Preserve user customizations to `.gitconfig` and `.claude.json` across sandbox sessions instead of overwriting them on each launch ([#109](https://github.com/webcoyote/sandvault/pull/109))

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.6.0] - 2026-04-13

### Added
- Add OpenCode agent support ([#107](https://github.com/webcoyote/sandvault/pull/107)) — thanks @MikeMcQuaid!

### Thanks to 2 contributors!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@webcoyote](https://github.com/webcoyote)

## [1.5.0] - 2026-04-12

### Fixed
- Fix native install for Codex and Gemini agents when nvm is in use — `.npmrc` prefix setting was breaking nvm

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.4.0] - 2026-04-12

### Fixed
- Prevent keychain login dialog from popping up during sandbox sessions ([#104](https://github.com/webcoyote/sandvault/pull/104))

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.3.0] - 2026-04-11

### Added
- Native install option for AI agents (Claude, Codex, Gemini) — run agents directly on the host with sandboxed access to the current project
- `SANDVAULT_ARGS` environment variable for setting default `sv` arguments

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.2.5] - 2026-04-10

_No user-facing changes. This release includes internal CI fixes._

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.2.4] - 2026-04-10

_No user-facing changes. This release includes internal release tooling fixes._

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.2.3] - 2026-04-10

_No user-facing changes. This release includes internal CI and release tooling improvements._

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.2.2] - 2026-04-10

_No user-facing changes. This release includes internal CI and release tooling improvements._

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.2.1] - 2026-04-10

### Fixed

- Fix version number not being updated in `sv` binary during 1.2.0 release ([#94](https://github.com/webcoyote/sandvault/pull/94))

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.1.34] - 2026-04-09

### Added

- Add browser automation and testing support

### Fixed

- Fix xargs error when no files synced with rsync

## [1.1.33] - 2026-04-03

### Fixed

- Fix session-exit cleanup scope bug — thanks @MikeMcQuaid!
- Warm up quarantined Homebrew tools to prevent first-run delays — thanks @MikeMcQuaid!

### Thanks to 2 contributors!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@webcoyote](https://github.com/webcoyote)

## [1.1.32] - 2026-04-01

### Fixed

- Fix WORKSPACE path to use Homebrew opt/ symlink instead of Cellar

## [1.1.31] - 2026-03-31

### Fixed

- Fix SSH mode when Remote Login is set to "All users" — thanks @jesserobbins!

### Thanks to 2 contributors!

- [@jesserobbins](https://github.com/jesserobbins)
- [@webcoyote](https://github.com/webcoyote)

## [1.1.30] - 2026-03-31

### Added

- Add `--fix-permissions` flag, umask detection, and permission hardening — thanks @jesserobbins!

### Thanks to 2 contributors!

- [@jesserobbins](https://github.com/jesserobbins)
- [@webcoyote](https://github.com/webcoyote)

## [1.1.29] - 2026-03-29

### Changed

- Move custom configuration to `$SHARED_WORKSPACE/user`

## [1.1.28] - 2026-03-16

### Fixed

- Fix zprofile PATH bootstrapping — thanks @MikeMcQuaid!

### Thanks to 2 contributors!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@webcoyote](https://github.com/webcoyote)

## [1.1.27] - 2026-03-08

### Fixed

- Fix install detection for AI agents

## [1.1.26] - 2026-03-03

### Fixed

- Fix initial directory when not cloning
- Handle user directory being a symlink — thanks @MikeMcQuaid!

### Thanks to 2 contributors!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@webcoyote](https://github.com/webcoyote)

## [1.1.25] - 2026-02-28

### Changed

- Speed up `--clone` for sandvault user accessible repositories — thanks @MikeMcQuaid!

### Thanks to 2 contributors!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@webcoyote](https://github.com/webcoyote)

## [1.1.24] - 2026-02-27

### Fixed

- Fix permissions errors when cloning repositories

### Changed

- Remove sandvault user from staff group for better isolation

## [1.1.23] - 2026-02-26

### Added

- Add `sv --clone` to clone repositories into the sandbox — thanks @MikeMcQuaid!

### Fixed

- Allow "." and ".." as repo names by resolving paths fully

### Thanks to 2 contributors!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@webcoyote](https://github.com/webcoyote)

## [1.1.22] - 2026-02-24

### Fixed

- Search for Claude at native install location

## [1.1.21] - 2026-02-12

### Added

- Enable running sandvault inside sandvault (nested sandboxes)

## [1.1.20] - 2026-02-10

### Changed

- Use Bash 3.2 for all scripts to ensure macOS compatibility
- Rename `VERBOSE` env-var to `SV_VERBOSE`

### Fixed

- Fix scripts to trap on error for better reliability

## [1.1.19] - 2026-02-07

### Added

- Add strict sandbox disk write rules for tighter security

### Changed

- Allow running `/bin/ps` in sandbox

## [1.1.18] - 2026-02-03

### Added

- Add `--no-sandbox` option to disable use of sandbox-exec

## [1.1.17] - 2026-02-03

### Fixed

- Fix ACL traversal for sandvault shared workspace

## [1.1.16] - 2026-02-03

### Fixed

- Fix PATH ordering: `/opt/homebrew/bin` before `/bin`

## [1.1.15] - 2026-02-03

### Fixed

- Clean up sandvault-configure sentinel files

## [1.1.14] - 2026-01-30

### Fixed

- Fix file ownership ordering for sandvault files

## [1.1.13] - 2026-01-29

### Fixed

- Fix quoting and stdin-piping for SSH mode
- Fix sudoers: move validated file to sudoers.d to avoid writing corrupted data
- Reduce sudoers privileges for better security

## [1.1.12] - 2026-01-29

### Fixed

- Revert sudoers fix (hotfix release)

## [1.1.11] - 2026-01-29

### Fixed

- Fix workspace resolution for Homebrew installations
- Remove overly permissive sudoers rule

## [1.1.10] - 2026-01-27

### Changed

- Reduce Homebrew dependencies — thanks @MikeMcQuaid!
- Improve rsync file ownership handling
- Improve SSH connectivity check
- Improve environment setup in sandbox execution

### Thanks to 2 contributors!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@webcoyote](https://github.com/webcoyote)

## [1.1.9] - 2026-01-24

### Added

- Add shell command argument passing support

### Fixed

- Propagate exit codes from sandbox commands

## [1.1.8] - 2026-01-23

### Added

- Show AI agent is running as sandvault user

## [1.1.7] - 2026-01-22

### Fixed

- Fix zsh profile files for non-interactive use

## [1.1.6] - 2026-01-21

### Fixed

- Fix race condition in multi-instance session cleanup

## [1.1.5] - 2026-01-21

### Fixed

- Fix TMPDIR ownership by creating it as sandvault user

## [1.1.4] - 2026-01-21

### Added

- Block access to external drives using sandbox-exec

## [1.1.3] - 2026-01-21

### Fixed

- Check SSH group membership before adding user
- Set unique TMPDIR to avoid conflicts between users

## [1.1.2] - 2026-01-20

### Changed

- Avoid unnecessary brew install commands
- Continue running when Remote Login is disabled (unless mode=SSH)

## [1.1.1] - 2026-01-19

### Added

- Add `--yolo` flag for Gemini to match Claude/Codex

## [1.1.0] - 2026-01-18

### Added

- Add support for Google Gemini
- Add support for OpenAI Codex
- Add pass-through arguments to claude/codex/shell commands
- Shorten shared directory name to `sv-$USER`
- ACL-based permissions with per-user sandboxes

### Fixed

- Fix reversed comparison that prevented sandvault shutdown
- Fix npm install location for claude & codex
- Fix missing arguments failure — thanks @MikeMcQuaid!
- Fix symlink resolution for script directory — thanks @AlessandroW!
- Fix Homebrew bootstrapping in sandbox — thanks @AlessandroW!
- Use less opinionated zshrc defaults — thanks @MikeMcQuaid!

### Changed

- Resync sandvault `$HOME` every run without password prompt
- Improve shared workspace permission management

### Removed

- Remove git-lfs dependency — thanks @MikeMcQuaid!

### Thanks to 7 contributors!

- [@AlessandroW](https://github.com/AlessandroW)
- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@KingMob](https://github.com/KingMob)
- [@redLocomotive](https://github.com/redLocomotive)
- [@jdaln](https://github.com/jdaln)
- [@jesserobbins](https://github.com/jesserobbins)
- [@webcoyote](https://github.com/webcoyote)
