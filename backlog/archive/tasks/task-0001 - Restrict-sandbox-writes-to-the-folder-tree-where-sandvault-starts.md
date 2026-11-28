---
id: TASK-0001
title: Restrict sandbox writes to the folder tree where sandvault starts
status: To Do
assignee: []
created_date: '2026-11-27T17:48:06Z'
updated_date: '2026-11-27T19:12:16Z'
labels:
  - sandbox-hardening
dependencies: []
references:
  - sv
type: enhancement
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Every sandvault session shares one sandbox profile (`/var/sandvault/sandbox-<user>.sb`) that allows writes to the whole shared workspace (`/Users/Shared/sv-<host-user>`) and the whole sandbox home. A session started in one project can therefore modify every other project in the shared workspace, so a misbehaving agent in one repo can damage or plant files in another. A per-instance sandbox would limit each session's writes to the directory tree it was started in. Open questions the worker must settle: which paths outside the start directory still need to be writable (sandbox home dotfiles and caches, temp directories), and what happens when sandvault is started outside the shared workspace. Converted from TODO.md, "Stricter sandbox" item 1.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A session started in one directory of the shared workspace cannot create, modify, or delete files in a sibling directory
- [ ] #2 Two concurrent sessions started in different directories each have their own write scope
- [ ] #3 Tools that write to the sandbox home and temp directories (shell history, package caches, git) still work in a session
- [ ] #4 Behaviour when sandvault starts outside the shared workspace is defined and tested
- [ ] #5 Tests cover the cross-directory write denial and README documents the write scope
<!-- AC:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-11-27T19:12:14Z

---

With easy-to-create and delete profiles in sv2, this item is no longer necessary

---
<!-- COMMENTS:END -->
