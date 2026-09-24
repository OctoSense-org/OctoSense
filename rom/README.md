# OctoSense ROM

[English](README.md) | [简体中文](README.zh-CN.md)

One Android product repository with two delivery modes:

- **OctoSense Home** is the installable launcher for ordinary Android phones,
  with bundled apps, App Hub, notifications and device controls through public
  APIs and user-granted access.
- **OctoSense ROM** preinstalls the same Home implementation with the privileged
  agent, Quickstep and SystemUI integration. The current device target is the
  OnePlus 6 (`enchilada`), based on LineageOS 22.2 / Android 15.

Home lives in `home/`. There is no separate launcher checkout in the build.
Ordinary Home and ROM Home retain their existing signing arrangements and
application ID `dev.makepad.octosense`; their signed APKs are separate artifacts.
Installing Home alone does not replace an ordinary phone's global SystemUI.

```text
home/                       Home Rust workspace, bundled apps and Android bridge
home/android/platform-build/ Quickstep and SystemUI sources/stagers
vendor/octosense/            ROM product, permissions, overlays and platform agent
scripts/                    Home and ROM build, staging and update entry points
patches/runtime/            Recorded runtime changes required by App Hub
web-installer/              OnePlus ROM installer
```

Start with `python3 scripts/setup-home.py` to prepare exact dependency revisions
under ignored `.sources/`. See [Home builds and ROM staging](docs/home-build.md)
for standalone and ROM commands, signing, SDK requirements and validation.
AOSP/LineageOS, kernel and vendor sources remain in the external OS build tree.
App Hub's catalog and publishing pipeline remain in
[OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub).

The [migration record](docs/home-migration.md) identifies imported source history,
outstanding branches and the checks required before retiring the old repository.
[ROM updates](docs/updates.md) continue to use this repository's existing release
feed. Keys stay outside the repository; source consolidation does not change them.

## Flash the ROM from a browser

Follow the [web flashing guide](web-installer/README.md#flash-from-your-browser)
to prepare the ROM images, start the installer on your computer, connect a
OnePlus 6 over USB, verify the release, install it and check the first boot.
The guide includes unlock instructions and troubleshooting.

The installer is currently a **local developer preview** for OnePlus 6
(`enchilada`). Fresh installation erases phone data. Public web flashing is
disabled, and the exact phone-identification method still needs hardware
qualification; an unverified phone remains read-only. See
[ADR 0001](docs/adr/0001-public-web-installer.md) for the public-release plan.

## Host chroot notes

The build runs in an Ubuntu 24.04 chroot (`~/octosense-adr0001/rootfs`) on a
26.04 host. The chroot has no `gpgv`, so apt cannot verify the archive; packages
are added by downloading the 24.04 `.deb` files and `dpkg -i` inside the chroot.
Added so far: `libssl3t64` and `libssl-dev` 3.0.13-0ubuntu3.15 (the msm-4.9
kernel's `sign-file` and `extract-cert` host tools need the OpenSSL headers).

## Flashing lessons (18 Sep 2026, first OctoSense ROM on the OnePlus 6)

- `adb sideload` and long fastboot transfers died on this Mac's USB link; small
  transfers (boot, dtbo, vbmeta) always worked. What landed the ROM:
  `scripts/split-sparse.py` cuts `system.img` into independent 50 MB sparse
  parts, each flashed with its own `fastboot flash system_b part` under a
  `perl -e 'alarm ...'` timeout and retried alone. Vendor (655 MB) went through
  with `fastboot -S 64M`.
- A hung `fastboot` process holds the USB handle: kill it and the device shows
  up again. If the bootloader itself stops answering, select "Restart
  bootloader" on the phone.
- A half-flashed slot boots into Qualcomm CrashDump; Power+VolDown (10 s), then
  VolUp+VolDown+Power reaches the bootloader again.
- `fastboot -w` only erases userdata here (type raw); Android formats it on
  first boot. USB debugging is off after the wipe: enable Developer options
  and USB debugging once; the pre-authorised key (`PRODUCT_ADB_KEYS`) then
  needs no prompt.
- The agent is a persistent app: Android refuses `adb install` updates for it
  (INSTALL_FAILED_INVALID_APK), so agent changes ship only in a ROM build.
- Updating a system app on the phone: sign the APK with the platform key
  (`apksigner` with the bundled JDK at
  `~/.local/share/octosense/android-tools/makepad-android/openjdk`) and
  `adb install -r`. A module alone builds with
  `run-rom-rootfs.sh module <Name>` (log in exports/rom-build/module.log).
