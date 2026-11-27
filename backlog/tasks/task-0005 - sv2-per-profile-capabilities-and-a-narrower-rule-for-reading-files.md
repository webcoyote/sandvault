---
id: TASK-0005
title: 'sv2: per-profile capabilities and a narrower rule for reading files'
status: To Do
assignee: []
created_date: '2026-11-27T17:59:06Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: high
type: feature
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Each sandvault profile already has its own root-owned `sandbox-exec` profile file so that profiles can be given different capabilities by the host user, but there is no way to state them: every profile gets the same rules. Open item 8 in the design doc is the undecided input format: a fixed set of named capabilities that the helper maps to rules it already contains (keeps root parser small and every rule reviewed), or raw SBPL added to the base profile (more flexible, but the helper must parse and validate it). Root never opens a host-owned file, so `sv2` sends the choices to the helper on stdin. The same deferred entry asks for a narrower rule for reading files than the profile has today. Deferred on 2026-11-26; listed as next in "Priority order" item 4.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The capability input format is decided and recorded in the design doc with the reason
- [ ] #2 The host user can give two profiles different capabilities, and each rendered profile file reflects only its own
- [ ] #3 The helper rejects capability input it does not recognise, and never reads a host-owned file to get it
- [ ] #4 The sandbox user cannot change its own capabilities
- [ ] #5 File reads are narrower than today, with each remaining allow justified by a failing probe kept as a test
- [ ] #6 Unit tests cover parsing and rendering; the integration suite covers one capability on and off
<!-- AC:END -->
