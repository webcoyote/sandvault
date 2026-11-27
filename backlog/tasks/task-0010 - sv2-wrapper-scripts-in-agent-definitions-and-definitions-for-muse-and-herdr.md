---
id: TASK-0010
title: 'sv2: wrapper scripts in agent definitions, and definitions for muse and herdr'
status: Done
assignee:
  - '@claude'
created_date: '2026-11-27T17:59:06Z'
updated_date: '2026-11-27T20:17:05Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: low
type: feature
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The definition format allows an optional `wrapper` script under `[run]` that runs inside the sandbox, for agents that do not fit the data-only run description. It is in the design and not built. v1 ships `muse` and `herdr`; sv2 has built-ins only for claude, codex, gemini, opencode and pi, so users of those two cannot move to sv2. Put off on 2026-11-26 until asked for ("Priority order" item 5). Whether muse or herdr need a wrapper is not known; herdr is a terminal multiplexer in v1, so read "Terminal multiplexers" first.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A definition can name a wrapper script, which runs inside the sandbox as the sandbox user and is checked by shellcheck
- [x] #2 `sv2 run muse` and `sv2 run herdr` start those programs, or the design doc records why one of them is not carried over
- [x] #3 Unit tests cover a definition with and without a wrapper
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. agent.rs: [run] wrapper = "NAME.sh", read like install.sh (in the definition dir, .sh, <=64 KB); Definition.wrapper.
2. Launcher protocol: agent NAME NATIVE SCRIPT WRAPPER N PATHS...; with a wrapper, the login zsh runs /bin/bash -c WRAPPER sv2-run-NAME PROGRAM ARGS (ARGS already carry the injected flag); banner says the wrapper is used.
3. Template documents wrapper; design doc updated.
4. sv2/agents/muse: data only, from v1 (cask muse-code, native installer, --yolo).
5. sv2/agents/herdr: no flag; wrapper refuses a session with SV_BROWSER_ENDPOINT, since herdr's server outlives it (v1 refused --chrome for the same reason).
6. Unit tests: parse with and without wrapper; launcher runs the wrapper with program+args, and runs the program directly without one. shellcheck covers sv2/agents/*/*.sh.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Built [run] wrapper (agent.rs, launcher protocol gains WRAPPER after SCRIPT, banner line in main.rs, template). Built-ins muse (data only) and herdr (wrapper refuses SV_BROWSER_ENDPOINT). Docs: SANDVAULT2.md milestone notes, priority order, features list; README-SV2.md.
Verified in a v1 sandbox (no sv2 helper installed): shellcheck of all guest/agent scripts, cargo fmt, clippy -D warnings, unit tests 94/98 in cli. The 4 failures (and 2 in helper) also fail on unchanged HEAD: fixtures in a TMPDIR owned by sandvault-pat are refused by design; sandbox-exec cannot nest. Mutation: disabling the launcher wrapper branch fails a_wrapper_runs_in_the_programs_place.
NOT verified (AC #2): sv2 run muse / sv2 run herdr on a Mac with sv2 installed; herdr server reattach; herdr moving to the per-user bootstrap (launchctl manageruid in a pane should equal id -u). Recorded in SANDVAULT2.md.

AC #2 and the remaining checks (sv2 run muse, sv2 run herdr, integration suite, scripts/validate) verified by the user on a Mac with sv2 installed, 2026-11-27.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Agent definitions can name a wrapper script ([run] wrapper = "NAME.sh"): read by sv2 like install.sh, run by the launcher inside the sandbox as the sandbox user with the program and its final arguments, announced in the banner, linted by shellcheck for built-ins. New built-ins muse (data only, --yolo) and herdr (no flag; its wrapper refuses a session with a browser, because its server outlives the session). Unit tests cover parsing with and without a wrapper, the launcher running one, and herdr's run.sh. Commit 2e0b012. The user verified sv2 run muse, sv2 run herdr, the integration suite and scripts/validate on a Mac with sv2 installed.
<!-- SECTION:FINAL_SUMMARY:END -->
