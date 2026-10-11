# Maps place-first implementation plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build the approved [place-first design](2026-10-06-maps-place-first-design.md): Maps becomes a map of places that can always be dragged and zoomed, with place cards, saved places, "What's here", and directions as one action on a place.

**Architecture:** A small opt-in addition to makepad's `MapView` gives scripts `fly_to`, `fit_route`, `clear_route` and the callbacks `on_tap`, `on_long_press`, `on_marker`, `on_viewport`. OctoSense carries it as a stacked runtime patch until the next makepad repin. Maps (`apps/maps/bundle/main.splash`) drops plan mode and uses one plain `MapView` for browsing, place cards and the route preview. It asks Photon and Overpass itself with `net.http_request` and keeps its place logic in pure functions that a shell unit test runs in a script VM.

**Tech stack:** Rust (makepad `widgets/src/map`), Makepad Splash (the app), Photon and Overpass (OpenStreetMap), OSRM through `sys.navroute` (unchanged).

---

## Ground rules for whoever runs this plan

- **Pull requests:** three are prepared (makepad, OctoSense, OctoScript-App-Design-Flow). Each is pushed and opened **only after the product owner approves that pull request**. Approval of one doesn't extend to the next. Stop at Tasks 6, 19 and 20 and ask.
- **Commits:** local commits on the feature branches are part of this plan. The commit identity follows AGENTS.md rule 8 (`git config user.email` must be a public address), and messages are factual, with no tool or assistant trailers.
- **Devices** (AGENTS.md rule 7): install only a separate test package; never replace the installed Home. Anything not run on a device stays **unverified**.
- **makepad's AGENTS.md** says not to add test code unless asked. The product owner asked for these tests in the approved design ("makepad: unit tests beside `MapView`'s own"). Say so in the makepad pull request.
- **PR #348** (the search list opens from the search box and closes from the map) is independent. If it merges before Task 12, rebase and follow Task 12's "if #348 is on main" notes.

Names used below (set them in your shell; never write their values into the repository):

```bash
OS=<this OctoSense checkout>                  # holds this plan
MAKEPAD=<your clone of OctoSense-org/makepad>  # the sources hub's makepad
MP=$MAKEPAD-map-script-api                     # a new worktree for the makepad change
```

Facts checked on 2026-10-06:
- The runtime lock (`runtime-patches.lock.json`, on main `90d5bef9`, 2026-10-07) is makepad `68d1f4ec` plus sixteen stacked patches. makepad `origin/main` (`dfd800e9`) adds four merged pull requests (camera, QR, video), none touching `widgets/src/map/`, and no runtime patch touches `widgets/src/map/` (one touches `widgets/src/widget_async.rs`; Task 7 checks that the stack still applies). So a diff made on `origin/main` applies to the locked runtime.
- `MapView` (`widgets/src/map/view.rs`) already emits `MapViewAction::{Tapped, LongPressed, MarkerClicked, ViewportChanged}` and has `fly_to(cx, lon, lat, zoom)`. Marker ids are 1-based positions in the parsed `route_markers` (`nav_update`).
- GestureView hands callbacks to scripts with `#[live] on_tap: ScriptFnRef` and `cx.widget_to_script_call(uid, NIL, source, fn, &args)` (`widgets/src/gesture_view.rs`).
- Splash strings: `split replace search strip_prefix url_encode to_chars to_f64 trim`; `replace` changes the first match only; there is no `to_upper` or `starts_with`. Strings may be single-quoted. Reading a missing field may raise, so the code reads with `try { o[k] } catch { nil }`. Math: `round floor sin cos asin sqrt radians`.
- `WebReader` opens any public https page only with the `web` grant (`widgets/src/web_reader.rs`). Maps doesn't hold `web` today, so **Website needs `web` added to Maps' manifest** (Task 14). News already holds it; YouTube's player host is on its own list.
- The shell's storage test (`crates/shell/src/app_storage/tests.rs`) requires every `fs.*` call outside `moved` in `apps/maps/bundle/main.splash` to take `data_path(` or `cache_path(` directly.
- Every system app's `main.splash` starts with a generated shared interface, between `// BEGIN shared app interface` and `// END shared app interface` (written by `tools/sync-app-interface.py`; never edit it by hand). It defines the theme's colours (`ui_ink`, `ui_page`, `ui_surface`, `ui_field`, `ui_muted`, `ui_link`, `ui_primary`, `ui_light`, …) and widgets (`UiButton`, `UiPrimary`, `UiPill`, `UiField` (48 high, margin 0), `UiCard`, `UiCaption`, `UiTitle`). Maps' `ink`, `secondary` and `accent` follow the theme, its `Panel` is `ui_surface`, its maps set `dark_theme: !ui_light`, and its small text is 13.
- Location is passive: `sys.gps(…)` only reads; only a person's action may call `sys.request_location()` (1: asked; 0: not available here; -1: no `location` grant). `crates/shell/src/module_resize_tests.rs`'s `maps_reads_are_passive_and_only_the_location_action_requests_permission` cuts `fix_origin`, `location_note` and `use_my_location` out of `main.splash` (each from `fn name(` to the next `\nfn `) and runs them with stubs. Keep those three functions as they are, and put only functions and comments right after each of them.

---

## Part A: makepad (`OctoSense-org/makepad`, branch `octosense/map-script-api`)

### Task 0: Worktree and baseline

**Step 1: Create the worktree from makepad's main**

```bash
git -C "$MAKEPAD" fetch origin
git -C "$MAKEPAD" worktree add -b octosense/map-script-api "$MP" origin/main
cd "$MP" && git config user.email   # must be a public address (rule 8)
```

**Step 2: Run the map tests as they are**

Run: `cargo test -p makepad-widgets --features maps --lib map::`
Expected: all pass. Note the count; later tasks add 6 tests.

### Task 1: One framing for plan mode and `fit_route`

Plan mode's framing moves into two free functions, so `fit_route` (Task 2) reuses it exactly and plan mode doesn't change.

**Files:**
- Modify: `widgets/src/map/view.rs` (`nav_plan_camera`, near the end of the file; tests in `mod tests`)

**Step 1: Write the failing test** (append inside `mod tests` in `view.rs`)

```rust
    fn bounds_of(a: Vec2d, b: Vec2d) -> (Vec2d, Vec2d) {
        (dvec2(a.x.min(b.x), a.y.min(b.y)), dvec2(a.x.max(b.x), a.y.max(b.y)))
    }

    #[test]
    fn plan_framing_fits_the_route_into_the_band_above_the_sheet() {
        let size = dvec2(400.0, 800.0);
        let bounds = bounds_of(
            lon_lat_to_normalized(-121.97, 37.376),
            lon_lat_to_normalized(-121.96, 37.370),
        );
        let zoom = plan_fit_zoom(bounds, size, 3.0, 18.0);
        let px = |d: f64| d * TILE_SIZE * zoom.exp2();
        assert!(px(bounds.1.x - bounds.0.x) <= size.x * 0.80 + 1e-6);
        assert!(px(bounds.1.y - bounds.0.y) <= size.y * 0.50 + 1e-6);
        // A few metres zoom no closer than 15.5; a continent no further than 10.
        let spot = lon_lat_to_normalized(-121.96, 37.37);
        assert_eq!(plan_fit_zoom((spot, spot + dvec2(1e-9, 1e-9)), size, 3.0, 18.0), 15.5);
        let wide = bounds_of(lon_lat_to_normalized(-125.0, 49.0), lon_lat_to_normalized(-70.0, 25.0));
        assert_eq!(plan_fit_zoom(wide, size, 3.0, 18.0), 10.0);
        // The route's centre sits at 30% of the height: the camera is below it.
        let world = tile_world_size_zoom(zoom);
        let c = (bounds.0 + bounds.1) * 0.5;
        let camera = plan_center(c, size.y, world);
        assert!(((camera.y - c.y) * world - 0.20 * size.y).abs() < 1e-6);
        assert_eq!(camera.x, c.x);
    }
```

**Step 2: Run it to see it fail**

Run: `cargo test -p makepad-widgets --features maps --lib plan_framing_fits`
Expected: compile error, `plan_fit_zoom` and `plan_center` not found.

**Step 3: Move the framing into the two functions**

In `view.rs`, after the `impl MapView` block that holds `nav_plan_camera` (the last block in the file), add:

```rust
/// The plan camera's zoom: `bounds` (normalized) fitted into the band a plan
/// card leaves above its summary sheet, 80% of the width and the top half of
/// the height. Plan mode reads it every frame, `fit_route()` once.
fn plan_fit_zoom(bounds: (Vec2d, Vec2d), size: Vec2d, min_zoom: f64, max_zoom: f64) -> f64 {
    let (min, max) = bounds;
    let dx = (max.x - min.x).max(1e-9);
    let dy = (max.y - min.y).max(1e-9);
    let fitw = size.x * 0.80;
    let fith = (size.y * 0.50).min(2.0 * (size.y * 0.30 - 52.0)).max(1.0);
    let zx = (fitw / (dx * TILE_SIZE)).log2();
    let zy = (fith / (dy * TILE_SIZE)).log2();
    let zmin = min_zoom.max(3.0);
    let zmax = max_zoom.max(zmin);
    zx.min(zy).clamp(zmin, zmax).clamp(10.0, 15.5)
}

/// The camera centre that puts `route_center` at 30% of the view's height.
fn plan_center(route_center: Vec2d, height: f64, world_size: f64) -> Vec2d {
    route_center + dvec2(0.0, (0.5 - 0.30) * height / world_size)
}
```

Replace the body of `nav_plan_camera` with:

```rust
    fn nav_plan_camera(&mut self, rect: Rect) {
        let Some(bounds) = self.nav.bounds() else {
            return; // nothing to frame — keep the current centre (no NaN)
        };
        let c = (bounds.0 + bounds.1) * 0.5;
        if !self.nav.user_adjusted {
            self.zoom = plan_fit_zoom(bounds, rect.size, self.min_zoom, self.max_zoom);
            self.nav.home_zoom = self.zoom;
        }
        self.nav.car = c;
        let world = tile_world_size_zoom(self.view_zoom());
        self.center_norm = plan_center(c, rect.size.y, world) + self.nav.pan;
        self.wrap_and_clamp_center();
        self.rotation = 0.0;
        self.tilt = 0.0;
        self.nav.release_puck(&mut self.overlay.puck);
    }
```

Keep its doc comment ("PLAN route-preview camera: …").

**Step 4: Run the map tests**

Run: `cargo test -p makepad-widgets --features maps --lib map::`
Expected: all pass, including the new test and `nav_modes_classify`.

**Step 5: Commit**

```bash
git add widgets/src/map/view.rs
git commit -m "map: plan mode's framing as two functions"
```

### Task 2: `fit_route()`

A script calls `fit_route()` right after `set_nav_polyline` and `set_route_markers`. The map adopts them on its next draw, so the call only marks a fit as pending; the next draw frames the route (or the pins, when there's no route) once, with a flight, and leaves the camera to the person.

**Files:**
- Modify: `widgets/src/map/view.rs` (`MapView` struct, `nav_update`, `impl MapView`, `mod tests`)

**Step 1: Write the failing test**

```rust
    fn l_route() -> String {
        super::super::encode_polyline5_for_test(&[
            (37.3700, -121.9700),
            (37.3700, -121.9600),
            (37.3760, -121.9600),
        ])
    }

    #[test]
    fn fit_route_frames_the_route_once_like_plan_mode() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut map = test_map(&mut cx);
        let rect = Rect { pos: dvec2(0.0, 0.0), size: dvec2(400.0, 800.0) };
        map.nav_polyline.as_mut_empty().push_str(&l_route());
        map.fit_route(&mut cx);
        map.nav_update(&mut cx, rect);
        let fly = map.fly.expect("fit_route starts a flight");
        let bounds = map.nav.bounds().unwrap();
        let zoom = plan_fit_zoom(bounds, rect.size, map.min_zoom, map.max_zoom);
        assert_eq!(fly.to_zoom, zoom);
        let expected = plan_center((bounds.0 + bounds.1) * 0.5, rect.size.y, tile_world_size_zoom(zoom));
        assert!((fly.to_center - expected).length() < 1e-9);
        // Once: the next frame leaves the camera alone.
        map.fly = None;
        map.nav_update(&mut cx, rect);
        assert!(map.fly.is_none(), "fit_route framed the route again");
    }
```

**Step 2: Run it to see it fail**

Run: `cargo test -p makepad-widgets --features maps --lib fit_route_frames`
Expected: compile error, no method `fit_route`.

**Step 3: Implement**

In the `MapView` struct, after `#[rust] nav: NavState,` add:

```rust
    /// `fit_route()` was called: the next draw frames the route once.
    #[rust]
    fit_route_pending: bool,
```

In `impl MapView` (next to `set_puck`), add:

```rust
    /// Frame the route and its pins once, as plan mode would, then leave the
    /// camera to the person. Takes effect on the next draw, after the map has
    /// adopted the route a script just set. A plain map only.
    pub fn fit_route(&mut self, cx: &mut Cx) {
        self.fit_route_pending = true;
        self.redraw(cx);
    }
```

In `nav_update`, between the `if self.nav.adopt(...) { ... }` block and `if kind == NavKind::Off {`, add:

```rust
        if self.fit_route_pending && kind == NavKind::Off {
            if let Some(bounds) = self.nav.bounds() {
                self.fit_route_pending = false;
                let zoom = plan_fit_zoom(bounds, rect.size, self.min_zoom, self.max_zoom);
                let world = tile_world_size_zoom(zoom);
                let (lon, lat) = normalized_to_lon_lat(plan_center((bounds.0 + bounds.1) * 0.5, rect.size.y, world));
                self.fly_to(cx, lon, lat, zoom);
            }
        }
```

In `script_call`, before `ScriptAsyncResult::MethodNotFound`, add:

```rust
        if method == live_id!(fit_route) {
            vm.with_cx_mut(|cx| self.fit_route(cx));
            return ScriptAsyncResult::Return(NIL);
        }
```

**Step 4: Run the map tests**

Run: `cargo test -p makepad-widgets --features maps --lib map::`
Expected: all pass.

**Step 5: Commit**

```bash
git add widgets/src/map/view.rs
git commit -m "map: fit_route frames a plain map's route once"
```

### Task 3: `clear_route()`

`set_nav_polyline("")` is ignored (so a card can't clear its route by accident), and nothing else removes a route. `clear_route()` does, and keeps the pins: `set_route_markers("")` already clears those.

**Files:**
- Modify: `widgets/src/map/view.rs`

**Step 1: Write the failing test**

```rust
    #[test]
    fn clear_route_removes_the_line_and_keeps_the_pins() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut map = test_map(&mut cx);
        let rect = Rect { pos: dvec2(0.0, 0.0), size: dvec2(400.0, 800.0) };
        map.nav_polyline.as_mut_empty().push_str(&l_route());
        map.route_markers.as_mut_empty().push_str("37.37,-121.97,0;37.376,-121.96,2");
        map.nav_update(&mut cx, rect);
        assert!(map.overlay.route.is_some());
        map.fit_route(&mut cx);
        map.clear_route(&mut cx);
        map.nav_update(&mut cx, rect);
        assert!(map.overlay.route.is_none(), "the route is still drawn");
        assert_eq!(map.overlay.markers.len(), 2, "the pins went with it");
        assert!(map.fly.is_none(), "a cleared route was still framed");
        // A new route draws again.
        map.nav_polyline.as_mut_empty().push_str(&l_route());
        map.nav_update(&mut cx, rect);
        assert!(map.overlay.route.is_some());
    }
```

**Step 2: Run it to see it fail**

Run: `cargo test -p makepad-widgets --features maps --lib clear_route_removes`
Expected: compile error, no method `clear_route`.

**Step 3: Implement**

In `impl MapView`, after `fit_route`:

```rust
    /// Remove the route line (and any fit still pending); the pins stay.
    pub fn clear_route(&mut self, cx: &mut Cx) {
        self.fit_route_pending = false;
        if !self.nav_polyline.as_ref().is_empty() {
            self.nav_polyline.as_mut_empty();
            self.redraw(cx);
        }
    }
```

In `script_call`, next to `fit_route`:

```rust
        if method == live_id!(clear_route) {
            vm.with_cx_mut(|cx| self.clear_route(cx));
            return ScriptAsyncResult::Return(NIL);
        }
```

**Step 4: Run the map tests**

Run: `cargo test -p makepad-widgets --features maps --lib map::`
Expected: all pass.

**Step 5: Commit**

```bash
git add widgets/src/map/view.rs
git commit -m "map: clear_route removes a route and keeps its pins"
```

### Task 4: `fly_to(lat, lon, zoom)` for scripts

Scripts pass latitude first, as `sys.gps` and Photon results read; the Rust method takes longitude first. The zoom is optional (the current zoom when left out). A 0,0 or out-of-range point is ignored, as `route_markers` ignores it.

**Files:**
- Modify: `widgets/src/map/view.rs` (`script_call`, `mod tests`)

**Step 1: Write the failing test**

```rust
    #[test]
    fn a_script_flies_the_map_lat_first() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut map = test_map(&mut cx);
        let call = |cx: &mut Cx, map: &mut MapView, values: &[f64]| {
            cx.with_vm(|vm| {
                let args = vm.bx.heap.new_object();
                vm.bx.heap.set_object_storage_vec2(args);
                let trap = vm.bx.threads.cur().trap.pass();
                for v in values {
                    vm.bx.heap.vec_push(args, NIL, (*v).into(), trap);
                }
                map.script_call(vm, live_id!(fly_to), args.into());
            });
        };
        call(&mut cx, &mut map, &[0.0, 0.0, 15.0]);
        assert!(map.fly.is_none(), "0,0 is no place");
        call(&mut cx, &mut map, &[37.3209796, -121.9486002, 15.0]);
        let fly = map.fly.expect("a flight");
        assert_eq!(fly.to_zoom, 15.0);
        let (lon, lat) = normalized_to_lon_lat(fly.to_center);
        assert!((lat - 37.3209796).abs() < 1e-6 && (lon + 121.9486002).abs() < 1e-6, "{lat},{lon}");
    }
```

**Step 2: Run it to see it fail**

Run: `cargo test -p makepad-widgets --features maps --lib a_script_flies`
Expected: FAIL, "a flight" (the method isn't found, so nothing flies).

**Step 3: Implement**

In `script_call`, next to `first_string_arg`, add:

```rust
        fn number_arg(vm: &mut ScriptVm, args: ScriptValue, index: usize) -> Option<f64> {
            let args_obj = args.as_object()?;
            let trap = vm.bx.threads.cur().trap.pass();
            let value = vm.bx.heap.vec_value(args_obj, index, trap);
            if value.is_err() {
                return None;
            }
            value.as_number().filter(|n| n.is_finite())
        }
```

and, with the other methods:

```rust
        if method == live_id!(fly_to) {
            // Latitude first, as scripts read places; zoom optional.
            let lat = number_arg(vm, args, 0);
            let lon = number_arg(vm, args, 1);
            let zoom = number_arg(vm, args, 2).unwrap_or_else(|| self.view_zoom());
            if let (Some(lat), Some(lon)) = (lat, lon) {
                if is_a_place(lat, lon) {
                    vm.with_cx_mut(|cx| self.fly_to(cx, lon, lat, zoom));
                }
            }
            return ScriptAsyncResult::Return(NIL);
        }
```

Update `script_call`'s doc comment:

```rust
    /// The script methods. Nav cards: `set_nav_polyline(s)`,
    /// `set_route_markers(s)`, `set_nav_recenter(_)`, `nav_zoom_by(delta)`,
    /// `nav_center_origin()` (see map/nav.rs). A plain map (no `nav_mode`):
    /// `fly_to(lat, lon, zoom)`, `fit_route()`, `clear_route()`.
```

**Step 4: Run the map tests**

Run: `cargo test -p makepad-widgets --features maps --lib map::`
Expected: all pass. If `cx.with_vm` refuses to nest `with_cx_mut`, call the Rust `fly_to` through a small `script_fly_to(&mut self, cx, lat, lon, zoom)` helper that `script_call` uses, test that helper instead, and note it in the pull request.

**Step 5: Commit**

```bash
git add widgets/src/map/view.rs
git commit -m "map: scripts can fly a plain map to a place"
```

### Task 5: Callbacks: `on_tap`, `on_long_press`, `on_marker`, `on_viewport`

Every action a script can hear goes out through one method. It queues the script's callback when one is set, then sends the widget action as before, so Rust hosts see no change.

**Files:**
- Modify: `widgets/src/map/view.rs` (imports, `MapViewAction` neighbourhood, `MapView` struct, the 7 emission sites, `mod tests`)

**Step 1: Write the failing tests**

```rust
    #[test]
    fn map_actions_reach_a_script_lat_first() {
        let tap = MapViewAction::Tapped { lon: -121.9, lat: 37.3, abs: dvec2(5.0, 6.0) };
        assert_eq!(map_script_event(&tap).map(|e| e.args()), Some(vec![37.3, -121.9]));
        let press = MapViewAction::LongPressed { lon: -121.9, lat: 37.3, abs: dvec2(5.0, 6.0) };
        assert_eq!(map_script_event(&press), Some(MapScriptEvent::LongPress { lat: 37.3, lon: -121.9 }));
        // Marker ids are 1-based positions in route_markers; scripts get the index.
        let pin = MapViewAction::MarkerClicked { id: 3 };
        assert_eq!(map_script_event(&pin).map(|e| e.args()), Some(vec![2.0]));
        assert_eq!(map_script_event(&MapViewAction::MarkerClicked { id: 0 }), None);
        let moved = MapViewAction::ViewportChanged { lon: -121.9, lat: 37.3, zoom: 14.5 };
        assert_eq!(map_script_event(&moved).map(|e| e.args()), Some(vec![37.3, -121.9, 14.5]));
        assert_eq!(map_script_event(&MapViewAction::TiltChanged { tilt: 30.0 }), None);
        let charger = MapViewAction::PinTapped { lon: -121.9, lat: 37.3, info: vec![] };
        assert_eq!(map_script_event(&charger), None);
    }

    #[test]
    fn a_map_without_callbacks_queues_nothing() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let map = test_map(&mut cx);
        for event in [
            MapScriptEvent::Tap { lat: 37.3, lon: -121.9 },
            MapScriptEvent::LongPress { lat: 37.3, lon: -121.9 },
            MapScriptEvent::Marker { index: 0.0 },
            MapScriptEvent::Viewport { lat: 37.3, lon: -121.9, zoom: 14.0 },
        ] {
            assert!(map.script_callback(event).is_none(), "{event:?}");
        }
    }
```

**Step 2: Run them to see them fail**

Run: `cargo test -p makepad-widgets --features maps --lib -- map_actions_reach a_map_without_callbacks`
Expected: compile errors, `map_script_event` and `MapScriptEvent` not found.

**Step 3: Implement**

Imports at the top of `view.rs`: change `use crate::widget_async::ScriptAsyncResult;` to

```rust
use crate::makepad_script::ScriptFnRef;
use crate::widget_async::{CxWidgetToScriptCallExt, ScriptAsyncResult};
```

After `enum MapViewAction { … }`, add:

```rust
/// What a script hears of a map action, latitude first as scripts read
/// places. The callbacks grant nothing: a tap reports the spot touched, and
/// the device's own position still comes only from `sys.gps`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum MapScriptEvent {
    Tap { lat: f64, lon: f64 },
    LongPress { lat: f64, lon: f64 },
    /// The pin's index in `route_markers` order.
    Marker { index: f64 },
    Viewport { lat: f64, lon: f64, zoom: f64 },
}

impl MapScriptEvent {
    fn args(&self) -> Vec<f64> {
        match *self {
            MapScriptEvent::Tap { lat, lon } | MapScriptEvent::LongPress { lat, lon } => vec![lat, lon],
            MapScriptEvent::Marker { index } => vec![index],
            MapScriptEvent::Viewport { lat, lon, zoom } => vec![lat, lon, zoom],
        }
    }
}

fn map_script_event(action: &MapViewAction) -> Option<MapScriptEvent> {
    match action {
        MapViewAction::Tapped { lon, lat, .. } => Some(MapScriptEvent::Tap { lat: *lat, lon: *lon }),
        MapViewAction::LongPressed { lon, lat, .. } => Some(MapScriptEvent::LongPress { lat: *lat, lon: *lon }),
        // Marker ids are 1-based positions in the pins the map was given.
        MapViewAction::MarkerClicked { id } if *id > 0 => Some(MapScriptEvent::Marker { index: (*id - 1) as f64 }),
        MapViewAction::ViewportChanged { lon, lat, zoom } => {
            Some(MapScriptEvent::Viewport { lat: *lat, lon: *lon, zoom: *zoom })
        }
        _ => None,
    }
}
```

In the `MapView` struct, after `route_badge`, add:

```rust
    /// Script callbacks, all optional; a map without them behaves as before.
    /// `on_tap: |lat, lon|`, `on_long_press: |lat, lon|` (a double click with
    /// a mouse), `on_marker: |index|` in `route_markers` order, and
    /// `on_viewport: |lat, lon, zoom|` once the camera settles.
    #[live]
    on_tap: ScriptFnRef,
    #[live]
    on_long_press: ScriptFnRef,
    #[live]
    on_marker: ScriptFnRef,
    #[live]
    on_viewport: ScriptFnRef,
```

In `impl MapView`, next to `emit_viewport_changed`:

```rust
    fn script_callback(&self, event: MapScriptEvent) -> Option<(ScriptFnRef, Vec<f64>)> {
        let script_fn = match event {
            MapScriptEvent::Tap { .. } => &self.on_tap,
            MapScriptEvent::LongPress { .. } => &self.on_long_press,
            MapScriptEvent::Marker { .. } => &self.on_marker,
            MapScriptEvent::Viewport { .. } => &self.on_viewport,
        };
        (script_fn.as_object() != ScriptObject::ZERO).then(|| (script_fn.clone(), event.args()))
    }

    /// Every action a script can hear goes out here: the script's callback,
    /// when it set one, is queued; then the widget action, as before.
    fn send_action(&mut self, cx: &mut Cx, action: MapViewAction) {
        if let Some((script_fn, args)) = map_script_event(&action).and_then(|e| self.script_callback(e)) {
            let args: Vec<ScriptValue> = args.into_iter().map(ScriptValue::from_f64).collect();
            cx.widget_to_script_call(self.uid, NIL, self.source.clone(), script_fn, &args);
        }
        cx.widget_action(self.uid, action);
    }
```

Replace `cx.widget_action(self.uid, X)` with `self.send_action(cx, X)` at exactly these sites (find them with `rg -n "MapViewAction::(Tapped|LongPressed|MarkerClicked|ViewportChanged)" widgets/src/map/view.rs`):
- `Hit::FingerLongPress` (LongPressed);
- `Hit::FingerUp`: the double-click LongPressed, MarkerClicked and Tapped;
- `handle_touch_tap`: MarkerClicked and Tapped;
- `emit_viewport_changed` (ViewportChanged).

Leave `PinTapped` and `TiltChanged` as they are. Leave `MapViewRef`'s `tapped()`, `long_pressed()` etc. (they read the actions).

Update `widgets/src/map/nav.rs`'s module comment: after the "script methods" bullet add

```rust
//! * for a plain map (no `nav_mode`) — `fly_to(lat, lon, zoom)`, `fit_route()`
//!   (frame the route once, as `"plan"` does, then leave the camera alone),
//!   `clear_route()`, and the callbacks `on_tap`, `on_long_press`,
//!   `on_marker`, `on_viewport` (see `MapView`).
```

**Step 4: Run the map tests**

Run: `cargo test -p makepad-widgets --features maps --lib map::`
Expected: all pass (the baseline count plus 6).

**Step 5: Commit**

```bash
git add widgets/src/map/view.rs widgets/src/map/nav.rs
git commit -m "map: on_tap, on_long_press, on_marker and on_viewport for scripts"
```

### Task 6: Check, squash and stop for approval

**Step 1: Check**

```bash
cargo check -p makepad-widgets --features maps 2>&1 | rg -c "^warning|^error"   # expect no output (0 matches)
cargo test -p makepad-widgets --features maps --lib map::
rustfmt --check --edition 2021 widgets/src/map/view.rs widgets/src/map/nav.rs
```

Expected: no warnings or errors; all map tests pass; rustfmt reports nothing. If rustfmt reformats lines this change didn't touch, format only the changed hunks.

**Step 2: Squash into one commit**

```bash
git reset --soft origin/main
git commit -m "map: a script can move a plain map and hear it"
```

**Step 3: Draft the pull request text** (in a scratch file, not the repository)

- Title: `map: a script can move a plain map and hear it`
- Body:
  - **Why:** OctoSense's Maps (a script app) becomes place-first. Its map must stay draggable while a route is shown, which plan mode doesn't allow. It also needs to move the camera to a place and hear taps, long presses and pin taps.
  - **What:** `fly_to(lat, lon, zoom)`, `fit_route()` (plan mode's framing, once), `clear_route()`; `on_tap`, `on_long_press`, `on_marker`, `on_viewport` (`ScriptFnRef`, queued with `widget_to_script_call` like GestureView's). Plan mode's framing moved into `plan_fit_zoom` and `plan_center` and is otherwise unchanged. Opt-in: no change for maps without the callbacks, for nav cards or for Rust hosts (widget actions still go out).
  - **Tests:** six unit tests beside `MapView`'s, at the request of OctoSense's product owner. Paste the test count and the `cargo check` result.
  - **Used by:** OctoSense's Maps pull request (linked once it is open).

**Step 4: Stop.** Show the product owner the diff summary and the text. Push and open the pull request **only after they approve it**:

```bash
git push -u origin octosense/map-script-api
gh pr create -R OctoSense-org/makepad --base main --head octosense/map-script-api --title "…" --body-file <scratch file>
```

Note the pull request number for Task 7.

---

## Part B: the runtime patch in OctoSense (branch `feat/maps-place-first`)

### Task 7: Stack the makepad change on the runtime

**Files:**
- Create: `tools/runtime-patches/makepad-map-script-api.patch`
- Modify: `runtime-patches.lock.json`

**Step 1: Write the patch**

```bash
cd "$OS"
git -C "$MP" diff origin/main...octosense/map-script-api -- widgets/src/map > tools/runtime-patches/makepad-map-script-api.patch
```

(Three dots: the diff from where the branch left makepad's main. If review changes the makepad pull request, write the patch again and redo Steps 2–7.)

**Step 2: Check that the runtime checkout is exactly the lock's**

```bash
python3 -c "import json; print(json.load(open('runtime-patches.lock.json'))['makepad']['tree'])"
git -C .sources/makepad write-tree
git -C .sources/makepad status --short | rg -v '^[MADR] ' | head   # expect nothing unstaged
```

Expected: the two trees are equal. If they differ, stop: `.sources/makepad` holds someone's work.

**Step 3: Apply the stack plus the new patch by hand and read the tree** (in bash: the loop relies on bash's word splitting)

```bash
BASE=$(python3 -c "import json; print(json.load(open('runtime-patches.lock.json'))['makepad']['base_revision'])")
git -C .sources/makepad reset -q --hard "$BASE"
for p in $(python3 -c "import json; m=json.load(open('runtime-patches.lock.json'))['makepad']; print(' '.join([m['patch']]+[s['patch'] for s in m['stacked']]))") tools/runtime-patches/makepad-map-script-api.patch; do
  git -C .sources/makepad apply --index "$PWD/$p" || { echo "FAILED: $p"; break; }
done
git -C .sources/makepad write-tree
shasum -a 256 tools/runtime-patches/makepad-map-script-api.patch
```

Expected: no FAILED line; note the new tree and the sha256.

**Step 4: Record them in the lock**

In `runtime-patches.lock.json` under `makepad`:
- set `"tree"` to the new tree;
- append to `"stacked"`:

  ```json
      {
        "patch": "tools/runtime-patches/makepad-map-script-api.patch",
        "sha256": "<the sha256>"
      }
  ```
- append to `"product_fixes"`:

  `"A script can move a plain map and hear it (makepad #<n>): fly_to, fit_route and clear_route, and the on_tap, on_long_press, on_marker and on_viewport callbacks on MapView; Maps' route preview stays draggable. Plan, follow and drive modes are unchanged."`

**Step 5: Let setup apply it the normal way and check**

```bash
git -C .sources/makepad reset -q --hard "$BASE"
python3 tools/setup.py --update
python3 tools/setup.py --check --cargo
```

Expected: both succeed. Setup applies the stack and compares the tree with the lock.

**Step 6: Build the desktop shell on the patched runtime**

Run: `cargo build --locked --release -p octosense`
Expected: builds.

**Step 7: Commit**

```bash
git add tools/runtime-patches/makepad-map-script-api.patch runtime-patches.lock.json
git commit -m "Stack makepad's MapView script calls and callbacks on the runtime"
```

---

## Part C: Maps (`apps/maps/bundle/`)

The app's place logic is pure functions defined between the shared interface and `start_timeout(` in `main.splash`. A shell unit test evaluates that part of the file in a script VM, as `photo_model` in `crates/shell/src/module_resize_tests.rs` does for Photos.

### Task 8: The model test harness and Photon results

**Files:**
- Create: `crates/shell/src/maps_model_tests.rs`
- Modify: `crates/shell/src/lib.rs` (register the module)
- Modify: `apps/maps/bundle/main.splash` (pure functions after `fn data_path`)

**Step 1: Register the test module**

In `crates/shell/src/lib.rs`, after `mod module_resize_tests;`:

```rust
#[cfg(test)]
mod maps_model_tests;
```

**Step 2: Write the harness and the failing tests**

`crates/shell/src/maps_model_tests.rs`:

```rust
//! Maps' place logic: the pure functions in `apps/maps/bundle/main.splash`
//! (everything between the shared interface and `start_timeout(`), run in a
//! script VM.
use makepad_widgets::*;

const MAPS: &str = include_str!("../../../apps/maps/bundle/main.splash");

/// Evaluate `expression` after Maps' functions; it must end in `.to_json()`.
fn maps_model(expression: &str) -> serde_json::Value {
    let source = MAPS.split_once("// END shared app interface\n").unwrap().1
        .split_once("\nstart_timeout(").unwrap().0;
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm {
        host: &mut host,
        bx: Box::new(ScriptVmBase::new()),
    };
    vm.bx.captured_errors = Some(Vec::new());
    let result = vm.with_instruction_limit(500_000, |vm| {
        vm.eval(ScriptMod {
            file: "maps_model_test.splash".into(),
            code: format!("use mod.math.*\n{source}\n{expression}\n;"),
            ..Default::default()
        })
    });
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    let json = vm.bx.heap.string_with(result, |_, value| value.to_string()).unwrap();
    serde_json::from_str(&json).unwrap()
}

/// Two hits for one way, an address, and a feature without coordinates. The
/// whole body goes into a single-quoted script string, so no `'` in it.
const PHOTON: &str = concat!(
    r#"{"type":"FeatureCollection","features":["#,
    r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[-121.9486002,37.3209796]},"properties":{"osm_type":"W","osm_id":25904339,"osm_key":"place","osm_value":"neighbourhood","name":"Santana Row","city":"San Jose","state":"California","country":"United States"}},"#,
    r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[-121.9486002,37.3209796]},"properties":{"osm_type":"W","osm_id":25904339,"osm_key":"place","osm_value":"neighbourhood","name":"Santana Row"}},"#,
    r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[-121.885,37.335]},"properties":{"osm_type":"N","osm_id":10735327671,"osm_key":"amenity","osm_value":"fast_food","housenumber":"10","street":"Market Street","city":"San Jose"}},"#,
    r#"{"type":"Feature","properties":{"name":"No geometry"}}"#,
    r#"]}"#
);

#[test]
fn maps_reads_photon_results_as_places() {
    let hits = maps_model(&format!("photon_hits('{PHOTON}').to_json()"));
    assert_eq!(
        hits,
        serde_json::json!([
            {"id": "W:25904339", "name": "Santana Row", "cat": "Neighbourhood",
             "label": "San Jose, California, United States", "lat": 37.3209796, "lon": -121.9486002},
            {"id": "N:10735327671", "name": "10 Market Street", "cat": "Fast food",
             "label": "Market Street, San Jose", "lat": 37.335, "lon": -121.885}
        ])
    );
}

#[test]
fn maps_tells_no_places_from_a_bad_answer() {
    let out = maps_model(r#"[photon_hits('{"features":[]}').len() photon_hits('<html>busy</html>') == nil].to_json()"#);
    assert_eq!(out, serde_json::json!([0, true]));
}

#[test]
fn maps_names_categories_like_a_person_would() {
    let out = maps_model(
        r#"[category("amenity", "fuel") category("building", "yes") category("shop", "ice_cream") category("amenity", "")].to_json()"#,
    );
    assert_eq!(out, serde_json::json!(["Gas station", "Building", "Ice cream", "Amenity"]));
}
```

(In Splash, call arguments take commas and array items take spaces.)

**Step 3: Run them to see them fail**

Run (from `phone/`): `cargo test --locked --features mobile-apps -p octosense-shell maps_model`
Expected: FAIL, script errors about `photon_hits` and `category` not found.

**Step 4: Add the functions** to `apps/maps/bundle/main.splash`, right after `fn data_path(name){ … }`:

```splash
fn cache_path(name){ return moved(name, "cache/" + name) }

// ---- Places: pure functions (crates/shell/src/maps_model_tests.rs runs them) ----

// A field that may be missing: reading a missing one can raise, not give nil.
fn optional(o, key, fallback){
    if o == nil { return fallback }
    let v = try { o[key] } catch { nil }
    if v == nil { return fallback }
    v
}
fn text_of(o, key){
    let v = optional(o, key, nil)
    if v == nil { return "" }
    ("" + v).trim()
}
// Every `a` in `s` replaced by `b` (a string's replace changes the first only).
fn replace_all(s, a, b){
    let out = ""
    for i part in s.split(a) {
        if i > 0 { out = out + b }
        out = out + part
    }
    out
}
let LOWER = "abcdefghijklmnopqrstuvwxyz"
let UPPER = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
fn cap(s){
    if s == "" { return s }
    let first = s.to_chars()[0]
    let i = LOWER.search(first)
    if i < 0 { return s }
    UPPER.to_chars()[i] + s.strip_prefix(first)
}
// OpenStreetMap's key and value as a category: a few renamed, the rest as
// written ("ice_cream" reads "Ice cream"); "yes" says only what the key is.
let CATEGORY = {fuel: "Gas station" atm: "ATM" doctors: "Doctor" townhall: "Town hall" house: "Address"}
fn category(key, value){
    let v = value
    if v == "" || v == "yes" { v = key }
    let named = optional(CATEGORY, v, nil)
    if named != nil { return named }
    cap(replace_all(v, "_", " "))
}
fn has_id(list, id){
    for h in list { if h.id == id { return true } }
    false
}
// Photon's GeoJSON as places {id name cat label lat lon}: nil when the answer
// is not Photon's (an error page), [] when nothing matched. `id` is
// OpenStreetMap's "N:123", "W:…" or "R:…", or "".
fn photon_hits(body){
    let feats = optional(body.parse_json(), "features", nil)
    if feats == nil { return nil }
    let out = []
    for f in feats {
        let p = optional(f, "properties", nil)
        let c = optional(optional(f, "geometry", nil), "coordinates", nil)
        if p == nil || c == nil || c.len() < 2 { continue }
        let name = text_of(p, "name")
        let street = text_of(p, "street")
        if name == "" && street != "" { name = (text_of(p, "housenumber") + " " + street).trim() }
        if name == "" { name = text_of(p, "city") }
        if name == "" { name = "Unnamed place" }
        let label = ""
        for k in ["street" "city" "state" "country"] {
            let v = text_of(p, k)
            if v != "" && v != name && label.search(v) < 0 {
                if label != "" { label = label + ", " }
                label = label + v
            }
        }
        let id = ""
        if text_of(p, "osm_type") != "" && text_of(p, "osm_id") != "" { id = text_of(p, "osm_type") + ":" + text_of(p, "osm_id") }
        if id != "" && has_id(out, id) { continue }
        out.push({id: id name: name cat: category(text_of(p, "osm_key"), text_of(p, "osm_value")) label: label lat: c[1] lon: c[0]})
    }
    out
}
```

**Step 5: Run the tests**

Run (from `phone/`): `cargo test --locked --features mobile-apps -p octosense-shell maps_model`
Expected: PASS. If an id reads `N:10735327671.0` or in exponent form, number-to-text isn't integral: build the id from `floor(optional(p, "osm_id", 0))` and add a `0` check, then rerun.

**Step 6: Commit**

```bash
git add crates/shell/src/lib.rs crates/shell/src/maps_model_tests.rs apps/maps/bundle/main.splash
git commit -m "Maps: read Photon's answer as places, with a model test"
```

### Task 9: Place details from Overpass

**Files:**
- Modify: `crates/shell/src/maps_model_tests.rs`, `apps/maps/bundle/main.splash`

**Step 1: Write the failing tests**

```rust
const OVERPASS: &str = r#"{"version":0.6,"elements":[{"type":"node","id":10735327671,"tags":{"amenity":"restaurant","name":"Pizza Place","opening_hours":"Mo-Su 11:00-22:00","contact:phone":"+1 408 555 0100","website":"http://pizza.example.com;https://other.example.com","cuisine":"pizza;italian_pizza"}}]}"#;

#[test]
fn maps_reads_a_places_hours_phone_website_and_cuisine() {
    let d = maps_model(&format!("place_details('{OVERPASS}').to_json()"));
    assert_eq!(
        d,
        serde_json::json!({"hours": "Mo-Su 11:00-22:00", "phone": "+1 408 555 0100",
            "website": "https://pizza.example.com", "cuisine": "pizza, italian pizza"})
    );
    let none = maps_model(r#"[place_details('{"elements":[]}') place_details('<html/>')].to_json()"#);
    let blank = serde_json::json!({"hours": "", "phone": "", "website": "", "cuisine": ""});
    assert_eq!(none, serde_json::json!([blank, blank]));
}

#[test]
fn maps_asks_overpass_only_for_openstreetmap_ids() {
    let q = maps_model(r#"[overpass_query("N:123") overpass_query("W:5") overpass_query("R:7") overpass_query("X:1") overpass_query("N:") overpass_query("N:1.5") overpass_query("") cache_name("W:5")].to_json()"#);
    assert_eq!(
        q,
        serde_json::json!([
            "[out:json][timeout:10];node(123);out tags;",
            "[out:json][timeout:10];way(5);out tags center;",
            "[out:json][timeout:10];rel(7);out tags center;",
            "", "", "", "",
            "place_W_5.json"
        ])
    );
}

#[test]
fn maps_opens_websites_over_https() {
    let u = maps_model(r#"[site_url("www.example.com") site_url("http://a.example.com") site_url("https://b.example.com") site_url("ftp://c.example.com") site_url("")].to_json()"#);
    assert_eq!(
        u,
        serde_json::json!(["https://www.example.com", "https://a.example.com", "https://b.example.com", "", ""])
    );
}
```

**Step 2: Run them to see them fail**

Run (from `phone/`): `cargo test --locked --features mobile-apps -p octosense-shell maps_model`
Expected: the three new tests FAIL (functions not found).

**Step 3: Add the functions** after `photon_hits` in `main.splash`:

```splash
// The Overpass query for one place's tags, or "" for an id that isn't
// OpenStreetMap's.
fn overpass_query(id){
    let parts = id.split(":")
    if parts.len() != 2 { return "" }
    let n = parts[1].to_f64()
    if !(n >= 0) || floor(n) != n { return "" }
    let kind = ""
    if parts[0] == "N" { kind = "node" } else if parts[0] == "W" { kind = "way" } else if parts[0] == "R" { kind = "rel" }
    if kind == "" { return "" }
    let out = "out tags center;"
    if kind == "node" { out = "out tags;" }
    "[out:json][timeout:10];" + kind + "(" + parts[1] + ");" + out
}
fn cache_name(id){ "place_" + replace_all(id, ":", "_") + ".json" }
fn blank_detail(){ return {hours: "" phone: "" website: "" cuisine: ""} }
// A website as the reader opens it: https, and the first of several.
fn site_url(s){
    if s == "" { return "" }
    let u = s.split(";")[0].trim()
    if u.search("https://") == 0 { return u }
    if u.search("http://") == 0 { return "https://" + u.strip_prefix("http://") }
    if u.search("://") >= 0 { return "" }
    if u == "" { return "" }
    "https://" + u
}
// Overpass's answer for one place, as the card's details.
fn place_details(body){
    let d = blank_detail()
    let els = optional(body.parse_json(), "elements", nil)
    if els == nil || els.len() == 0 { return d }
    let tags = optional(els[0], "tags", nil)
    if tags == nil { return d }
    d.hours = text_of(tags, "opening_hours")
    d.phone = text_of(tags, "phone")
    if d.phone == "" { d.phone = text_of(tags, "contact:phone") }
    d.website = site_url(text_of(tags, "website"))
    if d.website == "" { d.website = site_url(text_of(tags, "contact:website")) }
    d.cuisine = replace_all(replace_all(text_of(tags, "cuisine"), ";", ", "), "_", " ")
    d
}
```

**Step 4: Run the tests**

Run (from `phone/`): `cargo test --locked --features mobile-apps -p octosense-shell maps_model`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/shell/src/maps_model_tests.rs apps/maps/bundle/main.splash
git commit -m "Maps: read a place's OpenStreetMap details from Overpass"
```

### Task 10: Distance, coordinates and request addresses

**Files:**
- Modify: `crates/shell/src/maps_model_tests.rs`, `apps/maps/bundle/main.splash`

**Step 1: Write the failing tests**

```rust
#[test]
fn maps_says_how_far_a_place_is() {
    let km = maps_model("[distance_km(37.3350, -121.8850, 37.3209796, -121.9486002)].to_json()");
    let km = km[0].as_f64().unwrap();
    assert!((5.5..6.1).contains(&km), "{km} km downtown San Jose to Santana Row");
    let t = maps_model(r#"[distance_text(0.354) distance_text(2.44) distance_text(12.7) coords_text(37.33501, -121.88499)].to_json()"#);
    assert_eq!(t, serde_json::json!(["350 m", "2.4 km", "13 km", "37.335, -121.885"]));
}

#[test]
fn maps_asks_photon_near_the_visible_map() {
    let u = maps_model(r#"[search_url("Santana Row", 37.33501, -121.88499) reverse_url(37.33501, -121.88499)].to_json()"#);
    assert_eq!(
        u,
        serde_json::json!([
            "https://photon.komoot.io/api/?q=Santana%20Row&limit=8&lang=en&lat=37.335&lon=-121.885",
            "https://photon.komoot.io/reverse?lat=37.335&lon=-121.885&lang=en&limit=1"
        ])
    );
}
```

**Step 2: Run them to see them fail**

Run (from `phone/`): `cargo test --locked --features mobile-apps -p octosense-shell maps_model`
Expected: the two new tests FAIL.

**Step 3: Add the functions** after `place_details`:

```splash
fn round4(x){ round(x * 10000) / 10000 }
fn coords_text(lat, lon){ "" + round4(lat) + ", " + round4(lon) }
// Straight-line distance (km): Directions gives the road's.
fn distance_km(lat1, lon1, lat2, lon2){
    let dlat = radians(lat2 - lat1)
    let dlon = radians(lon2 - lon1)
    let a = sin(dlat / 2) * sin(dlat / 2) + cos(radians(lat1)) * cos(radians(lat2)) * sin(dlon / 2) * sin(dlon / 2)
    2 * 6371 * asin(sqrt(a))
}
fn distance_text(km){
    if km < 0.995 { return "" + round(km * 100) * 10 + " m" }
    if km < 10 { return "" + round(km * 10) / 10 + " km" }
    "" + round(km) + " km"
}
fn search_url(q, lat, lon){
    "https://photon.komoot.io/api/?q=" + q.url_encode() + "&limit=8&lang=en&lat=" + round4(lat) + "&lon=" + round4(lon)
}
fn reverse_url(lat, lon){
    "https://photon.komoot.io/reverse?lat=" + round4(lat) + "&lon=" + round4(lon) + "&lang=en&limit=1"
}
```

**Step 4: Run the tests**

Run (from `phone/`): `cargo test --locked --features mobile-apps -p octosense-shell maps_model`
Expected: PASS.

**Step 5: Commit**

```bash
git add crates/shell/src/maps_model_tests.rs apps/maps/bundle/main.splash
git commit -m "Maps: distances and Photon addresses near the visible map"
```

### Task 11: Saved places and pins

**Files:**
- Modify: `crates/shell/src/maps_model_tests.rs`, `apps/maps/bundle/main.splash`

**Step 1: Write the failing test**

```rust
#[test]
fn maps_saves_a_place_once_and_pins_the_open_one_last() {
    let out = maps_model(
        r#"let a = {id: "W:1" name: "A" cat: "" label: "" lat: 37.1 lon: -121.1}
let b = {name: "B" cat: "" label: "" lat: 37.2 lon: -121.2}
let s = with_saved(with_saved(with_saved([], a), b), a)
[s.len() is_saved(s, b) without_saved(s, a).len() pins_text(pin_places(s, b), b) pins_text(pin_places(s, nil), nil)].to_json()"#,
    );
    assert_eq!(
        out,
        serde_json::json!([2, true, 1, "37.1,-121.1,1;37.2,-121.2,2", "37.2,-121.2,1;37.1,-121.1,1"])
    );
}
```

**Step 2: Run it to see it fail**

Run (from `phone/`): `cargo test --locked --features mobile-apps -p octosense-shell maps_model`
Expected: the new test FAILS.

**Step 3: Add the functions** after `reverse_url`:

```splash
// What tells two places apart: OpenStreetMap's id, else the name at the point
// (a dropped pin, or a recent place from before ids were kept).
fn place_key(p){
    let id = text_of(p, "id")
    if id != "" { return id }
    p.name + "@" + round4(p.lat) + "," + round4(p.lon)
}
fn is_saved(list, p){
    let k = place_key(p)
    for s in list { if place_key(s) == k { return true } }
    false
}
fn without_saved(list, p){
    let k = place_key(p)
    let out = []
    for s in list { if place_key(s) != k { out.push(s) } }
    out
}
fn with_saved(list, p){
    let out = without_saved(list, p)
    out.push({id: text_of(p, "id") name: p.name cat: text_of(p, "cat") label: text_of(p, "label") lat: p.lat lon: p.lon})
    out
}
// The browse map's pins: saved places, then the open place once, last.
// Kind 1 is the stop colour, 2 the destination's (makepad's route pins).
fn pin_places(list, open){
    let out = []
    for s in list { if open == nil || place_key(s) != place_key(open) { out.push(s) } }
    if open != nil { out.push(open) }
    out
}
fn pins_text(pins, open){
    let out = ""
    for p in pins {
        let kind = 1
        if open != nil && place_key(p) == place_key(open) { kind = 2 }
        if out != "" { out = out + ";" }
        out = out + p.lat + "," + p.lon + "," + kind
    }
    out
}
```

**Step 4: Run the tests**

Run (from `phone/`): `cargo test --locked --features mobile-apps -p octosense-shell maps_model`
Expected: PASS (10 tests).

**Step 5: Commit**

```bash
git add crates/shell/src/maps_model_tests.rs apps/maps/bundle/main.splash
git commit -m "Maps: saved places and the browse map's pins"
```

### Task 12: One plain map: browse, close the list from the map, ◎

From here the tasks change the app's behaviour; each ends with a desktop check over `MAKEPAD_REMOTE` (AGENTS.md rule 6). Build once per task with `cargo build --locked --release -p octosense`. Run in a scratch home so the real profile stays untouched:

```bash
SCRATCH=$(mktemp -d)
OCTOSENSE_HOME=$SCRATCH MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=47315 target/release/octosense --test-action launch-maps &
# drive: /snap (filter ids and types yourself: a Splash card's text is its source), /click?x=&y=&wait=1,
# /t?t=<text>, /k?k=down&c=ReturnKey then /k?k=up&c=ReturnKey, /m?k=down|move|up&x=&y=, /g (screenshot); end with /gq
```

On a desktop a double click is the map's long press.

**Files:**
- Modify: `apps/maps/bundle/main.splash`

**Step 1: State.** Replace the `let` block right after the shared interface (`let origin = …` through `let view3d = …`) with:

```splash
let origin = {lat: 37.3350 lon: -121.8850 name: "San Jose (downtown)" picked: false}
let q = ""
let hits = []
let search_state = ""   // "" | "loading" | "done" | "failed"
let search_seq = 0      // the newest search: an older answer is dropped
let list_open = false
let place = nil
let detail = {hours: "" phone: "" website: "" cuisine: ""}
let card_seq = 0        // the open card: a lookup for an older one is dropped
let screen = "search"   // search | place | route | drive | origin | stop
let mode = "drive"
let recents = []
let saved = []
let shown_pins = []     // the places behind the browse map's pins, in pin order
let shown_route = ""
let finding = ""        // "" | "origin" | "stop": the search picks the start or a stop instead
let stops = []          // up to two stops on the way, in order
let view3d = true       // drive view: 3D follow camera, or the flat 2D map
let seen = {lat: 37.3350 lon: -121.8850}   // the browse map's centre (on_viewport)
let centered = false    // moved to the GPS fix already, or by the person
let OVERPASS = ["overpass-api.de" "overpass.kumi.systems" "overpass.openstreetmap.fr"]
let CACHE_DAYS = 7
```

**Step 2: The map.** Replace `plan_box := View{ plan_map := MapView{…} }` with:

```splash
    browse_box := View{width: Fill height: Fill
        browse_map := MapView{width: Fill height: Fill dark_theme: !ui_light zoom: 13.0 min_zoom: 3.0 max_zoom: 18.0
            center_lat: 37.3350 center_lon: -121.8850 archive_url: "https://makepad.nl/maps/world-20260926.mkmap" debug_cam: false
            on_tap: |lat, lon| map_tapped(lat, lon)
            on_long_press: |lat, lon| map_long_pressed(lat, lon)
            on_marker: |i| marker_tapped(i)
            on_viewport: |lat, lon, zoom| viewport_moved(lat, lon, zoom)}
    }
```

No `nav_mode`: this is a plain map, so it can always be dragged.

**Step 3: The search box and ◎.**

- Search box: if PR #348 is on main, it already has this wrapper; otherwise replace the bare `search := TextInput{…}` with it. Either way the `TextInput` gains `on_change`, so an emptied box shows Saved and Recent again:

  ```splash
              // A TextInput tells the script nothing about a tap, so a GestureView
              // with the field's size and margin sits under it: it still sees a
              // tap the field took (one inside it, a smaller widget, it would
              // leave alone).
              View{width: Fill height: Fit flow: Overlay
                  GestureView{width: Fill height: 48 margin: 0 on_tap: |x, y| show_results(true)}
                  search := UiField{width: Fill height: 48 empty_text: "Search Maps"
                      on_return: |text| search_for(text)
                      on_change: |text| { if ("" + text).trim() == "" && q != "" { search_for("") } }
                      draw_bg +: {…unchanged…}
                      draw_text +: {…unchanged…}
                  }
              }
  ```

  The `GestureView` matches `UiField`'s size and margin (48, 0). A fixed-height wrapper clips the field on a phone; keep `height: Fit` on the wrapper.
- Replace the spacer under the search panel (`View{width: Fill height: Fill}`, or #348's `map_tap := GestureView` inside it) with:

  ```splash
          // Over the map, it takes nothing but its button: the map under it
          // closes the list itself (on_tap, on_viewport).
          View{width: Fill height: Fill flow: Down align: Align{x: 1.0 y: 1.0}
              locate_box := View{visible: false width: Fit height: Fit padding: Inset{bottom: 8}
                  Pill{text: "◎" on_click: || locate()}
              }
          }
  ```

**Step 4: The list, and the map's callbacks.** Replace `fn show_results` (from #348; add it if absent) and add the map handlers after `use_my_location`:

```splash
// The list under the search box. On the search screen it is closed until the
// box is tapped or a search is made, and a tap or drag on the map closes it.
// It closes by hiding: a render that emits nothing keeps the rows it had.
// Choosing a start or a stop keeps its list open, with "‹ Back to route".
fn show_results(open){
    list_open = open
    let any = q != "" || saved.len() > 0 || recents.len() > 0
    ui.results.set_visible((open && any) || screen != "search")
}
fn map_tapped(lat, lon){
    if screen == "search" { show_results(false) }
}
fn viewport_moved(lat, lon, zoom){
    seen = {lat: lat lon: lon}
    centered = true
    if screen == "search" { show_results(false) }
}
// ◎ flies to the fix, or asks for one: asking is the person's act, so it
// happens only here and in use_my_location; every other read is passive.
fn locate(){
    if sys.gps("ok") >= 1 {
        centered = true
        ui.browse_map.fly_to(sys.gps("lat"), sys.gps("lon"), 15)
        return
    }
    centered = false   // center_on_fix flies there once the fix comes
    let request = sys.request_location()
    if request < 0 { ui.search_hint.set_text("Maps has no location access") }
    else if request == 0 { ui.search_hint.set_text("No location here") }
    else { ui.search_hint.set_text("Waiting for location…") }
}
// Maps opens at the GPS fix when one comes before the person moves the map.
fn center_on_fix(){
    if centered || screen != "search" || sys.gps("ok") < 1 { return }
    centered = true
    if finding == "" { ui.search_hint.set_text("Where to?") }
    ui.browse_map.fly_to(sys.gps("lat"), sys.gps("lon"), 14)
}
```

(`map_long_pressed` and `marker_tapped` come in Tasks 14–15; until then add `fn map_long_pressed(lat, lon){}` and `fn marker_tapped(i){}` so the file loads. Only functions and comments may follow `use_my_location`: main's location test cuts it out up to the next `\nfn `. A map reports its viewport only after a gesture, a zoom, `set_center` or a flight, never at start-up, so `centered` stays false until the person or ◎ moves it.)

**Step 5: `show` and `tick`.** In `show(s)`:
- `ui.plan_box.set_visible(…)` becomes `ui.browse_box.set_visible(s != "drive")`;
- add `ui.locate_box.set_visible(s == "search" && host.has("location"))`;
- before `tick()`, add `show_results(s != "search")` (if #348 isn't on main).

In `tick()`, call `center_on_fix()` after `fix_origin()`. Replace both `draw_route(ui.plan_map)` with `draw_route(ui.browse_map)` (Task 16 reworks routes).

**Step 6: Check on the desktop**

- At launch no list is open, and the map pans with a mouse drag (`/m` down, move 150 points, up; compare `/g` before and after).
- A click in the search box shows Recent; a click on the map closes it; so does a drag on the map. Seed one recent place before launching:

  ```bash
  mkdir -p "$SCRATCH/apps/os.maps/accounts/device"
  printf '%s' '[{"name":"Santana Row","cat":"Retail","label":"San Jose, California, United States","lat":37.3209796,"lon":-121.9486002}]' \
    > "$SCRATCH/apps/os.maps/accounts/device/recents.json"
  ```
- `/snap` shows `locate_box` only if the desktop reports the `location` grant; a click on ◎ there shows "No location here" (`sys.request_location()` is 0 off Android). On a device, Task 18 checks ◎.
- `cargo test --locked --features mobile-apps -p octosense-shell maps` (from `phone/`) still passes: main's location tests read `fix_origin`, `location_note` and `use_my_location` from the file.

**Step 7: Commit**

```bash
git add apps/maps/bundle/main.splash
git commit -m "Maps: one plain map; the map closes the search list itself"
```

### Task 13: Search near the visible map; Saved and Recent

**Files:**
- Modify: `apps/maps/bundle/main.splash`

**Step 1: Fetching.** After `cache_path`, before the pure functions' heading, add:

```splash
fn fetch(url){
    let p = promise()
    net.http_request(net.HttpRequest{url: url method: net.HttpMethod.GET headers: {"User-Agent": "OctoSense-Maps/1.0"}}) do net.HttpEvents{
        on_response: |res| p.resolve(res)
        on_error: |_err| p.resolve(nil)
    }
    p
}
fn body_of(res){
    if res == nil || res.status_code >= 400 || res.body == nil { return nil }
    res.body.to_string()
}
```

**Step 2: Search.** Replace `search_for` and delete `read_hits` (and its call in `tick`) and `loaded_q`:

```splash
fn search_for(text){
    q = ("" + text).trim()
    hits = []
    search_seq = search_seq + 1
    let mine = search_seq
    if q == "" { search_state = "" } else { search_state = "loading" }
    show_results(true)
    ui.results.render()
    if q == "" { return }
    let body = body_of(fetch(search_url(q, seen.lat, seen.lon)).await())
    if mine != search_seq { return }   // a newer search took over
    let found = nil
    if body != nil { found = photon_hits(body) }
    if found == nil { search_state = "failed" } else { hits = found; search_state = "done" }
    show_results(list_open)
    ui.results.render()
}
```

**Step 3: Saved.** After `load_recents`, add:

```splash
fn load_saved(){
    if fs.exists(data_path("saved.json")) {
        let v = fs.read(data_path("saved.json")).parse_json()
        if v != nil { saved = v }
    }
}
```

Call `load_saved()` in `boot()` after `load_recents()`.

**Step 4: The list.** Replace the `results` view's `on_render` body:

```splash
            results := View{width: Fill height: Fit flow: Down on_render: || {
                if screen == "origin" { Link{text: "◎  Your location" on_click: || use_my_location()} }
                if screen == "origin" || screen == "stop" { Link{text: "‹ Back to route" on_click: || { finding = ""; show("route") }} }
                if search_state == "loading" { Label{text: "Searching…" draw_text.color: secondary draw_text.text_style.font_size: 13} }
                else if search_state == "failed" { Label{text: "Search isn't available right now." draw_text.color: secondary draw_text.text_style.font_size: 13} }
                else if q != "" && hits.len() == 0 { Label{text: "No places found." draw_text.color: secondary draw_text.text_style.font_size: 13} }
                for i h in hits { if i < 6 { HitRow{on_tap: |x, y| pick(h) name.text: h.name label.text: h.label} } }
                if q == "" && saved.len() > 0 {
                    Label{text: "Saved" draw_text.color: secondary draw_text.text_style.font_size: 13}
                    for s in saved { HitRow{on_tap: |x, y| pick(s) name.text: s.name label.text: s.label} }
                }
                if q == "" && recents.len() > 0 {
                    Label{text: "Recent" draw_text.color: secondary draw_text.text_style.font_size: 13}
                    for r in recents { HitRow{on_tap: |x, y| pick(r) name.text: r.name label.text: r.label} }
                }
            }}
```

**Step 5: Check on the desktop**

- Search "Santana Row" (type, Return): "Searching…", then rows. Pan the map to New York (several drags, or a search and pick there), search "Pizza": the first rows are near the visible map, not near San Jose.
- With the network off (or Photon unreachable), a search shows "Search isn't available right now."
- Clearing the box (select all, Delete) shows Recent again.

**Step 6: Commit**

```bash
git add apps/maps/bundle/main.splash
git commit -m "Maps: search near the visible map; Saved above Recent"
```

### Task 14: The place card: details, Save, pins and Website

**Files:**
- Modify: `apps/maps/bundle/main.splash`, `apps/maps/bundle/manifest.json`

**Step 1: The `web` grant.** In `manifest.json`, add `"web"` to `capabilities` after `"location"`. Without it `WebReader` opens only hosts on the list, and a place's site isn't.

**Step 2: Opening and closing a card.** Replace `pick`'s last four lines (`place = p`, `stops = []`, `remember(p)`, `show("place")`) with `remember(p)` and `open_place(p)`, and add:

```splash
fn open_card(){
    card_seq = card_seq + 1
    detail = blank_detail()
    stops = []
    centered = true
    show("place")
}
fn open_place(p){
    place = p
    open_card()
    ui.browse_map.fly_to(p.lat, p.lon, 16)
    load_details(p, card_seq)
}
fn close_place(){
    card_seq = card_seq + 1
    place = nil
    show("search")
}
fn marker_tapped(i){
    if screen != "search" && screen != "place" { return }
    if i < 0 || i >= shown_pins.len() { return }
    let p = shown_pins[i]
    if place != nil && place_key(p) == place_key(place) { return }
    open_place(p)
}
```

**Step 3: Details.** Add:

```splash
fn cached_detail(id){
    if !fs.exists(cache_path(cache_name(id))) { return nil }
    let v = fs.read(cache_path(cache_name(id))).parse_json()
    let at = optional(v, "at", 0)
    if !(at > 0) || time_now() - at > CACHE_DAYS * 86400 { return nil }
    let d = optional(v, "detail", nil)
    if d == nil { return nil }
    return {hours: text_of(d, "hours") phone: text_of(d, "phone") website: text_of(d, "website") cuisine: text_of(d, "cuisine")}
}
// One Overpass request per opened place, on the three declared mirrors in
// turn, kept for a week. Without an answer the card shows Photon's data only.
fn load_details(p, mine){
    let query = overpass_query(text_of(p, "id"))
    if query == "" { return }
    let cached = cached_detail(p.id)
    if cached != nil { detail = cached; render_details(); return }
    for host in OVERPASS {
        let body = body_of(fetch("https://" + host + "/api/interpreter?data=" + query.url_encode()).await())
        if mine != card_seq { return }   // the card moved on
        if body != nil && body.search("\"elements\"") >= 0 {
            detail = place_details(body)
            fs.write(cache_path(cache_name(p.id)), {at: time_now() detail: detail}.to_json())
            render_details()
            return
        }
    }
}
fn has_details(){ detail.hours != "" || detail.phone != "" || detail.website != "" || detail.cuisine != "" }
// Hidden when empty: a render that emits nothing keeps the last place's rows.
fn render_details(){
    ui.details.set_visible(has_details())
    if has_details() { ui.details.render() }
}
fn place_distance(){
    if sys.gps("ok") < 1 { return "" }
    distance_text(distance_km(sys.gps("lat"), sys.gps("lon"), place.lat, place.lon)) + " away"
}
fn fill_place(){
    ui.pname.set_text(place.name)
    ui.pcat.set_text(place.cat)
    ui.paddr.set_text(place.label)
    ui.pdist.set_text(place_distance())
    sync_save()
    render_details()
}
```

**Step 4: Save and pins.** Add:

```splash
fn sync_save(){
    if place != nil && is_saved(saved, place) { ui.save.set_text("Saved") } else { ui.save.set_text("Save") }
}
fn toggle_save(){
    if place == nil { return }
    if is_saved(saved, place) { saved = without_saved(saved, place) } else { saved = with_saved(saved, place) }
    fs.write(data_path("saved.json"), saved.to_json())
    sync_save()
    show_pins()
}
// Saved places, and the open one, as pins on the browse map. Directions
// and the drive show only the route's own pins.
fn show_pins(){
    if screen != "search" && screen != "place" { return }
    let open = nil
    if screen == "place" { open = place }
    shown_pins = pin_places(saved, open)
    ui.browse_map.set_route_markers(pins_text(shown_pins, open))
}
```

**Step 5: Website.** Add:

```splash
fn open_site(url){
    ui.site_fail.set_text("")
    ui.site_title.set_text(place.name)
    ui.site_pane.set_visible(true)
    ui.site.open(url)
}
fn close_site(){
    ui.site.close()
    ui.site_pane.set_visible(false)
}
```

and, as the last child of the root `View{… flow: Overlay …}` (so it covers everything):

```splash
    site_pane := View{visible: false width: Fill height: Fill flow: Down new_batch: true show_bg: true draw_bg.color: ui_page
        View{width: Fill height: 52 flow: Right align: Align{y: 0.5} padding: Inset{left: 4 right: 8} spacing: 4
            Link{text: "‹ Maps" on_click: || close_site()}
            site_title := Label{width: Fill text: "" max_lines: 1 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 14}}
        }
        site_fail := Label{width: Fill padding: Inset{left: 18 right: 18} text: "" draw_text.color: secondary draw_text.text_style.font_size: 13}
        site := WebReader{width: Fill height: Fill on_error: || ui.site_fail.set_text(ui.site.error())}
    }
```

**Step 6: The card's layout.** Replace `place_panel`:

```splash
        place_panel := Panel{visible: false
            View{width: Fill height: Fit flow: Right align: Align{y: 0.5}
                pname := Label{width: Fill text: "" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 20}}
                Link{text: "Close" on_click: || close_place()}
            }
            pcat := Label{width: Fill text: "" draw_text.color: #xe37400 draw_text.text_style.font_size: 13}
            paddr := Label{width: Fill text: "" draw_text.color: secondary draw_text.text_style.font_size: 13}
            pdist := Label{width: Fill text: "" draw_text.color: #x188038 draw_text.text_style.font_size: 13}
            details := View{visible: false width: Fill height: Fit flow: Down spacing: 2 on_render: || {
                if detail.hours != "" { Label{width: Fill text: "Hours  " + detail.hours draw_text.color: ink draw_text.text_style.font_size: 13} }
                if detail.phone != "" { Label{width: Fill text: "Phone  " + detail.phone draw_text.color: ink draw_text.text_style.font_size: 13} }
                if detail.cuisine != "" { Label{width: Fill text: "Cuisine  " + detail.cuisine draw_text.color: ink draw_text.text_style.font_size: 13} }
                if detail.website != "" { Link{text: "Website" on_click: || open_site(detail.website)} }
            }}
            View{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                Primary{text: "Directions" on_click: || show("route")}
                save := Pill{text: "Save" on_click: || toggle_save()}
            }
        }
```

The panel is `ui_surface`, and `ink` and `secondary` follow the theme, so the card reads in dark mode too; the orange category and green distance are the colours main already uses for `pcat` and `peta`.

**Step 7: `show` and `tick`.** In `show(s)`: replace the `if s == "place" { … }` block with `if s == "place" { fill_place() }`, and call `show_pins()` just before `tick()`. In `tick()`, replace the `screen == "place"` block (ETA and route) with:

```splash
    if screen == "place" { ui.pdist.set_text(place_distance()) }
```

**Step 8: Check on the desktop**

- Pick "Santana Row": the map flies there; the card shows name, category and address. Pick a mapped restaurant (search "Pizza My Heart Santa Clara"): hours, phone, cuisine and **Website** appear within a few seconds. The second open of the same place reads `$SCRATCH/apps/os.maps/cache/place_*.json` (no new request; the file's mtime is unchanged).
- **Save**, **Close**: the saved place stays as a pin; a click on the pin opens its card. Relaunch: Saved is listed above Recent, and the pin is there.
- **Website** opens the reader with the place's site; **‹ Maps** returns to the card.
- `cargo test --locked --features mobile-apps -p octosense-shell should_keep_every_system_apps_files_in_its_account_folder_or_cache` (from `phone/`) passes.

**Step 9: Commit**

```bash
git add apps/maps/bundle/main.splash apps/maps/bundle/manifest.json
git commit -m "Maps: place cards with OpenStreetMap details, Save, pins and the website"
```

### Task 15: What's here

**Files:**
- Modify: `apps/maps/bundle/main.splash`

**Step 1: Replace the placeholder `map_long_pressed`**

```splash
// A long press drops a pin and asks Photon what is there. The card keeps the
// pressed point: it is where Directions goes.
fn map_long_pressed(lat, lon){
    if screen != "search" && screen != "place" { return }
    place = {id: "" name: "Dropped pin" cat: "" label: coords_text(lat, lon) lat: lat lon: lon}
    open_card()
    let mine = card_seq
    let body = body_of(fetch(reverse_url(lat, lon)).await())
    if mine != card_seq || body == nil { return }
    let found = photon_hits(body)
    if found == nil || found.len() == 0 { return }
    let h = found[0]
    place = {id: h.id name: h.name cat: h.cat label: h.label lat: lat lon: lon}
    fill_place()
    show_pins()
    load_details(place, mine)
}
```

**Step 2: Check on the desktop**

Double-click an empty spot of the map (two `/m` down/up pairs within 0.3 s): a pin and a "Dropped pin" card with coordinates, then a name or address. With Photon unreachable, the card stays "Dropped pin". **Directions** from it routes to the pressed point.

**Step 3: Commit**

```bash
git add apps/maps/bundle/main.splash
git commit -m "Maps: a long press shows what is there"
```

### Task 16: Directions on the same map

**Files:**
- Modify: `apps/maps/bundle/main.splash`

**Step 1: Routes frame once and clear when you leave.** Replace `draw_route`:

```splash
fn draw_route(map, fit){
    let key = route_key()
    if shown_route == key { return }
    let line = sys.navroute(origin.lat, origin.lon, place.lat, place.lon, "polyline", vias())
    // "—" while the route loads, "n/a" once it failed: not a line yet.
    if line == "" || line == "—" || line == "n/a" { return }
    map.set_nav_polyline(line)
    map.set_route_markers(markers())
    if fit { map.fit_route() }
    shown_route = key
}
```

In `tick()`: the route screen calls `draw_route(ui.browse_map, true)`, the drive screen `draw_route(active_drive_map(), false)`.

In `show(s)`, after the `set_visible` lines, add:

```splash
    // The route belongs to Directions and the drive; elsewhere the map is a
    // map of places.
    if s == "search" || s == "place" { ui.browse_map.clear_route() }
```

**Step 2: Check on the desktop**

- Open a place, **Directions**: the route is drawn and framed above the panel. Drag the map: it **moves** (the freeze is gone). Change Drive/Walk/Bike: the new route is framed once.
- **‹ Back**: the route is gone; the place's pin is there. **Close**: no route on the search screen; the map pans.
- **Start** and **End**: the drive view and its 2D/3D switch behave as on main.

**Step 3: Commit**

```bash
git add apps/maps/bundle/main.splash
git commit -m "Maps: directions on the browse map, framed once and draggable"
```

### Task 17: README rows (English and Chinese)

**Files:**
- Modify: `apps/README.md` (Maps row in the apps table), `apps/README.zh-CN.md` (same row)

Main changed both READMEs on 2026-10-07: read the current Maps rows first and keep every cell this task doesn't name.

**Step 1: English row** (replace the description and capabilities cells; hosts and notice unchanged):

- Description: `` `MapView` map of places that can always be dragged and zoomed: search near the visible area, place cards with OpenStreetMap details (hours, phone, website, cuisine), saved places as pins, a long press for "What's here", directions with a changeable start and up to two stops, and a drive mode with turn-by-turn and a 2D/3D view; starts at the device's GPS fix when there is one; the browse map draws makepad's pre-baked world map (`makepad.nl`), the drive maps and the place details read OpenStreetMap through Overpass ``
- Capabilities: `` `storage`, `net`, `location`, `web`, `glance` ``

**Step 2: Chinese row**

- Description: `` 随时可拖动和缩放的 `MapView` 地点地图：按可见区域搜索、带 OpenStreetMap 详情（营业时间、电话、网站、菜系）的地点卡片、以图钉显示的收藏地点、长按查看"这里是什么"、可更改起点并最多添加两个途经点的路线，以及带逐向导航和 2D/3D 视图的驾驶模式；有 GPS 定位时从当前位置开始；浏览地图使用 makepad 预先烘焙的世界地图（`makepad.nl`），驾驶地图和地点详情通过 Overpass 读取 OpenStreetMap ``
- Capabilities: `` `storage`、`net`、`location`、`web`、`glance` ``

**Step 3: Device notes.** Task 18 adds the dated result under "Maps" in both READMEs' verification list, next to the 2026-09-27 OnePlus 6 entry. Mark anything not run **unverified**.

**Step 4: Commit**

```bash
git add apps/README.md apps/README.zh-CN.md
git commit -m "apps README: Maps is place-first and holds web"
```

### Task 18: Full checks and devices

**Step 1: Repository checks**

```bash
python3 tools/setup.py --check --cargo
cargo check --locked -p octosense
(cd phone && cargo check --locked -p octosense-home --features mobile-apps)
(cd phone && cargo test --locked --features mobile-apps -p octosense-shell maps_model)
(cd phone && cargo test --locked --features mobile-apps -p octosense-shell app_storage)
(cd rom && python3 -m unittest tests.test_no_local_paths)
tools/ci-local.sh --only apps
```

Expected: all pass. Record each result for the pull request.

**Step 2: Pixel 7 Pro (dark mode), as a test package** (from `phone/`):

```bash
../.sources/makepad/target/release/cargo-makepad makepad android \
  --sdk-path="$HOME/.local/share/octosense/android-tools/makepad-android-sdk" \
  --package-name=dev.makepad.octosense.mapsfix build -p octosense-home --release
adb install -r target/android/makepad-android-apk/octosense_home/apk/octo_sense.apk
adb shell am start -n dev.makepad.octosense.mapsfix/.MakepadApp
```

Grant location to the test package. Walk Tasks 12–16's checks by touch, including ◎, a long press for What's here, a two-finger zoom in Directions, and dark-mode legibility of the card and list. On the Pixel the search is submitted with the keyboard's ✓.

**Step 3: OnePlus 6T (light mode)**, only if it is given for this task: the same checks, same package name. Otherwise it stays **unverified**.

**Step 4: README notes.** Add the dated device results (en and zh-CN) as Task 17 Step 3 says, then commit:

```bash
git add apps/README.md apps/README.zh-CN.md
git commit -m "apps README: Maps on the Pixel 7 Pro"
```

### Task 19: Prepare the OctoSense pull request and stop

**Step 1:** Rebase on `origin/main` (`git fetch origin && git rebase origin/main`). If #348 merged in the meantime, resolve its search list code in favour of this branch (its `map_tap` catcher is gone; the search-box `GestureView` stays). Rerun Task 18 Step 1.

**Step 2: Draft the pull request text** (scratch file): title `Maps: a map of places, with directions as one action`. In the body:
- **Behaviour:** what the person sees, per screen.
- **Runtime:** the stacked patch and its makepad pull request.
- **Grant:** `web`, for the Website action.
- **Storage:** `saved.json` in the account folder; details cached under `cache/` for 7 days.
- **Tests:** the commands and results from Task 18.
- **Devices:** what ran where; the rest **unverified**.

Link the design and this plan.

**Step 3: Stop.** Ask the product owner to approve this pull request. After approval:

```bash
git push -u origin feat/maps-place-first
gh pr create -R OctoSense-org/OctoSense --base main --head feat/maps-place-first --title "…" --body-file <scratch file>
```

---

## Part D: OctoScript-App-Design-Flow (`docs/SCRIPT-API.md`)

### Task 20: Document MapView's new calls and events, and stop

Do this once the makepad pull request has merged; the page describes the runtime OctoSense pins.

**Step 1:** In a branch `docs/mapview-script-api` from `origin/main` of the Design-Flow repository:
- Widgets table, `MapView` row: `` map. Nav cards: `set_nav_polyline` etc. A plain map: `fly_to(lat, lon, zoom)`, `fit_route()`, `clear_route()`, `set_route_markers(s)` (Maps) `` (gate unchanged).
- Events table: add `` `on_tap: |lat, lon|`, `on_long_press: |lat, lon|` `` (a double click with a mouse), `` `on_marker: |index|` `` (in `set_route_markers` order) and `` `on_viewport: |lat, lon, zoom|` `` (once the camera settles), widget `MapView`, example "Maps".

Mark them **✓ run** only where Task 18 ran them.

**Step 2:** Commit (`docs: MapView's plain-map calls and events`), draft the pull request text, and **stop** for the product owner's approval before pushing and opening it.

---

## Execution notes (2026-10-07 and 2026-10-08)

Where the build differs from the tasks above. Each task was reviewed, and the reviews' findings were fixed with tests.

**Rebase.** The branch moved to main `dba19330` after #348 merged. Main repinned makepad to `32d6415f`; the makepad branch was rebased there with no overlap, and the MapView patch was stacked last on main's stack.

**makepad (Part A).**
- `plan_fit_zoom` takes the lowest zoom: plan mode keeps 10, `fit_route` may zoom out to `min_zoom` (at least 3) so a long route fits whole.
- `fit_route` turns the view north-up and flat, waits for a draw with an area, does nothing on a nav-mode map, and is dropped if it arrives during a gesture.
- `clear_route` extends the existing Rust method: it also forgets a script route at once, its vehicle and a pending fit.
- Script `fly_to`: no zoom keeps the flight's target zoom; a later `fly_to` drops a pending fit; a call during the person's drag is ignored (a flight started mid-drag would land after the release and undo it).
- `on_marker` counts only the pins drawn from `route_markers` (unreadable, out-of-range and 0,0 entries are skipped).
- A wheel that a scroll view already used no longer zooms the map: one drawn over it (Maps' scrolling list) or one it sits in (a feed).

**Model (Tasks 8–11).**
- `cap` works on code points. A label compares whole address parts and doesn't repeat the street; only road `highway` values read "Street".
- Every read of network or disk data goes through `optional`/`text_of`/`number_of`. `optional` resolves inside its `try`, because `parse_json` keeps a bare `inf` or `NaN` as a name and using one outside raises.
- `photon_hits` keeps only features with real coordinates.
- `place_details` returns nil for an answer that isn't Overpass's, or for an Overpass error. Nil means the next mirror and nothing cached; a blank detail is a real "no details".
- `site_url` lets only http(s) out, always as https. It refuses control characters, spaces, a backslash, and `%` or anything but ASCII in the host. Unicode IDN hosts give no link.
- `distance_km` clamps the haversine term (f32 math gave NaN by the pole); `distance_text` picks its unit after rounding.
- Pins mirror makepad's `is_a_place`, so a pin tap's index finds its place. Saved and Recent files load only the places that can be shown.

**UI (Tasks 12–16).**
- Maps knows its own flights' landings by target (`app_target`, both axes within 1e-6°), not a one-shot flag; a tap that stops a flight reports no viewport.
- Panels claim the presses no child took, so a tap or drag on a card or the list doesn't reach the map. The drive bar doesn't, so a drag there still pans the drive map.
- The list is a capped scroll view, with "◎ Your location" and "‹ Back to route" kept above it.
- Every request gives up after 15 s: Android waits for good on a stalled server.
- The details view declares no `visible: false`: a render re-applies the declaration and would hide it again.
- Directions frames once on entering, on a new mode and on a new start, and frames a failed route's pins. A start that follows the GPS fix isn't re-framed on every tick. The old line is cleared when the pins change.
- A long press keeps the pressed point and doesn't move the map. A Save before Photon's answer keeps one saved entry, under the answer's name.
- `web` added to Maps' manifest for the Website reader. Only News held it before; the design said YouTube too.
- A card's flight puts the place 160 points above the map's centre (`card_centre`). On the Pixel 7 Pro a card with details reached past the middle, and the place's pin was hidden under it.

**Known limitations.**
- A route that arrives after the person has panned is still framed once.
- A ◎ or card flight asked for during a drag doesn't happen.
- Maps can't read the map's turn: on a map the person has turned, a card's place lands 160 points off the centre along that turn.
- On a desktop, a wheel over a card or Directions still zooms the map, and a long press is a double click whose first click is also a tap.
- The Website reader checks only the first URL; links and redirects are followed (as in News).
- No Call action: the phone number is text.
