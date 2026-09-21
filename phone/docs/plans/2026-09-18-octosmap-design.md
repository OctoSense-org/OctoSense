# OctosMap

A maps app for OctoSense with the interface of Google Maps or Apple Maps: a
full-screen map under a search bar, a place sheet that slides up from the
bottom, directions by car, on foot or by bike, and turn-by-turn navigation.
It follows the News app's shape: one crate that is a standalone Makepad
window, a process-hosted tile in the desktop catalog, and an in-process
`AppModule` that phones link automatically.

The reference implementation is the framework's `apps/route`
(`../makepad/apps/route`, the same app as `~/git/mp/makepad/apps/route`: three
files differ, none in behaviour). OctosMap takes its map, its navigation
session and its location handling from there, and replaces its interface,
which is a desktop assistant panel with no search bar, no sheet and no
responsive layout.

Decisions taken in the design conversation on 2026-09-18:

1. **Data plane: hybrid, global.** The base map is the hosted vector archive
   the reference already falls back to. Search, reverse geocoding and routing
   use the keyless public OpenStreetMap services the `nav` AppCard already
   uses, because the reference's own search and routing API covers Europe
   only (probed on 2026-09-18: `Santa Clara` finds nothing, San Jose to San
   Francisco answers `no route found`, Amsterdam works).
2. **Interface: native, from the shared kit.** One persistent `MapView` and
   panels written in the Makepad DSL, in the shell's skin. Not L0 cards: the
   AppCards repo's own review of 2026-09-05
   (`docs/reviews/nav-upstream-map-comparison-2026-09-05.md`) found that a
   card state change regenerates the widget tree and tears the map down, and
   recommends a persistent native map whose properties are updated.
3. **Scope of v1:** the map, search, the place sheet, directions in three
   modes, the locate, compass and layers buttons, and turn-by-turn navigation
   with a simulated drive. Saved places and recents, nearby category chips,
   the home tile and the assistant's tools are not in v1 (see Out of scope).
4. **No framework changes.** Everything is built in this repository on the
   pinned framework revision. `MapView` has no fit-to-bounds, so the app
   computes the camera for a route itself; its markers are coloured pins with
   no icon or label, which is enough for a selected place and a route's ends.
5. **Verification:** desktop first (tests, the standalone window, the app
   hosted in the phone shell on macOS), then a build on the OnePlus 6T for
   the GPS fix, the touch gestures and the soft keyboard.
6. **Git:** `feat/octosmap`, one commit per completed task, nothing pushed.

## Crate layout

`apps/maps`, package `octosense-maps`, module id `maps`, label `OctosMap`, a
workspace member.

| File | Owns |
|---|---|
| `src/lib.rs` | Re-exports; `pub mod` list; `MAPS_MODULE` |
| `src/geo.rs` | Distances and bearings, the polyline5 decoder, a bounding box, the camera that fits a box into the part of the viewport the panels leave free, distance and duration text in metric and imperial |
| `src/places.rs` | Photon: the search and reverse URLs, GeoJSON into `Place` rows, the address and category lines |
| `src/routing.rs` | OSRM: the URL for a mode, the response into the framework's `Route` plus the step list the sheet shows, the maneuver mapping |
| `src/guidance.rs` | `ActiveNav`: the framework's `NavSession` around a route, the simulated drive, one `NavTick` per position (puck, camera, banner, remaining time), the reroute rule. Ported from `apps/route/src/nav.rs` |
| `src/sheet.rs` | The bottom sheet's detents (peek, half, full): a drag into a height, a release into a snap |
| `src/model.rs` | `MapsModel`: the screen state machine, the selected place, origin and destination, a route per mode, request bookkeeping and supersession, `back`; `Skin`; the persisted state |
| `src/view.rs` | `MapsView` widget: the DSL and the events |
| `src/module.rs` | `impl AppModule for MapsModule`, `MapsExecutor` |
| `src/main.rs` | Standalone window: `--phone`, `--dark`, `--light`, `--at lat,lon` |
| `src/test_support.rs` | The isolate scaffolding the view and module tests share, as News has |
| `resources/icons/*.svg` | The interface's icons and the maneuver arrows |
| `tests/fixtures/*.json` | Saved Photon and OSRM replies |

Dependencies, all at the pinned framework revision: `makepad-widgets` with
`features = ["maps"]`, `makepad-app-module`, `makepad-wm-theme`,
`makepad-wm-api` (the standalone binary's title and close), and
`makepad-map-nav`. The last is new to this workspace: it has no dependencies
of its own, its `Route` and `Maneuver` have public fields and
`NavSession::new(route)` is public, so a route built from an OSRM reply gets
the reference's map matching, off-route detection and arrival for nothing.
The root `[patch]` table gains a line for it. `makepad-app-route` itself is
not a dependency: its tools take `&mut RouteView`, and it pulls the local
model, voice and bake stacks. JSON is parsed with the framework's
`makepad_micro_serde`, as News does.

## Host wiring

- Root `Cargo.toml`: `octosense-maps = { path = "apps/maps", optional = true }`,
  feature `app-maps = ["dep:octosense-maps"]`, `mobile-apps` gains `app-maps`,
  the Android/iOS target table links it unconditionally, `apps/maps` joins
  `workspace.members`, and `[patch]` gains
  `makepad-map-nav = { path = "../makepad/libs/map_nav" }`.
- `src/apps.rs::linked_modules()` pushes `octosense_maps::MAPS_MODULE` under
  `cfg(any(feature = "app-maps", target_os = "android", target_os = "ios"))`;
  the bundled-apps test's id list gains `maps`.
- `config/apps.json` and `config/apps.makepad.json` gain a `maps` entry:
  manifest `../apps/maps/Cargo.toml`, package and bin `octosense-maps`, policy
  `focus`.
- `src/mobile_tiles.rs`: `maps` joins a `LIBRARY_GROUPS` category. No
  `TILE_APPS` entry: v1 has no home tile, so the app opens from the library.
- `src/shell/launcher.rs::icon_for` and `src/desktop.rs::app_icon` gain a
  `maps` arm.
- `resources/apple/Info.plist` gains `NSLocationWhenInUseUsageDescription`.
  Android already declares both location permissions in the manifest the
  build tool writes.

Capabilities declared by the module: `storage`, `net`, `location`.

## Data plane

| Need | Service | Notes |
|---|---|---|
| Base map | `https://makepad.nl/maps/world-20260903.mkmap`, `TileSourceConfig::http_archive` | HTTP range reads of the archive; the reference's renderer path, with retained drawing and byte budgets |
| Search | `https://photon.komoot.io/api/?q=…&limit=8&lang=en&lat=…&lon=…` | Biased to the map's centre |
| Reverse | `https://photon.komoot.io/reverse?lat=…&lon=…&lang=en` | For a dropped pin |
| Routing | `https://routing.openstreetmap.de/routed-{car,bike,foot}/route/v1/driving/…?overview=full&steps=true&geometries=polyline` | Real profiles for all three modes; the OSRM demo server has driving only |
| Location | `cx.start_location_updates()`, `Event::LocationUpdate`, `Event::LocationError` | CoreLocation, Android's `LocationManager` |

All four services answered from the US on 2026-09-18 (San Jose to Santa
Clara: car 7.7 km in 10 min, bike 35 min, foot 1 h 44; `Santa Clara
University` with its street address).

**Whether the hosted archive has street detail outside Europe is not yet
known.** Its build scripts are planet-wide, which suggests it does. The first
task settles it by drawing San Jose. If it does not, the map switches to
`MapView`'s live OpenStreetMap path (`use_network: true`), which the L0 `nav`
card ships with today; one constant in `model.rs` decides.

These are public servers under fair-use terms, so the app behaves: a
`User-Agent` naming the app on every request, search debounced by 300 ms, a
superseded request cancelled, one routing request in flight at a time (the
selected mode first, then the other two so each tab shows its time), a route
cached per origin, destination and mode, and every reply capped with
`max_response_body_bytes`. The map carries the `© OpenStreetMap contributors`
line the licence asks for.

All HTTP goes through `cx.http_request`; replies land on the UI thread as
`Event::NetworkResponses`. No threads and no sockets, as in News.

## Screens

One widget, `flow: Overlay`: the `MapView` full-bleed, and over it a layer per
screen that the model shows or hides. The map is never rebuilt.

1. **Explore.** A search pill at the top ("Search here"), the layers button
   under it at the right, the locate button at the bottom right, a compass
   above it that appears only while the map is rotated or tilted and puts it
   north-up and flat again, the attribution at the bottom left. A tap on a
   point of interest (`PinTapped`) or a long press (`LongPressed`, reverse
   geocoded) opens Place.
2. **Search.** The pill becomes a field with a back arrow and a clear button,
   over an opaque page: a row per result with its category icon, name,
   address line and distance from the map's centre. The field takes the key
   focus after its first draw, as News' search does. A row opens Place. The
   same page picks a route's origin when Directions asks for one.
3. **Place.** The map flies to the place and pins it. The sheet at peek
   shows the name, category and distance and a Directions button; at half,
   the address, the coordinates, and the phone, website and opening hours
   when the tapped pin carried them. The pill shows the place's name with a
   clear button. A tap on the bare map goes back to Explore.
4. **Directions.** A card at the top with the origin and destination rows and
   a swap button, and three mode tabs, each with its duration. The sheet:
   time and distance, Start and Preview, and the step list at half and full.
   The route is drawn and the camera fits it into the room between the card
   and the sheet. The origin is the device's location: with no fix yet the
   app asks for one, and if none comes it asks the person to choose a
   starting point.
5. **Navigation.** A green banner at the top with the maneuver's arrow, the
   distance to it and the instruction. A bar at the bottom with the time
   left, the distance left, the arrival time and an End button. The camera
   chases the puck, tilted and heading-up; panning pauses the chase and shows
   a recentre button. Off the route for longer than the session's grace, the
   banner reads `Rerouting…` and a new route is asked for from the live
   position. Arrival shows `You have arrived` and a Done button. **Preview**
   is the same screen driven by the simulated drive, which is also how
   navigation is tested at a desk.

`Event::BackPressed` and the in-app back arrows share one `back`:
Navigation, Directions, Place, Search, then Explore, where it returns
`false` and the host handles it.

The layers sheet has a dark-map switch (following the skin until the person
flips it), 3D buildings, labels, and miles or kilometres. Units default from
the destination's country code (`US`, `GB`, `LR`, `MM` are imperial) until
the person chooses. There is no satellite view: `MapView` has no imagery
mode.

The app persists one small file in its storage jail: the last camera, the
layer switches and the units, so it reopens where it was.

## Camera fitting

`MapView` has `fly_to(lon, lat, zoom)` and nothing that takes a box. `geo.rs`
computes the camera: the Web Mercator zoom at which the route's bounding box
fits the viewport less the top card and the sheet, and a centre shifted by
half the difference of those two insets so the route sits in the free room
and not behind the sheet. It is a pure function of the box, the viewport size
and the insets, tested against `MapView::lon_lat_to_screen` once in the
isolate so the zoom convention is pinned rather than assumed.

## Errors

- A reply that is not 200, not JSON, or over the cap is an error row with a
  Retry button where the results or the route summary would be. The previous
  results stay until a new reply lands.
- Photon with no features: `No results`. OSRM with a `code` other than `Ok`:
  `No route found`, for that mode only, so the other tabs still work.
- `Event::LocationError(PermissionDenied)`: a line that says location is
  off for OctosMap and that a starting point can be chosen instead.
  `Unavailable` or the 20-second timeout: `Could not find your location`.
- A reroute that fails keeps the old route and tries again on the next
  off-route verdict, not in a loop.
- A reply for a request that was superseded is dropped by id.

## Testing

- `geo.rs`: haversine and bearing against known pairs, the polyline decoder
  against a saved OSRM geometry, the fit camera (a box wider than tall, taller
  than wide, a single point, asymmetric insets), distance and duration text
  in both unit systems.
- `places.rs`: the saved Photon replies (a university with a full address, a
  street, an empty collection, a malformed body); the URL escapes its query.
- `routing.rs`: the saved car, bike and foot replies into routes whose
  cumulative distances are monotonic and end at the length; the maneuver
  table (turn and modifier, roundabout exits, depart, arrive, the ramp and
  fork kinds); `NoRoute`.
- `guidance.rs`: the simulated drive reaches `Arrived`; a position 100 m off
  the route asks for one reroute after the grace; the banner names the next
  maneuver and its distance.
- `sheet.rs`: a drag follows the finger between the detents; a release snaps
  to the nearest; a flick goes one further.
- `model.rs`: every transition and `back` from every screen; a superseded
  search; the staged mode fetches never overlap.
- `view.rs` and `module.rs`, in an isolate as News does: the view evaluates
  with no script errors; a search reply fills the list; `BackPressed` walks
  back and then declines; the module registers, creates and shuts down.
- The host's bundled-apps test lists `maps`.
- By hand on macOS: the standalone window with `--phone`, and the app hosted
  in the phone shell. On the OnePlus 6T: a GPS fix, pan, pinch, rotate and
  tilt, typing in the search field with a finger (adb cannot inject text into
  a Makepad `TextInput`; AppCards requirement R11.4 records the same), and a
  short real drive or walk.

## Out of scope for v1

Each becomes a `BACKLOG.md` entry: saved places (Home, Work, starred) and
recent searches; nearby category chips backed by Overpass; the wide home
tile; the assistant's tools (`search_places`, `directions`,
`start_navigation`); spoken guidance; route alternatives and multiple stops;
offline regions baked with `map_build`; satellite imagery; keeping the
screen awake while navigating; transit.

## Delivery

One branch, `feat/octosmap`, a commit per task, the plan in
`docs/plans/2026-09-18-octosmap.md`. `docs/maps.md` records the behaviour,
the services and their terms, and the checks, as `docs/photos.md` does for
Photos.
