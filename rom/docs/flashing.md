# Flashing the OctoSense ROM on the OnePlus 6

For the browser workflow, use the [web flashing guide](../web-installer/README.md#flash-from-your-browser).
It covers the local developer preview, which requires a verified device and
erases data for a fresh install. Public web flashing and automatic recovery
are not available yet.

## Manual recovery and sideload workflow

The first flash from LineageOS wipes user data: the two builds are signed with
different keys and Android will not carry packages across them.

1. Build products in `exports/rom-build/` on the host: the `lineage-22.2-*-UNOFFICIAL-enchilada.zip`
   OTA package and, under `out/octosense-rom/target/product/enchilada/`, `boot.img`,
   `recovery.img`, `vbmeta.img` (the OnePlus 6 has A/B slots and no separate recovery
   partition; recovery lives in `boot.img`).
2. Boot to the bootloader (`adb reboot bootloader`), then flash our recovery-capable
   boot image so the sideload accepts our signature:
   `fastboot flash boot boot.img` and `fastboot flash vbmeta vbmeta.img`.
3. `fastboot reboot recovery`; in recovery choose Apply update → Apply from ADB, then
   `adb sideload lineage-22.2-*-UNOFFICIAL-enchilada.zip`.
4. Factory reset from recovery (Format data), reboot.
5. Run `scripts/checklist.sh` once booted and unlocked; then the manual items:
   emergency call screen, IME, notifications and replies, power and volume dialogs,
   Recents, Wi-Fi and Bluetooth switches from the shade.

Recovery back to LineageOS: sideload the LineageOS nightly with its own recovery,
then factory reset again.

## Build host

The build runs in an Ubuntu 24.04 chroot (`~/octosense-adr0001/rootfs`) on a
26.04 host. The scripts that run on the host use `~/octosense-adr0001` as their
build root by default; set `OCTOSENSE_BUILD_ROOT` to use another. The chroot
has no `gpgv`, so apt cannot verify the archive; packages are added by
downloading the 24.04 `.deb` files and `dpkg -i` inside the chroot.
Added so far: `libssl3t64` and `libssl-dev` 3.0.13-0ubuntu3.15 (the msm-4.9
kernel's `sign-file` and `extract-cert` host tools need the OpenSSL headers).

## Field notes from the first flash (18 Sep 2026)

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
