//! A published glance card on screen: a `Splash` isolate at the glance tile
//! size, running either an L0 card lowered the way App Hub's Card runner
//! lowers `page.card` (`octoscript_makepad::l0::prepare`, then
//! `design::to_makepad_ui`) or a `script` card as it was published.
//!
//! **Lowering.** A card bundle carries its kit in `kit/`; a glance card is
//! only a source and its data, so the host supplies the kit: the L0 kit
//! (palettes, derivations, `_kit.octoscript`) is compiled in from the pinned
//! Octoscript-Makepad checkout and assembled in memory in the Card runner's
//! order (base palette, the card's mood, derivations, kit, the lowered card).
//! Theme axes other than their identity values and native kit packs are not
//! offered on a tile: a card naming one is refused at publish, not drawn in
//! some other look.
//!
//! **Tile size.** Width: the glance column (the phone's screen minus 40 pt,
//! the desktop panel's 328 pt). Height: the card's own measured height,
//! clamped to [`TILE_MIN_HEIGHT`]..=[`TILE_MAX_HEIGHT`]; until the first
//! draw measures it, [`TILE_DEFAULT_HEIGHT`]. A taller card is clipped at the
//! cap; the app is one tap away. A script card should size its root `Fit`.
//!
//! **Policy.** A tile's isolate runs under the publishing app's resolved
//! policy, applied exactly as the Card runner applies it
//! (`octosense_app_policy::splash_adapter::apply` with the app's
//! `isolate_settings`: its jail and quota, capabilities, hosts, prompt right,
//! budget and heap), so a card can do whatever the app's own UI can. A
//! native module (no manifest) publishes tiles with no capabilities and no
//! hosts, as does an app whose policy cannot be resolved (logged).
//!
//! **Input.** A tile is interactive: the surface hands it every event
//! ([`GlanceTiles::handle_event`]), so taps, typing, focus and scrolling
//! reach the card's widgets and its handlers run. Its `host.request` calls
//! leave through the Card runner's own path (`octosense_appstore::services::
//! pump`: the isolate's capability gate, then the host services) as that
//! app, the way the app's own UI calls go out. A service sheet a tile's call
//! raises is not shown on the tile. The shell keeps one affordance of its
//! own on each tile, the open button at its top-right corner
//! ([`open_button`]), which opens the app.
//!
//! **L0 taps.** A lowered L0 card's taps and field edits call `NAV(t:
//! "l0:{e,k,v}", v?)` (Octoscript-Makepad's general translation). Every
//! isolate gets a host `NAV` ([`install_nav`]) that queues the call with the
//! isolate's heap key; nothing else reads it. The card window
//! (glance_sheet.rs) takes its own isolate's calls ([`take_taps`]) and runs
//! them through an [`L0Session`]: the declared transition (`octoscript_ui_l0`
//! dispatch), the §5.12 writes the host performs, and a re-lowering. A
//! glance-panel tile does not dispatch: its L0 taps stay inert.
use makepad_widgets::*;
use std::cell::RefCell;
use std::collections::HashMap;

pub const TILE_MIN_HEIGHT: f64 = 72.0;
pub const TILE_MAX_HEIGHT: f64 = 260.0;
pub const TILE_DEFAULT_HEIGHT: f64 = 148.0;
/// Script instructions a tile with no grants (a native module's) may run
/// over its life; an app's tile has its policy's budget.
const TILE_INSTRUCTION_BUDGET: u64 = 5_000_000;
/// The side of the open button in a tile's top-right corner.
pub const OPEN_BUTTON: f64 = 28.0;

/// Where a tile at `rect` has its open button.
pub fn open_button(rect: Rect) -> Rect {
    Rect { pos: dvec2(rect.pos.x + rect.size.x - OPEN_BUTTON - 6.0, rect.pos.y + 6.0), size: dvec2(OPEN_BUTTON, OPEN_BUTTON) }
}

macro_rules! l0_kit {
    ($name:literal) => {
        include_str!(concat!(env!("OCTOSENSE_WORKSPACE"), "/octoscript-makepad/components/l0/", $name))
    };
}

const PALETTE_BASE: &str = l0_kit!("_palette_dark.octoscript");
const DERIVE_COLOR: &str = l0_kit!("_derive_color.octoscript");
const DERIVE: &str = l0_kit!("_derive.octoscript");
const KIT: &str = l0_kit!("_kit.octoscript");
/// The mood deltas over the dark base (`octoscript_ui_l0::catalog::THEMES`).
const MOODS: &[(&str, &str)] = &[
    ("dark", ""),
    ("light", l0_kit!("_palette_light.octoscript")),
    ("glass", l0_kit!("_palette_glass.octoscript")),
    ("photo", l0_kit!("_palette_photo.octoscript")),
    ("vibrant", l0_kit!("_palette_vibrant.octoscript")),
    ("minimal", l0_kit!("_palette_minimal.octoscript")),
    ("atro", l0_kit!("_palette_atro.octoscript")),
    ("atro_light", l0_kit!("_palette_atro_light.octoscript")),
    ("camo", l0_kit!("_palette_camo.octoscript")),
    ("camo_light", l0_kit!("_palette_camo_light.octoscript")),
    ("taskplan_light", l0_kit!("_palette_taskplan_light.octoscript")),
];

/// Lower an admitted L0 card with its data to the Splash body a tile draws:
/// realize (the no-facts rule: every value from `data`), assemble with the
/// kit, evaluate the checked design VM, translate to Makepad UI.
pub fn lower(source: &str, data: &serde_json::Value) -> Result<String, String> {
    lower_report(source, octoscript_ui_l0::realize(source, data, Default::default()), false)
}

/// [`lower`], realized against a card's local state (its `InstanceStore`),
/// for the card window: through Octoscript-Makepad's L0 translation, which
/// keeps the kit's resolved sizes and its chips' labels (the general
/// translation [`lower`] uses re-lowers chips the Material way and drops
/// their text).
pub fn lower_with_state(source: &str, data: &serde_json::Value, store: &octoscript_ui_l0::InstanceStore) -> Result<String, String> {
    lower_report(source, octoscript_ui_l0::realize_with_state(source, data, store, Default::default()), true)
}

fn lower_report(source: &str, report: octoscript_ui_l0::RealizeReport, l0_ui: bool) -> Result<String, String> {
    let root = report.complete_root()?;
    if octoscript_ui_l0::kit_pack::contains(root) {
        return Err("native kit components are not offered on a glance tile".into());
    }
    let mood = octoscript_ui_l0::card_theme(source).unwrap_or_else(|| "dark".into());
    let delta = MOODS.iter().find(|(name, _)| *name == mood).map(|(_, d)| *d).ok_or_else(|| format!("theme {mood:?} is not offered on a glance tile"))?;
    for (axis, value) in octoscript_ui_l0::card_theme_axes(source) {
        if !matches!(value.as_str(), "neutral" | "regular" | "none" | "soft" | "sans") {
            return Err(format!("theme axis {axis}: .{value} is not offered on a glance tile"));
        }
    }
    let kit_source = [PALETTE_BASE, delta, DERIVE_COLOR, DERIVE, KIT, HOST_KIT, &octoscript_ui_l0::kit::lower(root)].join("\n");
    let tree = octoscript_makepad::design::prepare(&kit_source)?;
    // A measured design (an imported artboard) lowers as the Card runner
    // lowers it; a kit-composed card (columns, rows, text) through the
    // backend's general translation.
    let ui = octoscript_makepad::design::to_makepad_ui(&tree).unwrap_or_else(|_| if l0_ui { octoscript_makepad::to_makepad_l0_ui(&tree) } else { octoscript_makepad::to_makepad_ui(&tree) });
    // A card's page fills its screen; a tile measures it instead. The root's
    // own properties are the only lines at this indentation.
    let ui = ui.replacen("\n    height: Fill\n", "\n    height: Fit\n", 1);
    let ui = multiline_fields(&ui);
    Ok(format!("width:Fill height:Fit flow:Overlay {ui}"))
}

/// The host's additions to the L0 kit, after the kit so they win.
///
/// A multi-line field. L0's `Field` has no argument for it (that needs
/// Octoscript: a constructor argument in `ui-l0-constructors.toml`, the
/// checker, and the kit), so the host decides by what the field is for: a
/// field with no `on_commit` has nothing for Return to do, so its text is a
/// body (a reply draft) and it wraps over several lines; a field that
/// commits on Return (a search, a question) stays one line. The kit's own
/// `l0_field`, with the multi-line ones marked for [`multiline_fields`].
const HOST_KIT: &str = r#"
fn l0_field(text, placeholder, target, changing) {
    let node = {t: "input", fillw: 1, text: text, placeholder: placeholder,
            tapto: target, changeto: changing, bg: l0_fill, radius: 12, padx: 14,
            padtop: 14, padbottom: 14, size: 11, color: l0_text}
    if target == "" { node.id = "l0_multiline" }
    return node
}
"#;

/// The lines a multi-line field shows before it scrolls: it grows from
/// about three to about six lines of the kit's field text.
pub const MULTILINE_MIN: f64 = 92.0;
pub const MULTILINE_MAX: f64 = 156.0;

/// Make the fields [`HOST_KIT`] marked multi-line: wrapping, growing with
/// their text between [`MULTILINE_MIN`] and [`MULTILINE_MAX`], then
/// scrolling (Makepad's `TextInput` scrolls a multi-line input inside its
/// bounded height).
fn multiline_fields(ui: &str) -> String {
    let lines: Vec<&str> = ui.lines().collect();
    let mut out = Vec::with_capacity(lines.len() + 4);
    let mut close_at: Option<String> = None;
    for line in lines {
        if let Some(rest) = line.trim_start().strip_prefix("l0_multiline := TextInput {") {
            let indent = &line[..line.len() - line.trim_start().len()];
            out.push(format!("{indent}TextInput {{{rest}"));
            close_at = Some(format!("{indent}}}"));
            continue;
        }
        if close_at.as_deref() == Some(line) {
            let indent = &line[..line.len() - 1];
            out.push(format!("{indent}    is_multiline: true"));
            out.push(format!("{indent}    height: Fit{{min: FitBound.Abs({MULTILINE_MIN}) max: FitBound.Abs({MULTILINE_MAX})}}"));
            close_at = None;
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// The card's last measured height (unclamped), for a surface that sizes
/// to its card.
pub fn measured_height(key: &str) -> Option<f64> {
    HEIGHTS.with(|h| h.borrow().get(key).copied())
}

// ------------------------------------------------------------- L0 taps

/// One `NAV` call from a card: the isolate it came from, the target string
/// and, for a field, the text it carried.
#[derive(Clone, Debug, PartialEq)]
pub struct Tap {
    pub heap: usize,
    pub target: String,
    pub typed: Option<String>,
}

static TAPS: std::sync::Mutex<Vec<Tap>> = std::sync::Mutex::new(Vec::new());

/// The host `NAV` global, installed into every Splash isolate: it only
/// queues the call, tagged with the calling isolate's heap key, so a surface
/// acts only on calls from the isolate it owns.
pub fn install_nav(vm: &mut ScriptVm) {
    let nav = {
        let base = &mut *vm.bx;
        let mut native = base.code.native.borrow_mut();
        native.add_fn(&mut base.heap, script_args_def!(t = NIL, v = NIL), |vm, args| {
            let t = script_value!(vm, args.t);
            let v = script_value!(vm, args.v);
            let mut target = String::new();
            vm.bx.heap.cast_to_string(t, &mut target);
            let typed = (!v.is_nil()).then(|| {
                let mut text = String::new();
                vm.bx.heap.cast_to_string(v, &mut text);
                text
            });
            let heap = vm.bx.heap.heap_key();
            if let Ok(mut taps) = TAPS.lock() {
                // Bounded: a card no surface reads cannot grow it forever.
                if taps.len() >= 64 {
                    taps.remove(0);
                }
                taps.push(Tap { heap, target, typed });
            }
            NIL
        })
    };
    vm.set_injected_global(id!(NAV), nav.into());
}

/// Register [`install_nav`] for every isolate made from now on, once, with
/// the hit target a lowered card's taps are drawn as (`OctoscriptTap`, which
/// the Card runner's own isolate mods do not include).
pub fn ensure_nav() {
    thread_local! {
        static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if !DONE.with(|d| d.replace(true)) {
        widget_async::register_splash_isolate_mod(install_nav);
        #[cfg(any(feature = "app-hub", native_mobile))]
        widget_async::register_splash_isolate_mod(|vm| {
            octosense_appstore::octoscript_widgets::tap::script_mod(vm);
        });
    }
}

/// The queued `NAV` calls from the isolate `heap`; every other isolate's
/// queued calls are dropped (nothing dispatches them).
pub fn take_taps(heap: usize) -> Vec<Tap> {
    let taps = TAPS.lock().map(|mut t| std::mem::take(&mut *t)).unwrap_or_default();
    taps.into_iter().filter(|t| t.heap == heap).collect()
}

/// `l0:{"e":event,"k":instance key,"v":value}` → `(key, event, value)`.
pub fn parse_tap(target: &str) -> Option<(String, String, String)> {
    let json: serde_json::Value = serde_json::from_str(target.strip_prefix("l0:")?).ok()?;
    let field = |name: &str| json.get(name).and_then(serde_json::Value::as_str).map(str::to_string);
    Some((field("k")?, field("e")?, field("v").unwrap_or_default()))
}

/// What the demo host answers a question sent from a card's Ask (MVP: no
/// model call).
pub const DEMO_ANSWER: &str = "Demo answer: Mail's agent will reply here from the thread. (No model was called.)";

/// A live L0 card in the card window: its source, its data (which the
/// host's writes update) and its local state.
pub struct L0Session {
    pub source: String,
    pub data: serde_json::Value,
    pub store: octoscript_ui_l0::InstanceStore,
}

/// What a tap did to an [`L0Session`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TapOutcome {
    pub event: String,
    pub applied: bool,
    /// The card must be lowered again (not for a keystroke in a field,
    /// which already shows what was typed).
    pub relower: bool,
}

fn find_node<'a>(node: &'a octoscript_ui_l0::UiNode, key: &str) -> Option<&'a octoscript_ui_l0::UiNode> {
    if node.key == key {
        return Some(node);
    }
    node.children.iter().find_map(|c| find_node(c, key))
}

fn node_arg<'a>(node: &'a octoscript_ui_l0::UiNode, name: &str) -> Option<&'a octoscript_ui_l0::NodeValue> {
    node.args.iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

impl L0Session {
    pub fn new(l0: &crate::glance::L0Source) -> Self {
        L0Session { source: l0.source.clone(), data: l0.data.clone(), store: Default::default() }
    }

    /// The card as it stands now, lowered for a Splash.
    pub fn body(&self) -> Result<String, String> {
        lower_with_state(&self.source, &self.data, &self.store)
    }

    /// Carry out one `NAV` call from this card.
    pub fn tap(&mut self, target: &str, typed: Option<&str>) -> Result<TapOutcome, String> {
        use octoscript_ui_l0::NodeValue;
        let (key, event, value) = parse_tap(target).ok_or_else(|| format!("not an L0 target: {target}"))?;
        // The element as it stands now: what it carries and what raised it.
        let report = octoscript_ui_l0::realize_with_state(&self.source, &self.data, &self.store, Default::default());
        let node = report.complete_root().ok().and_then(|root| find_node(root, &key)).cloned();
        let keystroke = node.as_ref().is_some_and(|n| n.kind == "Field" && matches!(node_arg(n, "on_change"), Some(NodeValue::Event(e)) if *e == event));
        let payload = if value == "$$" {
            // A field's text, as typed.
            typed.map(|t| serde_json::Value::String(t.to_string()))
        } else if !value.is_empty() {
            Some(serde_json::Value::String(value))
        } else {
            // A payload bound to state was baked in when the card was last
            // lowered; a field edit since then did not re-lower. Read it
            // off the element as it would realize now.
            match node.as_ref().and_then(|n| node_arg(n, "value")) {
                Some(NodeValue::Text(t)) | Some(NodeValue::Token(t)) => Some(serde_json::Value::String(t.clone())),
                Some(NodeValue::Number(n)) => Some(serde_json::json!(n)),
                _ => None,
            }
        };
        let outcome = octoscript_ui_l0::dispatch_reporting(&self.source, &mut self.store, &key, &event, payload.as_ref(), &self.data);
        for write in &outcome.writes {
            self.perform(write);
        }
        let moved = !outcome.changed.is_empty() || !outcome.writes.is_empty();
        Ok(TapOutcome { event, applied: outcome.applied, relower: moved && !keystroke })
    }

    /// A §5.12 write the card reported, performed by this (demo) host: a
    /// question appended to `asked` adds a turn to the `turns` transcript
    /// with a canned answer. Anything else is not performed in the MVP.
    fn perform(&mut self, write: &octoscript_ui_l0::CollectionWrite) {
        match (write.source.as_str(), write.op.as_str()) {
            ("asked", "append") if !write.value.trim().is_empty() => {
                if !self.data.get("turns").is_some_and(serde_json::Value::is_array) {
                    self.data["turns"] = serde_json::json!([]);
                }
                if let Some(turns) = self.data["turns"].as_array_mut() {
                    let id = format!("q{}", turns.len() + 1);
                    turns.push(serde_json::json!({"id": id, "title": write.value, "summary": DEMO_ANSWER}));
                }
            }
            _ => log!("glance: the card's {} {} on {} is not performed (demo host)", write.op, write.helper, write.source),
        }
    }
}

/// The tile height for a measured card height.
pub fn clamp_height(measured: f64) -> f64 {
    measured.clamp(TILE_MIN_HEIGHT, TILE_MAX_HEIGHT)
}

thread_local! {
    /// Measured tile heights by card key (`app/card_id`), shared by every
    /// surface that draws the card (phone glance page, desktop panel).
    static HEIGHTS: RefCell<HashMap<String, f64>> = RefCell::new(HashMap::new());
}

/// The height a card's tile takes: its last measured height, clamped, or the
/// default before it has drawn once.
pub fn tile_height(key: &str) -> f64 {
    HEIGHTS.with(|h| h.borrow().get(key).copied()).map(clamp_height).unwrap_or(TILE_DEFAULT_HEIGHT)
}

fn record_height(key: &str, measured: f64) -> bool {
    HEIGHTS.with(|h| {
        let mut h = h.borrow_mut();
        let old = h.insert(key.to_string(), measured);
        old.map_or(true, |old| (old - measured).abs() > 0.5)
    })
}

script_mod! {
    use mod.prelude.widgets.*
    // One glance tile: the card's Splash in a clipping frame whose height the
    // host sets from the card's measured height.
    mod.widgets.GlanceTileFrame = View {
        width: Fill height: Fit flow: Down clip_y: true clip_x: true
        card := Splash { width: Fill height: Fit }
    }
    // The card window's frame: a card taller than the window scrolls.
    mod.widgets.GlanceSheetFrame = ScrollYView {
        width: Fill height: Fill flow: Down
        card := Splash { width: Fill height: Fit }
    }
}

struct Tile {
    frame: WidgetRef,
    body: std::sync::Arc<str>,
    /// The publishing app, whose requests the tile's calls go out as.
    app: String,
    contained: bool,
}

/// The live tiles one surface draws, by card key. A surface keeps one of
/// these; tiles for cards no longer shown are dropped (their isolates
/// stopped) at the end of each frame.
#[derive(Default)]
pub struct GlanceTiles {
    tiles: HashMap<String, Tile>,
    /// Cards scroll inside their rect instead of clipping (the card window).
    scroll: bool,
}

impl GlanceTiles {
    /// Tiles whose cards scroll inside their rect (the card window).
    pub fn scrolling() -> Self {
        GlanceTiles { tiles: HashMap::new(), scroll: true }
    }

    /// Draw `card` (its `body`, published by `app`) at `rect`: the rect's
    /// height is the tile height; the Splash lays out at its natural height,
    /// and that height is recorded for the next layout. Asks for a redraw
    /// when it changed. The first draw seats the isolate under `app`'s
    /// policy (module docs).
    pub fn draw(&mut self, cx: &mut Cx2d, key: &str, app: &str, contained: bool, body: &std::sync::Arc<str>, rect: Rect) {
        if !CAN_RENDER {
            return;
        }
        ensure_vocabulary(cx);
        let splash = self.open(cx, key, app, contained, body);
        let Some(tile) = self.tiles.get_mut(key) else { return };
        let walk = Walk { abs_pos: Some(rect.pos), width: Size::Fixed(rect.size.x), height: Size::Fixed(rect.size.y), ..Walk::default() };
        let mut scope = Scope::empty();
        // Inside the card's isolate, as the Card runner draws its card.
        match isolate_of(cx, &splash) {
            Some(vm_id) => widget_async::with_isolate(cx, vm_id, |cx| tile.frame.draw_walk_all(cx, &mut scope, walk)),
            None => tile.frame.draw_walk_all(cx, &mut scope, walk),
        }
        let measured = splash.area().rect(cx).size.y;
        if measured > 1.0 && record_height(key, measured) {
            cx.redraw_all();
        }
    }

    /// The tile for `key`, made and seated on first use, running `body`.
    pub(crate) fn open(&mut self, cx: &mut Cx, key: &str, app: &str, contained: bool, body: &std::sync::Arc<str>) -> SplashRef {
        let tile = self.tiles.entry(key.to_string()).or_insert_with(|| Tile { frame: WidgetRef::empty(), body: "".into(), app: app.to_string(), contained });
        if tile.frame.is_empty() {
            let scroll = self.scroll;
            tile.frame = cx.with_vm(|vm| {
                let value = if scroll { script_eval!(vm, { use mod.widgets.* GlanceSheetFrame {} }) } else { script_eval!(vm, { use mod.widgets.* GlanceTileFrame {} }) };
                WidgetRef::script_from_value(vm, value)
            });
            let splash = tile.frame.splash(cx, ids!(card));
            seat(cx, &splash, app, contained);
        }
        let splash = tile.frame.splash(cx, ids!(card));
        if tile.body.as_ref() != body.as_ref() {
            splash.set_text(cx, body);
            tile.body = body.clone();
        }
        splash
    }

    /// The heap key of the isolate `key`'s card runs in, once seated.
    pub fn heap_key(&self, cx: &mut Cx, key: &str) -> Option<usize> {
        let tile = self.tiles.get(key)?;
        let splash = tile.frame.splash(cx, ids!(card));
        let mut splash = splash.borrow_mut()?;
        splash.isolate_heap_key(cx)
    }

    /// Hand `event` to every live tile, inside its isolate (as the Card
    /// runner hands its card events), then run the Card runner's pump for
    /// it: its host requests go to the host services as its app, and the
    /// answers come back to it. A surface calls this for every event while
    /// it has tiles; pointer events only while the tiles are on screen.
    pub fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        for tile in self.tiles.values() {
            let splash = tile.frame.splash(cx, ids!(card));
            match isolate_of(cx, &splash) {
                Some(vm_id) => widget_async::with_isolate(cx, vm_id, |cx| {
                    if let Event::NetworkResponses(responses) = event {
                        // Splash's own pump looks the isolate up as not
                        // installed; installed here, as the Card runner does.
                        cx.handle_script_network_events_for_current_vm(responses);
                    }
                    tile.frame.handle_event(cx, event, &mut Scope::empty());
                }),
                None => tile.frame.handle_event(cx, event, &mut Scope::empty()),
            }
            #[cfg(any(feature = "app-hub", native_mobile))]
            if tile.contained {
                let host_dir = octosense_appstore::data_root(cx).join(".host");
                octosense_appstore::services::pump(cx, &tile.app, &host_dir, &splash, &SplashRef::default());
            }
        }
    }

    /// Stop the isolates of cards no longer published (`live`: the keys of
    /// the cards the surface would show). A card merely scrolled or paged
    /// out of view keeps its tile.
    pub fn sweep(&mut self, cx: &mut Cx, live: &[String]) {
        let gone: Vec<String> = self.tiles.keys().filter(|k| !live.contains(k)).cloned().collect();
        for key in gone {
            if let Some(tile) = self.tiles.remove(&key) {
                let splash = tile.frame.splash(cx, ids!(card));
                // Its waiting host requests end with it: a late answer goes
                // nowhere, never to a tile that takes its place.
                #[cfg(any(feature = "app-hub", native_mobile))]
                if let Some(heap) = splash.borrow_mut().and_then(|mut s| s.isolate_heap_key(cx)) {
                    octosense_appstore::services::cancel_heap(heap);
                }
                splash.set_text(cx, "");
            }
        }
    }
}

/// The isolate a tile's card runs in, once its body has been evaluated.
fn isolate_of(cx: &mut Cx, splash: &SplashRef) -> Option<widget_async::SplashVmId> {
    let splash = splash.borrow()?;
    cx.script_ref_vm_id(&splash.view.source)
}

/// Seat a new tile's isolate before its body runs: the publishing app's
/// policy for a contained app, none for a native module (module docs).
fn seat(cx: &mut Cx, splash: &SplashRef, app: &str, contained: bool) {
    #[cfg(any(feature = "app-hub", native_mobile))]
    if contained {
        match app_isolate(cx, app) {
            Ok(settings) => {
                let applied = octosense_app_policy::splash_adapter::apply(splash, cx, &settings);
                log!("glance: {app}'s tile runs under its policy: {} capability(ies), {} host(s)", applied.capabilities, applied.hosts);
                return;
            }
            Err(e) => log!("glance: {app}'s tile runs with no grants: {e}"),
        }
    }
    let _ = (app, contained);
    splash.set_policy(cx, Some(Vec::new()), Some(TILE_INSTRUCTION_BUDGET));
}

/// The isolate settings of `app`'s policy, resolved as the Card runner
/// resolves them when it opens the app: a system app's from its shipped
/// pack, any other from the last verified catalog. A tile is a background
/// surface, so a service its card calls may not raise a sheet over it.
#[cfg(any(feature = "app-hub", native_mobile))]
pub fn app_isolate(cx: &Cx, app: &str) -> Result<octosense_app_policy::IsolateSettings, String> {
    let root = octosense_appstore::data_root(cx);
    let policy = match octosense_appstore::system::system_app(app) {
        Some(system) => octosense_appstore::system::prepare(&root, &system)?.1,
        None => {
            let anchor = std::env::var("OCTOSENSE_HUB_ANCHOR").unwrap_or_else(|_| octosense_appstore::DEFAULT_ANCHOR.to_string());
            let mut store = octosense_app_hub::Store::new(&anchor, &root, octosense_app_contract::HostLimits::default());
            let catalog = std::fs::read_to_string(root.join("catalog.json")).unwrap_or_default();
            store.accept_catalog(&catalog).map_err(|e| format!("no verified catalog on this device ({e})"))?;
            store.may_run(app)?
        }
    };
    let mut settings = policy.isolate_settings(&root);
    settings.host_prompts = false;
    std::fs::create_dir_all(&settings.jail_root).map_err(|e| format!("the app's storage: {e}"))?;
    Ok(settings)
}

/// Whether this build can draw a card: the Card runner's vocabulary (the
/// design and kit widgets a lowered card names) comes with App Hub.
pub const CAN_RENDER: bool = cfg!(any(feature = "app-hub", native_mobile));

/// Give every Splash isolate the Card runner's vocabulary, once: the same
/// registration the `card` module makes before it runs an app.
fn ensure_vocabulary(cx: &mut Cx) {
    thread_local! {
        static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    ensure_nav();
    #[cfg(any(feature = "app-hub", native_mobile))]
    cx.with_vm(|vm| makepad_app_module::AppModule::register(&octosense_app_hub_app::CARD_MODULE, vm));
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    let _ = cx;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_digest_lowers_through_the_card_pipeline() {
        let (source, data) = crate::glance::demo_digest();
        let body = lower(&source, &data).expect("lowers");
        assert!(body.starts_with("width:Fill height:Fit"), "{body}");
        assert!(!body.contains("\n    height: Fill\n"), "the tile measures the card: {body}");
        // Every value on the card came from `data`.
        assert!(body.contains("3 stories since this morning") && body.contains("Makepad adds contained script isolates"), "{body}");
    }

    /// A system app, registered from a pack made on the fly, whose manifest
    /// grants `glance` and `storage` and nothing else.
    #[cfg(feature = "app-hub")]
    fn register_test_app(id: &'static str) {
        let dir = std::env::temp_dir().join(format!("glance-tile-app-{id}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.splash"), "View{}").unwrap();
        let digest = octosense_app_contract::digest_dir(&dir).unwrap();
        let manifest = format!(
            r#"{{"schema":1,"id":"{id}","version":"1","name":"Glance test","integrity":{{"bundle_blake3":"{digest}"}},"capabilities":["storage","glance"]}}"#
        );
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        let pack = serde_json::to_string(&octosense_app_hub::pack::pack_dir(&dir).unwrap()).unwrap();
        octosense_appstore::system::register_system_app(octosense_appstore::system::SystemApp { id, name: "Glance test", pack: Box::leak(pack.into_boxed_str()), assets: &[] });
    }

    /// A Cx with the widgets and the tile frame registered, for driving
    /// tiles without a window.
    #[cfg(feature = "app-hub")]
    fn tile_cx() -> Cx {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            super::script_mod(vm);
        });
        cx
    }

    #[cfg(feature = "app-hub")]
    fn heap_of(cx: &mut Cx, splash: &SplashRef) -> usize {
        splash.borrow_mut().unwrap().isolate_heap_key(cx).expect("the card runs in its own isolate")
    }

    /// A contained app's tile runs under that app's resolved policy, the one
    /// its Card runner applies: its grants pass the isolate's gate, and
    /// nothing it was not granted does. A native module's tile gets none.
    #[cfg(feature = "app-hub")]
    #[test]
    fn a_tile_runs_under_its_apps_policy() {
        use makepad_widgets::splash_policy::service_allowed;
        register_test_app("os.glancetile");
        let mut cx = tile_cx();
        let mut tiles = GlanceTiles::default();
        let app = tiles.open(&mut cx, "os.glancetile/c", "os.glancetile", true, &"View{}".into());
        let heap = heap_of(&mut cx, &app);
        assert!(service_allowed(heap, "glance.list").is_ok(), "the app's grant");
        assert!(service_allowed(heap, "mail.send").is_err(), "not granted to the app");
        let native = tiles.open(&mut cx, "news/c", "news", false, &"View{}".into());
        let heap = heap_of(&mut cx, &native);
        assert!(service_allowed(heap, "glance.list").is_err(), "a native module's tile has no grants");
    }

    // A widget a card can hold that answers typed text the way a card's
    // handler would: by making a host request, from the card's isolate.
    script_mod! {
        use mod.prelude.widgets.*
        mod.widgets.GlanceInputProbe = set_type_default() do #(GlanceInputProbe::register_widget(vm)) {}
        mod.prelude.widgets.GlanceInputProbe = mod.widgets.GlanceInputProbe
    }
    #[derive(Script, ScriptHook, Widget)]
    struct GlanceInputProbe {
        #[deref]
        view: View,
    }
    thread_local! {
        static TYPED: RefCell<String> = RefCell::new(String::new());
    }
    impl Widget for GlanceInputProbe {
        fn handle_event(&mut self, cx: &mut Cx, event: &Event, _: &mut Scope) {
            if let Event::TextInput(input) = event {
                TYPED.with(|t| t.borrow_mut().push_str(&input.input));
                cx.with_vm(|vm| {
                    script_eval!(vm, { mod.host.request("glance.publish", {card_id: "typed" title: "Typed" script: "View{}"}, nil) });
                });
            }
        }
        fn draw_walk(&mut self, _: &mut Cx2d, _: &mut Scope, _: Walk) -> DrawStep {
            DrawStep::done()
        }
    }

    /// Input reaches a tile's card, inside its isolate, and the request the
    /// card makes goes out through the Card runner's pump to the host
    /// services as the publishing app.
    #[cfg(feature = "app-hub")]
    #[test]
    fn input_reaches_the_card_and_its_requests_go_out_as_the_app() {
        register_test_app("os.glanceinput");
        crate::glance::register();
        widget_async::register_splash_isolate_mod(|vm| {
            script_mod(vm);
        });
        let mut cx = tile_cx();
        let mut tiles = GlanceTiles::default();
        tiles.open(&mut cx, "os.glanceinput/draft", "os.glanceinput", true, &"probe := GlanceInputProbe{}".into());
        tiles.handle_event(&mut cx, &Event::TextInput(TextInputEvent { input: "hello".into(), ..Default::default() }));
        assert_eq!(TYPED.with(|t| t.borrow().clone()), "hello", "the typed text reached the card");
        // The request the card queued goes out on the next event.
        tiles.handle_event(&mut cx, &Event::Signal);
        assert!(crate::glance::shown().iter().any(|c| c.key() == "os.glanceinput/typed" && c.contained), "published as the tile's app");
    }

    fn mail_session(card_id: &str) -> L0Session {
        let (_, _, source, data) = crate::glance::demo_mail().into_iter().find(|c| c.0 == card_id).unwrap();
        L0Session::new(&crate::glance::L0Source { source, data })
    }

    /// The tap targets a body offers, in order, as `(key, event)`.
    fn targets(body: &str) -> Vec<(String, String, String)> {
        body.split("NAV(t: ")
            .skip(1)
            .filter_map(|rest| {
                let lit: String = serde_json::from_str::<String>(&rest[..rest.find("\"}\"").map(|i| i + 3).unwrap_or(0)]).ok()?;
                parse_tap(&lit)
            })
            .collect()
    }
    fn target_for(body: &str, event: &str) -> String {
        let (k, e, v) = targets(body).into_iter().find(|(_, e, _)| e == event).unwrap_or_else(|| panic!("no {event} in {body}"));
        format!("l0:{}", serde_json::json!({"e": e, "k": k, "v": v}))
    }

    /// The fake email card, end to end through the host's L0 loop: Reply
    /// shows the AI draft in a field with Cancel/Send; Send shows "Sent
    /// (demo)"; Ask shows the transcript, and a question typed and sent adds
    /// itself and the canned answer.
    #[test]
    fn the_mail_card_replies_sends_and_asks_through_l0_taps() {
        let mut s = mail_session("ana-contract");
        let body = s.body().unwrap();
        for text in ["Ana Lee · Contract question", "10:42", "AI SUMMARY", "Suggested: Reply — confirm Friday", "Reply", "Mark done", "Ask"] {
            assert!(body.contains(text), "{text} in {body}");
        }
        assert!(!body.contains("Sent (demo)"));
        let reply = target_for(&body, "reply");
        assert!(s.tap(&reply, None).unwrap().relower);
        let body = s.body().unwrap();
        assert!(body.contains("AI DRAFT") && body.contains("Hi Ana, Friday works for us.") && body.contains("Cancel") && body.contains("Send"), "{body}");
        // The draft (no on_commit) wraps over several lines; nothing else does.
        assert_eq!(body.matches("is_multiline: true").count(), 1, "{body}");
        assert!(body.contains("max: FitBound.Abs(156)") && !body.contains("l0_multiline"), "{body}");
        // A keystroke in the draft updates state without re-lowering.
        let edit = target_for(&body, "edit");
        let typed = s.tap(&edit, Some("Hi Ana, Friday is fine.")).unwrap();
        assert!(typed.applied && !typed.relower, "{typed:?}");
        assert!(s.body().unwrap().contains("Hi Ana, Friday is fine."));
        let send = target_for(&s.body().unwrap(), "send");
        assert!(s.tap(&send, None).unwrap().relower);
        assert!(s.body().unwrap().contains("Sent (demo)"));

        let mut s = mail_session("ana-contract");
        let ask = target_for(&s.body().unwrap(), "ask");
        s.tap(&ask, None).unwrap();
        let body = s.body().unwrap();
        assert!(body.contains("What did they say about payment?") && body.contains("Net 30 instead of net 45"), "{body}");
        assert!(!body.contains("is_multiline"), "the question commits on Return: one line");
        let typing = target_for(&body, "typing");
        assert!(!s.tap(&typing, Some("When do they need it?")).unwrap().relower);
        // The arrow carries the question as it stands now, not as it was
        // when the card was last lowered (empty).
        let submit = targets(&body).into_iter().find(|(k, e, _)| e == "submit" && k.contains("Chip")).unwrap();
        let arrow = format!("l0:{}", serde_json::json!({"e": submit.1, "k": submit.0, "v": submit.2}));
        assert!(s.tap(&arrow, None).unwrap().relower);
        let body = s.body().unwrap();
        assert!(body.contains("When do they need it?") && body.contains(DEMO_ANSWER), "{body}");
    }

    /// The shipping card: carrier, status, ETA and Track; no reply.
    #[test]
    fn the_shipping_card_tracks_and_offers_no_reply() {
        let mut s = mail_session("ups-lamp");
        let body = s.body().unwrap();
        for text in ["UPS · Your desk lamp has shipped", "Out for delivery", "Today by 8 pm", "Track"] {
            assert!(body.contains(text), "{text} in {body}");
        }
        let events: Vec<String> = targets(&body).into_iter().map(|(_, e, _)| e).collect();
        assert_eq!(events, ["track", "done"]);
        s.tap(&target_for(&body, "track"), None).unwrap();
        assert!(s.body().unwrap().contains("Opening the carrier's tracking page (demo)"));
        assert!(parse_tap("NAV").is_none() && parse_tap("l0:{}").is_none());
    }

    /// A tile is a background surface: what its card asks of a host service
    /// cannot raise a sheet over the home screen.
    #[cfg(feature = "app-hub")]
    #[test]
    fn a_tiles_isolate_cannot_raise_a_sheet() {
        register_test_app("os.glancequiet");
        let cx = tile_cx();
        let settings = app_isolate(&cx, "os.glancequiet").unwrap();
        assert!(!settings.host_prompts, "a tile's requests may not raise a sheet");
        assert!(settings.capabilities.iter().any(|c| c == "glance"), "the app's own grants still apply");
    }

    /// A tile that goes away takes its waiting requests with it: an answer
    /// that arrives later goes nowhere.
    #[cfg(feature = "app-hub")]
    #[test]
    fn a_swept_tiles_requests_are_cancelled() {
        use octosense_appstore::services::{dispatch, register_host_service, take_replies_for, HostService, Replier, ServiceCall, ServiceHost};
        use std::sync::{Arc, Mutex};
        struct Holds(Arc<Mutex<Option<Replier>>>);
        impl HostService for Holds {
            fn family(&self) -> &'static str {
                "glancetilehold"
            }
            fn call(&mut self, _call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
                *self.0.lock().unwrap() = Some(reply);
            }
        }
        struct NoSheet;
        impl ServiceHost for NoSheet {
            fn open_sheet(&mut self, _: String) {}
            fn close_sheet(&mut self) {}
        }
        register_test_app("os.glancesweep");
        let held = Arc::new(Mutex::new(None));
        register_host_service(Box::new(Holds(held.clone())));
        let mut cx = tile_cx();
        let mut tiles = GlanceTiles::default();
        let splash = tiles.open(&mut cx, "os.glancesweep/c", "os.glancesweep", true, &"View{}".into());
        let heap = heap_of(&mut cx, &splash);
        let call = ServiceCall { app_id: "os.glancesweep".into(), service: "glancetilehold.wait".into(), args: serde_json::Value::Null,
            from_sheet: false, may_prompt: false, host_dir: std::env::temp_dir() };
        dispatch(call, heap, 1, &mut NoSheet);
        tiles.sweep(&mut cx, &[]);
        held.lock().unwrap().take().expect("the service holds the request").send(Ok(serde_json::Value::Null));
        assert!(take_replies_for(&[heap]).is_empty(), "nothing is left queued for the tile that went away");
    }

    #[test]
    fn heights_clamp_to_the_tile_range() {
        assert_eq!(clamp_height(10.0), TILE_MIN_HEIGHT);
        assert_eq!(clamp_height(900.0), TILE_MAX_HEIGHT);
        assert_eq!(clamp_height(120.0), 120.0);
        assert_eq!(tile_height("nobody/never"), TILE_DEFAULT_HEIGHT);
    }
}
