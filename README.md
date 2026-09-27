# OctoSense Desktop

English | [简体中文](README.zh-CN.md)

OctoSense-Desktop is the desktop shell of [OctoSense](https://github.com/OctoSense-org), the agent shell on top of your operating system. It is one Makepad window that is the desktop: a launcher, a dock and tiles, hosting system apps and App Hub store apps as contained script programs, trusted native modules in-process, and Makepad developer programs as child processes. It gets its apps the same way the phone shell, OctoSense-ROM's `home/`, does.

**Building an OctoSense app?** You do not need this repository to build, check or publish one: start at the [OctoSense-org profile](https://github.com/OctoSense-org)'s reading list (OctoScript-App-Design-Flow's `AGENTS.md`, then `docs/QUICKSTART.md`). Build this shell only if you want to see your app in the desktop shell before it is published ([Try your own app](#try-your-own-app-before-it-is-published)).

## Where it sits

| Repository | Role for this repo |
| --- | --- |
| [OctoSense-ROM](https://github.com/OctoSense-org/OctoSense-ROM) | The phone shell (`home/`). Same app model, same runtime, same system apps. |
| [OctoSense-System-Apps](https://github.com/OctoSense-org/OctoSense-System-Apps) | News, Photos, Maps, Camera, Mail and AI providers bundles, the Mail and `llm` host services, the octos kernel service (`crates/octos-core`) and the AppCard assistant (`octos-app`, opt-in, not shipped by default). Pinned sibling checkout. |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | The signed catalog, the store and the Card runner. Linked as the Git crate `octosense-app-hub-app`. |
| [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) | Where apps are designed, built and published to the App Hub. |
| [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad) | The runtime release that pins Makepad and OctoScript. Pinned sibling checkout. |
| [makepad (OctoSense fork)](https://github.com/OctoSense-org/makepad) | The framework. Pinned sibling checkout. |
| [octos](https://github.com/octos-org/octos) | The agent kernel, a shell service (`octos-core`, on by default): AI providers configures it, AppCard and other consumers connect to it. One revision (`6ad76e5c`, the one System-Apps pins), in `Cargo.lock`; the kernel itself is a separate binary (desktop: `OCTOS_APP_CORE_BIN`; Android: bundled `liboctos.so`). |

## Repository layout

| Path | What it is |
| --- | --- |
| `src/` | The shell (crate `octosense`): tiling, launcher, dock, bar, hosting (`host.rs`, `hub.rs`, `module_host.rs`), app registry (`apps.rs`). |
| `src/shell/` | Bar, launcher, menus, notifications, AI pane, gallery. |
| `src/octosense/` | OctoSense-specific parts: app catalog loading, state paths, styles, Makepad source resolution. |
| `apps/reference/` | `octosense-reference`, a small counter/text-input app that runs both as a hosted process and as a linked module. |
| `apps/appcard/` | `octosense-appcard`, the module that mounts the AppCard assistant (`octos-app`) in a tile. Opt-in (`app-appcard`); not shipped for now. |
| `config/apps.json` | The default developer-program catalog. `apps.makepad.json` is an identical copy for `--apps`; `apps.overlay.json` holds the adaptations applied when regenerating them. |
| `system-apps.json` | Which system apps this build packs, and from where. |
| `native-runtime.lock.json` | The OctoScript-Makepad release (and through it, Makepad and OctoScript). |
| `native-apps.lock.json` | The OctoSense-System-Apps revision. |
| `runtime-patches.lock.json` | Reviewed patches on top of the pinned Makepad, with their expected trees. Empty today: the pinned Makepad carries everything the shell needs. |
| `tools/setup-native.py` | Prepares and checks the pinned sibling checkouts. |
| `scripts/` | `upstream.py` (WM provenance and catalog regeneration), `smoke.py` (native smoke test), their Python tests, `system_apps_remote.sh` and `ai_providers_remote.sh` (hidden `--remote` end-to-end runs of the system apps and of AI providers), and `provision-appcard-llm.sh` (Android). |
| `upstream/makepad.json` | Provenance of every file imported from Makepad's `apps/wm`. |
| `resources/` | Themes, wallpapers, icons, Android manifest template, startup script. |
| `docs/` | [Validation record](docs/validation.md), [upstream sync](docs/upstream.md), [local AI](docs/local-ai.md), [Android AppCard build](docs/android-appcard-build.md), dated plans. |
| `KEYBINDINGS.md`, `BACKLOG.md` | Keymap notes; open follow-ups. |

## Prerequisites

- Stable Rust (`cargo` in `~/.cargo/bin`) and the native toolchain for your OS. On macOS, Xcode Command Line Tools (`xcode-select --install`).
- Git and Python 3.9+ for `tools/setup-native.py`. `scripts/upstream.py` needs Python 3.11+ (it imports `tomllib`).
- Network access for the first setup and build.

## Set up the sibling workspace

The build resolves Makepad, OctoScript, OctoScript-Makepad and OctoSense-System-Apps as **siblings** of this checkout (`../makepad`, and so on, through `[patch]` entries in `Cargo.toml`). Put this repository in its own workspace directory and let the setup script fill it in:

```sh
mkdir octosense-ws && cd octosense-ws
git clone https://github.com/OctoSense-org/OctoSense-Desktop.git
cd OctoSense-Desktop
python3 tools/setup-native.py
```

Result:

```text
octosense-ws/
  OctoSense-Desktop/       this repository
  octoscript-makepad/      the release native-runtime.lock.json selects
  makepad/, octoscript/    the revisions that release's runtime.json pins
  OctoSense-System-Apps/   the revision native-apps.lock.json pins
```

| Command | Effect |
| --- | --- |
| `python3 tools/setup-native.py` | Clone missing siblings at their pinned revisions (and apply any patch `runtime-patches.lock.json` names). |
| `python3 tools/setup-native.py --check` | Verify the siblings without changing anything. |
| `python3 tools/setup-native.py --check --cargo-manifest Cargo.toml` | Also check the locked Cargo graph: one Makepad, one octos, one App Hub. |
| `python3 tools/setup-native.py --update` | Move clean checkouts to the locked revisions (after the lock files change). |
| `--root DIR`, `--cache DIR` | Use another workspace directory; reuse local Git object caches. |

Local changes in a sibling are preserved; `--update` only moves clean checkouts. When `runtime-patches.lock.json` names a patch, a clean checkout of the commit it was cut from (`source_commit`) is accepted as the same tree.

## Build and run

From this directory, after setup:

```sh
cargo run --release
```

The desktop starts empty. Start App Hub, a system app or a developer program from the dock, the top-left **Apps** menu, or **⌘Space** (menu and search). **System → Quit OctoSense** closes the desktop and everything it hosts.

Developer programs from `config/apps.json` build on first launch (progress shows in the tile). To build the workspace ahead of time:

```sh
cargo build --release --workspace
```

| Platform | Status |
| --- | --- |
| macOS | Supported and validated (source builds, process hosting, App Hub, system apps). |
| Windows, Linux | Code paths are retained from upstream but not validated here. |
| Android | `cargo makepad android run -p octosense --release`; see [Phones](#phones). |
| iOS | Startup policy is tested, but a full build currently fails in the pinned Makepad Metal backend ([validation](docs/validation.md)). |

A relocatable `.app`, installers and a Linux session compositor are not provided.

### Cargo features

| Feature | Default | Effect |
| --- | --- | --- |
| `app-hub` | on | Links `octosense-app-hub-app` (store `apphub`, Card runner `card`, system apps) and the host services `octosense-mail-service` (Mail) and `octosense-llm-service` (AI providers). Without it the build has no App Hub and no system apps. |
| `app-reference` | off | Links Reference as a module. |
| `app-sheets` | off | Links Makepad's Sheets as a module. |
| `app-photos` | off | Links Makepad's native Photos module; it replaces the Photos system app of the same id (for comparison). |
| `app-appcard` | off | Links the AppCard assistant module (`apps/appcard`); implies `octos-core`. Opt-in on every target, phones included; not shipped for now. |
| `octos-core` | on | The octos kernel service (`octosense-octos-core`, from `../OctoSense-System-Apps/crates/octos-core`): the one kernel AppCard and other consumers share, configured by AI providers. Always on for Android and iOS. Leave it out with `--no-default-features --features app-hub` (and whatever else you want). |
| `app-aichat` | off | Links Makepad's AI chat as a module, without its model engine. |
| `app-rinx` | off | Links [Rinx](https://github.com/upstreamlabs/Rinx), the Matrix client, as a module. |
| `mobile-apps` | off | `app-reference` + `app-sheets` + `app-hub` + `octos-core`: the set phone builds link, for testing on desktop. Not AppCard. |

A linked module opens with `--module <id>` (or a `<id>: Module` line in `wm/apps.splash` under the state directory):

```sh
cargo run --release --features app-appcard -- --module appcard
cargo run --release --features mobile-apps -- --module reference --module sheets
cargo run --release --features app-rinx -- --module rinx
```

App Hub's modules are the exception: they have no process form and always open in-process.

### Flags and environment

| Name | Effect |
| --- | --- |
| `--apps <file>` | Use this developer-program catalog. |
| `--module <id>` | Host a linked module in-process. |
| `--assistant`, `--prewarm` | Start the assistant app / prewarm apps (need matching catalog entries). Off by default. |
| `--demo-home`, `--download-wallpapers` | Generate a demo filesystem; fetch the Omarchy theme's full wallpaper set. |
| `OCTOSENSE_HOME` | State directory (default `~/.octosense`; falls back to an existing `~/.makeos`, and `MAKEOS_HOME`). |
| `OCTOSENSE_APP_DATA` | Where App Hub keeps installed apps (default `apps/` in the platform data directory). |
| `OCTOSENSE_HUB`, `OCTOSENSE_HUB_ANCHOR` | App Hub catalog origin (path or URL) and trust anchor; default is the App Hub repository's `main`. |
| `OCTOSENSE_SYSTEM_APPS` | The system-app selection file; `.cargo/config.toml` sets it to `system-apps.json`. |
| `MAKEPAD_APP_CONFIG='{"mail_demo":true}'` | Serve Mail's demo mailbox (see [Demos](#demos)). |
| `OCTOSENSE_MAIL_VAULT=file` | Keep Mail passwords in a 0600 file instead of the macOS keychain. |
| `OCTOSENSE_LLM_VAULT=file` | Keep AI providers' keys in the owner-only octos profile instead of the macOS keychain. |
| `OCTOS_APP_CORE_BIN`, `OCTOS_APP_CORE_DIR` | The octos kernel binary the shell's kernel service runs (none: no kernel on this desktop) and its core dir (default `~/octos-home/.octos`; the AI providers profile is `<dir>/profiles/_main.json`). |
| `MAKEPAD_REMOTE`, `MAKEPAD_HIDE_WINDOWS` | Remote-control bridge; hidden windows (see [Demos](#demos)). |

## The app model

The launcher lists four kinds of app together:

| Kind | Comes from | Runs as | Launcher id |
| --- | --- | --- | --- |
| **System apps**: News, Photos, Maps, Camera, Mail, AI providers | OctoSense-System-Apps `apps/<name>/bundle`, selected by `system-apps.json`, packed into the build | Contained Splash programs in App Hub's Card runner, each in its own isolate under the capabilities its manifest asks for | `<name>` (manifest id `os.<name>`) |
| **Store apps** | The signed App Hub catalog, installed from the store (`apphub`) | The same Card runner. Every open is checked against the catalog; an update closes old instances. | `hub:<manifest-id>` |
| **Native modules** | Rust crates linked into this binary | In-process `AppModule`s. Trusted code only: App Hub, AppCard, Reference and the `app-*` features. | module id |
| **Developer programs** | `config/apps.json` | Separate processes in tiles, over Makepad's `--stdin-loop` hosting protocol, built on first launch | catalog `id` |

Precedence: a linked native module beats a system app of the same id, and a system app beats a catalog row of the same id. That is why Makepad's example Mail and Photos are dropped from the shipped catalogs (`drop` in `config/apps.overlay.json`).

### Containment and permissions

A contained app is a bundle: `manifest.json` (id, version, capabilities) plus `main.splash`. The Card runner grants only the capabilities the manifest lists (Mail asks for `storage` and `mail`). The pinned Makepad ([makepad#30](https://github.com/OctoSense-org/makepad/pull/30)) enforces this at every exit of an isolate: network and web sockets answer to the app's host list, raw sockets and servers are refused, files stay in the app's storage jail, and password or one-time-code fields are inert inside a policed isolate.

### Host services and host-owned sheets

Secrets are the host's. An app that needs an account calls a **host service** through `host.request`; the service runs in the shell with the credentials, and the app never gets a socket or a password.

Mail is the worked example (`octosense-mail-service`, from `../OctoSense-System-Apps/apps/mail/host-service`):

- `mail.add_account` raises the host's **sign-in sheet**, a separate isolate drawn over the app. Only that sheet's calls (`mail.sheet.submit`, `mail.sheet.cancel`) can carry a password.
- The service tests the account, stores the password in the platform secret store (macOS keychain), and grants the account only to the app that added it.
- Mail state lives under the host's own directory, outside every app's jail.

AI providers (`os.ai-providers`) edits the octos kernel's LLM providers through the `llm` service (`octosense-llm-service`, from `../OctoSense-System-Apps/apps/ai-providers/host-service`). Keys are typed only on host sheets and go to the macOS keychain entry octos reads; the providers are written to the kernel's profile under the shell's octos core dir (`<core dir>/profiles/_main.json`; core dir `OCTOS_APP_CORE_DIR`, else `~/octos-home/.octos`). A phone's provider QR is imported from a picture of it: **Choose image** opens the open panel, or drop a screenshot on the import sheet. **Start → Settings → AI providers** opens it. After a change the service restarts the kernel if one runs; its consumers (AppCard) reconnect to the new one.

**The octos kernel** is a shell service, not part of any app: `octosense-octos-core` (feature `octos-core`, default). The shell configures it at startup (`apps::configure_octos_kernel`); nothing runs until a consumer connects, then one kernel per process (`<OCTOS_APP_CORE_BIN> serve --stdio --data-dir <core dir>` on a desktop, the APK's `liboctos.so` on Android; none on iOS or on a desktop without `OCTOS_APP_CORE_BIN`). AppCard's agent connects to it; Rinx's host can take its own connection. It stops when the last consumer leaves and when the shell exits.

New app features that need a password, PIN or token belong in a host service and a host sheet, never in the app's own UI.

### Store apps (App Hub)

App Hub is on by default. Open **App Hub** from the launcher to browse the signed catalog and install apps; installed apps appear in the launcher without a restart. The catalog origin defaults to the App Hub repository and can be pointed elsewhere with `OCTOSENSE_HUB`. To build and publish an app, start from [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow).

#### Try your own app before it is published

Publish the bundle into a local catalog with a throwaway anchor (OctoScript-App-Design-Flow's [PUBLISHING §4](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/docs/PUBLISHING.md#4-rehearse-the-store-path-locally) gives the `hub keygen`/`certify`/`publish` commands), then point this shell at it:

```sh
OCTOSENSE_HUB=<mirror dir> OCTOSENSE_HUB_ANCHOR=<anchor hex> \
  OCTOSENSE_HOME=/tmp/octosense-test OCTOSENSE_APP_DATA=/tmp/octosense-test-apps \
  cargo run --release
```

Open **App Hub**, choose the app, **Get**, scroll to **Install**, then **Open**: it runs in the Card runner under its manifest, as a store app would. Verified on macOS on 2026-09-26 with a new script app. The two `OCTOSENSE_*` state variables keep the test out of `~/.octosense`.

### Choosing and overriding system apps

`system-apps.json` names the apps and where their bundles are:

```json
{
  "schema": 1,
  "source": "../OctoSense-System-Apps/apps",
  "apps": ["news", "photos", "maps", "camera", "mail", "ai-providers"],
  "assets": {}
}
```

- Remove an id from `apps` to leave it out; point `OCTOSENSE_SYSTEM_APPS` at another file for a different selection. Without that variable the build ships no system apps.
- To try a changed bundle, edit it in the `../OctoSense-System-Apps` checkout and rebuild; to ship it, land it in OctoSense-System-Apps and bump `native-apps.lock.json`.
- The desktop mounts no photo library, so Photos shows the thumbnails its bundle ships. To give it full-size photos, add `"assets": {"photos": {"photos": "<dir>"}}`.
- A native module of the same id overrides a system app (for example `--features app-photos`).

### Developer programs and the catalog

`config/apps.json` lists Reference and Makepad's own apps (Browser, Files, Terminal, Sheets, Notes, Calendar, Director under the id `studio`, and more). The Image, PDF and AI helpers also appear in the launcher unless their ids (`image`, `pdf`, `aichat`) are listed in `wm/launcher.hides` under the state directory.

Catalog lookup: `--apps <file>` if given, else `~/.octosense/apps.json` if it exists, else `config/apps.json`. A catalog is a JSON array; each entry picks one launch target:

```json
[
  { "id": "notes", "label": "Notes", "manifest": "../notes/Cargo.toml", "package": "my-notes", "bin": "notes", "policy": "new", "args": [] },
  { "id": "installed-notes", "label": "Installed Notes", "executable": "/opt/my-apps/notes" },
  { "id": "browser", "label": "Browser", "source": "makepad", "package": "makepad-browser", "bin": "browser", "policy": "focus" }
]
```

- `"source": "makepad"` resolves through Cargo to the same Makepad checkout as the host; such builds go to `~/.octosense/build/makepad`.
- Step-by-step guides: [docs/open-apps.md](docs/open-apps.md) and [docs/add-all-makepad-apps.md](docs/add-all-makepad-apps.md).
- Relative paths resolve from the catalog's directory. Arguments are passed literally, without a shell.
- `policy`: `"new"` opens another instance; `"focus"` (default) focuses a running one.
- Do not add `--stdin-loop` or Studio variables; the shell adds them. Restart after editing.
- A hosted program must be a Makepad app built against the same Makepad revision; the hosting protocol is not stable across revisions. Start from `apps/reference`.

The Makepad rows are generated from upstream's app registry at the pinned revision:

```sh
python3 scripts/upstream.py catalog          # report drift
python3 scripts/upstream.py catalog --apply  # rewrite config/apps.json and apps.makepad.json
```

### The AppCard assistant

AppCard is **not shipped for now**: it interfered with the other apps, so no build links it unless asked. Default, `mobile-apps`, Android and iOS builds leave its UI out (the octos kernel service stays), and it has no tile, group or launcher entry. `--features app-appcard` brings it back on any target (for a phone, pass the feature to `cargo makepad`); it needs the `../OctoSense-System-Apps` sibling that `tools/setup-native.py` prepares and pulls octos at the revision below.

`apps/appcard` (feature `app-appcard`, opt-in) hosts the whole AppCard assistant in one tile: `octos-app` from `../OctoSense-System-Apps/apps/appcard/app/app`, built without its `standalone` feature. Routing, cards, sessions, the composer and the kernel agent run inside the tile's isolate; `ask` is the module's AI-bus tool.

```sh
cargo run --release --features app-appcard -- --module appcard
```

It starts no kernel of its own: it connects to the shell's. On desktop that needs `OCTOS_APP_CORE_BIN` (and optionally `OCTOS_APP_CORE_DIR`); without a kernel it shows its login / WebSocket screen. Every octos crate comes from octos-org/octos at the one revision `octos-app` and `crates/octos-core` pin (`6ad76e5c`).

## Demos

### Mail without an account

```sh
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release
```

Open **Mail**, sign in on the host sheet with any address and password `demo`. The demo serves sample messages from a file vault: no network, no keychain.

With a real account on an unsigned development build, macOS asks for keychain access again after every rebuild. For development, keep passwords in a file instead:

```sh
OCTOSENSE_MAIL_VAULT=file cargo run --release
```

### Remote-control bridge

Every desktop Makepad app, this shell included, carries a localhost HTTP control surface. Start it with `MAKEPAD_REMOTE=<port>`, `MAKEPAD_REMOTE=on` (ephemeral port; a number is always read as the port, so `1` means port 1 and fails), or `--remote[=PORT]`:

```sh
MAKEPAD_REMOTE=8399 cargo run --release
# prints: [makepad-remote] listening on 127.0.0.1:8399 pid=... app=... grabs=...
```

| Route | Does |
| --- | --- |
| `/` | Cheat sheet of every route. |
| `/s` | Windows and their geometry. |
| `/snap?q=` | Visible widgets with rects and text, filtered by id/type/text. |
| `/click?x=&y=` | Click at window-local layout points. `/m`, `/k`, `/t` for mouse, keys, text. |
| `/g` | Grab a window to PNG. |
| `/log?n=` | Tail the log. |
| `/gq` | Grab every window, then quit. Use this (or `/quit`) to end any session you started. |

Add `&wait=1` to an input route to answer after the next frame. The bridge injects real input and serves screenshots: bind a non-loopback host (`MAKEPAD_REMOTE=0.0.0.0:8399`) only on a trusted network.

### Headless UI checks

On macOS, `MAKEPAD_HIDE_WINDOWS=1` keeps windows off screen while still rendering, so a remote-driven run does not take over the display:

```sh
MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=on cargo run --release
```

Makepad's [`makepad_test`](https://github.com/OctoSense-org/makepad/tree/main/libs/makepad_test) crate (in the `../makepad` sibling) builds on the same two pieces: `#[makepad_test]` tests launch the app hidden, drive it over `--remote` with selectors and waits, and close it with `/gq`. This repository does not have a `makepad_test` suite yet.

## Phones

With the Makepad Android toolchain installed and a device on ADB:

```sh
cargo makepad android run -p octosense --release
```

Phone builds always link Reference and Sheets and the octos kernel service, and App Hub with the system apps through the default feature; AppCard only with `--features app-appcard`. The launcher label is **OctoSense**, application id `dev.makepad.octosense`. The APK must bundle the kernel as `liboctos.so`: `python3 tools/android-kernel.py --sdk <cargo-makepad Android SDK> -- cargo makepad android run -p octosense --release` cross-builds `octos` at the revision `Cargo.lock` pins (in `target/octos-kernel/`) and runs the packager with `MAKEPAD_ANDROID_EXTRA_LIBS=liboctos.so=<octos>` (`--kernel <path>` for a prebuilt one). Without it the phone runs no kernel; the AI providers are still saved. AppCard's Java features (GPS, notifications, share, intents) need the fork's buildtool; see [docs/android-appcard-build.md](docs/android-appcard-build.md). The dedicated phone shell is OctoSense-ROM's `home/`.

## Desktop styles and settings

- Eight desktop styles. Desktop builds start in **OctoSense**, with Liquid Glass frames and a **Light / Dark** switch in the top bar; the others are Omarchy, macOS, Windows, Windows 2000, NeXTSTEP, iOS and Android. Theme sources are in `resources/themes/`, wallpaper provenance in [resources/wallpapers/README.md](resources/wallpapers/README.md).
- Keys: **⌘Space** menu, **⌘W** close tile, **⌘F** tile fullscreen, **⌘1…0** workspaces, **⌘Shift1…0** move tile. **Learn → Keybindings** lists them; see [KEYBINDINGS.md](KEYBINDINGS.md).
- State lives in `~/.octosense` (`OCTOSENSE_HOME`); hosted apps get it as `MAKEPAD_HOME`.
- Local models for the AI pane (**F10**): [docs/local-ai.md](docs/local-ai.md). The desktop works without a model.

## Pinning and updating siblings

| To move | Edit | Then |
| --- | --- | --- |
| Makepad / OctoScript | `native-runtime.lock.json` (a new OctoScript-Makepad release), and the `rev` of the Makepad Git dependencies in `Cargo.toml` and `apps/*/Cargo.toml` | If a patch is needed on top, record it in `runtime-patches.lock.json` (base, sha256, tree) |
| System apps, Mail service, AppCard | `revision` in `native-apps.lock.json` | — |
| App Hub | `rev` of `octosense-app-hub-app` in `Cargo.toml` | Keep it at the App Hub rev the pinned System-Apps Mail and `llm` services name (`octosense-appstore`), so the graph has one App Hub |

After any change: `python3 tools/setup-native.py --update`, `cargo update` as needed, then `python3 tools/setup-native.py --check --cargo-manifest Cargo.toml` and the tests below.

`scripts/upstream.py sync|status|diff|update` tracks the WM files imported from official Makepad (`upstream/makepad.json`, baseline `74b63be8`); see [docs/upstream.md](docs/upstream.md). It needs `--source` pointing at a full clone of official Makepad: the `../makepad` sibling is a shallow checkout of the fork and does not contain the baseline commit.

## Testing

```sh
cargo test --locked --workspace
cargo test --locked --workspace --features mobile-apps
cargo test --locked -p octosense --features mobile-apps,app-appcard appcard
cargo tree --locked --features mobile-apps -i octosense-octos-core   # the octos kernel service is linked
cargo tree --locked --features mobile-apps -i octosense-appcard      # must not match: no AppCard UI without app-appcard
python3 -m unittest tools/test_android_kernel.py
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/upstream.py catalog
python3 tools/setup-native.py --check --cargo-manifest Cargo.toml
```

Native smoke tests open their own windows, isolate state in a temporary directory and drive the shell over the remote bridge. They need GUI access and a prior release build:

```sh
cargo build --release --locked --workspace
python3 scripts/smoke.py --styles
python3 scripts/smoke.py --cargo-run --default-catalog
```

Results for each change are recorded in [docs/validation.md](docs/validation.md).

## CI

`.github/workflows/runtime.yml` runs on every push and pull request, on macOS 14: `setup-native.py`, `cargo check --locked` (default and `--features mobile-apps`), `cargo tree` checks that the octos kernel service is in the default and `mobile-apps` graphs and the AppCard UI (`octosense-appcard`) is not (host and Android), `cargo check --locked --workspace --features mobile-apps,app-appcard`, the `tools/android-kernel.py` tests, and `setup-native.py --check --cargo-manifest Cargo.toml`. It does **not** run `cargo test`, the other Python tests or the smoke tests; run those locally before opening a PR.

## Known gaps

- Only macOS is validated. Windows and Linux are untested; the iOS build fails in the pinned Metal backend.
- No packaged `.app` or installer; source builds read fonts and resources from the `../makepad` checkout, so keep it in place.
- Photos on desktop has thumbnails only unless you mount a photo directory.
- The hosted AppCard assistant does not yet wire notifications, share or the WebView overlay.
- Mobile Sheets needs grid-label and toolbar fixes ([BACKLOG.md](BACKLOG.md)).
- No `makepad_test` UI suite; CI compiles but does not test.

## Contributing

`main` is protected: every change goes through a pull request (admins included), and force pushes are blocked. Branch from `main`, run `setup-native.py --check`, the Rust and Python tests above and, for UI changes, a smoke or remote-driven run; record native checks in `docs/validation.md`. Keep the one-Makepad, one-octos, one-App-Hub rule: `setup-native.py --check --cargo-manifest Cargo.toml` must pass.

## License

Apache License 2.0 ([LICENSE](LICENSE), [NOTICE](NOTICE)). Source copied from Makepad keeps its [MIT notice](LICENSES/Makepad-MIT.txt). Dependencies keep their own licenses.
