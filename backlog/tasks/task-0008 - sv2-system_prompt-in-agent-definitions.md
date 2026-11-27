---
id: TASK-0008
title: 'sv2: system_prompt in agent definitions'
status: To Do
assignee: []
created_date: '2026-11-27T17:59:06Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: medium
type: feature
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The agent definition format in the design doc has a `system_prompt` key under `[run]`: text that sv2 adds to an agent system prompt, in whatever way that agent takes one (for example a flag such as `--append-system-prompt`). It is in the design ("Definition format") and not built, so sv2 has no way to tell an agent anything about the session it is in. Telling an agent that its session has a browser depends on this.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A definition can say how its agent takes extra system prompt text, and sv2 passes the text that way
- [ ] #2 The text is not injected into subcommands listed in `no_inject`
- [ ] #3 A definition without the key runs as it does today
- [ ] #4 The injected flag is checked against the agent real `--help`, as the review guidelines require for injected flags
- [ ] #5 `sv2 agent new` template shows the key, and unit tests cover parsing and the argument vector
<!-- AC:END -->
