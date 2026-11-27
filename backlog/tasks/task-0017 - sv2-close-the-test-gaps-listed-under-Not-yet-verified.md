---
id: TASK-0017
title: 'sv2: close the test gaps listed under "Not yet verified"'
status: To Do
assignee: []
created_date: '2026-11-27T17:59:07Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: medium
type: task
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The design doc lists behaviour that was built but has not run or is not covered by a test. This task covers the ones that need no design decision: (1) an account that stops the kill child on purpose, the path that continues it; (2) `^Z` typed while sudo is still starting, before the helper runs, which can leave that sudo waiting for good; (3) on a Mac where Remote Login is open to all users, that the suite login attempt succeeds against a build without the sshd drop-in, which shows the drop-in is what refuses it (the development Mac limits Remote Login to listed users, so the attempt is refused either way); (4) the quarantine message, which appears only when `sv2` installs a cask that is not notarized; (5) the headless browser probe, which the suite skips because its throwaway profile has no Playwright browsers; (6) what `dseditgroup` prints when the privacy permission is refused, since different wording would lose the hint. Each is independent and may be split out.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The kill-child-stopped path has a test, or the doc says why it cannot have one
- [ ] #2 `^Z` during sudo start is reproduced and either fixed or recorded as a sudo behaviour with what the user should do
- [ ] #3 The SSH login check is run on a Mac with Remote Login open to all users, against builds with and without the drop-in
- [ ] #4 The quarantine message is seen from a real install of a cask that is not notarized
- [ ] #5 The integration suite runs the headless browser probe without a manual step
- [ ] #6 The refusal from `dseditgroup` is seen, and the helper names the permission for it
- [ ] #7 Each item is removed from the doc when done, as the doc asks
<!-- AC:END -->
