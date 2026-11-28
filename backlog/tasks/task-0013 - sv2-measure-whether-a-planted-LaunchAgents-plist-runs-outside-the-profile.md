---
id: TASK-0013
title: 'sv2: measure whether a planted LaunchAgents plist runs outside the profile'
status: To Do
assignee: []
created_date: '2026-11-27T17:59:07Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: medium
type: spike
ordinal: 13000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The one entry under "Security gaps" in the design doc, and open item 7. A job in the sandbox account launchd domain would run as the sandbox user without the `sandbox-exec` profile. Measured on macOS 26.7 from inside a session: `launchctl submit`, `bootstrap user/<uid>`, `bootstrap gui/<uid>` and `load` all fail. Not measured: whether a plist the sandbox plants in `~/Library/LaunchAgents` (with `LimitLoadToSessionType` `Background`, the domain session type) loads when the domain is next created. The domain outlives the session, so the test needs `launchctl bootout user/<uid>` as root, or a reboot, before the next session. The direct routes are measured but not yet a test. Put off on 2026-11-26.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The planted-plist case is measured and the result recorded under open item 7 with the macOS version
- [ ] #2 If the plist loads, a fix is made or a follow-up task created, and the "Security gaps" entry says which
- [ ] #3 The direct routes already measured (`submit`, `bootstrap`, `load`) are checks in the integration suite
<!-- AC:END -->
