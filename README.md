# MakeOS

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

The first build downloads Makepad and other dependencies. The host and Reference app need no sibling Makepad checkout, Studio process, model download, or wallpaper download. The additional default apps use the sibling `../guofoo-makepad` checkout and build on first launch; unavailable apps are hidden. Fonts and other framework resources are read from Cargo's dependency checkout during source development, so keep that cache available.

The default desktop starts empty. AI assistant startup, background app prewarming, demo filesystem generation, and wallpaper downloads are off. **System → Quit MakeOS** closes the desktop and its hosted processes.

Omarchy starts with a bundled Tokyo Night wallpaper, so the background works offline on a fresh install. Installed images in `~/.makeos/wm/themes/tokyo-night/backgrounds/` take precedence. Use `cargo run -- --download-wallpapers` to fetch the theme’s full wallpaper set; **⌘CtrlSpace** cycles installed backgrounds. Asset provenance is in [resources/wallpapers/README.md](resources/wallpapers/README.md).

Eight desktop styles are available, including **MakeOS**, a dark floating desktop with Liquid Glass window frames, dock, bar and popups. Press **⌘Space**, type **MakeOS**, and press Enter to select it. The style includes a bundled vector wallpaper and rounded hosted surfaces; startup remains Omarchy. Select another style from the same appearance menu.

Use **⌘Space** for the menu, **⌘W** to close a tile, **⌘F** for tile fullscreen, **⌘1…0** to switch workspaces, and **⌘Shift1…0** to move the focused tile. The menu's **Learn → Keybindings** lists the inherited bindings; shortcuts for apps absent from your catalog report that the app is unavailable.

## Add an app

The default [config/apps.json](config/apps.json) includes Reference and the apps from the sibling `guofoo-makepad` checkout. Plain `cargo run` uses this catalog. An additional copy is available for explicit selection:

```sh
cargo run -- --apps config/apps.makepad.json
```

It includes Reference plus the fork's Browser, Files, Terminal, Mixer, Task Manager, Sheets, Photos, Clock, Weather, Fabric, Score, Video Player, Route, VJ, Fab and Studio. Image/PDF viewers are registered for file-opening and previews, and AI is registered for the assistant pane (F10). These three helper apps also appear in the launcher unless their IDs (`image`, `pdf`, `aichat`) are listed in `~/.makeos/wm/launcher.hides`.

App source stays in `../guofoo-makepad`; each app builds on demand using its package's normal default features and the source workspace's build cache. The catalog uses the workspace root manifest to preserve the fork apps' expected working directory. Files retains the fork's `--demo` argument; remove it to browse your real filesystem. Fab uses its built-in demo unless you add explicit file arguments. No apps start automatically; `--assistant` remains opt-in.

Keep that checkout at the revision in `upstream/makepad.json` so hosted apps and the host use matching framework/protocol code. Reference remains available independently of that checkout. A personal `~/.makeos/apps.json` takes precedence over the project default, while `--apps` always selects the named file. Relative manifest paths are based on the catalog's directory, so use absolute paths if moving this catalog into your home directory.

Applications must be compatible Makepad applications that support the `--stdin-loop` hosting protocol. Use the same Makepad revision as this project; the protocol is not a stable compatibility boundary across arbitrary revisions. Start from [apps/reference](apps/reference).

The default catalog is [config/apps.json](config/apps.json). To keep a personal catalog, create `~/.makeos/apps.json`; it replaces the default catalog. An explicit catalog can be selected with:

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

Relative paths resolve from the catalog's directory. Arguments are passed literally, without a shell. `policy: "new"` opens a new instance; `"focus"` focuses an existing instance and is the default. Restart MakeOS after editing the catalog. Only apps with available launch targets appear in the launcher. Missing manifests, invalid catalogs, and failed starts are reported in the desktop/logs.

MakeOS adds the hosting arguments and connection settings itself. Do not add `--stdin-loop` or Studio connection variables to the catalog. For executable registrations, supply the app's resources as required by that app's packaging.

## Settings and optional features

MakeOS state lives under `~/.makeos`; `MAKEOS_HOME` selects another directory. Theme files and hosting overrides retain the upstream `wm/` layout inside that directory. Hosted apps inherit the MakeOS state root through Makepad's compatible `MAKEPAD_HOME` setting.

Optional launch flags are `--assistant`, `--prewarm`, `--demo-home`, and `--download-wallpapers`. Assistant/prewarm flags require matching apps in your catalog. Theme importing remains an explicit action in the theme menu.

Upstream's linked-module infrastructure is retained behind `app-sheets`, `app-photos`, and `app-aichat` Cargo features; all are off by default. A module must be linked and selected with `--module <id>` or `wm/apps.splash`. This initial milestone validates process hosting. It does not provide runtime loading of native shared libraries or embedding of unrelated native desktop windows.

## Local AI setup

Store model weights outside this repository, normally at `~/.makeos/weights/Qwen3.5-9B-UD-Q4_K_XL.gguf`. Each user downloads the model once or references an existing copy; model files and machine-specific symlinks stay out of Git. `MAKEOS_HOME` relocates the state directory, and `MAKEPAD_AI_CHAT_MODEL` can select a model file anywhere on disk.

The [local AI setup guide](docs/local-ai.md) covers the assistant app prerequisite, the pinned model download and checksum, reusing existing weights, and checking the **F10** assistant. The desktop and Reference app work without a model.

## OpenHarmony phone

MakeOS also builds as a HarmonyOS/OpenHarmony app: the phone layout hosts every catalog app in-process, since an app sandbox cannot spawn child processes. The default `all-apps` feature links each one as a module (`app-<id>`), so the phone's app drawer lists them all and a tap opens each in its own isolate; a build can also link a subset by naming the features itself. On a device the desktop's phone emulation controls stay out of the way: no style/appearance/rotate toolbar, no drawn keyboard or status bar (the platform's own serve), and the surface is the whole window. Every app carries a phone layout that follows the tile or window width, and the desktop's Android emulation shows the same. Some apps degrade where the sandbox withholds a resource: the terminal has no pseudo-terminal, the task manager cannot read the system process list, and HTTPS is unavailable, so the weather forecast and web features stay empty. Validated on a Huawei Mate 70 Air (HarmonyOS 6.1, API 24).

```sh
scripts/ohos.sh deveco   # cross-compile and generate the DevEco project under target/
scripts/ohos.sh build    # package the HAP with hvigor
scripts/ohos.sh run      # build, install and launch on the connected phone
scripts/ohos.sh hilog    # follow the app's log
```

The script needs DevEco Studio (its SDK, node, hvigor and hdc; `DEVECO_HOME` defaults to the app bundle on macOS), nightly Rust with the `aarch64-unknown-linux-ohos` target, and the sibling `../guofoo-makepad` checkout on its `makeos-ohos` branch. The pinned Makepad revision does not compile for OpenHarmony as-is: the branch adds the platform gates in [ohos/makepad-ohos.patch](ohos/makepad-ohos.patch) (desktop-Linux code that OpenHarmony otherwise inherits, no OpenSSL or xkbcommon in its sysroot, hilog and rawfile font loading). The build patches every Makepad crate to that checkout through a generated `target/ohos/patch.toml`; desktop builds and `Cargo.lock` are left as they were. `cargo-makepad` must be built from the same checkout so its DevEco template matches the platform code.

To inspect the app on the device with makepad's own observability surface, launch with `MAKEOS_OHOS_REMOTE=0.0.0.0:8399 scripts/ohos.sh run` and drive it over the phone's IP: `curl http://<phone-ip>:8399/` lists the routes (`/s` status, `/g?raw=1` a framebuffer grab, `/d` the widget tree, `/snap` visible widgets, `/click`, `/k`, `/log`). The phone must stay awake and the app in the foreground, since the OpenHarmony loop is vsync-driven.

To check every catalog app at a phone size without a device or a display, `scripts/headless_apps.sh [out-dir] [app ...]` renders each one through Makepad Studio's headless mode: the apps built with `MAKEPAD=headless` (the sibling checkout's `target-headless`, plus this workspace's Reference app) are driven over stdin with the Studio protocol (a phone `WindowGeomChange` at 412x866 points, dpi 2, the Android widget style, ticks, then a `Screenshot` request) and each answer lands as `<out-dir>/<app>.png`. The browser's CEF path has no headless build and is left out; the VJ app starts its asset server and never reaches its first frame under the harness.

Because HarmonyOS reserves the bottom-edge swipe for its own system navigation (that swipe backgrounds the whole app), the phone shell shows a tappable Back / Home / Recents bar above the system home indicator on every screen except the home screen, and each card in the app switcher has a close button. Tap Home to leave an app, or open the switcher and tap a card's close button to shut it. This bar appears only on a device; the desktop's Android emulation keeps the swipe.

The phone renders through Makepad's Vulkan backend by default (`MAKEOS_OHOS_GPU=gl` selects OpenGL ES). The backend was Android-only; on OpenHarmony it gets its surface from `VK_OHOS_surface`, its loader from `/system/lib64/libvulkan.so`, and it persists the driver's pipeline cache under the app's data directory, so the first run of a fresh install compiles every pipeline it meets (a few seconds per app on the Maleoon driver) and every later run builds none. Measured on the Mate 70 Air, Vulkan holds the home at a vsync-locked 60 fps on about a tenth of a core against a third for GL, and paints a drag within one vsync.

Route, the map app, needs map data: on a fresh machine it streams Makepad's hosted world archive over HTTP Range requests and offers to download and bake an Amsterdam test map for routing and search. The phone has no HTTP backend yet, so bake the test map on the Mac (`cargo run -p makepad-map-tiles --bin makepad-map-tiles --release -- testmap --dir DIR` in the sibling checkout, about 75 s) and push its three files with the harness upload route into `maps/` under the app's home; a bake found there is adopted at startup. On the Mac the same files go into `local/maps` of this workspace.

To gauge performance on the device with Makepad's own instrumentation, the harness exposes the platform's frame monitor: `curl 'http://<phone-ip>:8399/perf?on=1'` starts collecting and `/perf` returns the last 240 painted frames as JSON, with the frame-to-frame gap percentiles and the main-thread time per channel (event dispatch, GL encode, swap wait, vsync wait, shader compile, and on the phone the touch-to-paint latency). The OpenHarmony loop feeds those channels; the Mac feeds them through Metal. `hiperf record --app <bundle> -s fp` on the device plus `llvm-nm` on the HAP's `libmakepad.so` (the reported `+0x…` offsets are file offsets) names the hot frames.

A commercial HarmonyOS phone only installs a HAP signed with AppGallery Connect material, which DevEco Studio's *Automatically generate signature* creates for one bundle name and the registered device. Point `MAKEOS_OHOS_SIGNING` at that project's `build-profile.json5` and `MAKEOS_OHOS_BUNDLE` at the bundle it is bound to; without them the HAP is built unsigned. `scripts/ohos_project.py` applies these and the other adaptations (labels, INTERNET permission, the XComponent library name, the SDK version) to the generated project after every regeneration.

## Upstream updates

[upstream/makepad.json](upstream/makepad.json) records every imported file, its original path/hash, and the matching framework revision. The source and dependency baseline is [guofoo/makepad at beb3857a](https://github.com/guofoo/makepad/commit/beb3857aea22a6a99fb4a7b6a3b60f92359f6a4d). Its widget changes provide the MakeOS style and glass support; framework code remains external.

Run this daily, or after any upstream pull. With Python 3.11+, update the Makepad
checkout using your normal Git workflow, then run one command from MakeOS:

```sh
git -C ../guofoo-makepad pull --ff-only origin work
python3 scripts/upstream.py sync
```

`sync` defaults to the recorded `../guofoo-makepad` checkout's current `HEAD`.
That fork must incorporate official Makepad updates through your source Git
workflow before they can be imported here. WM feature development now belongs
in this repository; framework changes remain in the pinned fork until available
upstream. When the checkout's HEAD matches the
recorded revision, it exits without building. Otherwise it requires a clean
MakeOS tree, saves comparison diffs, stages the merge, updates all dependency
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

The first smoke command checks hosted input, workspace movement, fullscreen resizing, independent instances, all eight desktop styles (including MakeOS glass and menus), failed launches, and quitting during an unfinished build. The second uses exactly `cargo run` with the shipped catalog. Python supplies app-local remote control and isolated state through the environment; neither is required for normal use. Smoke runs set Cargo offline and require GUI access.

See the [validation record](docs/validation.md). Source builds and process hosting are the initial target on macOS. Linux/Windows branches are retained but have not been validated here. A relocatable `.app`, installer, web/mobile delivery, and a Linux session compositor are separate work.

The copied Makepad source is covered by its [original MIT notice](LICENSES/Makepad-MIT.txt). Dependencies retain their respective licenses.
