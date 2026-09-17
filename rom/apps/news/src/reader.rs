//! The in-app article reader: the platform's native web view (WKWebView on
//! macOS and iOS, Android's WebView) glued to this widget's page rect while
//! the pane is open. No platform reports navigation at this revision, so
//! the bar shows the link's host, Back and Close; the page itself is the
//! web view's own affair. The overlay belongs to the window that owns the
//! widget's draw pass: the standalone window, or the host's window for an
//! in-process module, and it only moves or hides when this widget draws or
//! is told to. So while the pane is open a watchdog runs on a slow timer:
//! every half second it asks for a redraw and checks that the redraw it
//! asked for last time drew this widget; a tick that finds no draw (the
//! tile is on a hidden workspace, another tile went fullscreen) takes the
//! overlay off the window, and the next draw puts it back. Two redraws of
//! the tile a second while reading, none when the pane is closed: a closed
//! pane runs no timer.

use crate::model::host_of;
use makepad_widgets::*;

/// The watchdog's period: how long the overlay may outlive its tile.
const WATCHDOG_SECONDS: f64 = 0.5;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // The bar's buttons: the host's line icons in the glass ink, the size
    // of the list's refresh button.
    let BackButton = glass.IconButton{width: 34 height: 34 text: "" spacing: 0
        icon_walk: Walk{width: 16 height: 16}
        draw_icon +: {svg: crate_resource("self:resources/icons/chevron-left.svg") color: #xf8fbff}
    }
    let CloseButton = glass.IconButton{width: 34 height: 34 text: "" spacing: 0
        icon_walk: Walk{width: 16 height: 16}
        draw_icon +: {svg: crate_resource("self:resources/icons/close.svg") color: #xf8fbff}
    }

    mod.widgets.ArticleReaderBase = #(ArticleReader::register_widget(vm))
    mod.widgets.ArticleReader = set_type_default() do mod.widgets.ArticleReaderBase{
        width: Fill height: Fill flow: Down spacing: 8
        bar := glass.NavBar{margin: Inset{left: 12 right: 12}
            back := BackButton{}
            host := glass.Caption{width: Fill padding: 0 max_lines: 1 text_overflow: Ellipsis}
            close := CloseButton{}
        }
        // The page's ground until the web view covers it; the overlay is
        // glued to this view's rect, not the whole widget's, so the bar
        // stays above it.
        page := SolidView{width: Fill height: Fill draw_bg.color: #101623}
    }
}

/// What the reader tells its parent: the person closed it, so the list
/// comes back.
#[derive(Clone, Debug, Default)]
pub enum ReaderAction {
    #[default]
    None,
    Closed,
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
}

impl ArticleReader {
    /// The native web view is this widget's own: its uid is unique in the
    /// process, so two readers (two module instances) never share one.
    fn browser_id(&self) -> SystemBrowserId {
        SystemBrowserId(LiveId(self.widget_uid().0))
    }

    /// Show `url` in the pane.
    pub fn open(&mut self, cx: &mut Cx, url: &str) {
        let id = self.browser_id();
        if self.spawned {
            cx.system_browser(id).set_url(url, false);
        } else {
            cx.system_browser(id).spawn(url);
            self.spawned = true;
        }
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
    #[cfg(test)]
    pub(crate) fn url(&self) -> Option<&str> {
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
            if self.view.button(cx, ids!(back)).clicked(actions) && self.spawned {
                cx.system_browser(self.browser_id()).history_go(-1);
            }
            if self.view.button(cx, ids!(close)).clicked(actions) {
                self.close(cx);
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
            let area = self.page_area(cx);
            cx.system_browser(self.browser_id()).update(area, self.open);
            if self.open {
                self.overlay_visible = true;
                self.drawn = true;
            }
        }
        step
    }
}
