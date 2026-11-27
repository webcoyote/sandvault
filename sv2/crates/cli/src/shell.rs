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
    /// The first of `paths` that is a program: the places one install of
    /// the agent leaves it. When none is and `native` is set,
    /// `install_script` runs first. A `wrapper` that is not empty runs in
    /// the program's place, given its path and the arguments.
    Agent {
        name: &'a str,
        native: bool,
        install_script: &'a str,
        wrapper: &'a str,
        paths: &'a [String],
    },
    /// Unpack the archive on standard input into the sandbox home.
    Unpack,
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
            Kind::Unpack => vec!["unpack".into()],
            Kind::Agent {
                name,
                native,
                install_script,
                wrapper,
                paths,
            } => {
                let mut v: Vec<OsString> = vec![
                    "agent".into(),
                    (*name).into(),
                    if *native { "1" } else { "0" }.into(),
                    (*install_script).into(),
                    (*wrapper).into(),
                ];
                v.extend(counted(paths));
                v
            }
        }
    }
}

/// A browser the launcher starts in the sandbox before the session's
/// program, and stops after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Browser {
    Chrome,
    Lightpanda,
}

impl Browser {
    /// The launcher's name for it, and the option's.
    pub fn name(self) -> &'static str {
        match self {
            Browser::Chrome => "chrome",
            Browser::Lightpanda => "lightpanda",
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
    argv_with_browser(env, dir, None, kind, args)
}

/// The same, with a browser started first: the program at `path`, which
/// the sandbox must be able to read.
pub fn argv_with_browser(
    env: &[OsString],
    dir: &OsStr,
    browser: Option<(Browser, &str)>,
    kind: &Kind,
    args: &[OsString],
) -> Vec<OsString> {
    let mut v: Vec<OsString> = vec!["/usr/bin/env".into()];
    v.extend(env.iter().cloned());
    v.extend(["/bin/bash", "-c", LAUNCHER, "sv2-launch"].map(OsString::from));
    v.push(dir.to_owned());
    if let Some((browser, path)) = browser {
        v.extend(["browser", browser.name(), path].map(OsString::from));
    }
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
        // A browser goes between the directory and the kind.
        let b = argv_with_browser(
            &env,
            OsStr::new("/ws"),
            Some((Browser::Lightpanda, "/opt/homebrew/bin/lightpanda")),
            &Kind::Shell,
            &os(&["tmux"]),
        );
        assert_eq!(
            b[6..],
            os(&[
                "/ws",
                "browser",
                "lightpanda",
                "/opt/homebrew/bin/lightpanda",
                "shell",
                "tmux"
            ])
        );
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
                wrapper: "",
                paths: &paths,
            };
            let v = argv(&[], self.dir.as_os_str(), &kind, &os(args));
            self.command(&v).stdin(Stdio::null()).output().unwrap()
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
    }

    /// With a wrapper the wrapper runs, in the login shell's environment,
    /// and is given the program and the arguments unchanged; it decides
    /// what happens next. Without one the program runs itself.
    #[test]
    fn a_wrapper_runs_in_the_programs_place() {
        let s = Scratch::new("wrapper");
        let paths = vec!["/bin/echo".to_string()];
        let run = |wrapper: &str, args: &[&str]| {
            let kind = Kind::Agent {
                name: "tool",
                native: false,
                install_script: "",
                wrapper,
                paths: &paths,
            };
            let v = argv(&[], s.dir.as_os_str(), &kind, &os(args));
            s.command(&v).stdin(Stdio::null()).output().unwrap()
        };
        let wrapper = r#"set -euo pipefail
printf '%s|' "$0" "$PWD" "${PATH%%:*}" >&2
printf '[%s]' "$@" >&2
exec "$@"
"#;
        let out = run(wrapper, &["a b", "$HOME", "*", ""]);
        assert_eq!(text(&out.stdout), "a b $HOME * \n");
        let err = text(&out.stderr);
        assert!(err.contains("through the wrapper of tool"), "{err}");
        let home = s.home.to_str().unwrap();
        let dir = s.dir.to_str().unwrap();
        assert!(
            err.contains(&format!(
                "sv2-wrapper-tool|{dir}|{home}/.local/bin|[/bin/echo][a b][$HOME][*][]"
            )),
            "{err}"
        );
        // It may refuse; its status is the session's, and nothing runs.
        let out = run("echo refused >&2; exit 4", &["x"]);
        assert_eq!(out.status.code(), Some(4));
        assert_eq!(text(&out.stdout), "");
        // An empty wrapper is none.
        let out = run("", &["plain"]);
        assert_eq!(text(&out.stdout), "plain\n");
        assert!(!text(&out.stderr).contains("wrapper"));
    }

    /// What a definition sets with `env` reaches the agent as written,
    /// through the launcher and the login shell.
    #[test]
    fn a_definitions_variables_reach_the_agent() {
        let s = Scratch::new("env");
        let paths = vec!["/bin/sh".to_string()];
        let kind = Kind::Agent {
            name: "tool",
            native: false,
            install_script: "",
            wrapper: "",
            paths: &paths,
        };
        let env = os(&[r#"OPENCODE_PERMISSION={"*":"allow"}"#, "A_SECOND=two words"]);
        let v = argv(
            &env,
            s.dir.as_os_str(),
            &kind,
            &os(&["-c", r#"printf '%s|%s' "$OPENCODE_PERMISSION" "$A_SECOND""#]),
        );
        let out = s.command(&v).stdin(Stdio::null()).output().unwrap();
        assert_eq!(text(&out.stdout), r#"{"*":"allow"}|two words"#);
    }

    /// Text for an agent's system prompt is one argument of several lines.
    /// It reaches the agent as written, through the launcher and the login
    /// shell, and what follows it is still the user's.
    #[test]
    fn text_for_the_system_prompt_reaches_the_agent_as_one_argument() {
        let s = Scratch::new("prompt");
        let told =
            "# A heading\n\n    $SV_BROWSER_ENDPOINT\n- `quoted` \"twice\" 'thrice'; * \\ \u{e9}\n";
        let out = s.agent(
            false,
            "",
            &["/bin/sh"],
            &[
                "-c",
                r#"printf '%s|' "$#" "$@""#,
                "agent",
                "--append-system-prompt",
                told,
                "a b",
            ],
        );
        assert_eq!(
            text(&out.stdout),
            format!("3|--append-system-prompt|{told}|a b|")
        );
    }

    /// A stand-in for a browser: it says where it listens, as Chrome does,
    /// writes its process ID where the test can read it, and stays.
    fn stand_in_browser(s: &Scratch, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = s.root.join("browser");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_str().unwrap().to_string()
    }

    #[test]
    fn a_browser_is_started_for_the_session_and_stopped_after_it() {
        let s = Scratch::new("browser");
        let pid_file = s.root.join("pid");
        let browser = stand_in_browser(
            &s,
            &format!(
                "echo $$ > '{}'\necho 'DevTools listening on ws://127.0.0.1:4242/devtools/browser/x' >&2\nexec /bin/sleep 60",
                pid_file.display()
            ),
        );
        let v = argv_with_browser(
            &[],
            s.dir.as_os_str(),
            Some((Browser::Chrome, &browser)),
            &Kind::Shell,
            &os(&["/bin/sh", "-c", r#"printf %s "$SV_BROWSER_ENDPOINT""#]),
        );
        let out = s
            .command(&v)
            .env("TMPDIR", &s.root)
            .env("SV2_NO_BROWSER_INSTALL", "1")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(text(&out.stdout), "http://127.0.0.1:4242");
        assert!(
            text(&out.stderr).contains("sv2: chrome listens at http://127.0.0.1:4242"),
            "{}",
            text(&out.stderr)
        );
        // Stopped, and its directory removed, soon after the session ends.
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        let gone = |pid: &str| {
            !Command::new("/bin/kill")
                .args(["-0", pid.trim()])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        };
        let leftovers = || {
            std::fs::read_dir(&s.root)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with("sv2-browser."))
        };
        let mut waited = 0;
        while (!gone(&pid) || leftovers()) && waited < 50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            waited += 1;
        }
        assert!(gone(&pid), "the browser is still running");
        assert!(!leftovers(), "the browser's directory is still there");
    }

    /// For chrome, a headless shell of Playwright's in the home is used in
    /// place of the program given: the newest, by number.
    #[test]
    fn chrome_prefers_a_headless_shell_in_the_home() {
        use std::os::unix::fs::PermissionsExt;
        let s = Scratch::new("headless");
        let shell_at = |revision: &str, port: &str| {
            let dir = s.home.join(format!(
                "Library/Caches/ms-playwright/chromium_headless_shell-{revision}/chrome-headless-shell-mac-arm64"
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("chrome-headless-shell");
            std::fs::write(
                &path,
                format!("#!/bin/sh\necho 'DevTools listening on ws://127.0.0.1:{port}/devtools/browser/x' >&2\nexec /bin/sleep 60\n"),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        let run = |program: &str| {
            let v = argv_with_browser(
                &[],
                s.dir.as_os_str(),
                Some((Browser::Chrome, program)),
                &Kind::Shell,
                &os(&["/bin/sh", "-c", r#"printf %s "$SV_BROWSER_ENDPOINT""#]),
            );
            s.command(&v)
                .env("TMPDIR", &s.root)
                .env("SV2_NO_BROWSER_INSTALL", "1")
                .stdin(Stdio::null())
                .output()
                .unwrap()
        };
        let chrome = stand_in_browser(
            &s,
            "echo 'DevTools listening on ws://127.0.0.1:1111/devtools/browser/x' >&2\nexec /bin/sleep 60",
        );

        // None in the home: the program given, and a word about its limit.
        let out = run(&chrome);
        assert_eq!(text(&out.stdout), "http://127.0.0.1:1111");
        assert!(
            text(&out.stderr).contains("using Google Chrome, which in a sandbox ends"),
            "{}",
            text(&out.stderr)
        );
        // Neither: the session does not start.
        let out = run("/nonexistent/Google Chrome");
        assert_eq!(out.status.code(), Some(127));
        assert_eq!(text(&out.stdout), "");

        // 1000 is newer than 999, though it sorts before it; a directory
        // that is not a revision is not looked at.
        shell_at("999", "2999");
        shell_at("1000", "3000");
        shell_at("zzz", "4000");
        for program in [chrome.as_str(), "/nonexistent/Google Chrome"] {
            let out = run(program);
            assert_eq!(text(&out.stdout), "http://127.0.0.1:3000");
            let err = text(&out.stderr);
            assert!(
                err.contains("chrome (Playwright's headless shell) listens"),
                "{err}"
            );
            assert!(!err.contains("using Google Chrome"), "{err}");
        }
    }

    #[test]
    fn a_browser_that_does_not_start_stops_the_session() {
        let s = Scratch::new("nobrowser");
        let run = |browser: &str| {
            let v = argv_with_browser(
                &[],
                s.dir.as_os_str(),
                Some((Browser::Lightpanda, browser)),
                &Kind::Shell,
                &os(&["/bin/echo", "the session ran"]),
            );
            s.command(&v)
                .env("TMPDIR", &s.root)
                .env("SV2_NO_BROWSER_INSTALL", "1")
                .stdin(Stdio::null())
                .output()
                .unwrap()
        };
        // It ends without saying where it listens.
        let out = run(&stand_in_browser(&s, "echo 'no display' >&2; exit 1"));
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(text(&out.stdout), "");
        let err = text(&out.stderr);
        assert!(
            err.contains("the browser lightpanda did not start"),
            "{err}"
        );
        assert!(err.contains("no display"), "{err}");
        // It is not a program at all.
        let out = run("lightpanda");
        assert_eq!(out.status.code(), Some(127));
        assert_eq!(text(&out.stdout), "");
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
        assert_eq!(bash(&["agent", "tool", "0", "", ""]), Some(2));
        assert_eq!(
            bash(&["agent", "tool", "0", "", "", "x", "/bin/echo"]),
            Some(2)
        );
        assert_eq!(bash(&["agent", "tool", "0", "", "", "0"]), Some(2));
        // "probe" is no longer a kind: sv2 does not ask a sandbox what it has.
        assert_eq!(bash(&["probe", "1", "/bin/echo"]), Some(2));
        assert_eq!(
            bash(&["agent", "tool", "0", "", "", "3", "/bin/echo", "x"]),
            Some(2)
        );
        // The count separates paths from arguments: "/bin/cat" is an
        // argument here, not a candidate.
        let out = Command::new("/bin/bash")
            .args([
                "-c",
                LAUNCHER,
                "sv2-launch",
                "/",
                "agent",
                "tool",
                "0",
                "",
                "",
            ])
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
