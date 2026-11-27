---
id: TASK-0006
title: >-
  sv2: verify what a simulator app can do on the host, and document what --ios
  grants
status: To Do
assignee: []
created_date: '2026-11-27T17:59:06Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/security-review-2026-11-24.md
  - docs/SANDVAULT2.md
priority: high
type: spike
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Open item 2 in the design doc, and finding 6 of the security review, which marked it "verify": simulator apps are believed to run as processes of the host user, outside the sandbox account and the `sandbox-exec` profile. If that holds, an iOS bridge that lets the sandbox install and launch an app it built gives the sandbox code execution as the host user. Nothing has been measured. The suggested check is from the host side: launch a test app that writes a marker outside its container and see who owns it. The answer decides how the sv2 iOS bridge is restricted and what the README must say.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A test app installed and launched in the simulator is measured from the host side: which user it runs as, and whether it can read and write outside its container
- [ ] #2 The result is recorded in the design doc under open item 2, with the macOS and Xcode versions measured
- [ ] #3 The doc states in one sentence what trust `--ios` grants, ready for the README
<!-- AC:END -->
