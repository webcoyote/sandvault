---
id: TASK-0009
title: 'sv2: tell the agent that its session has a browser'
status: Done
assignee:
  - '@claude'
created_date: '2026-11-27T17:59:06Z'
updated_date: '2026-11-27T21:12:40Z'
labels:
  - sv2
dependencies:
  - TASK-0008
documentation:
  - docs/SANDVAULT2.md
priority: medium
type: feature
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
With `--chrome` or `--lightpanda` an sv2 session gets `SV_BROWSER_ENDPOINT` and nothing tells the agent, so it does not know a browser is there or how to use it. v1 adds `prompts/browser.md` to the agent system prompt. The text should carry the limit Chrome has in sv2: a window that closes ends it, so the agent must attach to the endpoint and not launch a browser. Depends on `system_prompt` support in agent definitions.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 An agent started with `--chrome` or `--lightpanda` is told the endpoint variable and that it must attach, not launch
- [x] #2 An agent started without a browser option gets no browser text
- [x] #3 An agent whose definition has no `system_prompt` still starts, and the banner or doctor says the agent was not told
- [x] #4 A test covers the text being present with a browser option and absent without
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. sv2/crates/cli/src/browser-prompt.md: the text, from v1 prompts/browser.md: CDP endpoint in $SV_BROWSER_ENDPOINT, attach do not launch, check with curl .../json/version; plus a paragraph per browser (Lightpanda: narrower CDP, suggest sv2 --chrome; Chrome: if it is Google Chrome, closing the last page of a context ends it, keep one open). Which Chrome runs is the launcher's choice, so the Chrome text is conditional.
2. main.rs run(): told = the text when a browser was asked for; passed through final_args.
3. Banner: with a browser, say the agent is told, or that it is not (no system_prompt in its definition, or a subcommand that takes none) and where the browser is.
4. Unit tests: text present with --chrome/--lightpanda and absent without; banner for each case; the text reaches claude's and muse's argument vectors.
5. SANDVAULT2.md Browsers section and feature list.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Built: browser-prompt.md (from v1 prompts/browser.md) plus a line per browser, Browser::told() in shell.rs; run() passes it through final_args when a browser option was given; banner line told_line() says told / not told and why (no system_prompt, or a subcommand in no_inject/inject_last) and where the browser is. README-SV2.md and SANDVAULT2.md ("Telling the agent" under Browsers) updated.
Verified: real Claude Code 2.1.326 (Homebrew, no v1 wrapper, SV_BROWSER_ENDPOINT unset), asked without tools whether its system prompt names a browser: with the Lightpanda text "SV_BROWSER_ENDPOINT, Lightpanda", without "NO". The text's Playwright example (playwright-core 1.59.1) loaded example.com through Lightpanda 1.0.6, and the browser was still running after. fmt, clippy -D warnings, cli tests 100/104 (the same 4 environment failures), sv2-common 12/12. New tests: the_agent_is_told_about_its_browser, the_banner_says_whether_the_agent_was_told.
NOT verified: an sv2 session end to end (needs a Mac with sv2 installed); pi and muse reading the text (no provider signed in); --chrome.

scripts/validate passed and the text was seen in an sv2 session, run by the user on a Mac with sv2 installed, 2026-11-27.

--chrome also checked by the user in an sv2 session, 2026-11-27.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
With --chrome or --lightpanda, sv2 now tells the agent through its definition's system_prompt that a browser is at $SV_BROWSER_ENDPOINT, to connect over CDP and not launch one, and to leave it running; plus a line for the browser asked for (Lightpanda: narrower CDP, try sv2 --chrome; Chrome: Google Chrome may end when the last page of a context closes). The banner says whether the agent was told, and if not why and where the browser is. Without a browser option nothing changes. Verified with Claude Code 2.1.326 (told vs control), the text's Playwright example against Lightpanda 1.0.6, unit tests, and the user's scripts/validate and sessions with --lightpanda and --chrome. Not seen: pi and muse reading the text (no provider signed in).
<!-- SECTION:FINAL_SUMMARY:END -->
