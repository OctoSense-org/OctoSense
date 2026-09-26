# OctoSense Home

English | [简体中文](README.zh-CN.md)

The OctoSense phone shell: a Makepad app that is the device's Home screen.
Home pages with live tiles and app pairs, a gesture layer, the shade
(notifications left, controls right), Recents, a live island for ongoing
activities, and hosted apps drawn in-process inside its tiles: App Hub and
the apps it runs, the system apps, AppCard, Reference and Sheets.

Setup, builds for every target, pins and CI are in the
[root README](../README.md). This page is the Home-specific deep dive.

## Relation to OctoSense-Desktop

Home was split from the desktop shell,
[OctoSense-Desktop](https://github.com/OctoSense-org/OctoSense-Desktop) (then
named OctoSense), on 15 September 2026, at the tip of its mobile shell chain
(PRs #22 to #28). The two still share much of their source (`src/main.rs`,
`desk.rs`, `layout.rs`, `clients.rs`, `shell/*`, the compositor). The phone
build is the `mobile_only` configuration of the one crate: `build.rs` turns
it on for Android, and `--features mobile-only` turns it on elsewhere.
Desktop-only work belongs in the desktop repository. A shared
`octosense-core` crate is the intended next step, so fixes stop needing
cherry-picks. `upstream/makepad.json` records which window-manager files were
imported from Makepad; `scripts/upstream.py` compares and merges them
([docs/upstream.md](docs/upstream.md)).

## The Home role

The activity offers the `HOME` intent filter and is `singleInstance`. On a device you control:

```sh
adb shell cmd package set-home-activity dev.makepad.octosense/.MakepadApp
```

or pick OctoSense in Android's Home chooser. A Home press or gesture then reaches the running shell as `Event::HomeIntent` and shows the home page. What the Home role does **not** change: the system keeps its bottom gesture zone, its Recents (swipe-up-and-hold) and its status-bar shade. **3-button navigation** removes the gesture-zone race and is the recommended mode:

```sh
adb shell cmd overlay enable-exclusive --category com.android.internal.systemui.navbar.threebutton
```

(`…navbar.gestural` restores gestures.) The privileged route — owning the gesture zone and Recents — is sized in [docs/android/launcher-plan.md](docs/android/launcher-plan.md) and not started.

## Gestures

| Where | Gesture | Does |
|---|---|---|
| Home page, middle | pull down | Search, with its input focused and keyboard ready |
| Home page, right quarter | pull down | the shade's Controls (Wi-Fi, brightness, …) |
| Home page, left quarter | pull down | the shade's Notifications |
| Top edge, left / right | pull down | Notifications / Controls (as well) |
| Home page | swipe sideways | pages: Glance ⇠ apps ⇢ App Library |
| App Library | drag | scrolls the grid; past either end it stretches and springs back (Back or Home closes it) |
| App Library or Search | swipe right across the content | returns to the Home page you left and dismisses the keyboard |
| Bottom band (above the system's) | swipe up / hold / sideways | Home / Recents / quick switch |
| Side edges | swipe in | Back |
| App icon | long press | Add to / remove from Home, dock, App info, Uninstall |
| Home-page icon | long press, then drag | Reorder the page (drop between icons), dock it (drop on the dock), make a folder (drop on another icon) or add to one (drop on a folder tile) |
| App pair tile | long press | Change either app, or remove the pair |
| Folder tile | long press | Remove one app, or the folder |
| App tile | long press | Remove the tile (the home menu's "Show hidden tiles" brings them back) |
| Empty home | long press | Widgets, Light/Dark appearance, Grid: 4 or 5 columns, Pull-downs (launcher shade or system-wide panel), System setup, Show hidden tiles |

A pull commits from 40 % of the way (≈135 px on a 1080-wide phone); navigation swipes need the full distance or a flick. While a pull is in flight the page dims and a search field follows the finger; a committed gesture gives a short haptic tick. Until each hidden gesture has been used once, the home page shows a one-line hint for it (`src/mobile_hints.rs`; Android remembers what was seen). A second Home press on a settled home page returns to the primary page.

Search opens only by pulling down on Home; the App Library has no search bar. Search ranks names that start with what you typed first and Return opens the best match. In the App Library, a letter column on the right jumps the grid, and with usage access a "Suggested" row of recently used apps sits on top. Icons carry a dot while their app has a notification in the shade. Recents lists the hosted apps as cards and, with usage access granted in Android's Settings (the card in Recents opens it), a row of the Android apps used lately. Every tappable region is an accessibility node with a spoken label, so TalkBack and UI automation can read and activate the shell (verified with TalkBack installed and with a UiAutomation probe: accessibility focus lands on a node and its click action opens the app, the shade or the drawer; note that `adb shell input` taps bypass TalkBack's touch exploration, so a real screen-reader touch cannot be scripted). Labels follow Android's text size setting. The shell follows Android's dark theme and draws under transparent system bars; the shade's Dark mode tile overrides the appearance until the system setting next changes. The bridge's failure reasons reach the person as plain sentences (`result_copy` in `src/android_integration.rs`), never as reason codes.

## System apps

News, Photos, Maps, Camera and Mail are contained script apps
([ADR 0004](docs/adr/0004-system-apps-are-contained-script-apps.md)). Their
bundles live in OctoSense-System-Apps (`apps/<name>/bundle/`, pinned by
`native-apps.lock.json`); `system-apps.json` names which this Home ships and
mounts the artwork Home owns (Photos' sample library,
`apps/photos/resources/photos`). App Hub's Card runner runs each in its own
isolate under its manifest's policy, in the standalone Home and in the ROM
alike. Each keeps its short launcher id (`news` for `os.news`), so icons,
tiles and the dock are unchanged.

Mail reads and sends through the `mail` host service (`apps/mail/host-service`
in OctoSense-System-Apps): the person signs in on the host's own sheet, the
password stays in the keychain or behind an Android Keystore key, and the app
never holds a socket or a password. For a demo mailbox (password `demo`):

```sh
# desktop, from home/
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release --features mobile-only
# phone
adb shell am start -n <package>/.MakepadApp --es makepad.APP_CONFIG '{"mail_demo":true}'
```

`app-news`, `app-photos` and `app-maps` link the earlier native modules in
place of their script apps, for comparison until the script apps are measured
on a device; their notes are [docs/photos.md](docs/photos.md) and
[docs/maps.md](docs/maps.md). Mail and Camera have no native module any more.

## App Hub

App Hub (`apphub`) browses the signed OctoSense catalog, searches, shows app
details, installs verified bundles and keeps an installed-app Library.
Installed apps open in contained Card instances (`card`) and appear
separately in the launcher and Recents. Both come from App Hub's shared shell
crate `octosense-app-hub-app` (OctoSense-App-Hub `crates/app-hub-app`), linked
by the default `app-hub` feature and on every mobile build. The **Preview
catalog** switch shows the built-in apps while the live catalog is empty.

See the crate's
[README](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/0d36f50b9f0fdfbd8247b5deb8da8baa53c83066/crates/app-hub-app/README.md)
at the pinned revision and the [native design evidence](docs/design/app-hub/README.md).
App authors start with
[OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow).

## Run on a desktop

The same shell in a phone-sized window, on Metal, DirectX or OpenGL:

```sh
cargo run --release --features mobile-only
cargo run --release --features mobile-only -- --test-action island:demo --test-action capture:/tmp/shell.png
```

`--test-action` pushes fixtures (`island:demo`, `island:expand`, `page:<n>`,
`ask-appcard:<text>`, `launch-<app id>`, `taps:<x>,<y>@<s>`), and
`capture:<path>` writes the presented frame every 5 s, so a scripted run can
be inspected without a screen. `MAKEPAD_APP_CONFIG='{"test_actions":[...]}'`
passes the same list where arguments cannot be given. A plain `cargo run` is
the universal desktop shell of the desktop repository; it keeps building here
but is not this repository's product. It starts in **OctoSense Light** with
its bundled wallpaper; Omarchy and the other styles remain in the style menu.

## Performance

Target on the OnePlus 6 (Android 15, Adreno 630, 60 Hz): **≥ 55 fps with p95 frame intervals ≤ 20 ms** on every shell transition, and an idle screen that presents about once a second. As of 16 September 2026 the shade (open/close), pages, Group open/close, Recents both ways (empty and populated) and AppCard opening pass warm and fresh-process blocks; native SystemUI still shows no early skipped refresh where a few of ours do. The measured reason for the remaining early skips is the GPU's DVFS floor (257 MHz for the first ~120 ms of a gesture), so the working rule is: a transition frame must cost ≤ ~4.5 ms of GPU at 710 MHz. The unchanged Vulkan backend is slower (it serialises CPU and GPU and the clock never ramps under it) and is not a route to the target.

Measure with the phone tools:

- `scripts/measure_android_frames.py` — SurfaceFlinger presentation timestamps for one injected gesture, joined to the shell's markers when the app is launched with `--es makepad.TRACE phone.frames` (`[phone.frames]`, `[phone.input]`, `[phone.scene]` in logcat).
- The bench's `target/perf-artifacts/` helpers (`run_cases.py` for the scenario blocks, `kgsl_gpu_timeline.py` / `kgsl_frames_summary.py` for Adreno GPU execution time and clock per frame from kgsl ftrace) — described in [docs/android/perf-gap-analysis.md](docs/android/perf-gap-analysis.md).
- Three quick taps on the status-bar battery icon toggle the on-device frame monitor; three quick taps on the clock push the island demo, on a bench run only.

Records: [docs/android/](docs/android/README.md) (gap analysis, plan, launcher plan, validation log, Vulkan probe) and the earlier [docs/perf-mobile-shell.md](docs/perf-mobile-shell.md).

## Layout

- `src/mobile*.rs`: the phone shell. State and navigation (`mobile.rs`), the
  gesture recognizer (`mobile_gestures.rs`), the surface that draws home,
  drawer, keyboard and overlays (`mobile_surface.rs`), pages, tiles, groups,
  the shade, the island, the thinking octopus, the perf monitor.
- `src/apps.rs`: which modules this build links, the system apps and
  installed apps as launcher rows, and how each is hosted.
- `src/desk/phone.rs`: the desk's phone composition: hosted-app captures, the
  kept home scene and its blur pyramid, the compositor path.
- `resources/android/AndroidManifest.xml.template`: the activity (Home role,
  share and deep-link intents).
- `resources/icons/apps/<style>/`: this shell's own icons for News and
  OctosMap, one 64x64 SVG per framework style, written by
  `python3 tools/build_app_icons.py` (`--sheet <path>` also renders a review
  sheet with `rsvg-convert`). The renderer has no clip paths, masks, filters
  or text, so the art stays inside its tile by construction; a test holds the
  files to that.
- `apps/appcard`: hosts the AppCard assistant (`octos-app`, a path dependency
  into `../.sources/system-apps/apps/appcard/app/app`).
- `apps/reference`: the reference module.
- `apps/news`, `apps/photos`, `apps/maps`: the native comparison modules
  (features `app-news`, `app-photos`, `app-maps`). Their design notes are in
  `docs/plans/`.
- `android/`: the System Bridge, contracts, Quickstep and SystemUI projects
  ([android/README.md](android/README.md)).
- `docs/`: records and recipes; `docs/adr/` the Home decisions;
  `docs/android/` the performance and launcher records.

## Dependencies

- Framework: the Octoscript-Makepad release selected by
  `native-runtime.lock.json`; its `runtime.json` pins Makepad and OctoScript.
  Cargo `[patch]` sections resolve every Makepad crate to
  `../.sources/makepad`, so the graph has one widgets/platform/script. Do not
  substitute a moving branch. How the fork relates to upstream Makepad and how
  a pin moves: [docs/makepad-fork.md](docs/makepad-fork.md).
- App Hub: `octosense-app-hub-app` and its backend crates, one pinned
  revision, the same one the Mail host service names, so no `[patch]` is
  needed for one App Hub source.
- OctoSense-System-Apps (`native-apps.lock.json`): the system-app bundles,
  the Mail host service and `octos-app`, which brings octos from
  `octos-org/octos` at one revision.
- The AppCard kernel is not a Cargo dependency: `liboctos.so` is bundled at
  APK build time with `MAKEPAD_ANDROID_EXTRA_LIBS`
  ([docs/android-appcard-build.md](docs/android-appcard-build.md); its pins
  predate the current ones). Without it, the AppCard tile falls back to its
  WebSocket transport and login screen.

## Tests and state

`cargo test --features mobile-only mobile -- --test-threads=1` runs the
shell's unit tests (gestures, pages, island, shade, groups, tiles). The full
CI set is in the [root README](../README.md#testing-and-validation).
`scripts/smoke.py` launches a release build under `MAKEPAD_REMOTE` and drives
it over HTTP. [docs/validation.md](docs/validation.md) and
[docs/android/validation-record.md](docs/android/validation-record.md) hold
the device validation.

State lives under `~/.octosense` on desktop and in the app's data directory on
Android; `OCTOSENSE_HOME` relocates it.
