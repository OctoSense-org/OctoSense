# Android contracts, System Bridge and Home layout service

These modules implement the first ADR 0001 prototype. `contracts` exports AIDL
interfaces for the ordinary Makepad Home client. `system-bridge` builds the
companion APK. The Gradle `quickstep` target builds the authenticated Home layout
service. The separate platform build now compiles native Quickstep controllers
and Recents. Its ROM-specific module is now active on the OnePlus 6 following
the user's [Trebuchet removal request](../docs/android/trebuchet-removal-record.md).
Basic native navigation and task resume/dismissal pass after reboot; broader
transition validation remains incomplete. See the [ADR](../docs/adr/0001-hybrid-android-launcher-and-system-bridge.md)
and [implementation record](../docs/android/adr-0001-implementation-record.md).

For the current launcher controls and permission flows, see
[System features in the current launcher](../docs/android/current-launcher-system-integration.md).
The native Quickstep build also supplies the opt-in
[OctoSense panel across apps](../docs/android/systemui-shade-replacement.md),
using the same authenticated System Bridge while preserving core SystemUI.
The [native OctoSense SystemUI build](../docs/android/octosense-systemui-build.md)
adds the remaining system-interface fork and records its release-signing gate.
The separate `systemui-preview` Gradle module tests only the device-controls page
under an ordinary temporary app identity; it cannot replace SystemUI.

## Build with installed tools

Pinned dependencies: Gradle 8.11.1, JDK 17, Android API 35, Build Tools 35.0.0,
AGP 8.9.2 and libsu 6.0.0. The launcher script uses an existing Gradle
distribution and does not download a wrapper. SDK auto-installation is disabled.
Dependency locks and SHA-256 verification metadata are checked in normal builds.

On the bench, the explicitly approved tools are installed under
`~/.local/share/octosense/android-tools`. Run from this directory:

```sh
export OCTOSENSE_TOOLS=~/.local/share/octosense/android-tools
export JAVA_HOME="$OCTOSENSE_TOOLS/temurin-17.0.20.1/jdk-17.0.20.1+1/Contents/Home"
export OCTOSENSE_GRADLE_HOME="$OCTOSENSE_TOOLS/gradle-8.11.1"
export GRADLE_USER_HOME="$OCTOSENSE_TOOLS/gradle-cache"
./gradlew --offline :contracts:exportHomeContracts \
  :system-bridge:assemblePrototype :system-bridge:lintPrototype \
  :quickstep:assemblePrototype :quickstep:lintPrototype
```

The ignored `local.properties` must point `sdk.dir` at the installed SDK. The
bench uses `$OCTOSENSE_TOOLS/sdk`. Offline builds require the approved dependencies
to have been resolved already; changing a lock or checksum requires reviewing
the actual dependency change.

Outputs:

- `../resources/android/libs/octosense-contracts.jar` — generated Home classes.
- `system-bridge/build/outputs/apk/prototype/system-bridge-prototype.apk`.
- `system-bridge/build/reports/lint-results-prototype.html`.
- `quickstep/build/outputs/apk/prototype/quickstep-prototype.apk`.

Build the sibling Makepad packager after its source changes, then invoke its
standalone binary from the mobile repository. This builds without installing:

```sh
# From ../makepad:
cargo build --locked --offline --release -p cargo-makepad

# From the OctoSense-mobile repository, with an existing Makepad SDK selected:
../makepad/target/release/cargo-makepad android \
  --sdk-path="$OCTOSENSE_MAKEPAD_SDK" --abi=aarch64 \
  build --release --locked --offline --no-default-features -p octosense
```

The packager compiles `resources/android/java/` and includes JARs from
`resources/android/libs/` in both javac and D8 inputs. Export contracts before
building Home. The framework's existing SDK layout is separate from this
Gradle SDK; a plain cross-target `cargo check` also needs the installed NDK
compiler environment that the packager normally supplies.

## Opt-in phone validation

Normal Home, bridge `prototype` and bridge `release` manifests contain no
instrumentation registration. The validation APKs use the same application
identities and signing relationship; installing them replaces those applications
and preserves their data. Retain a checksum-verified rollback APK and record
device settings before testing a user-owned phone.

Build the bridge runner with `:system-bridge:assembleValidation` and
`:system-bridge:lintValidation`. From the mobile repository, build the Home runner:

```sh
python3 android/scripts/build-home-validation.py \
  --packager ../makepad/target/release/cargo-makepad \
  --sdk "$OCTOSENSE_MAKEPAD_SDK"
```

This derives an ignored validation manifest from the normal Home manifest,
then selects it through `MAKEPAD_ANDROID_MANIFEST_TEMPLATE`. An explicitly
selected missing manifest fails the build. The normal source manifest is never
rewritten. Output is `target/android/home-validation/home-validation.apk`.
The bridge output is
`android/system-bridge/build/outputs/apk/validation/system-bridge-validation.apk`.

After installation is authorized, run these with the selected device serial:

```sh
adb -s "$OCTOSENSE_PHONE" shell am instrument -w \
  dev.makepad.octosense.bridge/dev.makepad.octosense.bridge.validation.CallerRejectionInstrumentation
adb -s "$OCTOSENSE_PHONE" shell am instrument -w -e mode controls \
  dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation
adb -s "$OCTOSENSE_PHONE" shell am instrument -w -e mode placements \
  dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation
adb -s "$OCTOSENSE_PHONE" shell am instrument -w -e mode widgets \
  dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation
```

Require `result=pass` and `INSTRUMENTATION_CODE: -1`, not just ADB exit status.
The control mode changes media volume by one step and restores it in cleanup;
other ungranted controls are tested only for missing-prerequisite results.
The placement mode uses disposable private-cache files and never changes the
user's favorites. These are native API/Binder/storage checks, without UI input
or screen capture; they do not prove layout, pixels, gesture behavior or parity.
Rebuild and install normal APKs afterward to remove the test registrations.

The widget mode uses a separate validation host ID, discovers providers,
allocates and deletes real Android widget IDs, and checks a disposable recovery
journal. It does not bind a provider, accept Android consent or verify a rendered
widget. Cleanup verifies that the validation host's IDs match the initial set.

## Public Home lifecycle validation

The bridge validation APK includes a disposable native launcher activity. With
both validation APKs installed, run `scripts/run-launcher-ui-validation.py` with
`--adb`, `--serial`, `--output`, `--bridge-normal` and `--bridge-validation`.
Retain the normal bridge APK before testing. This runner requires the bench's
already-authorized ADB `su` grant to temporarily disable only
`SystemBridgeService`; it does not request the bridge application's root grant.

The runner holds actual rendered icons, uses native placement-menu items,
recreates Home twice, launches/closes the owned native fixture, and verifies
public launching while the service is disabled. It restores the component's
original default state and checks automatic reconnection. Replacing the bridge
with its normal APK removes the fixture component; replacing it with validation
restores it. The test checks that saved placements survive that unavailable
interval. All placements use a disposable cache journal. Normal bridge cleanup,
owned Home process exit and component restoration are recorded in `results.json`.
A pass requires the Python result plus native instrumentation success; native
instrumentation alone verifies setup/cleanup, not the complete UI sequence.

The fixture's token-protected remote uses an Android abstract Unix socket
forwarded through ADB. It needs no Internet permission and is absent from the
normal bridge APK. Home captures only its own Window or Makepad SurfaceView.
Add `--process-death` to kill the identified owned Home PID after saving its
favorite/dock placements. The runner verifies that the PID exited, then starts
`launcher_resume` instrumentation, which reads the existing isolated journal
without reseeding it. Recovery requires a different PID and session, identical
placements, and both authenticated service connections before the remaining
launch/package lifecycle checks run. The first instrumentation process reports
an expected crash from the intentional SIGKILL; the recovery phase must pass.
The runner records incomplete capture transfers and retries only read-only
captures, at most three attempts. It never retries input/lifecycle requests.

Reboot, profile lock/unlock, native Recents and performance
parity remain outside this sequence.

## Bridge queue recovery validation

The bridge retains the latest notification-listener connection outside its
bounded worker queue. A full queue keeps one coalesced refresh retry; disconnects
clear notification/action state, and queued events from an earlier connection
cannot modify the new connection's notifications. This also distinguishes
reconnections using the same Java service instance. Reconciliation records an
event sequence boundary before querying Android: queued events covered by that
snapshot are skipped, while events received during the query still apply.
Native notification queries and reconciliation run on the bridge worker.

Subscription intent is also retained outside the queue. The worker reconciles
subscribe, replacement and unsubscribe by object identity; commands from a
replaced session are rejected immediately. A callback's Binder death removes
only that subscription, even while the queue is full.

Build `:system-bridge:assembleValidation` and `:system-bridge:lintValidation`,
then run from the mobile repository within the authorized device-test scope:

```sh
python3 android/scripts/run-bridge-queue-validation.py \
  --adb "$OCTOSENSE_ADB" --serial "$OCTOSENSE_PHONE" \
  --validation-apk android/system-bridge/build/outputs/apk/validation/system-bridge-validation.apk \
  --output target/android/adr-0001-artifacts/bridge-queue/new-run
```

The output directory must be new. The runner pulls and verifies the currently
installed bridge APK before replacing it, then restores that exact APK in
cleanup. It compares all three OctoSense package hashes, instrumentation
registrations, Home/Recents identities and recorded settings before and after.
The validation process must exit after restoration.

`BridgeQueueInstrumentation` exercises a private bridge instance with its real
64-entry executor, synthetic listener callbacks and synthetic notification data.
It covers connect/disconnect during saturation, coalescing a lifecycle burst,
replacement-service and same-instance reconnection, stale post/removal events,
access-denied snapshots, ordinary updates and explicit command backpressure.
It also fills the queue with real fixture post/removal events to verify that
snapshot recovery cannot restore an old title, remove a reposted notification
or resurrect a removed one, and preserves updates arriving during the query.
Subscription scenarios include saturated registration, replacement and removal,
plus actual Binder death of a validation-only callback process. Both owned test
processes must exit after restoration. The expanded phone suite passes 807
assertions, including the checks used to fill and drain the queue.
It grants no notification access, posts no real notifications and changes no
device controls. These checks do not establish actual listener permission
revocation, complete notification action delivery or rendering performance.
The normal APK has no instrumentation registration.

## Shortcut pin confirmation

Android resolves `CONFIRM_PIN_SHORTCUT` to the dedicated `ShortcutPinActivity`.
Putting that filter on Makepad's Home activity can create a second renderer when
Android launches confirmation as a separate task. The native activity shows the
shortcut icon, label and publisher with Add/Cancel controls. Platform queries,
acceptance and journal writes run on a bounded worker. Unconfirmed requests
survive configuration recreation; an already submitted request is never replayed.
Finishing confirmation returns to the requesting app, as in the reference
launcher. Home refreshes the pinned catalog and placement journal on resume.

`scripts/run-shortcut-ui-validation.py` builds on the existing validation APKs.
Supply `--adb`, `--serial`, `--output`, and both `--home-normal`/`--home-validation`
and `--bridge-normal`/`--bridge-validation`. It verifies the installed originals,
temporarily installs the validation pair, sends actual `ShortcutManager` requests
from the owned fixture, checks Cancel, native prompt recreation, acceptance and
Home placement persistence, then restores the exact original APKs. A unique
owner ID scopes shortcut cleanup, including its separate recovery mode. The
instrumentation also checks stale placement writers without changing production
favorites. Native captures use only the app's own windows and drawable.

With `--tap-file PATH`, the runner pauses after capturing the rendered shortcut
for up to 60 seconds. Inspect that app-owned capture, then write window-pixel
coordinates as `{"x":919,"y":604}` to the supplied path; those example coordinates
are not a portable locator. The runner requires the launched activity to receive
this run's exact shortcut ID. Alternatively, `--lifecycle` uses read-only
renderer hit bounds and performs the full publisher lifecycle sequence below.
Without either option it does not claim a shortcut launch test. Require both
the Python result and instrumentation success, then inspect the captures.

Home refreshes pinned shortcuts through `LauncherApps` callbacks, including
publisher label/icon changes. Unavailable icons are dimmed; disabled shortcuts
retain the publisher's message, bounded to 512 code points, and explain the
failure when tapped. Dispatch re-queries the exact package/user/pin on Home's
worker so a stale catalog cannot turn a removed or disabled shortcut into an
unqualified launch attempt. Android still owns the final launch/access check.
See [ShortcutManager](https://developer.android.com/reference/android/content/pm/ShortcutManager)
and [LauncherApps](https://developer.android.com/reference/android/content/pm/LauncherApps).

The `--lifecycle` fixture updates only its uniquely named accepted shortcut,
while Home is resumed. It changes the label, icon and launch intent; disables
the pin with a message; verifies a tap is rejected; re-enables and launches the
updated intent. Instrumentation then unpins this run's fixture IDs using the
public launcher API, retaining every unrelated pin. Home must observe removal,
show its stored placement as unavailable, permit Remove from Home, and preserve
that removal through activity recreation. A mismatched cleanup owner is rejected.
This live unpin is a test operation, distinct from the Home placement menu.
Profile locking, cold-process confirmation and reboot require separate tests.

## Android Home placement

Android app/shortcut icons in the drawer, search, Home favorites and dock share
the native badged icon cache. Holding an Android or hosted app icon opens the Home placement
menu: add/remove a favorite, choose a dock position, or remove it from the dock.
Placement uses component/shortcut identity and the Android user serial, stored
in the Home application's private `files/launcher-placements.json`.

All storage runs on the launcher worker. Favorites retain order; unavailable
packages/profiles retain their stored identity and show an unavailable entry.
Hosted apps start on Home according to the launchable catalog; removing one
saves its ID in `hidden_hosted`, while Add to Home clears that exclusion.
Dock placement supports both hosted and Android identities. Moving or removing
an item leaves the old slot empty; the same icon cannot occupy two dock slots.
Live cards keep their separate tap behavior and do not open the icon menu.
Corrupt and newer-version files produce an explicit storage error without
overwriting them or preventing public app discovery. Version 1 journals load
without a disk write and retain native favorites and dock IDs. An actual edit
saves version 2, including hosted exclusions and empty dock slots. Older Home
builds reject a version 2 journal; a production downgrade therefore needs its
retained version 1 journal as well as the APK. Validation uses a disposable cache
journal and does not migrate production placements.

`android/scripts/run-hosted-ui-validation.py` installs the supplied validation
Home/bridge APKs, runs native storage/migration checks, exercises icon menus and
activity recreation, then restores the exact original APKs. Supply `--adb`,
`--serial`, `--output`, `--home-normal`, `--home-validation`, `--bridge-normal`,
`--bridge-validation`, `--app` (an app visible on the first Home and drawer pages), and
`--points PATH`. At each capture checkpoint, inspect the app-owned image and
write its window-pixel coordinates into that JSON map, for example
`{"hosted-home":{"x":160,"y":1475}}`. Coordinates depend on the actual capture.
The next keys are `hosted-dock-first`, `hosted-dock-fourth`, and `hosted-drawer`.
Both the Python result and instrumentation must pass, including restoration.

## Native widget workspace

Hold unused space on Home and choose **Widgets** to open the native workspace.
**Add** lists providers in unlocked profiles. Android owns the binding consent
screen and the provider's configuration activity; the workspace receives their
results through the optional activity extension. **Configure**, **Resize** and
**Remove** operate on the selected widget. **Done**, Back or a new Home intent
closes the workspace. The same Home menu opens Android's wallpaper picker.

`NativeWidgets` hosts actual `AppWidgetHostView` instances. Widget updates,
provider click actions and input remain native. Discovery, ID management,
options and disk storage run on Home's worker; Android view creation/listener
lifecycle and external activity launches run on the Android main thread.
No widget pixel copy or per-frame root operation is used.

The stable production host ID is `0x4f4354`; the validation host is `0x4f4355`.
`files/launcher-widgets.json` records provider component, profile serial, widget
ID, dimensions, ordered placements and a pending bind/configure stage. A pending
addition is written before launching an external screen and can be continued
or cancelled after recreation. Removal clears persistence before deleting the
ID; any resulting orphan can be reclaimed after a successful journal read.
An unreadable or future-version journal prevents cleanup and is preserved.
Activity destruction stops listening and keeps placed IDs.

Resize respects provider resize axes and bounds and reports content dimensions
through widget options, including the responsive-size list on API 31+. Profile
events invalidate visible content before querying access again; stale query
results cannot republish it. Backup is disabled for Home, so cross-install
widget-ID remapping is not claimed.

Each placed widget also occupies a Home page after the app pages. The pager
retains widget identity when the number of app pages changes. A revisioned
snapshot supplies provider availability, and normalized viewport rectangles
position the native views without per-frame provider queries or pixel copies.
The native viewport is limited to the widget's saved size; empty space below
it remains available to Home gestures. The workspace edits the same journal.

Widgets currently hide while the drawer, shade, overview or keyboard is open.
They are separate Android views and are absent from Makepad's blur texture.
Gesture arbitration inside widget content and frame synchronization across the
two layers still need validation. Drag placement, widget pin requests,
auto-advance, dynamic theme colors and phone configuration/resize/profile
revocation proof remain pending.
Opening the wallpaper picker is not wallpaper offset or transition integration.
Follow Android's [widget host responsibilities](https://developer.android.com/develop/ui/views/appwidgets/host).

### Owned widget UI validation

The validation manifest alone includes `octosense.validation=true`. Starting
that APK with the `--remote` boolean intent extra enables a token-protected
loopback endpoint. `/surface` copies its Makepad SurfaceView; `/g` copies the
app Window cropped to the application overlay; `/gq` copies then closes the
owned activity. These are separate layers, not a display/compositor capture.
The normal APK cannot enable this endpoint through intent extras.

With the validation APK already installed under the approved test scope:

```sh
python3 android/scripts/run-widget-ui-validation.py \
  --adb "$OCTOSENSE_ADB" --serial "$OCTOSENSE_PHONE" \
  --output target/android/adr-0001-artifacts/widget-pages/new-run
```

This uses instrumentation mode `widget_ui`, test host `0x4f4356`, an isolated
cache journal and an installed unconfigured `com.android.deskclock` provider.
It temporarily adopts only `BIND_APPWIDGET` and drops that authority before
launching Home. This does not validate the user consent or configuration flow.
The fixture releases its ID, verifies host cleanup and recovers its known
journal after interruption. Unknown allocations or unreadable storage fail
closed. No production widget placement is edited. See Android's
[temporary instrumentation permission API](https://developer.android.com/reference/android/app/UiAutomation#adoptShellPermissionIdentity(java.lang.String...)).

Use `--done-tap X Y` only after inspecting the Done button's center in an
app-owned workspace capture for that device; this additionally tests native
button input. Reinstall the normal Home APK after testing to remove validation
metadata and instrumentation registration.

## Home layout service prototype

`dev.makepad.octosense.quickstep/.HomeIntegrationService` accepts Home geometry
through `IHomeIntegration`. Binding requires the signature permission
`dev.makepad.octosense.permission.BIND_HOME_INTEGRATION`; every transaction also
checks the actual caller's UID, signer, package and Android user. Only the Home
package is allowed. Installing this APK does not configure ROM Recents or add a
Quickstep controller. Both controller and transition capability flags are false.

The version 1 layout uses display-pixel LTRB rectangles, display ID, rotation,
inset distances, a client epoch and a positive revision. It contains at most 128
visible app targets identified by component and user serial. Home publishes
bounds from icons actually drawn with their native texture. Shortcuts, locked
or suspended apps and off-page icons do not provide transition targets.
Protocol 1.1 adds an optional nonnegative `transition_id` to this layout;
omission means `0` for ordinary or legacy publication. A future controller must
require its current positive transition ID before consuming a target. The
worker-side `boundsFor(component, userSerial)` lookup requires an exact match
and returns a defensive copy. The platform adapter's `HomeTransitions` coordinator
uses fresh positive IDs and asynchronously reports bounded lifecycle/progress
events. The SDK prototype does not enable this native endpoint. Neither a schema
check nor the coordinator test establishes native animation behavior.
The service copies and validates geometry before caching it. A matching
`setHomeReady` revision makes the cache usable; malformed geometry, an epoch
change or mismatched readiness invalidates it. Epoch changes and queue overflow
require a new subscription. Final unbind and callback death clear the cache.
Screen/profile/configuration and package changes retire the cached layout's
generation. False readiness also discards the cached layout; a later true
message cannot reuse it. The service reports `layout_invalidated` after external
changes, and Home requests and publishes fresh renderer geometry asynchronously.

Home coalesces asynchronous layout updates on its existing worker. Viewport,
focus, catalog/profile and native-workspace changes invalidate readiness until
fresh matching renderer geometry arrives. The renderer echoes the active
transition ID; retired connection/subscription callbacks cannot change it.
Native controller sources are prepared, but no app-to-Home transition is proven.

With the validation Home installed, run instrumentation modes `home_layout`
and `home_transport` using the runner above. The first validates the schema;
the second exercises real Home-UID Binder ordering, invalidation and resubscribe.
The transport runner also toggles its unexported validation-only activity alias
and restores its original enabled state, checking real package-change broadcasts
and rejection of delayed readiness for the old layout.
The bridge validation runner additionally accepts `-e target quickstep` to
check same-signer foreign-package rejection after Android permits binding.
Add `--geometry --require-quickstep` to `run-widget-ui-validation.py` to require
acknowledged native Clock icon geometry and readiness invalidation when the
widget workspace covers Home. Its favorite is an isolated cache fixture.
With the existing scoped ADB-root test approval, `--invalidate-layout` additionally
sends a protected package-change broadcast only to OctoSense Quickstep and checks
that Home republishes unchanged icon bounds under a new acknowledged revision.
It changes no package and does not notify Home's independent catalog listener.
Restore normal Home and bridge APKs after these tests.

The Quickstep `validation` variant adds an isolated coordinator runner:
`adb shell am instrument -w -r dev.makepad.octosense.quickstep/dev.makepad.octosense.quickstep.TransitionInstrumentation`.
It checks target identity, cancellation, supersession, expiry and queue overflow
with real Android profile/package queries and a test callback sink. It does not
register a gesture controller or change Recents. Restore the normal layout APK
after testing to remove its instrumentation registration.

## Native Quickstep platform build

The platform sources and toolchain use the separately approved Docker setup
described in [the build record](../docs/android/quickstep-build-environment.md).
`platform-build/stage-quickstep.py --tree /build --check` verifies source
preparation against the pinned Trebuchet revision. Writing the separate
`OctoSenseQuickstep` module additionally requires `--baseline-result` from a
passing upstream Quickstep build; it preserves upstream Java and uses its own
package-specific BuildConfig and overview observer. The baseline record must
identify the installed ROM's Trebuchet revision, verified platform source
manifest and SHA-256 of the retained upstream APK. Source preparation alone
does not permit staging.
After staging, the build supervisor holds its build lock while invoking
`stage-quickstep.py --tree /build --verify --baseline-result PATH`. Verification
recomputes the expected source hashes from the current adapter inputs and checks
the staged files, Blueprint fragment and original upstream Java. It performs no
writes and must run before compiling a candidate.

Before deployment, run `platform-build/inspect-quickstep-apk.py` against the
signed candidate, providing the SDK `aapt`/`apksigner` paths and the existing
OctoSense certificate SHA-256. This inspects the actual APK package, services,
provider authorities and Home boundary. It also requires the native controller
endpoint's manifest metadata to be enabled. It is a packaging gate; ROM grants,
SystemUI binding, transitions, rollback and performance require phone evidence.
Native controller deployment is separate from the layout-prototype approval.

`platform-build/package-quickstep-module.py` packages the inspected, signed APK
with a resource-only Recents overlay and a same-partition permission allowlist.
It requires the exact reviewed phone framework APK, the pinned permission
inventory, existing SDK tools and a keystore supplied explicitly. Passwords are
read from named environment variables. It writes a ZIP and reviewable payload
to an empty output directory; it never installs them. The installer is pinned
to the reviewed OnePlus 6 ROM and checks payload hashes. See the exact candidate,
changes, checks and rollback in the
[deployment review](../docs/android/quickstep-deployment-review.md).

## Notification app identity

Home resolves the posting package's label and badged application icon using
[Android PackageManager](https://developer.android.com/reference/android/content/pm/PackageManager).
This runs on Home's worker after an authenticated, current-user bridge snapshot.
Notification-supplied app labels and file paths are replaced. Missing packages
retain their package name and use the fallback icon; their content remains
visible. PNGs are bounded to 192 pixels per side and retained only for packages
in the current notification snapshot. Package/catalog changes invalidate the
metadata cache; unchanged image files are reused.

Rust decodes launcher and notification icons on its bounded worker pool. The
shade cache includes the label, icon identity and icon availability, so an
asynchronous image completion invalidates a stale recorded card. Updates with
the same notification handle retain their card identity while updating text.
The final JSON snapshot is measured in UTF-8 bytes against the JNI limit. If
necessary, oldest entries are omitted with `notifications_truncated=true` while
retaining device state and recent notifications.

`android/scripts/run-notification-ui-validation.py` accepts `--adb`, `--serial`,
`--output`, `--home-normal`, `--home-validation`, `--bridge-normal`,
and `--bridge-validation`. It runs native identity/cache/budget
checks, then uses synthetic card content with real installed-package metadata.
The validation-only, read-only renderer probe reports the actual notification
target bounds, shade-open state and decoded icon availability. The runner taps
the target through app-window input and requires the shade to be fully open
before capturing each card state. Inspect every resulting notification image;
passing model checks alone does not prove correct pixels. The runner restores
the exact original APKs and checks owned
process cleanup. It does not grant notification access, post real notifications,
or dispatch notification actions.

## Notification replies

Actions marked as free-form replies open a native editor in Home. Android owns
text editing, selection and IME input. **Send** submits the draft through the
existing authenticated bridge using a fresh command ID; ordinary notification
actions retain their original behavior. Drafts are memory-only, capped at 2,000
UTF-16 code units and cleared when Home leaves the foreground or the editor
closes. Empty replies and repeated Send taps are rejected.

The editor checks the live action handle again before dispatch. A changed or
removed notification invalidates an unsent draft. A bridge disconnect or a
15-second result timeout after submission displays an uncertain outcome and
does not retry. A matching late completion can resolve that state; completion
only means that the originating app's PendingIntent was dispatched, not that
the recipient received a message. Generation tokens isolate a new editor from
old command results. Native action objects remain inside the bridge.

With the authorized validation Home installed, instrumentation mode
`notification_input` verifies Android RemoteInput delivery to a disposable
in-app broadcast, including Unicode, invalid text, immutable and cancelled
actions. It grants no notification access and contacts no recipient.
`run-reply-ui-validation.py --adb PATH --serial SERIAL --output DIRECTORY`
opens a disposable native editor fixture using mode `reply_ui`. It exercises
the native InputConnection and app-local button input, captures only Home's
Window, and verifies cancellation, stale results, queue pressure and timeouts.
This fixture tests the editor and simulates completion callbacks; it does not
prove the Rust/JNI/Binder round trip or notification-listener consent/lifecycle.
Restore the normal Home APK after validation.

`run-bridge-notification-validation.py` runs the real listener path using
instrumentation mode `notification_roundtrip`. It requires explicit consent and
the `--allow-temporary-notification-access` flag, plus `--adb`, `--serial`,
`--output`, `--home-normal`, `--home-validation`, `--bridge-normal` and
`--bridge-test` (the normal bridge prototype). The new output directory retains
only fixture test results. Incoming snapshots are filtered to the fixture's
unique package/title before the test queues them.

The host grants access only after the fixture observes it revoked, then revokes
it while a fixture notification is still active. Tests cover delivery/update,
Unicode RemoteInput dispatch to an in-app receiver, command deduplication,
expired action handles, individual dismissal and permission-revocation cleanup.
No recipient is contacted and dismiss-all is never used. Cleanup restores the
original APKs, notification-posting permission/AppOp and listener-setting entries;
Android's normalization of existing package-only entries is handled explicitly.
This exercises authenticated Home-UID Binder calls, not the complete shade,
editor, Rust and JNI input path or Android's consent screen.

Add `--ui-flow --visible-seconds 20` to exercise the visible path. This mode
leaves Home's normal bridge subscription in charge: app-local touch opens the
shade, swipes the real card to reveal Reply/Clear, and taps the native Send
button after entering text through Android's InputConnection. The fixture posts
real notifications and receives its own PendingIntent replies; it never injects
a simulated bridge completion or opens the reply editor directly. It also checks
updated/revoked draft invalidation and individual dismissal in both the renderer
and NotificationManager. The three main screens pause for the requested duration.

The opt-in validation build filters unrelated notifications before Home queues
or renders a snapshot. The read-only renderer probe exposes actual card/action
hit bounds, and the remote reports the production editor state. Captures use
PixelCopy on Home's SurfaceView or Window, excluding the system display/keyboard.
The flow driver fetches a bounded in-memory capture in 16 KiB chunks to avoid
large ADB-forward responses. Inspect the saved PNGs before claiming visual
acceptance. Cleanup revokes access, cancels the fixture, restores APKs and posting
permission, removes the owned forward and verifies the test PID exited. Brief
ADB reconnects are retried for idempotent cleanup; input with uncertain delivery
is not replayed.

`--skip-captures` is a diagnostic option, not visual acceptance. On the current
OnePlus bench, both whole-image and chunked PixelCopy capture attempts coincided
with Mac ADB USB read failures. The no-capture run passed the complete UI action
sequence. Retain this distinction in reports until the transport/capture failure
is resolved and the combined-flow pixels are inspected.

Android's [RemoteInput API](https://developer.android.com/reference/android/app/RemoteInput)
defines the native reply payload, and [PendingIntent dispatch](https://developer.android.com/reference/android/app/PendingIntent)
delivers it to the original app.

## Signing and access

`prototype` uses the framework's bundled development key unless a managed
keystore is supplied. That key is public: this variant is for development,
and cannot establish a production signing/security claim. It uses optimized,
non-debuggable code. `release` remains unsigned without a managed keystore.

For a managed bridge build, set `OCTOSENSE_KEYSTORE`,
`OCTOSENSE_KEYSTORE_PASSWORD` and `OCTOSENSE_KEY_ALIAS` in the build environment.
`OCTOSENSE_KEY_PASSWORD` defaults to the store password. Build
`:system-bridge:assembleRelease`. Sign Home with the same keystore and alias
using the packager's `--keystore` and `--keystore-key-alias` options;
`MAKEPAD_KEYSTORE_PASS` supplies its store password. No ROM platform key is
assumed. Keep keystores and passwords outside source control.

The bridge checks actual Binder UID, signer, package and Android user for every
operation. Android owns notification-listener, write-settings, DND, camera
and Bluetooth permission prompts. The bridge's native settings entry point
opens these consent screens. Root binding starts only when the user presses
**Connect optional root controls**; there is no automatic root grant.

The fixed root adapter recognizes API 35, `ro.lineage.device=enchilada` and
LineageOS 22.2. It exposes Wi-Fi, Bluetooth and battery-saver setters through
fixed command arguments. Its own Magisk identity/grant, enforcing-mode behavior
and setter results still require phone validation. Root never receives shell
text from the UI or performs work for each animation frame.

The Bluetooth root control also requires `BLUETOOTH_CONNECT` to observe its
current state. Use the bridge's **Bluetooth and flashlight** consent button.
Permission results and returning from consent screens refresh capabilities on
the bridge worker.
