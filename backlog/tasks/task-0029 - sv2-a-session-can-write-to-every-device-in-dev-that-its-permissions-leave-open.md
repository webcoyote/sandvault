---
id: TASK-0029
title: >-
  sv2: a session can write to every device in /dev that its permissions leave
  open
status: To Do
assignee: []
created_date: '2026-11-28T05:30:09Z'
labels:
  - sandbox-hardening
  - sv2
dependencies: []
references:
  - sv2/crates/helper/src/base.sb
  - >-
    https://github.com/eugene1g/agent-safehouse/blob/490d8d4bdc090029cad647a4057fe4a35115bf28/profiles/10-system-runtime.sb
priority: low
type: enhancement
ordinal: 36000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`base.sb` allows `file-write*` and `file-ioctl` on all of `/dev`, apart from raw disks and `bpf`, and leaves the rest to the permissions of each device. Measured on 2026-11-27 (macOS 26.7.3, as the current user under the rendered `base.sb`): `/dev/console`, `/dev/cu.debug-console`, `/dev/aes_0`, `/dev/nfsclnt` and `/dev/autofs_nowait` open for writing; all are writable by everyone (`/dev/console` has mode `--w--w-` for group and others). agent-safehouse names the devices a session writes to: `/dev/fd`, `stdout`, `stderr`, `null`, `tty`, `ptmx`, `dtracehelper`, and the names starting `/dev/tty` and `/dev/pty`; ioctl only on the terminal ones. With both rules of `base.sb` narrowed to that list, all 51 probes pass and the five devices above are refused. Not measured: a terminal in a real session (raw mode, window size, tmux, `script`), where the session has a terminal of its own; the probes run without one. The list uses a pattern (`regex`), which the built-in policies may use and a policy of the user may not (docs/SANDVAULT2-POLICIES.md). Found by TASK-0002.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 In a session, opening `/dev/console` and `/dev/cu.debug-console` for writing is refused by the sandbox, and a probe that fails if either opens is kept as a test
- [ ] #2 Each device that stays writable, and each that stays open to ioctl, names the probe that fails without it, measured as the sandbox user in a real session
- [ ] #3 An agent that puts the terminal in raw mode, tmux, and a change of window size work in a real session
<!-- AC:END -->
