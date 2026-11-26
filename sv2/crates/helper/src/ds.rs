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

use crate::sys::{run, run_ok, Res};

const DSCL: &str = "/usr/bin/dscl";
const DSEDITGROUP: &str = "/usr/sbin/dseditgroup";

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
    run_ok(DSCL, [".", "-create", record, attr, value])
}

/// Create `record` with `attr` = `id`, or accept an existing one.
///
/// An existing record must already carry `id`: the helper never adopts or
/// renumbers an account it did not create with that ID. A record with no ID
/// at all is one a killed run created just before setting it (a new name is
/// refused earlier if any record for it exists), so the ID is filled in.
fn ensure_record_with_id(record: &str, attr: &str, id: u32) -> Res<()> {
    match read_attr(record, attr)? {
        None => run_ok(DSCL, [".", "-create", record])?,
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
    let _ = run(DSEDITGROUP, ["-o", "edit", "-d", name, "-t", "user", group]);
    let _ = run(DSCL, [".", "-delete", &record, "GroupMembership", name]);
    if !uuid.is_empty() {
        let _ = run(DSCL, [".", "-delete", &record, "GroupMembers", &uuid]);
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
    run_ok(DSEDITGROUP, ["-o", "edit", "-a", user, "-t", "user", group])
}

pub fn remove_member(user: &str, group: &str) -> Res<()> {
    if !is_listed_member(user, group)? {
        return Ok(());
    }
    run_ok(DSEDITGROUP, ["-o", "edit", "-d", user, "-t", "user", group])
}

pub fn create_group_auto_id(name: &str, real_name: &str) -> Res<()> {
    run_ok(DSEDITGROUP, ["-o", "create", "-r", real_name, name])
}

/// One command, so a killed run leaves either no group or a complete one.
pub fn create_group_with_id(name: &str, gid: u32, real_name: &str) -> Res<()> {
    run_ok(
        DSEDITGROUP,
        [
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
    run_ok(DSCL, [".", "-delete", record])
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
