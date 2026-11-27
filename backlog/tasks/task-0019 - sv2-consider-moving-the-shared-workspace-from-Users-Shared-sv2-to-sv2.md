---
id: TASK-0019
title: 'sv2: consider moving the shared workspace from /Users/Shared/sv2 to /sv2'
status: To Do
assignee: []
created_date: '2026-11-27T19:15:40Z'
labels:
  - sv2
dependencies: []
documentation:
  - docs/SANDVAULT2.md
type: spike
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
sv2 keeps every workspace under `/Users/Shared/sv2/<user>/<profile>/`; `sv2/` and `sv2/<user>/` are root-owned and not world-writable. The question is whether the root of that tree should be `/sv2`. The reason for wanting the move was not recorded when this task was created; ask the user before weighing it. Possible reasons (guesses by the task author, not the user): shorter paths in every session, and getting out from under `/Users/Shared`, which is world-writable with the sticky bit and shared with unrelated software.

Constraints to check before deciding, none measured yet:
- On current macOS the root of the system volume is read-only, so `/sv2` cannot be made with `mkdir`. The known route is an entry in `/etc/synthetic.conf`, which takes effect at boot (or through `apfs.util -t`) and gives either an empty directory or a symlink to a real location such as under `/System/Volumes/Data`. Which form works, who owns it, and whether it survives OS updates need measuring.
- With a symlink, the real path differs from the path the user types; the sandbox profile, `sv2 repair` (which resolves paths inside the workspace tree) and the working-directory mapping in the session all compare paths.
- `sv2 install` would gain a root-owned artifact (the `synthetic.conf` entry) that needs a version, an upgrade path and removal on `uninstall`, and may need a reboot before first use.
- Existing installs have workspaces with user files under `/Users/Shared/sv2`; `profile remove` never deletes them, so a move needs a migration story.
- The path is hard-coded in the helper and the CLI, in the three test suites under `sv2/tests`, and in `README-SV2.md`, `docs/SANDVAULT2.md` and `docs/DEPLOY-KEYS.md`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The reason for the move is recorded in the task or the design doc
- [ ] #2 Whether a root-owned, not world-writable `/sv2` can be created on current macOS is measured, with how it is created, whether a reboot is needed, and whether it survives an OS update
- [ ] #3 The effect on the sandbox profile, `sv2 repair` and the session working directory is checked, in particular if `/sv2` is a symlink
- [ ] #4 What happens to existing workspaces under `/Users/Shared/sv2` is stated
- [ ] #5 A decision to move or to stay is recorded in `docs/SANDVAULT2.md` under "Names and ownership" with the reasons, and a follow-up task exists if the decision is to move
<!-- AC:END -->
