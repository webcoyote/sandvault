//! sv2-helper: the only sandvault2 code that runs as root.
//!
//! Invoked through sudo as `sv2-helper MODE VERSION VERB [ARGS...]`, where
//! VERSION is `sv2_common::install_version()` as the calling `sv2` has it:
//!
//! - `call`: `profile-add NAME`, `profile-remove NAME`, `profile-list`,
//!   `launch NAME -- ARGV`. The sudoers line lets members of
//!   `sv2-users` run this mode without a password.
//! - `admin`: `install`, `uninstall`, `user-allow USER`, `user-deny USER`.
//!   No sandvault sudoers rule covers it, so sudo asks for a password and
//!   requires general sudo rights.
//!
//! It trusts nothing it is passed. The caller is `SUDO_UID` and nothing else;
//! there are no path arguments; the environment is discarded. The caller also
//! controls the working directory, resource limits and timing, and can kill
//! the helper at any point: see registry.rs for how verbs survive that.

/// Queue a line for standard output; see `sys::say`.
macro_rules! say {
    ($($arg:tt)*) => { crate::sys::say(format!($($arg)*)) };
}

mod ds;
mod launch;
mod ops;
mod registry;
mod sandbox;
mod sys;

use std::ffi::OsString;
use std::process::ExitCode;

use sv2_common::{install_version, valid_host_user, EXIT_VERSION_MISMATCH, MODE_ADMIN, MODE_CALL};

use ops::Caller;
use sys::Res;

const USAGE: &str = "sv2-helper is run by sv2; see: sv2 --help";

/// Identify the caller from `SUDO_UID`. Under sudo the real UID is 0, so
/// this is the only identity there is; sudo sets it and the caller cannot
/// override it.
fn caller_from_sudo() -> Res<Caller> {
    let raw = std::env::var("SUDO_UID").map_err(|_| "SUDO_UID is not set; run through sudo")?;
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err("SUDO_UID is not a number".to_string());
    }
    let uid: u32 = raw.parse().map_err(|_| "SUDO_UID is out of range")?;
    if uid == 0 {
        return Err("run sv2 as yourself, not as root".to_string());
    }
    let pw = sys::passwd_by_uid(uid).ok_or_else(|| format!("no account has UID {uid}"))?;
    if pw.uid != uid {
        return Err("account lookup returned a different UID".to_string());
    }
    if !valid_host_user(&pw.name) {
        return Err(format!(
            "user name '{}' has characters or a length sandvault2 does not support",
            pw.name
        ));
    }
    Ok(Caller {
        uid,
        gid: pw.gid,
        name: pw.name,
    })
}

fn one_arg<'a>(args: &'a [OsString], what: &str) -> Res<&'a str> {
    match args {
        [a] => a
            .to_str()
            .ok_or_else(|| format!("{what} is not valid UTF-8")),
        _ => Err(format!("expected exactly one argument: {what}")),
    }
}

fn no_args(args: &[OsString]) -> Res<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err("unexpected arguments".to_string())
    }
}

fn run() -> Result<ExitCode, String> {
    if sys::getuid() != 0 {
        return Err(USAGE.to_string());
    }
    sys::set_umask();
    sys::reset_signals();
    sys::ignore_terminal_stops();
    std::env::set_current_dir("/").map_err(|e| format!("chdir /: {e}"))?;

    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let mode = args.first().and_then(|a| a.to_str()).ok_or(USAGE)?;

    if mode == launch::STAGE2 {
        launch::stage2(&args[1..])?;
        return Ok(ExitCode::FAILURE);
    }
    if mode != MODE_CALL && mode != MODE_ADMIN {
        return Err(USAGE.to_string());
    }

    let version = args.get(1).and_then(|a| a.to_str()).ok_or(USAGE)?;
    // The application version is not compared: a new sv2 whose helper and
    // installed files are unchanged needs no reinstall.
    let mine = install_version();
    if version != mine {
        eprintln!(
            "sv2-helper: the installed helper ({mine}) is not the one this sv2 ships ({version}); run: sv2 install"
        );
        return Ok(ExitCode::from(EXIT_VERSION_MISMATCH));
    }
    let verb = args.get(2).and_then(|a| a.to_str()).ok_or(USAGE)?;
    let rest = &args[3..];

    let caller = caller_from_sudo()?;
    sys::set_lock_note(&format!("{} {verb}", caller.name));
    // Nothing else is read from the environment, and nothing is passed on.
    let names: Vec<OsString> = std::env::vars_os().map(|(k, _)| k).collect();
    for name in names {
        std::env::remove_var(name);
    }

    match (mode, verb) {
        (MODE_CALL, "profile-add") => ops::profile_add(&caller, one_arg(rest, "NAME")?)?,
        (MODE_CALL, "profile-remove") => ops::profile_remove(&caller, one_arg(rest, "NAME")?)?,
        (MODE_CALL, "profile-list") => {
            no_args(rest)?;
            ops::profile_list(&caller)?
        }
        (MODE_CALL, "launch") => {
            launch::stage1(&caller, rest)?;
            return Ok(ExitCode::FAILURE);
        }
        (MODE_ADMIN, "install") => {
            no_args(rest)?;
            ops::install(&caller)?
        }
        (MODE_ADMIN, "uninstall") => {
            no_args(rest)?;
            ops::uninstall()?
        }
        (MODE_ADMIN, "user-allow") => ops::user_allow(one_arg(rest, "USER")?)?,
        (MODE_ADMIN, "user-deny") => ops::user_deny(one_arg(rest, "USER")?)?,
        _ => return Err(format!("unknown verb '{verb}' for mode '{mode}'")),
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let result = run();
    // Every lock a verb took is released by now.
    sys::flush_said();
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("sv2-helper: {e}");
            ExitCode::FAILURE
        }
    }
}
