# iOS simulator

This session has an iOS simulator of its own, `$SV_IOS_SIMULATOR_UDID`. It
runs in this account, without a window, and you drive it yourself with
`xcrun simctl` and `iosef`. It is deleted when the session ends. Apps in it
run as this account but outside the sandbox profile.

It boots while the session starts (30 to 90 seconds). Wait for it first:

    xcrun simctl bootstatus "$SV_IOS_SIMULATOR_UDID" -b

Apps, with `xcrun simctl VERB "$SV_IOS_SIMULATOR_UDID" ...`:

- `install PATH/Name.app`, `uninstall BUNDLE_ID`
- `launch BUNDLE_ID`, `terminate BUNDLE_ID`
- `openurl URL`
- `spawn "$SV_IOS_SIMULATOR_UDID" log show --style compact --last 1m`
- `io "$SV_IOS_SIMULATOR_UDID" screenshot FILE.png`: in device pixels,
  three to a point. Take one only to check appearance (colors, layout).

The screen, with `iosef SUBCOMMAND --device "$SV_IOS_SIMULATOR_UDID"`. To
see what is on it use `describe`, not a screenshot: it prints the
accessibility tree, names, roles and positions in points. Run it again
after every launch, tap or swipe.

- `describe` (`--json`, `--depth N`, or `--x N --y N` for one point)
- `tap --name "Sign In"` (also `--identifier`, `--role`), or `--x N --y N`
- `type --text "hello"` into the focused field; printable ASCII
- `swipe --x-start N --y-start N --x-end N --y-end N`

`iosef view` does not work here (there is no window); use the screenshot
above. If `describe` shows nothing of an app that was launched before the
simulator had finished booting, terminate the app and launch it again.
Build for the simulator (`-sdk iphonesimulator`); there is no device.
