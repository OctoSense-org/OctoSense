//! Without the `web-card` feature there is no native web-card overlay (the fork makepad's
//! system browser view floating over a card): the reader and web apps keep their code paths
//! and do nothing. The names match the fork's so the call sites read the same.

use makepad_widgets::*;

/// The id every web card shared one native overlay under.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WebCardBrowserId(pub LiveId);

pub fn web_card_browser_id() -> WebCardBrowserId {
    WebCardBrowserId(live_id!(web_card))
}

/// A system browser that is not there.
pub struct NoBrowser;

impl NoBrowser {
    pub fn detach(&mut self) {}
    pub fn emit(&mut self, _event: &str, _payload: &str) {}
}

pub trait SystemBrowserShim {
    fn system_browser(&mut self, id: WebCardBrowserId) -> NoBrowser;
}

impl SystemBrowserShim for Cx {
    fn system_browser(&mut self, _id: WebCardBrowserId) -> NoBrowser {
        NoBrowser
    }
}

/// A web card widget that is not there.
pub struct NoWebCard;

impl NoWebCard {
    pub fn detach(&self, _cx: &mut Cx) {}
    pub fn navigate(&self, _cx: &mut Cx, _url: &str) {}
}

pub trait WebCardShim {
    fn web_card(&self, cx: &mut Cx, path: &[LiveId]) -> NoWebCard;
}

impl WebCardShim for WidgetRef {
    fn web_card(&self, _cx: &mut Cx, _path: &[LiveId]) -> NoWebCard {
        NoWebCard
    }
}
