# ADR 0001 implementation record — 17 September 2026

**Status:** M1/M2 prototype builds and existing logic tests pass; approved phone
installation and initial Binder checks pass. M4 widget pages and their native
workspace have real-provider binding, separate-layer captures, a native Done
button input check and cleanup evidence on the OnePlus 6. The separate Quickstep
package has an authenticated Home layout service with real Binder
and visible-icon geometry evidence. The ROM-matched native Quickstep experiment
passed boot, trusted grants/SystemUI binding, basic Home/cancel/Overview gestures
and normal rollback. Following the user's subsequent Trebuchet removal request,
the reviewed native Quickstep module is active, Trebuchet is uninstalled for user
0, and the updated normal Home and bridge remain installed. Home, native Recents,
owned-task resume and dismissal pass after a further reboot. See the
[migration record](trebuchet-removal-record.md) for exact state and recovery.
The subsequently requested global OctoSense panel now has native edge/window
integration, fixture reply/dismissal, rotation, task-switch and process-recovery
evidence. Its updated module has passed reboot, trusted grants and SystemUI
binding checks; core SystemUI remains running. See the
[global panel record](systemui-shade-replacement.md).
A reported stock-shade escape below the OnePlus notch is fixed in the deployed
v3 panel: all 17 boundary checks pass before and after reboot, including both
landscape orientations; normal app gestures below the boundary are preserved.
The subsequent native SystemUI fork compiles successfully and its device-controls
preview passes phone checks. Its development signer differs from this ROM's
release signer, so the native APK has not replaced the phone's SystemUI. See the
[native build record](octosense-systemui-build.md).
Implementation and broader phone validation remain in progress.
Native notification reply editing, IME visibility and isolated Android reply
dispatch have phone component evidence. Approved real-listener fixture tests
passed delivery, update, dismissal, Unicode reply dispatch, deduplication and
revocation. The real shade/editor/Rust/JNI reply, dismissal and revocation flow
now has phone behavior evidence. Temporary fixture grants were revoked after
testing; the user subsequently approved ongoing notification and write-settings
access, both enabled and verified after reboot. Thirteen rendered settings routes
and the setup hub's live permission status pass on the phone. Pixel capture of
the combined flow remains unverified because of USB read failures.
No milestone exit evidence or native-launcher parity claim is complete.
The [accepted ADR](../adr/0001-hybrid-android-launcher-and-system-bridge.md)
defines the remaining acceptance gates.

## Implemented prototype source

- Android Gradle modules for contracts v1, the System Bridge and the Quickstep
  layout-service prototype. Shared AIDL
  includes typed setters, revisioned snapshots, command results and Home
  coordination interfaces. Home now publishes bounded, revisioned visible-icon
  geometry to the separate service. A ROM-matched upstream Quickstep adapter
  supplies native controllers; exact Home icon-target alignment remains unverified.
- Makepad packager support for app Java and contract JARs, including incremental
  source/library fingerprints and D8 inclusion. APK signing now honors the
  existing keystore options; an unsigned APK cannot be installed by `run`.
- Optional activity extension and bounded JNI integration events. Worker queues
  own launcher queries, icon work, bridge handshakes and device operations.
  JNI returns backpressure; retained events retry while pending, state overflow
  requests a snapshot, and unavailable command outcomes are reported explicitly
  without automatic command replay. Native queue counts reset by generation on
  activity/bootstrap changes.
- Public `LauncherApps` discovery and launching with component/user identities,
  locked/suspended checks, badged icons, package callbacks, pinned shortcuts and
  native shortcut-pin confirmation. Installed apps appear in drawer/search;
  Android favorites and four dock positions now have a private, versioned
  placement store and a native icon hold menu for Android and hosted apps. Storage runs on the launcher
  worker; unknown packages/profiles keep their identities until explicitly
  removed. Corrupt or future-version files are preserved, and storage failure
  does not prevent public catalog discovery. Hosted favorite visibility and dock
  edits persist independently of the live-card layout. Pinned shortcuts use native badged icons and
  pin confirmation also requests persistent Home placement.
- Authenticated bridge operations with actual caller UID, signer, allowed
  package and Android user. Snapshot registration shares the worker update
  boundary. Command deduplication/results are bounded by session and ID.
  Final unbind and Binder death clean up subscriptions.
- Real device state through observers/callbacks, notification-listener models,
  original native `PendingIntent`/`RemoteInput` handles, dismissal and clear-all,
  and public brightness, volume, rotation, DND and flashlight operations.
  Android demo notifications are disabled. Controls reconcile observed state;
  native dismissals wait for notification-listener removal. Free-form reply
  actions now open a native editor with explicit Send, generation-scoped results,
  a bounded memory-only draft and no automatic retry after uncertainty.
- Explicit user-initiated libsu root binding with identity checks, denial/timeout
  retry and disconnect handling. The target adapter uses fixed arguments for
  Wi-Fi, Bluetooth and battery saver. Bluetooth also requires the ordinary
  runtime observation permission before its control becomes accessible; root
  alone must not enable a toggle whose current state cannot be read. Returning
  from bridge consent screens and permission results requests a worker refresh.
  Capabilities remain `validated=false`.
- Native widget workspace in Home, opened from the empty-space hold menu.
  Actual `AppWidgetHostView` instances own provider content and input. The
  worker journals provider/profile/ID/dimensions and pending bind/configure
  stages before launching Android consent or provider configuration. Placed
  IDs survive activity death; cancellation/removal releases IDs, and orphan
  cleanup requires a successfully read journal. Provider resize axes/bounds
  and responsive size options are respected. Profile events invalidate visible
  content before access is queried again. Each widget has a paged Home slot;
  widget identity survives changes in app page count. Revisioned snapshots and
  normalized rectangles position native views, with no per-frame Binder/root
  query or widget pixel copy. The menu also opens the installed wallpaper picker;
  wallpaper offsets/transitions remain pending.

Source statements above require the specific runtime coverage below; they are
not a visual or complete lifecycle verification. Queue
pressure can make a command outcome unavailable; the client requests current
state and reports uncertainty rather than repeating the operation.

## Approved tool installation

The user approved Gradle 8.11.1, API 35, Build Tools 35.0.0, AGP 8.9.2 and libsu
6.0.0 under `~/.local/share/octosense/android-tools`, then approved
Temurin JDK 17.0.20.1+1 after the old JDK's missing native libraries prevented
Gradle from starting. No system-wide Java or SDK settings were changed.

The JDK came from the official
[Temurin release](https://github.com/adoptium/temurin17-binaries/releases/tag/jdk-17.0.20.1%2B1).
Its archive SHA-256 matches the published checksum:
`196d13ba5f10414bef7f6a05a9b3f00edacb18ebacef2b99485db9e2ee18f0e8`.
`java -version` reports `17.0.20.1+1`, and the previously missing instrumentation
and management libraries are present.

Gradle archive SHA-256:
`f397b287023acdba1e9f6fc5ea72d22dd63669d59ed4a289a29b1a76eee151c6`.
The Google base API 35 platform is revision 2, not an extension platform;
its published SHA-1 is `0bb560a90a7a2cbd0dd8348224d518b638fe7949`.
The macOS Build Tools 35.0.0 archive SHA-1 is
`93ab8ce91230e067b5add4bfa79919c52b27f072`.
Installation records, archives and build logs remain under the tools directory.

The existing platform-tools and NDK are reused. SDK automatic downloads are
disabled. Gradle warns that the reused platform-tools directory lacks package
metadata; this is not a missing `adb` executable or an approved upgrade.

See [Android build instructions](../../android/README.md) for reproducible
commands and signing requirements. Normal Gradle builds verify locked dependency
checksums; the prototype uses the framework's public development signer and
must not be treated as a production security validation.

## Validation and artifacts

Final candidate artifacts are under `target/android/adr-0001-artifacts/`
(ignored). `manifest.json` records their SHA-256 hashes. Compilation, lint and
logic tests do not verify GPU output, native transitions or phone permissions.

| Check | Result |
|---|---|
| Gradle 8.11.1 / approved Temurin JDK | Contracts export, optimized bridge prototype APK and Android lint succeed, including an offline rebuild with dependency verification enabled |
| Bridge lint | `No issues found` |
| Standalone modified cargo-makepad | Release build and `cargo check --locked --offline -p cargo-makepad` pass |
| Existing packager tests | 67 passed, 0 failed |
| Mobile logic tests | `cargo test --release --locked --offline --no-default-features --bin octosense`: 271 passed, 0 failed; includes placement schema decoding and widget identity across app-page changes |
| Android native framework check | `cargo check --offline -p makepad-platform --target aarch64-linux-android` passes with existing dead-code warnings |
| Android Home release build | Modified packager completes Rust, javac, D8, resource packaging, alignment and signing; existing framework/hosted-app and Java deprecation warnings remain |
| APK signatures | Home v2/v3 and bridge v2 verify; both have the same certificate SHA-256 digest |
| Packaged integration | Home dex contains the app extension and both System Bridge interfaces; native library exports the JNI integration entry point; bridge includes libsu's `assets/main.jar` |
| Phone native Binder/API checks | 73 assertions pass, including invalid oneway input, missing prerequisites, observed media volume, command deduplication, expired command ID and restoration |
| Phone native placement storage | 13 assertions pass on disposable cache files; no user's favorites are modified |
| Phone widget host/recovery journal | 16 assertions pass; 28 providers discovered, real IDs allocated/deleted under a separate validation host, pending stages and sizes retained, future-schema file preserved, initial host ID set restored |
| Phone widget pages/workspace | Real DeskClock provider bound under temporary test permission; 9 native assertions pass, Home page visibility/geometry, native Done input and reparenting checks pass, separate app Window/Makepad SurfaceView captures inspected, test ID and process released |
| Phone caller package rejection | Same-signer, same-user bridge package is rejected outside the Home allowlist |
| Final installed APKs | Normal Home and bridge prototype hashes match retained artifacts; neither registers instrumentation |
| Device restoration | Every recorded system/secure/global setting, Home role and navigation mode matches the pre-test record |
| Source whitespace | `git diff --check` passes in both workspaces |

While preparing real Binder checks, an invalid-input bug was fixed: validation
in a `oneway` setter previously threw only in the helper process, leaving the
client without a correlated terminal result. Brightness/volume, DND-filter,
notification-handle and reply-length validation now runs within the admitted
operation. The command worker reports `INVALID_ARGUMENT`, retains its bounded
deduplication result, and does not invoke the invalid setter.

Native framework instrumentation runners use the installed Home and bridge
UIDs. Bridge `validation` is a separate optimized build type; bridge `prototype`
and `release` omit its runner entirely. Home validation derives an ignored
manifest from the normal source template using an explicit packager manifest
override. Normal Home has no instrumentation registration. Android's debug-ROM
instrumentation rules are not a production access boundary, so test entries
must remain opt-in. [Android 15 instrumentation implementation](https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/android15-release/services/core/java/com/android/server/am/ActivityManagerService.java).

An earlier plain Android Cargo check failed because NDK Clang was absent from
its environment. The final standalone packager supplies the installed NDK
compiler paths and completes the Android release APK build. This does not
change a failed check into a pass; it records the subsequently verified path.

Current retained artifacts (17 September 2026): APK rows are normal rollback
builds; source/JAR rows contain the later implementation. Per-run records above
and below identify the actual tested APK pairs.

| Artifact | SHA-256 |
|---|---|
| `home-prototype.apk` | `9a13c2aed97d47e233658f787b974009bd49ea4a4c10e4f494eb38f5ef98c5b5` |
| `system-bridge-prototype.apk` | `1a25e7b6e326d94f9abdb24f567484a21fc5a4bb09e301206caff0ed7e02e086` |
| `octosense-contracts.jar` | `c88301383a34982e1a86a0ab15e7b6dedecc65b727720ca018927c95e2e4f338` |
| `mobile-source.patch` | `6e033bf1c4f659f1a18ca2d1c915e21f16dc58eba2c733643483b11997bb644d` |
| `framework-source.patch` | `8e1d0dffb47962e121066aa35c2470355399863cd99911034d4cfbb39e5690e1` |
| `mobile-new-source.tar.gz` | `48e620148964616257ea8ebd05ed29968bd9b2bd1306a665671a33ffe4bd3958` |

Home source base is `45dbbfbf257c05a7c2d5149b21ebe77f5c71013c`;
framework base is `d4502ef1e4d196d2829a07aa137740cb0581e9b4`.
Both include uncommitted source changes; patches exclude Markdown, and the
archive retains new mobile integration source. The mobile patch also preserves
pre-existing work from another session; the records do not imply ownership of
that diff or a committed feature revision. Sibling hosted-app sources are not
archived by this patch set.

Shared prototype certificate digest:
`e96d983997281700b9d0ecc674ec11edec96d433c0fa9ddd5c216479f2b738c9`.
This is the bundled public development key, not a provisioned production key.

## Phone identity and runtime coverage

Both ADB transports were authorized during the read-only checks:
OnePlus 6 `cfb7c9e3` and OnePlus 6T `bf0a4730`. The initial target is the OnePlus 6.
On that phone, Android release is 15, SDK is 35,
`ro.lineage.device=enchilada`, and
`ro.lineage.version=22.2-20260708-NIGHTLY-enchilada`.
Its public fingerprint instead reports
`OnePlus/OnePlus6/OnePlus6:11/RKQ1.201217.002/2111252325:user/release-keys`.
Consequently, the root adapter checks the actual Lineage properties as well
as SDK level; stock-looking fingerprint/device strings cannot select it.

The user explicitly approved installing both prototypes, replacing the running
Home build and running integration tests. Before installation, the existing
Home APK was pulled and checksum-verified, and Home/navigation/permission and
device settings were recorded in `phone-before-tests.json`. App data was not
wiped. The existing instance had PID `7269`; its installed APK SHA-256 is
`d8460d97575012237c712945b4a528f1a911e07d6ac741fd81971883a949c2e5`,
now retained as `rollback-home.apk`. It verifies with the same development
certificate as the candidates. Prototype updates through `adb install -r`
succeeded and Android resumed the default Home. Scoped startup logs contained
no app exception, and service records showed Home UID `10220` bound to bridge
UID `10219` through the signature-protected service.

The final validation control runner passed 73 assertions: protocol/snapshot ordering,
three invalid oneway inputs, explicit missing-root prerequisite, a one-step
media-volume change observed through `AudioManager` and the bridge snapshot,
cached command-ID replay without applying a new value, rejection of an older
uncached ID, and restoration of the original volume. Missing write-settings,
DND, notification-listener and all three root control prerequisites also
returned explicit correlated errors. `LauncherApps` returned
42 launchable activities including Home. The bridge runner passed same-signer,
same-user rejection of a package outside the Home allowlist. This is not
foreign-certificate or cross-user rejection evidence.

The placement runner passed 13 assertions on native Android private-cache
files: favorites persist and deduplicate, distinct user-serial identities remain
distinct, a shortcut identity containing colons round-trips, dock placement and
movement persist, removal restores the hosted default, invalid identity does
not alter storage, and a newer-schema file is rejected without modification.
Serials 0 and 10 are test identities; this does not exercise a real work profile.
The runner deletes its temporary files and never modifies the user's placement
file. The full results and exact validation APK hashes are retained in
`phone-protocol-results.json`. Home's hold menu and drawn icons remain unverified.

The next widget validation build found 28 providers and passed 16 assertions
using real allocated IDs under a separate validation host. It verified pending
bind/configure stages survive journal reopening, provider/profile and resized
dimensions persist, duplicate IDs are rejected, a newer schema remains intact,
and removal releases the ID. The runner compared its host's ID set before and
after cleanup; the scoped service dump showed no OctoSense widget host left.
This exercises native allocation and storage, not provider binding or visible
widget behavior. Artifacts and the exact APK hash are in `widgets/results.json`.
The new Home control flow also passes all 268 existing mobile tests.

Normal and validation Home builds succeed. The first widget build exposed an
incorrect Java API owner for user-badged labels; using PackageManager's
`getUserBadgedLabel` fixed it. The widget-page build retains 14 javac deprecation
warnings, including the validation metadata lookup and four compatibility calls for window insets; none was
suppressed. Bridge source and its prior lint/API evidence are unchanged by the
widget work. No persistent widget consent, wallpaper setting or root grant was
changed. The later UI fixture temporarily adopts `BIND_APPWIDGET` through
instrumentation and drops it before launching Home.

After the initial protocol tests both normal prototypes were reinstalled.
After widget-page validation, normal Home was installed again and restored as
Home: PID `31512`; unchanged bridge PID `23845`. Device package records
and packaged manifests both show no OctoSense instrumentation registration.
Final PID-scoped startup logs show no app fatal exception/panic, and service
records list Home's bound bridge connection.

`phone-after-tests.json` confirmed all recorded system/secure/global values,
Home role and navigation mode matched the initial snapshot. The media-volume
restoration also passed inside the runner. Home bound the bridge again after
instrumentation finished. The initial protocol tests used no UI input; the
widget-page test below uses input within its owned app. No OS screenshots were used. Root,
notification-listener, write-settings and DND access remained ungranted; no
Magisk configuration was edited. Earlier ADB UID 0 access does not prove that
the bridge has its own grant.

### Widget pages: real-provider UI check

The optimized validation APK SHA-256 is
`92010581672dbb073119b5d9becd2048d2b1fb4611c7d4413cfdfd5355f7e520`.
`widget-pages/run-4` records the passing run on `cfb7c9e3`, owned PID `30590`.
The fixture binds `com.android.deskclock/com.android.alarmclock.AnalogAppWidgetProvider`
under host `0x4f4356` and an isolated cache journal. Its temporary instrumentation
permission is dropped before the activity is launched; no Magisk grant or
persistent widget permission is used. This intentionally does not exercise
Android's widget consent screen or a provider configuration activity.

The native runner passes 9 assertions. The app-owned remote additionally
verifies one native view on Home page 2, valid bounds and matching model/layout
revisions, hiding on the app pages, reparenting into the management workspace,
and returning after a real native Done-button tap at inspected window
coordinates `(912, 193)`. A wrong remote token is rejected. `/gq` closes the
owned activity, its PID exits, and the runner releases widget ID 9 and removes
the test journal. A scoped Android widget-service dump contains no OctoSense
host afterward. All recorded settings still match the original snapshot.

Inspected captures show analog-clock pixels in the app Window and the Home
label/background/page indicator in Makepad's separate SurfaceView. These are
actual phone renderings, but separate-layer copies do not prove the final
composited screen, animation alignment or performance parity. The runner uses
no display screenshot, external-app input or notification-content capture.
Reproduction instructions are in `android/README.md`; hashes and full results
are retained in `widget-pages/results.json` and the artifact manifest.

Earlier failed fixture attempts remain in `run-1` through `run-3`: they exposed
singleInstance activity reuse, cleanup ownership tracking and a capture before
the first GPU frame. The final harness handles reuse through the opt-in intent,
tracks the activity through its remote lifetime, bounds drawable retries and
recovers only its own interrupted journal. These failures are not counted as
passing validation.

A read-only probe also resolves the active framework Recents resource to
`com.android.launcher3/com.android.quickstep.RecentsActivity`. The installed
reference APK at
`/system/system_ext/priv-app/TrebuchetQuickStep/TrebuchetQuickStep.apk` was retained
and checksum-verified:
`c24ddf0b1b1a44a1f91bce000522faf197db5a9d8f042f6cf2e9b83ea008a5ea`.
Its package version is 15/code 35; the ROM incremental is `21be58cea4`.
The equivalent lookup under the SystemUI resource namespace failed; the
successful framework lookup is the evidence. No Recents configuration changed.

Source research found an additional feasibility lead: LineageOS 22.2's
`FallbackSwipeHandler` handles third-party Home and attaches a
`gesture_nav_contract_v1` Messenger contract. `GestureNavContract` describes
icon position, an optional icon surface and a finish callback. This suggests a
small external-Home contract experiment against the existing trusted ROM
controller before importing a full Quickstep fork. It is a source-based
inference, not a demonstrated phone transition or a change to the accepted
package boundary. Selecting the ROM controller instead of the ADR's separate
extension would require an explicit superseding architecture decision.
[FallbackSwipeHandler](https://raw.githubusercontent.com/LineageOS/android_packages_apps_Trebuchet/lineage-22.2/quickstep/src/com/android/quickstep/FallbackSwipeHandler.java),
[GestureNavContract](https://raw.githubusercontent.com/LineageOS/android_packages_apps_Trebuchet/lineage-22.2/src/com/android/launcher3/GestureNavContract.java).

### Home layout transport: approved installation and integration checks

The user approved installing and updating `dev.makepad.octosense.quickstep` on
OnePlus 6 `cfb7c9e3` for these tests. The 21,012-byte prototype contains only the
signature-protected `HomeIntegrationService`; its manifest has no Recents
activity or Quickstep controller service. No ROM Recents configuration, navigation
mode or Magisk grant was changed. It uses the existing public development key
and requires no new dependencies. Prototype and release lint report no issues.

Exact test APK SHA-256 values:

| APK | SHA-256 |
|---|---|
| Quickstep layout service | `c6ddfdab93fc821a9f03bbd1ba24205bc3340e5da75474b3cb098d65985837a3` |
| Home validation | `775255f3d484906600821a4336c15f8f229ba15f2c4bafae2f8e6c2ad1e1df14` |

Evidence is retained under `target/android/adr-0001-artifacts/home-layout/`:

- `home_layout-validation.log`: 18 native assertions covering bounded geometry,
  finite/in-viewport rectangles, component/profile identity and defensive copies.
- `home_transport-validation.log`: 75 assertions through the real Home UID
  (`10220`). Ordered callbacks correlate session, service epoch and event
  revision. Stale layout revisions cannot replace a ready layout; wrong readiness
  revisions and malformed layouts invalidate it. A changed client epoch requires
  resubscription before a new layout becomes ready.
- `foreign-package-rejection.log`: bridge UID `10219`, with the same signer and
  a granted signature binding permission, binds successfully but its transaction
  is rejected with `Caller package is not allowed`. This isolates the package
  authorization check; foreign-signer and cross-user cases remain untested.
- `ui-run-3/results.json`: Home and the service agree on revision 16 and one
  Clock icon on Home page 1, user serial 0, display 0, rotation 0. Published
  pixel bounds are `[329.0625, 518.75, 497.8125, 687.5]`; the inspected app-owned
  GPU capture shows the corresponding native Clock texture. The service reports
  Home unready when the native widget workspace covers it. Both controller and
  transition capability flags stay false.
- The same UI run passes 10 native fixture assertions, widget-page/native Done
  input and wrong-token rejection. Owned PID `8249` exits; temporary widget ID
  `12` and isolated widget/favorite journals are cleaned up. Earlier `ui-run-1`
  failed because the fixture expected the Clock icon on page 0; page 0 contains
  hosted favorites. The runner now checks the actual native-icon page. `ui-run-2`
  established local geometry before Quickstep installation.
- All 269 existing mobile tests pass. Home builds retain 25 javac deprecation
  warnings plus existing framework/hosted-app warnings; these were not suppressed.

After testing, normal Home SHA-256
`f11598fbf8ae211c69cce309aa921e73fb573d296d129784c756483599091e32`
was installed and resumed as default Home (PID `8940`). The unchanged normal
bridge was restored (PID `8542`); the layout service remains installed (PID
`7960`). Installed APK hashes match the retained artifacts. Both services list
Home's binding, and scoped startup logs show no fatal exception or panic.
`final-installation.json` confirms the original recorded settings, Home role,
three-button navigation and Trebuchet Recents component are unchanged. No
OctoSense instrumentation registration, widget host or ADB test forward remains.

The renderer publishes actual visible native-app bounds, excluding shortcuts,
locked/suspended apps and hidden/off-page targets. The Java adapter converts
normalized rectangles to display pixels, requires a matching catalog revision,
and invalidates readiness on viewport, focus, lifecycle and native-workspace
changes. A single replaceable pending layout coalesces work on Home's existing
worker. The service's bounded worker validates/copies snapshots and requires
fresh subscription after epoch changes or queue overflow. No per-frame root
operation or synchronous render-thread Binder wait is introduced.

This proves layout transport and specific invalidation paths. It does not prove
trusted SystemUI binding, native app surfaces, app-to-Home animation, swipe/
cancel/reverse behavior, rotation/profile lifecycle handling, helper death/
reconnect, queue saturation or performance parity. The service has no native
controller consuming its cache yet. M3 remains incomplete.

### Notification reply editor and native payload validation

Reply action metadata now survives the Rust presentation model. Tapping a reply
action opens `NativeReplyComposer` instead of invoking its PendingIntent without
text. The editor uses native `EditText`, Android input/selection, a 2,000 UTF-16
unit limit and explicit Send. JNI returns the submission to Rust to allocate a
fresh ordered bridge command ID. The native adapter verifies the current action
handle and matching draft token again before Binder dispatch. Closing Home or
the editor clears its draft. The native overlay invalidates Home geometry while
it covers the launcher.

Repeated Send taps are disabled. An expired unsent action clears the draft;
disconnect or a 15-second timeout after submission reports an uncertain outcome
and never resends automatically. Late completion can resolve that state, while
results for an older editor token cannot alter a new draft. Completion reports
dispatch to the originating app, not delivery to a remote recipient.

On the OnePlus 6, instrumentation mode `notification_input` passes 15 assertions
through the shared native action sender and a disposable in-app PendingIntent.
It verifies Unicode/newline content and free-form source metadata, excludes
choice-only fields, rejects missing/blank/oversized replies and immutable reply
actions, preserves ordinary actions, and observes cancelled PendingIntents.
No notification-listener permission or external message is involved.

The native editor UI run passes text entry through `InputConnection`,
app-local Send taps, duplicate suppression, cancellation, stale-result isolation,
expired drafts, queue pressure, a real 15-second timeout, late completion and
disconnect cases. App-owned Window captures show the editor and uncertainty
message. The fixture supplies completion callbacks; this is component evidence,
not proof of the complete shade/Rust/JNI/Binder reply path. Notification-listener
consent, revocation and real app action delivery remain unverified. Artifacts are
under `target/android/adr-0001-artifacts/notification-reply/`. The final `ui-run-2`
additionally confirms editor focus and Android IME visibility through native
window insets. Owned PID `13703` exits, with no pending fixture cleanup. The
widget regression uses owned PID `13095`, which also exits after fixture cleanup.

All 269 existing mobile tests pass, and contracts plus bridge prototype/release
lint report no issues. The widget/native-Done/Home-geometry regression run also
passes with the changed native-overlay coordination. Home builds retain existing
warnings, including 27 javac deprecation warnings; none was suppressed.

The final UI validation Home SHA-256 is
`69477d133e878af0269d7ad01d6b913753efefabc8789482ec1fb3ab7863fdd5`.
The earlier payload/widget run used
`5d7c35e21619a5a4e18fc95a92dfaec51e32d75a73f329cb8e2a3a6a5b68334c`;
that APK is retained with `ui-run-1`. The final revision adds IME/focus diagnostics
and its UI runner verifies 22 state checkpoints.

Normal Home SHA-256
`b0247bfe5cc1d56f4bedd5746f96759e795122672bc74b30ea83facf33a9e7ef`
and normal bridge SHA-256
`1a25e7b6e326d94f9abdb24f567484a21fc5a4bb09e301206caff0ed7e02e086`
are installed after validation (PIDs `14380` and `12525`). The Quickstep layout
service remains unchanged. `notification-reply/final-installation.json` records
matching installed hashes, restored normal manifests, no fatal startup logs,
Home bindings to both services, no test forwards/widget hosts, and the unchanged
settings, Home role and Trebuchet Recents component.

### Public Home launch, placement UI and helper recovery

The phone sequence now passes 28 UI/state checkpoints and five native
setup/cleanup assertions. Its disposable native activity exists only in the
bridge validation APK. Home uses an isolated cache placement journal throughout;
normal placement storage is not modified. Evidence is retained under
`target/android/adr-0001-artifacts/launcher-ui/`, with the passing launcher run
in `run-7/results.json`.

Verified behavior:

- Holding actual rendered app icons opens the native placement menu. Favorite
  add/remove and dock placement/removal update the stored model and visible
  geometry. App Window captures show the normal and unavailable-app menus.
- Activity recreation closes the open dialog and preserves favorites/dock
  placements. A second recreation preserves a temporarily unavailable identity.
  Both recreations keep the same Home process; this is not a process-death test.
- Tapping the fixture icon launches a real native activity in its own task.
  Closing that owned task returns to usable Home, both with the bridge connected
  and with only `SystemBridgeService` disabled.
- The bench's existing ADB root grant temporarily disables that component and
  restores its original default state. The bridge application's Magisk grant
  remains untested. Home reconnects automatically after the service is enabled.
- Replacing the bridge validation APK with its normal APK removes the fixture
  component from the catalog and published icon geometry while retaining its
  saved identity. Reinstalling validation restores the icon and connection.
- Final removal clears the disposable placements. The owned Home PID `24567`
  exits, fixture tasks close, component overrides are restored, and the runner
  reinstalls the normal bridge APK.

This sequence found a production recovery bug: after a bind failed while the
service was disabled, Home did not bind again when the component became
available. Home now responds to bridge package/component changes for its own
Android user by discarding the stale binding and attempting a fresh authenticated
handshake. It does not replay commands. Placement dialogs also close on activity
pause/destruction, preventing them from surviving recreation.

Earlier failed attempts are retained separately. TCP forwarding to the fixture
repeatedly stalled, including when it was launched independently of Home; the
fixture now uses an Android abstract Unix socket and requires no Internet
permission. Android rejected shell-UID component changes, and `disable-user`
did not actually disable this service; the runner now uses the existing ADB
root grant with `pm disable`, verifies its result and restores `default-state`.
`run-6` records the actual reconnection failure before the production fix;
`run-7` verifies the fix. Native instrumentation success alone is not a pass for
the complete Python UI sequence.

The passing validation Home SHA-256 is
`0ce9dfc170d8c1758c648285600d132d95bafc73b124ebd6db6e7bfda17f9841`.
The revised bridge validation APK SHA-256 is
`ac7cc13fccc09d8dc725e928785e7928a3f49fec1adc18e88a49de8744199c84`.
Bridge validation lint reports no issues. The same Home APK passes the widget
regression (10 native assertions, 11 state checkpoints, actual Done-button
input and authenticated geometry acknowledgement) and reply-editor regression
(22 checkpoints). Their owned PIDs `25151` and `25386` exit cleanly. The 269
existing Rust tests were last run on unchanged Rust source in the preceding
reply implementation; this iteration changes Java and validation scripts.
Existing Home build warnings remain.

The normal Home APK is restored with SHA-256
`4e1130f4d5992bc6872c96b74a1e28612e81dc822e211b5a9adcbaa5fac8e707`
(PID `25967`). Normal bridge and layout-service hashes are unchanged. Their
installed hashes match retained files, normal manifests contain no validation
registration, and the Home/bridge signing certificate matches the existing
installation. `launcher-ui/final-installation.json` records unchanged device
settings, Home role and Trebuchet Recents configuration, both service bindings,
no scoped fatal/window-leak logs, no test forwards/widget hosts, and restored
bridge component defaults.

Profile lock/unlock, shortcut pin UI, reboot, complete notification integration,
native Recents and performance parity are still unverified. Process-death
recovery was subsequently exercised as described below.

### Home process-death recovery

`process-recovery/run-2/results.json` passes 31 UI/state checkpoints and seven
native recovery/setup/cleanup assertions. After the placement UI saved its
favorite and dock entries, the runner verified the owned process name and killed
only Home PID `32693` with SIGKILL using the existing ADB root grant. Its first
instrumentation process reports the expected intentional crash. The new
`launcher_resume` instrumentation reads the same disposable cache journal without
resetting or reseeding it. Home PID `644` has a new session, identical saved
placements, both authenticated service connections and acknowledged ready
geometry. An owned SurfaceView capture shows the restored native dock icon.
The remaining native-launch, bridge-disable/reconnect, package-replacement and
placement-removal sequence also passes. Both Home PIDs exit and fixture tasks
`13246` and `13247` close; the isolated journal is deleted during cleanup.

The first run restored the journal successfully but failed during an incomplete
capture transfer. It remains recorded as a failed overall run. The runner now
retries only read-only captures, at most three attempts, recording each
incomplete transfer. Run 2 also records one truncated capture followed by a
complete transfer. This does not establish the underlying transport cause;
input and lifecycle operations are not retried. No production placement file
is modified, and this sequence does not test reboot or user/profile changes.

Validation Home SHA-256 is
`110cba682743c87d64b1396de6fdcb7b05674144ca81d1d3d03222370079641b`.
The normal Home rebuilt after validation has SHA-256
`9a13c2aed97d47e233658f787b974009bd49ea4a4c10e4f494eb38f5ef98c5b5`
and is restored as PID `1812`. Bridge and Quickstep layout-service APK hashes are
unchanged. `process-recovery/final-installation.json` verifies installed hashes,
normal manifests, the existing signer, both bindings, unchanged settings/Home
role/Trebuchet Recents, no scoped fatal/window-leak logs, no test forwards/widget
hosts and restored component defaults. This iteration changes validation Java
and the runner; production Java and Rust behavior are unchanged. Existing Home
build warnings remain, and no performance or complete milestone gate is claimed.

### Native Quickstep build investigation

The inspected LineageOS `lineage-22.2` branch head is commit
`2bee8237d1bb4bc79c348cc7178a780e44a822a3` (15 July 2026), which is newer than
the installed `22.2-20260708-NIGHTLY-enchilada` build. It is a research reference,
not a verified source match for ROM incremental `21be58cea4`. Read-only probes
found no source manifest at the three checked `/system/etc/` manifest paths.

The upstream tree has an Android Soong `Android.bp` and no root Gradle build.
Its Quickstep library uses platform APIs, SystemUI shared libraries, SettingsLib,
Lineage platform libraries, aconfig flags and generated ProtoLog sources. These
are absent from the installed public SDK and this checkout. The existing small
Gradle layout-service APK therefore cannot become native Quickstep merely by
adding the controller service to its manifest. The user confirmed there is no
existing Linux build host or LineageOS source checkout. The local Mac is ARM64
with 128 GiB RAM and about 1.3 TiB free disk. Its running Docker environment is
Linux ARM64 with 12 CPUs and 32 GiB assigned memory; its existing Ubuntu 24.04
image is also ARM64. Android documents x86-64 Linux, 64 GB RAM and 400 GB free
space for a supported platform build. A native x86-64 Linux host is the supported
route; local Docker AMD64 emulation is an untested alternative that needs an
explicitly approved image/toolchain installation and a feasibility check before
a large source sync. At the initial probe no additional build software had been installed, and no
Docker settings or architecture boundary had changed. The source manifest for
the installed ROM was unresolved at that initial probe. See `quickstep-source-inspection/host-options.json`
and [Android build requirements](https://source.android.com/docs/setup/start/requirements).
The [concrete local setup proposal](quickstep-build-environment.md) and Dockerfile
are prepared. The user approved the pinned Ubuntu AMD64 image, listed Ubuntu
build packages and LineageOS 22.2/AOSP source/toolchain setup (up to 400 GB),
with emulation checked first and existing Docker settings retained. Native
Quickstep deployment remains separate. The AMD64 base preflight and Linux build
image now pass. The platform Java 21.0.4 and Go 1.23.2 compilers both compile and
run a small program under emulation, and Ninja 1.9 runs. The 1,141-project
LineageOS 22.2 checkout is initialized at manifest commit
`52602acb3d7ebae4161cd63310cf53c4d8e197d2`; full source synchronization completed
successfully in 3,255 seconds, using about 123 GB within the 400 GB limit. The
resolved manifest pins all 1,141 projects and has SHA-256
`662f1e898f5a784cd278ec27f41061627b921732acc4b260ca4c4b20b79191e5`.
Platform Clang 19.0.1/LLD also passes a compile/run check. The first Soong
preflight selected Baklava preview flags through `trunk_staging`; that attempt
was stopped and retained. The corrected recipe uses `bp1a` and checks Android
15/API 35/REL build variables before compiling. Soong disables its inner nsjail
sandbox after a failed self-test inside Docker; container privileges and Docker
settings remain unchanged. The plain AOSP product also lacked the Lineage board
exports needed by global generators. The corrected recipe now uses the included
`lineage_gsi_arm64-bp1a-userdebug` product through the tree's environment setup
and `lunch`, with an effective-release/API/product gate. That configuration
passed, but the full Soong graph exceeded the container's 24 GiB limit; Docker
recorded an OOM event. The failed attempt and evidence are retained. A retry
uses two pinned CPUs and Go heap controls, with a small recorded Soong patch
that forwards those controls through its cleared environment. The supervisor
now captures cgroup memory/CPU telemetry. The retry completed Soong analysis
after the container alone was raised from 24 to 28 GiB; its peak was about
24.9 GiB with zero OOM events. Docker VM settings remain unchanged. The live
legacy Make parser was stopped intentionally to switch to the installed ROM's
sources and the supported `--soong-only` module mode. That feasibility run did
not pass its full preflight; its explicit stop reason is retained.
A later read of `/product/etc/build-manifest.xml` recovered the installed ROM's
1,145-project manifest (SHA-256
`eaecfdae859bde5555cfd1589e63ecb8b26b5be96ebdb921122c81c738f23014`).
Compared with the initial checkout, 1,072 revisions matched, 69 differed and
four OnePlus device/kernel projects were missing. The ROM pins Trebuchet at
`dfc9b2347e903ad6930274dafcfdba3e594074ab` and the framework at
`ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b`. The inspected Trebuchet adapter
source/build files are identical between the two revisions. Alignment completed
in 287 seconds; all 1,145 project identities and revisions now match the phone's
manifest. The resolved checkout manifest SHA-256 is
`9d08129ad7be4d2ef24b5596790023f4996aeb37170298a9c2882c21f236ef66`.
The new build audits revisions and working-tree edits before compilation, allowing
only the recorded Soong memory-control patch. All 977 Soong-only `aapt2` build
actions completed without an OOM. The post-build probe used the wrong output
directory; the compiled Linux tool runs from the corrected Soong path and reports
version `2.19-octosense-adr0001`. The failed probe and original recipe are retained
in `upstream-build-soong-output-path-check/`; the corrected preflight passed in
587 seconds with no OOM. The initial APK attempt required the image build's
`product_packages.txt` for dex-preoptimization. Its failure is retained in
`upstream-build-soong-dexpreopt-dependency/`. The APK-only retry uses Android's
supported `WITH_DEXPREOPT=false` configuration, retaining Java/DEX/R8/resource
compilation. That retry reached 346 of 9,227 build actions before the user
requested the native x86 Linux host. The emulated process was stopped cleanly
with SIGTERM and zero OOM events; sources and build cache are retained. The
first provided host was reachable, but rejected the supplied SSH public key after its
local permissions were corrected. The user subsequently supplied
`ubuntu@54.198.120.231` with `octosense.pem`; authentication succeeded. The
approved Ubuntu builder userspace is now running natively on that 72-CPU,
approximately 139-GiB host. Sync completed in 923 seconds and all 1,145 projects
pass the ROM source audit. The native `aapt2` preflight passed in 324 seconds,
with all 977 actions complete and zero OOM events. The upstream and OctoSense
adapter APK builds subsequently pass, as recorded below. The retained log
includes the same nonfatal Windows COFF strip diagnostic.
Its generic product does not reproduce the OnePlus ROM configuration;
native APK packaging passes, while phone transitions remain unvalidated.
The manifest, device identity and comparison are retained under
`quickstep-source-inspection/rom-manifest-lookup/`. The phone was unchanged.
Installation records are under
`~/.local/share/octosense/android-platform-build`.
[Pinned upstream build definition](https://github.com/LineageOS/android_packages_apps_Trebuchet/blob/2bee8237d1bb4bc79c348cc7178a780e44a822a3/Android.bp).

The controller's fallback path provides a concrete integration point: its native
`FallbackSwipeHandler` animates a separate Home and can retarget to a supplied
icon rectangle. The prepared adaptation consumes the authenticated layout cache
there and preserves native spring/surface handling. Compilation now passes;
actual ROM binding/grants remain to be proven. Research sources and hashes are retained in
`target/android/adr-0001-artifacts/quickstep-source-inspection/inspection.json`.
[Pinned fallback controller](https://github.com/LineageOS/android_packages_apps_Trebuchet/blob/2bee8237d1bb4bc79c348cc7178a780e44a822a3/quickstep/src/com/android/quickstep/FallbackSwipeHandler.java).

The separate `OctoSenseQuickstep` Soong module is prepared under
`android/platform-build/quickstep/`. Its stager checks the exact Trebuchet
revision and prepares separate copies of `BuildConfig` and the overview
observer, while preserving the upstream sources. The observer handles a missing
integrated Home and temporarily null default Home; BuildConfig and manifest
placeholders use the OctoSense package so provider calls do not target stock
Trebuchet. Source preparation, staging verification and compilation pass; the
native module has not been installed. The stager pins the phone's exact Trebuchet revision; its
read-only check passes and its failed-baseline rejection check passes. Staging
writes require a passing upstream baseline, ROM source provenance, a matching
retained APK hash and the shared build lock. The binary-manifest/signer inspector correctly rejects the existing
layout-only APK for its missing native controller/Recents components, and passes
the newly compiled, locally signed native APK. Actual gesture/surface behavior, grants/binding, deployment
and phone validation remain pending. See the
[build and staging record](quickstep-build-environment.md#separate-recents-module-preparation).

### Transition contract validation — 17 September 2026

Protocol 1.1 extends the version 1 layout with an optional nonnegative
`transition_id`. Missing IDs decode as zero for legacy/ordinary publications;
negative or incorrectly typed values are rejected. Target lookup matches the
canonical component and user serial and returns copied bounds. At this initial
schema-validation step, Home callbacks and native target consumption were still
pending; the later coordinator work is recorded below.

The contract AAR, exported Home JAR and layout-service prototype build offline;
prototype lint reports no issues. The release validation Home passes 27 schema
assertions and 75 Home-UID transport assertions on OnePlus 6 `cfb7c9e3`. The
transport tests run against the previously installed protocol 1.0 service.
The new layout-service APK was built but not installed. Validation Home SHA-256:
`ee41d410d9588aed892889a1b43b2340ad55b104c0dc15ee59fbbe97ca67b917`.
Its build retains 27 Java deprecation warnings and existing Rust/hosted-app
warnings; none were suppressed.

After testing, the retained normal Home APK
`9a13c2aed97d47e233658f787b974009bd49ea4a4c10e4f494eb38f5ef98c5b5`
was restored and resumed (PID `21230`). Installed Home, Bridge and layout-service
hashes match their previous normal artifacts. The recorded settings and Home
role are unchanged, and OctoSense instrumentation is no longer registered or
active. Evidence is under
`target/android/adr-0001-artifacts/transition-contract/`. No native transition or
rendering performance claim follows from these contract tests.

### Native geometry coordinator — 17 September 2026

The source adapter now connects the separate Home service to the native
`FallbackSwipeHandler` spring target. It retains a factory reference across Home
task appearance so cancellation and handler replacement still terminate the
correct transition. Superseded/finished/cancelled requests cannot regain a target.
PiP and split cases preserve their native paths. The staged source check covers
17 files; Blueprint and XML parsing pass. This native controller copy has not yet
compiled in the platform build or run on the phone.

`HomeTransitions` verifies the actual user serial, unlocked profile and enabled,
unsuspended activity on the bounded service worker. A target also requires the
matching session, positive transition ID, ready layout revision, component,
display and rotation. It expires after one second; the transition has a
five-second deadline. Native frames read primitive immutable bounds and write
one progress scalar. Changed progress is delivered from the worker at most
20 times per second, with one pending progress task. Terminal events stop that
timer. Home rejects callbacks from retired connections/subscriptions, asks its
renderer for fresh geometry at lifecycle boundaries and requires the renderer's
transition ID to match before publication. Rust records progress without
requesting a render for each progress event.

The SDK layout prototype retains false controller and validation flags. Only the
platform manifest enables the native endpoint, while validation remains false.
Both Gradle prototype/validation variants build offline and lint without issues;
the release Home APK builds with the retained 27 Java deprecation warnings and
existing hosted-app warnings. All 269 existing Rust tests pass in release mode.

The final progress-worker revision passes 191 phone assertions in
`native-transition/worker-progress/results.json`: 74 coordinator checks, 42
schema checks and 75 Home-UID transport checks. Coordinator checks cover wrong
component/profile/display/rotation/epoch/token, invalidation, expiry, cancellation
before worker start, supersession, terminal ordering and a full bounded queue.
These use real Android profile/package queries and a test lifecycle sink; they
do not prove native SurfaceControl animation or end-to-end transition callbacks.

| Validation APK | SHA-256 |
|---|---|
| Final Quickstep validation | `76b04db5dbac5021a6999fc3b221b22094049b8f7511b2878a0fb55640c90858` |
| Home validation | `840e5e71e7dfdf1db91727451f7452d7c61da44e6f3bf5219c8eff41cce78718` |

The Home geometry regression passes 11 state checkpoints and 10 native widget
assertions in `native-transition/geometry-regression/`. The app-owned GPU capture
shows the Clock icon acknowledged by the layout service; the native-view capture
shows the Clock widget. Covering Home invalidates both local and service readiness.
These are separate drawable captures, not composed system-display evidence.
The fixture released widget ID 15, its owned PID 27104 exited and its ADB forward
was removed. This run did not test the native Done button, consent UI or parity.
It used the preceding coordinator APK; the final change moves progress scheduling
off frames and is covered by the later 191-check run.

Every phone run restored the retained normal Home `9a13c2ae…` and original layout
prototype `c6ddfdab…` with exact hashes checked. The source implementation is ahead
of those restored APKs. Native controller deployment, actual swipe/cancel/reverse,
profile lifecycle, rollback and performance gates remain incomplete.

### Layout lifecycle invalidation — 17 September 2026

The service previously retained a cached layout after lifecycle invalidation.
A later readiness message could make it usable again without new geometry.
Invalidation now advances a generation, clears the cached layout and immediately
invalidates the transition target. Readiness requires a layout from the current
generation. Screen/profile/configuration and package-change events use this
boundary; Home asynchronously requests fresh renderer geometry when the service
reports `layout_invalidated`. A false readiness message also retires its cached
layout. These changes add no per-frame Binder calls or shell work.

The updated SDK prototype and validation APK build offline and lint without
issues. The release Home validation build retains the existing 27 Java
deprecation warnings and hosted-app warnings. On OnePlus 6, 238 assertions pass:
74 coordinator, 42 schema and 122 Home-UID transport checks. The transport checks
toggle an unexported, validation-only activity alias to cause real Android
package-change broadcasts, reject delayed readiness for the retired revision,
accept a fresh publication and restore the alias's original state.

The running Home also passes 12 geometry/widget state checkpoints and 10 widget
assertions. A protected broadcast sent through the previously authorized ADB
root session targets only OctoSense Quickstep, so Home's independent package
listener cannot mask a missing service callback. Home advances generation
13 to 14 and acknowledged layout revision 16 to 18, preserving the Clock icon's
exact bounds. This broadcast changes no installed package. App-owned captures
show the icon and the separate native widget layer; they do not prove composed
system animation. Widget ID 16 is released, owned PID 1526 exits and the ADB
forward is removed.

Evidence is under `native-transition/lifecycle-invalidation/`. Validation Home
SHA-256 is `3c90d9b4ce2c0fc7e68db88816522e792827b34f5aa3dc41bcf1206ab6cfbfd6`;
Quickstep validation is
`16fc274fa25b62f249696d074f5ab66293e926125156cbf9713d0e9df954a3fa`;
the layout prototype used for the renderer check is
`6a503b0665d47dbdd6a2722739d16bdb766720213c3825290c1cb1ea0f8c2a4c`.
Both runs restore the retained normal Home and original layout-service APKs.
Native controllers, profile lock/unlock, transition animation and parity remain
unvalidated.

A subsequent check covers a delayed false-readiness message from a retired
session. Because invalidation happens immediately, the worker now notifies the
current subscriber when that old message has invalidated its cache, allowing
fresh publication without replacing the current session. The final phone run
passes 258 assertions (74 coordinator, 42 schema, 142 transport) in
`native-transition/retired-session/results.json`. Its Quickstep validation SHA-256
is `b68dd9ceac75bf184c515933a199f3bb249c74f0332a3d25b8e55d2dbb3aee57`;
Home validation is
`8d7ee04a44ca6a81ab5cce29cbb49ccbb5fe906f97adef89c69f0e15d7948d8d`.
The prior geometry run covers the unchanged Home invalidation handler; the last
service change is exercised by the new transport assertions. Both normal APKs
are restored with exact hashes checked. The Home rebuild initially stopped on
a lockfile mismatch after a sibling app removed `makepad-plot`; offline Cargo
resolution removed only that unused package/edge. The lockfile delta and failed
attempt are retained alongside the successful rebuild.

### Native APK and deployment package — 17 September 2026

The native Linux host completes both targets. After a disk-sampling supervisor
error interrupted the first upstream attempt, the corrected monitor resumes its
cache and completes the remaining 575 actions in 337 seconds. The upstream APK
SHA-256 is `e2313039cd20832e3d10e8d0256a63510dccc6f84a2ea168d7bf25af2a19c9c7`.
The adapter then passes its 1,145-project/source-file audit and all 46 build
actions in 296 seconds, with a 34,753,990,656-byte peak and zero OOM events.
Both successful units exit with status zero; the first interrupted attempt and
its monitor diagnostics remain separately retained. These are incremental build
times, not full clean-build benchmarks.

The build-signed adapter is `3a7f3a755061f690e653deade4e6c0f96d4a0965659be5e389eb7b5135897959`.
Local signing with the existing development key produces
`fba417a89253cec42274d07077a757d3a565349e9a8f11b19de6c2e9374e0ee0`.
Its actual binary manifest/signature pass the native packaging checks, including
the controller metadata, protected TouchInteractionService, Recents activity,
provider authorities and separate Home boundary. A normal release Home rebuild
also passes, with SHA-256
`404948af9728852804c843c036661454ac8126dcbbf6c177c550725f82370c10`;
its manifest has no instrumentation or validation alias/metadata. Existing
compiler and launcher-icon warnings remain in the logs.

The prepared Magisk module is
`427ac446fbfbdb7d094481428bed62d02e9ca44707d345b3321876a1716ff329`.
It mounts the native APK, a resource-only Recents overlay and a same-partition
allowlist derived from the APK and pinned ROM permission definitions. Its
installer checks ROM/framework identity and payload hashes. Archive integrity,
APK alignment, installer syntax and single-resource mapping checks pass. The
phone's existing `idmap2` also resolves that mapping against its installed
framework with an explicitly supplied system policy. This is an offline file
check, not evidence of overlay activation or actual privilege grants; all owned
temporary files are removed.

Evidence is under `native-transition/native-package/` and
`quickstep-source-inspection/platform-setup/native-host/remote-exports/`.
The [deployment review](quickstep-deployment-review.md) lists exact artifacts,
system changes, permissions, acceptance checks and recovery steps. Approval for
native deployment, Recents configuration, reboots and gesture-mode testing is
pending. The phone still has the original Home and layout-prototype APKs,
Trebuchet Recents, three-button navigation and unchanged recorded settings.
No milestone is complete; actual bindings, native transitions, rollback and
performance have not been validated by compiling these artifacts.

### Public shortcut pinning — 17 September 2026

An actual `ShortcutManager.requestPinShortcut` call exposed a packaging problem:
resolving confirmation to the Makepad activity started a second renderer in the
same process. The failed first run is retained in `shortcut-pinning/run-1/`; it
restores both normal APKs and removes its owned fixture state.

The confirmation filter now belongs to a dedicated native `ShortcutPinActivity`.
It displays the requested badged icon, label, publisher and native Add/Cancel
buttons. It performs platform queries, acceptance and persistence on a bounded
worker. Unconfirmed requests survive native activity recreation. A submitted
request is owned by its admitted worker and is not replayed by a new activity.
Home remains a single renderer and reloads its placement journal when refreshing
the catalog. Home and pin confirmation serialize journal writes on workers and
reload before mutation, so an older writer cannot drop a newly pinned favorite
or revive a removed one. No render/input thread takes this storage lock.

The final release validation pair passes **21 Android assertions and 13 recorded
UI state checkpoints** in `shortcut-pinning/run-2/`. Real publisher callbacks and
`ShortcutManager` state prove that Cancel and recreation followed by Cancel do
not pin; Add does pin. The pending native prompt survives recreation without
changing Home's renderer session. The accepted placement survives Home activity
recreation. App-owned native/GPU captures show the requested compass icon in
the prompt and Home. Tapping that inspected icon launches the fixture with the
exact shortcut ID carried by its configured intent. These are public APIs; no
root grant, notification access or native Quickstep deployment is used.

Home validation SHA-256 is
`98b868d00a650d865acdb6ab7fe6bb6b57ea52581c9683f8f760205373ffddbc`;
bridge validation is
`04bef4eb28d961fe7a2d4294b0500562f113d0a6f44a463534b2ab65ec124d43`.
The bridge build/lint passes; Home retains the existing 27 Java deprecation and
hosted-app warnings. The test cleans only its uniquely named shortcuts, closes
the fixture tasks, removes its forwards and verifies owned PIDs 18337/18252 have
exited. Exact original Home/bridge APKs are restored; Quickstep's original layout
APK remains installed. `shortcut-pinning/final-installation.json` confirms all
recorded settings, Home role and Trebuchet Recents are unchanged, with no test
instrumentation, widget hosts or forwards remaining.

This verifies the basic current-user pin/icon/launch path. Profile lock/unlock,
publisher shortcut updates/removal, cold-process confirmation, reboot and broader
launcher lifecycle/performance gates remain. The native deployment review retains
its earlier exact Home APK; these newer public-Home changes are separate source
and validation artifacts. No milestone is complete.

### Hosted placement and drawer spacing — 17 September 2026

Home now supports hosted icon Add/Remove Home, dock placement, movement and
removal through the native menu. Version 2 stores hosted exclusions and allows
empty dock slots. The existing native component/profile/shortcut identities
remain intact. Version 1 journals are read without rewriting; the next actual
edit migrates them. Worker serialization and reload-before-write preserve
another writer's changes, and future versions remain untouched. An older Home
build cannot read a migrated version 2 journal; production downgrades must retain
and restore the matching journal. These tests use a separate cache journal, so
production placements were not migrated.

Live-card taps have a separate hit type and do not expose icon placement
commands. The actual drawer capture also exposed a row-spacing defect: its
minimum height was smaller than an icon plus its label. Rows now reserve label
and touch spacing and scroll instead of overlapping.

Evidence is under `target/android/adr-0001-artifacts/hosted-placements/`:

- `run-5/results.json`: **28 native storage assertions, 5 fixture lifecycle
  assertions, and 14 UI state checkpoints pass** on OnePlus 6 `cfb7c9e3`.
  AppCard moves from Home to dock 1 and then dock 4, retains its hidden-favorite
  and dock state through activity recreation, is removed from the dock, and is
  added back from the drawer. The native fixture favorite is preserved.
- `run-5/visual-review.json`: app-owned GPU/Window captures show the correct
  menu, persisted dock position, removed/restored favorite icon and readable
  drawer rows. The live cards retain their independent layout. Whole-system
  composition and transition performance are outside this check.
- Storage checks cover native profile/shortcut retention, stale writers,
  hosted hide/restore, dock uniqueness, empty slots, v1 migration without a
  read-time write, and future-schema protection including a stale writer.
  The release/offline Rust suite passes **271 tests**.
- Home validation SHA-256:
  `719c68bd2e4ce4e7c38cc8ef7e3fa80ca8bfba33e58fa0ce097d994b7be2e5f5`.
  Bridge validation SHA-256:
  `04bef4eb28d961fe7a2d4294b0500562f113d0a6f44a463534b2ab65ec124d43`.
  `builds.json` records the relevant source hashes; build warnings remain.

Earlier attempts remain recorded: `run-1` was stopped before placement input
because the selected app was on another page; `run-2` and `run-3` failed on
truncated drawer PNG transfers. The validation socket now waits, on its worker
and with a timeout, for the client to finish reading before closing. `run-4`
transferred the complete drawer but stopped when its selected icon was below the
visible area. The final AppCard scenario requires no scrolling and has no
capture-transfer failures. Each attempt restored the exact original APKs.

`final-installation.json` confirms the original Home/bridge/Quickstep APK hashes,
unchanged recorded settings, Home role and Trebuchet Recents, no validation
instrumentation/forwards/widget hosts, and no owned-process fatal/window-leak
logs. Test Home PID `25647` exited. No native deployment, root grant, reboot,
process-death or performance gate is claimed by this scenario. The separately
reviewed native deployment candidates remain unchanged.

### Notification app identity — 17 September 2026

Home now resolves each current-user notification's app label and badged icon
through PackageManager on its existing worker. The decorator replaces supplied
identity fields, preserves the original snapshot and bounds PNG dimensions to
192 pixels. Package/catalog changes invalidate metadata; unused notification
icons are removed from the dedicated cache. Missing packages retain their name
and the fallback icon. Rust decodes images on its bounded pool, keeps existing
card IDs through content updates and invalidates the recorded shade when a
label, icon or decoded-image availability changes.

The final JSON envelope is also bounded by UTF-8 bytes, because the Binder
Parcel budget alone does not enforce the JNI limit. When needed, the decorator
omits oldest entries and sets `notifications_truncated`, retaining device state
and recent notifications. The boundary test uses long emoji strings that exceed
the limit before trimming.

Evidence is under `target/android/adr-0001-artifacts/notification-identity/`:

- `run-4/results.json` and `instrumentation.log`: **20 Android assertions**
  pass, covering identity/cache/budget behavior and fixture setup/cleanup.
  **272 release/offline Rust tests** pass, including shade-cache invalidation.
- `run-4/visual-review.json`: all four owned GPU captures pass review. The open
  shade shows the installed “OctoSense System Bridge” label and wrench icon,
  updates the same card's title/body, shows a missing-package bell fallback,
  and displays “No notifications” after removal.
- The read-only validation probe reports actual Rust hit bounds, shade state
  and decoded icon availability. App-window input at the registered target
  center opens the shade; each capture requires open state of at least 0.99.
  The fixture does not open the shade programmatically.
- Home validation SHA-256:
  `69f1e452b0e9dded151f00dbce9d6c4feb5e901281860dbbccb05189c3c2f1af`.
  Bridge validation remains
  `04bef4eb28d961fe7a2d4294b0500562f113d0a6f44a463534b2ab65ec124d43`.
  `builds.json` records source hashes; the Home build retains 35 Java
  deprecation warnings and hosted-app warnings.

`run-1` failed while transferring its first capture. Runs 2 and 3 passed metadata
checks but their images still showed Home, so their visual checks are failures.
The measured SurfaceView/window offset was zero; it did not explain those
missed taps. Run 4 derives coordinates from the registered renderer target and
requires fresh, token-correlated renderer state. These earlier attempts remain
separately recorded.

`final-installation.json` verifies exact original Home/bridge/Quickstep APKs,
unchanged recorded settings, Home role and Trebuchet Recents. Owned Home PID
`1179` exited, fixture caches were removed, and no validation instrumentation,
ADB forwards, OctoSense widget hosts or owned fatal/window-leak logs remain.
This uses synthetic notification content with real installed-package identity.
Notification access, real listener delivery/actions/replies and performance
parity remain unverified. No native deployment or new root grant occurred;
the reviewed deployment candidates remain unchanged. No milestone is complete.

### Notification listener queue recovery — 17 September 2026

The bridge's original bounded queue could reject a listener connect/disconnect
callback and lose the only record of the change. It now retains the latest
connection independently of the queue, reconciles it on the worker, and keeps
one coalesced refresh retry until accepted. Each connection has a separate
identity, including reconnections of the same Java service instance. Queued
post/removal events from an earlier connection cannot alter the replacement
connection's notifications. Disconnect/reconciliation clears old action handles.

`target/android/adr-0001-artifacts/bridge-queue/run-1/results.json` records a
passing OnePlus 6 `cfb7c9e3` run with **371 assertions**, including fixture/queue
capacity checks. The instrumentation uses a private BridgeState instance, the
actual 64-entry executor and synthetic listener/notification objects. Scenarios
cover saturated connect/disconnect, 1,000 connect/disconnect pairs, stale
callbacks after replacement and same-instance reconnection, simulated access
denial, ordinary post/removal updates and command rejection without execution.

Both validation and normal prototype APK builds pass offline, and both Android
lint reports contain no issues. The validation fixture retains a deprecated
Android-constructor compiler note; no warning was suppressed. The validation
APK SHA-256 is
`c823969e074b02eb2ed8022b7536e432f859fc0288023901d907d3315990edbf`.
The retained build/source hashes and build logs are in `bridge-queue/builds.json`.

The runner restored the exact bridge APK captured immediately before this test.
All three installed OctoSense APK hashes, instrumentation registrations, recorded
settings, Home role and Trebuchet Recents match the initial snapshot. Validation
PID `4546` exited. No notification access was granted, no real notifications were
posted, and no root request, setter, reboot or native Quickstep deployment ran.
This validates worker recovery with synthetic callbacks; actual listener access
revocation, real action/reply delivery, remaining subscriber lifecycle/overflow
cases and performance gates remain open. No milestone is complete.

### Review: queued notification events after reconciliation — 17 September 2026

Review found a further overflow defect not covered by the initial 371-assertion
suite: an authoritative snapshot was applied while older events from the same
listener connection were still queued. Those events could overwrite the new
title, delete a reposted notification, or resurrect a removed notification.
`bridge-queue/review-reproduction/results.json` records the failure on the phone:
expected `latest title`, observed `queued old title`. That failing run restored
the retained APK and settings and exited its validation process.

Notification callbacks now carry monotonic sequence numbers. Reconciliation
captures its boundary before querying the listener and ignores queued events
at or below that boundary. Events received during the query remain eligible.
Queue rejection also retains a refresh so that recovery does not rely on a
later event if the worker drains before the dirty flag is set.

The expanded suite passes **389 assertions**, including queue/fixture checks,
on the same OnePlus 6; see `bridge-queue/review-fixed/results.json`. Added
scenarios cover same-connection post/removal backlogs, repost/removal recovery,
and an update received while the snapshot query is in flight. Both APK builds
pass offline and both lint reports contain no issues. The normal prototype
manifest has no instrumentation registrations. The new validation APK SHA-256
is `466fa1b1943cf64a10d85016fb0a4f438651030421a9a29b32e1eb2f221da501`.
`bridge-queue/review-builds.json` retains source and artifact hashes, with the
pre-fix source/APK and failing run preserved separately.

The final run restored all three initial APK hashes, recorded settings,
instrumentation registrations, Home role and Trebuchet Recents; validation
PID `11399` exited. These remain synthetic listener tests without notification
access or device-control changes. Actual notification delivery/actions and
native Quickstep deployment remain unvalidated.

### Subscriber lifecycle under queue pressure — 17 September 2026

A real queue-saturation test reproduced a missing initial subscription snapshot:
`bridge-subscriptions/reproduction/results.json` reports
`saturated_subscription_lost_initial_snapshot`. Subscribe/unsubscribe previously
depended on successfully enqueuing their lifecycle jobs. The bridge now retains
desired subscriptions in a concurrent map and reconciles them on its worker.
Replacement and Binder-death removal use object identity, preventing an old
callback from removing its replacement. Stale sessions become ineligible for
commands and callbacks immediately.

The expanded phone suite passes **807 assertions**, including queue-fill and
fixture checks. It covers subscription, replacement and unsubscribe during
saturation, stale-session commands, old-callback death and an actual remote
Binder callback process exiting while the worker queue is full. The nonexported
validation-only `QueueCallbackService` runs in `:queue_callback` and can terminate
only itself. Both validation processes exited; all original APK hashes, recorded
settings, instrumentation and Home/Recents identities were restored. Prototype
and validation builds and lint pass offline. See
`bridge-subscriptions/fixed/results.json`, `builds.json` and `fixed-artifacts/`.
The validation APK is
`0641137448b2f429587eb68b74a9322f5b71841f21fe7ea7322783b51c350b04`.
These queue tests use synthetic notifications and require no listener access.

### Approved native Quickstep experiment and rollback — 17 September 2026

After explicit approval, the exact artifacts in
`quickstep-deployment-review.md` were installed on `cfb7c9e3`. The phone booted,
recognized the system/privileged Quickstep package, granted the required
permissions and connected SystemUI's OverviewProxyService. The separate Home
integration service also bound. Three-button Recents, swipe Home, short-swipe
cancellation, swipe-and-hold Overview and recent-task resume passed using our
bridge Settings activity. Native controller logs recorded HOME, LAST_TASK and
RECENTS end states. The attempted card-dismiss swipe was inconclusive; icon-target
alignment, quick switch, reversal, stale geometry and performance remain open.

Rollback disabled the scoped module, restored three-button navigation, rebooted
and reinstalled the exact original APKs. All three package hashes, recorded
settings, Home role and instrumentation registrations match the initial record.
Trebuchet again handles Recents and the native OctoSense gesture service is absent.
The experimental module remains disabled. Evidence is retained in
`native-transition/device-experiment/`, especially `before.json`, `boot-state.json`,
controller logs, `recents-actions.json` and `rollback.json`. This proves the normal
rollback sequence; recovery from a boot failure remains untested.

### Real notification fixture and temporary access — 17 September 2026

The user explicitly approved temporary notification access for fixture tests,
followed by revocation. `run-bridge-notification-validation.py` installed the
current normal bridge prototype and validation Home without clearing app data.
Only uniquely tagged fixture notifications were posted. Incoming callback data
was filtered to that fixture before retention; no external recipient was
contacted and dismiss-all was never invoked.

The first run exposed a fixture isolation issue: the ordinary Home client could
replace the instrumentation's one-subscription-per-UID callback. The opt-in
fixture now suppresses that automatic Home binding. The first cleanup also
detected Android normalizing away the preexisting package-only
`com.westlake.host` listener-setting entry. The exact entry was restored, and
the runner now preserves original entries during grant/revoke. Failing results
and the explicit restoration record remain under `notification-roundtrip/run-1/`.

The isolated fixture passes **75 checks** on the phone. It observes access
initially revoked, real listener connection after grant, delivery and update,
Unicode RemoteInput delivery to its own broadcast receiver, command-ID
deduplication, updated/removed action-handle expiry, individual dismissal in both
bridge state and NotificationManager, and revocation while a fixture remains
active. Revocation clears notification state, expires actions and rejects
dismissal. The fixture cancels its notification and deletes its channel and
PendingIntent in cleanup.

`notification-roundtrip/isolated/results.json` records pass and exact restoration
of both APKs, the original listener setting, notification-posting permission and
AppOp, Home/Recents/navigation identities, instrumentation and forwards. The
validated Home APK is
`3c837f3dbe5db71a45abdeb4b59ade1ba7050e70ce35698206557ab0b61c4aad`;
the bridge prototype is
`d881017116b61578fc93d58f087351bdb4f67f2a5fc25c34a53120012a913873`.
Build/source artifacts and hashes are retained in `notification-roundtrip/builds.json`.
The release-mode Home build succeeds with existing warnings and the fixture's
deprecated Android API warnings. This verifies authenticated Home-UID Binder to
the real listener/action path. The shade/editor/Rust/JNI input path and Android
consent-screen interaction remain separate acceptance gates.

### Visible notification flow through Home — 17 September 2026

After the user requested continuation of the visible flow, the normal Home
subscription drove the test on OnePlus 6 `cfb7c9e3`. The new opt-in
`NotificationFlowFixture` supplies only the real notification and its in-app
reply receiver. It does not bind a second bridge client, inject completion events
or open the editor directly. Other notification content is filtered before
Home's worker, JNI and renderer queues. Android's actual notification/action
objects remain in the normal bridge prototype.

`run-bridge-notification-validation.py --ui-flow --visible-seconds 20` uses the
renderer's actual card/action hit bounds for app-window touch. The phone opens
the notification shade, swipes the card to reveal Reply, taps Reply, enters
`Hello from OnePlus 6 — 你好 👋` through the focused native editor's InputConnection,
and taps Send twice. The real receiver gets one reply with the exact text; the
native editor reports `Reply sent to the app.` and clears the draft. The receiver
updates its notification, which appears in the Rust shade model. Further checks
invalidate an unsent draft after a notification update, tap the individual Clear
action, confirm removal in both the renderer and NotificationManager, and revoke
access with another draft open. Revocation empties the shade and clears/disables
the draft. The notification, draft and completion screens each remain visible
for 20 seconds. No external recipient is contacted.

The combined behavior run passes in `notification-flow/run-5/results.json`, with
`ui_roundtrip_verified=true`, one receiver delivery and successful instrumentation
completion. All original APKs, listener/posting permissions and AppOp, role,
Recents/navigation settings, instrumentation and forwards are restored. Owned
Home PID `26612` exited. Tested Home SHA-256:
`4d9b5e37b7a6d75025f9481dbf43891801eae72733d5883b396c1403d39efeb3`;
bridge SHA-256 remains
`d881017116b61578fc93d58f087351bdb4f67f2a5fc25c34a53120012a913873`.
The release-mode validation build passes with existing warnings. Exact source,
APK and log hashes are retained in `notification-flow/builds.json`.

**Visual acceptance remains open.** Runs 1–4 lost their ADB transport at the
capture step; the Mac ADB log records `usb_read failed` with `e00002ed`/`e00002e8`.
Neither bounded 16 KiB capture transfers nor the installed ADB's alternate
libusb backend resolved this. The original native USB backend was restored.
Run 5 explicitly used `--skip-captures` and completed without a reconnect. It
proves the UI input/renderer/native/Binder behavior but supplies no accepted
PNGs. The correlation implicates capture/transfer on this bench; the underlying
cause is not established. Android consent-screen interaction, user keyboard
typing and rendered pixel review remain separate checks.

After the user confirmed OctoSense Home was visible, a further walkthrough
kept the notification, Unicode draft and delivery confirmation visible for
30 seconds each. `notification-flow/visible-demo/results.json` records another
passing UI round trip, one fixture reply, exact state restoration and exit of
owned PID `17624`. This run also skipped captures; the user's confirmation
establishes Home visibility, while combined-flow pixel acceptance remains open.

The first interrupted run required cleanup after reconnection; its explicit
restoration record is `notification-flow/run-1/interruption-recovery.json`.
The runner now retries only idempotent ADB reads/install/permission/forward
operations for brief disconnects and preserves the original listener-setting
entries again after APK restoration. Later interrupted runs restored state
automatically. Input with uncertain delivery is never retried. No failed run is
counted as a pass.

### Next phone validation sequence

1. Use the retained rollback and settings record for subsequent authorized
   updates; preserve app data and the installed signing relationship.
2. Extend the passing native placement/launch, activity/process-recreation and
   helper-absence sequence to locked profiles, shortcut pinning and reboot.
3. Extend actual UID/signer/package/user rejection, state observers and other
   permission lifecycle coverage. Resolve capture/USB failure and visually review
   the combined notification flow; test the Android consent UI and user keyboard
   entry. Record denial and uncertain outcomes distinctly.
4. With explicit user consent for the bridge's own Magisk prompt, verify root
   process identity, deny/retry/revoke behavior and fixed setters. Record before/
   after observed state and restore every changed device setting.
5. Measure event/queue pressure, idle render/shell activity and repeated lifecycle
   memory use. Do not count compilation or earlier rendering benchmarks as passes
   for the new integration. Extend the basic native Quickstep evidence to the
   remaining alignment, lifecycle and parity gates.

## Remaining milestone work and risks

| Milestone | Remaining work / evidence |
|---|---|
| M1 | Profile lock/unlock, reboot, cold shortcut confirmation and broader app/shortcut catalog lifecycle; hosted and native app placement UI, current-user shortcut pin/icon/launch, component replacement, activity/process recreation and public launch without the bridge service have phone evidence |
| M2 | Foreign-signer/cross-user rejection, other permission lifecycle, consent UI, combined notification-flow pixel/keyboard review, root denial/retry, observed non-volume setters and idle/memory evidence; full shade/editor/Rust/JNI reply/dismiss/revoke behavior and saturated subscriber lifecycle/real Binder death have phone evidence |
| M3 | Approved native deployment, trusted grants/SystemUI binding, basic Home/cancel/Overview gestures and normal rollback have phone evidence; icon-target alignment, reversal/stale-geometry fallback and broader native controller lifecycle/overflow remain |
| M4 | Task dismissal, quick switch, Back coordination, widget consent/configuration/resize/update/profile proof, composed-widget gestures and animation alignment, wallpaper integration and all declared same-device parity gates; basic native Recents and recent-task resume have phone evidence |
| M5 | Independently validated split-screen/PiP/unlock and root network/power adapters; messaging remains separately scoped |

Production signing is not provisioned. The root adapter targets this development
ROM; enforcing-mode compatibility is untested. Android 15 DND semantics require
checking against the native reference. Notification labels/icons and same-card
updates now have synthetic-content phone evidence; the native reply editor has
component evidence, and real listener/action/revocation integration has fixture
evidence. The full shade/editor/Rust/JNI flow has phone behavior evidence; its
combined pixel review is blocked by the recorded capture/USB issue.
Widget pages and the workspace have allocation/journal, temporary
binding and separate-layer rendering evidence. Consent/configuration, provider
click actions, resize/update callbacks and profile revocation remain unverified.
Widgets hide during shade/overview/keyboard transitions and are absent from the
Makepad blur texture; input arbitration inside widgets and synchronization of
native and Makepad frames remain gaps. Drag placement, widget pin requests,
auto-advance and dynamic theme colors remain unimplemented. Framework
and hosted-app build warnings remain; they were not suppressed to claim a clean
promotion gate. GPU, latency, idle and long-session memory gates remain unmeasured
for this new integration diff.


## Current launcher system integration — 2026-09-17

The user requested persistent notification, network and device integration in the
current launcher and selected device controls / permission setup as the priority.
The previous fixture-and-rollback result was insufficient for everyday use.

Home now exposes notification consent in its shade, live network status, actual
Wi-Fi/Bluetooth setup fallbacks, hotspot/VPN/battery/display/sound/accessibility
links, and a native setup hub with live access status. Permission grants remain
Android-owned. Existing granted flashlight/Bluetooth access is reflected; tapping
those granted entries opens app permission management. Brightness and rotation
share write-settings consent. Android 15 DND opens system Modes settings; the
legacy bridge setter explicitly rejects this unsupported global-toggle route.

Notification updates preserve a separate opaque visual identity while expiring
old action/dismissal handles. Reposts receive a new identity. Cards can open their
native content, ongoing cards omit Clear, and bridge disconnect removes stale
native content and access state. New status fields invalidate recorded shade
frames.

The final Settings route implementation fixes reuse of stale Android Settings
subpages with NEW_TASK | CLEAR_TOP. The phone regression reproduced Bluetooth
opening a previously stacked DND/Wi-Fi page before the fix, and now checks exact
activities for 13 UI entry points. The Internet panel delegates to a SystemUI
dialog on this ROM; the harness explicitly checks and dismisses that owned action's
panel. Earlier interpretation of that panel as an unrelated blocker was corrected.

Evidence: `target/android/adr-0001-artifacts/system-integration/`. Nine local shade
tests, 811 bridge assertions, 73 real control checks and the fixture-only full
notification UI flow pass. The final 13-route settings test passes. Prototype and
validation bridge builds/lint pass with zero lint issues; release-mode normal Home
and validation Home builds pass with existing dependency/deprecation warnings.
No new software or signing keys were installed. Normal APKs are left installed;
`deployment.json` verifies hashes, Home role, instrumentation cleanup and forwards.
Quickstep remains rolled back to Trebuchet with its module disabled.

Everyday notification access and write-settings access were still disabled at this
checkpoint. The prior user approval was limited to temporary notification fixture
tests; ongoing access was requested separately after the concrete build was
installed and checked. See [current launcher setup](current-launcher-system-integration.md)
for feature behavior, access requirements and validation limits.


### Everyday access approval

The user explicitly approved ongoing notification access and brightness/rotation
access. Both are now granted on the OnePlus. The bridge's own UI confirms
“Connected · Notifications appear in your launcher” and
“Enabled · Sliders and rotation work in the shade”. Existing camera/Bluetooth
access is also reflected. The package-only `com.westlake.host` setting entry was
preserved across Android's listener normalization; no unrelated grants changed.
Evidence: `system-integration/everyday-access.json`. Normal notification content
was not retained in the evidence.

The user then requested complete removal of Trebuchet. The follow-on
[migration record](trebuchet-removal-record.md) tracks replacing its Recents role
before unregistering it for the active user.


## Global panel across apps

The user explicitly selected replacement of the old physical top-edge panel
across other apps. The native Quickstep package now owns an opt-in input monitor
and an Android panel window above the current app, backed by its own authenticated
System Bridge session. It does not launch Home when opened. Core SystemUI and the
existing Recents connection remain intact; Trebuchet remains removed for user 0.

The normal bridge now accepts the actual signed Quickstep caller alongside Home.
The window renders notification actions/replies and existing device controls,
with bounded command dispatch, no uncertain write replay, native settings routes,
and explicit setup/disable controls. Binder-scoped panel suppression is released
on close, cancellation and process death. Task, keyguard, screen-off and
configuration events dismiss the panel. The locked phone retains Android's panel.

The bridge build/lint and 811 queue assertions pass. Native source verification
covers 1,145 pinned projects; the final APK builds in 292 seconds without OOM.
The fixture test passes delivery, exactly one local reply, visible keyboard,
updated-draft expiration, ongoing-card protection, dismissal and app return.
Lifecycle tests pass actual volume changes/restoration, task/Recents handling,
landscape entry, process death/recovery and the insecure lock-screen boundary.
Temporary fixture posting permission and validation instrumentation are removed;
approved everyday listener/write-settings access remains enabled.

The scoped module upgrade has activated through a completed reboot with matching
APK/payload hashes, all native grants, connected SystemUI, unchanged default Home
and navigation mode, and removed Trebuchet registration. No SystemUI APK or
privileged-permission allowlist change is involved. Exact artifacts, complete
evidence paths, limitations and recovery are recorded in
[OctoSense panel across apps](systemui-shade-replacement.md). This does not close
the ADR's parity, performance or broader lifecycle acceptance gates.
