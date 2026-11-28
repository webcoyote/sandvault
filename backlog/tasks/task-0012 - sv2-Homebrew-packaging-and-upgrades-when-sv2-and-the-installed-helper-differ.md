---
id: TASK-0012
title: 'sv2: Homebrew packaging, and upgrades when sv2 and the installed helper differ'
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
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
sv2 can only be run from a source build. Built-in agent definitions are meant to live in `<prefix>/share/sandvault2/agents/`, a directory that does not exist until sv2 is packaged, so that lookup has never run under a real Homebrew prefix. The helper is installed outside the prefix by `sv2 install`, so after `brew upgrade` the new `sv2` meets an older helper and older root-owned artifacts; `sv2` and the helper compare artifact versions, and what the user is told and has to do in that case is not settled. Put off on 2026-11-26 until asked for.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `brew install` gives a working `sv2` whose built-in agents are found under the prefix `share/` directory
- [ ] #2 After an upgrade that changes an artifact version, `sv2` says what is out of date and which command brings it up to date, and refuses to run a session against a helper it cannot work with
- [ ] #3 After an upgrade that changes no artifact version, sessions run with no extra step
- [ ] #4 README-SV2.md documents install and upgrade
<!-- AC:END -->
