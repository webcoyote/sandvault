---
id: TASK-0004
title: 'sv2: decide and implement a position on localhost networking and local port 22'
status: To Do
assignee: []
created_date: '2026-11-27T17:59:06Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/ssh-lockdown.md
  - docs/SANDVAULT2.md
priority: high
type: enhancement
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The sv2 design says the sandbox profile needs "a stated position on localhost networking. At minimum, deny local port 22", and the work was deferred on 2026-11-26. Today a session can reach services listening on this Mac: host services on localhost, debugging ports of browsers in other sessions, and anything else bound locally. The sshd drop-in already refuses logins to sandbox accounts, so port 22 here is about reaching the host account and defence in depth. Known constraints from `docs/ssh-lockdown.md`: a rule denying port 22 to every destination breaks git over SSH to remote hosts, and whether a loopback rule also covers the Mac own LAN address has not been measured. `--chrome` and `--lightpanda` sessions need their own browser endpoint on localhost to keep working. Source: "Sandbox profile", "Priority order" item 4 and "Features not in milestone 1" in the design doc.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The design doc states which local destinations and ports a session may connect to, and why
- [ ] #2 A session cannot connect to port 22 on this Mac, by loopback or by the Mac own LAN address, measured as the sandbox user under the rendered profile
- [ ] #3 git over SSH to a remote host still works from a session
- [ ] #4 Sessions started with `--chrome` or `--lightpanda` still reach their browser endpoint
- [ ] #5 The integration suite has a check for each rule added, and the profile artifact version is bumped so installed machines re-render
<!-- AC:END -->
