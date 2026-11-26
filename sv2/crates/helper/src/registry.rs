//! The registry: the root-owned record of which host user owns which sandbox
//! account. Ownership is read from here and never derived from a name.
//!
//! Format (one record per line, fields separated by single spaces):
//!
//! ```text
//! sv2-registry 1
//! next-id 602
//! profile 501 pat default 600 active
//! profile 501 pat work 601 adding
//! ```
//!
//! `profile <host uid> <host user> <profile> <id> <state>`; `<id>` is both the
//! sandbox account's UID and its primary group's GID.
//!
//! The state is the intent record that makes `profile-add` and
//! `profile-remove` safe to interrupt and repeat: an entry is written as
//! `adding` (or changed to `removing`) before the first system change, and
//! every step after that is idempotent, so a rerun picks up where a killed
//! run stopped.
//!
//! `next-id` only grows. An ID is never handed out twice, so files a removed
//! sandbox left in `/tmp` or `/var/folders` are never owned by a later one.
//! Reuse would go wrong in two ways:
//!
//! - a later sandbox, perhaps another user's, could read what the earlier
//!   one left behind;
//! - an earlier sandbox could leave files for a later one to trust, such as
//!   a poisoned cache.
//!
//! For the same reason `uninstall` leaves this file in place, with no
//! profiles in it, and `install` carries on from it. If the range (see
//! `MAX_ID`) were ever used up, allocation would fail; it does not start
//! again from the bottom.

use sv2_common::{account_name, valid_host_user, valid_profile, Obelus};

pub const HEADER: &str = "sv2-registry 1";

/// Above macOS service accounts (below 500) and the first local users (501+).
pub const MIN_ID: u32 = 600;
/// The largest ID that is positive as a signed 32-bit number. `initgroups`
/// takes the group ID as an `int` and some tools print IDs signed, and the
/// two IDs above this range that mean something (-1, "no change", and -2,
/// `nobody`) stay out of reach. With about two thousand million IDs, running
/// out is not a practical concern; allocation still fails rather than wraps.
pub const MAX_ID: u32 = i32::MAX as u32;

/// Each profile is a real account, so cap how many one user can create.
pub const MAX_PROFILES_PER_USER: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Adding,
    Active,
    Removing,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Adding => "adding",
            State::Active => "active",
            State::Removing => "removing",
        }
    }

    fn parse(s: &str) -> Option<State> {
        match s {
            "adding" => Some(State::Adding),
            "active" => Some(State::Active),
            "removing" => Some(State::Removing),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub host_uid: u32,
    pub host_user: String,
    pub profile: String,
    pub id: u32,
    pub state: State,
}

impl Entry {
    pub fn account(&self) -> String {
        account_name(&self.host_user, &self.profile)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registry {
    pub next_id: u32,
    pub entries: Vec<Entry>,
}

impl Default for Registry {
    fn default() -> Self {
        Registry {
            next_id: MIN_ID,
            entries: Vec::new(),
        }
    }
}

/// Decimal digits only: `u32::from_str` also accepts a leading `+`.
fn parse_u32(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 10 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

impl Registry {
    /// Strict: anything unexpected is an error, never skipped. A registry
    /// that does not parse stops the helper rather than being guessed at.
    pub fn parse(text: &str) -> Result<Registry, String> {
        let mut lines = text.lines();
        if lines.next() != Some(HEADER) {
            return Err(format!("registry: missing header '{HEADER}'"));
        }
        let mut next_id: Option<u32> = None;
        let mut reg = Registry::default();
        for (n, line) in lines.enumerate() {
            let lineno = n + 2;
            let bad = |what: &str| format!("registry line {lineno}: {what}");
            let fields: Vec<&str> = line.split(' ').collect();
            match fields.as_slice() {
                ["next-id", v] => {
                    if next_id.is_some() {
                        return Err(bad("duplicate next-id"));
                    }
                    let v = parse_u32(v).ok_or_else(|| bad("bad next-id"))?;
                    if !(MIN_ID..=MAX_ID + 1).contains(&v) {
                        return Err(bad("next-id out of range"));
                    }
                    next_id = Some(v);
                }
                ["profile", host_uid, host_user, profile, id, state] => {
                    let host_uid = parse_u32(host_uid).ok_or_else(|| bad("bad host uid"))?;
                    let id = parse_u32(id).ok_or_else(|| bad("bad id"))?;
                    let state = State::parse(state).ok_or_else(|| bad("bad state"))?;
                    if host_uid == 0 {
                        return Err(bad("host uid 0"));
                    }
                    if !valid_host_user(host_user) {
                        return Err(bad("bad host user"));
                    }
                    if !valid_profile(profile) {
                        return Err(bad("bad profile"));
                    }
                    // The helper drops to this ID and signals every process
                    // that has it. It must never be a system or root ID.
                    if !(MIN_ID..=MAX_ID).contains(&id) {
                        return Err(bad("id out of range"));
                    }
                    if host_uid == id {
                        return Err(bad("host uid equals sandbox id"));
                    }
                    for e in &reg.entries {
                        if e.id == id {
                            return Err(bad("duplicate id"));
                        }
                        if e.host_user == *host_user && e.profile == *profile {
                            return Err(bad("duplicate account"));
                        }
                        if e.host_uid == host_uid && e.profile == *profile {
                            return Err(bad("duplicate profile"));
                        }
                    }
                    reg.entries.push(Entry {
                        host_uid,
                        host_user: (*host_user).to_string(),
                        profile: (*profile).to_string(),
                        id,
                        state,
                    });
                }
                _ => return Err(bad("unrecognized record")),
            }
        }
        reg.next_id = next_id.ok_or("registry: missing next-id")?;
        if let Some(e) = reg.entries.iter().find(|e| e.id >= reg.next_id) {
            return Err(format!(
                "registry: id {} is not below next-id {}",
                e.id, reg.next_id
            ));
        }
        Ok(reg)
    }

    pub fn serialize(&self) -> String {
        let mut out = format!("{HEADER}\nnext-id {}\n", self.next_id);
        for e in &self.entries {
            out.push_str(&format!(
                "profile {} {} {} {} {}\n",
                e.host_uid,
                e.host_user,
                e.profile,
                e.id,
                e.state.as_str()
            ));
        }
        out
    }

    pub fn find(&self, host_uid: u32, profile: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.host_uid == host_uid && e.profile == profile)
    }

    pub fn set_state(&mut self, host_uid: u32, profile: &str, state: State) {
        for e in &mut self.entries {
            if e.host_uid == host_uid && e.profile == profile {
                e.state = state;
            }
        }
    }

    pub fn remove(&mut self, host_uid: u32, profile: &str) {
        self.entries
            .retain(|e| !(e.host_uid == host_uid && e.profile == profile));
    }

    pub fn for_user(&self, host_uid: u32) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.host_uid == host_uid)
    }

    /// True when `uid` is a sandbox account. Such a caller is refused.
    pub fn is_sandbox_id(&self, uid: u32) -> bool {
        self.entries.iter().any(|e| e.id == uid)
    }

    /// The account name is `sv2-<host user>-<profile>`, so a second host UID
    /// with the same username (an account deleted and recreated) must not get
    /// an entry that maps to an account another UID's entry already owns.
    pub fn account_taken_by_other(&self, host_uid: u32, host_user: &str, profile: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.host_user == host_user && e.profile == profile && e.host_uid != host_uid)
    }

    /// Pick the next ID that is free as both a UID and a GID, and advance
    /// `next_id` past it. `in_use` asks the system; the caller holds the
    /// registry lock.
    pub fn allocate_id(&mut self, in_use: impl Fn(u32) -> bool) -> Result<u32, String> {
        let mut id = self.next_id.max(MIN_ID);
        while id <= MAX_ID {
            if !in_use(id) && !self.is_sandbox_id(id) {
                self.next_id = id + 1;
                return Ok(id);
            }
            id += 1;
        }
        Err(format!(
            "No previously unused user ID is available. Reusing an ID could grant \
             the sandbox access to orphaned files. Search the Internet for \
             \"macOS UID reuse and orphaned files\" for solutions. {}",
            Obelus("dapper mackerel")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(host_uid: u32, user: &str, profile: &str, id: u32, state: State) -> Entry {
        Entry {
            host_uid,
            host_user: user.into(),
            profile: profile.into(),
            id,
            state,
        }
    }

    #[test]
    fn round_trip() {
        let reg = Registry {
            next_id: 603,
            entries: vec![
                entry(501, "pat", "default", 600, State::Active),
                entry(501, "pat", "work", 601, State::Adding),
                entry(502, "first.last", "default", 602, State::Removing),
            ],
        };
        let text = reg.serialize();
        assert_eq!(
            text,
            "sv2-registry 1\nnext-id 603\n\
             profile 501 pat default 600 active\n\
             profile 501 pat work 601 adding\n\
             profile 502 first.last default 602 removing\n"
        );
        assert_eq!(Registry::parse(&text).unwrap(), reg);
    }

    #[test]
    fn empty_registry_round_trips() {
        let reg = Registry::default();
        assert_eq!(Registry::parse(&reg.serialize()).unwrap(), reg);
    }

    #[test]
    fn rejects_malformed() {
        let ok = "sv2-registry 1\nnext-id 601\nprofile 501 pat default 600 active\n";
        assert!(Registry::parse(ok).is_ok());
        let cases: &[(&str, &str)] = &[
            ("", "empty"),
            ("sv2-registry 2\nnext-id 600\n", "other schema"),
            ("sv2-registry 1\n", "no next-id"),
            ("sv2-registry 1\nnext-id 600\nnext-id 600\n", "two next-id"),
            ("sv2-registry 1\nnext-id 599\n", "next-id too low"),
            ("sv2-registry 1\nnext-id +600\n", "signed number"),
            ("sv2-registry 1\nnext-id 600\n\n", "blank line"),
            ("sv2-registry 1\nnext-id 600\nbogus\n", "unknown record"),
            (
                "sv2-registry 1\nnext-id 601\nprofile 501 pat default 600 active extra\n",
                "extra field",
            ),
            (
                "sv2-registry 1\nnext-id 601\nprofile 501 pat  default 600 active\n",
                "double space",
            ),
            (
                "sv2-registry 1\nnext-id 601\nprofile 0 root default 600 active\n",
                "host uid 0",
            ),
            (
                "sv2-registry 1\nnext-id 601\nprofile 501 pat default 0 active\n",
                "sandbox id 0",
            ),
            (
                "sv2-registry 1\nnext-id 601\nprofile 501 pat default 501 active\n",
                "sandbox id below minimum",
            ),
            (
                "sv2-registry 1\nnext-id 601\nprofile 600 pat default 600 active\n",
                "host uid is the sandbox id",
            ),
            (
                "sv2-registry 1\nnext-id 600\nprofile 501 pat default 600 active\n",
                "id not below next-id",
            ),
            (
                "sv2-registry 1\nnext-id 601\nprofile 501 pat a-b 600 active\n",
                "bad profile",
            ),
            (
                "sv2-registry 1\nnext-id 601\nprofile 501 ../x default 600 active\n",
                "bad user",
            ),
            (
                "sv2-registry 1\nnext-id 601\nprofile 501 pat default 600 done\n",
                "bad state",
            ),
            (
                "sv2-registry 1\nnext-id 602\nprofile 501 pat default 600 active\nprofile 502 sam default 600 active\n",
                "duplicate id",
            ),
            (
                "sv2-registry 1\nnext-id 602\nprofile 501 pat default 600 active\nprofile 501 pat default 601 active\n",
                "duplicate profile",
            ),
            (
                "sv2-registry 1\nnext-id 602\nprofile 501 pat default 600 active\nprofile 502 pat default 601 active\n",
                "duplicate account",
            ),
        ];
        for (text, why) in cases {
            assert!(Registry::parse(text).is_err(), "accepted: {why}");
        }
    }

    #[test]
    fn ownership_is_by_host_uid() {
        let reg = Registry {
            next_id: 602,
            entries: vec![
                entry(501, "pat", "default", 600, State::Active),
                entry(502, "sam", "default", 601, State::Active),
            ],
        };
        assert_eq!(reg.find(501, "default").unwrap().id, 600);
        assert_eq!(reg.find(502, "default").unwrap().id, 601);
        assert!(reg.find(503, "default").is_none());
        assert!(reg.find(501, "work").is_none());
        assert_eq!(reg.for_user(501).count(), 1);
        assert!(reg.is_sandbox_id(600));
        assert!(!reg.is_sandbox_id(501));
        assert!(reg.account_taken_by_other(503, "pat", "default"));
        assert!(!reg.account_taken_by_other(501, "pat", "default"));
        assert!(!reg.account_taken_by_other(503, "pat", "work"));
    }

    #[test]
    fn allocation_skips_ids_in_use_and_never_reuses() {
        let mut reg = Registry::default();
        // 600 and 601 are taken on the system (for example by v1 accounts).
        let in_use = |id: u32| id == 600 || id == 601;
        assert_eq!(reg.allocate_id(in_use).unwrap(), 602);
        assert_eq!(reg.next_id, 603);
        assert_eq!(reg.allocate_id(in_use).unwrap(), 603);
        // Nothing was recorded for 602 or 603 (as after a removal), and the
        // system no longer lists them. They are still not handed out again.
        assert_eq!(reg.allocate_id(|_| false).unwrap(), 604);
    }

    #[test]
    fn allocation_fails_when_exhausted() {
        let mut reg = Registry {
            next_id: MAX_ID,
            entries: vec![],
        };
        assert_eq!(reg.allocate_id(|_| false).unwrap(), MAX_ID);
        let err = reg.allocate_id(|_| false).unwrap_err();
        // The obelus is not repeated here: it names one place in the code.
        // It ends with its obelus, shown in brackets.
        assert!(
            err.contains(" for solutions. (") && err.ends_with(')'),
            "{err}"
        );
        for part in ["UID reuse", "orphaned files"] {
            assert!(err.contains(part), "{part}: {err}");
        }
        // The exhausted state still serializes and parses.
        assert_eq!(Registry::parse(&reg.serialize()).unwrap(), reg);
    }
}
