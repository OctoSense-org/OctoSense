//! One-shot routes from a published Glance card to its own foreground app.
//! Routes are data, never filesystem paths or authority to act on an account.
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Routes(HashMap<String, (String, Instant)>);
impl Routes {
    fn insert(&mut self, app: &str, route: &str, now: Instant) {
        self.0.retain(|_, (_, expires)| *expires > now);
        if route.len() <= super::glance::ROUTE_MAX
            && !route.chars().any(char::is_control)
            && (self.0.len() < 64 || self.0.contains_key(app))
        {
            self.0
                .insert(app.into(), (route.into(), now + Duration::from_secs(120)));
        }
    }
    fn take(&mut self, app: &str, now: Instant) -> Option<String> {
        self.0
            .remove(app)
            .filter(|(_, expires)| *expires > now)
            .map(|(route, _)| route)
    }
}
fn routes() -> &'static Mutex<Routes> {
    static ROUTES: OnceLock<Mutex<Routes>> = OnceLock::new();
    ROUTES.get_or_init(|| Mutex::new(Routes::default()))
}
pub fn queue(app: &str, route: &str) {
    routes()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(app, route, Instant::now());
}
pub fn take(app: &str) -> Option<String> {
    routes()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take(app, Instant::now())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routes_are_bound_single_use_expiring_and_latest_wins() {
        let now = Instant::now();
        let mut routes = Routes::default();
        routes.insert("app.a", "event/one", now);
        routes.insert("app.a", "event/two", now);
        assert!(routes.take("app.b", now).is_none());
        assert_eq!(routes.take("app.a", now).as_deref(), Some("event/two"));
        assert!(routes.take("app.a", now).is_none());
        routes.insert("app.a", "event/old", now);
        assert!(routes
            .take("app.a", now + Duration::from_secs(120))
            .is_none());
        routes.insert("app.a", "bad\nroute", now);
        assert!(routes.take("app.a", now).is_none());
    }
}
