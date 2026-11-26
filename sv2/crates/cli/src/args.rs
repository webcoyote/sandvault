//! Command-line parsing.
//!
//! ```text
//! sv2 install | uninstall
//! sv2 user allow USER | user deny USER
//! sv2 profile add NAME | profile remove NAME | profile list
//! sv2 run [OPTIONS] NAME [ARGS...]
//! sv2 [OPTIONS] NAME [ARGS...]        shorthand for sv2 run
//! sv2 repair PATH | doctor
//! ```
//!
//! Options come before NAME. Everything after NAME goes to the agent
//! unchanged. Subcommands always win over agent names, and a first argument
//! that is neither is an error: nothing is ever looked up on `PATH`.

use std::ffi::OsString;

use sv2_common::{valid_profile, DEFAULT_PROFILE};

#[cfg(test)]
pub const SUBCOMMANDS: &[&str] = &[
    "install",
    "uninstall",
    "user",
    "profile",
    "run",
    "repair",
    "doctor",
    "help",
];

#[derive(Debug, PartialEq, Eq)]
pub enum Cmd {
    Help,
    Version,
    Install,
    Uninstall,
    UserAllow(String),
    UserDeny(String),
    ProfileAdd(String),
    ProfileRemove(String),
    ProfileList,
    Run {
        profile: String,
        agent: String,
        args: Vec<OsString>,
    },
    /// Parsed so the name is reserved; not implemented in this milestone.
    NotYet(&'static str),
}

/// Agent names match `^[a-z0-9][a-z0-9-]*$`.
pub fn valid_agent_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn utf8(arg: &OsString, what: &str) -> Result<String, String> {
    arg.to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{what} is not valid UTF-8"))
}

fn exactly_one(rest: &[OsString], usage: &str) -> Result<String, String> {
    match rest {
        [one] => utf8(one, "argument"),
        _ => Err(format!("usage: {usage}")),
    }
}

fn none(rest: &[OsString], usage: &str) -> Result<(), String> {
    if rest.is_empty() {
        Ok(())
    } else {
        Err(format!("usage: {usage}"))
    }
}

/// `[OPTIONS] NAME [ARGS...]`, for both `sv2 run` and the shorthand.
fn parse_run(args: &[OsString], is_agent: &dyn Fn(&str) -> bool) -> Result<Cmd, String> {
    let mut profile = DEFAULT_PROFILE.to_string();
    let mut i = 0;
    while i < args.len() {
        let Some(a) = args[i].to_str() else { break };
        if !a.starts_with('-') {
            break;
        }
        if a == "--profile" || a == "-p" {
            i += 1;
            let value = args
                .get(i)
                .ok_or_else(|| format!("{a} needs a profile name"))?;
            profile = utf8(value, "profile name")?;
        } else if let Some(value) = a.strip_prefix("--profile=") {
            profile = value.to_string();
        } else {
            return Err(format!("unknown option '{a}'"));
        }
        i += 1;
    }
    if !valid_profile(&profile) {
        return Err(format!(
            "invalid profile name '{profile}': use 1-16 characters from a-z and 0-9"
        ));
    }
    let agent = utf8(
        args.get(i)
            .ok_or("missing agent name; try: sv2 run shell")?,
        "agent name",
    )?;
    if !valid_agent_name(&agent) || !is_agent(&agent) {
        return Err(format!("unknown agent '{agent}'"));
    }
    Ok(Cmd::Run {
        profile,
        agent,
        args: args[i + 1..].to_vec(),
    })
}

pub fn parse(args: &[OsString], is_agent: &dyn Fn(&str) -> bool) -> Result<Cmd, String> {
    let Some(first) = args.first() else {
        return Ok(Cmd::Help);
    };
    let first = utf8(first, "command")?;
    let rest = &args[1..];
    match first.as_str() {
        "-h" | "--help" | "help" => Ok(Cmd::Help),
        "-V" | "--version" => Ok(Cmd::Version),
        "install" => none(rest, "sv2 install").map(|()| Cmd::Install),
        "uninstall" => none(rest, "sv2 uninstall").map(|()| Cmd::Uninstall),
        "user" => {
            const USAGE: &str = "sv2 user allow USER | sv2 user deny USER";
            match rest.first().and_then(|a| a.to_str()) {
                Some("allow") => exactly_one(&rest[1..], USAGE).map(Cmd::UserAllow),
                Some("deny") => exactly_one(&rest[1..], USAGE).map(Cmd::UserDeny),
                _ => Err(format!("usage: {USAGE}")),
            }
        }
        "profile" => {
            const USAGE: &str = "sv2 profile add NAME | sv2 profile remove NAME | sv2 profile list";
            match rest.first().and_then(|a| a.to_str()) {
                Some("add") => exactly_one(&rest[1..], USAGE).map(Cmd::ProfileAdd),
                Some("remove") => exactly_one(&rest[1..], USAGE).map(Cmd::ProfileRemove),
                Some("list") => none(&rest[1..], USAGE).map(|()| Cmd::ProfileList),
                _ => Err(format!("usage: {USAGE}")),
            }
        }
        "run" => parse_run(rest, is_agent),
        "repair" => Ok(Cmd::NotYet("repair")),
        "doctor" => Ok(Cmd::NotYet("doctor")),
        // Shorthand: `sv2 --profile work shell`.
        s if s.starts_with('-') => parse_run(args, is_agent),
        // Shorthand: `sv2 shell`. Subcommands were matched above, so they
        // always win over an agent of the same name.
        s if valid_agent_name(s) && is_agent(s) => parse_run(args, is_agent),
        s => Err(format!("unknown command or agent '{s}'; see: sv2 --help")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    fn only_shell(name: &str) -> bool {
        name == "shell"
    }

    fn p(v: &[&str]) -> Result<Cmd, String> {
        parse(&os(v), &only_shell)
    }

    fn run(profile: &str, args: &[&str]) -> Cmd {
        Cmd::Run {
            profile: profile.into(),
            agent: "shell".into(),
            args: os(args),
        }
    }

    #[test]
    fn subcommands() {
        assert_eq!(p(&[]), Ok(Cmd::Help));
        assert_eq!(p(&["--help"]), Ok(Cmd::Help));
        assert_eq!(p(&["--version"]), Ok(Cmd::Version));
        assert_eq!(p(&["install"]), Ok(Cmd::Install));
        assert_eq!(p(&["uninstall"]), Ok(Cmd::Uninstall));
        assert_eq!(
            p(&["user", "allow", "sam"]),
            Ok(Cmd::UserAllow("sam".into()))
        );
        assert_eq!(p(&["user", "deny", "sam"]), Ok(Cmd::UserDeny("sam".into())));
        assert_eq!(
            p(&["profile", "add", "work"]),
            Ok(Cmd::ProfileAdd("work".into()))
        );
        assert_eq!(
            p(&["profile", "remove", "work"]),
            Ok(Cmd::ProfileRemove("work".into()))
        );
        assert_eq!(p(&["profile", "list"]), Ok(Cmd::ProfileList));
        assert_eq!(p(&["doctor"]), Ok(Cmd::NotYet("doctor")));
        assert_eq!(p(&["repair", "x"]), Ok(Cmd::NotYet("repair")));
    }

    #[test]
    fn subcommand_arity() {
        for bad in [
            &["install", "x"][..],
            &["uninstall", "--all"][..],
            &["user"][..],
            &["user", "allow"][..],
            &["user", "allow", "a", "b"][..],
            &["user", "list"][..],
            &["profile"][..],
            &["profile", "add"][..],
            &["profile", "list", "x"][..],
        ] {
            assert!(p(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn run_and_shorthand_are_the_same() {
        assert_eq!(p(&["run", "shell"]), Ok(run("default", &[])));
        assert_eq!(p(&["shell"]), Ok(run("default", &[])));
        assert_eq!(
            p(&["run", "--profile", "work", "shell"]),
            Ok(run("work", &[]))
        );
        assert_eq!(p(&["--profile", "work", "shell"]), Ok(run("work", &[])));
        assert_eq!(p(&["--profile=work", "shell"]), Ok(run("work", &[])));
    }

    #[test]
    fn everything_after_the_agent_is_passed_unchanged() {
        let args = ["--profile", "x", "--", "-h", "install", "a b", "$HOME", ""];
        let mut line = vec!["shell"];
        line.extend(args);
        assert_eq!(p(&line), Ok(run("default", &args)));
        let mut line = vec!["run", "--profile", "work", "shell"];
        line.extend(args);
        assert_eq!(p(&line), Ok(run("work", &args)));
    }

    #[test]
    fn options_only_before_the_agent() {
        assert!(p(&["run", "--bogus", "shell"]).is_err());
        assert!(p(&["--no-sandbox", "shell"]).is_err());
        assert!(p(&["run", "--profile"]).is_err());
        assert!(p(&["run", "--profile", "work"]).is_err());
        assert!(p(&["run"]).is_err());
        for bad in ["Work", "a-b", "", "../x", "abcdefghijklmnopq"] {
            assert!(p(&["run", "--profile", bad, "shell"]).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn unknown_names_are_errors_and_never_run() {
        let err = p(&["claude"]).unwrap_err();
        assert!(err.contains("unknown command or agent"), "{err}");
        assert!(p(&["run", "claude"]).is_err());
        assert!(p(&["/bin/sh"]).is_err());
        assert!(p(&["run", "/bin/sh"]).is_err());
        assert!(p(&["run", "../shell"]).is_err());
    }

    /// An agent named like a subcommand cannot change what the subcommand
    /// does; it is reachable only through `sv2 run NAME`.
    #[test]
    fn subcommands_win_over_agents() {
        let everything = |_: &str| true;
        assert_eq!(parse(&os(&["install"]), &everything), Ok(Cmd::Install));
        assert_eq!(parse(&os(&["uninstall"]), &everything), Ok(Cmd::Uninstall));
        assert_eq!(
            parse(&os(&["doctor"]), &everything),
            Ok(Cmd::NotYet("doctor"))
        );
        assert_eq!(
            parse(&os(&["run", "install"]), &everything),
            Ok(Cmd::Run {
                profile: "default".into(),
                agent: "install".into(),
                args: vec![]
            })
        );
        for name in SUBCOMMANDS {
            assert!(
                !matches!(parse(&os(&[name]), &everything), Ok(Cmd::Run { .. })),
                "{name} was treated as an agent"
            );
        }
    }

    #[test]
    fn agent_names() {
        for ok in ["shell", "claude", "open-code", "0x"] {
            assert!(valid_agent_name(ok), "{ok}");
        }
        for bad in ["", "-x", "Shell", "a_b", "a/b", "a.b", "a b"] {
            assert!(!valid_agent_name(bad), "{bad:?}");
        }
    }
}
