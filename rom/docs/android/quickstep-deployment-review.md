# OnePlus 6 native Quickstep deployment review

17 September 2026. **Historical experiment review: user approved; boot/gesture
experiment and normal rollback completed.** The subsequent user-approved
[Trebuchet removal](trebuchet-removal-record.md) reactivated this native module
and retained the newer Home and System Bridge builds. The module is now active;
the Home candidate below describes the initial experiment, not the current Home
APK. This ADR 0001 M3 evidence does not establish launcher parity or complete a
milestone.

The later [global panel integration](systemui-shade-replacement.md) records the
updated bridge/native APKs and scoped module-upgrade guard. The hashes and
experiment-only rollback plan below describe the initial experiment.

## Exact artifacts

The native host `ubuntu@54.198.120.231` built the ROM-matched upstream baseline
and the separate OctoSense adapter. Source checks cover all 1,145 pinned projects
and the exact generated adapter files. Native compilation, DEX optimization and
APK signing completed. The adapter build took 296 seconds using its populated
build cache, with zero OOM events. Upstream compiler warnings remain in the logs.
The generic ARM64 build produces APKs only, not a OnePlus system image.

Artifacts are under
`target/android/adr-0001-artifacts/native-transition/native-package/`:

| Artifact | SHA-256 |
|---|---|
| `octosense-quickstep-native.apk` | `fba417a89253cec42274d07077a757d3a565349e9a8f11b19de6c2e9374e0ee0` |
| `home-native-candidate.apk` | `404948af9728852804c843c036661454ac8126dcbbf6c177c550725f82370c10` |
| `module/octosense_quickstep_enchilada.zip` | `427ac446fbfbdb7d094481428bed62d02e9ca44707d345b3321876a1716ff329` |

Both APKs and the resource overlay use the existing public development certificate
`e96d983997281700b9d0ecc674ec11edec96d433c0fa9ddd5c216479f2b738c9`.
This certificate is unsuitable for production authentication. The signing key
stayed on the Mac. Native APK inspection passes for package identity, signature,
protected services, provider authorities, enabled controller metadata and the
separate Home boundary. Neither APK registers validation instrumentation.
The Home build retains existing Java/Rust and missing-launcher-icon warnings.

The archive's checksums, APK alignment and installer shell syntax pass. The phone's
own `idmap2` successfully maps the overlay's single resource to its installed
framework under the supplied `system` policy. That check wrote temporary files
only, removed them afterward and did not install or activate an overlay. Actual
boot-time classification, grants and SystemUI binding now have the phone evidence
recorded below.

## Proposed changes

Target: OnePlus 6 `cfb7c9e3`, Android 15/API 35,
`22.2-20260708-NIGHTLY-enchilada`, incremental `21be58cea4`.

1. Update the existing Home and Quickstep packages with the exact APKs above,
   preserving their data. Home includes the tested layout and transition clients.
2. Install the module using the existing Magisk 29 installation. It adds three
   system files through Magisk's mounts:
   - `/system_ext/priv-app/OctoSenseQuickstep/OctoSenseQuickstep.apk`
   - `/system_ext/overlay/OctoSenseRecentsOverlay/OctoSenseRecentsOverlay.apk`
   - `/system_ext/etc/permissions/privapp-permissions-octosense-quickstep.xml`
3. Reboot to activate the mounted package, static overlay and permission policy.
   The overlay changes `android:string/config_recentsComponentName` from
   `com.android.launcher3/com.android.quickstep.RecentsActivity` to
   `dev.makepad.octosense.quickstep/com.android.quickstep.RecentsActivity`.
4. Check three-button Recents first. After required grants and the SystemUI
   connection pass, temporarily select the already-installed gestural navigation
   overlay for app-to-Home, cancel, reverse and stale-layout tests.
5. Restore three-button navigation, disable the module, reboot and restore the
   retained normal APKs. Verify the original Recents component, APK hashes,
   settings and Home role. Leave the experimental module disabled after the test.

Home remains `dev.makepad.octosense`. Trebuchet's APK stays installed. The module
contains no boot scripts, global system properties or SELinux policy changes.
Its installer checks the ROM identity, framework APK checksum, original Recents
component, installed OctoSense packages, absent system_ext overlay configuration
and payload checksums. It refuses an unmatched ROM or an already changed Recents
configuration. This first version is an initial-install experiment; upgrade
behavior needs its own validation.

Magisk mounts the module's `system/system_ext` content over that partition and
supports disabling a module with its `disable` marker.
[Magisk module guide](https://topjohnwu.github.io/Magisk/guides.html).
The installed framework declares no overlayable groups and system_ext has no
overlay configuration file. The pinned framework's `IdmapManager.java:250` and
`ResourceMapping.cpp:60` supply the preinstalled-system policy path. A static
overlay is used because the Recents identity must be available during boot-time
permission evaluation. [Android resource overlays](https://source.android.com/docs/core/runtime/rros).

### Permission boundary

The allowlist names all 11 requested permissions with the ROM's `privileged`
flag: `ACCESS_CONTEXTUAL_SEARCH`, `ACCESS_HIDDEN_PROFILES_FULL`,
`ALLOW_SLIPPERY_TOUCHES`, `BIND_APPWIDGET`, `BROADCAST_CLOSE_SYSTEM_DIALOGS`,
`CONTROL_REMOTE_APP_TRANSITION_ANIMATIONS`, `INTERACT_ACROSS_USERS`,
`START_TASKS_FROM_RECENTS`, `STATUS_BAR`, `STOP_APP_SWITCHES` and
`WRITE_SECURE_SETTINGS`. Names are prefixed with `android.permission.`.
The module builder checks this set against the actual APK and pinned framework
permission inventory. The phone enforces privileged permission allowlists.

Other required permissions, including `STATUS_BAR_SERVICE`, `MONITOR_INPUT` and
`MANAGE_ACTIVITY_TASKS`, use the framework's trusted Recents grant path. System
placement alone does not prove these grants. The pinned permission policy checks
the known Recents package at `AppIdPermissionPolicy.kt:1571`; SystemUI separately
resolves its protected Quickstep service. Record actual grants after reboot and
stop the experiment if they or the binding are missing. Do not compensate with
unreviewed global permission or signature bypasses.

## Rollback and recovery

Retained originals, relative to `target/android/adr-0001-artifacts/`:

- `home-prototype.apk`: `9a13c2aed97d47e233658f787b974009bd49ea4a4c10e4f494eb38f5ef98c5b5`
- `home-layout/quickstep-prototype.apk`: `c6ddfdab93fc821a9f03bbd1ba24205bc3340e5da75474b3cb098d65985837a3`

The approved and exercised normal rollback sequence is:

```sh
adb -s cfb7c9e3 shell cmd overlay enable-exclusive --user 0 --category com.android.internal.systemui.navbar.threebutton
adb -s cfb7c9e3 shell su -c 'touch /data/adb/modules/octosense_quickstep_enchilada/disable'
adb -s cfb7c9e3 reboot
adb -s cfb7c9e3 wait-for-device
# Wait for boot completion and an unlocked, authorized device before installs.
adb -s cfb7c9e3 install -r target/android/adr-0001-artifacts/home-prototype.apk
adb -s cfb7c9e3 install -r target/android/adr-0001-artifacts/home-layout/quickstep-prototype.apk
adb -s cfb7c9e3 shell cmd overlay lookup android android:string/config_recentsComponentName
```

The final lookup must identify Trebuchet. Verify installed APK hashes, no native
Quickstep binding/gesture monitor, the original Home role, all recorded settings,
no validation instrumentation, and no owned test processes or forwards. The
normal APKs are older rollback builds; restoring them also removes the new
integration code from the phone.

If the system fails to finish booting but ADB root works, create the same scoped
disable marker and reboot. If ADB is unavailable, physical recovery with access
to `/data/adb/modules/octosense_quickstep_enchilada/disable` is required. This
boot-failure recovery path has not been exercised; a boot failure could require
hands-on device recovery. Successful normal rollback does not establish recovery
from a boot failure. Do not disable unrelated modules.

## Phone experiment and rollback — 17 September 2026

The user explicitly approved the Quickstep experiment and rollback. The exact
reviewed APKs and module above passed installation guards on `cfb7c9e3`.
The phone completed boot, classified Quickstep as system/privileged in system_ext,
granted the applicable privileged and trusted-Recents permissions, and selected
the OctoSense Recents component. SystemUI's OverviewProxyService reported enabled,
bound and connected with no backoff. Home bound the separate integration service.

Three-button Recents opened the native OctoSense activity. In gestural mode, the
input monitor and receiver were present; native controller logs settled on HOME
for swipe Home, LAST_TASK for a short cancelled swipe, and RECENTS for a
multi-stage swipe with a one-second hold. Tapping the recent fixture task resumed
the bridge Settings activity. A single slow swipe settled on HOME, so it is not
counted as a hold/Overview pass. The injected card-dismiss swipe did not produce
conclusive task-removal evidence. No OctoSense crash-process entries were found
in the retained crash-buffer check. No OS-window screenshots were taken.

Normal rollback selected three-button navigation, disabled only
`octosense_quickstep_enchilada`, rebooted, and reinstalled the exact original Home
and Quickstep APKs without clearing data. All three OctoSense APK hashes, Home
role, recorded settings and instrumentation registrations matched the initial
snapshot. Recents returned to Trebuchet; the native OctoSense TouchInteractionService
was absent. The module remains disabled, and the staged installation zip was
removed. Evidence is under
`target/android/adr-0001-artifacts/native-transition/device-experiment/`, including
`before.json`, `boot-state.json`, controller logs, `recents-actions.json` and
`rollback.json`.

## Acceptance evidence still required

- A real app-to-Home native surface transition using the acknowledged Home icon;
  reversal, stale geometry fallback and broader controller lifecycle/overflow.
  The passing swipe-Home outcome does not establish exact icon-target alignment.
- Conclusive task dismissal, quick switch, supported Back behavior, profile lifecycle,
  restart/reboot, missing-package and upgrade handling.
- Same-device performance, idle and memory gates from ADR 0001. Compilation and
  the earlier 258 SDK transport/coordinator checks do not satisfy those gates.
