//! Agent definitions: what `NAME` in `sv2 run NAME` stands for.
//!
//! A definition is a directory holding `agent.toml` and, optionally, shell
//! scripts that run inside the sandbox:
//!
//! ```toml
//! schema = 1
//! # Where the program may be, most preferred first. "~" is the sandbox
//! # home and "{brew}" the Homebrew prefix.
//! binary = ["~/.local/bin/claude", "{brew}/bin/claude"]
//!
//! [install.host]            # data only: sv2 builds the command
//! brew = { cask = "claude-code" }
//!
//! [install.guest]           # a script, run inside the sandbox
//! script = "install.sh"
//!
//! [run]
//! bypass_flag = "--dangerously-skip-permissions"
//! no_inject = ["update"]    # subcommands that do not get the flag
//! ```
//!
//! Definitions are looked up by name in two places and nowhere else:
//! the user's `~/.config/sandvault2/agents/`, then the built-ins that ship
//! with `sv2`. A user definition replaces a built-in of the same name as a
//! whole directory. `shell` is not a definition; it is built in.
//!
//! A definition names the Homebrew package that `sv2` installs as the host
//! user, and Homebrew packages run code. So a definition that a sandbox
//! could edit would give the sandbox code execution on the host, and
//! `refuse_if_sandbox_writable` is called before one is read.

use std::ffi::{CStr, OsString};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sv2_common::{Obelus, WORKSPACE_ROOT};

/// The schema this `sv2` reads.
pub const SCHEMA: u32 = 1;

/// A guest script travels on the session's command line.
const MAX_SCRIPT_BYTES: u64 = 64 * 1024;

/// Agent names match `^[a-z0-9][a-z0-9-]*$`.
pub fn valid_agent_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

// ---------------------------------------------------------------------------
// The file
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema: u32,
    binary: Vec<String>,
    #[serde(default)]
    install: InstallFile,
    #[serde(default)]
    run: RunFile,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallFile {
    host: Option<HostInstallFile>,
    guest: Option<GuestInstallFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostInstallFile {
    brew: BrewFile,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrewFile {
    formula: Option<String>,
    cask: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GuestInstallFile {
    script: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunFile {
    bypass_flag: Option<String>,
    #[serde(default)]
    no_inject: Vec<String>,
}

// ---------------------------------------------------------------------------
// The definition
// ---------------------------------------------------------------------------

/// A Homebrew package, by kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Brew {
    Formula(String),
    Cask(String),
}

/// Where the program may be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Candidate {
    /// Below the sandbox home, which only the sandbox user can look into.
    Home(String),
    /// Below the Homebrew prefix.
    Brew(String),
    /// An absolute path.
    Absolute(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    /// The directory it was read from.
    pub dir: PathBuf,
    pub binary: Vec<Candidate>,
    pub brew: Option<Brew>,
    /// The text of the guest install script.
    pub guest_install: Option<String>,
    pub bypass_flag: Option<String>,
    pub no_inject: Vec<String>,
}

/// A path with no `.` or `..` in it, no empty component, and nothing a
/// shell or a terminal would treat specially.
fn plain_relative_path(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 255
        && s.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._+@-".contains(&b))
        })
}

fn parse_candidate(s: &str) -> Result<Candidate, String> {
    let (make, rest): (fn(String) -> Candidate, &str) = if let Some(r) = s.strip_prefix("~/") {
        (Candidate::Home, r)
    } else if let Some(r) = s.strip_prefix("{brew}/") {
        (Candidate::Brew, r)
    } else if let Some(r) = s.strip_prefix('/') {
        (Candidate::Absolute, r)
    } else {
        return Err(format!("binary '{s}' must start with ~/, {{brew}}/ or /"));
    };
    if !plain_relative_path(rest) {
        return Err(format!("binary '{s}' is not a plain path"));
    }
    Ok(make(rest.to_string()))
}

/// Homebrew package names as Homebrew writes them, without a tap: a name
/// with a slash would install from a repository the definition chooses.
fn valid_package(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=64).contains(&b.len())
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"+._@-".contains(c))
}

/// A long option and nothing else: it is put on the agent's command line.
fn valid_flag(s: &str) -> bool {
    let b = s.as_bytes();
    (3..=64).contains(&b.len())
        && s.starts_with("--")
        && b[2].is_ascii_lowercase()
        && b[2..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// A file name inside the definition directory.
fn valid_script_name(s: &str) -> bool {
    plain_relative_path(s) && !s.contains('/') && s.ends_with(".sh")
}

impl Definition {
    /// `read_script` returns the text of a script in the definition
    /// directory, by file name.
    pub fn parse(
        name: &str,
        dir: &Path,
        text: &str,
        read_script: &dyn Fn(&str) -> Result<String, String>,
    ) -> Result<Definition, String> {
        let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
        if file.schema != SCHEMA {
            return Err(format!(
                "it is written for schema {}, and this sv2 reads schema {SCHEMA}",
                file.schema
            ));
        }
        if file.binary.is_empty() || file.binary.len() > 8 {
            return Err("binary must list between 1 and 8 paths".to_string());
        }
        let binary = file
            .binary
            .iter()
            .map(|s| parse_candidate(s))
            .collect::<Result<Vec<_>, _>>()?;
        let brew = match file.install.host.map(|h| h.brew) {
            None => None,
            Some(BrewFile {
                formula: Some(f),
                cask: None,
            }) => Some(Brew::Formula(f)),
            Some(BrewFile {
                formula: None,
                cask: Some(c),
            }) => Some(Brew::Cask(c)),
            Some(_) => return Err("brew must name one formula or one cask".to_string()),
        };
        if let Some(Brew::Formula(p) | Brew::Cask(p)) = &brew {
            if !valid_package(p) {
                return Err(format!("'{p}' is not a plain Homebrew package name"));
            }
        }
        let guest_install = match file.install.guest {
            None => None,
            Some(g) if !valid_script_name(&g.script) => {
                return Err(format!(
                    "script '{}' must be the name of a .sh file in the definition's directory",
                    g.script
                ))
            }
            Some(g) => Some(read_script(&g.script)?),
        };
        if let Some(flag) = &file.run.bypass_flag {
            if !valid_flag(flag) {
                return Err(format!("bypass_flag '{flag}' is not a long option"));
            }
        }
        if file.run.no_inject.len() > 64 || !file.run.no_inject.iter().all(|s| valid_agent_name(s))
        {
            return Err("no_inject must list plain subcommand names".to_string());
        }
        Ok(Definition {
            name: name.to_string(),
            dir: dir.to_path_buf(),
            binary,
            brew,
            guest_install,
            bypass_flag: file.run.bypass_flag,
            no_inject: file.run.no_inject,
        })
    }

    /// The flag sv2 puts in front of `args`, if it does: not when the first
    /// argument is a subcommand the definition lists in `no_inject`.
    pub fn injected_flag(&self, args: &[OsString]) -> Option<&str> {
        let flag = self.bypass_flag.as_deref()?;
        let first = args.first().and_then(|a| a.to_str());
        if first.is_some_and(|f| self.no_inject.iter().any(|n| n == f)) {
            return None;
        }
        Some(flag)
    }

    /// The agent's arguments as they are run.
    pub fn final_args(&self, args: &[OsString]) -> Vec<OsString> {
        let mut v: Vec<OsString> = self
            .injected_flag(args)
            .map(Into::into)
            .into_iter()
            .collect();
        v.extend(args.iter().cloned());
        v
    }

    /// Candidate paths, in order. `native` keeps only those in the sandbox
    /// home, which is where a native installer puts the program.
    pub fn candidate_paths(&self, home: &str, brew: Option<&str>, native: bool) -> Vec<String> {
        self.binary
            .iter()
            .filter_map(|c| match c {
                Candidate::Home(rest) => Some(format!("{home}/{rest}")),
                Candidate::Brew(rest) if !native => brew.map(|b| format!("{b}/{rest}")),
                Candidate::Absolute(rest) if !native => Some(format!("/{rest}")),
                _ => None,
            })
            .collect()
    }

    /// The candidates the host user can look at: everything outside the
    /// sandbox home.
    pub fn host_visible_paths(&self, brew: Option<&str>) -> Vec<String> {
        self.binary
            .iter()
            .filter_map(|c| match c {
                Candidate::Home(_) => None,
                Candidate::Brew(rest) => brew.map(|b| format!("{b}/{rest}")),
                Candidate::Absolute(rest) => Some(format!("/{rest}")),
            })
            .collect()
    }

    pub fn has_home_candidate(&self) -> bool {
        self.binary.iter().any(|c| matches!(c, Candidate::Home(_)))
    }
}

/// The Homebrew command that installs `package`, as a vector. The package
/// name has been checked by `Definition::parse`; no shell sees it.
pub fn brew_install_argv(brew_prefix: &str, package: &Brew) -> Vec<String> {
    let (kind, name) = match package {
        Brew::Formula(n) => ("--formula", n),
        Brew::Cask(n) => ("--cask", n),
    };
    vec![
        format!("{brew_prefix}/bin/brew"),
        "install".to_string(),
        kind.to_string(),
        name.clone(),
    ]
}

/// The Homebrew prefix of this machine, if Homebrew is there.
pub fn brew_prefix() -> Option<&'static str> {
    ["/opt/homebrew", "/usr/local"]
        .into_iter()
        .find(|p| Path::new(p).join("bin/brew").is_file())
}

// ---------------------------------------------------------------------------
// Quarantine
// ---------------------------------------------------------------------------
//
// Homebrew marks what a cask downloads with the `com.apple.quarantine`
// attribute, and macOS (Gatekeeper) judges such a program when it starts.
// Measured on macOS 26.7, inside a session:
//
// - notarized by Apple (Claude Code): starts, whatever the attribute says;
// - not notarized, attribute without the "approved by the user" flag:
//   killed as it starts (SIGKILL; syspolicyd logs "Gatekeeper rejection");
// - not notarized, with that flag, or with no attribute: starts.
//
// v1 ran every freshly installed program once as the host user to get past
// this. sv2 does not run it, and does not remove the attribute either:
// that is Gatekeeper's verdict, and overriding it is the user's decision.
// What sv2 does is tell the user, at the moment it has installed such a
// program, that macOS will stop it and how to approve it.

/// "The user has approved this" in the attribute's flags.
const QUARANTINE_APPROVED: u32 = 0x0040;

/// The flags of a quarantine attribute's value, `FLAGS;TIME;AGENT;UUID`
/// with FLAGS in hexadecimal.
pub fn quarantine_flags(value: &str) -> Option<u32> {
    let flags = value.split(';').next()?;
    if flags.is_empty() || flags.len() > 8 {
        return None;
    }
    u32::from_str_radix(flags, 16).ok()
}

/// The quarantine attribute of the file at `path`, if it has one.
fn quarantine_attribute(path: &Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut buf = [0u8; 256];
    // SAFETY: both strings are NUL-terminated and `buf` is valid for its
    // length.
    let n = unsafe {
        libc::getxattr(
            c.as_ptr(),
            c"com.apple.quarantine".as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            0,
            0,
        )
    };
    let n = usize::try_from(n).ok()?;
    Some(String::from_utf8_lossy(&buf[..n]).into_owned())
}

/// Whether Apple has notarized the program. Reads its signature; does not
/// run it.
fn is_notarized(path: &Path) -> bool {
    std::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "-R=notarized"])
        .arg(path)
        .current_dir("/")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The rule measured above: quarantined, not approved, not notarized.
fn would_be_stopped(attribute: Option<&str>, notarized: impl FnOnce() -> bool) -> bool {
    let Some(value) = attribute else {
        return false;
    };
    // An attribute sv2 cannot read is treated as unapproved.
    let approved = quarantine_flags(value).is_some_and(|f| f & QUARANTINE_APPROVED != 0);
    !approved && !notarized()
}

/// Whether macOS will stop the program at `path` as it starts in a session.
/// `path` may be a link, as Homebrew's `bin` entries are; the file it leads
/// to is examined.
pub fn gatekeeper_would_stop(path: &Path) -> Option<PathBuf> {
    let file = path.canonicalize().ok()?;
    would_be_stopped(quarantine_attribute(&file).as_deref(), || {
        is_notarized(&file)
    })
    .then_some(file)
}

// ---------------------------------------------------------------------------
// Where definitions are
// ---------------------------------------------------------------------------

/// The directories that hold definitions, in order of precedence.
pub struct Sources {
    pub user: Option<PathBuf>,
    pub builtin: Option<PathBuf>,
}

fn passwd_home() -> Option<PathBuf> {
    // SAFETY: getuid cannot fail. getpwuid returns NULL or a pointer to
    // static storage, which is copied before any other passwd call.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() || (*pw).pw_dir.is_null() {
            return None;
        }
        let dir = CStr::from_ptr((*pw).pw_dir).to_str().ok()?;
        dir.starts_with('/').then(|| PathBuf::from(dir))
    }
}

impl Sources {
    /// Built-ins are found from this executable's own resolved path, never
    /// from the current directory or `PATH`: `<prefix>/bin/sv2` has them in
    /// `<prefix>/share/sandvault2/agents`. A build that was never installed
    /// has no such directory and uses the source tree it was built from.
    /// The user's home comes from the account, not from `HOME`.
    pub fn locate() -> Sources {
        let installed = std::env::current_exe()
            .and_then(|p| p.canonicalize())
            .ok()
            .and_then(|exe| Some(exe.parent()?.parent()?.join("share/sandvault2/agents")));
        let builtin = match installed {
            Some(dir) if dir.is_dir() => Some(dir),
            _ => {
                let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../agents");
                dev.is_dir().then_some(dev)
            }
        };
        Sources {
            user: passwd_home().map(|h| h.join(".config/sandvault2/agents")),
            builtin,
        }
    }

    fn dirs(&self) -> impl Iterator<Item = &PathBuf> {
        self.user.iter().chain(self.builtin.iter())
    }

    /// The names that have a definition. Only names are read here; a
    /// definition is examined when it is used.
    pub fn names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for dir in self.dirs() {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                if valid_agent_name(&name)
                    && entry.path().join("agent.toml").is_file()
                    && !names.contains(&name)
                {
                    names.push(name);
                }
            }
        }
        names.sort();
        names
    }

    /// Read the definition of `name`: the user's if there is one, else the
    /// built-in.
    pub fn load(&self, name: &str) -> Result<Definition, String> {
        if !valid_agent_name(name) {
            return Err(format!("unknown agent '{name}'"));
        }
        let dir = self
            .dirs()
            .map(|d| d.join(name))
            .find(|d| d.join("agent.toml").is_file())
            .ok_or_else(|| format!("unknown agent '{name}'"))?;
        let dir = dir
            .canonicalize()
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        refuse_if_sandbox_writable(&dir)?;
        let in_dir = |what: String| format!("agent definition {}: {what}", dir.display());
        let text = std::fs::read_to_string(dir.join("agent.toml"))
            .map_err(|e| in_dir(format!("agent.toml: {e}")))?;
        let read_script = |file: &str| -> Result<String, String> {
            let path = dir.join(file);
            let meta = path
                .symlink_metadata()
                .map_err(|e| format!("{file}: {e}"))?;
            if !meta.is_file() {
                return Err(format!("{file} is not a regular file"));
            }
            if meta.len() > MAX_SCRIPT_BYTES {
                return Err(format!(
                    "{file} is larger than {} KB",
                    MAX_SCRIPT_BYTES / 1024
                ));
            }
            std::fs::read_to_string(&path).map_err(|e| format!("{file}: {e}"))
        };
        Definition::parse(name, &dir, &text, &read_script).map_err(in_dir)
    }
}

// ---------------------------------------------------------------------------
// Who can write a definition
// ---------------------------------------------------------------------------

/// Account and group names that sandvault gives to sandboxes: v2's and v1's.
fn is_sandbox_principal(name: &str) -> bool {
    name.starts_with("sv2-") || name.starts_with("sandvault-")
}

fn user_name(uid: u32) -> Option<String> {
    // SAFETY: getpwuid returns NULL or a pointer to static storage, which
    // is copied before any other passwd call.
    unsafe {
        let pw = libc::getpwuid(uid);
        (!pw.is_null()).then(|| CStr::from_ptr((*pw).pw_name).to_string_lossy().into_owned())
    }
}

fn group_name(gid: u32) -> Option<String> {
    // SAFETY: as user_name, for the group database.
    unsafe {
        let gr = libc::getgrgid(gid);
        (!gr.is_null()).then(|| CStr::from_ptr((*gr).gr_name).to_string_lossy().into_owned())
    }
}

/// The rights in an ACL entry that let their holder change a directory's
/// entries, a file's content, or the permissions of either.
const WRITE_RIGHTS: [&str; 10] = [
    "write",
    "append",
    "add_file",
    "add_subdirectory",
    "delete",
    "delete_child",
    "writeattr",
    "writeextattr",
    "writesecurity",
    "chown",
];

/// The principal an ACL (in `acl_to_text` form) lets write, if it is a
/// sandbox account or group, or everyone. Lines read
/// `user:UUID:name:id:allow:right,right`.
fn acl_grants_sandbox_write(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let f: Vec<&str> = line.split(':').collect();
        let [_kind, _uuid, name, _id, verdict, rights] = f.as_slice() else {
            return None;
        };
        let writes = rights.split(',').any(|r| WRITE_RIGHTS.contains(&r));
        (verdict.starts_with("allow")
            && writes
            && (is_sandbox_principal(name) || *name == "everyone"))
            .then(|| (*name).to_string())
    })
}

/// Why a sandbox account could change `path` itself, if it could.
fn sandbox_can_write(path: &Path) -> Result<Option<String>, String> {
    let at = |e: std::io::Error| format!("cannot examine {}: {e}", path.display());
    let meta = path.symlink_metadata().map_err(at)?;
    // The directory was resolved, so only a file inside it can be a link.
    if meta.file_type().is_symlink() {
        return Ok(Some(
            "is a symbolic link, and what it points to is not examined".to_string(),
        ));
    }
    if meta.mode() & 0o002 != 0 {
        return Ok(Some("is writable by everyone".to_string()));
    }
    if let Some(owner) = user_name(meta.uid()).filter(|n| is_sandbox_principal(n)) {
        return Ok(Some(format!("belongs to {owner}")));
    }
    if meta.mode() & 0o020 != 0 {
        if let Some(group) = group_name(meta.gid()).filter(|n| is_sandbox_principal(n)) {
            return Ok(Some(format!("is writable by the group {group}")));
        }
    }
    let acl = crate::repair::acl_text_of(path).map_err(at)?;
    Ok(acl
        .as_deref()
        .and_then(acl_grants_sandbox_write)
        .map(|who| format!("has an access list that lets {who} write")))
}

/// Refuse a definition directory that a sandbox account can change: it, a
/// file in it, or any directory on the way to it. `dir` must be resolved
/// (no links, no `..`), so that the path examined is the path used.
///
/// Refused on sight: anything in a workspace or a sandbox home. There,
/// ownership proves nothing, because a sandbox can move the user's own
/// directories into place.
pub fn refuse_if_sandbox_writable(dir: &Path) -> Result<(), String> {
    let refuse = |why: &str| {
        Err(format!(
            "refusing the agent definition in {}: {why}, so a sandbox could change what the definition makes sv2 run on this Mac. Keep definitions where only you or an administrator can write. {}",
            dir.display(),
            Obelus("meddlesome ferret")
        ))
    };
    if !dir.is_absolute() || dir.components().any(|c| matches!(c, Component::ParentDir)) {
        return refuse("its path is not a resolved one");
    }
    if dir.starts_with(WORKSPACE_ROOT) {
        return refuse("it is inside a sandbox workspace");
    }
    let mut parts = dir.components().skip(1);
    if parts.next().is_some_and(|c| c.as_os_str() == "Users")
        && parts
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .is_some_and(is_sandbox_principal)
    {
        return refuse("it is inside a sandbox account's home");
    }

    let mut examine: Vec<PathBuf> = dir.ancestors().map(Path::to_path_buf).collect();
    examine.reverse();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        examine.push(entry.path());
    }
    for path in examine {
        if let Some(why) = sandbox_can_write(&path)? {
            return refuse(&format!("{} {why}", path.display()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    const CLAUDE: &str = r#"
schema = 1
binary = ["~/.local/bin/claude", "{brew}/bin/claude"]

[install.host]
brew = { cask = "claude-code" }

[install.guest]
script = "install.sh"

[run]
bypass_flag = "--dangerously-skip-permissions"
no_inject = ["update"]
"#;

    fn parse(text: &str) -> Result<Definition, String> {
        Definition::parse("claude", Path::new("/defs/claude"), text, &|name| {
            Ok(format!("# {name}\n"))
        })
    }

    #[test]
    fn a_whole_definition() {
        let d = parse(CLAUDE).unwrap();
        assert_eq!(
            d,
            Definition {
                name: "claude".into(),
                dir: "/defs/claude".into(),
                binary: vec![
                    Candidate::Home(".local/bin/claude".into()),
                    Candidate::Brew("bin/claude".into())
                ],
                brew: Some(Brew::Cask("claude-code".into())),
                guest_install: Some("# install.sh\n".into()),
                bypass_flag: Some("--dangerously-skip-permissions".into()),
                no_inject: vec!["update".into()],
            }
        );
    }

    #[test]
    fn the_smallest_definition() {
        let d = parse("schema = 1\nbinary = [\"/bin/echo\"]\n").unwrap();
        assert_eq!(d.binary, vec![Candidate::Absolute("bin/echo".into())]);
        assert_eq!(
            (d.brew, d.guest_install, d.bypass_flag, d.no_inject),
            (None, None, None, vec![])
        );
    }

    #[test]
    fn what_a_definition_may_not_say() {
        let with = |from: &str, to: &str| {
            assert!(CLAUDE.contains(from), "{from}");
            CLAUDE.replace(from, to)
        };
        let cases = [
            ("another schema", with("schema = 1", "schema = 2")),
            ("no schema", with("schema = 1", "")),
            (
                "a key sv2 does not know",
                with("schema = 1", "schema = 1\nwrapper = \"run.sh\""),
            ),
            (
                "an unknown key in a table",
                with("no_inject", "extra = 1\nno_inject"),
            ),
            (
                "no binary",
                with(
                    "binary = [\"~/.local/bin/claude\", \"{brew}/bin/claude\"]",
                    "binary = []",
                ),
            ),
            (
                "a relative binary",
                with("~/.local/bin/claude", "bin/claude"),
            ),
            (
                "a binary that climbs out",
                with("~/.local/bin/claude", "~/../../bin/sh"),
            ),
            (
                "a binary with a space",
                with("~/.local/bin/claude", "/bin/sh -c"),
            ),
            (
                "a binary under another user's home",
                with("~/.local/bin/claude", "~pat/bin/claude"),
            ),
            (
                "a tapped package",
                with("claude-code", "evil/tap/claude-code"),
            ),
            ("a package that is an option", with("claude-code", "--HEAD")),
            (
                "a formula and a cask",
                with("cask = \"claude-code\"", "cask = \"a\", formula = \"b\""),
            ),
            (
                "neither formula nor cask",
                with("cask = \"claude-code\"", ""),
            ),
            (
                "a script outside the directory",
                with("install.sh", "../install.sh"),
            ),
            (
                "a script in a subdirectory",
                with("install.sh", "sub/install.sh"),
            ),
            ("a script that is not .sh", with("install.sh", "agent.toml")),
            ("a definition that is not TOML", "schema: 1".to_string()),
            (
                "a short option",
                with("--dangerously-skip-permissions", "-y"),
            ),
            (
                "two options in one",
                with("--dangerously-skip-permissions", "--a --b"),
            ),
            (
                "an option with a value",
                with("--dangerously-skip-permissions", "--mode=x"),
            ),
            ("a subcommand with a slash", with("\"update\"", "\"a/b\"")),
        ];
        for (why, text) in cases {
            assert!(parse(&text).is_err(), "accepted: {why}");
        }
        // A script that cannot be read is the definition's error.
        let err = Definition::parse("claude", Path::new("/d"), CLAUDE, &|_| {
            Err("install.sh: gone".to_string())
        })
        .unwrap_err();
        assert_eq!(err, "install.sh: gone");
    }

    #[test]
    fn the_flag_goes_first_unless_the_subcommand_is_listed() {
        let d = parse(CLAUDE).unwrap();
        let flag = "--dangerously-skip-permissions";
        assert_eq!(d.final_args(&[]), os(&[flag]));
        assert_eq!(d.final_args(&os(&["-p", "hi"])), os(&[flag, "-p", "hi"]));
        assert_eq!(
            d.final_args(&os(&["mcp", "list"])),
            os(&[flag, "mcp", "list"])
        );
        // Listed: passed through untouched, and the banner must not claim
        // the flag.
        assert_eq!(d.final_args(&os(&["update"])), os(&["update"]));
        assert_eq!(d.injected_flag(&os(&["update"])), None);
        // Only the first argument is a subcommand.
        assert_eq!(
            d.final_args(&os(&["-p", "update"])),
            os(&[flag, "-p", "update"])
        );
        // No flag in the definition: nothing is added.
        let plain = parse("schema = 1\nbinary = [\"/bin/echo\"]\n").unwrap();
        assert_eq!(plain.final_args(&os(&["a"])), os(&["a"]));
    }

    #[test]
    fn candidate_paths_by_situation() {
        let d = parse(CLAUDE).unwrap();
        let home = "/Users/sv2-pat-work";
        assert_eq!(
            d.candidate_paths(home, Some("/opt/homebrew"), false),
            [
                "/Users/sv2-pat-work/.local/bin/claude",
                "/opt/homebrew/bin/claude"
            ]
        );
        // Without Homebrew its candidates do not exist.
        assert_eq!(
            d.candidate_paths(home, None, false),
            ["/Users/sv2-pat-work/.local/bin/claude"]
        );
        // --native: only where a native installer puts things.
        assert_eq!(
            d.candidate_paths(home, Some("/opt/homebrew"), true),
            ["/Users/sv2-pat-work/.local/bin/claude"]
        );
        assert_eq!(
            d.host_visible_paths(Some("/opt/homebrew")),
            ["/opt/homebrew/bin/claude"]
        );
        assert!(d.has_home_candidate());
    }

    #[test]
    fn the_homebrew_command_is_a_vector() {
        assert_eq!(
            brew_install_argv("/opt/homebrew", &Brew::Cask("claude-code".into())),
            ["/opt/homebrew/bin/brew", "install", "--cask", "claude-code"]
        );
        assert_eq!(
            brew_install_argv("/usr/local", &Brew::Formula("codex".into())),
            ["/usr/local/bin/brew", "install", "--formula", "codex"]
        );
    }

    #[test]
    fn quarantine_flags_are_the_first_field_in_hexadecimal() {
        assert_eq!(
            quarantine_flags("0181;6b07f2d0;Homebrew Cask;A689"),
            Some(0x181)
        );
        assert_eq!(quarantine_flags("01c1;6b07f2d0;;"), Some(0x1c1));
        assert_eq!(quarantine_flags("0041"), Some(0x41));
        for bad in ["", ";0181", "zz81;x", "0181 ;x", "123456789;x"] {
            assert_eq!(quarantine_flags(bad), None, "{bad:?}");
        }
    }

    /// The combinations measured in a session: only a quarantined program
    /// that is neither approved nor notarized is stopped.
    #[test]
    fn what_gatekeeper_stops() {
        let stopped = |attr: Option<&str>, notarized: bool| would_be_stopped(attr, || notarized);
        for flags in ["0181", "0081", "0001", "0281", "0201", "0101"] {
            let attr = format!("{flags};6b07f2d0;Homebrew Cask;A689");
            assert!(stopped(Some(&attr), false), "{flags}");
            assert!(!stopped(Some(&attr), true), "{flags}, notarized");
        }
        for flags in ["0041", "01c1"] {
            let attr = format!("{flags};6b07f2d0;Homebrew Cask;A689");
            assert!(!stopped(Some(&attr), false), "{flags}");
        }
        assert!(!stopped(None, false));
        // Unreadable flags count as not approved.
        assert!(stopped(Some("garbage"), false));
        assert!(!stopped(Some("garbage"), true));
        // The signature is not examined when the answer is already known.
        assert!(!would_be_stopped(None, || panic!("examined")));
        assert!(!would_be_stopped(Some("0041;x"), || panic!("examined")));
    }

    /// Against real files and the real codesign: a copy of this test
    /// program, which nobody has notarized.
    #[test]
    fn gatekeeper_check_on_real_files() {
        let t = Tree::new("quarantine");
        let file = t.0.join("program");
        std::fs::copy(std::env::current_exe().unwrap(), &file).unwrap();
        let link = t.0.join("link");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let set = |value: &str| {
            let ok = std::process::Command::new("/usr/bin/xattr")
                .args(["-w", "com.apple.quarantine", value])
                .arg(&file)
                .status()
                .unwrap()
                .success();
            assert!(ok, "could not set the attribute");
        };

        assert_eq!(gatekeeper_would_stop(&file), None);
        set("0181;6b07f2d0;Homebrew Cask;A689CCAF-9023-478E-A715-93E029978479");
        assert_eq!(gatekeeper_would_stop(&file), Some(file.clone()));
        // Through a link, the file it leads to is named.
        assert_eq!(gatekeeper_would_stop(&link), Some(file.clone()));
        set("01c1;6b07f2d0;Homebrew Cask;A689CCAF-9023-478E-A715-93E029978479");
        assert_eq!(gatekeeper_would_stop(&file), None);
        assert_eq!(gatekeeper_would_stop(&t.0.join("missing")), None);
        // An Apple program has no such attribute.
        assert_eq!(gatekeeper_would_stop(Path::new("/bin/ls")), None);
    }

    #[test]
    fn acl_entries_that_let_a_sandbox_write() {
        let acl = |entry: &str| format!("!#acl 1\n{entry}\n");
        let grants = |entry: &str| acl_grants_sandbox_write(&acl(entry));
        assert_eq!(
            grants("user:AAAA:sv2-pat-work:602:allow,file_inherit:read,write"),
            Some("sv2-pat-work".into())
        );
        assert_eq!(
            grants("group:BBBB:sandvault-pat:600:allow:add_file,delete_child"),
            Some("sandvault-pat".into())
        );
        assert_eq!(
            grants("group:CCCC:everyone:12:allow:writesecurity"),
            Some("everyone".into())
        );
        // Reading is not writing; a deny entry grants nothing; the host
        // user and an administrators' group are not sandboxes.
        assert_eq!(
            grants("user:AAAA:sv2-pat-work:602:allow:read,execute"),
            None
        );
        assert_eq!(grants("user:AAAA:sv2-pat-work:602:deny:write"), None);
        assert_eq!(grants("user:DDDD:pat:501:allow:write"), None);
        assert_eq!(grants("group:EEEE:admin:80:allow:write,delete_child"), None);
        assert_eq!(acl_grants_sandbox_write("!#acl 1\n"), None);
    }

    /// A scratch tree for definitions. It is in the per-user temporary
    /// directory, which nobody else can write.
    struct Tree(PathBuf);

    impl Tree {
        fn new(tag: &str) -> Tree {
            let root =
                std::env::temp_dir().join(format!("sv2-agent-test-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Tree(root.canonicalize().unwrap())
        }

        fn definition(&self, at: &str, toml: &str) -> PathBuf {
            let dir = self.0.join(at);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("agent.toml"), toml).unwrap();
            std::fs::write(dir.join("install.sh"), "#!/bin/bash\necho installed\n").unwrap();
            dir
        }

        fn sources(&self) -> Sources {
            Sources {
                user: Some(self.0.join("user")),
                builtin: Some(self.0.join("builtin")),
            }
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn chmod(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn a_users_definition_replaces_the_builtin_whole() {
        let t = Tree::new("precedence");
        t.definition("builtin/claude", CLAUDE);
        t.definition("builtin/other", "schema = 1\nbinary = [\"/bin/echo\"]\n");
        let mine = t.definition("user/claude", "schema = 1\nbinary = [\"/bin/cat\"]\n");
        // Not definitions: no agent.toml, or not a name.
        std::fs::create_dir_all(t.0.join("user/empty")).unwrap();
        t.definition("user/Not_A_Name", CLAUDE);

        let s = t.sources();
        assert_eq!(s.names(), ["claude", "other"]);
        let d = s.load("claude").unwrap();
        assert_eq!(d.dir, mine);
        assert_eq!(d.binary, vec![Candidate::Absolute("bin/cat".into())]);
        // Nothing of the built-in shows through: no installer, no flag.
        assert_eq!((d.guest_install, d.bypass_flag), (None, None));

        assert_eq!(s.load("other").unwrap().dir, t.0.join("builtin/other"));
        for missing in ["nothere", "empty", "../builtin/other", "Not_A_Name", ""] {
            let err = s.load(missing).unwrap_err();
            assert!(err.contains("unknown agent"), "{missing}: {err}");
        }
    }

    #[test]
    fn a_definition_reads_its_script_and_reports_its_own_errors() {
        let t = Tree::new("load");
        t.definition("builtin/claude", CLAUDE);
        let s = t.sources();
        assert_eq!(
            s.load("claude").unwrap().guest_install.as_deref(),
            Some("#!/bin/bash\necho installed\n")
        );

        let dir = t.definition("builtin/broken", "schema = 1\nbinary = 5\n");
        let err = s.load("broken").unwrap_err();
        assert!(err.contains(dir.to_str().unwrap()), "{err}");

        // The script is a link: it could point anywhere.
        let dir = t.definition("builtin/linked", CLAUDE);
        std::fs::remove_file(dir.join("install.sh")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", dir.join("install.sh")).unwrap();
        let err = s.load("linked").unwrap_err();
        assert!(err.contains("install.sh is a symbolic link"), "{err}");
    }

    #[test]
    fn a_definition_a_sandbox_could_write_is_refused() {
        let t = Tree::new("writable");
        let dir = t.definition("builtin/claude", CLAUDE);
        let s = t.sources();
        s.load("claude").unwrap();
        let refused = |why: &str| {
            let err = s.load("claude").unwrap_err();
            assert!(err.contains("refusing the agent definition"), "{err}");
            assert!(err.contains(why), "expected '{why}' in: {err}");
        };

        // World-writable: the directory, a directory above it, a file in it.
        for path in [
            dir.clone(),
            t.0.join("builtin"),
            dir.join("agent.toml"),
            dir.join("install.sh"),
        ] {
            let mode = if path.is_dir() { 0o755 } else { 0o644 };
            chmod(&path, mode | 0o002);
            refused("is writable by everyone");
            chmod(&path, mode);
            s.load("claude").unwrap();
        }
        // Sticky does not excuse it: this is what /tmp and /Users/Shared are.
        chmod(&t.0.join("builtin"), 0o1777);
        refused("is writable by everyone");
        chmod(&t.0.join("builtin"), 0o755);

        // Group-writable is fine when the group is not a sandbox's (the
        // Homebrew prefix is like this).
        chmod(&dir, 0o775);
        s.load("claude").unwrap();
    }

    #[test]
    fn workspaces_and_sandbox_homes_are_refused_by_where_they_are() {
        for (path, why) in [
            (
                "/Users/Shared/sv2/pat/work/agents/claude",
                "sandbox workspace",
            ),
            (
                "/Users/sv2-pat-work/.config/sandvault2/agents/claude",
                "sandbox account's home",
            ),
            (
                "/Users/sandvault-pat/agents/claude",
                "sandbox account's home",
            ),
            ("relative/agents/claude", "not a resolved one"),
            ("/Users/pat/../sv2-pat-work/claude", "not a resolved one"),
        ] {
            let err = refuse_if_sandbox_writable(Path::new(path)).unwrap_err();
            assert!(err.contains(why), "{path}: {err}");
        }
    }

    #[test]
    fn names() {
        for ok in ["shell", "claude", "open-code", "0x"] {
            assert!(valid_agent_name(ok), "{ok}");
        }
        for bad in ["", "-x", "Shell", "a_b", "a/b", "a.b", "a b"] {
            assert!(!valid_agent_name(bad), "{bad:?}");
        }
    }
}
