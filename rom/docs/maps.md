# OctosMap

`apps/maps` is the maps AppModule of the OctoSense phone shell: a full-screen
map under a search bar, a place sheet that slides up from the bottom,
directions by car, on foot and by bike, and turn-by-turn navigation. The
layout follows the [Google Maps listing](https://apps.apple.com/us/app/google-maps/id585027354).
Its launcher id is `maps`, its label **OctosMap**, and it opens from the home
grid and the App Library's Productivity card. It has no home tile yet
(MAPS-03).

It is built on the framework's route app (`../makepad/apps/route`): the same
`MapView`, the same hosted vector archive, the same navigation session and
location handling, under a phone interface of its own. The design and the
plan are `docs/plans/2026-09-18-octosmap-design.md` and
`docs/plans/2026-09-18-octosmap.md`.

## Using the app

- **The map** pans with a finger, zooms with a pinch, and rotates and tilts
  with two fingers. A **compass** appears while it is not north-up and flat;
  a tap puts it back.
- **Locate** (bottom right) asks for the device's location the first time,
  then flies to the fix and draws the puck; after that a tap recentres. If
  location is off or no fix comes in twenty seconds the app says so, and the
  next tap tries again.
- **Layers** (top right) sets the dark map (it follows the shell's light or
  dark mode until it is flipped), 3D buildings, labels, and miles or
  kilometres. Distances are in the units of the destination's country until
  the switch is used.
- **Search here** opens a page of its own. Results appear a moment after the
  typing pauses, or on Return: a place's kind, name and address, and how far
  it is once there is a fix. A failed search says why and offers **Retry**.
- A result opens its **place**: a pin, the map moved so the pin sits between
  the search bar and the sheet, and the sheet at its lowest with the name,
  the category, the distance and **Directions**. Drag the sheet's head, or tap
  it, for the address and the coordinates. A tap on the bare map, the back
  arrow or the back gesture lets go of the place.
- **A long press** on the map drops a pin at once; the reverse lookup then
  names it.
- **Directions** shows both ends on a card, with a swap button, and a tab for
  driving, walking and cycling, each with its time. The shown tab's route is
  drawn and fitted between the card and the sheet; the sheet gives the time,
  the distance and every step. Tap an end to search for another. With no
  location the app asks for one, and a starting point can be chosen instead.
  A mode with no route says so on its own tab and offers **Retry**.
- **Start** navigates over live fixes; **Preview** drives the same screen
  with a simulated drive of between twenty and ninety seconds. The banner
  shows the next turn's arrow, how far it is and what to do; the bar shows
  the time and distance left and the arrival time. The map chases the puck,
  tilted and heading-up; dragging it pauses the chase and shows **Recentre**.
  Off the route for more than a few seconds the banner reads `Rerouting…`
  and a new route is fetched from the current position. Arrival shows
  **Done**, which goes back to the place; **End** goes back to the routes.
- The back gesture walks back one screen at a time and leaves the app from
  the map.

The app reopens where the map last was, with the layer switches as they were
left.

## Services and their terms

OctosMap has no backend of its own. It uses public, keyless services, all of
them OpenStreetMap data, and the map carries the licence's line
(`© OpenStreetMap contributors`).

| Need | Service |
|---|---|
| Base map | `https://makepad.nl/maps/world-20260903.mkmap`, the framework's hosted vector archive, read over HTTP range requests |
| The sea | Two overlay archives beside it, `ocean-low` and `ocean-high`, drawn as the map's `ocean` layer: the base archive has no water of its own at the far zooms |
| Search, reverse lookup | Photon, `https://photon.komoot.io` |
| Routing | The FOSSGIS OSRM servers, `https://routing.openstreetmap.de/routed-{car,foot,bike}` |
| Location | The platform: CoreLocation, Android's `LocationManager` |

These are fair-use servers, not a contract. The app names itself in a
`User-Agent` on every request, sends a search only after a 300 ms pause in
the typing, cancels a request it no longer wants, asks for one route at a
time (the shown tab's first), keeps a route per mode until an end changes,
and caps every reply at 4 MiB. A product build needs services of its own
(MAPS-08). The framework route app's own search and routing API was not
used: it covers Europe only (probed on 2026-09-18).

There is no satellite view: `MapView` has no imagery mode (MAPS-07).

## Implementation and storage

| File | Owns |
|---|---|
| `src/geo.rs` | The polyline decoder, the camera that fits a box into the room the panels leave, distance, duration and arrival text; distances and bearings are the navigation library's |
| `src/places.rs` | Photon's URLs and GeoJSON as `Place` rows |
| `src/routing.rs` | OSRM's URLs and replies as the framework's `Route`, plus the step list |
| `src/guidance.rs` | The framework's `NavSession` fed by fixes or the simulated drive, one `NavTick` per position; the reroute rule |
| `src/sheet.rs` | The sheet's three heights, its drag and its snap |
| `src/model.rs` | The screen state machine, the requests in flight, the settings, the skin |
| `src/view.rs` | `MapsView`: one persistent `MapView` and a layer per screen |
| `src/module.rs` | The `AppModule` the shell links |
| `src/main.rs` | The standalone window and its dev flags |

Everything that is not drawing is pure Rust with no widget in sight and is
tested without a window; the view is tested in an isolate with injected
replies and fixes, as News is. No test touches the network: the parsers read
`tests/fixtures`, saved from the live services.

The map is never rebuilt: the screens are layers over it that the model
shows or hides. That is the reason this is a native module and not the L0
`nav` card: a card's state change regenerates its widgets and tears the map
down.

The one file the app keeps, `state` in its storage jail, holds the last
camera and the layer switches, every field optional. It is written two
seconds after the last change and on shutdown.

The module declares `storage`, `net` and `location`. Android's manifest
already has both location permissions and the platform asks the person on
the first `start_location_updates`; iOS asks with the sentence in
`resources/apple/Info.plist`.

## Build and validation

```sh
cargo test -p octosense-maps
cargo check --locked --workspace --features mobile-apps
cargo test --features mobile-apps --bin octosense \
  bundled_apps_open_without_catalog_files_or_child_processes

# Standalone desktop development window, phone-sized
cargo run -p octosense-maps -- --phone
```

In the phone shell on a desktop, hosted in-process as a phone hosts it (the
desktop launches catalog apps as child processes unless told otherwise):

```sh
cargo run --features mobile-only,mobile-apps -- --module maps \
  --test-action launch-maps
```

Verified on 2026-09-18 on macOS: the log reads `modules linked: [..., "maps"]`
and `launched maps as client 1 (in-process)`; the app draws under the shell's
status bar and above its gesture bar; a tap delivered by the shell
(`--test-action taps:200,76@9`) opens Search; the home grid shows the app
with the route app's icon.

On the OnePlus 6T (Android 9) on 2026-09-18, a release APK of this branch:
the module is linked (`modules linked: [..., "maps"]`) and launches
in-process; News and OctosMap sit in the dock's bottom row with Photos; the
app's interface draws inside the shell's safe area; a tap on Locate brings a
GPS fix within seconds (location events do reach an in-process app). Two
framework problems stop it there, both in the fork and both written up in
`BACKLOG.md`: the map draws no tiles on Android (MAPS-12, the Android
network backend's silent cancel) and opening the app can freeze the shell
(MAPS-13, the GL backend unwrapping a pass with no draw list). Not yet run
on a phone because of them: search with the soft keyboard, the place sheet
and directions over a drawn map, a preview, and a real drive.

The window's dev flags: `--phone`, `--dark`, `--light`, `--at lat,lon` (open
the map there), `--fix lat,lon` (the device is there, for a desk with no
GPS), and `--show <state>` to open on a state without driving the window:
`layers`, `search:<query>`, `place:<query>`, `directions:<query>`,
`preview:<query>`. For example, a simulated drive from downtown San Jose:

```sh
cargo run -p octosense-maps -- --phone --at 37.3382,-121.8863 \
  --fix 37.3382,-121.8863 --show "preview:santa clara university"
```
