---
id: TASK-0007
title: 'sv2: iOS bridge'
status: Done
assignee:
  - '@claude'
created_date: '2026-11-27T17:59:06Z'
updated_date: '2026-11-28T02:55:01Z'
labels:
  - sv2
dependencies:
  - TASK-0006
documentation:
  - docs/ios-simulator-tools.md
  - docs/security-review-2026-11-24.md
  - docs/SANDVAULT2.md
priority: high
type: feature
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
v1 lets a sandboxed agent drive the iOS Simulator through an HTTP bridge that runs as the host user; sv2 has no equivalent, so iOS app testing is not possible in an sv2 session. The design decided to redesign the bridge rather than drop it. It is one of the two stated exceptions to the sv2 rule that the host never works inside sandbox-writable directories, so it is opt-in and held to the requirements in the "iOS bridge" section, which answer findings 6 and 15 of the security review. `docs/ios-simulator-tools.md` explains the simctl and iosef split the v1 bridge uses. Depends on the simulator trust measurement for the README wording.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A session started with the opt-in flag can install, launch and drive a simulator app; a session without it cannot reach a bridge
- [x] #2 Every request needs a per-session token and passes a `Host` header check; a request without the token from another local process is refused
- [x] #3 An install is refused for a path outside this profile workspace, and the bundle is copied into a host-only directory without following links before the copy is installed
- [x] #4 The scratch directory is created and removed by the sandbox user or reached through a directory handle, never by path through sandbox-writable parents
- [x] #5 Every argument is validated and passed so it cannot be read as an option
- [x] #6 The bridge runs as the host user under `sv2`, never from the root helper
- [x] #7 A planted-attack test covers a symlinked bundle and a symlinked scratch directory, verified from the host side
- [x] #8 README-SV2.md states exactly what trust the flag grants
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. New module sv2/crates/cli/src/ios.rs: simulator lifecycle (simctl create/boot/shutdown/delete, newest iPhone and runtime), the HTTP bridge on 127.0.0.1 (threads inside sv2, host user), and the install copy.
2. Every request: Host header must be 127.0.0.1:PORT or localhost:PORT, and Authorization: Bearer TOKEN (compared in constant time). No token in a URL.
3. Token: 32 random bytes from /dev/urandom. It reaches the sandbox on stdin of a short session that writes it to a 0600 file in the sandbox home; the launcher reads it, deletes the file and exports SV_IOS_BRIDGE_TOKEN. Never on a command line (ps and sudo's log show those).
4. /install: app_path must be inside this profile's workspace. The workspace is opened by path (its parents are root's), then each component with openat O_NOFOLLOW|O_DIRECTORY; the tree is copied through descriptors into a host-only mkdtemp directory; a link or special file anywhere refuses the install; only the copy is given to simctl.
5. Screenshots are returned in the response body, so the host writes nothing in sandbox territory. The scratch directory (SV_IOS_SCRATCH_DIR) is made and removed by the launcher, as the sandbox user. iosef runs with --local in the host-only directory.
6. Arguments: numbers, bundle ids, URLs, selectors validated; option values passed as --name=value; tools run by absolute path, no shell.
7. sv2 run --ios: spawn the session and wait (no exec), then shut down and delete the simulator and remove the host-only directory. herdr refuses --ios like a browser. Agents with a system_prompt are told.
8. Tests: unit tests with stand-ins for simctl and iosef, including planted links (bundle, a parent, inside the bundle, scratch directory) judged from the host side; an integration check that a session without --ios has no endpoint; an end-to-end run against a real simulator by hand.
9. README-SV2.md and docs/SANDVAULT2.md. The README's trust statement depends on TASK-0006, which is not done.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Built in sv2/crates/cli/src/ios.rs (bridge, simulator lifecycle, install copy), with guest/launch.sh (KIND ios FILE PORT), args.rs (--ios), main.rs (run_with_ios, token hand-over, signals), agents/herdr/run.sh. The helper and common crates are untouched; the installed helper stayed byte-identical to the build.

Evidence per criterion:
1. tests/integration, iOS section, against a real simulator (macOS 26.7.3, Xcode 27.2): install 200, launch 200, describe shows the app, tap changes the button, both screenshots arrive; a session without --ios has neither variable, and no listener is started.
2. Unit test a_request_needs_the_servers_name_and_the_sessions_token (401 without or with a wrong token, 403 under another Host, no tool run); integration: curl from the host without the token gets 401, the token is on no command line in ps.
3. Unit tests an_install_from_outside_the_workspace_is_refused, an_install_gives_simctl_a_copy_of_the_bundle; integration: the host's bundle outside the workspace gets 400.
4. Screenshots are the response body; SV_IOS_SCRATCH_DIR is made and removed by the launcher (integration: owned by the sandbox account, mode 700, gone after the session). Unit test the_host_writes_nothing_through_a_planted_scratch_directory.
5. Unit test arguments_are_validated_and_cannot_be_options; iosef checked with --name=--help.
6. The bridge is threads of sv2; no file of the helper changed.
7. Unit tests with planted links (bundle, parent, inside, FIFO; scratch and .iosef names), judged by what the host holds; integration looks at simctl listapps from the host while the session waits. Each guard was removed in turn and its test failed (token/Host, tools' directory, descriptor walk, no-follow copy, install of the copy).
8. README-SV2.md, 'An iOS simulator'. The statement rests on a measurement made here: an app installed through the bridge ran as uid 501, listed the host home and wrote a file in it.

Not verified: scripts/validate as a whole. tests/integration stops at two profile-removal checks ('^Z: does not suspend a removal either', 'remove: succeeds against a fork storm') because macOS asks whether the terminal may administer the computer and nobody answered; unrelated to this task (TASK-0018). The iOS section passed 29 of 29, twice. CI has not run it.

Decisions made while building, for review: screenshots in the response instead of a shared directory; a bundle with any symbolic link is refused rather than copied with its links; no --ios-gui; the token goes through a file in the sandbox home written by a session of its own.

Superseded the same day by TASK-0025: the bridge was removed and the simulator runs in the sandbox account. The criteria above describe code that is no longer in the tree.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
sv2 run --ios gives a session an iOS simulator and an HTTP bridge to it. The bridge runs inside sv2 as the host user; every request needs the session token and a matching Host; installs are copied out of the workspace through descriptors, without following links, and only the copy is installed; the host writes nothing in sandbox territory; arguments are validated and passed as single words. sv2 stays alive for the session and deletes the simulator afterwards.

Verified by 20 new unit tests with stand-in tools, mutation checks of each guard, and 29 integration checks against a real simulator. Measured on the way: a simulator app runs as the host user with access to the host home, so --ios grants code execution as the host user; README-SV2.md and the banner say so, and docs/SANDVAULT2.md records the measurement (this is TASK-0006's question; that task is left open).

Risks and follow-ups: the integration section will run on CI for the first time; the guest launcher is 593 bytes under its 16 KB limit; ~/.config/sandvault2/agents needs scripts/sv2-dev-agents rerun for herdr's refusal of --ios to take effect on a development Mac.
<!-- SECTION:FINAL_SUMMARY:END -->
