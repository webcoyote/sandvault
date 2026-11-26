//! The command line a session runs.
//!
//! This is built here, as the host user, and executed by the helper as the
//! sandbox user under the sandbox profile. It is a vector the whole way; the
//! user's arguments are positional parameters of a fixed script and are never
//! parsed as shell text.
//!
//! The helper gives a session a fixed minimal environment and takes neither
//! environment nor paths from its caller. What the terminal needs (TERM,
//! locale) is therefore expressed here, through `/usr/bin/env`, which runs
//! as the sandbox user.
//!
//! Everything else a session needs before its program starts is done by the
//! guest launcher, `guest/launch.sh`: a script whose text is compiled into
//! `sv2` and passed on the command line. No copy of it is kept in the
//! sandbox home, so there is nothing there to unpack, to version, or for one
//! session to change for the next.

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

/// The guest launcher. See the file for what it does and what it is given.
pub const LAUNCHER: &str = include_str!("../../../guest/launch.sh");

/// What the launcher is asked to do. See the launcher for each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind<'a> {
    /// A login zsh, or the arguments as a command in its environment.
    Shell,
    /// The first of `paths` that is a program. When none is and `native` is
    /// set, `install_script` runs first.
    Agent {
        name: &'a str,
        native: bool,
        install_script: &'a str,
        paths: &'a [String],
    },
    /// Whether one of `paths` is a program. Starts nothing.
    Probe { paths: &'a [String] },
}

impl Kind<'_> {
    /// The launcher's arguments between DIR and ARGS.
    fn words(&self) -> Vec<OsString> {
        let counted = |paths: &[String]| {
            let mut v = vec![OsString::from(paths.len().to_string())];
            v.extend(paths.iter().map(OsString::from));
            v
        };
        match self {
            Kind::Shell => vec!["shell".into()],
            Kind::Agent {
                name,
                native,
                install_script,
                paths,
            } => {
                let mut v: Vec<OsString> = vec![
                    "agent".into(),
                    (*name).into(),
                    if *native { "1" } else { "0" }.into(),
                    (*install_script).into(),
                ];
                v.extend(counted(paths));
                v
            }
            Kind::Probe { paths } => {
                let mut v = vec![OsString::from("probe")];
                v.extend(counted(paths));
                v
            }
        }
    }
}

/// `NAME=value` pairs for `/usr/bin/env`, from the host environment.
/// TERMINFO (set by terminals such as Ghostty to a directory in their app
/// bundle) is forwarded unless it is in the host home, which the sandbox
/// cannot read. `PATH` is not forwarded: the launcher sets it.
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

/// The command for a session: the launcher, told where to start and what.
///
/// `args` go to the launcher as positional parameters and from there to the
/// program, unchanged.
pub fn argv(env: &[OsString], dir: &OsStr, kind: &Kind, args: &[OsString]) -> Vec<OsString> {
    let mut v: Vec<OsString> = vec!["/usr/bin/env".into()];
    v.extend(env.iter().cloned());
    v.extend(["/bin/bash", "-c", LAUNCHER, "sv2-launch"].map(OsString::from));
    v.push(dir.to_owned());
    v.extend(kind.words());
    v.extend(args.iter().cloned());
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output, Stdio};

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
            ])
        );
    }

    #[test]
    fn terminfo_in_the_host_home_is_dropped() {
        let get = |name: &str| -> Option<OsString> {
            (name == "TERMINFO").then(|| "/Users/pat/.terminfo".into())
        };
        let env = forwarded_env(&get, Some(OsStr::new("/Users/pat")));
        assert_eq!(env, os(&[]));
        // A sibling whose name merely starts with the home is not the home.
        let get = |name: &str| -> Option<OsString> {
            (name == "TERMINFO").then(|| "/Users/patricia/.terminfo".into())
        };
        let env = forwarded_env(&get, Some(OsStr::new("/Users/pat")));
        assert_eq!(env.len(), 1);
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
    fn argv_shape() {
        let env = os(&["TERM=xterm"]);
        let v = argv(&env, OsStr::new("/ws"), &Kind::Shell, &os(&["tmux", "new"]));
        assert_eq!(
            v,
            os(&[
                "/usr/bin/env",
                "TERM=xterm",
                "/bin/bash",
                "-c",
                LAUNCHER,
                "sv2-launch",
                "/ws",
                "shell",
                "tmux",
                "new"
            ])
        );
        // The helper requires an absolute command.
        assert!(v[0].to_str().unwrap().starts_with('/'));
        // The launcher travels on every command line, and sudo logs it.
        assert!(LAUNCHER.len() < 16 * 1024, "the launcher has grown large");
    }

    /// A scratch home and starting directory for running the real launcher
    /// as the current user.
    struct Scratch {
        root: PathBuf,
        home: PathBuf,
        dir: PathBuf,
    }

    impl Scratch {
        fn new(tag: &str) -> Scratch {
            let root =
                std::env::temp_dir().join(format!("sv2-launch-test-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            // Resolved, so that it compares equal to what `pwd` prints.
            std::fs::create_dir_all(&root).unwrap();
            let root = root.canonicalize().unwrap();
            let home = root.join("home");
            let dir = root.join("dir with space");
            std::fs::create_dir_all(&home).unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            Scratch { root, home, dir }
        }

        fn command(&self, v: &[OsString]) -> Command {
            let mut c = Command::new(&v[0]);
            c.args(&v[1..]).env_clear().env("HOME", &self.home);
            c
        }

        /// The launcher as a session gets it, with ARGS.
        fn run(&self, args: &[&str]) -> Output {
            let v = argv(&[], self.dir.as_os_str(), &Kind::Shell, &os(args));
            self.command(&v).stdin(Stdio::null()).output().unwrap()
        }

        fn stdout(&self, args: &[&str]) -> String {
            let out = self.run(args);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).into_owned()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Arguments arrive exactly as given: no word splitting, no expansion,
    /// no option parsing.
    #[test]
    fn arguments_survive_unchanged() {
        let s = Scratch::new("args");
        let tricky = [
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
        ];
        assert_eq!(
            s.stdout(&tricky),
            "a b\n$HOME\n`id`\nsemi;colon\nquote'\"\n\n-l\n--\nünï\n*\n"
        );
    }

    #[test]
    fn starts_in_the_directory_and_passes_the_exit_status_on() {
        let s = Scratch::new("dir");
        assert_eq!(s.stdout(&["/bin/pwd"]).trim_end(), s.dir.to_str().unwrap());
        assert_eq!(s.run(&["/bin/sh", "-c", "exit 7"]).status.code(), Some(7));

        // A directory that is not there is not an error: the session stays
        // where the helper put it.
        let v = argv(
            &[],
            OsStr::new("/nonexistent/sv2"),
            &Kind::Shell,
            &os(&["/bin/pwd"]),
        );
        let out = s.command(&v).current_dir(&s.home).output().unwrap();
        assert!(out.status.success());
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim_end(),
            s.home.to_str().unwrap()
        );
    }

    /// No arguments and piped input: the login shell reads the pipe.
    #[test]
    fn a_shell_without_arguments_reads_piped_input() {
        use std::io::Write;
        let s = Scratch::new("pipe");
        let v = argv(&[], s.dir.as_os_str(), &Kind::Shell, &[]);
        let mut child = s
            .command(&v)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"echo piped-$((6*7))\n")
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "piped-42\n");
    }

    #[test]
    fn the_users_bin_directories_come_first_in_a_login_shell() {
        let s = Scratch::new("path");
        let path = s.stdout(&["/bin/sh", "-c", "printf %s \"$PATH\""]);
        let home = s.home.to_str().unwrap();
        assert!(
            path.starts_with(&format!("{home}/.local/bin:{home}/bin:")),
            "{path}"
        );
        let at = |dir: &str| path.split(':').position(|p| p == dir);
        let system = at("/usr/bin").expect("no /usr/bin in PATH");
        for brew in ["/opt/homebrew", "/usr/local"] {
            if Path::new(brew).join("bin/brew").exists() {
                let brew_bin = at(&format!("{brew}/bin")).expect("Homebrew is not in PATH");
                assert!(brew_bin < system, "Homebrew comes after the system: {path}");
                break;
            }
        }
        // Nothing from the environment this test runs in.
        assert!(!path.contains(".cargo"), "{path}");
    }

    #[test]
    fn startup_files_are_written_once_and_then_left_alone() {
        let s = Scratch::new("seed");
        let zprofile = s.home.join(".zprofile");
        let zshrc = s.home.join(".zshrc");

        s.stdout(&["/usr/bin/true"]);
        let seeded = std::fs::read_to_string(&zprofile).unwrap();
        assert!(seeded.contains("Written once by sandvault2"), "{seeded}");
        assert_eq!(std::fs::read_to_string(&zshrc).unwrap(), "");

        // The user's own versions: a file with other content, and a link
        // that points nowhere. Neither is replaced, and the first is read.
        std::fs::write(&zprofile, "export SV2_TEST_MARK=mine\n").unwrap();
        std::fs::remove_file(&zshrc).unwrap();
        std::os::unix::fs::symlink(s.root.join("nowhere"), &zshrc).unwrap();
        assert_eq!(
            s.stdout(&["/bin/sh", "-c", "printf %s \"$SV2_TEST_MARK\""]),
            "mine"
        );
        assert_eq!(
            std::fs::read_to_string(&zprofile).unwrap(),
            "export SV2_TEST_MARK=mine\n"
        );
        assert!(zshrc.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(!s.root.join("nowhere").exists());
    }

    /// A home that cannot be written still gets a session.
    #[test]
    fn a_read_only_home_is_reported_and_does_not_stop_the_session() {
        use std::os::unix::fs::PermissionsExt;
        let s = Scratch::new("readonly");
        std::fs::set_permissions(&s.home, std::fs::Permissions::from_mode(0o500)).unwrap();
        let out = s.run(&["/bin/echo", "ran"]);
        std::fs::set_permissions(&s.home, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "ran\n");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("could not write"), "{err}");
    }

    impl Scratch {
        /// The launcher asked for an agent whose candidates are `paths`.
        fn agent(&self, native: bool, script: &str, paths: &[&str], args: &[&str]) -> Output {
            let paths: Vec<String> = paths.iter().map(|p| (*p).to_string()).collect();
            let kind = Kind::Agent {
                name: "tool",
                native,
                install_script: script,
                paths: &paths,
            };
            let v = argv(&[], self.dir.as_os_str(), &kind, &os(args));
            self.command(&v).stdin(Stdio::null()).output().unwrap()
        }

        fn probe(&self, paths: &[&str]) -> Option<i32> {
            let paths: Vec<String> = paths.iter().map(|p| (*p).to_string()).collect();
            let v = argv(
                &[],
                self.dir.as_os_str(),
                &Kind::Probe { paths: &paths },
                &[],
            );
            self.command(&v).output().unwrap().status.code()
        }
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn an_agent_is_the_first_candidate_that_is_a_program() {
        let s = Scratch::new("agent");
        let home = s.home.to_str().unwrap();
        // Not programs: missing, a directory, a file without the execute
        // bit, a name that is not a path.
        std::fs::create_dir_all(s.home.join("adir")).unwrap();
        std::fs::write(s.home.join("plain"), "").unwrap();
        let missing = format!("{home}/missing");
        let adir = format!("{home}/adir");
        let plain = format!("{home}/plain");
        let out = s.agent(
            false,
            "",
            &[&missing, &adir, &plain, "echo", "/bin/echo", "/bin/cat"],
            &["a b", "$HOME", "--", "*"],
        );
        assert_eq!(text(&out.stdout), "a b $HOME -- *\n");
        assert!(
            text(&out.stderr).contains("sv2: running /bin/echo as"),
            "{}",
            text(&out.stderr)
        );
        // The agent's exit status is the session's.
        let out = s.agent(false, "", &["/bin/sh"], &["-c", "exit 9"]);
        assert_eq!(out.status.code(), Some(9));

        assert_eq!(s.probe(&[&missing, "/bin/echo"]), Some(0));
        assert_eq!(s.probe(&[&missing, &adir, &plain, "echo"]), Some(1));
    }

    #[test]
    fn a_missing_agent_is_installed_only_when_asked() {
        let s = Scratch::new("native");
        let home = s.home.to_str().unwrap();
        let tool = format!("{home}/.local/bin/tool");
        let install = r#"mkdir -p "$HOME/.local/bin"
printf '#!/bin/sh\necho "tool ran: $*"\n' > "$HOME/.local/bin/tool"
chmod +x "$HOME/.local/bin/tool"
echo "installer says hello" >&2
"#;

        // Not asked: nothing is installed, and nothing runs.
        let out = s.agent(false, install, &[&tool], &["x"]);
        assert_eq!(out.status.code(), Some(127));
        assert_eq!(text(&out.stdout), "");
        assert!(text(&out.stderr).contains("tool is not installed in this sandbox"));
        assert!(!Path::new(&tool).exists());
        // Asked, with no installer: the same.
        assert_eq!(s.agent(true, "", &[&tool], &[]).status.code(), Some(127));

        // An installer that fails stops the session with its status.
        let out = s.agent(true, "echo broken >&2; exit 3", &[&tool], &["x"]);
        assert_eq!(out.status.code(), Some(3));
        assert_eq!(text(&out.stdout), "");
        // One that succeeds without producing the program is reported.
        let out = s.agent(true, "true", &[&tool], &["x"]);
        assert_eq!(out.status.code(), Some(127));
        assert!(text(&out.stderr).contains("none of these exists"));

        // Asked: installed, then run with the arguments.
        let out = s.agent(true, install, &[&tool], &["one", "two words"]);
        assert_eq!(text(&out.stdout), "tool ran: one two words\n");
        let err = text(&out.stderr);
        assert!(err.contains("installing tool in the sandbox home"), "{err}");
        assert!(err.contains("installer says hello"), "{err}");

        // Present now: the installer is not run again.
        let out = s.agent(true, "echo again >&2; exit 5", &[&tool], &["x"]);
        assert_eq!(text(&out.stdout), "tool ran: x\n");
        assert!(!text(&out.stderr).contains("again"));
    }

    #[test]
    fn the_launcher_checks_its_list_of_paths() {
        let s = Scratch::new("paths");
        let bash = |args: &[&str]| {
            Command::new("/bin/bash")
                .args(["-c", LAUNCHER, "sv2-launch", "/"])
                .args(args)
                .env_clear()
                .env("HOME", &s.home)
                .output()
                .unwrap()
                .status
                .code()
        };
        // A count that is not a number, is zero, or exceeds what follows.
        assert_eq!(bash(&["probe"]), Some(2));
        assert_eq!(bash(&["probe", "x", "/bin/echo"]), Some(2));
        assert_eq!(bash(&["probe", "0"]), Some(2));
        assert_eq!(bash(&["probe", "2", "/bin/echo"]), Some(2));
        assert_eq!(bash(&["agent", "tool", "0", ""]), Some(2));
        assert_eq!(
            bash(&["agent", "tool", "0", "", "3", "/bin/echo", "x"]),
            Some(2)
        );
        // The count separates paths from arguments: "/bin/cat" is an
        // argument here, not a candidate.
        let out = Command::new("/bin/bash")
            .args(["-c", LAUNCHER, "sv2-launch", "/", "agent", "tool", "0", ""])
            .args(["1", "/bin/echo", "/bin/cat"])
            .env_clear()
            .env("HOME", &s.home)
            .output()
            .unwrap();
        assert_eq!(text(&out.stdout), "/bin/cat\n");
    }

    #[test]
    fn the_launcher_refuses_what_it_does_not_know() {
        let s = Scratch::new("usage");
        let bash = |args: &[&str]| {
            Command::new("/bin/bash")
                .args(["-c", LAUNCHER, "sv2-launch"])
                .args(args)
                .env_clear()
                .env("HOME", &s.home)
                .output()
                .unwrap()
        };
        assert_eq!(bash(&[]).status.code(), Some(2));
        assert_eq!(bash(&["/"]).status.code(), Some(2));
        let out = bash(&["/", "claude", "/bin/echo", "ran"]);
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "");
    }
}
