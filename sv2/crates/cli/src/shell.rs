//! The built-in `shell` agent: the command line a session runs.
//!
//! This is built here, as the host user, and executed by the helper as the
//! sandbox user under the sandbox profile. It is a vector the whole way; the
//! user's arguments are positional parameters of a fixed script and are never
//! parsed as shell text.
//!
//! The helper gives a session a fixed minimal environment and takes neither
//! environment nor paths from its caller. What the terminal needs (TERM,
//! locale) and where the session starts are therefore expressed here, through
//! `/usr/bin/env` and `cd`, which run as the sandbox user.

use std::ffi::{OsStr, OsString};

/// Forwarded when set: terminal type and colour support, and the locale, so
/// UTF-8 renders instead of falling back to "C".
pub const FORWARDED: &[&str] = &[
    "TERM",
    "COLORTERM",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "LC_COLLATE",
    "LC_MESSAGES",
    "LC_MONETARY",
    "LC_NUMERIC",
    "LC_TIME",
];

pub fn brew_prefix() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "/opt/homebrew"
    } else {
        "/usr/local"
    }
}

/// Homebrew first, so tools installed for the session (tmux) are found.
pub fn session_path() -> String {
    let brew = brew_prefix();
    format!("{brew}/bin:{brew}/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin")
}

/// `NAME=value` pairs for `/usr/bin/env`, from the host environment.
/// TERMINFO (set by terminals such as Ghostty to a directory in their app
/// bundle) is forwarded unless it is in the host home, which the sandbox
/// cannot read.
pub fn forwarded_env(
    get: &dyn Fn(&str) -> Option<OsString>,
    host_home: Option<&OsStr>,
) -> Vec<OsString> {
    let pair = |name: &str, value: &OsStr| {
        let mut s = OsString::from(name);
        s.push("=");
        s.push(value);
        s
    };
    let mut out = Vec::new();
    for name in FORWARDED {
        if let Some(v) = get(name).filter(|v| !v.is_empty()) {
            out.push(pair(name, &v));
        }
    }
    if let Some(v) = get("TERMINFO").filter(|v| !v.is_empty()) {
        let in_home =
            host_home.is_some_and(|h| !h.is_empty() && std::path::Path::new(&v).starts_with(h));
        if !in_home {
            out.push(pair("TERMINFO", &v));
        }
    }
    out.push(pair("PATH", OsStr::new(&session_path())));
    out
}

/// Start in the host's current directory when it is inside the workspace,
/// otherwise at the workspace root. Compares path components, so
/// `/Users/Shared/sv2/pat/work2` is not "inside" `.../work`.
pub fn initial_dir(cwd: Option<&std::path::Path>, workspace: &str) -> OsString {
    match cwd {
        Some(c) if c.starts_with(workspace) => c.as_os_str().to_owned(),
        _ => OsString::from(workspace),
    }
}

/// zsh offers to write startup files (zsh-newuser-install) when the home has
/// none. The sandbox user creates an empty one itself.
const ENSURE_ZSHRC: &str = r#"[[ -e ~/.zshrc ]] || : >> ~/.zshrc"#;

/// The command for `sv2 run shell [ARGS...]`.
///
/// - No arguments: a login zsh. Interactive on a terminal; with piped stdin
///   it reads commands from the pipe.
/// - Arguments: a login zsh sources the startup files once (for `PATH`) and
///   then `exec`s the arguments as given.
pub fn argv(env: &[OsString], dir: &OsStr, args: &[OsString]) -> Vec<OsString> {
    let mut v: Vec<OsString> = vec!["/usr/bin/env".into()];
    v.extend(env.iter().cloned());
    v.push("/bin/zsh".into());
    if args.is_empty() {
        // -f: this outer shell reads no startup files; the login shell it
        // becomes reads them, once.
        v.push("-fc".into());
        v.push(format!(r#"{ENSURE_ZSHRC}; cd -- "$1" 2>/dev/null; exec /bin/zsh -l"#).into());
        v.push("sv2".into());
        v.push(dir.to_owned());
    } else {
        v.push("-lc".into());
        v.push(format!(r#"{ENSURE_ZSHRC}; cd -- "$1" 2>/dev/null; shift; exec -- "$@""#).into());
        v.push("sv2".into());
        v.push(dir.to_owned());
        v.extend(args.iter().cloned());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn forwards_only_the_listed_variables() {
        let get = |name: &str| -> Option<OsString> {
            match name {
                "TERM" => Some("xterm-ghostty".into()),
                "LANG" => Some("en_US.UTF-8".into()),
                "LC_ALL" => Some("".into()),
                "TERMINFO" => Some("/Applications/Ghostty.app/Contents/Resources/terminfo".into()),
                "PATH" => Some("/host/evil/bin".into()),
                "HOME" => Some("/Users/pat".into()),
                "SSH_AUTH_SOCK" => Some("/tmp/agent".into()),
                _ => None,
            }
        };
        let env = forwarded_env(&get, Some(OsStr::new("/Users/pat")));
        assert_eq!(
            env,
            os(&[
                "TERM=xterm-ghostty",
                "LANG=en_US.UTF-8",
                "TERMINFO=/Applications/Ghostty.app/Contents/Resources/terminfo",
                &format!("PATH={}", session_path()),
            ])
        );
    }

    #[test]
    fn terminfo_in_the_host_home_is_dropped() {
        let get = |name: &str| -> Option<OsString> {
            (name == "TERMINFO").then(|| "/Users/pat/.terminfo".into())
        };
        let env = forwarded_env(&get, Some(OsStr::new("/Users/pat")));
        assert_eq!(env, os(&[&format!("PATH={}", session_path())]));
        // A sibling whose name merely starts with the home is not the home.
        let get = |name: &str| -> Option<OsString> {
            (name == "TERMINFO").then(|| "/Users/patricia/.terminfo".into())
        };
        let env = forwarded_env(&get, Some(OsStr::new("/Users/pat")));
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn initial_directory() {
        let ws = "/Users/Shared/sv2/pat/work";
        let d = |cwd: &str| initial_dir(Some(Path::new(cwd)), ws);
        assert_eq!(d("/Users/Shared/sv2/pat/work"), OsString::from(ws));
        assert_eq!(
            d("/Users/Shared/sv2/pat/work/repo/src"),
            OsString::from("/Users/Shared/sv2/pat/work/repo/src")
        );
        assert_eq!(d("/Users/pat/src"), OsString::from(ws));
        assert_eq!(d("/Users/Shared/sv2/pat/work2"), OsString::from(ws));
        assert_eq!(d("/Users/Shared/sv2/pat/default"), OsString::from(ws));
        assert_eq!(initial_dir(None, ws), OsString::from(ws));
    }

    #[test]
    fn argv_shapes() {
        let env = os(&["TERM=xterm"]);
        let v = argv(&env, OsStr::new("/ws"), &[]);
        assert_eq!(
            &v[..4],
            &os(&["/usr/bin/env", "TERM=xterm", "/bin/zsh", "-fc"])[..]
        );
        assert_eq!(&v[5..], &os(&["sv2", "/ws"])[..]);

        let v = argv(&env, OsStr::new("/ws"), &os(&["tmux", "new", "-A"]));
        assert_eq!(
            &v[..4],
            &os(&["/usr/bin/env", "TERM=xterm", "/bin/zsh", "-lc"])[..]
        );
        assert_eq!(&v[5..], &os(&["sv2", "/ws", "tmux", "new", "-A"])[..]);
        // The helper requires an absolute command.
        assert!(v[0].to_str().unwrap().starts_with('/'));
    }

    /// Run the real command line (as the current user, with a scratch HOME)
    /// and check that arguments arrive exactly as given: no word splitting,
    /// no expansion, no option parsing.
    #[test]
    fn arguments_survive_unchanged() {
        let scratch = std::env::temp_dir().join(format!("sv2-shell-test-{}", std::process::id()));
        let home = scratch.join("home");
        let dir = scratch.join("dir with space");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&dir).unwrap();

        let tricky = os(&[
            "/usr/bin/printf",
            "%s\\n",
            "a b",
            "$HOME",
            "`id`",
            "semi;colon",
            "quote'\"",
            "",
            "-l",
            "--",
            "ünï",
            "*",
        ]);
        let v = argv(&[], dir.as_os_str(), &tricky);
        let out = Command::new(&v[0])
            .args(&v[1..])
            .env_clear()
            .env("HOME", &home)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "a b\n$HOME\n`id`\nsemi;colon\nquote'\"\n\n-l\n--\nünï\n*\n"
        );

        // The session starts in the requested directory, and the sandbox
        // user's own shell created the empty .zshrc.
        let v = argv(&[], dir.as_os_str(), &os(&["/bin/pwd"]));
        let out = Command::new(&v[0])
            .args(&v[1..])
            .env_clear()
            .env("HOME", &home)
            .output()
            .unwrap();
        let pwd = String::from_utf8_lossy(&out.stdout);
        assert!(pwd.trim_end().ends_with("/dir with space"), "{pwd}");
        assert!(home.join(".zshrc").exists());

        // Exit status propagates.
        let v = argv(&[], dir.as_os_str(), &os(&["/bin/sh", "-c", "exit 7"]));
        let st = Command::new(&v[0])
            .args(&v[1..])
            .env_clear()
            .env("HOME", &home)
            .status()
            .unwrap();
        assert_eq!(st.code(), Some(7));

        // No arguments and piped stdin: the login shell reads the pipe.
        let v = argv(&[], dir.as_os_str(), &[]);
        let mut child = Command::new(&v[0])
            .args(&v[1..])
            .env_clear()
            .env("HOME", &home)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"echo piped-$((6*7))\n")
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "piped-42\n");

        std::fs::remove_dir_all(&scratch).unwrap();
    }
}
