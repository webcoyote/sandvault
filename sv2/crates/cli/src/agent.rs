//! Agent definitions: what `NAME` in `sv2 run NAME` stands for.
//!
//! A definition is a directory holding `agent.toml` and, optionally, shell
//! scripts that run inside the sandbox:
//!
//! ```toml
//! schema = 1
//!
//! [install.host]            # the program as sv2 installs it on this Mac
//! brew = { cask = "claude-code" }       # data only: sv2 builds the command
//! binary = ["{brew}/bin/claude"]        # where that leaves it
//!
//! [install.guest]           # the program as its own installer installs it
//! script = "install.sh"                 # run inside the sandbox
//! binary = ["~/.local/bin/claude"]      # where that leaves it
//!
//! [run]
//! bypass_flag = "--dangerously-skip-permissions"
//! grants = "it acts without asking"   # said in the banner with the flag
//! no_inject = ["update"]      # subcommands that do not get the flag
//! inject_last = ["config"]    # subcommands that get it after their arguments
//! system_prompt = { flag = "--append-system-prompt" }   # how it takes text
//! # or: system_prompt = { config = { flag = "--config", key = "run.x" } }
//! env = { NAME = "value" }    # set for the agent
//! wrapper = "run.sh"          # runs in the sandbox, and runs the agent
//! ```
//!
//! The two installs are separate, and a run uses one of them: the host's
//! by default, the sandbox's own with `--native`. They are never mixed, so
//! which program runs does not depend on which happens to be present.
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
    brew: Option<BrewFile>,
    binary: Vec<String>,
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
    script: Option<String>,
    binary: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunFile {
    bypass_flag: Option<String>,
    grants: Option<String>,
    #[serde(default)]
    no_inject: Vec<String>,
    #[serde(default)]
    inject_last: Vec<String>,
    #[serde(default)]
    env: std::collections::BTreeMap<String, String>,
    system_prompt: Option<SystemPromptFile>,
    wrapper: Option<String>,
}

/// How the agent takes text for its system prompt: exactly one of the keys.
/// A table, so that another way is a new key here and not a new shape.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemPromptFile {
    flag: Option<String>,
    config: Option<SettingFile>,
}

/// A setting given on the command line: `--config KEY=VALUE` (muse).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingFile {
    flag: String,
    key: String,
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
    /// Where the host's install leaves the program: below the Homebrew
    /// prefix, or absolute. Empty when the definition has no host install.
    pub host_binary: Vec<Candidate>,
    pub brew: Option<Brew>,
    /// Where the program's own installer leaves it, in the sandbox home.
    /// Empty when the definition has no install of its own.
    pub guest_binary: Vec<Candidate>,
    /// The text of the guest install script.
    pub guest_install: Option<String>,
    pub bypass_flag: Option<String>,
    /// What the flag and the environment below let the agent do, in the
    /// definition's own words, for the banner.
    pub grants: Option<String>,
    pub no_inject: Vec<String>,
    pub inject_last: Vec<String>,
    /// Set for the agent, in name order.
    pub env: Vec<(String, String)>,
    /// How the agent takes text for its system prompt. sv2 tells the agent
    /// about its session this way.
    pub prompt: Option<Prompt>,
    /// The name and text of the wrapper script: it runs in the sandbox in
    /// place of the agent, is given the agent's path and its arguments,
    /// and is expected to run it.
    pub wrapper: Option<(String, String)>,
}

/// How an agent takes text for its system prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prompt {
    /// An option whose value is the text: `--append-system-prompt TEXT`.
    Flag(String),
    /// An option whose value sets one setting, the text as a JSON string:
    /// `--config KEY="TEXT"`.
    Setting { flag: String, key: String },
}

impl Prompt {
    /// The option that comes before the value.
    pub fn flag(&self) -> &str {
        match self {
            Prompt::Flag(flag) | Prompt::Setting { flag, .. } => flag,
        }
    }

    /// The option's value that carries `text`.
    fn value(&self, text: &str) -> String {
        match self {
            Prompt::Flag(_) => text.to_string(),
            Prompt::Setting { key, .. } => format!("{key}={}", json_string(text)),
        }
    }
}

/// `text` as a JSON string. Every control character is escaped, so the
/// result is also a valid TOML basic string.
fn json_string(text: &str) -> String {
    let mut s = String::with_capacity(text.len() + 2);
    s.push('"');
    for c in text.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                s.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

/// A setting's name: lower case words joined by `.`, `_` or `-`.
fn valid_setting_key(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(&c))
}

/// Where the flag goes on one command line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag<'a> {
    /// Before the user's arguments.
    First(&'a str),
    /// After them: some programs know a subcommand only as the first word.
    Last(&'a str),
    /// Not at all.
    None,
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

/// Homebrew package names as Homebrew writes them: `name`, or
/// `user/repository/name` for a package from a tap.
fn valid_package(s: &str) -> bool {
    let part = |p: &str| {
        let b = p.as_bytes();
        (1..=64).contains(&b.len())
            && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
            && b.iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"+._@-".contains(c))
    };
    let parts: Vec<&str> = s.split('/').collect();
    matches!(parts.len(), 1 | 3) && parts.iter().all(|p| part(p))
}

/// What the helper sets for every session, and what sv2 forwards from the
/// terminal. A definition does not get to change these.
const RESERVED_ENV: [&str; 7] = [
    "HOME", "USER", "LOGNAME", "SHELL", "PATH", "TMPDIR", "TERMINFO",
];

fn valid_env_name(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=64).contains(&b.len())
        && b[0].is_ascii_uppercase()
        && b.iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'_')
        && !RESERVED_ENV.contains(&s)
        && !s.starts_with("SV2_")
        && !crate::shell::FORWARDED.contains(&s)
}

/// Printable ASCII: it is put on a command line or printed to a terminal.
fn plain_text(s: &str, max: usize) -> bool {
    s.len() <= max && s.bytes().all(|b| (0x20..0x7f).contains(&b))
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
        let paths = |list: &[String], what: &str, in_home: bool| {
            if list.is_empty() || list.len() > 8 {
                return Err(format!("{what} must list between 1 and 8 paths"));
            }
            list.iter()
                .map(|s| match parse_candidate(s)? {
                    c @ Candidate::Home(_) if in_home => Ok(c),
                    c @ (Candidate::Brew(_) | Candidate::Absolute(_)) if !in_home => Ok(c),
                    _ if in_home => Err(format!(
                        "'{s}' in {what}: the program's own installer puts it in the sandbox home, so the path starts with ~/"
                    )),
                    _ => Err(format!(
                        "'{s}' in {what}: the host's install is outside the sandbox home, so the path starts with {{brew}}/ or /"
                    )),
                })
                .collect::<Result<Vec<_>, String>>()
        };
        if file.install.host.is_none() && file.install.guest.is_none() {
            return Err(
                "it has neither [install.host] nor [install.guest], so there is no program to run"
                    .to_string(),
            );
        }
        let (host_binary, brew) = match file.install.host {
            None => (Vec::new(), None),
            Some(host) => {
                let binary = paths(&host.binary, "install.host.binary", false)?;
                let brew = match host.brew {
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
                (binary, brew)
            }
        };
        if let Some(Brew::Formula(p) | Brew::Cask(p)) = &brew {
            if !valid_package(p) {
                return Err(format!("'{p}' is not a plain Homebrew package name"));
            }
        }
        let script = |key: &str, name: &str| {
            if valid_script_name(name) {
                read_script(name)
            } else {
                Err(format!(
                    "{key} '{name}' must be the name of a .sh file in the definition's directory"
                ))
            }
        };
        let (guest_binary, guest_install) = match file.install.guest {
            None => (Vec::new(), None),
            Some(guest) => {
                let binary = paths(&guest.binary, "install.guest.binary", true)?;
                let text = match guest.script {
                    None => None,
                    Some(name) => Some(script("script", &name)?),
                };
                (binary, text)
            }
        };
        let wrapper = match file.run.wrapper {
            None => None,
            Some(name) => {
                let text = script("wrapper", &name)?;
                Some((name, text))
            }
        };
        if let Some(flag) = &file.run.bypass_flag {
            if !valid_flag(flag) {
                return Err(format!("bypass_flag '{flag}' is not a long option"));
            }
        }
        for list in [&file.run.no_inject, &file.run.inject_last] {
            if list.len() > 64 || !list.iter().all(|s| valid_agent_name(s)) {
                return Err(
                    "no_inject and inject_last must list plain subcommand names".to_string()
                );
            }
        }
        if let Some(both) = file
            .run
            .no_inject
            .iter()
            .find(|s| file.run.inject_last.contains(s))
        {
            return Err(format!("'{both}' is in both no_inject and inject_last"));
        }
        if !file.run.inject_last.is_empty() && file.run.bypass_flag.is_none() {
            return Err("inject_last needs a bypass_flag to place".to_string());
        }
        let prompt = match file.run.system_prompt {
            None => None,
            Some(SystemPromptFile {
                flag: Some(flag),
                config: None,
            }) => Some(Prompt::Flag(flag)),
            Some(SystemPromptFile {
                flag: None,
                config: Some(SettingFile { flag, key }),
            }) => {
                if !valid_setting_key(&key) {
                    return Err(format!(
                        "system_prompt config key '{key}' must be lower case words joined by '.', '_' or '-'"
                    ));
                }
                Some(Prompt::Setting { flag, key })
            }
            Some(_) => {
                return Err("system_prompt needs one of flag or config".to_string());
            }
        };
        if let Some(flag) = prompt.as_ref().map(Prompt::flag) {
            if !valid_flag(flag) {
                return Err(format!("system_prompt flag '{flag}' is not a long option"));
            }
            if file.run.bypass_flag.as_deref() == Some(flag) {
                return Err(format!(
                    "'{flag}' is both bypass_flag and the system_prompt flag"
                ));
            }
        }
        if let Some(text) = &file.run.grants {
            if text.is_empty() || !plain_text(text, 200) {
                return Err("grants must be a short line of plain text".to_string());
            }
        }
        if file.run.env.len() > 16 {
            return Err("env may set at most 16 variables".to_string());
        }
        for (name, value) in &file.run.env {
            if !valid_env_name(name) {
                return Err(format!(
                    "env may not set '{name}': names are upper case, and those sv2 sets itself are taken"
                ));
            }
            if !plain_text(value, 1024) {
                return Err(format!("the value of {name} must be plain text"));
            }
        }
        Ok(Definition {
            name: name.to_string(),
            dir: dir.to_path_buf(),
            host_binary,
            brew,
            guest_binary,
            guest_install,
            bypass_flag: file.run.bypass_flag,
            grants: file.run.grants,
            no_inject: file.run.no_inject,
            inject_last: file.run.inject_last,
            env: file.run.env.into_iter().collect(),
            prompt,
            wrapper,
        })
    }

    /// Whether the first of `args` is a subcommand in `list`.
    fn first_is_in(list: &[String], args: &[OsString]) -> bool {
        args.first()
            .and_then(|a| a.to_str())
            .is_some_and(|first| list.iter().any(|n| n == first))
    }

    /// Where the flag goes for `args`: nowhere when the first argument is a
    /// subcommand listed in `no_inject`, after the arguments when it is in
    /// `inject_last`, otherwise in front.
    pub fn flag_for(&self, args: &[OsString]) -> Flag<'_> {
        let Some(flag) = self.bypass_flag.as_deref() else {
            return Flag::None;
        };
        if Self::first_is_in(&self.no_inject, args) {
            Flag::None
        } else if Self::first_is_in(&self.inject_last, args) {
            Flag::Last(flag)
        } else {
            Flag::First(flag)
        }
    }

    /// How text for the agent's system prompt goes on this command line.
    /// None when the definition names no way to give it, and for a
    /// subcommand in either list: those lists name commands that manage the
    /// program, which have no system prompt, and a subcommand that takes the
    /// bypass flag only after its arguments would not know this one in
    /// front of them.
    pub fn prompt_for(&self, args: &[OsString]) -> Option<&Prompt> {
        let prompt = self.prompt.as_ref()?;
        let listed =
            Self::first_is_in(&self.no_inject, args) || Self::first_is_in(&self.inject_last, args);
        (!listed).then_some(prompt)
    }

    /// The agent's arguments as they are run. `told` is what sv2 has to say
    /// to the agent about its session; it travels as one argument after the
    /// option that takes it, as the definition says (as it is, or as a
    /// setting's JSON value), whatever is in it.
    pub fn final_args(&self, args: &[OsString], told: Option<&str>) -> Vec<OsString> {
        let mut v: Vec<OsString> = Vec::new();
        let bypass = self.flag_for(args);
        if let Flag::First(flag) = bypass {
            v.push(flag.into());
        }
        if let (Some(prompt), Some(text)) = (self.prompt_for(args), told) {
            if !text.is_empty() {
                v.extend([prompt.flag().into(), prompt.value(text).into()]);
            }
        }
        v.extend(args.iter().cloned());
        if let Flag::Last(flag) = bypass {
            v.push(flag.into());
        }
        v
    }

    /// `NAME=value` for each variable the definition sets.
    pub fn env_pairs(&self) -> Vec<OsString> {
        self.env
            .iter()
            .map(|(name, value)| format!("{name}={value}").into())
            .collect()
    }

    /// What this run is trusted with, for the banner: the flag (when this
    /// command line gets it) and the variables, then the definition's own
    /// description of what they allow. None when the agent runs plain.
    pub fn trust_line(&self, args: &[OsString]) -> Option<String> {
        let mut with: Vec<String> = Vec::new();
        if let Flag::First(flag) | Flag::Last(flag) = self.flag_for(args) {
            with.push(flag.to_string());
        }
        with.extend(self.env.iter().map(|(n, v)| format!("{n}={v}")));
        if with.is_empty() {
            return None;
        }
        let what = with.join(" and ");
        Some(match &self.grants {
            Some(grants) => format!("{what} ({grants})"),
            None => what,
        })
    }

    /// Whether a run uses the program's own install in the sandbox: with
    /// --native, or when the definition has no other (gemini).
    pub fn uses_own_install(&self, native: bool) -> bool {
        native || self.host_binary.is_empty()
    }

    /// Where the host's install may have left the program, in order. Those
    /// below the Homebrew prefix are left out on a Mac without Homebrew.
    pub fn host_paths(&self, brew: Option<&str>) -> Vec<String> {
        self.host_binary
            .iter()
            .filter_map(|c| match c {
                Candidate::Brew(rest) => brew.map(|b| format!("{b}/{rest}")),
                Candidate::Absolute(rest) => Some(format!("/{rest}")),
                Candidate::Home(_) => None,
            })
            .collect()
    }

    /// Where the program's own installer may have left it, in the sandbox
    /// home `home`, in order.
    pub fn guest_paths(&self, home: &str) -> Vec<String> {
        self.guest_binary
            .iter()
            .filter_map(|c| match c {
                Candidate::Home(rest) => Some(format!("{home}/{rest}")),
                _ => None,
            })
            .collect()
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

use crate::passwd_home;

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
// New definitions
// ---------------------------------------------------------------------------

/// What `sv2 agent new NAME` writes: every key, explained, with the
/// optional ones commented out. `{name}` stands for the agent's name.
const TEMPLATE: &str = include_str!("agent-template.toml");

fn template_for(name: &str) -> String {
    TEMPLATE.replace("{name}", name)
}

fn make_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(0o755).create(path)
}

impl Sources {
    /// Create the user's definition of `name`: from the template, or as a
    /// copy of the definition `from` (a built-in, to change it, or another
    /// of the user's). Returns the new directory. An existing definition
    /// is never replaced.
    pub fn create(&self, name: &str, from: Option<&str>) -> Result<PathBuf, String> {
        if !valid_agent_name(name) {
            return Err(format!(
                "'{name}' cannot be an agent's name: use a-z, 0-9 and '-', starting with a letter or digit"
            ));
        }
        let user = self
            .user
            .as_ref()
            .ok_or("cannot find your home directory")?;
        // What goes into the new directory, read before anything is made.
        let files: Vec<(String, Vec<u8>)> = match from {
            None => vec![("agent.toml".to_string(), template_for(name).into_bytes())],
            Some(other) => {
                // Checked and parsed like any definition that is used.
                let source = self.load(other)?.dir;
                let mut files = Vec::new();
                let entries =
                    std::fs::read_dir(&source).map_err(|e| format!("{}: {e}", source.display()))?;
                for entry in entries {
                    let entry = entry.map_err(|e| format!("{}: {e}", source.display()))?;
                    let Ok(file) = entry.file_name().into_string() else {
                        continue;
                    };
                    // The definition and its scripts; nothing hidden.
                    if file.starts_with('.') || !entry.path().is_file() {
                        continue;
                    }
                    let bytes = std::fs::read(entry.path())
                        .map_err(|e| format!("{}: {e}", entry.path().display()))?;
                    files.push((file, bytes));
                }
                files
            }
        };
        std::fs::create_dir_all(user).map_err(|e| format!("{}: {e}", user.display()))?;
        let dir = user.join(name);
        match make_dir(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(format!(
                    "{} already exists; edit it, or remove it first",
                    dir.display()
                ))
            }
            Err(e) => return Err(format!("{}: {e}", dir.display())),
        }
        for (file, bytes) in files {
            std::fs::write(dir.join(&file), bytes)
                .map_err(|e| format!("{}: {e}", dir.join(&file).display()))?;
        }
        // Say so now if sv2 would refuse to read what it has just written
        // (a config directory that others can write, for one).
        let resolved = dir
            .canonicalize()
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        refuse_if_sandbox_writable(&resolved)?;
        Ok(dir)
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

[install.host]
brew = { cask = "claude-code" }
binary = ["{brew}/bin/claude"]

[install.guest]
script = "install.sh"
binary = ["~/.local/bin/claude"]

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
                host_binary: vec![Candidate::Brew("bin/claude".into())],
                brew: Some(Brew::Cask("claude-code".into())),
                guest_binary: vec![Candidate::Home(".local/bin/claude".into())],
                guest_install: Some("# install.sh\n".into()),
                bypass_flag: Some("--dangerously-skip-permissions".into()),
                grants: None,
                no_inject: vec!["update".into()],
                inject_last: vec![],
                env: vec![],
                prompt: None,
                wrapper: None,
            }
        );
    }

    #[test]
    fn a_definition_with_a_wrapper() {
        let d = parse(&CLAUDE.replace("no_inject", "wrapper = \"run.sh\"\nno_inject")).unwrap();
        assert_eq!(
            d.wrapper,
            Some(("run.sh".to_string(), "# run.sh\n".to_string()))
        );
        // The rest is read as without it.
        assert_eq!(Definition { wrapper: None, ..d }, parse(CLAUDE).unwrap());
        // A wrapper sv2 cannot read stops the definition.
        let unreadable = Definition::parse(
            "claude",
            Path::new("/defs/claude"),
            &CLAUDE.replace("no_inject", "wrapper = \"run.sh\"\nno_inject"),
            &|name| match name {
                "run.sh" => Err(format!("{name}: No such file or directory")),
                _ => Ok(String::new()),
            },
        );
        assert_eq!(unreadable.unwrap_err(), "run.sh: No such file or directory");
    }

    #[test]
    fn the_smallest_definition() {
        let d = parse("schema = 1\n[install.host]\nbinary = [\"/bin/echo\"]\n").unwrap();
        assert_eq!(d.host_binary, vec![Candidate::Absolute("bin/echo".into())]);
        assert!(d.guest_binary.is_empty());
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
                with("schema = 1", "schema = 1\ncolour = \"red\""),
            ),
            (
                "a wrapper outside [run]",
                with("schema = 1", "schema = 1\nwrapper = \"run.sh\""),
            ),
            (
                "a wrapper outside the directory",
                with("no_inject", "wrapper = \"../run.sh\"\nno_inject"),
            ),
            (
                "a wrapper that is not .sh",
                with("no_inject", "wrapper = \"agent.toml\"\nno_inject"),
            ),
            (
                "two wrappers",
                with("no_inject", "wrapper = [\"a.sh\", \"b.sh\"]\nno_inject"),
            ),
            (
                "an unknown key in a table",
                with("no_inject", "extra = 1\nno_inject"),
            ),
            (
                "no path for the host's install",
                with("binary = [\"{brew}/bin/claude\"]", "binary = []"),
            ),
            (
                "an install with no path",
                with("binary = [\"~/.local/bin/claude\"]\n", ""),
            ),
            (
                "a sandbox path for the host's install",
                with("{brew}/bin/claude", "~/.local/bin/claude"),
            ),
            (
                "a host path for the sandbox's install",
                with("~/.local/bin/claude", "/opt/homebrew/bin/claude"),
            ),
            (
                "a Homebrew path for the sandbox's install",
                with("~/.local/bin/claude", "{brew}/bin/claude"),
            ),
            (
                "no install at all",
                "schema = 1\n[run]\nbypass_flag = \"--yes\"\n".to_string(),
            ),
            (
                "the old single list",
                with("schema = 1", "schema = 1\nbinary = [\"/bin/echo\"]"),
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
            ("half a tap", with("claude-code", "tap/claude-code")),
            ("a package with a path", with("claude-code", "a/b/c/d")),
            (
                "a package that climbs",
                with("claude-code", "../tap/claude-code"),
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
            (
                "a subcommand in both lists",
                with(
                    "no_inject = [\"update\"]",
                    "no_inject = [\"update\"]\ninject_last = [\"update\"]",
                ),
            ),
            (
                "PATH in env",
                with("no_inject", "env = { PATH = \"/evil\" }\nno_inject"),
            ),
            (
                "HOME in env",
                with("no_inject", "env = { HOME = \"/evil\" }\nno_inject"),
            ),
            (
                "a forwarded variable in env",
                with("no_inject", "env = { TERM = \"x\" }\nno_inject"),
            ),
            (
                "an SV2_ variable in env",
                with("no_inject", "env = { SV2_PROFILE = \"x\" }\nno_inject"),
            ),
            (
                "a lower-case variable",
                with("no_inject", "env = { path = \"x\" }\nno_inject"),
            ),
            (
                "a variable name with =",
                with("no_inject", "env = { \"A=B\" = \"x\" }\nno_inject"),
            ),
            (
                "a value with a newline",
                with("no_inject", "env = { A = \"x\\ny\" }\nno_inject"),
            ),
            (
                "grants with an escape",
                with("no_inject", "grants = \"\\u001b[31m\"\nno_inject"),
            ),
            (
                "a system prompt that is not a table",
                with("no_inject", "system_prompt = \"--append\"\nno_inject"),
            ),
            (
                "a system prompt with no flag",
                with("no_inject", "system_prompt = {}\nno_inject"),
            ),
            (
                "a system prompt given some other way",
                with(
                    "no_inject",
                    "system_prompt = { flag = \"--append\", file = \"x\" }\nno_inject",
                ),
            ),
            (
                "a system prompt flag that is a short option",
                with("no_inject", "system_prompt = { flag = \"-s\" }\nno_inject"),
            ),
            (
                "a system prompt flag with a value",
                with(
                    "no_inject",
                    "system_prompt = { flag = \"--append=x\" }\nno_inject",
                ),
            ),
            (
                "one flag for both",
                with(
                    "no_inject",
                    "system_prompt = { flag = \"--dangerously-skip-permissions\" }\nno_inject",
                ),
            ),
            (
                "a system prompt both ways",
                with(
                    "no_inject",
                    "system_prompt = { flag = \"--a\", config = { flag = \"--config\", key = \"k\" } }\nno_inject",
                ),
            ),
            (
                "a setting with no key",
                with(
                    "no_inject",
                    "system_prompt = { config = { flag = \"--config\" } }\nno_inject",
                ),
            ),
            (
                "a setting with a short option",
                with(
                    "no_inject",
                    "system_prompt = { config = { flag = \"-c\", key = \"k\" } }\nno_inject",
                ),
            ),
            (
                "a setting key with an equals sign",
                with(
                    "no_inject",
                    "system_prompt = { config = { flag = \"--config\", key = \"k=x\" } }\nno_inject",
                ),
            ),
            (
                "a setting key with a space",
                with(
                    "no_inject",
                    "system_prompt = { config = { flag = \"--config\", key = \"a b\" } }\nno_inject",
                ),
            ),
            (
                "an empty setting key",
                with(
                    "no_inject",
                    "system_prompt = { config = { flag = \"--config\", key = \"\" } }\nno_inject",
                ),
            ),
            (
                "a setting with something else",
                with(
                    "no_inject",
                    "system_prompt = { config = { flag = \"--config\", key = \"k\", x = 1 } }\nno_inject",
                ),
            ),
            (
                "a setting by the bypass flag",
                with(
                    "no_inject",
                    "system_prompt = { config = { flag = \"--dangerously-skip-permissions\", key = \"k\" } }\nno_inject",
                ),
            ),
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
        assert_eq!(d.final_args(&[], None), os(&[flag]));
        assert_eq!(
            d.final_args(&os(&["-p", "hi"]), None),
            os(&[flag, "-p", "hi"])
        );
        assert_eq!(
            d.final_args(&os(&["mcp", "list"]), None),
            os(&[flag, "mcp", "list"])
        );
        // Listed: passed through untouched, and the banner must not claim
        // the flag.
        assert_eq!(d.final_args(&os(&["update"]), None), os(&["update"]));
        assert_eq!(d.flag_for(&os(&["update"])), Flag::None);
        assert_eq!(d.trust_line(&os(&["update"])), None);
        assert_eq!(d.trust_line(&[]).as_deref(), Some(flag));
        // Only the first argument is a subcommand.
        assert_eq!(
            d.final_args(&os(&["-p", "update"]), None),
            os(&[flag, "-p", "update"])
        );
        // No flag in the definition: nothing is added.
        let plain = parse("schema = 1\n[install.host]\nbinary = [\"/bin/echo\"]\n").unwrap();
        assert_eq!(plain.final_args(&os(&["a"]), None), os(&["a"]));
    }

    /// pi's shape: a flag that some subcommands take only after their
    /// arguments and others not at all. And opencode's: a variable in
    /// place of a flag.
    #[test]
    fn a_flag_placed_last_and_a_variable() {
        let pi = parse(
            "schema = 1\n[install.host]\nbinary = [\"/bin/echo\"]\n[run]\nbypass_flag = \"--approve\"\n\
             grants = \"trusts project-local files\"\n\
             no_inject = [\"auth\"]\ninject_last = [\"install\", \"config\"]\n",
        )
        .unwrap();
        assert_eq!(pi.final_args(&os(&["hi"]), None), os(&["--approve", "hi"]));
        assert_eq!(
            pi.final_args(&os(&["install", "-l", "x"]), None),
            os(&["install", "-l", "x", "--approve"])
        );
        assert_eq!(
            pi.final_args(&os(&["auth", "login"]), None),
            os(&["auth", "login"])
        );
        assert_eq!(
            pi.trust_line(&os(&["config"])).as_deref(),
            Some("--approve (trusts project-local files)")
        );
        assert_eq!(pi.trust_line(&os(&["auth"])), None);

        let opencode = parse(
            "schema = 1\n[install.host]\nbinary = [\"/bin/echo\"]\n[run]\n\
             env = { OPENCODE_PERMISSION = '{\"*\":\"allow\"}', A_SECOND = \"2\" }\n",
        )
        .unwrap();
        assert_eq!(
            opencode.env_pairs(),
            os(&["A_SECOND=2", "OPENCODE_PERMISSION={\"*\":\"allow\"}"])
        );
        // No flag: the arguments are untouched, and the banner still says
        // what the agent runs with, whatever the subcommand.
        assert_eq!(opencode.final_args(&os(&["mcp"]), None), os(&["mcp"]));
        assert_eq!(
            opencode.trust_line(&os(&["mcp"])).as_deref(),
            Some("A_SECOND=2 and OPENCODE_PERMISSION={\"*\":\"allow\"}")
        );
        // A tapped package is a package.
        let tapped = parse(
            "schema = 1\n[install.host]\nbinary = [\"/bin/echo\"]\nbrew = { formula = \"anomalyco/tap/opencode\" }\n",
        )
        .unwrap();
        assert_eq!(
            brew_install_argv("/opt/homebrew", tapped.brew.as_ref().unwrap()),
            [
                "/opt/homebrew/bin/brew",
                "install",
                "--formula",
                "anomalyco/tap/opencode"
            ]
        );
    }

    /// What sv2 has to say goes after the option that takes it, as one
    /// argument, and only where a chat could start.
    #[test]
    fn text_for_the_system_prompt() {
        let with_prompt = |text: &str| {
            parse(&text.replace(
                "[run]",
                "[run]\nsystem_prompt = { flag = \"--append-system-prompt\" }",
            ))
            .unwrap()
        };
        let d = with_prompt(CLAUDE);
        assert_eq!(
            d.prompt.as_ref().map(Prompt::flag),
            Some("--append-system-prompt")
        );
        let bypass = "--dangerously-skip-permissions";
        let flag = "--append-system-prompt";
        // One argument, whatever is in it: lines, quotes, a leading dash.
        let text = "--not an option\nA \"second\" line with $HOME and 'quotes'.\n";
        assert_eq!(
            d.final_args(&os(&["-p", "hi"]), Some(text)),
            os(&[bypass, flag, text, "-p", "hi"])
        );
        assert_eq!(d.final_args(&[], Some(text)), os(&[bypass, flag, text]));
        // A subcommand that is not listed gets it like any command line.
        assert_eq!(
            d.final_args(&os(&["mcp", "list"]), Some("t")),
            os(&[bypass, flag, "t", "mcp", "list"])
        );
        // Listed in no_inject: neither flag, and nothing of the text.
        assert_eq!(d.final_args(&os(&["update"]), Some(text)), os(&["update"]));
        assert_eq!(d.prompt_for(&os(&["update"])).map(Prompt::flag), None);
        assert_eq!(
            d.prompt_for(&os(&["-p", "update"])).map(Prompt::flag),
            Some(flag)
        );
        // Nothing to say: the command line is what it was without the key.
        let without = parse(CLAUDE).unwrap();
        for args in [os(&[]), os(&["-p", "hi"]), os(&["update"])] {
            assert_eq!(d.final_args(&args, None), without.final_args(&args, None));
            assert_eq!(
                d.final_args(&args, Some("")),
                without.final_args(&args, None)
            );
        }
        // A definition without the key is not told, and runs as before.
        assert_eq!(without.prompt_for(&[]), None);
        assert_eq!(
            without.final_args(&os(&["-p", "hi"]), Some(text)),
            os(&[bypass, "-p", "hi"])
        );

        // pi's shape: a subcommand that takes the bypass flag after its
        // arguments does not get the text in front of them.
        let pi = with_prompt(
            "schema = 1\n[install.host]\nbinary = [\"/bin/echo\"]\n[run]\nbypass_flag = \"--approve\"\n\
             no_inject = [\"auth\"]\ninject_last = [\"install\"]\n",
        );
        assert_eq!(
            pi.final_args(&os(&["hi"]), Some("t")),
            os(&["--approve", flag, "t", "hi"])
        );
        assert_eq!(
            pi.final_args(&os(&["install", "-l"]), Some("t")),
            os(&["install", "-l", "--approve"])
        );
        assert_eq!(pi.final_args(&os(&["auth"]), Some("t")), os(&["auth"]));

        // The key alone, with no bypass flag.
        let plain = with_prompt("schema = 1\n[install.host]\nbinary = [\"/bin/echo\"]\n[run]\n");
        assert_eq!(
            plain.final_args(&os(&["a"]), Some("t")),
            os(&[flag, "t", "a"])
        );
        // The banner speaks of trust, and the text grants none.
        assert_eq!(plain.trust_line(&[]), None);
    }

    /// muse's shape: the text is a setting, `--config KEY=<JSON string>`,
    /// in one argument after the option, and only where a chat could start.
    #[test]
    fn text_for_the_system_prompt_as_a_setting() {
        let d = parse(
            "schema = 1\n[install.host]\nbinary = [\"/bin/echo\"]\n[run]\nbypass_flag = \"--yolo\"\n\
             no_inject = [\"mcp\"]\ninject_last = [\"exec\"]\n\
             system_prompt = { config = { flag = \"--config\", key = \"run.system_prompt_append\" } }\n",
        )
        .unwrap();
        assert_eq!(
            d.prompt,
            Some(Prompt::Setting {
                flag: "--config".into(),
                key: "run.system_prompt_append".into()
            })
        );
        let text =
            "--not an option\nA \"second\" line\twith $HOME, \\ and 'quotes'.\u{1b}\u{7f}é\n";
        let value = "run.system_prompt_append=\
                     \"--not an option\\nA \\\"second\\\" line\\twith $HOME, \\\\ and 'quotes'.\\u001b\\u007fé\\n\"";
        assert_eq!(
            d.final_args(&os(&["hi"]), Some(text)),
            os(&["--yolo", "--config", value, "hi"])
        );
        assert_eq!(
            d.final_args(&[], Some("t")),
            os(&["--yolo", "--config", "run.system_prompt_append=\"t\""])
        );
        // Neither list gets it, whatever the first argument's place.
        assert_eq!(
            d.final_args(&os(&["exec", "hi"]), Some("t")),
            os(&["exec", "hi", "--yolo"])
        );
        assert_eq!(d.final_args(&os(&["mcp"]), Some("t")), os(&["mcp"]));
        // Nothing to say, nothing added.
        assert_eq!(d.final_args(&os(&["hi"]), Some("")), os(&["--yolo", "hi"]));
        assert_eq!(d.final_args(&os(&["hi"]), None), os(&["--yolo", "hi"]));
    }

    /// The value is a JSON string: every control character escaped, and
    /// the rest as it is.
    #[test]
    fn a_json_string() {
        assert_eq!(json_string(""), "\"\"");
        assert_eq!(json_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(json_string("\n\r\t"), "\"\\n\\r\\t\"");
        assert_eq!(
            json_string("\u{0}\u{1f}\u{7f}"),
            "\"\\u0000\\u001f\\u007f\""
        );
        assert_eq!(json_string("${X} é 日本 / '"), "\"${X} é 日本 / '\"");
        // Every character of the first 256 that is not printable is escaped.
        let all: String = (0u8..=255).map(char::from).collect();
        let out = json_string(&all);
        assert!(!out.chars().any(|c| (c as u32) < 0x20 || c == '\u{7f}'));
    }

    /// Every definition that ships parses, has its script, and says what
    /// its flag or variables grant.
    #[test]
    fn the_builtin_definitions() {
        let agents = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../agents");
        let mut seen = Vec::new();
        for entry in std::fs::read_dir(&agents).unwrap() {
            let dir = entry.unwrap().path();
            let name = dir.file_name().unwrap().to_str().unwrap().to_string();
            assert!(valid_agent_name(&name), "{name}");
            let text = std::fs::read_to_string(dir.join("agent.toml")).unwrap();
            let d = Definition::parse(&name, &dir, &text, &|file| {
                std::fs::read_to_string(dir.join(file)).map_err(|e| format!("{file}: {e}"))
            })
            .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(d.guest_install.is_some(), "{name} has no installer");
            // gemini only: Homebrew deprecated its formula.
            if name == "gemini" {
                assert!(d.host_binary.is_empty(), "{name} has a host install");
            } else {
                assert!(d.brew.is_some(), "{name} names no package");
                assert!(!d.host_binary.is_empty(), "{name} has no host install");
            }
            assert!(
                !d.guest_binary.is_empty(),
                "{name} has no install of its own"
            );
            if d.trust_line(&[]).is_some() {
                assert!(d.grants.is_some(), "{name} does not say what it grants");
            }
            if name == "claude" {
                // Measured against the real program: its commands for
                // background sessions must be the first word.
                assert_eq!(
                    d.prompt.as_ref().map(Prompt::flag),
                    Some("--append-system-prompt")
                );
                assert_eq!(
                    d.final_args(&os(&["stop", "1"]), Some("t")),
                    os(&["stop", "1"])
                );
                assert_eq!(d.final_args(&os(&["mcp"]), Some("t")).len(), 4);
            }
            if name == "pi" {
                // Measured against the real program: every subcommand it
                // has must be the first word, so none gets the text.
                assert_eq!(
                    d.final_args(&os(&["hi"]), Some("t")),
                    os(&["--approve", "--append-system-prompt", "t", "hi"])
                );
                for sub in [
                    "install",
                    "remove",
                    "uninstall",
                    "update",
                    "list",
                    "config",
                    "auth",
                    "mcp",
                ] {
                    assert_eq!(d.prompt_for(&os(&[sub])).map(Prompt::flag), None, "{sub}");
                }
            }
            if name == "muse" {
                // Measured against muse 1.4.25: a subcommand must be the
                // first word, and none takes an option in front. exec,
                // resume and attach take --yolo after their arguments; the
                // others refuse it either side.
                assert_eq!(
                    d.final_args(&os(&["hi"]), Some("t")),
                    os(&["--yolo", "--config", "run.system_prompt_append=\"t\"", "hi"])
                );
                for sub in ["exec", "resume", "attach"] {
                    assert_eq!(d.flag_for(&os(&[sub])), Flag::Last("--yolo"), "{sub}");
                }
                for sub in [
                    "resume",
                    "exec",
                    "config",
                    "export",
                    "trace",
                    "model-profile",
                    "skills",
                    "voice",
                    "plugins",
                    "sandbox",
                    "schema",
                    "serve",
                    "mxp",
                    "attach",
                    "acp",
                    "session-message",
                    "sessions",
                    "mcp",
                    "auth",
                    "login",
                    "logout",
                    "usage",
                    "init",
                    "help",
                ] {
                    assert_eq!(d.final_args(&os(&[sub]), Some("t"))[0], sub, "{sub}");
                }
            }
            if name == "herdr" {
                // A multiplexer: no system prompt of its own to add to.
                assert_eq!(d.prompt, None);
            }
            seen.push(name);
        }
        seen.sort();
        assert_eq!(
            seen,
            ["claude", "codex", "gemini", "herdr", "muse", "opencode", "pi"]
        );
    }

    /// herdr's wrapper: a session with a browser is refused, because the
    /// server it starts outlives the browser; otherwise herdr runs with its
    /// arguments as given.
    #[test]
    fn herdrs_wrapper_refuses_a_session_with_a_browser() {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../agents/herdr/run.sh");
        let text = std::fs::read_to_string(script).unwrap();
        let run = |endpoint: Option<&str>| {
            let mut c = std::process::Command::new("/bin/bash");
            c.args([
                "-c",
                &text,
                "sv2-wrapper-herdr",
                "/bin/echo",
                "a b",
                "--session",
            ])
            .env_remove("SV_BROWSER_ENDPOINT")
            .env_remove("SV_IOS_SIMULATOR_UDID");
            if let Some(e) = endpoint {
                c.env("SV_BROWSER_ENDPOINT", e);
            }
            c.output().unwrap()
        };
        let out = run(None);
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout), "a b --session\n");
        let out = run(Some("http://127.0.0.1:9222"));
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "");
        assert!(String::from_utf8_lossy(&out.stderr).contains("--lightpanda"));
        // And one with a simulator, which is deleted when the session ends.
        let out = std::process::Command::new("/bin/bash")
            .args(["-c", &text, "sv2-wrapper-herdr", "/bin/echo", "a b"])
            .env_remove("SV_BROWSER_ENDPOINT")
            .env(
                "SV_IOS_SIMULATOR_UDID",
                "AAAAAAAA-0000-4000-8000-00000000ABCD",
            )
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "");
        assert!(String::from_utf8_lossy(&out.stderr).contains("--ios"));
    }

    /// The two installs are separate lists, and a run reads one of them.
    #[test]
    fn each_install_has_its_own_paths() {
        let d = parse(CLAUDE).unwrap();
        assert_eq!(
            d.host_paths(Some("/opt/homebrew")),
            ["/opt/homebrew/bin/claude"]
        );
        // Without Homebrew, what it would have installed is nowhere.
        assert_eq!(d.host_paths(None), Vec::<String>::new());
        assert_eq!(
            d.guest_paths("/Users/sv2-pat-work"),
            ["/Users/sv2-pat-work/.local/bin/claude"]
        );
        // Neither list ever holds a path of the other kind, so a copy in
        // the sandbox home cannot be run in place of the host's, or the
        // other way about.
        assert!(d
            .host_paths(Some("/opt/homebrew"))
            .iter()
            .all(|p| !p.starts_with("/Users/")));
        assert!(d
            .guest_paths("/Users/sv2-pat-work")
            .iter()
            .all(|p| p.starts_with("/Users/sv2-pat-work/")));

        // A definition may have only one of the two.
        let host_only =
            parse("schema = 1\n[install.host]\nbinary = [\"/bin/echo\", \"{brew}/bin/x\"]\n")
                .unwrap();
        assert_eq!(host_only.host_paths(None), ["/bin/echo"]);
        assert!(host_only.guest_binary.is_empty());
        let guest_only =
            parse("schema = 1\n[install.guest]\nbinary = [\"~/.x/bin/x\", \"~/.local/bin/x\"]\n")
                .unwrap();
        assert!(guest_only.host_binary.is_empty());
        assert_eq!(
            guest_only.guest_paths("/h"),
            ["/h/.x/bin/x", "/h/.local/bin/x"]
        );
        assert_eq!(guest_only.guest_install, None);

        // Without --native, each uses the one install it has.
        assert!(!host_only.uses_own_install(false));
        assert!(host_only.uses_own_install(true));
        assert!(guest_only.uses_own_install(false));
        assert!(guest_only.uses_own_install(true));
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
        t.definition(
            "builtin/other",
            "schema = 1\n[install.host]\nbinary = [\"/bin/echo\"]\n",
        );
        let mine = t.definition(
            "user/claude",
            "schema = 1\n[install.host]\nbinary = [\"/bin/cat\"]\n",
        );
        // Not definitions: no agent.toml, or not a name.
        std::fs::create_dir_all(t.0.join("user/empty")).unwrap();
        t.definition("user/Not_A_Name", CLAUDE);

        let s = t.sources();
        assert_eq!(s.names(), ["claude", "other"]);
        let d = s.load("claude").unwrap();
        assert_eq!(d.dir, mine);
        assert_eq!(d.host_binary, vec![Candidate::Absolute("bin/cat".into())]);
        // Nothing of the built-in shows through: no installer, no flag.
        assert_eq!((d.guest_install, d.bypass_flag), (None, None));

        assert_eq!(s.load("other").unwrap().dir, t.0.join("builtin/other"));
        for missing in ["nothere", "empty", "../builtin/other", "Not_A_Name", ""] {
            let err = s.load(missing).unwrap_err();
            assert!(err.contains("unknown agent"), "{missing}: {err}");
        }
    }

    #[test]
    fn the_template_is_a_definition_as_written() {
        let d = Definition::parse(
            "my-agent",
            Path::new("/d"),
            &template_for("my-agent"),
            &|_| Err("the template names no script".to_string()),
        )
        .unwrap();
        assert_eq!(d.host_binary, vec![Candidate::Brew("bin/my-agent".into())]);
        assert!(d.guest_binary.is_empty());
        assert_eq!((d.brew, d.guest_install, d.bypass_flag), (None, None, None));
        // And with every commented-out key switched on. (Comment lines
        // start with "# "; the keys with "#" alone.)
        let full: String = template_for("my-agent")
            .lines()
            .map(|l| match l.strip_prefix('#') {
                Some(rest) if !rest.starts_with(' ') && !rest.is_empty() => rest,
                _ => l,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let d =
            Definition::parse("my-agent", Path::new("/d"), &full, &|_| Ok(String::new())).unwrap();
        assert_eq!(d.brew, Some(Brew::Formula("my-agent".into())));
        assert_eq!(d.bypass_flag.as_deref(), Some("--yes"));
        assert_eq!(d.no_inject, ["update"]);
        assert_eq!(d.inject_last, ["config"]);
        assert_eq!(d.grants.as_deref(), Some("it acts without asking"));
        assert_eq!(
            d.env,
            [("EXAMPLE_VARIABLE".to_string(), "value".to_string())]
        );
        assert_eq!(
            d.prompt.as_ref().map(Prompt::flag),
            Some("--append-system-prompt")
        );
        assert!(d.guest_install.is_some());
        assert_eq!(d.wrapper.map(|(file, _)| file).as_deref(), Some("run.sh"));
        assert_eq!(
            d.guest_binary,
            vec![Candidate::Home(".local/bin/my-agent".into())]
        );
    }

    #[test]
    fn a_new_definition_from_the_template_or_from_another() {
        let t = Tree::new("create");
        t.definition("builtin/claude", CLAUDE);
        let s = t.sources();

        // From the template: one file, and it loads.
        let dir = s.create("mine", None).unwrap();
        assert_eq!(dir, t.0.join("user/mine"));
        let text = std::fs::read_to_string(dir.join("agent.toml")).unwrap();
        assert!(text.contains("\"~/.local/bin/mine\""), "{text}");
        assert!(!text.contains("{name}"), "{text}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        assert_eq!(s.load("mine").unwrap().dir, dir);
        assert!(s.names().contains(&"mine".to_string()));

        // From a built-in, under its own name: a copy that now wins.
        let dir = s.create("claude", Some("claude")).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("agent.toml")).unwrap(),
            CLAUDE
        );
        assert!(dir.join("install.sh").is_file());
        assert_eq!(s.load("claude").unwrap().dir, dir);

        // Never over something that is there, even an empty directory.
        for (name, from) in [("mine", None), ("claude", Some("claude"))] {
            let err = s.create(name, from).unwrap_err();
            assert!(err.contains("already exists"), "{err}");
        }
        std::fs::create_dir(t.0.join("user/taken")).unwrap();
        assert!(s.create("taken", None).is_err());

        // Bad names and unknown sources make nothing.
        for bad in ["", "Bad", "a/b", "../x", "-x"] {
            assert!(s.create(bad, None).is_err(), "{bad:?}");
        }
        let err = s.create("other", Some("nothere")).unwrap_err();
        assert!(err.contains("unknown agent"), "{err}");
        assert!(!t.0.join("user/other").exists());
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

        let dir = t.definition("builtin/broken", "schema = 1\n[install.host]\nbinary = 5\n");
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
            ("/Users/sv2/pat/work/agents/claude", "sandbox workspace"),
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
