---
id: TASK-0008
title: 'sv2: system_prompt in agent definitions'
status: Done
assignee:
  - '@claude'
created_date: '2026-11-27T17:59:06Z'
updated_date: '2026-11-27T20:12:19Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
priority: medium
type: feature
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The agent definition format in the design doc has a `system_prompt` key under `[run]`: text that sv2 adds to an agent system prompt, in whatever way that agent takes one (for example a flag such as `--append-system-prompt`). It is in the design ("Definition format") and not built, so sv2 has no way to tell an agent anything about the session it is in. Telling an agent that its session has a browser depends on this.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A definition can say how its agent takes extra system prompt text, and sv2 passes the text that way
- [x] #2 The text is not injected into subcommands listed in `no_inject`
- [x] #3 A definition without the key runs as it does today
- [x] #4 The injected flag is checked against the agent real `--help`, as the review guidelines require for injected flags
- [x] #5 `sv2 agent new` template shows the key, and unit tests cover parsing and the argument vector
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. agent.rs: read `system_prompt = { flag = "--long-option" }` under [run]; keep it as the definition's prompt flag. Same checks as bypass_flag, and it may not be the same flag.
2. final_args takes the text sv2 has for the session and puts FLAG TEXT (two arguments) after the bypass flag, before the user's arguments. Not for a first argument listed in no_inject, and not for one in inject_last either (pi's package commands know only their own options). No text or no key: the arguments are as today.
3. main.rs passes no text yet: the first text is the browser's (TASK-0009). The schema number stays 1: nothing is released.
4. Built-ins: claude gets the key, checked against the real claude --help and by running each subcommand with the flag in front. pi is not installed on this Mac, so its key waits for TASK-0011; codex, gemini and opencode take text another way than a flag.
5. Template, unit tests (parsing, argument vector, text through the real launcher), docs/SANDVAULT2.md.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Decisions: the key is a table with one key, `flag` (a long option); FLAG TEXT go after the bypass flag and before the user's arguments, as two arguments. A first argument in no_inject or inject_last gets neither. Schema stays 1 (nothing released).
Found while checking claude 2.1.326: attach, logs, respawn, rm, stop and kill are known only as the first word, so the bypass flag in front already turned them into a chat prompt. They are now in claude's no_inject; the definition's comment that the flag is accepted before every subcommand was wrong.
Only claude has the key. pi is not installed on this Mac, so its flag could not be checked (TASK-0011). codex, gemini and opencode take text by a file, a directory or --prompt, which `flag` does not describe.
sv2 has no text to give yet (TASK-0009), so run() passes None and no session's command line has changed.

2026-11-27, after pi 1.6.1 was installed: pi now has the key. `pi --approve --append-system-prompt TEXT` is accepted (a misspelt option is reported as Unknown option); all eight of pi's subcommands are first-word-only and already in no_inject or inject_last, so none gets the flag. pi had no provider signed in, so no text was seen to reach a model.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
`[run] system_prompt = { flag = "--long-option" }` in an agent definition names how the agent takes text for its system prompt. `Definition::final_args(args, told)` puts the flag and the text, as one argument, after the bypass flag and before the user's arguments; not for a subcommand listed in no_inject or inject_last, and not when there is no text or no key. `prompt_flag_for` says whether a command line would be told, for the banner TASK-0009 needs. The claude built-in has the key, and its no_inject now lists the six background-session subcommands that any option in front turns into a chat prompt. The `sv2 agent new` template shows the key; docs/SANDVAULT2.md records the design and what was measured.

Verified: cargo fmt, clippy -D warnings, unit tests (97 cli, 12 common, 47 helper; new ones cover parsing, refusals, the argument vector, and a multi-line text through the real launcher and login shell), profile probes, sv2 integration suite 174 passed. Against claude 2.1.326: each of the 15 remaining subcommands prints the same --help with both flags and a two-line text in front as without.

Not verified: no session passes text yet, because sv2 has none until TASK-0009, so nothing was run end to end in a sandbox. pi, codex, gemini and opencode have no key. scripts/validate stopped in the v1 suite (scripts/tests) at a sudo password prompt, after the sv2 part passed.
<!-- SECTION:FINAL_SUMMARY:END -->
