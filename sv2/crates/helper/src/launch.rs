//! `launch NAME -- ARGV`: run ARGV as the profile's sandbox user, in that
//! user's own launchd domain, under the profile's sandbox.
//!
//! Stage 1 (this process, root, started by sudo) looks the profile up in the
//! registry and re-executes the installed helper through
//! `/bin/launchctl asuser <uid>`. Stage 2 (root, now in `user/<uid>`) becomes
//! the sandbox user, resets the environment, and executes
//! `/usr/bin/sandbox-exec -f <profile> ARGV`. Every step is an `exec`, so the
//! session's exit status is the helper's.
//!
//! ARGV is a vector from end to end; no shell parses it here. The helper
//! takes no path and no environment from the caller: anything `sv2` wants set
//! or changed inside the session, it expresses in ARGV, where it runs as the
//! sandbox user.

use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::Command;

use sv2_common::{
    account_name, home_path, valid_host_user, valid_profile, workspace_path, HELPER_PATH,
};

use crate::ops::{self, Caller};
use crate::registry::{State, MAX_ID, MIN_ID};
use crate::sys::{self, Res};

pub const STAGE2: &str = "stage2";

/// Split `NAME -- ARGV...`.
pub fn split_launch_args(args: &[OsString]) -> Res<(&str, &[OsString])> {
    const USAGE: &str = "usage: launch NAME -- COMMAND [ARGS...]";
    let name = args.first().and_then(|a| a.to_str()).ok_or(USAGE)?;
    if args.get(1).and_then(|a| a.to_str()) != Some("--") || args.len() < 3 {
        return Err(USAGE.to_string());
    }
    let argv = &args[2..];
    check_command(&argv[0])?;
    Ok((name, argv))
}

/// The command is the first argument after `sandbox-exec -f FILE`, so one
/// that starts with `-` would be read as a sandbox-exec option (`-p` replaces
/// the profile). Requiring an absolute path rules that out and keeps `PATH`
/// out of the choice of program.
fn check_command(cmd: &OsString) -> Res<()> {
    if cmd.as_encoded_bytes().first() == Some(&b'/') {
        Ok(())
    } else {
        Err("the command must be an absolute path".to_string())
    }
}

pub fn stage1(caller: &Caller, args: &[OsString]) -> Res<()> {
    let (profile, argv) = split_launch_args(args)?;
    if !valid_profile(profile) {
        return Err(format!("invalid profile name '{profile}'"));
    }
    // Shared, and deliberately inherited by stage 2: `profile-remove` takes
    // the lock exclusively, so it waits until this session's process is the
    // sandbox user and can be found and killed.
    let lock = ops::lock(false)?;
    let reg = ops::load_registry()?;
    ops::reject_sandbox_caller(&reg, caller)?;
    let entry = reg
        .find(caller.uid, profile)
        .ok_or_else(|| format!("no profile named '{profile}'; run: sv2 profile add {profile}"))?;
    if entry.state != State::Active {
        return Err(format!(
            "profile '{profile}' is {}; run: sv2 profile {} {profile}",
            entry.state.as_str(),
            if entry.state == State::Adding {
                "add"
            } else {
                "remove"
            }
        ));
    }
    // There is no unsandboxed fallback: without the file, no session.
    let sb = ops::profile_file(&entry.host_user, &entry.profile);
    if !sys::exists(&sb) {
        return Err(format!("{sb} is missing; run: sv2 profile add {profile}"));
    }
    lock.keep_across_exec()?;

    let err = Command::new("/bin/launchctl")
        .arg("asuser")
        .arg(entry.id.to_string())
        .arg(HELPER_PATH)
        .arg(STAGE2)
        .arg(entry.id.to_string())
        .arg(&entry.host_user)
        .arg(&entry.profile)
        .arg("--")
        .args(argv)
        .env_clear()
        .current_dir("/")
        .exec();
    Err(format!("cannot run /bin/launchctl: {err}"))
}

pub struct Stage2Args<'a> {
    pub id: u32,
    pub host_user: &'a str,
    pub profile: &'a str,
    pub argv: &'a [OsString],
}

/// `ID HOST_USER PROFILE -- ARGV...`. Stage 2 is only reachable by root (the
/// sudoers line covers `call` alone), but it still checks what it is given.
pub fn parse_stage2_args(args: &[OsString]) -> Res<Stage2Args<'_>> {
    const BAD: &str = "stage2: bad arguments";
    if args.len() < 5 || args[3].to_str() != Some("--") {
        return Err(BAD.to_string());
    }
    let id_str = args[0].to_str().ok_or(BAD)?;
    if id_str.is_empty() || !id_str.bytes().all(|b| b.is_ascii_digit()) {
        return Err(BAD.to_string());
    }
    let id: u32 = id_str.parse().map_err(|_| BAD)?;
    let host_user = args[1].to_str().ok_or(BAD)?;
    let profile = args[2].to_str().ok_or(BAD)?;
    if !(MIN_ID..=MAX_ID).contains(&id) || !valid_host_user(host_user) || !valid_profile(profile) {
        return Err(BAD.to_string());
    }
    check_command(&args[4])?;
    Ok(Stage2Args {
        id,
        host_user,
        profile,
        argv: &args[4..],
    })
}

/// The whole environment of a session. Nothing is inherited.
pub fn session_env(
    account: &str,
    host_user: &str,
    profile: &str,
    tmpdir: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut env = vec![
        ("HOME", home_path(account)),
        ("USER", account.to_string()),
        ("LOGNAME", account.to_string()),
        ("SHELL", "/bin/zsh".to_string()),
        ("PATH", "/usr/bin:/bin:/usr/sbin:/sbin".to_string()),
        ("SV2_HOST_USER", host_user.to_string()),
        ("SV2_PROFILE", profile.to_string()),
        ("SV2_WORKSPACE", workspace_path(host_user, profile)),
    ];
    if let Some(t) = tmpdir {
        env.push(("TMPDIR", t.to_string()));
    }
    env
}

pub fn stage2(args: &[OsString]) -> Res<()> {
    let a = parse_stage2_args(args)?;
    let account = account_name(a.host_user, a.profile);
    let sb = ops::profile_file(a.host_user, a.profile);
    let workspace = workspace_path(a.host_user, a.profile);

    // From here on this process is the sandbox user; the account must have
    // the UID the registry recorded, or the name and the ID disagree.
    match sys::passwd_by_name(&account) {
        Some(pw) if pw.uid == a.id => {}
        _ => return Err(format!("account {account} does not have UID {}", a.id)),
    }
    sys::become_user(a.id, a.id, &account)?;
    // The per-user temp dir is created on demand for the UID that asks, so
    // ask only now.
    let tmpdir = sys::user_temp_dir();

    // As the sandbox user, so a workspace or home the user has replaced with
    // something odd costs root nothing.
    if std::env::set_current_dir(&workspace).is_err()
        && std::env::set_current_dir(home_path(&account)).is_err()
    {
        std::env::set_current_dir("/").map_err(|e| format!("chdir /: {e}"))?;
    }

    let mut cmd = Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-f").arg(&sb).args(a.argv).env_clear();
    for (k, v) in session_env(&account, a.host_user, a.profile, tmpdir.as_deref()) {
        cmd.env(k, v);
    }
    // Releases the registry lock inherited from stage 1, and anything else.
    sys::close_inherited_fds();
    let err = cmd.exec();
    Err(format!("cannot run /usr/bin/sandbox-exec: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn launch_args() {
        let a = os(&["work", "--", "/bin/zsh", "-l"]);
        let (name, argv) = split_launch_args(&a).unwrap();
        assert_eq!(name, "work");
        assert_eq!(argv, &os(&["/bin/zsh", "-l"])[..]);

        // Arguments are passed through untouched, including a later "--".
        let a = os(&["work", "--", "/bin/echo", "--", "a b", "$HOME", ""]);
        let (_, argv) = split_launch_args(&a).unwrap();
        assert_eq!(argv, &os(&["/bin/echo", "--", "a b", "$HOME", ""])[..]);

        for bad in [
            &[][..],
            &["work"][..],
            &["work", "--"][..],
            &["work", "/bin/zsh"][..],
            &["--", "work", "/bin/zsh"][..],
        ] {
            assert!(split_launch_args(&os(bad)).is_err(), "{bad:?}");
        }
    }

    /// `sandbox-exec -f FILE -p '(allow default)' /bin/zsh` would run the
    /// session under a profile of the caller's choosing.
    #[test]
    fn command_cannot_be_a_sandbox_exec_option() {
        for cmd in ["-p", "-f", "-D", "--", "zsh", "./zsh", ""] {
            let a = os(&["work", "--", cmd, "(version 1)(allow default)", "/bin/zsh"]);
            assert!(split_launch_args(&a).is_err(), "stage 1 accepted {cmd:?}");
            let a = os(&["600", "pat", "work", "--", cmd, "/bin/zsh"]);
            assert!(parse_stage2_args(&a).is_err(), "stage 2 accepted {cmd:?}");
        }
    }

    #[test]
    fn stage2_args() {
        let a = os(&["600", "pat", "work", "--", "/bin/zsh"]);
        let p = parse_stage2_args(&a).unwrap();
        assert_eq!((p.id, p.host_user, p.profile), (600, "pat", "work"));
        assert_eq!(p.argv, &os(&["/bin/zsh"])[..]);

        for bad in [
            &["0", "pat", "work", "--", "/bin/zsh"][..],
            &["501", "pat", "work", "--", "/bin/zsh"][..],
            &["+600", "pat", "work", "--", "/bin/zsh"][..],
            &["600", "../x", "work", "--", "/bin/zsh"][..],
            &["600", "pat", "a-b", "--", "/bin/zsh"][..],
            &["600", "pat", "work", "/bin/zsh", "x"][..],
            &["600", "pat", "work", "--"][..],
        ] {
            assert!(parse_stage2_args(&os(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn session_environment_is_fixed() {
        let env = session_env("sv2-pat-work", "pat", "work", Some("/var/folders/x/T/"));
        let get = |k: &str| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("HOME"), Some("/Users/sv2-pat-work"));
        assert_eq!(get("USER"), Some("sv2-pat-work"));
        assert_eq!(get("PATH"), Some("/usr/bin:/bin:/usr/sbin:/sbin"));
        assert_eq!(get("SV2_WORKSPACE"), Some("/Users/Shared/sv2/pat/work"));
        assert_eq!(get("TMPDIR"), Some("/var/folders/x/T/"));
        assert!(session_env("a", "b", "c", None)
            .iter()
            .all(|(n, _)| *n != "TMPDIR"));
    }
}
