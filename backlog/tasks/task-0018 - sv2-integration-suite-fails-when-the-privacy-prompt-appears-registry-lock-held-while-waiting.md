---
id: TASK-0018
title: >-
  sv2: integration suite fails when the privacy prompt appears; registry lock
  held while waiting
status: To Do
assignee: []
created_date: '2026-11-27T17:59:07Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: medium
type: bug
ordinal: 18000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In 9 runs in a row on 2026-11-26 the integration suite failed once: `dscl . -delete` of an account waited on the "administer your computer" permission with the helper holding the registry lock, so that removal and the next one failed. Which removal meets it is chance. Since helper version 6 the removal fails after 60 seconds and names the permission, so the suite fails sooner and says why, but it still fails, since nobody answers the prompt. Open item 10 leaves undecided whether the helper should release the registry lock while it waits; today every other profile operation is blocked for up to 60 seconds. A daemon may remove the prompt altogether (see the daemon measurement task), but that is scheduled last.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A suite run in which the prompt appears does not report a product failure: it retries, skips with a named reason, or avoids the prompt
- [ ] #2 A decision on releasing the registry lock during the wait is recorded under open item 10, and implemented if the decision is to release it
- [ ] #3 If the lock is released, a test shows a second profile operation completes while the first waits, and that an interrupted removal can be run again
- [ ] #4 The doc entry "The integration suite fails now and then for a known reason" is updated or removed
<!-- AC:END -->
