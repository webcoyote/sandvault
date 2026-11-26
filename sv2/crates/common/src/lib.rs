//! Names, paths, and protocol constants shared by `sv2` and `sv2-helper`.
//!
//! This crate has no dependencies and does no I/O: everything in it also runs
//! as root inside the helper.

/// The application version: what `sv2 --version` prints and a release is
/// named by. It does not decide whether an installed machine is up to date;
/// the artifact versions below do.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// Artifact versions
// ---------------------------------------------------------------------------
//
// `sv2 install` puts a root-owned helper on the machine, and the helper
// writes root-owned files. None of them changes when a new `sv2` is unpacked:
// only `sv2 install` (sudo) replaces them. Each has a number here. Bump the
// number when the thing changes, and every installed machine is told to run
// `sv2 install` again; leave them alone, and a new application version needs
// no reinstall.
//
// `sv2` passes `install_version()` on every call and the helper refuses one
// that differs from its own. Each file also carries its own number in a
// comment line, which the helper reads back before it starts a session, so a
// file left behind by an install that stopped halfway is noticed too.
//
// A unit test beside each file's text pins the text to its number: changing
// one without the other fails the test.

/// The helper itself: bump on any change to its code that an installed
/// machine must get (a fix, a new check, a new verb or argument). Nothing
/// can detect a forgotten bump here.
pub const HELPER_VERSION: u32 = 5;
/// `/etc/sudoers.d/sandvault2`.
pub const SUDOERS_VERSION: u32 = 2;
/// The sshd drop-in that refuses sandbox accounts.
pub const SSHD_DROPIN_VERSION: u32 = 1;
/// The rendered `sandbox-exec` profile: `base.sb` and how it is filled in.
pub const SANDBOX_PROFILE_VERSION: u32 = 1;

/// The key of the comment line each file carries: `<key>: <number>`.
pub const SUDOERS_STAMP: &str = "sv2-sudoers-version";
pub const SSHD_DROPIN_STAMP: &str = "sv2-sshd-version";
pub const SANDBOX_PROFILE_STAMP: &str = "sv2-profile-version";

/// All artifact versions as one word, for the helper's command line and for
/// `sv2 --version`. No spaces, and nothing a shell or sudo treats specially.
pub fn install_version() -> String {
    format!(
        "helper={HELPER_VERSION},sudoers={SUDOERS_VERSION},sshd={SSHD_DROPIN_VERSION},profile={SANDBOX_PROFILE_VERSION}"
    )
}

/// The number a file carries for `key`: the first line that, after its
/// comment characters (`#` or `;`), reads exactly `<key>: <digits>`. None
/// when there is no such line or the number does not parse.
pub fn stamped_version(text: &str, key: &str) -> Option<u32> {
    text.lines().find_map(|line| {
        let digits = line
            .trim_start_matches(['#', ';'])
            .strip_prefix(' ')?
            .strip_prefix(key)?
            .strip_prefix(": ")?;
        if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok()
    })
}

/// The marker at the end of an error message a user sees: two words that
/// occur at one place in the code, so that a screenshot or a bug report
/// leads straight to the line that raised the error. See `docs/OBELUS.md`.
///
/// Written as a literal at the place the error is raised, and shown as
/// `(two words)`. A test in this crate reads every source file and fails on
/// an obelus that is used twice, is not two lowercase words, or is not a
/// literal.
pub struct Obelus(pub &'static str);

impl std::fmt::Display for Obelus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({})", self.0)
    }
}

/// Where `sv2 install` puts the root-owned helper. Outside the Homebrew
/// prefix, which an admin user can write.
pub const HELPER_PATH: &str = "/Library/PrivilegedHelperTools/sv2-helper";

/// Members of this group may run the helper's `call` verbs without a password.
/// The `sv2-` prefix keeps it clear of v1, whose sandbox group for a host
/// user is `sandvault-<user>`.
pub const ALLOWED_GROUP: &str = "sv2-users";

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

/// The helper's `uninstall` takes this word to mean "and remove every
/// profile there is"; `sv2 uninstall --all-profiles` passes it.
pub const UNINSTALL_ALL_PROFILES: &str = "all-profiles";

/// Helper exit status for "sv2 and the installed helper differ in an artifact
/// version" (`install_version`). Only
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

    fn rust_sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    fn two_lowercase_words(s: &str) -> bool {
        let words: Vec<&str> = s.split(' ').collect();
        words.len() == 2
            && words
                .iter()
                .all(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_lowercase()))
    }

    /// Every obelus in the sources of all crates: a literal, two lowercase
    /// words, and used once. This reads the source text, so it sees an
    /// obelus on a path no test reaches.
    #[test]
    fn every_obelus_is_unique_and_two_lowercase_words() {
        assert!(two_lowercase_words("dapper mackerel"));
        for bad in [
            "dapper",
            "dapper  mackerel",
            "Dapper mackerel",
            "a b c",
            "a b2",
            " a b",
            "",
        ] {
            assert!(!two_lowercase_words(bad), "{bad:?}");
        }

        // Spelled in two parts so that this line is not itself a use.
        let open = concat!("Obel", "us(");
        let definition = concat!("Obel", "us(pub &'static str)");
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let mut files = Vec::new();
        rust_sources(&crates, &mut files);
        assert!(files.len() >= 10, "found only {} source files", files.len());

        let mut seen: Vec<(String, String)> = Vec::new();
        let mut problems = Vec::new();
        for file in &files {
            let name = file.strip_prefix(&crates).unwrap().display().to_string();
            let text = std::fs::read_to_string(file).unwrap();
            for (at, _) in text.match_indices(open) {
                if text[at..].starts_with(definition) {
                    continue;
                }
                let rest = &text[at + open.len()..];
                let literal = rest
                    .strip_prefix('"')
                    .and_then(|r| r.split_once('"'))
                    .map(|(words, _)| words);
                let Some(words) = literal else {
                    problems.push(format!("{name}: an obelus that is not a literal"));
                    continue;
                };
                if !two_lowercase_words(words) {
                    problems.push(format!("{name}: \"{words}\" is not two lowercase words"));
                }
                if let Some((_, first)) = seen.iter().find(|(w, _)| w == words) {
                    problems.push(format!("\"{words}\" is in {first} and in {name}"));
                }
                seen.push((words.to_string(), name.clone()));
            }
        }
        assert_eq!(problems, Vec::<String>::new());
        // A scan that finds nothing has stopped looking in the right place.
        assert!(!seen.is_empty(), "found no obelus at all");
    }

    #[test]
    fn install_version_is_one_plain_word() {
        let v = install_version();
        assert_eq!(v, "helper=5,sudoers=2,sshd=1,profile=1");
        assert!(v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'=' || b == b','));
    }

    #[test]
    fn reads_the_version_a_file_carries() {
        let sudoers = "# sv2-sudoers-version: 3\n# words\n%g ALL=(root) NOPASSWD: /x call *\n";
        assert_eq!(stamped_version(sudoers, SUDOERS_STAMP), Some(3));
        let sb = "(version 1)\n;; sv2-profile-version: 12\n(allow default)\n";
        assert_eq!(stamped_version(sb, SANDBOX_PROFILE_STAMP), Some(12));
        // Another file's key, no line at all, an empty file.
        assert_eq!(stamped_version(sudoers, SSHD_DROPIN_STAMP), None);
        assert_eq!(
            stamped_version("(allow default)\n", SANDBOX_PROFILE_STAMP),
            None
        );
        assert_eq!(stamped_version("", SUDOERS_STAMP), None);
        // Only a whole comment line counts, and only plain digits.
        for bad in [
            "sv2-sudoers-version: 3",
            "#sv2-sudoers-version: 3",
            "x # sv2-sudoers-version: 3",
            "# sv2-sudoers-version: 3 ",
            "# sv2-sudoers-version: +3",
            "# sv2-sudoers-version: 3x",
            "# sv2-sudoers-version:3",
            "# sv2-sudoers-version: ",
            "# sv2-sudoers-version: 99999999999",
            "# sv2-sudoers-version-old: 3",
        ] {
            assert_eq!(stamped_version(bad, SUDOERS_STAMP), None, "{bad:?}");
        }
        // The first matching line decides.
        assert_eq!(
            stamped_version(
                "# sv2-sudoers-version: 1\n# sv2-sudoers-version: 2\n",
                SUDOERS_STAMP
            ),
            Some(1)
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
