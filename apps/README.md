# OctoSense System Apps

English | [简体中文](README.zh-CN.md)

The first-party apps that ship with [OctoSense](https://github.com/OctoSense-org),
the agent shell on top of your operating system:

- **News, Photos, Maps, Camera and Mail** are *contained script apps*. Each is
  an OctoScript (Splash) program in a `bundle/`, run by App Hub's Card runner
  in its own isolate, under exactly the permissions its `manifest.json` asks
  for. That is the same containment a store app gets. They are also worked
  examples of the app shape any developer publishes through the App Hub.
- **Mail's host service** (`apps/mail/host-service`) is the Rust half of Mail:
  IMAP/POP3/SMTP, the account store and the sign-in sheet, run by the shell.
  The app gets mail, never a password or a socket.
- **AppCard** (`apps/appcard`) is the one native app: the "Ask anything"
  assistant, a Rust module (`octos-app`) that the shells link in-process and
  that runs on the [octos](https://github.com/octos-org/octos) agent kernel.

Rules for agents working here are in [AGENTS.md](AGENTS.md) and
[apps/appcard/AGENTS.md](apps/appcard/AGENTS.md).

## The apps

| App | Id | What it does | Capabilities (manifest) | Network hosts (manifest) | Host services |
| --- | --- | --- | --- | --- | --- |
| [News](apps/news/bundle) | `os.news` | Hacker News, TechMeme and Google News feeds in tabs (Today, HN, TechMeme, Google, Saved), with a reader for stories | `storage`, `net`, `images`, `web` | `hn.algolia.com`, `www.techmeme.com`, `news.google.com` | none |
| [Photos](apps/photos/bundle) | `os.photos` | A sample library: moments, albums, people, favorites, a grid with selection, a full-screen viewer | `storage` | none | none (full-size files come from a shell asset mount, see below) |
| [Maps](apps/maps/bundle) | `os.maps` | `MapView` map, place search, places, routes and a drive mode; starts at the device's GPS fix when there is one | `storage`, `net`, `location` | `photon.komoot.io`, `router.project-osrm.org`, `overpass-api.de`, `overpass.kumi.systems`, `maps.mail.ru`, `overpass.openstreetmap.fr` | none |
| [Camera](apps/camera/bundle) | `os.camera` | Photo and video over the runtime's `CameraPreview` widget, flash and zoom, a thumbnail of the last shot and a viewer | `storage`, `camera`, `microphone`, `library` | none | none |
| [Mail](apps/mail/bundle) | `os.mail` | Accounts, folders, message list, reader (HTML rebuilt by the service) and composer | `storage`, `mail` | none (the service connects, not the app) | [`mail`](apps/mail/host-service) |
| [AppCard](apps/appcard) | native | The AppCard assistant: a routing brain picks or composes an app agent, which generates a live Splash or webview card | n/a (not a bundle) | n/a | n/a |

What each capability means is defined by App Hub's closed list
(`KNOWN_CAPABILITIES` in `crates/app-policy/src/manifest.rs`): `images` shows
pictures from any public https host, `web` opens a page in the system WebView,
`library` offers captures to the system photo library, `mail` reaches the
host's mail service. `net` reaches only the hosts the manifest lists.

### Status and known gaps

- **Camera**: on the OnePlus 6 test run (2026-09-25) Camera captured a photo
  and released the camera in the background, but the live preview drew pure
  black; unresolved. Desktop builds have no camera and the Android emulator
  refuses one, so capture is untested elsewhere.
- **Photos**: the bundle ships only 75 thumbnails (`bundle/thumbs/`, about
  2 MB). The full-size files the viewer shows are served at
  `{{assets}}/photos/...` only when a shell mounts them: ROM Home mounts
  `home/apps/photos/resources/photos` (in OctoSense-ROM, about 87 MB);
  OctoSense-Desktop mounts nothing, so the viewer has no full-size image
  there.
- **News, Maps**: run in `card-host` during development, but not exercised
  end to end in the shell PRs' test runs (the test phone had no network).
- **Mail**: verified with the demo mailbox on desktop and on the OnePlus 6.
  The host service pins App Hub `0d36f50b` (main after
  OctoSense-App-Hub#4), the rev the shells link, so a shell's graph has one
  `octosense-appstore` and one host-service registry without a `[patch]`.
- **Script bundles have no CI here.** `.github/workflows/appcard.yml` covers
  only `apps/appcard/**`.
- **AppCard `personal-data` skill** reads the old native Mail module's
  `mailbox-*.json` files. The script Mail app's mail now lives in the host
  service's own directory (`<host_dir>/mail/box-*.json`), so the skill
  probably no longer sees it; not verified.
- Only Camera ships its own launcher icon (`bundle/icon.png`); the shells
  draw the others.

## How shells consume this repository

The shells pin a revision of this repository and choose which apps to ship.
This wiring is **in progress** in two open pull requests:
[OctoSense-ROM#18](https://github.com/OctoSense-org/OctoSense-ROM/pull/18)
(Home, standalone launcher and ROM image) and
[OctoSense-Desktop#36](https://github.com/OctoSense-org/OctoSense-Desktop/pull/36).
Until they merge, the shells' `main` branches still use the older native
modules and Octoscript-AppCard.

With those PRs, a shell:

1. Pins this repository in `native-apps.lock.json` (ROM: `home/native-apps.lock.json`,
   checked out at `.sources/system-apps`; Desktop: a sibling checkout at
   `../OctoSense-System-Apps`).
2. Lists the apps in `system-apps.json` and points `OCTOSENSE_SYSTEM_APPS` at
   it in `.cargo/config.toml`. App Hub's shell crate `octosense-app-hub-app`
   reads that file at build time, packs each `apps/<name>/bundle/` into the
   binary and fills in its digest. `assets` maps extra directories into an
   app's `{{assets}}` (Photos, in the ROM):

   ```json
   {
     "schema": 1,
     "source": "../.sources/system-apps/apps",
     "apps": ["news", "photos", "maps", "camera", "mail"],
     "assets": { "photos": { "photos": "apps/photos/resources/photos" } }
   }
   ```

3. Links `octosense-mail-service` (path dependency on the pinned checkout) and
   registers it at startup: `register()` for real accounts, or
   `register_demo()` when the shell's app config has `mail_demo: true`.
4. Links AppCard's `octos-app` with `default-features = false` and mounts it
   through its `AppShell` widget (see [AppCard](#the-appcard-assistant)).

Changes here reach a device only when a shell moves its pin, in a pull request
in that shell's repository.

## Repository layout

```
apps/<name>/bundle/          a contained script app: manifest.json, main.splash, artwork
apps/mail/host-service/      octosense-mail-service, the `mail` host service (Rust)
apps/appcard/                the native AppCard assistant
  app/                       Cargo workspace: octos-app + store/transport/render crates
  a2app/                     Splash card memory (specs, widget patterns, lint rules), compiled in
  a2app-l0/                  L0 card framework, catalog and per-app exemplar cards, compiled in
  personal-data/             octos skill: read-only search over Mail and Calendar data
  vendor/                    vendored third-party crates (rustyline, mmap-rs; see NOTICE)
  tools/                     setup-native.py, octos macOS/OpenHarmony runners, build-android.sh, ...
  docs/                      architecture, build and review notes
  native-runtime.lock.json   the Octoscript-Makepad release AppCard builds against
.github/workflows/appcard.yml   CI for apps/appcard
```

## A system app bundle

```
apps/<name>/bundle/
  manifest.json     id, version, name, capabilities, network.hosts, integrity
  main.splash       the program
  icon.png|svg      optional launcher art (Camera has one)
  thumbs/ ...       any other files the app loads, as {{assets}}/<path>
```

`main.splash` refers to its own files through the `{{assets}}` placeholder,
which the runner replaces with the origin it serves the bundle from (Photos:
`let assets = "{{assets}}"`, then `assets + "/thumbs/" + id + ".jpg"`).

A system app has the same shape as a store app, with these differences:

| | System app (this repo) | Store app (App Hub) |
| --- | --- | --- |
| Id | `os.<name>`. `os.` is reserved: `hub check` refuses it and no device installs one from a store | any other id |
| Delivery | packed into the shell binary at build time from `system-apps.json` | downloaded from the signed catalog |
| Admission | by digest only (`HostLimits::system()`); the source manifest leaves `integrity.bundle_blake3` empty and the build fills it | digest plus publisher signature |
| Ceilings | `HostLimits::system()`: 64 MB storage, 128 MB memory, a larger instruction budget, since the app lives as long as it is open | `HostLimits::default()`: sized for a card |
| Extra files | a shell can mount directories into `{{assets}}` | only what is in the bundle |

Everything else is identical: the same isolate, the same capability checks,
the same network allowlist. How to write such an app (language, APIs, the
`octo` CLI) is in
[OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)
(`docs/QUICKSTART.md`, `docs/SCRIPT-API.md`).

## Running a bundle during development

App Hub's `card-host` runs one bundle under the policy its manifest resolves
to, with the same admission order a device uses. The `--system` and `--static`
flags, and host-service support, are on App Hub's `apps/script-and-system-apps`
branch
([OctoSense-App-Hub#4](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/4),
open); App Hub `main` does not have them yet.

```sh
# in an OctoSense-App-Hub checkout on that branch
cargo build --release -p octosense-card-host --bin card-host

card-host --bundle <System-Apps>/apps/news/bundle --system
card-host --bundle <System-Apps>/apps/photos/bundle --system --static photos=<dir of full-size photos>
```

| Flag | Effect |
| --- | --- |
| `--bundle <dir>` | the bundle (default: current directory) |
| `--system` | admit as a system app: digest only, system ceilings; an empty digest is filled in memory |
| `--static <prefix>=<dir>` | serve `<dir>` at `{{assets}}/<prefix>/...`, as a shell serves a mounted directory |
| `--app-data <dir>` | where the app's storage jail is made (default `$TMPDIR/octosense-card-apps`) |
| `--allow-unsigned`, `--stamp` | for store bundles; not needed with `--system` |

The log line `card-host: <id> <version> admitted — capabilities …, hosts …`
shows what the app got; `card-host: refused: …` means nothing is drawn.

Set `MAKEPAD_REMOTE=<port>` to drive the window over localhost HTTP
(`/snap`, `/click?x=..&y=..`, `/g` for a screenshot, `/quit`); App Hub's
`docs/DEVELOPMENT.md` lists the routes.

**Mail** needs its host service, and `card-host` registers none. Run Mail in
a shell build that links the service, with the demo mailbox (any address,
password `demo`, sample messages, sends that go nowhere):

```sh
# OctoSense-Desktop with #36 (repository root), or ROM Home with #18 (in home/)
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release -p octosense
```

The demo keeps its password in a file, so no keychain prompt appears.

## Host services and sheets

Some work needs something a contained app must never hold: a socket, a
credential, a device. A **host service** does that work in the shell, in
Rust. The app calls it with `host.request("<family>.<method>", args, fn(r){…})`;
the isolate refuses the call unless the manifest grants the family (`mail`),
and the service answers with data, never the means. The runtime side lives in
App Hub (`crates/appstore/src/services.rs`).

When the person has to act (type a password, approve an account), the service
raises a **sheet**: a host-owned Splash surface drawn over the app, in its own
isolate under no app's policy. Calls from the sheet arrive marked
`from_sheet`.

**Secrets are the host's.** No app collects a password, PIN or one-time code:

- a password field in a contained app takes no input;
- methods that carry a secret live under `<family>.sheet.*`
  (`mail.sheet.submit`, `mail.sheet.cancel`) and are dispatched only when
  they come from the sheet, before any service sees them;
- only a service can open a sheet; an app cannot.

### The `mail` service

`octosense-mail-service` (`apps/mail/host-service/src/`):

| File | Role |
| --- | --- |
| `lib.rs` | the service: `mail.accounts`, `add_account` (raises the sign-in sheet), `remove_account`, `folders`, `sync`, `list`, `message`, `mark_read`, `send`; `register()`, `register_demo()`, `register_with*()`; the `Transport` trait |
| `imap.rs` | IMAP client (folders, read flag back to the server) |
| `network.rs` | POP3 and SMTP, MIME decoding; credentials never appear in errors |
| `html.rs` | rebuilds a message as the few tags Mail's `Html` view draws, with nothing remote in it |
| `vault.rs` | where passwords go: macOS/iOS Keychain, Android (a file sealed with an Android Keystore key), owner-only file elsewhere; `OCTOSENSE_MAIL_VAULT=file` forces the file store for unsigned dev builds |

Account metadata (no passwords) and fetched mail live under the host's own
directory (`<host_dir>/mail`), outside every app's jail. Each account is
granted only to the apps that added it. The service tests an account before
keeping it.

## The AppCard assistant

The "Ask anything" tile. You type a request; a routing brain (the AMA) picks
or composes an app agent; the agent generates a live card, Splash or
webview, that binds real data at render time. It talks to octos over the
octos UI Protocol v1.

- **Code**: `apps/appcard/app`, a Cargo workspace with `octos-app` (router,
  composer, multi-agent dispatch, Splash renderer and validator, L0 card
  generation, WebView overlay), `octos-app-store` (state reducer, no
  Makepad), `octos-app-transport` (WebSocket and REST client for the octos
  UI Protocol) and `octos-app-render` (streaming-markdown renderer).
- **octos**: every octos crate comes from git `octos-org/octos` at the one
  rev in `apps/appcard/app/Cargo.toml` (today `18fcd3f1`, branch
  `appcard/mate70-on-main` until it lands on octos `main`). A shell that also
  depends on octos must use the same rev.
- **Makepad**: not vendored. Makepad, Octoscript and Octoscript-Makepad are
  checkouts *beside* this repository, at the release
  `apps/appcard/native-runtime.lock.json` selects.

Build and test (details in [apps/appcard/README.md](apps/appcard/README.md)):

```sh
cd apps/appcard
python3 tools/setup-native.py                       # prepare the sibling runtime
python3 tools/setup-native.py --check --cargo-manifest app/Cargo.toml
PYTHONPATH=tools python3 -m unittest core.test_native_runtime

cd app
cargo check
cargo test --workspace
cargo clippy -p octos-app -p octos-app-store -p octos-app-transport -p octos-app-render --all-targets --no-deps -- -D warnings
cargo run -p octos-app                               # standalone window (default feature `standalone`)
```

The standalone app reaches octos through `~/.config/octos-app/server.json`,
or `OCTOS_BASE_URL`/`OCTOS_BEARER`/`OCTOS_PROFILE_ID`, or a local core
binary via `OCTOS_APP_CORE_BIN` and `OCTOS_APP_CORE_DIR`
(`tools/octos-macos.py` sets these up; see `tools/OCTOS-MACOS.md`). Android
and OpenHarmony builds: `docs/BUILDING-ANDROID.md`,
`docs/BUILDING-OPENHARMONY.md`.

**How shells embed it.** A shell depends on `octos-app` with
`default-features = false` (no `fn main`), calls
`octos_app::register_script_mods(vm)`, and mounts `AppShell::create(vm)`: a
widget that owns the app and draws `OctosAppBody`, the app's root without
the standalone `Window`. `AppShell::ask` submits text as if typed. In the ROM
and Desktop shells this sits in an `AppCardModule` that implements the
shell's `AppModule` trait (`home/apps/appcard` and `apps/appcard` in those
repositories).

**CI**: [.github/workflows/appcard.yml](.github/workflows/appcard.yml) runs on
changes under `apps/appcard/**` (macOS): it prepares the locked runtime, runs
the runtime-lock tests, clippy for the four crates (which compiles the whole
app), and checks the graph has one octos and one Makepad source.
`apps/appcard/app/.github/workflows/` is left over from the original
repository and does not run here.

## Changing an app

1. Edit `apps/<name>/bundle/`. Use only APIs documented in
   OctoScript-App-Design-Flow's `docs/SCRIPT-API.md` or already used by
   another app here; check the runtime source before using anything else.
2. Ask only for what the app uses. A new network host goes in
   `network.hosts`; a new capability must exist in App Hub's
   `KNOWN_CAPABILITIES`.
3. Never add a password or code field. If the app needs a secret, a host
   service and its sheet handle it.
4. Run it with `card-host --system` (Mail: in a shell with the demo). Test on
   a phone through the ROM's Home as a separate test package, never by
   replacing the device's installed Home.
5. Open a pull request here. After it merges, bump the pin in each shell
   (`native-apps.lock.json`) in a pull request there.

A **new** system app is a new `apps/<name>/bundle/` with an `os.<name>` id,
plus an entry in each shell's `system-apps.json`.

## Testing

| What | How |
| --- | --- |
| Mail service | from a shell workspace that links it: `cargo test -p octosense-mail-service` (ROM: in `home/`). The keychain test is ignored by default: `cargo test -p octosense-mail-service -- --ignored keychain` |
| AppCard | the commands above; CI in `appcard.yml` |
| Script bundles | by hand in `card-host` and in a shell, driven over `MAKEPAD_REMOTE`. No automated UI tests here yet |

## Related repositories

| Repository | Role |
| --- | --- |
| [OctoSense-ROM](https://github.com/OctoSense-org/OctoSense-ROM) | phone shell (`home/`): standalone launcher or burned into the ROM image |
| [OctoSense-Desktop](https://github.com/OctoSense-org/OctoSense-Desktop) | desktop shell |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | catalog, gate (`hub stamp`, `check`, `scan`, `sign-manifest`, `publish`), `card-host`, the Card runner and host-service registry, and `octosense-app-hub-app`, the crate every shell links |
| [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) | how to design, build, check and publish an app |
| [OctoScript](https://github.com/OctoSense-org/OctoScript), [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad), [makepad](https://github.com/OctoSense-org/makepad) | the language and runtime |
| [octos](https://github.com/octos-org/octos) | the agent kernel AppCard runs on |

## Contributing

- Pull requests against `main`; never force-push `main`.
- Keep changes small and test them in a shell. Follow [AGENTS.md](AGENTS.md).
- AppCard changes must pass `appcard.yml`.

## History and license

The bundles and the Mail service were first written in OctoSense-mobile
(archived) and OctoScript-App-Design-Flow (formerly Octoscript-AppCard),
where their history remains. AppCard came from
OctoSense-org/OctoSense-AppCard (`d0a836b8`), split from
OctoScript-App-Design-Flow's `app/` at `cbbda4da`.

Apache-2.0 ([LICENSE](LICENSE)). Third-party components are listed in
[NOTICE](NOTICE).
