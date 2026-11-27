---
id: TASK-0022
title: 'sv2: move from the sudoers helper to a launchd daemon'
status: To Do
assignee: []
created_date: '2026-11-27T20:24:47Z'
labels:
  - sv2
dependencies:
  - TASK-0015
documentation:
  - docs/SANDVAULT2.md
priority: low
type: feature
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
sv2 runs its root helper through one sudoers line. The design (open item 3, "Privileged helper") calls a launchd daemon the stronger design for an untrusted caller, because the caller can only send it messages, and the helper's verbs were designed as a wire protocol so they can move behind one. It was put off because it needs a terminal proxy, a versioned protocol, and a root socket that sandboxed processes must be kept away from. The likely gain now is the "administer your computer" prompt: under sudo macOS charges account changes to the user's terminal app, and a daemon has no such app behind it. TASK-0015 measures whether that is so. Scheduled last in the priority order. A daemon needs the same `launchctl asuser` step as `launch` does today.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Whether to build it is decided from the TASK-0015 measurement and recorded in the design doc, with which verbs move (at least account creation and deletion if the prompt goes away; `launch` may stay on sudo)
- [ ] #2 If built: the daemon's socket cannot be reached from a sandbox session, shown by a test that tries from one
- [ ] #3 If built: install, upgrade from the sudoers helper, and uninstall remove the old artifacts, and the integration suite passes against the daemon
<!-- AC:END -->
