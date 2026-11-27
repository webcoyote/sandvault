//! `sv2 doctor`: looks at what is installed and says what is wrong with it,
//! and what to run to put it right. It changes nothing, asks for no
//! password, and runs as the host user.
//!
//! What it looks at:
//!
//! - the installed helper, and whether it matches this `sv2`;
//! - the caller's profiles: one left half added or half removed (the helper
//!   was killed, or macOS refused a step) is finished by running the same
//!   command again;
//! - each profile's account, and what sshd would do with a login as it;
//! - the ports this Mac's sshd listens on, and whether each profile's
//!   sandbox file closes them;
//! - the user's own configuration (`home.rs`): whether there is any, and
//!   which sandbox homes have it;
//! - every agent definition: whether it can be read, whether its name is
//!   also a command, and whether macOS will let its program start.
//!
//! The profiles of other users are theirs to look at: the helper lists only
//! the caller's.

use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use sv2_common::{
    local_ports_left_open, sandbox_profile_file, ssh_ports, ssh_ports_in_services,
    ssh_ports_in_sshd_config, ALLOWED_GROUP, EXIT_VERSION_MISMATCH, HELPER_PATH, SANDBOX_GROUP,
    SERVICES_FILE,
};

use crate::agent::{self, Sources};
use crate::{args, call_command, home, in_allowed_group, parse_profile_list, ProfileLine};

const SSHD: &str = "/usr/sbin/sshd";
const SSHD_DROPIN: &str = "/etc/ssh/sshd_config.d/000-sandvault2.conf";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Ok,
    /// Worth knowing; nothing is broken.
    Note,
    /// Something to put right; the line says how.
    Problem,
}

#[derive(Debug, Default)]
pub struct Report(pub Vec<(Level, String)>);

impl Report {
    fn ok(&mut self, line: impl Into<String>) {
        self.0.push((Level::Ok, line.into()));
    }
    fn note(&mut self, line: impl Into<String>) {
        self.0.push((Level::Note, line.into()));
    }
    fn problem(&mut self, line: impl Into<String>) {
        self.0.push((Level::Problem, line.into()));
    }

    pub fn problems(&self) -> usize {
        self.0.iter().filter(|(l, _)| *l == Level::Problem).count()
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for (level, line) in &self.0 {
            let tag = match level {
                Level::Ok => "ok      ",
                Level::Note => "note    ",
                Level::Problem => "PROBLEM ",
            };
            out.push_str(&format!("{tag}{line}\n"));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

/// A profile's state, as the helper lists it.
pub fn check_profile_states(profiles: &[ProfileLine], report: &mut Report) {
    if profiles.is_empty() {
        report.note("you have no profiles; 'sv2 run shell' creates the default one");
    }
    for p in profiles {
        match p.state.as_str() {
            "active" => report.ok(format!("profile '{}' ({})", p.name, p.account)),
            "adding" => report.problem(format!(
                "profile '{0}' was left half added; finish with: sv2 profile add {0}",
                p.name
            )),
            "removing" => report.problem(format!(
                "profile '{0}' was left half removed; finish with: sv2 profile remove {0}",
                p.name
            )),
            other => report.problem(format!(
                "profile '{}' is in a state this sv2 does not know: {other}",
                p.name
            )),
        }
    }
}

fn account_exists(account: &str) -> bool {
    let Ok(c) = std::ffi::CString::new(account) else {
        return false;
    };
    // SAFETY: `c` is NUL-terminated; only the null-ness of the result is read.
    unsafe { !libc::getpwnam(c.as_ptr()).is_null() }
}

// ---------------------------------------------------------------------------
// sshd
// ---------------------------------------------------------------------------

/// What sshd must report for a sandbox account, in the lower case of
/// `sshd -T`: each alone refuses the login (see the drop-in's text in the
/// helper).
const SSHD_REFUSALS: [&str; 3] = [
    "pubkeyauthentication no",
    "authorizedkeysfile none",
    "forcecommand /usr/bin/false",
];

/// The refusals missing from `sshd -T` output for an account. Empty means
/// sshd refuses the account every way the drop-in says.
pub fn sshd_missing_refusals(output: &str) -> Vec<String> {
    let lines: Vec<String> = output.lines().map(|l| l.trim().to_lowercase()).collect();
    let mut missing: Vec<String> = SSHD_REFUSALS
        .iter()
        .filter(|want| !lines.iter().any(|l| l == *want))
        .map(|s| (*s).to_string())
        .collect();
    // DenyGroups is one line listing every denied group.
    let group = SANDBOX_GROUP.to_lowercase();
    let denied = lines.iter().any(|l| {
        l.strip_prefix("denygroups ")
            .is_some_and(|rest| rest.split([' ', ',']).any(|g| g == group))
    });
    if !denied {
        missing.push(format!("denygroups {group}"));
    }
    missing
}

/// Ask sshd what it would apply to a login as each account. `sshd -T` wants
/// a host key it can read; a throwaway one in a private directory will do.
fn check_sshd(accounts: &[&str], report: &mut Report) {
    if !Path::new(SSHD_DROPIN).is_file() {
        report.problem(format!(
            "{SSHD_DROPIN} is missing, so sshd does not refuse sandbox accounts; run: sv2 install"
        ));
        return;
    }
    if accounts.is_empty() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("sv2-doctor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let key = dir.join("hostkey");
    let made = {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(&dir).is_ok()
            && Command::new("/usr/bin/ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&key)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
    };
    if !made {
        report
            .note("could not make a throwaway host key, so sshd was not asked about the accounts");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    for account in accounts {
        let out = Command::new(SSHD)
            .arg("-T")
            .arg("-h")
            .arg(&key)
            .arg("-C")
            .arg(format!("user={account}"))
            .stdin(Stdio::null())
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let missing = sshd_missing_refusals(&String::from_utf8_lossy(&o.stdout));
                if missing.is_empty() {
                    report.ok(format!("sshd refuses a login as {account}"));
                } else {
                    report.problem(format!(
                        "sshd would not fully refuse a login as {account} (not in effect: {}). Something in sshd's configuration overrides {SSHD_DROPIN}; run 'sv2 install', and look for a Match block of your own that sets these",
                        missing.join("; ")
                    ));
                }
            }
            Ok(o) => report.problem(format!(
                "sshd cannot read its configuration: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            )),
            Err(e) => report.note(format!("cannot run {SSHD}: {e}")),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// This Mac's SSH ports
// ---------------------------------------------------------------------------

fn port_list(ports: &[u16]) -> String {
    let words: Vec<String> = ports.iter().map(u16::to_string).collect();
    words.join(", ")
}

/// Whether each profile's sandbox file closes the SSH ports found. The
/// helper makes the same comparison before every session and refuses one
/// that would fail it; here it is said without starting anything.
/// `sandbox_file` gives a profile's sandbox file as text.
fn check_ssh_ports(
    profiles: &[&ProfileLine],
    ports: &[u16],
    sshd_error: Option<&str>,
    sandbox_file: &dyn Fn(&ProfileLine) -> Result<String, String>,
    report: &mut Report,
) {
    if let Some(e) = sshd_error {
        report.problem(format!(
            "sshd cannot read its configuration, so the ports it names are not known: only {} can be checked. Repair it (sudo sshd -G shows where). sshd said: {e}",
            port_list(ports)
        ));
    }
    let extra = if ports.len() > 1 {
        format!(" (this Mac's sshd: {})", port_list(ports))
    } else {
        String::new()
    };
    for p in profiles {
        match sandbox_file(p) {
            Err(e) => report.problem(format!(
                "the sandbox file of profile '{0}' cannot be read ({e}); run: sv2 profile add {0}",
                p.name
            )),
            Ok(text) => {
                let open = local_ports_left_open(&text, ports);
                if open.is_empty() {
                    report.ok(format!(
                        "profile '{}' cannot connect to this Mac's SSH port{extra}",
                        p.name
                    ));
                } else {
                    report.problem(format!(
                        "this Mac's sshd listens on port {1}, which profile '{0}' does not close, so no session starts in it; run: sv2 profile add {0}",
                        p.name,
                        port_list(&open)
                    ));
                }
            }
        }
    }
}

/// The SSH ports of this Mac as the helper finds them (`find_ssh_ports`
/// there), read here as the user: both sources are readable by everyone.
fn find_ssh_ports() -> (Vec<u16>, Option<String>) {
    let mut found = std::fs::read_to_string(SERVICES_FILE)
        .map(|text| ssh_ports_in_services(&text))
        .unwrap_or_default();
    let out = Command::new(SSHD).arg("-G").stdin(Stdio::null()).output();
    let error = match out {
        Ok(o) if o.status.success() => {
            found.extend(ssh_ports_in_sshd_config(&String::from_utf8_lossy(
                &o.stdout,
            )));
            None
        }
        Ok(o) => Some(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Some(format!("cannot run {SSHD}: {e}")),
    };
    (ssh_ports(found), error)
}

/// The host user in an account's name: `sv2-<user>-<profile>`.
fn host_user_of(p: &ProfileLine) -> Option<&str> {
    p.account
        .strip_prefix("sv2-")?
        .strip_suffix(p.name.as_str())?
        .strip_suffix('-')
}

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

fn is_program(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub fn check_agents(sources: &Sources, brew: Option<&str>, report: &mut Report) {
    let names = sources.names();
    if sources.builtin.is_none() {
        report.note("no built-in agent definitions were found beside this sv2");
    }
    for name in &names {
        if name == "shell" {
            report
                .note("a definition named 'shell' is never used: shell is part of sv2".to_string());
            continue;
        }
        let def = match sources.load(name) {
            Ok(def) => def,
            Err(e) => {
                report.problem(format!("agent '{name}' cannot be used: {e}"));
                continue;
            }
        };
        if args::SUBCOMMANDS.contains(&name.as_str()) {
            report.note(format!(
                "agent '{name}' has the name of a command; run it with: sv2 run {name}"
            ));
        }
        let visible = def.host_paths(brew);
        match visible.iter().find(|p| is_program(p)) {
            Some(program) => match agent::gatekeeper_would_stop(Path::new(program)) {
                Some(file) => report.problem(format!(
                    "macOS will stop agent '{name}' as it starts: {} was downloaded and Apple has not notarized it. If you trust it: xattr -d com.apple.quarantine '{}'",
                    file.display(),
                    file.display()
                )),
                None => report.ok(format!("agent '{name}' ({program})")),
            },
            None if def.host_binary.is_empty() => report.note(format!(
                "agent '{name}' is installed inside the sandbox on first use"
            )),
            None => report.note(format!(
                "agent '{name}' is not installed on this Mac; sv2 installs it on first use"
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// The whole examination
// ---------------------------------------------------------------------------

pub fn run(sources: &Sources) -> ExitCode {
    let mut report = Report::default();
    examine(sources, &mut report);
    print!("{}", report.text());
    match report.problems() {
        0 => {
            println!("nothing is wrong");
            ExitCode::SUCCESS
        }
        n => {
            println!("{n} to put right");
            ExitCode::FAILURE
        }
    }
}

fn examine(sources: &Sources, report: &mut Report) {
    let brew = agent::brew_prefix();
    if brew.is_none() {
        report.note("Homebrew was not found; agents can still be installed with --native");
    }

    if !Path::new(HELPER_PATH).is_file() {
        report.problem("sandvault2 is not installed; an administrator must run: sv2 install");
        check_agents(sources, brew, report);
        return;
    }
    let listed = call_command("profile-list")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let profiles = match listed {
        Err(e) => {
            report.problem(format!("cannot run sudo: {e}"));
            None
        }
        Ok(out) if out.status.success() => {
            match parse_profile_list(&String::from_utf8_lossy(&out.stdout)) {
                Ok(profiles) => {
                    report.ok("installed, and the helper matches this sv2");
                    Some(profiles)
                }
                Err(e) => {
                    report.problem(e);
                    None
                }
            }
        }
        Ok(out) if out.status.code() == Some(i32::from(EXIT_VERSION_MISMATCH)) => {
            report.problem(
                "the installed helper or its files are older or newer than this sv2; an administrator must run: sv2 install",
            );
            None
        }
        Ok(_) if !in_allowed_group() => {
            report.problem(format!(
                "you are not in the {ALLOWED_GROUP} group; an administrator must run: sv2 user allow USER"
            ));
            None
        }
        Ok(out) => {
            report.problem(format!(
                "the helper could not list your profiles ({})",
                out.status
            ));
            None
        }
    };

    if let Some(profiles) = profiles {
        check_profile_states(&profiles, report);
        let mut accounts = Vec::new();
        for p in profiles.iter().filter(|p| p.state == "active") {
            if account_exists(&p.account) {
                accounts.push(p.account.as_str());
            } else {
                report.problem(format!(
                    "profile '{0}' has no account ({1}); make it again with: sv2 profile add {0}",
                    p.name, p.account
                ));
            }
        }
        check_sshd(&accounts, report);
        let with_accounts: Vec<&ProfileLine> = profiles
            .iter()
            .filter(|p| accounts.contains(&p.account.as_str()))
            .collect();
        let (ports, sshd_error) = find_ssh_ports();
        check_ssh_ports(
            &with_accounts,
            &ports,
            sshd_error.as_deref(),
            &|p| {
                let user = host_user_of(p).ok_or("its account has an unexpected name")?;
                let path = sandbox_profile_file(user, &p.name);
                std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))
            },
            report,
        );
        if home::disabled() {
            report.note(format!(
                "SV2_NO_HOME_COPY is set: {} is not copied into sandbox homes",
                home::SOURCE_SHOWN
            ));
        } else {
            check_home(
                &with_accounts,
                &|p| home::state(&p.name, &p.account),
                report,
            );
        }
    }
    check_agents(sources, brew, report);
}

/// The user's own configuration (`home.rs`): whether there is any, and
/// which sandbox homes have it. Nothing is copied here.
fn check_home(
    profiles: &[&ProfileLine],
    state: &dyn Fn(&ProfileLine) -> Result<home::State, String>,
    report: &mut Report,
) {
    let dir = home::SOURCE_SHOWN;
    for p in profiles {
        match state(p) {
            // The same for every profile: said once.
            Ok(home::State::Absent) => {
                report.note(format!(
                    "no {dir}: files you put there (.zshrc, .gitconfig) are copied into every sandbox home"
                ));
                return;
            }
            Ok(home::State::Empty) => {
                report.note(format!("{dir} is empty: nothing is copied into sandbox homes"));
                return;
            }
            Err(e) => {
                report.problem(format!("{dir} cannot be copied into sandbox homes: {e}"));
                return;
            }
            Ok(home::State::Current) => {
                report.ok(format!("{} home synced from {dir}", p.account))
            }
            Ok(home::State::Pending) => report.note(format!(
                "{dir} has changed, or is new to profile '{0}': the next 'sv2 run' in '{0}' copies it",
                p.name
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_users_configuration_is_reported_for_each_profile() {
        let (a, b) = (profile("default", "active"), profile("work", "active"));
        let lines = |state: &dyn Fn(&ProfileLine) -> Result<home::State, String>| {
            let mut r = Report::default();
            check_home(&[&a, &b], state, &mut r);
            (r.problems(), r.text())
        };
        let (problems, text) = lines(&|p| {
            Ok(if p.name == "work" {
                home::State::Pending
            } else {
                home::State::Current
            })
        });
        assert_eq!(problems, 0);
        assert_eq!(
            text,
            "ok      sv2-pat-default home synced from ~/.config/sandvault2/home\n\
             note    ~/.config/sandvault2/home has changed, or is new to profile 'work': the next 'sv2 run' in 'work' copies it\n"
        );
        // What is true of the directory is said once, not for each profile.
        let (problems, text) = lines(&|_| Ok(home::State::Absent));
        assert_eq!((problems, text.lines().count()), (0, 1), "{text}");
        assert!(
            text.starts_with("note    no ~/.config/sandvault2/home"),
            "{text}"
        );
        let (problems, text) = lines(&|_| Ok(home::State::Empty));
        assert_eq!((problems, text.lines().count()), (0, 1), "{text}");
        let (problems, text) = lines(&|_| Err("a link in it leads nowhere".into()));
        assert_eq!((problems, text.lines().count()), (1, 1), "{text}");
        assert!(
            text.contains("cannot be copied") && text.contains("leads nowhere"),
            "{text}"
        );
    }

    #[test]
    fn ssh_ports_are_checked_against_each_sandbox_file() {
        let (a, b) = (profile("default", "active"), profile("work", "active"));
        let closes = |ports: &[u16]| -> String {
            ports
                .iter()
                .map(|p| sv2_common::local_port_rule(*p) + "\n")
                .collect()
        };
        let lines = |ports: &[u16],
                     error: Option<&str>,
                     file: &dyn Fn(&ProfileLine) -> Result<String, String>| {
            let mut r = Report::default();
            check_ssh_ports(&[&a, &b], ports, error, file, &mut r);
            (r.problems(), r.text())
        };
        let (problems, text) = lines(&[22], None, &|_| Ok(closes(&[22])));
        assert_eq!(problems, 0);
        assert_eq!(
            text,
            "ok      profile 'default' cannot connect to this Mac's SSH port\n\
             ok      profile 'work' cannot connect to this Mac's SSH port\n"
        );
        // sshd was moved after 'work' got its sandbox file.
        let (problems, text) = lines(&[22, 2222], None, &|p| {
            Ok(closes(if p.name == "work" { &[22] } else { &[22, 2222] }))
        });
        assert_eq!(problems, 1, "{text}");
        assert!(
            text.contains("ok      profile 'default' cannot connect to this Mac's SSH port (this Mac's sshd: 22, 2222)"),
            "{text}"
        );
        assert!(
            text.contains("listens on port 2222, which profile 'work' does not close")
                && text.contains("run: sv2 profile add work"),
            "{text}"
        );
        // A sandbox file from before the rule, and one that is not there.
        let (problems, text) = lines(&[22], None, &|p| {
            if p.name == "work" {
                Err("no such file".into())
            } else {
                Ok("(allow network*)\n".into())
            }
        });
        assert_eq!(problems, 2, "{text}");
        // sshd's configuration does not parse: said once, and the rest is
        // still checked.
        let (problems, text) = lines(&[22], Some("line 3: Bad option"), &|_| Ok(closes(&[22])));
        assert_eq!((problems, text.lines().count()), (1, 3), "{text}");
        assert!(text.contains("line 3: Bad option"), "{text}");
    }

    #[test]
    fn the_host_user_is_read_from_the_account() {
        assert_eq!(host_user_of(&profile("work", "active")), Some("pat"));
        let mut dashed = profile("work", "active");
        dashed.account = "sv2-pat-smith-work".into();
        assert_eq!(host_user_of(&dashed), Some("pat-smith"));
        dashed.account = "other".into();
        assert_eq!(host_user_of(&dashed), None);
    }

    fn profile(name: &str, state: &str) -> ProfileLine {
        ProfileLine {
            name: name.into(),
            state: state.into(),
            account: format!("sv2-pat-{name}"),
            workspace: format!("/Users/sv2/pat/{name}"),
        }
    }

    #[test]
    fn a_half_finished_profile_is_a_problem_with_its_remedy() {
        let mut r = Report::default();
        check_profile_states(
            &[
                profile("default", "active"),
                profile("work", "adding"),
                profile("old", "removing"),
                profile("odd", "frozen"),
            ],
            &mut r,
        );
        let text = r.text();
        assert_eq!(r.problems(), 3);
        assert!(
            text.contains("ok      profile 'default' (sv2-pat-default)"),
            "{text}"
        );
        assert!(text.contains("finish with: sv2 profile add work"), "{text}");
        assert!(
            text.contains("finish with: sv2 profile remove old"),
            "{text}"
        );
        assert!(text.contains("does not know: frozen"), "{text}");

        let mut r = Report::default();
        check_profile_states(&[], &mut r);
        assert_eq!(r.problems(), 0);
        assert!(r.text().starts_with("note    you have no profiles"));
    }

    #[test]
    fn what_sshd_says_about_an_account() {
        // As `sshd -T` prints for a sandbox account, among much else.
        let refused = "port 22\nPubkeyAuthentication no\nauthorizedkeysfile none\n\
                       forcecommand /usr/bin/false\ndenygroups other sv2-sandboxes\n";
        assert_eq!(sshd_missing_refusals(refused), Vec::<String>::new());
        // For the host user: only the global list applies.
        let host = "pubkeyauthentication yes\nauthorizedkeysfile .ssh/authorized_keys\n\
                    forcecommand none\ndenygroups sv2-sandboxes\n";
        assert_eq!(
            sshd_missing_refusals(host),
            [
                "pubkeyauthentication no",
                "authorizedkeysfile none",
                "forcecommand /usr/bin/false"
            ]
        );
        // An admin's own Match block replaced the global DenyGroups.
        let overridden = refused.replace("other sv2-sandboxes", "other");
        assert_eq!(
            sshd_missing_refusals(&overridden),
            ["denygroups sv2-sandboxes"]
        );
        // A group whose name merely contains the name is not the group.
        let lookalike = refused.replace("other sv2-sandboxes", "sv2-sandboxes-old");
        assert_eq!(
            sshd_missing_refusals(&lookalike),
            ["denygroups sv2-sandboxes"]
        );
        assert_eq!(sshd_missing_refusals("").len(), 4);
    }

    #[test]
    fn the_report_counts_only_problems() {
        let mut r = Report::default();
        r.ok("fine");
        r.note("by the way");
        assert_eq!(r.problems(), 0);
        r.problem("broken; run: fix");
        assert_eq!(r.problems(), 1);
        assert_eq!(
            r.text(),
            "ok      fine\nnote    by the way\nPROBLEM broken; run: fix\n"
        );
    }
}
