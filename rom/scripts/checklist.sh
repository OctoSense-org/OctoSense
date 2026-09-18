#!/bin/bash
# The device checklist for a freshly booted OctoSense ROM. Usage: checklist.sh [serial]
# Prints one line per check; exits non-zero when any check fails.
set -u
ADB=${ADB:-adb}; D=${1:-cfb7c9e3}
sh() { "$ADB" -s "$D" shell "$@" 2>/dev/null | tr -d '\r'; }
ok=0; fail=0
check() { local name=$1 result=$2 want=$3; if [ "$result" = "$want" ]; then echo "PASS  $name ($result)"; ok=$((ok+1)); else echo "FAIL  $name (got '$result', want '$want')"; fail=$((fail+1)); fi; }
check "boot completed" "$(sh getprop sys.boot_completed)" 1
check "OctoSense ROM property" "$(sh getprop ro.octosense.rom)" 1
check "build signed with release keys" "$(sh getprop ro.build.tags)" release-keys
check "home role is OctoSense" "$(sh cmd package resolve-activity --brief -c android.intent.category.HOME -a android.intent.action.MAIN | tail -1)" dev.makepad.octosense/.MakepadApp
check "launcher is a privileged system app" "$(sh pm path dev.makepad.octosense | grep -c system_ext/priv-app)" 1
check "bridge is a privileged system app" "$(sh pm path dev.makepad.octosense.bridge | grep -c system_ext/priv-app)" 1
check "bridge has notification access" "$(sh settings get secure enabled_notification_listeners | grep -c dev.makepad.octosense.bridge)" 1
check "framework overlay enabled" "$(sh 'cmd overlay list' | grep -c '^\[x\] dev.makepad.octosense.overlay.framework')" 1
check "Trebuchet absent" "$(sh pm list packages | grep -c '^package:com.android.launcher3$')" 0
check "agent service installed" "$(sh pm path dev.makepad.octosense.agent | grep -c system_ext/priv-app)" 1
check "purple palette provisioned" "$(sh settings get secure theme_customization_overlay_packages | grep -c 6750A4)" 1
check "Magisk absent" "$(sh pm list packages | grep -c com.topjohnwu.magisk)" 0
check "keyguard present" "$(sh dumpsys window | grep -c 'mDreamingLockscreen\|KeyguardController')" 1
echo "passed $ok, failed $fail"; [ "$fail" -eq 0 ]
