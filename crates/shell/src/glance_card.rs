//! A published glance card on screen: the L0 card lowered the way App Hub's
//! Card runner lowers `page.card` (`octoscript_makepad::l0::prepare`, then
//! `design::to_makepad_ui`), drawn by a contained `Splash` isolate at the
//! glance tile size.
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
//! cap: a glance card is a summary, the app is one tap away.
//!
//! **Containment.** Each tile's isolate runs under an enforced policy with no
//! capabilities and no hosts (`Splash::set_policy`): it can make no host
//! request and fetch nothing, and a budget bounds its instructions. The tile
//! takes no input; the whole tile is one tap target the shell owns (open the
//! app), so a card's own events never run on the glance screen.
use makepad_widgets::*;
use std::cell::RefCell;
use std::collections::HashMap;

pub const TILE_MIN_HEIGHT: f64 = 72.0;
pub const TILE_MAX_HEIGHT: f64 = 260.0;
pub const TILE_DEFAULT_HEIGHT: f64 = 148.0;
/// Script instructions one tile's isolate may run over its life.
const TILE_INSTRUCTION_BUDGET: u64 = 5_000_000;

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
}

/// The live tiles one surface draws, by card key. A surface keeps one of
/// these; tiles for cards no longer shown are dropped (their isolates
/// stopped) at the end of each frame.
#[derive(Default)]
pub struct GlanceTiles {
    tiles: HashMap<String, Tile>,
}

impl GlanceTiles {
    /// Draw `card` (lowered `body`) at `rect`: the rect's height is the tile
    /// height; the Splash lays out at its natural height, and that height is
    /// recorded for the next layout. Asks for a redraw when it changed.
    pub fn draw(&mut self, cx: &mut Cx2d, key: &str, body: &std::sync::Arc<str>, rect: Rect) {
        if !CAN_RENDER {
            return;
        }
        let tile = self.tiles.entry(key.to_string()).or_insert_with(|| Tile { frame: WidgetRef::empty(), body: "".into() });
        if tile.frame.is_empty() {
            ensure_vocabulary(cx);
            tile.frame = cx.with_vm(|vm| {
                let value = script_eval!(vm, { use mod.widgets.* GlanceTileFrame {} });
                WidgetRef::script_from_value(vm, value)
            });
        }
        let splash = tile.frame.splash(cx, ids!(card));
        if tile.body.as_ref() != body.as_ref() {
            if let Some(mut s) = splash.borrow_mut() {
                // No capabilities, no hosts, a bounded budget: see the module docs.
                s.set_policy(cx, Some(Vec::new()), Some(TILE_INSTRUCTION_BUDGET));
            }
            splash.set_text(cx, body);
            tile.body = body.clone();
        }
        let walk = Walk { abs_pos: Some(rect.pos), width: Size::Fixed(rect.size.x), height: Size::Fixed(rect.size.y), ..Walk::default() };
        let mut scope = Scope::empty();
        tile.frame.draw_walk_all(cx, &mut scope, walk);
        let measured = splash.area().rect(cx).size.y;
        if measured > 1.0 && record_height(key, measured) {
            cx.redraw_all();
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

    #[test]
    fn heights_clamp_to_the_tile_range() {
        assert_eq!(clamp_height(10.0), TILE_MIN_HEIGHT);
        assert_eq!(clamp_height(900.0), TILE_MAX_HEIGHT);
        assert_eq!(clamp_height(120.0), 120.0);
        assert_eq!(tile_height("nobody/never"), TILE_DEFAULT_HEIGHT);
    }
}
