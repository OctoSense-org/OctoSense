# Unified Home device validation

Validation date: 2026-09-21 (local), 2026-09-22 UTC. Source is the
`feat/home-in-rom` worktree, not the old mobile checkout. Raw receipts, original
Android APK backups, fixture logs and app-owned captures are kept locally under
ignored `out/device-validation/`; signing material and personal notification
contents are not part of this report.

## OnePlus 6: existing OctoSense ROM

The OnePlus runs Android 15 / LineageOS 22.2 with privileged OctoSense services.
The new Home and Bridge install as updates under the existing platform signer,
retaining application data and the OctoSense default Home role. This is not an
unrooted Android test or a newly built ROM boot test.

Measured integration results:

| Check | Result and scope |
| --- | --- |
| Device controls | 61 checks pass, including volume commands, deduplication, stale results and restoration. |
| Widgets | 16 checks pass for provider discovery, widget ID bookkeeping and recovery; real widget binding/rendering is not claimed. |
| Placements | 28 checks pass for persistence and schema handling. |
| Notification reply input | 15 synthetic RemoteInput/PendingIntent checks pass; no messages sent to other people. |
| Reply interface | Fixture UI checks pass for Unicode, multiline/empty drafts, duplicates, cancellation, timeout, disconnect and limits. This is not notification-listener lifecycle coverage. |
| Home geometry | 42 checks pass for profile bounds, defensive copies and invalidation. |
| Private Quickstep connection | Fails: the installed Quickstep uses the AOSP test key while original and new Home/Bridge use the ROM platform key. |

The Quickstep mismatch predates this migration. The platform build fragment now
selects the platform certificate. The phone's installed Quickstep was not
re-signed, uninstalled or replaced; resolving that mismatch requires a new ROM
build and its acceptance checks. Caller signature enforcement stays enabled.

## Signed App Hub fixture

The current public catalog is empty. Validation used historical, already signed
catalogs from App Hub commits `db45768` and `1522cec`, with the normal embedded
trust anchor. The Camera card is a static UI fixture with no requested device
capabilities; it does not take photographs.

- Installation through App Hub succeeds, including version 1.0.2 and its assets
  fetched over real GitHub HTTPS after Wi-Fi was connected.
- A changed catalog correctly refuses to run installed 1.0.1 when 1.0.2 is
  offered. The current store still presents Open instead of Update. Removal and
  installation work, but in-place updating is not a passing acceptance result.
- The card launches as its own Home client with its storage, network, memory
  and instruction policy applied. App-owned pixels confirm rendering.
- With the fixture catalog origin made unavailable and Home restarted, the
  installed launcher entry opens against the cached verified catalog. This
  simulates an unavailable origin; the phone's Wi-Fi was left enabled.
- The test exposed and fixed two nested Splash reload defects: packaged mobile
  font resolution after resource removal, and lost registered Design/Kit
  widgets after rebuilding `mod.widgets`. Script errors are now captured for
  diagnosis. The confined resource namespace is restored before card evaluation.

Visual acceptance remains incomplete: the historical card uses a fixed narrow
layout and shows missing Chinese glyphs, and the store has dark-mode contrast
issues. Interrupted installation recovery and atomic bundle replacement remain
release gates. Fixture removal and restoration of the public catalog are part
of the device cleanup.

## Mate 70 Air: normal OpenHarmony application

The Mate runs OpenHarmony 6.1.1.120. A signed HAP built from `home/` updates the
existing Home prototype under `com.example.myapplication`, the identity its
existing development profile authorizes. No new keys or profiles were created.

Home launches, its pages respond, and all ten bundled modules are linked in
process. App Hub opens and accepts the live public HTTPS catalog, showing its
empty state. App-owned GPU captures confirm both Home and App Hub. OpenHarmony's
missing framebuffer capture response was added to make those checks possible.
This is normal-app coverage; it does not establish Android Home-role behavior,
Huawei system-launcher replacement, or App Hub fixture installation on the Mate.

## Local checks and remaining gates

Home/Hub/policy tests: 337 pass. Product build/source-preservation tests: 14 pass.
Runtime policy/network-gate tests: 11 pass. Existing font-policy tests: five pass;
the host-vocabulary registration test passes. The recorded framework patch
reconstructs its locked tree from a clean base, and the Cargo graph check passes.

An additional upstream `mobile_typefaces_keep_symbol_fallbacks_across_appearances`
test fails: it expects `jetbrains_ui_symbols`, while the native default selects
the International policy with IBM Plex, LXGW and emoji. Its theme/font-policy
files are unchanged by this migration. This report does not claim every
framework test passes.

Before retiring the old mobile repository, reconcile the remaining Calendar
work, test a genuinely unrooted Android phone, build and validate a new OnePlus
ROM with matching Quickstep signing, and complete App Hub update/recovery and
visual acceptance. No full ROM image was built or flashed during these checks.
