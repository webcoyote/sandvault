---
id: TASK-0010
title: 'sv2: wrapper scripts in agent definitions, and definitions for muse and herdr'
status: To Do
assignee: []
created_date: '2026-11-27T17:59:06Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: low
type: feature
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The definition format allows an optional `wrapper` script under `[run]` that runs inside the sandbox, for agents that do not fit the data-only run description. It is in the design and not built. v1 ships `muse` and `herdr`; sv2 has built-ins only for claude, codex, gemini, opencode and pi, so users of those two cannot move to sv2. Put off on 2026-11-26 until asked for ("Priority order" item 5). Whether muse or herdr need a wrapper is not known; herdr is a terminal multiplexer in v1, so read "Terminal multiplexers" first.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A definition can name a wrapper script, which runs inside the sandbox as the sandbox user and is checked by shellcheck
- [ ] #2 `sv2 run muse` and `sv2 run herdr` start those programs, or the design doc records why one of them is not carried over
- [ ] #3 Unit tests cover a definition with and without a wrapper
<!-- AC:END -->
