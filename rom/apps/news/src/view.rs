//! The news surface: the DSL for both faces — the full list with a tab per
//! source, and the wide home tile the host switches to with
//! `HostedViewMode` over `Event::Custom` (the `HostedView` child reads it
//! and swaps faces on its own) — plus the fetches, the tick timer, the
//! storage jail (the person's feeds file, read once at the start, and the
//! per-source cache) and the open-in-browser hand-off. `ensure_started`
//! seeds everything on the first event or draw, whichever the host gives
//! it first: a window has a `Startup` event, a module instance does not.

use crate::model::{body_text, host_of, meta_line, parse_body, parse_user_feeds, tile_line, Headline, NewsModel, FEEDS_KEY, MAX_BODY_BYTES};
use makepad_widgets::makepad_platform::storage::{StorageHandle, StorageRequestId, StorageResponse, StorageResult};
use makepad_widgets::*;

/// The most tabs the strip shows: All, the three built-in, four user feeds.
pub(crate) const MAX_TABS: usize = 8;
/// Below this height the tile drops its heading to keep three headlines.
/// Below roughly 70 pt (the 60 pt banner floor on the smallest Android homes
/// with five tile apps) the third headline clips; no shipping phone size
/// reaches it.
const SHORT_TILE: f64 = 96.0;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    let Body = Label{width: Fill padding: 0 draw_text +: {color: theme.color_text text_style: theme.font_regular{font_size: 12.75}}}
    // The meta role, not the disabled one: this text sits on the app
    // background, where disabled is nearly invisible on the dark theme.
    let Muted = Body{draw_text.color: theme.color_text_meta draw_text.text_style.font_size: 9.75}
    let Tab = RadioButtonTab{visible: false text: ""}

    let NewsRow = View{
        width: Fill height: Fit flow: Down
        cursor: MouseCursor.Hand
        content := View{width: Fill height: Fit flow: Down spacing: 4 padding: Inset{left: 16 right: 16 top: 10 bottom: 10}
            title := Body{max_lines: 2 text_overflow: Ellipsis}
            meta := Muted{max_lines: 1 text_overflow: Ellipsis}
            detail := View{visible: false width: Fill height: Fit flow: Down spacing: 8 margin: Inset{top: 4}
                summary := Body{max_lines: 8 draw_text.text_style.font_size: 11.25}
                View{width: Fill height: Fit flow: Right spacing: 12 align: Align{y: 0.5}
                    open := Button{text: "Open"}
                    host := Muted{width: Fill max_lines: 1 text_overflow: Ellipsis}
                }
            }
        }
        separator := View{width: Fill height: 0.5 show_bg: true draw_bg.color: theme.color_bevel_outset_2 margin: Inset{left: 16 right: 16}}
    }

    mod.widgets.NewsViewBase = #(NewsView::register_widget(vm))
    mod.widgets.NewsView = set_type_default() do mod.widgets.NewsViewBase{
        width: Fill height: Fill
        app_view := HostedView{
            full: View{width: Fill height: Fill flow: Down show_bg: true draw_bg.color: theme.color_bg_app
                bar := View{width: Fill height: Fit flow: Down spacing: 8 padding: Inset{left: 16 right: 16 top: 12 bottom: 8}
                    View{width: Fill height: Fit flow: Right spacing: 12 align: Align{y: 0.5}
                        Label{text: "News" padding: 0 draw_text +: {color: theme.color_text text_style: theme.font_bold{font_size: 20}}}
                        status := Muted{width: Fill max_lines: 1 text_overflow: Ellipsis}
                        refresh := Button{text: "Refresh"}
                    }
                    tabs := ScrollXView{width: Fill height: Fit flow: Right spacing: 4
                        tab_0 := Tab{} tab_1 := Tab{} tab_2 := Tab{} tab_3 := Tab{}
                        tab_4 := Tab{} tab_5 := Tab{} tab_6 := Tab{} tab_7 := Tab{}
                    }
                }
                error_line := Muted{visible: false width: Fill padding: Inset{left: 16 right: 16 bottom: 6} draw_text.color: theme.color_warning}
                list := PortalList{width: Fill height: Fill
                    Row := NewsRow{}
                }
                empty := View{visible: false width: Fill height: Fill align: Align{x: 0.5 y: 0.5}
                    empty_text := Muted{width: Fit text: "Loading…"}
                }
            }
            tile: View{width: Fill height: Fill flow: Down spacing: 4 show_bg: true draw_bg.color: theme.color_bg_app
                padding: Inset{left: 16 right: 16 top: 10 bottom: 10}
                tile_title := Muted{text: "NEWS" draw_text.text_style: theme.font_bold{font_size: 9.75}}
                tile_0 := Body{max_lines: 1 text_overflow: Ellipsis draw_text.text_style.font_size: 11.25}
                tile_1 := Body{max_lines: 1 text_overflow: Ellipsis draw_text.text_style.font_size: 11.25}
                tile_2 := Body{max_lines: 1 text_overflow: Ellipsis draw_text.text_style.font_size: 11.25}
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

    /// The timer stops and every fetch in flight is cancelled; nothing else
    /// this widget owns outlives its isolate.
    pub fn shutdown(&mut self, cx: &mut Cx) {
        if let Some(t) = self.tick.take() {
            cx.stop_timer(t);
        }
        for id in self.model.cancel_all() {
            cx.cancel_http_request(id);
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

    /// Hand the row's link to the system browser (a stub on Linux, Android
    /// and iOS at this revision; the row shows the host as text regardless).
    fn open_row(&self, cx: &mut Cx, row: usize) {
        if let Some(h) = self.shown.get(row) {
            cx.open_url(&h.link, OpenUrlInPlace::No);
        }
    }

    fn tab_ids() -> [&'static [LiveId]; MAX_TABS] {
        [ids!(tab_0), ids!(tab_1), ids!(tab_2), ids!(tab_3), ids!(tab_4), ids!(tab_5), ids!(tab_6), ids!(tab_7)]
    }

    /// Push the model into both faces.
    fn render(&mut self, cx: &mut Cx) {
        let tab = self.model.tab;
        for (i, id) in Self::tab_ids().iter().enumerate() {
            let shown = i < self.model.tab_count();
            self.view.widget(cx, id).set_visible(cx, shown);
            if shown {
                let button = self.view.radio_button(cx, id);
                button.set_text(self.model.tab_label(i));
                // Programmatic state, not a click: `select` would emit one.
                button.set_active(cx, i == tab, Animate::No);
            }
        }
        self.view.label(cx, ids!(status)).set_text(cx, &self.model.status_text(tab));
        let error = self.model.error_text(tab);
        self.view.widget(cx, ids!(error_line)).set_visible(cx, error.is_some());
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
        self.view.widget(cx, ids!(list)).set_visible(cx, !empty);
        self.view.widget(cx, ids!(empty)).set_visible(cx, empty);
        let placeholder = if self.model.loading(tab) { "Loading…" } else { "No headlines" };
        self.view.label(cx, ids!(empty_text)).set_text(cx, placeholder);

        let tile = self.model.tile_rows();
        for (i, id) in [ids!(tile_0), ids!(tile_1), ids!(tile_2)].iter().enumerate() {
            let text = tile.get(i).map(tile_line).unwrap_or_else(|| if i == 0 { self.model.status_text(0) } else { String::new() });
            self.view.label(cx, *id).set_text(cx, &text);
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
            item.label(cx, ids!(title)).set_text(cx, &row.title);
            item.label(cx, ids!(meta)).set_text(cx, &meta_line(row, now));
            item.widget(cx, ids!(detail)).set_visible(cx, expanded);
            if expanded {
                let summary = if row.summary.is_empty() { "No summary from this source." } else { row.summary.as_str() };
                item.label(cx, ids!(summary)).set_text(cx, summary);
                item.label(cx, ids!(host)).set_text(cx, &host_of(&row.link));
            }
            item.draw_all(cx, &mut Scope::empty());
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Widget for NewsView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.ensure_started(cx);
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
            if self.view.button(cx, ids!(refresh)).clicked(actions) {
                self.fetch_tab(cx, self.model.tab, true);
                self.render(cx);
            }
            for (i, id) in Self::tab_ids().iter().enumerate() {
                if self.view.radio_button(cx, id).clicked(actions) {
                    self.select_tab(cx, i);
                }
            }
            let list = self.view.portal_list(cx, ids!(list));
            let mut opened = None;
            let mut toggled = None;
            for (index, item) in list.items_with_actions(actions) {
                if item.button(cx, ids!(open)).clicked(actions) {
                    opened = Some(index);
                } else if item.as_view().finger_up(actions).is_some_and(|up| up.was_tap()) {
                    // A tap, not the end of a scroll drag over the row.
                    toggled = Some(index);
                }
            }
            if let Some(index) = opened {
                self.open_row(cx, index);
            }
            // Beyond `shown`: a row removed by a refresh in the same frame.
            if let Some(link) = toggled.and_then(|index| self.shown.get(index)).map(|row| row.link.clone()) {
                self.model.toggle_expanded(&link);
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
