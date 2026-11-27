---
id: TASK-0009
title: 'sv2: tell the agent that its session has a browser'
status: To Do
assignee: []
created_date: '2026-11-27T17:59:06Z'
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
- [ ] #1 An agent started with `--chrome` or `--lightpanda` is told the endpoint variable and that it must attach, not launch
- [ ] #2 An agent started without a browser option gets no browser text
- [ ] #3 An agent whose definition has no `system_prompt` still starts, and the banner or doctor says the agent was not told
- [ ] #4 A test covers the text being present with a browser option and absent without
<!-- AC:END -->
