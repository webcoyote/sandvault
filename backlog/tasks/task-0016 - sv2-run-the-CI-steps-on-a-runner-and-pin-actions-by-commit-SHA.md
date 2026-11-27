---
id: TASK-0016
title: 'sv2: run the CI steps on a runner and pin actions by commit SHA'
status: To Do
assignee: []
created_date: '2026-11-27T17:59:07Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/security-review-2026-11-24.md
  - docs/SANDVAULT2.md
priority: low
type: task
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`.github/workflows/ci.yml` has "Install sandvault2" and "SandVault2 Tests"; neither has run on a runner because the branch has not been pushed. The install step turns Remote Login on (`launchctl load -w .../ssh.plist`) so that the suite SSH login attempt runs, and whether that works on a runner is unknown. The suite also meets the "administer your computer" prompt at account deletion, which nobody answers on a runner. Separately, the design says to pin CI actions by commit SHA and the workflows still pin by tag, including `mislav/bump-homebrew-formula-action@v4.2`, which receives `HOMEBREW_GITHUB_API_TOKEN` (finding 19 of the security review). Pushing the branch is the user decision. Put off on 2026-11-26.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both sv2 steps pass on a GitHub runner, with no check skipped under `SV_TESTS_FAIL_ON_SKIP`
- [ ] #2 The doc entry "CI steps are unverified" is updated with what happened to Remote Login and the privacy prompt on the runner
- [ ] #3 Every action in `.github/workflows` is pinned by commit SHA
<!-- AC:END -->
