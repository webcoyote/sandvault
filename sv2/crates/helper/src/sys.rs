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

/// An advisory lock on a root-owned file. Released when the last descriptor
/// for it closes, including when the helper is killed.
pub struct Lock(File);

impl Lock {
    pub fn acquire(path: &str, exclusive: bool) -> Res<Lock> {
        let f = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| format!("open {path}: {e} (is sandvault2 installed? run: sv2 install)"))?;
        let op = if exclusive {
            libc::LOCK_EX
        } else {
            libc::LOCK_SH
        };
        loop {
            // SAFETY: plain system call on an open descriptor.
            if unsafe { libc::flock(f.as_raw_fd(), op) } == 0 {
                return Ok(Lock(f));
            }
            let err = io::Error::last_os_error();
            if err.kind() != io::ErrorKind::Interrupted {
                return Err(format!("lock {path}: {err}"));
            }
        }
    }

    /// Keep the lock across `exec`, so it is held until the last process that
    /// inherits the descriptor closes it.
    pub fn keep_across_exec(&self) -> Res<()> {
        // SAFETY: plain system call on an open descriptor.
        if unsafe { libc::fcntl(self.0.as_raw_fd(), libc::F_SETFD, 0) } != 0 {
            return Err(os_err("fcntl"));
        }
        Ok(())
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

/// SIGKILL every process of `uid`. A child drops to `uid` and signals -1,
/// which for a non-root caller means "every process with my UID": the kernel
/// does the matching, so there is no list-then-kill window.
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
            libc::kill(-1, libc::SIGKILL);
            libc::_exit(0);
        }
        let mut status: libc::c_int = 0;
        loop {
            if libc::waitpid(pid, &mut status, 0) == pid {
                break;
            }
            if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return Err(os_err("waitpid"));
            }
        }
        // The child may itself be killed by a racing process of the same UID
        // after it drops; that is not a failure. Exit status 1 is.
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
