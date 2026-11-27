---
id: TASK-0002
title: Evaluate the agent-safehouse sandbox policy for a stricter profile
status: To Do
assignee: []
created_date: '2026-11-27T17:48:06Z'
updated_date: '2026-11-27T19:13:46Z'
labels:
  - sandbox-hardening
dependencies: []
references:
  - https://eugene1g.github.io/agent-safehouse/
  - https://gist.github.com/eugene1g/ad3ff9783396e2cf35354689cc6473e1
  - sv
  - sv2
type: spike
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The sandvault profile starts from `(allow default)` and denies selected operations, so anything not explicitly denied is permitted. TODO.md pointed at a stricter published policy as a source of ideas; that gist now only says the policy has moved to the agent-safehouse project. This task compares that policy with sandvault's profile and decides which restrictions are worth adopting, without breaking the tools sandvault sessions run. Converted from TODO.md, "Stricter sandbox" item 2.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The agent-safehouse policy is compared against the profile sandvault generates, and each restriction sandvault lacks is listed with an adopt or reject decision and the reason
- [ ] #2 Each adopted restriction has a follow-up Backlog task, or is implemented with tests
- [ ] #3 The licence of agent-safehouse is checked before any of its policy text is copied
<!-- AC:END -->
