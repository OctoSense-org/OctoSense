# ADR 0006: Built-in OctoSense Settings

Renumbered from ADR 0004 to avoid the independently proposed contained-apps ADR in PR #18.

Date: 2026-09-24

Status: Accepted; full replacement target, implementation in progress

The Rust-owned application-controller decision below is superseded by
[ADR 0005](0005-settings-octoscript-controller.md), following the user's explicit
request to port the complete Settings application logic to Octoscript. Native
authority boundaries remain in place. The controller port is not yet complete.

## Context

The phone has a shared theme catalog and a privileged Android service boundary,
but configuring it moves between unrelated native screens. The product target
is a complete OctoSense system Settings experience authored through
Octoscript–Makepad. A page of links to Android Settings does not satisfy that
target. Giving every hosted app the Home process's Android transport would
expose system mutations to ordinary app scripts.

## Decision

Ship a compiled Settings module in Home. `resources/settings/settings.splash`
is evaluated into a `UiNode` tree by Octoscript and translated by
`octoscript-makepad` into native Makepad widgets. Stable widget IDs are retained
through observations and shared-theme reapplication. Native Rust owns the
interaction state and emits a finite typed request enum; scripts receive no
generic Android settings key, intent, Binder, or shell-command API.

Replace ordinary configuration screens incrementally, retaining the Android
services as the authority for state and policy. Keep a feature parity ledger
against the ROM's actual Android 15 / LineageOS Settings source. Each row records
implemented controls, temporary native links, missing controls, hardware or
policy conditions, and acceptance checks. A native link is migration coverage,
not implemented OctoSense UI. See [the parity checklist](../android/settings-parity.md)
for the navigation, complete target surface, and current delivery boundary.

`ModuleHost` grants Settings authority from the compiled singleton's identity
and the current instance root UID. The host checks the sender again for every
request, validates numeric ranges, and checks the Android adapters' current
capabilities before dispatch. A claimed `settings` app ID or capability string
does not confer authority. Closed roots lose authority immediately. Android's
existing Binder caller and permission checks remain in force afterward.

The Apps area follows the same boundary. Android returns bounded current-user
catalog and permission pages. A package target is minted from an observed row;
the host correlates query, inventory generation, page and selected package, and
requires freshly observed action capabilities before launch, uninstall or native
app-info routing. Uninstall always uses Android confirmation. Catalog changes
invalidate pagination; replacing a row during a press cannot retarget the click.
Permission grant state is read-only in this increment. Separate persistent list
and detail views preserve the filter's native TextInput, caret and list scroll;
snapshots never rebuild or set the text of an active input.

Wi-Fi follows the same rule: only a bounded observed network fingerprint can
identify a network action. A foreground Settings root, its selected network,
fresh advertised capabilities and the backend's current policy checks must all
agree. Reads run while a Wi-Fi page is visible; polling never starts scans.
The ordinary Home package observes permitted public information. Direct radio
and saved-network changes go through appended, finite methods on the trusted
ROM helper. Neither scripts nor Home's transport accept a WifiConfiguration,
password, enterprise identity, certificate alias, settings key or intent.
Credential configuration remains an explicit Android-owned flow during this
increment, with the corresponding parity row still incomplete.

This isolates app scripts; it is not an OS sandbox between native modules.
Linked Rust code shares the Home process/UID and must be trusted. Ordinary apps
retain their existing narrow host APIs (launch, title, notification, etc.). No
privileged settings API is registered in their script isolates, and the Settings
agent-service manifest exposes no mutation tools.

Appearance uses the existing versioned catalog/choice, shared Java application
backend, and per-user storage. Other pages use finite service operations with
capability observations and readback. Expand the existing ROM helper when a
public app API is insufficient; do not use arbitrary SettingsProvider keys,
shell execution, reflection, or permissive intents as a general substitute.
Capability availability reflects permission, Android version, device support,
current user, and administrator restrictions. The backend rechecks these at
execution time.

Security pages also belong in OctoSense: present lock state, authentication
methods, enrollment choices, privacy controls, certificates and policies in the
shared UI. Credential verification, trusted confirmation, biometric enrollment
and authentication, and account-provider consent remain mediated by the
platform's secure components. Octoscript must never receive passwords, PINs,
credential hashes, authentication tokens, private keys or biometric templates.
Android's [Gatekeeper architecture](https://source.android.com/docs/security/features/authentication/gatekeeper)
keeps credential verification and throttling behind the system services and
trusted execution environment; replacing Settings presentation does not replace
that trust boundary.

## State and failure behavior

Device observations update existing labels and controls. Theme changes reapply
the same shared stylesheet without remounting the form; page, draft selection,
pending status, root and control identity survive. Apply persists a complete
theme choice; Cancel or Back from Appearance discards an unsaved draft. Screen
Back returns to the parent Settings page, then the overview and Home. Page navigation resets the
scroll offset so a shorter page cannot open scrolled offscreen.

Commands await a correlated completion, show partial Android theme support
explicitly, and read observed state after completion. An accepted command is not
reported as applied. Queue resynchronization or timeout releases the busy state
with an uncertain-outcome message; it never automatically repeats a mutation.
Offline/unavailable controls display that state, rather than plausible defaults.

## Delivery and completion criteria

Deliver a useful native page with each backend increment, rather than counting
additional Android Settings destinations as completed pages. Current native
links remain temporary recovery routes until their ledger rows pass. The
Android Settings package must remain installed for platform-owned secure flows,
external app contracts and recovery. Once parity is proven, it need not be the
routine user-facing Settings entry. The existing Java appearance picker remains
a recovery entry and shares the same authoritative backend.

Completion requires all applicable parity rows on OnePlus 6, coverage of Android
Settings entry intents and search, shared live themes without state loss,
localization, accessibility, per-user and managed-policy behavior, and readable
failure/recovery paths. Emulator coverage alone cannot establish modem,
fingerprint, display calibration, NFC, charging or other hardware parity. OTA is
a real OctoSense page only after the updater service and its verified install
contract are integrated; a System link does not establish OTA completion.

The module inherits the current app theme contract. This does not finish the
remaining custom-color migrations in Mail or other apps. The first Settings
layout is English; localization and dedicated Octos/provider pages remain work.
Makepad's upstream Android accessibility update path remains incomplete;
this UI must not be presented as TalkBack-complete. The existing native Android
Settings destination is the accessibility fallback. No runtime upgrade, ROM
flash, signing-policy change, or physical-phone installation is implied by this
ADR.


The ordinary policy pages use compiled descriptors: a finite page/control/value
vocabulary, fixed Octoscript–Makepad row slots and an authoritative backend
snapshot of values, availability and permitted options. Backend observations
cannot inject layout, labels, callbacks, settings keys or intents. Host checks
include the current page as well as root identity and request lifetime; options
are revoked when that context changes. A held control cannot be retargeted by
a poll. Bluetooth pairing, accounts and OTA retain domain-specific controllers
rather than being modeled as generic toggles. Their platform-owned consent and
credential flows remain separate security boundaries.


Bluetooth uses its own bounded controller for radio/discovery, adapter naming,
paired/nearby inventories, pairing/connection actions and contact/message
sharing. Device addresses are display data; only opaque targets decoded from
fresh observations may address a mutation. Device-specific actions additionally
require that the trusted Settings root is displaying that selected device.
Snapshots are reads only. Stopping discovery requires an explicit capability
for the backend's own discovery session, so Settings cannot cancel a scan owned
by another app. Unknown bond, connection and permission states remain unknown.
Android's trusted pairing component owns passkeys, PINs and confirmation; none
are accepted or returned through the Octoscript UI. Name edits preserve their
native input and draft until cancelled or matching observed readback arrives.


Accounts and sync use their own controller and typed opaque account/provider/
authority identities. The account inventory always declares `full`, `limited`,
or `unavailable`. Public AccountManager reads can be visibility-filtered even
with GET_ACCOUNTS_PRIVILEGED; a limited empty list never implies that the phone
has no accounts. A narrow, on-demand `OctoSenseSettingsBroker` system APK uses
`android.uid.system` to supply full owner-user reads only to the platform-signed
ROM agent. Both pinned ROM and AOSP Android15 SystemUI use `android.uid.systemui`,
which does not supply this bypass; the sensor-privacy adapter remains in SystemUI
because its authority comes from the SystemUI role. The general agent retains
its separate UID. If the broker is absent, Home uses its own persistent public
backend, preserving Home's actual account visibility. Secondary-user routing
remains unsupported and fails closed.

Reads and mutation authority are separate: the host correlates root, request ID,
selected account and authority, and retires capabilities on pause/timeout. Java
backends retain bounded 30-second observations and freshly resolve targets before
mutating; Home additionally binds each observed route with a 20-second lifetime.
A failed mutation is never replayed through another route. A resumed detail read
can refresh a previously observed account without granting stale mutation rights.
Sync accepts only automatic-on/off, request-now and cancel, with no arbitrary
sync Bundle or authority accepted from scripts. Pending/active state is observed;
queuing work does not imply a completed provider sync.

Add uses Android's provider-filtered account flow. Account visibility consent
uses Android's account chooser under Home's launching identity. Removal uses an
immutable one-shot PendingIntent to an unexported broker confirmation Activity,
which hides non-system overlays and rechecks policy and exact account incarnation
before invoking AccountManager's provider-owned removal/authentication flow.
The incarnation is a native-only hash of Account.getAccessId(); the pinned account
service creates a new access ID on insertion/cache reconstruction. A changed or
missing ID fails closed, preventing a pending confirmation from targeting a
removed-and-recreated account with the same name/type. No password, access token,
raw access ID or authenticator result Bundle enters the Settings script protocol.


Settings search is a local index of reviewed, typed built-in destinations. Query
text and aliases cannot select Android intents, settings keys or host commands.
Result selection only navigates to an existing page, where its normal observed
capabilities still govern changes. The input and result widgets remain mounted;
queries, caret, page and scroll survive polls, themes and navigation. A held row
retains its original identity until release, preventing a changed query from
retargeting the click. Search indexes migrated pages; unimplemented settings
must not appear as if full parity has been delivered.


The System updates controller reuses the ROM updater and its signed A/B payload
and PackageInstaller paths. It does not create another downloader. Automatic
foreground reads only inspect status; checking the remote feed is an explicit
operation. System and Home installation use separate finite parts and the opaque
key of the exact reviewed offer. The service captures the reviewed manifest,
expires it, verifies its source/current versions and serializes installation;
confirmation cannot silently select a newer release. Restart is a separate
confirmed action bound to the observed ready engine generation. Host identity,
foreground state, observed capabilities and keys are rechecked at dispatch.

Offer changes, lost capability, pause/timeout and Back invalidate local review.
An accepted check/install/restart request is not reported as finished work;
actual service phases, progress and errors drive the view. Unknown values remain
unavailable. The standalone APK can show actual build/Home version observations
but cannot grant itself ROM updater authority. Cancel/resume and feed editing
are separate migration work. Emulator validation must never start a real update
or reboot merely to exercise this UI.


Advanced connections uses a dedicated typed controller for Airplane mode, global
Data Saver and Private DNS. Every mutation carries the opaque key of the exact
observed network-policy state. The helper retains a 20-second observation and
rechecks raw stored values, user/admin policy and permissions before writing.
Rust additionally binds requests to the foreground trusted root and current page.
Private DNS mode and hostname are an explicit local draft. Polls and theme updates
do not replace the input or write its text. A changed observation key preserves
the draft but retires Save; Cancel followed by Edit reviews the new values.
Android performs authoritative hostname/IDNA validation. Null mode may coexist
with an authorized write capability to repair malformed settings; this never
invents a current mode. Saved configuration, active encryption and actual DNS
validation are displayed separately. Per-app data rules, traffic limits, proxy
and remaining connectivity pages are independent parity work.


Android editor transport acceptance (build2026092431): a registered overlay on
the pinned runtime gives the focused stable WidgetUid a sequenced editor session.
Typed edits and editor actions apply to canonical widget state; Java retains an
optimistic query buffer and accepts only appropriate acknowledgements. Retired
InputConnections cannot read or submit a different editor. Actual Activity
Resume is reconciled after startup, and queued show-keyboard commands recheck
user dismissal. Gboard cold typing, immediate selected replacement/Clear,
composition, Back across polling and native-Activity return, retap and Done passed
on the isolated emulator. This introduces no fixed input delays or blocking
pointer barrier. Other IMEs, accessibility and hardware still require their own
acceptance; see the detailed [input validation](../android/settings.md).

Sound selection uses a separate finite ringtone/notification/alarm contract and
a bounded RingtoneManager catalog. Media URIs remain in the native backend; only
opaque observed catalog/row identities reach Settings. A local draft does not
write anything. Preview uses the matching Android audio usage, never changes
volume or Do Not Disturb, stops on leaving/background/lock/user change, and has a
five-second maximum. Save rechecks observed current value, active owner, policy,
write permission and fresh media membership before applying and reading back the
new default. Catalog browsing lasts at most five minutes. Custom audio import and
advanced vibration/charging policy remain explicitly separate parity gaps.


Cold-editor follow-up: Android can request multiple InputConnections for the
same Surface and retain an earlier returned handle. Connection authority follows
the focused widget session, not the latest Java connection pointer. Open handles
created before the first active editor can adopt only that first session; closed
handles and handles from retired sessions cannot read, edit or submit a new
field. This fixes the demonstrated cold Gboard handoff while preserving the
existing canonical edit protocol and avoiding timing delays. A shared
connection epoch retires every handle immediately on pause or an inactive reply;
an initial empty session0 observation preserves the pre-focus cohort. The Android
multi-connection regression fails before the change and passes afterward. Exact
production2436 passed cold Gboard/hardware, selected replacement, Clear, keyboard
Back/retap/Done and native-Activity return on two independent API35 emulators;
the implementation validation document records the counts and remaining gates.

## Settings accessibility presentation

The built-in module exports a bounded Android accessibility tree only while an
accessibility service is enabled and the trusted Settings instance is fully
foreground. This is a presentation adapter for the compiled Octoscript widget
tree, not a new settings capability API. Labels, buttons and non-password editors
come from the visible widgets; clipped, rounded physical bounds match the window.
The active ScrollYView is an explicit ancestor with forward/backward actions.
Repeated controls receive contextual names such as “Increase media volume by
10%” and “Do Not Disturb: Priority only”.

The page token identifies a visit to a trusted root/page/detail target. Node IDs
are process-monotonic and never reused for a different semantic row. Polls,
layout and theme changes preserve the identity of surviving nodes; recycled app,
account, network, sound, search and history slots include their observed target.
An action is accepted only against a freshly derived visible node, current
capability and matching token/target. Requests use a process-wide monotonic
sequence so changing a token cannot replay an old action. Clicks and editor
changes use the existing widget handlers and their typed backend checks. They
cannot name arbitrary Android intents, settings keys or scripts.

Java presents virtual nodes and queues bounded asynchronous actions. It sends
click/text events only after a matching accepted result; it never blocks the UI
thread on Rust. Disable, pause and destruction clear the native tree, while Rust
retires the owner and republishes on resume. Accessibility state is reconciled
through the existing initial UI-mode observation, including cold startup. In the
ordinary disabled case no widget traversal or per-frame inactive JSON allocation
is required. Enabled layout publication is bounded to four snapshots per second,
with action-driven refresh, and at most 256 visible nodes per snapshot.

This adapter does not implement the system Accessibility settings pages or prove
complete TalkBack, switch-access, localization or hardware acceptance. Unit
coverage includes real rendered text/editor/button geometry, scroll actions,
recycled-row rejection, capability revocation, theme and 150% font identity,
and keyboard reopening after an explicit accessibility focus following Back.
Build2026092437 passed33 actual public-UiAutomation checks on both API35
emulators: editing, stable focus, keyboard Back/reopen, scroll ancestry/actions,
Activity retirement and stale/disabled node rejection. This is not a complete
TalkBack traversal or assistive-technology certification; native Android Settings
remains a recovery destination. The combined Home suite passes403 tests.

## Public Settings entry routes

A public `SettingsEntry` activity alias targets the existing single-instance Home
renderer. The alias accepts the custom navigation action; the renderer itself
accepts the reviewed Android Settings actions. The Java adapter turns these into `settings.entry` with exactly
`{schema:1,id,route}` for the22 finite untargeted routes. The standard per-app
notification action has one additional `package` navigation selector; it is never
an observed target or mutation capability until a correlated current-user details
lookup succeeds. No caller UID, channel ID, query, setting value, raw intent or
mutation operation crosses into the module. Standard Android
Settings actions retain their fixed meaning even if callers supply a custom
route extra. Invalid custom routes, wrong extra types, selectors and data are
ignored rather than interpreted as commands.

`ACTION_APP_NOTIFICATION_SETTINGS` requires a syntactically valid string
`Settings.EXTRA_APP_PACKAGE`. Its `app_notifications` route cannot be invoked
through the custom route extra. Rust binds the read-only lookup to the retained
entry ID, exact selector and live trusted root; Back or a newer entry rejects old
results. Missing packages show non-actionable app details. Only a successful
fresh observation mints the normal AppTarget and permits subsequent notification
reads; all mutations still require their own observed targets and explicit review.
The new Android action occupies a separate priority-2 renderer filter to preserve
existing updated-system filter priorities. Native recovery remains pinned.

Java retains its latest entry until Rust explicitly announces readiness and
acknowledges receipt. JNI queue acceptance alone is insufficient: the pinned
Makepad bootstrap discards pre-surface integration messages before Startup. This
handshake avoids a timing-dependent lost cold entry without delays or runtime
changes. Rust retains the latest valid entry before shell initialization or Resume
and rejects duplicate/older process-monotonic IDs. The router finds the trusted
compiled Settings singleton directly, opens it only when absent, and closes shell
overlays before focusing it. Entry navigation retires old accessibility identities,
editor focus, pressed targets and mutation drafts/confirmations. Harmless local
list/search filters survive. It uses the same page-specific read paths as normal
navigation; it never checks for updates, installs, reboots, changes a setting or
approves an old confirmation. Leaving sound selection stops its preview. A later
Home action cancels a still-pending entry without resetting replay protection.

The first native-ready transition also republishes authoritative Activity and
accessibility state. Those initial observations can be lost in the same bootstrap
window as navigation. This reconciliation runs once, so the resulting `ui_mode`
observation and readiness response cannot form a loop. Changes to the native
virtual tree publish a host subtree-change event independently of pane-title
announcements, allowing accessibility services to invalidate cached descendants.

Native recovery buttons are pinned to `com.android.settings` before these public
actions are advertised, including generic Wi-Fi credential fallbacks. This avoids
resolving a recovery action back into OctoSense or a chooser. Declaring supported
actions does not make OctoSense the device-wide default resolver, nor does it
establish parity for unsupported per-app, profile, credential or hardware routes.
Build 2026092503 passed six cold-entry trials, all 20 custom routes and 19 Android
actions, and 33 accessibility checks on each API35 emulator. An acknowledged entry
did not replay after Back/native Activity return. Alternating alias/main launches
retained the same Activity on the Gboard emulator. The Home suite has 408 passing
tests including all 20 routes, pre-start queuing, replay denial and mutation-draft
retirement.

Quickstep's navigation path uses a separate `SystemSettings.openPreferred` API.
It inspects the exact Home alias and renderer target, enabled/exported state,
system-app flags and platform signature before sending a finite route. Missing or
ineligible Home installations retain the native destination. Recovery and consent
callers keep `SystemSettings.open`, so a system default cannot redirect a required
platform confirmation back into OctoSense. The native Internet panel is retained
until the complete Wi-Fi/mobile entry is migrated. The ROM's real renderer Activity
requests priority 2 for the 19 standard actions; the custom alias remains priority
0. Android caps ordinary installations at 0. In an updated system package,
`ComponentResolver.findMatchingActivity` matches an alias against its target before
matching the exact alias, so placing the standard filters only on the alias loses
priority against the base Activity's filters. The 2504 privileged scan reproduced
that failure; 2505 moves those filters onto the renderer, with a manifest regression
test protecting the placement. The dedicated AOSP emulator passed all 19 implicit
routes, preferred/native fallbacks and the actual SystemUI gear in 2505 (46 checks),
then cold entries and accessibility smoke. A data reinstall retained priority 2;
the same update over the old 2504 base remained native as expected. UID/data and
device preferences were preserved. This privileged framework-rescan fixture is
distinct from a complete production ROM boot or running the customized Quickstep
APK. The repository suite has 52 passing tests, including routing authority and
manifest placement regressions.


### Display migration and density lifecycle

Display size and Night Light use a dedicated finite controller rather than raw
SettingsProvider keys. WMS governs default-display density; ColorDisplayManager
provides real Night Light capability and bounded settings. One-field reviews
avoid pretending multiple platform writes are atomic. Observed keys expire and
are rechecked before claiming a mutation; post-write configuration changes do not
invalidate an already-authorized write. Readback distinguishes applied from a
pending request. Ordinary installations cannot manufacture privileged capability.
Helper permissions remain separate from Home and the system-UID broker is not
involved in this domain.

The pinned Makepad density path now carries configuration and inset density
through bootstrap and live window-geometry updates. Density handling preserves
physical dimensions, touch mapping, widget identity and the canonical editor
session. A registered overlay plus the corresponding Home config mask is required;
a config mask alone would leave stale runtime metrics. Actual 2506 density tests
pass on both owned emulators with original settings restored. Final2508 adds
120 supported-service UI checks and24 unsupported checks after removing the
capability overlay, with all original preferences restored. Night Light physical
tint and device hardware behavior remain separate from emulator compositor evidence.

New public Android action filters are separate from existing base-ROM filters.
Android requires an updated filter's actions to be a subset of its matching base
filter before preserving elevated priority. Thus Night Display's new filter must
not cause existing 19 routes to lose their priority on an older base image.


Sound feedback uses the finite Controls descriptors instead of another generic
SettingsProvider bridge. Supported vibration levels and usage defaults come from
the actual native resources and vibrator service. Compatibility writes for ring
and touch are explicitly non-atomic: readback distinguishes completed, pending
and partial results. Stable choice slots bind accessibility and press identity to
the semantic control/value, so a capability change cannot reuse an old target.


The per-app notification increment extends the narrow system-UID SettingsBroker
with a finite notification service. Public Home and Agent processes do not gain
broad notification-service privileges. Opaque observed targets, package
incarnation/state leases, typed choices and explicit reviews preserve the same
host boundary; platform-owned notification permission semantics remain decisive.
Native channel copies preserve unedited fields and existing user-lock bits.
Linked legacy default-channel/app-permission changes are explicitly non-atomic
and report partial completion after per-call authority checks and readback.


Default-app choices use a narrow adapter inside the existing PermissionController,
where native role qualification, restriction intents, confirmation text and
selection side effects already live. The Home picker receives bounded observed
candidates and can request an immutable native confirmation; it has no direct
role-grant API. App incarnation, current holder, owner and policy are rechecked
at the positive button and actual holder readback. Enhanced Confirmation and
administrator-restricted choices remain platform-mediated. The adapter adds no
broad role permission to Home or Agent and preserves the native controller's
package, resources, role service and data. See the [default-role audit](../android/settings-default-roles-audit.md).

Common runtime permissions use a second finite service inside that same native
PermissionController, not a Home/Agent grant API. The native group and per-group
ViewModels supply current categories, precision/background choices, fixed-policy
and warning requirements. Octoscript renders those observations; an immutable
one-use ticket invokes the native requestChange path in an unexported Activity.
The adapter rechecks package incarnation, owner/unlock and the full observed
permission fingerprint before dispatch and before a warning's positive button.
A retained operation state prevents configuration changes from replaying writes.
Native observers remain main-thread-owned; Binder readers wait on a bounded
worker and remove observers on completion, interruption or timeout.

Ask every time is a native revoke with one-time semantics; a currently active
one-time grant is observation only. Precision is a distinct native operation.
Raw PackageManager grant bits are insufficient to describe legacy denial:
REVOKED_COMPAT plus AppOps may deny access while the legacy grant bit stays true.
The native model remains authoritative for selected choices. Specialized media,
selected photos, Health Connect, virtual-device permission scopes and special
access require their own models/consent and are not folded into this common
adapter. Ordinary installations have explicit unavailable state and native
recovery. See the [runtime-permission audit](../android/settings-runtime-permissions-audit.md).

DND policy and time schedules extend the existing Zen broker. Home and Agent
receive no notification-policy grant or general rule-management interface.
Observed one-use keys authorize finite policy fields and verified Android time
rules. Unedited native fields survive copying; Android may normalize deprecated
visual-effect encodings without changing their meaning. Native readback decides
whether a save completed, and pending or rejected saves retain their drafts.

Android's condition provider owns schedule activation and exit, including when
Home is stopped. Compatibility is limited to two audited native activity-status
methods and two exact Android provider spellings, with owner/type/condition checks
still required. Unknown status stays unknown and other provider types remain
read-only with native recovery. The dedicated public DND entry only navigates;
intent extras cannot change policy or create rules. Calendar/provider triggers,
per-rule interruption policies and effects retain separate implementation and
acceptance work. See the [DND audit](../android/settings-dnd-rules-audit.md).
