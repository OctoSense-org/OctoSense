#!/bin/bash
# Exercises the agent platform on the bench phone through its dumpsys harness.
# Usage: agent-test.sh [serial] — prints each capability's result.
set -u
ADB=${ADB:-adb}; D=${1:-cfb7c9e3}
svc="dev.makepad.octosense.agent/.AgentPlatformService"
run() { echo "== $*"; "$ADB" -s "$D" shell dumpsys activity service "$svc" "$@" | tr -d '\r' | grep -v '^$' | head -20; }
run
run tasks 5
run screen /data/local/tmp/agent-screen.png 540
run get secure theme_customization_overlay_packages
run qs; sleep 1; run collapse
run tap 540 1800
run audit
"$ADB" -s "$D" pull /data/local/tmp/agent-screen.png "${OUT:-.}/agent-screen.png" >/dev/null 2>&1 && echo "screen capture saved to ${OUT:-.}/agent-screen.png"
