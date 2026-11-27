---
id: TASK-0024
title: >-
  sv2: system prompt text for muse, and muse subcommands that refuse a flag in
  front
status: Done
assignee:
  - '@claude'
created_date: '2026-11-27T20:35:49Z'
updated_date: '2026-11-27T20:54:31Z'
labels:
  - sv2
dependencies:
  - TASK-0008
documentation:
  - docs/SANDVAULT2.md
priority: medium
type: feature
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-0008 gave definitions `system_prompt = { flag = ... }`, which only claude and pi use. muse and herdr were added after it (TASK-0010). herdr is a multiplexer and has no system prompt. muse 1.4.25 has no flag for it: it takes the text as a setting, `-c run.system_prompt_append=<JSON string>` (an unknown key or a value that is not JSON is refused with exit 2), so the flag form cannot express it. Measured while looking: muse refuses every option in front of a subcommand ("`mcp` is a command, not a root option"), and its definition lists no subcommands, so `sv2 muse exec ...`, `sv2 muse mcp ...` and the rest fail today with `--yolo` in front. The browser text (TASK-0009) needs both fixed before it can reach muse.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A definition can say its agent takes system prompt text as a setting (an option and a key), and sv2 passes the text JSON-encoded as KEY=VALUE after that option
- [x] #2 Text of several lines, quotes, dollar signs and a leading dash arrives as one valid JSON string; unit tests cover it
- [x] #3 muse has the key, checked against muse 1.4.25: the key is accepted and a misspelt one refused
- [x] #4 Every muse subcommand is in no_inject or inject_last, each placement checked against the real program, so neither the bypass flag nor the text breaks one
- [x] #5 Definitions with the flag form, or without the key, run as before
- [x] #6 docs/SANDVAULT2.md and the agent new template describe the setting form and record that herdr takes no text
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. agent.rs: system_prompt is { flag } or { config = { flag, key } }; Definition.prompt becomes an enum (Flag / Setting). Setting passes FLAG and KEY=<JSON string> as two arguments. Long options only (muse: --config, not -c). Key: lowercase ASCII, digits, ._- . Same refusals as the flag form (both kinds, bypass flag clash, unknown keys).
2. A small JSON string encoder (no new dependency): escapes quote, backslash, control characters.
3. muse/agent.toml: system_prompt config key run.system_prompt_append; no_inject and inject_last from measurement against muse 1.4.25 (exec, resume, attach accept --yolo after their arguments; every other subcommand refuses any option in front or after).
4. Unit tests: parsing and refusals, argument vector and JSON for text with lines/quotes/$/leading dash/control characters, muse built-in lists.
5. Template, module doc, SANDVAULT2.md (setting form, muse measurements, herdr takes no text, echo-provider refusal).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Built: system_prompt = { config = { flag, key } } beside { flag }; Definition.prompt is enum Prompt (Flag / Setting); Setting passes FLAG and KEY=<JSON string> (hand-written encoder, no new dependency; escapes quote, backslash, all C0 controls and DEL). muse: --config run.system_prompt_append, inject_last [exec, resume, attach], no_inject the other 21 subcommands. Template and SANDVAULT2.md ("Text as a setting") updated; herdr recorded as taking no text.
Measured against muse 1.4.25 (Homebrew cask, run directly): the exact value from the unit test is accepted; misspelt key and non-JSON value refused (exit 2); echo provider refuses a run with the setting ("provider does not support base instructions"); ${...} is not substituted for this key. Every subcommand: --help alone exits 0; with --yolo after, only exec/resume/attach accept it (compared against an unknown flag); with any option in front, all refuse.
Verified here (sandbox user, v1 sandbox): cargo fmt, clippy -D warnings, cli unit tests 98/102 (the 4 failures fail identically on b429ad3: fixtures in a TMPDIR owned by sandvault-pat are refused by design), sv2-common 12/12 incl. obelus check, helper 45/47 (2 sandbox-exec nesting failures, untouched). Mutation: dropping muse's lists fails the_builtin_definitions.
NOT verified: scripts/validate end to end and the integration suite (need a Mac with sv2 installed, run as the host user); no Meta provider signed in, so text was not seen to reach a model.

scripts/validate and the integration suite passed, run by the user on a Mac with sv2 installed, 2026-11-27.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Definitions can say an agent takes system prompt text as a setting: system_prompt = { config = { flag, key } } beside { flag }. sv2 passes the option and KEY=<text as a JSON string>. muse uses it (--config run.system_prompt_append), measured against muse 1.4.25. Fixed on the way: muse refuses any option in front of a subcommand, so sv2 muse exec/mcp/... failed with --yolo in front; exec, resume and attach now get --yolo after their arguments, and the other 21 subcommands get neither flag nor text. herdr is a multiplexer and takes no text. Unit tests cover parsing, refusals, the JSON encoding and the muse lists; the user ran scripts/validate and the integration suite on a Mac with sv2 installed. Not seen: the text reaching a model (no provider signed in); a provider without base instructions refuses a run that carries text.
<!-- SECTION:FINAL_SUMMARY:END -->
