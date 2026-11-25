
# Stricter sandbox

1. Per-instance sandbox: only allow writes to the folder tree where sandvault starts
2. Stricter sandbox: https://gist.github.com/eugene1g/ad3ff9783396e2cf35354689cc6473e1
3. Port 22 is reachable from the sandbox. Consider turning off Remote Login or denying the sandbox user in sshd.
4. Run sandbox sessions in the sandbox user's own launchd domain (e.g. `launchctl asuser`). Sandbox processes look up mach services in the host user's login session (`launchctl managername` = Aqua, `manageruid` = the host uid), so every per-user agent the profile allows resolves to the host user's instance. Today that is `com.apple.TrustEvaluationAgent` and the `com.apple.cfprefsd` prefix (`cfprefsd.agent`). It also blocks `com.apple.trustd.agent`, which Go needs for TLS; `SSL_CERT_FILE` in guest `.zshenv` works around that. Moving to `user/<sandbox uid>` would let those names reach the sandbox user's own agents (a `trustd --agent` already runs there). Check that `sudo`, `--ssh`, Keychain, and the browser still work.
