---
id: TASK-0028
title: 'sv2: Unix sockets of system daemons are open to a session'
status: To Do
assignee: []
created_date: '2026-11-28T05:30:09Z'
labels:
  - sandbox-hardening
  - sv2
dependencies: []
references:
  - sv2/crates/helper/src/base.sb
  - sv2/crates/common/src/lib.rs
  - >-
    https://github.com/eugene1g/agent-safehouse/blob/490d8d4bdc090029cad647a4057fe4a35115bf28/profiles/20-network.sb
priority: medium
type: enhancement
ordinal: 35000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`base.sb` has `(allow network*)`, which covers connecting to a Unix socket at any path; only the socket's own permissions decide. Several daemons leave theirs open to everyone. Measured on 2026-11-27 (macOS 26.7.3, as the current user under the rendered `base.sb`): a session connects to `/var/run/cupsd` (printing; `lpstat -r` answers), `/var/run/usbmuxd` (devices attached by USB) and `/private/tmp/warp_service` (a VPN client). agent-safehouse allows the network by address family and names the one socket a session needs, `/private/var/run/mDNSResponder`. With `(allow network*)` replaced by: outbound to `(remote ip)`, bind and inbound on `(local ip)`, mDNSResponder, and Unix sockets under the home, the workspace, `/private/tmp` and `/private/var/folders`, all 51 probes pass and `logger` works; cupsd and usbmuxd are refused. A socket another user leaves in `/private/tmp` still connects (the VPN client's does). One thing must change with it: the rule that closes this Mac's SSH port is written `(deny network* ...)`, and a rule that names an operation (`network-outbound`) beats a rule on `network*` wherever it stands, so with the narrower allows port 22 connected until the deny named `network-outbound` too (see the note of 2026-11-27 on TASK-0005). Not measured: a real session, with its tmux server, the account's own launchd sockets, and the browsers of `--chrome` and `--lightpanda`. TASK-0005 cuts `base.sb` into small policies, of which `network` is one; this belongs in that one. Found by TASK-0002.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 In a session, connecting to `/var/run/cupsd` and to `/var/run/usbmuxd` is refused by the sandbox, and a probe that fails if either connects is kept as a test
- [ ] #2 This Mac's SSH port is still refused at every address with the narrower rules, and the helper still reads its port rules back before a session
- [ ] #3 Each Unix socket path that stays allowed names the probe that fails without it, measured as the sandbox user in a real session
- [ ] #4 What is decided about sockets of other users in `/private/tmp` is recorded with the reason
- [ ] #5 The integration suite passes, with tmux, `--chrome` and `--lightpanda`
<!-- AC:END -->
