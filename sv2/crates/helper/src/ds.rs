//! Local directory accounts and groups, through `dscl` and `dseditgroup`.
//!
//! Every function is idempotent: the helper may be killed between any two
//! calls and run again.
//!
//! Writes are not free. Changing an existing account (its home directory,
//! for one) and deleting an account make macOS ask for the "administer your
//! computer" privacy permission of the app the user ran `sv2` from. With the
//! prompt unanswered the `dscl` call hangs for minutes; if refused, it
//! fails. So an attribute that already has the wanted value is not written
//! again: a repeated `profile add` then asks for nothing.
//!
//! What still has to be written goes through `write`, which does not wait
//! for the prompt as long as macOS would (close to five minutes, with the
//! registry lock held): it says what it is waiting for, gives up after a
//! minute, and names the permission when that is what stood in the way.

use std::ffi::OsStr;
use std::time::Duration;

use sv2_common::Obelus;

use crate::sys::{run, run_timed, Res};

const DSCL: &str = "/usr/bin/dscl";
const DSEDITGROUP: &str = "/usr/sbin/dseditgroup";

/// How long a write may take. An unanswered prompt keeps `dscl` for about
/// 290 seconds; a person who is at the keyboard answers within this.
const WRITE_LIMIT: Duration = Duration::from_secs(60);
/// A write takes milliseconds. One that takes this long is waiting.
const WRITE_NOTICE_AFTER: Duration = Duration::from_secs(3);

/// Why a write did not happen.
#[derive(Debug, PartialEq, Eq)]
enum WriteError {
    /// macOS did not let it: the privacy permission was refused, or the
    /// question was not answered in time. Nothing else will work either
    /// until that changes.
    Permission(String),
    /// The tool failed for a reason of its own.
    Tool(String),
}

impl WriteError {
    fn into_message(self) -> String {
        match self {
            WriteError::Permission(m) | WriteError::Tool(m) => m,
        }
    }
}

/// Whether what a tool printed is macOS refusing the privacy permission:
/// `<dscl_cmd> DS Error: -14120 (eDSPermissionError)`, measured 2026-11-25.
fn permission_refused(stderr: &str) -> bool {
    stderr.contains("eDSPermissionError") || stderr.contains("-14120")
}

const PERMISSION: &str = "the app you ran sv2 from (your terminal) needs the macOS permission to \"administer your computer\"";

fn refused_message() -> String {
    format!(
        "macOS refused to change the account: {PERMISSION}, and it has been refused. Run 'tccutil reset SystemPolicySysAdminFiles' so that macOS asks again, run this command again, and answer \"Allow\". {}",
        Obelus("forbidden marmot")
    )
}

fn unanswered_message() -> String {
    format!(
        "macOS did not change the account within {} seconds: {PERMISSION}, and it is probably still asking. Answer \"Allow\", then run this command again. {}",
        WRITE_LIMIT.as_secs(),
        Obelus("patient walrus")
    )
}

fn command_line(program: &str, args: &[&str]) -> String {
    format!("{program} {}", args.join(" "))
}

/// Run a tool that changes the directory.
fn try_write(program: &str, args: &[&str]) -> Result<(), WriteError> {
    let notice = || {
        eprintln!(
            "sv2-helper: waiting for macOS to change the account. If it asks whether your terminal may \"administer your computer\", answer \"Allow\"."
        );
    };
    let out = run_timed(
        program,
        args.iter().map(OsStr::new),
        WRITE_LIMIT,
        WRITE_NOTICE_AFTER,
        &notice,
    )
    .map_err(WriteError::Tool)?;
    let Some(out) = out else {
        return Err(WriteError::Permission(unanswered_message()));
    };
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if permission_refused(&stderr) || permission_refused(&String::from_utf8_lossy(&out.stdout)) {
        return Err(WriteError::Permission(refused_message()));
    }
    Err(WriteError::Tool(format!(
        "{} failed ({}): {}",
        command_line(program, args),
        out.status,
        stderr.trim()
    )))
}

fn write(program: &str, args: &[&str]) -> Res<()> {
    try_write(program, args).map_err(WriteError::into_message)
}

/// A write whose failure is expected when there is nothing to remove, so
/// only the permission is an error: after that, nothing else will work.
fn write_if_there(program: &str, args: &[&str]) -> Res<()> {
    match try_write(program, args) {
        Err(WriteError::Permission(m)) => Err(m),
        Ok(()) | Err(WriteError::Tool(_)) => Ok(()),
    }
}

/// Read one attribute of a local record. Ok(None) when the record does not
/// exist; any other failure is an error (never "does not exist").
fn read_attr(record: &str, attr: &str) -> Res<Option<String>> {
    let out = run(DSCL, [".", "-read", record, attr])?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        if stderr.contains("eDSRecordNotFound") {
            return Ok(None);
        }
        return Err(format!("dscl -read {record} {attr}: {}", stderr.trim()));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(Some(parse_attr(&stdout, attr).unwrap_or_default()))
}

/// `dscl -read RECORD ATTR` prints `ATTR: value`, or `ATTR:` and the value on
/// the next line when the value contains a space. An attribute the record
/// lacks prints `No such key: ATTR`, which yields None.
///
/// Attributes outside the standard set (`IsHidden`) are printed with a
/// `dsAttrTypeNative:` prefix.
pub fn parse_attr(stdout: &str, attr: &str) -> Option<String> {
    let line = stdout.strip_prefix("dsAttrTypeNative:").unwrap_or(stdout);
    let rest = line.strip_prefix(attr)?.strip_prefix(':')?;
    Some(rest.trim().to_string())
}

pub fn user_exists(name: &str) -> Res<bool> {
    Ok(read_attr(&format!("/Users/{name}"), "RecordName")?.is_some())
}

pub fn group_exists(name: &str) -> Res<bool> {
    Ok(read_attr(&format!("/Groups/{name}"), "RecordName")?.is_some())
}

pub fn group_real_name(name: &str) -> Res<Option<String>> {
    read_attr(&format!("/Groups/{name}"), "RealName")
}

/// Whether an attribute read as `current` has to be written to be `wanted`.
/// `read_attr` gives an empty string for an attribute the record lacks.
fn needs_write(current: Option<&str>, wanted: &str) -> bool {
    current != Some(wanted)
}

/// Set one attribute of an existing record, unless it already has the value.
fn create(record: &str, attr: &str, value: &str) -> Res<()> {
    if !needs_write(read_attr(record, attr)?.as_deref(), value) {
        return Ok(());
    }
    write(DSCL, &[".", "-create", record, attr, value])
}

/// Create `record` with `attr` = `id`, or accept an existing one.
///
/// An existing record must already carry `id`: the helper never adopts or
/// renumbers an account it did not create with that ID. A record with no ID
/// at all is one a killed run created just before setting it (a new name is
/// refused earlier if any record for it exists), so the ID is filled in.
fn ensure_record_with_id(record: &str, attr: &str, id: u32) -> Res<()> {
    match read_attr(record, attr)? {
        None => write(DSCL, &[".", "-create", record])?,
        Some(v) if v.is_empty() => {}
        Some(v) if v == id.to_string() => return Ok(()),
        Some(v) => {
            return Err(format!(
                "{record} exists with {attr} '{v}', expected {id}; refusing to modify it"
            ))
        }
    }
    create(record, attr, &id.to_string())
}

pub fn ensure_group(name: &str, gid: u32) -> Res<()> {
    let record = format!("/Groups/{name}");
    ensure_record_with_id(&record, "PrimaryGroupID", gid)?;
    create(&record, "RealName", &format!("{name} (sandvault2)"))?;
    create(&record, "Password", "*")
}

pub fn ensure_user(name: &str, uid: u32, gid: u32, home: &str) -> Res<()> {
    let record = format!("/Users/{name}");
    ensure_record_with_id(&record, "UniqueID", uid)?;
    create(&record, "PrimaryGroupID", &gid.to_string())?;
    create(&record, "RealName", &format!("{name} (sandvault2)"))?;
    create(&record, "NFSHomeDirectory", home)?;
    create(&record, "UserShell", "/bin/zsh")?;
    // No usable password: "*" matches no hash, and no shadow hash is created.
    create(&record, "Password", "*")?;
    create(&record, "IsHidden", "1")
}

/// One attribute of a local account: None when there is no such account,
/// an empty string when it lacks the attribute.
pub fn user_attr(name: &str, attr: &str) -> Res<Option<String>> {
    read_attr(&format!("/Users/{name}"), attr)
}

/// An account that exists only so that its name is taken: hidden, with no
/// usable password, no shell, no home, and no group of its own (`-2` is
/// `nobody`'s). `real_name` is written right after the record, because it
/// is what marks the account as sandvault2's; a killed run leaves a record
/// with that mark, or one with no attributes at all.
pub fn ensure_reserved_user(name: &str, uid: u32, real_name: &str) -> Res<()> {
    let record = format!("/Users/{name}");
    if read_attr(&record, "RecordName")?.is_none() {
        write(DSCL, &[".", "-create", &record])?;
    }
    create(&record, "RealName", real_name)?;
    ensure_record_with_id(&record, "UniqueID", uid)?;
    create(&record, "PrimaryGroupID", "-2")?;
    create(&record, "NFSHomeDirectory", "/var/empty")?;
    create(&record, "UserShell", "/usr/bin/false")?;
    create(&record, "Password", "*")?;
    create(&record, "IsHidden", "1")
}

fn has_token(list: &Option<String>, token: &str) -> bool {
    list.as_deref()
        .is_some_and(|l| l.split_whitespace().any(|t| t == token))
}

/// Remove `name` from `group` in both forms a membership is stored in (the
/// name and the GeneratedUID), then verify. Works when the account or the
/// group no longer exists. Ported from v1.
pub fn purge_member(name: &str, group: &str) -> Res<()> {
    let record = format!("/Groups/{group}");
    let uuid = read_attr(&format!("/Users/{name}"), "GeneratedUID")?.unwrap_or_default();
    let listed_by_name = has_token(&read_attr(&record, "GroupMembership")?, name);
    let listed_by_uuid = !uuid.is_empty() && has_token(&read_attr(&record, "GroupMembers")?, &uuid);
    // Nothing to do, so nothing is written: see the note on writes above.
    if !listed_by_name && !listed_by_uuid {
        return Ok(());
    }
    // Each of these fails when there is nothing to remove; the check below is
    // what decides.
    write_if_there(
        DSEDITGROUP,
        &["-o", "edit", "-d", name, "-t", "user", group],
    )?;
    write_if_there(DSCL, &[".", "-delete", &record, "GroupMembership", name])?;
    if !uuid.is_empty() {
        write_if_there(DSCL, &[".", "-delete", &record, "GroupMembers", &uuid])?;
    }
    if has_token(&read_attr(&record, "GroupMembership")?, name) {
        return Err(format!("could not remove {name} from the {group} group"));
    }
    if !uuid.is_empty() && has_token(&read_attr(&record, "GroupMembers")?, &uuid) {
        return Err(format!(
            "could not remove {name} (by UUID) from the {group} group"
        ));
    }
    Ok(())
}

/// The sandbox account must not be in `staff`, which can read most of the
/// host user's files.
pub fn remove_from_staff(name: &str) -> Res<()> {
    purge_member(name, "staff")
}

fn is_listed_member(user: &str, group: &str) -> Res<bool> {
    Ok(has_token(
        &read_attr(&format!("/Groups/{group}"), "GroupMembership")?,
        user,
    ))
}

/// Checks first, so a rerun does not depend on how dseditgroup treats a user
/// who is already a member.
pub fn add_member(user: &str, group: &str) -> Res<()> {
    if is_listed_member(user, group)? {
        return Ok(());
    }
    write(
        DSEDITGROUP,
        &["-o", "edit", "-a", user, "-t", "user", group],
    )
}

pub fn remove_member(user: &str, group: &str) -> Res<()> {
    if !is_listed_member(user, group)? {
        return Ok(());
    }
    write(
        DSEDITGROUP,
        &["-o", "edit", "-d", user, "-t", "user", group],
    )
}

pub fn create_group_auto_id(name: &str, real_name: &str) -> Res<()> {
    write(DSEDITGROUP, &["-o", "create", "-r", real_name, name])
}

/// One command, so a killed run leaves either no group or a complete one.
pub fn create_group_with_id(name: &str, gid: u32, real_name: &str) -> Res<()> {
    write(
        DSEDITGROUP,
        &[
            "-o",
            "create",
            "-i",
            &gid.to_string(),
            "-r",
            real_name,
            name,
        ],
    )
}

fn delete_record(record: &str) -> Res<()> {
    if read_attr(record, "RecordName")?.is_none() {
        return Ok(());
    }
    write(DSCL, &[".", "-delete", record])
}

pub fn delete_user(name: &str) -> Res<()> {
    delete_record(&format!("/Users/{name}"))
}

pub fn delete_group(name: &str) -> Res<()> {
    delete_record(&format!("/Groups/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dscl_output_shapes() {
        assert_eq!(
            parse_attr("PrimaryGroupID: 600\n", "PrimaryGroupID").as_deref(),
            Some("600")
        );
        // A value with a space is printed on its own line.
        assert_eq!(
            parse_attr("RealName:\n sandvault2 allowed users\n", "RealName").as_deref(),
            Some("sandvault2 allowed users")
        );
        assert_eq!(
            parse_attr(
                "GroupMembership: root pat sv2-pat-default\n",
                "GroupMembership"
            )
            .as_deref(),
            Some("root pat sv2-pat-default")
        );
        assert_eq!(parse_attr("No such key: RealName\n", "RealName"), None);
        // A different attribute with the same prefix is not a match.
        assert_eq!(parse_attr("UniqueIDX: 5\n", "UniqueID"), None);
        // Native attributes carry a prefix.
        assert_eq!(
            parse_attr("dsAttrTypeNative:IsHidden: 1\n", "IsHidden").as_deref(),
            Some("1")
        );
        assert_eq!(
            parse_attr("Password: *\n", "Password").as_deref(),
            Some("*")
        );
    }

    /// A value already in place is not written again: writing is what makes
    /// macOS ask for the privacy permission.
    #[test]
    fn writes_only_what_differs() {
        assert!(!needs_write(
            Some("/Users/sv2-pat-work"),
            "/Users/sv2-pat-work"
        ));
        assert!(!needs_write(Some("1"), "1"));
        assert!(needs_write(Some("/Users/other"), "/Users/sv2-pat-work"));
        // Missing attribute (read as empty) and missing record.
        assert!(needs_write(Some(""), "1"));
        assert!(needs_write(None, "1"));
        // No trimming or prefix matching.
        assert!(needs_write(Some("10"), "1"));
    }

    /// What dscl printed when the permission had been refused, and when the
    /// prompt went unanswered (which is told by the clock, not the text).
    #[test]
    fn knows_the_refusal_from_other_failures() {
        assert!(permission_refused(
            "<dscl_cmd> DS Error: -14120 (eDSPermissionError)\n"
        ));
        assert!(!permission_refused(
            "<dscl_cmd> DS Error: -14987 (eUndefinedError)\n"
        ));
        assert!(!permission_refused(
            "<dscl_cmd> DS Error: -14136 (eDSRecordNotFound)\n"
        ));
        assert!(!permission_refused(""));
    }

    /// The messages say what to do, and carry what a report can be found by.
    #[test]
    fn permission_messages_name_the_permission_and_the_way_out() {
        for m in [refused_message(), unanswered_message()] {
            assert!(m.contains("administer your computer"), "{m}");
            assert!(m.contains("run this command again"), "{m}");
        }
        assert!(refused_message().contains("tccutil reset SystemPolicySysAdminFiles"));
        assert!(unanswered_message().contains("60 seconds"));
    }

    /// A tool's own failure is passed on; only the permission stops a
    /// removal that expects to find nothing.
    #[test]
    fn writes_report_the_tool_and_its_words() {
        let r = try_write("/bin/sh", &["-c", "echo no such thing >&2; exit 3"]);
        let Err(WriteError::Tool(m)) = r else {
            panic!("{r:?}")
        };
        assert!(
            m.contains("no such thing") && m.contains("/bin/sh -c"),
            "{m}"
        );
        assert_eq!(write_if_there("/bin/sh", &["-c", "exit 3"]), Ok(()));
        let refused = "echo '<dscl_cmd> DS Error: -14120 (eDSPermissionError)' >&2; exit 40";
        assert_eq!(
            try_write("/bin/sh", &["-c", refused]),
            Err(WriteError::Permission(refused_message()))
        );
        assert_eq!(
            write_if_there("/bin/sh", &["-c", refused]),
            Err(refused_message())
        );
        assert_eq!(write("/usr/bin/true", &[]), Ok(()));
    }

    #[test]
    fn membership_matches_whole_tokens() {
        let list = Some("root pat sv2-pat-default".to_string());
        assert!(has_token(&list, "pat"));
        assert!(has_token(&list, "sv2-pat-default"));
        assert!(!has_token(&list, "sv2-pat"));
        assert!(!has_token(&list, "pa"));
        assert!(!has_token(&None, "pat"));
    }
}
