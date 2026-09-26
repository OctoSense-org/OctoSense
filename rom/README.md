# OctoSense ROM

English | [简体中文](README.zh-CN.md)

OctoSense is an agent shell on top of an operating system, built on
[Makepad](https://github.com/OctoSense-org/makepad). This repository holds the
phone shell, **OctoSense Home** (`home/`), and everything that delivers it: as
an ordinary Home app on any Android phone, or preinstalled with a privileged
agent, Quickstep and SystemUI in a LineageOS 22.2 (Android 15) image for the
OnePlus 6 (`enchilada`). The same Home also runs in a window on macOS for
development, and builds for OpenHarmony and the iOS simulator.

## Where this repository sits

| Repository | Role | How this repository uses it |
| --- | --- | --- |
| **OctoSense-ROM** (this one) | Phone shell, ROM image, installer | |
| [OctoSense-Desktop](https://github.com/OctoSense-org/OctoSense-Desktop) | The desktop shell | Home was split from it on 15 September 2026; the two still share much of their source (see [home/README.md](home/README.md)). |
| [OctoSense-System-Apps](https://github.com/OctoSense-org/OctoSense-System-Apps) | News, Photos, Maps, Camera and Mail as contained script apps, the Mail host service, and the AppCard assistant | Pinned in `home/native-apps.lock.json`, checked out to `.sources/system-apps`. |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | Signed catalog, admission gate, `hub` CLI, `card-host`, and the shared shell crate `octosense-app-hub-app` | Git dependency pinned in `home/Cargo.toml`. |
| [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) | How to build and publish an OctoSense app | Not a build input. Start there to write an app for Home. |
| [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad) | Runtime release: names the Makepad and OctoScript revisions | Pinned in `home/native-runtime.lock.json`. |
| [makepad](https://github.com/OctoSense-org/makepad) (OctoSense fork) | UI framework and the `cargo-makepad` packager | Checked out to `.sources/makepad` at the runtime's revision. |
| [octos](https://github.com/octos-org/octos) | The agent kernel behind AppCard | One revision, the one OctoSense-System-Apps' `octos-app` pins. |

The organisation overview is at
[github.com/OctoSense-org](https://github.com/OctoSense-org).

## Repository layout

| Path | Contents |
| --- | --- |
| `home/` | The Home Rust workspace (crate `octosense`), its Android, iOS and OpenHarmony resources |
| `home/src/` | The shell: `mobile*.rs` is the phone shell, `apps.rs` wires the linked modules and system apps |
| `home/apps/` | Native modules: `appcard` (the AppCard host), `reference`, and the comparison-only `news`, `photos` and `maps` |
| `home/android/` | Gradle projects: AIDL contracts, System Bridge APK, Quickstep, SystemUI previews and platform build stagers |
| `home/tools/` | `setup-native.py` (runtime sources), `build_app_icons.py`, `a11y-probe/` |
| `home/scripts/` | Desktop smoke test, Makepad import sync (`upstream.py`), Android frame measurement |
| `home/docs/` | Home ADRs, Android and performance records, design notes |
| `home/*.lock.json`, `home/system-apps.json` | Source pins and the system-app selection (see [Pins and updates](#pins-and-updates)) |
| `vendor/octosense/` | ROM product layer: makefiles, permissions, overlays, sepolicy, the privileged agent |
| `patches/` | LineageOS and kernel patches; `patches/runtime/` holds a reviewed Makepad patch when `home/runtime-patches.lock.json` names one (none today) |
| `scripts/` | Home builds, ROM staging, build, flash, release and phone checks |
| `web-installer/` | WebUSB installer for the OnePlus 6 (local developer preview) |
| `docs/` | ROM ADRs, build, flashing, update and validation records |
| `tests/` | Python tests for the build, staging, manifest and installer scripts |
| `.sources/` | Ignored. Pinned dependency checkouts made by setup |
| `out/` | Ignored. Build outputs and receipts |

## Quick start

### Prerequisites

- Git, Python 3.9 or newer (`home/scripts/upstream.py` and its test need 3.11).
- Rust stable. On machines where `cargo` lives only in `~/.cargo/bin`, put it
  on `PATH` first: `export PATH="$HOME/.cargo/bin:$PATH"`.
- Android APKs: a `cargo-makepad` Android SDK/NDK directory, the Android SDK
  with platform 35 and build-tools 35.0.0, a full JDK 17+ and Gradle 8.11.1.
  The build scripts install none of these. `cargo-makepad makepad android
  --sdk-path=<dir> install-toolchain` prepares the first.
- ROM images: a LineageOS 22.2 tree for `enchilada`, the OnePlus vendor blobs,
  the kernel source and the ROM signing keys, all outside this repository.

### Prepare the pinned sources

Run from the repository root:

```sh
python3 scripts/setup-home.py              # check out every pin into .sources/
python3 scripts/setup-home.py --check --cargo
```

Setup checks out OctoSense-System-Apps, OctoScript-Makepad, Makepad and
OctoScript at their locked revisions (applying any reviewed Makepad patch
`home/runtime-patches.lock.json` names; none today), and refuses to touch a checkout with local changes. `--update` moves clean
checkouts to new pins; `--check` changes nothing and fails unless every
checkout matches its lock; `--cargo` also rejects a second copy of any core
Makepad crate in the dependency graph.

### Run Home on a desktop

```sh
cd home
cargo run --release --features mobile-only
```

This is the phone shell in a phone-sized window, with App Hub and the five
system apps. Without `mobile-only`, `cargo run --release` starts the universal
desktop shell; it keeps building here, but the desktop product is
OctoSense-Desktop. Only macOS is built in CI.

Useful switches:

| Switch | Effect |
| --- | --- |
| `--features mobile-apps` | Also link the native modules (Reference, Sheets, AppCard, and the native News, Photos and Maps, which then replace their script apps) |
| `-- --module <id>` | Host a linked module in-process instead of as a child process |
| `-- --test-action <name>` | Fire a shell action at startup: `launch-<app id>`, `page:<n>`, `island:demo`, `capture:<path>`, `ask-appcard:<text>`, `taps:<x>,<y>@<s>` |
| `MAKEPAD_WM_TEST_APP=<app>[:<count>]` | Launch an app (count times) once the shell is up |
| `MAKEPAD_APP_CONFIG='{"mail_demo":true}'` | Serve Mail from a demo mailbox (password `demo`) |
| `OCTOSENSE_HOME=<dir>` | Keep state somewhere other than `~/.octosense` |

### Build the Android Home APK

`scripts/build-home.sh` builds the Home APK and its System Bridge APK, signs
them together and writes `OctoSenseHome.apk`, `OctoSenseBridge.apk` and a
`build.json` receipt to `out/home/<variant>/`. It never installs or flashes.
A standalone development pair, signed with Makepad's development key:

```sh
scripts/build-home.sh --variant standalone --development \
  --sdk /path/to/makepad-android \
  --android-sdk /path/to/android-sdk \
  --gradle-home /path/to/gradle-8.11.1 \
  --java-home /path/to/full-jdk \
  --packager .sources/makepad/target/release/cargo-makepad
```

Add `--dry-run` to print the plan. For a release, replace `--development`
with `--sign-key` and `--sign-cert` pointing at the existing signer, kept
outside the checkout.

**Known problem:** without `--packager`, the script builds the pinned
`cargo-makepad` with `cargo build --locked`, which fails because the Makepad
checkout has no `Cargo.lock`. Build the packager yourself first and pass
`--packager` as above:

```sh
cargo build --release --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml
```

Use the pinned packager, not upstream's: it carries this app's Java activity
([home/docs/build-tool.md](home/docs/build-tool.md)).

**Package name.** Home's application ID is `dev.makepad.octosense`, in both
the standalone and the ROM variant. A phone running the OctoSense ROM already
has that ID, signed with the platform key, so a development build cannot
replace it. To install a test build beside it, call the packager directly
with another package name (`build-home.sh` has no option for this):

```sh
cd home
../.sources/makepad/target/release/cargo-makepad makepad android \
  --sdk-path=/path/to/makepad-android \
  --package-name=dev.makepad.octosense.scriptapps \
  build -p octosense --release
```

The APK lands in `home/target/android/makepad-android-apk/octosense/apk/`;
`run` in place of `build` also installs and starts it. Address a test package
with its own name, for example
`adb shell am start -n dev.makepad.octosense.scriptapps/.MakepadApp`.

To make a build the Home app on a phone you control, pick OctoSense in
Android's Home chooser, or:

```sh
adb shell cmd package set-home-activity dev.makepad.octosense/.MakepadApp
```

[docs/home-build.md](docs/home-build.md) covers signing, receipts and the ROM
variant; [home/README.md](home/README.md) covers the Home role, gestures and
navigation modes.

### Build and flash the ROM image (OnePlus 6)

The OS build runs on a Linux build host inside an Ubuntu 24.04 chroot, against
an external LineageOS tree. From this repository:

```sh
scripts/build-home.sh --variant rom --sdk ... --android-sdk ... \
  --gradle-home ... --java-home ... --packager ... \
  --sign-key /private/rom-keys/platform.pk8 \
  --sign-cert /private/rom-keys/platform.x509.pem
python3 scripts/stage-home.py                 # verify the receipt, copy the APKs to vendor/octosense/prebuilt/
scripts/stage-forks.sh /path/to/lineage-tree  # apply vendor/octosense and stage the Quickstep and SystemUI forks
```

Then, on the host, `scripts/run-rom-rootfs.sh` enters the chroot (under
`OCTOSENSE_BUILD_ROOT`, default `/home/ubuntu/octosense-adr0001`) and runs
`build-rom.sh preflight`, `bacon` (the full signed build) or `module <name>`.
It expects `build-rom.sh` at `exports/build-rom.sh` under the build root and
is started by systemd; that unit and the host set-up are not in this
repository. `scripts/make-keys.sh` generates the signing keys once;
`scripts/release.sh <build-tag>` fetches a finished build from the host named
in `~/.config/octosense/build.env`.

To flash:

- **Browser:** the [web installer](web-installer/README.md#flash-from-your-browser),
  a local developer preview. A fresh install erases the phone. Public web
  flashing is off until [ADR 0001](docs/adr/0001-public-web-installer.md) is
  done.
- **Command line:** `scripts/flash.sh <build dir> [serial]`, or the recovery
  sideload in [docs/flashing.md](docs/flashing.md), which also holds the
  lessons from the first flash.
- **Afterwards:** `scripts/verify-phone.sh <build-tag> [serial]` waits for
  boot and runs `scripts/checklist.sh` and `scripts/agent-test.sh`.

Updates reach a flashed phone over the air from this repository's GitHub
Releases ([docs/updates.md](docs/updates.md)); `scripts/ota-push.sh` pushes
one from a Mac.

### OpenHarmony and iOS

- **OpenHarmony:** `python3 scripts/build-home-ohos.py --deveco-home ...
  --packager ... --signing-config ...` builds a normal OpenHarmony app with
  an existing DevEco signing profile. See
  [docs/home-build.md](docs/home-build.md#openharmony-home).
- **iOS simulator:** from `home/`,
  `../.sources/makepad/target/release/cargo-makepad makepad apple ios --org=dev.makepad --app=octosense run-sim -p octosense --features mobile-only`.
  iOS is not built in CI.

## How apps get into Home

| Kind | Source | Runs as |
| --- | --- | --- |
| System apps: News, Photos, Maps, Camera, Mail | OctoSense-System-Apps `apps/<name>/bundle/`, selected by `home/system-apps.json` | Contained script apps, packed into the build |
| Store apps | The App Hub catalog, installed at run time | Contained script or card apps |
| AppCard assistant | OctoSense-System-Apps `apps/appcard/app/app` (`octos-app`) | Native module, linked in |
| Native modules | `home/apps/*`, Sheets from Makepad | Linked modules, behind features |

**System apps** ([ADR 0004](home/docs/adr/0004-system-apps-are-contained-script-apps.md)).
The build of App Hub's shell crate packs every bundle that
`home/system-apps.json` names; it finds that file through
`OCTOSENSE_SYSTEM_APPS`, which `home/.cargo/config.toml` sets. Unset, no
system apps ship. Each app runs in its own isolate under its manifest's
policy, keeps its short id (`news` for `os.news`) in the launcher, and
cannot be replaced from the store, since ids under `os.` are reserved.
Photos' sample library is mounted from `home/apps/photos/resources/photos`
rather than packed.

**Store apps.** App Hub (`apphub`) browses the signed catalog, verifies and
installs bundles, and the Card runner (`card`) opens each installed app in
its own isolate under the permissions its manifest asks for. Installed apps
appear in the launcher and Recents as `hub:<manifest-id>`. Both come from
`octosense-app-hub-app` and are linked by the default `app-hub` feature and
on every mobile build. Writing and publishing an app:
[OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow).

**Host services and secrets.** An app reaches what it must not hold through a
host service: `host.request("family.method", ...)`, granted by its manifest.
Mail is the first: the `mail` service (OctoSense-System-Apps
`apps/mail/host-service`) holds accounts and passwords, keeps passwords in
the keychain or behind an Android Keystore key, and gives the app folders,
messages and a send, never a socket or a password. Secrets are the host's:
no script app collects a password, PIN or one-time code. A person types
one only on a host-owned sheet, the runtime makes password fields inert in
a contained app, and the App Hub gate refuses bundles that declare them.

**Native modules.** Mobile builds always link Reference, Sheets, AppCard and
App Hub. On a desktop build, features opt in:

| Feature | Links |
| --- | --- |
| `app-hub` (default) | App Hub, the Card runner, the system apps and the Mail service |
| `app-reference`, `app-sheets`, `app-appcard` | Reference, Makepad Sheets, AppCard |
| `app-news`, `app-photos`, `app-maps` | The native News, Photos and Maps, for comparison; each replaces its script app |
| `app-aichat` | Makepad's aichat assistant as a module |
| `mobile-apps` | All of the above except `app-aichat` |
| `mobile-only` | The standalone phone shell (Android sets it itself) |

**AppCard** ("Ask anything") is `octos-app`'s `AppShell`, hosted by
`home/apps/appcard` as a path dependency into
`.sources/system-apps/apps/appcard/app/app`. Its kernel, `octos`, is not a
Cargo dependency of Home; to bundle it in an APK, set
`MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=<path>"` for the packager
([home/docs/android-appcard-build.md](home/docs/android-appcard-build.md),
whose pins are older than the current ones). Without it, AppCard falls back
to its WebSocket transport and login screen.

## Pins and updates

| File | Pins |
| --- | --- |
| `home/native-apps.lock.json` | OctoSense-System-Apps revision (`.sources/system-apps`) |
| `home/system-apps.json` | Which system apps ship, and the assets Home mounts for them |
| `home/native-runtime.lock.json` | OctoScript-Makepad revision; its `runtime.json` names Makepad and OctoScript |
| `home/runtime-patches.lock.json` | Reviewed Makepad patches on top of the runtime (base revision, source commit, SHA-256, resulting tree); empty today |
| `home/Cargo.toml`, `home/Cargo.lock` | Makepad `rev` (must equal the runtime's), App Hub `rev`, the octos `rev` used for `nix` |
| `home/upstream/makepad.json` | Provenance of the window-manager sources imported from Makepad |

The Cargo manifests keep **one source of each**: `[patch]` sections point
every Makepad crate (including the copies App Hub's crate and `octos-app`
name) at `.sources/makepad`; `nix` comes from the same octos revision
`octos-app` uses. App Hub needs no `[patch]`: Home and the Mail host service
name the same App Hub revision, so the graph has one App Hub source.

- **System-Apps.** Set the new revision in `home/native-apps.lock.json`, run
  `python3 scripts/setup-home.py --update`, then build once from `home/`
  without `--locked` if `octos-app`'s dependencies changed, and commit
  `home/Cargo.lock`. If the new System-Apps pins a different octos revision,
  move the `nix` patch in `home/Cargo.toml` to it.
- **Makepad / OctoScript.** Move the OctoScript-Makepad pin in
  `home/native-runtime.lock.json`, repeat its Makepad revision in every
  `rev = "…"` in `home/Cargo.toml` and `home/apps/*/Cargo.toml`, and rebase
  or drop any runtime patch. `setup-home.py --check --cargo` fails on any
  mismatch. The full procedure is in
  [home/docs/makepad-fork.md](home/docs/makepad-fork.md#adopting-a-fork-revision).
- **Runtime patch.** None today: the runtime's Makepad (main `cd812acd`)
  includes [makepad#30](https://github.com/OctoSense-org/makepad/pull/30)
  (contained script apps). When a fix must ship ahead of a runtime release,
  put the patch in `patches/runtime/` and record it as a `makepad` entry in
  `home/runtime-patches.lock.json`; setup applies it and leaves it staged, and
  `--check` accepts only the exact recorded tree.
- **App Hub.** Change the `rev` of `octosense-app-hub-app` (both
  dependency lines) to the App Hub revision the pinned System-Apps' Mail host
  service names for `octosense-appstore`. Two App Hub revisions would mean two
  host-service registries, and the Card runner would never see Mail's.

## Testing and validation

CI (`.github/workflows/home.yml`) runs the product tests on Ubuntu and the
Home build and tests on macOS, from the repository root:

```sh
python3 -m unittest discover -s tests -v
python3 scripts/setup-home.py
python3 scripts/setup-home.py --check --cargo
cd home
cargo check --locked --workspace --features mobile-apps
cargo test --locked --features mobile-apps -p octosense -p octosense-app-policy -p octosense-app-hub -p octosense-news -p octosense-appcard
cargo test --locked -p octosense-maps -- --skip view::tests --skip module::tests
cargo test --locked -p makepad-widgets splash_policy
cargo test --locked -p makepad-script-std gate::tests
```

Maps' view and module tests are skipped because they need the macOS main
thread. `octosense-app-hub-app` is tested in OctoSense-App-Hub's CI, not
here. The phone shell's own unit tests: `cargo test --features mobile-only
mobile -- --test-threads=1`.

Other checks:

- **Home scripts:** from `home/scripts`, `python3 -m unittest test_smoke
  test_upstream` (Python 3.11 for `test_upstream`).
- **Web installer:** from `web-installer/`, `npm ci --ignore-scripts`,
  `npm test`, `npm run test:browser` (`.github/workflows/web-installer.yml`).
- **UI without a person.** Every Makepad app has a localhost control surface:
  `MAKEPAD_REMOTE=<port>` (or `--remote`) prints the port and serves
  screenshots, the widget tree and real input over HTTP (`/help` lists the
  routes; not on Android). `MAKEPAD_HIDE_WINDOWS=1` keeps windows off screen
  on macOS and Windows. Combine them with `--test-action`,
  `MAKEPAD_WM_TEST_APP` and `--module`. `python3 home/scripts/smoke.py`
  drives a release build this way. The pinned Makepad's `makepad_test` crate
  (hidden windows by default) is available, but Home has no `makepad_test`
  suite yet.
- **Devices:** `scripts/checklist.sh`, `scripts/agent-test.sh` and
  `scripts/verify-phone.sh` for a flashed ROM;
  `home/scripts/measure_android_frames.py` for frame timing. Records:
  [docs/home-device-validation.md](docs/home-device-validation.md),
  [home/docs/validation.md](home/docs/validation.md),
  [home/docs/android/](home/docs/android/README.md).

Source and build checks do not prove a ROM boots, or that radios,
notifications, Recents, emergency calls and OTA recovery work. Run the device
checks before changing certificates or releasing.

## Documentation

- ROM decisions: [docs/adr/](docs/adr/README.md). Home decisions:
  [home/docs/adr/](home/docs/adr/README.md), including ADR 0004 (system apps
  as contained script apps).
- Build and delivery: [docs/home-build.md](docs/home-build.md),
  [docs/flashing.md](docs/flashing.md), [docs/updates.md](docs/updates.md),
  [web-installer/README.md](web-installer/README.md).
- ROM platform: [docs/agent-service.md](docs/agent-service.md),
  [home/android/README.md](home/android/README.md), [PLAN.md](PLAN.md).
- Home: [home/README.md](home/README.md),
  [home/docs/makepad-fork.md](home/docs/makepad-fork.md),
  [home/docs/build-tool.md](home/docs/build-tool.md).
- History: [docs/home-migration.md](docs/home-migration.md) (how Home moved
  into this repository).

## Contributing

`main` is protected: work on a branch and open a pull request against it.
CI must pass. Keep signing keys, keystores and personal paths out of the
repository (`.gitignore` and `tests/test_no_local_paths.py` check for them).
When a change needs a dependency change, land it upstream first and move
the pin here; do not point a pin at a moving branch.

## License

Apache License 2.0 ([LICENSE](LICENSE), [NOTICE](NOTICE)). Third-party
licenses are in [LICENSES/](LICENSES).
