#!/system/bin/sh
# Sourced by the existing Magisk 29 installer after default extraction.
[ "$BOOTMODE" = true ] || abort "Install from the booted Android system. Recovery installation is unsupported."
[ "$MAGISK_VER_CODE" -ge 29000 ] || abort "Magisk 29 or later is required."
[ "$API" = 35 ] && [ "$ARCH" = arm64 ] || abort "This experiment requires Android 15 ARM64."
[ "$(getprop ro.lineage.device)" = enchilada ] || abort "This module is only for the reviewed OnePlus 6."
[ "$(getprop ro.lineage.version)" = 22.2-20260708-NIGHTLY-enchilada ] || abort "LineageOS build differs."
[ "$(getprop ro.build.version.incremental)" = 21be58cea4 ] || abort "ROM incremental revision differs."
[ "$(sha256sum /system/framework/framework-res.apk | cut -d ' ' -f 1)" = f23f0e7d35876801cb4eb4bf318496db10561c73ca51af408ce066c74ea34b03 ] || abort "Framework resource checksum differs."
[ ! -e /system_ext/overlay/config/config.xml ] || abort "Overlay configuration requires a new review."
current_recents="$(cmd overlay lookup android android:string/config_recentsComponentName)"
case "$current_recents" in
  com.android.launcher3/com.android.quickstep.RecentsActivity) ;;
  dev.makepad.octosense.quickstep/com.android.quickstep.RecentsActivity)
    octosense_previous=/data/adb/modules/octosense_quickstep_enchilada
    [ -n "${OCTOSENSE_UPGRADE_FROM_SHA:-}" ] || abort "This archive has no reviewed native upgrade baseline."
    [ -f "$octosense_previous/module.prop" ] && [ ! -e "$octosense_previous/disable" ] || abort "The expected active module is missing."
    grep -q '^id=octosense_quickstep_enchilada$' "$octosense_previous/module.prop" || abort "Existing module identity differs."
    [ "$(sha256sum /system_ext/priv-app/OctoSenseQuickstep/OctoSenseQuickstep.apk | cut -d ' ' -f 1)" = "$OCTOSENSE_UPGRADE_FROM_SHA" ] || abort "Mounted native APK differs from the reviewed upgrade baseline."
    ;;
  *) abort "Unexpected Recents provider; preserve the existing configuration." ;;
esac
case "$(pm path dev.makepad.octosense)" in package:*) ;; *) abort "OctoSense Home must already be installed." ;; esac
case "$(pm path dev.makepad.octosense.quickstep)" in package:*) ;; *) abort "The Quickstep package must already be installed." ;; esac
(cd "$MODPATH" && sha256sum -c payload.sha256) || abort "Module payload checksum failed."
set_perm_recursive "$MODPATH" 0 0 0755 0644
ui_print "Prepared native OctoSense for the reviewed OnePlus 6 ROM. Reboot to activate."
ui_print "Panel rollback: turn off the system-wide panel in OctoSense system setup; keep this module for Recents."
ui_print "Full module rollback: restore Trebuchet for user 0 before disabling this module and rebooting."
