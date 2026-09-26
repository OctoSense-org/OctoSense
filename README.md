# OctoSense Desktop

English | [简体中文](README.zh-CN.md)

OctoSense-Desktop is the desktop shell of [OctoSense](https://github.com/OctoSense-org), the agent shell on top of your operating system. It is one Makepad window that is the desktop: a launcher, a dock and tiles, hosting system apps and App Hub store apps as contained script programs, trusted native modules in-process, and Makepad developer programs as child processes. It gets its apps the same way the phone shell, OctoSense-ROM's `home/`, does.

## Where it sits

| Repository | Role for this repo |
| --- | --- |
| [OctoSense-ROM](https://github.com/OctoSense-org/OctoSense-ROM) | The phone shell (`home/`). Same app model, same runtime patch, same system apps. |
| [OctoSense-System-Apps](https://github.com/OctoSense-org/OctoSense-System-Apps) | News, Photos, Maps, Camera and Mail bundles, the Mail host service and the AppCard assistant (`octos-app`). Pinned sibling checkout. |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | The signed catalog, the store and the Card runner. Linked as the Git crate `octosense-app-hub-app`. |
| [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) | Where apps are designed, built and published to the App Hub. |
| [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad) | The runtime release that pins Makepad and OctoScript. Pinned sibling checkout. |
| [makepad (OctoSense fork)](https://github.com/OctoSense-org/makepad) | The framework. Pinned sibling checkout, plus a reviewed patch. |
| [octos](https://github.com/octos-org/octos) | The agent kernel behind the AppCard assistant. Git dependency, one revision (`18fcd3f1`). |

## Repository layout

| Path | What it is |
| --- | --- |
| `src/` | The shell (crate `octosense`): tiling, launcher, dock, bar, hosting (`host.rs`, `hub.rs`, `module_host.rs`), app registry (`apps.rs`). |
| `src/shell/` | Bar, launcher, menus, notifications, AI pane, gallery. |
| `src/octosense/` | OctoSense-specific parts: app catalog loading, state paths, styles, Makepad source resolution. |
| `apps/reference/` | `octosense-reference`, a small counter/text-input app that runs both as a hosted process and as a linked module. |
| `apps/appcard/` | `octosense-appcard`, the module that mounts the AppCard assistant (`octos-app`) in a tile. |
| `config/apps.json` | The default developer-program catalog. `apps.makepad.json` is an identical copy for `--apps`; `apps.overlay.json` holds the adaptations applied when regenerating them. |
| `system-apps.json` | Which system apps this build packs, and from where. |
| `native-runtime.lock.json` | The OctoScript-Makepad release (and through it, Makepad and OctoScript). |
| `native-apps.lock.json` | The OctoSense-System-Apps revision. |
| `runtime-patches.lock.json`, `patches/runtime/` | The reviewed Makepad patch ([OctoSense-org/makepad#30](https://github.com/OctoSense-org/makepad/pull/30)) and its expected tree. |
| `tools/setup-native.py` | Prepares and checks the pinned sibling checkouts. |
| `scripts/` | `upstream.py` (WM provenance and catalog regeneration), `smoke.py` (native smoke test), their Python tests, and `provision-appcard-llm.sh` (Android). |
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
  makepad/, octoscript/    the revisions that release's runtime.json pins; makepad carries the patch
  OctoSense-System-Apps/   the revision native-apps.lock.json pins
```

| Command | Effect |
| --- | --- |
| `python3 tools/setup-native.py` | Clone missing siblings at their pinned revisions and apply the Makepad patch. |
| `python3 tools/setup-native.py --check` | Verify the siblings without changing anything. |
| `python3 tools/setup-native.py --check --cargo-manifest Cargo.toml` | Also check the locked Cargo graph: one Makepad, one octos, one App Hub. |
| `python3 tools/setup-native.py --update` | Move clean checkouts to the locked revisions (after the lock files change). |
| `--root DIR`, `--cache DIR` | Use another workspace directory; reuse local Git object caches. |

Local changes in a sibling are preserved; `--update` only moves clean checkouts. A clean checkout of the commit the patch was cut from (`source_commit` in `runtime-patches.lock.json`) is accepted as the same tree.

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
| `app-hub` | on | Links `octosense-app-hub-app` (store `apphub`, Card runner `card`, system apps) and the Mail host service `octosense-mail-service`. Without it the build has no App Hub and no system apps. |
| `app-reference` | off | Links Reference as a module. |
| `app-sheets` | off | Links Makepad's Sheets as a module. |
| `app-photos` | off | Links Makepad's native Photos module; it replaces the Photos system app of the same id (for comparison). |
| `app-appcard` | off | Links the AppCard assistant module (`apps/appcard`). |
| `app-aichat` | off | Links Makepad's AI chat as a module, without its model engine. |
| `app-rinx` | off | Links [Rinx](https://github.com/upstreamlabs/Rinx), the Matrix client, as a module. |
| `mobile-apps` | off | `app-reference` + `app-sheets` + `app-appcard` + `app-hub`: the set phone builds link, for testing on desktop. |

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
| `OCTOS_APP_CORE_BIN`, `OCTOS_APP_CORE_DIR` | Point the AppCard assistant at a local octos kernel. |
| `MAKEPAD_REMOTE`, `MAKEPAD_HIDE_WINDOWS` | Remote-control bridge; hidden windows (see [Demos](#demos)). |

## The app model

The launcher lists four kinds of app together:

| Kind | Comes from | Runs as | Launcher id |
| --- | --- | --- | --- |
| **System apps**: News, Photos, Maps, Camera, Mail | OctoSense-System-Apps `apps/<name>/bundle`, selected by `system-apps.json`, packed into the build | Contained Splash programs in App Hub's Card runner, each in its own isolate under the capabilities its manifest asks for | `<name>` (manifest id `os.<name>`) |
| **Store apps** | The signed App Hub catalog, installed from the store (`apphub`) | The same Card runner. Every open is checked against the catalog; an update closes old instances. | `hub:<manifest-id>` |
| **Native modules** | Rust crates linked into this binary | In-process `AppModule`s. Trusted code only: App Hub, AppCard, Reference and the `app-*` features. | module id |
| **Developer programs** | `config/apps.json` | Separate processes in tiles, over Makepad's `--stdin-loop` hosting protocol, built on first launch | catalog `id` |

Precedence: a linked native module beats a system app of the same id, and a system app beats a catalog row of the same id. That is why Makepad's example Mail and Photos are dropped from the shipped catalogs (`drop` in `config/apps.overlay.json`).

### Containment and permissions

A contained app is a bundle: `manifest.json` (id, version, capabilities) plus `main.splash`. The Card runner grants only the capabilities the manifest lists (Mail asks for `storage` and `mail`). The Makepad patch ([makepad#30](https://github.com/OctoSense-org/makepad/pull/30)) enforces this at every exit of an isolate: network and web sockets answer to the app's host list, raw sockets and servers are refused, files stay in the app's storage jail, and password or one-time-code fields are inert inside a policed isolate.

### Host services and host-owned sheets

Secrets are the host's. An app that needs an account calls a **host service** through `host.request`; the service runs in the shell with the credentials, and the app never gets a socket or a password.

Mail is the worked example (`octosense-mail-service`, from `../OctoSense-System-Apps/apps/mail/host-service`):

- `mail.add_account` raises the host's **sign-in sheet**, a separate isolate drawn over the app. Only that sheet's calls (`mail.sheet.submit`, `mail.sheet.cancel`) can carry a password.
- The service tests the account, stores the password in the platform secret store (macOS keychain), and grants the account only to the app that added it.
- Mail state lives under the host's own directory, outside every app's jail.

New app features that need a password, PIN or token belong in a host service and a host sheet, never in the app's own UI.

### Store apps (App Hub)

App Hub is on by default. Open **App Hub** from the launcher to browse the signed catalog and install apps; installed apps appear in the launcher without a restart. The catalog origin defaults to the App Hub repository and can be pointed elsewhere with `OCTOSENSE_HUB`. To build and publish an app, start from [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow).

### Choosing and overriding system apps

`system-apps.json` names the apps and where their bundles are:

```json
{
  "schema": 1,
  "source": "../OctoSense-System-Apps/apps",
  "apps": ["news", "photos", "maps", "camera", "mail"],
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

`apps/appcard` (feature `app-appcard`, always linked on phones) hosts the whole AppCard assistant in one tile: `octos-app` from `../OctoSense-System-Apps/apps/appcard/app/app`, built without its `standalone` feature. Routing, cards, sessions, the composer and the kernel agent run inside the tile's isolate; `ask` is the module's AI-bus tool.

```sh
cargo run --release --features app-appcard -- --module appcard
```

On desktop, `OCTOS_APP_CORE_BIN` and `OCTOS_APP_CORE_DIR` point it at a local octos kernel; without one it shows its login / WebSocket screen. Every octos crate comes from octos-org/octos at the one revision `octos-app` pins.

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

Phone builds always link Reference, Sheets and AppCard, and App Hub with the system apps through the default feature. The launcher label is **OctoSense**, application id `dev.makepad.octosense`. AppCard's Java features (GPS, notifications, share, intents) and the bundled `liboctos.so` kernel need the fork's buildtool and `MAKEPAD_ANDROID_EXTRA_LIBS`; see [docs/android-appcard-build.md](docs/android-appcard-build.md). The dedicated phone shell is OctoSense-ROM's `home/`.

## Desktop styles and settings

- Eight desktop styles. Desktop builds start in **OctoSense**, with Liquid Glass frames and a **Light / Dark** switch in the top bar; the others are Omarchy, macOS, Windows, Windows 2000, NeXTSTEP, iOS and Android. Theme sources are in `resources/themes/`, wallpaper provenance in [resources/wallpapers/README.md](resources/wallpapers/README.md).
- Keys: **⌘Space** menu, **⌘W** close tile, **⌘F** tile fullscreen, **⌘1…0** workspaces, **⌘Shift1…0** move tile. **Learn → Keybindings** lists them; see [KEYBINDINGS.md](KEYBINDINGS.md).
- State lives in `~/.octosense` (`OCTOSENSE_HOME`); hosted apps get it as `MAKEPAD_HOME`.
- Local models for the AI pane (**F10**): [docs/local-ai.md](docs/local-ai.md). The desktop works without a model.

## Pinning and updating siblings

| To move | Edit | Then |
| --- | --- | --- |
| Makepad / OctoScript | `native-runtime.lock.json` (a new OctoScript-Makepad release), and the `rev` of the Makepad Git dependencies in `Cargo.toml` and `apps/*/Cargo.toml` | Rebase the patch if needed, update `runtime-patches.lock.json` (base, sha256, tree) |
| System apps, Mail service, AppCard | `revision` in `native-apps.lock.json` | — |
| App Hub | `rev` of `octosense-app-hub-app` and the three `[patch]` entries in `Cargo.toml` | — |

After any change: `python3 tools/setup-native.py --update`, `cargo update` as needed, then `python3 tools/setup-native.py --check --cargo-manifest Cargo.toml` and the tests below.

`scripts/upstream.py sync|status|diff|update` tracks the WM files imported from official Makepad (`upstream/makepad.json`, baseline `74b63be8`); see [docs/upstream.md](docs/upstream.md). It needs `--source` pointing at a full clone of official Makepad: the `../makepad` sibling is a shallow checkout of the fork and does not contain the baseline commit.

## Testing

```sh
cargo test --locked --workspace
cargo test --locked --workspace --features mobile-apps
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

`.github/workflows/runtime.yml` runs on every push and pull request, on macOS 14: `setup-native.py`, `cargo check --locked --workspace --features mobile-apps`, and `setup-native.py --check --cargo-manifest Cargo.toml`. It does **not** run `cargo test`, the Python tests or the smoke tests; run those locally before opening a PR.

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
