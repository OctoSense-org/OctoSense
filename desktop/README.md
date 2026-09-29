# OctoSense desktop

English | [简体中文](README.zh-CN.md)

The desktop shell of [OctoSense](https://github.com/OctoSense-org), the agent shell on top of your operating system, and the desktop packaging of the OctoSense repository (formerly the OctoSense-Desktop repository). It is one Makepad window that is the desktop: a launcher, a dock and tiles, hosting system apps and App Hub store apps as contained script programs, trusted native modules in-process, and Makepad developer programs as child processes. It gets its apps the same way the phone shell, [Home](../phone/README.md), does. Setup, the repository layout and CI are in the [root README](../README.md).

**Building an OctoSense app?** You do not need this repository to build, check or publish one: start at the [OctoSense-org profile](https://github.com/OctoSense-org)'s reading list (OctoScript-App-Design-Flow's `AGENTS.md`, then `docs/QUICKSTART.md`). Build this shell only if you want to see your app in the desktop shell before it is published ([Try your own app](#try-your-own-app-before-it-is-published)).

## Where it sits

| Where | Role for the desktop |
| --- | --- |
| [`../phone/`](../phone/README.md) | Home, the phone shell. Same app model, same runtime, same system apps. |
| [`../apps/`](../apps/README.md) | News, Photos, Maps, Mail, AI providers and YouTube bundles (Camera is phone-only), the Mail and `llm` host services, the AppCard assistant (`octos-app`, opt-in, not shipped by default), and Reference. |
| [`../crates/`](../crates/) | The shell itself (`crates/shell`, package `octosense-shell`, which this package wraps), its AI services (`crates/ai-host`), the octos kernel service (`crates/kernel`, package `octosense-kernel`) and the app-agent broker (`crates/app-peers`). |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | The signed catalog, the store and the Card runner. Linked as the Git crate `octosense-app-hub-app`. |
| [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) | Where apps are designed, built and published to the App Hub. |
| [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad) | The runtime release that pins Makepad and OctoScript. Checked out in `.sources/`. |
| [makepad (OctoSense fork)](https://github.com/OctoSense-org/makepad) | The framework. Checked out in `.sources/makepad`. |
| [octos](https://github.com/octos-org/octos) | The agent kernel, a shell service (`octos-core`, on by default): AI providers configures it, AppCard, Rinx and other consumers connect to it. One revision, pinned in the root `Cargo.toml`; the kernel itself is a separate binary (desktop: `OCTOS_APP_CORE_BIN`; Android: bundled `liboctos.so`). |
| [Rinx](https://github.com/hagency-org/Rinx) | Matrix chats and mini apps, linked as a module (`app-rinx`, on by default). |

## Layout of `desktop/`

| Path | What it is |
| --- | --- |
| `src/main.rs` | The entry point (package `octosense`): `octosense_main!()` over the shell's `App`. The shell (tiling, launcher, dock, bar, hosting, the app registry, `shell/`, `octosense/`) is [`../crates/shell/src`](../crates/shell/src). |
| `config/apps.json` | The default developer-program catalog. `apps.makepad.json` is an identical copy for `--apps`; `apps.overlay.json` holds the adaptations applied when regenerating them. |
| `system-apps.json` | Which system apps this build packs, and from where (`../apps`). |
| `scripts/` | `upstream.py` (WM provenance and catalog regeneration), `smoke.py` (native smoke test), their Python tests, `system_apps_remote.sh`, `ai_providers_remote.sh` and `glance_remote.sh` (hidden `--remote` end-to-end runs of the system apps and of AI providers), and `provision-appcard-llm.sh` (Android). |
| `upstream/makepad.json` | Provenance of every file imported from Makepad's `apps/wm`. |
| `resources/android/` | The Android manifest template. Themes, wallpapers, icons and the startup script are the shell's, in [`../crates/shell/resources`](../crates/shell/resources). |
| `docs/` | [Validation record](docs/validation.md), [upstream sync](docs/upstream.md), [local AI](docs/local-ai.md), [Android AppCard build](docs/android-appcard-build.md), dated plans. |
| `KEYBINDINGS.md`, `BACKLOG.md` | Keymap notes; open follow-ups. |

The modules the desktop links live elsewhere in the repository: Reference in `../apps/reference`, the AppCard module in `../apps/appcard/module` (with `octos-app` in `../apps/appcard/app/app`).

## Prerequisites

- Stable Rust (`cargo` in `~/.cargo/bin`) and the native toolchain for your OS. On macOS, Xcode Command Line Tools (`xcode-select --install`).
- Git and Python 3.9+ for `tools/setup.py`. `scripts/upstream.py` needs Python 3.11+ (it imports `tomllib`).
- Network access for the first setup and build.

## Set up

From the repository root, prepare the pinned framework sources once:

```sh
python3 tools/setup.py                  # makepad, octoscript, octoscript-makepad into .sources/
python3 tools/setup.py --check --cargo  # verify: one Makepad, App Hub, octos and Rinx in the graph
```

Details, `--update` and `--cache`: [root README](../README.md#set-up).

## Build and run

From the repository root or `desktop/`, after setup:

```sh
cargo run --release -p octosense
```

The desktop starts empty. Start App Hub, a system app or a developer program from the dock, the top-left **Apps** menu, or **⌘Space** (menu and search). **System → Quit OctoSense** closes the desktop and everything it hosts.

Developer programs from `config/apps.json` build on first launch (progress shows in the tile).

| Platform | Status |
| --- | --- |
| macOS | Supported and validated (source builds, process hosting, App Hub, system apps). |
| Windows, Linux | Code paths are retained from upstream but not validated here. |
| Android | `cargo makepad android run -p octosense --release`; see [Phones](#phones). |
| iOS | Startup policy is tested, but a full build currently fails in the pinned Makepad Metal backend ([validation](docs/validation.md)). |

A relocatable `.app`, installers and a Linux session compositor are not provided.

### Cargo features

The native apps' features (`app-hub`, `app-rinx`, `app-reference`, `app-sheets`, `app-terminal`, `app-appcard`), the default set and `mobile-apps` come from [`native-apps.json`](../native-apps.json) (ADR 0004 §1): edit the manifest and run `python3 tools/native_apps.py`, never the generated blocks in the `Cargo.toml`s.

| Feature | Default | Effect |
| --- | --- | --- |
| `app-hub` | on | Links `octosense-app-hub-app` (store `apphub`, Card runner `card`, system apps) and the host services `octosense-mail-service` (Mail) and `octosense-llm-service` (AI providers). Without it the build has no App Hub and no system apps. |
| `octos-core` | on | The octos kernel service (`octosense-kernel`, from `../crates/kernel`) and the app-agent broker (`octosense-app-peers`): the one kernel AppCard, Rinx and other consumers share, configured by AI providers. Always on for Android and iOS. Leave it out with `--no-default-features --features app-hub` (and whatever else you want). |
| `app-rinx` | on | Links [Rinx](https://github.com/hagency-org/Rinx), the Matrix client, as a module; implies `octos-core` (its assistant is the shell's). |
| `app-reference` | off | Links Reference (`../apps/reference`) as a module. |
| `app-sheets` | off | Links Makepad's Sheets as a module. |
| `app-terminal` | on | Links Makepad's Terminal as a system app: a login shell in a tile. On macOS and Windows it runs as its own process (`terminal`, built from the pinned Makepad checkout with `cargo run`, else the binary beside `octosense`), so a crash in it leaves the shell running; on Linux only with a Vulkan build in a Wayland session. Where it cannot start a process (no checkout and no binary: release packages do not ship it yet, [#94](https://github.com/OctoSense-org/OctoSense/pull/94)) it opens in-process, as it does on phones; a `terminal: Module` or `terminal: Process` line in `wm/apps.splash` under the state directory overrides that. In either hosting the assistant gets the same tools (ADR 0004 §10): it reads (`read_screen`, `read_scrollback`) and may type a command (`run`), and every command waits for the person's live confirmation on the assistant's confirm card (`confirm: host`, `auto_approvable: false` in `native-apps.json`); a command too long for the card to show in full is refused. In-process on macOS, the shell's PTY helper is `octosense` itself. |
| `app-appcard` | off | Links the AppCard assistant module (`../apps/appcard/module`); implies `octos-core`. Opt-in on every target, phones included; not shipped for now. |
| `app-aichat` | off | Links Makepad's AI chat as a module, without its model engine. |
| `mobile-apps` | off | `app-rinx` + `app-reference` + `app-sheets` + `app-hub` + `octos-core`: the set phone builds link, for testing on desktop. Not AppCard. |

A linked native app is hosted as its `hosting` in `native-apps.json` says for the platform: App Hub, Rinx, AppCard, Reference and Sheets in-process everywhere, the Terminal as a process on macOS and Windows (and on Linux with a Vulkan build in a Wayland session). `--module <id>` (or a `<id>: Module` line in `wm/apps.splash` under the state directory) opens one in-process instead:

```sh
cargo run --release -p octosense -- --module terminal
```

App Hub's modules have no process form and always open in-process.

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
| `OCTOSENSE_SYSTEM_APPS` | The system-app selection file; the root `.cargo/config.toml` sets it to `desktop/system-apps.json`. |
| `MAKEPAD_APP_CONFIG='{"mail_demo":true}'` | Serve Mail's demo mailbox (see [Demos](#demos)). |
| `OCTOSENSE_MAIL_VAULT=file` | Keep Mail passwords in a 0600 file instead of the macOS keychain. |
| `OCTOSENSE_LLM_VAULT=file` | Keep AI providers' keys in the owner-only octos profile instead of the macOS keychain. |
| `OCTOS_APP_CORE_BIN`, `OCTOS_APP_CORE_DIR` | The octos kernel binary the shell's kernel service runs (none: no kernel on this desktop) and its core dir (default `~/octos-home/.octos`; the AI providers profile is `<dir>/profiles/_main.json`). |
| `OCTOSENSE_GLANCE_DEMO=1` | Publish a sample L0 News digest card (as `os.news`) to the glance screen at startup: F9 on a desktop style, the glance page on a phone style. A test path for the `glance` service. |
| `MAKEPAD_REMOTE`, `MAKEPAD_HIDE_WINDOWS` | Remote-control bridge; hidden windows (see [Demos](#demos)). |

## The app model

The launcher lists four kinds of app together:

| Kind | Comes from | Runs as | Launcher id |
| --- | --- | --- | --- |
| **System apps**: News, Photos, Maps, Mail, AI providers, YouTube (Camera ships on the phone only) | `../apps/<name>/bundle`, selected by `system-apps.json`, packed into the build | Contained Splash programs in App Hub's Card runner, each in its own isolate under the capabilities its manifest asks for | `<name>` (manifest id `os.<name>`) |
| **Store apps** | The signed App Hub catalog, installed from the store (`apphub`) | The same Card runner. Every open is checked against the catalog; an update closes old instances. | `hub:<manifest-id>` |
| **Native modules** | Rust crates linked into this binary | In-process `AppModule`s. Trusted code only: App Hub, AppCard, Rinx, Reference and the `app-*` features. | module id |
| **Developer programs** | `config/apps.json` | Separate processes in tiles, over Makepad's `--stdin-loop` hosting protocol, built on first launch | catalog `id` |

Precedence: a linked native module beats a system app of the same id, and a system app beats a catalog row of the same id. That is why Makepad's example Mail and Photos are dropped from the shipped catalogs (`drop` in `config/apps.overlay.json`).

### Containment and permissions

A contained app is a bundle: `manifest.json` (id, version, capabilities) plus `main.splash`. The Card runner grants only the capabilities the manifest lists (Mail asks for `storage` and `mail`). The pinned Makepad ([makepad#30](https://github.com/OctoSense-org/makepad/pull/30)) enforces this at every exit of an isolate: network and web sockets answer to the app's host list, raw sockets and servers are refused, files stay in the app's storage jail, and password or one-time-code fields are inert inside a policed isolate.

### Host services and host-owned sheets

Secrets are the host's. An app that needs an account calls a **host service** through `host.request`; the service runs in the shell with the credentials, and the app never gets a socket or a password.

Mail is the worked example (`octosense-mail-service`, from [`../apps/mail/host-service`](../apps/mail/host-service)):

- `mail.add_account` raises the host's **sign-in sheet**, a separate isolate drawn over the app. Only that sheet's calls (`mail.sheet.submit`, `mail.sheet.cancel`) can carry a password.
- The service tests the account, stores the password in the platform secret store (macOS keychain), and grants the account only to the app that added it.
- Mail state lives under the host's own directory, outside every app's jail.

AI providers (`os.ai-providers`) edits the octos kernel's LLM providers through the `llm` service (`octosense-llm-service`, from [`../apps/ai-providers/host-service`](../apps/ai-providers/host-service)). Keys are typed only on host sheets and go to the macOS keychain entry octos reads; the providers are written to the kernel's profile under the shell's octos core dir (`<core dir>/profiles/_main.json`; core dir `OCTOS_APP_CORE_DIR`, else `~/octos-home/.octos`). A phone's provider QR is imported from a picture of it: **Choose image** opens the open panel, or drop a screenshot on the import sheet. **Start → Settings → AI providers** opens it. After a change the service restarts the kernel if one runs; its consumers (AppCard) reconnect to the new one.

**The octos kernel** is a shell service, not part of any app: `octosense-kernel` ([`../crates/kernel`](../crates/kernel), feature `octos-core`, default). The shell starts it through its AI services at startup ([`../crates/ai-host`](../crates/ai-host/README.md), `octosense_ai_host::start`); nothing runs until a consumer connects, then one kernel per process (`<OCTOS_APP_CORE_BIN> serve --stdio --data-dir <core dir>` on a desktop, the APK's `liboctos.so` on Android; none on iOS or on a desktop without `OCTOS_APP_CORE_BIN`). AppCard's agent connects to it; Rinx reaches it through the app-agent broker. It stops when the last consumer leaves and when the shell exits.

**Talk to Octos** (off by default): **AI providers → Talk to Octos** turns on a loopback server so a web client or a terminal UI can talk to this device's assistant. While it is on, the kernel runs as `octos serve --host-managed` instead of `--stdio` and native apps keep working over its WebSocket; external clients get a separate token that opens the UI Protocol socket and nothing else. A web client pairs with a one-time code or the QR of its link; a terminal client of this user reads the private connection file. The server stays up when native apps close, until it is turned off or the shell exits. See [ADR 0003](../docs/adr/0003-shared-octos-client-access.md) and the [kernel guide](../crates/kernel/README.md).

New app features that need a password, PIN or token belong in a host service and a host sheet, never in the app's own UI.

### Store apps (App Hub)

App Hub is on by default. Open **App Hub** from the launcher to browse the signed catalog and install apps; installed apps appear in the launcher without a restart. The catalog origin defaults to the App Hub repository and can be pointed elsewhere with `OCTOSENSE_HUB`. To build and publish an app, start from [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow).

#### Try your own app before it is published

Publish the bundle into a local catalog with a throwaway anchor (OctoScript-App-Design-Flow's [PUBLISHING §4](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/docs/PUBLISHING.md#4-rehearse-the-store-path-locally) gives the `hub keygen`/`certify`/`publish` commands), then point this shell at it:

```sh
OCTOSENSE_HUB=<mirror dir> OCTOSENSE_HUB_ANCHOR=<anchor hex> \
  OCTOSENSE_HOME=/tmp/octosense-test OCTOSENSE_APP_DATA=/tmp/octosense-test-apps \
  cargo run --release -p octosense
```

Open **App Hub**, choose the app, **Get**, scroll to **Install**, then **Open**: it runs in the Card runner under its manifest, as a store app would. Verified on macOS on 2026-09-26 with a new script app (in the OctoSense-Desktop repository, before the merge). The two `OCTOSENSE_*` state variables keep the test out of `~/.octosense`.

### Choosing and overriding system apps

`system-apps.json` names the apps and where their bundles are (paths relative to this file):

```json
{
  "schema": 1,
  "source": "../apps",
  "apps": ["news", "photos", "maps", "mail", "ai-providers"],
  "assets": {}
}
```

- Remove an id from `apps` to leave it out; point `OCTOSENSE_SYSTEM_APPS` at another file for a different selection. Without that variable the build ships no system apps.
- To change a bundle, edit it in `../apps/<name>/bundle` and rebuild; it ships with the next desktop build, in the same pull request.
- The desktop mounts no photo library, so Photos shows the thumbnails its bundle ships. To give it full-size photos, add `"assets": {"photos": {"photos": "<dir>"}}`.
- A linked native module of the same id would override a system app; none does (the native News, Photos and Maps modules are deleted).

### Developer programs and the catalog

`config/apps.json` lists Reference and Makepad's own apps (Browser, Files, Terminal, Sheets, Notes, Calendar, Director under the id `studio`, and more). Terminal is also linked (`app-terminal`, on by default); its `config/apps.json` row is the process form it opens in on macOS and Windows, and the linked module is the in-process form. The Image, PDF and AI helpers also appear in the launcher unless their ids (`image`, `pdf`, `aichat`) are listed in `wm/launcher.hides` under the state directory.

Catalog lookup: `--apps <file>` if given, else `~/.octosense/apps.json` if it exists, else `config/apps.json`. A catalog is a JSON array; each entry picks one launch target:

```json
[
  { "id": "notes", "label": "Notes", "manifest": "../notes/Cargo.toml", "package": "my-notes", "bin": "notes", "policy": "new", "args": [] },
  { "id": "installed-notes", "label": "Installed Notes", "executable": "/opt/my-apps/notes" },
  { "id": "browser", "label": "Browser", "source": "makepad", "package": "makepad-browser", "bin": "browser", "policy": "focus" }
]
```

- `"source": "makepad"` resolves through Cargo to the same Makepad checkout as the host (`.sources/makepad`); such builds go to `~/.octosense/build/makepad`.
- Step-by-step guides (Chinese): [open a hosted app](docs/open-apps.md), including the fix for an empty catalog when `cargo metadata --offline` fails, and [open the full Makepad catalog](docs/add-all-makepad-apps.md).
- Relative paths resolve from the catalog's directory. Arguments are passed literally, without a shell.
- `policy`: `"new"` opens another instance; `"focus"` (default) focuses a running one.
- Do not add `--stdin-loop` or Studio variables; the shell adds them. Restart after editing.
- A hosted program must be a Makepad app built against the same Makepad revision; the hosting protocol is not stable across revisions. Start from `../apps/reference`.

The Makepad rows are generated from upstream's app registry at the pinned revision:

```sh
python3 scripts/upstream.py catalog          # report drift
python3 scripts/upstream.py catalog --apply  # rewrite config/apps.json and apps.makepad.json
```

### The AppCard assistant

AppCard is **not shipped for now**: it interfered with the other apps, so no build links it unless asked. Default, `mobile-apps`, Android and iOS builds leave its UI out (the octos kernel service stays), and it has no tile, group or launcher entry. `--features app-appcard` brings it back on any target (for a phone, pass the feature to `cargo makepad`).

`../apps/appcard/module` (package `octosense-appcard`, feature `app-appcard`, opt-in) hosts the whole AppCard assistant in one tile: `octos-app` from `../apps/appcard/app/app`, built without its `standalone` feature. Routing, cards, sessions, the composer and the kernel agent run inside the tile's isolate; `ask` is the module's AI-bus tool.

```sh
cargo run --release -p octosense --features app-appcard -- --module appcard
```

It starts no kernel of its own: it connects to the shell's. On desktop that needs `OCTOS_APP_CORE_BIN` (and optionally `OCTOS_APP_CORE_DIR`); without a kernel it shows its login / WebSocket screen. Every octos crate comes from octos-org/octos at the one revision the root `Cargo.toml` pins.

## Demos

### Mail without an account

```sh
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release -p octosense
```

Open **Mail**, sign in on the host sheet with any address and password `demo`. The demo serves sample messages from a file vault: no network, no keychain.

With a real account on an unsigned development build, macOS asks for keychain access again after every rebuild. For development, keep passwords in a file instead:

```sh
OCTOSENSE_MAIL_VAULT=file cargo run --release -p octosense
```

### Remote-control bridge

Every desktop Makepad app, this shell included, carries a localhost HTTP control surface. Start it with `MAKEPAD_REMOTE=<port>`, `MAKEPAD_REMOTE=on` (ephemeral port; a number is always read as the port, so `1` means port 1 and fails), or `--remote[=PORT]`:

```sh
MAKEPAD_REMOTE=8399 cargo run --release -p octosense
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
MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=on cargo run --release -p octosense
```

Makepad's [`makepad_test`](https://github.com/OctoSense-org/makepad/tree/main/libs/makepad_test) crate (in `.sources/makepad`) builds on the same two pieces: `#[makepad_test]` tests launch the app hidden, drive it over `--remote` with selectors and waits, and close it with `/gq`. The desktop does not have a `makepad_test` suite yet.

## Phones

With the Makepad Android toolchain installed and a device on ADB:

```sh
cargo makepad android run -p octosense --release
```

Phone builds of this package always link Reference and Sheets and the octos kernel service, and App Hub with the system apps through the default feature; AppCard only with `--features app-appcard`. The launcher label is **OctoSense**, application id `dev.makepad.octosense`. The APK must bundle the kernel as `liboctos.so`: `python3 ../tools/kernel-artifact.py --sdk <cargo-makepad Android SDK> -- cargo makepad android run -p octosense --release` (from `desktop/`) cross-builds `octos` at the revision the workspace `Cargo.lock` pins and runs the packager with `MAKEPAD_ANDROID_EXTRA_LIBS=liboctos.so=<octos>` (`--kernel <path>` for a prebuilt one). Without it the phone runs no kernel; the AI providers are still saved. AppCard's Java features (GPS, notifications, share, intents) need the fork's buildtool; see [docs/android-appcard-build.md](docs/android-appcard-build.md). The dedicated phone shell is [Home](../phone/README.md) (package `octosense-home`); this Android build is for development.

## Desktop styles and settings

- Eight desktop styles. Desktop builds start in **OctoSense**, with Liquid Glass frames and a **Light / Dark** switch in the top bar; the others are Omarchy, macOS, Windows, Windows 2000, NeXTSTEP, iOS and Android. Theme sources are in `../crates/shell/resources/themes/`, wallpaper provenance in [crates/shell/resources/wallpapers/README.md](../crates/shell/resources/wallpapers/README.md).
- Keys: **⌘Space** menu, **⌘W** close tile, **⌘F** tile fullscreen, **⌘1…0** workspaces, **⌘Shift1…0** move tile. **Learn → Keybindings** lists them; see [KEYBINDINGS.md](KEYBINDINGS.md).
- State lives in `~/.octosense` (`OCTOSENSE_HOME`); hosted apps get it as `MAKEPAD_HOME`.
- Local models for the AI pane (**F10**): [docs/local-ai.md](docs/local-ai.md). The desktop works without a model.

## Pins and upstream sync

Every external dependency is pinned once for the whole repository: Makepad and OctoScript through `native-runtime.lock.json` (and the reviewed patch in `runtime-patches.lock.json`), App Hub, octos and Rinx in the root `Cargo.toml` `[workspace.dependencies]`. The system apps, the host services, the kernel service and AppCard are in this repository and change in the same pull request as the shell; there is nothing to pin. After moving a pin: `python3 tools/setup.py --update`, `cargo update` as needed, then `python3 tools/setup.py --check --cargo` and the tests below.

`scripts/upstream.py sync|status|diff|update` tracks the WM files imported from official Makepad (`upstream/makepad.json`, baseline `74b63be8`); see [docs/upstream.md](docs/upstream.md). It needs `--source` pointing at a full clone of official Makepad: `.sources/makepad` is a checkout of the fork and does not contain the baseline commit.

## Testing

CI (`.github/workflows/desktop.yml`) runs on macOS 14 for changes under `desktop/`, `crates/`, `apps/`, `tools/` and the workspace files:

```sh
python3 tools/setup.py
cd desktop
cargo check --locked -p octosense
cargo check --locked -p octosense --features mobile-apps
cargo check --locked -p octosense -p octosense-reference -p octosense-appcard --features mobile-apps,app-appcard
bash ../tools/check-shell-graph.sh -p octosense   # AI services linked, no AppCard UI without app-appcard, Rinx only as a module, one Makepad/App Hub/octos (host and aarch64-linux-android)
cd ..
python3 -m unittest discover -s tools -p 'test_*.py'
python3 tools/setup.py --check --cargo
```

It also fails when a shell source file exists in two of `crates/shell/src`, `desktop/src` and `phone/src`.

The desktop job does **not** run `cargo test` (the shell's tests run in `phone.yml`), the `desktop/scripts` tests or the smoke tests; run those locally before opening a pull request:

```sh
(cd phone && cargo test --locked --features mobile-apps -p octosense-shell)   # the shell's tests, as phone.yml runs them
python3 -m unittest discover -s desktop/scripts -p 'test_*.py'
python3 desktop/scripts/upstream.py catalog
```

Native smoke tests open their own windows, isolate state in a temporary directory and drive the shell over the remote bridge. They need GUI access and a prior release build (**unverified** since the move into `desktop/`: `smoke.py` still looks for Reference at `desktop/apps/reference`):

```sh
cargo build --release --locked -p octosense
python3 desktop/scripts/smoke.py --styles
python3 desktop/scripts/smoke.py --cargo-run --default-catalog
```

Results for each change are recorded in [docs/validation.md](docs/validation.md).

## Known gaps

- Only macOS is validated. Windows and Linux are untested; the iOS build fails in the pinned Metal backend.
- No packaged `.app` or installer; source builds read fonts and resources from the `.sources/makepad` checkout, so keep it in place.
- Photos on desktop has thumbnails only unless you mount a photo directory.
- The hosted AppCard assistant does not yet wire notifications, share or the WebView overlay.
- Mobile Sheets needs grid-label and toolbar fixes ([BACKLOG.md](BACKLOG.md)).
- No `makepad_test` UI suite; CI compiles but does not test.

## Contributing

`main` is protected: every change goes through a pull request (admins included), and force pushes are blocked. Branch from `main`, run `python3 tools/setup.py --check --cargo`, the Rust and Python tests above and, for UI changes, a smoke or remote-driven run with hidden windows; record native checks in `docs/validation.md`. Keep the one-Makepad, one-octos, one-App-Hub rule: `python3 tools/setup.py --check --cargo` must pass. Rules for people and coding agents: [AGENTS.md](../AGENTS.md).

## License

Apache License 2.0 ([LICENSE](../LICENSE), [NOTICE](../NOTICE)). Source copied from Makepad keeps its [MIT notice](../LICENSES/Makepad-MIT.txt). Dependencies keep their own licenses.
