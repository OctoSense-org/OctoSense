# Flashing the OctoSense ROM on the OnePlus 6

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
