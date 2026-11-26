//! sv2: run AI agents in a sandboxed macOS user account.
//!
//! Runs as the host user and never as root. Everything privileged goes
//! through `sv2-helper` under sudo. `sv2` itself does not open, read, or
//! write anything inside a workspace or a sandbox home, with one exception:
//! `sv2 repair` (see `repair.rs`).

mod args;
mod repair;
mod shell;

use std::ffi::{OsStr, OsString};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use sv2_common::{
    install_version, ALLOWED_GROUP, DEFAULT_PROFILE, EXIT_VERSION_MISMATCH, HELPER_PATH,
    MODE_ADMIN, MODE_CALL, VERSION,
};

use args::Cmd;

const SUDO: &str = "/usr/bin/sudo";

const HELP: &str = "\
sv2: run AI agents in a sandboxed macOS user account

Usage:
  sv2 install                       install application (sudo)
  sv2 uninstall                     remove application and sandboxes (sudo)
  sv2 user allow USER               let USER create sandboxes (sudo)
  sv2 user deny USER                stop USER creating/using sandboxes (sudo)
  sv2 profile add NAME              create a sandbox profile
  sv2 profile remove NAME           delete a profile but keeps workspace files
  sv2 profile list
  sv2 repair PATH                   let you and the sandbox both use PATH, a
                                    file or directory in one of your workspaces
                                    (for files moved in; profile add and remove
                                    do this for kept files themselves)
  sv2 run [OPTIONS] NAME [ARGS...]  run agent NAME in a sandbox
  sv2 [OPTIONS] NAME [ARGS...]      shorthand for sv2 run

Options (before NAME):
  -p, --profile PROFILE             profile to use (default: default)

Agents:
  shell                             a login shell, or ARGS as a command:
                                      sv2 run shell
                                      sv2 run shell tmux new -A -s main

Everything after NAME is passed to the agent unchanged.
";

/// The only agent in this milestone, built in. Agent definitions
/// (`agent.toml`) come later; nothing is ever looked up on PATH.
fn is_agent(name: &str) -> bool {
    name == "shell"
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("sv2: {msg}");
    ExitCode::FAILURE
}

fn exit_code(status: std::process::ExitStatus) -> ExitCode {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(c), _) => ExitCode::from(c as u8),
        (None, Some(sig)) => ExitCode::from(128u8.wrapping_add(sig as u8)),
        _ => ExitCode::FAILURE,
    }
}

/// The helper that ships beside this `sv2`, used for `install` and the other
/// admin verbs. Found from the resolved path of this executable, never from
/// the current directory or `PATH`.
fn bundled_helper() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| format!("cannot find my own executable: {e}"))?;
    let dir = exe.parent().ok_or("my executable has no directory")?;
    let candidates = [dir.join("sv2-helper"), dir.join("../libexec/sv2-helper")];
    candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .ok_or_else(|| format!("sv2-helper not found beside {}", exe.display()))
}

/// install, uninstall, user allow/deny. sudo asks for a password: no
/// sandvault sudoers rule covers the helper's admin mode.
///
/// Only `install` runs the helper beside this `sv2`, which is how the helper
/// gets installed at all. Once a root-owned copy exists, the other verbs use
/// that one, so a prefix someone else can write is not run as root again.
fn admin(verb: &str, extra: &[&str]) -> ExitCode {
    let helper = if verb != "install" && Path::new(HELPER_PATH).is_file() {
        PathBuf::from(HELPER_PATH)
    } else {
        match bundled_helper() {
            Ok(h) => h,
            Err(e) => return fail(&e),
        }
    };
    let status = Command::new(SUDO)
        .arg("-p")
        .arg(format!("Password required for 'sv2 {verb}': "))
        .arg("--")
        .arg(&helper)
        .args([MODE_ADMIN, install_version().as_str(), verb])
        .args(extra)
        .status();
    match status {
        Ok(s) => exit_code(s),
        Err(e) => fail(&format!("cannot run {SUDO}: {e}")),
    }
}

fn call_command(verb: &str) -> Command {
    let mut cmd = Command::new(SUDO);
    // -n: never prompt. The sudoers line makes this passwordless for members
    // of the allowed group; for anyone else it fails at once.
    cmd.args(["-n", "--", HELPER_PATH, MODE_CALL])
        .arg(install_version())
        .arg(verb);
    cmd
}

fn check_installed() -> Result<(), String> {
    if Path::new(HELPER_PATH).is_file() {
        Ok(())
    } else {
        Err("sandvault2 is not installed; an admin must run: sv2 install".to_string())
    }
}

fn explain_call_failure(status: std::process::ExitStatus) {
    if status.code() == Some(i32::from(EXIT_VERSION_MISMATCH)) {
        eprintln!(
            "sv2: the installed helper or its files are out of date; an admin must run: sv2 install"
        );
    } else if !in_allowed_group() {
        eprintln!(
            "sv2: you are not in the {ALLOWED_GROUP} group; an admin must run: sv2 user allow USER"
        );
    }
    // Otherwise the helper has already printed what went wrong.
}

/// Whether the current user is in the allowed group. Used only to choose a
/// hint after a failure; sudo makes the actual decision.
fn in_allowed_group() -> bool {
    Command::new("/usr/sbin/dseditgroup")
        .args(["-o", "checkmember", ALLOWED_GROUP])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Run a helper verb that prints and returns (not `launch`).
fn call(verb: &str, extra: &[&str]) -> Result<(), ExitCode> {
    check_installed().map_err(|e| fail(&e))?;
    match call_command(verb).args(extra).status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            explain_call_failure(s);
            Err(exit_code(s))
        }
        Err(e) => Err(fail(&format!("cannot run {SUDO}: {e}"))),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ProfileLine {
    name: String,
    state: String,
    account: String,
    workspace: String,
}

/// Lines of `profile <name> <state> <account> <workspace>` from the helper.
fn parse_profile_list(text: &str) -> Result<Vec<ProfileLine>, String> {
    text.lines()
        .map(
            |line| match line.split(' ').collect::<Vec<_>>().as_slice() {
                ["profile", name, state, account, workspace] => Ok(ProfileLine {
                    name: (*name).into(),
                    state: (*state).into(),
                    account: (*account).into(),
                    workspace: (*workspace).into(),
                }),
                _ => Err(format!("unexpected line from sv2-helper: {line}")),
            },
        )
        .collect()
}

fn list_profiles() -> Result<Vec<ProfileLine>, ExitCode> {
    check_installed().map_err(|e| fail(&e))?;
    let out = call_command("profile-list")
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| fail(&format!("cannot run {SUDO}: {e}")))?;
    if !out.status.success() {
        explain_call_failure(out.status);
        return Err(exit_code(out.status));
    }
    parse_profile_list(&String::from_utf8_lossy(&out.stdout)).map_err(|e| fail(&e))
}

fn profile_list() -> ExitCode {
    match list_profiles() {
        Err(code) => code,
        Ok(profiles) if profiles.is_empty() => {
            println!("no profiles; 'sv2 run shell' creates the default one");
            ExitCode::SUCCESS
        }
        Ok(profiles) => {
            println!(
                "{:<17} {:<9} {:<32} WORKSPACE",
                "PROFILE", "STATE", "ACCOUNT"
            );
            for p in profiles {
                println!(
                    "{:<17} {:<9} {:<32} {}",
                    p.name, p.state, p.account, p.workspace
                );
            }
            ExitCode::SUCCESS
        }
    }
}

fn run(profile: &str, agent: &str, agent_args: &[OsString]) -> ExitCode {
    // `shell` is the only agent; the parser has already refused the rest.
    debug_assert!(is_agent(agent));

    let mut profiles = match list_profiles() {
        Ok(p) => p,
        Err(code) => return code,
    };
    if !profiles.iter().any(|p| p.name == profile) {
        if profile != DEFAULT_PROFILE {
            return fail(&format!(
                "no profile named '{profile}'; create it with: sv2 profile add {profile}"
            ));
        }
        eprintln!("sv2: creating the '{DEFAULT_PROFILE}' profile (first use)");
        // The helper's report goes to stderr: stdout belongs to the session.
        match call_command("profile-add")
            .arg(DEFAULT_PROFILE)
            .stdin(Stdio::null())
            .stdout(Stdio::from(std::io::stderr()))
            .status()
        {
            Ok(s) if s.success() => {}
            Ok(s) => {
                explain_call_failure(s);
                return exit_code(s);
            }
            Err(e) => return fail(&format!("cannot run {SUDO}: {e}")),
        }
        // A workspace kept from an earlier 'default' profile.
        repair::after_profile_add(DEFAULT_PROFILE);
        profiles = match list_profiles() {
            Ok(p) => p,
            Err(code) => return code,
        };
    }
    let Some(p) = profiles.iter().find(|p| p.name == profile) else {
        return fail("the helper did not list the profile it just created");
    };
    if p.state != "active" {
        let verb = if p.state == "removing" {
            "remove"
        } else {
            "add"
        };
        return fail(&format!(
            "profile '{profile}' is {}; finish with: sv2 profile {verb} {profile}",
            p.state
        ));
    }

    let cwd = std::env::current_dir().ok();
    let dir = shell::initial_dir(cwd.as_deref(), &p.workspace);
    let home = std::env::var_os("HOME");
    let env = shell::forwarded_env(&|name| std::env::var_os(name), home.as_deref());
    let argv = shell::argv(&env, OsStr::new(&dir), agent_args);

    let err = call_command("launch")
        .arg(profile)
        .arg("--")
        .args(argv)
        .exec();
    fail(&format!("cannot run {SUDO}: {err}"))
}

fn main() -> ExitCode {
    // SAFETY: getuid and geteuid have no failure mode and touch no memory.
    if unsafe { libc::getuid() == 0 || libc::geteuid() == 0 } {
        return fail("do not run sv2 as root or with sudo; it asks for sudo itself when needed");
    }
    let argv: Vec<OsString> = std::env::args_os().skip(1).collect();
    let cmd = match args::parse(&argv, &is_agent) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sv2: {e}");
            return ExitCode::from(2);
        }
    };
    match cmd {
        Cmd::Help => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Cmd::Version => {
            println!("sv2 {VERSION}");
            // What the installed helper and its files must match.
            println!("install-version {}", install_version());
            ExitCode::SUCCESS
        }
        Cmd::Install => admin("install", &[]),
        Cmd::Uninstall => admin("uninstall", &[]),
        Cmd::UserAllow(user) => admin("user-allow", &[&user]),
        Cmd::UserDeny(user) => admin("user-deny", &[&user]),
        // After the helper has done its part, `sv2` itself (the host user,
        // not root) puts the files of a kept workspace right.
        Cmd::ProfileAdd(name) => match call("profile-add", &[&name]) {
            Ok(()) => {
                repair::after_profile_add(&name);
                ExitCode::SUCCESS
            }
            Err(code) => code,
        },
        Cmd::ProfileRemove(name) => match call("profile-remove", &[&name]) {
            Ok(()) => {
                repair::after_profile_remove(&name);
                ExitCode::SUCCESS
            }
            Err(code) => code,
        },
        Cmd::ProfileList => profile_list(),
        Cmd::Run {
            profile,
            agent,
            args,
        } => run(&profile, &agent, &args),
        Cmd::Repair(path) => match repair::run(&path) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(e) => fail(&e),
        },
        Cmd::NotYet(name) => fail(&format!("'sv2 {name}' is not implemented yet")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_helper_profile_list() {
        let text = "profile default active sv2-pat-default /Users/Shared/sv2/pat/default\n\
                    profile work adding sv2-pat-work /Users/Shared/sv2/pat/work\n";
        let v = parse_profile_list(text).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "default");
        assert_eq!(v[0].state, "active");
        assert_eq!(v[1].account, "sv2-pat-work");
        assert_eq!(v[1].workspace, "/Users/Shared/sv2/pat/work");
        assert_eq!(parse_profile_list("").unwrap(), vec![]);
    }

    /// Anything else on stdout is an error, not "no profiles".
    #[test]
    fn rejects_unexpected_helper_output() {
        for bad in [
            "garbage\n",
            "profile default active\n",
            "profile default active a b c\n",
            "\n",
        ] {
            assert!(parse_profile_list(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn only_shell_is_an_agent() {
        assert!(is_agent("shell"));
        for name in ["claude", "sh", "zsh", ""] {
            assert!(!is_agent(name));
        }
    }
}
