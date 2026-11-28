---
id: TASK-0025
title: 'sv2: run the iOS simulator inside the sandbox and remove the host bridge'
status: Done
assignee:
  - '@claude'
created_date: '2026-11-28T02:24:07Z'
updated_date: '2026-11-28T02:55:01Z'
labels:
  - sv2
dependencies: []
priority: high
ordinal: 25000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Decided 2026-11-27 after TASK-0007 was built. The host-side bridge of TASK-0007 runs the simulator as the host user, and a simulator app is an unconfined process of whoever runs the simulator: measured, an app installed through the bridge listed and wrote the host home. So --ios with the bridge gives the sandbox code execution as the host user. Measured the same day: the sandbox account can create, boot and use a headless simulator itself (39 s to boot, 1.8 GB a device), and under the sandbox profile simctl and iosef (describe, tap, swipe) work once mach-lookup of names starting com.apple.CoreSimulator. is allowed. This task replaces the bridge: --ios selects that rule for the session, and the session runs simctl and iosef itself, as the sandbox account. What the rule costs: the simulator and its apps are started by launchd, so they run as the sandbox account without the sandbox-exec profile; only sessions started with --ios get the rule.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 With --ios a session creates, boots and drives a simulator itself (install, launch, describe, tap, screenshot), as the sandbox account; nothing of sv2 runs a simulator or a bridge as the host user
- [x] #2 Without --ios a session cannot reach CoreSimulator's services: the same commands fail
- [x] #3 The rule is in the one profile file and is switched by a parameter the helper passes; the helper validates and versions the file as before, and a profile started without the parameter does not load
- [x] #4 The simulator a session makes is shut down and deleted when the session ends, and one left by a killed session is deleted by the next
- [x] #5 An app run in the simulator is measured from the host side: which account it runs as, and that it cannot read or write the host home
- [x] #6 The host bridge, its token hand-over and its dependency are removed
- [x] #7 README-SV2.md and docs/SANDVAULT2.md state what --ios grants now: code outside the profile, inside the account
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. common: bump HELPER_VERSION and SANDBOX_PROFILE_VERSION; name the launch word and the profile parameter.
2. helper: base.sb gets the CoreSimulator rule inside (if (string=? (param "SV2_IOS") "1") ...); 'launch NAME [ios] -- CMD' carries the word to stage 2, which passes -D SV2_IOS=1 or 0; the file's validation passes the parameter too.
3. profile probes: the services are closed without the parameter and open with it.
4. cli: delete ios.rs, serde_json, the token session, the waiting and signal code; --ios adds the word to the helper call and 'ios XCRUN' to the launcher.
5. launcher: with ios it deletes simulators of sessions that are gone, creates one (newest iOS runtime, newest iPhone it takes), boots it in the background, exports SV_IOS_SIMULATOR_UDID, installs iosef with uv when missing, and a watcher shuts the device down and deletes it when the session ends. The 16 KB limit on the launcher is raised.
6. Prompt, banner, README, design doc: what --ios grants now.
7. tests/integration: the iOS section runs simctl and iosef in the session, plants nothing (there is no host code left to mislead), and measures from the host which account the app runs as and that the host home is closed to it.
8. Needs 'sv2 install' by an admin before the integration section can run: the helper changes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Built, not yet run for real: helper (launch NAME [ios], -D SV2_IOS=0|1, profile rule behind the parameter, versions helper=10 profile=9), launcher (KIND ios XCRUN), cli (--ios; bridge, serde_json and token code removed; the old ios.rs is kept outside the repository at /private/tmp/sv2-bridge-ios.rs.removed), prompt, README, design doc, integration section. Unit tests, clippy, shellcheck and both profile-probe passes are green. The integration section and the real-session measurement need the new helper installed: sv2/target/release/sv2 install.

Run for real on 2026-11-27 with helper 10 installed: the iOS section of tests/integration, against the default profile, 23 of 23. First run failed at install (Mach error -308, server died): the launcher's background process restarted com.apple.CoreSimulator.bridge after boot to turn accessibility on, at the moment the session installed. Fixed by writing the accessibility keys into the device's settings before boot; nothing is restarted. Evidence: (1) boot, install, launch, screenshot, iosef describe and tap in the session; host ps shows the app as the sandbox account, and the host has no sv2-ios simulator. (2) without --ios, simctl lists nothing; profile probe 'simulator: its services are closed without --ios'. (3) helper unit tests: the profile does not load without the parameter, the word ios is the only one accepted before --; the pinned text and Mach list were updated. (4) integration: no launchd_sim of the account afterwards and the device directory gone; stale devices are covered by the launcher's unit test with a stand-in xcrun only. (5) the app reports the sandbox uid, cannot list the host home or a host-only directory, cannot write there; the host found nothing written. (6) ios.rs, the token session, the signal code and serde_json are gone. (7) README-SV2.md 'An iOS simulator', docs/SANDVAULT2.md 'iOS simulator'. Not run: the whole integration suite and scripts/validate (profile removal needs someone to answer the macOS prompt); CI.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
sv2 run --ios now lets the session run an iOS simulator itself, as the sandbox account; the host bridge of TASK-0007 is removed. The helper passes a parameter to the one profile file that opens CoreSimulator's services for that session only; the launcher creates, boots and deletes the simulator and installs iosef in the sandbox home. Verified in a real session: 23 of 23 integration checks, including that an app in the simulator is the sandbox account's process and cannot read or write the host home. What --ios grants now, and the docs say: code outside the sandbox profile, inside the account. Risks: the simulator's processes are not bound by the profile's own denials; about 2 GB and a few hundred processes while a simulator runs; the launcher's size limit was raised to 20 KB; helper 10 and profile 9 need 'sv2 install' on every Mac.
<!-- SECTION:FINAL_SUMMARY:END -->
