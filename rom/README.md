# OctoSense mobile

## Shared Octoscript-Makepad runtime

`native-runtime.lock.json` selects one
[Octoscript-Makepad](https://github.com/OctoSense-org/Octoscript-Makepad)
release. Its `runtime.json` owns the exact Makepad and Octoscript revisions,
shared with AppCards, Mail and the other OctoSense applications.

Before building, run `python3 tools/setup-native.py` (Python 3.9+). The framework
repositories are siblings of this app: `../octoscript-makepad`, `../makepad`
and `../octoscript`. Local changes are preserved; `--update` only updates clean
checkouts. CI verifies the selected release and rejects duplicate Makepad sources.
Use `python3 tools/setup-native.py --check --cargo-manifest Cargo.toml`
to check the local dependency graph. Existing platform rendering backends remain
part of their applications; the framework controls the shared VM and UI sources.


The OctoSense phone shell: an Android app with home pages, live tiles and app pairs, a gesture layer, the shade (notifications left, controls right), Recents, and hosted apps (Reference, Sheets, Photos, AppCard and Mail) drawn in-process. It uses the runtime selected by [Octoscript-Makepad](https://github.com/OctoSense-org/Octoscript-Makepad), including its pinned underlying Makepad fork.

This repository was split from the desktop [OctoSense](https://github.com/OctoSense-org/OctoSense) on 15 September 2026, at the tip of the mobile shell chain (its PRs #22–#28). The two still share most of their source (`src/main.rs`, `desk.rs`, `layout.rs`, `clients.rs`, `shell/*`, the compositor); the Android build is the `mobile-only` configuration of that one crate. Desktop-only work stays in the desktop repository; a shared `octosense-core` crate is the intended next step, so fixes stop needing cherry-picks.

## Build and run on a phone

Requires Rust stable, an installed Android SDK/toolchain and a device on ADB.
Keep AppCards at `../Octosense-Service-AppCards` for the Mail module. Build
`cargo-makepad` from the exact sibling revision selected by the shared runtime;
it carries this app's Java activity (HOME, GPS, share and deep-link intents):

```sh
python3 tools/setup-native.py --check --cargo-manifest Cargo.toml
cargo build --release --manifest-path ../makepad/tools/cargo_makepad/Cargo.toml
../makepad/target/release/cargo-makepad makepad android \
  --sdk-path=/path/to/existing/android_sdk build -p octosense --release
```

`build` makes the APK (`target/android/makepad-android-apk/octosense/apk/octo_sense.apk`). Application ID `dev.makepad.octosense`, label **OctoSense**. Reference, Sheets, Photos, AppCard and Mail are linked in automatically; to bundle AppCard's kernel, add `MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=<path to the octos aarch64 build>"` — the recipe is in [docs/android-appcard-build.md](docs/android-appcard-build.md). Without it the AppCard tile falls back to its WebSocket transport and login screen. The Mail preview instructions below install a separate test package.

### Make it the Home app

The activity offers the `HOME` intent filter and is `singleInstance`. On a device you control:

```sh
adb shell cmd package set-home-activity dev.makepad.octosense/.MakepadApp
```

or pick OctoSense in Android's Home chooser. A Home press or gesture then reaches the running shell as `Event::HomeIntent` and shows the home page. What the Home role does **not** change: the system keeps its bottom gesture zone, its Recents (swipe-up-and-hold) and its status-bar shade. **3-button navigation** removes the gesture-zone race and is the recommended mode:

```sh
adb shell cmd overlay enable-exclusive --category com.android.internal.systemui.navbar.threebutton
```

(`…navbar.gestural` restores gestures.) The privileged route — owning the gesture zone and Recents — is sized in [docs/android/launcher-plan.md](docs/android/launcher-plan.md) and not started.

### Gestures

| Where | Gesture | Does |
|---|---|---|
| Home page, middle | pull down | App Library with its search field |
| Home page, right quarter | pull down | the shade's Controls (Wi-Fi, brightness, …) |
| Home page, left quarter | pull down | the shade's Notifications |
| Top edge, left / right | pull down | Notifications / Controls (as well) |
| Home page | swipe sideways | pages: Glance ⇠ apps ⇢ App Library |
| App Library | pull down | back to the home page |
| Bottom band (above the system's) | swipe up / hold / sideways | Home / Recents / quick switch |
| Side edges | swipe in | Back |
| App icon | long press | Add to / remove from Home, dock, App info, Uninstall |
| Home-page icon | long press, then drag | Reorder the page (drop between icons), dock it (drop on the dock), make a folder (drop on another icon) or add to one (drop on a folder tile) |
| App pair tile | long press | Change either app, or remove the pair |
| Folder tile | long press | Remove one app, or the folder |
| App tile | long press | Remove the tile (the home menu's "Show hidden tiles" brings them back) |
| Empty home | long press | Widgets, Wallpaper, System setup |

A pull commits from 40 % of the way (≈135 px on a 1080-wide phone); navigation swipes need the full distance or a flick. While a pull is in flight the page dims and a search field follows the finger; a committed gesture gives a short haptic tick. Until each hidden gesture has been used once, the home page shows a one-line hint for it (`src/mobile_hints.rs`; Android remembers what was seen). A second Home press on a settled home page returns to the primary page.

Recents lists the hosted apps as cards and, with usage access granted in Android's Settings (the card in Recents opens it), a row of the Android apps used lately. Every tappable region is an accessibility node with a spoken label, so TalkBack and UI automation can read and activate the shell. Labels follow Android's text size setting. The shell follows Android's dark theme and draws under transparent system bars; the shade's Dark mode tile overrides the appearance until the system setting next changes. The bridge's failure reasons reach the person as plain sentences (`result_copy` in `src/android_integration.rs`), never as reason codes.

## Mail preview on Android

Mail is an in-process AppModule from the sibling
`../Octosense-Service-AppCards/apps/mail/native`. It uses the same locked
Octoscript-Makepad release as this launcher. The default Rust backend connects
directly from Android to Gmail using verified POP3/TLS, with private on-device
accounts, cached mail and drafts. No Mac service or USB connection is needed at
runtime. Android renders native inbox/search/compose/settings and a platform
WebView for full-length plain and HTML messages.

With the already-installed Android SDK, build a separate preview package:

```sh
cargo build --release --manifest-path ../makepad/tools/cargo_makepad/Cargo.toml
../makepad/target/release/cargo-makepad makepad android \
  --sdk-path=/path/to/existing/android_sdk \
  --package-name=dev.makepad.octosense.mailpreview --app-label='OctoSense Mail' \
  build -p octosense --release
```

Use `apps/mail/scripts/open_android.py` in the AppCards repository to install
and open the APK, as described in the [Mail app](../Octosense-Service-AppCards/apps/mail/README.md#standalone-android-mail).
Use `--demo --probe` for isolated fictional mail and measured touch testing;
`--record --demo` enables timestamped app-owned GPU frames. WebView needs its
own page snapshot when making a video. Account provisioning uses the on-device
settings or a private one-time `--bootstrap` file. `--companion` explicitly opts
into the older USB Mac service. The preview package preserves the installed
launcher and Home role. Android SMTP has not been live-send verified; IMAP sync
and external attachment previews remain desktop-only.

## Run on a desktop

The same shell in a phone-sized window, on Metal/DX/GL:

```sh
cargo run --release --features mobile-only
cargo run --release --features mobile-only -- --test-action island:demo --test-action capture:/tmp/shell.png
```

`--test-action` pushes fixtures (`island:demo`, `island:expand`, `page:<n>`, `ask-appcard:<text>`) and `capture:<path>` writes the presented frame every 5 s, so a scripted run can be looked at without a screen. A plain `cargo run` is the universal desktop shell of the desktop repository; it is kept building here but is not this repository's product.

## Performance

Target on the OnePlus 6 (Android 15, Adreno 630, 60 Hz): **≥ 55 fps with p95 frame intervals ≤ 20 ms** on every shell transition, and an idle screen that presents about once a second. As of 16 September 2026 the shade (open/close), pages, Group open/close, Recents both ways (empty and populated) and AppCard opening pass warm and fresh-process blocks; native SystemUI still shows no early skipped refresh where a few of ours do. The measured reason for the remaining early skips is the GPU's DVFS floor (257 MHz for the first ~120 ms of a gesture), so the working rule is: a transition frame must cost ≤ ~4.5 ms of GPU at 710 MHz. The unchanged Vulkan backend is slower (it serialises CPU and GPU and the clock never ramps under it) and is not a route to the target.

Measure with the phone tools:

- `scripts/measure_android_frames.py` — SurfaceFlinger presentation timestamps for one injected gesture, joined to the shell's markers when the app is launched with `--es makepad.TRACE phone.frames` (`[phone.frames]`, `[phone.input]`, `[phone.scene]` in logcat).
- The bench's `target/perf-artifacts/` helpers (`run_cases.py` for the scenario blocks, `kgsl_gpu_timeline.py` / `kgsl_frames_summary.py` for Adreno GPU execution time and clock per frame from kgsl ftrace) — described in [docs/android/perf-gap-analysis.md](docs/android/perf-gap-analysis.md).
- Three quick taps on the status-bar battery icon toggle the on-device frame monitor; three quick taps on the clock push the island demo, on a bench run only.

Records: [docs/android/](docs/android/README.md) (gap analysis, plan, launcher plan, validation log, Vulkan probe) and the earlier [docs/perf-mobile-shell.md](docs/perf-mobile-shell.md).

## Layout

- `src/mobile*.rs` — the phone shell: state and navigation (`mobile.rs`), the gesture recognizer (`mobile_gestures.rs`), the surface that draws home, drawer, keyboard and overlays (`mobile_surface.rs`), pages, tiles, groups, the shade, the island, the thinking octopus, the perf monitor.
- `src/desk/phone.rs` — the desk's phone composition: hosted-app captures, the kept home scene and its blur pyramid, the compositor path.
- `resources/android/AndroidManifest.xml.template` — the activity (Home role, share and deep-link intents).
- `apps/appcard`, `apps/reference` — the hosted modules built into the APK.
- `docs/` — records and recipes; `docs/android/` the performance and launcher records.

## Dependencies

- Framework: the exact Octoscript-Makepad release selected by `native-runtime.lock.json`. Its `runtime.json` pins Makepad and Octoscript. Cargo patches resolve the prepared siblings, with one widgets/platform/script graph; do not substitute a moving branch.
- `OctoSense-org/Octoscript-AppCard` (`octos-app`, the hosted AppCard) and, through it, `Octoscript`, `Octoscript-Makepad` (component kits) and a few chart/diagram crates.
- The AppCard kernel is not a Cargo dependency: `liboctos.so` is bundled at build time (above).

Tests: `cargo test --features mobile-only mobile -- --test-threads=1` runs the shell's unit tests (gestures, pages, island, shade, groups, tiles). `docs/validation.md` and `docs/android/validation-record.md` hold the device validation.

State lives under `~/.octosense` on desktop and the app's data directory on Android; `OCTOSENSE_HOME` relocates it.

## Robrix Matrix module

Robrix2 is imported into the sibling AppCards repository at
`apps/robrix/native` and linked as `octosense-robrix`. It uses this launcher's
locked Octoscript-Makepad release and opens as an embedded module by default.
Run `cargo run --release -- --test-action launch-robrix`; for the macOS phone
shell add `--features mobile-apps,mobile-only` before `--`.

The AppCards checkout and its `octos` submodule are required. The accompanying
AppCard/octos rusqlite 0.37 update unifies SQLite with the Matrix SDK. Both
launchers patch the legacy AppCard Git dependencies to that canonical checkout.
See the [Robrix app](../Octosense-Service-AppCards/apps/robrix/README.md) for
Android build instructions, the message AppCard format and validation scope.
