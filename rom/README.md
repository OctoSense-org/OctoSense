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
