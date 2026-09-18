//! First-use hints for the home page's hidden gestures.
//!
//! Three of the shell's gestures have no on-screen affordance: the pull
//! that opens the App Library, the corner pulls that open the shade, and
//! the bottom-band swipe-and-hold for Recents. Until each has been used
//! once, the home page shows a one-line hint for it (mobile_surface.rs).
//! Android keeps what was seen across restarts (the extension's
//! `launcher.hints` snapshot); elsewhere the hints reset with the process.
use crate::mobile_gestures::GestureKind;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hints {
    pub search: bool,
    pub shade: bool,
    pub recents: bool,
    /// Set by the platform's snapshot: until then nothing is shown, so a
    /// returning person never sees a hint flash before it loads.
    pub known: bool,
    /// Which hint was just found this session: `saw` records it here and
    /// the shell forwards it to the platform store.
    pub just_seen: Option<&'static str>,
}

impl Hints {
    pub const KINDS: [&'static str; 3] = ["search", "shade", "recents"];
    pub fn all_seen(&self) -> bool { self.search && self.shade && self.recents }
    /// The hint to show now, in the order a new person meets the shell.
    pub fn pending(&self) -> Option<(&'static str, &'static str)> {
        if !self.known { return None; }
        if !self.search { return Some(("search", "Pull down for your apps and search")); }
        if !self.shade { return Some(("shade", "Pull from a top corner for notifications and controls")); }
        if !self.recents { return Some(("recents", "Swipe up from the bottom and hold for Recents")); }
        None
    }
    /// A gesture committed: the matching hint is done with.
    pub fn saw(&mut self, kind: GestureKind) {
        let key = match kind {
            GestureKind::HomeSearch => "search",
            GestureKind::Shade(_) => "shade",
            GestureKind::Switcher => "recents",
            _ => return,
        };
        if self.mark(key) { self.just_seen = Some(key); }
    }
    /// Marks one hint seen; true when that changed anything.
    pub fn mark(&mut self, key: &str) -> bool {
        let slot = match key { "search" => &mut self.search, "shade" => &mut self.shade, "recents" => &mut self.recents, _ => return false };
        let changed = !*slot;
        *slot = true;
        changed
    }
    /// The platform's stored state: which hints were seen before.
    pub fn load(&mut self, seen: impl Iterator<Item = String>) {
        for key in seen { self.mark(&key); }
        self.known = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobile_gestures::ShadeSide;
    #[test]
    fn hints_show_in_order_and_only_once_known() {
        let mut h = Hints::default();
        assert_eq!(h.pending(), None);
        h.load(std::iter::empty());
        assert_eq!(h.pending().map(|p| p.0), Some("search"));
        h.saw(GestureKind::HomeSearch);
        assert_eq!(h.just_seen, Some("search"));
        assert_eq!(h.pending().map(|p| p.0), Some("shade"));
        h.saw(GestureKind::Page(crate::mobile_gestures::Dir::Left));
        assert_eq!(h.pending().map(|p| p.0), Some("shade"));
        h.saw(GestureKind::Shade(ShadeSide::Controls));
        h.saw(GestureKind::Switcher);
        assert!(h.all_seen());
        assert_eq!(h.pending(), None);
    }
    #[test]
    fn loading_marks_only_known_keys() {
        let mut h = Hints::default();
        h.load(["shade".to_string(), "bogus".to_string()].into_iter());
        assert!(h.shade && !h.search && h.known);
    }
}
