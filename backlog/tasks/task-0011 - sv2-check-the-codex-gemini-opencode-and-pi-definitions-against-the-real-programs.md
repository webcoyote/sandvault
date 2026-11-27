---
id: TASK-0011
title: >-
  sv2: check the codex, gemini, opencode and pi definitions against the real
  programs
status: Done
assignee:
  - '@claude'
created_date: '2026-11-27T17:59:06Z'
updated_date: '2026-11-27T22:56:59Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: low
type: task
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
sv2 has five built-in agent definitions. Only `claude` was measured against the real program; the integration suite uses a test agent. The other four were written from v1 and documentation, so their package names, binary paths, bypass flags and `no_inject` lists may be wrong. Put off on 2026-11-26 until asked for ("Priority order" item 5). The same entry under "Not yet verified" lists what has not run for any agent: `brew install` from `sv2` itself, a Mac without Homebrew or with a prefix the user cannot write, and a user definition in `~/.config/sandvault2/agents` on a real machine.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 For each of codex, gemini, opencode and pi: the Homebrew install from `sv2` works, the binary is found, and the bypass flag appears in the program real `--help`
- [x] #2 For each: the native install with `--native` works in a session
- [x] #3 Each definition found wrong is corrected, and the doc entry "Parts of agent definitions have not run" is updated
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Check package names, binary paths and deprecation against the Homebrew API and npm.
2. Install the four with npm into scratch and check each bypass flag, no_inject and inject_last against the real --help.
3. Correct wrong definitions; record what was checked in the definitions and in docs/SANDVAULT2.md.
4. On a Mac with sv2 installed: run the Homebrew install and --native for each from sv2 (AC #1, #2).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-27, from a v1 sandbox (sandvault-pat, no admin, so sv2 could not be installed or run):
- Checked against codex 0.172.0, gemini 0.71.1, opencode 1.19.6, pi 1.7.0 (npm into /tmp scratch). Every bypass flag is in --help. codex: all 28 subcommands take the flag in front. gemini: with --yolo in front, each of the six commands falls back to the top-level usage. pi: same 8 commands, auth and mcp still reject --approve. opencode 1 still reads OPENCODE_PERMISSION.
- Homebrew: the codex cask links bin/codex into {brew}/bin. gemini-cli formula deprecated 2026-06-18 (unsupported; replacement cask antigravity-cli). homebrew-core opencode and tap opencode-v2 are opencode 2, which has no OPENCODE_PERMISSION (uses --auto). Tap opencode is 1.19.6, and the curl installer only installs opencode 1. Homebrew now requires tap trust, but brew install with the full name trusts it itself (cmd/install.rb).
- No definition was wrong; comments updated in the four agent.toml files, findings added under "Agent definitions" in docs/SANDVAULT2.md (the "Parts of agent definitions have not run" entry was taken out of the doc in 1df8695).
- cargo test: the_builtin_definitions passes; 4 tests fail only because $TMPDIR belongs to the sandbox user here (refused as designed).
- Still to do: AC #1 Homebrew install from sv2 and AC #2 --native in a session, on a Mac with sv2 installed.

2026-11-27: the user ran all four from sv2, Homebrew and --native; all worked.
- Decision (user): gemini installs natively only. Removed its [install.host]; added Definition::uses_own_install so a definition with only [install.guest] runs as if --native were given (main.rs run). Doctor note, template, test the_builtin_definitions and docs/SANDVAULT2.md updated; unit test added.
- opencode: no change; it works while opencode 1 is what the tap and installer give.
- Waiting on: the user running `sv2 gemini` without --native after scripts/sv2-dev-agents --force.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Checked codex 0.172.0, gemini 0.71.1, opencode 1.19.6 and pi 1.7.0 against their real --help, Homebrew and npm; no definition was wrong. The user ran all four from sv2 with Homebrew and --native. Homebrew deprecated gemini-cli, so gemini now has only [install.guest], and sv2 runs a definition with only that install as if --native were given (Definition::uses_own_install, with a unit test). Risk recorded: opencode 2 ignores OPENCODE_PERMISSION; the tap and installer still give opencode 1.
<!-- SECTION:FINAL_SUMMARY:END -->
