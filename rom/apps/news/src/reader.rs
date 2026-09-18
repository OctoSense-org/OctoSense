//! The in-app article reader: the platform's native web view (WKWebView on
//! macOS and iOS, Android's WebView) glued to this widget's page rect while
//! the pane is open, under a bar in the Apple News manner: a round back
//! button, the link's host centred (no platform reports navigation at this
//! revision, so there is no page title), and a `•••` button for the story's
//! actions. The overlay belongs to the window that owns the widget's draw
//! pass: the standalone window, or the host's window for an in-process
//! module, and it only moves or hides when this widget draws or is told
//! to. So while the pane is open a watchdog runs on a slow timer: every
//! half second it asks for a redraw and checks that the redraw it asked for
//! last time drew this widget; a tick that finds no draw (the tile is on a
//! hidden workspace, another tile went fullscreen) takes the overlay off
//! the window, and the next draw puts it back. Two redraws of the tile a
//! second while reading, none when the pane is closed: a closed pane runs
//! no timer.

use crate::model::{host_of, Skin};
use makepad_widgets::*;

/// The watchdog's period: how long the overlay may outlive its tile.
const WATCHDOG_SECONDS: f64 = 0.5;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    let c_ground = #(Skin::for_vm(vm).ground)
    let c_card = #(Skin::for_vm(vm).card)
    let c_ink = #(Skin::for_vm(vm).ink)
    let c_secondary = #(Skin::for_vm(vm).secondary)
    let c_accent = #(Skin::for_vm(vm).accent)
    let c_clear = #(Vec4f::from_u32(0x00000000))

    // The bar's buttons: round lenses carrying the host's line icons in the
    // skin's ink, centred.
    let ReaderButton = glass.IconButton{width: 40 height: 40 text: "" spacing: 0 padding: 0 align: Center
        icon_walk: Walk{width: 16 height: 16}
        draw_bg +: {border_radius: uniform(20.0)}
        draw_icon +: {color: c_ink}
    }

    mod.widgets.ArticleReaderBase = #(ArticleReader::register_widget(vm))
    mod.widgets.ArticleReader = set_type_default() do mod.widgets.ArticleReaderBase{
        width: Fill height: Fill flow: Overlay
        SolidView{width: Fill height: Fill draw_bg.color: c_ground}
        View{width: Fill height: Fill flow: Down
            bar := View{width: Fill height: 56 flow: Right spacing: 8 align: Align{y: 0.5} padding: Inset{left: 10 right: 10}
                show_bg: true draw_bg.color: c_ground
                back := ReaderButton{draw_icon.svg: crate_resource("self:resources/icons/chevron-left.svg")}
                View{width: Fill height: Fit flow: Down align: Align{x: 0.5}
                    host := Label{width: Fill padding: 0 max_lines: 1 text_overflow: Ellipsis align: Align{x: 0.5}
                        draw_text +: {color: c_ink text_style: theme.font_bold{font_size: 13}}}
                    hint := Label{width: Fill padding: 0 max_lines: 1 text: "Article" align: Align{x: 0.5}
                        draw_text +: {color: c_secondary text_style: theme.font_regular{font_size: 10}}}
                }
                more := ReaderButton{draw_icon.svg: crate_resource("self:resources/icons/more.svg")}
            }
            // The page's ground until the web view covers it; the overlay is
            // glued to this view's rect, not the whole widget's, so the bar
            // stays above it.
            // The web view covers this while a page is up. When a load
            // fails the overlay is taken off and this is what is left, so
            // it says so instead of showing a blank pane.
            page := SolidView{width: Fill height: Fill draw_bg.color: c_card
                failure := View{visible: false width: Fill height: Fill flow: Down spacing: 6
                    align: Align{x: 0.5 y: 0.5} padding: Inset{left: 32 right: 32}
                    Label{width: Fill align: Align{x: 0.5} text: "Couldn't load this page"
                        draw_text +: {color: c_ink text_style: theme.font_bold{font_size: 16}}}
                    reason := Label{width: Fill align: Align{x: 0.5} max_lines: 3
                        draw_text +: {color: c_secondary text_style: theme.font_regular{font_size: 13}}}
                    retry := ButtonFlat{width: Fit height: 40 margin: Inset{top: 6} padding: Inset{left: 14 right: 14}
                        text: "Try again" align: Center
                        draw_bg +: {
                            border_size: uniform(0.0)
                            color: uniform(c_clear) color_hover: uniform(c_clear)
                            color_down: uniform(#00000018) color_focus: uniform(c_clear)
                            border_color: uniform(c_clear) border_color_hover: uniform(c_clear)
                            border_color_down: uniform(c_clear) border_color_focus: uniform(c_clear)
                        }
                        draw_text +: {color: c_accent color_hover: c_accent color_down: c_accent
                            color_focus: c_accent text_style: theme.font_regular{font_size: 15}}}
                }
            }
        }
    }
}

/// What the reader tells its parent: the person closed it, so the page
/// comes back; or asked for the story's actions.
#[derive(Clone, Debug, Default)]
pub enum ReaderAction {
    #[default]
    None,
    Closed,
    More,
}

#[derive(Script, ScriptHook, Widget)]
pub struct ArticleReader {
    #[deref]
    view: View,
    /// The link on show, once one was opened.
    #[rust]
    url: Option<String>,
    /// Whether the platform was asked for the web view: once, on the first
    /// open; later links navigate it.
    #[rust]
    spawned: bool,
    /// Whether the pane is showing: the overlay follows this on every draw.
    #[rust]
    open: bool,
    /// Whether the overlay is on the window: put there by a draw of an
    /// open pane, taken off by a close or by the watchdog.
    #[rust]
    overlay_visible: bool,
    /// The watchdog's timer while the pane is open; `None` when closed, so
    /// nothing ticks then.
    #[rust]
    timer: Option<Timer>,
    /// Whether this widget drew since the watchdog last looked.
    #[rust]
    drawn: bool,
    /// Set when the platform reported that the page did not load. The web
    /// view is taken off the window while this is set, so the pane shows
    /// the failure instead of the web view's empty background.
    #[rust]
    failed: bool,
}

impl ArticleReader {
    /// The native web view is this widget's own: its uid is unique in the
    /// process, so two readers (two module instances) never share one.
    fn browser_id(&self) -> SystemBrowserId {
        SystemBrowserId(LiveId(self.widget_uid().0))
    }

    /// Show `url` in the pane.
    pub fn open(&mut self, cx: &mut Cx, url: &str) {
        // Navigable: a story link is often a redirector (Google News RSS
        // links are), and a page's own links are part of reading it. Spawn
        // carries that policy and loads the url, and is idempotent for a
        // browser that already exists — so every open states the policy
        // rather than trusting a view created earlier to still carry it.
        cx.system_browser(self.browser_id()).spawn_navigable(url);
        self.spawned = true;
        self.clear_failure(cx);
        self.url = Some(url.to_string());
        self.open = true;
        self.view.label(cx, ids!(host)).set_text(cx, &host_of(url));
        self.view.set_visible(cx, true);
        self.view.redraw(cx);
        // The redraw just asked for is the first draw the watchdog waits
        // on; the open counts as one so the first tick does not hide.
        self.overlay_visible = true;
        self.drawn = true;
        if self.timer.is_none() {
            self.timer = Some(cx.start_interval(WATCHDOG_SECONDS));
        }
    }

    /// Put the pane back in its loading state: the failure is off and the
    /// web view may cover the page again.
    fn clear_failure(&mut self, cx: &mut Cx) {
        if self.failed {
            self.failed = false;
            self.view.widget(cx, ids!(failure)).set_visible(cx, false);
        }
    }

    /// The platform could not load `url`. Take the web view off the window
    /// — it has nothing to show — and say so in its place.
    fn show_failure(&mut self, cx: &mut Cx, reason: &str) {
        self.failed = true;
        self.hide_overlay(cx);
        let host = self.url.as_deref().map(host_of).unwrap_or_default();
        let reason = if reason.is_empty() {
            host.clone()
        } else {
            format!("{host} — {reason}")
        };
        self.view.label(cx, ids!(reason)).set_text(cx, &reason);
        self.view.widget(cx, ids!(failure)).set_visible(cx, true);
        self.view.redraw(cx);
    }

    /// Hide the pane and its overlay; the parent hears `ReaderAction::Closed`.
    pub fn close(&mut self, cx: &mut Cx) {
        if !self.open {
            return;
        }
        self.open = false;
        // The pane is not drawn again while hidden, so this is the update
        // that takes the overlay off the window.
        self.hide_overlay(cx);
        self.stop_watchdog(cx);
        self.view.set_visible(cx, false);
        self.view.redraw(cx);
        cx.widget_action(self.widget_uid(), ReaderAction::Closed);
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether the overlay is on the window: false while the pane is
    /// closed, and while an open pane is not being drawn.
    pub fn overlay_visible(&self) -> bool {
        self.overlay_visible
    }

    /// The watchdog's timer, while the pane is open.
    #[cfg(test)]
    pub(crate) fn watchdog_timer(&self) -> Option<Timer> {
        self.timer
    }

    /// The link on show, if the pane was ever opened.
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// The web view goes with the widget: off the window and released.
    pub fn shutdown(&mut self, cx: &mut Cx) {
        if self.spawned {
            let mut browser = cx.system_browser(self.browser_id());
            browser.detach();
            browser.close();
            self.spawned = false;
        }
        self.open = false;
        self.overlay_visible = false;
        self.stop_watchdog(cx);
        self.drawn = false;
    }

    fn stop_watchdog(&mut self, cx: &mut Cx) {
        if let Some(timer) = self.timer.take() {
            cx.stop_timer(timer);
        }
    }

    /// Take the overlay off the window; the pane's own state is untouched,
    /// so the next draw of an open pane puts the overlay back.
    fn hide_overlay(&mut self, cx: &mut Cx) {
        if self.spawned && self.overlay_visible {
            let area = self.page_area(cx);
            cx.system_browser(self.browser_id()).update(area, false);
        }
        self.overlay_visible = false;
        self.drawn = false;
    }

    /// The watchdog's tick. Drawn since the last one: clear the mark and
    /// ask for the redraw the next tick looks for (the framework draws on
    /// demand, so an idle pane would otherwise never draw again and look
    /// hidden). Not drawn: the tile is off screen and the overlay goes with
    /// it; the timer keeps running, so the tile's return is seen.
    fn on_tick(&mut self, cx: &mut Cx) {
        if self.drawn {
            self.drawn = false;
            self.view.redraw(cx);
        } else {
            self.hide_overlay(cx);
        }
    }

    fn page_area(&self, cx: &Cx) -> Area {
        self.view.widget(cx, ids!(page)).area()
    }
}

impl Widget for ArticleReader {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if self.timer.is_some_and(|timer| timer.is_event(event).is_some()) {
            self.on_tick(cx);
        }
        if let Event::Actions(actions) = event {
            if self.view.button(cx, ids!(back)).clicked(actions) {
                self.close(cx);
            }
            if self.view.button(cx, ids!(more)).clicked(actions) {
                cx.widget_action(self.widget_uid(), ReaderAction::More);
            }
            if self.view.button(cx, ids!(retry)).clicked(actions) {
                if let Some(url) = self.url.clone() {
                    self.open(cx, &url);
                }
            }
            // The platform reports a failed main-frame load for one browser;
            // only this widget's own failures belong to this pane.
            for action in actions {
                if let Some(err) = action.downcast_ref::<
                    makepad_widgets::makepad_platform::event::NativeSystemBrowserPageError,
                >() {
                    if err.browser_id == self.browser_id().0.get_value() && self.open {
                        self.show_failure(cx, &err.description);
                    }
                }
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let step = self.view.draw_walk(cx, scope, walk);
        // Keep the overlay glued to the page's rect while the pane is
        // drawn: the rect moves with the tile, and a resize or a scroll of
        // the host's desk lands here as a redraw. A closed pane whose
        // overlay is already off sends nothing: a draw costs it no op.
        if step.is_done() && self.spawned && (self.open || self.overlay_visible) {
            // A failed pane keeps the web view off the window: it has
            // nothing to show, and it would cover the failure.
            let show = self.open && !self.failed;
            let area = self.page_area(cx);
            cx.system_browser(self.browser_id()).update(area, show);
            if show {
                self.overlay_visible = true;
                self.drawn = true;
            }
        }
        step
    }
}
