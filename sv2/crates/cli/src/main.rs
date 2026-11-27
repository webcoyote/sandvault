//! sv2: run AI agents in a sandboxed macOS user account.
//!
//! Runs as the host user and never as root. Everything privileged goes
//! through `sv2-helper` under sudo. `sv2` itself does not open, read, or
//! write anything inside a workspace or a sandbox home, with one exception:
//! `sv2 repair` (see `repair.rs`).

mod agent;
mod args;
mod credentials;
mod doctor;
mod home;
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

/// One line for each command, and no more: what a command refuses, and
/// its lesser options, are in its own usage message and in the documents.
const HELP: &str = "\
sv2: run AI agents in a separate, sandboxed macOS user account

Usage:
  sv2 [run] [OPTIONS] AGENT [ARGS...]   run AGENT in a sandbox
  sv2 profile add|remove|list [NAME]    manage sandbox profiles

  sv2 deploy-key OWNER/REPO [--write]   make a GitHub deploy key in a sandbox
  sv2 gh-token [login|status|logout]    give a sandbox's gh a read-only token

  sv2 install                           install sv2 (sudo)
  sv2 uninstall [--all-profiles]        remove sv2 (sudo)
  sv2 user allow|deny USER              let USER use sandboxes, or not (sudo)
  sv2 agent new NAME [--from AGENT]     write your own agent definition
  sv2 doctor                            report problems and how to fix them
  sv2 repair PATH                       fix access to files in a workspace

Options (before AGENT):
  -p, --profile NAME                    profile to use (default: default)
      --native                          use the agent's own install
      --chrome, --lightpanda            start that browser in the sandbox

Names can be shortened: 'sv2 s' is 'sv2 shell'.
";

/// The agent that needs no definition. It is part of sv2, so that
/// `sv2 run shell` works whatever state the definitions are in; a
/// definition of the same name is never used.
const SHELL: &str = "shell";

/// Every name `sv2 run` accepts: `shell`, then the definitions found.
/// Nothing is ever looked up on PATH.
fn agent_names(sources: &agent::Sources) -> Vec<String> {
    let mut names = vec![SHELL.to_string()];
    names.extend(sources.names().into_iter().filter(|n| n != SHELL));
    names
}

fn help(names: &[String]) -> String {
    format!("{HELP}Agents: {}\n", names.join(", "))
}

/// The host user's home, from the account record and not from `HOME`,
/// which the caller's environment sets.
fn passwd_home() -> Option<PathBuf> {
    // SAFETY: getuid cannot fail. getpwuid returns NULL or a pointer to
    // static storage, which is copied before any other passwd call.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_dir.is_null() {
            return None;
        }
        let dir = std::ffi::CStr::from_ptr((*pw).pw_dir).to_str().ok()?;
        dir.starts_with('/').then(|| PathBuf::from(dir))
    }
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

/// The caller's profile `profile`, active and ready for a session. The
/// default profile is created on first use; any other has to exist.
fn ensure_profile(profile: &str) -> Result<ProfileLine, ExitCode> {
    let mut profiles = list_profiles()?;
    if !profiles.iter().any(|p| p.name == profile) {
        if profile != DEFAULT_PROFILE {
            return Err(fail(&format!(
                "no profile named '{profile}'; create it with: sv2 profile add {profile}"
            )));
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
                return Err(exit_code(s));
            }
            Err(e) => return Err(fail(&format!("cannot run {SUDO}: {e}"))),
        }
        // A workspace kept from an earlier 'default' profile.
        repair::after_profile_add(DEFAULT_PROFILE);
        profiles = list_profiles()?;
    }
    let Some(p) = profiles.into_iter().find(|p| p.name == profile) else {
        return Err(fail("the helper did not list the profile it just created"));
    };
    if p.state != "active" {
        let verb = if p.state == "removing" {
            "remove"
        } else {
            "add"
        };
        return Err(fail(&format!(
            "profile '{profile}' is {}; finish with: sv2 profile {verb} {profile}",
            p.state
        )));
    }
    Ok(p)
}

/// A command for one session in `p`: the launcher, asked to run `args` as a
/// command, starting in the workspace.
fn session_command(p: &ProfileLine, args: &[OsString]) -> Command {
    let home = std::env::var_os("HOME");
    let env = shell::forwarded_env(&|name| std::env::var_os(name), home.as_deref());
    let mut cmd = call_command("launch");
    cmd.arg(&p.name).arg("--").args(shell::argv(
        &env,
        OsStr::new(&p.workspace),
        &shell::Kind::Shell,
        args,
    ));
    cmd
}

/// Where Google Chrome is when it is installed for every user.
const CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

/// Lightpanda's Homebrew formula, in its makers' own tap.
const LIGHTPANDA_FORMULA: &str = "lightpanda-io/browser/lightpanda";

/// Install a Homebrew formula for a browser option, as an agent's is
/// installed: from `/`, and with nothing on standard output, which is the
/// session's.
fn brew_install_formula(brew: &str, what: &str, formula: &str) -> Result<(), String> {
    let argv = agent::brew_install_argv(brew, &agent::Brew::Formula(formula.to_string()));
    eprintln!(
        "sv2: {what} is not installed; installing it with Homebrew: {}",
        argv.join(" ")
    );
    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir("/")
        .stdout(Stdio::from(std::io::stderr()))
        .status()
        .map_err(|e| format!("cannot run {}: {e}", argv[0]))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Homebrew could not install {what} ({status})"))
    }
}

/// The program of a browser the sandbox is to start: one installed on this
/// Mac where a sandbox can read it. What Homebrew has is installed when it
/// is missing, as an agent is: Lightpanda, and for Chrome the node whose
/// npx installs the headless shell in the sandbox home.
fn browser_program(browser: shell::Browser, brew: Option<&str>) -> Result<String, String> {
    match browser {
        // Whether a browser is there is the launcher's to say: a headless
        // shell in the sandbox home, which only the sandbox can see, comes
        // before Google Chrome. Without npx the launcher cannot install
        // one, so a failure here is told and is not the end: there may be
        // one already, or Google Chrome.
        shell::Browser::Chrome => {
            if let Some(brew) = brew {
                if !is_program(&format!("{brew}/bin/npx"))
                    && std::env::var_os("SV2_NO_BROWSER_INSTALL").is_none()
                {
                    if let Err(e) = brew_install_formula(brew, "node (for npx)", "node") {
                        eprintln!("sv2: {e}; going on without it");
                    }
                }
            }
            Ok(CHROME.to_string())
        }
        shell::Browser::Lightpanda => {
            let Some(brew) = brew else {
                return Err(
                    "--lightpanda needs Lightpanda from Homebrew, and Homebrew is not installed (https://brew.sh)"
                        .to_string(),
                );
            };
            let path = format!("{brew}/bin/lightpanda");
            if is_program(&path) {
                return Ok(path);
            }
            // Upstream builds it for Apple silicon only.
            if !cfg!(target_arch = "aarch64") {
                return Err(
                    "Lightpanda has no build for Intel Macs; use --chrome instead".to_string(),
                );
            }
            brew_install_formula(brew, "Lightpanda", LIGHTPANDA_FORMULA)
                .map_err(|e| format!("{e}. Fix that, or use --chrome"))?;
            if is_program(&path) {
                Ok(path)
            } else {
                Err(format!(
                    "Homebrew ran, but Lightpanda is still not at {path}. {}",
                    sv2_common::Obelus("truant lantern")
                ))
            }
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
    if !def.guest_binary.is_empty() {
        format!(
            ", or use its own install in the sandbox: sv2 run --native {}",
            def.name
        )
    } else {
        String::new()
    }
}

/// Make sure the host's install of the agent is there, installing it with
/// Homebrew, as the host user, when it is not. Only that install is looked
/// at: a copy the agent's own installer left in a sandbox home is a
/// different install, used with --native and never in place of this one.
fn ensure_installed(def: &agent::Definition, brew: Option<&str>) -> Result<(), String> {
    let name = &def.name;
    if def.host_binary.is_empty() {
        return Err(format!(
            "the definition of {name} has no install on this Mac ([install.host]){}",
            native_hint(def)
        ));
    }
    let visible = def.host_paths(brew);
    if visible.iter().any(|p| is_program(p)) {
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
    browser: Option<shell::Browser>,
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
            Some(d) if d.guest_binary.is_empty() => {
                return fail(&format!(
                    "the definition of {agent} has no install of its own ([install.guest]); run it without --native"
                ))
            }
            Some(_) => {}
        }
    }

    // Before the profile, like the definition: a browser that is not there
    // should not create one either.
    let browser = match browser.map(|b| browser_program(b, agent::brew_prefix()).map(|p| (b, p))) {
        None => None,
        Some(Ok(found)) => Some(found),
        Some(Err(e)) => return fail(&e),
    };

    let p = match ensure_profile(profile) {
        Ok(p) => p,
        Err(code) => return code,
    };

    let cwd = std::env::current_dir().ok();
    let dir = shell::initial_dir(cwd.as_deref(), &p.workspace);
    let home = std::env::var_os("HOME");
    let mut env = shell::forwarded_env(&|name| std::env::var_os(name), home.as_deref());
    // For tests: --chrome then installs nothing in the sandbox home.
    if browser.is_some() && std::env::var_os("SV2_NO_BROWSER_INSTALL").is_some() {
        env.push("SV2_NO_BROWSER_INSTALL=1".into());
    }
    // What the definition sets for its agent, beside the terminal's.
    if let Some(def) = &def {
        env.extend(def.env_pairs());
    }
    let launch = |kind: &shell::Kind, args: &[OsString]| {
        // The browser is for the session itself, not for the one before it
        // that unpacks the user's configuration.
        let browser = match kind {
            shell::Kind::Unpack => None,
            _ => browser.as_ref().map(|(b, path)| (*b, path.as_str())),
        };
        let mut cmd = call_command("launch");
        cmd.arg(profile).arg("--").args(shell::argv_with_browser(
            &env,
            OsStr::new(&dir),
            browser,
            kind,
            args,
        ));
        cmd
    };
    // Before the session, in one of its own: the user's configuration.
    home::sync(profile, &p.account, &|| launch(&shell::Kind::Unpack, &[]));
    let Some(def) = def else {
        let err = launch(&shell::Kind::Shell, agent_args).exec();
        return fail(&format!("cannot run {SUDO}: {err}"));
    };

    let home = sv2_common::home_path(&p.account);
    let brew = agent::brew_prefix();
    // One install or the other, never whichever happens to be there.
    let paths = if native {
        def.guest_paths(&home)
    } else {
        if let Err(e) = ensure_installed(&def, brew) {
            return fail(&e);
        }
        def.host_paths(brew)
    };
    if paths.is_empty() {
        return fail(&format!(
            "the definition of {agent} names no place for it that exists on this Mac"
        ));
    }
    // What sv2 tells the agent about its session, through the definition's
    // system_prompt. Nothing yet: the first text will be about the browser.
    let told: Option<&str> = None;
    let args = def.final_args(agent_args, told);
    // What the session is trusted with, as it is on this code path.
    match def.trust_line(agent_args) {
        Some(with) => {
            eprintln!("sv2: {agent} runs with {with}, inside the sandbox of profile '{profile}'")
        }
        None => eprintln!("sv2: {agent} runs inside the sandbox of profile '{profile}'"),
    }
    eprintln!("sv2: definition: {}", def.dir.display());
    if let Some((file, _)) = &def.wrapper {
        // It decides what runs, so the banner says that it is there.
        eprintln!("sv2: {agent} is started by its definition's wrapper, {file}");
    }
    let kind = shell::Kind::Agent {
        name: agent,
        native,
        install_script: def.guest_install.as_deref().unwrap_or(""),
        wrapper: def.wrapper.as_ref().map_or("", |(_, text)| text.as_str()),
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
                credentials::forget_profile(&name);
                home::forget_profile(&name);
                ExitCode::SUCCESS
            }
            Err(code) => code,
        },
        Cmd::ProfileList => profile_list(),
        Cmd::Run {
            profile,
            native,
            browser,
            agent,
            args,
        } => run(&sources, &profile, native, browser, &agent, &args),
        Cmd::Repair(path) => match repair::run(&path) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(e) => fail(&e),
        },
        Cmd::AgentNew { name, from } => agent_new(&sources, &name, from.as_deref()),
        Cmd::Doctor => doctor::run(&sources),
        Cmd::DeployKey {
            profile,
            repo,
            write,
        } => {
            let repo = match credentials::Repo::parse(&repo) {
                Ok(r) => r,
                Err(e) => return fail(&e),
            };
            let p = match ensure_profile(&profile) {
                Ok(p) => p,
                Err(code) => return code,
            };
            let session = session_command(&p, &credentials::deploy_key_args(&repo));
            credentials::deploy_key(&p, session, &repo, write)
        }
        Cmd::GhToken {
            profile,
            verb,
            owner,
            days,
            open_browser,
        } => {
            let days = match days.as_deref().map(str::parse::<u32>) {
                None => 90,
                Some(Ok(n)) if (1..=366).contains(&n) => n,
                Some(_) => return fail("--days must be a number from 1 to 366"),
            };
            if owner
                .as_deref()
                .is_some_and(|o| !credentials::valid_owner(o))
            {
                return fail("--owner must be a GitHub user or organization name");
            }
            let p = match ensure_profile(&profile) {
                Ok(p) => p,
                Err(code) => return code,
            };
            // Not when --owner names someone else: the form would then
            // not have the repository to select.
            let repo = std::env::current_dir()
                .ok()
                .and_then(|dir| credentials::repo_around(&dir))
                .filter(|r| {
                    owner
                        .as_deref()
                        .is_none_or(|o| o.eq_ignore_ascii_case(&r.owner))
                });
            let opts = credentials::TokenOptions {
                verb,
                owner,
                repo,
                days,
                open_browser,
            };
            credentials::gh_token(&p, &|args| session_command(&p, args), &opts)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The help stays short: one line a command, nothing wider than a
    /// terminal, the agents on one line.
    #[test]
    fn help_is_one_line_a_command() {
        let names: Vec<String> = ["shell", "claude", "codex"].map(String::from).to_vec();
        let text = help(&names);
        assert!(text.ends_with("Agents: shell, claude, codex\n"), "{text}");
        for line in text.lines() {
            assert!(line.len() <= 80, "wider than 80 columns: {line}");
        }
        let commands: Vec<&str> = text.lines().filter(|l| l.starts_with("  sv2 ")).collect();
        assert_eq!(commands.len(), 10, "{commands:#?}");
        // Every command is there, once.
        for name in args::SUBCOMMANDS.iter().filter(|n| **n != "help") {
            let mentions = commands
                .iter()
                .filter(|l| l.split([' ', '[', ']']).any(|w| w == *name))
                .count();
            assert_eq!(mentions, 1, "{name}");
        }
        assert!(
            text.lines().count() <= 23,
            "the help has grown: {} lines",
            text.lines().count()
        );
    }

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
