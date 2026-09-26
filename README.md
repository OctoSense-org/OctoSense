# OctoSense

## Shared Octoscript-Makepad runtime

`native-runtime.lock.json` selects one
[Octoscript-Makepad](https://github.com/OctoSense-org/Octoscript-Makepad)
release. Its `runtime.json` owns the exact Makepad and Octoscript revisions,
shared with AppCards, Mail and the other OctoSense applications.

Before building, run `python3 tools/setup-native.py` (Python 3.9+). The pinned
repositories are siblings of this app:

| Sibling | Pinned by | What it is |
| --- | --- | --- |
| `../octoscript-makepad` | `native-runtime.lock.json` | the runtime release |
| `../makepad`, `../octoscript` | that release's `runtime.json` | the framework; Makepad carries the reviewed patch in `runtime-patches.lock.json` |
| `../OctoSense-System-Apps` | `native-apps.lock.json` | the system apps' bundles, the Mail host service and the AppCard assistant |

Makepad's patch is [OctoSense-org/makepad#30](https://github.com/OctoSense-org/makepad/pull/30)
(contained script apps, host services), the same one OctoSense ROM's Home
applies; setup applies it to the locked revision and checks the resulting tree.
A clean checkout of the commit the patch was cut from (`source_commit`) is
accepted as the same tree. Local changes are preserved; `--update` only updates
clean checkouts. CI verifies the selected release and rejects duplicate Makepad
and octos sources. Use `python3 tools/setup-native.py --check --cargo-manifest Cargo.toml`
to check the local dependency graph. Existing platform rendering backends remain
part of their applications; the framework controls the shared VM and UI sources.

## App model

OctoSense-Desktop takes its apps the way OctoSense ROM's Home does. There are
four kinds, and the launcher lists them together:

| Kind | Where it comes from | How it runs |
| --- | --- | --- |
| **System apps** — News, Photos, Maps, Camera, Mail | [OctoSense-System-Apps](https://github.com/OctoSense-org/OctoSense-System-Apps) `apps/<name>/bundle`, chosen by [system-apps.json](system-apps.json) and packed into the build | contained Splash programs (ADR 0004), each in its own isolate under the permissions its manifest asks for, run by App Hub's Card runner (the `card` module); ids `os.<name>`, launcher ids `<name>` |
| **Store apps** | the signed App Hub catalog, installed from the App Hub store (`apphub`) | the same Card runner, under `hub:<manifest-id>`; each open is checked against the catalog, and an update closes the old instances |
| **Native modules** | Rust crates linked into this binary | in-process `AppModule`s; only trusted ones: App Hub (store and Card runner), the AppCard assistant, Reference, and the optional `app-*` features |
| **Developer programs** | [config/apps.json](config/apps.json): Reference and Makepad's own apps (Browser, Files, Terminal, Sheets, Notes, …) | separate processes in tiles, built on first launch (see *Add an app*) |

App Hub is on by default (feature `app-hub`): it links
[`octosense-app-hub-app`](https://github.com/OctoSense-org/OctoSense-App-Hub/tree/4605128d46fb982828d8198e0d71d62a39c7d6d6/crates/app-hub-app)
and the Mail host service (`octosense-mail-service`, in
`../OctoSense-System-Apps/apps/mail/host-service`), which keeps Mail's accounts
and passwords on the host; the Mail app never gets a socket or a secret. A
system app takes precedence over a catalog row of the same id, so Makepad's
example **Mail** and **Photos** programs are no longer in `config/apps.json`
or `config/apps.makepad.json` (`drop` in `config/apps.overlay.json`); if a
personal catalog still lists them, the system app replaces the row. A linked
native module of the same id wins over a system app, for comparison builds
(`--features app-photos` links Makepad's native Photos).

`.cargo/config.toml` sets `OCTOSENSE_SYSTEM_APPS` to `system-apps.json`;
without it the build ships no system apps. The desktop has no photo library of
its own, so `system-apps.json` mounts no `photos` assets: Photos shows its
sample library from the thumbnails its bundle ships (`thumbs/`, every photo),
and a photo opened full size has no full-resolution file behind it. Home
mounts its own `apps/photos/resources/photos` (87 MB) for that; a desktop that
wants it adds `"assets": {"photos": {"photos": "<dir>"}}` to its selection.

To try Mail without an account or the keychain, run with its demo mailbox
(a file vault, no network, password `demo`):

```sh
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release
```

With a real account, Mail stores passwords in the macOS keychain; an
unsigned development binary is a new program to the keychain after every
rebuild, so macOS asks again each time. `OCTOSENSE_MAIL_VAULT=file` keeps
them in a 0600 file in Mail's host directory instead, for development.


A Makepad desktop that hosts compatible applications inside one window. The shell comes from Makepad's WM app; framework libraries remain external Cargo dependencies pinned to the same upstream commit.

## Run

Install stable Rust and the native development tools for your OS. On macOS, install Xcode Command Line Tools (`xcode-select --install`) if needed. Validated with Rust 1.98.1 on macOS.

From this directory:

```sh
cargo run
```

Open **Apps → Reference** from the top-left menu. The reference app has a counter and text input, and runs as a separate process inside a tile. Its first launch builds the release executable; build progress appears in the tile. Later launches reuse Cargo's build cache. To build both executables ahead of time:

```sh
cargo build --release --workspace
cargo run --release
```

Prepare the sibling framework sources with the setup command above; the first build downloads any remaining dependencies. The host and Reference app need no Studio process, model download, or wallpaper download. The default apps resolve through Cargo's dependency graph and build on first launch from the same Makepad checkout as the host; unavailable apps are hidden. Fonts and other framework resources are read from that checkout during source development, so keep it available.

On macOS, `.cargo/config.toml` sets the native menu-bar name to **OctoSense**.
Makepad otherwise derives it from the checkout directory, which may still be
named `makeos`. Rebuild and relaunch after updating; Cargo regenerates the
development `Info.plist` automatically.

The build target selects the startup shell automatically: desktop/web builds start with **OctoSense Light** and its bundled wallpaper, Android uses the Android phone layout, and iOS uses the iOS phone layout. You can still switch styles from the shell's style menu. Native phone toolbars are 48 points high and respect the window's safe-area insets.

The default desktop starts empty. AI assistant startup, background app prewarming, demo filesystem generation, and wallpaper downloads are off. **System → Quit OctoSense** closes the desktop and its hosted processes.

Selecting Omarchy uses a bundled Tokyo Night wallpaper, so its background also works offline on a fresh install. Installed images in `~/.octosense/wm/themes/tokyo-night/backgrounds/` take precedence. Use `cargo run -- --download-wallpapers` to fetch the theme’s full wallpaper set; **⌘CtrlSpace** cycles installed backgrounds. Asset provenance is in [resources/wallpapers/README.md](resources/wallpapers/README.md).

Eight desktop styles are available. The default **OctoSense** is a floating desktop with Liquid Glass window frames, dock, bar and popups. Click **Light / Dark** beside the style name in the top bar to switch appearances, or press **⌘Space** and search for **OctoSense Dark** directly. Light uses pearl and pale aqua surfaces with dark text; Dark keeps the ink-blue palette. Both include matching versions of **Abyssal Currents**, the original oceanic wallpaper, and rounded hosted surfaces. The wallpaper switches with the appearance, fills the window with a centered crop and works offline. Android retains its animated background.

Use **⌘Space** for the menu, **⌘W** to close a tile, **⌘F** for tile fullscreen, **⌘1…0** to switch workspaces, and **⌘Shift1…0** to move the focused tile. The menu's **Learn → Keybindings** lists the inherited bindings; shortcuts for apps absent from your catalog report that the app is unavailable.

## Android

With the Makepad Android toolchain installed and a device connected through ADB:

```sh
cargo makepad android run -p octosense --release
```

The Android launcher label is **OctoSense** and its application ID is `dev.makepad.octosense`. It installs separately from an existing MakeOS Android app because the application ID changed.

`run` builds, installs, and launches the app; `build` only creates the APK.
Native Android/iOS builds automatically link **Reference and Sheets** as
embedded apps, and App Hub with the system apps (News, Photos, Maps, Camera,
Mail) through the default `app-hub` feature. They need no extra feature flags
after runtime setup. On an installed device, the launcher derives its default
catalog from those linked modules and system apps. Missing Clock/Weather tiles
give their space to the available app icons.

The phone build also links **AppCard** (`apps/appcard`, feature `app-appcard`
on desktop): the whole AppCard assistant, hosted in-process in a wide home
tile. The module takes `octos-app` — the crate the standalone AppCard APK is
built from — from the pinned OctoSense-System-Apps checkout
(`apps/appcard/app/app`, without its default `standalone` feature; every octos
crate comes from git octos-org/octos at the one rev octos-app pins, and
`OCTOSENSE_WORKSPACE` in `.cargo/config.toml` points its asset embedding at the
sibling framework checkouts) and mounts its `AppShell` widget: the routing
brain, the card store and transport, the L0 lowering pipeline, sessions, the
composer and the kernel agent all run inside the tile's isolate. The kernel
(`liboctos.so serve --stdio`) is spawned from this APK's native library dir
when it is bundled (`MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=<path>"` at
build time); without it the app falls back to its WebSocket transport / login
screen. `ask` is the module's one AI-bus tool (the composer). On a desktop,
`cargo run --features app-appcard -- --module appcard` opens the same app;
`OCTOS_APP_CORE_BIN` / `OCTOS_APP_CORE_DIR` point it at a local kernel.
GPS reaches the app through the buildtool activity described below;
notifications, share and the WebView overlay (the standalone APK's other
Java features) are not wired to the hosted shell yet.

To get AppCard's Java activity features (GPS, notifications, share and
deep-link intents), this repository's `resources/android/AndroidManifest.xml.template`
and the octos kernel bundled as `liboctos.so`, build with the fork's buildtool
`cargo-makepad` and `MAKEPAD_ANDROID_EXTRA_LIBS` instead of the stock command
above — the full recipe, including the kernel cross-build, is in
[docs/android-appcard-build.md](docs/android-appcard-build.md):

```sh
MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=/abs/path/to/octos/target/aarch64-linux-android/release/octos" \
  /abs/path/to/makepad-buildtool/target/debug/cargo-makepad makepad android run -p octosense --release
```

When the AppCard tile starts, the hosted app spawns that kernel and logs
`stdio: octos=… HOME=…` to logcat; an APK built without it logs `stdio:
bundled octos not found under …; using WebSocket transport` and shows the
app's login screen instead.

Switching desktop OctoSense to the Android style changes its interface; it still
uses desktop process hosting and the full desktop catalog. The other desktop
apps (including Browser, Files, Terminal, and AI Chat) need embedded mobile
implementations before they can be bundled in the phone build. Photos includes
the app, not your desktop photo library; local Qwen weights are not packaged.

To exercise the same embedded apps on desktop:

```sh
cargo run --features mobile-apps -- --module reference --module sheets
```

Reference shares its counter and text-input view between the standalone desktop
process and the embedded mobile module. Sheets remains an external Git crate at
the same pinned Makepad revision.

Mobile app support is still partial: Sheets needs grid-label and toolbar fixes.
These follow-ups are tracked in [BACKLOG.md](BACKLOG.md).

The iOS startup policy is covered by tests, but a complete iOS build currently
fails in the pinned Makepad Metal backend; see [validation](docs/validation.md).

## Add an app

The default [config/apps.json](config/apps.json) includes Reference and Makepad's own apps. Plain `cargo run` uses this catalog. An additional copy is available for explicit selection:

```sh
cargo run -- --apps config/apps.makepad.json
```

It includes Reference plus Makepad's Browser, Files, Terminal, Mixer, Task Manager, Sheets, Clock, Weather, Finance, Notes, Calendar, Reminders, Calculator, Fabric, Score, Video Player, Route, VJ, Fab and Director. Image/PDF viewers are registered for file-opening and previews, and AI is registered for the assistant pane (F10). These three helper apps also appear in the launcher unless their IDs (`image`, `pdf`, `aichat`) are listed in `~/.octosense/wm/launcher.hides`.

Makepad's apps carry `"source": "makepad"` instead of a path: they resolve through Cargo's dependency graph to the shared runtime checkout prepared above, or to Cargo's cached checkout when Git dependencies are used without path overrides. Each app builds on demand using its package's normal default features. Those builds go to `~/.octosense/build/makepad` rather than into Cargo's cache, which Cargo alone manages. The catalog uses the workspace root manifest to preserve the apps' expected working directory. Files retains the catalog's `--demo` argument; remove it to browse your real filesystem. Fab uses its built-in demo unless you add explicit file arguments. Upstream replaced Studio with Director; the catalog keeps the `studio` ID for existing launch references and runs `makepad-director`. No apps start automatically; `--assistant` remains opt-in.

Hosted apps and the host therefore always share one revision's framework and protocol code. Reference builds from this repository and is available regardless. A personal `~/.octosense/apps.json` takes precedence over the project default, while `--apps` always selects the named file. Relative manifest paths are based on the catalog's directory, so use absolute paths if moving this catalog into your home directory.

Applications must be compatible Makepad applications that support the `--stdin-loop` hosting protocol. Use the same Makepad revision as this project; the protocol is not a stable compatibility boundary across arbitrary revisions. Start from [apps/reference](apps/reference).

The default catalog is [config/apps.json](config/apps.json). To keep a personal catalog, create `~/.octosense/apps.json`; it replaces the default catalog. An explicit catalog can be selected with:

```sh
cargo run -- --apps /path/to/apps.json
```

A catalog is a JSON array. Each entry chooses exactly one of a Cargo manifest, an installed executable, or a named upstream source:

```json
[
  {
    "id": "notes",
    "label": "Notes",
    "manifest": "../notes/Cargo.toml",
    "package": "my-notes",
    "bin": "notes",
    "policy": "new",
    "args": []
  },
  {
    "id": "installed-notes",
    "label": "Installed Notes",
    "executable": "/opt/my-apps/notes",
    "policy": "focus"
  },
  {
    "id": "browser",
    "label": "Browser",
    "source": "makepad",
    "package": "makepad-browser",
    "bin": "browser",
    "policy": "focus"
  }
]
```

`"source": "makepad"` names the host's Makepad dependency rather than a location: the row resolves through the checkout reported by Cargo, and is skipped on a host that has no such checkout. Relative paths resolve from the catalog's directory. Arguments are passed literally, without a shell. `policy: "new"` opens a new instance; `"focus"` focuses an existing instance and is the default. Restart OctoSense after editing the catalog. Existing personal catalogs should rename `makeos-reference` package/bin entries to `octosense-reference`. Only apps with available launch targets appear in the launcher. Missing manifests, invalid catalogs, and failed starts are reported in the desktop/logs.

The Makepad rows in the shipped catalog are generated from upstream's own registry at the pinned revision, under the named adaptations in [config/apps.overlay.json](config/apps.overlay.json). Check for drift, or regenerate, with:

```sh
python3 scripts/upstream.py catalog
python3 scripts/upstream.py catalog --apply
```

OctoSense adds the hosting arguments and connection settings itself. Do not add `--stdin-loop` or Studio connection variables to the catalog. For executable registrations, supply the app's resources as required by that app's packaging.

## Settings and optional features

OctoSense state lives under `~/.octosense`; `OCTOSENSE_HOME` selects another directory. Existing installations continue using `~/.makeos` if `~/.octosense` does not exist, preserving settings and local model links. The old `MAKEOS_HOME` override remains supported; `OCTOSENSE_HOME` takes precedence. Theme files and hosting overrides retain the upstream `wm/` layout inside that directory. Hosted apps inherit the OctoSense state root through Makepad's compatible `MAKEPAD_HOME` setting.

Optional launch flags are `--assistant`, `--prewarm`, `--demo-home`, and `--download-wallpapers`. Assistant/prewarm flags require matching apps in your catalog. Theme importing remains an explicit action in the theme menu.

Upstream's linked-module infrastructure is retained behind `app-sheets`, `app-photos`, and `app-aichat` Cargo features; all are off by default. A module must be linked and selected with `--module <id>` or `wm/apps.splash`. App Hub's modules are the exception: the store and the apps its Card runner hosts (system and installed apps) have no process form and always open in-process. This initial milestone validates process hosting. It does not provide runtime loading of native shared libraries or embedding of unrelated native desktop windows.

## Local AI setup

Store model weights outside this repository, normally at `~/.octosense/weights/Qwen3.5-9B-UD-Q4_K_XL.gguf`. Each user downloads the model once or references an existing copy; model files and machine-specific symlinks stay out of Git. `OCTOSENSE_HOME` relocates the state directory, and `MAKEPAD_AI_CHAT_MODEL` can select a model file anywhere on disk.

The [local AI setup guide](docs/local-ai.md) covers the assistant app prerequisite, the pinned model download and checksum, reusing existing weights, and checking the **F10** assistant. The desktop and Reference app work without a model.

## Upstream updates

[upstream/makepad.json](upstream/makepad.json) records every imported WM file, its original path/hash, and the matching framework revision. The source and dependency baseline is [official Makepad at 74b63be8](https://github.com/makepad/makepad/commit/74b63be83e101ab3a28d3604df77e9662d50a833). All framework crates, including `libs/wm_api` and `libs/wm_theme`, stay external at that exact Git revision. OctoSense owns its additional style, theme assets and wallpaper behavior locally.

Run this daily, or after any upstream pull. With Python 3.11+, update the Makepad
checkout using your normal Git workflow, then run one command from OctoSense:

```sh
git -C ../makepad pull --ff-only origin work
python3 scripts/upstream.py sync
```

`sync` defaults to the recorded `../makepad` checkout's current `HEAD`.
WM feature development belongs in this repository; framework updates come from
official Makepad. When the checkout's HEAD matches the
recorded revision, it exits without building. Otherwise it requires a clean
OctoSense tree, saves comparison diffs, stages the merge, updates all dependency
pins and the lockfile, runs compile/Rust/Python checks, builds both profiles,
and runs both native smoke modes. It reuses an ignored staging build cache.

After every check passes, it creates a unique `sync/makepad-<revision>` branch
and applies the verified changes, leaving them unstaged and uncommitted. You
then review the diff and report, commit, and merge. Conflicts or failed checks
stop with diagnostics and preserve the live import. The command never pulls,
commits, merges branches, or pushes. Full sync currently requires macOS GUI
access; a headless session cannot pass its native runtime checks.

Reports and captured frames are under `target/makepad-sync/reports/`; the command
prints the exact directory. Use `--source /path/to/makepad` or `--to <commit>`
when needed. See the [full workflow and conflict recovery](docs/upstream.md) for
manual commands and how to investigate a failed candidate.

## Verification and scope

```sh
cargo test --locked --workspace
cargo test --locked --workspace --features mobile-apps
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The native smoke test opens and closes its own test windows, isolates settings in a temporary directory, and records app-provided frames/logs. After dependencies are downloaded:

```sh
cargo build --release --locked --workspace
python3 scripts/smoke.py --styles
python3 scripts/smoke.py --cargo-run --default-catalog
```

The first smoke command checks hosted input, workspace movement, fullscreen resizing, independent instances, all eight desktop styles (including both OctoSense appearances, glass and menus), failed launches, and quitting during an unfinished build. The second uses exactly `cargo run` with the shipped catalog. Python supplies app-local remote control and isolated state through the environment; neither is required for normal use. Smoke runs set Cargo offline and require GUI access.

See the [validation record](docs/validation.md). Source builds and process hosting are the initial target on macOS. Linux/Windows branches are retained but have not been validated here. A relocatable `.app`, installer, web/mobile delivery, and a Linux session compositor are separate work.

OctoSense is licensed under the [Apache License 2.0](LICENSE); see [NOTICE](NOTICE). The copied Makepad source is covered by its [original MIT notice](LICENSES/Makepad-MIT.txt). Dependencies retain their respective licenses.
