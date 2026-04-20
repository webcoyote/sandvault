# iOS Simulator automation: tool comparison and rationale

This document explains the iOS-simulator automation landscape, why sandvault
splits work between `xcrun simctl` and `iosef`, where each tool's runtime
model differs, and the constraints that drove the design (especially around
asynchronous boot and screenshot capture).

This is a reference document. For agent-facing usage instructions see
`guest/home/bin/prompts/ios-simulator.md`.

---

## The three tools

All three eventually reach the same private framework
(`CoreSimulator.framework`) that Apple uses internally. The differences are
in what they expose, how they run, and what they're optimized for.

### `xcrun simctl` (Apple)

Apple's official CLI. Each invocation is a short-lived process that loads
`CoreSimulator.framework` and calls into it. No daemon. State lives in
`~/Library/Developer/CoreSimulator/Devices/<UDID>/`.

**Capabilities used by sandvault:** create / boot / shutdown / delete
devices; install / uninstall / launch / terminate apps; openurl;
log show; framebuffer screenshots (`simctl io <UDID> screenshot`).

**Cannot do:** UI automation. There is no public Apple API for
synthesizing taps, types, swipes, or reading the accessibility tree
from outside a test target. simctl literally has no subcommand for any
of these.

### `iosef`

Independent Swift binary, links the same private `CoreSimulator.framework`
plus undocumented HID and accessibility APIs. Each invocation is also
short-lived. Per-session state lives in `~/.iosef/state.json` (global) or
`./.iosef/state.json` (when called with `--local`).

**Capabilities used by sandvault:** UI interaction (tap, type, swipe),
accessibility tree (describe).

**Cannot do (under our constraints):** headless screenshots. iosef's
`view` subcommand captures via the macOS WindowServer, which requires a
visible Simulator.app window. See "Screenshot capture" below.

### `idb` (Facebook / Meta)

Long-lived daemon (`idb_companion`) plus a separate CLI (`idb`). The
daemon links `CoreSimulator.framework` and `FBSimulatorControl` and stays
resident; the CLI is a thin gRPC client.

**Capabilities:** the union of simctl and iosef plus more — file push/pull
into app sandboxes, push notifications, location, crash log handling,
video recording. Notably also supports **real devices** via a separate
`idb_companion` running on a host plugged into the device.

**Why not used by sandvault:** see "Why not idb" below.

---

## Why the simctl / iosef split

For each capability, we use whichever tool can do the job with the least
fuss:

| Capability                       | Tool we use      | Reason                                                                                  |
|----------------------------------|------------------|------------------------------------------------------------------------------------------|
| Boot, shutdown, delete           | `simctl`         | Only simctl exposes these; iosef's `start` would also create a device, but we want explicit control over device type and runtime selection. |
| Install / launch / terminate     | `simctl`         | iosef has equivalents, but using simctl avoids one more dependency for a stable Apple-supported path. |
| openurl / log                    | `simctl`         | iosef has no equivalent.                                                                 |
| Tap / type / swipe               | `iosef`          | simctl cannot do this. There is no public alternative.                                   |
| Accessibility tree (describe)    | `iosef`          | Same reason.                                                                             |
| Screenshot                       | `simctl io ...`  | iosef's `view` requires a visible Simulator.app window; ours are headless by default. See below. |

The split keeps sandvault's iosef dependency narrow: only four endpoints
of the bridge actually call iosef. If iosef breaks against a new Xcode
release, app lifecycle and screenshots keep working through simctl.

---

## Asynchronous boot

CoreSimulator's `simctl boot` returns almost immediately, but the
simulator inside takes 30–90 seconds to finish booting (springboard,
launchd, system services). Until that completes, taps and AX queries are
unreliable or return errors.

`sv` does not wait for boot. The original implementation called
`xcrun simctl bootstatus -b` synchronously, which blocked the user's
shell for the full boot duration before any agent could even start.
That was unacceptable for a tool meant to be invoked interactively
("`sv --ios claude`") — the user wants their prompt back fast.

The current model:

1. `sv` calls `simctl boot` and returns once the bridge is listening
   (~50 ms). The user gets their shell or AI agent immediately.
2. The bridge spawns a background thread that runs
   `simctl bootstatus <UDID> -b` and sets a `READY` flag when it
   completes (or `READY_ERROR` on failure).
3. `GET /ready` is always available and returns `OK` (200) when
   booted, or 503 with an error message while still starting.
4. All other endpoints return `503 Service Unavailable` until ready.
   Agents poll `/ready` instead of retrying random endpoints.

Agents are expected to poll `/ready` until it returns 200 before
issuing interaction requests. The tool prompt documents this loop.

---

## Multiple devices and parallel sandvault sessions

sandvault spawns one fresh scratch simulator per session
(`sandvault-<SV_SESSION_ID>`). Multiple `sv --ios` invocations
run side by side without coordinating beyond CoreSimulator itself.

CoreSimulator handles concurrent simulators fine — Apple's CI farms run
hundreds in parallel on a single host. The constraints that show up at
the sandvault layer:

**Per-device isolation in CoreSimulator:** automatic. Each `SimDevice`
has its own UDID, file system root, and process tree. simctl operations
take the UDID as a positional argument; nothing leaks across devices.

**iosef's state.json:** *not* automatically isolated. Without
intervention, every iosef invocation by every parallel bridge would
write to the shared `~/.iosef/state.json`. Even though our bridges
always pass `--device <UDID>` explicitly (so iosef never reads the
state file to *resolve* the device), iosef may still rewrite the file
on each call as a side effect. Concurrent rewrites could corrupt it,
which would silently affect any host-side `iosef` use the user does
outside sandvault.

Sandvault's fix: the bridge creates a fresh tempdir at startup,
`chdir`s into it, and passes `--local` to every iosef invocation. Each
bridge then has an independent `./.iosef/state.json` rooted in its own
scratch dir. The dir is removed on bridge exit. The user's own
`~/.iosef/state.json` is never touched.

**Real devices:** unsupported. simctl and iosef both target simulators
only. Driving a real iPhone requires a different runtime model: the
device must be attached over USB or paired over Wi-Fi, and a host-side
agent (idb_companion, or Apple's own `devicectl`/`xcodebuild`) must be
running to relay events. Sandvault's per-session "create scratch
device, boot, delete on exit" pattern doesn't translate — real devices
are persistent, owned by the user, and shouldn't be reset between
sessions. If real-device support is ever needed, it would be a separate
flag (`--ios-device`?) with a different lifecycle model and almost
certainly built on idb or `devicectl`, not simctl/iosef.

---

## Why not idb

idb has the broadest capability surface of the three, including real
devices, file push/pull, video recording, and the things iosef does.

**Selector-based queries.** iosef has first-class
`--name` / `--role` / `--identifier` selectors on `tap`, `type`,
`find`, `wait`, etc. idb has no equivalent — you query the AX tree
yourself and tap by coordinates. For LLM agents, semantic selectors
("tap the button labeled Sign In") are a substantial ergonomic
advantage.

If iosef ever stops being maintained or breaks on a new Xcode release
that we can't easily patch around, the fallback is idb. Replacing
iosef with idb in the bridge would touch five endpoints (`view`,
`describe`, `tap`, `type`, `swipe`), roughly 50 lines of Python.

---

## Screenshot capture: framebuffer vs. WindowServer

This is the subtlest of the design decisions and the source of a real
bug we hit during initial implementation.

There are two ways to get pixels out of a running simulator:

### WindowServer capture (iosef's `view`)

`iosef view` reads pixels via `IOSurface` from the macOS WindowServer
— the same mechanism that an off-screen screenshot of a regular Mac
window would use. You get the error iosef returns in headless mode:

> Error: No Simulator window found for device '...'. Is the Simulator
> running and visible?

This is a constraint of the WindowServer APIs, not iosef being lazy:
the OS exposes capture only for windows that exist on the display
hierarchy.

**Property of WindowServer captures:** they come out at iOS-point
resolution. iPhone 15 Pro returns a 393×852 PNG, not 1179×2556. This
is intentional — iosef advertises "1 pixel = 1 iOS point" so an agent
can read a screenshot, identify a button at pixel `(195, 420)`, and
then `iosef tap --x 195 --y 420` without doing any coordinate math.

### Framebuffer capture (`simctl io ... screenshot`)

`simctl io <UDID> screenshot` doesn't go through the WindowServer at
all. It calls `SimDeviceIOClient.screenshot` on `CoreSimulator`
directly, which asks the simulated device to emit a frame from its
own video pipeline. The simulator process renders frames whether or
not anyone is watching them. No window required.

**Property of framebuffer captures:** they come out at *native*
device pixel resolution. iPhone 15 Pro returns 1179×2556 (3× scale).
This is what the simulated device would actually output to a physical
display.

### What sandvault uses

Sandvault boots simulators headlessly by default (`simctl boot` only,
no `open -a Simulator`). The `--ios-gui` flag opens
Simulator.app for visual debugging, but headless is the default
because:

- It's faster (no GUI to render).
- It avoids cluttering the user's display with a window they didn't
  ask for.
- It works in CI where there may be no display at all.

The bridge exposes two screenshot endpoints:

- **`/view`** — calls `iosef view` (without `--output`, which doesn't
  work when the Simulator window is hidden). Returns a PNG at
  iOS-point resolution (e.g. 393×852 for iPhone 15 Pro). Coordinates
  in the image match the point coordinates returned by `/describe`
  and accepted by `/tap`.

- **`/view_pixels`** — calls `simctl io ... screenshot`, which
  captures from the simulator framebuffer directly and works headless.
  Returns a PNG at native pixel resolution (3× points, e.g.
  1179×2556). Useful for visual regression testing or when
  pixel-perfect detail is needed.

### The coordinate-mismatch caveat

If an agent visually identifies a button at pixel `(600, 1200)` in a
`/view_pixels` screenshot and tries `/tap` with those coordinates, it
misses — those are pixel coordinates, not the logical-point
coordinates that `/tap` expects. The agent would have to divide by the
device's scale factor (3 for most modern iPhones) before passing to
tap. Use `/view` instead to get a screenshot whose coordinates align
with `/tap` directly.

In practice the AX tree is the agent's primary tap-coordinate source.
`GET /describe` returns element positions already in iOS points, and
the agent uses those directly. Screenshots are mostly for
sanity-checking and debugging.
