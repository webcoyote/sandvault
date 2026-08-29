# Changelog

All notable user-facing changes to SandVault are documented in this file.

## [1.30.0] - 2026-08-29

### Changed

- LightPanda now runs inside the sandbox rather than on the host, so browser automation is subject to the same isolation as the rest of the sandboxed environment ([#230](https://github.com/webcoyote/sandvault/pull/230))

### Fixed

- iOS device commands no longer hang indefinitely: `iosef` calls now time out, and device description polling is bounded by wall-clock time ([#231](https://github.com/webcoyote/sandvault/pull/231))

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.29.0] - 2026-08-23

### Added

- `sv-agentsview-setup` now re-syncs on every run once enabled, so an agent added after you opt in gets its mirror symlink and config scan path automatically instead of staying invisible in agentsview until you reset the opt-in state. ([#225](https://github.com/webcoyote/sandvault/pull/225)) — thanks @jesserobbins!
- Extra SSH keys can be placed in an `authorized_keys.d` directory, letting other machines reach the sandvault user without hand-editing a generated file that upgrades overwrite. `authorized_keys` is regenerated in full each run, so deleting a file revokes that key, and private keys are rejected outright. ([#224](https://github.com/webcoyote/sandvault/pull/224)) — thanks @MikeMcQuaid!

### Fixed

- Declining a newly detected agent is now remembered permanently instead of re-prompting on every run. Remove the entry from the decline list to re-enable it. ([#225](https://github.com/webcoyote/sandvault/pull/225)) — thanks @jesserobbins!
- A hand-written scalar value for a managed key in `config.toml` is no longer silently split into individual characters and rewritten; the setup now fails with a clear error instead. ([#225](https://github.com/webcoyote/sandvault/pull/225)) — thanks @jesserobbins!
- A hand-edited `config.toml` no longer triggers a confirmation prompt on every run. Comment and formatting differences introduced by the config writer are ignored, and only genuinely missing scan paths prompt. ([#225](https://github.com/webcoyote/sandvault/pull/225)) — thanks @jesserobbins!
- Configuration errors during agentsview setup are now surfaced rather than being swallowed, and error messages name the script that failed. ([#225](https://github.com/webcoyote/sandvault/pull/225)) — thanks @jesserobbins!

### Thanks to 4 contributors!

- [@gamepoet](https://github.com/gamepoet)
- [@jesserobbins](https://github.com/jesserobbins)
- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@webcoyote](https://github.com/webcoyote)

## [Unreleased]

### Added

- Extra SSH public keys can now reach the sandvault user: drop one key per file into `~/.config/codeofhonor/sandvault/authorized_keys.d/` and run `sv build`. The sandvault user's `authorized_keys` is regenerated from that directory on every run, so deleting a file revokes the key. Files that are not public keys are ignored with a warning, and a private key left there stops the build instead of being copied into the sandbox.

## [1.28.0] - 2026-08-19

### Fixed

- Sandvault now uses a dedicated keychain instead of the macOS-managed login keychain, so the security prompt no longer appears when starting an agent after a reboot on macOS 26.6 and later. Existing Claude Code credentials are migrated automatically; when migration isn't possible, the old keychain is preserved and recovery instructions are provided. Keychain setup failures are also no longer fatal, so a keychain problem can't break the rest of the session. ([#208](https://github.com/webcoyote/sandvault/pull/208)) — thanks @MikeMcQuaid! — thanks @remonh87 for the report!

### Thanks to 3 contributors!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@remonh87](https://github.com/remonh87)
- [@webcoyote](https://github.com/webcoyote)

## [1.27.0] - 2026-08-04

### Added

- `pi` is now a first-class agent: run it with `sv pi` (or `sv p`), alongside the other supported agents. Installs via Homebrew by default, or from npm with `sv -N pi`, and is covered by `--fix-permissions`. Sessions are exported to agentsview, with pi's credential directory kept private. ([#182](https://github.com/webcoyote/sandvault/pull/182)) — thanks @jesserobbins!

### Thanks to 1 contributor!

- [@jesserobbins](https://github.com/jesserobbins)

## [1.26.0] - 2026-07-30

### Fixed

- Support login keychains on macOS 27 ([#191](https://github.com/webcoyote/sandvault/pull/191)) — thanks @MikeMcQuaid!

### Thanks to 1 contributor!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)

## [1.25.0] - 2026-07-26

### Added

- Add `sv-visible-browser` script to launch a visible Chrome browser session ([#187](https://github.com/webcoyote/sandvault/pull/187))

### Changed

- Connect SSH mode to the loopback interface instead of `$HOSTNAME` for more reliable local connections ([#189](https://github.com/webcoyote/sandvault/pull/189))
- Make deploy keys opt-in via the `-k`/`-w` flags in `sv-clone` ([#186](https://github.com/webcoyote/sandvault/pull/186)) — thanks @jesserobbins!

### Fixed

- Fix deploy-key origin rewrite on machines configured with git `insteadOf` ([#186](https://github.com/webcoyote/sandvault/pull/186))
- Fix deploy-key read-only detection in `sv-clone` ([#186](https://github.com/webcoyote/sandvault/pull/186))

### Removed

- Remove `--no-sandbox` flag for the Chrome browser so it now runs sandboxed ([#187](https://github.com/webcoyote/sandvault/pull/187))

### Thanks to 2 contributors!

- [@jesserobbins](https://github.com/jesserobbins)
- [@webcoyote](https://github.com/webcoyote)

## [1.24.0] - 2026-07-12

### Fixed

- Trust shared Sandvault repos so Git stays usable in shared-workspace worktrees owned by the host user ([#179](https://github.com/webcoyote/sandvault/pull/179)) — thanks @MikeMcQuaid!
- Warm up the Codex host helper so it works even when Homebrew leaves it quarantined ([#183](https://github.com/webcoyote/sandvault/pull/183)) - thanks @MikeMcQuaid!

### Thanks to 1 contributor!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)

## [1.23.0] - 2026-06-27

### Fixed

- Neutralize guest-controlled git configuration during host-side git operations, closing a sandbox-escape vector ([#177](https://github.com/webcoyote/sandvault/pull/177))
- Prevent symlink-following during the buildhome ownership change, closing a root privilege-escalation vector ([#176](https://github.com/webcoyote/sandvault/pull/176))

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.22.0] - 2026-06-10

### Fixed

- Detect and warn when a legacy `~/node_modules` directory from older native-install wrappers is present, with manual cleanup steps. The stale directory disables Bun auto-install across the sandbox home, and updating wrappers alone doesn't remove it. ([#169](https://github.com/webcoyote/sandvault/pull/169))
- Switch the `codex` and `gemini` native-install wrappers to use `$HOME/.local` as the npm prefix instead of `$HOME/node_modules`, so installs land in `~/.local/bin` and `~/.local/lib/node_modules` and no longer trip Bun's ancestor-`node_modules` heuristic that silently disabled auto-install across `$HOME`. — thanks @nichenke!

### Thanks to 2 contributors!

- [@nichenke](https://github.com/nichenke)
- [@webcoyote](https://github.com/webcoyote)

## [1.21.0] - 2026-06-10

### Changed

- `sv-clone` now uses a read-only token instead of a read-write token, reducing the blast radius of a rogue agent. ([#170](https://github.com/webcoyote/sandvault/pull/170)) — thanks @MikeMcQuaid!

### Thanks to 1 contributor!

- [@MikeMcQuaid](https://github.com/MikeMcQuaid)

## [1.20.0] - 2026-05-11

### Changed
- Harden sandbox-exec profile: block raw disk and packet-capture devices, default-deny reads under `/Users` (re-allowing only sandvault paths), and deny `/Library/Keychains` ([#164](https://github.com/webcoyote/sandvault/pull/164))

### Fixed
- Preserve unicode characters in agent arguments, fix unbound variable expansion in bash 3.2, and simplify shell-variable expansion ([#167](https://github.com/webcoyote/sandvault/pull/167)) — thanks @MikeMcQuaid!
- Error trap handler now writes to stderr instead of stdout ([#165](https://github.com/webcoyote/sandvault/pull/165))

### Thanks to 2 contributors!
- [@MikeMcQuaid](https://github.com/MikeMcQuaid)
- [@webcoyote](https://github.com/webcoyote)

## [1.19.0] - 2026-05-09

### Added

- Support for the lightpanda browser ([#161](https://github.com/webcoyote/sandvault/pull/161))

### Fixed

- Translate quoted `~` in `INITIAL_DIR` to the sandvault user's `HOME` ([#159](https://github.com/webcoyote/sandvault/pull/159)) — thanks @vre!

### Thanks to 2 contributors!

- [@vre](https://github.com/vre)
- [@webcoyote](https://github.com/webcoyote)

## [1.18.0] - 2026-05-04

### Fixed
- Fix Homebrew path resolution when sandvault is installed under `libexec` instead of `Cellar`

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.17.0] - 2026-05-04

### Added
- Export agentsview from sandbox sessions using `sv-agentsview-setup` script.

### Changed
- Consolidate sandvault-managed paths under `_sandvault/`.

### Fixed
- File ACLs avoid creating files with the `execute` ACL.

### Thanks to 2 contributors!

- [@jesserobbins](https://github.com/jesserobbins) — thanks @jesserobbins!
- [@webcoyote](https://github.com/webcoyote)

## [1.16.0] - 2026-05-01

### Fixed

- Fix race condition and collisions in UID/GID allocation.
- Fix TOCTOU vulnerability in shared-folder ACL removal.

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.15.0] - 2026-04-29

### Fixed

- Fix stdin processing so piped input works with `sv claude` and `sv claude -- -p` (e.g. `echo "who are you" | sv claude`) ([#150](https://github.com/webcoyote/sandvault/pull/150))

### Removed

- Remove `.zlogin`/`.zlogout` handling in the guest shell ([#150](https://github.com/webcoyote/sandvault/pull/150))

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.14.0] - 2026-04-28

### Added
- New `/sv` Claude Code skill for handing tasks off to sandvault directly from Claude Code
- Automatically symlink `/sv` skill into `~/.claude/skills/` during `sv build`

### Fixed
- Fix skill link path and improve terminal compatibility for the `/sv` skill

### Thanks to 2 contributors!

- [@jesserobbins](https://github.com/jesserobbins)
- [@webcoyote](https://github.com/webcoyote)

## [1.13.0] - 2026-04-26

### Changed
- Increase Chrome and iOS simulator startup timeouts from 5s to 15s for better reliability under load ([#146](https://github.com/webcoyote/sandvault/pull/146))
- Pass `VERBOSE` environment variable through to sandboxed processes

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.12.0] - 2026-04-25

### Added
- Per-repo SSH deploy keys for cloned repositories ([#139](https://github.com/webcoyote/sandvault/pull/139)) — thanks @jesserobbins!
- Standalone `sv-clone` command, replacing the previous `sv --clone` flag ([#133](https://github.com/webcoyote/sandvault/pull/133))

### Fixed
- File permissions no longer set the execute bit on regular files in the vault ([#141](https://github.com/webcoyote/sandvault/pull/141))

### Thanks to 2 contributors!

- [@jesserobbins](https://github.com/jesserobbins)
- [@webcoyote](https://github.com/webcoyote)

## [1.11.0] - 2026-04-20

### Fixed
- Fix sandvault user not being added to the sandvault group ([#131](https://github.com/webcoyote/sandvault/pull/131))

### Changed
- Forward `COLORTERM` environment variable from host to guest for proper terminal color support ([#131](https://github.com/webcoyote/sandvault/pull/131))

### Thanks to 2 contributors!

- [@jesserobbins](https://github.com/jesserobbins)
- [@webcoyote](https://github.com/webcoyote)

## [1.10.0] - 2026-04-18

### Added
- Add iOS simulator sandbox bridge for testing mobile apps in the sandbox ([#118](https://github.com/webcoyote/sandvault/pull/118))
- Make AI agents aware of browser endpoint via `SV_BROWSER_ENDPOINT` ([#116](https://github.com/webcoyote/sandvault/pull/116))

### Changed
- Move session files to shared workspace so users can manage them directly ([#117](https://github.com/webcoyote/sandvault/pull/117))

### Fixed
- Suppress noisy process-kill notifications when closing iOS simulator or browser sessions ([#118](https://github.com/webcoyote/sandvault/pull/118))

### Thanks to 1 contributor!

- [@webcoyote](https://github.com/webcoyote)

## [1.9.0] - 2026-04-15

### Fixed
- Fix Ensure AI agent scripts write header to stderr ([#114](https://github.com/webcoyote/sandvault/pull/114)) — thanks @nichenke!

### Thanks to 2 contributors!

- [@nichenke](https://github.com/nichenke)
- [@webcoyote](https://github.com/webcoyote)

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
