# OctoSense ROM

English | [简体中文](README.zh-CN.md)

**New to the code?** Read the [desktop, Home, ROM and system-app walkthrough](../desktop/docs/code-walkthrough.md), then the [agent and Tokio walkthrough](../docs/architecture-walkthrough.md). The first follows launch, native hosting, script bundles, app data and Android platform boundaries.

The OctoSense ROM is LineageOS 22.2 (Android 15) for the OnePlus 6
(`enchilada`) with the OctoSense Home app preinstalled beside a privileged
agent, Quickstep and SystemUI, so the agent reaches the system layer. This
directory holds the image only: the product layer, the patches, the scripts
that build, sign, flash and update the image, and the web installer. It was
the OctoSense-ROM repository (retired; merged into this repository; its
`home/` is now [`../phone/`](../phone/README.md));
see [ADR 0001](../docs/adr/0001-one-octosense-repository.md).

The Home app itself, which also installs as an ordinary Home app on any
Android phone, is built from [`../phone/`](../phone/README.md). The image
consumes that APK, signed with the platform key, and adds the privileged
system side.

> **Building an OctoSense app?** You do not need the ROM. Start at the
> [OctoSense-org profile](https://github.com/OctoSense-org)'s reading list:
> the `AGENTS.md` of OctoSense App Flow (formerly Design Flow), then its
> `docs/QUICKSTART.md`.
> Installing your own bundle on a phone is not supported yet; to see it in a
> shell before publication, use the desktop shell with a local catalog
> ([desktop README](../desktop/README.md#try-your-own-app-before-it-is-published)).

## Layout

| Path | Contents |
| --- | --- |
| `vendor/octosense/` | ROM product layer: makefiles, permissions, overlays, sepolicy, Settings backends, the privileged agent (and its OTA updater) |
| `patches/` | LineageOS and kernel patches. The reviewed Makepad runtime patch is at the repository root, [`../tools/runtime-patches/`](../tools/runtime-patches) |
| `scripts/` | Home APK builds (`build-home.sh`, `build-home.py`, `build-home-ohos.py`), ROM staging, build, flash, release and phone checks |
| `web-installer/` | WebUSB installer for the OnePlus 6 (local developer preview) |
| `docs/` | ROM decisions ([docs/adr/](docs/adr/README.md)), build, flashing, update and validation records |
| `tests/` | Python tests for the build, staging, manifest and installer scripts |
| `PLAN.md` | The ROM plan |
| `out/` | Ignored. Build outputs and receipts (`out/home/<variant>/`) |

The phone side of the system bridge (AIDL contracts, the System Bridge APK,
Quickstep and SystemUI projects, platform build stagers) lives with Home in
[`../phone/android/`](../phone/android/README.md).

## Prerequisites

- Everything the Home APK needs ([phone/README.md](../phone/README.md#build-and-run)):
  the prepared framework sources (`python3 tools/setup.py` at the repository
  root), the pinned `cargo-makepad`, an Android SDK/NDK, a full JDK 17+ and
  Gradle 8.11.1.
- A LineageOS 22.2 tree for `enchilada`, the OnePlus vendor blobs, the kernel
  source and the ROM signing keys, all outside this repository.
- A Linux build host for the OS build (an Ubuntu 24.04 chroot).

## Build and flash the image

From the repository root, build the ROM variant of the Home APK pair, signed
with the platform key, then stage it and the forks into the LineageOS tree:

```sh
rom/scripts/build-home.sh --variant rom --sdk ... --android-sdk ... \
  --gradle-home ... --java-home ... --packager ... \
  --sign-key /private/rom-keys/platform.pk8 \
  --sign-cert /private/rom-keys/platform.x509.pem
python3 rom/scripts/stage-home.py                 # verify the receipt, copy the APKs to vendor/octosense/prebuilt/
rom/scripts/stage-forks.sh /path/to/lineage-tree  # apply vendor/octosense and stage the Quickstep and SystemUI forks
```

**App Studio development builds.** Home’s build wrapper accepts `--dev-mode` independently of the `--development` signing option. Standalone tests can use `--package-name dev.makepad.octosense.studio`; only Home is renamed, so keep the installed Bridge. Developer-enabled receipts cannot be staged or published as ROM releases. See the [build contract](docs/home-build.md#android-builds) and [in-progress milestone](../docs/adr/0006-app-studio-on-the-phone.md). Build-plan tests do not establish device operation.

Then, on the host, `scripts/run-rom-rootfs.sh` enters the chroot (under
`OCTOSENSE_BUILD_ROOT`, default `/home/ubuntu/octosense-adr0001`) and runs
`build-rom.sh preflight`, `bacon` (the full signed build) or `module <name>`.
It expects `build-rom.sh` at `exports/build-rom.sh` under the build root and
is started by systemd; that unit and the host set-up are not in this
repository. `scripts/make-keys.sh` generates the signing keys once;
`scripts/release.sh <build-tag>` fetches a finished build from the host named
in `~/.config/octosense/build.env`. Signing, receipts and the ROM variant:
[docs/home-build.md](docs/home-build.md).

To flash:

- **Image:** build one as above, or download the last published build,
  [`rom-v20260919-j`](https://github.com/OctoSense-org/OctoSense/releases/tag/rom-v20260919-j).

- **Browser:** the [web installer](web-installer/README.md#flash-from-your-browser),
  a local developer preview. A fresh install erases the phone. Public web
  flashing is off until [ROM ADR 0001](docs/adr/0001-public-web-installer.md) is
  done.
- **Command line:** `scripts/flash.sh <build dir> [serial]`, or the recovery
  sideload in [docs/flashing.md](docs/flashing.md), which also holds the
  lessons from the first flash.
- **Afterwards:** `scripts/verify-phone.sh <build-tag> [serial]` waits for
  boot and runs `scripts/checklist.sh` and `scripts/agent-test.sh`.

Updates reach a flashed phone over the air from this repository's GitHub
Releases: each build is a `rom-v<build-tag>` release, and the phone reads
`update.json` from the moving `rom-latest` release
([docs/updates.md](docs/updates.md)); `scripts/ota-push.sh` pushes one from a
Mac. Images `20260919-j` and earlier read the releases of the OctoSense-ROM
repository (retired; merged into this repository), which no longer exists, so a
phone flashed with one must be reflashed once to receive updates.

## What the image adds to Home

| Piece | Where |
| --- | --- |
| Home and the System Bridge, as platform-signed privileged apps | built from [`../phone/`](../phone/README.md), staged into `vendor/octosense/prebuilt/` |
| The privileged agent service | `vendor/octosense/agent/` ([docs/agent-service.md](docs/agent-service.md)) |
| Quickstep and SystemUI forks, PermissionController hooks | staged by `scripts/stage-forks.sh` from `../phone/android/platform-build/` |
| Privileged permissions, overlays, sepolicy, Settings backends | `vendor/octosense/` |
| Shared phone themes | [ROM ADR 0002](docs/adr/0002-rom-themes.md) |

The apps, the octos kernel service and App Hub are Home's, identical in the
standalone APK and in the image ([phone/README.md](../phone/README.md)).

## Testing and validation

CI (`.github/workflows/rom.yml` at the repository root) runs for `rom/` and
the phone's Android sources, from this directory:

```sh
python3 -m unittest discover -s tests -v
python3 scripts/generate-agent-aidl.py --check --sdk "$ANDROID_HOME"
bash -n scripts/stage-forks.sh scripts/publish-release.sh scripts/apply-to-tree.sh scripts/build-rom.sh
(cd web-installer && npm ci --ignore-scripts && npm test && npm run test:browser)
```

The image build itself is not in CI. Source and build checks do not prove a
ROM boots, or that radios, notifications, Recents, emergency calls and OTA
recovery work: run the device checks (`scripts/checklist.sh`,
`scripts/agent-test.sh`, `scripts/verify-phone.sh`) before changing
certificates or releasing. Records:
[docs/home-device-validation.md](docs/home-device-validation.md),
[phone/docs/validation.md](../phone/docs/validation.md),
[phone/docs/android/](../phone/docs/android/README.md).

## Documentation

- ROM decisions: [docs/adr/](docs/adr/README.md). Home decisions (including
  Home ADR 0004, system apps as contained script apps) and the repository's
  own: [../docs/adr/](../docs/adr/README.md).
- Build and delivery: [docs/home-build.md](docs/home-build.md),
  [docs/flashing.md](docs/flashing.md), [docs/updates.md](docs/updates.md),
  [web-installer/README.md](web-installer/README.md).
- ROM platform: [docs/agent-service.md](docs/agent-service.md),
  [phone/android/README.md](../phone/android/README.md), [PLAN.md](PLAN.md).
- History: [docs/home-migration.md](docs/home-migration.md) (how Home moved
  into the ROM repository, before both joined this one).

## Contributing

Work on a branch and open a pull request against `main`; see the repository's
[AGENTS.md](../AGENTS.md). Keep signing keys, keystores and personal paths out
of the repository (`.gitignore` and `tests/test_no_local_paths.py` check for
them). Never flash a phone you were not given for the task.

## License

Apache License 2.0 ([LICENSE](LICENSE), [NOTICE](NOTICE)). Third-party
licenses are in [LICENSES/](LICENSES).
