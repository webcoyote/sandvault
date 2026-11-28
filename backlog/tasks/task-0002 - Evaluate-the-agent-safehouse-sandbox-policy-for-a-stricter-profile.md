---
id: TASK-0002
title: Evaluate the agent-safehouse sandbox policy for a stricter profile
status: Done
assignee:
  - '@claude'
created_date: '2026-11-27T17:48:06Z'
updated_date: '2026-11-28T05:30:40Z'
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
- [x] #1 The agent-safehouse policy is compared against the profile sandvault generates, and each restriction sandvault lacks is listed with an adopt or reject decision and the reason
- [x] #2 Each adopted restriction has a follow-up Backlog task, or is implemented with tests
- [x] #3 The licence of agent-safehouse is checked before any of its policy text is copied
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Read the agent-safehouse profiles at a fixed commit (core layers 00 to 50, the optional ones that bear on sandvault) and check its licence.
2. Read what sandvault generates: the heredoc in sv and sv2/crates/helper/src/base.sb.
3. List each restriction agent-safehouse has that sandvault lacks.
4. Measure each candidate as the current user under sandbox-exec: render base.sb to scratch paths, change one rule, run tests/profile-probes.sh, then check what the rule refuses.
5. Record adopt or reject for each with the reason; create a follow-up task for each one adopted.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-27. Compared agent-safehouse at commit 490d8d4 (profiles/00-base, 10-system-runtime, 20-network, 40-shared, 50-integrations-core, and the optional keychain, launch-services, process-control and ssh) with the heredoc in sv (profile version 7) and sv2/crates/helper/src/base.sb (version 9). Licence: Apache-2.0, as sandvault is; no policy text was copied.

The description is out of date for sv2: base.sb already starts from `(deny default)`. Only sv starts from `(allow default)`. sv2 is stricter than agent-safehouse in places: it allows no `system-socket`, no task ports, no diskarbitrationd, logd, analyticsd or configd, and it closes this Mac's SSH port.

The difference that decides most of the list: agent-safehouse runs the agent as the user, so its profile is the only wall and it must fence off parts of that user's home. sandvault runs it as another account, whose home holds nothing of the host user's.

Measured as the current user under sandbox-exec, macOS 26.7.3 (25G342): base.sb rendered to scratch paths, one rule changed, tests/profile-probes.sh run (51 pass unchanged), then a check of what the rule refuses. Not measured in a real session as a sandbox account: that needs sudo, which this session did not have. Each follow-up task says so.

Restrictions sandvault lacks, and the decision:
1. One opendirectoryd name in place of the `com.apple.system.` prefix. ADOPT, TASK-0027 (sv and sv2). The prefix lets a session check passwords of this Mac's accounts with `dscl . -authonly`. 51 probes pass with the one name.
2. Network by address family, Unix sockets by path. ADOPT, TASK-0028. A session connects to cupsd, usbmuxd and a VPN client's socket today. 51 probes pass narrowed, once the SSH port deny names `network-outbound`.
3. Named devices in /dev for writing and ioctl. ADOPT, TASK-0029. /dev/console and four others open for writing today. 51 probes pass narrowed.
4. Reading only named system paths, with metadata-only traversal. ADOPT, already tracked as TASK-0005.06; its list is noted there as a starting point.
5. `(deny default)` for sv. REJECT. sv2 is the rewrite and has it; doing it in sv means measuring every rule again in a second profile. Item 1 is taken into sv because it is one line.
6. `signal` only to `(target same-sandbox)`. REJECT. Measured: it also refuses a signal from one session to another session of the same profile, so a second `sv2 run` could not stop what the first left running. The account already limits signals to its own processes.
7. No argv of other processes (deny the `procargs` sysctls and `process-info-pidinfo` outside the sandbox). REJECT. Measured: the rules work, and `/bin/ps`, which base.sb runs outside the sandbox because a probe needs it, prints the same argv. The kernel already refuses the sysctl for another user's process (EINVAL for pid 1).
8. No program run outside the sandbox (agent-safehouse has no `with no-sandbox`). REJECT. Probe "ps shows this process" needs it.
9. Writing to named parts of the home only; no reading of shell startup files, ~/.ssh, publishing credentials, browser data. REJECT. The home is the sandbox account's own.
10. Keychain (SecurityServer) only on request. REJECT as a default: the keychains reached are the sandbox account's own and agents keep credentials there. TASK-0005 already makes `keychain` a policy of its own that a profile can leave out.
11. Shared memory by name (agent-safehouse allows one name to read, the rest on request). REJECT. Probe "shared memory and semaphores (python)" and the browsers make names of their own.
12. Denying Docker, Podman and ssh-agent sockets by path. REJECT as its own rule: item 2 refuses them without a list of names.
13. A cleaned environment. Not a profile rule; the sv2 helper already starts a session from an empty environment (launch.rs, env_clear).

Found on the way and recorded on TASK-0005: a rule that names one operation beats a rule on a wildcard wherever it stands, which the policy design does not allow for.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Compared agent-safehouse (Apache-2.0, commit 490d8d4) with the profiles of sv and sv2. sv2 is already default deny, so the comparison came down to 13 restrictions, each listed in the notes with adopt or reject and the reason. Adopted: one opendirectoryd name in place of the `com.apple.system.` prefix (TASK-0027, sv and sv2; today a session can check account passwords with `dscl . -authonly`), Unix sockets by path (TASK-0028), named devices in /dev (TASK-0029), and narrower reads (already TASK-0005.06). Rejected: default deny for sv, `same-sandbox` signals, hiding argv, and the rules that only matter when the agent runs as the user. No code changed and no policy text was copied.

Verification: each adopted rule was measured as the current user under sandbox-exec on macOS 26.7.3 with all 51 probes passing. None was measured in a real session as a sandbox account (no sudo here); each follow-up task requires that.

Risk found on the way, recorded on TASK-0005: a rule naming one operation beats a wildcard rule wherever it stands, so the "denies no policy may undo" of the policy design can be undone as written.
<!-- SECTION:FINAL_SUMMARY:END -->
