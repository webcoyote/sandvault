//! The helper's verbs, except `launch`.
//!
//! Rules that hold for everything here (docs/SANDVAULT2.md):
//!
//! - Root never opens, reads, writes, or walks anything inside a workspace
//!   or a sandbox home. Root creates those two directories and removes them
//!   when empty; everything inside is done by the sandbox user.
//! - Ownership comes from the registry, keyed by the caller's UID.
//! - Every verb is safe to kill halfway and run again.

use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use sv2_common::{
    hand_back_argv, home_path, stamped_version, valid_host_user, valid_profile, workspace_path,
    workspace_user_dir, Obelus, ALLOWED_GROUP, HELPER_PATH, MODE_CALL, SANDBOX_GROUP,
    SANDBOX_PROFILE_STAMP, SANDBOX_PROFILE_VERSION, SSHD_DROPIN_STAMP, SSHD_DROPIN_VERSION,
    SUDOERS_STAMP, SUDOERS_VERSION, WORKSPACE_ROOT,
};

use crate::registry::{Entry, Registry, State, MAX_PROFILES_PER_USER};
use crate::sys::{self, Lock, Res};
use crate::{ds, sandbox};

pub const STATE_DIR: &str = "/Library/Application Support/sandvault2";
const REGISTRY_NAME: &str = "registry";
const LOCK_NAME: &str = "lock";
const PROFILES_DIR_NAME: &str = "profiles";
const HELPER_DIR: &str = "/Library/PrivilegedHelperTools";
const HELPER_NAME: &str = "sv2-helper";
const SUDOERS_DIR: &str = "/etc/sudoers.d";
const SUDOERS_NAME: &str = "sandvault2";
/// Marks the allowed-users group as ours. `install` and `uninstall` refuse a
/// group of that name with any other RealName.
const ALLOWED_GROUP_REAL_NAME: &str = "sandvault2 allowed users";
/// Marks the group of all sandbox accounts as ours, in the same way.
const SANDBOX_GROUP_REAL_NAME: &str = "sandvault2 sandbox accounts";
const SSHD: &str = "/usr/sbin/sshd";
const SSHD_DROPIN_DIR: &str = "/etc/ssh/sshd_config.d";
/// Sorts before every other drop-in (macOS ships `100-macos.conf`): among
/// `Match` blocks the first one to set a keyword wins.
const SSHD_DROPIN_NAME: &str = "000-sandvault2.conf";

/// The host user the helper is acting for: taken from `SUDO_UID` and nothing
/// else.
pub struct Caller {
    pub uid: u32,
    pub gid: u32,
    pub name: String,
}

fn lock_path() -> String {
    format!("{STATE_DIR}/{LOCK_NAME}")
}

fn registry_path() -> String {
    format!("{STATE_DIR}/{REGISTRY_NAME}")
}

fn profiles_user_dir(host_user: &str) -> String {
    format!("{STATE_DIR}/{PROFILES_DIR_NAME}/{host_user}")
}

pub fn profile_file(host_user: &str, profile: &str) -> String {
    format!("{}/{profile}.sb", profiles_user_dir(host_user))
}

fn sshd_dropin_path() -> String {
    format!("{SSHD_DROPIN_DIR}/{SSHD_DROPIN_NAME}")
}

fn sudoers_path() -> String {
    format!("{SUDOERS_DIR}/{SUDOERS_NAME}")
}

/// A root-owned file must carry the version this helper writes. A file that
/// is missing, unreadable, or carries another number is an error ending in
/// `fix`.
fn check_stamp(path: &str, key: &str, wanted: u32, fix: &str) -> Res<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("{path} is missing; {fix}"))
        }
        Err(e) => return Err(format!("read {path}: {e}")),
    };
    match stamped_version(&text, key) {
        Some(v) if v == wanted => Ok(()),
        Some(v) => Err(format!(
            "{path} is version {v} and this helper writes version {wanted}; {fix}"
        )),
        None => Err(format!("{path} carries no version; {fix}")),
    }
}

/// The files `install` writes are the ones this helper writes. They can
/// differ when an install stopped after it replaced the helper: the sudoers
/// file gives a session its own terminal and the drop-in keeps the account
/// from logging in over SSH, so no session starts and no profile is built
/// on stale ones.
pub fn check_installed_files() -> Res<()> {
    const FIX: &str = "an admin must run: sv2 install";
    check_stamp(&sudoers_path(), SUDOERS_STAMP, SUDOERS_VERSION, FIX)?;
    check_stamp(
        &sshd_dropin_path(),
        SSHD_DROPIN_STAMP,
        SSHD_DROPIN_VERSION,
        FIX,
    )
}

/// The same for one profile's sandbox file, which `profile add` rewrites
/// without a password. There is no unsandboxed fallback: without a current
/// file, no session.
pub fn check_profile_file(host_user: &str, profile: &str) -> Res<()> {
    check_stamp(
        &profile_file(host_user, profile),
        SANDBOX_PROFILE_STAMP,
        SANDBOX_PROFILE_VERSION,
        &format!("run: sv2 profile add {profile}"),
    )
}

pub fn lock(exclusive: bool) -> Res<Lock> {
    Lock::acquire(&lock_path(), exclusive)
}

pub fn load_registry() -> Res<Registry> {
    let path = registry_path();
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("read {path}: {e} (is sandvault2 installed? run: sv2 install)"))?;
    Registry::parse(&text)
}

fn save_registry(reg: &Registry) -> Res<()> {
    let text = reg.serialize();
    // Never write a registry the next run would refuse to read.
    Registry::parse(&text).map_err(|e| format!("internal error: {e}"))?;
    sys::write_atomic(STATE_DIR, REGISTRY_NAME, text.as_bytes(), 0o600, |_| Ok(()))
}

/// A sandbox account must never drive the helper, whatever group it is in.
pub fn reject_sandbox_caller(reg: &Registry, caller: &Caller) -> Res<()> {
    if reg.is_sandbox_id(caller.uid) {
        return Err(format!("{} is a sandbox account", caller.name));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// profile-add
// ---------------------------------------------------------------------------

pub fn profile_add(caller: &Caller, profile: &str) -> Res<()> {
    if !valid_profile(profile) {
        return Err(format!(
            "invalid profile name '{profile}': use 1-16 characters from a-z and 0-9"
        ));
    }
    let _lock = lock(true)?;
    let mut reg = load_registry()?;
    reject_sandbox_caller(&reg, caller)?;
    check_installed_files()?;

    match reg.find(caller.uid, profile).map(|e| e.state) {
        Some(State::Removing) => {
            return Err(format!(
                "removal of profile '{profile}' did not finish; run: sv2 profile remove {profile}"
            ));
        }
        // `adding`: a killed run left off somewhere below. `active`: run the
        // same idempotent steps again, which repairs anything missing.
        Some(State::Adding) | Some(State::Active) => {}
        None => {
            if reg.for_user(caller.uid).count() >= MAX_PROFILES_PER_USER {
                return Err(format!(
                    "limit of {MAX_PROFILES_PER_USER} profiles per user reached"
                ));
            }
            if reg.account_taken_by_other(caller.uid, &caller.name, profile) {
                return Err(format!(
                    "the registry lists profile '{profile}' for another user named {}; an admin must remove it",
                    caller.name
                ));
            }
            let account = sv2_common::account_name(&caller.name, profile);
            if ds::user_exists(&account)? || ds::group_exists(&account)? {
                return Err(format!(
                    "an account or group named {account} exists but is not in the registry; refusing to adopt it"
                ));
            }
            let id = reg.allocate_id(sys::id_in_use)?;
            reg.entries.push(Entry {
                host_uid: caller.uid,
                host_user: caller.name.clone(),
                profile: profile.to_string(),
                id,
                state: State::Adding,
            });
            // The intent is on disk before the first system change.
            save_registry(&reg)?;
        }
    }

    let entry = reg
        .find(caller.uid, profile)
        .cloned()
        .ok_or("internal error: entry vanished")?;
    build_profile(&entry, caller.gid)?;
    reg.set_state(caller.uid, profile, State::Active);
    save_registry(&reg)?;
    say!(
        "profile {} ready: account {}, workspace {}",
        entry.profile,
        entry.account(),
        workspace_path(&entry.host_user, &entry.profile)
    );
    Ok(())
}

fn build_profile(entry: &Entry, host_gid: u32) -> Res<()> {
    let account = entry.account();
    let home = home_path(&account);

    ds::ensure_group(&account, entry.id)?;
    ds::ensure_user(&account, entry.id, entry.id, &home)?;
    // Before the account has a home it could put an authorized_keys file in.
    join_sandbox_group(&account)?;
    ds::remove_from_staff(&account)?;
    ds::add_member(&account, &account)?;

    ensure_home(entry, &account)?;
    ensure_workspace(entry, &account, host_gid)?;
    write_profile_file(entry)
}

/// Put a sandbox account in the group the sshd drop-in refuses. Only a group
/// `install` created is used: a profile is not built on a machine where the
/// SSH route out of the sandbox cannot be closed.
fn join_sandbox_group(account: &str) -> Res<()> {
    match ds::group_real_name(SANDBOX_GROUP)? {
        Some(real) if real == SANDBOX_GROUP_REAL_NAME => ds::add_member(account, SANDBOX_GROUP),
        _ => Err(format!(
            "the {SANDBOX_GROUP} group is missing or was not created by sv2; an admin must run: sv2 install"
        )),
    }
}

/// The sandbox home: `/Users` is root-owned, so nobody can swap the entry.
/// Root creates the directory and hands it over; it never looks inside.
fn ensure_home(entry: &Entry, account: &str) -> Res<()> {
    let users = sys::open_dir("/Users")?;
    let created = sys::mkdir_at(&users, account, 0o700)?;
    let home = sys::open_dir_at(&users, account)?;
    if !created {
        let st = sys::fstat(&home)?;
        if st.uid != entry.id && st.uid != 0 {
            return Err(format!(
                "/Users/{account} exists and belongs to UID {}; refusing to use it",
                st.uid
            ));
        }
    }
    sys::fchown(&home, entry.id, entry.id)?;
    sys::fchmod(&home, 0o700)
}

/// Access-control entries for the workspace. The sandbox user reaches the
/// workspace only through these; the host user needs them for what the
/// sandbox creates.
///
/// Three shapes, measured with `ls -le` at depth 0, 1 and 2:
/// - `root`: the workspace directory itself, not inherited. No `delete`,
///   `writesecurity` or `chown`: the sandbox must not remove, rename, or
///   re-permission the workspace itself.
/// - `dirs`: inherited by every directory below.
/// - `files`: inherited by every file below. Without `search`, which on a
///   file is the execute bit.
fn workspace_aces(host_user: &str, account: &str) -> Vec<String> {
    const COMMON: &str = "readattr,writeattr,readextattr,writeextattr,readsecurity";
    let root = format!("list,add_file,search,add_subdirectory,delete_child,{COMMON}");
    let dirs = format!(
        "list,add_file,search,add_subdirectory,delete_child,delete,{COMMON},writesecurity,chown,directory_inherit,only_inherit"
    );
    let files = format!(
        "read,write,append,delete,{COMMON},writesecurity,chown,file_inherit,directory_inherit,only_inherit"
    );
    vec![
        format!("user:{account} allow {root}"),
        format!("user:{account} allow {dirs}"),
        format!("user:{account} allow {files}"),
        format!("user:{host_user} allow {dirs}"),
        format!("user:{host_user} allow {files}"),
    ]
}

/// `/Users/Shared/sv2/<user>/<profile>`. `/Users/Shared` is world-writable,
/// so `sv2` and `sv2/<user>` must be root-owned and are refused otherwise;
/// below them nobody but root can create or replace a name (finding 5).
fn ensure_workspace(entry: &Entry, account: &str, host_gid: u32) -> Res<()> {
    let shared = sys::open_dir("/Users/Shared")?;
    let root = sys::ensure_root_dir_at(&shared, "sv2", WORKSPACE_ROOT)?;
    let user_dir_path = workspace_user_dir(&entry.host_user);
    let user_dir = sys::ensure_root_dir_at(&root, &entry.host_user, &user_dir_path)?;

    let path = workspace_path(&entry.host_user, &entry.profile);
    let created = sys::mkdir_at(&user_dir, &entry.profile, 0o700)?;
    let ws = sys::open_dir_at(&user_dir, &entry.profile).map_err(|e| format!("{path}: {e}"))?;
    if !created {
        // Left by an earlier `profile remove` because it was not empty.
        let st = sys::fstat(&ws)?;
        if st.uid != entry.host_uid && st.uid != 0 {
            return Err(format!(
                "{path} exists and belongs to UID {}; refusing to use it",
                st.uid
            ));
        }
    }
    // Only the directory itself is touched, never its contents. By path is
    // safe here: both parents are root-owned and were just verified, so the
    // name cannot be replaced; -h in case that ever stops being true.
    sys::run_ok("/bin/chmod", ["-h", "-N", path.as_str()])?;
    for ace in workspace_aces(&entry.host_user, account) {
        sys::run_ok("/bin/chmod", ["-h", "+a", ace.as_str(), path.as_str()])?;
    }
    sys::fchown(&ws, entry.host_uid, host_gid)?;
    sys::fchmod(&ws, 0o700)
}

fn validate_sandbox_profile(path: &str) -> Res<()> {
    let out = sys::run("/usr/bin/sandbox-exec", ["-f", path, "/usr/bin/true"])?;
    if out.status.success() {
        return Ok(());
    }
    Err(format!(
        "rendered sandbox profile does not load: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

/// Root-owned, outside the workspace and the sandbox home, world-readable
/// (the sandbox user's `sandbox-exec` reads it). A profile that does not load
/// is never renamed into place.
fn write_profile_file(entry: &Entry) -> Res<()> {
    let text = sandbox::render(&entry.host_user, &entry.profile)?;
    sys::ensure_root_dir(&format!("{STATE_DIR}/{PROFILES_DIR_NAME}"))?;
    let dir = profiles_user_dir(&entry.host_user);
    sys::ensure_root_dir(&dir)?;
    sys::write_atomic(
        &dir,
        &format!("{}.sb", entry.profile),
        text.as_bytes(),
        0o444,
        validate_sandbox_profile,
    )
}

// ---------------------------------------------------------------------------
// profile-remove
// ---------------------------------------------------------------------------

pub fn profile_remove(caller: &Caller, profile: &str) -> Res<()> {
    if !valid_profile(profile) {
        return Err(format!("invalid profile name '{profile}'"));
    }
    let _lock = lock(true)?;
    let mut reg = load_registry()?;
    reject_sandbox_caller(&reg, caller)?;
    if reg.find(caller.uid, profile).is_none() {
        return Err(format!("no profile named '{profile}'"));
    }
    remove_entry(&mut reg, caller.uid, profile)
}

fn processes_remain(uid: u32) -> Res<bool> {
    let out = sys::run("/usr/bin/pgrep", ["-u", &uid.to_string()])?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(format!(
            "pgrep failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// How long removal goes on killing an account's processes before it gives
/// up (and leaves the profile `removing`, to be finished by another run).
const KILL_WAIT: Duration = Duration::from_secs(10);

/// Detached processes may outlive `sv2 run`, so removal kills every process
/// of the sandbox UID. The caller holds the exclusive lock, and a launch
/// holds the shared lock until its process has become the sandbox user, so
/// no session can slip in behind this.
fn kill_sandbox_processes(id: u32) -> Res<()> {
    // Tear down the account's launchd domain; fails when there is none.
    let _ = sys::run("/bin/launchctl", ["bootout", &format!("user/{id}")]);
    let deadline = Instant::now() + KILL_WAIT;
    loop {
        sys::kill_all_processes_of(id, id)?;
        // A killed process takes a moment to disappear from the table.
        for _ in 0..10 {
            if !processes_remain(id)? {
                return Ok(());
            }
            sleep(Duration::from_millis(20));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "processes of UID {id} are still running after {} seconds of being killed; run the removal again. {}",
                KILL_WAIT.as_secs(),
                Obelus("unsinkable gosling")
            ));
        }
    }
}

/// Run a tool as the sandbox user, from `/`, with nothing inherited.
fn run_as_sandbox(id: u32, program: &str, args: &[&str]) -> Res<()> {
    use std::os::unix::process::CommandExt;
    // With a UID set and no groups given, std clears the supplementary
    // groups before it sets the GID and UID.
    Command::new(program)
        .args(args)
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .uid(id)
        .gid(id)
        .status()
        .map(|_| ())
        .map_err(|e| format!("cannot run {program}: {e}"))
}

/// Before the account goes, have it let the host user at what it owns in the
/// workspace. A kept workspace is the host user's own files after a remove,
/// but an entry of the sandbox account that has no ACL is closed to the host
/// user, and once the account is deleted nobody but root could open it.
///
/// Root does not walk the workspace: `find` and `chmod` run as the sandbox
/// user, and all they do is add an ACL entry for the host user to what that
/// account owns, which the account could do itself. Every process of the
/// account is dead and the caller holds the exclusive lock, so nothing puts
/// an entry back in that state afterwards. Taking ownership is `sv2`'s part,
/// as the host user, once the helper has returned.
///
/// Nothing depends on the outcome here: what stays closed (an immutable
/// flag, a deny entry) is reported by that pass of `sv2`.
fn hand_back_workspace(entry: &Entry) -> Res<()> {
    let argv = hand_back_argv(&entry.host_user, &entry.profile, entry.id);
    let args: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    run_as_sandbox(entry.id, &argv[0], &args)
}

/// Empty the sandbox home as the sandbox user, then remove the empty
/// directory as root. Root never walks the tree, so nothing planted in it
/// can redirect a root operation.
///
/// The sandbox may have left entries its own `rm` cannot remove: immutable
/// flags, mode-000 directories, deny ACLs. Each blocks the tool that would
/// clear the next, so the clearing pass is repeated; every process of the
/// account is dead, so the tree no longer changes underneath. A tree nested
/// deeper than the passes reach is left for the user, with the entry still
/// `removing`.
fn remove_home(entry: &Entry, account: &str) -> Res<()> {
    let home = home_path(account);
    let users = sys::open_dir("/Users")?;
    for _ in 0..8 {
        // None of these can remove the home itself (the sandbox user cannot
        // write /Users), so their exit status says nothing: the rmdir decides.
        run_as_sandbox(
            entry.id,
            "/usr/bin/chflags",
            &["-R", "nouchg,nouappnd", &home],
        )?;
        run_as_sandbox(entry.id, "/bin/chmod", &["-R", "-N", &home])?;
        run_as_sandbox(entry.id, "/bin/chmod", &["-R", "u+rwx", &home])?;
        run_as_sandbox(entry.id, "/bin/rm", &["-rf", "--", &home])?;
        if rmdir_home(&users, account, &home)? {
            return Ok(());
        }
    }
    Err(format!(
        "{home} could not be emptied as {account}; remove its contents and run the removal again"
    ))
}

/// How long macOS may go on refusing the rmdir after the account is gone.
const HOME_RELEASE_WAIT: Duration = Duration::from_secs(15);

/// Remove the (emptied) home. Ok(false) means it is not empty yet.
///
/// macOS refuses, even to root, to remove the home of an account that
/// exists, and goes on refusing for a moment after the account record is
/// deleted: measured, 3 of 8 removals got EPERM here and every one succeeded
/// a second later. So a refusal is waited out, for a bounded time.
fn rmdir_home(users: &std::os::fd::OwnedFd, account: &str, home: &str) -> Res<bool> {
    let deadline = Instant::now() + HOME_RELEASE_WAIT;
    loop {
        match sys::try_rmdir_at(users, account)? {
            sys::Rmdir::Removed => return Ok(true),
            sys::Rmdir::NotEmpty => return Ok(false),
            sys::Rmdir::Refused if Instant::now() < deadline => sleep(Duration::from_millis(100)),
            sys::Rmdir::Refused => {
                return Err(format!(
                    "macOS still refuses to remove {home} {} seconds after the account was deleted; run the removal again",
                    HOME_RELEASE_WAIT.as_secs()
                ))
            }
        }
    }
}

fn remove_entry(reg: &mut Registry, host_uid: u32, profile: &str) -> Res<()> {
    let entry = reg
        .find(host_uid, profile)
        .cloned()
        .ok_or("internal error: entry vanished")?;
    let account = entry.account();

    reg.set_state(host_uid, profile, State::Removing);
    save_registry(reg)?;

    kill_sandbox_processes(entry.id)?;
    hand_back_workspace(&entry)?;
    sys::remove_file(&profile_file(&entry.host_user, &entry.profile))?;
    // The account record goes before its home: macOS refuses, even to root,
    // to remove the home directory of an account that still exists (EPERM).
    // v1 used the same order. Everything after this works from the numeric
    // ID, which the registry entry keeps until the end.
    // The shared group lists members by name, and a later profile of the
    // same name gets the same account name: leave no entry behind.
    ds::purge_member(&account, SANDBOX_GROUP)?;
    ds::delete_user(&account)?;
    ds::delete_group(&account)?;
    remove_home(&entry, &account)?;
    // Emptying the home ran tools as the sandbox UID; leave nothing of it.
    kill_sandbox_processes(entry.id)?;

    // The user's files are never deleted: only an empty workspace goes.
    let ws_path = workspace_path(&entry.host_user, &entry.profile);
    let user_dir_path = workspace_user_dir(&entry.host_user);
    let mut workspace_kept = false;
    if sys::exists(&user_dir_path) {
        let user_dir = sys::open_dir(&user_dir_path)?;
        workspace_kept = !sys::rmdir_at(&user_dir, &entry.profile)?;
    }

    reg.remove(host_uid, profile);
    save_registry(reg)?;

    // Tidy the per-user directories once the user's last profile is gone.
    // Both calls only ever remove an empty directory.
    if reg.for_user(host_uid).next().is_none() {
        if let Ok(root) = sys::open_dir(WORKSPACE_ROOT) {
            let _ = sys::rmdir_at(&root, &entry.host_user);
        }
        if let Ok(profiles) = sys::open_dir(&format!("{STATE_DIR}/{PROFILES_DIR_NAME}")) {
            let _ = sys::rmdir_at(&profiles, &entry.host_user);
        }
    }

    say!("removed profile {} (account {account})", entry.profile);
    if workspace_kept {
        say!("kept {ws_path}: it is not empty");
        say!("adding the profile again reuses the directory and the files in it");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// profile-list
// ---------------------------------------------------------------------------

/// One line per profile of the caller:
/// `profile <name> <state> <account> <workspace>`. No field contains a space.
pub fn profile_list(caller: &Caller) -> Res<()> {
    let _lock = lock(false)?;
    let reg = load_registry()?;
    reject_sandbox_caller(&reg, caller)?;
    for e in reg.for_user(caller.uid) {
        say!(
            "profile {} {} {} {}",
            e.profile,
            e.state.as_str(),
            e.account(),
            workspace_path(&e.host_user, &e.profile)
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// install / uninstall / user-allow / user-deny (admin mode only)
// ---------------------------------------------------------------------------

fn sudoers_content() -> String {
    format!(
        "# {SUDOERS_STAMP}: {SUDOERS_VERSION}\n\
         # sandvault2: written by 'sv2 install', removed by 'sv2 uninstall'.\n\
         # Members of {ALLOWED_GROUP} may create, list, remove and enter their own\n\
         # sandboxes without a password. Only the helper's '{MODE_CALL}' mode is covered;\n\
         # install, uninstall and user allow/deny fall through to ordinary sudo rules.\n\
         %{ALLOWED_GROUP} ALL=(root) NOPASSWD: {HELPER_PATH} {MODE_CALL} *\n\
         # A session gets a terminal of its own. On the caller's terminal it could\n\
         # queue keystrokes (TIOCSTI) for the caller's shell to run afterwards.\n\
         Defaults!{HELPER_PATH} use_pty\n"
    )
}

fn validate_sudoers(path: &str) -> Res<()> {
    let out = sys::run("/usr/sbin/visudo", ["-c", "-f", path])?;
    if out.status.success() {
        return Ok(());
    }
    Err(format!(
        "generated sudoers file is not valid: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

/// The file that keeps sandbox accounts from logging in over SSH.
///
/// A sandbox account's `authorized_keys` is in its own writable home, so with
/// Remote Login on it could connect to itself and get a shell that is not
/// under its sandbox profile. Two independent refusals, each measured alone
/// against a real login (OpenSSH 10.5, macOS 26.7):
///
/// - `DenyGroups`: sshd treats a member as an invalid user, whatever it
///   offers. Several `DenyGroups` lines add up, so this does not replace an
///   admin's own.
/// - the `Match` block: no way to authenticate, and `/usr/bin/false` in place
///   of any command. It ends where this file ends, so it does not swallow the
///   drop-ins read after it.
///
/// Only keywords every OpenSSH that macOS ships accepts: a file sshd rejects
/// would stop `install`.
fn sshd_dropin_content() -> String {
    format!(
        "# {SSHD_DROPIN_STAMP}: {SSHD_DROPIN_VERSION}\n\
         # sandvault2: written by 'sv2 install', removed by 'sv2 uninstall'.\n\
         # Sandbox accounts may not log in over SSH: a login would run outside the\n\
         # sandbox profile. Every sandbox account is a member of {SANDBOX_GROUP}.\n\
         DenyGroups {SANDBOX_GROUP}\n\
         Match Group {SANDBOX_GROUP}\n\
         \x20   PubkeyAuthentication no\n\
         \x20   PasswordAuthentication no\n\
         \x20   KbdInteractiveAuthentication no\n\
         \x20   HostbasedAuthentication no\n\
         \x20   AuthorizedKeysFile none\n\
         \x20   AllowAgentForwarding no\n\
         \x20   AllowTcpForwarding no\n\
         \x20   AllowStreamLocalForwarding no\n\
         \x20   X11Forwarding no\n\
         \x20   PermitTunnel no\n\
         \x20   PermitTTY no\n\
         \x20   ForceCommand /usr/bin/false\n"
    )
}

/// `sshd -G` parses a configuration and prints it. Unlike `-t` it needs no
/// host keys, which a Mac that never had Remote Login on does not have.
/// `config` None means sshd's own `/etc/ssh/sshd_config`.
fn sshd_parse(config: Option<&str>) -> Res<String> {
    let mut args = vec!["-G"];
    if let Some(path) = config {
        args.extend(["-f", path]);
    }
    let out = sys::run(SSHD, args)?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
}

fn validate_sshd_dropin(path: &str) -> Res<()> {
    sshd_parse(Some(path))
        .map(|_| ())
        .map_err(|e| format!("generated sshd drop-in is not valid: {e}"))
}

/// Whether a configuration printed by `sshd -G` denies the sandbox group.
/// sshd prints one `DenyGroups` line per group; the keyword's case has
/// changed between versions.
fn denies_sandbox_group(printed: &str) -> bool {
    printed.lines().any(|line| {
        let mut words = line.split_whitespace();
        words
            .next()
            .is_some_and(|k| k.eq_ignore_ascii_case("DenyGroups"))
            && words.any(|g| g == SANDBOX_GROUP)
    })
}

/// The drop-in only counts if sshd reads it: `config` must parse as a whole
/// and must include the drop-in directory.
fn check_sshd_reads_dropin(config: Option<&str>) -> Res<()> {
    let shown = config.unwrap_or("/etc/ssh/sshd_config");
    let printed = sshd_parse(config).map_err(|e| {
        format!("sshd rejects its configuration ({shown}), so SSH logins of sandbox accounts cannot be ruled out; fix it and run the install again: {e}")
    })?;
    if denies_sandbox_group(&printed) {
        return Ok(());
    }
    Err(format!(
        "sshd does not read {}: {shown} needs the line 'Include {SSHD_DROPIN_DIR}/*' before any other setting; add it and run the install again",
        sshd_dropin_path()
    ))
}

fn install_sshd_dropin() -> Res<()> {
    let dir = sys::open_dir(SSHD_DROPIN_DIR)
        .map_err(|e| format!("{e}: this sshd has no drop-in directory, so SSH logins of sandbox accounts cannot be refused"))?;
    let st = sys::fstat(&dir)?;
    if st.uid != 0 || st.mode & 0o022 != 0 {
        return Err(format!("{SSHD_DROPIN_DIR} is not a root-only directory"));
    }
    sys::write_atomic(
        SSHD_DROPIN_DIR,
        SSHD_DROPIN_NAME,
        sshd_dropin_content().as_bytes(),
        0o644,
        validate_sshd_dropin,
    )?;
    check_sshd_reads_dropin(None)
}

/// The group's ID comes from the registry's allocator, not from
/// `dseditgroup`: an ID that a removed sandbox once had would hand every
/// sandbox the files that one left in `/tmp` with group access.
fn ensure_sandbox_group(reg: &mut Registry) -> Res<()> {
    match ds::group_real_name(SANDBOX_GROUP)? {
        None => {
            let gid = reg.allocate_id(sys::id_in_use)?;
            save_registry(reg)?;
            ds::create_group_with_id(SANDBOX_GROUP, gid, SANDBOX_GROUP_REAL_NAME)
        }
        Some(real) if real == SANDBOX_GROUP_REAL_NAME => Ok(()),
        Some(real) => Err(format!(
            "a group named {SANDBOX_GROUP} already exists ('{real}') and was not created by sv2; refusing to use it"
        )),
    }
}

fn ensure_allowed_group() -> Res<()> {
    match ds::group_real_name(ALLOWED_GROUP)? {
        None => ds::create_group_auto_id(ALLOWED_GROUP, ALLOWED_GROUP_REAL_NAME),
        Some(real) if real == ALLOWED_GROUP_REAL_NAME => Ok(()),
        Some(real) => Err(format!(
            "a group named {ALLOWED_GROUP} already exists ('{real}') and was not created by sv2; refusing to use it"
        )),
    }
}

/// Copy the running executable to the root-owned helper path. This is the
/// one place root reads a file the admin controls: the admin chose to run it
/// as root, with a password, a moment ago.
///
/// The file is found by path a second time, so whoever can write its
/// directory could swap it between sudo's exec and this read. That is not
/// defended against, because it gains them nothing: they could have swapped
/// it before sudo ran it, and been root outright. The defence is where the
/// file is installed from, not here.
fn install_helper_binary() -> Res<()> {
    let me = std::env::current_exe().map_err(|e| format!("cannot find my own executable: {e}"))?;
    if me == std::path::Path::new(HELPER_PATH) {
        return Ok(());
    }
    let bytes = std::fs::read(&me).map_err(|e| format!("read {}: {e}", me.display()))?;
    match std::fs::create_dir(HELPER_DIR) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("create {HELPER_DIR}: {e}")),
    }
    let dir = sys::open_dir(HELPER_DIR)?;
    let st = sys::fstat(&dir)?;
    if st.uid != 0 || st.mode & 0o022 != 0 {
        return Err(format!("{HELPER_DIR} is not a root-only directory"));
    }
    sys::write_atomic(HELPER_DIR, HELPER_NAME, &bytes, 0o755, |_| Ok(()))
}

pub fn install(caller: &Caller) -> Res<()> {
    install_helper_binary()?;

    sys::ensure_root_dir(STATE_DIR)?;
    sys::ensure_root_dir(&format!("{STATE_DIR}/{PROFILES_DIR_NAME}"))?;
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(lock_path())
        .map_err(|e| format!("create {}: {e}", lock_path()))?;
    let _lock = lock(true)?;

    let mut reg = if sys::exists(&registry_path()) {
        load_registry()?
    } else {
        let reg = Registry::default();
        save_registry(&reg)?;
        reg
    };
    reject_sandbox_caller(&reg, caller)?;

    let shared = sys::open_dir("/Users/Shared")?;
    sys::claim_root_dir_at(&shared, "sv2", WORKSPACE_ROOT)?;

    ensure_allowed_group()?;
    ds::add_member(&caller.name, ALLOWED_GROUP)?;

    // The SSH route out of the sandbox. Upgrade path: accounts created by a
    // version without the group join it here. (An `adding` profile joins when
    // its `profile add` is run again; it has no session until then.)
    ensure_sandbox_group(&mut reg)?;
    for e in reg.entries.iter().filter(|e| e.state == State::Active) {
        ds::add_member(&e.account(), SANDBOX_GROUP)?;
    }
    install_sshd_dropin()?;

    // Upgrade path for the base sandbox profile: re-render every profile the
    // registry lists.
    for e in reg.entries.iter().filter(|e| e.state == State::Active) {
        write_profile_file(e)?;
    }

    // Last: the sudoers line makes the rest reachable.
    sys::write_atomic(
        SUDOERS_DIR,
        SUDOERS_NAME,
        sudoers_content().as_bytes(),
        0o440,
        validate_sudoers,
    )?;

    // The artifact versions are not shown: `sv2 --version` has them.
    say!("sandvault2 {} installed", sv2_common::VERSION);
    say!("{} may now create sandboxes: sv2 run shell", caller.name);
    Ok(())
}

/// Refuses while any profile exists, unless `all_profiles` is set.
///
/// Removes exactly what the registry lists, then the installed artifacts,
/// except the registry itself. With every profile gone it holds only
/// `next-id`, which is what keeps an account ID from being given out twice:
/// a sandbox created after a reinstall must not get the ID of an earlier
/// one, and with it the files that one left in `/tmp`.
///
/// The exclusive lock keeps every `call` verb out while accounts go. One
/// profile that cannot be removed does not stop the others, but it does stop
/// the uninstall before the sudoers line, registry and helper are touched:
/// the machine is left installed and consistent, and `sv2 uninstall` can be
/// run again.
/// Why `uninstall` will not go ahead: the profiles that exist, each with
/// the user it belongs to. A profile is a sandbox home with whatever the
/// agents kept in it (logins, installed programs), and none of that comes
/// back. So it goes only when asked for by name: by each user's own
/// `profile remove`, or by `uninstall --all-profiles`.
fn profiles_in_the_way(reg: &Registry) -> Option<String> {
    if reg.entries.is_empty() {
        return None;
    }
    let mut lines: Vec<String> = reg
        .entries
        .iter()
        .map(|e| format!("  {}: {} (account {})", e.host_user, e.profile, e.account()))
        .collect();
    lines.sort();
    Some(format!(
        "not uninstalled: these sandbox profiles exist, and uninstalling deletes their accounts and sandbox homes for good:\n{}\nRemove them first (each user: sv2 profile remove NAME), or remove everything with: sv2 uninstall --all-profiles",
        lines.join("\n")
    ))
}

pub fn uninstall(all_profiles: bool) -> Res<()> {
    if sys::exists(&lock_path()) {
        let _lock = lock(true)?;
        if sys::exists(&registry_path()) {
            let mut reg = load_registry()?;
            if !all_profiles {
                if let Some(refusal) = profiles_in_the_way(&reg) {
                    return Err(refusal);
                }
            }
            let all: Vec<(u32, String)> = reg
                .entries
                .iter()
                .map(|e| (e.host_uid, e.profile.clone()))
                .collect();
            let mut failures = Vec::new();
            for (host_uid, profile) in all {
                if let Err(e) = remove_entry(&mut reg, host_uid, &profile) {
                    failures.push(format!("profile {profile} of UID {host_uid}: {e}"));
                }
            }
            if !failures.is_empty() {
                return Err(format!(
                    "not uninstalled; these profiles could not be removed:\n  {}",
                    failures.join("\n  ")
                ));
            }
        }
        sys::remove_file(&sudoers_path())?;
        if let Ok(state) = sys::open_dir(STATE_DIR) {
            let _ = sys::rmdir_at(&state, PROFILES_DIR_NAME);
        }
        sys::remove_file(&lock_path())?;
    }
    sys::remove_file(&sudoers_path())?;
    // Every sandbox account is gone, so there is nobody left to refuse.
    sys::remove_file(&sshd_dropin_path())?;
    match ds::group_real_name(SANDBOX_GROUP)? {
        Some(real) if real == SANDBOX_GROUP_REAL_NAME => ds::delete_group(SANDBOX_GROUP)?,
        Some(_) => say!("kept group {SANDBOX_GROUP}: it was not created by sv2"),
        None => {}
    }
    // The registry stays, and nothing is said about it: it is sandvault's
    // own bookkeeping, not something the user has to look after.
    if !sys::exists(&registry_path()) {
        if let Ok(parent) = sys::open_dir("/Library/Application Support") {
            if !sys::rmdir_at(&parent, "sandvault2")? {
                say!("kept {STATE_DIR}: it is not empty");
            }
        }
    }

    match ds::group_real_name(ALLOWED_GROUP)? {
        Some(real) if real == ALLOWED_GROUP_REAL_NAME => ds::delete_group(ALLOWED_GROUP)?,
        Some(_) => say!("kept group {ALLOWED_GROUP}: it was not created by sv2"),
        None => {}
    }

    if let Ok(shared) = sys::open_dir("/Users/Shared") {
        // Only when it is ours; rmdir removes it only when empty.
        if let Ok(root) = sys::open_dir_at(&shared, "sv2") {
            if sys::fstat(&root)?.uid == 0 && !sys::rmdir_at(&shared, "sv2")? {
                say!("kept {WORKSPACE_ROOT}: workspaces with files remain");
            }
        }
    }

    sys::remove_file(HELPER_PATH)?;
    say!("sandvault2 uninstalled");
    Ok(())
}

fn check_allowable_user(user: &str) -> Res<sys::Passwd> {
    if !valid_host_user(user) {
        return Err(format!("invalid user name '{user}'"));
    }
    let pw = sys::passwd_by_name(user).ok_or_else(|| format!("no such user: {user}"))?;
    if pw.uid == 0 {
        return Err("root cannot be a sandvault user".to_string());
    }
    let _lock = lock(false)?;
    if load_registry()?.is_sandbox_id(pw.uid) {
        return Err(format!("{user} is a sandbox account"));
    }
    Ok(pw)
}

pub fn user_allow(user: &str) -> Res<()> {
    check_allowable_user(user)?;
    ensure_allowed_group()?;
    ds::add_member(user, ALLOWED_GROUP)?;
    say!("{user} may now create sandboxes");
    Ok(())
}

pub fn user_deny(user: &str) -> Res<()> {
    if !valid_host_user(user) {
        return Err(format!("invalid user name '{user}'"));
    }
    match ds::group_real_name(ALLOWED_GROUP)? {
        Some(real) if real == ALLOWED_GROUP_REAL_NAME => {}
        _ => return Err("sandvault2 is not installed".to_string()),
    }
    ds::remove_member(user, ALLOWED_GROUP)?;
    say!("{user} may no longer create or enter sandboxes; existing profiles were not removed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each generated file is pinned to its version number. When this fails,
    /// the text changed: bump the file's version in sv2_common (installed
    /// machines are then told to run `sv2 install`) and record the new
    /// fingerprint here.
    #[test]
    fn generated_files_are_pinned_to_their_versions() {
        let now = [
            ("sudoers", SUDOERS_VERSION, sudoers_content()),
            ("sshd drop-in", SSHD_DROPIN_VERSION, sshd_dropin_content()),
            (
                "sandbox profile",
                SANDBOX_PROFILE_VERSION,
                sandbox::render("pat", "work").unwrap(),
            ),
        ]
        .map(|(name, version, text)| (name, version, sys::fingerprint(&text)));
        let pinned = [
            ("sudoers", 2, 2_912_536_391_597_273_097u64),
            ("sshd drop-in", 1, 1_053_182_674_171_491_837),
            ("sandbox profile", 6, 14_511_806_113_650_886_307),
        ];
        assert_eq!(
            now, pinned,
            "a generated file's text changed: bump its version and update its pin"
        );
    }

    #[test]
    fn generated_files_carry_their_versions() {
        assert_eq!(
            stamped_version(&sudoers_content(), SUDOERS_STAMP),
            Some(SUDOERS_VERSION)
        );
        assert_eq!(
            stamped_version(&sshd_dropin_content(), SSHD_DROPIN_STAMP),
            Some(SSHD_DROPIN_VERSION)
        );
    }

    #[test]
    fn a_stale_or_missing_file_is_refused() {
        let dir = std::env::temp_dir().join(format!("sv2-stamp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("file");
        let path = path.to_str().unwrap();
        let check = || check_stamp(path, SUDOERS_STAMP, 2, "do the fix");

        let missing = check().unwrap_err();
        std::fs::write(path, "# sv2-sudoers-version: 2\nrule\n").unwrap();
        let current = check();
        std::fs::write(path, "# sv2-sudoers-version: 1\nrule\n").unwrap();
        let older = check().unwrap_err();
        std::fs::write(path, "# sv2-sudoers-version: 3\nrule\n").unwrap();
        let newer = check().unwrap_err();
        // What a version before the stamps wrote.
        std::fs::write(path, "# sandvault2 0.1.1: written by 'sv2 install'\nrule\n").unwrap();
        let unstamped = check().unwrap_err();
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(current, Ok(()));
        assert!(missing.contains("is missing; do the fix"), "{missing}");
        assert!(
            older.contains("is version 1") && older.ends_with("do the fix"),
            "{older}"
        );
        assert!(newer.contains("is version 3"), "{newer}");
        assert!(
            unstamped.contains("carries no version; do the fix"),
            "{unstamped}"
        );
    }

    #[test]
    fn uninstall_names_the_profiles_in_its_way() {
        assert_eq!(profiles_in_the_way(&Registry::default()), None);
        let reg = Registry::parse(
            "sv2-registry 1\nnext-id 603\n\
             profile 502 sam default 601 active\n\
             profile 501 pat work 600 adding\n\
             profile 501 pat default 602 removing\n",
        )
        .unwrap();
        let text = profiles_in_the_way(&reg).unwrap();
        // Every profile, whatever its state, with its owner; sorted.
        let listed: Vec<&str> = text.lines().filter(|l| l.starts_with("  ")).collect();
        assert_eq!(
            listed,
            [
                "  pat: default (account sv2-pat-default)",
                "  pat: work (account sv2-pat-work)",
                "  sam: default (account sv2-sam-default)",
            ]
        );
        assert!(text.contains("sv2 uninstall --all-profiles"), "{text}");
        assert!(text.contains("sv2 profile remove NAME"), "{text}");
    }

    #[test]
    fn sudoers_covers_only_call_mode() {
        let text = sudoers_content();
        let rules: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(
            rules,
            [
                "%sv2-users ALL=(root) NOPASSWD: /Library/PrivilegedHelperTools/sv2-helper call *",
                "Defaults!/Library/PrivilegedHelperTools/sv2-helper use_pty",
            ]
        );
        assert!(text.ends_with('\n'));
    }

    /// visudo -c only parses the file, so this needs no privileges.
    #[test]
    fn sudoers_passes_visudo() {
        let path = std::env::temp_dir().join(format!("sv2-sudoers-test-{}", std::process::id()));
        std::fs::write(&path, sudoers_content()).unwrap();
        let ok = validate_sudoers(path.to_str().unwrap());
        std::fs::write(&path, "%sv2-users ALL=(root NOPASSWD /x\n").unwrap();
        let bad = validate_sudoers(path.to_str().unwrap());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(ok, Ok(()));
        assert!(bad.is_err(), "visudo accepted a broken file");
    }

    fn sshd_rules(text: &str) -> Vec<&str> {
        text.lines()
            .filter(|l| !l.starts_with('#'))
            .map(str::trim)
            .collect()
    }

    #[test]
    fn sshd_dropin_refuses_the_sandbox_group() {
        let text = sshd_dropin_content();
        assert_eq!(
            sshd_rules(&text),
            [
                "DenyGroups sv2-sandboxes",
                "Match Group sv2-sandboxes",
                "PubkeyAuthentication no",
                "PasswordAuthentication no",
                "KbdInteractiveAuthentication no",
                "HostbasedAuthentication no",
                "AuthorizedKeysFile none",
                "AllowAgentForwarding no",
                "AllowTcpForwarding no",
                "AllowStreamLocalForwarding no",
                "X11Forwarding no",
                "PermitTunnel no",
                "PermitTTY no",
                "ForceCommand /usr/bin/false",
            ]
        );
        assert!(text.ends_with('\n'));
        // Nothing may follow the Match block: it would apply to the group
        // only, and nothing global may come after the first Match line.
        let first_match = text.lines().position(|l| l.starts_with("Match")).unwrap();
        assert!(text
            .lines()
            .skip(first_match + 1)
            .all(|l| l.starts_with("    ")));
    }

    #[test]
    fn finds_the_deny_line_sshd_prints() {
        // OpenSSH 10 prints keywords as written in the manual, older ones in
        // lower case; one line per group.
        assert!(denies_sandbox_group(
            "UsePAM yes\nDenyGroups other\nDenyGroups sv2-sandboxes\n"
        ));
        assert!(denies_sandbox_group("denygroups sv2-sandboxes\n"));
        assert!(denies_sandbox_group("denygroups other sv2-sandboxes\n"));
        assert!(!denies_sandbox_group("DenyGroups sv2-sandboxes-not\n"));
        assert!(!denies_sandbox_group("DenyUsers sv2-sandboxes\n"));
        assert!(!denies_sandbox_group("AllowGroups sv2-sandboxes\n"));
        assert!(!denies_sandbox_group(""));
    }

    /// A scratch directory for the sshd tests. sshd -G only parses, so this
    /// needs no privileges and no host keys.
    struct SshdFixture(std::path::PathBuf);

    impl SshdFixture {
        fn new(tag: &str) -> SshdFixture {
            let dir = std::env::temp_dir().join(format!("sv2-sshd-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("d")).unwrap();
            SshdFixture(dir)
        }

        fn write(&self, name: &str, text: &str) -> String {
            let path = self.0.join(name);
            std::fs::write(&path, text).unwrap();
            path.to_str().unwrap().to_string()
        }

        /// A main configuration shaped like the one macOS ships.
        fn main_config(&self) -> String {
            let include = format!("Include {}/d/*\n", self.0.display());
            self.write(
                "sshd_config",
                &(include + "AuthorizedKeysFile .ssh/authorized_keys\n"),
            )
        }
    }

    impl Drop for SshdFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sshd_dropin_passes_sshd() {
        let fx = SshdFixture::new("valid");
        let good = fx.write("good", &sshd_dropin_content());
        let bad = fx.write("bad", "Match Group x\n    NoSuchKeyword yes\n");
        assert_eq!(validate_sshd_dropin(&good), Ok(()));
        assert!(
            validate_sshd_dropin(&bad).is_err(),
            "sshd accepted a broken file"
        );
    }

    /// The drop-in is read first, and macOS's own drop-in after it sets
    /// keywords that are errors inside a Match block. The configuration
    /// parses only if our Match block ends with our file.
    #[test]
    fn sshd_dropin_does_not_swallow_later_dropins() {
        let fx = SshdFixture::new("order");
        fx.write(&format!("d/{SSHD_DROPIN_NAME}"), &sshd_dropin_content());
        fx.write(
            "d/100-macos.conf",
            "UsePAM yes\nAcceptEnv LANG LC_*\nDenyGroups other\n",
        );
        let main = fx.main_config();
        assert_eq!(check_sshd_reads_dropin(Some(&main)), Ok(()));
        let printed = sshd_parse(Some(&main)).unwrap().to_lowercase();
        // Global settings stay global: only the Match block changes these.
        assert!(printed.contains("usepam yes"), "{printed}");
        assert!(printed.contains("pubkeyauthentication yes"), "{printed}");
        assert!(printed.contains("forcecommand none"), "{printed}");
        assert!(printed.contains("denygroups other"), "{printed}");
    }

    #[test]
    fn a_dropin_sshd_does_not_read_is_an_error() {
        // No drop-in in the included directory.
        let fx = SshdFixture::new("absent");
        fx.write("d/100-macos.conf", "UsePAM yes\n");
        let main = fx.main_config();
        let err = check_sshd_reads_dropin(Some(&main)).unwrap_err();
        assert!(err.contains("does not read"), "{err}");

        // A main configuration without the Include line.
        fx.write(&format!("d/{SSHD_DROPIN_NAME}"), &sshd_dropin_content());
        let no_include = fx.write("no-include", "AuthorizedKeysFile .ssh/authorized_keys\n");
        let err = check_sshd_reads_dropin(Some(&no_include)).unwrap_err();
        assert!(err.contains("does not read"), "{err}");

        // A configuration that does not parse at all.
        let broken = fx.write("broken", "NoSuchKeyword yes\n");
        let err = check_sshd_reads_dropin(Some(&broken)).unwrap_err();
        assert!(err.contains("rejects its configuration"), "{err}");
    }

    /// The temp file `write_atomic` validates sits in the drop-in directory.
    /// Its name starts with a dot, so the Include glob must not pick it up:
    /// a half-written or invalid candidate would otherwise be live.
    #[test]
    fn sshd_ignores_the_temp_file_being_validated() {
        let fx = SshdFixture::new("temp");
        fx.write(
            &format!("d/.{SSHD_DROPIN_NAME}.tmp.123"),
            "NoSuchKeyword yes\n",
        );
        let main = fx.main_config();
        assert!(sshd_parse(Some(&main)).is_ok());
    }

    #[test]
    fn workspace_root_ace_cannot_remove_or_repermission_the_workspace() {
        let aces = workspace_aces("pat", "sv2-pat-work");
        let root = &aces[0];
        assert!(root.starts_with("user:sv2-pat-work allow "));
        for inherit in ["file_inherit", "directory_inherit", "only_inherit"] {
            assert!(!root.contains(inherit));
        }
        let rights: Vec<&str> = root.rsplit(' ').next().unwrap().split(',').collect();
        for denied in ["delete", "writesecurity", "chown"] {
            assert!(!rights.contains(&denied), "root ACE grants {denied}");
        }
        // Every other entry only takes effect on children.
        for ace in &aces[1..] {
            assert!(ace.contains("only_inherit"), "{ace}");
        }
    }

    #[test]
    fn inherited_file_ace_has_no_execute() {
        for ace in workspace_aces("pat", "sv2-pat-work") {
            if ace.contains("file_inherit") {
                let rights: Vec<&str> = ace.rsplit(' ').next().unwrap().split(',').collect();
                assert!(!rights.contains(&"search"), "{ace}");
                assert!(!rights.contains(&"execute"), "{ace}");
            }
        }
    }
}
