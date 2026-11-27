---
id: TASK-0021
title: 'sv2: find out what opendirectoryd does after the helper kills a waiting dscl'
status: To Do
assignee: []
created_date: '2026-11-27T20:24:47Z'
labels:
  - sv2
dependencies: []
references:
  - TASK-0015
  - TASK-0018
documentation:
  - docs/SANDVAULT2.md
priority: low
type: spike
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Deleting an account, or changing an existing account's home directory, makes `opendirectoryd` ask TCC for the "administer your computer" permission. Since helper version 6 the helper gives each `dscl`/`dseditgroup` write 60 seconds and then kills the tool (see "Account changes need a macOS privacy permission" in the design doc). Killing the client does not necessarily cancel the request inside `opendirectoryd`: it may still complete later, fail, or hold state, which matters for the verb being run again and for the registry, which the helper updates on the assumption that the write did not happen. Nobody has looked.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 With the prompt left unanswered and the tool killed at 60 seconds, it is recorded whether the account change later takes effect, is dropped, or completes when the prompt is answered afterwards
- [ ] #2 Running the same verb again afterwards is shown to end in a consistent registry and directory, or the inconsistency is filed as a bug
- [ ] #3 The result is written into the design doc next to the 60-second limit
<!-- AC:END -->
