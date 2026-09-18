# OctoSense ROM

A LineageOS 22.2 build for the OnePlus 6 (`enchilada`) signed with OctoSense's own
keys, carrying the OctoSense launcher, System Bridge and Quickstep fork as
platform-signed privileged system apps.

- `vendor/octosense/` — the product layer: `octosense.mk` (packages), `Android.bp`
  (prebuilt imports re-signed with the platform certificate), the framework
  overlay (notification access for the bridge, Recents provider) and the
  privileged-permission allowlist. `prebuilt/` holds the launcher and bridge APKs
  taken from OctoSense-mobile's release build; any signature on them is replaced
  at build time.
- `scripts/make-keys.sh` — generates the platform, APK and APEX keys into
  `~/octosense-adr0001/keys` and installs them as `vendor/lineage-priv/keys`.
- `scripts/apply-to-tree.sh <tree>` — copies the layer into the tree and makes
  `lineage_enchilada.mk` inherit it.
- `scripts/build-rom.sh` and `scripts/run-rom-rootfs.sh` — the build inside the
  host's chroot: `preflight` lunches and dumps the configuration, `bacon` makes
  the flashable zip into `/exports/rom-build`.

The Quickstep fork and the SystemUI fork are staged into the tree from
OctoSense-mobile's `android/platform-build` (`stage-quickstep.py`,
`stage-systemui.py`) before a build.

Keys are the ROM's identity: keep `~/octosense-adr0001/keys` backed up and out
of any repository. The first flash from LineageOS wipes user data because the
signers differ.

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
