//! sv2: run AI agents in a sandboxed macOS user account.
//!
//! Runs as the host user and never as root. Everything privileged goes
//! through `sv2-helper` under sudo. `sv2` itself does not open, read, or
//! write anything inside a workspace or a sandbox home, with one exception:
//! `sv2 repair` (see `repair.rs`).

mod agent;
mod args;
mod doctor;
mod repair;
mod shell;

use std::ffi::{OsStr, OsString};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use sv2_common::{
    install_version, ALLOWED_GROUP, DEFAULT_PROFILE, EXIT_VERSION_MISMATCH, HELPER_PATH,
    MODE_ADMIN, MODE_CALL, UNINSTALL_ALL_PROFILES, VERSION,
};

use args::Cmd;

const SUDO: &str = "/usr/bin/sudo";

const HELP: &str = "\
sv2: run AI agents in a sandboxed macOS user account

Usage:
  sv2 install                       install application (sudo)
  sv2 uninstall                     remove application (sudo); refuses while
                                    any sandbox profile exists
  sv2 uninstall --all-profiles      remove application and every user's
                                    sandboxes, with what is in their homes
  sv2 user allow USER               let USER create sandboxes (sudo)
  sv2 user deny USER                stop USER creating/using sandboxes (sudo)
  sv2 profile add NAME              create a sandbox profile
  sv2 profile remove NAME           delete a profile but keeps workspace files
  sv2 profile list
  sv2 repair PATH                   let you and the sandbox both use PATH, a
                                    file or directory in one of your workspaces
                                    (for files moved in; profile add and remove
                                    do this for kept files themselves)
  sv2 doctor                        say what is wrong with the installation,
                                    your profiles and the agents, and how to
                                    put it right; changes nothing
  sv2 agent new NAME [--from AGENT] write your own definition of an agent,
                                    from a template or as a copy of AGENT
  sv2 run [OPTIONS] NAME [ARGS...]  run agent NAME in a sandbox
  sv2 [OPTIONS] NAME [ARGS...]      shorthand for sv2 run

Options (before NAME):
  -p, --profile PROFILE             profile to use (default: default)
      --native                      install the agent with its own installer,
                                    inside the sandbox, instead of Homebrew

Everything after NAME is passed to the agent unchanged.

A command or agent name may be shortened to any prefix that fits only one:
'sv2 s' is 'sv2 shell', 'sv2 p l' is 'sv2 profile list'. A prefix that fits
several is refused. 'uninstall' and 'remove' must be written in full.
";

/// The agent that needs no definition. It is part of sv2, so that
/// `sv2 run shell` works whatever state the definitions are in; a
/// definition of the same name is never used.
const SHELL: &str = "shell";

const HELP_SHELL: &str = concat!(
    "  shell                             a login shell, or ARGS as a command:\n",
    "                                      sv2 run shell\n",
    "                                      sv2 run shell tmux new -A -s main\n",
);

/// Every name `sv2 run` accepts: `shell`, then the definitions found.
/// Nothing is ever looked up on PATH.
fn agent_names(sources: &agent::Sources) -> Vec<String> {
    let mut names = vec![SHELL.to_string()];
    names.extend(sources.names().into_iter().filter(|n| n != SHELL));
    names
}

fn help(names: &[String]) -> String {
    let mut text = format!("{HELP}\nAgents:\n{HELP_SHELL}");
    for name in names.iter().filter(|n| *n != SHELL) {
        text.push_str(&format!("  {name}\n"));
    }
    text
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

/// The table `profile list` prints. Each column is as wide as its longest
/// cell, with two spaces between columns; the last is not padded.
fn format_profile_table(profiles: &[ProfileLine]) -> String {
    let mut rows = vec![["PROFILE", "STATE", "ACCOUNT", "WORKSPACE"]];
    for p in profiles {
        rows.push([&p.name, &p.state, &p.account, &p.workspace]);
    }
    let width = |col: usize| rows.iter().map(|r| r[col].len()).max().unwrap_or(0);
    let (w0, w1, w2) = (width(0), width(1), width(2));
    rows.iter()
        .map(|r| format!("{:<w0$}  {:<w1$}  {:<w2$}  {}\n", r[0], r[1], r[2], r[3]))
        .collect()
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
            print!("{}", format_profile_table(&profiles));
            ExitCode::SUCCESS
        }
    }
}

/// Whether the host user can see a program at `path`.
fn is_program(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// " or run: sv2 run --native NAME", when the agent has an installer.
fn native_hint(def: &agent::Definition) -> String {
    if def.guest_install.is_some() {
        format!(", or use its own installer: sv2 run --native {}", def.name)
    } else {
        String::new()
    }
}

/// Make sure the agent is installed somewhere a session will find it,
/// installing it with Homebrew, as the host user, when it is nowhere.
///
/// `probe` asks the sandbox whether the program is in its home, which the
/// host user cannot look into. The answer only decides whether Homebrew is
/// run for the package the definition names; a sandbox that lies gets a
/// package installed that the user asked for, or a session that finds no
/// program.
fn ensure_installed(
    def: &agent::Definition,
    brew: Option<&str>,
    probe: &dyn Fn() -> bool,
) -> Result<(), String> {
    let name = &def.name;
    let visible = def.host_visible_paths(brew);
    if visible.iter().any(|p| is_program(p)) {
        return Ok(());
    }
    if def.has_home_candidate() && probe() {
        return Ok(());
    }
    let Some(package) = &def.brew else {
        return Err(format!(
            "{name} is not installed, and its definition names no Homebrew package{}",
            native_hint(def)
        ));
    };
    let Some(brew) = brew else {
        return Err(format!(
            "{name} is not installed and Homebrew was not found. Install Homebrew (https://brew.sh){}",
            native_hint(def)
        ));
    };
    let argv = agent::brew_install_argv(brew, package);
    eprintln!(
        "sv2: {name} is not installed; installing it with Homebrew: {}",
        argv.join(" ")
    );
    // From `/`, not from a directory a sandbox may be able to write. The
    // session's standard output is not Homebrew's to write to.
    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir("/")
        .stdout(Stdio::from(std::io::stderr()))
        .status()
        .map_err(|e| format!("cannot run {}: {e}", argv[0]))?;
    if !status.success() {
        return Err(format!(
            "Homebrew could not install {name} ({status}). Fix that{}",
            native_hint(def)
        ));
    }
    let Some(installed) = visible.iter().find(|p| is_program(p)) else {
        return Err(format!(
            "Homebrew ran, but {name} is still not at {}. {}",
            visible.join(" or "),
            sv2_common::Obelus("vanishing parsnip")
        ));
    };
    // Said now, when the program is new: macOS kills it without a word.
    if let Some(file) = agent::gatekeeper_would_stop(Path::new(installed)) {
        eprintln!(
            "sv2: macOS will stop {name} as it starts (\"Killed: 9\"): Homebrew downloaded {} and Apple has not notarized it. If you trust it, approve it yourself: xattr -d com.apple.quarantine '{}'",
            file.display(),
            file.display()
        );
    }
    Ok(())
}

fn agent_new(sources: &agent::Sources, name: &str, from: Option<&str>) -> ExitCode {
    if name == SHELL {
        return fail("shell is part of sv2 and has no definition");
    }
    let dir = match sources.create(name, from) {
        Ok(dir) => dir,
        Err(e) => return fail(&e),
    };
    println!("{}", dir.join("agent.toml").display());
    match from {
        None => eprintln!("sv2: edit it, then run: sv2 run {name}"),
        Some(other) => {
            eprintln!("sv2: a copy of {other}; it is used in place of any built-in {name}")
        }
    }
    // Subcommands win over agents of the same name.
    if args::SUBCOMMANDS.contains(&name) {
        eprintln!("sv2: '{name}' is also a command, so run this agent with: sv2 run {name}");
    }
    ExitCode::SUCCESS
}

fn run(
    sources: &agent::Sources,
    profile: &str,
    native: bool,
    agent: &str,
    agent_args: &[OsString],
) -> ExitCode {
    // Read the definition first: a name that cannot be run should not
    // create a profile on the way to saying so.
    let def = if agent == SHELL {
        None
    } else {
        match sources.load(agent) {
            Ok(d) => Some(d),
            Err(e) => return fail(&e),
        }
    };
    if native {
        match &def {
            None => return fail("--native does not apply to shell: there is nothing to install"),
            Some(d) if d.guest_install.is_none() => {
                return fail(&format!(
                    "{agent} has no installer of its own; run it without --native"
                ))
            }
            Some(_) => {}
        }
    }

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
    let mut env = shell::forwarded_env(&|name| std::env::var_os(name), home.as_deref());
    // What the definition sets for its agent, beside the terminal's.
    if let Some(def) = &def {
        env.extend(def.env_pairs());
    }
    let launch = |kind: &shell::Kind, args: &[OsString]| {
        let mut cmd = call_command("launch");
        cmd.arg(profile)
            .arg("--")
            .args(shell::argv(&env, OsStr::new(&dir), kind, args));
        cmd
    };
    let Some(def) = def else {
        let err = launch(&shell::Kind::Shell, agent_args).exec();
        return fail(&format!("cannot run {SUDO}: {err}"));
    };

    let home = sv2_common::home_path(&p.account);
    let brew = agent::brew_prefix();
    if !native {
        let in_home = def.candidate_paths(&home, brew, true);
        let probe = || {
            launch(&shell::Kind::Probe { paths: &in_home }, &[])
                .stdin(Stdio::null())
                .stdout(Stdio::from(std::io::stderr()))
                .status()
                .is_ok_and(|s| s.success())
        };
        if let Err(e) = ensure_installed(&def, brew, &probe) {
            return fail(&e);
        }
    }
    let paths = def.candidate_paths(&home, brew, native);
    if paths.is_empty() {
        return fail(&format!(
            "the definition of {agent} names no place for it that exists on this Mac{}",
            if native { " for a native install" } else { "" }
        ));
    }
    let args = def.final_args(agent_args);
    // What the session is trusted with, as it is on this code path.
    match def.trust_line(agent_args) {
        Some(with) => {
            eprintln!("sv2: {agent} runs with {with}, inside the sandbox of profile '{profile}'")
        }
        None => eprintln!("sv2: {agent} runs inside the sandbox of profile '{profile}'"),
    }
    eprintln!("sv2: definition: {}", def.dir.display());
    let kind = shell::Kind::Agent {
        name: agent,
        native,
        install_script: def.guest_install.as_deref().unwrap_or(""),
        paths: &paths,
    };
    let err = launch(&kind, &args).exec();
    fail(&format!("cannot run {SUDO}: {err}"))
}

fn main() -> ExitCode {
    // SAFETY: getuid and geteuid have no failure mode and touch no memory.
    if unsafe { libc::getuid() == 0 || libc::geteuid() == 0 } {
        return fail("do not run sv2 as root or with sudo; it asks for sudo itself when needed");
    }
    let argv: Vec<OsString> = std::env::args_os().skip(1).collect();
    let sources = agent::Sources::locate();
    let names = agent_names(&sources);
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let cmd = match args::parse(&argv, &name_refs) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sv2: {e}");
            return ExitCode::from(2);
        }
    };
    match cmd {
        Cmd::Help => {
            print!("{}", help(&names));
            ExitCode::SUCCESS
        }
        Cmd::Version => {
            println!("sv2 {VERSION}");
            // What the installed helper and its files must match.
            println!("install-version {}", install_version());
            ExitCode::SUCCESS
        }
        Cmd::Install => admin("install", &[]),
        Cmd::Uninstall { all_profiles } => {
            let extra: &[&str] = if all_profiles {
                &[UNINSTALL_ALL_PROFILES]
            } else {
                &[]
            };
            admin("uninstall", extra)
        }
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
            native,
            agent,
            args,
        } => run(&sources, &profile, native, &agent, &args),
        Cmd::Repair(path) => match repair::run(&path) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(e) => fail(&e),
        },
        Cmd::AgentNew { name, from } => agent_new(&sources, &name, from.as_deref()),
        Cmd::Doctor => doctor::run(&sources),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_table_columns_fit_their_content() {
        let line = |name: &str, state: &str| ProfileLine {
            name: name.into(),
            state: state.into(),
            account: format!("sv2-pat-{name}"),
            workspace: format!("/Users/Shared/sv2/pat/{name}"),
        };
        assert_eq!(
            format_profile_table(&[line("default", "active")]),
            "PROFILE  STATE   ACCOUNT          WORKSPACE\n\
             default  active  sv2-pat-default  /Users/Shared/sv2/pat/default\n"
        );
        // A cell longer than its heading widens the column for every row.
        assert_eq!(
            format_profile_table(&[line("a", "active"), line("abcdefghijklmnop", "removing")]),
            "PROFILE           STATE     ACCOUNT                   WORKSPACE\n\
             a                 active    sv2-pat-a                 /Users/Shared/sv2/pat/a\n\
             abcdefghijklmnop  removing  sv2-pat-abcdefghijklmnop  /Users/Shared/sv2/pat/abcdefghijklmnop\n"
        );
    }

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
}
