---
id: TASK-0030
title: 'sv2: an agent definition for Google''s Antigravity CLI (agy), for Gemini 4 Neon'
status: Done
assignee: []
created_date: '2026-11-28T08:35:10Z'
updated_date: '2026-11-28T08:41:53Z'
labels: []
dependencies: []
ordinal: 37000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Gemini CLI stopped serving Google AI Pro and Ultra subscribers on 2026-06-18; Google's Gemini 4 models, Neon among them (2026-11-18), are offered through the Antigravity CLI, agy. sv2 had no definition for it. Checked against agy 1.5.18 (Homebrew cask antigravity-cli) in a scratch HOME, with no account signed in.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 sv2/agents/agy defines a host install (cask antigravity-cli, {brew}/bin/agy) and a guest install (~/.local/bin/agy)
- [x] #2 The bypass flag and empty no_inject are checked against the real program's subcommands
- [x] #3 install.sh installs agy without changing ~/.zshrc or ~/.zprofile
- [x] #4 The built-in definitions unit test covers agy
- [x] #5 sv2 agy and sv2 --native agy start a session on a Mac with sv2 installed and an account that has Neon
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified: agy 1.5.18 --help per subcommand with and without --dangerously-skip-permissions (identical; acp shows it applied); --dangerously-skip-permissions mcp list runs mcp list. --settings refuses unknown keys and refused every name tried for system-prompt text, so no system_prompt. install.sh run end to end in a scratch HOME (umask 077, GNU tools first on PATH): installed 1.5.19 to ~/.local/bin/agy, .zshrc/.zprofile unchanged, nothing left in ~/.cache. the_builtin_definitions passes; 4 other cli unit tests (and some in common/helper) fail identically on unchanged HEAD because this checkout runs as a v1 sandbox account (sandvault-pat). Not done: running from sv2, signing in, seeing Neon answer.

2026-11-28: the user ran it from sv2 and reports it works.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added sv2/agents/agy, a built-in definition for Google's Antigravity CLI (agy), through which Gemini 4 Neon is offered. Host install is the antigravity-cli cask, guest install Google's installer run with a scratch HOME so it cannot edit .zshrc/.zprofile. Bypass flag --dangerously-skip-permissions in front of every subcommand; no system_prompt (agy has no way to take the text); no model pinned. Checked against agy 1.5.18; the user confirmed it works from sv2.
<!-- SECTION:FINAL_SUMMARY:END -->
