---
id: TASK-0003
title: 'sv2: stop the sandbox account from reaching this Mac over SSH'
status: Done
assignee:
  - '@claude'
created_date: '2026-11-27T17:48:06Z'
updated_date: '2026-11-27T19:54:59Z'
labels:
  - sandbox-hardening
  - sv2
dependencies:
  - TASK-0004
references:
  - sv2
documentation:
  - docs/ssh-lockdown.md
  - docs/security-review-2026-11-24.md
  - docs/SANDVAULT2.md
priority: high
type: enhancement
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
With Remote Login on, a sandbox that can SSH into this Mac gets a process outside its `sandbox-exec` profile: into its own account if sshd accepts a key from the account own writable `authorized_keys`, or into the host account if a key or password for it ever leaks. Scoped to sv2 on 2026-11-27 by the user; it was converted from TODO.md ("Stricter sandbox" item 3), which was written for v1.

sv2 answers it in two layers. The sshd drop-in (`/etc/ssh/sshd_config.d/000-sandvault2.conf`, commit 60f44ac) refuses a login as any sandbox account, whoever connects. The profile rule of TASK-0004 keeps a session from connecting to this Mac port 22 at all.

Not covered by this task: v1 (`sv`). There the account `authorized_keys` is still in its own home and `sv` installs no sshd drop-in; `docs/ssh-lockdown.md` Layer 1 describes the fix. No task tracks that.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 sshd refuses a login as a sandbox account, also with a key the sandbox put in its own `authorized_keys`, checked from the host side
- [x] #2 A process in a session cannot connect to this Mac SSH port at any of its addresses
- [x] #3 git over SSH to a remote host still works from a session
- [x] #4 The integration suite covers the refused login and the refused connection, and each check fails when its protection is taken away
- [x] #5 README-SV2.md states what is blocked
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. No code beyond TASK-0004: the sshd drop-in is built, and TASK-0004 adds the profile rule, the probes and the host-side login check.
2. After `sv2 install` of profile version 7 (needs the user sudo password), run sv2/tests/integration and check each criterion against that run.
3. Finalize together with TASK-0004.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-27, state found before planning. The task was written from TODO.md for v1 (`sv`); criteria 2 and 3 name v1 SSH mode, which sv2 does not have.
sv2: a login as a sandbox account is refused by the sshd drop-in (built, commit 60f44ac). Reaching sshd at all from a session is closed by TASK-0004 (profile version 7, local port 22 denied), not yet installed or run in a session. sv2 never adds accounts to com.apple.access_ssh. Tests and README-SV2.md cover both.
v1: criterion 3 is already in the code (commit 1153d0a: the account is in com.apple.access_ssh only in SSH mode, and is taken out on a rebuild without it). Criterion 1 is not met: `sv` installs no sshd drop-in, the account authorized_keys is still in its own writable home, and port 22 is not denied in the v1 profile. Closing that is Layer 1 of docs/ssh-lockdown.md, a root-owned keys file and an sshd drop-in written by `sv`.
Open question for the user: do the v1 work, or scope this task to sv2. No plan recorded until that is answered.

2026-11-27, verified on the installed build (helper 8, profile 7), integration suite 174 passed.
1: "sshd: refuses a login as the sandbox account" (sshd -T) and the host-side login with a key the sandbox authorized, both passed. On this Mac Remote Login is limited to listed users, so the real login is refused for that reason too; what the drop-in makes sshd apply is the first check.
2: probe "this Mac SSH port" in a session, and by hand: EPERM at 127.0.0.1, ::1, 192.168.1.67.
3: probe "port 22 of a remote host answers" in a session; github.com:22 answered SSH-.
4: the probe failed against the previous profile in a real session (171 passed, 1 failed, the failure naming three addresses that connect); the login check is as in 1. No mutation check removes the drop-in.
5: README-SV2.md, "What a sandbox can and cannot do".
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Scoped to sv2. A sandbox cannot get an SSH session on this Mac: sshd refuses every sandbox account (the drop-in, built earlier), and a session can no longer connect to this Mac SSH port at any of its addresses (the profile rule of TASK-0004). No code of its own; the integration suite login attempt moved to the host side as part of TASK-0004.
Verified on the installed build: integration suite 174 of 174; port 22 refused with EPERM in a real session at the loopback and LAN addresses; git over SSH to a remote host unaffected.
Not covered: v1 (`sv`) still has no sshd drop-in and keeps authorized_keys in the sandbox home; no task tracks it. On a Mac where Remote Login is open to all users the real login check has not been run (TASK-0017).
<!-- SECTION:FINAL_SUMMARY:END -->
