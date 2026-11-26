//! `sv2 repair PATH`: give PATH, and everything below it, the access a file
//! created in the workspace gets by inheritance.
//!
//! Files moved into a workspace keep the permissions they had, and files kept
//! across `profile remove` and `profile add` name the old sandbox account.
//! Either way one of the two users cannot open them.
//!
//! This is the one place `sv2` works inside a workspace, which the sandbox
//! can write at any moment, so it is held to review-guidelines §1:
//!
//! - It runs as the host user. Nothing here is root.
//! - PATH is checked as text (no `..`, inside the caller's own workspace) and
//!   then opened one component at a time, never following a link.
//! - Every entry is opened without following links and is examined and
//!   changed through that descriptor, never by path. A name the sandbox swaps
//!   after the check is not the object that gets changed.
//! - Only regular files and directories are touched. Links are left alone,
//!   and a FIFO cannot block the walk (`O_NONBLOCK`, then the type is checked
//!   again on the descriptor).
//! - A file with more than one hard link is skipped: its other name may be
//!   outside the workspace.
//! - An entry that belongs to some other account is skipped. Entries owned by
//!   the host user, by this profile's sandbox account, or by an account that
//!   no longer exists (a removed sandbox) are repaired.
//!
//! The ACL written is the one the kernel would have inherited from the
//! workspace directory, computed from that directory's own ACL. The helper
//! writes that ACL, and the sandbox cannot change it. Mode bits and ownership
//! are left as they are.

use std::ffi::{c_char, c_int, c_void, CStr, CString, OsStr, OsString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

use sv2_common::{account_name, valid_host_user, valid_profile, WORKSPACE_ROOT};

/// Directories nested deeper than this below PATH are reported, not entered:
/// each level holds a descriptor open.
const MAX_DEPTH: usize = 64;

const ACL_HEADER: &str = "!#acl 1";
/// `ACL_TYPE_EXTENDED` from `<sys/acl.h>`.
const ACL_TYPE_EXTENDED: c_int = 0x100;

extern "C" {
    fn acl_get_fd_np(fd: c_int, acl_type: c_int) -> *mut c_void;
    fn acl_set_fd_np(fd: c_int, acl: *mut c_void, acl_type: c_int) -> c_int;
    fn acl_from_text(buf: *const c_char) -> *mut c_void;
    fn acl_to_text(acl: *mut c_void, len: *mut isize) -> *mut c_char;
    fn acl_free(obj: *mut c_void) -> c_int;
}

/// An ACL from the system library, freed on drop.
struct Acl(*mut c_void);

impl Drop for Acl {
    fn drop(&mut self) {
        // SAFETY: the pointer came from acl_from_text or acl_get_fd_np and is
        // freed exactly once.
        unsafe { acl_free(self.0) };
    }
}

impl Acl {
    fn from_text(text: &str) -> Result<Acl, String> {
        let c = CString::new(text).map_err(|_| "ACL text contains a NUL byte".to_string())?;
        // SAFETY: `c` is a valid NUL-terminated string for the call.
        let p = unsafe { acl_from_text(c.as_ptr()) };
        if p.is_null() {
            return Err(format!(
                "the system rejected an ACL: {}",
                io::Error::last_os_error()
            ));
        }
        Ok(Acl(p))
    }

    /// The ACL on an open file; `None` when it has none.
    fn of_fd(fd: RawFd) -> io::Result<Option<Acl>> {
        // SAFETY: `fd` is an open descriptor owned by the caller.
        let p = unsafe { acl_get_fd_np(fd, ACL_TYPE_EXTENDED) };
        if !p.is_null() {
            return Ok(Some(Acl(p)));
        }
        let e = io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::ENOENT) {
            Ok(None)
        } else {
            Err(e)
        }
    }

    fn text(&self) -> Result<String, String> {
        // SAFETY: self.0 is a live ACL. The returned string is copied and
        // then freed with acl_free, as the API requires.
        unsafe {
            let p = acl_to_text(self.0, std::ptr::null_mut());
            if p.is_null() {
                return Err(format!(
                    "cannot read an ACL: {}",
                    io::Error::last_os_error()
                ));
            }
            let s = CStr::from_ptr(p).to_string_lossy().into_owned();
            acl_free(p.cast());
            Ok(s)
        }
    }

    fn set_on(&self, fd: RawFd) -> io::Result<()> {
        // SAFETY: `fd` is an open descriptor and self.0 a live ACL.
        if unsafe { acl_set_fd_np(fd, self.0, ACL_TYPE_EXTENDED) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

/// The ACL a new child inherits from a directory whose ACL is `parent`, in
/// the text form of `acl_to_text`:
///
/// ```text
/// !#acl 1
/// user:<uuid>:<name>:<id>:allow,file_inherit,directory_inherit:read,write
/// ```
///
/// This follows the kernel. A file takes the entries marked `file_inherit`,
/// a directory those marked `directory_inherit`. The copy is marked
/// `inherited`; a directory keeps the inherit flags so its own children
/// inherit in turn (unless `limit_inherit`), and loses `only_inherit` so the
/// entry applies to it.
///
/// The result has only the header line when nothing is inheritable.
pub fn inherited_acl_text(parent: &str, is_dir: bool) -> Result<String, String> {
    const KNOWN_FLAGS: &[&str] = &[
        "inherited",
        "file_inherit",
        "directory_inherit",
        "limit_inherit",
        "only_inherit",
    ];
    let mut lines = parent.lines();
    if lines.next() != Some(ACL_HEADER) {
        return Err("unexpected ACL format".to_string());
    }
    let mut out = format!("{ACL_HEADER}\n");
    for line in lines.filter(|l| !l.is_empty()) {
        let bad = || format!("unexpected ACL entry: {line}");
        let fields: Vec<&str> = line.split(':').collect();
        let [kind, uuid, name, id, tag_flags, rights] = fields.as_slice() else {
            return Err(bad());
        };
        let mut tf = tag_flags.split(',');
        let tag = tf.next().filter(|t| matches!(*t, "allow" | "deny"));
        let tag = tag.ok_or_else(bad)?;
        let flags: Vec<&str> = tf.collect();
        if flags.iter().any(|f| !KNOWN_FLAGS.contains(f)) {
            return Err(bad());
        }
        let wanted = if is_dir {
            "directory_inherit"
        } else {
            "file_inherit"
        };
        if !flags.contains(&wanted) {
            continue;
        }
        let mut new_flags = vec![tag, "inherited"];
        if is_dir && !flags.contains(&"limit_inherit") {
            for keep in ["file_inherit", "directory_inherit"] {
                if flags.contains(&keep) {
                    new_flags.push(keep);
                }
            }
        }
        out.push_str(&format!(
            "{kind}:{uuid}:{name}:{id}:{}:{rights}\n",
            new_flags.join(",")
        ));
    }
    Ok(out)
}

/// What `repair PATH` names: a profile of the caller's, and a path below
/// that profile's workspace (empty for the workspace itself).
#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    pub profile: String,
    pub rel: Vec<OsString>,
}

/// Decide, from the text of the path alone, whether it is inside one of the
/// caller's workspaces. Nothing is opened and no link is resolved here; the
/// components are opened one at a time, without following links, later.
pub fn resolve(arg: &OsStr, cwd: Option<&Path>, host_user: &str) -> Result<Target, String> {
    let arg_path = Path::new(arg);
    let abs: PathBuf = if arg_path.is_absolute() {
        arg_path.to_path_buf()
    } else {
        cwd.ok_or("cannot find the current directory")?
            .join(arg_path)
    };
    let mut parts: Vec<&OsStr> = Vec::new();
    for c in abs.components() {
        match c {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(p) => parts.push(p),
            Component::ParentDir => {
                return Err("the path must not contain '..'".to_string());
            }
            Component::Prefix(_) => return Err("unsupported path".to_string()),
        }
    }
    let outside = || {
        format!(
            "{} is not inside one of your workspaces ({WORKSPACE_ROOT}/{host_user}/PROFILE)",
            abs.display()
        )
    };
    let root_parts: Vec<&OsStr> = Path::new(WORKSPACE_ROOT)
        .components()
        .filter_map(|c| match c {
            Component::Normal(p) => Some(p),
            _ => None,
        })
        .collect();
    let n = root_parts.len();
    if parts.len() < n + 2 || parts[..n] != root_parts[..] || parts[n] != OsStr::new(host_user) {
        return Err(outside());
    }
    let profile = parts[n + 1]
        .to_str()
        .filter(|p| valid_profile(p))
        .ok_or_else(outside)?;
    Ok(Target {
        profile: profile.to_string(),
        rel: parts[n + 2..].iter().map(|p| (*p).to_owned()).collect(),
    })
}

#[derive(Debug, Default)]
pub struct Stats {
    pub repaired: u64,
    pub unchanged: u64,
    /// Path and reason for everything that was not repaired.
    pub skipped: Vec<(PathBuf, String)>,
}

struct Walk {
    host_uid: u32,
    sandbox_uid: Option<u32>,
    dev: libc::dev_t,
    file_acl: Acl,
    file_text: String,
    dir_acl: Acl,
    dir_text: String,
    stats: Stats,
}

fn cstr(name: &OsStr) -> Result<CString, String> {
    CString::new(name.as_bytes()).map_err(|_| "name contains a NUL byte".to_string())
}

fn fstat(fd: RawFd) -> io::Result<libc::stat> {
    // SAFETY: `st` is a valid out-pointer; it is read only on success.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        if libc::fstat(fd, &mut st) == 0 {
            Ok(st)
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

fn lstat_at(dir: RawFd, name: &CStr) -> io::Result<libc::stat> {
    // SAFETY: `name` is NUL-terminated and `st` a valid out-pointer.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        if libc::fstatat(dir, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) == 0 {
            Ok(st)
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

/// Open `name` in `dir` without following a link in the last component.
/// `O_NONBLOCK` so that a FIFO planted under the name returns at once.
fn open_at(dir: RawFd, name: &CStr, directory: bool) -> io::Result<OwnedFd> {
    let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
    if directory {
        flags |= libc::O_DIRECTORY;
    }
    // SAFETY: `name` is NUL-terminated; a non-negative result is a new
    // descriptor that nothing else owns.
    unsafe {
        let fd = libc::openat(dir, name.as_ptr(), flags);
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(OwnedFd::from_raw_fd(fd))
        }
    }
}

fn open_dir_path(path: &str) -> io::Result<OwnedFd> {
    let c = CString::new(path).map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;
    open_at(libc::AT_FDCWD, &c, true)
}

/// The names in an open directory, without `.` and `..`.
fn list(dir: RawFd) -> io::Result<Vec<CString>> {
    // SAFETY: fdopendir takes ownership of the duplicate, which closedir
    // releases. Each d_name is a NUL-terminated string valid until the next
    // readdir, and is copied before that.
    unsafe {
        let dup = libc::fcntl(dir, libc::F_DUPFD_CLOEXEC, 0);
        if dup < 0 {
            return Err(io::Error::last_os_error());
        }
        let d = libc::fdopendir(dup);
        if d.is_null() {
            let e = io::Error::last_os_error();
            libc::close(dup);
            return Err(e);
        }
        // The duplicate shares its offset with `dir`.
        libc::rewinddir(d);
        let mut names = Vec::new();
        loop {
            let ent = libc::readdir(d);
            if ent.is_null() {
                break;
            }
            let name = CStr::from_ptr((*ent).d_name.as_ptr());
            if name.to_bytes() != b"." && name.to_bytes() != b".." {
                names.push(name.to_owned());
            }
        }
        libc::closedir(d);
        Ok(names)
    }
}

fn account_exists(uid: u32) -> bool {
    // SAFETY: getpwuid returns NULL or a pointer to static storage that is
    // not read here.
    unsafe { !libc::getpwuid(uid).is_null() }
}

fn uid_of(account: &str) -> Option<u32> {
    let c = CString::new(account).ok()?;
    // SAFETY: `c` is NUL-terminated; the result is read before any other
    // passwd call.
    unsafe {
        let pw = libc::getpwnam(c.as_ptr());
        if pw.is_null() {
            None
        } else {
            Some((*pw).pw_uid)
        }
    }
}

impl Walk {
    fn skip(&mut self, path: &Path, why: impl Into<String>) {
        self.stats.skipped.push((path.to_path_buf(), why.into()));
    }

    /// Repair the entry `name` in the open directory `dir`, and what is
    /// below it. `path` is only for messages.
    fn entry(&mut self, dir: RawFd, name: &CStr, path: &Path, depth: usize) {
        let st = match lstat_at(dir, name) {
            Ok(st) => st,
            Err(e) => return self.skip(path, e.to_string()),
        };
        let is_dir = match st.st_mode & libc::S_IFMT {
            // Access to a link is decided by what it points to.
            libc::S_IFLNK => return,
            libc::S_IFDIR => true,
            libc::S_IFREG => false,
            _ => return self.skip(path, "not a regular file or a directory"),
        };
        let fd = match open_at(dir, name, is_dir) {
            Ok(fd) => fd,
            Err(e) => return self.skip(path, format!("cannot open it: {e}")),
        };
        // From here on, only the descriptor: the name may already be
        // something else.
        let st = match fstat(fd.as_raw_fd()) {
            Ok(st) => st,
            Err(e) => return self.skip(path, e.to_string()),
        };
        let kind = st.st_mode & libc::S_IFMT;
        if kind != (if is_dir { libc::S_IFDIR } else { libc::S_IFREG }) {
            return self.skip(path, "it changed while being examined");
        }
        if st.st_dev != self.dev {
            return self.skip(path, "it is on another volume");
        }
        if !is_dir && st.st_nlink > 1 {
            return self.skip(
                path,
                "it has more than one hard link, so it may also exist outside the workspace",
            );
        }
        let owner = st.st_uid;
        if owner != self.host_uid && Some(owner) != self.sandbox_uid && account_exists(owner) {
            return self.skip(path, format!("it belongs to another account (UID {owner})"));
        }

        let current = match Acl::of_fd(fd.as_raw_fd()) {
            Ok(Some(acl)) => match acl.text() {
                Ok(t) => t,
                Err(e) => return self.skip(path, e),
            },
            Ok(None) => String::new(),
            Err(e) => return self.skip(path, format!("cannot read its ACL: {e}")),
        };
        let (acl, text) = if is_dir {
            (&self.dir_acl, &self.dir_text)
        } else {
            (&self.file_acl, &self.file_text)
        };
        if current == *text {
            self.stats.unchanged += 1;
        } else if let Err(e) = acl.set_on(fd.as_raw_fd()) {
            self.skip(path, format!("cannot change its ACL: {e}"));
        } else {
            self.stats.repaired += 1;
        }

        if is_dir {
            if depth >= MAX_DEPTH {
                return self.skip(
                    path,
                    "its contents are nested too deeply; run sv2 repair on it directly",
                );
            }
            self.children(fd.as_raw_fd(), path, depth + 1);
        }
    }

    fn children(&mut self, dir: RawFd, path: &Path, depth: usize) {
        let names = match list(dir) {
            Ok(n) => n,
            Err(e) => return self.skip(path, format!("cannot list it: {e}")),
        };
        for name in names {
            let child = path.join(OsStr::from_bytes(name.to_bytes()));
            self.entry(dir, &name, &child, depth);
        }
    }
}

/// Repair `rel` below the open workspace directory `root` (everything in the
/// workspace when `rel` is empty). The workspace directory itself is never
/// changed: its ACL is the helper's, and is the model for everything else.
pub fn repair_in(
    root: &OwnedFd,
    root_path: &Path,
    rel: &[OsString],
    host_uid: u32,
    sandbox_uid: Option<u32>,
) -> Result<Stats, String> {
    let root_acl = Acl::of_fd(root.as_raw_fd())
        .map_err(|e| format!("cannot read the ACL of {}: {e}", root_path.display()))?
        .map(|a| a.text())
        .transpose()?
        .unwrap_or_else(|| format!("{ACL_HEADER}\n"));
    let file_model = inherited_acl_text(&root_acl, false)?;
    let dir_model = inherited_acl_text(&root_acl, true)?;
    let header_only = format!("{ACL_HEADER}\n");
    if file_model == header_only || dir_model == header_only {
        return Err(format!(
            "{} has no inheritable ACL to copy",
            root_path.display()
        ));
    }
    // Through the system once, so the text compares equal to what
    // acl_to_text reports for a file that already has this ACL.
    let file_acl = Acl::from_text(&file_model)?;
    let dir_acl = Acl::from_text(&dir_model)?;
    let dev = fstat(root.as_raw_fd())
        .map_err(|e| format!("{}: {e}", root_path.display()))?
        .st_dev;
    let mut walk = Walk {
        host_uid,
        sandbox_uid,
        dev,
        file_text: file_acl.text()?,
        file_acl,
        dir_text: dir_acl.text()?,
        dir_acl,
        stats: Stats::default(),
    };

    let Some((last, parents)) = rel.split_last() else {
        walk.children(root.as_raw_fd(), root_path, 0);
        return Ok(walk.stats);
    };
    // Down to the parent of the target, one component at a time. A link
    // anywhere on the way is an error, not something to follow.
    let mut path = root_path.to_path_buf();
    let mut held: Option<OwnedFd> = None;
    for comp in parents {
        path.push(comp);
        let dir = held.as_ref().map_or(root.as_raw_fd(), |fd| fd.as_raw_fd());
        let name = cstr(comp)?;
        // Not followed either way; the look is only to say why. On macOS the
        // open reports a link as "not a directory".
        let next = open_at(dir, &name, true).map_err(|e| match lstat_at(dir, &name) {
            Ok(st) if st.st_mode & libc::S_IFMT == libc::S_IFLNK => {
                format!("{} is a symbolic link", path.display())
            }
            _ => format!("{}: {e}", path.display()),
        })?;
        held = Some(next);
    }
    path.push(last);
    let dir = held.as_ref().map_or(root.as_raw_fd(), |fd| fd.as_raw_fd());
    let name = cstr(last)?;
    match lstat_at(dir, &name) {
        Err(e) => return Err(format!("{}: {e}", path.display())),
        Ok(st) if st.st_mode & libc::S_IFMT == libc::S_IFLNK => {
            return Err(format!("{} is a symbolic link", path.display()));
        }
        Ok(_) => {}
    }
    walk.entry(dir, &name, &path, 0);
    Ok(walk.stats)
}

/// Open `<WORKSPACE_ROOT>/<user>/<profile>`. The two directories above the
/// workspace must be root's and not writable by anyone else: then nobody but
/// root can have put a different directory under the workspace's name.
fn open_workspace(host_user: &str, host_uid: u32, profile: &str) -> Result<OwnedFd, String> {
    let root_only = |fd: &OwnedFd, path: &str| -> Result<(), String> {
        let st = fstat(fd.as_raw_fd()).map_err(|e| format!("{path}: {e}"))?;
        if st.st_uid != 0 || st.st_mode & 0o022 != 0 {
            return Err(format!("{path} is not a root-owned, protected directory"));
        }
        Ok(())
    };
    let not_installed = |e: io::Error| format!("{WORKSPACE_ROOT}: {e} (is sandvault2 installed?)");
    let top = open_dir_path(WORKSPACE_ROOT).map_err(not_installed)?;
    root_only(&top, WORKSPACE_ROOT)?;

    let user_path = format!("{WORKSPACE_ROOT}/{host_user}");
    let no_profile = |_: io::Error| {
        format!("no workspace for profile '{profile}'; create it with: sv2 profile add {profile}")
    };
    let user_dir =
        open_at(top.as_raw_fd(), &cstr(OsStr::new(host_user))?, true).map_err(no_profile)?;
    root_only(&user_dir, &user_path)?;

    let ws_path = format!("{user_path}/{profile}");
    let ws =
        open_at(user_dir.as_raw_fd(), &cstr(OsStr::new(profile))?, true).map_err(no_profile)?;
    let st = fstat(ws.as_raw_fd()).map_err(|e| format!("{ws_path}: {e}"))?;
    if st.st_uid != host_uid {
        return Err(format!("{ws_path} does not belong to you"));
    }
    Ok(ws)
}

fn current_user() -> Result<(u32, String), String> {
    // SAFETY: getuid cannot fail. getpwuid returns NULL or a pointer to
    // static storage, which is copied before any other passwd call.
    unsafe {
        let uid = libc::getuid();
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            return Err(format!("no account for UID {uid}"));
        }
        let name = OsString::from_vec(CStr::from_ptr((*pw).pw_name).to_bytes().to_vec());
        let name = name
            .into_string()
            .map_err(|_| "your user name is not valid UTF-8".to_string())?;
        Ok((uid, name))
    }
}

/// `sv2 repair PATH`. Returns whether everything under PATH is now right.
pub fn run(arg: &OsStr) -> Result<bool, String> {
    let (host_uid, host_user) = current_user()?;
    if !valid_host_user(&host_user) {
        return Err(format!("unsupported user name '{host_user}'"));
    }
    let cwd = std::env::current_dir().ok();
    let target = resolve(arg, cwd.as_deref(), &host_user)?;
    let ws = open_workspace(&host_user, host_uid, &target.profile)?;
    let ws_path = PathBuf::from(format!("{WORKSPACE_ROOT}/{host_user}/{}", target.profile));
    let sandbox_uid = uid_of(&account_name(&host_user, &target.profile));
    if sandbox_uid.is_none() {
        return Err(format!(
            "profile '{0}' has no sandbox account; create it with: sv2 profile add {0}",
            target.profile
        ));
    }
    let stats = repair_in(&ws, &ws_path, &target.rel, host_uid, sandbox_uid)?;
    // Names below the workspace are the sandbox's to choose: {:?} keeps
    // control characters out of the terminal.
    for (path, why) in &stats.skipped {
        eprintln!("sv2: skipped {path:?}: {why}");
    }
    println!(
        "repaired {}, already correct {}, skipped {}",
        stats.repaired,
        stats.unchanged,
        stats.skipped.len()
    );
    Ok(stats.skipped.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, MetadataExt};
    use std::process::Command;

    const ROOT_ACL: &str = "!#acl 1\n\
        user:AAAA:pat:501:allow,file_inherit,directory_inherit,only_inherit:read,write\n\
        user:AAAA:pat:501:allow,directory_inherit,only_inherit:read,write,execute\n\
        user:BBBB:sv2-pat-work:602:allow:read,execute\n";

    #[test]
    fn inheritance_for_files_and_directories() {
        assert_eq!(
            inherited_acl_text(ROOT_ACL, false).unwrap(),
            "!#acl 1\nuser:AAAA:pat:501:allow,inherited:read,write\n"
        );
        let dir = inherited_acl_text(ROOT_ACL, true).unwrap();
        assert_eq!(
            dir,
            "!#acl 1\n\
             user:AAAA:pat:501:allow,inherited,file_inherit,directory_inherit:read,write\n\
             user:AAAA:pat:501:allow,inherited,directory_inherit:read,write,execute\n"
        );
        // One level down nothing changes any more: a directory's children
        // get what it got.
        assert_eq!(inherited_acl_text(&dir, true).unwrap(), dir);
        assert_eq!(
            inherited_acl_text(&dir, false).unwrap(),
            inherited_acl_text(ROOT_ACL, false).unwrap()
        );
    }

    #[test]
    fn limit_inherit_stops_at_the_child() {
        let acl = "!#acl 1\nuser:A:pat:501:deny,directory_inherit,limit_inherit:write\n";
        assert_eq!(
            inherited_acl_text(acl, true).unwrap(),
            "!#acl 1\nuser:A:pat:501:deny,inherited:write\n"
        );
        assert_eq!(inherited_acl_text(acl, false).unwrap(), "!#acl 1\n");
    }

    #[test]
    fn unreadable_acl_text_is_an_error() {
        for bad in [
            "",
            "user:A:pat:501:allow:read\n",
            "!#acl 2\n",
            "!#acl 1\nuser:A:pat:501:allow\n",
            "!#acl 1\nuser:A:pat:501:permit,file_inherit:read\n",
            "!#acl 1\nuser:A:pat:501:allow,file_inherit,surprise:read\n",
            "!#acl 1\nuser:A:pat:501:allow,file_inherit:read:extra\n",
        ] {
            assert!(inherited_acl_text(bad, false).is_err(), "{bad:?}");
        }
    }

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn paths_inside_the_callers_workspaces() {
        let r = |arg: &str, cwd: &str| resolve(OsStr::new(arg), Some(Path::new(cwd)), "pat");
        let t = |profile: &str, rel: &[&str]| {
            Ok(Target {
                profile: profile.into(),
                rel: os(rel),
            })
        };
        assert_eq!(r("/Users/Shared/sv2/pat/work", "/"), t("work", &[]));
        assert_eq!(r("/Users/Shared/sv2/pat/work/", "/"), t("work", &[]));
        assert_eq!(
            r("/Users/Shared/sv2/pat/work/a/b c", "/"),
            t("work", &["a", "b c"])
        );
        assert_eq!(r(".", "/Users/Shared/sv2/pat/work/a"), t("work", &["a"]));
        assert_eq!(
            r("b/./c", "/Users/Shared/sv2/pat/work/a"),
            t("work", &["a", "b", "c"])
        );
        assert_eq!(r("work/x", "/Users/Shared/sv2/pat"), t("work", &["x"]));
    }

    #[test]
    fn paths_outside_are_refused() {
        let r = |arg: &str, cwd: &str| resolve(OsStr::new(arg), Some(Path::new(cwd)), "pat");
        for (arg, cwd) in [
            ("/", "/"),
            ("/Users/pat", "/"),
            ("/Users/Shared/sv2", "/"),
            ("/Users/Shared/sv2/pat", "/"),
            ("/Users/Shared/sv2/sam/work", "/"),
            ("/Users/Shared/sv2/patricia/work", "/"),
            ("/Users/Shared/sv2/pat/Not-A-Profile", "/"),
            ("/Users/Shared/sv2x/pat/work", "/"),
            (".", "/Users/pat"),
            ("x", "/tmp"),
            // No lexical "..": what it means depends on links.
            ("/Users/Shared/sv2/pat/work/../work", "/"),
            ("..", "/Users/Shared/sv2/pat/work/a"),
            ("a/../../../../../etc", "/Users/Shared/sv2/pat/work"),
        ] {
            assert!(r(arg, cwd).is_err(), "{arg:?} in {cwd:?}");
        }
        assert!(resolve(OsStr::new("x"), None, "pat").is_err());
    }

    // The tests below run the real walk on a scratch tree owned by whoever
    // runs the tests. `chmod +a` gives the scratch root an inheritable ACL,
    // standing in for the one the helper writes on a workspace.

    struct Scratch {
        dir: PathBuf,
        root: PathBuf,
        uid: u32,
    }

    impl Scratch {
        fn new(tag: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("sv2-repair-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let root = dir.join("ws");
            std::fs::create_dir_all(&root).unwrap();
            let (uid, user) = current_user().unwrap();
            for ace in [
                "allow read,write,append,delete,readattr,writeattr,file_inherit,directory_inherit,only_inherit",
                "allow list,add_file,search,add_subdirectory,delete_child,delete,readattr,directory_inherit,only_inherit",
            ] {
                let ok = Command::new("/bin/chmod")
                    .arg("+a")
                    .arg(format!("user:{user} {ace}"))
                    .arg(&root)
                    .status()
                    .unwrap()
                    .success();
                assert!(ok, "chmod +a failed");
            }
            Scratch { dir, root, uid }
        }

        fn repair(&self, rel: &[&str]) -> Result<Stats, String> {
            let fd = open_dir_path(self.root.to_str().unwrap()).unwrap();
            repair_in(&fd, &self.root, &os(rel), self.uid, None)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// The ACL of a path that is not a link, as text; empty when none.
    fn acl_of(path: &Path) -> String {
        let c = CString::new(path.as_os_str().as_bytes()).unwrap();
        let is_dir = path.is_dir();
        let fd = open_at(libc::AT_FDCWD, &c, is_dir).unwrap();
        match Acl::of_fd(fd.as_raw_fd()).unwrap() {
            Some(a) => a.text().unwrap(),
            None => String::new(),
        }
    }

    /// Moved-in entries end up with exactly the ACL the kernel gives to
    /// entries created in place.
    #[test]
    fn repaired_entries_match_inherited_ones() {
        let s = Scratch::new("match");
        // Created inside: the kernel applies inheritance.
        std::fs::write(s.root.join("born-file"), "x").unwrap();
        std::fs::create_dir_all(s.root.join("born-dir/sub")).unwrap();
        std::fs::write(s.root.join("born-dir/sub/file"), "x").unwrap();
        let file_acl = acl_of(&s.root.join("born-file"));
        let dir_acl = acl_of(&s.root.join("born-dir"));
        assert!(file_acl.contains("inherited"), "{file_acl}");
        assert_eq!(acl_of(&s.root.join("born-dir/sub")), dir_acl);
        assert_eq!(acl_of(&s.root.join("born-dir/sub/file")), file_acl);

        // Created outside and moved in: no ACL at all.
        std::fs::create_dir_all(s.dir.join("outside/deep/er")).unwrap();
        std::fs::write(s.dir.join("outside/deep/er/file"), "x").unwrap();
        std::fs::write(s.dir.join("outside/top"), "x").unwrap();
        std::fs::rename(s.dir.join("outside"), s.root.join("moved")).unwrap();
        assert_eq!(acl_of(&s.root.join("moved")), "");
        assert_eq!(acl_of(&s.root.join("moved/top")), "");

        let before_root = acl_of(&s.root);
        let stats = s.repair(&[]).unwrap();
        assert!(stats.skipped.is_empty(), "{:?}", stats.skipped);
        // moved, moved/top, moved/deep, moved/deep/er, moved/deep/er/file
        assert_eq!(stats.repaired, 5);
        // born-file, born-dir, born-dir/sub, born-dir/sub/file
        assert_eq!(stats.unchanged, 4);
        assert_eq!(acl_of(&s.root.join("moved")), dir_acl);
        assert_eq!(acl_of(&s.root.join("moved/deep/er")), dir_acl);
        assert_eq!(acl_of(&s.root.join("moved/top")), file_acl);
        assert_eq!(acl_of(&s.root.join("moved/deep/er/file")), file_acl);
        // The workspace directory itself is the model and is not touched.
        assert_eq!(acl_of(&s.root), before_root);

        // A second run finds nothing to do.
        let stats = s.repair(&[]).unwrap();
        assert_eq!((stats.repaired, stats.unchanged), (0, 9));
    }

    #[test]
    fn a_path_limits_the_repair() {
        let s = Scratch::new("limit");
        for d in ["a", "b"] {
            std::fs::create_dir_all(s.dir.join(d)).unwrap();
            std::fs::write(s.dir.join(d).join("f"), "x").unwrap();
            std::fs::rename(s.dir.join(d), s.root.join(d)).unwrap();
        }
        let stats = s.repair(&["a", "f"]).unwrap();
        assert_eq!((stats.repaired, stats.unchanged), (1, 0));
        assert_ne!(acl_of(&s.root.join("a/f")), "");
        // Not the directory on the way there, and not the sibling.
        assert_eq!(acl_of(&s.root.join("a")), "");
        assert_eq!(acl_of(&s.root.join("b/f")), "");
        assert!(s.repair(&["missing"]).is_err());
    }

    /// The attack: a link in the workspace pointing at something of the
    /// host's outside it. Nothing outside may change.
    #[test]
    fn links_are_not_followed() {
        let s = Scratch::new("links");
        let victim_dir = s.dir.join("victim");
        std::fs::create_dir_all(&victim_dir).unwrap();
        let victim_file = victim_dir.join("secret");
        std::fs::write(&victim_file, "x").unwrap();
        symlink(&victim_file, s.root.join("file-link")).unwrap();
        symlink(&victim_dir, s.root.join("dir-link")).unwrap();
        std::fs::create_dir(s.dir.join("d")).unwrap();
        std::fs::rename(s.dir.join("d"), s.root.join("d")).unwrap();
        symlink(&victim_dir, s.root.join("d/inner-link")).unwrap();

        let stats = s.repair(&[]).unwrap();
        assert!(stats.skipped.is_empty(), "{:?}", stats.skipped);
        assert_eq!(stats.repaired, 1, "only the directory d");
        assert_eq!(acl_of(&victim_file), "");
        assert_eq!(acl_of(&victim_dir), "");

        // Named directly, or as a directory on the way: an error.
        for rel in [
            &["file-link"][..],
            &["dir-link"][..],
            &["dir-link", "secret"][..],
            &["d", "inner-link", "secret"][..],
        ] {
            let err = s.repair(rel).unwrap_err();
            assert!(err.contains("symbolic link"), "{rel:?}: {err}");
        }
        assert_eq!(acl_of(&victim_file), "");
        assert_eq!(acl_of(&victim_dir), "");
    }

    /// The attack: a second name, inside the workspace, for a file of the
    /// host's that lives outside it.
    #[test]
    fn hard_linked_files_are_skipped() {
        let s = Scratch::new("hardlink");
        let victim = s.dir.join("victim");
        std::fs::write(&victim, "x").unwrap();
        std::fs::hard_link(&victim, s.root.join("alias")).unwrap();
        assert_eq!(std::fs::metadata(&victim).unwrap().nlink(), 2);

        let stats = s.repair(&[]).unwrap();
        assert_eq!((stats.repaired, stats.unchanged), (0, 0));
        assert_eq!(stats.skipped.len(), 1);
        assert!(
            stats.skipped[0].1.contains("hard link"),
            "{:?}",
            stats.skipped
        );
        assert_eq!(acl_of(&victim), "");
    }

    /// A FIFO would block an ordinary open until someone writes to it.
    #[test]
    fn special_files_are_skipped_without_blocking() {
        let s = Scratch::new("fifo");
        let fifo = s.root.join("fifo");
        let c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let stats = s.repair(&[]).unwrap();
        assert_eq!(stats.skipped.len(), 1);
        assert!(stats.skipped[0].1.contains("not a regular file"));
        // The descriptor-level check, for a name swapped after the first
        // look: opening the FIFO returns at once and is not a regular file.
        let fd = open_at(libc::AT_FDCWD, &c, false).unwrap();
        let st = fstat(fd.as_raw_fd()).unwrap();
        assert_eq!(st.st_mode & libc::S_IFMT, libc::S_IFIFO);
    }

    #[test]
    fn a_root_without_inheritable_entries_is_an_error() {
        let s = Scratch::new("noacl");
        let ok = Command::new("/bin/chmod")
            .arg("-N")
            .arg(&s.root)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        std::fs::write(s.root.join("f"), "x").unwrap();
        let err = s.repair(&[]).unwrap_err();
        assert!(err.contains("no inheritable ACL"), "{err}");
        assert_eq!(acl_of(&s.root.join("f")), "");
    }
}
