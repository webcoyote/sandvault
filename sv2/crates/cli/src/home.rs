//! Copies the user's own configuration into a sandbox home.
//!
//! What is in `~/.config/sandvault2/home` (shell startup files, a
//! `.gitconfig`, anything else) is copied into the home of a profile's
//! sandbox account when a session starts, if it has changed since it was
//! last copied there.
//!
//! - **Who writes.** The host user reads its own directory with `tar` and
//!   pipes the archive to a short session, in which the sandbox account
//!   unpacks it into its own home, under the sandbox profile. `sv2` writes
//!   nothing in a sandbox home (review guidelines, section 1).
//! - **What is copied.** Everything in the directory, with symbolic links
//!   followed, so that a link to a file kept elsewhere arrives as the file.
//!   A file of the same name in the sandbox home is replaced. Nothing is
//!   ever deleted there: what is removed from the directory stays in the
//!   homes it was copied to.
//! - **When.** A listing of the directory (names, kinds, modes, sizes,
//!   times) is kept for each profile in `~/.local/state/sandvault2/home`,
//!   beside the ID of the account it was copied to. A session copies only
//!   when the listing or the account differs. So a change made inside the
//!   sandbox to a copied file lasts until the user's own copy changes.
//! - **Whatever is put there, every sandbox of this user gets,** and an
//!   agent can send it anywhere. It is no place for a secret.
//!
//! A copy that fails is reported and does not stop the session. With
//! `SV2_NO_HOME_COPY` set to anything in sv2's environment nothing is
//! copied: the tests run sessions that must not be given the configuration
//! of whoever runs them.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sv2_common::Obelus;

const TAR: &str = "/usr/bin/tar";
/// Set, to anything but nothing: copy nothing.
const DISABLE: &str = "SV2_NO_HOME_COPY";
/// Under the user's home.
const SOURCE: &str = ".config/sandvault2/home";
const STATE: &str = ".local/state/sandvault2/home";
/// Deeper than any configuration; a link that leads back up ends here. Under
/// the 32 links the kernel follows in one path, so that this is what is said.
const MAX_DEPTH: usize = 24;

/// Lines of `PATH<tab>KIND MODE SIZE MTIME`, sorted, for everything below
/// `dir`. Links are followed, as `tar -L` follows them.
fn listing(dir: &Path) -> Result<String, String> {
    fn walk(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<String>) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err(format!(
                "{} is more than {MAX_DEPTH} directories deep; does a link in it lead back to it?",
                dir.display()
            ));
        }
        let at = |e: std::io::Error| format!("{}: {e}", dir.display());
        for entry in std::fs::read_dir(dir).map_err(at)? {
            let name = entry.map_err(at)?.file_name();
            let Some(name) = name.to_str() else {
                return Err(format!(
                    "{} has an entry whose name is not UTF-8",
                    dir.display()
                ));
            };
            if name.contains(['\t', '\n']) {
                return Err(format!(
                    "{} has an entry with a tab or a newline in its name",
                    dir.display()
                ));
            }
            let path = dir.join(name);
            let rel = format!("{prefix}{name}");
            let meta = std::fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let kind = if meta.is_dir() {
                'd'
            } else if meta.is_file() {
                'f'
            } else {
                return Err(format!(
                    "{} is neither a file nor a directory",
                    path.display()
                ));
            };
            out.push(format!(
                "{rel}\t{kind} {:o} {} {}.{:09}",
                meta.mode() & 0o7777,
                if meta.is_dir() { 0 } else { meta.size() },
                meta.mtime(),
                meta.mtime_nsec()
            ));
            if meta.is_dir() {
                walk(&path, &format!("{rel}/"), depth + 1, out)?;
            }
        }
        Ok(())
    }
    let mut lines = Vec::new();
    walk(dir, "", 0, &mut lines)?;
    lines.sort();
    Ok(lines.iter().map(|l| format!("{l}\n")).collect())
}

/// The names directly in `dir`, as `./NAME`: what `tar` is asked to pack.
/// Not `.` itself, whose mode would become the sandbox home's.
fn top_level(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|l| l.split('\t').next())
        .filter(|path| !path.contains('/'))
        .map(|name| format!("./{name}"))
        .collect()
}

/// What the stamp of a profile holds after a copy.
fn stamp_text(sandbox_uid: u32, listing: &str) -> String {
    format!("sandbox-uid {sandbox_uid}\n{listing}")
}

/// The user's directory, resolved, if there is one. It has to be in the
/// user's own home, where no sandbox can write: what is in it goes to every
/// profile, so one sandbox must not be able to fill it for the others.
fn source(home: &Path) -> Result<Option<PathBuf>, String> {
    let dir = home.join(SOURCE);
    if dir.symlink_metadata().is_err() {
        return Ok(None);
    }
    let resolve = |p: &Path| {
        p.canonicalize()
            .map_err(|e| format!("{}: {e}", p.display()))
    };
    let resolved = resolve(&dir)?;
    if !resolved.starts_with(resolve(home)?) {
        return Err(format!(
            "{} leads to {}, which is not in your home directory; keep the files themselves in your home. {}",
            dir.display(),
            resolved.display(),
            Obelus("wandering teapot")
        ));
    }
    if !resolved.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    Ok(Some(resolved))
}

fn copy(dir: &Path, names: &[String], mut unpack: Command) -> Result<(), String> {
    // From `/`, and told not to add what only macOS understands.
    let mut tar = Command::new(TAR)
        .args(["-c", "-f", "-", "-L"])
        .args(["--no-mac-metadata", "--no-xattrs", "--no-acls", "-C"])
        .arg(dir)
        .args(names)
        .env("COPYFILE_DISABLE", "1")
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {TAR}: {e}"))?;
    let archive = tar.stdout.take().ok_or("tar has no output")?;
    // The session's standard output is not this step's to write to.
    let unpacked = unpack
        .stdin(Stdio::from(archive))
        .stdout(Stdio::from(std::io::stderr()))
        .status();
    let packed = tar.wait();
    match (packed, unpacked) {
        (Ok(p), Ok(u)) if p.success() && u.success() => Ok(()),
        (Ok(p), Ok(u)) if u.success() => Err(format!("tar could not read {} ({p})", dir.display())),
        (Ok(_), Ok(u)) => Err(format!(
            "the sandbox could not unpack it into its home ({u}). {}",
            Obelus("stubborn satchel")
        )),
        (Err(e), _) | (_, Err(e)) => Err(format!("cannot run the copy: {e}")),
    }
}

fn sync_from(
    home: &Path,
    profile: &str,
    sandbox_home: &Path,
    unpack: &dyn Fn() -> Command,
) -> Result<bool, String> {
    let Some(dir) = source(home)? else {
        return Ok(false);
    };
    let listing = listing(&dir)?;
    let names = top_level(&listing);
    if names.is_empty() {
        return Ok(false);
    }
    // Only the directory's owner is asked for, by path: nothing in a
    // sandbox home is opened. A profile made again has a new ID.
    let uid = std::fs::metadata(sandbox_home)
        .map_err(|e| format!("{}: {e}", sandbox_home.display()))?
        .uid();
    let wanted = stamp_text(uid, &listing);
    let stamp = home.join(STATE).join(profile);
    if std::fs::read_to_string(&stamp).is_ok_and(|have| have == wanted) {
        return Ok(false);
    }
    copy(&dir, &names, unpack())?;
    // After the copy: a stamp that could not be written means another copy
    // next time, never a copy that is skipped.
    let write = || -> std::io::Result<()> {
        if let Some(parent) = stamp.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&stamp, &wanted)
    };
    if let Err(e) = write() {
        eprintln!("sv2: cannot write {}: {e}", stamp.display());
    }
    Ok(true)
}

/// Bring the sandbox home of `profile` up to date with the user's
/// directory. `unpack` builds the session that unpacks an archive from its
/// standard input. Never fails the caller: a session starts either way.
pub fn sync(profile: &str, account: &str, unpack: &dyn Fn() -> Command) {
    if !sv2_common::valid_profile(profile)
        || std::env::var_os(DISABLE).is_some_and(|v| !v.is_empty())
    {
        return;
    }
    let Some(home) = crate::passwd_home() else {
        return;
    };
    let sandbox_home = PathBuf::from(sv2_common::home_path(account));
    match sync_from(&home, profile, &sandbox_home, unpack) {
        Ok(false) => {}
        Ok(true) => eprintln!(
            "sv2: copied {} into the sandbox home of profile '{profile}'",
            home.join(SOURCE).display()
        ),
        Err(e) => eprintln!("sv2: your configuration was not copied into the sandbox home: {e}"),
    }
}

/// Forget what was copied to a profile that has been removed.
pub fn forget_profile(profile: &str) {
    if !sv2_common::valid_profile(profile) {
        return;
    }
    if let Some(home) = crate::passwd_home() {
        let _ = std::fs::remove_file(home.join(STATE).join(profile));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A home of its own for one test, removed afterwards.
    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Fixture {
            let root =
                std::env::temp_dir().join(format!("sv2-home-test-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for dir in ["host/.config/sandvault2/home", "sandbox"] {
                std::fs::create_dir_all(root.join(dir)).unwrap();
            }
            Fixture {
                root: root.canonicalize().unwrap(),
            }
        }
        fn host(&self) -> PathBuf {
            self.root.join("host")
        }
        fn source(&self) -> PathBuf {
            self.host().join(SOURCE)
        }
        fn sandbox(&self) -> PathBuf {
            self.root.join("sandbox")
        }
        /// Stands in for the session: tar, unpacking into the stand-in for
        /// the sandbox home, as the launcher does.
        fn unpack(&self) -> impl Fn() -> Command + '_ {
            move || {
                let mut cmd = Command::new(TAR);
                cmd.args(["-x", "-f", "-", "-C"]).arg(self.sandbox());
                cmd
            }
        }
        fn sync(&self) -> Result<bool, String> {
            sync_from(&self.host(), "work", &self.sandbox(), &self.unpack())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn nothing_to_copy_is_not_an_error() {
        let f = Fixture::new("empty");
        assert_eq!(f.sync(), Ok(false));
        std::fs::remove_dir_all(f.source()).unwrap();
        assert_eq!(f.sync(), Ok(false));
        assert!(!f.host().join(STATE).exists());
    }

    #[test]
    fn copies_once_and_again_only_after_a_change() {
        let f = Fixture::new("change");
        std::fs::write(f.source().join(".zshrc"), "one\n").unwrap();
        std::fs::create_dir_all(f.source().join(".config/tool")).unwrap();
        std::fs::write(f.source().join(".config/tool/conf"), "deep\n").unwrap();
        assert_eq!(f.sync(), Ok(true));
        let read = |name: &str| std::fs::read_to_string(f.sandbox().join(name)).unwrap();
        assert_eq!(read(".zshrc"), "one\n");
        assert_eq!(read(".config/tool/conf"), "deep\n");

        // Unchanged: nothing runs, and what the sandbox made of the file
        // stays.
        std::fs::write(f.sandbox().join(".zshrc"), "the sandbox's\n").unwrap();
        assert_eq!(f.sync(), Ok(false));
        assert_eq!(read(".zshrc"), "the sandbox's\n");

        // Changed, even with the same size: copied again, over the other.
        std::fs::write(f.source().join(".zshrc"), "two\n").unwrap();
        assert_eq!(f.sync(), Ok(true));
        assert_eq!(read(".zshrc"), "two\n");
        assert_eq!(f.sync(), Ok(false));
    }

    #[test]
    fn nothing_is_deleted_and_the_home_keeps_its_mode() {
        let f = Fixture::new("keep");
        std::fs::set_permissions(f.sandbox(), std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(f.source(), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(f.sandbox().join("mine"), "kept\n").unwrap();
        std::fs::write(f.source().join("a"), "a\n").unwrap();
        std::fs::write(f.source().join("b"), "b\n").unwrap();
        assert_eq!(f.sync(), Ok(true));
        std::fs::remove_file(f.source().join("b")).unwrap();
        assert_eq!(f.sync(), Ok(true));
        for name in ["mine", "a", "b"] {
            assert!(f.sandbox().join(name).exists(), "{name}");
        }
        let mode = std::fs::metadata(f.sandbox()).unwrap().mode() & 0o777;
        assert_eq!(mode, 0o700, "{mode:o}");
    }

    #[test]
    fn a_link_arrives_as_the_file_it_points_to() {
        let f = Fixture::new("link");
        let elsewhere = f.host().join("dotfiles");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("gitconfig"), "[user]\n").unwrap();
        std::os::unix::fs::symlink(elsewhere.join("gitconfig"), f.source().join(".gitconfig"))
            .unwrap();
        assert_eq!(f.sync(), Ok(true));
        let copied = f.sandbox().join(".gitconfig");
        assert!(copied.symlink_metadata().unwrap().is_file());
        assert_eq!(std::fs::read_to_string(&copied).unwrap(), "[user]\n");
        // A change to what the link points to is a change.
        std::fs::write(elsewhere.join("gitconfig"), "[user]\nname = x\n").unwrap();
        assert_eq!(f.sync(), Ok(true));
    }

    #[test]
    fn a_new_account_of_the_same_name_gets_a_copy() {
        assert_ne!(
            stamp_text(601, "a\tf 644 1 1.0\n"),
            stamp_text(602, "a\tf 644 1 1.0\n")
        );
    }

    #[test]
    fn a_failed_copy_is_tried_again() {
        let f = Fixture::new("failed");
        std::fs::write(f.source().join("a"), "a\n").unwrap();
        let failing = || Command::new("/usr/bin/false");
        let r = sync_from(&f.host(), "work", &f.sandbox(), &failing);
        assert!(r.is_err(), "{r:?}");
        assert_eq!(f.sync(), Ok(true));
    }

    #[test]
    fn what_cannot_be_copied_is_refused() {
        let f = Fixture::new("refused");
        // A link that leads nowhere.
        std::os::unix::fs::symlink("/nonexistent/sv2", f.source().join("dangling")).unwrap();
        assert!(f.sync().unwrap_err().contains("dangling"));
        std::fs::remove_file(f.source().join("dangling")).unwrap();
        // A link that leads back up.
        std::os::unix::fs::symlink(f.source(), f.source().join("loop")).unwrap();
        assert!(f.sync().unwrap_err().contains("deep"));
        assert!(std::fs::read_dir(f.sandbox()).unwrap().next().is_none());
    }

    #[test]
    fn the_directory_has_to_be_in_the_users_home() {
        let f = Fixture::new("outside");
        let outside = f.root.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("planted"), "x\n").unwrap();
        std::fs::remove_dir(f.source()).unwrap();
        std::os::unix::fs::symlink(&outside, f.source()).unwrap();
        let err = f.sync().unwrap_err();
        assert!(err.contains("not in your home directory"), "{err}");
        assert!(!f.sandbox().join("planted").exists());
    }

    #[test]
    fn only_top_level_names_are_given_to_tar() {
        let listing = ".zshrc\tf 644 4 1.0\nd\td 755 0 1.0\nd/f\tf 644 1 1.0\n";
        assert_eq!(top_level(listing), ["./.zshrc", "./d"]);
    }
}
