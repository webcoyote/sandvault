---
id: TASK-0020
title: 'sv2: test `install` over a `/Users/Shared/sv2` that belongs to another user'
status: To Do
assignee: []
created_date: '2026-11-27T20:24:47Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: low
type: task
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`/Users/Shared` is writable by everyone, so another host user can create `sv2` there before `sv2 install` runs ("A taken workspace root" under "Milestone 1"). `install` replaces an empty directory or a symbolic link and refuses anything else with the owner's name. `sv2/tests/root-checks` covers the "taken name" cases, but it creates the directory as the user who runs the check, so it exercises the same code without ever meeting a directory that really belongs to a different account. A real second owner is what other users on a shared Mac would produce. root-checks deletes every profile on the Mac and is run by hand.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A check creates `/Users/Shared/sv2` owned by a second, throwaway host account (empty directory, symbolic link, and directory with a file), runs `sv2 install`, and asserts the result for each
- [ ] #2 The refusal names the other account as owner, and nothing of that account is opened, followed, or removed
- [ ] #3 The throwaway account and its directory are removed afterwards, also when the check fails
<!-- AC:END -->
