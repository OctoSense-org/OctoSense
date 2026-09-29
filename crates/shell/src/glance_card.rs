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
    let report = octoscript_ui_l0::realize(source, data, Default::default());
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
    let kit_source = [PALETTE_BASE, delta, DERIVE_COLOR, DERIVE, KIT, &octoscript_ui_l0::kit::lower(root)].join("\n");
    let tree = octoscript_makepad::design::prepare(&kit_source)?;
    // A measured design (an imported artboard) lowers as the Card runner
    // lowers it; a kit-composed card (columns, rows, text) through the
    // backend's general translation.
    let ui = octoscript_makepad::design::to_makepad_ui(&tree).unwrap_or_else(|_| octoscript_makepad::to_makepad_ui(&tree));
    // A card's page fills its screen; a tile measures it instead. The root's
    // own properties are the only lines at this indentation.
    let ui = ui.replacen("\n    height: Fill\n", "\n    height: Fit\n", 1);
    Ok(format!("width:Fill height:Fit flow:Overlay {ui}"))
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
}

impl GlanceTiles {
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
    fn open(&mut self, cx: &mut Cx, key: &str, app: &str, contained: bool, body: &std::sync::Arc<str>) -> SplashRef {
        let tile = self.tiles.entry(key.to_string()).or_insert_with(|| Tile { frame: WidgetRef::empty(), body: "".into(), app: app.to_string(), contained });
        if tile.frame.is_empty() {
            tile.frame = cx.with_vm(|vm| {
                let value = script_eval!(vm, { use mod.widgets.* GlanceTileFrame {} });
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
                tile.frame.splash(cx, ids!(card)).set_text(cx, "");
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
/// pack, any other from the last verified catalog.
#[cfg(any(feature = "app-hub", native_mobile))]
pub fn app_isolate(cx: &Cx, app: &str) -> Result<octosense_app_policy::IsolateSettings, String> {
    let root = octosense_appstore::data_root(cx);
    let policy = match octosense_appstore::system::system_app(app) {
        Some(system) => octosense_appstore::system::prepare(&root, &system)?.1,
        None => {
            let anchor = std::env::var("OCTOSENSE_HUB_ANCHOR").unwrap_or_else(|_| octosense_appstore::DEFAULT_ANCHOR.to_string());
            let mut store = octosense_app_hub::Store::new(&anchor, &root, octosense_app_policy::HostLimits::default());
            let catalog = std::fs::read_to_string(root.join("catalog.json")).unwrap_or_default();
            store.accept_catalog(&catalog).map_err(|e| format!("no verified catalog on this device ({e})"))?;
            store.may_run(app)?
        }
    };
    let settings = policy.isolate_settings(&root);
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
        let digest = octosense_app_policy::digest_dir(&dir).unwrap();
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

    #[test]
    fn heights_clamp_to_the_tile_range() {
        assert_eq!(clamp_height(10.0), TILE_MIN_HEIGHT);
        assert_eq!(clamp_height(900.0), TILE_MAX_HEIGHT);
        assert_eq!(clamp_height(120.0), 120.0);
        assert_eq!(tile_height("nobody/never"), TILE_DEFAULT_HEIGHT);
    }
}
