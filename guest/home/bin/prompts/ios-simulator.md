# iOS Simulator Automation

An iOS Simulator is available for app testing. It's available via HTTP at:

    $SV_IOS_SIMULATOR_ENDPOINT

All endpoints return plain text by default. Append `?format=json` to any URL for JSON responses.

## CRITICAL: Always use /describe, not /view

**Your default tool for understanding the simulator UI is `GET /describe`, NOT screenshots.** After launching an app or performing any interaction, call `/describe` to get the accessibility tree. This returns element names, roles, positions, and states as structured text — everything you need to understand the UI and decide what to tap next.

**Do NOT call `/view` or `/view_pixels` unless you have a specific reason to check visual appearance** (e.g., verifying colors, layout, or images). Taking a screenshot to "see what the app looks like" is not a valid reason — `/describe` tells you what's on screen faster and more reliably.

## Endpoints

### GET /ready

Returns `OK` when the simulator is booted and ready for commands. Returns HTTP 503 while still booting.

### GET /describe

Accessibility tree or element(s) at a coordinate. Optional query parameters: `?x=&y=&depth=`. Omit all parameters to get the full tree. **This is your PRIMARY tool for all UI interaction.** Returns element names, roles, positions, and states as structured text. Always call this first after launching an app, tapping a button, or performing any action. Only fall back to `/view` when you need to verify visual appearance (colors, layout, images).

### POST /tap

Tap an element or point. Body: `{"name":"..."}`, `{"identifier":"..."}`, `{"role":"..."}` (any combination narrows the match), OR `{"x":N,"y":N}` for coordinate-based tap.

### POST /type

Type into the currently focused field. Body: `{"text":"..."}`.

### POST /swipe

Swipe gesture. Body: `{"from_x":N,"from_y":N,"to_x":N,"to_y":N}`.

### GET /view

Take a screenshot in points (e.g. 393x852). Returns the filename of the saved image. Generates small, lightweight images that are fast to transfer. Use this to verify visual layout or appearance after using `/describe` to understand the UI structure.

### GET /view_pixels

Take a screenshot in native pixels (3x points, e.g. 1179x2556). Returns the filename of the saved image. Useful when you need high-quality, full-resolution images, but the files are quite large. Use `/view` instead unless pixel-level detail is needed.

### POST /install

Install an `.app` bundle. Body: `{"app_path":"/Users/Shared/..."}`.

### POST /uninstall

Uninstall an app. Body: `{"bundle_id":"com.foo.bar"}`.

### POST /launch

Launch an app. Body: `{"bundle_id":"com.foo.bar"}`.

### POST /terminate

Terminate an app. Body: `{"bundle_id":"com.foo.bar"}`.

### POST /openurl

Open a deep link or URL. Body: `{"url":"myapp://path"}`.

### GET /log

Recent simulator system log. Optional query: `?last=30s` (or `Nm`/`Nh`, default `1m`).

## Important

- **ALWAYS use `/describe` first — NEVER start with `/view`.** After every app launch, tap, swipe, or navigation action, call `/describe` to see the current UI state. Do not take screenshots to figure out what's on screen — that's what `/describe` is for. `/view` is only for verifying visual details (colors, spacing, images) after you already understand the UI structure via `/describe`.
- The simulator boots in the background (~30-90s). Try your request — if it returns `503 Service Unavailable`, poll `GET /ready` until it returns `OK`, then retry.
- Do NOT try to run `xcrun simctl` or `iosef` directly — they only work on the host. Use the HTTP endpoints above.
- `.app` bundles passed to `/install` must live under `/Users/Shared/` (the bridge rejects other paths).
- The simulator is fresh for this session and is deleted on exit.
