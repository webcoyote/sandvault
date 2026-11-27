---
id: TASK-0007
title: 'sv2: iOS bridge'
status: To Do
assignee: []
created_date: '2026-11-27T17:59:06Z'
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
- [ ] #1 A session started with the opt-in flag can install, launch and drive a simulator app; a session without it cannot reach a bridge
- [ ] #2 Every request needs a per-session token and passes a `Host` header check; a request without the token from another local process is refused
- [ ] #3 An install is refused for a path outside this profile workspace, and the bundle is copied into a host-only directory without following links before the copy is installed
- [ ] #4 The scratch directory is created and removed by the sandbox user or reached through a directory handle, never by path through sandbox-writable parents
- [ ] #5 Every argument is validated and passed so it cannot be read as an option
- [ ] #6 The bridge runs as the host user under `sv2`, never from the root helper
- [ ] #7 A planted-attack test covers a symlinked bundle and a symlinked scratch directory, verified from the host side
- [ ] #8 README-SV2.md states exactly what trust the flag grants
<!-- AC:END -->
