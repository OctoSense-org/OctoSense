# OctoSense

English | [简体中文](README.zh-CN.md)

[OctoSense](https://github.com/OctoSense-org) is an agent shell on top of your operating system: a launcher and apps that look like the ones you know, with one agent behind them. This repository holds all of OctoSense's own code in one place ([ADR 0001](docs/adr/0001-one-octosense-repository.md)): the shell, its services, the first-party system apps, and the three products built from them.

| Product | What it is | Where |
| --- | --- | --- |
| **OctoSense desktop** | The shell as one Makepad window on macOS (Windows and Linux untested): launcher, dock, tiles, hosted apps | [`desktop/`](desktop/README.md) |
| **OctoSense Home** | The phone shell, an ordinary Home app for any Android phone (also OpenHarmony and the iOS simulator) | [`phone/`](phone/README.md) |
| **OctoSense ROM** | LineageOS 22.2 for the OnePlus 6 with Home, the privileged system bridge, Quickstep and SystemUI preinstalled | [`rom/`](rom/README.md) |

It was OctoSense-Desktop; OctoSense-ROM and OctoSense-System-Apps were imported into it with their history on 2026-09-27 and are archived.

> **Building an OctoSense app?** You do not need this repository to build, check or publish one. Start at the [OctoSense-org profile](https://github.com/OctoSense-org)'s reading list: [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) (`AGENTS.md`, then `docs/QUICKSTART.md`) and [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub). The system apps in [`apps/`](apps/README.md) are complete examples of the same app shape (`apps/<name>/bundle/`). Build the desktop shell from here only to see your app in a shell before it is published ([PUBLISHING §4](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/docs/PUBLISHING.md#4-rehearse-the-store-path-locally)).

## Layout

| Path | What it is |
| --- | --- |
| [`desktop/`](desktop/README.md) | Desktop packaging, package `octosense`: the desktop shell sources (`src/`, until the shared shell crate lands), catalogs (`config/apps.json`), themes and wallpapers, the window-manager sync from upstream Makepad (`upstream/`, `scripts/upstream.py`), the desktop's system-app selection. |
| [`phone/`](phone/README.md) | The Home app, package `octosense-home` (APK id `dev.makepad.octosense`): the phone shell sources (`src/`), Android, OpenHarmony and iOS packaging, the built-in Settings app, the phone side of the system bridge (`android/`), the phone's system-app selection. |
| [`rom/`](rom/README.md) | The OnePlus 6 ROM image only: `vendor/` (product, privileged permissions, overlays, Settings backends, the privileged agent), `patches/`, image, flash and OTA scripts, the Home APK build scripts, `web-installer/`, product tests. |
| `crates/kernel/` | The octos kernel service: the [octos](https://github.com/octos-org/octos) agent kernel as a shell service, one per process, configured by AI providers and shared by its consumers. |
| `crates/app-peers/` | The app-agent broker: apps' access to the assistant ([Rinx ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md)). |
| [`apps/`](apps/README.md) | The system apps (News, Photos, Maps, Camera, Mail, AI providers) as contained script apps, their host services (`mail`, `llm`), the native comparison modules (`apps/*/native`), `apps/reference`, and the opt-in AppCard assistant (`apps/appcard`). |
| `tools/` | `setup.py` (the pinned framework sources), the reviewed Makepad runtime patch (`runtime-patches/`). |
| [`docs/adr/`](docs/adr/README.md) | Architecture decisions: this repository's, and the Home decisions 0001–0006 kept as history. |
| `Cargo.toml`, `Cargo.lock` | One workspace. Every external dependency is pinned once in `[workspace.dependencies]`. |
| `native-runtime.lock.json`, `runtime-patches.lock.json` | The OctoScript-Makepad release (and through it Makepad and OctoScript), and the reviewed patch on top of Makepad. |

The shell exists once per packaging today (`desktop/src`, `phone/src`); merging them into one shell crate, with desktop and phone as targets and features, is the next phase of [ADR 0001](docs/adr/0001-one-octosense-repository.md).

## What it depends on

Pinned exactly once, in the root `Cargo.toml` and the runtime locks:

| Repository | Role |
| --- | --- |
| [makepad (OctoSense fork)](https://github.com/OctoSense-org/makepad) | The UI framework and the `cargo-makepad` packager. Checked out in `.sources/makepad`, plus the reviewed runtime patch. |
| [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad), [OctoScript](https://github.com/OctoSense-org/OctoScript) | The runtime release that names the Makepad and OctoScript revisions (`native-runtime.lock.json`). |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | The signed catalog, the store, the Card runner that contains every app (`octosense-app-hub-app`). |
| [octos](https://github.com/octos-org/octos) | The agent kernel. On Android the APK bundles it as `liboctos.so`; on a desktop the kernel service runs the binary named by `OCTOS_APP_CORE_BIN`. |
| [Rinx](https://github.com/hagency-org/Rinx) | Matrix chats and mini apps, hosted as a native module. |

Related, not build inputs: [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) (how apps are built and published), [OctoScript-Android](https://github.com/OctoSense-org/OctoScript-Android) and [OctoScript-OH](https://github.com/OctoSense-org/OctoScript-OH) (other renderers), the [OctoSense website](https://github.com/OctoSense-org/octosense-org.github.io).

## Set up

Stable Rust (`cargo` in `~/.cargo/bin`), Git, Python 3.9+ (3.11 for `desktop/scripts/upstream.py`) and, on macOS, the Xcode Command Line Tools. Makepad and OctoScript resolve to checkouts in `.sources/` (git-ignored) that the setup script prepares at the pinned revisions:

```sh
git clone https://github.com/OctoSense-org/OctoSense.git
cd OctoSense
python3 tools/setup.py                  # prepare .sources/ (makepad, octoscript, octoscript-makepad)
python3 tools/setup.py --check --cargo  # verify: one Makepad, App Hub, octos and Rinx in the graph
```

`--update` moves clean checkouts after the locks change; `--cache DIR` borrows Git objects from existing clones (`DIR/makepad`, `DIR/octoscript`, `DIR/octoscript-makepad`). Local changes in `.sources/` are preserved.

## Build

**Desktop** (from the root or `desktop/`; details in [desktop/README.md](desktop/README.md)):

```sh
cargo run --release -p octosense
cargo check --locked -p octosense --features mobile-apps                        # the set phones link
cargo check --locked -p octosense -p octosense-appcard --features mobile-apps,app-appcard
```

**Phone** (from `phone/`, which selects the phone's system apps; details in [phone/README.md](phone/README.md)):

```sh
cd phone
cargo run --release -p octosense-home --features mobile-only    # Home in a phone-sized window
cargo check --locked -p octosense-home --features mobile-apps
python3 ../rom/scripts/build-home.py --help                     # the Home and Bridge APK pair, liboctos.so bundled
```

**ROM image** (Linux build host, external LineageOS tree; not in CI): [rom/README.md](rom/README.md).

Hosted apps and UI tests run with hidden windows and a local control surface: `MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=<port>` (routes under `/help`).

## CI

Path-filtered workflows in `.github/workflows/`, so a change runs only the jobs its paths need:

| Workflow | Runs for | Checks |
| --- | --- | --- |
| `desktop.yml` | `desktop/`, `crates/`, `apps/`, the workspace files, `tools/` | compiles the desktop (default, `mobile-apps`, `mobile-apps,app-appcard`), the desktop and setup tool tests |
| `phone.yml` | `phone/`, `crates/`, `apps/`, the workspace files, `tools/` | compiles Home and its bundled modules and runs its tests on macOS; the longest job |
| `apps.yml` | `apps/`, `crates/`, the workspace files | the kernel service, app peers, AI providers config, the Mail and `llm` host services, AppCard |
| `rom.yml` | `rom/`, `phone/android/`, the phone's Android resources and tests | product tests, the generated Agent Binder client, the web installer |

Each workflow's graph check (`tools/setup.py --check --cargo`) asserts one Makepad, one App Hub, one octos and one Rinx in the locked graph.

## Releases

ADR 0001 tags each product on its own: `desktop-v*`, `home-v*` (APK), `rom-v*` (image), with build receipts that record the repository commit. System apps ship only inside the shells, admitted by digest; they are not released separately. The ROM releases published so far (for example `20260919-j`, which flashed phones update from) are on the archived OctoSense-ROM repository.

## Contributing

`main` is protected: every change goes through a pull request, and force pushes are blocked. One change is one pull request, across `desktop/`, `phone/`, `crates/` and `apps/` as needed; there are no internal pins to move. Rules for people and coding agents are in [AGENTS.md](AGENTS.md).

## License

Apache License 2.0 ([LICENSE](LICENSE), [NOTICE](NOTICE)). Source copied from Makepad keeps its MIT notice ([LICENSES/](LICENSES)). Dependencies keep their own licenses.
