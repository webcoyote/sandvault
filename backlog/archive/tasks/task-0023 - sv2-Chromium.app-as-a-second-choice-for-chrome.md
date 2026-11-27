---
id: TASK-0023
title: 'sv2: Chromium.app as a second choice for --chrome'
status: To Do
assignee: []
created_date: '2026-11-27T20:24:47Z'
updated_date: '2026-11-27T21:13:28Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: low
type: spike
ordinal: 23000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`sv2 --chrome` starts Playwright's headless shell from the sandbox home, installing it with `npx` when it can, and falls back to Google Chrome in `/Applications`, which in a sandbox ends when a page's window closes ("Browsers" and "Google Chrome" under "Milestone 1"). A user who has Chromium.app and not Google Chrome gets neither. Whether Chromium.app runs under the sandbox profile, what rules it needs, and whether it has Chrome's window limit has not been measured.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Chromium.app is measured in a session under the current profile: whether it starts headless, what profile rules it needs, and whether closing the last page of a context ends it
- [ ] #2 Either sv2 tries it after the headless shell and before or after Google Chrome, with the launcher saying which it started, or the design doc records why not
- [ ] #3 If adopted: a probe or integration check covers it, and any profile change bumps SANDBOX_PROFILE_VERSION (sv2/crates/common/src/lib.rs)
<!-- AC:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-11-27T21:13:25Z

---

❯ brew info chromium
==> chromium ⊘ (disabled) (Chromium): latest
Free and open-source web browser
https://www.chromium.org/Home
Disabled because it does not pass the macOS Gatekeeper check! It was disabled on 2026-09-01.

---
<!-- COMMENTS:END -->
