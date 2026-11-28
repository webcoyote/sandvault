---
id: TASK-0014
title: 'sv2: audit cron, at and Screen Sharing as routes out of the sandbox'
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
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The profile confines only processes that sandvault starts. "Closing other routes out of the sandbox" in the design doc closes SSH and ends: "Other routes to audit before release: cron, `at`, launchd agents, Screen Sharing." launchd agents have their own task; the other three have not been looked at. Each is a way the sandbox account might get a process running outside the profile.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 For each of cron, `at` and Screen Sharing: whether the sandbox account can use it to run a process outside the profile is measured from inside a session and verified from the host side
- [ ] #2 Each route found open is closed or has a follow-up task
- [ ] #3 The design doc records the result for each route, and the closed routes are checks in the integration suite
<!-- AC:END -->
