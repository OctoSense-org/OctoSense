# OctoSense Home

English | [简体中文](README.zh-CN.md)

**New to the code?** Read the [desktop, Home, ROM and system-app walkthrough](../desktop/docs/code-walkthrough.md), then the [agent and Tokio walkthrough](../docs/architecture-walkthrough.md). The first follows launch, native hosting, script bundles, app data and Android platform boundaries.

> **Where this fits.** On a phone, OctoSense-hosted native modules and script apps run in the Home process (no desktop-style hosted process apps); ordinary Android apps still run in their own Android processes. The octos kernel is the APK's `liboctos.so` as a child process on Android, an in-process task on OpenHarmony, and absent on iOS. Apps still reach their agents only through the shell. Diagrams of the processes, an app agent's two lanes and a tool call with its approval: [How it fits together](../README.md#how-it-fits-together); the details: [docs/architecture.md](../docs/architecture.md) and [ADR 0004](../docs/adr/0004-native-apps-hosting-and-peers.md).

The OctoSense phone shell: a Makepad app that is the device's Home screen.
Home pages with live tiles and app pairs, a gesture layer, the shade
(notifications left, controls right), Recents, a live island for ongoing
activities, and hosted apps drawn in-process inside its tiles: App Hub and
the apps it runs, the system apps, Reference and Sheets, and the octos agent
kernel as a service. (AppCard is not shipped for now; it links only with
`--features app-appcard`.)

Home is one of the three products in this repository (the
[root README](../README.md) has setup, the layout and CI). The ROM image that
preinstalls it with the privileged system side is [`rom/`](../rom/README.md).

## Relation to the desktop shell

Home was split from the desktop shell on 15 September 2026, at the tip of its
mobile shell chain, and the two copies lived in OctoSense-Desktop and
OctoSense-ROM (retired; merged into this repository), under `home/`, until both repositories merged into this one on
27 September 2026 ([ADR 0001](../docs/adr/0001-one-octosense-repository.md)).
Since then the shell exists once, in [`crates/shell`](../crates/shell)
(package `octosense-shell`), which both packages link; this package adds
the entry point (`src/main.rs`, an `App` that wraps the shell's) and the
built-in Settings app (`src/settings_*.rs`, `src/android_settings.rs`). The
phone build is the `mobile_only`
configuration: `build.rs` turns it on for Android, and `--features
mobile-only` turns it on elsewhere. `upstream/makepad.json` records which
window-manager files were imported from Makepad; `scripts/upstream.py`
compares and merges them ([docs/upstream.md](docs/upstream.md)).

## Build and run

Prepare the pinned sources once from the repository root
(`python3 tools/setup.py`, see the [root README](../README.md#set-up)), then
run cargo from this directory: `phone/.cargo/config.toml` selects the
phone's system apps.

**On a desktop**, the phone shell in a phone-sized window with App Hub, the
seven system apps and the built-in Settings (only macOS is built in CI):

```sh
cargo run --release -p octosense-home --features mobile-only
```

| Switch | Effect |
| --- | --- |
| `--features mobile-apps` | Also link the native modules Reference and Sheets; not AppCard |
| `--features app-appcard` | Also link the AppCard assistant, which is not shipped by default for now |
| `-- --module <id>` | Host a linked module in-process instead of as a child process |
| `-- --test-action <name>` | Fire a shell action at startup (see [Run on a desktop](#run-on-a-desktop)) |
| `MAKEPAD_WM_TEST_APP=<app>[:<count>]` | Launch an app (count times) once the shell is up |
| `MAKEPAD_APP_CONFIG='{"mail_demo":true}'` | Serve Mail from a demo mailbox (password `demo`) |
| `OCTOSENSE_HOME=<dir>` | Keep state somewhere other than `~/.octosense` |

**Android APK.** `rom/scripts/build-home.sh` (a wrapper for
`build-home.py`) builds the Home APK and its System Bridge APK, signs them
together, bundles the octos kernel as `liboctos.so`, and writes
`OctoSenseHome.apk`, `OctoSenseBridge.apk` and a `build.json` receipt. It never
installs or flashes. A standalone development pair, signed with Makepad's
development key, from the repository root:

```sh
cargo build --release --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml
rom/scripts/build-home.sh --variant standalone --development \
  --sdk /path/to/makepad-android \
  --android-sdk /path/to/android-sdk \
  --gradle-home /path/to/gradle-8.11.1 \
  --java-home /path/to/full-jdk \
  --packager .sources/makepad/target/release/cargo-makepad
```

You need a `cargo-makepad` Android SDK/NDK directory (`cargo-makepad makepad
android --sdk-path=<dir> install-toolchain`), the Android SDK with platform 35
and build-tools 35.0.0, a full JDK 17+ and Gradle 8.11.1; the script installs
none of them. Add `--dry-run` to print the plan; for a release, replace
`--development` with `--sign-key` and `--sign-cert` pointing at the existing
signer, kept outside the checkout. Use the pinned packager, not upstream's: it
carries this app's Java activity ([docs/build-tool.md](docs/build-tool.md)).
Signing, receipts and the ROM variant: [rom/docs/home-build.md](../rom/docs/home-build.md).

**Package name.** Home's application ID is `dev.makepad.octosense`, in both
the standalone and the ROM variant. A phone running the OctoSense ROM already
has that ID, signed with the platform key, so a development build cannot
replace it. To install a test build beside it, call the packager with another
package name, from `phone/`:

```sh
../.sources/makepad/target/release/cargo-makepad makepad android \
  --sdk-path=/path/to/makepad-android \
  --package-name=dev.makepad.octosense.scriptapps \
  build -p octosense-home --release
```

`run` in place of `build` also installs and starts it; address it with its own
name, for example `adb shell am start -n dev.makepad.octosense.scriptapps/.MakepadApp`.

**OpenHarmony:** `python3 rom/scripts/build-home-ohos.py --deveco-home ...
--packager ... --signing-config ...` builds a normal OpenHarmony app with an
existing DevEco signing profile
([rom/docs/home-build.md](../rom/docs/home-build.md#openharmony-home)).
**iOS simulator:** from `phone/`,
`../.sources/makepad/target/release/cargo-makepad makepad apple ios --org=dev.makepad --app=octosense run-sim -p octosense-home --features mobile-only`.
Neither is built in CI.

## Application icons

Home uses the same green eight-petal OctoSense mark as the desktop package:

| Platform | Packaged artwork |
| --- | --- |
| Android | Five legacy launcher densities, adaptive foreground/background layers, and Android 13 monochrome artwork for themed icons; the normal manifest names these resources |
| iOS/iPadOS | `packaging/ios/icons/Assets.xcassets`, with correctly sized iPhone, iPad and App Store PNGs; opaque squares let iOS apply the corner mask |
| OpenHarmony | `ohos/icons/`, copied by `rom/scripts/build-home-ohos.py` over the pinned template's application, layered launcher and launch-window icons |
| Desktop preview | `resources/icon_*.png`, `icon.ico` and `icon.icns`, plus the workspace's Cargo icon environment |

The Android foreground fits the platform's
[adaptive-icon safe area](https://developer.android.com/develop/ui/compose/system/icon_design_adaptive).
The iOS catalog supplies the
[Apple app-icon slots](https://developer.apple.com/library/archive/documentation/Xcode/Reference/xcode_ref-Asset_Catalog_Format/AppIconType.html)
directly, avoiding Makepad's fallback that reuses one image at every size.
Normal builds consume committed assets. Regeneration and checks are documented
in [Application icons](../desktop/README.md#application-icons).

Android resources and iOS catalogs compile with the local platform tools;
OpenHarmony template replacement has a packaging regression test. Home was
freshly installed as `OctoSense Icons` (`dev.makepad.octosense.icontest`) on a
Pixel 7 Pro running Android 17: App info displays the branded adaptive icon
and the Home screen renders after launch. The original installation and
default Pixel Launcher were preserved. Android themed-icon mode, installed
iOS/OpenHarmony appearance and a complete OpenHarmony HAP build remain
**unverified**.

`OctoSense Icons` was a command-line label override for that separate test
installation. Normal Android/iOS/desktop builds remain named **OctoSense**;
OpenHarmony retains its existing **OctoSense Home** name.

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

On Android the top edge is the system's: it opens the system-wide OctoSense panel when that is on (the OctoSense ROM; Home's side pulls then open search), and Android's own panel otherwise. OpenHarmony has no shade.

A pull commits from 40 % of the way (≈135 px on a 1080-wide phone); navigation swipes need the full distance or a flick. While a pull is in flight the page dims and a search field rises from the bottom with the finger; a committed gesture gives a short haptic tick. Until each hidden gesture has been used once, the home page shows a one-line hint for it (`crates/shell/src/mobile_hints.rs`; Android remembers what was seen). A second Home press on a settled home page returns to the primary page.

Search opens only by pulling down on Home; the App Library has no search bar. As in iOS, the search field sits at the bottom above the keyboard, the list stays empty until you type, and every keystroke narrows it: an app matches when its name, or a word in it (a capital inside a word counts, so "tube" finds YouTube), starts with what you typed, ignoring case and accents. Names that start with it come first, and Return opens the best match. In the App Library, a letter column on the right jumps the grid, and with usage access a "Suggested" row of recently used apps sits on top. Icons carry a dot while their app has a notification in the shade. Recents lists the hosted apps as cards and, with usage access granted in Android's Settings (the card in Recents opens it), a row of the Android apps used lately. Every tappable region is an accessibility node with a spoken label, so TalkBack and UI automation can read and activate the shell (verified with TalkBack installed and with a UiAutomation probe: accessibility focus lands on a node and its click action opens the app, the shade or the drawer; note that `adb shell input` taps bypass TalkBack's touch exploration, so a real screen-reader touch cannot be scripted). Labels follow Android's text size setting. The shell follows Android's dark theme and draws under transparent system bars; the shade's Dark mode tile overrides the appearance until the system setting next changes. The bridge's failure reasons reach the person as plain sentences (`result_copy` in `crates/shell/src/android_integration.rs`), never as reason codes.

## Built-in Settings

Open **OctoSense Settings** in the app catalog for shared themes, supported
display and sound controls, and device information. Its Octoscript–Makepad UI
follows live theme and text-size changes while preserving the current page.
Navigation, search, drafts, reviews and application event handlers execute in
[Octoscript controllers](resources/settings/controller); native code retains
rendering, text input and typed Android bindings. See the
[port design and validation status](../docs/adr/home/0005-settings-octoscript-controller.md).
Complete system Settings replacement is in progress; some areas still open
Android Settings. See the [current controls and validation](docs/android/settings.md),
[feature parity checklist](docs/android/settings-parity.md), and
[architecture decision](../docs/adr/home/0006-builtin-settings.md).

## System apps

News, Photos, Maps, Camera, Mail, AI providers and YouTube are contained script apps
([ADR 0004](../docs/adr/home/0004-system-apps-are-contained-script-apps.md)). Their
bundles live in [`apps/`](../apps/README.md) (`apps/<name>/bundle/`);
this directory's `system-apps.json` names which this Home ships and
mounts the artwork Home owns (Photos' sample library,
`apps/photos/resources/photos`). App Hub's Card runner runs each in its own
isolate under its manifest's policy, in the standalone Home and in the ROM
alike. Each keeps its short launcher id (`news` for `os.news`), so icons,
tiles and the dock are unchanged.

Mail reads and sends through the `mail` host service
([`apps/mail/host-service`](../apps/mail/host-service)): the person signs in on the host's own sheet, the
password stays in the keychain or behind an Android Keystore key, and the app
never holds a socket or a password. For a demo mailbox (password `demo`):

```sh
# desktop, from phone/
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release -p octosense-home --features mobile-only
# phone
adb shell am start -n <package>/.MakepadApp --es makepad.APP_CONFIG '{"mail_demo":true}'
```

The earlier native News, Photos and Maps modules are deleted (native-apps ADR 0004 §1, [#113](https://github.com/OctoSense-org/OctoSense/pull/113));
none of the system apps has a native module any more.

### The octos kernel

The octos agent kernel is a Home service, independent of any app:
`octosense-kernel` ([`crates/kernel`](../crates/kernel)), feature
`octos-core` (default, and always on in Android, iOS and OpenHarmony
builds). Home starts it at startup with its data dir through the shell's AI
services ([`crates/ai-host`](../crates/ai-host/README.md)); nothing runs until a consumer
connects. Then there is one kernel per process: `liboctos.so serve --stdio`
from the APK's native lib dir on Android (every APK `rom/scripts/build-home.sh`
builds carries it), the core in-process on OpenHarmony, the binary named by
`OCTOS_APP_CORE_BIN` on a desktop (none otherwise), none on iOS. Its core
dir is `<data dir>/octos-home/.octos` on a phone and `OCTOS_APP_CORE_DIR`,
else `~/octos-home/.octos`, on a desktop. The AI providers app configures it
(below); AppCard (opt-in) and, next, Rinx connect to it and share it; it
stops when the last one leaves and on Home's shutdown. To build without it
(desktop only): `--no-default-features` plus the features you want, e.g.
`--features app-hub`.

**Talk to Octos** (off by default): **AI providers → Talk to Octos** turns on a loopback server so a web client or a terminal UI can talk to this device's assistant. While it is on, the kernel runs as `octos serve --host-managed` instead of `--stdio` and native apps keep working over its WebSocket; external clients get a separate token that opens the UI Protocol socket and nothing else. A web client pairs with a one-time code or the QR of its link; a terminal client of this user reads the private connection file. The server stays up when native apps close, until it is turned off or the shell exits. See [ADR 0003](../docs/adr/0003-shared-octos-client-access.md) and the [kernel guide](../crates/kernel/README.md).

### AI providers

AI providers (`os.ai-providers`) edits the octos kernel's LLM providers
through the `llm` host service ([`apps/ai-providers/host-service`](../apps/ai-providers/host-service)), which Home
registers at startup through [`crates/ai-host`](../crates/ai-host/README.md):

- the profile it writes is the kernel's, `<core dir>/profiles/_main.json`
  (`<data dir>/octos-home/.octos` on a phone; `OCTOS_APP_CORE_DIR` overrides
  it); on Android the keys are in that app-private profile, since octos
  reads them there;
- on Android the import sheet can **scan** a provider QR with the camera
  (Makepad's `cx.show_qr_scanner()`, makepad#31, in the runtime since
  `d0a9def5`) or read one from a **chosen image**
  (`QrImagePickActivity`: the system picker, the bytes handed over in a
  private cache file on the `qr.image.result` packet); elsewhere it takes a
  pasted code;
- after any change the service restarts the kernel (if one runs): its
  consumers reconnect to a fresh kernel that reads the new profile (AppCard
  keeps its window and sessions; a request in flight fails with "the octos
  kernel restarted").

## App Hub

App Hub (`apphub`) browses the signed OctoSense catalog, searches, shows app
details, installs verified bundles and keeps an installed-app Library.
Installed apps open in contained Card instances (`card`) and appear
separately in the launcher and Recents. Both come from App Hub's shared shell
crate `octosense-app-hub-app` (OctoSense-App-Hub `crates/app-hub-app`), linked
by the default `app-hub` feature and on every mobile build. The **Preview
catalog** switch shows the built-in apps while the live catalog is empty.

See the crate's
[README](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/crates/app-hub-app/README.md)
(read the revision selected by the root `Cargo.toml`) and the [native design evidence](docs/design/app-hub/README.md).
App authors start with
[OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow).

## Run on a desktop

The same shell in a phone-sized window, on Metal, DirectX or OpenGL:

```sh
cargo run --release -p octosense-home --features mobile-only
cargo run --release -p octosense-home --features mobile-only -- --test-action island:demo --test-action capture:/tmp/shell.png
```

`--test-action` pushes fixtures (`island:demo`, `island:expand`, `page:<n>`,
`ask-appcard:<text>`, `launch-<app id>`, `taps:<x>,<y>@<s>`), and
`capture:<path>` writes the presented frame every 5 s, so a scripted run can
be inspected without a screen. `MAKEPAD_APP_CONFIG='{"test_actions":[...]}'`
passes the same list where arguments cannot be given. Without `mobile-only`
this package starts the universal desktop shell; the desktop product is
[`desktop/`](../desktop/README.md). It starts in **OctoSense Light** with
its bundled wallpaper; Omarchy and the other styles remain in the style menu.

## Performance

Target on the OnePlus 6 (Android 15, Adreno 630, 60 Hz): **≥ 55 fps with p95 frame intervals ≤ 20 ms** on every shell transition, and an idle screen that presents about once a second. As of 16 September 2026 the shade (open/close), pages, Group open/close, Recents both ways (empty and populated) and AppCard opening pass warm and fresh-process blocks; native SystemUI still shows no early skipped refresh where a few of ours do. The measured reason for the remaining early skips is the GPU's DVFS floor (257 MHz for the first ~120 ms of a gesture), so the working rule is: a transition frame must cost ≤ ~4.5 ms of GPU at 710 MHz. The unchanged Vulkan backend is slower (it serialises CPU and GPU and the clock never ramps under it) and is not a route to the target.

Measure with the phone tools:

- `scripts/measure_android_frames.py` — SurfaceFlinger presentation timestamps for one injected gesture, joined to the shell's markers when the app is launched with `--es makepad.TRACE phone.frames` (`[phone.frames]`, `[phone.input]`, `[phone.scene]` in logcat).
- The bench's `target/perf-artifacts/` helpers (`run_cases.py` for the scenario blocks, `kgsl_gpu_timeline.py` / `kgsl_frames_summary.py` for Adreno GPU execution time and clock per frame from kgsl ftrace) — described in [docs/android/perf-gap-analysis.md](docs/android/perf-gap-analysis.md).
- Three quick taps on the status-bar battery icon toggle the on-device frame monitor; three quick taps on the clock push the island demo, on a bench run only.

Records: [docs/android/](docs/android/README.md) (gap analysis, plan, launcher plan, validation log, Vulkan probe) and the earlier [docs/perf-mobile-shell.md](docs/perf-mobile-shell.md).

## Layout

- `src/main.rs`: the entry point: this package's `App` wraps the shell's
  (`#[deref] shell`) and adds the Settings runtime.
- `src/settings_*.rs`, `src/android_settings.rs`, `resources/settings/`: the
  built-in Settings app and its Android channels.
- `../crates/shell/src/mobile*.rs`: the phone layer of the shell. State and navigation (`mobile.rs`), the
  gesture recognizer (`mobile_gestures.rs`), the surface that draws home,
  drawer, keyboard and overlays (`mobile_surface.rs`), pages, tiles, groups,
  the shade, the island, the thinking octopus, the perf monitor.
- `../crates/shell/src/apps.rs`: which modules this build links, the system apps and
  installed apps as launcher rows, and how each is hosted.
- `../crates/shell/src/desk/phone.rs`: the desk's phone composition: hosted-app captures, the
  kept home scene and its blur pyramid, the compositor path.
- `resources/android/AndroidManifest.xml.template`: the activity (Home role,
  share and deep-link intents).
- `../crates/shell/resources/icons/apps/<style>/`: the shell's own icons for
  News and OctosMap, one 64x64 SVG per framework style, written by
  `python3 tools/build_app_icons.py` (`--sheet <path>` also renders a review
  sheet with `rsvg-convert`; **unverified** since the move: the script still
  writes `phone/resources/icons/apps/`). The renderer has no clip paths, masks, filters
  or text, so the art stays inside its tile by construction; a test holds the
  files to that.
- `../apps/appcard/module`: hosts the AppCard assistant (`octos-app`, in
  `../apps/appcard/app/app`). Opt-in only, on every target:
  `--features app-appcard` (it implies `octos-core`; the assistant connects
  to Home's kernel). Default, `mobile-apps` and native mobile builds leave
  the AppCard UI out, not the kernel service.
- `../apps/reference`: the reference module.
- `android/`: the System Bridge, contracts, Quickstep and SystemUI projects
  ([android/README.md](android/README.md)).
- `docs/`: records and recipes; `docs/android/` the performance and launcher
  records. The Home decisions (ADRs 0001–0006) are in
  [`../docs/adr/home/`](../docs/adr/README.md).

## Dependencies

Every external dependency is pinned once, at the repository root
([root README](../README.md#what-it-depends-on)):

- Framework: the Octoscript-Makepad release selected by
  `native-runtime.lock.json`; its `runtime.json` pins Makepad and OctoScript,
  checked out in `.sources/` with the reviewed Settings patch
  (`runtime-patches.lock.json`, `tools/runtime-patches/`). Every Makepad crate
  resolves to `.sources/makepad`, so the graph has one widgets/platform/script.
  How the fork relates to upstream Makepad and how a pin moves:
  [docs/makepad-fork.md](docs/makepad-fork.md).
- App Hub: `octosense-app-hub-app` and its backend crates, one revision for
  Home and the Mail and `llm` host services.
- In this repository, by path: the system-app bundles and host services
  (`apps/`), the shell (`crates/shell`) and its AI services
  (`crates/ai-host`), the octos kernel service (`crates/kernel`), the app-agent broker
  (`crates/app-peers`) and `octos-app` (`apps/appcard`). octos itself comes
  from `octos-org/octos` at one revision: as a Cargo dependency only on
  OpenHarmony (the in-process core) and in `app-appcard` builds.
- The kernel binary is not a Cargo dependency on Android: `liboctos.so` is
  cross-built from that revision and bundled at APK build time with
  `MAKEPAD_ANDROID_EXTRA_LIBS` by `rom/scripts/build-home.sh` (`--octos-kernel`
  for a prebuilt one, `--no-octos-kernel` for none;
  [docs/android-appcard-build.md](docs/android-appcard-build.md)). Without
  it the phone runs no kernel; the providers are still saved, and AppCard
  (if linked) falls back to its WebSocket transport and login screen.

## Tests and state

`cargo test --locked --features mobile-apps -p octosense-shell -p
octosense-home` runs the shell's unit tests (gestures, pages, island, shade,
groups, tiles) and Settings'. The full CI set is
`.github/workflows/phone.yml` (compile Home and its bundled modules, the
shell graph guards in `tools/check-shell-graph.sh`, and the tests of the
shell, Home, the AI services, App Hub admission and runtime policy).
`scripts/smoke.py` launches a release build under `MAKEPAD_REMOTE` and drives
it over HTTP. [docs/validation.md](docs/validation.md) and
[docs/android/validation-record.md](docs/android/validation-record.md) hold
the device validation.

State lives under `~/.octosense` on desktop and in the app's data directory on
Android; `OCTOSENSE_HOME` relocates it.
