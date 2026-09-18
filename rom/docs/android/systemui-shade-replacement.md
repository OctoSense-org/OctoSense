# OctoSense panel across apps

The user requested replacement of Android's pull-down panel across the phone,
including other apps. The native OctoSense Recents package now supplies an
opt-in notification and controls window above the current app. SystemUI remains
installed and running for the lock screen, status/navigation bars, system dialogs
and its connection to Quickstep. Trebuchet remains uninstalled for user 0.

The later [native SystemUI build](octosense-systemui-build.md) adds an OctoSense
fork for the remaining system interface. That APK builds successfully but is
blocked from phone replacement by its incompatible release signer. The global
panel described here remains the deployed phone integration.

## Use on the OnePlus 6

With the phone unlocked, swipe down from the physical top-right edge for
**OctoSense Controls**, or the top-left edge for **OctoSense Notifications**.
Close or Back returns to the underlying app. The panel opens a window without
launching Home or replacing the resumed app's task. The top gesture region
includes Android's extra touch margin below the notch. Home's existing internal
shade remains available when a swipe starts below that system gesture region.

**System setup → System-wide OctoSense panel** opens the persistent switch,
**Use OctoSense panel across apps**. Switching it off immediately restores
Android's top-edge panel without changing the Recents provider. The lock screen
continues to use Android's panel. Native settings screens still handle network
credentials, pairing, and Android permission consent.

Both panels use the existing System Bridge state and operations: notifications,
original app actions and inline replies, dismissal, connectivity/settings,
brightness, media volume, flashlight and rotation. Direct network switches remain
subject to the existing bridge capabilities; otherwise the tiles open settings.
The user's approved everyday notification and write-settings access is retained.

## Native integration

`GlobalShadeController` is attached to the ROM-native `TouchInteractionService`
lifecycle through the audited staging transform. Its input monitor only claims
unlocked default-display gestures beginning in the system's top gesture region. Normal app
touches return before any system query. Gesture cancellation has a bounded
timeout. The controller holds its own Binder-scoped `DISABLE_EXPAND` token only
while claiming a gesture or showing the panel.

`GlobalShadePanel` uses `TYPE_STATUS_BAR_SUB_PANEL` in a display/window context,
with native input, insets, scrolling and a memory-only reply editor. Task changes,
configuration changes, screen-off, keyguard transitions and bridge disconnection
dismiss it. Disabling the feature disposes the input monitor. Process death
releases the window, input monitor and status-bar token through Android.

Quickstep and Home use separate authenticated bridge sessions. Caller identity
still requires the actual UID, same Android user, allowed package and matching
signer; the bridge now accepts the Quickstep package as another client. Commands
use bounded worker queues and are not automatically replayed after uncertain
outcomes. Notification updates expire old reply authority. Logs do not include
notification contents or reply drafts.

The existing Recents grants supply native input/window access. This change adds
no Android privileged permission to the module allowlist, no replacement
SystemUI APK, no SELinux policy, and no boot service script. The native APK adds
the application's signature-protected System Bridge permission.

## Artifacts and validation

The original deployment evidence is under
`target/android/adr-0001-artifacts/global-shade/`. The subsequent notch-boundary
regression and fix are recorded under `global-shade-v3/` beside it.

### Notch gesture regression

The user observed Android's notification and settings shade even with the global
panel enabled. On the OnePlus, the status bar is 80 pixels tall, but
`SystemGesturesPointerEventListener` accepts top swipes through pixel 114:
80 pixels of cutout height plus the ROM's 34-pixel cutout touch margin.
OctoSense previously stopped at pixel 80. The retained `boundary-before` check
reproduces stock shade entry at pixels 81 and 114, from both sides, above both
Home and System Bridge. Its input monitor and service were still active.

`GlobalShadeController.geometry()` now reads the same framework gesture-start
and cutout-margin dimensions, including the display's current top cutout bounds.
It recomputes them on configuration changes. The monitor covers Android's
actual region while leaving gestures below that region to the current app.
This fixes unlocked pull-down entry; it does not deploy the separately built
SystemUI fork or replace Android's Settings activities and lock screen.

`android/scripts/run-global-shade-boundary.py` checks the old boundary, the
previously uncovered strip, the exact system boundary, and an ordinary app swipe
one pixel below it. It retains only gesture coordinates and window identities.

The v3 native build passed in 291 seconds, verified all 1,145 pinned ROM
projects, and recorded no OOM events. The installed candidate passed all 17
boundary checks (both entry sides over Home and Bridge, ordinary app gestures,
and both landscape orientations), with rotation settings restored. Its Android
manifest is byte-for-byte identical to v2's decoded manifest and it adds no
permissions. The retained native SystemUI staging was independently verified
without changing or deploying that separate APK.

- Native v3 APK: `01eb0baab479dd7125e0ce29c780a9dd19dcbbe265a3411c6f5aa43e865232b5`.
- Installed v3 module: `9cf5e50f9020d3856e57e3d7dd1d8adb98cee97328aff1e18e3801205ee0df81`.
- Upgrade baseline: the mounted v2 APK, `bf65e737c77287bdf701e6f7d5571fa1f1d84dc3b949ac26bd0bfd48eea366a0`.

The v3 module is deployed and verified after reboot
(`ea8a2459-6d2d-4fdd-9997-93036814f1e4`). All 17 boundary checks pass again
without visiting the enable switch first. The five panel smoke checks and eight
lifecycle checks also pass, including disabling/restoring Android's panel,
Recents, process recovery, and keeping the lock-screen boundary. Volume and
rotation are restored. `module-deployment.json` and `final-device-state.json`
verify the exact mounted APK, approved grants, unchanged Home/Bridge/SystemUI,
and removed Trebuchet registration. The phone is left showing OctoSense Controls.

The following table and checks describe the original v2 deployment; the v3
build and phone evidence are retained separately.

| Artifact | SHA-256 |
| --- | --- |
| Normal Home, unchanged | `c23cba361b3d4e6bfeb5b8228db7d8189d4904df586aa825bb721888248cfb7d` |
| Normal System Bridge | `8efffa0fd2bdabcf424ad16687a074ee755d7def9b8f3c404e7fa0c79e45c6fd` |
| Native Quickstep with global panel | `bf65e737c77287bdf701e6f7d5571fa1f1d84dc3b949ac26bd0bfd48eea366a0` |
| Installed module, `module-final/octosense_quickstep_enchilada.zip` | `cce909f9a8304579b88e87fae13bf319cf41686d951f7244b5245b120b92e473` |

The normal/validation bridge builds and lint pass; the bridge queue regression
passes 811 assertions. The native build verifies all 1,145 pinned projects,
compiles in 292 seconds with no OOM, and passes APK inspection/signature checks.
`source-manifest.json` identifies native inputs; the later installer-only changes
are separately fingerprinted in `module-final/packaging-source.json`.

Passing phone evidence includes:

- `smoke-v1/results.json`: both physical edge entries above the same resumed
  Bridge task, Back/Close, immediate Android-panel fallback on disabling, and
  re-enabling.
- `fixture-v4/results.json`: real global-panel reply editing and visible IME,
  delivery exactly once to the local fixture receiver, stale-draft expiration
  after update, ongoing-card protection, dismissal and return to the app.
  The normal bridge APK is restored and its temporary notification-posting grant
  revoked. Everyday notification access and write-settings access remain enabled.
- `lifecycle-v5/results.json`: volume changes and restoration, task-switch
  dismissal, Recents, rotation and landscape entry, process-death window removal,
  service/panel recovery, and Android's panel on this phone's insecure lock screen.
  Original volume and rotation settings are restored.
- `module-deployment.json`: exact runtime module payload activated after reboot,
  unchanged SystemUI APK and Home, all 14 native grants plus the application
  bridge grant, connected SystemUI/Quickstep, approved everyday access, default
  Home and three-button navigation preserved, and Trebuchet still uninstalled
  for user 0. Boot ID: `762bc988-b948-410f-9f9b-a5b2bf97e477`.
- `post-reboot-ui.json`: panel entry works before touching the enable switch,
  above the same resumed app. Internet, Wi-Fi, Bluetooth and setup routes reach
  their actual destinations. The panel opens above Android Settings and returns
  to its same task; notifications, Close, Recents and Home also pass after boot.
- `smoke-post-reboot/results.json`: the final deployed build passes both panel
  entries, same-task continuity, Back/Close, immediate fallback to Android's
  panel when disabled, and restoration of the OctoSense panel when re-enabled.
- `final-device-state.json`: final APKs, enabled preference, default Home,
  removed Trebuchet registration and everyday permissions verified again.
  Notification capability is available after reboot, temporary fixture posting
  permission and instrumentation are absent, volume/rotation are restored, and
  no owned temporary files, ADB forwards or OctoSense crash entries remain.
  The phone is left showing OctoSense Controls above Home. The original raw
  listener setting entry normalized away by Android on APK replacement is
  merged back without removing current entries.

Earlier USB disconnects and incomplete runs are retained as failures. Harness
corrections preserve uncompressed layout parents for ongoing cards, use the
existing ADB root authorization for exact volume restoration (shell-UID writes
were silently ignored), and wait for wake/lock transitions. Magisk consumes and
removes `customize.sh` after validating it; post-install hash checks cover the
remaining runtime payload. No native code change was needed for these harness
corrections. Fixture evidence retains only owned notification content.

This is a ROM-specific prototype, not full SystemUI feature or launcher parity.
Three-button navigation remains selected. Combined pixel capture, performance,
secure-keyguard user interaction and broader third-party app coverage are not
established by these checks. The public development signing certificate is not
suitable for production authentication.

## Deployment and recovery

The module installer checks the exact OnePlus 6 ROM, framework resource checksum,
active module identity, and the explicitly packaged old mounted APK hash before
allowing a native-module upgrade. The v3 archive accepts the v2 baseline,
`bf65e737c77287bdf701e6f7d5571fa1f1d84dc3b949ac26bd0bfd48eea366a0`.
Unmatched native configurations are rejected. The v2 payload and APK are retained
under `global-shade/`; earlier payloads remain under its `before/` directory and
the native-transition artifacts.

`global-shade-v3/rollback-module/` contains a prepared, uninstalled v2 module
whose upgrade guard accepts the currently mounted v3 APK. A v3 rollback would
install the retained v2 APK as an application update, install that specifically
prepared rollback module, and reboot. This retains OctoSense Recents, Home, the
panel preference, and the approved Bridge access. It also restores v2's known
notch-boundary bug. The original v2 deployment ZIP has an older upgrade guard
and should not be used directly to revert v3. This particular rollback archive
has been packaged and inspected, but its installation has not been exercised.

For panel-only recovery, switch off **Use OctoSense panel across apps**. Keep the
native module active so Android Recents continues working. For a complete module
rollback, **restore Trebuchet's registration for user 0 first**, then disable only
`octosense_quickstep_enchilada` and reboot, following the
[Trebuchet migration recovery steps](trebuchet-removal-record.md#recovery).
Keep the current Home and the approved bridge access. Boot-failure recovery
requiring hands-on access has not been exercised.
