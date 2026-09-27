# OctoSense

English | [简体中文](README.zh-CN.md)

[OctoSense](https://github.com/OctoSense-org) is an agent shell on top of your operating system. This repository holds all of it in one place ([ADR 0001](docs/adr/0001-one-octosense-repository.md)): the shell for desktop and phone, its services, the system apps, and the ROM image. It was OctoSense-Desktop; OctoSense-ROM and OctoSense-System-Apps were imported with their history.

**Building an OctoSense app?** You do not need this repository: start at the [OctoSense-org profile](https://github.com/OctoSense-org)'s reading list (OctoScript-App-Design-Flow's `AGENTS.md`, then `docs/QUICKSTART.md`).

## Layout

| Path | What it is |
| --- | --- |
| `desktop/` | Desktop packaging, package `octosense`: the desktop shell (`src/` until the shared shell crate lands), catalogs, themes, upstream window-manager sync. [README](desktop/README.md) |
| `phone/` | The Home app, package `octosense-home` (APK id `dev.makepad.octosense`): Android, OpenHarmony and iOS packaging, the phone shell (`src/`), the Settings app, the phone side of the system bridge. [README](phone/README.md) |
| `rom/` | The OnePlus 6 ROM image only: `vendor/`, `patches/`, image/flash/OTA scripts, `web-installer/`, product tests. [README](rom/README.md) |
| `crates/kernel/` | The octos kernel service (`octosense-octos-core`). |
| `crates/app-peers/` | The app-agent broker (`octosense-app-peers`, Rinx ADR 0007). |
| `apps/` | System apps (News, Photos, Maps, Camera, Mail, AI providers) as script bundles, their host services, the native module variants (`apps/*/native`), `apps/reference`, and the opt-in AppCard assistant (`apps/appcard`, module in `apps/appcard/module`). [README](apps/README.md) |
| `tools/` | `setup.py` (framework sources), the reviewed Makepad runtime patch (`runtime-patches/`). |
| `docs/adr/` | Architecture decisions for the repository. |
| `Cargo.toml`, `Cargo.lock` | One workspace. Every external dependency (Makepad, OctoScript, App Hub, octos, Rinx) is pinned once in `[workspace.dependencies]`. |
| `native-runtime.lock.json`, `runtime-patches.lock.json` | The OctoScript-Makepad release (and through it Makepad and OctoScript), and the reviewed patch on top of Makepad. |

## Set up

Stable Rust (`cargo` in `~/.cargo/bin`), Git and Python 3.9+. Makepad and OctoScript resolve to checkouts in `.sources/` (git-ignored) that the setup script prepares at the pinned revisions:

```sh
git clone https://github.com/OctoSense-org/OctoSense.git
cd OctoSense
python3 tools/setup.py                  # prepare .sources/ (makepad, octoscript, octoscript-makepad)
python3 tools/setup.py --check --cargo  # verify: one Makepad, App Hub, octos and Rinx in the graph
```

`--update` moves clean checkouts after the locks change; `--cache DIR` reuses local Git object caches. Local changes in `.sources/` are preserved.

## Build

Desktop (run from the root or `desktop/`):

```sh
cargo run --release -p octosense
cargo check --locked -p octosense --features mobile-apps           # the embedded-apps set
cargo check --locked -p octosense -p octosense-appcard --features mobile-apps,app-appcard
```

Phone (run from `phone/`, which selects the phone's system apps):

```sh
cd phone
cargo check --locked -p octosense-home --features mobile-apps
python3 ../rom/scripts/build-home.py --help   # the Home/Bridge APK pair, with liboctos.so bundled
```

CI is path-filtered: `Desktop` (desktop, crates, apps), `Phone` (phone, crates, apps), `Apps and services` (apps, crates) and `ROM` (rom, phone Android sources).

## License

Apache-2.0 (see [LICENSE](LICENSE) and [NOTICE](NOTICE)); third-party notices in `LICENSES/`.
