//! Thin wrappers over the system calls and system tools the helper uses.
//!
//! Every tool is named by absolute path and run with an empty environment
//! from `/`: the caller controls the helper's environment and working
//! directory.

use std::ffi::{CStr, CString, OsStr};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread::sleep;
use std::time::{Duration, Instant};

use sv2_common::Obelus;

pub type Res<T> = Result<T, String>;

pub fn getuid() -> u32 {
    // SAFETY: getuid has no failure mode and touches no memory.
    unsafe { libc::getuid() }
}

/// The caller chooses the helper's signal dispositions. With SIGCHLD ignored
/// no child could be waited for, so every tool run would report failure.
pub fn reset_signals() {
    // SAFETY: restoring a default disposition.
    unsafe { libc::signal(libc::SIGCHLD, libc::SIG_DFL) };
}

/// The signals a terminal sends to suspend a program (^Z, and background
/// reads and writes).
const TERMINAL_STOPS: [libc::c_int; 3] = [libc::SIGTSTP, libc::SIGTTIN, libc::SIGTTOU];

/// The helper is not to be suspended from the keyboard: stopped while it
/// holds the registry lock, it would keep every other user's `sv2` waiting.
/// (SIGSTOP cannot be ignored, but only root can send it to the helper.)
pub fn ignore_terminal_stops() {
    for sig in TERMINAL_STOPS {
        // SAFETY: setting a disposition.
        unsafe { libc::signal(sig, libc::SIG_IGN) };
    }
}

/// An ignored signal stays ignored across `exec`. A session must be able to
/// suspend its own programs, so this is undone just before it starts.
pub fn restore_terminal_stops() {
    for sig in TERMINAL_STOPS {
        // SAFETY: restoring a default disposition.
        unsafe { libc::signal(sig, libc::SIG_DFL) };
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

static SAID: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Queue a line for standard output. Nothing is written while a verb runs:
/// the caller chooses what standard output is, and a pipe that is already
/// full would block the helper for as long as the caller liked, with the
/// registry lock held. `flush_said` writes the lines once the lock is gone.
pub fn say(line: String) {
    SAID.lock().unwrap_or_else(|e| e.into_inner()).push(line);
}

/// Write what `say` queued. Call with no lock held. A reader that has gone
/// away is not an error.
pub fn flush_said() {
    let lines = std::mem::take(&mut *SAID.lock().unwrap_or_else(|e| e.into_inner()));
    let mut out = io::stdout().lock();
    for line in lines {
        if writeln!(out, "{line}").is_err() {
            return;
        }
    }
    let _ = out.flush();
}

pub fn set_umask() {
    // SAFETY: umask only sets the process file mode creation mask.
    unsafe { libc::umask(0o022) };
}

fn cstring(s: &str) -> Res<CString> {
    CString::new(s).map_err(|_| "string contains a NUL byte".to_string())
}

fn os_err(what: &str) -> String {
    format!("{what}: {}", io::Error::last_os_error())
}

pub struct Passwd {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
}

/// # Safety
/// `p` must be null or a pointer returned by getpwuid/getpwnam that has not
/// been invalidated by a later call.
unsafe fn copy_passwd(p: *const libc::passwd) -> Option<Passwd> {
    if p.is_null() {
        return None;
    }
    let name = CStr::from_ptr((*p).pw_name).to_str().ok()?.to_owned();
    Some(Passwd {
        name,
        uid: (*p).pw_uid,
        gid: (*p).pw_gid,
    })
}

pub fn passwd_by_uid(uid: u32) -> Option<Passwd> {
    // SAFETY: the helper is single-threaded and the result is copied out
    // before any other passwd call.
    unsafe { copy_passwd(libc::getpwuid(uid)) }
}

pub fn passwd_by_name(name: &str) -> Option<Passwd> {
    let c = CString::new(name).ok()?;
    // SAFETY: as passwd_by_uid; `c` outlives the call.
    unsafe { copy_passwd(libc::getpwnam(c.as_ptr())) }
}

/// True when `id` is some account's UID or some group's GID.
pub fn id_in_use(id: u32) -> bool {
    // SAFETY: only the null-ness of the results is examined.
    unsafe { !libc::getpwuid(id).is_null() || !libc::getgrgid(id).is_null() }
}

/// Run a system tool and capture its output.
pub fn run<I, S>(program: &str, args: I) -> Res<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(program)
        .args(args)
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))
}

/// Run a system tool that may not come back, for at most `limit`. `None`
/// when it had not finished by then: it has been killed and reaped. After
/// `notice_after`, `notice` is called once, to say what is being waited for.
///
/// For tools that print little: the output is read after the tool exits,
/// so one that fills a pipe waits until `limit` and is killed.
pub fn run_timed<I, S>(
    program: &str,
    args: I,
    limit: Duration,
    notice_after: Duration,
    notice: &dyn Fn(),
) -> Res<Option<Output>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    let start = Instant::now();
    let mut noticed = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(e) => return Err(format!("cannot wait for {program}: {e}")),
        }
        let waited = start.elapsed();
        if waited >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        if !noticed && waited >= notice_after {
            noticed = true;
            notice();
        }
        sleep(Duration::from_millis(20));
    }
    child
        .wait_with_output()
        .map(Some)
        .map_err(|e| format!("cannot read the output of {program}: {e}"))
}

/// Run a system tool; a non-zero exit is an error carrying its stderr.
pub fn run_ok<I, S>(program: &str, args: I) -> Res<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<std::ffi::OsString> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
    let out = run(program, &args)?;
    if out.status.success() {
        return Ok(());
    }
    Err(format!(
        "{program} {} failed ({}): {}",
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" "),
        out.status,
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

// ---------------------------------------------------------------------------
// Directories, by handle
// ---------------------------------------------------------------------------

pub struct Stat {
    pub uid: u32,
    pub mode: u32,
    pub is_dir: bool,
}

pub fn fstat(fd: &OwnedFd) -> Res<Stat> {
    // SAFETY: `st` is a valid out-pointer; fstat fills it on success.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd.as_raw_fd(), &mut st) } != 0 {
        return Err(os_err("fstat"));
    }
    Ok(Stat {
        uid: st.st_uid,
        mode: u32::from(st.st_mode) & 0o7777,
        is_dir: (st.st_mode & libc::S_IFMT) == libc::S_IFDIR,
    })
}

const DIR_FLAGS: libc::c_int =
    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

/// Open a directory by absolute path without following a link at the last
/// component. Use only where every parent is root-owned.
pub fn open_dir(path: &str) -> Res<OwnedFd> {
    let c = cstring(path)?;
    // SAFETY: `c` is a valid C string for the duration of the call.
    let fd = unsafe { libc::open(c.as_ptr(), DIR_FLAGS) };
    if fd < 0 {
        return Err(os_err(&format!("open {path}")));
    }
    // SAFETY: fd is a fresh descriptor owned by nobody else.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Open `name` inside `parent` as a directory, without following a link.
pub fn open_dir_at(parent: &OwnedFd, name: &str) -> Res<OwnedFd> {
    let c = cstring(name)?;
    // SAFETY: as open_dir.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), c.as_ptr(), DIR_FLAGS) };
    if fd < 0 {
        return Err(os_err(&format!("open {name}")));
    }
    // SAFETY: fd is a fresh descriptor owned by nobody else.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// mkdir `name` inside `parent`. Returns false when it already exists.
pub fn mkdir_at(parent: &OwnedFd, name: &str, mode: u32) -> Res<bool> {
    let c = cstring(name)?;
    // SAFETY: as open_dir.
    if unsafe { libc::mkdirat(parent.as_raw_fd(), c.as_ptr(), mode as libc::mode_t) } == 0 {
        return Ok(true);
    }
    let err = io::Error::last_os_error();
    if err.kind() == io::ErrorKind::AlreadyExists {
        return Ok(false);
    }
    Err(format!("mkdir {name}: {err}"))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Rmdir {
    /// Gone, or was not there.
    Removed,
    NotEmpty,
    /// EPERM. Root gets this for the home directory of an account macOS
    /// still knows about.
    Refused,
}

/// rmdir `name` inside `parent`. Succeeds only on an empty directory and
/// never follows a link.
pub fn try_rmdir_at(parent: &OwnedFd, name: &str) -> Res<Rmdir> {
    let c = cstring(name)?;
    // SAFETY: as open_dir.
    if unsafe { libc::unlinkat(parent.as_raw_fd(), c.as_ptr(), libc::AT_REMOVEDIR) } == 0 {
        return Ok(Rmdir::Removed);
    }
    let err = io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::ENOENT) => Ok(Rmdir::Removed),
        Some(libc::ENOTEMPTY) | Some(libc::EEXIST) => Ok(Rmdir::NotEmpty),
        Some(libc::EPERM) => Ok(Rmdir::Refused),
        _ => Err(format!("rmdir {name}: {err}")),
    }
}

/// As `try_rmdir_at`, for directories nothing protects: Ok(false) means "not
/// empty", and a refusal is an error.
pub fn rmdir_at(parent: &OwnedFd, name: &str) -> Res<bool> {
    match try_rmdir_at(parent, name)? {
        Rmdir::Removed => Ok(true),
        Rmdir::NotEmpty => Ok(false),
        Rmdir::Refused => Err(format!("rmdir {name}: Operation not permitted")),
    }
}

pub fn fchown(fd: &OwnedFd, uid: u32, gid: u32) -> Res<()> {
    // SAFETY: plain system call on an owned descriptor.
    if unsafe { libc::fchown(fd.as_raw_fd(), uid, gid) } != 0 {
        return Err(os_err("fchown"));
    }
    Ok(())
}

pub fn fchmod(fd: &OwnedFd, mode: u32) -> Res<()> {
    // SAFETY: plain system call on an owned descriptor.
    if unsafe { libc::fchmod(fd.as_raw_fd(), mode as libc::mode_t) } != 0 {
        return Err(os_err("fchmod"));
    }
    Ok(())
}

/// Create (or accept) a root-owned directory that only root can write, and
/// return a handle to it. An existing entry that is anything else is an
/// error: the parent may be world-writable (`/Users/Shared`), so somebody
/// else may have created the name first.
pub fn ensure_root_dir_at(parent: &OwnedFd, name: &str, shown: &str) -> Res<OwnedFd> {
    mkdir_at(parent, name, 0o755)?;
    let fd = open_dir_at(parent, name).map_err(|e| format!("{shown}: {e}"))?;
    let st = fstat(&fd)?;
    if !st.is_dir || st.uid != 0 {
        return Err(format!(
            "{shown} exists and is not a root-owned directory; refusing to use it"
        ));
    }
    // Ours (root-owned): set the group and mode we expect.
    fchown(&fd, 0, 0)?;
    fchmod(&fd, 0o755)?;
    Ok(fd)
}

/// As `ensure_root_dir_at`, for a name in a directory anybody can write
/// (`/Users/Shared`), where another user may have put something first and
/// so kept `install` from ever succeeding. What holds nobody's data is
/// replaced: an empty directory, or a symbolic link. Anything else is left
/// alone and reported, with its owner.
///
/// Nothing here follows a link or opens what it removes: `unlinkat` takes
/// the name in `parent`, and removes a directory only when it is empty.
pub fn claim_root_dir_at(parent: &OwnedFd, name: &str, shown: &str) -> Res<OwnedFd> {
    make_way_at(parent, name, shown, |uid| uid == 0)?;
    ensure_root_dir_at(parent, name, shown)
}

/// The part of `claim_root_dir_at` that needs no privilege: afterwards
/// `name` is a directory that this process made or that `ours` accepts the
/// owner of.
fn make_way_at(parent: &OwnedFd, name: &str, shown: &str, ours: impl Fn(u32) -> bool) -> Res<()> {
    let c = cstring(name)?;
    // The other user can put the name back between the removal and the
    // mkdir, so try a few times; they cannot win every time by accident.
    for _ in 0..5 {
        if mkdir_at(parent, name, 0o755)? {
            return Ok(());
        }
        // SAFETY: `st` is a valid out-pointer; `c` is a valid C string.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let found = unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                c.as_ptr(),
                &mut st,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } == 0;
        if !found {
            continue; // Gone again; mkdir once more.
        }
        let kind = st.st_mode & libc::S_IFMT;
        if kind == libc::S_IFDIR && ours(st.st_uid) {
            return Ok(());
        }
        let flag = match kind {
            libc::S_IFDIR => libc::AT_REMOVEDIR,
            libc::S_IFLNK => 0,
            _ => return Err(not_ours(shown, st.st_uid, "is not a directory")),
        };
        // SAFETY: as open_dir.
        if unsafe { libc::unlinkat(parent.as_raw_fd(), c.as_ptr(), flag) } != 0 {
            let err = io::Error::last_os_error();
            match err.raw_os_error() {
                Some(libc::ENOENT) => {}
                Some(libc::ENOTEMPTY) | Some(libc::EEXIST) => {
                    return Err(not_ours(shown, st.st_uid, "has files in it"))
                }
                _ => return Err(format!("remove {shown}: {err}")),
            }
        }
    }
    Err(format!(
        "{shown} keeps being created by somebody else; look at who owns it. {}",
        Obelus("stubborn limpet")
    ))
}

fn not_ours(shown: &str, uid: u32, why: &str) -> String {
    let owner = passwd_by_uid(uid).map_or_else(|| format!("UID {uid}"), |pw| pw.name);
    format!(
        "{shown} was made by {owner}, not by sandvault2, and {why}. sandvault2 needs that name: move it away (sudo mv {shown} {shown}.moved) and run the install again. {}",
        Obelus("squatting heron")
    )
}

/// `mkdir -p` for a path whose parents are all root-owned system
/// directories, ending in a root-owned 0755 directory.
pub fn ensure_root_dir(path: &str) -> Res<OwnedFd> {
    let (parent, name) = path
        .rsplit_once('/')
        .filter(|(p, n)| !p.is_empty() && !n.is_empty())
        .ok_or_else(|| format!("bad directory path {path}"))?;
    let parent_fd = open_dir(parent)?;
    ensure_root_dir_at(&parent_fd, name, path)
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

/// Write `dir/name` atomically: a fresh temp file beside the target, fsync,
/// `validate`, then rename. `dir` must be root-owned and writable only by
/// root. The caller holds the registry lock, so the PID makes the temp name
/// unique; a leftover from a killed run with a recycled PID is removed first.
/// The temp name starts with a dot and contains dots, so sudo ignores it in
/// `/etc/sudoers.d`.
pub fn write_atomic(
    dir: &str,
    name: &str,
    data: &[u8],
    mode: u32,
    validate: impl FnOnce(&str) -> Res<()>,
) -> Res<()> {
    let prefix = format!(".{name}.tmp.");
    let tmp = format!("{dir}/{prefix}{}", std::process::id());
    let dst = format!("{dir}/{name}");
    // Leftovers of killed runs. Writers of one target are serialized (the
    // registry lock, or an admin running install), so none is in use.
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let result = (|| -> Res<()> {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)
            .map_err(|e| format!("create {tmp}: {e}"))?;
        f.write_all(data).map_err(|e| format!("write {tmp}: {e}"))?;
        let fd: OwnedFd = f.into();
        fchown(&fd, 0, 0)?;
        fchmod(&fd, mode)?;
        File::from(fd)
            .sync_all()
            .map_err(|e| format!("fsync {tmp}: {e}"))?;
        validate(&tmp)?;
        std::fs::rename(&tmp, &dst).map_err(|e| format!("rename {tmp} to {dst}: {e}"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Remove a file; a missing file is fine.
pub fn remove_file(path: &str) -> Res<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("remove {path}: {e}")),
    }
}

pub fn exists(path: &str) -> bool {
    Path::new(path).symlink_metadata().is_ok()
}

// ---------------------------------------------------------------------------
// Locking
// ---------------------------------------------------------------------------

/// How long a verb waits for the registry lock before it gives up and says
/// who holds it. Most holds are well under a second, and a slow
/// `profile remove` (up to 15 seconds for a home macOS will not let go of)
/// still fits. One that sits behind a privacy prompt takes minutes; whoever
/// waits behind that is told so and tries again.
pub const LOCK_WAIT: Duration = Duration::from_secs(30);
/// After this long the waiter says what it is waiting for.
const LOCK_NOTICE_AFTER: Duration = Duration::from_secs(3);

static LOCK_NOTE: OnceLock<String> = OnceLock::new();

/// What this process is doing, for another user who has to wait for it:
/// the caller's name and the verb. Written into the lock file while the
/// lock is held exclusively.
pub fn set_lock_note(note: &str) {
    let _ = LOCK_NOTE.set(note.to_string());
}

/// An advisory lock on a root-owned file. Released when the last descriptor
/// for it closes, including when the helper is killed.
pub struct Lock {
    file: File,
    exclusive: bool,
}

/// The holder a lock file names, if that process is still running: the file
/// is `<pid> <note>`, written by an exclusive holder and emptied when it
/// lets go. A holder that was killed leaves its line behind, hence the check.
fn describe_holder(text: &str) -> Option<String> {
    let line = text.lines().next()?;
    let (pid, note) = line.split_once(' ').unwrap_or((line, ""));
    if pid.is_empty() || pid.len() > 9 || !pid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let pid_n: libc::pid_t = pid.parse().ok()?;
    // SAFETY: signal 0 only asks whether the process exists.
    if pid_n <= 0 || unsafe { libc::kill(pid_n, 0) } != 0 {
        return None;
    }
    let plain = note.len() <= 80
        && note
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b" ._-".contains(&b));
    Some(if note.is_empty() || !plain {
        format!("PID {pid}")
    } else {
        format!("PID {pid}, {note}")
    })
}

impl Lock {
    pub fn acquire(path: &str, exclusive: bool) -> Res<Lock> {
        Self::acquire_within(path, exclusive, LOCK_WAIT)
    }

    /// Never waits without limit: a holder that has stopped (it can be
    /// stopped by root, or be waiting on macOS) must not stop everyone else.
    pub fn acquire_within(path: &str, exclusive: bool, wait: Duration) -> Res<Lock> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| format!("open {path}: {e} (is sandvault2 installed? run: sv2 install)"))?;
        let op = libc::LOCK_NB
            | if exclusive {
                libc::LOCK_EX
            } else {
                libc::LOCK_SH
            };
        let holder = || {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|t| describe_holder(&t))
                .map_or(String::new(), |h| format!(" ({h})"))
        };
        let start = Instant::now();
        let mut noticed = false;
        loop {
            // SAFETY: plain system call on an open descriptor.
            if unsafe { libc::flock(file.as_raw_fd(), op) } == 0 {
                break;
            }
            let err = io::Error::last_os_error();
            match err.raw_os_error() {
                Some(libc::EWOULDBLOCK) => {}
                Some(libc::EINTR) => continue,
                _ => return Err(format!("lock {path}: {err}")),
            }
            let waited = start.elapsed();
            if waited >= wait {
                return Err(format!(
                    "another sv2 command{} has not finished after {} seconds; try again when it has. {}",
                    holder(),
                    wait.as_secs(),
                    Obelus("loitering pelican")
                ));
            }
            if !noticed && waited >= LOCK_NOTICE_AFTER {
                noticed = true;
                eprintln!(
                    "sv2-helper: waiting for another sv2 command{} to finish",
                    holder()
                );
            }
            sleep(Duration::from_millis(50));
        }
        if exclusive {
            let note = LOCK_NOTE.get().map_or("", String::as_str);
            let line = format!("{} {note}\n", std::process::id());
            // For the benefit of whoever waits; the lock does not depend on it.
            let _ = file.set_len(0);
            let _ = file.write_all(line.as_bytes());
        }
        Ok(Lock { file, exclusive })
    }

    /// Keep the lock across `exec`, so it is held until the last process that
    /// inherits the descriptor closes it.
    pub fn keep_across_exec(&self) -> Res<()> {
        // SAFETY: plain system call on an open descriptor.
        if unsafe { libc::fcntl(self.file.as_raw_fd(), libc::F_SETFD, 0) } != 0 {
            return Err(os_err("fcntl"));
        }
        Ok(())
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if self.exclusive {
            // Still locked here; the file closes after this returns.
            let _ = self.file.set_len(0);
        }
    }
}

/// Close every descriptor above stderr.
pub fn close_inherited_fds() {
    // SAFETY: closing descriptors the process does not use; errors (EBADF for
    // unused numbers) are expected and ignored.
    unsafe {
        let max = libc::getdtablesize().clamp(256, 65_536);
        for fd in 3..max {
            libc::close(fd as RawFd);
        }
    }
}

// ---------------------------------------------------------------------------
// Becoming the sandbox user
// ---------------------------------------------------------------------------

/// Drop from root to `uid`/`gid` for good, with the supplementary groups of
/// `account` (or none when `account` is None). Verifies the drop, including
/// that root cannot be regained.
///
/// # Safety
/// Call only in a single-threaded process (or a freshly forked child).
unsafe fn drop_privileges(uid: u32, gid: u32, account: Option<&CStr>) -> bool {
    if uid == 0 || gid == 0 {
        return false;
    }
    let groups_ok = match account {
        Some(name) => libc::initgroups(name.as_ptr(), gid as libc::c_int) == 0,
        None => libc::setgroups(0, std::ptr::null()) == 0,
    };
    if !groups_ok || libc::setgid(gid) != 0 || libc::setuid(uid) != 0 {
        return false;
    }
    libc::getuid() == uid
        && libc::geteuid() == uid
        && libc::getgid() == gid
        && libc::getegid() == gid
        && libc::setuid(0) != 0
}

/// Become the sandbox account in this process. There is no way back.
pub fn become_user(uid: u32, gid: u32, account: &str) -> Res<()> {
    let name = cstring(account)?;
    // SAFETY: the helper is single-threaded.
    if unsafe { drop_privileges(uid, gid, Some(&name)) } {
        Ok(())
    } else {
        Err(format!("could not drop privileges to {account}"))
    }
}

/// How many times the killing child freezes the account before it kills.
/// Measured: one pass froze a storm of 400 processes.
const STOP_PASSES: u32 = 16;

/// SIGKILL every process of `uid`. A child drops to `uid` and signals -1,
/// which for a non-root caller means "every process with my UID": the kernel
/// does the matching, so there is no list-then-kill window.
///
/// One pass reaches the processes that exist when it starts. An account
/// that forks without pause has new ones by the time the pass ends, and
/// after a SIGKILL pass those have room to multiply again: measured, 20
/// rounds of SIGKILL a tenth of a second apart left a storm of 400 at 391.
/// So the child first sends SIGSTOP, several times: a stopped process cannot
/// fork, each pass leaves only what was born during it, and no process slot
/// is freed for the survivors to grow into. The SIGKILL then goes to an
/// account that is standing still. The caller repeats the whole thing until
/// no process is left.
///
/// On macOS `kill(-1, sig)` also signals the process that calls it
/// (measured; other systems leave the caller out). So the child is stopped
/// by each of its own SIGSTOP passes, and this process, which as root may
/// signal it, sets it going again. Its own SIGKILL ends it, and that is the
/// normal way for it to end.
///
/// The child is, for a moment, a process of the account, and the account
/// can signal it too. Killed early, it is simply gone and the caller tries
/// again. Stopped by the account, it is continued like after its own stop;
/// the number of times that is done is bounded, so the account cannot keep
/// this process here.
pub fn kill_all_processes_of(uid: u32, gid: u32) -> Res<()> {
    if uid == 0 || gid == 0 {
        return Err("refusing to signal UID 0".to_string());
    }
    // SAFETY: the helper is single-threaded; the child calls only
    // async-signal-safe functions and then _exit.
    unsafe {
        let pid = libc::fork();
        if pid < 0 {
            return Err(os_err("fork"));
        }
        if pid == 0 {
            // As root, kill(-1) would signal the whole system. drop_privileges
            // returns true only after verifying this process is `uid` and
            // cannot become root again.
            if !drop_privileges(uid, gid, None) {
                libc::_exit(1);
            }
            for _ in 0..STOP_PASSES {
                libc::kill(-1, libc::SIGSTOP);
            }
            libc::kill(-1, libc::SIGKILL);
            libc::_exit(0);
        }
        let mut status: libc::c_int = 0;
        let mut continued = 0;
        loop {
            if libc::waitpid(pid, &mut status, libc::WUNTRACED) == pid {
                if !libc::WIFSTOPPED(status) {
                    break;
                }
                // Root may signal the child whatever UID it has now.
                if continued < 2 * STOP_PASSES {
                    continued += 1;
                    libc::kill(pid, libc::SIGCONT);
                } else {
                    libc::kill(pid, libc::SIGKILL);
                }
                continue;
            }
            if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return Err(os_err("waitpid"));
            }
        }
        // Ending by a signal is expected. Exit status 1 is not: the child
        // could not become `uid`, and signalled nothing.
        if libc::WIFEXITED(status) && libc::WEXITSTATUS(status) != 0 {
            return Err(format!("could not drop privileges to UID {uid}"));
        }
    }
    Ok(())
}

/// The per-user temporary directory (`/var/folders/.../T/`) of the current
/// UID, created on demand by the system.
pub fn user_temp_dir() -> Option<String> {
    let mut buf = [0u8; 1024];
    // SAFETY: buf is valid for buf.len() bytes.
    let n = unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_TEMP_DIR,
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };
    if n == 0 || n > buf.len() {
        return None;
    }
    let s = CStr::from_bytes_until_nul(&buf).ok()?.to_str().ok()?;
    s.starts_with('/').then(|| s.to_owned())
}

/// FNV-1a, for the tests that pin a generated file's text to its version
/// number. Not for anything that needs a real hash.
#[cfg(test)]
pub fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timed_run_gives_up_and_says_what_it_waits_for() {
        use std::cell::Cell;
        let noticed = Cell::new(0);
        let notice = || noticed.set(noticed.get() + 1);
        let ms = Duration::from_millis;
        // Finishes in time: its output and status, and no notice.
        let out = run_timed(
            "/bin/sh",
            ["-c", "echo out; echo err >&2; exit 3"],
            ms(5000),
            ms(4000),
            &notice,
        )
        .unwrap()
        .unwrap();
        assert_eq!(out.status.code(), Some(3));
        assert_eq!(
            (out.stdout.as_slice(), out.stderr.as_slice()),
            (&b"out\n"[..], &b"err\n"[..])
        );
        assert_eq!(noticed.get(), 0);
        // Does not: killed, after one notice, well before it would end.
        let start = Instant::now();
        let out = run_timed("/bin/sleep", ["30"], ms(400), ms(100), &notice).unwrap();
        assert!(out.is_none());
        assert_eq!(noticed.get(), 1);
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sv2-sys-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_held_lock_is_waited_for_only_so_long() {
        let dir = scratch("lock");
        let path = dir.join("lock");
        std::fs::write(&path, "").unwrap();
        let path = path.to_str().unwrap();
        let brief = Duration::from_millis(200);
        let me = std::process::id();

        // An exclusive holder names itself, and both kinds of waiter give up.
        let held = Lock::acquire_within(path, true, brief).unwrap();
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .starts_with(&format!("{me} ")));
        for exclusive in [false, true] {
            let start = Instant::now();
            let err = Lock::acquire_within(path, exclusive, brief)
                .err()
                .expect("got a lock that is held");
            assert!(start.elapsed() >= brief, "gave up early");
            assert!(start.elapsed() < Duration::from_secs(5), "waited too long");
            assert!(err.contains("has not finished"), "{err}");
            assert!(err.contains(&format!("(PID {me}")), "{err}");
        }
        // Letting go takes the name away and lets the next one in.
        drop(held);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "");

        // Shared holders do not exclude each other, do exclude a writer, and
        // are not named.
        let one = Lock::acquire_within(path, false, brief).unwrap();
        let two = Lock::acquire_within(path, false, brief).unwrap();
        let err = Lock::acquire_within(path, true, brief).err().unwrap();
        assert!(
            err.contains("another sv2 command has not finished"),
            "{err}"
        );
        drop((one, two));
        drop(Lock::acquire_within(path, true, brief).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_holder_is_named_only_while_it_runs() {
        let me = std::process::id();
        assert_eq!(
            describe_holder(&format!("{me} pat profile-remove\n")),
            Some(format!("PID {me}, pat profile-remove"))
        );
        assert_eq!(
            describe_holder(&format!("{me} \n")),
            Some(format!("PID {me}"))
        );
        assert_eq!(
            describe_holder(&format!("{me}\n")),
            Some(format!("PID {me}"))
        );
        // A note is shown only when it is plain text.
        assert_eq!(
            describe_holder(&format!("{me} \x1b[31mpat\n")),
            Some(format!("PID {me}"))
        );
        // No process has this PID (above the system's largest).
        assert_eq!(describe_holder("999999999 pat profile-remove\n"), None);
        for bad in ["", "\n", "pat\n", "-1 x\n", "0 x\n", "+5 x\n", " 5 x\n"] {
            assert_eq!(describe_holder(bad), None, "{bad:?}");
        }
    }

    /// What `install` does with a name somebody else got to first. `ours`
    /// says no to every owner, so the entries this test makes stand in for
    /// another user's.
    #[test]
    fn a_name_taken_by_someone_else() {
        let dir = scratch("claim");
        let parent = open_dir(dir.to_str().unwrap()).unwrap();
        let nobody = |_: u32| false;
        let at = |name: &str| dir.join(name);
        let is_real_dir = |name: &str| {
            std::fs::symlink_metadata(at(name))
                .unwrap()
                .file_type()
                .is_dir()
        };

        // Free: made.
        make_way_at(&parent, "free", "free", nobody).unwrap();
        assert!(is_real_dir("free"));

        // An empty directory: replaced by one of ours.
        std::fs::create_dir(at("empty")).unwrap();
        let before = std::fs::metadata(at("empty")).unwrap();
        make_way_at(&parent, "empty", "empty", nobody).unwrap();
        let after = std::fs::metadata(at("empty")).unwrap();
        use std::os::unix::fs::MetadataExt;
        assert!(is_real_dir("empty"));
        assert!(
            before.ino() != after.ino() || before.ctime_nsec() != after.ctime_nsec(),
            "the directory was not replaced"
        );

        // A link: replaced, and what it pointed to is untouched.
        std::fs::create_dir(at("target")).unwrap();
        std::fs::write(at("target/file"), "x").unwrap();
        std::os::unix::fs::symlink(at("target"), at("link")).unwrap();
        make_way_at(&parent, "link", "link", nobody).unwrap();
        assert!(is_real_dir("link"));
        assert_eq!(std::fs::read_to_string(at("target/file")).unwrap(), "x");
        assert_eq!(std::fs::read_dir(at("link")).unwrap().count(), 0);

        // A directory with something in it, and a file: left alone.
        std::fs::create_dir(at("full")).unwrap();
        std::fs::write(at("full/file"), "x").unwrap();
        let err = make_way_at(&parent, "full", "/shown/full", nobody).unwrap_err();
        assert!(err.contains("has files in it"), "{err}");
        assert!(
            err.contains("sudo mv /shown/full /shown/full.moved"),
            "{err}"
        );
        assert_eq!(std::fs::read_to_string(at("full/file")).unwrap(), "x");
        std::fs::write(at("file"), "x").unwrap();
        let err = make_way_at(&parent, "file", "file", nobody).unwrap_err();
        assert!(err.contains("is not a directory"), "{err}");
        assert_eq!(std::fs::read_to_string(at("file")).unwrap(), "x");

        // One of ours is kept as it is, whatever is in it.
        make_way_at(&parent, "full", "full", |_| true).unwrap();
        assert_eq!(std::fs::read_to_string(at("full/file")).unwrap(), "x");

        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Lines are held back until asked for, in order.
    #[test]
    fn said_lines_wait_for_the_flush() {
        say("sv2-test-line".to_string());
        let queued = SAID.lock().unwrap().clone();
        assert!(queued.contains(&"sv2-test-line".to_string()));
    }
}
