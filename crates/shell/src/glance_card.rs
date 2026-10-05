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
//! bounded below by [`TILE_MIN_HEIGHT`]; until the first draw measures it,
//! [`TILE_DEFAULT_HEIGHT`]. The phone feed uses the full measured height so
//! its scroll range includes trailing controls. Desktop tiles are capped
//! at [`TILE_MAX_HEIGHT`] ([`overflow`] reports the excess), with scrolling
//! inside the tile where offered ([`GlanceTiles::draw_scrolled`]). The
//! expanded card scrolls its focused editor into view when resized, leaving
//! room for the following action row. A script card should size its root `Fit`.
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
//! (glance_sheet.rs), the desktop's glance panel (glance_panel.rs) and the
//! expanded phone workspace (glance_sheet.rs) keep their L0 cards
//! in a [`LiveCards`], one path for all three: a tile's own isolate's calls
//! ([`take_taps`], never another tile's) run through its card's
//! [`L0Session`], made for the app that published the card: the declared
//! transition (`octoscript_ui_l0` dispatch), the §5.12 writes the host
//! performs (a `sys.chat` append, glance_chat.rs), and a re-lowering. On the
//! phone the shell first drops a card's clicks ([`drop_clicks`]) when the
//! finger was no plain tap on that card: a page swipe, a pull, a long press.
use makepad_widgets::*;
use std::cell::RefCell;
use std::collections::HashMap;

pub const TILE_MIN_HEIGHT: f64 = 72.0;
pub const TILE_MAX_HEIGHT: f64 = 440.0;
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

/// Where a tile at `rect` has its dismiss button: left of its open button.
pub fn dismiss_button(rect: Rect) -> Rect {
    let open = open_button(rect);
    Rect { pos: dvec2(open.pos.x - OPEN_BUTTON - 4.0, open.pos.y), size: open.size }
}

macro_rules! l0_kit {
    ($name:literal) => {
        include_str!(concat!(env!("OCTOSENSE_WORKSPACE"), "/octoscript-makepad/components/l0/", $name))
    };
}

const PALETTE_BASE: &str = l0_kit!("_palette_dark.octoscript");

/// The shell's mode, for a card that names no theme of its own: it takes
/// the light palette in a light shell and the dark one in a dark shell, as
/// every other surface does (desktop_app.rs sets it with the style; a live
/// card lowers again when it changes, [`L0Session::mode_moved`]).
static DARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Set the shell's mode; true when it changed.
pub fn set_dark(dark: bool) -> bool {
    DARK.swap(dark, std::sync::atomic::Ordering::Relaxed) != dark
}

pub fn dark() -> bool {
    DARK.load(std::sync::atomic::Ordering::Relaxed)
}
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
/// kit, evaluate the checked design VM, translate to Makepad UI through
/// Octoscript-Makepad's L0 translation, as the card window does: it keeps
/// the kit's resolved sizes, its tiles' colours and its chips' labels (the
/// general translation re-lowered chips the Material way and dropped their
/// text, and drew the shipping card's tiles light under white text).
pub fn lower(source: &str, data: &serde_json::Value) -> Result<String, String> {
    lower_report(source, octoscript_ui_l0::realize(source, data, Default::default()), true)
}

/// [`lower`], realized against a card's local state (its `InstanceStore`),
/// for the card window.
pub fn lower_with_state(source: &str, data: &serde_json::Value, store: &octoscript_ui_l0::InstanceStore) -> Result<String, String> {
    lower_report(source, octoscript_ui_l0::realize_with_state(source, data, store, Default::default()), true)
}

fn lower_report(source: &str, report: octoscript_ui_l0::RealizeReport, l0_ui: bool) -> Result<String, String> {
    let root = report.complete_root()?;
    if octoscript_ui_l0::kit_pack::contains(root) {
        return Err("native kit components are not offered on a glance tile".into());
    }
    let mood = octoscript_ui_l0::card_theme(source).unwrap_or_else(|| if dark() { "dark" } else { "light" }.into());
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
    let ui = theme_fonts(&ui);
    Ok(format!("width:Fill height:Fit flow:Overlay {ui}"))
}

/// The card's text in the theme's fonts, as every app's: the backend names
/// Roboto (its reference renderer's face) for text in no font of its own,
/// so a glance card would not match the shell and the apps around it. Such
/// a line takes the theme's bold role at weight 500 and up, its regular one
/// below, at the same size and spacing. A mood with a font of its own
/// (atro's Montserrat) keeps it.
fn theme_fonts(ui: &str) -> String {
    const BACKEND_FACE: &str = "crate_resource(\"makepad_widgets:resources/Roboto-Regular.ttf\")";
    const STYLE: &str = "draw_text.text_style: TextStyle{";
    let number = |line: &str, key: &str| {
        let rest = line.split(key).nth(1)?;
        let n: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        (!n.is_empty()).then_some(n)
    };
    let mut out: Vec<String> = Vec::new();
    for line in ui.lines() {
        let (Some(at), true) = (line.find(STYLE), line.contains(BACKEND_FACE)) else {
            out.push(line.to_string());
            continue;
        };
        let (Some(weight), Some(size)) = (number(line, "weight: "), number(line, "font_size: ")) else {
            out.push(line.to_string());
            continue;
        };
        let role = if weight.parse::<f64>().unwrap_or(400.0) >= 500.0 { "font_bold" } else { "font_regular" };
        let spacing = number(line, "line_spacing: ").unwrap_or_else(|| "1.45".into());
        out.push(format!("{}draw_text.text_style: mod.theme.{role}{{ line_spacing: {spacing} font_size: {size} }}", &line[..at]));
    }
    out.join("\n")
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
            queue_tap(Tap { heap: vm.bx.heap.heap_key(), target, typed });
            NIL
        })
    };
    vm.set_injected_global(id!(NAV), nav.into());
}

/// The most `NAV` calls kept waiting: a card no surface reads cannot grow
/// the queue forever.
const TAPS_MAX: usize = 64;

fn queue_tap(tap: Tap) {
    if let Ok(mut taps) = TAPS.lock() {
        if taps.len() >= TAPS_MAX {
            taps.remove(0);
        }
        taps.push(tap);
    }
}

/// [`queue_tap`], for another module's tests: a card's `NAV` call.
#[cfg(all(test, feature = "app-hub"))]
pub(crate) fn queue_tap_for_test(tap: Tap) {
    queue_tap(tap)
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

/// The queued `NAV` calls from the isolate `heap`, in order, taken off the
/// queue. Every other isolate's calls stay for the surface whose tile it is
/// (two surfaces dispatch: the card window and the glance panel); those of
/// a tile no surface dispatches go when the queue is full, or when the tile
/// does ([`drop_taps`]).
pub fn take_taps(heap: usize) -> Vec<Tap> {
    let Ok(mut taps) = TAPS.lock() else { return Vec::new() };
    if !taps.iter().any(|t| t.heap == heap) {
        return Vec::new();
    }
    let (mine, others) = std::mem::take(&mut *taps).into_iter().partition(|t| t.heap == heap);
    *taps = others;
    mine
}

/// Whether the isolate `heap` has a click waiting: a call with no text (a
/// tap target's; a field's carries its text). A tap target claims no press,
/// so this is how a surface learns that a press was on one (glance_panel.rs).
pub fn has_clicks(heap: usize) -> bool {
    TAPS.lock().is_ok_and(|taps| taps.iter().any(|t| t.heap == heap && t.typed.is_none()))
}

/// Forget the queued calls of the isolate `heap`, whose tile went away: a
/// heap key is unique only while its isolate lives, so a later isolate may
/// get the same one, and must not run the calls the old one queued.
pub fn drop_taps(heap: usize) {
    if let Ok(mut taps) = TAPS.lock() {
        taps.retain(|t| t.heap != heap);
    }
}

/// Forget the isolate `heap`'s queued clicks, its calls with no text (a tap
/// target's), and keep its field edits (a field's call carries the text the
/// person typed): for a surface whose finger was no tap on that tile
/// (mobile_pages.rs `GlanceCards`). How many went.
pub fn drop_clicks(heap: usize) -> usize {
    let Ok(mut taps) = TAPS.lock() else { return 0 };
    let before = taps.len();
    taps.retain(|t| t.heap != heap || t.typed.is_some());
    before - taps.len()
}

/// `l0:{"e":event,"k":instance key,"v":value}` → `(key, event, value)`.
pub fn parse_tap(target: &str) -> Option<(String, String, String)> {
    let json: serde_json::Value = serde_json::from_str(target.strip_prefix("l0:")?).ok()?;
    let field = |name: &str| json.get(name).and_then(serde_json::Value::as_str).map(str::to_string);
    Some((field("k")?, field("e")?, field("v").unwrap_or_default()))
}

/// A live L0 card (in the card window or a glance-panel tile): the app that
/// published it, its source, its data as published and its local state. Its
/// `sys.chat` sources are answered by the host on every lowering and
/// dispatch (glance_chat.rs), never from `data`.
pub struct L0Session {
    pub app: String,
    pub source: String,
    pub data: serde_json::Value,
    pub store: octoscript_ui_l0::InstanceStore,
    pub mail: Option<crate::mail_card::Session>,
    /// The chat generation the card was last lowered at.
    chat_generation: u64,
    /// The card reads a `sys.chat` (worked out once: every event asks).
    reads_chat: bool,
    /// The shell's mode the card was last lowered in.
    dark: bool,
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
    pub(crate) fn has_chat(&self) -> bool { self.chat_source().is_ok() }
    pub(crate) fn mail_reply(&self) -> Option<&serde_json::Value> { self.mail.as_ref().map(|m| m.snapshot()) }

    pub(crate) fn chat_access(&self) -> (Option<String>, bool) {
        (crate::ai_host::contained::account_of(&self.app), crate::agents::access(&self.app) == crate::agents::Access::Allowed && self.mail.as_ref().is_none_or(|m| crate::mail_card::account_valid(&m.binding.account)))
    }

    fn chat_source(&self) -> Result<octosense_l0_chat::ChatSource, String> {
        let mut sources = octosense_l0_chat::sources(&self.source).into_iter().filter(|source| source.app.as_deref() == Some(self.app.as_str()));
        let source = sources.next().ok_or("This card has no conversation")?;
        if sources.next().is_some() { return Err("Open the conversation from this card's own controls".into()); }
        Ok(source)
    }

    pub(crate) fn chat_snapshot(&mut self) -> Result<serde_json::Value, String> {
        if let Some(mail) = &mut self.mail { mail.refresh(); }
        let source = self.chat_source()?;
        let data = self.data_now();
        data.pointer(&format!("/{}", source.name.replace('.', "/"))).cloned().ok_or_else(|| "This conversation is unavailable".into())
    }

    /// The host's fixed composer accepts text from its native input only.
    /// Identity, account, thread, consent and draft context still go through
    /// the same sys.chat adapter used by the generated card.
    pub(crate) fn chat_submit(&mut self, text: &str) -> Result<(), String> {
        let source = self.chat_source()?;
        if let Some(mail) = &mut self.mail { mail.refresh(); }
        let write = octoscript_ui_l0::CollectionWrite {
            source: source.name, helper: "sys.chat".into(), op: "append".into(), value: text.into(), field: String::new(),
        };
        let data = self.data_now();
        let origin = Some(octoscript_ui_l0::ValueOrigin::UserInput);
        match &self.mail {
            Some(mail) => {
                let binding = mail.chat_edit_binding()?;
                if let Err(error) = crate::glance_chat::perform_bound(&self.app, &self.source, &self.store, &data, &write, origin, &binding) {
                    drop(crate::mail_card::ChatEditLease::new(Some(&binding)));
                    return Err(error);
                }
            }
            None => { crate::glance_chat::perform(&self.app, &self.source, &self.store, &data, &write, origin)?; }
        }
        Ok(())
    }

    /// A generated transition opened its chat page. The focused host can
    /// present the same transcript in the native conversation column.
    pub(crate) fn shows_chat(&self) -> bool {
        fn contains_chat(session: &L0Session, data: &serde_json::Value, node: &octoscript_ui_l0::UiNode) -> bool {
            if node.kind == "ChatEntry" { return true; }
            // An empty conversation has no ChatEntry yet. Inspect its declared
            // actions in a disposable state clone; never perform these writes.
            for (_, arg) in &node.args {
                if let octoscript_ui_l0::NodeValue::Event(event) = arg {
                    let mut state = session.store.clone();
                    let outcome = octoscript_ui_l0::dispatch_reporting_with_origin(&session.source, &mut state, &node.key, event,
                        Some(&serde_json::json!("")), data, octoscript_ui_l0::ValueOrigin::UserInput);
                    if outcome.writes.iter().any(|write| write.helper == "sys.chat" && write.op == "append") { return true; }
                }
            }
            node.children.iter().any(|node| contains_chat(session, data, node))
        }
        if !self.reads_chat { return false; }
        let data = self.data_now();
        octoscript_ui_l0::realize_with_state(&self.source, &data, &self.store, Default::default())
            .complete_root().is_ok_and(|node| contains_chat(self, &data, node))
    }
    /// The card `app` published.
    pub fn new(app: &str, l0: &crate::glance::L0Source) -> Self {
        L0Session {
            app: app.to_string(),
            source: l0.source.clone(),
            data: l0.data.clone(),
            store: Default::default(),
            mail: l0.mail.clone().map(crate::mail_card::Session::new),
            chat_generation: crate::glance_chat::generation(),
            reads_chat: crate::glance_chat::reads_chat(&l0.source),
            dark: dark(),
        }
    }

    /// The data the card reads now: as published, with the host's answer
    /// to each `sys.chat`.
    fn data_now(&self) -> serde_json::Value {
        if let Some(mail) = &self.mail {
            let seeded = mail.seed(&self.source, &self.data);
            return match mail.chat_binding().and_then(|binding| crate::glance_chat::seed_bound(&self.app, &self.source, &seeded, &self.store, &binding)) {
                Ok(data) => data,
                Err(_) => crate::mail_card::unavailable_chat(&self.source, &seeded),
            };
        }
        crate::glance_chat::seed(&self.app, &self.source, &self.data, &self.store)
    }

    /// The card as it stands now, lowered for a Splash.
    pub fn body(&mut self) -> Result<String, String> {
        if let Some(mail) = &mut self.mail { mail.refresh(); }
        self.chat_generation = crate::glance_chat::generation();
        self.dark = dark();
        lower_with_state(&self.source, &self.data_now(), &self.store)
    }

    /// The shell changed mode since the card was last lowered: lower it
    /// again in the new palette.
    pub fn mode_moved(&self) -> bool {
        self.dark != dark()
    }

    /// A conversation the card reads changed since it was last lowered (a
    /// reply arrived): lower it again.
    pub fn chat_moved(&self) -> bool {
        (self.reads_chat && self.chat_generation != crate::glance_chat::generation())
            || self.mail.as_ref().is_some_and(crate::mail_card::Session::moved)
    }

    /// Carry out one tile's queued `NAV` calls, in order, then lower the
    /// card again when a tap, or a conversation the card reads (the agent's
    /// reply), moved it: the dispatch the card window and the glance panel
    /// share ([`LiveCards`]). The new body when the card changed; `who`
    /// heads the log lines (`glance sheet: os.mail/ana-contract`).
    pub fn run(&mut self, taps: Vec<Tap>, who: &str) -> Option<String> {
        let mut relower = self.chat_moved() || self.mode_moved();
        for tap in taps {
            match self.tap(&tap.target, tap.typed.as_deref()) {
                Ok(outcome) => {
                    log!("{who} tap {} (applied {}, relower {})", outcome.event, outcome.applied, outcome.relower);
                    relower |= outcome.relower;
                }
                Err(e) => log!("{who} tap refused: {e}"),
            }
        }
        if !relower {
            return None;
        }
        match self.body() {
            Ok(body) => Some(body),
            Err(e) => {
                log!("{who} does not lower: {e}");
                None
            }
        }
    }

    /// Carry out one `NAV` call from this card.
    pub fn tap(&mut self, target: &str, typed: Option<&str>) -> Result<TapOutcome, String> {
        use octoscript_ui_l0::NodeValue;
        let (key, event, value) = parse_tap(target).ok_or_else(|| format!("not an L0 target: {target}"))?;
        // The element as it stands now: what it carries and what raised it.
        let data = self.data_now();
        let report = octoscript_ui_l0::realize_with_state(&self.source, &data, &self.store, Default::default());
        let root = report.complete_root().ok();
        let node = root.and_then(|root| find_node(root, &key)).cloned();
        // Where the payload came from: what the person typed into a field,
        // or the value the element carries (§4's event origins).
        let origin = root.and_then(|root| octoscript_ui_l0::event_payload_origin(root, &key, &event));
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
        let outcome = octoscript_ui_l0::dispatch_reporting_with_origin(&self.source, &mut self.store, &key, &event, payload.as_ref(), &data, origin.unwrap_or(octoscript_ui_l0::ValueOrigin::Authored));
        for write in &outcome.writes {
            self.perform(write, origin, &data, keystroke);
        }
        let moved = !outcome.changed.is_empty() || !outcome.writes.is_empty();
        Ok(TapOutcome { event, applied: outcome.applied, relower: moved && !keystroke })
    }

    /// A §5.12 write the card reported, performed by the host: a
    /// `sys.chat` append (glance_chat.rs). The demo host performs no other.
    fn perform(&mut self, write: &octoscript_ui_l0::CollectionWrite, origin: Option<octoscript_ui_l0::ValueOrigin>, data: &serde_json::Value, from_field: bool) {
        if matches!(write.helper.as_str(), "sys.mail_draft" | "sys.mail_review") {
            let result = self.mail.as_mut().ok_or_else(|| "Mail card has no host binding".to_string()).and_then(|mail| mail.perform(write, origin, from_field));
            if let Err(error) = result { log!("glance: Mail write refused: {error}"); }
            return;
        }
        if write.helper == "sys.chat" {
            let result = match &self.mail {
                Some(mail) => mail.chat_binding().and_then(|binding| crate::glance_chat::perform_bound(&self.app, &self.source, &self.store, data, write, origin, &binding)),
                None => crate::glance_chat::perform(&self.app, &self.source, &self.store, data, write, origin),
            };
            match result {
                Ok(entry) => log!("glance: {} chat {} recorded {}", self.app, write.source, entry.id),
                Err(e) => log!("glance: {} chat {} refused: {e}", self.app, write.source),
            }
            return;
        }
        log!("glance: the card's {} {} on {} is not performed (demo host)", write.op, write.helper, write.source);
    }
}

/// The L0 cards one surface keeps live, by tile key: each card's
/// [`L0Session`] and the body its tile draws now. The card window keeps one
/// for its card, and the glance panel and the phone's glance page one for
/// their tiles, so a tap runs the same way on each (module docs, "L0
/// taps"). A script card is not kept here: it runs as published and keeps
/// its own state.
#[derive(Default)]
pub struct LiveCards {
    cards: HashMap<String, LiveCard>,
}

struct LiveCard {
    /// The card's key (`app/card_id`), for the log.
    key: String,
    /// The publish the session runs: a newer publish of the card starts over.
    published: std::sync::Arc<crate::glance::L0Source>,
    session: L0Session,
    body: std::sync::Arc<str>,
    lowered: bool,
}

impl LiveCards {
    /// A host-owned pane needs the publication's session, not a generated
    /// layout. Defer lowering until an actual generated tile asks for it.
    pub(crate) fn prepare_native(&mut self, tile: &str, card: &crate::glance::GlanceCard) {
        let Some(l0) = &card.l0 else { self.cards.remove(tile); return; };
        if self.cards.get(tile).is_some_and(|live| std::sync::Arc::ptr_eq(&live.published, l0)) { return; }
        self.cards.insert(tile.to_string(), LiveCard {
            key: card.key(), published: l0.clone(), session: L0Session::new(&card.app, l0),
            body: card.body.clone(), lowered: false,
        });
    }

    pub(crate) fn session_mut(&mut self, tile: &str) -> Option<&mut L0Session> {
        self.cards.get_mut(tile).map(|card| &mut card.session)
    }

    pub(crate) fn restore_store(&mut self, tile: &str, store: octoscript_ui_l0::InstanceStore) {
        if let Some(card) = self.cards.get_mut(tile) {
            card.session.store = store;
            if let Ok(body) = card.session.body() { card.body = body.into(); card.lowered = true; }
        }
    }
    /// What tile `tile` draws for `card`: an L0 card as its session has it
    /// now (made on first use for the app that published the card, and made
    /// again for a newer publish), a script card as published. `who` heads
    /// the log lines (`glance panel`).
    pub fn body(&mut self, tile: &str, card: &crate::glance::GlanceCard, who: &str) -> std::sync::Arc<str> {
        let Some(l0) = &card.l0 else {
            self.cards.remove(tile);
            return card.body.clone();
        };
        if let Some(live) = self.cards.get_mut(tile).filter(|live| std::sync::Arc::ptr_eq(&live.published, l0)) {
            // The shell changed mode: the card takes the new palette.
            if !live.lowered || live.session.mode_moved() || live.session.chat_moved() {
                match live.session.body() {
                    Ok(body) => { live.body = body.into(); live.lowered = true; }
                    Err(e) => log!("{who}: {} does not lower in the new mode: {e}", live.key),
                }
            }
            return live.body.clone();
        }
        let mut session = L0Session::new(&card.app, l0);
        let body: std::sync::Arc<str> = match session.body() {
            Ok(body) => body.into(),
            Err(e) => {
                log!("{who}: {} lowers as published only: {e}", card.key());
                card.body.clone()
            }
        };
        self.cards.insert(tile.to_string(), LiveCard { key: card.key(), published: l0.clone(), session, body: body.clone(), lowered: true });
        body
    }

    /// Run each live card's queued taps, those of its own tile's isolate
    /// only, through its session ([`L0Session::run`]). True when a card
    /// changed: the surface redraws.
    pub fn dispatch(&mut self, cx: &mut Cx, tiles: &GlanceTiles, who: &str) -> bool {
        let mut changed = false;
        for (tile, live) in &mut self.cards {
            let taps = tiles.heap_key(cx, tile).map(take_taps).unwrap_or_default();
            if taps.is_empty() && !live.session.chat_moved() && !live.session.mode_moved() {
                continue;
            }
            if let Some(body) = live.session.run(taps, &format!("{who}: {}", live.key)) {
                live.body = body.into();
                live.lowered = true;
                changed = true;
            }
        }
        changed
    }

    /// Forget the cards whose tiles the surface no longer has (`live`: the
    /// tile keys it keeps).
    pub fn retain(&mut self, live: &[String]) {
        self.cards.retain(|tile, _| live.contains(tile));
    }

    pub fn clear(&mut self) {
        self.cards.clear();
    }
}

/// The tile height for a measured card height.
pub fn clamp_height(measured: f64) -> f64 {
    measured.clamp(TILE_MIN_HEIGHT, TILE_MAX_HEIGHT)
}

/// How much taller than its tile a card is, as last measured: what of it the
/// tile cannot show at once (0 when it fits).
pub fn overflow(key: &str) -> f64 {
    measured_height(key).map_or(0.0, |m| (m - tile_height(key)).max(0.0))
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

/// A phone feed scrolls the entire card instead of clipping it at a tile
/// cap. Include long chat replies and the controls following them in its
/// scroll extent; desktop tiles retain their own bounded scrolling policy.
pub fn feed_height(key: &str) -> f64 {
    measured_height(key).filter(|h| h.is_finite())
        .map(|h| h.max(TILE_MIN_HEIGHT)).unwrap_or(TILE_DEFAULT_HEIGHT)
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
    viewport: Option<Rect>,
    focused: Option<WidgetUid>,
}

fn focused_widget(widget: &WidgetRef, focus: Area) -> Option<WidgetUid> {
    if focus.is_empty() { return None; }
    if widget.area() == focus { return Some(widget.widget_uid()); }
    let mut found = None;
    widget.children(&mut |_, child| {
        if found.is_none() { found = focused_widget(&child, focus); }
    });
    found
}

/// Keep the editor and its following action row inside a resized viewport.
/// Only applied on a viewport/focus change, so reading older chat by hand
/// never snaps the scroll position back to the composer.
pub(crate) fn editor_scroll(current: f64, viewport: Rect, editor: Rect, content_height: f64) -> f64 {
    let actions = 88.0_f64.min((viewport.size.y - editor.size.y - 16.0).max(0.0));
    let top = viewport.pos.y + 8.0;
    let bottom = viewport.pos.y + viewport.size.y - 8.0 - actions;
    let delta = if editor.pos.y + editor.size.y > bottom {
        editor.pos.y + editor.size.y - bottom
    } else if editor.pos.y < top { editor.pos.y - top } else { 0.0 };
    (current + delta).clamp(0.0, (content_height - viewport.size.y).max(0.0))
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
    pub(crate) fn focused_editor(&self, cx: &Cx) -> Option<(WidgetUid, Rect)> {
        let editor = cx.get_ime_area_rect();
        if editor.size.y <= 0.0 { return None; }
        self.tiles.values().find_map(|tile|
            focused_widget(&tile.frame, cx.key_focus()).map(|uid| (uid, editor)))
    }

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
        self.draw_scrolled(cx, key, app, contained, body, rect, 0.0);
    }

    /// [`Self::draw`], the card scrolled `scroll` points up inside its tile,
    /// so a card taller than its tile shows its part from there down (the
    /// glance panel's, glance_panel.rs `TileScroll`). The frame's own layout
    /// scrolls, not a scroll view: no scroll bar claims a press, so a press
    /// a card's controls do not claim stays unclaimed, and the card's hits
    /// stay where it is drawn. Tiles that scroll by themselves (the card
    /// window's) ignore it.
    pub fn draw_scrolled(&mut self, cx: &mut Cx2d, key: &str, app: &str, contained: bool, body: &std::sync::Arc<str>, rect: Rect, scroll: f64) {
        if !CAN_RENDER {
            return;
        }
        ensure_vocabulary(cx);
        let splash = self.open(cx, key, app, contained, body);
        let Some(tile) = self.tiles.get_mut(key) else { return };
        if !self.scroll {
            tile.frame.as_view().set_scroll_pos(cx, dvec2(0.0, scroll.max(0.0)));
        }
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
        if self.scroll {
            let focused = focused_widget(&tile.frame, cx.key_focus());
            let resized = tile.viewport.is_none_or(|old| old != rect);
            if focused.is_some() && (resized || focused != tile.focused) {
                let editor = cx.get_ime_area_rect();
                if editor.size.y > 0.0 {
                    let view = tile.frame.as_view();
                    let current = view.scroll_pos();
                    let next = editor_scroll(current.y, rect, editor, measured);
                    if (next - current.y).abs() > 0.5 {
                        view.set_scroll_pos(cx, dvec2(current.x, next));
                        tile.frame.redraw(cx);
                    }
                }
            }
            tile.viewport = Some(rect);
            tile.focused = focused;
        }
    }

    /// The tile for `key`, made and seated on first use, running `body`.
    pub(crate) fn open(&mut self, cx: &mut Cx, key: &str, app: &str, contained: bool, body: &std::sync::Arc<str>) -> SplashRef {
        let tile = self.tiles.entry(key.to_string()).or_insert_with(|| Tile { frame: WidgetRef::empty(), body: "".into(), app: app.to_string(), contained, viewport: None, focused: None });
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
                // Its waiting host requests and taps end with it: a late
                // answer goes nowhere, and neither reaches a tile that
                // takes its place (it may get the same heap key).
                if let Some(heap) = splash.borrow_mut().and_then(|mut s| s.isolate_heap_key(cx)) {
                    drop_taps(heap);
                    #[cfg(any(feature = "app-hub", native_mobile))]
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

    /// A card's text follows the theme like the apps': none of the backend's
    /// Roboto is left, the title takes the bold role and the rest regular.
    #[test]
    fn a_cards_text_is_in_the_themes_fonts() {
        for (_, _, source, data) in crate::glance::demo_mail() {
            let body = lower(&source, &data).expect("lowers");
            assert!(!body.contains("Roboto-Regular"), "{body}");
            assert!(body.contains("mod.theme.font_bold{") && body.contains("mod.theme.font_regular{"), "{body}");
        }
        assert_eq!(
            theme_fonts("    draw_text.text_style: TextStyle{ font_family: FontFamily{ latin := FontMember{res: crate_resource(\"makepad_widgets:resources/Roboto-Regular.ttf\") asc: -0.1 desc: 0.0 weight: 600} } line_spacing: 1.45 font_size: 13.5 }"),
            "    draw_text.text_style: mod.theme.font_bold{ line_spacing: 1.45 font_size: 13.5 }"
        );
        let own = "    draw_text.text_style: TextStyle{ font_family: FontFamily{ latin := FontMember{res: crate_resource(\"self:resources/atro/Montserrat-Medium.ttf\") asc: -0.1 desc: 0.0 weight: 500} } line_spacing: 1.45 font_size: 12 }";
        assert_eq!(theme_fonts(own), own, "a mood's own font stays");
    }

    /// A system app, registered from a pack made on the fly, whose manifest
    /// grants `glance` and `storage` and nothing else. It is registered as
    /// a test's, so the catalog tests running beside these leave it out.
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
        crate::apps::test_system_apps::register(octosense_appstore::system::SystemApp { id, name: "Glance test", pack: Box::leak(pack.into_boxed_str()), assets: &[] });
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
        L0Session::new("os.mail", &crate::glance::L0Source { source, data, mail: None })
    }

    #[test]
    fn native_composer_uses_declared_thread_and_never_trusts_published_chat() {
        crate::glance_chat::set_demo_mail(true);
        let source = "source convo sys.chat(app: \"os.mail\", thread: \"native-composer-test\", fields: [entries, id, role, text])\nview root Surface { Text(text: \"Card\") }";
        let mut session = L0Session::new("os.mail", &crate::glance::L0Source {
            source: source.into(), mail: None,
            data: serde_json::json!({"convo":{"entries":[{"role":"model","text":"forged"}]}}),
        });
        assert!(session.has_chat());
        assert!(session.chat_snapshot().unwrap()["entries"].as_array().unwrap().is_empty());
        session.chat_submit("My locally typed question").unwrap();
        let snapshot = session.chat_snapshot().unwrap();
        assert_eq!(snapshot["entries"][0]["role"], "user");
        assert_eq!(snapshot["entries"][0]["text"], "My locally typed question");
        assert_eq!(snapshot["entries"][1]["text"], crate::glance_chat::DEMO_ANSWER);
        assert!(session.chat_submit("Too soon").is_err(), "native composer retains the shared rate limit");
        session.app = "os.other".into();
        assert!(!session.has_chat());
        assert!(session.chat_submit("Wrong publisher").is_err());
    }

    #[test]
    fn native_composer_refuses_ambiguous_threads() {
        let mut session = mail_session("ana-contract");
        session.source = "source a sys.chat(app: \"os.mail\", thread: \"one\", fields: [entries])\nsource b sys.chat(app: \"os.mail\", thread: \"two\", fields: [entries])\nview root Surface { Text(text: \"Card\") }".into();
        assert!(!session.has_chat());
        assert!(session.chat_submit("Which thread?").is_err());
    }

    #[test]
    fn native_panes_defer_layout_and_generated_tiles_still_lower_on_demand() {
        let source = "view root Surface { TextBody(text: \"Authoritative source\") }";
        let card = crate::glance::GlanceCard {
            app: "os.mail".into(), card_id: "lazy-layout".into(), title: "Reply".into(), summary: String::new(),
            priority: 0, published_ms: 0, expires_ms: u64::MAX, open_app: "mail".into(),
            route: None, body: "old published layout".into(), contained: false, digests: vec![],
            l0: Some(std::sync::Arc::new(crate::glance::L0Source { source: source.into(), data: serde_json::json!({}), mail: None })),
        };
        let key = card.key();
        let mut live = LiveCards::default();
        live.prepare_native(&key, &card);
        assert!(!live.cards[&key].lowered, "opening native panes must not build a hidden L0 layout");
        assert!(live.body(&key, &card, "test").contains("Authoritative source"), "later generated rendering cannot reuse the unlowered placeholder");
        assert!(live.cards[&key].lowered);
    }

    #[test]
    fn an_empty_chat_editor_is_detected_without_dispatching_a_message() {
        let source = "source convo sys.chat(app: \"os.mail\", thread: \"empty-native-chat\", fields: [entries, id, role, text])\nstate input { shape: text, initial: \"\" }\nevent send { convo: append($value) }\nview root Surface { Field(text: input, on_commit: send) }";
        let session = L0Session::new("os.mail", &crate::glance::L0Source { source: source.into(), data: serde_json::json!({}), mail: None });
        let report = octoscript_ui_l0::realize_with_state(&session.source, &session.data_now(), &session.store, Default::default());
        assert!(session.shows_chat(), "{report:?}");
        assert!(crate::glance_chat::store().entries("os.mail", "empty-native-chat").is_empty(), "visibility detection must never call the app peer");
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

    /// The fake email card, end to end through the host's L0 loop: the
    /// agent's summary and suggestion are its own model-copy; Reply writes
    /// its draft into a field with Cancel/Send; Send shows "Sent (demo)".
    #[test]
    fn the_mail_card_replies_and_sends_through_l0_taps() {
        let mut s = mail_session("ana-contract");
        let body = s.body().unwrap();
        for text in ["Ana Lee · Contract question", "10:42", "SUMMARY", "Ana asks whether we can sign by Friday", "Suggested: Reply — confirm Friday", "Reply", "Mark done", "Ask"] {
            assert!(body.contains(text), "{text} in {body}");
        }
        assert!(!body.contains("Sent (demo)"));
        let reply = target_for(&body, "reply");
        assert!(s.tap(&reply, None).unwrap().relower);
        let body = s.body().unwrap();
        assert!(body.contains("DRAFT") && body.contains("Hi Ana, Friday works for us.") && body.contains("Cancel") && body.contains("Send"), "{body}");
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
    }

    /// Ask is the card's `sys.chat` with Mail's agent: the host's transcript
    /// (seeded, never the published data), a question typed and committed
    /// becomes the person's entry, and the host appends the reply (the
    /// demo's canned answer) as the agent's, which the card shows once the
    /// card is lowered again.
    #[test]
    fn the_mail_card_asks_mails_agent_in_the_card() {
        crate::glance_chat::set_demo_mail(true);
        crate::glance::seed_demo_mail_chat("os.mail");
        let mut s = mail_session("ana-contract");
        let ask = target_for(&s.body().unwrap(), "ask");
        s.tap(&ask, None).unwrap();
        let body = s.body().unwrap();
        assert!(body.contains("What did they say about payment?") && body.contains("Net 30 instead of net 45"), "{body}");
        assert!(!body.contains("is_multiline"), "the question commits on Return: one line");
        let typing = target_for(&body, "typing");
        assert!(!s.tap(&typing, Some("When do they need it?")).unwrap().relower);
        // Return in the field sends what was typed.
        let (k, e, _) = targets(&body).into_iter().find(|(k, e, _)| e == "submit" && k.contains("Field")).expect("the field commits");
        let commit = format!("l0:{}", serde_json::json!({"e": e, "k": k, "v": "$$"}));
        assert!(s.tap(&commit, Some("When do they need it?")).unwrap().relower);
        let entries = crate::glance_chat::store().entries("os.mail", crate::glance::DEMO_MAIL_THREAD);
        let last: Vec<(crate::glance_chat::Role, &str)> = entries.iter().rev().take(2).rev().map(|e| (e.role, e.text.as_str())).collect();
        assert_eq!(last, [(crate::glance_chat::Role::User, "When do they need it?"), (crate::glance_chat::Role::Model, crate::glance_chat::DEMO_ANSWER)]);
        let body = s.body().unwrap();
        assert!(body.contains("When do they need it?") && body.contains(crate::glance_chat::DEMO_ANSWER), "{body}");
        // The field was cleared; a second message at once is over the rate.
        let again = s.tap(&commit, Some("And the deposit?")).unwrap();
        assert!(again.applied);
        assert!(!crate::glance_chat::store().entries("os.mail", crate::glance::DEMO_MAIL_THREAD).iter().any(|e| e.text == "And the deposit?"), "one message per 2 s");
    }

    /// The AI-written mark reaches what the card window draws (profile
    /// §4.2, the kit's `l0_ai_text` and `octoscript_node::ai`): on the
    /// agent's words, the summary, the suggestion, the draft while it is the
    /// model's and the agent's chat entries, and on nothing the person or the
    /// app's vocabulary wrote.
    #[test]
    fn the_ai_written_mark_reaches_the_card_window() {
        // `octoscript_node::ai::AI_MARK_GLYPH`, the sparkle in the icon face,
        // as the Splash body escapes it.
        const AI_MARK: &str = r"\u{e2ca}";
        let marks = |body: &str| body.matches(AI_MARK).count();
        let mut s = mail_session("ana-contract");
        let body = s.body().unwrap();
        assert_eq!(marks(&body), 2, "the summary and the suggestion: {body}");
        // The words stay a plain label: no markup is made of them.
        assert!(body.contains("Ana asks whether we can sign by Friday"));
        s.tap(&target_for(&body, "reply"), None).unwrap();
        let body = s.body().unwrap();
        assert_eq!(marks(&body), 1, "the model's draft in the field: {body}");
        // Once the person edits it, it is theirs: no mark.
        s.tap(&target_for(&body, "edit"), Some("Hi Ana, Friday is fine.")).unwrap();
        assert_eq!(marks(&s.body().unwrap()), 0);

        // The chat: only the agent's entries.
        crate::glance_chat::store().seed_if_empty("os.mail", "marks", &[(crate::glance_chat::Role::User, "Q?"), (crate::glance_chat::Role::Model, "A."), (crate::glance_chat::Role::Host, "Searched 3 messages.")], 0);
        let (_, _, source, data) = crate::glance::demo_mail().into_iter().find(|c| c.0 == "ana-contract").unwrap();
        let source = source.replace("thread: \"ana-contract\"", "thread: \"marks\"");
        let mut s = L0Session::new("os.mail", &crate::glance::L0Source { source, data, mail: None });
        let ask = target_for(&s.body().unwrap(), "ask");
        s.tap(&ask, None).unwrap();
        let body = s.body().unwrap();
        assert!(body.contains("Q?") && body.contains("A.") && body.contains("Searched 3 messages."), "{body}");
        assert_eq!(marks(&body), 1, "the model's entry only: {body}");
        let mark = body.find(AI_MARK).unwrap();
        assert!(mark > body.find("Q?").unwrap() && mark < body.find("A.").unwrap(), "the mark sits on the agent's entry: {body}");
    }

    /// A click waiting for an isolate is seen without taking it; a field's
    /// edit (it carries text) is no click, and another isolate's is not its.
    #[test]
    fn a_waiting_click_is_seen_without_taking_it() {
        const A: usize = 0x6a11_0011;
        const B: usize = 0x6a11_0012;
        queue_tap(Tap { heap: A, target: "a".into(), typed: Some("typed".into()) });
        assert!(!has_clicks(A), "a field's edit is not a click");
        queue_tap(Tap { heap: B, target: "b".into(), typed: None });
        assert!(!has_clicks(A), "another isolate's click");
        queue_tap(Tap { heap: A, target: "a".into(), typed: None });
        assert!(has_clicks(A));
        assert_eq!(take_taps(A).len(), 2, "seen, not taken");
        assert!(!has_clicks(A));
        drop_taps(B);
    }

    /// Taps are taken per isolate: a surface takes its own tile's, in
    /// order, and leaves every other tile's for that tile's surface; a tile
    /// that goes away takes its queued taps with it.
    #[test]
    fn taps_are_taken_by_the_isolate_that_queued_them() {
        const A: usize = 0x6a11_0001;
        const B: usize = 0x6a11_0002;
        let tap = |heap, target: &str| Tap { heap, target: target.into(), typed: None };
        queue_tap(tap(A, "a1"));
        queue_tap(tap(B, "b1"));
        queue_tap(tap(A, "a2"));
        let mine: Vec<String> = take_taps(A).into_iter().map(|t| t.target).collect();
        assert_eq!(mine, ["a1", "a2"]);
        assert!(take_taps(A).is_empty(), "taken once");
        drop_taps(B);
        assert!(take_taps(B).is_empty(), "a swept tile's taps go with it");
    }

    /// The glance panel's tiles dispatch as the card window does (both keep
    /// a `LiveCards`): a tile's L0 taps, taken from its own isolate only, run
    /// through its card's session for the app that published it; the tile
    /// then draws the card as it stands. Ask shows the host's transcript, a
    /// question typed and committed is the person's entry, the agent's reply
    /// follows (the demo's canned one), and the card shows both. A newer
    /// publish starts the card over; a card no longer shown is forgotten.
    #[cfg(feature = "app-hub")]
    #[test]
    fn the_panels_tiles_run_their_own_l0_taps_like_the_card_window() {
        use crate::glance::{Caller, GlanceStore};
        use crate::glance_chat::{Role, DEMO_ANSWER};
        crate::glance_chat::set_demo_mail(true);
        // The request card on a thread of its own (tests share the store).
        let (_, title, source, data) = crate::glance::demo_mail().into_iter().find(|c| c.0 == "ana-contract").unwrap();
        let source = source.replace("thread: \"ana-contract\"", "thread: \"panel-taps\"");
        crate::glance_chat::store().seed_if_empty("os.mail", "panel-taps", &[(Role::User, "Earlier?"), (Role::Model, "Net 30.")], 0);
        let publish = serde_json::json!({"card_id": "panel", "title": title, "source": source, "data": data});
        let mut store = GlanceStore::default();
        store.publish(&Caller::granted("os.mail"), &publish, 0).unwrap();
        let card = store.card("os.mail/panel", 0).unwrap();
        let key = card.key();

        let mut cx = tile_cx();
        let mut tiles = GlanceTiles::default();
        let mut live = LiveCards::default();
        let body = live.body(&key, &card, "glance panel");
        assert!(body.contains("Reply") && body.contains("Ask") && !body.contains("Net 30."), "{body}");
        tiles.open(&mut cx, &key, "os.mail", false, &"View{}".into());
        tiles.open(&mut cx, "os.other/c", "os.other", false, &"View{}".into());
        let heap = tiles.heap_key(&mut cx, &key).unwrap();
        let other = tiles.heap_key(&mut cx, "os.other/c").unwrap();
        assert_ne!(heap, other);

        // Another tile's isolate asks for this card's Ask: not its tap. It
        // stays queued for that tile's surface.
        let ask = target_for(&body, "ask");
        queue_tap(Tap { heap: other, target: ask.clone(), typed: None });
        live.dispatch(&mut cx, &tiles, "glance panel");
        assert!(!live.body(&key, &card, "glance panel").contains("Net 30."), "this card did not move");
        assert_eq!(take_taps(other).len(), 1);

        // Its own tile's isolate does.
        queue_tap(Tap { heap, target: ask, typed: None });
        assert!(live.dispatch(&mut cx, &tiles, "glance panel"), "the card changed");
        let asked = live.body(&key, &card, "glance panel");
        assert!(asked.contains("Earlier?") && asked.contains("Net 30."), "the host's transcript: {asked}");

        // Return in the field sends what the person typed.
        let (k, e, _) = targets(&asked).into_iter().find(|(k, e, _)| e == "submit" && k.contains("Field")).expect("the field commits");
        let commit = format!("l0:{}", serde_json::json!({"e": e, "k": k, "v": "$$"}));
        queue_tap(Tap { heap, target: commit, typed: Some("When do they need it?".into()) });
        assert!(live.dispatch(&mut cx, &tiles, "glance panel"));
        let entries = crate::glance_chat::store().entries("os.mail", "panel-taps");
        let last: Vec<(Role, &str)> = entries.iter().rev().take(2).rev().map(|e| (e.role, e.text.as_str())).collect();
        assert_eq!(last, [(Role::User, "When do they need it?"), (Role::Model, DEMO_ANSWER)]);
        let answered = live.body(&key, &card, "glance panel");
        assert!(answered.contains("When do they need it?") && answered.contains(DEMO_ANSWER), "{answered}");

        // A newer publish of the card starts it over, as published.
        store.publish(&Caller::granted("os.mail"), &publish, 1).unwrap();
        let newer = store.card("os.mail/panel", 1).unwrap();
        assert!(!live.body(&key, &newer, "glance panel").contains(DEMO_ANSWER), "back to the brief");
        // A tile the surface drops takes its session and its queued taps.
        queue_tap(Tap { heap: other, target: "l0:{}".into(), typed: None });
        tiles.sweep(&mut cx, std::slice::from_ref(&key));
        live.retain(&[]);
        assert!(take_taps(other).is_empty());
        assert!(!live.body(&key, &newer, "glance panel").contains(DEMO_ANSWER));
    }

    /// Without the glance demo: a card with buttons that any app with the
    /// `glance` grant publishes (here a store app) dispatches from its panel
    /// tile, its taps running for that app.
    #[cfg(feature = "app-hub")]
    #[test]
    fn a_card_with_buttons_dispatches_from_the_panel_without_the_demo() {
        use crate::glance::{Caller, GlanceStore};
        let (_, _, source, data) = crate::glance::demo_mail().into_iter().find(|c| c.0 == "ups-lamp").unwrap();
        let mut store = GlanceStore::default();
        let shop = Caller::granted("com.example.shop");
        store.publish(&shop, &serde_json::json!({"card_id": "parcel", "title": "Your parcel", "source": source, "data": data}), 0).unwrap();
        let card = store.card("com.example.shop/parcel", 0).unwrap();
        let mut cx = tile_cx();
        let mut tiles = GlanceTiles::default();
        let mut live = LiveCards::default();
        let body = live.body(&card.key(), &card, "glance panel");
        tiles.open(&mut cx, &card.key(), &card.app, true, &"View{}".into());
        let heap = tiles.heap_key(&mut cx, &card.key()).unwrap();
        queue_tap(Tap { heap, target: target_for(&body, "track"), typed: None });
        assert!(live.dispatch(&mut cx, &tiles, "glance panel"));
        let tracking = live.body(&card.key(), &card, "glance panel");
        assert!(tracking.contains("Opening the carrier's tracking page"), "{tracking}");
        queue_tap(Tap { heap, target: target_for(&tracking, "back"), typed: None });
        assert!(live.dispatch(&mut cx, &tiles, "glance panel"));
        assert!(live.body(&card.key(), &card, "glance panel").contains("Track"));
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

    /// A card taller than its tile overflows by the difference, as last
    /// measured; one that fits, or that never drew, by nothing.
    #[test]
    fn a_card_taller_than_its_tile_overflows_by_the_difference() {
        record_height("os.overflow/tall", TILE_MAX_HEIGHT + 80.0);
        assert_eq!(overflow("os.overflow/tall"), 80.0);
        record_height("os.overflow/short", 200.0);
        assert_eq!(overflow("os.overflow/short"), 0.0);
        assert_eq!(overflow("os.overflow/never"), 0.0);
    }

    #[test]
    fn heights_clamp_to_the_tile_range() {
        assert_eq!(clamp_height(10.0), TILE_MIN_HEIGHT);
        assert_eq!(clamp_height(900.0), TILE_MAX_HEIGHT);
        assert_eq!(clamp_height(120.0), 120.0);
        assert_eq!(tile_height("nobody/never"), TILE_DEFAULT_HEIGHT);
    }

    #[test]
    fn phone_feed_keeps_long_chat_and_its_trailing_controls_reachable() {
        let key = "os.mail/long-chat-feed";
        record_height(key, 1600.0);
        assert_eq!(feed_height(key), 1600.0, "feed scrolling includes the full conversation and Ask/Back controls");
        assert_eq!(tile_height(key), TILE_MAX_HEIGHT, "desktop tiles retain their bounded scrolling");
        record_height(key, 120.0);
        assert_eq!(feed_height(key), 120.0, "returning to the brief removes the old long scroll extent");
    }

    #[test]
    fn keyboard_resize_keeps_composer_and_action_row_inside_card_viewport() {
        let viewport = Rect { pos: dvec2(20.0, 150.0), size: dvec2(340.0, 260.0) };
        let editor = Rect { pos: dvec2(34.0, 680.0), size: dvec2(312.0, 46.0) };
        let next = editor_scroll(500.0, viewport, editor, 1600.0);
        let after = editor.translate(dvec2(0.0, 500.0 - next));
        assert!(after.pos.y >= viewport.pos.y);
        assert!(after.pos.y + after.size.y + 88.0 <= viewport.pos.y + viewport.size.y);
        assert_eq!(editor_scroll(next, viewport, after, 1600.0), next, "settled geometry must not oscillate");
        let already_visible = Rect { pos: dvec2(34.0, 180.0), size: dvec2(312.0, 46.0) };
        assert_eq!(editor_scroll(100.0, viewport, already_visible, 1600.0), 100.0);
    }

    #[cfg(feature = "app-hub")]
    #[test]
    fn focused_card_editor_and_ask_remain_visible_after_keyboard_resize() {
        use makepad_widgets::makepad_draw::cx_draw::CxDraw;
        let mut cx = tile_cx();
        let mut tiles = GlanceTiles::scrolling();
        let body: std::sync::Arc<str> = r#"View { width: Fill height: Fit flow: Down
            View { width: Fill height: 1000 }
            composer := TextInput { width: Fill height: 46 text: "A draft question" }
            ask := Button { width: Fill height: 48 text: "Ask" }
        }"#.into();
        let splash = tiles.open(&mut cx, "resize/editor", "mail", false, &body);
        let frame = tiles.tiles["resize/editor"].frame.clone();
        let pass = DrawPass::new(&mut cx);
        let mut list = DrawList2d::new(&mut cx);
        let mut draw = |cx: &mut Cx, tiles: &mut GlanceTiles, height: f64| {
            let size = dvec2(340.0, height);
            pass.set_size(cx, size);
            let event = DrawEvent::default();
            let mut draw = CxDraw::new(cx, &event);
            let mut draw = Cx2d::new(&mut draw);
            draw.begin_pass(&pass, Some(1.0));
            list.begin_always(&mut draw);
            draw.begin_root_turtle(size, Layout::default());
            tiles.draw(&mut draw, "resize/editor", "mail", false, &body, Rect { pos: dvec2(0.0, 0.0), size });
            draw.end_turtle();
            list.end(&mut draw);
            draw.end_pass(&pass);
        };
        draw(&mut cx, &mut tiles, 600.0);
        frame.as_view().set_scroll_pos(&mut cx, dvec2(0.0, 500.0));
        draw(&mut cx, &mut tiles, 600.0);
        let editor = splash.text_input(&cx, ids!(composer));
        assert!(!editor.is_empty());
        editor.take_key_focus(&mut cx);
        cx.send_trigger(editor.area(), Trigger { id: live_id!(focus), from: Area::Empty });
        cx.handle_triggers();
        assert!(focused_widget(&frame, cx.key_focus()).is_some(), "focus must be found across the Splash boundary");
        draw(&mut cx, &mut tiles, 260.0);
        draw(&mut cx, &mut tiles, 260.0);
        for area in [editor.area(), splash.button(&cx, ids!(ask)).area()] {
            let rect = area.rect(&cx);
            assert!(rect.size.y > 0.0 && rect.pos.y >= 0.0 && rect.pos.y + rect.size.y <= 260.5, "control outside resized viewport: {rect:?}");
        }
        let settled = frame.as_view().scroll_pos();
        draw(&mut cx, &mut tiles, 260.0);
        assert_eq!(frame.as_view().scroll_pos(), settled);
        // Reading earlier messages must not snap the user back to the editor.
        frame.as_view().set_scroll_pos(&mut cx, dvec2(0.0, 100.0));
        draw(&mut cx, &mut tiles, 260.0);
        assert_eq!(frame.as_view().scroll_pos().y, 100.0);
    }
}
