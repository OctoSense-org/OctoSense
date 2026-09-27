#!/bin/bash
# Exercises the agent platform on the bench phone through its dumpsys harness.
# Usage: agent-test.sh [serial] — prints each capability's result.
set -u
ADB=${ADB:-adb}; D=${1:-cfb7c9e3}
svc="dev.makepad.octosense.agent/.AgentPlatformService"
run() { echo "== $*"; "$ADB" -s "$D" shell dumpsys activity service "$svc" "$@" | tr -d '\r' | grep -v '^$' | head -20; }
run
run tasks 5
echo "== screen 540"; "$ADB" -s "$D" shell dumpsys activity service "$svc" screen 540 | tr -d '\r' | sed -n 's/^ *png_base64=//p' | base64 -d > "${OUT:-.}/agent-screen.png" && echo "screen capture saved to ${OUT:-.}/agent-screen.png ($(wc -c < "${OUT:-.}/agent-screen.png") bytes)"
run get secure theme_customization_overlay_packages
run qs; sleep 1; run collapse
run tap 540 1800
run audit
