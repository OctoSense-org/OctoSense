//! `CurrentScreen` enum + transitions. Owned by W02; the reducer here only
//! handles the routing decisions (`OpenSession`, `OpenProject`, `Logout`).
//! See `04-IA-AND-NAVIGATION.md` for the full IA tree.

use octos_core::SessionKey;
use std::fmt;

/// Producer surface for the M3 project screens (Studio / Slides / Sites).
/// Held as a separate axis from `CurrentScreen` so a single `OpenProject`
/// event can route to any of the three.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Producer {
    Studio,
    Slides,
    Sites,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProjectId(pub String);

impl ProjectId { pub fn as_str(&self) -> &str { &self.0 } }
impl From<String> for ProjectId { fn from(s: String) -> Self { Self(s) } }
impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

/// Top-level destination. Studio/Slides/Sites carry an optional project id
/// (None = empty index, Some = a specific deck/site/notebook).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CurrentScreen {
    Login,
    Home,
    Chat { session: Option<SessionKey> },
    Coding,
    Studio { project: Option<ProjectId> },
    Slides { project: Option<ProjectId> },
    Sites { project: Option<ProjectId> },
    /// Content browser — gallery over `/api/my/content` rows.
    /// W04 § "Content browser"; matches the sidebar nav slot in
    /// `04-IA-AND-NAVIGATION.md` § Top-level shell.
    Content,
}

impl Default for CurrentScreen {
    fn default() -> Self { Self::Home }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavigationEvent {
    NavigateTo(CurrentScreen),
    OpenSession(SessionKey),
    OpenProject(Producer, ProjectId),
    /// Local logout. Drops session pointer + routes to Login. The auth slice
    /// is cleared via `AuthEvent::Logout` in the parent reducer; this just
    /// owns the navigation half.
    Logout,
}

/// Apply a navigation event. Mutates `screen` and (where relevant)
/// `current_session` in lockstep so the parent reducer doesn't drift.
pub fn reduce(
    screen: &mut CurrentScreen,
    current_session: &mut Option<SessionKey>,
    event: NavigationEvent,
) {
    match event {
        NavigationEvent::NavigateTo(target) => {
            if let CurrentScreen::Chat { ref session } = target {
                *current_session = session.clone();
            }
            *screen = target;
        }
        NavigationEvent::OpenSession(id) => {
            *current_session = Some(id.clone());
            *screen = CurrentScreen::Chat { session: Some(id) };
        }
        NavigationEvent::OpenProject(producer, project) => {
            let project = Some(project);
            *screen = match producer {
                Producer::Studio => CurrentScreen::Studio { project },
                Producer::Slides => CurrentScreen::Slides { project },
                Producer::Sites => CurrentScreen::Sites { project },
            };
        }
        NavigationEvent::Logout => {
            *current_session = None;
            *screen = CurrentScreen::Login;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> SessionKey { SessionKey(s.to_owned()) }

    #[test]
    fn navigation_logout_clears_session_pointer() {
        let mut screen = CurrentScreen::Chat { session: Some(key("a")) };
        let mut cur = Some(key("a"));
        reduce(&mut screen, &mut cur, NavigationEvent::Logout);
        assert_eq!(screen, CurrentScreen::Login);
        assert_eq!(cur, None);
    }

    #[test]
    fn open_session_routes_to_chat_and_sets_pointer() {
        let mut screen = CurrentScreen::Home;
        let mut cur: Option<SessionKey> = None;
        reduce(&mut screen, &mut cur, NavigationEvent::OpenSession(key("s1")));
        assert_eq!(screen, CurrentScreen::Chat { session: Some(key("s1")) });
        assert_eq!(cur, Some(key("s1")));
    }

    #[test]
    fn open_project_dispatches_per_producer() {
        let mut s = CurrentScreen::Home;
        let mut cur = None;
        reduce(
            &mut s,
            &mut cur,
            NavigationEvent::OpenProject(Producer::Studio, ProjectId("p".into())),
        );
        assert_eq!(s, CurrentScreen::Studio { project: Some(ProjectId("p".into())) });
        reduce(
            &mut s,
            &mut cur,
            NavigationEvent::OpenProject(Producer::Sites, ProjectId("q".into())),
        );
        assert_eq!(s, CurrentScreen::Sites { project: Some(ProjectId("q".into())) });
    }

    #[test]
    fn navigate_to_chat_syncs_session_pointer() {
        let mut s = CurrentScreen::Home;
        let mut cur: Option<SessionKey> = None;
        reduce(
            &mut s,
            &mut cur,
            NavigationEvent::NavigateTo(CurrentScreen::Chat { session: Some(key("z")) }),
        );
        assert_eq!(cur, Some(key("z")));
    }
}
