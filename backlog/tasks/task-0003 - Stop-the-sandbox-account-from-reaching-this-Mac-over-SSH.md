---
id: TASK-0003
title: Stop the sandbox account from reaching this Mac over SSH
status: To Do
assignee: []
created_date: '2026-11-27T17:48:06Z'
updated_date: '2026-11-27T19:14:01Z'
labels:
  - sandbox-hardening
dependencies: []
references:
  - sv
  - sv2
documentation:
  - docs/ssh-lockdown.md
  - docs/security-review-2026-11-24.md
type: enhancement
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Port 22 on this Mac is reachable from inside the sandbox. When Remote Login is on, the sandbox user can SSH into its own account and get a session with no `sandbox-exec` profile, which removes the `/Users`, `/Volumes`, and Keychains read denies and the mach-lookup restrictions: the account's `authorized_keys` is in its own writable home, and `sv` adds the account to `com.apple.access_ssh` on every rebuild whether or not `--ssh` is used. A login to the host account has no known route today, but any future bug that writes one line to the host's `authorized_keys` would become a full login. `docs/ssh-lockdown.md` is an unimplemented proposal with layered options and a suggested order; items 8 and 9 of the security review give the findings. TODO.md suggested turning Remote Login off or denying the sandbox user in sshd. Converted from TODO.md, "Stricter sandbox" item 3.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A process running as the sandbox user cannot obtain an SSH session on this Mac as the sandbox account or as the host account
- [ ] #2 The host user can still connect to the sandbox account with `sv --ssh`
- [ ] #3 The sandbox account is in `com.apple.access_ssh` only when SSH mode is used
- [ ] #4 git over SSH to remote hosts still works from inside the sandbox
- [ ] #5 Tests cover the blocked sandbox-to-host connection and the working host-to-sandbox connection
- [ ] #6 README states what is blocked and what users with Remote Login off need to do, if anything
<!-- AC:END -->
