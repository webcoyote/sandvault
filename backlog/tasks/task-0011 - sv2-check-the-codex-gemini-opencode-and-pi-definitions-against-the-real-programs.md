---
id: TASK-0011
title: >-
  sv2: check the codex, gemini, opencode and pi definitions against the real
  programs
status: To Do
assignee: []
created_date: '2026-11-27T17:59:06Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: low
type: task
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
sv2 has five built-in agent definitions. Only `claude` was measured against the real program; the integration suite uses a test agent. The other four were written from v1 and documentation, so their package names, binary paths, bypass flags and `no_inject` lists may be wrong. Put off on 2026-11-26 until asked for ("Priority order" item 5). The same entry under "Not yet verified" lists what has not run for any agent: `brew install` from `sv2` itself, a Mac without Homebrew or with a prefix the user cannot write, and a user definition in `~/.config/sandvault2/agents` on a real machine.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 For each of codex, gemini, opencode and pi: the Homebrew install from `sv2` works, the binary is found, and the bypass flag appears in the program real `--help`
- [ ] #2 For each: the native install with `--native` works in a session
- [ ] #3 Each definition found wrong is corrected, and the doc entry "Parts of agent definitions have not run" is updated
<!-- AC:END -->
