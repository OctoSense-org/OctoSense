//! Host verification seam for native activity locators. No kernel or write grants.

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pointer {
    pub version: u8,
    pub app: String,
    pub resource: String,
    pub room: String,
}
impl Pointer {
    fn check(&self) -> Result<(), String> {
        let id = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        };
        if self.version != 1
            || !id(&self.app)
            || !id(&self.resource)
            || self.room.len() > 255
            || !self.room.starts_with('!')
            || !self.room[1..].contains(':')
            || self
                .room
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err("Unsupported activity locator".into());
        }
        Ok(())
    }
}

/// The native host obtains this from the SDK, never card JSON or script arguments.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Session {
    pub account: String,
    pub generation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub room: String,
    pub event: String,
    pub sender: String,
}
#[derive(Clone, Debug)]
pub struct SourceEvent {
    pub source: Source,
    pub pointer: Pointer,
}

/// Implemented by the host's verified SDK adapter and registered native apps.
/// `read_event` fetches the exact server event; `member` and `allows_share`
/// use current verified state. Call this on the host worker, not the UI thread.
pub trait ActivityBackend {
    fn session(&self) -> Option<Session>;
    fn read_event(&self, session: &Session, room: &str, event: &str)
        -> Result<SourceEvent, String>;
    fn member(&self, session: &Session, room: &str) -> bool;
    fn registered(&self, app: &str) -> bool;
    fn allows_share(&self, session: &Session, pointer: &Pointer, sender: &str) -> bool;
    /// Display/focus only. Enrollment, room joins and sends remain separate.
    fn open_or_focus(&mut self, key: &OpenKey) -> Result<Opened, String>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Opened {
    Created,
    Focused,
}

/// The focus key includes the verified account and session generation, so
/// a different account can never focus another account's resource view.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OpenKey {
    session: Session,
    pointer: Pointer,
}
impl OpenKey {
    pub fn session(&self) -> &Session {
        &self.session
    }
    pub fn pointer(&self) -> &Pointer {
        &self.pointer
    }
}

/// Private fields prevent deserializing an untrusted card into a verified open.
#[derive(Debug)]
pub struct VerifiedOpen {
    session: Session,
    pointer: Pointer,
    source: Source,
    until: u64,
}

fn verify(
    b: &impl ActivityBackend,
    session: &Session,
    source: &Source,
    pointer: &Pointer,
) -> Result<(), String> {
    pointer.check()?;
    if b.session().as_ref() != Some(session) {
        return Err("Activity account changed".into());
    }
    if source.event.is_empty()
        || source.event.len() > 255
        || !source.event.starts_with('$')
        || source.sender.is_empty()
        || !source.sender.starts_with('@')
        || source.sender.len() > 255
        || source.room.is_empty()
        || !source.room.starts_with('!')
        || source.room.len() > 255
    {
        return Err("Invalid activity source".into());
    }
    let actual = b.read_event(session, &source.room, &source.event)?;
    if actual.source != *source || actual.pointer != *pointer {
        return Err("Activity source or pointer differs".into());
    }
    if !b.registered(&pointer.app)
        || !b.member(session, &source.room)
        || !b.member(session, &pointer.room)
        || !b.allows_share(session, pointer, &actual.source.sender)
    {
        return Err("Activity source, membership or application unavailable".into());
    }
    if b.session().as_ref() != Some(session) {
        return Err("Activity account changed".into());
    }
    Ok(())
}

pub fn prepare(
    b: &impl ActivityBackend,
    session: Session,
    source: Source,
    pointer: Pointer,
    now: u64,
) -> Result<VerifiedOpen, String> {
    verify(b, &session, &source, &pointer)?;
    Ok(VerifiedOpen {
        session,
        source,
        pointer,
        until: now.checked_add(120).ok_or("Invalid activity clock")?,
    })
}

/// Rechecks queued work against the current account, source and membership.
/// The host adapter queues only display/focus for its native UI after this.
pub fn complete(
    b: &mut impl ActivityBackend,
    open: VerifiedOpen,
    now: u64,
) -> Result<Opened, String> {
    if now >= open.until {
        return Err("Activity open expired".into());
    }
    verify(b, &open.session, &open.source, &open.pointer)?;
    b.open_or_focus(&OpenKey {
        session: open.session,
        pointer: open.pointer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    struct Host {
        session: Session,
        event: SourceEvent,
        joined: bool,
        authorized: bool,
        seen: BTreeSet<OpenKey>,
        opens: usize,
    }
    impl ActivityBackend for Host {
        fn session(&self) -> Option<Session> {
            Some(self.session.clone())
        }
        fn read_event(&self, _: &Session, _: &str, _: &str) -> Result<SourceEvent, String> {
            Ok(self.event.clone())
        }
        fn member(&self, _: &Session, _: &str) -> bool {
            self.joined
        }
        fn registered(&self, app: &str) -> bool {
            app == "org.example.bookclub"
        }
        fn allows_share(&self, _: &Session, _: &Pointer, _: &str) -> bool {
            self.authorized
        }
        fn open_or_focus(&mut self, key: &OpenKey) -> Result<Opened, String> {
            self.opens += 1;
            Ok(if self.seen.insert(key.clone()) {
                Opened::Created
            } else {
                Opened::Focused
            })
        }
    }
    fn host() -> Host {
        Host {
            session: Session {
                account: "@member:example.org".into(),
                generation: 1,
            },
            event: SourceEvent {
                source: Source {
                    room: "!chat:example.org".into(),
                    event: "$server-event".into(),
                    sender: "@owner:example.org".into(),
                },
                pointer: Pointer {
                    version: 1,
                    app: "org.example.bookclub".into(),
                    resource: "reading-42".into(),
                    room: "!activity:example.org".into(),
                },
            },
            joined: true,
            authorized: true,
            seen: BTreeSet::new(),
            opens: 0,
        }
    }
    fn pending(h: &Host) -> Result<VerifiedOpen, String> {
        prepare(
            h,
            h.session.clone(),
            h.event.source.clone(),
            h.event.pointer.clone(),
            100,
        )
    }
    #[test]
    fn repeated_card_open_focuses_same_resource() {
        let mut h = host();
        let o = pending(&h).unwrap();
        assert_eq!(complete(&mut h, o, 101).unwrap(), Opened::Created);
        let o = pending(&h).unwrap();
        assert_eq!(complete(&mut h, o, 102).unwrap(), Opened::Focused);
        assert_eq!(h.seen.len(), 1)
    }
    #[test]
    fn different_accounts_get_separate_views() {
        let mut h = host();
        let o = pending(&h).unwrap();
        assert_eq!(complete(&mut h, o, 101).unwrap(), Opened::Created);
        h.session.account = "@other:example.org".into();
        h.session.generation += 1;
        let o = pending(&h).unwrap();
        assert_eq!(complete(&mut h, o, 102).unwrap(), Opened::Created);
        assert_eq!(h.seen.len(), 2);
    }
    #[test]
    fn forged_source_and_room_cannot_open() {
        let h = host();
        let mut s = h.event.source.clone();
        s.sender = "@forged:example.org".into();
        assert!(prepare(&h, h.session.clone(), s, h.event.pointer.clone(), 100).is_err());
        let mut p = h.event.pointer.clone();
        p.room = "!wrong:example.org".into();
        assert!(prepare(&h, h.session.clone(), h.event.source.clone(), p, 100).is_err());
        assert_eq!(h.opens, 0)
    }
    #[test]
    fn unknown_version_cannot_open() {
        let mut h = host();
        h.event.pointer.version = 2;
        assert!(pending(&h).is_err());
        assert_eq!(h.opens, 0)
    }
    #[test]
    fn account_switch_invalidates_queued_open() {
        let mut h = host();
        let o = pending(&h).unwrap();
        h.session.account = "@other:example.org".into();
        assert!(complete(&mut h, o, 101).is_err());
        assert_eq!(h.opens, 0)
    }
    #[test]
    fn same_account_relogin_invalidates_generation() {
        let mut h = host();
        let o = pending(&h).unwrap();
        h.session.generation += 1;
        assert!(complete(&mut h, o, 101).is_err());
        assert_eq!(h.opens, 0)
    }
    #[test]
    fn membership_and_authority_are_rechecked() {
        let mut h = host();
        let o = pending(&h).unwrap();
        h.joined = false;
        assert!(complete(&mut h, o, 101).is_err());
        h.joined = true;
        let o = pending(&h).unwrap();
        h.authorized = false;
        assert!(complete(&mut h, o, 101).is_err());
        assert_eq!(h.opens, 0)
    }
    #[test]
    fn expired_open_has_no_effect() {
        let mut h = host();
        let o = pending(&h).unwrap();
        assert!(complete(&mut h, o, 220).is_err());
        assert_eq!(h.opens, 0)
    }
}
