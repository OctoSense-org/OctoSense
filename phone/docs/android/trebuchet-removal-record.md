# Trebuchet removal on the OnePlus 6

The user approved ongoing notification and write-settings access, then explicitly
requested removal of the old launcher and identified Trebuchet after its role in
Android Recents was explained. This supersedes the earlier experiment-only
Quickstep scope for this phone. OctoSense Home and its new System Bridge build
must remain installed throughout.

Target: `cfb7c9e3`, Android 15/API 35,
`22.2-20260708-NIGHTLY-enchilada`, incremental `21be58cea4`.

The retained native Quickstep APK and disabled Magisk module match the previously
reviewed SHA-256 hashes. Every installed module payload, the framework resource
APK, and the unchanged Home/bridge APKs are verified in
`target/android/adr-0001-artifacts/trebuchet-removal/before.json`.

The migration activates the reviewed separate Recents provider, reboots, and
checks system placement, trusted grants, SystemUI binding, Home, Recents, app
resume and task dismissal before removing Trebuchet for user 0. The factory APK
is part of the read-only ROM and is not erased. Retain Trebuchet data for recovery
with the package manager's `-k` option; removal still unregisters the app for the
active user. No other launchers or preview apps are targeted.

## Recovery

If navigation fails after user removal, first restore Trebuchet's user
registration, then disable only the scoped Quickstep module and reboot:

```sh
adb -s cfb7c9e3 shell cmd package install-existing --user 0 com.android.launcher3
adb -s cfb7c9e3 shell "su -c 'touch /data/adb/modules/octosense_quickstep_enchilada/disable'"
adb -s cfb7c9e3 reboot
# After boot completion and unlock, restore the retained layout-only Quickstep:
adb -s cfb7c9e3 install -r target/android/adr-0001-artifacts/native-transition/device-experiment/dev.makepad.octosense.quickstep.apk
```

Keep the current Home/bridge APKs and the approved everyday permissions. The
restored Recents resource must name
`com.android.launcher3/com.android.quickstep.RecentsActivity`. Confirm Home remains
`dev.makepad.octosense`. If ADB is unavailable during a boot failure, the previously
documented physical recovery path requires disabling this one module under
`/data/adb/modules`; boot-failure recovery has not been exercised.

This is a phone-specific prototype migration, not a native parity or performance
claim.

## Installed outcome

The reviewed module is active. Trebuchet (`com.android.launcher3`) is uninstalled
for the only Android user, user 0, with `installed=false` and `stopped=true`.
It has no Home or launcher activity available for that user. Its factory APK and
retained data remain recoverable; this did not erase a system partition. The two
remaining `com.android.launcher3` resource-overlay packages are not the launcher.

Home remains `dev.makepad.octosense`. Android Recents resolves to
`dev.makepad.octosense.quickstep/com.android.quickstep.RecentsActivity`, and
SystemUI binds to its native Quickstep service. This provider is built from the
ROM-matched Trebuchet/Quickstep source and retains the upstream Recents interface;
similar appearance is not evidence that the removed launcher is running.

Installed APK SHA-256 values:

| Package | SHA-256 |
| --- | --- |
| Home | `c23cba361b3d4e6bfeb5b8228db7d8189d4904df586aa825bb721888248cfb7d` |
| System Bridge | `a7e9127c78f7ebecccf1b0c83b2ab87741d6071539a5463ff0654d5ceec892fe` |
| Native Recents | `fba417a89253cec42274d07077a757d3a565349e9a8f11b19de6c2e9374e0ee0` |

Evidence is under `target/android/adr-0001-artifacts/trebuchet-removal/`:

- `pre-removal-replacement-checks.json` and
  `pre-removal-replacement-actions.json`: replacement grants, binding, Home,
  Recents, owned-task resume and dismissal verified before uninstalling Trebuchet.
- `removal.json`: successful `pm uninstall -k --user 0`, package state and absence
  from the Home resolver. Exact package matching excludes the resource overlays.
- `replacement-checks.json`: another boot completed after uninstall; the new
  Recents provider retains system/privileged placement, all 14 required grants,
  and a connected SystemUI service without reconnect backoff.
- `replacement-actions.json`: owned Bridge task opens from its actual Recents
  card and is removed by swiping that card. Task existence is checked before and
  after dismissal. Home returns successfully and the crash buffer has no
  OctoSense process crash entry.
- `final-device-state.json`: unchanged current Home/bridge hashes, native Recents
  hash, active module, removed Trebuchet user registration, default Home,
  approved everyday grants, no OctoSense validation instrumentation or ADB
  forwards. Final boot ID: `7a0869e9-8ec6-436a-96bd-6897b460ba91`.

Three-button navigation remains selected. The test records include an immediate
post-launch Recents input that selected the previous Settings task; waiting for
the activity animation to settle passed. Fast switching and full gesture/Back
parity are not established. An earlier resume path required Back through Recents
before Home, while the final run returned directly to Home. Tests retained only
owned task identity and geometry, not other apps' titles or thumbnails.

The user subsequently reported that Trebuchet still appeared to work. A live
read-only recheck found OctoSense Home in focus, no Trebuchet process, no available
Trebuchet Home/launcher activity, and the user-0 package still uninstalled. The
native replacement is labelled `OctoSense Recents` and exposes no Home or app
drawer launcher activity. The user clarified that Trebuchet was gone and that
the familiar panel appeared on a top-right downward swipe. Live window state
identified that panel as Android SystemUI's `NotificationShade`, which this
migration does not replace. See the
[Home shade entry instructions](current-launcher-system-integration.md#on-the-phone).

The user then requested replacement across other apps. The subsequent
[global OctoSense panel integration](systemui-shade-replacement.md) adds that
native window and edge gesture while preserving SystemUI's core services and
Trebuchet's removed user registration. Its artifact hashes supersede the
bridge/Recents hashes in this historical migration record.
