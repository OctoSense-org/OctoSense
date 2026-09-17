//! The news surface: the DSL for both faces — the full list with a tab per
//! source, and the wide home tile the host switches to with
//! `HostedViewMode` over `Event::Custom` (the `HostedView` child reads it
//! and swaps faces on its own) — plus the fetches, the tick timer, the
//! storage jail (the person's feeds file, read once at the start, and the
//! per-source cache) and the opener chain: the bundled Browser through the
//! host, the in-app reader on the native web view, the system browser, a
//! notification, the first that can here (`OpenPolicy`). Both faces are liquid
//! glass over the app's own backdrop: the glass widgets assume a dark scene
//! with light text, and the backdrop is what gives them one in the light
//! appearance too. `ensure_started` seeds everything on the first event or
//! draw, whichever the host gives it first: a window has a `Startup` event,
//! a module instance does not.

use crate::model::{
    body_text, host_of, is_http_url, meta_line, parse_body, parse_user_feeds, source_color, source_short_label, tile_line, Headline,
    Hosting, NewsModel, OpenPolicy, OpenTier, WmUnavailable, FEEDS_KEY, MAX_BODY_BYTES,
};
use crate::reader::{ArticleReader, ReaderAction};
use makepad_widgets::makepad_platform::storage::{StorageHandle, StorageRequestId, StorageResponse, StorageResult};
use makepad_widgets::*;
use makepad_wm_api::WmRequest;

/// Below this height the tile drops its heading to keep three headlines.
/// Below roughly 70 pt (the 60 pt banner floor on the smallest Android homes
/// with five tile apps) the third headline clips; no shipping phone size
/// reaches it.
const SHORT_TILE: f64 = 96.0;
/// The chevron's turn when a row is open: the down chevron points up.
const OPEN_CHEVRON: f32 = std::f32::consts::PI;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // The app's ground in both appearances: deep ink at the top-left
    // warming to navy at the bottom-right, with a faint diagonal highlight.
    let Backdrop = View{width: Fill height: Fill show_bg: true draw_bg +: {
        pixel: fn(){
            let ink = vec3(0.055, 0.075, 0.125)
            let navy = vec3(0.110, 0.135, 0.205)
            let d = smoothstep(0.0, 1.0, self.pos.x * 0.35 + self.pos.y * 0.65)
            let band = 1.0 - abs(self.pos.x - self.pos.y)
            let c = mix(ink, navy, d) + vec3(0.03, 0.03, 0.03) * smoothstep(0.7, 1.0, band)
            return vec4(c, 1.0)
        }
    }}
    // A plain `View` background has no shader at this revision (its
    // `DrawQuad` base declares no pixel function), so the hairline, the
    // badge and the tile dot sit on the views that do declare one.
    let Hairline = SolidView{width: Fill height: 0.5 draw_bg.color: #ffffff22}
    // The glass text roles without `Label`'s own padding, so the rows and
    // the tile lines are as tall as their text.
    let Body = glass.Body{padding: 0}
    let Caption = glass.Caption{padding: 0}
    // The framework's `glass.Badge` shape, on a base that draws it: the
    // source colour fills it (`draw_bg.color`, set per row; `#7d8aa5` is
    // `source_color`'s slate, for a row drawn before `render` colours it).
    let Badge = RoundedView{width: Fit height: 20 flow: Right align: Center padding: Inset{left: 7 right: 7 top: 0 bottom: 0}
        draw_bg +: {color: #7d8aa5 border_color: #ffffff30 border_size: 1.0 border_radius: 5.0}
    }
    // The glass icon buttons carry the host's line icons in the glass ink.
    // Pulled up so its centre sits on the title's first line, like the badge's.
    let ChevronButton = glass.IconButton{width: 28 height: 28 text: "" spacing: 0 margin: Inset{top: -4}
        icon_walk: Walk{width: 14 height: 14}
        draw_icon +: {svg: crate_resource("self:resources/icons/chevron-down.svg") color: #xf8fbff}
    }
    let RefreshButton = glass.IconButton{width: 34 height: 34 text: "" spacing: 0
        icon_walk: Walk{width: 16 height: 16}
        draw_icon +: {svg: crate_resource("self:resources/icons/refresh.svg") color: #xf8fbff}
    }

    // One headline: a card. A tap anywhere on it opens the link; the
    // chevron on the title line reveals the summary inside the same card.
    let NewsRow = View{width: Fill height: Fit flow: Down padding: Inset{left: 12 right: 12 top: 4 bottom: 4}
        card := glass.Card{width: Fill height: Fit flow: Down spacing: 6 cursor: MouseCursor.Hand
            View{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.0}
                badge := Badge{
                    badge_text := Label{width: Fit padding: 0 draw_text +: {color: #ffffff text_style: theme.font_bold{font_size: 9}}}
                }
                title := Body{max_lines: 2 text_overflow: Ellipsis draw_text.color: #xf2f6ffff draw_text.text_style.font_size: 14}
                expand := ChevronButton{}
            }
            meta := Caption{width: Fill max_lines: 1 text_overflow: Ellipsis}
            detail := View{visible: false width: Fill height: Fit flow: Down spacing: 8
                Hairline{}
                summary := Body{max_lines: 8 draw_text.text_style.font_size: 12}
                View{width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5}
                    host := Caption{width: Fill max_lines: 1 text_overflow: Ellipsis}
                    discussion := glass.Chip{visible: false height: 28 text: "Discussion"}
                }
            }
        }
    }

    // One tile line: the source's dot (slate until `render` colours it, as
    // the badge) and the headline.
    let TileLine = View{width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
        dot := RoundedView{width: 7 height: 7 draw_bg +: {color: #7d8aa5 border_radius: 3.5}}
        line := Body{max_lines: 1 text_overflow: Ellipsis draw_text.color: #xf2f6ffff draw_text.text_style.font_size: 11.25}
    }

    mod.widgets.NewsViewBase = #(NewsView::register_widget(vm))
    mod.widgets.NewsView = set_type_default() do mod.widgets.NewsViewBase{
        width: Fill height: Fill
        app_view := HostedView{
            full: View{width: Fill height: Fill flow: Overlay
                Backdrop{}
                View{width: Fill height: Fill flow: Down spacing: 8 padding: Inset{top: 10}
                    bar := glass.NavBar{margin: Inset{left: 12 right: 12}
                        Label{text: "News" padding: Inset{left: 6} draw_text +: {color: #ffffff text_style: theme.font_bold{font_size: 20}}}
                        status := Caption{width: Fill max_lines: 1 text_overflow: Ellipsis}
                        refresh := RefreshButton{}
                    }
                    View{width: Fill height: Fit padding: Inset{left: 12 right: 12}
                        tabs := glass.GlassSegmented{width: Fill height: 38 labels: ["All"]}
                    }
                    // Fixed amber, like every other ink on the backdrop: the
                    // theme's warning colour is for its own ground.
                    error_line := Caption{visible: false width: Fill padding: Inset{left: 16 right: 16} draw_text.color: #f0b35c}
                    // In a view of its own: a `PortalList` has no visibility
                    // to set, and it must give the whole column up to the
                    // placeholder or the reader.
                    list_box := View{width: Fill height: Fill
                        list := PortalList{width: Fill height: Fill
                            Row := NewsRow{}
                        }
                    }
                    empty := View{visible: false width: Fill height: Fill align: Align{x: 0.5 y: 0.5}
                        empty_text := Caption{width: Fit text: "Loading…"}
                    }
                    // Takes the list's place while an article is open; the
                    // header and the tabs stay.
                    reader := ArticleReader{visible: false}
                }
            }
            tile: View{width: Fill height: Fill flow: Overlay
                Backdrop{}
                glass.Card{width: Fill height: Fill margin: 5 flow: Down spacing: 3 padding: Inset{left: 12 right: 12 top: 6 bottom: 6}
                    tile_title := Caption{text: "NEWS" draw_text.text_style.font_size: 9.5}
                    tile_0 := TileLine{}
                    tile_1 := TileLine{}
                    tile_2 := TileLine{}
                }
            }
        }
    }
}

/// The headlines, the tabs and the tile: one widget with two resident
/// faces (`app_view`, a `HostedView`). Owns everything the standalone
/// window's `App` would otherwise own, so a module host gets the same news
/// a window does.
#[derive(Script, ScriptHook, Widget)]
pub struct NewsView {
    #[deref]
    view: View,
    /// Set once, on the first event or draw.
    #[rust]
    started: bool,
    #[rust]
    model: NewsModel,
    /// The current tab's rows, recomputed by `render`, read by the list.
    #[rust]
    shown: Vec<Headline>,
    /// The labels the tab strip was last given: it is re-applied when a
    /// feed is added, not on every render.
    #[rust]
    tab_labels: Vec<String>,
    /// A slow tick that re-checks the refresh budget and ages the times.
    #[rust]
    tick: Option<Timer>,
    /// The instance's storage jail (module) or its own namespace
    /// (standalone): the feeds file and the per-source cache live there.
    #[rust]
    storage: Option<StorageHandle>,
    #[rust]
    feeds_load: Option<StorageRequestId>,
    /// Cache reads in flight, with the source id each is for: the source
    /// may have moved (or gone) by the time the bytes land.
    #[rust]
    cache_loads: Vec<(StorageRequestId, String)>,
    /// Cache writes in flight, with the key each is for, so a failure is
    /// logged by name.
    #[rust]
    cache_writes: Vec<(StorageRequestId, String)>,
    /// Whether the tile last drew without its heading, so the visibility
    /// flips once per size change rather than every frame.
    #[rust]
    short_tile: Option<bool>,
    /// How this view is hosted (`main.rs` and the module's `create` say):
    /// the opener policy and the request channel read it.
    #[rust]
    hosting: Hosting,
    /// The link being opened and the tiers still to try for it: the host's
    /// `wm_unavailable` reply for that link moves on to the next. A Browser
    /// that launched sends no reply, so after a Browser request the entry
    /// lingers until the next open or a reply, which is what lets a process
    /// child's asynchronous `wm_unavailable` land over its socket. A
    /// module's host answers while it handles the actions pass that carries
    /// the posted request, so there the entry is dropped on the pass after
    /// that one (`asked_host`): by then the answer has come or never will.
    #[rust]
    pending_open: Option<(String, Vec<OpenTier>)>,
    /// A module posted a Browser request the host has not had its actions
    /// pass for yet.
    #[rust]
    asked_host: bool,
}

impl NewsView {
    /// A host handed this instance its storage: feeds and cache load from
    /// and save to it from now on. Call once: a second handle is ignored,
    /// so the feeds file is read from one jail only. Before the start
    /// `ensure_started` issues the loads; after it they go out at once, so
    /// a handle that arrives late is never a silent no-op.
    pub fn set_storage(&mut self, cx: &mut Cx, storage: StorageHandle) {
        if self.storage.is_some() {
            log!("news: set_storage called twice; keeping the first jail");
            return;
        }
        self.storage = Some(storage.clone());
        if self.started {
            self.load_from(cx, &storage);
        }
    }

    #[cfg(test)]
    pub(crate) fn has_pending_loads(&self) -> bool {
        self.feeds_load.is_some() || !self.cache_loads.is_empty()
    }

    /// The link an opener tier was asked for and not yet answered.
    #[cfg(test)]
    pub(crate) fn pending_open_url(&self) -> Option<&str> {
        self.pending_open.as_ref().map(|(url, _)| url.as_str())
    }

    /// Where this view runs; the default is a window of its own.
    pub fn set_hosting(&mut self, hosting: Hosting) {
        self.hosting = hosting;
    }

    /// Whether the reader pane is showing an article over the list.
    pub fn reader_open(&self, cx: &Cx) -> bool {
        self.view.widget(cx, ids!(reader)).borrow::<ArticleReader>().is_some_and(|r| r.is_open())
    }

    pub fn model(&self) -> &NewsModel {
        &self.model
    }

    /// The face the host asked for, read off the embedded `HostedView`.
    pub fn face(&self, cx: &Cx) -> HostedViewMode {
        self.view.widget(cx, ids!(app_view)).borrow::<HostedView>().map(|h| h.mode()).unwrap_or_default()
    }

    /// One line for the assistant's volatile context.
    pub fn ai_summary(&self) -> String {
        crate::ai::context_line(&self.model)
    }

    /// The timer stops, every fetch in flight is cancelled and the reader's
    /// web view is released; nothing else this widget owns outlives its
    /// isolate.
    pub fn shutdown(&mut self, cx: &mut Cx) {
        if let Some(t) = self.tick.take() {
            cx.stop_timer(t);
        }
        for id in self.model.cancel_all() {
            cx.cancel_http_request(id);
        }
        if let Some(mut reader) = self.view.widget(cx, ids!(reader)).borrow_mut::<ArticleReader>() {
            reader.shutdown(cx);
        }
    }

    /// Start on the first event or draw, whichever comes first.
    fn ensure_started(&mut self, cx: &mut Cx) {
        if self.started {
            return;
        }
        self.started = true;
        if let Some(storage) = self.storage.clone() {
            self.load_from(cx, &storage);
        }
        self.tick = Some(cx.start_interval(30.0));
        self.fetch_tab(cx, self.model.tab, false);
        self.render(cx);
    }

    /// The feeds file and every source's cache, from the jail.
    fn load_from(&mut self, cx: &mut Cx, storage: &StorageHandle) {
        self.feeds_load = Some(storage.get(cx, FEEDS_KEY));
        self.load_caches(cx, storage, 0..self.model.sources.len());
    }

    fn load_caches(&mut self, cx: &mut Cx, storage: &StorageHandle, sources: std::ops::Range<usize>) {
        for i in sources {
            let key = self.model.cache_key(i);
            let id = self.model.sources[i].def.id.clone();
            self.cache_loads.push((storage.get(cx, &key), id));
        }
    }

    /// Fetch the tab's stale sources (`force`: every source it shows, the
    /// manual refresh — `due` would hold a fresh or failed one for the
    /// budget).
    fn fetch_tab(&mut self, cx: &mut Cx, tab: usize, force: bool) {
        for i in self.model.sources_for_tab(tab) {
            if force || self.model.due(i) {
                self.fetch_source(cx, i);
            }
        }
    }

    fn fetch_source(&mut self, cx: &mut Cx, source: usize) {
        if let Some(old) = self.model.sources[source].pending.take() {
            cx.cancel_http_request(old);
        }
        let (id, url) = self.model.begin_fetch(source);
        let mut request = HttpRequest::new(url, HttpMethod::GET);
        request.set_header(
            "Accept".into(),
            "application/rss+xml, application/atom+xml, application/xml, text/xml, application/json;q=0.9, */*;q=0.8".into(),
        );
        request.set_header("User-Agent".into(), "OctoSense-News/0.1".into());
        // A user feed may point anywhere: the backend stops allocating at
        // the cap while bytes arrive. The Android Java backend has no
        // transport cap, so the check on the landed body (`body_text`) is
        // the line that holds there.
        request.max_response_body_bytes = MAX_BODY_BYTES as u64;
        cx.http_request(id, request);
    }

    fn handle_response(&mut self, cx: &mut Cx, id: LiveId, result: Result<HttpResponse, String>) {
        // A reply for a request we already superseded: drop it untouched.
        let Some(source) = self.model.owns(id) else { return };
        let kind = self.model.sources[source].def.kind;
        let parsed = result
            .and_then(|response| body_text(response.status_code, response.body.as_deref()).and_then(|text| parse_body(kind, text)));
        let good = parsed.is_ok();
        self.model.complete(id, parsed);
        if good {
            // Only rows from the network are worth saving: a failure keeps
            // whatever the source had, which is already in the jail.
            if let Some(storage) = self.storage.as_ref() {
                let key = self.model.cache_key(source);
                let id = storage.set(cx, &key, self.model.cache_bytes(source));
                self.cache_writes.push((id, key));
            }
        }
        self.render(cx);
    }

    fn on_storage(&mut self, cx: &mut Cx, responses: &[StorageResponse]) {
        for response in responses {
            if self.feeds_load == Some(response.request_id) {
                self.feeds_load = None;
                match &response.result {
                    Ok(StorageResult::Value(Some(bytes))) => self.apply_feeds_file(cx, bytes),
                    // No file: the built-in three alone.
                    Ok(_) => {}
                    Err(e) => log!("news: could not read {FEEDS_KEY}: {e}"),
                }
                self.render(cx);
            } else if let Some(pos) = self.cache_loads.iter().position(|(id, _)| *id == response.request_id) {
                let (_, source_id) = self.cache_loads.remove(pos);
                match &response.result {
                    Ok(StorageResult::Value(Some(bytes))) => {
                        if let Some(i) = self.model.sources.iter().position(|s| s.def.id == source_id) {
                            self.model.load_cache(i, bytes);
                            self.render(cx);
                        }
                    }
                    Ok(_) => {}
                    Err(e) => log!("news: could not read the saved headlines of {source_id}: {e}"),
                }
            } else if let Some(pos) = self.cache_writes.iter().position(|(id, _)| *id == response.request_id) {
                let (_, key) = self.cache_writes.remove(pos);
                if let Err(e) = &response.result {
                    log!("news: could not save {key}: {e}");
                }
            }
        }
    }

    /// The person's feeds file landed: the user tabs follow it, their
    /// caches load, and whatever the current tab now shows is fetched.
    fn apply_feeds_file(&mut self, cx: &mut Cx, bytes: &[u8]) {
        let parsed = std::str::from_utf8(bytes).map_err(|_| "not UTF-8".to_string()).and_then(parse_user_feeds);
        let feeds = match parsed {
            Ok(feeds) => feeds,
            Err(e) => {
                log!("news: {FEEDS_KEY} is not readable: {e}");
                return;
            }
        };
        let (added, cancelled) = self.model.set_user_feeds(feeds);
        for id in cancelled {
            cx.cancel_http_request(id);
        }
        if let Some(storage) = self.storage.clone() {
            self.load_caches(cx, &storage, added);
        }
        // The new sources are due wherever the current tab shows them:
        // All, or the user tab itself.
        self.fetch_tab(cx, self.model.tab, false);
    }

    fn select_tab(&mut self, cx: &mut Cx, tab: usize) {
        if self.model.select_tab(tab) {
            self.fetch_tab(cx, tab, false);
            self.view.portal_list(cx, ids!(list)).set_first_id_and_scroll(0, 0.0);
            self.render(cx);
        }
    }

    /// Open a link with the first tier that can here: the bundled Browser
    /// through the host, the in-app reader, the system browser, a
    /// notification (`OpenPolicy`). The one place a link leaves the list: a
    /// card tap and the Discussion chip both come here, and only an http(s)
    /// link gets past it. Nothing on the tile face: a tap there is the
    /// host's, and opens the full app.
    pub fn open_link(&mut self, cx: &mut Cx, url: &str) {
        if !is_http_url(url) || self.face(cx) == HostedViewMode::Tile {
            return;
        }
        let tiers = OpenPolicy::for_platform(self.hosting).tiers();
        self.pending_open = Some((url.to_string(), tiers));
        self.try_next_tier(cx);
    }

    /// The next tier for the pending link, and the one after when a tier
    /// cannot even be asked. A tier that was asked (the Browser request)
    /// leaves `pending_open` in place for the host's answer; the others
    /// settle the link here.
    fn try_next_tier(&mut self, cx: &mut Cx) {
        self.asked_host = false;
        loop {
            let Some((url, tiers)) = self.pending_open.as_mut() else { return };
            if tiers.is_empty() {
                self.pending_open = None;
                return;
            }
            let tier = tiers.remove(0);
            let url = url.clone();
            match tier {
                OpenTier::Browser => {
                    let req = WmRequest::Open { app: Some("browser".into()), path: url };
                    if self.send_request(cx, req) {
                        self.asked_host = self.hosting == Hosting::Module;
                        return;
                    }
                }
                OpenTier::Reader => {
                    self.pending_open = None;
                    if let Some(mut reader) = self.view.widget(cx, ids!(reader)).borrow_mut::<ArticleReader>() {
                        reader.open(cx, &url);
                    }
                    self.render(cx);
                    return;
                }
                OpenTier::System => {
                    self.pending_open = None;
                    cx.open_url(&url, OpenUrlInPlace::No);
                    return;
                }
                OpenTier::Notify => {
                    self.pending_open = None;
                    if !self.send_request(cx, browser_failed_notice(&url)) {
                        log!("news: nothing here can open {url}");
                    }
                    return;
                }
            }
        }
    }

    /// A request to the window manager by the channel this hosting has: a
    /// widget action from the root for a module (the host attributes it by
    /// the root's uid), the studio socket for a process. False when there
    /// is no window manager to ask.
    fn send_request(&mut self, cx: &mut Cx, req: WmRequest) -> bool {
        match self.hosting {
            Hosting::Module => {
                cx.widget_action(self.widget_uid(), req);
                true
            }
            Hosting::Process => makepad_wm_api::send(cx, &req),
            Hosting::Standalone => false,
        }
    }

    /// Whether this actions pass is the one carrying a request this root
    /// posted: the host answers during it, so it settles nothing yet.
    fn carries_own_request(&self, actions: &Actions) -> bool {
        let uid = self.widget_uid();
        actions
            .iter()
            .filter_map(|a| a.as_widget_action())
            .any(|wa| wa.widget_uid == uid && wa.action.downcast_ref::<WmRequest>().is_some())
    }

    /// Close the reader pane, if open, and show the list again.
    pub fn close_reader(&mut self, cx: &mut Cx) {
        if let Some(mut reader) = self.view.widget(cx, ids!(reader)).borrow_mut::<ArticleReader>() {
            reader.close(cx);
        }
        self.render(cx);
    }

    /// The badge's text: the row's own source when it names one (an
    /// outlet), else its tab's label, shortened.
    fn badge_label(&self, row: &Headline) -> String {
        let source = if row.source.is_empty() {
            self.model.sources.iter().find(|s| s.def.id == row.source_id).map(|s| s.def.label.as_str()).unwrap_or("")
        } else {
            row.source.as_str()
        };
        source_short_label(&row.source_id, source)
    }

    /// Push the model into both faces.
    fn render(&mut self, cx: &mut Cx) {
        let tab = self.model.tab;
        let labels: Vec<String> = (0..self.model.tab_count()).map(|i| self.model.tab_label(i).to_string()).collect();
        if labels != self.tab_labels {
            self.tab_labels = labels.clone();
            let mut tabs = self.view.widget(cx, ids!(tabs));
            script_apply_eval!(cx, tabs, { labels: #(labels) });
        }
        let tabs = self.view.glass_segmented(cx, ids!(tabs));
        // Only when it differs: a tap already moved the strip's own
        // selection, and setting it again would cut the pill's glide.
        if tabs.selected() != tab {
            tabs.set_selected(cx, tab);
        }
        self.view.label(cx, ids!(status)).set_text(cx, &self.model.status_text(tab));
        // An open article takes the list's place; the header and the tabs
        // stay, the rest waits behind it.
        let reader_open = self.reader_open(cx);
        let error = self.model.error_text(tab);
        self.view.widget(cx, ids!(error_line)).set_visible(cx, error.is_some() && !reader_open);
        self.view.label(cx, ids!(error_line)).set_text(cx, error.as_deref().unwrap_or(""));

        self.shown = self.model.rows_for_tab(tab);
        let list = self.view.portal_list(cx, ids!(list));
        // A refresh into a shorter list: back to the top rather than a
        // blank page past the end.
        let first = list.first_id();
        if first > 0 && self.shown.len() <= first {
            list.set_first_id_and_scroll(0, 0.0);
        }
        let empty = self.shown.is_empty();
        self.view.widget(cx, ids!(list_box)).set_visible(cx, !empty && !reader_open);
        self.view.widget(cx, ids!(empty)).set_visible(cx, empty && !reader_open);
        let placeholder = if self.model.loading(tab) { "Loading…" } else { "No headlines" };
        self.view.label(cx, ids!(empty_text)).set_text(cx, placeholder);

        let tile = self.model.tile_rows();
        for (i, id) in [ids!(tile_0), ids!(tile_1), ids!(tile_2)].iter().enumerate() {
            let line = self.view.widget(cx, *id);
            let row = tile.get(i);
            let text = row.map(tile_line).unwrap_or_else(|| if i == 0 { self.model.status_text(0) } else { String::new() });
            line.label(cx, ids!(line)).set_text(cx, &text);
            // The status line, or an empty one, has no source to mark.
            line.widget(cx, ids!(dot)).set_visible(cx, row.is_some());
            if let Some(row) = row {
                let mut dot = line.widget(cx, ids!(dot));
                let color = Vec4f::from_u32(source_color(&row.source_id));
                script_apply_eval!(cx, dot, { draw_bg.color: #(color) });
            }
        }
        self.view.redraw(cx);
    }

    fn draw_rows(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        let now = now_unix();
        list.set_item_range(cx, 0, self.shown.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(row) = self.shown.get(index) else { continue };
            let item = list.item(cx, index, live_id!(Row));
            let expanded = self.model.is_expanded(&row.link);
            let mut badge = item.widget(cx, ids!(badge));
            let color = Vec4f::from_u32(source_color(&row.source_id));
            script_apply_eval!(cx, badge, { draw_bg.color: #(color) });
            item.label(cx, ids!(badge_text)).set_text(cx, &self.badge_label(row));
            item.label(cx, ids!(title)).set_text(cx, &row.title);
            item.label(cx, ids!(meta)).set_text(cx, &meta_line(row, now));
            // The icon's own turn, not a script apply: that would re-apply
            // the button and drop its loaded document.
            if let Some(mut expand) = item.button(cx, ids!(expand)).borrow_mut() {
                expand.draw_icon.rotation = if expanded { OPEN_CHEVRON } else { 0.0 };
            }
            item.widget(cx, ids!(detail)).set_visible(cx, expanded);
            if expanded {
                let summary = if row.summary.is_empty() { "No summary from this source." } else { row.summary.as_str() };
                item.label(cx, ids!(summary)).set_text(cx, summary);
                item.label(cx, ids!(host)).set_text(cx, &host_of(&row.link));
                item.widget(cx, ids!(discussion)).set_visible(cx, row.discussion.is_some());
            }
            item.draw_all(cx, &mut Scope::empty());
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The last tier's notice: the Browser could not be launched here, or was
/// and failed to start, and nothing else on this platform opens a link.
pub(crate) fn browser_failed_notice(url: &str) -> WmRequest {
    WmRequest::Notify { title: "Could not open in the Browser".into(), body: host_of(url) }
}

impl Widget for NewsView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.ensure_started(cx);
        if let Event::Custom(json) = event {
            if let Some(reply) = WmUnavailable::parse(json) {
                // The host cannot launch the Browser for this link: the next
                // tier. An answer for some other link is not ours to act on.
                if self.pending_open.as_ref().is_some_and(|(url, _)| *url == reply.path) {
                    self.try_next_tier(cx);
                }
            } else if HostedViewMode::parse(json) == Some(HostedViewMode::Tile) {
                // The full face goes away under an open article: the overlay
                // would stay on the window on its own.
                self.close_reader(cx);
            }
        }
        if let Event::Storage(responses) = event {
            self.on_storage(cx, responses);
        }
        if let Event::NetworkResponses(responses) = event {
            for response in responses {
                match response {
                    NetworkResponse::HttpResponse { request_id, response } => {
                        self.handle_response(cx, *request_id, Ok(response.clone()));
                    }
                    NetworkResponse::HttpError { request_id, error } => {
                        self.handle_response(cx, *request_id, Err(error.message.clone()));
                    }
                    _ => {}
                }
            }
        }
        if self.tick.as_ref().is_some_and(|t| t.is_event(event).is_some()) {
            self.fetch_tab(cx, self.model.tab, false);
            self.render(cx);
        }
        if let Event::Actions(actions) = event {
            if self.asked_host && !self.carries_own_request(actions) {
                // The host had its pass for the request: it answered (and
                // the link moved on, above) or launched the Browser, which
                // never answers. Either way nothing waits any more.
                self.asked_host = false;
                self.pending_open = None;
            }
            if self.view.button(cx, ids!(refresh)).clicked(actions) {
                self.fetch_tab(cx, self.model.tab, true);
                self.render(cx);
            }
            let tabs = self.view.glass_segmented(cx, ids!(tabs));
            if tabs.changed(actions) {
                let tab = tabs.selected();
                self.select_tab(cx, tab);
            }
            let list = self.view.portal_list(cx, ids!(list));
            let mut open = None;
            let mut toggle = None;
            for (index, item) in list.items_with_actions(actions) {
                // Beyond `shown`: a row removed by a refresh in the same frame.
                let Some(row) = self.shown.get(index) else { continue };
                if item.button(cx, ids!(expand)).clicked(actions) {
                    toggle = Some(row.link.clone());
                } else if item.button(cx, ids!(discussion)).clicked(actions) {
                    open = row.discussion.clone();
                } else if item.widget(cx, ids!(card)).as_view().finger_up(actions).is_some_and(|up| up.was_tap()) {
                    // A tap, not the end of a scroll drag over the card. The
                    // glass card shares its inner view's uid, which is all
                    // `finger_up` reads.
                    open = Some(row.link.clone());
                }
            }
            if let Some(link) = toggle {
                self.model.toggle_expanded(&link);
                self.render(cx);
            }
            if let Some(url) = open {
                self.open_link(cx, &url);
            }
            let reader_uid = self.view.widget(cx, ids!(reader)).widget_uid();
            if let ReaderAction::Closed = actions.find_widget_action(reader_uid).cast::<ReaderAction>() {
                self.render(cx);
            }
        }
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.ensure_started(cx);
        let height = cx.turtle().rect().size.y;
        // A zero-height first frame says nothing about the room.
        if height >= 1.0 {
            let short = height < SHORT_TILE;
            if self.short_tile != Some(short) {
                self.short_tile = Some(short);
                self.view.widget(cx, ids!(tile_title)).set_visible(cx, !short);
            }
        }
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.as_portal_list().borrow_mut() {
                self.draw_rows(cx, &mut list);
            }
        }
        DrawStep::done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::isolate_root;

    /// A `NewsView` in an isolate of its own, started on the full face and
    /// hosted as `hosting`, handed to `test` inside the isolate; torn down
    /// in the host's order afterwards. No network: nothing lands.
    fn with_view(hosting: Hosting, test: impl FnOnce(&mut Cx, &WidgetRef)) {
        let (mut iso, root) = isolate_root();
        iso.entered(|cx| {
            root.handle_event(cx, &Event::Custom(HostedViewMode::Full.to_json()), &mut Scope::empty());
            root.borrow_mut::<NewsView>().unwrap().set_hosting(hosting);
            test(cx, &root);
            root.borrow_mut::<NewsView>().unwrap().shutdown(cx);
        });
        iso.teardown(root);
    }

    /// Whether a view hosted as `hosting` gets the reader here: the
    /// policy's own answer for this platform, not a guess at the OS.
    fn reader_here(hosting: Hosting) -> bool {
        OpenPolicy::for_platform(hosting).has_webview
    }

    fn unavailable(path: &str) -> Event {
        Event::Custom(WmUnavailable { app: "browser".into(), path: path.into() }.to_json())
    }

    /// The actions the root posted while `f` ran.
    fn posted_actions(cx: &mut Cx, root: &WidgetRef, f: impl FnOnce(&mut Cx, &mut NewsView)) -> ActionsBuf {
        cx.capture_actions(|cx| f(cx, &mut root.borrow_mut::<NewsView>().unwrap()))
    }

    /// The `WmRequest` the root posted while `f` ran, if one.
    fn posted_request(cx: &mut Cx, root: &WidgetRef, f: impl FnOnce(&mut Cx, &mut NewsView)) -> Option<WmRequest> {
        let uid = root.widget_uid();
        posted_actions(cx, root, f)
            .iter()
            .filter_map(|a| a.as_widget_action())
            .find(|wa| wa.widget_uid == uid)
            .and_then(|wa| wa.action.downcast_ref::<WmRequest>().cloned())
    }

    fn visible(cx: &Cx, root: &WidgetRef, id: &[LiveId]) -> bool {
        root.borrow::<NewsView>().unwrap().view.widget(cx, id).visible()
    }

    #[test]
    fn the_reader_starts_hidden_behind_the_list() {
        with_view(Hosting::Standalone, |cx, root| {
            let view = root.borrow::<NewsView>().unwrap();
            assert!(!view.reader_open(cx));
            assert_eq!(view.pending_open_url(), None);
            drop(view);
            assert!(!visible(cx, root, ids!(reader)));
            assert!(visible(cx, root, ids!(empty)), "nothing landed: the placeholder shows");
        });
    }

    /// A module asks the host for the Browser first; the host's
    /// `wm_unavailable` for that link moves on to the reader where the
    /// platform has a web view, else past it.
    #[test]
    fn a_module_asks_for_the_browser_and_falls_back_on_the_reply() {
        with_view(Hosting::Module, |cx, root| {
            let req = posted_request(cx, root, |cx, view| view.open_link(cx, "https://x/a"));
            assert_eq!(req, Some(WmRequest::Open { app: Some("browser".into()), path: "https://x/a".into() }));
            {
                let view = root.borrow::<NewsView>().unwrap();
                assert!(!view.reader_open(cx), "the host has not answered yet");
                assert_eq!(view.pending_open_url(), Some("https://x/a"), "the link waits for the answer");
            }
            root.handle_event(cx, &unavailable("https://x/a"), &mut Scope::empty());
            let view = root.borrow::<NewsView>().unwrap();
            assert_eq!(view.reader_open(cx), reader_here(Hosting::Module), "the reader where the platform has a web view");
            assert_eq!(view.pending_open_url(), None, "the reply settled the link");
            drop(view);
            if reader_here(Hosting::Module) {
                assert!(visible(cx, root, ids!(reader)));
                assert!(!visible(cx, root, ids!(empty)), "the placeholder waits behind the article");
                assert!(!visible(cx, root, ids!(list_box)));
            }
        });
    }

    /// The host answers a module's request during the actions pass that
    /// carries it, and a Browser that launched never answers: the link
    /// waits through that pass and is forgotten on the next, so a late
    /// reply cannot open a reader for a tap long settled.
    #[test]
    fn a_module_forgets_an_unanswered_browser_request_after_the_hosts_pass() {
        with_view(Hosting::Module, |cx, root| {
            let carrying = posted_actions(cx, root, |cx, view| view.open_link(cx, "https://x/a"));
            assert!(carrying.iter().filter_map(|a| a.as_widget_action()).any(|wa| wa.action.downcast_ref::<WmRequest>().is_some()));
            root.handle_event(cx, &Event::Actions(carrying), &mut Scope::empty());
            assert_eq!(root.borrow::<NewsView>().unwrap().pending_open_url(), Some("https://x/a"), "the host may still answer during this pass");
            root.handle_event(cx, &Event::Actions(ActionsBuf::new()), &mut Scope::empty());
            {
                let view = root.borrow::<NewsView>().unwrap();
                assert_eq!(view.pending_open_url(), None, "no answer came: the Browser has the link");
                assert!(!view.reader_open(cx));
            }
            root.handle_event(cx, &unavailable("https://x/a"), &mut Scope::empty());
            assert!(!root.borrow::<NewsView>().unwrap().reader_open(cx), "a reply after the host's pass is not ours");
        });
    }

    #[test]
    fn the_last_tier_names_the_browser_and_the_links_host() {
        let notice = browser_failed_notice("https://www.example.com/a/b");
        assert_eq!(notice, WmRequest::Notify { title: "Could not open in the Browser".into(), body: "example.com".into() });
    }

    #[test]
    fn a_reply_for_another_link_is_not_ours() {
        with_view(Hosting::Module, |cx, root| {
            let req = posted_request(cx, root, |cx, view| view.open_link(cx, "https://x/a"));
            assert!(matches!(req, Some(WmRequest::Open { .. })));
            root.handle_event(cx, &unavailable("https://x/b"), &mut Scope::empty());
            let view = root.borrow::<NewsView>().unwrap();
            assert!(!view.reader_open(cx));
            assert_eq!(view.pending_open_url(), Some("https://x/a"), "still waiting for the answer about a");
        });
    }

    /// A window of its own has no host to ask: the reader at once where
    /// there is a web view; Close brings the list back.
    #[test]
    fn standalone_opens_the_reader_at_once_and_close_restores_the_list() {
        with_view(Hosting::Standalone, |cx, root| {
            let req = posted_request(cx, root, |cx, view| view.open_link(cx, "https://x/a"));
            assert_eq!(req, None, "nobody to ask");
            let view = root.borrow::<NewsView>().unwrap();
            assert_eq!(view.reader_open(cx), reader_here(Hosting::Standalone));
            assert_eq!(view.pending_open_url(), None);
            drop(view);
            if !reader_here(Hosting::Standalone) {
                return;
            }
            {
                let reader = root.borrow::<NewsView>().unwrap().view.widget(cx, ids!(reader));
                assert_eq!(reader.borrow::<ArticleReader>().unwrap().url(), Some("https://x/a"));
            }
            assert!(!visible(cx, root, ids!(empty)));
            root.borrow_mut::<NewsView>().unwrap().close_reader(cx);
            assert!(!root.borrow::<NewsView>().unwrap().reader_open(cx));
            assert!(!visible(cx, root, ids!(reader)));
            assert!(visible(cx, root, ids!(empty)), "the list's face is back");
        });
    }

    /// A tap on the tile is the host's (it opens the full app); the host
    /// switching to the tile takes an open article with it.
    #[test]
    fn the_tile_face_never_opens_a_link_and_closes_an_open_one() {
        with_view(Hosting::Standalone, |cx, root| {
            if reader_here(Hosting::Standalone) {
                root.borrow_mut::<NewsView>().unwrap().open_link(cx, "https://x/a");
                assert!(root.borrow::<NewsView>().unwrap().reader_open(cx));
                root.handle_event(cx, &Event::Custom(HostedViewMode::Tile.to_json()), &mut Scope::empty());
                assert!(!root.borrow::<NewsView>().unwrap().reader_open(cx), "the overlay would outlive the face");
            } else {
                root.handle_event(cx, &Event::Custom(HostedViewMode::Tile.to_json()), &mut Scope::empty());
            }
            let req = posted_request(cx, root, |cx, view| view.open_link(cx, "https://x/b"));
            assert_eq!(req, None);
            let view = root.borrow::<NewsView>().unwrap();
            assert_eq!(view.face(cx), HostedViewMode::Tile);
            assert!(!view.reader_open(cx));
            assert_eq!(view.pending_open_url(), None, "a tile tap is not an open");
        });
    }

    /// One tick of the reader's own watchdog timer.
    fn tick_for(timer: Timer) -> Event {
        Event::Timer(TimerEvent { timer_id: timer.0, time: None })
    }

    /// The overlay is the window's, not the tile's: when the tile stops
    /// being drawn (a hidden workspace, another tile gone fullscreen) the
    /// reader's watchdog takes the web view off the window. The pane stays
    /// open, the watch keeps running, and the next draw puts the overlay
    /// back.
    #[test]
    fn an_open_reader_that_is_not_drawn_hides_its_overlay() {
        with_view(Hosting::Standalone, |cx, root| {
            if !reader_here(Hosting::Standalone) {
                return;
            }
            root.borrow_mut::<NewsView>().unwrap().open_link(cx, "https://x/a");
            let reader = root.borrow::<NewsView>().unwrap().view.widget(cx, ids!(reader));
            let state = |cx: &Cx| {
                let r = reader.borrow::<ArticleReader>().unwrap();
                (r.overlay_visible(), r.watchdog_timer().map(|t| t.0), root.borrow::<NewsView>().unwrap().reader_open(cx))
            };
            let (visible, timer, open) = state(cx);
            assert!(visible && open, "opening puts the overlay on the window");
            let timer = Timer(timer.expect("the watchdog runs while the pane is open"));
            // One tick with no draw is still fine: the open counted as a
            // draw, and the tick asks for the one the next tick looks for.
            root.handle_event(cx, &tick_for(timer), &mut Scope::empty());
            let (visible, running, open) = state(cx);
            assert!(visible && open);
            assert_eq!(running, Some(timer.0), "the same timer keeps running");
            // A second tick without a draw: the tile is not being drawn.
            root.handle_event(cx, &tick_for(timer), &mut Scope::empty());
            let (visible, running, open) = state(cx);
            assert!(!visible, "the overlay is taken off the window");
            assert!(open, "the pane itself stays open");
            assert_eq!(running, Some(timer.0), "and the watchdog keeps looking for the tile's return");
            // Closing the pane stops the watchdog.
            root.borrow_mut::<NewsView>().unwrap().close_reader(cx);
            let (visible, running, open) = state(cx);
            assert!(!visible && !open && running.is_none());
        });
    }

    /// A closed reader costs nothing: no timer runs.
    #[test]
    fn a_closed_reader_runs_no_timer() {
        with_view(Hosting::Standalone, |cx, root| {
            let reader = root.borrow::<NewsView>().unwrap().view.widget(cx, ids!(reader));
            let r = reader.borrow::<ArticleReader>().unwrap();
            assert!(r.watchdog_timer().is_none());
            assert!(!r.overlay_visible());
        });
    }

    /// Only http(s) links leave the list: the one seam where a row's link
    /// reaches a browser refuses anything else.
    #[test]
    fn open_link_refuses_anything_but_http_links() {
        with_view(Hosting::Module, |cx, root| {
            for bad in ["javascript:alert(1)", "file:///etc/passwd", "not a url", ""] {
                let req = posted_request(cx, root, |cx, view| view.open_link(cx, bad));
                assert_eq!(req, None, "{bad:?} asks nobody");
                let view = root.borrow::<NewsView>().unwrap();
                assert!(!view.reader_open(cx), "{bad:?} opens nothing");
                assert_eq!(view.pending_open_url(), None, "{bad:?} is not pending");
            }
            let req = posted_request(cx, root, |cx, view| view.open_link(cx, "HTTP://x/a"));
            assert!(matches!(req, Some(WmRequest::Open { .. })), "the scheme check is case-insensitive");
        });
    }
}
