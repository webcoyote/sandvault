//! Names, paths, and protocol constants shared by `sv2` and `sv2-helper`.
//!
//! This crate has no dependencies and does no I/O: everything in it also runs
//! as root inside the helper.

/// `sv2` and the helper must be the same version. `sv2` passes this string on
/// every call and the helper refuses a different one.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where `sv2 install` puts the root-owned helper. Outside the Homebrew
/// prefix, which an admin user can write.
pub const HELPER_PATH: &str = "/Library/PrivilegedHelperTools/sv2-helper";

/// Members of this group may run the helper's `call` verbs without a password.
pub const ALLOWED_GROUP: &str = "sandvault-users";

/// Every sandbox account is a member of this group, and of nothing else that
/// sandvault creates besides its own primary group. The sshd drop-in refuses
/// the group, so a sandbox account cannot log in to this Mac over SSH and get
/// a shell outside its sandbox profile. No account is ever named this:
/// `account_name` always produces a second dash.
pub const SANDBOX_GROUP: &str = "sv2-sandboxes";

/// Root-owned parent of every workspace: `<root>/<user>/<profile>`.
pub const WORKSPACE_ROOT: &str = "/Users/Shared/sv2";

pub const DEFAULT_PROFILE: &str = "default";

/// First argument to the helper. The sudoers line allows `call` without a
/// password; `admin` matches no sandvault rule, so sudo applies its ordinary
/// policy (a password, from a user with general sudo rights).
pub const MODE_CALL: &str = "call";
pub const MODE_ADMIN: &str = "admin";

/// Helper exit status for "sv2 and the helper are different versions". Only
/// meaningful for verbs that do not end in `exec` (everything but `launch`).
pub const EXIT_VERSION_MISMATCH: u8 = 3;

/// Profiles match `^[a-z0-9]{1,16}$`. No dash, so in `sv2-<user>-<profile>`
/// the last dash always separates user from profile.
pub fn valid_profile(s: &str) -> bool {
    (1..=16).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// Host usernames: an explicit character set and a length limit. The first
/// character may not be `.` or `-`, so a name is never `.`, `..`, or read as
/// an option, and it contains no `/`, quote, or whitespace.
pub fn valid_host_user(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=32).contains(&b.len())
        && (b[0].is_ascii_alphanumeric() || b[0] == b'_')
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-'))
}

/// The sandbox account (and its like-named primary group).
pub fn account_name(user: &str, profile: &str) -> String {
    format!("sv2-{user}-{profile}")
}

pub fn workspace_user_dir(user: &str) -> String {
    format!("{WORKSPACE_ROOT}/{user}")
}

pub fn workspace_path(user: &str, profile: &str) -> String {
    format!("{WORKSPACE_ROOT}/{user}/{profile}")
}

pub fn home_path(account: &str) -> String {
    format!("/Users/{account}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names() {
        for ok in ["default", "work", "a", "0", "abcdefghijklmnop"] {
            assert!(valid_profile(ok), "{ok}");
        }
        for bad in [
            "",
            "abcdefghijklmnopq",
            "Work",
            "a-b",
            "a_b",
            "a.b",
            "a b",
            "a/b",
            "..",
            "é",
            "a\n",
        ] {
            assert!(!valid_profile(bad), "{bad:?}");
        }
    }

    #[test]
    fn host_user_names() {
        for ok in ["pat", "pat-work", "first.last", "_svc", "Pat", "u1"] {
            assert!(valid_host_user(ok), "{ok}");
        }
        let long = "a".repeat(33);
        for bad in [
            "",
            ".",
            "..",
            ".hidden",
            "-rf",
            "a/b",
            "a b",
            "a\"b",
            "a*",
            "a$b",
            "a\n",
            "é",
            long.as_str(),
        ] {
            assert!(!valid_host_user(bad), "{bad:?}");
        }
    }

    /// The v1 collision: `pat` + `work` and `pat-work` + nothing produced the
    /// same account. With a mandatory dash-free profile they cannot.
    #[test]
    fn accounts_do_not_collide() {
        assert_eq!(account_name("pat", "work"), "sv2-pat-work");
        assert_eq!(account_name("pat-work", "default"), "sv2-pat-work-default");
        assert_ne!(
            account_name("pat", "work"),
            account_name("pat-work", "default")
        );
        // The shared group of all sandbox accounts is not an account name.
        assert!(SANDBOX_GROUP.matches('-').count() < 2);
        assert!(account_name("a", "b").matches('-').count() >= 2);
        assert_ne!(
            workspace_path("pat", "work"),
            workspace_path("pat-work", "default")
        );
    }

    #[test]
    fn paths() {
        assert_eq!(
            workspace_path("pat", "default"),
            "/Users/Shared/sv2/pat/default"
        );
        assert_eq!(home_path("sv2-pat-default"), "/Users/sv2-pat-default");
    }
}
