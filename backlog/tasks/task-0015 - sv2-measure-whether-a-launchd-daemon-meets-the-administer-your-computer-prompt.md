---
id: TASK-0015
title: >-
  sv2: measure whether a launchd daemon meets the "administer your computer"
  prompt
status: To Do
assignee: []
created_date: '2026-11-27T17:59:07Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: low
type: spike
ordinal: 15000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Deleting an account, and changing an existing account home directory, make `opendirectoryd` ask TCC for `kTCCServiceSystemPolicySysAdminFiles`. Under sudo, macOS charges this to the terminal app the user ran `sv2` from, asks again every few minutes, and the helper waits up to 60 seconds with the registry lock held. This affects `profile remove` and `uninstall`. A daemon started by launchd has no such app behind it, and this is now the main reason to move from the sudoers helper to a daemon (open items 3 and 10). Before building the daemon, the doc asks for a measurement with a throwaway LaunchDaemon that deletes a test account: is the request granted, refused, or still shown to someone? Open item 10 also lists smaller unknowns: why requests were granted silently and later prompted, exactly which operations ask, and whether v1 has the same dependency. Put off on 2026-11-26; the daemon is scheduled last.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A throwaway LaunchDaemon that deletes a test account is run, and whether TCC grants, refuses or prompts is recorded under open item 3
- [ ] #2 Which directory operations ask for the permission is recorded (`IsHidden` and group membership included)
- [ ] #3 The doc records a decision: move account creation and deletion into a daemon, or not, with a follow-up task if so
<!-- AC:END -->
