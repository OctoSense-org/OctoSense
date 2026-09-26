# OctoSense Settings

Open **OctoSense Settings** from the app catalog. The existing **Settings** entry
continues to open Android Settings.

- **Search settings:** find migrated pages with English or Chinese keywords. Search stays local; Back returns to the same results after visiting a page.
- **Appearance:** choose OctoSense, Minimal, Paper or Vivid, System/Light/Dark,
  and gradient/solid wallpaper, then Apply. Cancel or Back discards edits.
- **Display:** brightness in 10% steps, automatic brightness, rotation lock,
  screen timeout, font size and touch vibration.
  Missing permissions show the corresponding access button. Manual brightness
  steps disable automatic brightness; the lowest manual step is 5%.
- **Sound:** media, alarm, ring and notification volume in 10% steps, touch
  sounds and four manual Do Not Disturb modes under Notifications. Built-in default
  interruption policy and Android time schedules are undergoing emulator acceptance;
  other rule types retain Android recovery. Sounds has a bounded
  built-in catalog with Preview, Save and Cancel; sound feedback, vibration and
  supported per-app notification controls have built-in pages.
- **Apps:** browse Android packages for the current user, filter by name or
  package, optionally show system apps, and use Previous/Next for 20-app pages.
  Search or Return applies the typed filter. App details show version, state,
  install/update dates, storage statistics and requested permissions with actual
  grant state and permission type. Launch uses the app's observed launch target;
  Uninstall opens Android's confirmation. Notification settings has built-in
  app/channel/group controls. Permissions opens built-in common runtime groups
  and observed choices; Android retains its required warnings. Unsupported
  specialized controls keep explicit native recovery.
- **Battery, Storage, About phone:** real battery/power/temperature, data-volume
  capacity and free space, build/Android/kernel and memory observations. Advanced
  management still opens Android Settings.
- **Connections:** a built-in Wi-Fi page shows observed radio/connection state,
  nearby and saved networks with 20-row pages, scan age and permission-dependent
  actions. Direct radio changes and saved-network connect/forget require the
  authorized ROM helper; standalone Home shows those controls as unavailable.
  Scan is explicit and can be throttled by Android. Forget asks for confirmation.
  **Configure in Android** retains Android's credential and enterprise
  certificate flow. Bluetooth has built-in discovery, name and device pages;
  advanced profile options, mobile, hotspot and VPN keep native fallbacks.
- **Accessibility (under System):** color inversion/correction, mono audio, balance, captions, caption language and custom caption appearance. The ROM helper supplies authorized controls. Caption size/style/appearance edits enable captions; language edits preserve the current on/off state. Custom appearance requires the Custom caption preset. The preview shows color and opacity, with Android-specific typeface/edge rendering still pending.
- **System:** a built-in Date and time page with observed local time/zone/locale,
  12/24-hour format and permission-dependent automatic time/zone controls;
  temporary native links for other System, Security and Accessibility controls.
  Accounts has a built-in inventory, provider list and per-service sync pages. System updates has a built-in status/review page that reuses the ROM updater.

System controls need Android and their corresponding platform permissions.
**Allow OctoSense controls** grants Home's write-settings access; **Allow display
controls** grants the separate Bridge package's brightness/rotation access. The
ROM helper can provide authorized settings access without those standalone
grants. Automatic time controls require privileged access and may be restricted
by administrator policy. Unavailable controls are disabled. Desktop builds can
render and navigate the module but do
not change the desktop operating system. Themes always use the shared catalog
and storage, including changes made through the older native appearance picker.

Snapshots refresh every five seconds while Settings is active and on return.
Controls display observed values, including Android's actual volume steps, and
never substitute zero for missing data. Font-size changes scale this module's
existing widgets and preserve its current page. Other apps' custom text still
need the corresponding shared-font migration.

Apps retains the search field and list scroll position when returning from
details. Polls and live theme changes preserve typing and cursor state. Inventory
changes invalidate pagination and return to the first page, with an explanation.
Storage statistics can require **Allow storage statistics** (Android usage
access); unavailable figures are shown as unavailable. Data includes cache, so
the three figures must not be added together. Some Android packages share
storage statistics through a shared UID. Hosted OctoSense modules such as Mail
and Settings share Home's Android package and data; they are not separate Android
packages in this list.

Implementation: [ADR 0006](../adr/0006-builtin-settings.md), the
[full replacement parity ledger](settings-parity.md), and the
[app theme contract](app-theme-contract.md). Complete replacement is the target;
native links are temporary migration coverage. TalkBack support for every hosted
control and Chinese localization remain required work.

## First increment validation (2026-09-24)

The standalone Android build `2026092407` was installed on an API 35 ARM64
emulator. Verified entry from Home, Back navigation, theme Apply/Cancel and
persistence across restart, an external native-picker theme update while the
Display page remained open, and wrapped status text on the phone-sized layout.
The standalone build reports partial system styling because the ROM-only
platform helper is absent.

Display controls initially remained disabled without write-settings permission.
After granting it through Android's permission screen, observed brightness
changed from 102 to 128, automatic brightness from off to on, and rotation from
unlocked to locked. Media volume changed from 5 to 7 of 15. Wi-Fi, System, DND
and notification buttons opened the expected native Android destinations; Wi-Fi
returned to the same Settings page. Emulator theme and control values were
restored after testing. No physical-phone installation was performed.

`cargo test --locked --bin octosense --features mobile-apps` passed 319 tests,
including forged/retired authority rejection and a 220dp text-layout regression.
The repository Python suite passed 27 tests. Java compilation, the Android APK
build, and `git diff --check` passed.

## Expanded device pages (2026-09-24)

Build `2026092411` adds the device pages and typed controls described above.
Validation used a separate API 35 ARM64 emulator, without changing the OnePlus.
About, data storage and battery observations matched Android. Battery changes
were simulated through Android's BatteryService and restored after the test.

Home's write-settings grant was exercised through Android's actual permission
screen. Before granting access, a disabled font-size control could not mutate
Android. After granting access, timeout, font size, touch vibration, touch
sounds and 12/24-hour format changed and survived a process restart. Alarm
volume changed from 6 to 5 of 7; ring and notification each changed from 5 to 4
of 7. UI values matched Android's quantized stream values. Automatic time/zone
controls stayed disabled without the privileged ROM service; their real ROM
mutations and administrator behavior still require phone validation.

External timeout and clock-format changes appeared on the active page within
the five-second refresh interval plus delivery time. Large text reached 150%
without resetting the Display page. The initial emulator run exposed a missing
cold-start resume observation; Java now supplies the actual resumed state in
its retained UI-mode snapshot. The emulator check also caught a fixed-width Back label wrapping mid-word;
Back now measures its natural width, with a regression check for single-line
text at large font sizes.

The full Rust suite passed **327 tests**, including current-root authorization,
observation ordering, missed initial Resume, actual wrapped controls and complete
220dp content columns at 150% text size. The repository suite passed **28 tests**,
including the Java finite-setting contract. Android packaging and Java compilation
passed. This validates this increment, not complete Settings parity; the
[remaining work](settings-parity.md) includes OnePlus hardware, localization,
TalkBack, managed users and the larger native-screen migrations.

The final APK was also installed on the existing visible emulator and left at
OctoSense Settings. Cold-start About values loaded there. Its original timeout,
font scale, touch sounds, touch vibration and clock-format preferences were
preserved. Final 150% screenshots verify a single-line Back label and readable
device information; return from a native screen refreshed the same About page.
No AndroidRuntime errors were logged by the isolated validation emulator.

## Apps increment validation (2026-09-24)

The full Rust suite, `cargo test --locked --bin octosense --features mobile-apps`,
passed **337 tests**. Apps coverage includes malformed and incomplete catalog
pages, unavailable storage and permission observations, stale response
correlation, inventory-generation changes, a row changing during a press,
and returning to the same scrolled list. Native search text, cursor and widget
identity survive polling and a live theme update; submission reads the final
input text. The layout regression includes the Apps list and detail pages in a
220dp content column at 150% font size. A native touch regression covers swipes
starting on app rows, a detail action and an Overview button, plus metadata.
It requires widget-requested repainting and measured content movement, and
checks release over the moved button emits cancellation instead of a click or
Settings operation. Small-jitter taps still activate the same buttons and
handoff clears their pressed appearance. `git diff --check` passed.

Build `2026092412` was exercised on an isolated API 35 ARM64 emulator. Its
current-user catalog contained three non-system packages and 242 packages with
system apps included. Search retained an unsubmitted draft across refreshes;
Search applied it, including an empty-results case. Twenty-app pagination worked,
and disabling a test app while viewing page two invalidated the generation and
returned to page one with an explanation. App details showed the observed
disabled state, and Back retained the filter.

The disposable test app's 29 requested permissions appeared as pages of 20 and
nine with real grant states. Other-package storage was unavailable until the
native usage-access grant; afterward its observed figures were 29,184 app bytes,
3,219,456 data bytes and 1,085,440 cache bytes. Home's own storage remained
available after usage access was revoked. Data includes cache.

After the test app was enabled, Launch opened it. Android's uninstall
confirmation was exercised both ways: Cancel kept the package, and confirmation
removed it; returning to details showed that it was no longer installed.
Actual touch swipes over metadata moved app details and exposed its permission
controls. Swipes starting on an app button exposed a separate capture problem:
the native Button kept the finger instead of handing it to the scroll view.
Settings now transfers a button's capture to its containing scroll view only
after a vertical gesture exceeds 8dp, cancels the button press and uses the
existing scrollbar's drag, momentum and redraw behavior. TextInput and Back
are excluded from this handoff. Live System dark mode and 150% font changes
preserved the draft filter and kept Back readable. The repository suite passed
29 tests.

Build `2026092413` passed the follow-up gesture check on an isolated API 35
ARM64 emulator. The same 500ms drag starting on the first app row moved its
label from y=1405 to y=615 (790 physical pixels); build `2026092412` left the
row stationary. The gesture stayed on Apps without opening the row. A normal
tap then opened the selected app's details, dragging from Refresh details
scrolled that page, and Back restored the prior list position. These are
functional gesture checks, not a frame-rate benchmark. The fresh AVD required
waiting for its first renderer startup before testing.

Build `2026092413` is also installed on the shared emulator (`emulator-5554`).
The final gesture check used the isolated instance because another automation
session was opening its own test app on the shared emulator. Native app-info
and notification links were checked there in `2026092412`, including return to
the same details page. Temporary validation AVDs were removed after testing.

This validates the implemented Apps increment, not complete Settings parity.
Permission editing, defaults, broader app policy, TalkBack, localization and
OnePlus hardware remain in the [parity ledger](settings-parity.md). No physical
OnePlus installation was performed.

## Wi-Fi increment verification (2026-09-24)

The full Rust suite passed **343 tests**. Wi-Fi coverage rejects malformed or
unbounded snapshots and invalid targets, retires action capabilities with the
request/root lifetime, preserves selection and navigation through polling and
live themes, prevents a replaced row from taking a previous row's press, and
requires explicit Forget confirmation. The native touch regression also covers
a Wi-Fi row, including repaint, scrolling, cancelled clicks and ordinary taps.
Both Wi-Fi pages participate in the 220dp / 150% font layout check. Missing saved
profile access is displayed as unavailable, rather than claiming a scanned
network is not saved. `git diff --check` passed.

Android acceptance of the integrated Wi-Fi UI and real ROM-helper authorization
is still pending. Enterprise credentials, network sharing and advanced policy
remain separate migration work in the parity ledger.


## Compiled controls pages

Location, Privacy and Notifications open directly from the OctoSense Settings
overview. Sound also opens the same Notifications page; Battery opens Battery
saver and policy. Back returns to the page that opened the controls.

These pages share four stable Octoscript–Makepad row slots. Compiled Rust enums
own the page, control IDs, labels and finite choice vocabulary. Android returns
observed values, availability and the currently authorized choices; it cannot
supply widget code, callbacks or arbitrary settings keys. Unavailable values are
not shown as Off. Read-only observations stay visible with disabled controls.
Camera/microphone On means apps may use that sensor, subject to their own
permissions. Battery threshold 0 means Never; a non-preset observed threshold
such as 23% remains 23%, with the nearest permitted Lower/Higher choice.

The host binds every snapshot to the foreground Settings root and selected
controls page, retires mutation authority when that context changes, and
rechecks the observed option before sending a finite command. Reads are
correlated, limited to one current request, refreshed every five seconds, and
expire after 20 seconds. A pressed button also retains its choice: a poll cannot
change a held Higher button from 25% to 30% without a new tap. Live themes and
font scaling keep the same native widgets.

This increment adds location/scanning, camera/microphone access, notification
lock-screen/history/bubbles policy and battery saver/threshold/adaptive policy.
Integrated Android and ROM-helper acceptance is pending. Detailed dashboards,
per-app policy, hardware-specific features, secure credential flows and the
remaining [parity gates](settings-parity.md) are tracked separately; the More in
Android Settings button is a migration fallback, not evidence of full parity.

The full Rust suite passed **349 tests** after this increment (focused Settings:
37 tests). New checks cover finite page/choice decoding, malformed and unavailable
observations, custom thresholds, retired page/root authority, stable theme and
Back behavior, poll-time press retargeting, all four pages at 220dp and 150% font,
and native button-start swipe/tap/redraw. `git diff --check` passed. These tests
do not substitute for the pending Android readback and ROM-helper policy checks.


## Bluetooth controller

Connections → Bluetooth opens the built-in device list. The page shows the
observed radio/discovery state, Paired and Nearby tabs with 20 device rows per page,
and a Bluetooth name input with explicit Save/Cancel. Only manual Scan starts
discovery; periodic reads do not scan. Stop scan is offered only when the backend
owns that discovery session. Radio-off or denied paired-device access is shown
as unavailable, not an empty inventory.

Device details show actual bond/connection observations, device type/transport,
optional address and last-seen age. Only an observed opaque device key can target
Pair, Cancel pairing, Connect, Disconnect or Forget. Forget requires an explicit
confirmation; Android's trusted pairing dialog owns passkey/PIN consent. A
request-sent result does not imply Paired or Connected. Contact and message
sharing show the current Ask/Allow/Deny state and only authorized choices.
Unknown connection or sharing state remains unavailable.

Name edits use the same native TextInput through polls, live themes and Back
from device details, preserving the draft/caret. External observations do not
overwrite a dirty draft; matching observed readback marks a saved name. No input
is remounted during typing. List slots and the selected device remain stable;
replacing a row while pressed cannot redirect its action. Host authority is
bound to the foreground Settings root, current selected device, fresh capabilities
and correlated read lifetime. Retirement preserves observations while revoking
actions and sharing choices.

Integrated Android and ROM-helper/peripheral verification is still pending.
Advanced profile policy and hardware-specific Bluetooth features remain in the
[parity ledger](settings-parity.md). This work is validated in emulators only;
no physical phone installation is authorized for the current phase.

The Bluetooth increment passed **355 Rust tests** (focused Settings: 43), including
UTF-8 name limits, owned-discovery stop capability, unknown connection/bond state,
retired target/sharing authority, draft/caret preservation through live themes,
explicit Forget, both Bluetooth pages at 220dp/150% font and native button-start
scroll/tap/redraw on both list and detail. The touch test positions the list
before the gesture; the actual swipe must request repaint and suppress clicks.
`git diff --check` passed. Emulator/service/peripheral acceptance remains pending.


## Accounts and sync controller

System → Accounts opens the current account inventory with explicit full,
limited or unavailable visibility. Limited means only accounts visible to Home;
empty results are not a claim that no accounts exist on Android. Lists are bounded
at 128 accounts/providers and 64 sync authorities, with 20 stable rows per page.
Account details lead to actual automatic/syncable/active/pending observations for
each sync service. Automatic sync, Sync now and Cancel appear only when current
observations authorize them. Unknown values remain unavailable. Refresh and
foreground polling never request synchronization by themselves.

Add account selects an observed provider and starts Android/provider sign-in.
Choose accounts visible to OctoSense opens Android's visibility chooser. Remove
account opens a native confirmation, available only on the full system route;
the ordinary APK cannot claim removal permission. The confirmation rechecks
current user, unlock/policy and exact account incarnation; authenticator-owned
extra authentication remains native. No credentials or authenticator results
are transported to the Octoscript module. Master/per-authority settings are
confirmed by readback, while sync requests are explicitly asynchronous.

Back preserves account/service pagination and list scroll. Theme changes and
150% fonts preserve widget identity and selection. Returned lists cannot retarget
a held button; the scoped Settings scroll handoff covers account and authority
rows. Capability retirement preserves visible values but disables their actions.
Detailed sync failures/history, managed-profile account administration and
password/passkey/autofill settings remain migration gaps; this increment does not
complete Android Settings parity.

### Backend validation during Accounts work

The isolated API35 emulator's Bluetooth system-UID API harness verified actual
radio/name reads, scan ownership, rename/readback/restoration, toggle/readback,
and unavailable pairing data when off. It rejected forged device targets and
refused to cancel discovery begun elsewhere. This is framework API evidence,
not installed-ROM-helper Binder end-to-end or physical-peripheral validation.
The disposable Controls APK with granted WRITE_SECURE_SETTINGS verified history
write/readback, malformed values remaining unknown/read-only, custom 23% battery
threshold preservation, preset25% application and automatic-schedule rejection.
Fixture changes were restored.

The disposable `dev.makepad.octosense.accountsfixture` authenticator/sync adapter
verified caller-visible inventory labeled limited, a real sync authority,
automatic-sync on/off readback, forged account/authority rejection and unavailable
removal on the limited route. Manual Sync now queued real Android work; forcing
the fixture job through the scheduler exercised adapter invocation/completion.
The emulator's unvalidated network can leave ordinary jobs pending, so this is
not evidence of internet account synchronization. Actual Home UI and privileged
native-removal validation are recorded separately after the next APK build.

The Accounts increment passed **360 Rust tests** (focused Settings:48). New
checks cover inventory/visibility bounds, capability retirement and request/root
correlation, exact sync targets, poll changes during a held button, preserved
account/authority pagination through themes and Back, all four pages at220dp and
150% font, and real button-start touch scrolling/no accidental click on both
account and authority lists. The standalone Java contract test also passed
identity scoping, finite action/value validation, bounded observation eviction,
expiry, clock rollback and explicit retirement. Commands/logs:

```sh
cargo test --locked --bin octosense --features mobile-apps settings_
cargo test --locked --bin octosense --features mobile-apps
JAVA_HOME=<JDK17> python3 -m unittest discover -s tests -p 'test_accounts_settings_contract.py'
```

Rust logs: `/tmp/octosense-settings-accounts-focused.log` and
`/tmp/octosense-settings-accounts-full.log`. Shared Accounts Java also compiled
against the installed Android35 SDK. Privileged broker/sensor APIs require the
separate pinned-framework build; ordinary SDK compilation alone does not
validate that deployment. No OnePlus change was made.


## Local Settings search

Search settings on the overview opens a stable text input and a locally ranked
index with English/Chinese aliases. Matching is immediate; the query never goes
to Android or a server. Results use 20 fixed slots per page. Selecting a result
opens a typed existing page, starts only that page's normal observation reads,
and hides the keyboard. Back preserves the query, caret, result pagination and
scroll. Nested account/app/network details and battery policy retain their
ordinary Back hierarchy before returning to the search results.

The index describes available OctoSense pages, including permission-dependent
controls. It does not claim that native fallback sections have been migrated.
Chinese aliases improve discovery; full product localization and accessibility
remain separate acceptance gates. Normal Accounts list Back now returns to its
System parent, while search-launched Accounts returns to Search.

Search passed **365 full Rust tests** and **53 focused Settings tests**, including
English/Chinese matching and invalid query limits, every typed destination,
caret/widget preservation under themes, exact nested return route, actual result
scroll preservation, narrow220dp/150% layout and real button-start touch scroll
with tap preservation. Commands remain `cargo test --locked --bin octosense
--features mobile-apps` (add `settings_` for the focused suite); logs are
`/tmp/octosense-settings-search-full.log` and
`/tmp/octosense-settings-search-focused.log`. Emulator validation is pending the
next APK; these unit checks do not replace device/IME/accessibility acceptance.


## System updates

System → System updates shows actual device/current versions, remote-check
state, offered releases, ROM/Home update state, progress and errors. The screen
works with the existing ROM updater; a standalone APK on stock Android reports
the service as unavailable, while retaining any observed local version data.
Refresh status and the five-second foreground poll do not access the remote
feed or start installation. Check for updates is explicit asynchronous work.

Review system update and Review OctoSense Home update open separate local
confirmations. Install reviewed update sends the exact opaque offer key and
finite component; a changed/expired offer or revoked capability cancels review.
A ready system update enables Review restart, followed by a distinct Restart now
confirmation. Leaving the page or losing foreground authority also invalidates
review. Accepted requests remain requests until the observed service phase
confirms progress; errors remain visible even when an earlier check succeeded.

Cancel/resume, feed/channel editing and real signed-update/reboot acceptance
remain separate work. Tests for this increment capture typed actions and inspect
status only; they must not install a real update or reboot an emulator/phone.

The updates increment passed **370 full Rust tests** and **58 focused Settings
tests**. Coverage includes exact reviewed offer keys, optional/invalid progress,
request/root retirement, no install before confirmation, new-offer cancellation,
separate restart review/cancel, live theme/widget preservation and narrow150%
layout. The touch harness verifies update-page button swipes without activating
an operation. All update/restart actions in these checks are captured Rust
messages, not Android calls. Logs:
`/tmp/octosense-settings-updates-full.log` and
`/tmp/octosense-settings-updates-focused.log`; commands remain the full/focused
`cargo test --locked --bin octosense --features mobile-apps` suite.

### Android input handoff

An isolated API35 emulator reproduced a first-character duplication immediately
after focusing Search. Metadata-only tracing on diagnostic build2026092419 proved
that the same B press reached `MakepadInputConnection` on ACTION_DOWN (inserting
one character), while its ACTION_UP reached `MakepadSurface` (inserting it again).
The trace recorded event identity and text lengths, never typed text. The stable
Search input was not remounted or written during typing.

The registered `patches/runtime/makepad-contained-apps.patch` makes the surface
insert characters on ACTION_DOWN, matching the existing input-connection path.
Key-release callbacks, repeats and legacy ACTION_MULTIPLE remain supported. Both
handoff directions now have one insertion edge; there is no timing/text-based
deduplication or input delay. The runtime revision stays pinned, and setup
verifies the overlay hash and expected Git tree. Rebuild cargo-makepad when this
patch changes because its Android Java templates are embedded in the packager.

`tests/test_android_ime_handoff.py` extracts and compiles the actual installed
surface/input-connection routing methods with minimal Android transport doubles.
Both handoff directions, same-path delivery, repeated identical characters,
auto-repeat, Shift/navigation, delete, volume and legacy MULTIPLE passed. The
unpatched runtime fails the same regression. These tests cover event routing;
actual IME composition requires emulator checks. The repository suite passed
**37 tests**; log `/tmp/octosense-settings-ime-repo-tests.log`. Diagnostic logging
is absent from the final patch. Final APK input/OTA UI validation follows below.


### Account authority correction

Inspection of both pinned ROM and AOSP Android15 manifests established that
SystemUI runs as `android.uid.systemui`, not UID1000. The earlier account adapter
would therefore correctly remain limited but could not deliver the intended full
inventory. Account service, AIDL and native removal confirmation now live in the
small platform-signed `OctoSenseSettingsBroker` system APK with
`android.uid.system`; it is included in the ROM product. The agent and Home keep
their separate UIDs. Sensor privacy stays in the role-owned SystemUI adapter.

The broker has no launcher, boot receiver or persistent-process declaration.
Its only exported component requires the existing Agent signature permission;
each Binder method also verifies caller package/signature, same/current user and
unlock state. Existing bounded observations, sync allowlists and exact-incarnation
confirmation are preserved. Home checks the broker package as PendingIntent
creator. Full support is currently owner-user only; secondary-user routing remains
fail-closed. See `vendor/octosense/settings-broker/README.md`. Real broker Binder
and confirmation validation on the separate AOSP emulator is still required.

The final input APK2026092420 passed first-focus fast `bluetooth`, repeated
`bookkeeper`, one Gboard B with deletion, and reopening Search with fast `updates`.
The Updates page reported actual local model/build/Home version and unavailable
stock service; Refresh preserved that observation and Back restored the exact
query. No install or restart was requested. A separate Clear-search focus issue
was found: the visible keyboard could outlive the field's focus after clearing.
Clear now explicitly returns focus to the same input; its regression checks that
the next character is accepted without replacing the widget. The next APK must
validate that focus change on Gboard. Only isolated emulator5556 was modified.

The Clear-search regression and complete Settings work passed **371 Rust tests**
(6.13 seconds), log `/tmp/octosense-settings-input-full.log`. The repository suite
passed **39 tests**, including the broker's manifest export/signature boundary,
Agent UID separation and exact helper/broker AIDL identity; log
`/tmp/octosense-settings-broker-repo-tests.log`. `git diff --check` passed. Manifest
and host tests do not substitute for installed platform Broker Binder checks.


## Advanced connections

Connections → Advanced connections, also available through English/Chinese
Settings search, displays observed connectivity, transport, Internet validation,
Airplane mode, global Data Saver and Private DNS. Only currently authorized
controls can write; standalone installations retain observations while disabled
controls remain unavailable. Private DNS displays saved mode/provider separately
from active encryption and validated DNS state. Saving a provider does not claim
working Internet or a successfully validated encrypted connection.

Edit Private DNS opens a stable local mode/hostname form. Save sends the finite
mode and optional provider under the current opaque observation key; Off and
Automatic never carry a provider payload. Cancel sends no command. External
settings changes keep draft text but disable Save until Cancel → Edit reviews the
new values. Pressed controls are bound to the state seen at press time, preventing
a poll from retargeting a held toggle or Save. Reads poll only on this foreground
page, have bounded timeouts, and never change network settings.

Airplane/Saver writes require observed booleans. Malformed DNS settings may expose
an unknown mode plus a write capability so the user can explicitly choose a valid
replacement. Invalid URLs/IPs/ports/control text are rejected; Android validates
and normalizes IDNA hostnames before mutation. Proxy, traffic usage/limits,
per-app exemptions and carrier/hotspot/VPN migration remain pending.

Build 2026092421 exposed a Clear-search failure despite the isolated focus test
passing. Subsequent metadata-only diagnostics found mixed Surface delta events
and InputConnection full-state edits, a temporary editor blur during Clear, and
a needless initial InputConnection restart. The registered runtime patch routes
active editor hardware edits through its shared Editable and aligns the initial
single-line configuration. Settings routes Clear as an editor accessory, retaining
focus throughout the complete pointer gesture. No text deduplication or input
delay is used.

Diagnostic build 2026092425 passed the previous Clear → typing reproduction five
times, first-focus fast typing, and real Gboard B/delete. A stricter zero-delay
Clear → typing run still used the old Java buffer; the Android event batch now
flushes editor operations after each processed event, matching the existing
non-vsync path. Build 2026092426 passed five of six immediate Clear → typing runs, but one
`bluetooth` injection lost its leading `bl`. This stricter race remains open:
Java can edit its buffer before Rust's programmatic clear reaches the UI thread.
The batch fix reduces delivery latency but does not establish a causal protocol
between the independent buffer owners.
Tests extract the installed Java methods and actual Android batch block;
pre-patch batch code fails the Clear/query ordering regression.

Network passed **377 full Rust tests** (9.02 seconds) and **65 focused Settings
tests**. New coverage includes optional state and malformed input, exact keys and
root/request retirement, DNS draft/caret/widget preservation under polls/themes,
explicit Save/Cancel payloads, stale held controls, search and nested Back,
220dp/150% editor geometry, and real button-start scrolling without activation.
Logs: `/tmp/octosense-settings-network-full.log` and
`/tmp/octosense-settings-network-focused.log`; command remains
`cargo test --locked --bin octosense --features mobile-apps` (add `settings_` for
focused checks). Android helper capability and encrypted-DNS acceptance are
separate runtime checks. Build2422 is a temporary metadata-only focus diagnostic;
it must not be presented as the final non-diagnostic artifact.


## Manual Do Not Disturb modes

Sound → Do Not Disturb and Notifications expose four explicit choices: Off,
Priority only, Alarms only and Total silence. The selected label comes from the
observed Android mode. Only observed, authorized choices emit a finite request;
standalone Android installations show the current mode without mutation rights.
Priority only retains the existing allowed-interruptions policy. Rules, schedules
and allowed interruptions still use an explicit native Settings destination and
remain incomplete parity items. The ROM path uses the narrow system-UID Settings
Broker rather than per-app implicit Zen rules. Final APK and four-mode readback
acceptance are recorded in the platform emulator validation document.

Settings result notices remain on the page that initiated the operation, including
when a completion arrives after navigation. Updates renders its actual check state
in place; an accepted check no longer leaves a redundant global pending message.


## Build 2026092426 validation

The non-diagnostic APK is `out/home/settings-dnd-input-final/OctoSenseHome.apk`.
The complete Rust suite passed **381 tests** (10.15 seconds), focused Settings
checks **69 tests** (8.47 seconds), and repository checks **41 tests** (22.02
seconds). Logs: `/tmp/octosense-settings-dnd-input-{full,focused,repo}.log`.
`git diff --check` passed. The packager was rebuilt to embed the reviewed runtime
overlay, with no input-trace logging.

On isolated API35 emulator5556, first-focus fast typing was exact; the original
0.6-second Clear → typing reproduction passed for Wi-Fi, bookkeeper and Bluetooth.
Actual Gboard B and Delete both worked after Clear. Android Back changed
`mInputShown` to false and it remained false after a six-second refresh interval;
the draft remained unchanged and tapping the field reopened the keyboard. The
stricter immediate-input failure described above is not considered fixed.

Standalone DND displayed actual Off, Read only and all four named choices. Tapping
the disabled Total silence choice left Android `zen_mode=0`. Privileged mode
changes and system policy checks are covered separately by the ROM-service
emulator run. No physical phone, ROM update install or reboot was requested by
this UI validation.


The follow-up editor fix removes Settings' untracked raw IME write. A focused
TextInput now sends programmatic text/selection changes immediately through its
existing last-sent-state bookkeeping; the following draw cannot enqueue the same
Clear again over newer Java edits. The regression inserts the installed setter
and synchronization methods, verifies one immediate Clear and no second update,
checks character-based selection offsets and excludes background/read-only fields.
The pinned pre-patch setter fails this test. Build 2427 still failed strict
immediate Clear/replacement stress; this smaller fix did not resolve the race.


## Causal editor transport (in validation)

Build 2429 replaces unversioned Java full-buffer writes with ordered commit,
composition, selection and deletion operations. The focused Rust TextInput is
canonical. Its stable widget identity defines an editor session; Android keeps
an optimistic Editable for synchronous keyboard queries. Replies acknowledge
applied operations, so a delayed Clear or selection reply cannot overwrite newer
optimistic edits. Retired connections reject writes and return an empty buffer
through every Android query family. This is a bounded patch to the pinned runtime,
not a runtime version upgrade.

The real API35 EditText InputConnection and Rust operation engine matched text,
selection and composition in 16 synthetic cases, including selected deletion,
surrogate pairs, composing-region replacement and positive/negative cursor
positioning. A separate reproducible fixture compiles the actual patched
MakepadInputConnection and verifies operation ordering, atomic replacement,
composition/batching and retired-query isolation:
`home/android/scripts/run-android-editor-probe.py --help` describes its explicit
SDK/JDK/ADB and dedicated-emulator arguments. The fixture has no permissions and
is removed after the run. These checks supplement actual Gboard acceptance.

Build 2429 exposed an additional cold-start defect: Android Resume can arrive
before the native message queue or be consumed by the pre-surface bootstrap.
The editor and some Settings read controllers consequently remained inactive.
The follow-up records the actual Activity lifecycle before queue delivery and
reconciles it through the normal Resume handler after Startup. It does not assume
that a created app is foreground. Regression tests cover lifecycle events before
the queue, bootstrap reconciliation and later Pause/Resume transitions.

Warm 2429 passed 18 successive immediate Ctrl+A replacements, 12 immediate
Clear-to-hardware-input runs and eight actual Gboard Clear-to-composed-word runs.
Build 2430 passed cold first-focus typing on Gboard, eight more immediate
replacements, and immediate cold Settings observation loading. The separate
AOSP-IME run passed nine replacements, twelve Clear/input runs and six rapid
Date/time replacements before saving a real reviewed time; see
[platform validation](settings-platform-emulator-validation.md).

The fixture also compares an explicitly supplied composing payload after Clear
with native EditText: both insert the supplied full composition (`blueh`). The
transport does not guess which prefix the keyboard intended. No blocking
pointer barrier or string-repair heuristic was introduced. Actual Gboard tests
passed; these tests do not establish behavior for every third-party IME or an
artificially stalled editor.

The combined build2431 includes the final inactive-editor acknowledgement,
dispatch-time keyboard dismissal check and session-bound editor actions. An IME
Done/Go action now follows the same sequenced session protocol as text edits;
queued actions from a retired editor cannot submit a newly focused field.
New-session inactive replies still clear old editor data; lower-session replies
cannot clear a newer editor.

Actual Gboard2431 acceptance passed cold first-focus typing, eight immediate
Ctrl+A replacements and eight Clear/input replacements. Back kept the keyboard
hidden at one and six seconds, and deliberate retap reopened it. The same
Back/retap check passed after returning from the separate native editor fixture
Activity; keyboard Done hid the keyboard and preserved the exact draft. The
fixture also passed composition, deletion, batching, current editor action and
retired editor action/query isolation. The extracted dispatch test fails before
the dismissal guard and passes afterward. All six editor regressions pass;
the combined Home build passed389 Rust tests. No blocking pointer barrier or
fixed input delay was added.
No physical phone was changed.


Sound selection increment (local and ROM-helper emulators2432–2433 accepted): Sound
opens a built-in ringtone/notification/alarm catalog with20-row pages, at most1000
entries and an explicit Silent row. Selecting a row creates a local draft;
Preview/Stop does not save it. Save applies only the exact reviewed target after
fresh policy/current-default/media validation, then requires Android readback.
Catalog keys expire after five minutes; external changes retire the draft's
write authority. Rows retain their widgets across theme/font changes, and a
pressed row cannot be rebound by a new observation. Preview uses the selected
stream's audio usage, honors current volume/DND and stops after five seconds or
when Settings leaves the foreground. Native Sound remains the custom-audio and
advanced-policy fallback. The new Binder calls are append-only52–55
(snapshot/preview/save/stop); stop keeps caller authentication but remains allowed
after lock or user change to release playback. No media URI enters Octoscript.


A later first-navigation test on build2432 showed `gtone` after immediate ADB
hardware injection of `ringtone` into newly opened Search. The settled screenshot
confirmed the missing prefix. Earlier2431 tests above passed; this additional
cold-navigation timing case was investigated separately from Sound.
The following repeat tests and metadata trace isolated the cause.


Local API35/Gboard emulator5556 build2432 sound acceptance:98ringtone,
74notification and19alarm rows were observed; the second ringtone page showed
21–40. Home's native Modify system settings grant enabled Save. Selecting,
previewing and cancelling Canis Major left the default untouched; explicit Save
changed the canonical Android URI and observed title, and the original Flutey
Phone was restored through the UI. Notification Silent saved a null default;
external restoration retired the old catalog and disabled stale actions. Alarm
Argon saved with matching readback. AudioService showed actual MediaPlayer
playback using USAGE_NOTIFICATION_RINGTONE/USAGE_ALARM; Stop released playback,
and opening native Settings released the alarm preview about0.5s after start.
Ring/notification/alarm volumes stayed5/5/6 and DND stayedoff. Exact original
canonical defaults and Home's WRITE_SETTINGS app-op were restored. The
scroll-to-review follow-up passed393Home tests, including actual touch swipe/tap
and review visibility; production2433 passed actual scroll-to-review and Cancel acceptance. The owner-service emulator also passed all three default types and nine caller-boundary checks, with original defaults and audio policy restored.
Custom audio import, vibration/charging policies and physical-device audio remain
outside this slice's acceptance.

The first-focus2432 issue is now reproducible: two of six immediate cold ADB
hardware-entry trials lost the first character. In three fresh Gboard trials,
tappingB-o-o-k showed Book suggestions in Gboard while the Search field remained
empty. This reopens the cold-editor transport gate despite the earlier warm and
2431 passes. Diagnostic2434 logs session/ownership/sequence/length only; no text
content is logged. A causal fix and repeat acceptance are required.


Diagnostic2435 identified the cold-editor cause: Android requested two
InputConnections before any widget was focused, and Gboard retained the first.
The first canonical editor reply bound only the Surface's latest connection;
the earlier open handle was rejected because it was not the last Java pointer.
The overlay now authorizes open handles by stable editor session. Only handles
created before the first editor may adopt session1; a closed handle never revives,
and an unused bootstrap handle cannot adopt any later editor. Session changes
continue to block all old reads, writes and editor actions. A Surface connection
epoch also retires the entire open-handle cohort immediately on Activity pause
or an inactive-editor reply, even before the native session changes. The initial
inactive/session0 observation preserves pre-focus handles. No delay or blocking
barrier is used. The actual Android fixture failed on the earlier retained handle
before the fix and passes afterward, including duplicate handles, closed handles,
unconsumed bootstrap retirement and successful new-editor input. Production2436
acceptance passed on API35/Gboard: six cold Gboard and six immediate hardware
entries, eight Ctrl+A replacements and eight immediate Clear/input trials. One
cold navigation attempt stopped before typing because a Home tutorial hint had
disappeared; rerunning with a stable Home label passed. Back stayed hidden at one
and six seconds; retap restored typing. After the separate native fixture
Activity returned, the exact draft survived, selected replacement worked, Back
stayed hidden, and retap/Done worked without changing the draft. Independent
AOSP acceptance passed three cold entries, four Ctrl+A and four Clear trials,
Back and native-Activity return. All394 Home tests and48 repository tests passed;
the actual Android fixture passed duplicate handles, initial-empty observation,
same-session inactive and immediate pause retirement, later-editor rejection,
composition, selected/surrogate deletion and retired query/action isolation.
Diagnostic logging was removed from the registered production overlay
(tree6c8183cc12797f16f8a7f4d8800e8c253ea14dc0). No physical phone was changed.


Media volume now uses the same typed DeviceSettings observation and mutation
path as alarm/ring/notification volumes, so it works without the optional
System Bridge. Actual owner-service emulator2435 showed5/15as33%, applied
increase/decrease with exact AudioManager readback, observed an external9/15as60%,
and rejected writes after no_adjust_volume was enabled. Original streams, DND
and policy were restored. The disconnected-bridge/missing-state/revoked-authority
UI regression is included in the394passing Home tests.

## Android accessibility adapter

The Settings module now exports its visible compiled controls to a dedicated
Android virtual-node provider when accessibility is enabled. This covers readable
labels, contextual buttons, non-password editors and the active scroll ancestor.
Actions operate the existing widgets: search text edits retain the real editor,
clicks retain confirmation/capability checks, and focus can reopen an editor after
Back dismissed its keyboard. Offscreen controls are omitted until scrolled into
view. System Accessibility preferences themselves remain a separate migration.

The implementation is split across `settings_accessibility.rs` (finite protocol
and identities), `settings_accessibility_ui.rs` (visible widgets/actions),
`settings_accessibility_host.rs` (trusted foreground instance and lifecycle), and
the native `SettingsAccessibility` adapter. Dynamic row identity includes its
observed target, so an Android node retained from a recycled app/account/sound
slot cannot operate the replacement. Page retirement and process-wide request
sequence checks reject stale/replayed actions; a fresh visible/capability check
runs before dispatch. Password fields are never exported.

The rendered-widget tests cover clipping at a fractional density, real scrolling,
live theme/150% font identity, stale app rows, revoked controls, text replacement,
and focus after keyboard dismissal. Build2026092437 passes403 Home tests and33 actual public-UiAutomation checks on
both API35 emulators (Gboard5556 and AOSP5558), including scroll ancestry, editor keyboard reopening,
6.2-second stable focus, native Activity retirement/return and stale-node denial.
The reproducible probe is `home/android/scripts/run-settings-accessibility-probe.py`
with `--scenario smoke` from the Settings overview. It edits only a Search draft
and cleans up its independent fixture APK. Full TalkBack traversal, switch-access
behavior, localization and physical-device acceptance remain separate gates.

## Open a Settings page from Android

The public entry is the `dev.makepad.octosense.SettingsEntry` activity alias. It
uses the existing Home renderer, with no separate launcher icon. Applications can
send the custom action `dev.makepad.octosense.action.SETTINGS` to the Home package
and choose one finite route through
`dev.makepad.octosense.extra.SETTINGS_ROUTE`:

`overview`, `search`, `appearance`, `display`, `display_options`, `sound`, `wifi`, `bluetooth`,
`apps`, `accounts`, `notifications`, `privacy`, `location`, `battery`,
`battery_policy`, `storage`, `date_time`, `about`, `system`, `updates`,
`advanced_network`.

For example, on an explicitly selected emulator:

```sh
adb -s emulator-5556 shell am start \
  -a dev.makepad.octosense.action.SETTINGS -p dev.makepad.octosense \
  --es dev.makepad.octosense.extra.SETTINGS_ROUTE wifi
```

Twenty-one reviewed `android.settings.*` actions on `MakepadApp` also map to these
pages; the alias accepts only the custom action. The exact mapping is in
`SettingsEntryContract.java`. Package-scoped dispatch is deterministic
when native Android Settings is also installed. This does not set a system-wide
default handler. `ACTION_APP_NOTIFICATION_SETTINGS` additionally accepts the
strict `Settings.EXTRA_APP_PACKAGE` navigation selector. It does not accept a
caller-selected UID, channel/highlight, fragment, credential or setting value.
All other actions ignore that package extra. Opening System updates performs no update check,
installation or reboot. Native fallback buttons are explicitly pinned to Android
Settings to avoid loops after OctoSense advertises these actions.

External entries discard old mutation drafts/confirmations and editor focus while
preserving harmless local list/search filters. Back uses the normal page hierarchy.
Cold entries wait for initialization and Resume. Java retains the latest entry
until explicit native readiness/receipt, because a pre-surface JNI enqueue can be
discarded during runtime bootstrap. Duplicate/older entry IDs are ignored and an
intervening Home action cancels pending navigation. The first ready transition also
resends Activity/accessibility state once, so pre-surface observations cannot leave
the accessible tree disabled. Native tree changes emit subtree invalidation as
well as pane announcements.

Build 2026092503 passed six cold starts (25 checks), all 20 custom routes and 19
Android actions, and the 33-check accessibility smoke on both API35 emulators.
Back/native Activity return did not replay an acknowledged entry. Three alternating
main/alias launch pairs retained the same Activity record on the Gboard emulator;
the crash buffer stayed empty. The 408 Home tests cover the finite schema, cold
queue, all routes and absence of mutations. Its existing Bridge was updated with
the same signer/version while preserving data identity and permissions; Bluetooth
fallback opened Android Settings directly.

Quickstep uses `SystemSettings.openPreferred` for ordinary navigation. This only
selects an enabled, exported Home alias targeting the known renderer when Home is
a system app with the platform signature. Missing, disabled, incompatible or
ordinary installations use the native route. Consent destinations, including
notification access and write-settings access, retain their platform screens;
Internet still uses the combined Wi-Fi/mobile panel. `SystemSettings.open` remains
the explicit native recovery API. Standard actions request priority 2 on the real
renderer Activity. Android matches an updated alias against its target first, so
putting these filters only on the alias loses priority after an APK update over
the ROM copy. Build 2026092505 passed all 19 implicit actions, preferred/native
fallbacks and the actual AOSP SystemUI gear after a privileged base-package scan
on the dedicated emulator (46 checks). Reinstalling it as a data update retained
priority 2. Ordinary installs passed their native-default/fallback checks, six
cold starts and all explicit routes. The temporary framework-rescan fixture
preserved the app UID/data and device preferences; a complete production ROM boot,
the customized Quickstep APK and physical hardware remain separate acceptance.


## Display size and Night Light

Display → Display size and Night Light presents real observations and a separate
review with Save/Cancel for each change. Search includes English and Chinese
aliases. The finite `display_options` entry and Android `NIGHT_DISPLAY_SETTINGS`
action navigate here without changing a setting. Night Display uses its own
priority-2 filter on the renderer; adding a new action to the original filter
would cap the existing 19 actions when updating an older ROM base.

The default-display size choices follow the pinned SettingsLib bounds (70–150%,
minimum 9% intervals, at most three choices on either side, larger choices retain
320 logical pixels on the short edge). The interface shows Default or a relative
percentage; current custom values are observations, not new arbitrary write
choices. Default clears the forced density. WMS initial/base density is
authoritative; DisplayInfo supplies actual display type and pixel dimensions.

Night Light uses ColorDisplayManager availability, actual activation, temperature
bounds, schedule mode and custom times. Warmth is offered only while activated;
custom start/end editing is offered only under a custom schedule. HH:MM:SS values
retain seconds. Sunset scheduling requires Location already enabled and never
changes Location implicitly. A successful request with delayed readback is shown
as requested, not applied. Physical color calibration, resolution, refresh-rate
selection and Lineage LiveDisplay remain in Android Settings.

The shared contract exposes no arbitrary display ID, density, settings key or
color matrix. Helper Binder additions 56/57 read bounded state and write one finite
field. The helper alone holds CONTROL_DISPLAY_COLOR_TRANSFORMS; density uses its
existing WRITE_SECURE_SETTINGS. Each 20-second observed key is re-resolved against
actual values, authority and the current unlocked owner before a single claim.
Changes to policy, display geometry, actual values or owner retire old reviews.
The Home client pins the observed helper and never retries writes via a fallback.
Ordinary installations show observed current density and unavailable controls.
Foreground reads run at five-second intervals; paused or replaced Settings roots
retain read-only observations and lose action authority.

Density support required a registered patch to the pinned Makepad runtime:
configuration/safe-inset messages carry the actual density, startup preserves its
latest value, and window geometry is rebased without replacing the Surface,
widget or editor session. Home handles density/smallest-size/screen-layout
configuration changes in place. Two platform tests check physical geometry,
touch coordinates and explicit DPI override recovery. Build2026092506 passed the
25-check actual density scenario on both API35 emulators (420→504→378→420),
including the same process/editor/focus, physical Clear and exact typing. Exact
raw settings state was restored. Accessibility smoke33 passed on both; cold
entries25 also passed on the AOSP emulator.

Final build2026092508 passes 415 Rust tests and 54 repository tests. The supported
owner-service emulator passed 120 actual UI checks: reviewed density changes and
restoration, real Night Light activation and SurfaceFlinger color matrix changes,
warmth, Off, exact custom schedule seconds and the sunset Location gate. After
removing the temporary capability resource overlay and rebooting that owned
emulator, another 24 checks confirmed unsupported gating. All original density,
Night Light and general preferences were restored; the crash buffer was empty.
The ordinary emulator passed 50 entry checks (21 custom and 20 explicit Android
routes plus recovery checks). A temporary privileged2510 base fixture also passed all20 implicit standard
routes, including Night Display, preferred/native recovery and the actual
SystemUI gear (48 checks); a data APK update retained Night Display priority2. Physical tint, full ROM boot and
OnePlus display-driver stress remain separate pending gates.

The new shared-backend tests execute expiry, single claim, forged choice,
external state/capability changes, lock/non-owner denial, actual versus pending
readback, temperature range, Location gating and second-preserving schedules.
UI regressions retain draft text/caret/widget identity across polling and themes,
reject stale held choices and reviews, and preserve nested Back.

## Sound feedback and haptics

Sound → Sound feedback and haptics uses the existing finite Controls channel.
Charging sounds/vibration, screen-lock sounds, dial-pad tones, master vibration,
keyboard vibration and ring/notification/alarm/media/touch intensity each retain
an observed value and only the device-offered choices. Search and the finite
`sound_feedback` navigation route open the same page; Back returns to Sound.
Ordinary installations cannot create privileged write authority.

The helper follows the pinned native controllers: platform-signed Android Settings
resources determine supported levels and charging/lock visibility, actual vibrator
capabilities supply per-usage defaults, and absent keyboard capability stays
unsupported. One-level devices offer Off/Device default; two-level devices offer
Off/Low/High; three-level devices offer Off/Low/Medium/High. Existing custom
values outside those choices remain visible until the user explicitly changes
them. Master vibration gates dependent controls; native ring/notification
intensity is unavailable in silent ringer mode. Do Not Disturb and silent mode
can suppress charging playback without preventing edits to the saved preference.

Ring intensity also writes its legacy ring preference. Touch intensity writes the
legacy touch switch and hardware-touch intensity, retaining the native default
for hardware feedback when touch feedback is off. These are separate provider
writes, with per-write authority checks and readback; partial completion is
reported without replaying the operation. No control plays a preview or claims
that an emulator has physically vibrated. UI state and accessibility targets are
bound to each actual control/value; polling cannot retarget a held choice.

Final2510 validation: 420 Rust tests and55 repository tests pass. The owner-service
emulator passes191 actual UI/service checks, including charging/lock/dial-pad
preferences, master dependency, all five actual Vibrator intensity states,
ring/touch compatibility rows, native one-level choices, unsupported keyboard,
and exact restoration of14 original rows. Ordinary5556 passes51 route checks,
33 accessibility smoke checks and29 checks across seven cold starts. Normal and
150% screenshots confirm that content clears the scrollbar and unavailable
values are not duplicated; original font scale1.0 was restored. A rendered
ScrollForward regression reaches every collapsed row at both scales. The
emulator fixture waits for an observed writable control before paging: Refresh
being enabled is not proof that a fresh snapshot has arrived. Physical feedback,
higher-level device granularity and full ROM boot remain separate gates.


## Per-app notifications

Apps → app detail → Notification settings opens the built-in current-user app
notification page. A stable 20-row catalog contains Android channels and groups;
opening a row retains the app-list query and notification-list position for Back.
The first increment supports app notification permission, group blocking, channel
blocking, and explicit reviewed importance choices. Ordinary Home installations
show unavailable controls and retain a package-scoped Android recovery link.

Only the platform-signed, system-UID SettingsBroker can call the required native
NotificationManager operations. Its new finite service accepts only the exact
signed Agent package and rechecks unlocked owner/admin policy. Agent Binder
transactions58/59 append snapshot/set to the existing interface; the broker has
two methods. Home and Rust separately require a live trusted Settings root,
selected observed app, correlated page and freshly observed opaque targets.
No UID, channel ID, arbitrary notification parcel, permission name, credential,
or package-selected intent is exposed to scripts.

Snapshot generations bind stable sorted row identity. A changed catalog resets
to its first page. Mutation leases bind the app incarnation (UID, install/update
identity and signer), complete native channel/group state, notification permission
flags and policy. They expire after20 seconds, renew on an identical visible-page
observation, and can be claimed once. Every save re-reads Android before writing;
pause, stale replies, failed reads and root retirement clear writable choices.

The UI distinguishes Silent/minimized, Silent, Alerting and Alerting/allow pop-up.
Promotion from a silent lower importance explicitly discloses restoring Android's
default notification sound, matching the pinned native controller. Save copies
the actual native channel, changes only the reviewed fields and retains prior
user-lock bits; user-selected importance is different from system/policy locks.
An app's sole legacy default channel and app permission require two native calls.
Authority/incarnation are checked between them and both fields are read back;
partial or uncertain completion is reported without replay. “Allowed by app and
group” describes saved blocking preferences, not guaranteed delivery under DND.

The unchanged Home UI passes all428 Rust tests; the repository suite now passes58
tests including the legacy-channel backend regression. Contract
tests cover bounds, policy distinctions, observation expiry, changed state,
one-time claims and interrupted linked writes. UI tests cover reviewed
sound changes, theme/font state, stale drafts, recycled-row presses and cached
page-reset replay. Native helper/Broker compilation and17 real boundary checks
per unrelated signer passed on the owned AOSP emulator. Ordinary2512 passes24 checks for unavailable controls, denied mutations, native
recovery and nested Back with the original app filter. A pre-existing native
Settings task exposed a real Back-navigation defect: package notification
recovery now uses an explicit fresh, recents-excluded Android Settings task,
preserving the previous native task and returning to Home. This recovery is
pinned to com.android.settings so future public entry filters cannot loop.
The unchanged Rust UI also passes33 accessibility smoke and29 cold-entry checks
on2511. Owner-service acceptance exposed Android's synthetic null-ID group for
ungrouped channels. The backend now omits that collection marker from actionable
groups while retaining the actual channels. A regression executes the real backend
with legacy and mixed inventories, linked app/channel writes and policy changes;
removing the filter reproduces the observed exception. The updated native service
with Home2512 passes281 actual checks: modern mixed grouped/ungrouped inventory,
app permission and fixed-state denial, channel/group delivery, 20-row paging,
reviewed importance and sound changes with other fields/user locks preserved,
deleted/recreated target rejection, linked legacy app/channel behavior and nested
Back. Synthetic publishers were removed after validation. Channel sound/vibration/badge editing, conversation options, DND bypass,
LED behavior and full ROM/device validation are still separate parity work.

The standard per-app notification entry uses a separate priority-2 intent filter
so an APK update cannot cap older ROM filters. Its package string remains an
untrusted selector until a correlated, fresh current-user PackageManager details
read succeeds. The trusted Settings root and retained entry ID authorize only
that read; there is no `AppTarget` or notification mutation capability while it
is pending. A newer entry or Back rejects the old result. Missing apps open the
non-actionable missing-app details page. Lookup failures keep Refresh available.
Existing apps open the same notification page and reviewed controls as Apps;
Back returns through App details and the preserved Apps filter. Native recovery
remains pinned to Android Settings. Home2513 passes431 Rust tests,119 focused
Settings tests and58 repository tests. On the ordinary API35 emulator the entry
passes33 checks covering cold/warm targets, malformed extras, missing packages,
no replay and native recovery; the existing accessibility smoke also passes33.
Owner-service2513 also passes33 targeted-entry checks; its privileged-base fixture passes53 checks including21 implicit standard actions and data-update priority survival. Restoring ordinary Home retains native fallback5, cold29, accessibility33 and all47 tracked preferences. No phone was modified.


### Default-app roles (emulator validated2515)

Apps → Default apps uses stable Octoscript overview and candidate pages. The
compiled eight-role vocabulary covers Browser, Home, Assistant, Phone, SMS, Call
screening, Call redirection and Wallet. Native availability and qualification
control what appears; unavailable authority never becomes a guessed holder or
an enabled choice. Candidate pages contain at most20 rows, retain their generation,
and use semantic accessibility targets that retire when an app changes. Search
and the finite custom `default_apps` entry reach this page. The standard Android
Defaults action is mapped to navigation only in2516; its independent priority-3 renderer filter competes with native PermissionController priority2. Existing21 priority-2 actions retain their original filters. Ordinary installs keep the native resolver. Preferred routing checks an explicit alias capability so older signed ROM Home builds retain native Defaults recovery. Build2516 passes53 ordinary entry checks,11 targeted Defaults-entry checks and7 native/preferred fallback checks. On the disposable ROM fixture, data2516 over older2513 base retains native priority2/Home0; refreshed2516 base resolves Home3 and retains it after a data update. All22 standard routes and preferred/native recovery pass60 ROM checks. Restored ordinary state passed native fallback7, cold-entry29 and accessibility33 checks;47 preferences,40 roles, three Assistant rows and controller UID/data identity were preserved.

The ROM stages a narrow adapter in the existing PermissionController. It reuses
native `Roles`/`Role`, restriction checks, warning text, confirmation Fragment,
`DefaultAppViewModel` and post-selection behavior. No broad role permission is
added to Home or Agent. An observed choice can only obtain an immutable one-shot
native confirmation token; it cannot directly grant a role. Home and the helper
retain their existing signed-caller checks, plus foreground/current-root gating.
The controller accepts only the exact platform-signed Agent and unlocked owner.

Candidate authority binds native role, package UID/signer/install/update identity,
current holder and restriction state. Page observations expire after20seconds;
review tickets expire after2minutes and survive Activity configuration changes
without replaying a grant. Process death, Cancel, changed policy/holder, removed
or reinstalled candidates invalidate the review. The native positive button
rechecks all of these before the existing ViewModel operation; its asynchronous
success must also match the actual holder and owner before post-selection hooks.
None is offered only where the native role model supports it. Android15 Enhanced
Confirmation/admin-restricted candidates stay disabled; the explicit native
Defaults recovery provides their platform-owned restriction flows. Recovery accepts
only the exact DefaultAppListActivity in the AOSP/Google PermissionController
system packages, with enabled/exported/system-origin checks; Google mainline
modules can have a different signer from the framework. The role adapter and
confirmation PendingIntent still require the exact AOSP controller package.

`roles_snapshot` / `launcher.roles_state` carries only bounded observations;
`role_confirm` carries a finite role and opaque observed keys. Agent Binder
transactions60/61 are append-only. The two controller transactions return an
observation or native PendingIntent, never a general role setter. The native
confirmation and recovery use separate recents-excluded tasks so Back returns to
the preserved OctoSense page even when another PermissionController task exists.
See [the platform audit](settings-default-roles-audit.md) for exact source/API and
reversible adapter validation details. Build2515 passed83 actual checks on the disposable clone: browser Cancel/Back and confirmed A/B choices, stale reinstalled candidate denial, Assistant selection and native None, nested navigation and native recovery. All40 original role holders and the three legacy Assistant rows were restored; fixtures were removed. Both caller signatures failed22 boundary probes. The ordinary Google image passed11 unavailable/recovery checks,52 entry checks,29 cold checks and33 accessibility checks. The unchanged Rust suite passed439 and the repository suite61. Other role behaviors, profile management, production controller updates and full-ROM boot remain separate acceptance gates.

## Common runtime permissions (2521, emulator accepted)

Apps → app details → **Permissions** presents native permission groups and
allowed/ask/denied observations. Selecting a common group opens the group’s
permission choices. The current choice is marked Selected; selecting a different
choice dispatches an immutable, one-use operation to PermissionController.
OctoSense does not optimistically change the displayed permission. Android’s
native model decides whether to apply the choice or show its warning, and the
page refreshes on return. Ask every time and persistent denial are distinct; an
existing one-time grant is observation only. Location precision is separate from
foreground/background access.

The finite initial group set covers camera, microphone, location, contacts,
calendar, phone, call logs, SMS, nearby devices, physical activity and body
sensors, only when the native model actually offers them. Native-only groups
remain visible with Android recovery. Specialized media/selected photos, Health
Connect, virtual devices, special app access and automatic unused-app revocation
are not implemented by this slice. Ordinary Home installations report the
service as unavailable and retain **Android app settings**; they do not gain
permission-grant privileges.

The exported controller service requires the existing signature permission and
the exact signed ROM Agent, current unlocked owner user, an observed package
incarnation and an unchanged native permission fingerprint. The unexported
operation Activity rechecks those conditions before accepting a required warning,
preserves one-use state across rotation and rejects expired/replaced targets.
Home and Agent never receive raw grant/revoke authority. Reads wait for the
native LiveData model on a bounded worker and release observers on completion,
interruption or timeout. Polls never change permissions.

The corrected2521 source passes451 Rust and63 repository tests, including stale
choice/press/accessibility rejection, disabled one-time commands, observed-state
preservation, nested Back/filter and live theme persistence, and narrow150%
layouts. The real backend authority test covers observed/expired/claimed tickets,
warning-time policy/incarnation changes and readback. Pinned controller and Agent
compilation and exact-original-controller package preservation are verified on
the disposable5560 fixture; privileged permission mutation acceptance passes521 checks across all11 common
groups, plus three104-check legacy repetitions. All906 unrelated permission
records across183 packages are preserved. SMS and call-log choices reflect the
native installer exemption; tests did not set exemptions or bypass restrictions.
Hardware and a complete production-ROM boot remain separate validation gates. Ordinary5556 passes25 unavailable/recovery checks,29 cold-entry checks
and33 accessibility/IME smoke checks on2521 without a navigation retry. The
corrected native model waits for fresh inventory and a new
choice-map emission; Home publishes drawn accessibility state before acknowledging
same-page actions. See [the API audit](settings-runtime-permissions-audit.md) and
[platform validation](settings-platform-emulator-validation.md).


## Built-in DND policy and native time schedules

Notifications → Do Not Disturb now contains the four manual modes, saved/default
allowed interruptions and Android time schedules. The policy page distinguishes
saved values from the consolidated current policy; manual Off does not disable
active rules. Calls, messages, conversations, repeated callers, alarms, media,
system sounds, reminders and calendar events have finite observed choices.

Time schedules use Android's own condition provider. Create/edit reviews name,
days, start/end, alarm-exit preference and enabled state, with explicit Save/Cancel
and reviewed deletion. Copies preserve unedited native rule and policy fields.
An external state change disables an old Save while preserving its draft; failed
or pending saves never discard text or replay a mutation. Other provider/calendar
rules retain native recovery and do not acquire unsupported edit actions.

The existing Zen broker has appended transactions2–6, mirrored by Agent64–68.
Only the trusted Settings root may invoke them; Home receives no notification-policy
privilege. Fresh owner, focus, policy, incarnation and one-use lease checks protect
writes. Native compatibility uses the verified Lineage `isActive()` or original
AOSP35 `isAutomaticActive()` method; unavailable status stays unknown, and missing
framework linkage returns unavailable without crashing the broker.

Build2523 adds finite custom `dnd` and standard `ZEN_MODE_SETTINGS` navigation.
Preferred system navigation checks the installed Home DND capability metadata;
old Home builds retain native recovery. The native fallback opens a separate,
recents-excluded task so Back returns correctly with an existing Android Settings
task. Ordinary5556 passed77 DND unavailable/recovery checks,15 direct-entry checks,
33 cold-entry checks across8 launches and33 accessibility/keyboard smoke checks.
Build 2524 also disables the selected manual mode and rejects repeated actions;
the regression fails before the fix and passes afterward. All 458 Rust and 66
repository tests pass. Privileged policy acceptance passes 837 checks covering
all nine fields and independent synthetic notification ranking, with exact
restoration of the captured policy/rule and listener baseline. Android's one-time
normalization of a deprecated visual-effect bit is recorded and independently
verified with native policy equality in the platform validation notes. The native
schedule test passes 210 checks after correcting the short versus full provider
name mismatch. It covers Save/Cancel, edit/delete reviews, empty days, stale
drafts, and actual native start/end with Home stopped, then restores the captured
policy/rule and listener baseline. Per-rule policies/effects, calendar/provider triggers, sender affinity,
managed users, hardware and full ROM validation remain separate parity work.
