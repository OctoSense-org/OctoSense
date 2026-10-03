//! First-use hints for the home page's hidden gestures.
//!
//! First-use instructions explain the pull that opens the App Library,
//! the pulls that open the shade (named for how it opens here:
//! `ShadeReach`), and the swipe-and-hold from the bottom for Recents (from
//! its chevron where the shell draws one; floating navigation has none).
//! Until each has been used once, the home page shows a one-line hint for
//! it (mobile_surface.rs); a gesture with nothing to open gets none.
//! Android keeps what was seen across restarts (the extension's
//! `launcher.hints` snapshot); elsewhere the hints reset with the process.
use crate::mobile_gestures::GestureKind;
use crate::mobile_shade::ShadeReach;

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
    /// `reach`: how notifications and controls open here. The shade hint
    /// names that pull, and is skipped where there is no shade: it could
    /// never be learned, and would hold back the Recents hint for good.
    pub fn pending(&self, reach: ShadeReach) -> Option<(&'static str, &'static str)> {
        if !self.known { return None; }
        if !self.search { return Some(("search", "Pull down for your apps and search")); }
        if !self.shade {
            match reach {
                ShadeReach::SystemPanel => return Some(("shade", "Pull from the very top edge for notifications and controls")),
                ShadeReach::TopCorners => return Some(("shade", "Pull from a top corner for notifications and controls")),
                ShadeReach::Sides => return Some(("shade", "Pull down at either side for notifications and controls")),
                ShadeReach::Unavailable => {}
            }
        }
        if !self.recents { return Some(("recents", Self::recents_hint(!crate::mobile_navigation::ENABLED))); }
        None
    }
    /// Only the shell without floating navigation (not Android or
    /// OpenHarmony) draws the swipe-start chevron the hint can point at.
    fn recents_hint(chevron: bool) -> &'static str {
        if chevron { "Swipe up from the chevron and hold for Recents" } else { "Swipe up from the bottom and hold for Recents" }
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
        assert_eq!(h.pending(ShadeReach::TopCorners), None);
        h.load(std::iter::empty());
        assert_eq!(h.pending(ShadeReach::TopCorners).map(|p| p.0), Some("search"));
        h.saw(GestureKind::HomeSearch);
        assert_eq!(h.just_seen, Some("search"));
        assert_eq!(h.pending(ShadeReach::TopCorners).map(|p| p.0), Some("shade"));
        assert!(h.pending(ShadeReach::SystemPanel).unwrap().1.contains("top edge"));
        h.saw(GestureKind::Page(crate::mobile_gestures::Dir::Left));
        assert_eq!(h.pending(ShadeReach::TopCorners).map(|p| p.0), Some("shade"));
        h.saw(GestureKind::Shade(ShadeSide::Controls));
        h.saw(GestureKind::Switcher);
        assert!(h.all_seen());
        assert_eq!(h.pending(ShadeReach::TopCorners), None);
    }
    #[test]
    fn the_shade_hint_names_how_the_shade_opens_here_or_steps_aside() {
        let mut h = Hints::default();
        h.load(["search".to_string()].into_iter());
        assert!(h.pending(ShadeReach::TopCorners).unwrap().1.contains("top corner"));
        assert!(h.pending(ShadeReach::SystemPanel).unwrap().1.contains("very top edge"));
        // Android without the system-wide panel: the top edge is Android's.
        let sides = h.pending(ShadeReach::Sides).unwrap();
        assert_eq!(sides.0, "shade");
        assert!(sides.1.contains("either side") && !sides.1.contains("top"), "{}", sides.1);
        // Nothing to pull (OpenHarmony): Recents is next, not a hint that never goes.
        assert_eq!(h.pending(ShadeReach::Unavailable).map(|p| p.0), Some("recents"));
    }
    #[test]
    fn recents_hint_names_the_chevron_only_where_it_is_drawn() {
        let mut h = Hints::default();
        h.load(["search".to_string(), "shade".to_string()].into_iter());
        assert_eq!(h.pending(ShadeReach::TopCorners).map(|p| p.1.contains("chevron")), Some(!crate::mobile_navigation::ENABLED));
        assert!(Hints::recents_hint(true).contains("chevron"));
        assert!(!Hints::recents_hint(false).contains("chevron"));
    }
    #[test]
    fn loading_marks_only_known_keys() {
        let mut h = Hints::default();
        h.load(["shade".to_string(), "bogus".to_string()].into_iter());
        assert!(h.shade && !h.search && h.known);
    }
}
