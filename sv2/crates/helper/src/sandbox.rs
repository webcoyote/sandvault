//! Renders a profile's `sandbox-exec` file from the embedded base profile.
//!
//! The base profile is default deny; every allow in it names the probe in
//! `tests/profile-probes.sh` that fails without it. The `profile_probes`
//! test below runs those probes under the rendered profile as the current
//! user, with no root: `cargo test -p sv2-helper -- --ignored profile_probes`.

use sv2_common::{
    account_name, home_path, local_port_rule, ssh_ports, valid_host_user, valid_profile,
    workspace_path, workspace_user_dir, PROFILE_PARAM_IOS, SANDBOX_PROFILE_VERSION, WORKSPACE_ROOT,
};

const BASE: &str = include_str!("base.sb");

/// The values go inside SBPL string literals, so they must not contain a
/// quote or a backslash. The name validators already guarantee that; this is
/// the check at the point of use.
fn sbpl_safe(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-' | b'/'))
}

/// `ssh_ports` are the ports of this Mac's sshd that were found; port 22 is
/// closed whether or not it is among them.
pub fn render(host_user: &str, profile: &str, ssh_ports: &[u16]) -> Result<String, String> {
    if !valid_host_user(host_user) || !valid_profile(profile) {
        return Err("cannot render a sandbox profile for an invalid name".to_string());
    }
    let account = account_name(host_user, profile);
    render_paths(
        WORKSPACE_ROOT,
        &workspace_user_dir(host_user),
        &workspace_path(host_user, profile),
        &home_path(&account),
        ssh_ports,
    )
}

/// What `sandbox-exec` is given before `-f FILE`: whether the session may
/// use the iOS simulator. Always given, as 1 or 0; the file reads it.
pub fn ios_param(ios: bool) -> [String; 2] {
    [
        "-D".to_string(),
        format!("{PROFILE_PARAM_IOS}={}", u8::from(ios)),
    ]
}

/// The base profile with these paths filled in.
fn render_paths(
    workspace_root: &str,
    workspace_user_dir: &str,
    workspace: &str,
    home: &str,
    found_ssh_ports: &[u16],
) -> Result<String, String> {
    let subs = [
        ("__PROFILE_VERSION__", SANDBOX_PROFILE_VERSION.to_string()),
        ("__IOS_PARAM__", PROFILE_PARAM_IOS.to_string()),
        ("__WORKSPACE_ROOT__", workspace_root.to_string()),
        ("__WORKSPACE_USER_DIR__", workspace_user_dir.to_string()),
        ("__WORKSPACE__", workspace.to_string()),
        ("__HOME__", home.to_string()),
    ];
    let mut out = BASE.to_string();
    for (key, value) in &subs {
        if !sbpl_safe(value) {
            return Err(format!("unsafe value for {key} in sandbox profile"));
        }
        if !out.contains(key) {
            return Err(format!("base sandbox profile has no {key}"));
        }
        out = out.replace(key, value);
    }
    const RULES: &str = "__SSH_PORT_RULES__";
    if out.matches(RULES).count() != 1 {
        return Err(format!("base sandbox profile needs {RULES} once"));
    }
    let rules: Vec<String> = ssh_ports(found_ssh_ports.iter().copied())
        .into_iter()
        .map(local_port_rule)
        .collect();
    out = out.replace(RULES, &rules.join("\n"));
    if out.contains("__") {
        return Err("sandbox profile has an unfilled placeholder".to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn fills_every_placeholder() {
        let text = render("pat", "work", &[]).unwrap();
        assert!(!text.contains("__"));
        assert_eq!(
            sv2_common::stamped_version(&text, sv2_common::SANDBOX_PROFILE_STAMP),
            Some(SANDBOX_PROFILE_VERSION)
        );
        assert!(text.contains("(subpath \"/Users/sv2/pat/work\")"));
        assert!(text.contains("(subpath \"/Users/sv2-pat-work\")"));
        assert!(text.contains("(literal \"/Users/sv2\")"));
        assert!(text.contains("(literal \"/Users/sv2/pat\")"));
        // Nothing of /Users/Shared, which anybody can write.
        assert!(!text.contains("\"/Users/Shared"));
        // v1's text: nothing of the bash heredoc survives.
        assert!(!text.contains('$'));
        assert!(!text.contains("sandvault-"));
    }

    #[test]
    fn profiles_are_distinct_per_user_and_profile() {
        let a = render("pat", "work", &[]).unwrap();
        let b = render("pat-work", "default", &[]).unwrap();
        assert!(!a.contains("/Users/sv2/pat-work/"));
        assert!(!b.contains("\"/Users/sv2/pat/work\""));
        assert!(!b.contains("\"/Users/sv2-pat-work\""));
    }

    #[test]
    fn refuses_invalid_names() {
        for (user, profile) in [
            ("pat\"", "work"),
            ("pat", "wo\"rk"),
            ("pat) (allow default", "work"),
            ("../x", "work"),
            ("pat", "a/b"),
            ("", "work"),
            ("pat", ""),
        ] {
            assert!(render(user, profile, &[]).is_err(), "{user:?} {profile:?}");
        }
    }

    /// One rule a port, port 22 always, and what the helper reads back
    /// before a session is what was asked for.
    #[test]
    fn closes_port_22_and_the_ssh_ports_found() {
        use sv2_common::{local_ports_closed, local_ports_left_open};
        let plain = render("pat", "work", &[]).unwrap();
        assert_eq!(local_ports_closed(&plain), [22]);
        assert_eq!(plain, render("pat", "work", &[22, 22]).unwrap());
        let moved = render("pat", "work", &[2222, 22, 2200, 2222]).unwrap();
        assert_eq!(local_ports_closed(&moved), [22, 2200, 2222]);
        assert_eq!(
            local_ports_left_open(&moved, &[22, 2222]),
            Vec::<u16>::new()
        );
        assert_eq!(local_ports_left_open(&plain, &[22, 2222]), [2222]);
        // Each after the allow it cuts a piece out of: later rules win.
        let allow = moved.find("(allow network*)").unwrap();
        assert!(moved.find("localhost:22\"").unwrap() > allow);
    }

    /// A profile with the extra rules loads, and they do what they say: as
    /// the current user, a listener of this test's own is refused under a
    /// profile that closes its port, and reached under one that does not.
    #[test]
    fn a_closed_port_is_refused_and_another_is_not() {
        use std::net::{TcpListener, TcpStream};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = std::env::temp_dir().join(format!("sv2-port-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let connects = |ports: &[u16]| {
            let path = dir.join(format!("p{}.sb", ports.len()));
            std::fs::write(&path, render("pat", "work", ports).unwrap()).unwrap();
            Command::new("/usr/bin/sandbox-exec")
                .args(ios_param(false))
                .arg("-f")
                .arg(&path)
                .args(["/usr/bin/nc", "-z", "-G", "5", "127.0.0.1"])
                .arg(port.to_string())
                .output()
                .unwrap()
                .status
                .success()
        };
        let open = connects(&[]);
        let closed = connects(&[port]);
        let _ = std::fs::remove_dir_all(&dir);
        // Not vacuous: without the sandbox the listener is there.
        assert!(TcpStream::connect(("127.0.0.1", port)).is_ok());
        assert!(open, "a port the profile does not name was refused");
        assert!(!closed, "a port the profile closes was reached");
    }

    /// The profile is default deny, and says so once.
    #[test]
    fn the_profile_denies_by_default() {
        let text = render("pat", "work", &[]).unwrap();
        let rules: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with(";;"))
            .collect();
        assert!(rules.contains(&"(deny default)"));
        assert!(!rules.iter().any(|l| l.contains("(allow default)")));
        // The Mach services a session may reach, and no others.
        let names: Vec<&str> = rules
            .iter()
            .filter(|l| l.starts_with("(global-name"))
            .map(|l| l.trim_end_matches(')'))
            .collect();
        assert_eq!(
            names,
            [
                "(global-name-prefix \"com.apple.system.\"",
                "(global-name \"com.apple.TrustEvaluationAgent\"",
                "(global-name \"com.apple.trustd.agent\"",
                "(global-name \"com.apple.SecurityServer\"",
                "(global-name \"com.apple.FSEvents\"",
                // Headless Chromium: lookup and registration, per launch.
                "(global-name-prefix \"org.chromium.Chromium.MachPortRendezvousServer.\"",
                // Google Chrome with a debugging port (sv2 --chrome).
                "(global-name-prefix \"com.google.Chrome.MachPortRendezvousServer.\"",
                "(global-name \"com.apple.coreservices.launchservicesd\"",
                // Only when the helper passes SV2_IOS=1 (sv2 run --ios).
                "(global-name-prefix \"com.apple.CoreSimulator.\"",
                // The explicit denies that follow the allow.
                "(global-name \"com.apple.DiskArbitration.diskarbitrationd\"",
                "(global-name \"com.apple.NetAuthAgent\"",
                "(global-name \"com.apple.NetAuthSysAgent\"",
                "(global-name \"com.apple.appleeventsd\"",
            ]
        );
    }

    /// Runs tests/profile-probes.sh under the rendered profile, as the
    /// current user, with a scratch home and workspace. This is how the
    /// profile is worked on: change base.sb, run this, read what failed.
    ///
    /// Ignored by default because it uses the network and takes a while;
    /// scripts/tests-sv2 runs it. The same probes run in a real session, as
    /// the sandbox user in its own launchd domain, in tests/integration.
    #[test]
    #[ignore = "uses the network; run by scripts/tests-sv2"]
    fn profile_probes() {
        let probes = include_str!("../../../tests/profile-probes.sh");
        let root = std::env::temp_dir().join(format!("sv2-probes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // Resolved: the profile names paths as the kernel sees them.
        let root = root.canonicalize().unwrap();
        let at = |rest: &str| root.join(rest).to_str().unwrap().to_string();
        for dir in ["home", "tmp", "ws/user/work"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        // A browser, if this user has one of Playwright's: the probe looks
        // for it in the home it is given, so it gets a copy there.
        if let Some(shell) = playwright_headless_shell() {
            let cache = root.join("home/Library/Caches/ms-playwright");
            std::fs::create_dir_all(&cache).unwrap();
            let copied = Command::new("/bin/cp")
                .arg("-R")
                .arg(&shell)
                .arg(&cache)
                .status()
                .unwrap();
            assert!(copied.success(), "could not copy {}", shell.display());
        }
        let text = render_paths(
            &at("ws"),
            &at("ws/user"),
            &at("ws/user/work"),
            &at("home"),
            &[],
        )
        .expect("the scratch paths do not fit in a profile");
        std::fs::write(root.join("profile.sb"), text).unwrap();
        let user = std::env::var("USER").unwrap_or_else(|_| "nobody".into());
        // As a session without --ios has it, then as one with.
        for ios in [false, true] {
            let out = Command::new("/usr/bin/sandbox-exec")
                .args(ios_param(ios))
                .arg("-f")
                .arg(root.join("profile.sb"))
                .args(["/bin/bash", "-c", probes])
                .env_clear()
                .env("HOME", at("home"))
                .env("USER", &user)
                .env("LOGNAME", &user)
                .env("SHELL", "/bin/zsh")
                .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
                .env("TMPDIR", at("tmp"))
                .env("SV2_WORKSPACE", at("ws/user/work"))
                // Outside a session there is no account behind HOME.
                .env("SV2_PROBE_NO_ACCOUNT", "1")
                .env("SV2_PROBE_IOS", if ios { "1" } else { "0" })
                .current_dir(root.join("ws/user/work"))
                .output()
                .unwrap();
            let report = String::from_utf8_lossy(&out.stdout);
            let failed: Vec<&str> = report.lines().filter(|l| l.starts_with("FAIL")).collect();
            let passed = report.lines().filter(|l| l.starts_with("ok ")).count();
            for line in report.lines().filter(|l| l.starts_with("skip")) {
                eprintln!("{line}");
            }
            assert!(
                failed.is_empty(),
                "probes failed under the profile:\n{}\n{}",
                failed.join("\n"),
                String::from_utf8_lossy(&out.stderr)
            );
            // Not vacuous: the script ran to its end.
            assert!(passed >= 35, "only {passed} probes passed:\n{report}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The newest `chromium_headless_shell-N` directory in this user's
    /// Playwright cache, if there is one.
    fn playwright_headless_shell() -> Option<std::path::PathBuf> {
        let cache =
            std::path::Path::new(&std::env::var_os("HOME")?).join("Library/Caches/ms-playwright");
        let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(cache)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("chromium_headless_shell-"))
            })
            .collect();
        found.sort();
        found.pop()
    }

    /// The same check the helper makes before renaming a profile into place:
    /// the rendered text must load. Needs no privileges.
    #[test]
    fn rendered_profile_loads() {
        let text = render("first.last", "work", &[]).unwrap();
        let path = std::env::temp_dir().join(format!("sv2-profile-test-{}.sb", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let out = Command::new("/usr/bin/sandbox-exec")
            .args(ios_param(false))
            .arg("-f")
            .arg(&path)
            .arg("/usr/bin/true")
            .output()
            .unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(
            out.status.success(),
            "sandbox-exec rejected the profile: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// The file asks whether the session may use the simulator, and does
    /// not load when it is not told: no session starts by oversight with
    /// the rule on, or with a parameter of another name.
    #[test]
    fn the_profile_does_not_load_without_its_parameter() {
        let text = render("pat", "work", &[]).unwrap();
        assert!(text.contains("(param \"SV2_IOS\")"), "{text}");
        assert!(!text.contains("__IOS_PARAM__"));
        let path =
            std::env::temp_dir().join(format!("sv2-profile-param-{}.sb", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let loads = |args: &[&str]| {
            Command::new("/usr/bin/sandbox-exec")
                .args(args)
                .arg("-f")
                .arg(&path)
                .arg("/usr/bin/true")
                .output()
                .unwrap()
                .status
                .success()
        };
        let (off, on) = (ios_param(false), ios_param(true));
        assert_eq!(off, ["-D", "SV2_IOS=0"]);
        assert_eq!(on, ["-D", "SV2_IOS=1"]);
        assert!(loads(&["-D", "SV2_IOS=0"]) && loads(&["-D", "SV2_IOS=1"]));
        assert!(!loads(&[]));
        assert!(!loads(&["-D", "IOS=1"]));
        std::fs::remove_file(&path).unwrap();
    }

    /// The load test must be able to fail: a broken profile is rejected.
    #[test]
    fn broken_profile_does_not_load() {
        let text = render("pat", "work", &[]).unwrap() + "\n(allow file-read*";
        let path = std::env::temp_dir().join(format!("sv2-profile-bad-{}.sb", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let out = Command::new("/usr/bin/sandbox-exec")
            .args(ios_param(false))
            .arg("-f")
            .arg(&path)
            .arg("/usr/bin/true")
            .output()
            .unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(!out.status.success());
    }
}
