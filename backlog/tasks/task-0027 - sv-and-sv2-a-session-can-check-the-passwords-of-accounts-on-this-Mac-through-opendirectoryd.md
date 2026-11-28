---
id: TASK-0027
title: >-
  sv and sv2: a session can check the passwords of accounts on this Mac through
  opendirectoryd
status: In Progress
assignee:
  - '@claude'
created_date: '2026-11-28T05:30:09Z'
updated_date: '2026-11-28T05:40:17Z'
labels:
  - sandbox-hardening
  - sv2
dependencies: []
references:
  - sv2/crates/helper/src/base.sb
  - sv
  - sv2/tests/profile-probes.sh
  - >-
    https://github.com/eugene1g/agent-safehouse/blob/490d8d4bdc090029cad647a4057fe4a35115bf28/profiles/10-system-runtime.sb
priority: high
type: bug
ordinal: 34000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Both profiles allow every Mach service whose name starts with `com.apple.system.`, for the sake of one of them: `com.apple.system.opendirectoryd.libinfo`, which turns user and group numbers into names. The prefix also admits `com.apple.system.opendirectoryd.api`, the service `dscl` talks to. Measured on 2026-11-27 (macOS 26.7.3, as the current user under the rendered `base.sb`): `dscl . -authonly NAME PASSWORD` gets the same answer from opendirectoryd inside the sandbox as outside it, so a session can test passwords of the host user and of every other account, and `dscl . -read /Users/NAME` reads their records. `su`, `login` and this Mac's sshd are already closed to a session; this is the way left open. Whether opendirectoryd slows or locks out repeated wrong passwords was not measured. With the prefix replaced by the one name, all 51 probes of `tests/profile-probes.sh` pass, `id`, `whoami`, `ls -l`, `getpwnam` and `logger` work, and `dscl` fails with eServerError. agent-safehouse names three services under the prefix (`opendirectoryd.libinfo`, `notification_center`, `logger`); no probe needed the last two. Not measured: a real session as a sandbox account, where preferences go through the account's own cfprefsd (the `defaults write and read` probe is skipped outside a session, and a comment in `sv` says `defaults` writes do not persist without the prefix). Found by TASK-0002.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 In a session of sv2 and in a session of sv, `dscl . -authonly` with a name that does not exist and `dscl . -read` of the host user both fail, and a probe that fails if either works is kept as a test
- [ ] #2 Every Mach name under `com.apple.system.` that stays allowed names the probe that fails without it, measured as the sandbox user in a real session
- [ ] #3 The probes for names (`id shows names`, `ssh-keygen`) and `defaults write and read` pass in a real session of each
- [x] #4 The profile version of each (`SANDBOX_PROFILE_VERSION`, `SV_SANDBOX_PROFILE_VERSION`) is raised so that installed profiles are written again
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. sv2: in base.sb allow `com.apple.system.opendirectoryd.libinfo` by name in place of the `com.apple.system.` prefix; update the pinned list of names in sandbox.rs; raise SANDBOX_PROFILE_VERSION 9 to 10.
2. sv2: add a probe to tests/profile-probes.sh that fails if `dscl . -authonly` or `dscl . -read` of an account gets an answer from opendirectoryd. It names no real account for the password check.
3. sv: the same one name in the heredoc; raise SV_SANDBOX_PROFILE_VERSION 7 to 8; add the same check to scripts/tests.
4. Update the line in docs/SANDVAULT2.md that lists the names.
5. Run the helper unit tests and profile_probes (as the current user), shellcheck through scripts/validate.
6. Not possible in this session (no sudo): the probes in a real session of each, where `defaults write and read` runs. Leave the criteria that need it unchecked and say so.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-28. Done in code: sv2 base.sb and sv name `com.apple.system.opendirectoryd.libinfo` in place of the prefix; SANDBOX_PROFILE_VERSION 9 to 10 (with the pinned fingerprint and the pinned list of Mach names), SV_SANDBOX_PROFILE_VERSION 7 to 8. Tests added: probes "check a password (dscl)" and "read an account's record (dscl)" in sv2/tests/profile-probes.sh; three tests in scripts/tests (the two dscl ones, and that user and group names still resolve).

Verified as the current user: sv2 unit tests pass (12 and 48), profile_probes passes with the two new probes, and both new probes FAIL under the old prefix (measured in TASK-0002: dscl answers eDSAuthFailed and prints the record). shellcheck is clean.

Not verified, no sudo in this session: the sv2 integration suite (skipped: "the installed helper is not this build", needs `sv2 install`) and scripts/tests for sv (stopped at its first sudo). So criteria 1 to 3 are unproven where they say "in a session" or "in a real session": the probes as a sandbox account, `defaults write and read` (skipped outside a session), and guest/home/configure, which runs `defaults write com.apple.dt.Xcode`. Criterion 4 is met.

2026-11-28. Pat ran the integration suite on the first commit (d949e93): 53 probes ok, "defaults write and read" failed ("The domain/default pair of (com.sandvault2.probe, k) does not exist"). Cause, found by adding one name at a time as the current user with the real home: `defaults write` needs `com.apple.system.opendirectoryd.membership`, which answers whether a user is in a group and checks no password. With it added, `defaults` writes and reads, and both dscl probes are still refused (eServerError). notification_center, logger, the two DirectoryService names and opendirectoryd.api did not help. Both profiles now name libinfo and membership; versions 11 (sv2) and 9 (sv). A preferences test was added to scripts/tests for sv. Still to be confirmed by a real-session run: criteria 1 to 3.
<!-- SECTION:NOTES:END -->
