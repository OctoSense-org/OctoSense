# OctoSense

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

The first build downloads Makepad and other dependencies. The host and Reference app need no sibling Makepad checkout, Studio process, model download, or wallpaper download. The additional default apps use the sibling `../makepad` checkout and build on first launch; unavailable apps are hidden. Fonts and other framework resources are read from Cargo's dependency checkout during source development, so keep that cache available.

On macOS, `.cargo/config.toml` sets the native menu-bar name to **OctoSense**.
Makepad otherwise derives it from the checkout directory, which may still be
named `makeos`. Rebuild and relaunch after updating; Cargo regenerates the
development `Info.plist` automatically.

The build target selects the startup shell automatically: Android uses the Android phone layout, iOS uses the iOS phone layout, and desktop/web builds keep Omarchy. You can still switch styles from the shell's style menu. Native phone toolbars are 48 points high and respect the window's safe-area insets.

The default desktop starts empty. AI assistant startup, background app prewarming, demo filesystem generation, and wallpaper downloads are off. **System → Quit OctoSense** closes the desktop and its hosted processes.

Omarchy starts with a bundled Tokyo Night wallpaper, so the background works offline on a fresh install. Installed images in `~/.octosense/wm/themes/tokyo-night/backgrounds/` take precedence. Use `cargo run -- --download-wallpapers` to fetch the theme’s full wallpaper set; **⌘CtrlSpace** cycles installed backgrounds. Asset provenance is in [resources/wallpapers/README.md](resources/wallpapers/README.md).

Eight desktop styles are available, including **OctoSense**, a floating desktop with Liquid Glass window frames, dock, bar and popups. Press **⌘Space**, type **OctoSense**, and press Enter to select its light appearance. Click **Light / Dark** beside the style name in the top bar to switch appearances, or search for **OctoSense Dark** directly. Light uses pearl and pale aqua surfaces with dark text; Dark keeps the ink-blue palette. Both include matching versions of **Abyssal Currents**, the original oceanic wallpaper, and rounded hosted surfaces. The wallpaper switches with the appearance, fills the window with a centered crop and works offline. Desktop startup remains Omarchy; Android retains its animated background.

Use **⌘Space** for the menu, **⌘W** to close a tile, **⌘F** for tile fullscreen, **⌘1…0** to switch workspaces, and **⌘Shift1…0** to move the focused tile. The menu's **Learn → Keybindings** lists the inherited bindings; shortcuts for apps absent from your catalog report that the app is unavailable.

## Android

With the Makepad Android toolchain installed and a device connected through ADB:

```sh
cargo makepad android run -p octosense --release
```

The Android launcher label is **OctoSense** and its application ID is `dev.makepad.octosense`. It installs separately from an existing MakeOS Android app because the application ID changed.

`run` builds, installs, and launches the app; `build` only creates the APK.
Native Android/iOS builds automatically link **Reference, Sheets, and Photos**
as embedded apps. They need no sibling checkout or extra feature flags. On an
installed device, the launcher derives its default catalog from those linked
modules. Missing Clock/Weather tiles give their space to the available app icons.

The phone build also links **AppCard** (`apps/appcard`, feature `app-appcard`
on desktop): Phase A of the Octoscript-AppCard port, one L0 weather card
rendered in-process in a wide home tile. The module carries the card's `sys.*`
helpers (live values through the platform's fetch layer, "—" until they land)
and the pre-lowered weather exemplar; the card store, routing brain and the
kernel are later phases. It needs the `appcard,maps` features of the makepad
fork and the Roboto faces under `apps/appcard/resources`. On a desktop,
`cargo run --features app-appcard -- --module appcard` opens the same card.

Switching desktop OctoSense to the Android style changes its interface; it still
uses desktop process hosting and the full desktop catalog. The other desktop
apps (including Browser, Files, Terminal, and AI Chat) need embedded mobile
implementations before they can be bundled in the phone build. Photos includes
the app, not your desktop photo library; local Qwen weights are not packaged.

To exercise the same embedded apps on desktop:

```sh
cargo run --features mobile-apps -- --module reference --module sheets --module photos
```

Reference shares its counter and text-input view between the standalone desktop
process and the embedded mobile module. Sheets and Photos remain external Git
crates at the same pinned Makepad revision.

Mobile app support is still partial: Sheets needs grid-label and toolbar fixes,
and Photos needs a picture library/import setup. These follow-ups are tracked
in [BACKLOG.md](BACKLOG.md).

The iOS startup policy is covered by tests, but a complete iOS build currently
fails in the pinned Makepad Metal backend; see [validation](docs/validation.md).

## Add an app

The default [config/apps.json](config/apps.json) includes Reference and the apps from the sibling `makepad` checkout. Plain `cargo run` uses this catalog. An additional copy is available for explicit selection:

```sh
cargo run -- --apps config/apps.makepad.json
```

It includes Reference plus Makepad's Browser, Files, Terminal, Mixer, Task Manager, Sheets, Photos, Clock, Weather, Fabric, Score, Video Player, Route, VJ, Fab and Director. Image/PDF viewers are registered for file-opening and previews, and AI is registered for the assistant pane (F10). These three helper apps also appear in the launcher unless their IDs (`image`, `pdf`, `aichat`) are listed in `~/.octosense/wm/launcher.hides`.

App source stays in `../makepad`; each app builds on demand using its package's normal default features and the source workspace's build cache. The catalog uses the workspace root manifest to preserve the apps' expected working directory. Files retains the catalog's `--demo` argument; remove it to browse your real filesystem. Fab uses its built-in demo unless you add explicit file arguments. Upstream replaced Studio with Director; the catalog keeps the `studio` ID for existing launch references and runs `makepad-director`. No apps start automatically; `--assistant` remains opt-in.

Keep that checkout at the revision in `upstream/makepad.json` so hosted apps and the host use matching framework/protocol code. Reference remains available independently of that checkout. A personal `~/.octosense/apps.json` takes precedence over the project default, while `--apps` always selects the named file. Relative manifest paths are based on the catalog's directory, so use absolute paths if moving this catalog into your home directory.

Applications must be compatible Makepad applications that support the `--stdin-loop` hosting protocol. Use the same Makepad revision as this project; the protocol is not a stable compatibility boundary across arbitrary revisions. Start from [apps/reference](apps/reference).

The default catalog is [config/apps.json](config/apps.json). To keep a personal catalog, create `~/.octosense/apps.json`; it replaces the default catalog. An explicit catalog can be selected with:

```sh
cargo run -- --apps /path/to/apps.json
```

A catalog is a JSON array. Each entry chooses either a Cargo manifest or an executable:

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
  }
]
```

Relative paths resolve from the catalog's directory. Arguments are passed literally, without a shell. `policy: "new"` opens a new instance; `"focus"` focuses an existing instance and is the default. Restart OctoSense after editing the catalog. Existing personal catalogs should rename `makeos-reference` package/bin entries to `octosense-reference`. Only apps with available launch targets appear in the launcher. Missing manifests, invalid catalogs, and failed starts are reported in the desktop/logs.

OctoSense adds the hosting arguments and connection settings itself. Do not add `--stdin-loop` or Studio connection variables to the catalog. For executable registrations, supply the app's resources as required by that app's packaging.

## Settings and optional features

OctoSense state lives under `~/.octosense`; `OCTOSENSE_HOME` selects another directory. Existing installations continue using `~/.makeos` if `~/.octosense` does not exist, preserving settings and local model links. The old `MAKEOS_HOME` override remains supported; `OCTOSENSE_HOME` takes precedence. Theme files and hosting overrides retain the upstream `wm/` layout inside that directory. Hosted apps inherit the OctoSense state root through Makepad's compatible `MAKEPAD_HOME` setting.

Optional launch flags are `--assistant`, `--prewarm`, `--demo-home`, and `--download-wallpapers`. Assistant/prewarm flags require matching apps in your catalog. Theme importing remains an explicit action in the theme menu.

Upstream's linked-module infrastructure is retained behind `app-sheets`, `app-photos`, and `app-aichat` Cargo features; all are off by default. A module must be linked and selected with `--module <id>` or `wm/apps.splash`. This initial milestone validates process hosting. It does not provide runtime loading of native shared libraries or embedding of unrelated native desktop windows.

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

The copied Makepad source is covered by its [original MIT notice](LICENSES/Makepad-MIT.txt). Dependencies retain their respective licenses.
