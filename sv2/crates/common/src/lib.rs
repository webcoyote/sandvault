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
pub const HELPER_VERSION: u32 = 10;
/// `/etc/sudoers.d/sandvault2`.
pub const SUDOERS_VERSION: u32 = 2;
/// The sshd drop-in that refuses sandbox accounts.
pub const SSHD_DROPIN_VERSION: u32 = 1;
/// The rendered `sandbox-exec` profile: `base.sb` and how it is filled in.
pub const SANDBOX_PROFILE_VERSION: u32 = 11;

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

/// Root-owned parent of every workspace: `<root>/<user>/<profile>`. In
/// `/Users`, where only root can create a name; `/Users/Shared` would let any
/// user take it first.
pub const WORKSPACE_ROOT: &str = "/Users/sv2";

/// The hidden account that `install` creates so that nobody else can be
/// given this name: a new account named `sv2` would get `WORKSPACE_ROOT`
/// as its home folder, and macOS may hand an existing folder to it.
pub const RESERVED_ACCOUNT: &str = "sv2";

pub const DEFAULT_PROFILE: &str = "default";

/// First argument to the helper. The sudoers line allows `call` without a
/// password; `admin` matches no sandvault rule, so sudo applies its ordinary
/// policy (a password, from a user with general sudo rights).
pub const MODE_CALL: &str = "call";
pub const MODE_ADMIN: &str = "admin";

/// The helper's `uninstall` takes this word to mean "and remove every
/// profile there is"; `sv2 uninstall --all-profiles` passes it.
pub const UNINSTALL_ALL_PROFILES: &str = "all-profiles";

/// The word after the profile's name in `launch NAME ios -- COMMAND`: the
/// session may use the iOS simulator (`sv2 run --ios`).
pub const LAUNCH_IOS: &str = "ios";

/// The parameter of the sandbox profile that the word above sets to 1. The
/// helper always passes it, as 0 or 1: the profile does not load without.
pub const PROFILE_PARAM_IOS: &str = "SV2_IOS";

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

/// The command a sandbox account runs so that the host user can take over
/// what the account owns in its workspace: `/usr/bin/find` and its
/// arguments.
///
/// An entry the account owns that has no ACL (it was moved in from the
/// sandbox home, or the ACL was removed) is closed to the host user, who can
/// neither take it over nor repair it. Only its owner can change that, so
/// the owner does: on each file and directory it owns it adds an entry that
/// lets the host user open it, take ownership and write its ACL. `sv2 repair`
/// does the rest as the host user. Links are not followed and not changed.
///
/// A directory the account has closed to itself is opened (`u+rx`) before
/// `find` goes into it; `find` evaluates a directory before it reads it.
/// Every other entry is passed to `chmod` in batches.
pub fn hand_back_argv(host_user: &str, profile: &str, sandbox_uid: u32) -> Vec<String> {
    hand_back_argv_in(&workspace_path(host_user, profile), host_user, sandbox_uid)
}

fn hand_back_argv_in(dir: &str, host_user: &str, owner_uid: u32) -> Vec<String> {
    const COMMON: &str = "readattr,readextattr,readsecurity,writesecurity,chown";
    const CHMOD: &str = "/bin/chmod";
    let dir_entry = format!("user:{host_user} allow list,search,{COMMON}");
    let file_entry = format!("user:{host_user} allow read,{COMMON}");
    let uid = owner_uid.to_string();
    #[rustfmt::skip]
    let words = [
        "/usr/bin/find", dir, "-user", &uid,
        "(",
            "(", "-type", "d",
                "(", "-perm", "-500", "-o", "-exec", CHMOD, "u+rx", "{}", ";", ")",
                "-exec", CHMOD, "+a", &dir_entry, "{}", "+",
            ")",
            "-o",
            "(", "-type", "f", "-exec", CHMOD, "+a", &file_entry, "{}", "+", ")",
        ")",
    ];
    words.iter().map(|w| (*w).to_string()).collect()
}

pub fn home_path(account: &str) -> String {
    format!("/Users/{account}")
}

// ---------------------------------------------------------------------------
// This Mac's SSH ports
//
// A sandbox profile closes this Mac's SSH port to a session (see base.sb).
// Port 22 always; and whatever else this Mac says its sshd listens on. Two
// places say: /etc/services, where launchd looks up the `ssh` service whose
// socket it hands to sshd, and sshd's own configuration, which counts for an
// sshd that is started by hand. Neither shows an sshd started with `-p`, or
// a launchd job of the user's own with a port number in it.
//
// The helper asks when it writes a profile and again before every session;
// `sv2 doctor` asks too. The parsing is here so that both read alike.
// ---------------------------------------------------------------------------

/// Where the helper keeps each profile's sandbox file. Root's, and readable
/// by everyone: the sandbox user's `sandbox-exec` reads it.
pub const SANDBOX_PROFILES_DIR: &str = "/Library/Application Support/sandvault2/profiles";

pub fn sandbox_profile_file(host_user: &str, profile: &str) -> String {
    format!("{SANDBOX_PROFILES_DIR}/{host_user}/{profile}.sb")
}

pub const DEFAULT_SSH_PORT: u16 = 22;
pub const SERVICES_FILE: &str = "/etc/services";

fn port_number(text: &str) -> Option<u16> {
    if text.is_empty() || text.len() > 5 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|p| *p != 0)
}

/// The TCP ports /etc/services gives the name `ssh`, as a service's name or
/// as one of its aliases: `ssh 22/tcp`, `other 2222/tcp ssh`.
pub fn ssh_ports_in_services(text: &str) -> Vec<u16> {
    let mut found = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut words = line.split_whitespace();
        let (Some(name), Some(port)) = (words.next(), words.next()) else {
            continue;
        };
        let Some(port) = port.strip_suffix("/tcp").and_then(port_number) else {
            continue;
        };
        if name == "ssh" || words.any(|alias| alias == "ssh") {
            found.push(port);
        }
    }
    found
}

/// The ports in a configuration as `sshd -G` prints it: `Port N`, and
/// `ListenAddress` with a port (`host:N`, `[address]:N`). The keywords'
/// case has changed between versions.
pub fn ssh_ports_in_sshd_config(printed: &str) -> Vec<u16> {
    let mut found = Vec::new();
    for line in printed.lines() {
        let mut words = line.split_whitespace();
        let (Some(key), Some(value)) = (words.next(), words.next()) else {
            continue;
        };
        let port = if key.eq_ignore_ascii_case("port") {
            port_number(value)
        } else if key.eq_ignore_ascii_case("listenaddress") {
            if value.starts_with('[') {
                value.rsplit_once("]:").and_then(|(_, p)| port_number(p))
            } else if value.matches(':').count() == 1 {
                // More colons and no brackets is an IPv6 address alone.
                value.rsplit_once(':').and_then(|(_, p)| port_number(p))
            } else {
                None
            }
        } else {
            None
        };
        found.extend(port);
    }
    found
}

/// Port 22 and the ports found, each once, in order.
pub fn ssh_ports(found: impl IntoIterator<Item = u16>) -> Vec<u16> {
    let mut ports: Vec<u16> = found.into_iter().filter(|p| *p != 0).collect();
    ports.push(DEFAULT_SSH_PORT);
    ports.sort_unstable();
    ports.dedup();
    ports
}

const LOCAL_PORT_RULE: (&str, &str) = ("(deny network-outbound (remote tcp \"localhost:", "\"))");

/// The profile rule that closes one port of this Mac to a session.
pub fn local_port_rule(port: u16) -> String {
    format!("{}{port}{}", LOCAL_PORT_RULE.0, LOCAL_PORT_RULE.1)
}

/// The ports of this Mac a rendered profile closes: its rules, read back.
/// Only a rule that is a whole line counts, so not one inside a comment.
pub fn local_ports_closed(profile_text: &str) -> Vec<u16> {
    profile_text
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix(LOCAL_PORT_RULE.0)?
                .strip_suffix(LOCAL_PORT_RULE.1)
                .and_then(port_number)
        })
        .collect()
}

/// The ports among `wanted` that a rendered profile does not close.
pub fn local_ports_left_open(profile_text: &str, wanted: &[u16]) -> Vec<u16> {
    let closed = local_ports_closed(profile_text);
    wanted
        .iter()
        .copied()
        .filter(|p| !closed.contains(p))
        .collect()
}

#[cfg(test)]
mod ssh_port_tests {
    use super::*;

    #[test]
    fn reads_the_ssh_port_from_services() {
        let macos = "#\n# Network services, Internet style\n#\n\
            ftp              21/tcp     # File Transfer [Control]\n\
            ssh              22/udp     # SSH Remote Login Protocol\n\
            ssh              22/tcp     # SSH Remote Login Protocol\n\
            telnet           23/tcp     # Telnet\n\
            sftp-ssh        115/tcp     # not the name ssh\n";
        assert_eq!(ssh_ports_in_services(macos), [22]);
        // Moved, as an alias of another name, and tabs.
        assert_eq!(ssh_ports_in_services("ssh\t2222/tcp\n"), [2222]);
        assert_eq!(ssh_ports_in_services("mine 2200/tcp ssh x\n"), [2200]);
        assert_eq!(
            ssh_ports_in_services("ssh 22/tcp\nssh 2222/tcp\n"),
            [22, 2222]
        );
        // Not ssh over TCP, commented out, or no port at all.
        for none in [
            "",
            "ssh 22/udp\n",
            "# ssh 2222/tcp\n",
            "sshd 2222/tcp\n",
            "ssh\n",
            "ssh tcp\n",
            "ssh 0/tcp\n",
            "ssh 70000/tcp\n",
            "ssh -22/tcp\n",
            "x 22/tcp # ssh\n",
        ] {
            assert_eq!(ssh_ports_in_services(none), Vec::<u16>::new(), "{none:?}");
        }
    }

    #[test]
    fn reads_the_ports_sshd_prints() {
        // As `sshd -G` prints a default macOS configuration.
        let printed = "Port 22\nAddressFamily any\nListenAddress 0.0.0.0:22\nListenAddress [::]:22\nUsePAM yes\n";
        assert_eq!(ssh_ports_in_sshd_config(printed), [22, 22, 22]);
        // Older versions print the keywords in lower case.
        assert_eq!(
            ssh_ports_in_sshd_config(
                "port 2222\nlistenaddress [::1]:2022\nlistenaddress 10.0.0.5:2023\n"
            ),
            [2222, 2022, 2023]
        );
        // An address without a port, and lines that only look alike.
        for none in [
            "ListenAddress ::1\n",
            "ListenAddress 10.0.0.5\n",
            "ListenAddress [::1]\n",
            "Port\n",
            "Port ssh\n",
            "Port 0\n",
            "GatewayPorts 2222\n",
            "PermitOpen any:2222\n",
        ] {
            assert_eq!(
                ssh_ports_in_sshd_config(none),
                Vec::<u16>::new(),
                "{none:?}"
            );
        }
    }

    #[test]
    fn port_22_is_always_among_them() {
        assert_eq!(ssh_ports([]), [22]);
        assert_eq!(ssh_ports([22, 22]), [22]);
        assert_eq!(ssh_ports([2222, 0, 21, 2222]), [21, 22, 2222]);
    }

    #[test]
    fn reads_back_the_ports_a_profile_closes() {
        let text = format!(
            "(version 1)\n(allow network*)\n;; {}\n{}\n  {}\n(deny network-outbound (remote tcp \"*:25\"))\n",
            local_port_rule(9),
            local_port_rule(22),
            local_port_rule(2222)
        );
        assert_eq!(local_ports_closed(&text), [22, 2222]);
        assert_eq!(local_ports_left_open(&text, &[22]), Vec::<u16>::new());
        assert_eq!(local_ports_left_open(&text, &[22, 2200, 9]), [2200, 9]);
        // A profile from before the rule closes nothing.
        assert_eq!(local_ports_left_open("(allow network*)\n", &[22]), [22]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real `find` and `chmod`, as the current user on a scratch tree:
    /// a private file, a directory closed even to its owner, and a link.
    #[test]
    fn hand_back_opens_what_the_owner_closed() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;
        let text = |cmd: &mut Command| {
            String::from_utf8(cmd.output().expect("run").stdout).expect("utf-8")
        };
        let user = text(Command::new("/usr/bin/id").arg("-un"))
            .trim()
            .to_string();
        let uid: u32 = text(Command::new("/usr/bin/id").arg("-u"))
            .trim()
            .parse()
            .unwrap();
        let top = std::env::temp_dir().join(format!("sv2-hand-back-{}", std::process::id()));
        let closed = top.join("ws/closed");
        let inner = closed.join("inner");
        let outside = top.join("outside");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(inner.join("file"), "x").unwrap();
        std::os::unix::fs::symlink(&outside, closed.join("link")).unwrap();
        let mode = |p: &std::path::Path, m| {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap()
        };
        mode(&inner.join("file"), 0o600);
        mode(&inner, 0o000);
        mode(&closed, 0o000);

        let ws = top.join("ws");
        let argv = hand_back_argv_in(ws.to_str().unwrap(), &user, uid);
        let status = Command::new(&argv[0]).args(&argv[1..]).status().unwrap();
        assert!(status.success());
        // Twice: an entry that is there already is not added again.
        assert!(Command::new(&argv[0])
            .args(&argv[1..])
            .status()
            .unwrap()
            .success());

        let acl = |p: &std::path::Path| text(Command::new("/bin/ls").arg("-lde").arg(p));
        let common = "readattr,readextattr,readsecurity,writesecurity,chown";
        for dir in [&closed, &inner] {
            let listed = acl(dir);
            assert!(listed.starts_with("dr-x------"), "{listed}");
            assert_eq!(listed.matches(" allow ").count(), 1, "{listed}");
            assert!(listed.contains(&format!(" 0: user:{user} allow list,search,{common}")));
        }
        let listed = acl(&inner.join("file"));
        assert_eq!(listed.matches(" allow ").count(), 1, "{listed}");
        assert!(listed.contains(&format!(" 0: user:{user} allow read,{common}")));
        // Neither the link nor what it points to.
        assert!(!acl(&closed.join("link")).contains(" allow "));
        assert!(!acl(&outside).contains(" allow "));

        mode(&closed, 0o700);
        mode(&inner, 0o700);
        std::fs::remove_dir_all(&top).unwrap();
    }

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
        assert_eq!(v, "helper=10,sudoers=2,sshd=1,profile=11");
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
        assert_eq!(workspace_path("pat", "default"), "/Users/sv2/pat/default");
        assert_eq!(home_path("sv2-pat-default"), "/Users/sv2-pat-default");
        // The account that keeps the root's name taken is named for it.
        assert_eq!(WORKSPACE_ROOT, format!("/Users/{RESERVED_ACCOUNT}"));
    }
}
