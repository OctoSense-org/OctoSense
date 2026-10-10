//! The launcher uses the same editor as applications: caret, selection,
//! clipboard and native IME stay with TextInput. Only its soft keys are hosted.
use super::*;

#[derive(Clone, Default)]
struct SearchIcons(std::sync::Arc<std::collections::HashMap<String, Texture>>);
impl PartialEq for SearchIcons {
    fn eq(&self, other: &Self) -> bool { std::sync::Arc::ptr_eq(&self.0, &other.0) }
}

#[derive(Clone, PartialEq)]
struct SearchSnapshotKey {
    screen: Rect,
    window: Vec2d,
    dpi: f64,
    style: DesktopStyle,
    dark: bool,
    font_scale: f64,
    catalog: u64,
    visuals: u64,
    icons: SearchIcons,
    query: String,
    scroll: f64,
}

impl SearchSnapshotKey {
    fn new(cx: &Cx2d, state: &WmState, screen: Rect) -> Self {
        use std::hash::{Hash, Hasher};
        // Icons arrive asynchronously and notification dots can change
        // without a catalog update. Neither may revive an older capture.
        let mut visuals = std::collections::hash_map::DefaultHasher::new();
        for note in &state.phone.shade.notifications { note.app.hash(&mut visuals); }
        Self { screen, window: cx.cx.windows[CxWindowPool::id_zero()].window_geom.inner_size,
            dpi: cx.current_dpi_factor(), style: state.style.target, dark: state.style.dark,
            font_scale: state.phone.android.font_scale, catalog: state.phone.android.catalog_revision,
            visuals: visuals.finish(),
            // Holding the Arc forces the icon loader's make_mut to replace
            // the map, so pointer equality also detects in-place updates.
            icons: SearchIcons(state.phone.android.icons.clone()),
            query: state.phone.search_query.clone(), scroll: state.phone.search_scroll }
    }
    fn matches_dismissal(&self, current: &Self) -> bool {
        // KeyboardView grows the body as the IME hides. Retain the outgoing
        // sheet's original height, but never reuse it after a window resize.
        let mut key = self.clone();
        key.screen.size.y = current.screen.size.y;
        key == *current
    }
}

/// Prepare an outgoing search sheet only after its content has stayed still.
/// Typing and scrolling draw live; closing can then fade one texture while
/// Android animates the keyboard instead of redrawing every result beneath it.
#[derive(Default)]
pub(super) struct SearchSnapshot {
    frame: Option<crate::dock_warp::WindowFrame>,
    ready: Option<SearchSnapshotKey>,
    pending: Option<(SearchSnapshotKey, std::time::Instant)>,
    field_top: f64,
}
impl SearchSnapshot {
    pub(super) fn invalidate(&mut self) { self.ready = None; self.pending = None; }
}

/// Lowercase with the common Latin accents folded, so "cafe" finds "Café".
fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
            'ñ' | 'ń' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'ū' => 'u',
            'ý' | 'ÿ' => 'y',
            'ś' | 'š' => 's',
            'ź' | 'ż' | 'ž' => 'z',
            other => other,
        })
        .collect()
}

/// Where a name's words start, as iOS's app search sees them: after a space
/// or punctuation, and at a capital inside a word ("YouTube" starts "tube").
fn word_starts(label: &str) -> Vec<usize> {
    let chars: Vec<char> = label.chars().collect();
    let mut starts = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() { continue; }
        let start = match i.checked_sub(1).map(|j| chars[j]) {
            None => true,
            Some(prev) if !prev.is_alphanumeric() => true,
            Some(prev) => c.is_uppercase() && prev.is_lowercase(),
        };
        if start { starts.push(i); }
    }
    starts
}

/// How well `label` answers `query`: `Some(0)` when the name starts with
/// it, `Some(1)` when a later word does, `None` when it does not match.
/// Like iOS, matching is by word prefix, never from the middle of a word,
/// and every word of a multi-word query must start some word of the name.
fn rank(label: &str, query: &str) -> Option<u8> {
    let chars: Vec<char> = label.chars().collect();
    let folded: Vec<String> = word_starts(label)
        .into_iter()
        .map(|i| fold(&chars[i..].iter().collect::<String>()))
        .collect();
    let whole = fold(label);
    let query = fold(query);
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.is_empty() { return None; }
    if whole.starts_with(query.trim()) { return Some(0); }
    words
        .iter()
        .all(|word| folded.iter().any(|tail| tail.starts_with(word)))
        .then_some(1)
}

/// The catalog positions of the apps matching `query`, best first: names
/// that start with the query, then names with a word that does, each
/// alphabetically. An empty query matches nothing, as on iOS.
fn matching_indices(apps: &[(String, String)], query: &str) -> Vec<usize> {
    let mut found: Vec<(u8, String, usize)> = apps
        .iter()
        .enumerate()
        .filter_map(|(index, (_, label))| rank(label, query).map(|r| (r, fold(label), index)))
        .collect();
    found.sort();
    found.into_iter().map(|(_, _, index)| index).collect()
}
fn matching_apps<'a>(apps: &'a [(String, String)], query: &str) -> Vec<&'a (String, String)> {
    matching_indices(apps, query).into_iter().map(|index| &apps[index]).collect()
}
/// The indices into the catalog that match a query, kept for as long as
/// the query and the catalog stay the same.
#[derive(Default)]
pub struct SearchResults {
    key: Option<(String, u64)>,
    pub found: Vec<usize>,
}
impl SearchResults {
    /// The matches for `query` in `apps`, recomputed only when either changed.
    pub fn update(&mut self, apps: &[(String, String)], query: &str) -> &[usize] {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        apps.hash(&mut hasher);
        let key = (query, hasher.finish());
        if self.key.as_ref().is_none_or(|(q, h)| q != key.0 || *h != key.1) {
            self.found = matching_indices(apps, query);
            self.key = Some((query.to_string(), key.1));
        }
        &self.found
    }
}
/// The result Return launches: the best match for the query, if any.
pub fn top_hit(apps: &[(String, String)], query: &str) -> Option<String> {
    if query.trim().is_empty() { return None; }
    matching_apps(apps, query).first().map(|(id, _)| id.clone())
}

/// The height of one search result row.
const ROW: f64 = 56.0;

impl PhoneSurface {
    pub(super) fn present_search_dismissal(&mut self, cx: &mut Cx2d, state: &WmState, screen: Rect) -> bool {
        if !state.phone.search_closing { return false; }
        let key = SearchSnapshotKey::new(cx, state, screen);
        let Some(ready) = self.search_snapshot.ready.as_ref().filter(|old| old.matches_dismissal(&key)) else { return false; };
        let Some(frame) = self.search_snapshot.frame.as_ref() else { return false; };
        let p = state.phone.search_reveal.clamp(0.0, 1.0) as f32;
        let ground = self.theme_ground(if state.style.dark {rgb(24,22,31)}else{rgb(249,245,255)});
        frame.attach(cx);
        self.android_icon.draw_vars.set_texture(0, frame.texture());
        self.android_icon.opacity = p;
        // The results retain the finger's elastic offset; the field follows
        // the disappearing IME. Moving the whole snapshot as one sheet would
        // snap stretched rows back to rest at the start of dismissal.
        let top = ready.screen.pos.y + if ready.screen.size.x > ready.screen.size.y {24.0} else {42.0};
        let split = self.search_snapshot.field_top;
        let field_height = ready.screen.pos.y + ready.screen.size.y - split;
        let field_y = screen.pos.y + screen.size.y - field_height + 64.0 * (1.0 - p as f64);
        let results_height = (split - top).max(0.0);
        // The capture already contains the opaque ground. Fill only the
        // gaps around its two slices, so fading does not blend it twice.
        let bottom = screen.pos.y + screen.size.y;
        let mut fill_y = screen.pos.y;
        for (start, end) in [
            ((top + state.phone.search_stretch).max(top), (top + state.phone.search_stretch + results_height).min(field_y)),
            (field_y, field_y + field_height),
        ] {
            let (start, end) = (start.clamp(screen.pos.y, bottom), end.clamp(screen.pos.y, bottom));
            if end <= start { continue; }
            if start > fill_y { self.d.solid(cx, rect(screen.pos.x, fill_y, screen.size.x, start-fill_y), alpha(ground, p)); }
            fill_y = fill_y.max(end);
        }
        if fill_y < bottom { self.d.solid(cx, rect(screen.pos.x, fill_y, screen.size.x, bottom-fill_y), alpha(ground, p)); }
        if results_height > 0.0 {
            cx.begin_turtle(Walk::abs_rect(rect(screen.pos.x, top, screen.size.x, (field_y-top).max(0.0))), Layout::default());
            self.android_icon.image_scale = vec2(1.0, (results_height / ready.screen.size.y) as f32);
            self.android_icon.image_pan = vec2(0.0, ((top - ready.screen.pos.y) / ready.screen.size.y) as f32);
            self.android_icon.draw_abs(cx, rect(screen.pos.x, top + state.phone.search_stretch, screen.size.x, results_height));
            cx.end_turtle();
        }
        self.android_icon.image_scale = vec2(1.0, (field_height / ready.screen.size.y) as f32);
        self.android_icon.image_pan = vec2(0.0, ((split - ready.screen.pos.y) / ready.screen.size.y) as f32);
        self.android_icon.draw_abs(cx, rect(screen.pos.x, field_y, screen.size.x, field_height));
        self.android_icon.image_scale = vec2(1.0, 1.0);
        self.android_icon.image_pan = vec2(0.0, 0.0);
        self.hits.clear();
        self.search_rect = Rect::default();
        if crate::mobile_perf::trace_on() { log!("[phone.search] cached_dismissal"); }
        true
    }

    pub(super) fn begin_search_snapshot(&mut self, cx: &mut Cx2d, state: &WmState, screen: Rect) -> bool {
        if state.phone.search_closing { return false; }
        let key = SearchSnapshotKey::new(cx, state, screen);
        let cache = &mut self.search_snapshot;
        if cache.pending.as_ref().is_none_or(|(old, _)| *old != key) {
            cache.pending = Some((key.clone(), std::time::Instant::now()));
        }
        if cache.ready.as_ref() == Some(&key) || state.phone.search_reveal != 1.0
            || state.phone.search_reveal_velocity != 0.0 || state.phone.gesture.is_some()
            || state.phone.search_stretch != 0.0 || state.phone.search_stretch_velocity != 0.0
            || state.phone.search_velocity != 0.0
            || cache.pending.as_ref().unwrap().1.elapsed().as_secs_f64() < 0.35 {
            return false;
        }
        cache.ready = Some(key);
        cache.frame.get_or_insert_with(|| crate::dock_warp::WindowFrame::new_with_name(cx, "phone_search_dismissal"))
            .begin(cx, screen);
        true
    }

    pub(super) fn end_search_snapshot(&mut self, cx: &mut Cx2d, screen: Rect, field_top: f64, recording: bool) {
        if !recording { return; }
        self.search_snapshot.field_top = field_top;
        let frame = self.search_snapshot.frame.as_mut().unwrap();
        frame.end(cx);
        if crate::mobile_perf::trace_on() { log!("[phone.search] recorded_dismissal"); }
        self.android_icon.draw_vars.set_texture(0, frame.texture());
        self.android_icon.opacity = 1.0;
        self.android_icon.draw_abs(cx, screen);
    }

    /// Search results use 16 px labels, a different raster size from Home
    /// and the editor. Prepare a bounded set of catalog glyphs while Home is
    /// quiet so the first query/scroll does not build them in its draw call.
    pub(super) fn prewarm_search_labels(&mut self, cx: &mut Cx2d, state: &WmState, hidden: Rect, step: usize) -> bool {
        let mut seen = std::collections::HashSet::new();
        let labels: Vec<&str> = state.phone.android.rows.iter().map(|(_, label)| label.as_str()).collect();
        let mut chars = Vec::new();
        for c in labels.iter().flat_map(|s| s.chars()).filter(|c| !c.is_whitespace()) {
            if seen.insert(c) {
                chars.push(c);
                if seen.len() == 96 { break; }
            }
        }
        let mut glyphs = String::new();
        for c in chars.iter().skip(step * 8).take(8) {
            glyphs.push(*c);
            glyphs.push(' '); // Do not turn adjacent catalog characters into ligatures.
        }
        self.d.label(cx, hidden, false, 16.0, vec4(0.0,0.0,0.0,0.0), HAlign::Left, &glyphs);
        (step + 1) * 8 < chars.len()
    }

    pub fn dismiss_search(&mut self, cx: &mut Cx, phone: &mut PhoneState, clear: bool) {
        if clear { self.search_snapshot.invalidate(); }
        let input = self.search.text_input(cx, ids!(input));
        if !input.area().is_empty() && cx.has_key_focus(input.area()) {
            cx.set_key_focus(Area::Empty);
            cx.hide_text_ime();
        }
        phone.search_focused = false;
        phone.search_focus_requested = false;
        self.search_focus_pending = false;
        self.search_pointer = false;
        self.search_press = None;
        if clear && !phone.search_query.is_empty() {
            input.set_text(cx, "");
            phone.search_query.clear();
            phone.search_scroll = 0.0;
        }
    }

    pub fn clear_search(&mut self, cx: &mut Cx, phone: &mut PhoneState) {
        self.search_snapshot.invalidate();
        let input = self.search.text_input(cx, ids!(input));
        input.set_text(cx, "");
        phone.search_query.clear();
        phone.search_scroll = 0.0;
        phone.search_focus_requested = true;
    }

    pub fn focus_search(&mut self, cx: &mut Cx, phone: &mut PhoneState) {
        self.search_snapshot.invalidate();
        // A pull-down can be the editor's first appearance. Take focus again
        // after its first draw, when it has a real area for the native IME.
        phone.search_focus_requested = false;
        self.search_focus_pending = true;
        self.search.text_input(cx, ids!(input)).take_key_focus(cx);
        phone.search_focused = true;
        phone.search_focus_at = crate::host::now();
    }

    /// Pointer capture stays with the editor for selection drags. Soft-key
    /// presses never reach its outside-click handler and therefore keep focus.
    pub fn search_event(
        &mut self,
        cx: &mut Cx,
        event: &Event,
        phone: &mut PhoneState,
        enabled: bool,
    ) -> bool {
        let input = self.search.text_input(cx, ids!(input));
        if !enabled {
            self.dismiss_search(cx, phone, !phone.searching());
            if matches!(
                event,
                Event::KeyFocus(_) | Event::KeyFocusLost(_) | Event::Timer(_) | Event::NextFrame(_)
            ) {
                self.search.handle_event(cx, event, &mut Scope::empty());
            }
            return false;
        }
        let pointer = match event {
            Event::MouseDown(e) => Some((e.abs, e.time, true, false)),
            Event::MouseMove(e) => Some((e.abs, e.time, false, false)),
            Event::MouseUp(e) => Some((e.abs, e.time, false, true)),
            Event::TouchUpdate(e) => e.touches.first().map(|t| {
                (
                    t.abs,
                    t.time,
                    t.state == makepad_platform::event::TouchState::Start,
                    t.state == makepad_platform::event::TouchState::Stop,
                )
            }),
            _ => None,
        };
        let mut consumed = false;
        if let Some((point, time, down, up)) = pointer {
            let inside = self.search_rect.contains(point) && self.hit(point).is_none();
            if down && inside {
                // Cursor/selection edits can change the image without
                // changing the query. A fresh capture must follow them.
                self.search_snapshot.invalidate();
                self.search_pointer = true;
                self.search_press = Some((point, time));
            }
            // A pull down from the field is a pull on the list (mobile.rs):
            // let it go, and the shell takes it from where it landed.
            if let (true, Some((start, at))) = (self.search_pointer && !down && !up, self.search_press) {
                if crate::mobile::search_field_releases(start, point, phone.search_scroll) {
                    self.search_pointer = false;
                    self.search_press = None;
                    phone.search_field_pull = Some((start, at));
                    return false;
                }
            }
            consumed = inside || self.search_pointer;
            if up {
                self.search_pointer = false;
                self.search_press = None;
            }
            if !consumed {
                return false;
            }
        }
        let focused = cx.has_key_focus(input.area());
        if focused {
            if matches!(event, Event::KeyDown(_) | Event::TextInput(_) | Event::TextCut(_)) {
                self.search_snapshot.invalidate();
            }
            consumed |= matches!(
                event,
                Event::KeyDown(_)
                    | Event::KeyUp(_)
                    | Event::TextInput(_)
                    | Event::TextCopy(_)
                    | Event::TextCut(_)
            );
        }
        let actions =
            cx.capture_actions(|cx| self.search.handle_event(cx, event, &mut Scope::empty()));
        if input.returned(&actions).is_some() || input.escaped(&actions) {
            // Return opens the best match; Escape only closes the field.
            if input.returned(&actions).is_some() {
                let apps: Vec<(String, String)> = if phone.android.rows.is_empty() {
                    crate::shell::launcher::apps().iter().map(|a| (a.id.trim_start_matches("apps.").to_string(), a.label.clone())).collect()
                } else { phone.android.rows.to_vec() };
                phone.search_launch = top_hit(&apps, &input.text());
            }
            self.dismiss_search(cx, phone, input.escaped(&actions));
        } else {
            phone.search_focused = cx.has_key_focus(input.area());
        }
        let query = input.text();
        if phone.search_query != query {
            phone.search_query = query;
            phone.search_scroll = 0.0;
            phone.search_velocity = 0.0;
            phone.search_stretch = 0.0;
            phone.search_stretch_velocity = 0.0;
        }
        consumed
    }

    pub(super) fn draw_search(
        &mut self,
        cx: &mut Cx2d,
        state: &WmState,
        screen: Rect,
        ink: Vec4f,
    ) -> Rect {
        let ios = state.style.target == DesktopStyle::Ios;
        let editing = state.phone.searching() || state.phone.search_reveal > 0.0;
        // At the bottom, just above the keyboard, as in iOS's search: the
        // field sits where the thumb already is. A native IME has reflowed
        // the viewport above itself; only the shell's own soft keyboard (a
        // desktop preview) is drawn over it and taken off here.
        let bottom = screen.pos.y + screen.size.y
            - state.phone.keyboard.max(state.phone.keyboard_target)
            - state.phone.search_keyboard_lift(crate::host::now())
            - if editing { 10.0 } else { 34.0 };
        let pill = rect(
            screen.pos.x + 20.0,
            bottom - 40.0 + (1.0-state.phone.search_reveal.clamp(0.0,1.0))*64.0,
            screen.size.x - 40.0 - if editing { 64.0 } else { 0.0 },
            40.0,
        );
        self.search_rect = pill;
        if self.search_style != Some((ios, state.style.dark)) {
            let timing = crate::mobile_perf::work_start();
            let mut input = self.search.text_input(cx, ids!(input));
            let muted = alpha(ink, 0.55);
            if ios {
                script_apply_eval!(cx,input,{draw_text.text_style: mod.widgets.PhoneSurface.ios_font{font_size: 14.0}});
            } else {
                script_apply_eval!(cx,input,{draw_text.text_style: mod.widgets.PhoneSurface.android_font{font_size: 14.0}});
            }
            script_apply_eval!(cx, input, {
                draw_text +: {
                    color: #(ink) color_hover: #(ink) color_focus: #(ink) color_down: #(ink)
                    color_empty: #(muted)
                }
            });
            input.set_empty_text(cx, if ios { "Search" } else { "Search apps" }.into());
            self.search_style = Some((ios, state.style.dark));
            crate::mobile_perf::work_end("search.style", timing);
        }
        let accent = self.theme_accent(if ios {
            rgb(0, 122, 255)
        } else {
            rgb(126, 94, 190)
        });
        if state.phone.search_focused {
            self.rounded(cx, pill, 14.0, alpha(accent, 0.65));
            self.rounded(
                cx,
                rect(
                    pill.pos.x + 1.5,
                    pill.pos.y + 1.5,
                    pill.size.x - 3.0,
                    pill.size.y - 3.0,
                ),
                12.5,
                self.theme_face(if state.style.dark {
                    rgb(40, 40, 48)
                } else {
                    rgb(238, 238, 245)
                }),
            );
        } else {
            self.rounded(cx, pill, 14.0, alpha(ink, 0.10));
        }
        self.d.icon_centered(
            cx,
            Ico::Search,
            rect(pill.pos.x + 8.0, pill.pos.y, 28.0, pill.size.y),
            15.0,
            alpha(ink, 0.55),
        );
        let timing = crate::mobile_perf::work_start();
        self.search
            .draw_walk_all(cx, &mut Scope::empty(), Walk::abs_rect(pill));
        crate::mobile_perf::work_end("search.editor", timing);
        if self.search_focus_pending {
            self.search.text_input(cx, ids!(input)).take_key_focus(cx);
            self.search_focus_pending = false;
        }
        if !state.phone.search_query.is_empty() {
            let clear = rect(pill.pos.x + pill.size.x - 32.0, pill.pos.y, 32.0, 40.0);
            self.label(cx, clear, "×", 20.0, false, alpha(ink, 0.6));
            self.hits.push((clear, PhoneHit::ClearSearch));
        }
        if editing {
            let cancel = rect(pill.pos.x + pill.size.x + 4.0, pill.pos.y, 60.0, 40.0);
            self.label(cx, cancel, "Cancel", 13.0, false, accent);
            self.hits.push((cancel, PhoneHit::CancelSearch));
        }
        pill
    }

    pub(super) fn draw_search_results(
        &mut self,
        cx: &mut Cx2d,
        state: &WmState,
        screen: Rect,
        pill: Rect,
        apps: &[(String, String)],
        ink: Vec4f,
    ) {
        let timing = crate::mobile_perf::work_start();
        let matching = crate::mobile_perf::work_start();
        let mut results = std::mem::take(&mut self.search_found);
        let count = results.update(apps, &state.phone.search_query).len();
        crate::mobile_perf::work_end("search.matching", matching);
        // The results fill the space between the status bar and the field.
        let top = screen.pos.y + if screen.size.x > screen.size.y { 24.0 } else { 42.0 };
        let bottom = pill.pos.y - 10.0;
        let height = (bottom - top).max(0.0);
        if count == 0 {
            self.search_found = results;
            self.search_scroll_max = 0.0;
            // Nothing typed yet: nothing listed, as on iOS.
            if !state.phone.search_query.trim().is_empty() {
                self.label(
                    cx,
                    rect(screen.pos.x + 20.0, bottom - 54.0, screen.size.x - 40.0, 30.0),
                    "No Results",
                    15.0,
                    false,
                    alpha(ink, 0.6),
                );
            }
            return;
        }
        self.search_scroll_max = (count as f64 * ROW - height).max(0.0);
        // The stretch past either end moves the rows with the finger too.
        let scroll = state.phone.search_scroll.clamp(0.0, self.search_scroll_max) - state.phone.search_stretch;
        cx.begin_turtle(
            Walk::abs_rect(rect(screen.pos.x, top, screen.size.x, height)),
            Layout::default(),
        );
        // Only the rows on screen are laid out and drawn.
        let first = (scroll / ROW).floor().max(0.0) as usize;
        let last = (((scroll + height) / ROW).ceil().max(0.0) as usize).min(count);
        let mut rows = Vec::with_capacity(last.saturating_sub(first));
        for index in first..last {
            let (id, label) = &apps[results.found[index]];
            let y = top + index as f64 * ROW - scroll;
            if y + ROW <= top || y >= bottom {
                continue;
            }
            let row = rect(screen.pos.x + 20.0, y, screen.size.x - 40.0, 56.0);
            rows.push((id, label, row));
        }
        // Icons, labels and separators occupy disjoint rectangles. Submit
        // each kind together so consecutive labels/fills share draw calls;
        // alternating all three per row fragments the same atlas into one
        // GL submission per label during scrolling and IME dismissal.
        for (id, _, row) in &rows {
            let icon_timing = crate::mobile_perf::work_start();
            self.draw_launcher_icon(
                cx,
                state,
                id,
                rect(row.pos.x + 4.0, row.pos.y + 6.0, 44.0, 44.0),
                ink,
                1.0,
            );
            if icon_timing.is_some() {
                crate::mobile_perf::work_end(&format!("search.icon.{id}"), icon_timing);
            }
        }
        for (_, label, row) in &rows {
            let label_timing = crate::mobile_perf::work_start();
            self.d.label_elided(
                cx,
                rect(row.pos.x + 64.0, row.pos.y, row.size.x - 64.0, 56.0),
                false,
                16.0,
                ink,
                HAlign::Left,
                label,
            );
            crate::mobile_perf::work_end("search.label", label_timing);
        }
        for (id, _, row) in rows {
            self.d.solid(
                cx,
                rect(row.pos.x + 64.0, row.pos.y + 55.0, row.size.x - 64.0, 0.5),
                alpha(ink, 0.12),
            );
            let y0 = row.pos.y.max(top);
            self.hits.push((
                rect(row.pos.x, y0, row.size.x, (row.pos.y + 56.0).min(bottom) - y0),
                PhoneHit::App(id.clone()),
            ));
        }
        cx.end_turtle();
        self.search_found = results;
        crate::mobile_perf::work_end("search.results", timing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dismissal_snapshot_accepts_ime_reflow_but_rejects_stale_content_and_geometry() {
        let key = SearchSnapshotKey {
            screen: rect(0.0,24.0,384.0,440.0), window: dvec2(384.0,800.0), dpi: 2.8125,
            style: DesktopStyle::Android, dark: false, font_scale: 1.0, catalog: 1, visuals: 1, icons: SearchIcons::default(),
            query: "octo".into(), scroll: 0.0,
        };
        let mut hidden_ime = key.clone();
        hidden_ime.screen.size.y = 740.0;
        assert!(key.matches_dismissal(&hidden_ime));
        let mut stale = hidden_ime.clone(); stale.query.push('s');
        assert!(!key.matches_dismissal(&stale));
        let mut stale = hidden_ime.clone(); stale.scroll = 56.0;
        assert!(!key.matches_dismissal(&stale));
        let mut stale = hidden_ime.clone(); stale.catalog += 1;
        assert!(!key.matches_dismissal(&stale));
        let mut stale = hidden_ime.clone(); stale.visuals += 1;
        assert!(!key.matches_dismissal(&stale));
        let mut stale = hidden_ime.clone(); stale.icons = SearchIcons::default();
        assert!(!key.matches_dismissal(&stale));
        let mut stale = hidden_ime.clone(); stale.dark = true;
        assert!(!key.matches_dismissal(&stale));
        let mut stale = hidden_ime.clone(); stale.font_scale = 1.5;
        assert!(!key.matches_dismissal(&stale));
        let mut resized = hidden_ime.clone(); resized.window.y = 700.0;
        assert!(!key.matches_dismissal(&resized));
        let mut rotated = hidden_ime; rotated.screen.size.x = 740.0;
        assert!(!key.matches_dismissal(&rotated));
        let mut cache = SearchSnapshot { ready: Some(key), ..Default::default() };
        cache.invalidate();
        assert!(cache.ready.is_none(), "a palette change retires the old image");
    }
    #[test]
    fn search_matches_word_starts_like_ios() {
        let apps: Vec<(String, String)> = [
            ("task", "Task Manager"),
            ("files", "Files"),
            ("photos", "Photos"),
            ("youtube", "YouTube"),
            ("mail", "Mail"),
            ("maps", "Maps"),
            ("hub", "App Hub"),
            ("cafe", "Café Menu"),
        ]
        .into_iter()
        .map(|(id, label)| (id.to_string(), label.to_string()))
        .collect();
        let ids = |q: &str| matching_apps(&apps, q).iter().map(|a| a.0.as_str()).collect::<Vec<_>>();
        assert!(ids("").is_empty(), "nothing typed lists nothing");
        assert!(ids("   ").is_empty());
        assert_eq!(ids("m"), ["mail", "maps", "cafe", "task"], "each keystroke: name starts first, then later words");
        assert_eq!(ids("ma"), ["mail", "maps", "task"]);
        assert_eq!(ids("me"), ["cafe"]);
        assert_eq!(ids("map"), ["maps"]);
        assert_eq!(ids("TASK man"), ["task"]);
        assert_eq!(ids("man"), ["task"], "a later word of the name");
        assert_eq!(ids("tube"), ["youtube"], "a capital inside a word starts a word");
        assert!(ids("ube").is_empty(), "never from the middle of a word");
        assert!(ids("hotos").is_empty());
        assert_eq!(ids("hub"), ["hub"]);
        assert_eq!(ids("cafe"), ["cafe"], "accents fold");
        assert!(ids("task photos").is_empty());
        assert!(ids("youtube").len() == 1 && ids("you").len() == 1);
    }
    #[test]
    fn results_are_kept_per_query_and_catalog() {
        let mut apps: Vec<(String, String)> = vec![("files".into(), "Files".into()), ("photos".into(), "Photos".into())];
        let mut results = SearchResults::default();
        assert_eq!(results.update(&apps, "p"), [1]);
        results.found.push(9);
        assert_eq!(results.update(&apps, "p"), [1, 9], "the same query and catalog are not matched again");
        assert_eq!(results.update(&apps, "f"), [0]);
        apps.push(("fun".into(), "Fun".into()));
        assert_eq!(results.update(&apps, "f"), [0, 2], "a changed catalog is");
    }
}
