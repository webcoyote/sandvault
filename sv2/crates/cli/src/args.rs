//! Command-line parsing.
//!
//! ```text
//! sv2 install | uninstall [--all-profiles]
//! sv2 user allow USER | user deny USER
//! sv2 profile add NAME | profile remove NAME | profile list
//! sv2 run [OPTIONS] NAME [ARGS...]   OPTIONS: --profile NAME, --native
//! sv2 [OPTIONS] NAME [ARGS...]        shorthand for sv2 run
//! sv2 repair PATH | doctor
//! sv2 agent new NAME [--from AGENT]
//! sv2 deploy-key [--profile NAME] OWNER/REPO [--write]
//! sv2 gh-token [--profile NAME] [--owner NAME] [--days N] [--no-browser]
//!              [login | status | logout]
//! ```
//!
//! Options come before NAME. Everything after NAME goes to the agent
//! unchanged. Subcommands always win over agent names, and a first argument
//! that is neither is an error: nothing is ever looked up on `PATH`.
//!
//! A subcommand, a verb after `user` or `profile`, and an agent name may be
//! shortened to any prefix that fits exactly one name: `sv2 s` and `sv2 sh`
//! are `sv2 shell`. See `resolve` for the rules.

use std::ffi::OsString;

use sv2_common::{valid_profile, DEFAULT_PROFILE};

use crate::agent::valid_agent_name;
use crate::credentials::TokenVerb;

pub const SUBCOMMANDS: &[&str] = &[
    "install",
    "uninstall",
    "user",
    "profile",
    "run",
    "repair",
    "doctor",
    "deploy-key",
    "gh-token",
    "agent",
    "help",
];

#[derive(Debug, PartialEq, Eq)]
pub enum Cmd {
    Help,
    Version,
    Install,
    Uninstall {
        /// Remove every user's profiles too, instead of refusing.
        all_profiles: bool,
    },
    UserAllow(String),
    UserDeny(String),
    ProfileAdd(String),
    ProfileRemove(String),
    ProfileList,
    Run {
        profile: String,
        /// Install the agent with its own installer, inside the sandbox.
        native: bool,
        agent: String,
        args: Vec<OsString>,
    },
    Repair(OsString),
    Doctor,
    /// Make a deploy key for a repository in a profile's sandbox.
    DeployKey {
        profile: String,
        /// As the user wrote it; checked where it is used.
        repo: String,
        /// Ask for a key that may push.
        write: bool,
    },
    /// Give a profile's sandbox a read-only token for GitHub's API.
    GhToken {
        profile: String,
        verb: TokenVerb,
        owner: Option<String>,
        days: Option<String>,
        open_browser: bool,
    },
    /// Create the user's definition of an agent.
    AgentNew {
        name: String,
        /// The definition to copy; the template when None.
        from: Option<String>,
    },
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

const USER_VERBS: &[&str] = &["allow", "deny"];
const AGENT_VERBS: &[&str] = &["new"];
const PROFILE_VERBS: &[&str] = &["add", "remove", "list"];

/// Commands that delete things and ask nothing first. A slip of one letter
/// must not reach them, so they are never matched by a prefix.
const WRITTEN_IN_FULL: &[&str] = &["uninstall", "remove"];

/// The name in `names` that `word` stands for, or None when it stands for
/// none of them.
///
/// - A whole name always matches itself, also when it is the start of a
///   longer one.
/// - Otherwise `word` must be the start of exactly one name. When it starts
///   several, nothing is chosen and the error lists them: a command or agent
///   added later can turn a prefix that worked into an error, never into
///   another command.
/// - A name in `WRITTEN_IN_FULL` still counts towards that, but is refused
///   when it is the only match.
fn resolve<'a>(word: &str, names: &[&'a str]) -> Result<Option<&'a str>, String> {
    if let Some(exact) = names.iter().find(|n| **n == word) {
        return Ok(Some(exact));
    }
    if word.is_empty() {
        return Ok(None);
    }
    let mut matches: Vec<&str> = Vec::new();
    for n in names.iter().filter(|n| n.starts_with(word)) {
        if !matches.contains(n) {
            matches.push(n);
        }
    }
    match matches.as_slice() {
        [] => Ok(None),
        [one] if WRITTEN_IN_FULL.contains(one) => Err(format!(
            "'{word}' is short for '{one}', which must be written in full"
        )),
        [one] => Ok(Some(one)),
        many => Err(format!(
            "'{word}' is ambiguous: it could be {}",
            many.join(", ")
        )),
    }
}

/// `[OPTIONS] NAME [ARGS...]`, for both `sv2 run` and the shorthand.
fn parse_run(args: &[OsString], agents: &[&str]) -> Result<Cmd, String> {
    let mut profile = DEFAULT_PROFILE.to_string();
    let mut native = false;
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
        } else if a == "--native" {
            native = true;
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
    let agent = match resolve(&agent, agents)? {
        Some(name) if valid_agent_name(name) => name.to_string(),
        _ => return Err(format!("unknown agent '{agent}'")),
    };
    Ok(Cmd::Run {
        profile,
        native,
        agent,
        args: args[i + 1..].to_vec(),
    })
}

/// The words of a command that takes options anywhere: `-p NAME`,
/// `--profile NAME` and `--profile=NAME` are taken out, and the rest is
/// returned with the profile.
fn take_profile(rest: &[OsString]) -> Result<(String, Vec<String>), String> {
    let mut profile = DEFAULT_PROFILE.to_string();
    let mut words = Vec::new();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        let a = utf8(arg, "argument")?;
        if a == "-p" || a == "--profile" {
            let value = it
                .next()
                .ok_or_else(|| format!("{a} needs a profile name"))?;
            profile = utf8(value, "profile name")?;
        } else if let Some(value) = a.strip_prefix("--profile=") {
            profile = value.to_string();
        } else {
            words.push(a);
        }
    }
    if !valid_profile(&profile) {
        return Err(format!(
            "invalid profile name '{profile}': use 1-16 characters from a-z and 0-9"
        ));
    }
    Ok((profile, words))
}

fn parse_deploy_key(rest: &[OsString]) -> Result<Cmd, String> {
    const USAGE: &str = "usage: sv2 deploy-key [--profile NAME] OWNER/REPO [--write]";
    let (profile, words) = take_profile(rest)?;
    let mut repo = None;
    let mut write = false;
    for w in words {
        match w.as_str() {
            "--write" => write = true,
            w if w.starts_with('-') => return Err(format!("unknown option '{w}'; {USAGE}")),
            _ if repo.is_some() => return Err(USAGE.to_string()),
            _ => repo = Some(w),
        }
    }
    Ok(Cmd::DeployKey {
        profile,
        repo: repo.ok_or(USAGE)?,
        write,
    })
}

fn parse_gh_token(rest: &[OsString]) -> Result<Cmd, String> {
    const USAGE: &str = "usage: sv2 gh-token [--profile NAME] [--owner NAME] [--days N] [--no-browser] [login | status | logout]";
    let (profile, words) = take_profile(rest)?;
    let (mut verb, mut owner, mut days, mut open_browser) = (None, None, None, true);
    let mut it = words.into_iter();
    while let Some(w) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match w.as_str() {
            "--owner" => owner = Some(value("--owner")?),
            "--days" => days = Some(value("--days")?),
            "--no-browser" => open_browser = false,
            "login" | "status" | "logout" if verb.is_none() => {
                verb = Some(match w.as_str() {
                    "login" => TokenVerb::Login,
                    "status" => TokenVerb::Status,
                    _ => TokenVerb::Logout,
                });
            }
            _ => return Err(USAGE.to_string()),
        }
    }
    Ok(Cmd::GhToken {
        profile,
        verb: verb.unwrap_or(TokenVerb::Login),
        owner,
        days,
        open_browser,
    })
}

/// `agents` is every agent name there is. Nothing else is ever run.
pub fn parse(args: &[OsString], agents: &[&str]) -> Result<Cmd, String> {
    let Some(first) = args.first() else {
        return Ok(Cmd::Help);
    };
    let first = utf8(first, "command")?;
    let rest = &args[1..];
    match first.as_str() {
        "-h" | "--help" => return Ok(Cmd::Help),
        "-V" | "--version" => return Ok(Cmd::Version),
        // Shorthand: `sv2 --profile work shell`.
        s if s.starts_with('-') => return parse_run(args, agents),
        _ => {}
    }
    // Subcommands come first in the list, so one always wins over an agent
    // of the same name; a prefix that fits a subcommand and an agent is
    // ambiguous and runs neither.
    let mut names = SUBCOMMANDS.to_vec();
    names.extend(agents);
    let Some(name) = resolve(&first, &names)? else {
        return Err(format!(
            "unknown command or agent '{first}'; see: sv2 --help"
        ));
    };
    let verb = |verbs: &[&'static str]| match rest.first().and_then(|a| a.to_str()) {
        Some(word) => resolve(word, verbs),
        None => Ok(None),
    };
    match name {
        "help" => Ok(Cmd::Help),
        "install" => none(rest, "sv2 install").map(|()| Cmd::Install),
        "uninstall" => match rest.iter().map(|a| a.to_str()).collect::<Vec<_>>()[..] {
            [] => Ok(Cmd::Uninstall {
                all_profiles: false,
            }),
            // Written in full, like the command: it deletes sandbox homes.
            [Some("--all-profiles")] => Ok(Cmd::Uninstall { all_profiles: true }),
            _ => Err("usage: sv2 uninstall [--all-profiles]".to_string()),
        },
        "user" => {
            const USAGE: &str = "sv2 user allow USER | sv2 user deny USER";
            match verb(USER_VERBS)? {
                Some("allow") => exactly_one(&rest[1..], USAGE).map(Cmd::UserAllow),
                Some("deny") => exactly_one(&rest[1..], USAGE).map(Cmd::UserDeny),
                _ => Err(format!("usage: {USAGE}")),
            }
        }
        "profile" => {
            const USAGE: &str = "sv2 profile add NAME | sv2 profile remove NAME | sv2 profile list";
            match verb(PROFILE_VERBS)? {
                Some("add") => exactly_one(&rest[1..], USAGE).map(Cmd::ProfileAdd),
                Some("remove") => exactly_one(&rest[1..], USAGE).map(Cmd::ProfileRemove),
                Some("list") => none(&rest[1..], USAGE).map(|()| Cmd::ProfileList),
                _ => Err(format!("usage: {USAGE}")),
            }
        }
        "run" => parse_run(rest, agents),
        "repair" => match rest {
            [path] => Ok(Cmd::Repair(path.clone())),
            _ => Err("usage: sv2 repair PATH".to_string()),
        },
        "doctor" => none(rest, "sv2 doctor").map(|()| Cmd::Doctor),
        "deploy-key" => parse_deploy_key(rest),
        "gh-token" => parse_gh_token(rest),
        "agent" => {
            const USAGE: &str = "sv2 agent new NAME [--from AGENT]";
            let words: Option<Vec<&str>> = rest.iter().map(|a| a.to_str()).collect();
            let words = words.ok_or("an argument is not valid UTF-8")?;
            match (verb(AGENT_VERBS)?, words.get(1..).unwrap_or(&[])) {
                (Some("new"), [name]) => Ok(Cmd::AgentNew {
                    name: (*name).into(),
                    from: None,
                }),
                (Some("new"), [name, "--from", other]) => Ok(Cmd::AgentNew {
                    name: (*name).into(),
                    from: Some((*other).into()),
                }),
                _ => Err(format!("usage: {USAGE}")),
            }
        }
        // Shorthand: `sv2 shell`. `parse_run` reads the same word again and,
        // among agents alone, finds the same one.
        _ => parse_run(args, agents),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    const UNINSTALL: Cmd = Cmd::Uninstall {
        all_profiles: false,
    };

    fn p(v: &[&str]) -> Result<Cmd, String> {
        parse(&os(v), &["shell"])
    }

    fn run_agent(agent: &str) -> Cmd {
        Cmd::Run {
            profile: "default".into(),
            native: false,
            agent: agent.into(),
            args: vec![],
        }
    }

    fn run(profile: &str, args: &[&str]) -> Cmd {
        Cmd::Run {
            profile: profile.into(),
            native: false,
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
        assert_eq!(p(&["uninstall"]), Ok(UNINSTALL));
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
        assert_eq!(p(&["doctor"]), Ok(Cmd::Doctor));
        assert_eq!(p(&["repair", "x"]), Ok(Cmd::Repair("x".into())));
        // The path is not an option list and not an agent line.
        assert_eq!(
            p(&["repair", "--profile"]),
            Ok(Cmd::Repair("--profile".into()))
        );
    }

    #[test]
    fn uninstall_removes_profiles_only_when_told_to() {
        assert_eq!(p(&["uninstall"]), Ok(UNINSTALL));
        assert_eq!(
            p(&["uninstall", "--all-profiles"]),
            Ok(Cmd::Uninstall { all_profiles: true })
        );
        // Not shortened, not guessed at, not repeated.
        for bad in [
            &["uninstall", "--all"][..],
            &["uninstall", "--all-p"][..],
            &["uninstall", "all-profiles"][..],
            &["uninstall", "-a"][..],
            &["uninstall", "--all-profiles", "--all-profiles"][..],
            &["uninstall", "--all-profiles", "x"][..],
        ] {
            assert!(p(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn agent_new() {
        let new = |name: &str, from: Option<&str>| {
            Ok(Cmd::AgentNew {
                name: name.into(),
                from: from.map(Into::into),
            })
        };
        assert_eq!(p(&["agent", "new", "codex"]), new("codex", None));
        assert_eq!(
            p(&["agent", "new", "mine", "--from", "claude"]),
            new("mine", Some("claude"))
        );
        assert_eq!(p(&["a", "n", "codex"]), new("codex", None));
        // The names are checked where the definition is made, not here: the
        // name of a new agent matches no existing one.
        assert_eq!(p(&["agent", "new", "Bad Name"]), new("Bad Name", None));
        for bad in [
            &["agent"][..],
            &["agent", "new"][..],
            &["agent", "list"][..],
            &["agent", "new", "a", "b"][..],
            &["agent", "new", "a", "--from"][..],
            &["agent", "new", "a", "--from", "b", "c"][..],
            &["agent", "new", "--from", "b", "a"][..],
        ] {
            assert!(p(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn deploy_key() {
        let key = |profile: &str, write: bool| {
            Ok(Cmd::DeployKey {
                profile: profile.into(),
                repo: "octocat/Hello-World".into(),
                write,
            })
        };
        assert_eq!(
            p(&["deploy-key", "octocat/Hello-World"]),
            key("default", false)
        );
        assert_eq!(
            p(&["deploy-key", "octocat/Hello-World", "--write"]),
            key("default", true)
        );
        assert_eq!(
            p(&["deploy-key", "-p", "work", "--write", "octocat/Hello-World"]),
            key("work", true)
        );
        assert_eq!(
            p(&["deploy-key", "octocat/Hello-World", "--profile=work"]),
            key("work", false)
        );
        for bad in [
            &["deploy-key"][..],
            &["deploy-key", "--write"][..],
            &["deploy-key", "a/b", "c/d"][..],
            &["deploy-key", "a/b", "--force"][..],
            &["deploy-key", "a/b", "-w"][..],
            &["deploy-key", "a/b", "-p"][..],
            &["deploy-key", "a/b", "-p", "Bad"][..],
        ] {
            assert!(p(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn gh_token() {
        let token = |profile: &str, verb, owner: Option<&str>, days: Option<&str>, open| {
            Ok(Cmd::GhToken {
                profile: profile.into(),
                verb,
                owner: owner.map(Into::into),
                days: days.map(Into::into),
                open_browser: open,
            })
        };
        assert_eq!(
            p(&["gh-token"]),
            token("default", TokenVerb::Login, None, None, true)
        );
        assert_eq!(
            p(&["gh-token", "status", "-p", "work"]),
            token("work", TokenVerb::Status, None, None, true)
        );
        assert_eq!(
            p(&[
                "gh-token",
                "--owner",
                "octo-org",
                "--days",
                "30",
                "--no-browser",
                "login"
            ]),
            token(
                "default",
                TokenVerb::Login,
                Some("octo-org"),
                Some("30"),
                false
            )
        );
        assert_eq!(
            p(&["gh-token", "logout"]),
            token("default", TokenVerb::Logout, None, None, true)
        );
        for bad in [
            &["gh-token", "revoke"][..],
            &["gh-token", "login", "status"][..],
            &["gh-token", "--owner"][..],
            &["gh-token", "--days"][..],
            &["gh-token", "--token", "x"][..],
        ] {
            assert!(p(bad).is_err(), "{bad:?}");
        }
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
            &["repair"][..],
            &["repair", "a", "b"][..],
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
    fn native_is_an_option_before_the_agent() {
        let native = |profile: &str| Cmd::Run {
            profile: profile.into(),
            native: true,
            agent: "shell".into(),
            args: vec![],
        };
        assert_eq!(p(&["run", "--native", "shell"]), Ok(native("default")));
        assert_eq!(p(&["--native", "shell"]), Ok(native("default")));
        assert_eq!(p(&["--native", "-p", "work", "s"]), Ok(native("work")));
        assert_eq!(p(&["-p", "work", "--native", "s"]), Ok(native("work")));
        // After the agent it is the agent's argument.
        assert_eq!(p(&["shell", "--native"]), Ok(run("default", &["--native"])));
        // It is not shortened, and takes no value.
        assert!(p(&["--nat", "shell"]).is_err());
        assert!(p(&["--native=yes", "shell"]).is_err());
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
        let everything = SUBCOMMANDS;
        assert_eq!(parse(&os(&["install"]), everything), Ok(Cmd::Install));
        assert_eq!(parse(&os(&["uninstall"]), everything), Ok(UNINSTALL));
        assert_eq!(parse(&os(&["doctor"]), everything), Ok(Cmd::Doctor));
        assert_eq!(
            parse(&os(&["run", "install"]), everything),
            Ok(run_agent("install"))
        );
        for name in SUBCOMMANDS {
            assert!(
                !matches!(parse(&os(&[name]), everything), Ok(Cmd::Run { .. })),
                "{name} was treated as an agent"
            );
        }
        // Nor does a prefix reach the agent past the subcommand.
        assert_eq!(parse(&os(&["inst"]), everything), Ok(Cmd::Install));
        assert_eq!(parse(&os(&["doc"]), everything), Ok(Cmd::Doctor));
    }

    #[test]
    fn a_prefix_that_fits_one_name_is_that_name() {
        for short in ["s", "sh", "she", "shel", "shell"] {
            assert_eq!(p(&[short]), Ok(run("default", &[])), "{short}");
            assert_eq!(p(&["run", short]), Ok(run("default", &[])), "{short}");
            assert_eq!(p(&["ru", "-p", "work", short]), Ok(run("work", &[])));
            assert_eq!(p(&["-p", "work", short]), Ok(run("work", &[])));
        }
        // What follows the agent is still passed unchanged.
        assert_eq!(p(&["s", "s", "ru"]), Ok(run("default", &["s", "ru"])));
        assert_eq!(p(&["i"]), Ok(Cmd::Install));
        assert_eq!(p(&["h"]), Ok(Cmd::Help));
        assert_eq!(p(&["do"]), Ok(Cmd::Doctor));
        assert_eq!(p(&["rep", "x"]), Ok(Cmd::Repair("x".into())));
        assert_eq!(p(&["us", "a", "sam"]), Ok(Cmd::UserAllow("sam".into())));
        assert_eq!(p(&["us", "d", "sam"]), Ok(Cmd::UserDeny("sam".into())));
        assert_eq!(p(&["p", "a", "work"]), Ok(Cmd::ProfileAdd("work".into())));
        assert_eq!(p(&["p", "l"]), Ok(Cmd::ProfileList));
        // Arity is checked as for the whole word.
        assert!(p(&["i", "x"]).is_err());
        assert!(p(&["p", "l", "x"]).is_err());
        // Not a prefix: a longer word, another case, the empty word.
        for bad in ["shells", "S", "", "hell"] {
            assert!(p(&[bad]).is_err(), "{bad:?}");
            assert!(p(&["run", bad]).is_err(), "{bad:?}");
        }
        assert!(p(&["profile", ""]).is_err());
    }

    #[test]
    fn an_ambiguous_prefix_runs_nothing() {
        let err = p(&["r", "x"]).unwrap_err();
        assert_eq!(err, "'r' is ambiguous: it could be run, repair");
        let err = p(&["u"]).unwrap_err();
        assert_eq!(err, "'u' is ambiguous: it could be uninstall, user");
        // Among agents, and between an agent and a subcommand.
        let agents = &["shell", "shout", "pi", "sh"][..];
        let err = parse(&os(&["s"]), agents).unwrap_err();
        assert_eq!(err, "'s' is ambiguous: it could be shell, shout, sh");
        assert_eq!(parse(&os(&["run", "sho"]), agents), Ok(run_agent("shout")));
        let err = parse(&os(&["p", "list"]), agents).unwrap_err();
        assert_eq!(err, "'p' is ambiguous: it could be profile, pi");
        // After `run` or an option only an agent can follow.
        assert_eq!(parse(&os(&["run", "p"]), agents), Ok(run_agent("pi")));
        assert_eq!(
            parse(&os(&["-p", "work", "p"]), agents),
            Ok(Cmd::Run {
                profile: "work".into(),
                native: false,
                agent: "pi".into(),
                args: vec![]
            })
        );
        // A whole name is itself, though longer names start with it.
        assert_eq!(parse(&os(&["sh"]), agents), Ok(run_agent("sh")));
        assert_eq!(parse(&os(&["pi"]), agents), Ok(run_agent("pi")));
        // An agent named like a subcommand is one name, not two.
        assert_eq!(parse(&os(&["inst"]), &["install"]), Ok(Cmd::Install));
    }

    #[test]
    fn commands_that_delete_are_written_in_full() {
        for short in ["un", "unin", "uninstal"] {
            let err = p(&[short]).unwrap_err();
            assert!(
                err.contains("'uninstall', which must be written in full"),
                "{err}"
            );
        }
        for short in ["r", "rem", "remov"] {
            let err = p(&["profile", short, "work"]).unwrap_err();
            assert!(
                err.contains("'remove', which must be written in full"),
                "{err}"
            );
        }
        assert_eq!(p(&["uninstall"]), Ok(UNINSTALL));
        assert_eq!(
            p(&["p", "remove", "work"]),
            Ok(Cmd::ProfileRemove("work".into()))
        );
        // An agent cannot make a prefix of `uninstall` mean something else...
        let err = parse(&os(&["un"]), &["unzip"]).unwrap_err();
        assert_eq!(err, "'un' is ambiguous: it could be uninstall, unzip");
        // ...and an agent named `remove` is spelled out too: the list of
        // such words is one list, whatever the word names.
        assert!(parse(&os(&["run", "rem"]), &["remove"]).is_err());
        assert_eq!(
            parse(&os(&["run", "remove"]), &["remove"]),
            Ok(run_agent("remove"))
        );
    }
}
