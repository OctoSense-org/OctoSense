# System features in the current launcher

The current Home shade connects to the Android System Bridge. This build is intended
to remain installed; temporary fixture grants are removed after testing.

## On the phone

- Across unlocked apps, pull down from the physical **top-right edge** for
  **OctoSense Controls**, or the **top-left edge** for **OctoSense Notifications**.
  Close or Back returns to the app underneath. **System setup → System-wide
  OctoSense panel** controls this persistent feature. The lock screen retains
  Android's panel. See the [native panel record](systemui-shade-replacement.md)
  for exact artifacts, validation and recovery.
- On Home, pull down inside the right side, starting below Android's status bar,
  for **Controls**. The connection summary opens the
  Internet panel. Wi-Fi and Bluetooth open Android setup when a direct switch is
  unavailable. Hold either tile to open connection settings even when direct
  switching is enabled.
- The controls page includes hotspot, VPN, battery saver, display, sound and
  accessibility. Android owns network credentials, device pairing and consent.
- Tap **System setup** for permission status and setup. The same page is available
  from the Home long-press menu and the System Bridge app in the app drawer.
- On Home, pull down inside the left side, starting below Android's status bar,
  for notifications. **Enable notifications** opens
  Android's access screen specifically for OctoSense. Once access is enabled,
  notification cards expose app actions, native inline replies and dismissal.
  Tap a card to open its content; swipe left for actions and right to dismiss.
  Ongoing notifications have no Clear action. Updates retain the card's visual
  identity while replacing all old action and dismissal handles.

The physical top-edge panel is supplied by native OctoSense Quickstep; the
inside-Home panel is supplied by the existing Makepad Home. They share System
Bridge state and actions. Switching off the system-wide panel immediately restores
Android's physical top-edge entry without changing Home or Recents.

## Access and device behavior

| Feature | Access / behavior |
| --- | --- |
| Notification delivery, replies and dismissal | User enables notification access for OctoSense System Bridge. Revocation clears native cards, handles and unfinished reply drafts. |
| Media volume | Public Android API; reads observed volume after changes. |
| Brightness and rotation | User enables “Allow modifying system settings” for the bridge. Disabled brightness opens this consent screen. |
| Flashlight | Camera permission and flashlight hardware; the setup page requests camera access separately. |
| Bluetooth status | Nearby-device permission on Android 12 and later; pairing is available through Android settings regardless. |
| Wi-Fi / Bluetooth direct switches | Optional, explicitly requested bridge root connection and a supported ROM adapter. Otherwise tiles open working Android settings. |
| DND on Android 15 and later | Opens Android Modes/DND settings. Target-35 applications cannot reliably toggle other apps' global DND rules through the legacy setter. |
| Battery saver, hotspot, VPN, display, sound, accessibility | Native Android settings, reachable from Home. |

The connection summary observes the active network and reports Wi-Fi/mobile/VPN,
validated internet, sign-in requirements and metered status. It does not read
network names, location or credentials. Setup status updates on return from Android
consent and while bridge state changes; it does not poll periodically.

Android 15 DND behavior is documented in the
[NotificationManager API](https://developer.android.com/reference/android/app/NotificationManager#setInterruptionFilter(int)).

## Validation

Evidence is retained under
`target/android/adr-0001-artifacts/system-integration/`:

- `queue-retry/results.json`: 811 bridge assertions, including stable visual
  identity on update, expiration of old dismissal authority, and a new identity
  when a removed notification is reposted.
- `notifications/results.json`: real Home/Rust/JNI/Binder notification delivery,
  Unicode reply, duplicate-send protection, update identity, draft expiration,
  dismissal and revocation. Only the owned fixture's content was retained.
- `device-controls.log`: 73 phone checks including applying, observing,
  deduplicating and restoring media volume, plus denied-prerequisite behavior.
- `settings-ui-fixed/results.json`: all 13 rendered settings routes pass, including
  Internet panel delegation to SystemUI, Bluetooth after another Settings page,
  native notification access, and the setup hub. No grants changed.
- `resolved-settings.json`: native destinations resolve on the OnePlus 6 ROM.
- `deployment.json`: final normal Home and bridge checksums verified on the phone;
  default Home retained, no validation instrumentation or ADB forwards left.
- Nine local shade tests pass, including cache invalidation for permission,
  connection and dismissal state, and preservation of ongoing notifications.

Gradle builds and lint run offline for normal/prototype and validation variants.
Home is built in release mode with the normal manifest for deployment. Validation
registrations are excluded from that installed normal APK.

The subsequent user-approved Trebuchet removal activated the separate native
OctoSense Recents provider on this phone. See the
[migration record](trebuchet-removal-record.md) for the final installed state and
recovery steps. The launcher does not replace SystemUI or require a bridge root
grant for the public controls above.

The Settings route regression reproduced an Android task reuse problem: Home is
`singleInstance`, so opening a destination whose Settings task already existed
could resume an unrelated page on top. Routes now use `NEW_TASK | CLEAR_TOP`.
The phone UI test requires the exact destination activity, with the Internet panel
explicitly permitted to delegate to SystemUI. Early harness attempts and the
reproduction are retained alongside the passing run. Pixel capture remains
unverified; the acceptance evidence is input, renderer geometry, observed native
activity/window transitions, and real API results.


## Everyday access enabled

The user subsequently approved both grants for everyday use. Notification access
is enabled and the bridge's own setup UI reports its listener connected.
`Settings.System.canWrite` is true in the app; the setup UI confirms brightness
and rotation access. Camera and Bluetooth status access remain enabled.
`system-integration/everyday-access.json` records the grants and exact verified
status labels without retaining notification content.

Trebuchet removal was requested afterward; see
[the migration record](trebuchet-removal-record.md) for the latest Recents state.
