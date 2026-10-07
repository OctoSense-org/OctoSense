# Maps: place-first design

**Goal:** Make Maps a map of places, with routing as one action on a place. You browse a map you can always drag and zoom, look places up, open a card about any of them, save the ones you want, and ask for directions only when you want them.

**Architecture:** Maps stays a contained script app (`apps/maps/bundle`, ADR 0004). One plain `MapView` (no `nav_mode`) serves browsing, place cards and the route preview. The drive view keeps its own 2D/3D maps. A small, opt-in makepad addition lets a script move the camera, frame and clear a route, and hear taps, long presses, marker taps and viewport changes. Place search and details come from OpenStreetMap services Maps already declares, called with `net.http_request`.

**Tech stack:** Makepad Splash (the app), Rust (the makepad `MapView` addition), Photon and Overpass (OpenStreetMap data), OSRM through `sys.navroute` (routes, unchanged).

**Status:** design only, approved by the product owner on 2026-10-06. Nothing here is built; every check below is still to be run.

## Why

Maps is route-first. Its only map runs `nav_mode: "plan"`, a static route-preview camera, and picking any place immediately draws a route from the start point. Two problems follow (runtime: makepad `c155f61` plus the reviewed patches):

- Once a route is drawn, the map can't be dragged or zoomed: on the place card, in Directions, and after **Close** on the search screen. `MapView::nav_plan_camera` (`widgets/src/map/view.rs`) re-fits the route on every frame and ignores the gesture. The follow modes take a drag into `nav.pan`; plan mode doesn't. Reproduced on the desktop shell (a drag after Close moves nothing, on main and on PR #348) and on a Pixel 7 Pro.
- The app can't remove a route: `set_nav_polyline` ignores an empty string, and nothing else clears it. The old route stays on the search screen.

A Maps script also can't move a plain map's camera or hear the map. `MapView` exposes five script methods (`set_nav_polyline`, `set_route_markers`, `set_nav_recenter`, `nav_zoom_by`, `nav_center_origin`), and the camera ones act only in nav modes. Yet `MapView` already has `fly_to(lon, lat, zoom)` in Rust and emits `MapViewAction::Tapped`, `LongPressed`, `MarkerClicked` and `ViewportChanged`. Search ignores where you are looking too: `sys.search` asks Photon with no position (`widgets/src/splash.rs`).

## Decisions

| Question | Decision |
| --- | --- |
| How rich is a place card? | Name, category, address and distance, plus OpenStreetMap details where mapped: opening hours, phone, website, cuisine. No ratings or photos. |
| What does touching the map do? | A long press anywhere shows "What's here". A tap on a pin opens its card. A tap or drag on the map closes the search list; a place card stays until **Close**. |
| Saving | A saved-places list (star on the card), shown above Recent and as pins. No Home/Work yet. |
| The route preview | On the same plain map, framed once and then free to drag and zoom. Plan mode is no longer used by Maps. |
| Nearby | Search favours the visible area. No category chips yet. |

Alternatives considered:
- A separate plan-mode map for Directions keeps the preview frozen and holds one more map.
- Changing plan mode itself would change AppCard's nav cards.
- A host service can't draw on or move a widget inside the app's isolate.
- A native Maps module goes against ADR 0004.

## Screens and flow

- **Browse** (home): the plain map fills the screen.
  - A search box sits at the top.
  - A ◎ button flies to your location; it is shown only when `location` is granted.
  - Saved places show as pins.
  - Maps opens at your GPS fix, or San Jose without one.
- **Search list:** closed until the search box is tapped (as PR #348 does).
  - With the box empty, it shows **Saved**, then **Recent**.
  - A submitted search shows results near the visible area.
  - A tap or drag on the map closes the list.
- **Place card** (from a result, a pin, Recent or Saved): the map flies to the place and shows its pin. The card shows:
  - name, category and address;
  - distance from you (straight line, only with a fix);
  - opening hours as mapped (for example "Mo–Fr 07:00–19:00"), phone, website and cuisine when they exist.

  Its actions:
  - **Directions**;
  - **Save/Saved**;
  - **Website**, which opens in the app's `WebReader`. A place's site is on no host list, so Maps adds the `web` grant (News and YouTube already hold it).

  There is no **Call** action: the runtime can't hand a `tel:` link to the phone, so the phone number is shown as text.

  **Close** returns to Browse. The pin stays only if the place is saved.
- **What's here:** a long press drops a pin and opens a card from Photon's reverse lookup (the nearest place or address). With no answer the card says "Dropped pin" with the coordinates. Directions works to that point.
- **Directions:** today's route panel: Drive/Walk/Bike, From with Change, up to two stops, To, and the ETA.
  - The route is drawn on the same map and framed once with `fit_route()`; the map can still be dragged and zoomed.
  - **Back** clears the route with `clear_route()` and returns to the place card.
  - **Start** opens today's drive view, unchanged.

## The makepad addition

All of it is new and opt-in on `MapView`. A map without the callbacks behaves as today, and plan, follow and drive modes are unchanged.

| Kind | API | Built on |
| --- | --- | --- |
| Call | `fly_to(lat, lon, zoom)` | `MapView::fly_to` |
| Call | `fit_route()`: frame the current route and stops once, then leave the camera to the person | the bounds `nav_plan_camera` uses |
| Call | `clear_route()`: remove the route line (`set_route_markers("")` already clears pins) | the nav overlay |
| Event | `on_tap: \|lat, lon\|` | `MapViewAction::Tapped` |
| Event | `on_long_press: \|lat, lon\|` | `MapViewAction::LongPressed` |
| Event | `on_marker: \|index\|`, in `set_route_markers` order | `MapViewAction::MarkerClicked` |
| Event | `on_viewport: \|lat, lon, zoom\|`, once the camera settles | `MapViewAction::ViewportChanged` |

Callbacks are queued with `widget_to_script_call`, like GestureView's. They grant nothing: a tap reports only the spot touched, and the device's position still comes only from `sys.gps` with the `location` grant. With `on_tap`, the redesign closes the search list from the map's own tap instead of PR #348's GestureView catcher.

## Data, storage and errors

- **Search:** `net.http_request` GET to `photon.komoot.io/api/?q=…&limit=8&lang=en&lat=…&lon=…`. The position is the last `on_viewport` centre, else the GPS fix, else the default. The script reads:
  - name and address parts;
  - the OpenStreetMap type and id;
  - `osm_key`/`osm_value`, which a small table maps to a category label (falling back to the raw value).

  A search runs on Return.
- **Place details:** one Overpass query per opened place, by the result's OpenStreetMap id: `node(ID);out tags;`, or `way(ID);out tags center;`.
  - It reads `opening_hours`, `phone`/`contact:phone`, `website`/`contact:website` and `cuisine`.
  - It tries `overpass-api.de`, then `overpass.kumi.systems`, then `overpass.openstreetmap.fr`; all three are already declared.
  - Answers are cached per id under `cache_path` for 7 days.
- **What's here:** `photon.komoot.io/reverse?lat=…&lon=…`.
- **Saved places:** `data_path("saved.json")`, holding id, name, category, address, lat and lon. **Recent** stays as it is (`recents.json`, at most 8).
- **Pins:** `set_route_markers` shows saved places in the stop colour and the open place in the destination colour. Directions shows only the route's own pins. A distinct saved-pin style would need a fourth marker kind in makepad, so it isn't in this version.
- **Distance:** straight-line from the GPS fix, computed in the script. ETA appears only in Directions, from `sys.navroute` as today.
- **Errors:**
  - Search unavailable → "Search isn't available right now." in the list.
  - Overpass failing or returning no tags → the card shows Photon's data only.
  - Reverse lookup failing → "Dropped pin".
  - A route failing → today's "n/a".
- **Courtesy to free services:** a `User-Agent: OctoSense-Maps` header, one Overpass request per opened place, the cache, and no requests from moving the map.

## Testing

- **makepad:** unit tests beside `MapView`'s own:
  - `fly_to` starts a flight;
  - `fit_route` frames the route once and then leaves the camera alone;
  - `clear_route` empties the route overlay;
  - simulated taps, long presses and marker taps queue their callbacks with the right values;
  - a map without callbacks queues nothing.
- **OctoSense unit:** VM tests of the script's pure functions, in the style of Photos' `photo_model`:
  - Photon JSON → hits and categories;
  - Overpass tags → card fields;
  - distance text;
  - saved add and remove.

  Also: `should_keep_every_system_apps_files_in_its_account_folder_or_cache`, and `python3 tools/setup.py --check --cargo` with the patch in the lock.
- **Desktop acceptance:** `--test-action launch-maps`, driven over `MAKEPAD_REMOTE`, in a scratch `OCTOSENSE_HOME`:
  - the list is closed at launch;
  - the search request carries `lat`/`lon` (app log);
  - the card shows the OpenStreetMap fields;
  - Save survives a relaunch;
  - a long press opens "What's here";
  - in Directions the route is drawn and the map still pans;
  - Back clears the route;
  - drive Start and End are unchanged.
- **Devices:** a Pixel 7 Pro as a separate test package (dark mode) and the OnePlus 6T (light mode). iOS and OpenHarmony stay **unverified** unless run.

## Rollout

Each pull request is opened only with the product owner's approval.

1. **makepad:** the calls and events above, with their tests.
2. **OctoSense:**
   - `tools/runtime-patches/makepad-map-script-api.patch`, stacked in `runtime-patches.lock.json` (sha256 and resulting tree) until a runtime repin includes it;
   - the Maps redesign, with the `web` grant in its manifest;
   - the Maps row in `apps/README.md` and `apps/README.zh-CN.md`.

   They go together because the redesign needs the patch.
3. **OctoScript-App-Design-Flow:** `MapView`'s new calls and events in `docs/SCRIPT-API.md`.

PR #348 (the search list opens from the search box and closes from the map) is independent and can merge first.

## Not in this version

- a dedicated saved-pin style;
- **Call**, until the runtime can open a `tel:` link;
- category chips;
- Home and Work;
- "open now";
- ratings, reviews and photos (they would need a commercial places API);
- new tools for Maps' agent;
- removing the runtime patch, which happens at the next makepad repin.
