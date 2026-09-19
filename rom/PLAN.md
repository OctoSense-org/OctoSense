# OctoSense ROM plan

Goal: the OnePlus 6 as a test device for agent-driven UX, with deep system API
access and full look-and-feel control. Phases run in parallel where they do not
touch the same build tree.

## Phase 0 — prerequisites (done 18 Sep 2026)

- The build host (address in the uncommitted `~/.config/octosense/build.env`): LineageOS 22.2 tree pinned to the phone's
  installed nightly, OnePlus vendor blobs (TheMuppets, LFS pulled), kernel source.
- OctoSense platform, APK and APEX keys generated (`scripts/make-keys.sh`).
- This repository; `scripts/build-rom.sh` runs inside the host's chroot.

## Phase 1 — same UI, our keys

1. Baseline: plain LineageOS for `enchilada` signed with the OctoSense keys.
2. Apply `vendor/octosense`: launcher, bridge and Quickstep fork as platform-signed
   privileged apps; the framework overlay grants the bridge notification access
   and points Recents at the fork.
3. Stage the SystemUI fork (`stage-systemui.py`) so the shade is OctoSense's.
4. Flash, run the device checklist (boot, lock screen, emergency call, IME,
   notifications and replies, power and volume dialogs, Recents, direct radio
   switches without root), produce the first OTA.

Exit: today's UI with no Magisk anywhere and no LineageOS shade reachable.

## Phase 2 — agent platform

- An OctoSense system service (persistent privileged process, one Binder
  surface) exposed through the bridge contracts as capabilities: task list with
  thumbnails, screen capture, input injection, secure settings, notification
  control, app lifecycle. Allowlisted by signature.
- The launcher and cards consume it like the bridge today; fallbacks stay so
  the Home-app build works on other phones.
- Phone-in-the-loop harness on the host: build, flash, checklist, captures.

Exit: an agent card reads the screen, acts, and observes the result without a prompt.

## Phase 3 — look and feel (in parallel from day one)

- The system-wide panel in the launcher's purple language (OctoSense-mobile #20).
- SystemUI shade, lock screen as an agent glance, dialogs, theme overlays,
  fonts, boot animation.

## Maintenance

Monthly: merge the LineageOS tag, backport the bulletin's kernel CVEs to
msm-4.9, rebuild, sign, OTA. Closed firmware (modem, DSP, TrustZone) stays at
OnePlus's last release; a second device with a living vendor should follow.
