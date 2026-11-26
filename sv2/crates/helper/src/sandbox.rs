//! Renders a profile's `sandbox-exec` file from the embedded base profile.
//!
//! Milestone 1 ports v1's profile unchanged (allow-default with its deny
//! list). Default deny is required before release (docs/SANDVAULT2.md,
//! "Sandbox profile").

use sv2_common::{
    account_name, home_path, valid_host_user, valid_profile, workspace_path, workspace_user_dir,
    WORKSPACE_ROOT,
};

/// Bump on every change to `base.sb` or to the rendering. `install` then
/// re-renders every profile file the registry lists.
pub const PROFILE_VERSION: u32 = 1;

const BASE: &str = include_str!("base.sb");

/// The values go inside SBPL string literals, so they must not contain a
/// quote or a backslash. The name validators already guarantee that; this is
/// the check at the point of use.
fn sbpl_safe(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-' | b'/'))
}

pub fn render(host_user: &str, profile: &str) -> Result<String, String> {
    if !valid_host_user(host_user) || !valid_profile(profile) {
        return Err("cannot render a sandbox profile for an invalid name".to_string());
    }
    let account = account_name(host_user, profile);
    let subs = [
        ("__PROFILE_VERSION__", PROFILE_VERSION.to_string()),
        ("__WORKSPACE_ROOT__", WORKSPACE_ROOT.to_string()),
        ("__WORKSPACE_USER_DIR__", workspace_user_dir(host_user)),
        ("__WORKSPACE__", workspace_path(host_user, profile)),
        ("__HOME__", home_path(&account)),
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
        let text = render("pat", "work").unwrap();
        assert!(!text.contains("__"));
        assert!(text.contains(";; sv2-profile-version: 1\n"));
        assert!(text.contains("(subpath \"/Users/Shared/sv2/pat/work\")"));
        assert!(text.contains("(subpath \"/Users/sv2-pat-work\")"));
        assert!(text.contains("(literal \"/Users/Shared/sv2\")"));
        assert!(text.contains("(literal \"/Users/Shared/sv2/pat\")"));
        // v1's text: nothing of the bash heredoc survives.
        assert!(!text.contains('$'));
        assert!(!text.contains("sandvault-"));
    }

    #[test]
    fn profiles_are_distinct_per_user_and_profile() {
        let a = render("pat", "work").unwrap();
        let b = render("pat-work", "default").unwrap();
        assert!(!a.contains("/Users/Shared/sv2/pat-work/"));
        assert!(!b.contains("\"/Users/Shared/sv2/pat/work\""));
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
            assert!(render(user, profile).is_err(), "{user:?} {profile:?}");
        }
    }

    /// The same check the helper makes before renaming a profile into place:
    /// the rendered text must load. Needs no privileges.
    #[test]
    fn rendered_profile_loads() {
        let text = render("first.last", "work").unwrap();
        let path = std::env::temp_dir().join(format!("sv2-profile-test-{}.sb", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let out = Command::new("/usr/bin/sandbox-exec")
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

    /// The load test must be able to fail: a broken profile is rejected.
    #[test]
    fn broken_profile_does_not_load() {
        let text = render("pat", "work").unwrap() + "\n(allow file-read*";
        let path = std::env::temp_dir().join(format!("sv2-profile-bad-{}.sb", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let out = Command::new("/usr/bin/sandbox-exec")
            .arg("-f")
            .arg(&path)
            .arg("/usr/bin/true")
            .output()
            .unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(!out.status.success());
    }
}
