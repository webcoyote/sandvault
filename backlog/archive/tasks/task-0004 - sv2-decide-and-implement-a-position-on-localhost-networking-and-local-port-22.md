---
id: TASK-0004
title: 'sv2: decide and implement a position on localhost networking and local port 22'
status: Done
assignee:
  - '@claude'
created_date: '2026-11-27T17:59:06Z'
updated_date: '2026-11-27T19:57:50Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/ssh-lockdown.md
  - docs/SANDVAULT2.md
priority: high
type: enhancement
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The sv2 design says the sandbox profile needs "a stated position on localhost networking. At minimum, deny local port 22", and the work was deferred on 2026-11-26. Today a session can reach services listening on this Mac: host services on localhost, debugging ports of browsers in other sessions, and anything else bound locally. The sshd drop-in already refuses logins to sandbox accounts, so port 22 here is about reaching the host account and defence in depth. Known constraints from `docs/ssh-lockdown.md`: a rule denying port 22 to every destination breaks git over SSH to remote hosts, and whether a loopback rule also covers the Mac own LAN address has not been measured. `--chrome` and `--lightpanda` sessions need their own browser endpoint on localhost to keep working. Source: "Sandbox profile", "Priority order" item 4 and "Features not in milestone 1" in the design doc.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The design doc states which local destinations and ports a session may connect to, and why
- [x] #2 A session cannot connect to port 22 on this Mac, by loopback or by the Mac own LAN address, measured as the sandbox user under the rendered profile
- [x] #3 git over SSH to a remote host still works from a session
- [x] #4 Sessions started with `--chrome` or `--lightpanda` still reach their browser endpoint
- [x] #5 The integration suite has a check for each rule added, and the profile artifact version is bumped so installed machines re-render
- [x] #6 Besides 22, a profile closes the port `/etc/services` gives for `ssh` and every port the configuration of sshd names
- [x] #7 No session starts when an SSH port found on this Mac is not closed in the profile, and the message names the command that fixes it
- [x] #8 An sshd configuration that does not parse does not stop sessions: the other sources are used, and `sv2 doctor` reports it
- [x] #9 `sv2 doctor` reports for each of the caller profiles whether the SSH ports found are closed
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Measure, as the current user under sandbox-exec, what a profile rule on `localhost:22` covers (loopback v4 and v6, the LAN address, a remote port 22, another local port). Done before planning: it covers every local address and leaves the rest alone.
2. base.sb: after `(allow network*)`, deny outbound TCP to local port 22; the comment names the probe.
3. profile-probes.sh: three probes: local port 22 is refused by the sandbox itself (EPERM, so the probe means something with Remote Login off); port 22 of a remote host answers; a session reaches its own listener on localhost.
4. Bump SANDBOX_PROFILE_VERSION 6 to 7 and the pinned fingerprint.
5. tests/integration: the existing check has the sandbox ssh to its own account from inside a session, which the new rule stops before sshd. Move the login attempt to the host side with a host-made key whose public half the sandbox authorizes, so the sshd drop-in is still tested by a real login.
6. Docs: the position (local port 22 denied; the rest of localhost stays open, and why) in docs/SANDVAULT2.md, README-SV2.md, and the measured fact in docs/ssh-lockdown.md.
7. Run scripts/tests-sv2 as far as it goes without sudo; the integration suite needs `sv2 install` by the user.
Proposed position, for the user to confirm: only local port 22 is closed. Other local ports stay open because a session own servers (dev servers, the browser endpoint) listen on ports chosen at run time, and a profile rule can name a port but not who listens on it.

8. Added 2026-11-27 at the user request: ports besides 22. Sources: the `ssh` line of /etc/services (what launchd listens on) and the Port and ListenAddress lines of `sshd -G`. Pure parsing in sv2-common; the helper detects at `profile add` and `install` and writes one deny rule per port.
9. `launch` detects again (about 6 ms) and refuses when a found port is not closed in the profile file; a profile that closes more is accepted. If `sshd -G` fails, only 22 and /etc/services are used, so a broken sshd_config cannot lock sessions out.
10. `sv2 doctor`: the same comparison per profile, and a note when sshd configuration does not parse.
11. HELPER_VERSION 7 to 8. Unit tests for the parsing, the rendering and the comparison. No integration check changes the system SSH port: that would mean editing /etc/services or sshd_config on the machine under test.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-27. Rule added to base.sb: `(deny network-outbound (remote tcp "localhost:22"))`; profile version 7.
Measured as the current user under sandbox-exec (macOS 26.7.3, Remote Login on): EPERM at 127.0.0.1, ::1, fe80::1 and the LAN address; github.com:22 and another local port connect. With the rule taken out the new probe fails and names the three addresses.
Three probes added to tests/profile-probes.sh. The helper unit test profile_probes passes with them.
tests/integration: the SSH login attempt now runs from the host with a host-made key the sandbox authorizes, because a session can no longer reach sshd.
Position written to docs/SANDVAULT2.md ("Sandbox profile", "Local port 22"), README-SV2.md and docs/ssh-lockdown.md.
Not done: no run in a real session under profile 7. That needs `sv2 install` with sudo, which asks for a password.

Multi-homed check, 2026-11-27: under the rule, port 22 gave EPERM at all 14 addresses on 11 interfaces of this Mac (lo0, en0, awdl0, llw0, utun0 to utun7). Not tried: a second routable address.
How the SSH port is set on macOS, read on this Mac: launchd owns the listening socket (ssh.plist, SockServiceName "ssh", so the `ssh` line of /etc/services); `sshd -G` prints `Port 22` from sshd_config, which is a different source.

2026-11-27, ports besides 22 (plan steps 8 to 11) built. sv2-common has the parsing; the helper finds the ports at `profile add` and `install`, writes one rule a port, and `launch` refuses when a found port is not closed in the sandbox file. `sv2 doctor` reports per profile. HELPER_VERSION 8; the profile text changed again under version 7, which was never installed (the two installed sandbox files carry 6).
scripts/tests-sv2: passes (fmt, clippy, 95 + 12 + 47 unit tests, profile probes); integration suite skipped, the installed helper is not this build.
Not verified: anything in a real session or through the installed helper (needs `sv2 install` with sudo); the refusal at launch with a really moved SSH port (no test moves it); the doctor lines against a real install.

2026-11-27, after the user ran `sv2 install` (helper 8, profile 7; both installed sandbox files carry the rule). sv2/tests/integration: 174 passed, 0 failed, including the probes in a session, the host-side login attempt, `--chrome` and `--lightpanda` at SV_BROWSER_ENDPOINT, and the doctor line. By hand in a session of profile default: port 22 EPERM at 127.0.0.1, ::1 and 192.168.1.67; github.com:22 answered SSH-. `sv2 doctor` on the real install: "profile ... cannot connect to this Mac SSH port" for both profiles.
Criterion 7 is left unchecked: the refusal at launch is covered by unit tests of the comparison and its message, and has not been seen end to end, since that needs a second SSH port on this Mac (an `ssh` alias line in /etc/services, as root). Criterion 6 is checked on unit tests (parsing, rendering, and a listener refused under sandbox-exec when its port is named); on this Mac both sources give 22 only.

Criterion 8 unchecked again for the same reason as 7: unit tests cover the fallback and the doctor line, but no session has been started on a Mac whose sshd_config does not parse.

2026-11-27: the user chose to close the task with criteria 7 and 8 covered by unit tests only. Neither has been seen end to end.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
A session can no longer connect to this Mac SSH port. The sandbox profile denies outbound TCP to local port 22 (profile version 7), and the helper also closes the ports this Mac says its sshd has: the `ssh` line of /etc/services and the Port and ListenAddress lines of `sshd -G` (helper version 8). The helper looks again before every session and refuses one whose sandbox file leaves a found port open; `sv2 doctor` reports the same per profile. Position on localhost, in docs/SANDVAULT2.md and README-SV2.md: every other local port stays open, because a session own servers listen on ports chosen at run time and a rule cannot tell who listens.
Verified on the installed build: integration suite 174 of 174; in a real session port 22 refused with EPERM at 127.0.0.1, ::1 and the LAN address, github.com:22 answering, both browsers at their endpoints. Against the previous profile the new probe failed in a real session.
Covered by unit tests only, accepted by the user: the refusal at launch when the SSH port was moved after the profile was written, and sessions on a Mac whose sshd_config does not parse.
Limits: an sshd started with -p or a launchd job with a hard-coded port is not found; a second routable address was not tried; a host service on localhost that asks for no credentials is still open to a sandbox.
<!-- SECTION:FINAL_SUMMARY:END -->
