---
id: TASK-0020
title: 'sv2: keep workspaces in /Users/sv2 and reserve the name sv2'
status: Done
assignee:
  - '@claude'
created_date: '2026-11-27T20:24:47Z'
updated_date: '2026-11-27T21:53:09Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: low
type: task
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Workspaces lived under `/Users/Shared/sv2`. `/Users/Shared` is writable by everyone, so another host user could create `sv2` there before `sv2 install` and block it ("A taken workspace root" under "Milestone 1"); a per-user name only narrows that to one victim. Move the root to `/Users/sv2`: only root can create entries in `/Users`. The remaining clash is an admin creating an account named `sv2`, whose home would be `/Users/sv2`. `install` reserves that name with a hidden account sandvault2 creates, so macOS refuses the name.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 WORKSPACE_ROOT is /Users/sv2, and the helper creates it in /Users with no taken-name handling (claim_root_dir_at and its tests are gone)
- [x] #2 install creates a hidden sv2 account (no password, shell /usr/bin/false, home /var/empty, ID from the registry), and refuses when an account named sv2 exists that sandvault2 did not create
- [x] #3 uninstall deletes the sv2 account only when /Users/sv2 is gone; when it is kept, the account is kept and the output says so
- [x] #4 The sandbox profile no longer re-allows /Users/Shared, SANDBOX_PROFILE_VERSION is bumped, and the profile probes still pass
- [x] #5 docs/SANDVAULT2.md, unit tests, tests/integration, tests/root-checks and tests/mutation-checks use the new root
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. common: WORKSPACE_ROOT=/Users/sv2, RESERVED_ACCOUNT=sv2; bump SANDBOX_PROFILE_VERSION.
2. helper ds: ensure_reserved_user (RealName first, then UniqueID, PrimaryGroupID -2, /var/empty, /usr/bin/false, Password *, IsHidden).
3. helper ops: install reserves the name (allocating its ID from the registry like the sandbox group), then ensure_root_dir_at(/Users, sv2); ensure_workspace opens /Users; uninstall removes the account only after /Users/sv2 is removed; user allow refuses the reserved account.
4. sys: remove claim_root_dir_at/make_way_at/not_ours and their test.
5. base.sb: drop /Users/Shared literal; probes skip /Users/sv2/ instead of /Users/Shared/.
6. Update unit-test paths, tests/integration, root-checks (replace taken-root cases with reserved-account cases), mutation-checks, docs.
7. scripts/validate; root-run suites left to the user (sandbox cannot sudo).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented (uncommitted). Verified in the sandbox: build, cargo fmt, clippy -D warnings, shellcheck, sv2-common tests (12/12), sv2-helper tests 45/47, sv2 CLI tests 100/104.
The failures are environmental and fail the same way on unmodified HEAD: 4 agent:: tests refuse TMPDIR because it belongs to sandvault-pat; rendered_profile_loads, a_closed_port_is_refused_and_another_is_not and profile_probes cannot nest sandbox-exec ("sandbox_apply: Operation not permitted").
Not run (needs root): tests/root-checks (new section "the name sv2"), tests/integration, tests/mutation-checks. Not known: whether System Settings refuses a name held by a hidden account.
Upgrade on a Mac with an earlier build: sudo mv /Users/Shared/sv2 /Users/sv2 before installing.

User ran on the development Mac, 2026-11-27, and reported all passing: scripts/tests-sv2, tests/root-checks, tests/integration, tests/mutation-checks.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Moved the sv2 workspace root from /Users/Shared/sv2 to /Users/sv2. /Users/Shared is writable by everyone, so another user could take the name and block install; only root can create names in /Users, so the taken-name handling (claim_root_dir_at) is removed. install now reserves the name with a hidden sandvault2 account `sv2` (no password, /usr/bin/false, /var/empty, ID from the registry) so no new account is given /Users/sv2 as its home; it refuses an `sv2` account it did not create, completes one a killed install left, and uninstall keeps it while /Users/sv2 is kept. user allow refuses it. The sandbox profile no longer re-allows /Users/Shared (profile version 8, helper version 9). Tests, root-checks ("the name sv2"), integration, docs and README updated.

Verified: unit tests, probes, root-checks, integration and mutation checks, run by the user on the development Mac.
Risks / follow-ups: not known whether System Settings refuses a name held by a hidden account; existing workspaces must be moved by hand (sudo mv /Users/Shared/sv2 /Users/sv2); sv2 doctor does not check the reserved account.
<!-- SECTION:FINAL_SUMMARY:END -->
