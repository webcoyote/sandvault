---
id: TASK-0005
title: >-
  sv2: policies: per-profile rules chosen by the host user, and a narrower rule
  for reading files
status: In Progress
assignee:
  - '@claude'
created_date: '2026-11-27T17:59:06Z'
updated_date: '2026-11-28T05:15:12Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
  - docs/SANDVAULT2-POLICIES.md
priority: high
type: feature
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Each sandvault profile already has its own root-owned `sandbox-exec` file so that profiles can be given different rules by the host user, but there is no way to state them: every profile gets the same rules. Open item 8 of the design doc asked for the input format, and the same deferred entry asked for a narrower rule for reading files than the profile has today. Decided on 2026-11-27 (docs/SANDVAULT2-POLICIES.md): a profile has one policy, a named list of rules written in a rules-only format that the helper can parse and check; not a fixed menu of named capabilities, and not the whole `sandbox-exec` language. Root never opens a host-owned file, so `sv2` sends the policy to the helper on stdin. The work is done through the subtasks; this task is finished when they are and its own criteria hold.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The policy format is decided and recorded in the design doc with the reason
- [ ] #2 The host user can give two profiles different policies, and each rendered file reflects only its own
- [ ] #3 The helper rejects policy text it does not recognise, and never reads a host-owned file to get it
- [ ] #4 The sandbox user cannot change its own policy
- [ ] #5 File reads are narrower than today, with each remaining allow justified by a failing probe kept as a test
- [ ] #6 Unit tests cover parsing and rendering; the integration suite covers one rule on and off
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Done through the subtasks, in dependency order: TASK-0005.01 (parser and built-in policies), .02 (helper renders from a policy), .03 (setting the policy of a profile), .04 (the policy commands), .05 (the wizard), .06 (narrower reads, strict and offline). .06 needs only .02 and can run beside .03 to .05.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-27: open item 8 decided with Pat. Not the fixed menu of named capabilities and not raw SBPL: policies written as rules only (allow, deny, include). Measured as the current user on macOS 26.7.3 (25G342) under sandbox-exec: a file that redefines deny loads and a later deny then denies nothing; (allow default) after (deny default) loads; a relative path loads; an unknown operation or filter does not load; a host name in a remote filter is refused; prefix, two operations in one rule, and (param) inside subpath load. Recorded in docs/SANDVAULT2.md. Nothing is built. The acceptance criteria still say "capability"; they read correctly with "policy" in its place and were left as written.

2026-11-27: at Pat's request the design is in its own file, docs/SANDVAULT2-POLICIES.md; docs/SANDVAULT2.md keeps a pointer under "Sandbox profile" and in open item 8.

2026-11-27: at the request of Pat the build order and the unmeasured items were taken out of the design doc and are tracked here: six subtasks. The title, description and criteria were reworded from "capability" to "policy" to match the decision; what each criterion asks is unchanged.
<!-- SECTION:NOTES:END -->
