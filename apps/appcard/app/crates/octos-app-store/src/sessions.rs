//! `SessionMap`, hydrate semantics, and sidebar selectors.
//!
//! The sidebar list is the canonical projection: `ordered` is the visible
//! order (most recently active first), `by_id` carries the metadata. On every
//! `touch()` the key moves to index 0 — this is W04's "move-to-front" rule
//! mirrored from `octos-web/src/components/session-list.tsx`.

use crate::auth::ProfileId;
use chrono::{DateTime, Utc};
// see octos-core ui_protocol.rs:8 (re-export from types.rs:160)
use octos_core::SessionKey;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: SessionKey,
    pub profile_id: ProfileId,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_message_preview: Option<String>,
    pub is_streaming: bool,
    pub has_active_task: bool,
}

impl Session {
    pub fn new(
        id: SessionKey, profile_id: ProfileId,
        title: impl Into<String>, created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id, profile_id, title: title.into(), created_at, updated_at: created_at,
            last_message_preview: None, is_streaming: false, has_active_task: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SessionMap {
    pub by_id: HashMap<SessionKey, Session>,
    pub ordered: Vec<SessionKey>,
}

impl SessionMap {
    pub fn new() -> Self { Self::default() }

    /// Insert or replace. New ids land at the front; existing ids keep their
    /// slot (use [`Self::touch`] to move to front).
    pub fn insert(&mut self, session: Session) {
        let id = session.id.clone();
        if !self.by_id.contains_key(&id) { self.ordered.insert(0, id.clone()); }
        self.by_id.insert(id, session);
    }

    /// Remove a session entirely.
    pub fn remove(&mut self, id: &SessionKey) -> Option<Session> {
        let removed = self.by_id.remove(id);
        if removed.is_some() { self.ordered.retain(|k| k != id); }
        removed
    }

    /// Move-to-front with `updated_at` bump. No-op on missing id.
    pub fn touch(&mut self, id: &SessionKey, updated_at: DateTime<Utc>) {
        if let Some(s) = self.by_id.get_mut(id) {
            s.updated_at = updated_at;
            self.ordered.retain(|k| k != id);
            self.ordered.insert(0, id.clone());
        }
    }

    pub fn get(&self, id: &SessionKey) -> Option<&Session> { self.by_id.get(id) }
    pub fn get_mut(&mut self, id: &SessionKey) -> Option<&mut Session> { self.by_id.get_mut(id) }
    pub fn len(&self) -> usize { self.ordered.len() }
    pub fn is_empty(&self) -> bool { self.ordered.is_empty() }

    /// Sessions in sidebar render order (most recent first).
    pub fn sessions_for_sidebar(&self) -> Vec<&Session> {
        self.ordered.iter().filter_map(|k| self.by_id.get(k)).collect()
    }

    /// Lit if streaming tokens or a running tool/task. W04 § 4 dot states.
    pub fn is_session_active(&self, id: &SessionKey) -> bool {
        self.by_id.get(id).map(|s| s.is_streaming || s.has_active_task).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> SessionKey { SessionKey(s.to_owned()) }
    fn pid() -> ProfileId { ProfileId::from("acme".to_owned()) }
    fn ts(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(secs, 0).unwrap()
    }

    fn sess(id: &str, t: i64) -> Session {
        Session::new(key(id), pid(), format!("Session {id}"), ts(t))
    }

    #[test]
    fn session_map_move_to_front_on_touch() {
        let mut m = SessionMap::new();
        m.insert(sess("a", 100));
        m.insert(sess("b", 200));
        m.insert(sess("c", 300));
        // Insertion order: c, b, a (newest at front).
        assert_eq!(m.ordered, vec![key("c"), key("b"), key("a")]);
        // Touching `a` jumps to front and bumps updated_at.
        m.touch(&key("a"), ts(999));
        assert_eq!(m.ordered, vec![key("a"), key("c"), key("b")]);
        assert_eq!(m.get(&key("a")).unwrap().updated_at, ts(999));
    }

    #[test]
    fn remove_clears_both_index_and_order() {
        let mut m = SessionMap::new();
        m.insert(sess("a", 1));
        m.insert(sess("b", 2));
        assert!(m.remove(&key("a")).is_some());
        assert!(m.get(&key("a")).is_none());
        assert_eq!(m.ordered, vec![key("b")]);
        assert!(m.remove(&key("missing")).is_none());
    }

    #[test]
    fn is_session_active_reflects_flags() {
        let mut m = SessionMap::new();
        m.insert(sess("a", 1));
        assert!(!m.is_session_active(&key("a")));
        m.get_mut(&key("a")).unwrap().is_streaming = true;
        assert!(m.is_session_active(&key("a")));
        m.get_mut(&key("a")).unwrap().is_streaming = false;
        m.get_mut(&key("a")).unwrap().has_active_task = true;
        assert!(m.is_session_active(&key("a")));
        assert!(!m.is_session_active(&key("missing")));
    }

    #[test]
    fn sidebar_order_matches_ordered_vec() {
        let mut m = SessionMap::new();
        m.insert(sess("a", 1));
        m.insert(sess("b", 2));
        let titles: Vec<_> = m.sessions_for_sidebar().iter().map(|s| s.title.clone()).collect();
        assert_eq!(titles, vec!["Session b".to_string(), "Session a".to_string()]);
    }
}
