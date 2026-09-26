# Settings platform emulator validation

This is an integration record, not a claim of complete Android Settings parity.
The feature inventory and remaining acceptance gates are in
[settings-parity.md](settings-parity.md).

## Environment

On 2026-09-24 and 2026-09-25, validation used two dedicated Android 15 / API 35 arm64 emulators:

- `OctoSense_Settings_Validation_API35`: Google APIs image, normal development
  Home APK, no privileged Settings helper. This checks permission consent,
  limited public observations and unsupported controls.
- `OctoSense_ROM_Settings_API35`: AOSP default image revision 2, writable system
  overlay, Home builds `2026092421`, `2026092423`, and `2026092426` through `2026092505`, the framework-compiled Agent and
  SettingsBroker (including the subsequent manual DND adapter), and the finite sensor service added to SystemUI. This checks
  the actual Binder chain and Android enforcement.

The second emulator uses the **public AOSP platform test certificate**, matching
its image. These APKs are disposable validation artifacts and must not be
distributed or installed on a phone. Production signing is unchanged. The
original SDK image and ROM framework source were not modified; only the owned
AVD's writable overlay was changed. Other emulators and the OnePlus were excluded
from these tests.

The helper and broker compiled against the pinned ROM framework, together, in
isolated Soong validation modules. Both module builds succeeded; the subsequent Agent + DND broker rebuild also
succeeded against the pinned framework (6 minutes 2 seconds). Their temporary
installed product-output directories were removed afterward to prevent duplicate
packages from entering a ROM build.

The SystemUI test overlay retains the original AOSP APK and adds only the sensor
service manifest entry and compiled service classes. Decoding and comparing its
manifest verified that removing that one entry reproduces the original manifest
tree. It is not a replacement for the production SystemUI stager/build check.

## Findings and observed results

| Check | Result |
| --- | --- |
| Boot with Agent, broker and sensor adapter | Boot completed; no crash-buffer entries |
| Account broker identity | Actual package UID 1000; SystemUI remains its separate `android.uid.systemui` UID |
| Full account inventory through Home → Agent → broker | Independently signed synthetic account, explicitly hidden from Home's public account view, appears in the full current-user inventory |
| Account details | Real provider and sync authority shown; automatic sync observed Off |
| Removal launch on build 2421 | Android 15 blocked the PendingIntent launch; sender opt-in added |
| Account removal and Add on build 2423 | Native confirmation opened; Cancel preserved the account; confirmed removal left zero accounts; provider Back preserved zero; provider Add restored one synthetic account and returned to Home |
| Account replacement during open confirmation | The synthetic provider removed/re-added the same account name/type; pressing the old Remove confirmation rejected it and preserved the new account. The temporary replacement receiver was removed afterward |
| Sensor writes through Home → Agent → SystemUI | Camera and microphone Off/On matched `dumpsys sensor_privacy` software-toggle state; both restored On |
| Actual Camera behavior with global access Off | Android showed “Unblock device camera?” and blocked the preview; Cancel preserved the block |
| Actual microphone use with global access Off/On | Separate ordinary-permission AudioRecord fixture reported `isClientSilenced=true` with access Off and `false` with access On; Android displayed its unblock prompt; no audio bytes were read or saved; fixture uninstalled |
| Wi-Fi toggle and saved connection | UI Off/On matched `cmd wifi status`; helper-originated saved-network connect reached the connected state with DHCP |
| Bluetooth adapter on build 2423 | UI Off/On matched BluetoothManager OFF/ON state; enable log attributed both operations to the Agent; original On/name retained; no peripheral pairing tested |
| Bluetooth name on build 2426 | Rename changed the actual Android name; Cancel preserved the saved name; original `Android SDK built for arm64` restored through the UI. Back hid the keyboard and preserved the draft |
| Date/time existing controls on build 2427 | Automatic time and time zone both changed On → Off → On through the UI; 12/24-hour mode changed both ways. Original automatic values and absent format row restored. One initial unavailable observation recovered after Activity resume; its cause is still unconfirmed |
| Bluetooth discovery on build 2427 | Scan changed actual BluetoothManager `Discovering` to true; Stop scan changed it to false; observed UI agreed in both directions |
| Location and scanning on resumed build 2429 | Location Off/On matched both `cmd location is-location-enabled` and the secure setting; Wi-Fi and Bluetooth scanning On/Off matched Android settings and UI readback. Original On/Off/absent states restored |
| Location administrator restriction | `no_config_location` disabled all three choices; pressing Location Off left actual location On. Original false restriction restored |
| Battery Saver on resumed build 2429 | While charging, On was disabled and PowerManager remained Off. Simulated unplugged 45% allowed manual On/Off; both matched PowerManager and the built-in observed value |
| Automatic Battery Saver threshold | Higher changed Never to 5%. An external custom 23% remained visible; Higher selected 25% and Lower selected 20%. After a simulated charging cycle and unplugged 15%, PowerManager actually enabled Battery Saver automatically |
| Other battery policies | Adaptive battery Off/On matched the stored policy and UI. The AOSP image reported the sticky 90% control unsupported, with disabled choices. Normal live battery reporting was restored at charging 100%, Saver Off, with the original absent threshold/adaptive rows restored |
| Targeted Wi-Fi configuration | Exact AndroidWifi native dialog opened and returned to the selected OctoSense network details |
| Ordinary app access to privileged services | Independent signer denied all four service bindings (Agent, accounts, sensors and DND) and direct entry to the unexported removal confirmation |
| Unrelated platform-signed app access | Signature permission granted, but all four real Binder observations denied by the in-service package gate; direct removal activity also denied |
| Updates helper on a non-A/B image | Starts without UpdateEngine; ROM update unavailable; read polling works |
| Explicit update check | Real HTTPS release feed check completed and displayed offered versions; no update was installed and no updater restart command was sent |
| Airplane mode and Data Saver on build 2423 | Both UI toggles matched actual framework state in both directions; initial Off values restored |
| Private DNS on build 2423 | Invalid URL input kept Save disabled; saving `dns.google` set hostname mode and Android reported encrypted DNS active and validated over the default Mobile transport; Off and Automatic matched framework mode readback; original absent mode/specifier rows restored |
| Private DNS external change | With an unsaved Off draft, changing Data Saver externally showed a changed-settings message, retained the draft, and disabled Save; tapping Save left DNS unchanged; Cancel → Edit loaded current Automatic mode; Data Saver restored Off |
| Manual DND modes on build 2426 | All four UI choices matched the framework mode and selected-label readback. An ordinary fixture’s message/alarm notifications matched the expected interruption filters: Priority false/true, Alarms false/true, Silence false/false, Off true/true; original Off restored, two existing automatic rules retained, no implicit OctoSense rule created |
| DND administrator restriction on build 2426 | Temporarily applying `no_adjust_volume` rejected a press made from the previous enabled UI, showed a policy error and disabled the choices; a further disabled press left mode Off. Restriction restored false and the notification fixture/listener grant removed |
| Optional bridge refresh on build 2423 | Successful account and network actions no longer showed a misleading missing-bridge notification |

The account test uses `dev.makepad.octosense.accountsfixture`, a separately signed
disposable provider with no real credentials. Full inventory must be distinguished
from the normal-app account chooser grant tested in the Google APIs emulator.
Neither test proves work-profile or secondary-user support.

The removal test exposed Android's explicit PendingIntent sender requirement.
The fix supplies launch options only from the resumed, focused Home activity for
the validated broker token. The creator does not grant its background launch
privilege. The pinned compilation SDK exposes the older boolean launch setter;
the pinned Android 15 `ComponentOptions` implementation maps `true` to the same
allowed mode. The runtime call is guarded for API 34 and newer. See Android's
[activity launch requirements](https://developer.android.com/guide/components/activities/secure-bal).
An accepted Binder result alone did not prove the confirmation screen opened;
the ActivityTaskManager log was essential to finding the failure.

The AOSP image was provisioned before the helper was added. Adding a default
permission XML afterward did not rerun its first-boot runtime grants. For this
integration test only, the helper's declared Bluetooth Connect/Scan and notification
permissions were granted with `pm grant`, matching the ROM's existing default
permission file. This does not validate clean-ROM first-boot provisioning.

## Repeating the caller-boundary check

With the helper, broker and sensor adapter already staged in a dedicated AOSP
API 35 test emulator, run
[run-settings-boundary-probe.py](../../android/scripts/run-settings-boundary-probe.py):

```sh
python3 home/android/scripts/run-settings-boundary-probe.py \
  --serial emulator-5558 --adb /path/to/platform-tools/adb \
  --sdk /path/to/android-sdk --java-home /path/to/jdk-17 \
  --platform-test-key /path/to/public-aosp/platform.pk8 \
  --platform-test-cert /path/to/public-aosp/platform.x509.pem
```

The SDK needs platform 35 and build tools 35.0.0. The runner refuses physical
devices and an already installed fixture package. It creates a fresh ordinary
test signer and checks that the platform test signer actually receives the
signature permission before testing the in-service gates. Only the known public
AOSP platform certificate is accepted. Both test APKs are uninstalled afterward; the nine checks cover four service
observations, the Agent time/history/sound transactions, unconditional sound Stop
and the unexported confirmation;
the fixture neither changes settings nor requests account contents successfully.

## Manual Do Not Disturb adapter

`OctoSenseZenSettingsService` is a second finite, bind-only service in the
system-UID SettingsBroker. It accepts only `off`, `priority`, `alarms` and
`silence`; it accepts no rule IDs, condition URIs, policy objects or setting keys.
The exact signed Agent caller and current unlocked administrator are checked,
including `DISALLOW_ADJUST_VOLUME`, before a write. Home has public read-only
observations when the helper is absent.

The pinned framework's `NotificationManagerService.setZenMode` requires system
or SystemUI authority, and its user-origin option is similarly protected. The
broker uses `setZenMode(mode, null, "OctoSenseSettings", true)`, matching the
pinned Android Settings manual control. This avoids the Android 15 public app
API's implicit-rule behavior described in
[NotificationManager.setInterruptionFilter](https://developer.android.com/reference/android/app/NotificationManager#setInterruptionFilter(int)).
An accepted call remains distinct from observed readback. Rule/exception editing
is still a separate parity item.

The new broker installed as an update in the owned AOSP emulator. Android refused
to update the persistent Agent as a normal data APK, so its writable system
overlay APK was replaced and **only this emulator** was rebooted. Both ordinary
and unrelated platform-signed fixtures then passed all five caller-boundary
checks. Four-mode UI behavior passed using Home build 2426 and a disposable ordinary
notification fixture. The fixture observes ranking only for its own two
synthetic notifications; it logs no other notification content. Administrator
denial also passed: both the initially enabled press and a later disabled press
left DND Off under `no_adjust_volume`. The restriction was removed, its false
readback checked, listener access revoked and the fixture uninstalled.


## Date and time adapter

The finite Agent time adapter compiled against the pinned framework (final
rebuild: 3 minutes 51 seconds), installed in the owned AOSP emulator, and booted
without crash-buffer entries. It uses `TimeManager` for capability/configuration
and authoritative zone readback, plus the same manual time/zone suggestion APIs
as the pinned Android Settings. The two new permissions are held by the Agent,
not app scripts. Ordinary and unrelated platform-signed callers both failed the
new real time-observation transaction, alongside the prior five caller checks.

Manual values are tied to an expiring observation key. Automatic-source, zone,
policy and material clock changes invalidate the old draft. Civil time uses a
strict calendar parser; nonexistent daylight-saving hours are rejected, and
repeated hours have explicit first/second choices. Unit tests cover leap years,
invalid dates, both occurrences and non-DST zones. The time-zone inventory is
bounded to 1,024 system-provided identifiers and shown 20 rows at a time; selecting
a row opens review, and only the subsequent Apply requests a change.

Home build 2428 passed the following on the dedicated AOSP emulator:

- Automatic time/zone changes matched Android's stored configuration. Manual
  controls remained disabled while the corresponding automatic source was on.
- Filtering for Shanghai, selecting it and cancelling left Los Angeles intact.
  Applying the reviewed zone changed both the time service and
  `persist.sys.timezone` to `Asia/Shanghai`; it survived a Home process restart.
  Los Angeles and automatic zone were restored through the built-in UI.
- `2026-02-30 12:00` disabled Save. Pressing it and then Cancel left the wall-clock
  offset from uptime unchanged. The real helper rejected Los Angeles
  `2026-03-08 02:30`, a nonexistent daylight-saving hour, without changing time.
- Saving a valid reviewed draft changed the actual Android clock to that minute
  and closed the editor. Enabling `no_config_date_time` while a draft was open
  showed the restriction and invalidated Save; a subsequent press left time
  unchanged. The test restriction was removed.
- Enabling automatic time restored the original wall-clock offset from uptime
  within three seconds. Original automatic sources, Los Angeles zone, absent
  `time_12_24` row and unrestricted policy were all checked after restoration.

Repeated Ctrl+A replacement in the clock field reproduced the 2428 input
synchronization bug: text was inserted into the old date. Invalid text remained
unsavable. Build 2430 then passed six immediate Ctrl+A replacements in the same
editor, including valid dates, invalid dates and DST examples. A seventh typed
value saved to Android's actual clock. Automatic time and the original clock
offset were restored; that run used the test harness's SettingsProvider fallback
after its screenshot caught the success message before the new layout settled.
The earlier 2428 run independently passed restoration through the built-in UI.
Build 2430 also includes readable global time-error notifications.

No physical device or published ROM was changed by this validation.

## Combined keyboard acceptance on build 2431

On the Google APIs emulator with Gboard, build 2431 passed cold first-word input,
eight immediate Ctrl+A replacements and eight immediate Clear→input sequences.
Back remained hidden at one second and six seconds (across the Settings refresh);
a deliberate editor tap reopened the keyboard. The same checks passed after an
Activity roundtrip through the native input fixture. Current editor Done hid the
keyboard and preserved the exact draft. The real InputConnection fixture passed
composition, deletion, batching, current Done and retired action/query isolation.

This build includes the no-editor acknowledgement path, dispatch-time dismissal
guard and ordered, session-bound editor actions. The queued field-A action cannot
submit field B. These are general Makepad runtime fixes. No synchronous blocking
UI/render-thread barrier or typed-prefix stripping heuristic was introduced.
These checks passed the reproduced 2431 failures; they do not establish
compatibility with every keyboard or physical device. A subsequent 2432 cold
Search navigation/immediate hardware-injection case displayed `gtone` after
`ringtone` was sent. Actual Gboard cold Search entry then reproduced an empty
field in three of three runs despite visible “Book” suggestions; two of six
hardware-entry runs also lost a leading character. This is a confirmed first
editor session bug under diagnosis with metadata-only traces in diagnostic2434.
The production runtime was unchanged between2431 and2432; earlier warm-session
passes remain valid but do not close the cold-input gate.

Diagnostic2435 identified two pre-focus Android InputConnections: Gboard retained
the earlier one, while only the latest Java pointer was bound to the first native
editor session. The earlier connection rejected every key before enqueueing it.
The correction authorizes connections by editor session and cohort lifetime,
permits only initial0→first-session1 bootstrap, and retires every connection in a
cohort on pause or authoritative editor retirement. The first inactive/session0
reply preserves pending pre-focus connections; a later inactive reply retires the
cohort even if the session number is unchanged. Retired or closed handles cannot
read, edit or perform Done on a newer editor.

The exact Android fixture failed before the fix and passed afterward, including
two pre-focus handles, an unused bootstrap handle missing the first editor,
multiple current handles, same-session retirement, pause before the native reply,
closed connections and next-editor isolation. The full 394 Home suite and all 48
repository tests passed. Build 2436 has no diagnostic logs. Actual Gboard first-focus typing passed six
consecutive cold launches on the Google APIs emulator, replacing the earlier
three-of-three empty-field reproduction. The Google APIs image also passed six immediate cold hardware-entry runs.
Warm Gboard acceptance then passed eight immediate Ctrl+A replacements and eight
Clear/input runs, Back hidden at one and six seconds, retap, and Done preserving
the draft. A roundtrip through the native InputConnection test Activity passed
all cohort/composition/isolation checks and returned to the exact Settings draft;
subsequent replacement, Back and retap worked. On the separate AOSP emulator, build
2436 passed three cold first-editor runs, four immediate Ctrl+A replacements,
four immediate Clear/input sequences, Back staying hidden at one and six seconds,
and an Activity roundtrip preserving the draft before retap and replacement.

## Notification history build and unit validation

Build 2431 adds the built-in read-only notification history page, with twenty
rows per page, stable expiring snapshot keys and direct Settings search routing.
The owner-only helper checks both notification user and UID, access permission,
AppOps and lock state. Disable confirmation warns that Android deletes saved
history; Cancel makes no change. The Home host discards content when the page or
trusted instance retires. Neither scripts nor the page supply package names,
listener tokens or notification actions.

All 389 Home Rust tests and 47 repository tests passed before installation.
Coverage includes Unicode limits, malformed/denied pages, stable paging, live
theme and large font changes, clearing hidden row text and rejecting an unreviewed
or no-longer-authorized disable action. The APK correctly showed unavailable
history when paired with the older helper. Full history-service behavior is
recorded below only after actual emulator readback.

The remote validation build exposed an incremental-build trap: an updated file
staged with its original timestamp was older than an intermediate compiled after
the previous staging. Ninja reported no work while the APK lacked the observer
invalidation change. Comparing source digests alone did not detect this; APK DEX
inspection did. The affected isolated sources were touched and rebuilt; subsequent
staging refreshes source timestamps. This stale artifact was not installed.

### Notification history service acceptance on build 2431

The corrected helper compiled against the pinned framework, and APK inspection
confirmed the cache-generation/observer implementation before overlay installation.
Only the dedicated AOSP emulator rebooted. Android granted the declared narrow
`ACCESS_NOTIFICATIONS` permission to the Agent; Home did not gain it.

- History initially reported Off. Enabling it from OctoSense changed the actual
  current-user secure setting to 1.
- An ordinary independently signed disposable app generated 24 synthetic records.
  OctoSense displayed the actual Android archive: 1–20 and 21–24, with correct
  reverse time order and Previous navigation. Chinese text and an envelope emoji
  rendered; the bounded long preview wrapped within the narrow content column.
- Turning history Off and On externally between read polls invalidated the old
  snapshot and cleared its displayed bodies. Refresh then confirmed Android's
  history was empty. New synthetic notifications appeared after returning from
  another Activity, using a fresh read.
- Setting only the Agent's `ACCESS_NOTIFICATIONS` AppOp to ignore cleared the
  displayed records and showed unavailable access. Restoring its original default
  mode and explicitly refreshing read the records again.
- The ordinary and unrelated platform-signed caller fixtures each passed all
  seven then-current boundary checks, including the real history transaction.
  Both were removed. After installing the Sound helper, both signers passed all
  nine checks, including the actual Sound read and unconditional Stop. Neither
  test fixture remains installed.

- A background synthetic arrival left the active three-record snapshot stable
  across polling. Explicit Refresh loaded the fourth record. The two-minute
  snapshot then expired despite continued polling and cleared every displayed
  record; polling did not extend its lifetime.
- Off opened the visible deletion review while the actual flag remained On.
  Cancel preserved all four records. Confirm changed Android to Off; re-enabling
  showed an empty archive. The original absent/effective-Off history setting and
  all three other notification preferences matched their captured baseline at
  cleanup. Notification access was restored to its original default AppOp mode.
  The disposable synthetic notification package was uninstalled; the crash
  buffer remained empty.

## Sound catalog acceptance on builds 2432 and 2433

The shared catalog backend and finite Agent methods compiled against the pinned
framework. Source digests and APK DEX inspection confirmed both the history
observer and sound implementation before installing the helper in the owned
AOSP emulator. The temporary installed validation-module output directories were
removed after export. Home 2432 passed both the ordinary-installation path on the
Google APIs emulator and the privileged-helper path on the AOSP emulator.

- Both paths observed 98 ringtone choices, 74 notification choices and 19 alarm
  choices, including Silent. Choices are returned twenty at a time using expiring
  keys; scripts never receive or supply audio URIs.
- On the ordinary installation, Save remained disabled until Android granted
  Home's Modify system settings access. Saving a ringtone, Silent notification
  and an alarm changed the actual defaults and displayed the observed values.
  The original permission AppOp mode was restored after testing.
- On the helper path, ringtone Preview was attributed to the Agent UID with
  `USAGE_NOTIFICATION_RINGTONE`. Preview and Cancel left the default unchanged;
  Stop released playback. Saving Andromeda changed the actual canonical default
  and displayed it; paging to Flutey Phone restored the original through the UI.
- Temporarily setting `no_adjust_volume` rejected Save from a previously enabled
  screen. Restoring the original restriction allowed saving again. An external
  change of the default invalidated an older unsaved draft; its Save control
  became disabled and pressing it preserved the newer external value.
- Saving Silent for notifications stored a null default, with matching UI
  readback. The original Pixie Dust default was restored. Saving Argon for alarms
  changed the real default; its preview used `USAGE_ALARM`. Opening Android
  Settings released playback in roughly half a second. Cesium was restored
  through the built-in UI after returning.
- The three original defaults, ring/notification/alarm stream levels and DND Off
  were verified after cleanup on both emulators. No fixture or policy restriction
  remains from these checks.
- Build 2433 additionally passed actual row-selection navigation: choosing
  Andromeda scrolled to its visible review, Preview, Save sound and Cancel
  selection controls. Cancel cleared the draft and preserved Flutey Phone.

All 393 Home tests and 48 repository tests passed for the Sound increment; the
row-navigation change passed the full Home suite again. Custom audio import,
physical audio routing, secondary users and device-specific vibration/charging
features remain separate parity items. Testing also found that the older media
volume UI depended on the optional bridge. It now uses the existing typed
AudioManager device-control path; its focused Rust widget test and all seven Java
device-contract tests passed. Diagnostic2435 included this production media path
alongside temporary metadata-only keyboard traces. On the AOSP emulator it showed
33% for the actual media level5/15, changed that stream through both UI buttons,
and restored5/15. An external change to9 appeared as60%. The administrator
restriction rejected a previously enabled press and a later disabled press.
Original media/alarm/ring/notification levels and DND were verified after cleanup,
with the restriction removed. Build2436 then confirmed the Search→Media volume route reached Sound and showed
33% for the restored5/15 stream, with an empty crash buffer.

## Lock-screen notification policy on build 2433

The built-in visibility and sensitive-content controls each changed the actual
secure setting in both directions. With all lock-screen notifications hidden,
the sensitive-content control became read-only; a press could not change its
stored preference. Both preferences were restored through the built-in UI.

An independently signed fixture then posted one synthetic public and one private
notification. A temporary test-only PIN secured this dedicated AOSP emulator:

- With both preferences On, the locked screen displayed both synthetic contents.
- Turning sensitive content Off in OctoSense hid the private content on the next
  secure lock screen while retaining the public notification.
- Turning all lock-screen notifications Off in OctoSense removed both fixture
  notifications from the locked screen.
- Both preferences returned to their original On values through OctoSense. The
  temporary test credential was cleared, the original unsecured lock setting
  restored and verified, and the fixture uninstalled. History remained Off.

The unlock harness used Android's `wm dismiss-keyguard` to open its real PIN
prompt, followed by the fixture credential; it did not bypass confirmation.
A swipe-based attempt failed to open that prompt after the second lock and was
replaced in the harness. No OctoSense unlock or credential UI was introduced.
The AOSP image reports `config_supportsBubble=false`; the page correctly showed
bubbles unsupported. Actual bubble behavior remains a separate hardware/image
acceptance check.

## Android accessibility on build 2437

The baseline UiAutomation probe on build 2436 could reach Home's window but
could not read the visible Settings controls. Build 2437 adds a native virtual
tree for the compiled Settings widgets, enabled only while an Android
accessibility service is active. Ordinary rendering does not traverse the tree.
The tree exposes clipped physical bounds, labels, editors, disabled controls and
the actual scroll ancestor. Actions recheck the current trusted Settings page
and observed target; retained nodes cannot operate a replacement page or row.

All 403 Home tests and 49 repository tests passed for this increment. The
public-API UiAutomation fixture passed on both dedicated emulators:

- The Google APIs/Gboard emulator passed 30 smoke checks and 16 tree-inspection
  checks, including actual forward/back scrolling on Sound.
- The larger AOSP/helper emulator passed the revised 33-check smoke test and
  16 inspection checks. Sound fits its taller viewport, so the final shared
  fixture exercises scrolling on the longer overview page instead.
- Search navigation, exact text replacement, readable disabled controls and
  rejection of disabled/read-only/stale actions passed. The editor kept its
  identity and accessibility focus through more than six seconds of polling.
- Accessibility Focus opened the real keyboard. Back kept it hidden, and
  activating that same editor reopened it with the draft intact.
- Opening native Android Settings retired the old editor. Returning preserved
  the draft under a fresh accessibility identity; the old node stayed invalid.
- Nested Back returned from Sound to Search and then to the overview. Scrolling
  moved the first row offscreen and restored the same row identity on return.

The first inspection attempt accepted a pre-publication Home button; the
fixture now waits for the Settings scroll container. Neither that timing fix
nor choosing a page that actually overflows required a production-code change.
The fixture removes itself and changes no device settings. This validates
Android's accessibility APIs, not real TalkBack speech, switch navigation or
physical-device acceptance.

Run the checked-in fixture with an explicit owned emulator serial:

```sh
python3 home/android/scripts/run-settings-accessibility-probe.py \
  --serial emulator-5558 --adb /path/to/adb --sdk /path/to/android-sdk \
  --java-home /path/to/jdk17 --scenario smoke
```

## External Settings entries on builds 2501–2503

The exported `SettingsEntry` alias targets the existing single-instance Home
Activity. It accepts 20 finite navigation routes and 19 reviewed Android Settings
actions. It cannot execute settings writes or carry arbitrary intents. Explicit
native fallback buttons pin `com.android.settings`, avoiding loops now that Home
also advertises Settings actions. Unsupported and consent-specific actions retain
the native destination.

Build 2501 passed on the Google APIs emulator but lost a cold entry on AOSP.
The pinned Makepad runtime consumes unrelated queue messages while waiting for
both Init and SurfaceChanged; successful JNI enqueue therefore did not prove
Rust had received the entry. A Rust-side pre-state queue alone could not recover
that message. Build 2502 introduced a ready/receipt handshake: Java holds the
latest entry until the initialized shell is ready and retains it until Rust
acknowledges the matching ID. Superseded entries and acknowledged entries are
never replayed. No startup delay or runtime patch was added.

The same bootstrap could consume the initial Activity/accessibility observation.
Build 2503 reconciles current UI state once when readiness first arrives, before
sending the pending entry. It also invalidates Android's accessibility subtree
when the published tree changes or clears. Both emulators then passed:

| Public-API fixture | AOSP/helper emulator | Google APIs/Gboard emulator |
| --- | --- | --- |
| Six force-stopped cold entries, with native Activity return and no replay | 25 checks | 25 checks |
| All 20 custom routes and 19 standard actions | 48 checks | 49 checks |
| Accessibility navigation, editor, retirement and real scrolling | 33 checks | 33 checks |

The one extra Google APIs check reflects an additional necessary scroll on its
shorter viewport. Repeated main-Activity/alias launches on that emulator retained
one ActivityRecord and task. Crash buffers were empty. All 408 Home tests and
50 repository tests passed. The fixture uninstalls itself; it changes only
navigation and a disposable Search draft. These results do not validate release
signing, ROM intent priority, Quick Settings entry or physical-device behavior.

## ROM default routing on builds 2504–2505

The default-route check exposed an Android PackageManager update issue. A
privileged build 2504 scanned successfully, retaining Home's UID, both app-data
directories and first-install identity, but its Settings alias still resolved
at priority 0. The pinned framework's `findMatchingActivity` matches an alias
against its target Activity before reaching the matching system alias. The
updated alias's priority was therefore capped against Home's original filters.
A fresh-base-only test would have missed this update path.

Build 2505 places the 19 standard Settings actions at priority 2 on the existing
single-instance Makepad Activity. The alias retains only the finite custom
navigation action at priority 0. A manifest regression protects this split.
An APK update over the old 2504 base correctly retained native default routing;
a 2505 base rescan resolved Settings to Home at priority 2. Reinstalling the
same 2505 APK as a data update retained priority 2. This requires a ROM base
update; delivering only an APK on an older ROM does not establish the default.

The dedicated AOSP emulator had only 17 MiB free in its writable system overlay,
less than the full Home APK. Validation used a temporary read-only bind mount
of the exact test-signed APK at `/system_ext/priv-app/OctoSenseHome`, the production
Home-only privileged-permission block, and an Android framework restart.
PackageManager reported SYSTEM, UPDATED_SYSTEM_APP and PRIVILEGED. The SDK image
was unchanged, no data was cleared, and no phone was touched. This validates
privileged package scanning and update routing; it is not a complete production
ROM boot, release-signing check or custom Quickstep runtime test.

The actual build 2505 privileged fixture passed 46 checks: every one of the
19 implicit standard actions selected Home and exposed its expected Settings
pane; an independently signed foreground caller reached Sound through the
trusted preferred-route API; the explicit Sound fallback and unmigrated
Accessibility destination opened native Android Settings; and clicking the
real AOSP Quick Settings gear opened OctoSense's overview. A subsequent
six-cold-start run passed 25 checks and the accessibility smoke passed 33.
The Google APIs emulator's ordinary install passed four native-default/fallback
checks, 25 cold-entry checks and 48 explicit-route checks. All 52 repository
tests passed; the unchanged Rust implementation retained its 408-test result.

Two fixture assumptions were corrected during this run: Sound's heading can
appear before its final buttons, so the fallback helper waits for either the
target or a scroll action; the AOSP Compose gear labels its icon “Open settings.”
and puts the click action on the parent View. The fixture also collapses any
previously expanded shade before starting a new route test. These changes did
not require product-code edits or substitute a shell launch for clicking the
actual gear.

The temporary Home mount, source copies and extra permission file were removed,
then the Android framework restarted. Home returned to an ordinary data APK;
native Settings again resolved at priority 1. UID, both app-data directory inode
IDs, first-install identity and all 26 tracked device preferences were unchanged.
A final ordinary-install fallback probe passed all four checks. Three pairs of
main-Activity/custom-alias launches retained one ActivityRecord and task. During
fixture removal, a rapid framework stop/start produced a transient
`system_server` DeadSystemException before Home started; its diagnostic logs were
retained. Subsequent full boots of the dedicated emulator completed with empty
crash buffers. No complete ROM image was built or installed in this routing
increment.

## Live display-density prerequisite on build 2506

The pinned runtime previously read Android display density only at startup, and
Home's configuration mask did not handle density changes. The real public-API
fixture failed on build 2505 when changing the dedicated AOSP emulator from
420 to 504 DPI: the accessible Search editor and its draft were no longer
available. Its finally block restored the original density; the crash buffer
remained empty.

Build 2506 handles density/screen-layout/smallest-screen-size changes in the
existing Activity. An authoritative runtime event updates the native density
baseline and window geometry together. Insets carry their own density basis,
so a callback from the previous configuration cannot silently scale physical
safe areas using the new density. The current page and editor are retained.
Two focused platform geometry tests, all 408 Home tests and 52 repository tests
passed before packaging.

On the AOSP emulator, the final density fixture passed 25 checks at
420 → 504 → 378 → 420 DPI. It retained the editor identity, exact draft, process,
input focus and visible keyboard. A physical-coordinate tap on Clear search
worked after each change, followed by exact hardware-injected typing. The
original `wm density` state and absent `display_density_forced` row were restored.
The fixture's first build used the wrong button label (“Clear” instead of
“Clear search”); correcting that test did not require a product change. Its
cleanup now preserves absent versus empty secure rows because WMS reset stores
an empty row even when there was originally no override. Restoration uses the
public Settings API with temporary UiAutomation shell permission, avoiding
shell-argument ambiguity for an exact empty string. The same build then passed
the 33-check accessibility smoke and all six cold entries (25 checks). The
independent Google APIs/Gboard emulator also passed the 25-check density fixture
and 33-check smoke, preserving its originally empty density row exactly.

This is a runtime prerequisite for the Display size page; build 2506 does not
yet expose a user-facing density or Night Light control. The original AOSP image
reports Night display unavailable. A temporary, static, test-signed
[resource fixture](../../android/validation-fixtures/night-light-capability/README.md)
enabled the real ColorDisplay service on this owned emulator for the subsequent
control tests, then was removed after build 2508 acceptance. Its initial actual
state was Off at 2850 K. Physical-panel color
accuracy and Lineage LiveDisplay remain separate checks.

## Display controls integration, builds 2507–2508

The owner helper compiled successfully against the pinned LineageOS 22.2 /
Android 15 platform sources. Before installation, all six relevant Java, AIDL
and manifest source hashes matched the isolated build checkout; the resulting
DEX contained the new Display read/write methods and WMS base-density readback.
Only this exported validation APK was re-signed with the public AOSP test key.
Temporary validation package directories were removed from the ROM product
output after capture; this was a module build, not a ROM build or release.

The helper was updated on the owned AOSP emulator with its exact production
privapp permission block, including `CONTROL_DISPLAY_COLOR_TRANSFORMS`.
Its UID, both app-data directory inode IDs and first-install timestamp were
preserved. A full emulator reboot completed with an empty crash buffer. No
physical phone was accessed. The actual Binder boundary fixture passed all
11 checks for an ordinary caller and all 11 for an unrelated caller signed with
the public platform test key, including Display snapshot and mutation denial.

The first UI/backend gates passed 414 Home tests and 54 repository tests. Actual
build 2507 testing then found that hiding the time TextInput directly did not
hide it during a density or warmth review. Build 2508 places it in a conditional
container and clears old editor focus when changing reviews. A rendered
accessibility regression checks editor visibility and stale Save-node denial;
all 415 Home tests passed before final packaging. The initial fixture also
needed the controls' semantic accessibility names, and an observed intervening
pane before same-page external entries to avoid racing their asynchronous
scroll reset. Those harness corrections did not require product changes.

On final build 2508, the actual supported-device fixture passed 120 checks:

- Density Cancel preserves the original value; Save changes 420 to 294 DPI,
  and Default clears the override. Home's process survives both changes.
- Density and warmth reviews expose no time editor. Only schedule editing
  exposes it; Save/Cancel use the real built-in controls.
- Night Light On reaches ColorDisplay and changes SurfaceFlinger's color matrix
  from identity. Warmer changes 2850 to 2750 K; Cooler restores 2850 K; Off
  reaches the real service. This establishes emulator service/compositor
  behavior, not physical-panel color accuracy.
- Custom start `23:01:02` and end `07:05:06` persist with exact seconds. Invalid
  `24:99` disables Save without changing the stored time.
- With Location off, sunset scheduling is disabled and rejects accessibility
  activation without enabling Location. With Location on, Sunset and No
  schedule reach their real Android modes. An actual sunrise/sunset time
  transition still requires a location/time scenario.

ColorDisplay caches its temperature, so successful fixture cleanup restores
warmth through the UI before restoring raw Settings rows. The temporary static
capability overlay was then removed and only the owned emulator was rebooted.
The fixture package is absent, Night display is unavailable again, all seven
Display rows and all 26 tracked general preferences match the original
baselines, and density is 420 with no override. The crash buffer is empty.
The unsupported-device scenario passed another 24 checks: density remains
available through the helper, while all eight Night Light controls are disabled
and reject actions.

The independent ordinary-install Google APIs emulator passed all 50 entry
checks on build 2508: 21 custom routes, 20 standard Android actions, invalid
entry denial and native fallback. Its prior build 2507 passed the 33-check
accessibility smoke. The new Night Display action has a separate manifest
filter so a data update preserves the original 19-action filter on an older
ROM base. The 19 implicit ROM routes were accepted in build 2505; the new Night
Display action's implicit resolution was subsequently accepted on the temporary
privileged build 2510 base below. A complete production ROM boot and physical
device acceptance remain pending.

## Twenty standard ROM entries, build 2510

A temporary read-only bind mount exposed the actual public-test-signed Home2510
APK as a privileged base on the owned AOSP emulator, retaining its installed
data update, UID, both data-directory inode IDs and first-install timestamp.
The actual `rom_defaults` scenario passed 48 checks: all 20 implicit Android
Settings actions reach the intended OctoSense pane, including Night Display;
trusted preferred routing, explicit native recovery and unsupported fallback
work, and the real SystemUI gear opens OctoSense. Reinstalling the data APK
retains priority 2 for the Night Display filter.

The temporary Home mount, exact Home permission block and staging files were
removed. A full emulator reboot restored ordinary Home with preserved app
identity and an empty crash buffer. All 47 tracked preferences match their
original values; density remains 420 with no override. This closes the
20th-action routing gap without claiming a full production ROM boot or custom
Quickstep APK validation. The unsupported emulator may still resolve its sole
Night Display handler to ordinary Home at priority 0; the ordinary main Settings
default and preferred-fallback behavior passed a separate four-check scenario.

## Sound feedback and haptics integration, builds 2509–2510

The owned AOSP emulator reports native Settings intensity granularity `1`, with
Medium defaults for ring, notification, alarm, media and touch in
`dumpsys vibrator_manager`. Its native charging-sound and screen-lock-sound
resource flags and voice capability are true. The keyboard-vibration capability
resource is absent, so that control must remain unsupported. The 14 touched
System/Secure rows were captured before this increment, including absence of
the intensity and hardware-haptic rows and the existing legacy ring value `0`.

The unit gates passed 418 Home tests and 55 repository tests. They cover actual
offered intensity levels, compatible ring/touch writes, partial-write failures,
authority loss, current/custom observations, stable choice identities, held
choice retirement, large fonts and touch scrolling. Review corrected the
observation encoding on devices with two or three levels: their current value
uses Low/Medium/High, while Device default is offered for the one-level case.
The earlier isolated helper compile was stopped and restaged with that correction
before export. The corrected owner helper compiled successfully against the
pinned platform in 5m59s. All six relevant source hashes matched the staged
checkout and the expected DEX markers were present before export. Temporary
validation product outputs were removed after capture. This was a module build,
not a ROM rebuild or release.

The exported validation helper was re-signed with the public AOSP test key and
updated only on the owned AOSP emulator. Its UID, both data-directory inode IDs
and first-install timestamp were preserved. The existing production Agent
permission block was retained; Sound adds no new permission or Binder endpoint.
The reboot completed with an empty crash buffer. The actual caller fixture
passed all 13 denial checks for both an ordinary caller and an unrelated caller
signed with that public platform key, including Sound feedback read/write.

Build 2510 reserves a 14 dp content gutter for the scrollbar and collapses a
duplicate unavailable note. Measured layout tests and screenshots cover normal
and 150% text; a rendered accessibility scroll test reaches all 11 collapsed
rows at both sizes. The final Home suite passed 420 tests. The unchanged Java
backend retains the 55 passing repository tests.

The supported-device acceptance passed 191 checks on build 2510:

- Charging sounds/vibration, screen-lock sounds and dial-pad tones change their
  real stored preferences through the built-in controls.
- The master vibration switch reaches VibratorManager and removes all enabled
  intensity choices while off.
- All five intensity categories change the real service between Off and Medium
  (the observed device default), without inventing unsupported Low/High choices.
- Ring and touch changes maintain the native legacy companion settings; touch
  Off retains the hardware-haptic default as required by the platform.
- Missing keyboard capability remains unavailable. All 14 original preference
  values, including absent rows, are restored, and actual vibrator state returns
  to the original defaults.

Two harness issues were corrected during acceptance. Public Settings.System
writes reject private keys even with adopted shell permission; exact cleanup
now uses the shell settings command with constant keys and prevalidated numeric
or absent values. The earlier cleanup failure was immediately restored and
checked independently. Also, Refresh is enabled while a new snapshot is still
loading, so it is not a readiness marker. The fixture now waits for a real
writable Charging sounds choice before scrolling; the previous early scroll
could race the dependency layout. No production scrolling change was needed.
After final acceptance, all 47 tracked Sound, Display and general preference
comparisons pass, density remains 420 without an override, and the crash buffer
is empty. Physical haptic strength and audible charging playback are not
established by these emulator checks.

The supported AOSP build also passed 29 cold-entry checks and the 33-check
accessibility smoke after Sound acceptance.

The independent ordinary-install Google APIs/Gboard emulator passed all 51
entry checks (22 custom routes and 20 standard actions), 33 accessibility smoke
checks, and 29 cold-entry checks including Sound Back navigation and no replay.
The ordinary installation keeps unsupported mutations disabled. Its font scale
was restored to 1.0 after the 150% layout review. No physical phone was accessed.

## Per-app notifications, Home 2512 and corrected native services

The notification adapter and narrow system-UID broker compile against the pinned
Android platform. The corrected-service build completed in 5m56s; all 59 staged
Java/AIDL/manifest hashes match. Exported validation APKs use only the public
AOSP test signing key. Temporary validation package directories were removed
from the ROM product output after export. Agent and Broker retain their UIDs,
data-directory inode IDs and first-install timestamps on the owned emulator;
there is no new broad Agent permission.

The corrected APKs pass all **17 caller-boundary checks twice**, for an ordinary
caller and an unrelated platform-test-signed caller. Final cleanup confirms all
47 recorded preference comparisons match their original values, density remains
420 without an override, temporary notification/UI fixtures are absent, and the
crash buffer is empty. Agent/Broker data identities remain unchanged.

The accessibility-driven scenario passes **281 checks** with Home 2512 and the
corrected services. Its independently signed public-SDK publisher has 24
channels: 23 in two actual groups and one ungrouped. A separate target-25 app
uses Android's sole default channel. The scenario verifies:

- App permission Off/On, including an earlier USER_FIXED denial, with actual
  permission readback and delivered/blocked synthetic notifications.
- Stable 20/6 paging and channel/group Off/On with real delivery checks.
- Min/Low/High importance, Save/Cancel, disclosed restoration of default sound,
  preserved unrelated channel fields and hexadecimal user-lock bits.
- A deleted channel retires its open review; recreating its ID cannot rebind
  that retired action.
- Both app and channel switches maintain the native legacy permission/channel
  coupling, including restoration of IMPORTANCE_UNSPECIFIED.
- Nested Back preserves the Apps filter. All disposable publishers and the
  UI fixture are removed after acceptance.

Ordinary-install Home 2512 separately passes **24 checks** for unavailable-state
presentation, disabled-action rejection, native recovery and nested Back. The
same Home build passes **29 cold-entry** and **33 accessibility smoke** checks
on the AOSP emulator. New standard per-app intent routing is a later increment;
these results do not claim acceptance of that entry route.

Acceptance found and fixed two product defects. Native notification recovery
previously reused an existing Android Settings task, so Back could land on an
old Sound page. Recovery now pins `com.android.settings` and opens a separate,
excluded-from-recents task. Pinned PreferencesHelper also supplies a synthetic
NotificationChannelGroup(null, null) for ungrouped channels; treating it as an
actual group threw during parent matching. The backend now removes only that
container, retaining every channel from the independent channel inventory.
A real-backend JVM regression covers legacy/mixed inventories and coupled writes;
a temporary pre-fix source copy reproduces the original NullPointerException.

Harness corrections wait for Search acknowledgment, the exact publisher's
filtered catalog and fresh app-detail capability. User-lock flags are read as
hexadecimal from current notification preferences, excluding historical records.
The earlier partial 232-check result and failed legacy runs remain diagnostic
history; the complete 281-check run supersedes them.

## Standard per-app notification entry, Home 2513

Home 2513 adds ACTION_APP_NOTIFICATION_SETTINGS in its own priority-2 filter.
Only a valid package selector is retained; supplied UID, channel, fragment,
route and mutation extras are not authority. The trusted Settings root opens a
cleared loading page and requests correlated current-user app details. Only the
fresh matching response can supply an observed app target. Back or a newer
entry retires pending adoption. Missing apps show disabled app details.

Both API35 emulators pass **33 entry checks**: cold launch and nonreplay,
warm target replacement, missing-target denial, malformed/missing/non-string
selectors, data rejection, ignored mutation extras, custom-route rejection,
and native recovery with an existing Android Settings task. Ordinary Home 2513
also passes the **24-check unavailable** path and **33-check accessibility**
smoke test. All **431 Home tests**, **119 focused Settings tests** and **58
repository tests** pass; changed Java compiles against the pinned API33 SDK.

The dedicated AOSP emulator temporarily scanned the exact signed Home 2513 APK
as a privileged system base while preserving its data update, UID, data-directory
inodes and first-install timestamp. **53 checks pass for all 21 standard Android
actions**, including the real SystemUI Settings gear, preferred destinations,
per-app notification entry and explicitly pinned native recovery. Main Settings,
Night Display and the new per-app filter retain priority 2 after reinstalling the
Home data APK over that base.

The temporary read-only Home mount, permission allowlist and helper files were
then removed and the owned emulator rebooted. Ordinary-install native fallback
passes **5 checks**, cold-entry nonreplay passes **29**, and accessibility smoke
passes **33**. All 47 recorded preference comparisons still match, density is
420 without an override, the crash buffer is empty, and helper data identities
are unchanged. This is emulator integration evidence; it does not replace a
complete production-ROM boot or hardware acceptance. No physical phone was used.

The app-list fixture permits one navigation retry only when a filtered catalog
has actually retired the clicked semantic node and a newly observed node for
the same exact package appears. It never retries a current node or any mutation;
retries are reported explicitly. The ordinary unavailable path passes with that
synchronization and no additional product changes.

## Default-app adapter and native selection, Home 2515

Default-app work uses the disposable `OctoSense_ROM_Roles_API35` clone on
`emulator-5560`. The accepted Home2513/notification baseline on `emulator-5558`
is preserved; physical phones are excluded. The clone retains the original
controller UID, data-directory inodes, first-install timestamp and all 40 role
holder records. Installing the exact original controller APK as a data update
was verified before installing any new controller code.

The pinned PermissionController role adapter compiles against the real
`PermissionController-lib` and `system_current`, with the native library used
only as a compile dependency. Its temporary build module is removed afterward;
the pinned Permission checkout remains clean. The production stager's read-only
check passes for all 7 proposed files against revision
`12d670229861f4ec3289418128589d751ac20c8c`.

For emulator validation, `build-permissioncontroller-validation-apk.py` adds
only the adapter DEX and the two declared manifest components to the exact
original `AE3A.240806.019` controller APK. It preserves all 3,154 original payloads
(including resources and runtime metadata) and all 10,089 native classes. The 26
added classes belong only to the adapter/contract namespaces; no native or stub
classes are duplicated. Existing binary-XML nodes and their string/resource
indices are retained, and the new APK uses only the public AOSP emulator test
key. This avoids replacing the emulator's unrelated controller implementation
with a different ROM/QPR build. It is not a production APK delivery mechanism.

Installing that validation APK preserves controller identity/data and all 40
role assignments; the crash buffer remains empty. **20 caller-denial checks
pass for both an ordinary caller and an unrelated platform-test-signed caller**,
including both adapter transactions and the unexported confirmation Activity.
The latter caller holds the signature permission, so this also exercises the
service's exact-package gate. With the Agent's appended role transactions,
**22 caller-denial checks pass for each identity**, including the helper's
snapshot/confirmation methods. No broad role-management permission was added
to Home or Agent.

The complete Home 2515 picker/native-consent run passes **83 checks**. Two
independently signed public-SDK browser fixtures qualify through native role
policy. Native Cancel and Back preserve the current holder; positive consent
selects each fixture and its own public `RoleManager.isRoleHeld` confirms the
actual assignment. Removing and reinstalling a candidate while its confirmation
is open invalidates that review. A separate public `ACTION_ASSIST` fixture also
qualifies and observes its actual Assistant role. The native Assistant **None**
choice clears that holder. Browser does not offer None on this image; that
capability is never invented by the built-in picker.

The run also verifies selected-choice disablement, explicit native recovery with
an older controller task present, and nested Back preserving the Apps filter.
All 40 role holder records and the three legacy Assistant preference rows are
restored. All three role fixtures and the temporary reinstall APK are removed.
On the ordinary Google APIs emulator, unavailable-service recovery passes
**11 checks**, general entries **52**, cold entries **29**, and accessibility
smoke **33**. Recovery accepts only the exact enabled system DefaultAppListActivity
in the AOSP or Google controller package: the legitimate Google module has its
own signing identity, so equality with the framework signer is not required for
this read-only navigation.

The first cloned-emulator attempt timed out during initial UI startup while
the cloned GPU cache was recompiling shaders. A later attempt exposed a fixture
label mismatch: native labels are "Default browser app", not "Browser". The
fixture now uses actual platform labels. The completed 83-check run supersedes
those partial attempts; no startup-performance fix is claimed from them.

## Standard Default Apps entry, Home 2516

Home 2516 registers `MANAGE_DEFAULT_APPS_SETTINGS` in a separate priority-3
filter. It routes only to the default-app overview. Role, package, user, UID,
target, key, requested-route and mutation extras confer no authority. Both
emulators pass **11 entry checks** for cold/warm entry, nonreplay, malformed
data/type rejection and unchanged role holders. The ordinary Google emulator
also passes **53 general-entry**, **11 unavailable-service** and **7 native
fallback** checks.

The disposable AOSP emulator verifies compatibility with both ROM bases. When
Home 2516 is a data update over the older privileged Home 2513 base, Android caps
the new filter at priority 0 and its native controller remains preferred at 2.
After scanning the exact signed Home 2516 privileged base, the new filter gets
priority 3 and retains it after reinstalling the data APK. The trusted preferred
route additionally requires explicit Default Apps metadata on the system alias.
The full ROM-routing scenario passes **60 checks for all 22 standard actions**,
including the real SystemUI Settings gear and native recovery Back.

The temporary privileged Home mount, allowlist and helper files are removed,
and the disposable emulator is fully rebooted. Final Home 2516 ordinary-install
fallback passes **7 checks**, cold entry/nonreplay **29**, and accessibility
smoke **33**. All 47 original preference comparisons, all 40 role holder records,
the three legacy Assistant rows, density 420 and Home/controller/helper data
identities are preserved; the crash buffer is empty. No physical phone was used.
This proves the tested emulator integration, not full Settings parity or a
production-ROM/hardware boot.

## Runtime permissions: validation preparation

The next increment uses the same disposable `emulator-5560`; the original
baseline emulator and physical phones remain untouched. The independently signed
target-35 and target-22 permission fixtures compile against the public SDK and
report only their own permission grant bits and raw AppOp modes. Installation
preflight passes: modern dangerous permissions start denied, while legacy
Camera/Microphone/Contacts start granted with `REVIEW_REQUIRED`. Both fixtures
are removed after preflight.

A direct native Android App info → Permissions → Camera run establishes the
legacy reference behavior. Its visible old-Android warning precedes denial.
Positive confirmation leaves the three public Camera grant observations true,
changes its raw AppOp from foreground-only (4) to ignored (1), sets `USER_SET`
and `REVOKED_COMPAT`, and clears `REVIEW_REQUIRED`. Actual denial cannot be
verified from PackageManager's grant bit alone. No real sensor data is accessed.

An earlier shell shortcut to the native permission Activity omitted its required
Parcelable user handle and crashed the original native fragment at
`KotlinUtils.getPackageLabel(user=null)`. That is a preflight harness error:
the new permission adapter was not installed. The crash was archived separately,
the dialog and invalid task were removed, and the corrected navigation above
passed. Subsequent baseline verification preserves all 47 preferences, 40 role
records, controller/helper identity and an empty crash buffer. These setup checks
do not yet establish built-in permission-editing acceptance.

Home2517, the Agent's finite permission transactions and the controller adapter
compile against the pinned platform. The validation controller preserves all
3,154 original APK payloads and 10,089 native classes, adding only 65 adapter
classes and the four finite role/permission components. Installation preserves
the controller's UID, data directories, first-install identity and all 40 roles.
Both an unrelated ordinary signer and an unrelated platform-test signer fail
all **27 caller-boundary probes** as required; these include the controller and
Agent permission methods and the unexported operation Activity.

The first actual built-in permission run reaches the Camera group but reports
unavailable instead of choices. A second diagnostic run reproduces this after
22 checks, with no grant mutation. Both runs remove their synthetic packages
and preserve all **906 existing grant/flag records across 183 packages**. The
failure and build/boundary receipts are retained under
`out/home/settings-runtime-permissions/validation/`; permission editing is still
pending runtime acceptance. Native model loading is being diagnosed on the
disposable emulator only.

A metadata-only diagnostic controller reproduces the cause: the native group
inventory is fresh, and the button callback emits all nine native button states,
but its aggregate `isStale` remains true. Android's static rationale LiveData
causes that flag to remain set for these groups; the native fragment consumes
the nonnull button result. OctoSense's additional blanket button-staleness gate
discards it and reaches the five-second timeout. The trace contains only session
counters, readiness flags and button counts. This diagnostic changes no grant
behavior and again preserves all 906 existing records. The production correction
and its mutation acceptance are tracked separately from this reproduced failure.

The corrected controller requires fresh inventory and a newly emitted nonnull
native button map. Its real-source regression fails before the correction and
passes afterward, including uninitialized data, stale inventory and observer
cleanup after interruption/timeout. The pinned platform rebuild and 13-file
production stager check pass. Home2519 also fixes accessibility acknowledgment
ordering: successful actions publish their drawn tree before acknowledgment.
This removes the reproduced Apps Search → existing-row race. Ordinary5556
passes permission-unavailable/recovery **25**, cold entry **29** across seven
launches, and smoke **33**;5560 also passes smoke **33**.

On5560, one full corrected permission run passes **463 checks**: nine common
groups, Ask versus Deny flags, precise/approximate/background location, legacy
warning Cancel/Back/rotation/positive confirmation, native AppOp compatibility,
and rejection of an old warning after reinstall. All 906 unrelated permission
records remain unchanged. An earlier run fails after 413 checks when a legacy
warning's next action is rejected as stale. That intermittent return/observation
race remains open; the successful run does not close it. Both receipts are kept,
and the focused `permissions_legacy` scenario repeats the same warning cases
with the same fixture-only mutations and cleanup. No phone is used.

Home2520 adds temporary metadata-only diagnostics. A focused run reproduces
the original rejection after rotation restoration: Rust still considers
snapshot216 actionable, but Java observes `resumed=true, focused=false` and
rejects the request before obtaining a native token. The 51-check focused
failure matches the earlier 413-check full-run phase. The final native focus
guard correctly rejects the request; the trace does not show a reused native
ticket. Another focused run shows the
related return-read problem: a read starts before the resumed window gains
focus, reports restricted, and is not refreshed for five seconds. Both runs
preserve all 906 unrelated grant/flag records. Home2521 retires permission
authority on focus loss and refreshes after focus returns. Native focus checks
remain enforced and no mutation is retried. The trace also
exposes incorrect generic “Setting applied” copy for successfully opened native
flows; Home2521 reports the native handoff without claiming the setting changed.

Home2521 passes451 Rust and63 repository checks. Ordinary5556 passes
permission-unavailable/recovery25, cold-entry29 and accessibility/IME33.
A focused privileged run still reproduces the51-check rotation case. This
reveals an additional harness synchronization defect: UiAutomation's rotation
request returns before the display/configuration transition finishes. Checking
the stored rotation preference does not prove that Home has regained focus in
the restored orientation. The fixture now waits for the actual display rotation
and a newly focused native warning window, proving warning recreation. After
Cancel and restoration it waits for the original rotation and a focused Home
whose laid-out orientation matches the display. It never retries a mutation or
weakens the production focus gate. Three consecutive corrected focused runs
each pass104 checks and preserve all906 unrelated grant/flag records.

The full Home2521 run passes521 checks across all11 common groups: Camera,
Microphone, Location, Contacts, Calendar, Phone, Call logs, SMS, Nearby devices,
Physical activity and Body sensors. It verifies actual grant bits and AppOps,
Ask versus Deny flags, precise/approximate/background Location, native recovery,
nested Back and retained filter, plus all legacy warning and stale-reinstall
cases. Call-log and SMS permissions have Android's normal ADB-install
`RESTRICTION_INSTALLER_EXEMPT` flag; the fixture does not alter restriction flags
or assign roles to make those groups grantable. All906 existing grant/flag
records across183 packages and all40 role assignments are preserved. Receipts,
including the initial2521 failure, are retained under
`out/home/settings-permission-focus/validation/`.

This accepts the common permission editor on the disposable emulator. Selected
photos/media, Health Connect, virtual-device permissions, special access and
unused-app revocation retain their separate native flows and acceptance work.
The production ROM and physical-device integration are not established by these
tests.

## Do Not Disturb: independent observer preparation

The public-SDK `dnd-observer` fixture passes its read-only preflight on5560.
Under temporary instrumentation shell identity it reads the native default and
consolidated notification policies and both existing automatic rules. It retains
the complete Parcelable encoding as well as the visible fields, so subsequent
UI acceptance can detect lost native fields. It contains no policy/rule setter.

With its own temporary notification permission and listener grant, six synthetic
notification categories all report `matchesInterruptionFilter=true` under the
existing Off mode. None bypass DND or are suspended. The listener only reports
this fixture's notices; call/message categories do not place calls or send texts.
The policy and complete rule records remain exactly unchanged before and after
the probe. The listener grant is removed, its previous raw setting restored by
normal grant removal, and the fixture package uninstalled. Receipts live under
`out/home/settings-dnd-preparation/validation/`. This establishes an independent
observation mechanism, not acceptance of the upcoming DND policy/schedule UI.

A nested observer check also passes6 checks while the Settings fixture retains
its UiAutomation connection. Native observation uses that outer connection;
the synthetic publisher's ranking-only operation never creates a second one.
An initial harness run expected the wrong overview title (`Settings` instead of
`OctoSense Settings`) and failed before any UI mutation. Its immediate cleanup
check also recorded an unequal raw listener setting, without recording the two
values. The corrected runner captures both values, waits for asynchronous grant
removal, and restores only representation differences after the actual component
sets match. The corrected run preserves the exact empty listener setting and
complete native policy/rules, and removes both fixture packages. The failed and
corrected receipts are retained together.

The frozen DND Agent and broker compile against the pinned platform in6m08s
(56 Agent inputs,18 broker inputs; invocation
`74f1b8b0912849c6a52e0a4fdedb15e8`). Both explicit build-success markers and the
frozen input hashes are verified. The broker installs as a data update. Android
rejects a data update of the persistent Agent, so its system-ext APK is replaced
atomically and only5560 is restarted. A subsequent verification confirms both
installed APK hashes, all four package/data identities,40 role assignments,
906 grant/flag records,47 stored preferences and an empty crash buffer.

Both the ordinary signer and an unrelated AOSP platform-test signer pass37
caller-boundary denials each. The latter holds the signature permission, so
the ten added checks exercise the broker's five DND methods and the Agent's
five methods against their exact-package caller checks. Native reads and
synthetic ranking still preserve complete policy/rules and listener state
after installation. These native/fixture checks do not yet accept the built-in
policy or schedule editor.

The first actual policy run on Home 2522 failed before changing policy or rules.
The broker called `ZenRule.isActive()`, which exists in the pinned Lineage source
but not in this AOSP emulator's framework. The resulting `NoSuchMethodError`
escaped its exception boundary. The failed receipt and crash log are preserved;
the runner confirms unchanged complete native policy/rules, listener state and
fixture removal.

Inspection of the emulator's actual framework DEX verifies the older native
`isAutomaticActive()` method. The corrected adapter selects one of these two
native methods and reports unknown status if neither is available; it does not
reimplement Android's condition/snoozing logic. The exact-caller check remains
outside the service's exception/linkage-error recovery boundary. Both native
method shapes, missing dependencies and propagation of VM errors have regression
coverage. The broker rebuild passes in 5m59s with 19 frozen inputs (invocation
`ffc76a3bca1445c6a746cca8e21b048c`). The emulator's actual flags are Modes API on,
Modes UI off. Framework descriptors, flags and fingerprint/hash evidence are
retained with the failed run under `out/home/settings-dnd/validation/`.

Home 2523 also fixes two navigation issues found during ordinary-install testing:
the standard DND action now opens its own page, and native recovery uses a separate
task so Back returns even when an older Android Settings task exists. The ordinary
emulator passes 77 unavailable/recovery checks, 15 dedicated-entry checks,
33 cold-entry checks across eight launches, and 33 accessibility/input checks.
The source suites pass 457 Rust and 65 repository tests. These checks do not yet
accept privileged policy editing or Android's scheduled activation/exit.

The corrected broker installs as a data update, preserving four package/data
identities, all 40 role assignments and 906 grant/flag records. The two caller
boundary variants still reject all 37 calls each. The old crash buffer is archived
before starting the corrected-run baseline.

The first 2523 policy run reveals a fixture synchronization error: immediately
searching by scrolling after a mutation can move past the clicked row before its
selected label arrives. The fixture now waits in place for actual readback. That
run also changes native visual flags from 157 to 159 on the first policy write.
NMS reconstructs the deprecated `SUPPRESSED_EFFECT_SCREEN_ON` bit from the already
present `PEEK` bit. A read-only program running against the emulator's framework
confirms both policies compare equal with native `Policy.equals`, in both
directions; changing the real badge-suppression flag correctly compares unequal.
All rule records, categories, senders and the interruption filter are unchanged
after cleanup. This is semantic preservation with a recorded native encoding
normalization, not byte-for-byte restoration to the pre-write value 157.

A second run reaches 610 checks and exercises all nine fields before exposing
another fixture timing error: the manual-mode page was visible before its separate
capability observation was ready. The test now waits for that observation before
clicking. This run restores the complete policy/rule encoding exactly to its
captured baseline (159), removes fixtures and preserves listener state. Both
failed receipts remain under `out/home/settings-dnd-entry/validation/`.

The subsequent run reaches the manual-mode operation and uncovers a product
regression: the new DND page leaves the selected mode actionable, although the
older Notifications controls disable it. Native mode changes succeed, but the
test correctly rejects the still-clickable selected button. Its cleanup hits
the same issue. A finite recovery fixture restores all nine captured policy
choices through the actual UI and verifies exact native baseline equality
(122 checks); the mode was already Off and rules were unchanged.

Home 2524 disables the selected mode and rejects a forged/repeated action at the
view handler. The regression fails on 2523 source and passes with the change.
The full Rust suite passes 458 tests; the repository Settings suite passes 66.

The first schedule run also finds a distinct platform compatibility issue.
The emulator's actual `ZenModeConfig.getScheduleConditionProvider()` returns
`android/ScheduleConditionProvider`, while NMS reports stored rules with owner
`android/com.android.server.notification.ScheduleConditionProvider`. The new
rule is created but fails the original strict owner-equality readback, leaving
the draft unsaved and the reported rule read-only. The recorded failed run stops
at 51 checks. A narrowly scoped cleanup removes only that new, disabled synthetic
rule after comparing its full captured Parcel, and verifies the exact pre-run
policy/rule baseline (8 checks). It does not count as acceptance of UI deletion.

The correction recognizes both verified Android class spellings, requires both
the rule package and owner package to remain `android`, and still requires the
native schedule parser and supported type. Regression checks reject foreign
packages, similar class names, nulls and the event provider. The new broker build
and actual schedule UI acceptance are pending. All failed receipts and recovery
results remain available alongside the corrected runs.

Home 2524 with the compatible broker now passes the complete policy acceptance:
837 checks cover every finite choice in all nine policy fields, selected-choice
denial, actual native readback and preservation of unrelated fields/rules.
Independent ranking of six synthetic notification categories verifies Priority
blocking, then allowing alarms, reminders, events, messages from anyone and calls
from anyone individually. These are local notifications, not real calls, messages
or audible-alarm tests. Complete policy/rule encodings (including visual flags
159) and listener state return exactly to the captured baseline; fixture packages
are removed. Source and receipts are under
`out/home/settings-dnd-selection/validation/`. Contact affinity, repeated-call
history, important conversations and physical effects remain separate checks.

The provider-compatible broker passes its pinned build in 6m01s (20 frozen
inputs, invocation `d292a9926eba4b0085b1c3750859e26b`). Installing it preserves
the four package/data identities, all 40 roles and 906 grant/flag records, with no
new crash. Home 2524 and this broker pass 210 schedule acceptance checks:
invalid time rejection; creation; exact owner/condition readback; edit/Cancel;
delete review/Cancel; renamed and equal-endpoint schedules; preservation of
unreviewed native fields; enable/disable; empty-day safety; confirmed deletion;
and a retained draft with an invalidated Save after external removal of its
disabled synthetic rule.

The scheduled-start observation is 181 ms after its target minute, and the end
observation is 376 ms after its target minute. Home is force-stopped before both
and remains absent throughout; Android's native condition provider owns both
transitions. These are observation latencies from one run, not timing guarantees.
The complete original policy/rule encoding and listener setting are restored,
and both fixture packages are removed. Exact epochs and before/after records
are retained under `out/home/settings-dnd-selection/validation/`.

ROM-style routing passes 64 checks on each of two temporary system-base states:
Home 2522 as the base with active Home 2524 as a data update, then a refreshed
2524 base with that same data update. Both runs verify all 22 standard actions,
preferred DND navigation, native DND recovery, unsupported-route fallback and
the actual SystemUI gear. DND retains priority 2. The temporary system mount,
permission XML and helper are removed, and only the disposable emulator is
rebooted. Home's data APK, UID, data-directory inodes and first-install time are
preserved. Ordinary-install fallback then passes 10 checks; native DND resumes
as the ordinary emulator's default handler. This is an updated-system routing
fixture, not a complete production ROM boot or phone deployment.

After routing cleanup, Home 2524 passes 33 cold-entry checks across eight fresh
starts and 33 accessibility/input checks. Both ordinary and unrelated platform
signers are still denied by all 37 caller checks each. Final verification
preserves all four package/data identities, 40 role assignments, 906 grant/flag
records, 47 stored preferences, three Assistant settings and density 420; test
packages are absent and the crash buffer is empty.

The final independent observer preserves its complete before/after state. Across
the emulator reboot, the stored/default policy and every automatic rule are
byte-for-byte unchanged, while Android recomputes the consolidated policy's legacy
visual bits from 159 to 157. This is the reverse of the previously audited
normalization; the captured records differ only in that field and its Parcel
encoding. A separate native `Policy.equals` comparison verifies semantic equality.
The final record therefore distinguishes exact stored-policy/rule preservation
from semantic preservation of the recomputed consolidated policy.

## Per-app network and battery policy, Home 2525

The pinned Agent and Broker build passed in 5m58s, invocation
`454f6f24668d481ca12362574d73d205`, with 64 frozen Agent and 30 Broker inputs.
Home 2525 passed 467 Rust tests and 69 repository tests. Installing the helpers
and Home on disposable emulator5560 preserved four package/data identities,
40 default-role records and all 906 original grant/flag records. Only that
emulator restarted for its persistent Agent update; no phone was accessed.

The per-app network screen passes 68 checks against the real native UID policy:
background restrictions, Data Saver exceptions and coupled-bit behavior;
independent native changes and Refresh; persistence after stopping/restarting
Home; protected-host denial; nested Back and retained app filter. Native
NetworkPolicyManager reports the synthetic background UID as `USER_RESTRICTED`
on metered networks. This is policy-enforcement state, not an actual packet or
throughput measurement. All four Lineage-only restriction controls are disabled
on this AOSP image. Every original UID policy and global Data Saver setting is
preserved and the fixture is removed. The earlier 63-check run stopped on a
fixture navigation race after asynchronous app details pushed a button out of
the viewport; the corrected probe waits for the enabled visible destination.

Both ordinary and unrelated platform-signed callers pass all 45 denial checks
each, including the new Agent69–72 and two finite Broker services. Ordinary Home
on emulator5556 passes battery-unavailable24, network-unavailable27, cold33
across eight launches, and accessibility/IME33. The crash buffer is empty.
Service-enabled emulator5560 also passes cold33 across eight launches and
accessibility/IME33 after the policy tests and caller-boundary probes.

Privileged battery acceptance is still pending. Its first run stopped at19
because the probe queued another review before Cancel had finished. Waiting
for review closure resolves that fixture race. The next run reaches48 and
finds an actual adapter bug: `checkOpNoThrow` translates stored
`MODE_FOREGROUND` to `MODE_IGNORED` for a background package, causing the UI to
show Restricted instead of Custom Android state. The corrected native adapter
uses `unsafeCheckOpRawNoThrow` for the two policy AppOps; build and emulator
acceptance of that correction are recorded separately. Both failed probes
remove all synthetic packages and preserve the original power allowlist and
both background AppOps inventories. Failed receipts are retained alongside
successful results under `out/home/settings-app-policy/validation/`.

The stored-AppOp correction passes the pinned Broker build in 3m51s, invocation
`31790c735ccc4f959dfdb5dc757bdd45`. Its emulator APK SHA-256 is
`383976220bff20ef1a5554a6e744518c70839320db08503388fd3687e5b20d68`.
Installing that Broker data update requires no reboot and preserves all four
package/data identities, 40 roles and 906 grant/flag records. Home 2525 then
passes all 111 privileged battery checks: actual AppOps and allowlist readback
for all three modes, review/Cancel, custom stored-mode observation and repair,
independent-change stale-review denial, persistence after Home restart, pre-O
AppOp coupling, and protected/shared-UID denial. The original power allowlist
and both background AppOps inventories remain exact and every temporary package
is removed. These checks establish policy behavior, not battery consumption or
actual Doze/job delivery on hardware.

## Per-app storage, Home 2526

The built-in App storage & cache page and finite asynchronous Broker API are
integrated. Home 2526 builds successfully, 472 Rust tests with `mobile-apps`
pass, and 70 repository tests pass. The new network/battery/storage pages are
included in narrow-column/150% font and real button-start swipe tests. Storage
reviews, live themes, stale-target denial and pending-completion behavior have
dedicated UI tests. A focused host regression also verifies that an accepted
clear request and an app-owned Manage space handoff never say “Setting applied”.

Two initial storage test failures came from a JSON fixture helper replacing a
missing `operation` field. Adding the fixture's initial null field fixes both;
no production behavior changed. The failed run is retained.

The pinned native build passes in 6m00s, invocation
`6ae0ad4b760c421f83e0453f0660b2a2`, with 68 frozen Agent and 35 Broker inputs.
The emulator-test Agent SHA-256 is
`aeecec05dcce0f4e0cecc57582dd908045591f7c276e179cba634a5222e4bd63`;
Broker SHA-256 is
`3a2d19d0e1cbc048fd1582f33ac9588ced00cc8d2a3b90c164e702e62a377724`.
Installing them and restarting only disposable emulator5560 preserves four
package/data identities, all 40 roles and 906 original grant/flag records.
No crash is observed. Agent73–74 and the new Broker interface bring caller
denial checks to 49 for both ordinary and unrelated platform signers; all pass.

Privileged UI acceptance remains in progress. The first storage probe stops
at4 during fixture preparation, before any Settings clear action. Starting
another instrumentation process retires the synthetic URI grant being measured.
The fixture transport is being changed to finite shell-gated ordered broadcasts
so observation does not force-stop the test apps. The original URI-preservation
and clear-data reset assertions remain. All temporary packages are removed and
all 906 original grant/flag records are preserved. Receipts are stored under
`out/home/settings-app-storage/validation/`; this is not a production ROM or
physical-phone storage-clear test.

## Remaining integration checks

- Cold startup of build 2429 lost the Activity resume event before the native
  render queue existed. Search opened the keyboard but dropped input, and
  Location remained waiting indefinitely. Opening native Settings and returning
  restored both. Build 2430 retains and reconciles Activity state after native
  Startup. Cold Location now loads without that detour. On the AOSP emulator,
  nine immediate Ctrl+A Search replacements and twelve immediate Clear/typing
  runs passed; Back stayed hidden beyond the next refresh. The clock-editor
  checks above passed with this runtime as well. The Google APIs/Gboard results
  and remaining protocol caveats are recorded in the runtime validation notes.
- Bluetooth adapter, rename, Cancel, discovery start and stop passed with the
  staged runtime grants. Pairing and physical profiles still require hardware.
- The early Search/Clear race is resolved in build 2436. The final runtime
  accepts every current-editor InputConnection in the active Surface cohort,
  while rejecting closed or retired sessions. Both emulators passed cold typing,
  immediate Clear/Ctrl+A replacement, Back/retap and native-Activity return.
  The runtime patch and its regression/acceptance record retain the earlier
  failures; they are not outstanding generic input work.
- Validate fresh-ROM default grants, production SELinux policy, release signing,
  hardware behavior, accessibility and the remaining parity ledger on the actual
  ROM. The AOSP image lacks the ROM's update-engine policy and is not an OTA test.


### Storage revision and color controls, build 2026092528 (acceptance in progress)

The first extended storage run passed the actual file, permission, channel, job and URI resets, then failed check 70 because the fixture treated an extant PendingIntent token as a scheduled alarm. The assertion now checks AlarmManager’s actual bounded pending-alarm store. A subsequent run on Home2527 failed check39 before mutation: normal cache-size changes invalidated the reviewed key. The native Broker revision now binds the app incarnation, UID membership, policy, manage-space target, stats availability and action eligibility rather than volatile byte counts. Reinstallation still retires a review. A deterministic synthetic cache-growth action now occurs while confirmation is open. These failed receipts are retained; they are not acceptance passes.

Build2528 adds System → Accessibility: colors, a custom navigation-only entry and English/Chinese search aliases. Shared controls render four explicit color-correction modes while retaining independent enable switches. Standard Android accessibility actions remain pinned to native Settings. The queued storage status now says that Android reports completion below, avoiding a stale “waiting” message after the operation completes.

The complete Home mobile-apps suite passes476 tests, including actual color widget dispatch, saved-mode changes while correction is Off, stale held choices, live theme,150% text and narrow layout. The focused Java/backend/routing run passes13 tests. API35 Home packaging succeeds. Native storage/vision acceptance is still pending at this checkpoint; no OnePlus or other physical device was touched.


### Storage2528 and vision2529 acceptance

Storage2528 passes129 native checks, including deliberate cache growth while confirmation is open, actual cache-only preservation and clear-data Android resets, stale package replacement denial, declared manage-space Back and protected Home. All906 pre-existing permission grant/flag records remain unchanged after fixture removal. See `out/home/settings-storage-vision/validation/storage2528-acceptance.json`.

Vision2529 passes106 checks on5560 with actual SurfaceFlinger transform changes for all four correction modes, inversion and composition, selection while correction is disabled, Home restart, external edits and unknown raw-state denial. All three raw preferences (including absence) and the original compositor matrix were restored. The earlier2528 run stopped at94 because it attempted Refresh while the control was disabled during a pending observation; the corrected fixture waits for an enabled control before its single click.

Ordinary5556 exposed a real native recovery defect: Back returned to a pre-existing Sound Settings task. The scoped Accessibility fallback now uses a dedicated native recovery task. Build2529 passes18 ordinary unavailable/recovery checks, preserving all color preferences. Both emulators pass33 cold-entry checks/eight launches and33 accessibility/IME checks. Routing regression passes2 tests. Home remains an ordinary data APK; these records do not claim a complete ROM boot or phone acceptance. Receipts live in `out/home/settings-vision-recovery/validation`.


### Per-app language, Home 2026092530

The built-in picker is accepted on disposable emulator-5560 with 141 UI checks.
A translated fixture confirms real French, simplified Chinese and traditional
Chinese resource changes and Activity recreation. Tests cover System default,
complete ordered custom LocaleLists, filtered branch Back, restart persistence,
runtime LocaleConfig changes, app reinstall retirement, protected platform apps
and exact native-picker recovery after a different Settings task was open.
The original 906 grant/flag records remain unchanged. Ordinary emulator-5556
passes 27 unavailable/recovery checks; both emulators pass cold33 and smoke33.
Host tests pass482 and the focused repository checks pass17. Both ordinary and
platform-signed impostor packages are denied by all53 caller checks each.

Two initial fixture assertions were corrected: the Chinese fixture's actual
translation text, and the native collector retaining a French region branch
when the runtime config lists fr-FR. These were test assumptions, not language
setter failures. Their failed receipts are retained. A separate native Broker
label fix removes “Unknown language” from the System default sentinel; its
follow-up inspection passes14. See `out/home/settings-app-language/validation`.
No physical phone was accessed. Arabic resource preflight is independent of
picker acceptance; numbering variants and physical/full-ROM behavior remain
explicitly unproven by these emulator tests.

## Hearing controls: Home2532

The built-in hearing page passed306 native checks on disposable5560 and30
ordinary unavailable/recovery checks on5556. Both passed cold33/eight launches
and accessibility smoke33. Native AudioFlinger mono/balance and active speaker
mixer gains were verified with silent synthetic playback; the independent
caption app observed callbacks and actual text visibility/size/colors for all
five sizes and six presets. Custom raw fields, unknown-state denial, stale
accessible actions and restart persistence passed. All12 raw preference rows
were restored exactly; unrelated stream volume and DND state were unchanged.
All4 package identities,40 roles and906 existing grants/flags were preserved;
fixtures were removed and the crash buffer was empty. No physical device was
touched. Receipts and accepted Agent APK are in
`out/home/settings-hearing-coupled/validation`; see the
[hearing audit](settings-accessibility-hearing-audit.md) for test corrections,
frozen source evidence and explicit hardware/custom-editor/locale limits.


## Caption appearance and language —2533

Home2026092533 and native Agent SHA256
`a2632bf67a7867cb6c3b0ff25e347313045a1d70a59ad93ca273c28ddd23f8a7`
passed on disposable5560:86 caption-language checks,316 custom-appearance checks,
58 denial checks for each of two caller signing identities,41 checks over10 cold
launches and33 smoke checks. Ordinary5556 passed23 language-unavailable,38 custom-
unavailable,41 cold and33 smoke checks. Source suites passed501 Rust/mobile-app
and22 Java contract tests. The pinned native module compiled in6m08s.

Independent CaptioningManager consumers verified real language/style callbacks
and native typefaces/packed colors. Original preferences/audio state were
restored exactly;4 package identities,40 roles and906 grants/flags were preserved.
Fixtures were removed, crash buffers empty, and no physical phone was touched.
Accepted logs, initial fixture failures, frozen source/build inputs and installed
artifacts are retained in `out/home/settings-caption-editors/validation/`.
See the hearing audit for the three fixture synchronization corrections and
remaining custom preview/hardware limitations. Full Settings replacement remains
in progress.

## Text and interaction —2535, startup follow-up2536

Home2026092535 plus Agent SHA256
`f187f7bcd862c01b1f83021e24615d73d529f74e9433492bfd4af8f0e40f330b`
passed555 native checks on disposable5560 and27 ordinary unavailable/recovery
checks on5556. Native checks covered every offered choice and independent Android
consumers: TextView paint weight, long presses at400/1000/1500ms, short taps,
both recommended timeouts, actual WindowManager animation scales and automatic
click through a temporary kernel mouse. Only mouse motion was injected; Android
had to generate the click. Both caller signing identities passed60 denials each.
The final2535 full Home regression passed510 tests and the focused text renderer
regression passed. All11 original raw preferences,4 package identities,40 roles
and906 grant/flag records were preserved. Fixtures and the kernel mouse were
removed; the crash buffer was empty. Bold/contrast screenshots changed in place
and restored a byte-identical image with the same process/Activity.

Ordinary5556 passed45 cold-entry checks/11 launches and33 smoke checks after cache
warm-up. **Privileged2535 cold navigation is not accepted:** it passed the first
three launches/12 checks, then Search exceeded its10s deadline. The page appeared
later; logs showed50 rejected GL program binaries. Earlier2534/2535 first-launch
failures remain archived as well. An independent public GLES30 test reproduced
export-success/import-success/error0 but LINK_STATUS0 for a freshly exported
program in the same emulator context. Temporarily setting aside only Makepad's
shader files reached Display in4.668s, then restored all58 original shader files.
This test retained the host driver's cache, so it does not establish a completely
uncached driver launch. These are startup limitations, not passed navigation.

The2536 candidate detects whether a driver can reload a freshly exported program
before continuing cache reads/writes. A rejected old binary triggers validation;
it does not permanently disable a working driver after a driver update. The
source-linked program remains active. Validation results will be recorded after
the candidate is built and installed. Its isolated source capture contains the
same284 product inputs as2535 with only the runtime patch and its lock changed.
System Languages work is excluded from that candidate. See
`out/home/settings-text-interaction-r2/validation` and the
[text/interaction audit](settings-accessibility-text-motor-audit.md).

The2536 isolated packaging attempt omitted AndroidManifest.xml.template and was
rejected before Home launch; no crash occurred. Corrected2537 includes the same
manifest as2535 (apart from version/build metadata), passes45 privileged cold
checks/11 launches and33 smoke checks, and preserves the byte-identical restored
text screenshot. With app shader files absent, Display was ready in4.763s and no
unusable cache files were created; all58 originals were restored. Four package
identities,40 roles,906 grants/flags and11 raw preference rows remain unchanged.
Artifacts/receipts: `out/home/settings-renderer-cache-r2/validation`.


## Ordered system languages —2539

Home2026092539 plus System Languages Agent
`facfb9ed7e29131b1bde2561d19ac6406659031d6fcd97ff42ca4a8ceb442eb0`
and Broker r2
`e819d8f552d03c0d18d0163149569a30305995471ee3de8aad95c367506db96d`
passed248 native language checks on disposable5560. The fixture changed languages
through the built-in reviewed UI and observed actual Android configuration and
independent rendered French, Simplified Chinese and Arabic resources. Arabic
resolved to RTL. All seven applied language changes preserved both Home's PID
and its actual Java Activity identity. Review/Cancel, order/removal, restart,
regional Unicode preferences and stale-review denial passed. No numbering leaf
was selected on this emulator; that hierarchy has native-adapter/unit coverage,
not a claimed device result.

The exact original en-US configuration, userSetLocale=false and absent locale
provider rows were restored. Four package identities,40 roles,906 grants/flags
and11 accessibility preferences remained unchanged; fixtures were removed and
the crash buffer was empty. The same candidate passed64 caller denials per
signing identity,49 cold checks/12 launches,60 entry checks (30 custom routes and
21 generic standard intents) and33 accessibility smoke checks. Full Home tests
passed518 before the resource-only2539 lifecycle correction; no Rust logic
changed afterward. Artifact and source receipts are under
`out/home/settings-system-languages-r2/validation`.

Retained failures explain the two product fixes. Broker r1 directly referenced a
package-private AOSP collector class and failed with IllegalAccessError; r2 uses
finite reflection for the class, constructor and exact supported-list method.
The public-compile/private-runtime regression fails before that fix and passes
afterward. Home2538 handled neither CONFIG_LOCALE nor CONFIG_LAYOUT_DIRECTION;
language changes recreated the renderer Activity and could leave its native
receiver shut down. Home2539 adds those bits to its existing configuration mask,
using MakepadActivity's existing configuration callbacks to keep the renderer
and pending draft alive. The248-check run asserts Activity identity on every
write, rather than accepting an ActivityRecord token that survives recreation.

Earlier fixture failures (Unicode extension parsing and retired scroll actions)
remain archived separately. The2538 lifecycle failure also prevented automated
cleanup; the original language was restored through Android's native Languages
UI before2539 testing, with a separate recovery receipt. No physical device was
used. Ordinary5556 also passed15 unavailable/recovery checks,49 cold/12 launches,
60 entry checks and33 smoke checks on2539, preserving its identity, locale and
accessibility preferences. Its summary is `ordinary2539-summary5556.json` beside
the native receipts. Full product localization/RTL, secondary-user behavior and
production ROM boot/hardware acceptance remain separate work.
