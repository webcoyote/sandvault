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
use std::time::Duration;

use sv2_common::{
    home_path, valid_host_user, valid_profile, workspace_path, workspace_user_dir, ALLOWED_GROUP,
    HELPER_PATH, MODE_CALL, VERSION, WORKSPACE_ROOT,
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
/// group of that name with any other RealName: v1 would name the sandbox
/// group of a host user called `users` exactly `sandvault-users`.
const ALLOWED_GROUP_REAL_NAME: &str = "sandvault2 allowed users";

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
    println!(
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
    ds::remove_from_staff(&account)?;
    ds::add_member(&account, &account)?;

    ensure_home(entry, &account)?;
    ensure_workspace(entry, &account, host_gid)?;
    write_profile_file(entry)
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

/// Detached processes may outlive `sv2 run`, so removal kills every process
/// of the sandbox UID. The caller holds the exclusive lock, and a launch
/// holds the shared lock until its process has become the sandbox user, so
/// no session can slip in behind this.
fn kill_sandbox_processes(id: u32) -> Res<()> {
    // Tear down the account's launchd domain; fails when there is none.
    let _ = sys::run("/bin/launchctl", ["bootout", &format!("user/{id}")]);
    for _ in 0..20 {
        sys::kill_all_processes_of(id, id)?;
        sleep(Duration::from_millis(100));
        if !processes_remain(id)? {
            return Ok(());
        }
    }
    Err(format!("processes of UID {id} survived SIGKILL"))
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
        if sys::rmdir_at(&users, account)? {
            return Ok(());
        }
    }
    Err(format!(
        "{home} could not be emptied as {account}; remove its contents and run the removal again"
    ))
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
    sys::remove_file(&profile_file(&entry.host_user, &entry.profile))?;
    // The account record goes before its home: macOS refuses, even to root,
    // to remove the home directory of an account that still exists (EPERM).
    // v1 used the same order. Everything after this works from the numeric
    // ID, which the registry entry keeps until the end.
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

    println!("removed profile {} (account {account})", entry.profile);
    if workspace_kept {
        println!("kept {ws_path}: it is not empty");
        println!("adding the profile again reuses the directory and the files in it");
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
        println!(
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
        "# sandvault2 {VERSION}: written by 'sv2 install', removed by 'sv2 uninstall'.\n\
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

    let reg = if sys::exists(&registry_path()) {
        load_registry()?
    } else {
        let reg = Registry::default();
        save_registry(&reg)?;
        reg
    };
    reject_sandbox_caller(&reg, caller)?;

    let shared = sys::open_dir("/Users/Shared")?;
    sys::ensure_root_dir_at(&shared, "sv2", WORKSPACE_ROOT)?;

    ensure_allowed_group()?;
    ds::add_member(&caller.name, ALLOWED_GROUP)?;

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

    println!("sandvault2 {VERSION} installed");
    println!("{} may now create sandboxes: sv2 run shell", caller.name);
    Ok(())
}

/// Removes exactly what the registry lists, then the installed artifacts.
///
/// The exclusive lock keeps every `call` verb out while accounts go. One
/// profile that cannot be removed does not stop the others, but it does stop
/// the uninstall before the sudoers line, registry and helper are touched:
/// the machine is left installed and consistent, and `sv2 uninstall` can be
/// run again.
pub fn uninstall() -> Res<()> {
    if sys::exists(&lock_path()) {
        let _lock = lock(true)?;
        if sys::exists(&registry_path()) {
            let mut reg = load_registry()?;
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
        sys::remove_file(&format!("{SUDOERS_DIR}/{SUDOERS_NAME}"))?;
        sys::remove_file(&registry_path())?;
        if let Ok(state) = sys::open_dir(STATE_DIR) {
            let _ = sys::rmdir_at(&state, PROFILES_DIR_NAME);
        }
        sys::remove_file(&lock_path())?;
    }
    sys::remove_file(&format!("{SUDOERS_DIR}/{SUDOERS_NAME}"))?;
    if let Ok(parent) = sys::open_dir("/Library/Application Support") {
        if !sys::rmdir_at(&parent, "sandvault2")? {
            println!("kept {STATE_DIR}: it is not empty");
        }
    }

    match ds::group_real_name(ALLOWED_GROUP)? {
        Some(real) if real == ALLOWED_GROUP_REAL_NAME => ds::delete_group(ALLOWED_GROUP)?,
        Some(_) => println!("kept group {ALLOWED_GROUP}: it was not created by sv2"),
        None => {}
    }

    if let Ok(shared) = sys::open_dir("/Users/Shared") {
        // Only when it is ours; rmdir removes it only when empty.
        if let Ok(root) = sys::open_dir_at(&shared, "sv2") {
            if sys::fstat(&root)?.uid == 0 && !sys::rmdir_at(&shared, "sv2")? {
                println!("kept {WORKSPACE_ROOT}: workspaces with files remain");
            }
        }
    }

    sys::remove_file(HELPER_PATH)?;
    println!("sandvault2 uninstalled");
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
    println!("{user} may now create sandboxes");
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
    println!("{user} may no longer create or enter sandboxes; existing profiles were not removed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sudoers_covers_only_call_mode() {
        let text = sudoers_content();
        let rules: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(
            rules,
            [
                "%sandvault-users ALL=(root) NOPASSWD: /Library/PrivilegedHelperTools/sv2-helper call *",
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
        std::fs::write(&path, "%sandvault-users ALL=(root NOPASSWD /x\n").unwrap();
        let bad = validate_sudoers(path.to_str().unwrap());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(ok, Ok(()));
        assert!(bad.is_err(), "visudo accepted a broken file");
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
