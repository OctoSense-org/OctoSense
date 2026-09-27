# Runtime permission editing: native model and implementation

Status: the bounded common-group implementation is present; native emulator
acceptance passed on2521. Home2517 exposed a native readiness defect and an
accessibility publication race; the corrected implementation is being validated.
This does not complete specialized permission or hardware acceptance.
Inspected the actual ROM checkout on2026-09-25 at
Permission revision `12d670229861f4ec3289418128589d751ac20c8c` and the preserved
AE3A.240806.019 PermissionController APK from the disposable emulator baseline.
All platform paths below are relative to that checkout.

## Existing inventory is insufficient for editing

OctoSense App details already lists requested permissions, actual grant state,
runtime/protection distinctions and bounded pages. PackageManager's per-permission
grant bits do not express the effective native group choices: foreground versus
background, Ask every time, a currently active one-time grant, approximate versus
precise location, AppOps, fixed policy, restricted permissions and split grants.

The native editing surface is PermissionController, not a new raw
PackageManager grant/revoke interface in Home or the general Agent process.

| Pinned source | Relevant contract |
| --- | --- |
| `PermissionController/AndroidManifest.xml`, ManagePermissionsActivity | Native manage-permissions actions require `GRANT_RUNTIME_PERMISSIONS`; the separate public app-settings alias requires `LAUNCH_PERMISSION_SETTINGS` |
| `frameworks/base/core/res/AndroidManifest.xml` | Those permissions are respectively `signature\|installer\|verifier` and `signature\|privileged`; ordinary applications cannot impersonate the Settings caller |
| `src/.../permission/ui/model/AppPermissionGroupsViewModel.kt` | Actual package group inventory, native Allowed/Ask/Denied categories and foreground/background subtitles; excludes groups native policy says not to show |
| `src/.../permission/ui/model/AppPermissionViewModel.kt` | `buttonStateLiveData` yields shown/enabled/checked choices using system-fixed, policy-fixed, target SDK, Enhanced Confirmation and foreground/background rules |
| `src/.../permission/ui/handheld/max35/LegacyAppPermissionFragment.java` | Maps user choices into the native model, displays native warning text, and invokes its confirmation callbacks |
| `src/.../permission/utils/PermissionMapping.kt` | Platform permission-to-group mapping, one-time support, media splits, HealthConnect and newer platform feature gates |

The `src/...` prefix in this table expands to
`packages/modules/Permission/PermissionController/src/com/android/permissioncontroller`.
`PermissionController/AndroidManifest.xml` is under that same Permission module.

## Preserve the native choice semantics

Use the model's observed state, not a second policy implementation. For common
runtime groups its finite buttons mean:

| Native button | Change / limitation |
| --- | --- |
| Allow | Grant foreground permissions through the model |
| Allow all the time | Grant foreground and background, only when shown and enabled |
| Allow only while using the app | Grant foreground and revoke background, according to the native mapping |
| Ask every time | Revoke with the model's one-time flag; this is not a persistent grant |
| Current one-time grant (`ASK_ONCE`) | Read-only checked state in native Settings; **must not become a grant-once command** |
| Don't allow / deny foreground | Native revoke-both or foreground-only path, only when offered |
| Precise location | Separate fine-location grant/revoke choice, with native accuracy flags and policy |

`requestChange(boolean, Fragment, ConfirmDialogShowingFragment, ChangeRequest, int)`
also determines whether a warning is necessary. Revoking a default, legacy or
install-to-runtime split grant can require a system/old-SDK warning. Companion
device role grants can require a role warning. Media storage supergroups have
their own advanced confirmation. Use the native message IDs and callback path;
never call `onDenyAnyWay` without the corresponding current positive confirmation.
Native callbacks can update multiple permission/AppOp/flag fields and kill a
revoked app process. A dispatched callback is not proof of the final grant state.

Enhanced Confirmation/admin-disabled choices remain disabled. Their native
restriction flow can remain an explicit recovery action; do not call
`clearRestriction`, invent an exemption, or broaden a disabled choice.

## Proposed built-in slice

Add App details → Permissions with a stable, bounded native group list, current
category/subtitle and the existing requested-permission detail page. A group opens
an Octoscript choice page. Display the platform group's localized label and actual
shown choices; state remains unchanged until refreshed native observations arrive.
Keep query/scroll/Back, theme changes and semantic accessibility IDs stable.

The first editable set should cover the common platform groups through the native
model: camera, microphone, location, contacts, calendar, phone, call log, SMS,
nearby devices, activity recognition and body sensors. Capability checks may
disable individual choices; restricted grants are not assumed to succeed.
Requested permissions outside this set remain visible with an explicit native
recovery path. Do not advertise all runtime-permission parity from this slice.

Specialized follow-ups remain explicit: selected photos and media supergroups,
all-files access, HealthConnect's dedicated consent UI, virtual/external-device
permission scope, custom permission groups, hibernation/auto-revoke, global
permission manager/privacy history and special access. Notification permission
navigation should use the already implemented per-app Notifications controller,
which preserves its notification-specific app/channel policy.

## Narrow controller adapter

Extend the already staged PermissionController adapter with a separate finite
service and unexported operation Activity. Keep `BIND_AGENT_PLATFORM`, exact
Agent package/signature checks, owner0/current-unlocked-user gates and Home's
trusted Settings-root routing. Add no broad grant/revoke permission to Home or
Agent. The helper may only obtain observations and a native PendingIntent.

The adapter observes native LiveData on the main thread with bounded lifetimes.
Binder workers must never block the controller main thread waiting for that same
thread. Inventory must be fresh, and a newly constructed group model must emit a
non-null choice map before any choices become available. Follow the native
fragment's choice readiness rule: the model's aggregate `isStale` can remain true
because its static, source-less rationale dependency was populated before it was
observed. A real Camera session emitted nine choices with that flag still true;
rejecting the map caused a five-second timeout. This exception does not accept a
stale inventory or synthesize choices. The native map still supplies shown,
enabled and checked policy state, and mutation rechecks the current target.

The regression compiles the real adapter model against native-like LiveData
fixtures. It fails before the readiness correction and covers emitted maps,
null-map loading, stale inventory, main-thread blocking rejection, interrupted
reads and timeout cleanup. Remove all observers and ViewModels when the short
observation session retires.

Proposed finite boundary:

- Package-group snapshot: fresh observed AppTarget, page offset/generation;
  maximum64 groups,20 rows per page. Return native category, localized label,
  opaque group identity, supported scope and read-only reasons.
- Group snapshot: observed group target; return shown/checked/enabled finite
  choices, accuracy state, current restriction explanation and opaque state key.
- Choice request: state key + observed group/choice target only. No raw permission
  name, UID, user, flags, AppOp, arbitrary ChangeRequest integer or direct setter.
  Return an immutable one-shot native PendingIntent; never a claimed grant result.

Observation authority should bind current package UID/signer/install/update
incarnation, requested permissions and relevant grant/flag/AppOp state, native
choice availability and current owner. Use a short observation lease and a bounded
review ticket, as for roles. Re-resolve immediately before execution and again
before a warning's positive callback. Removed/reinstalled apps, external state,
policy, lock/user changes or expired tickets retire the operation.

The native Activity hosts an adapter Fragment and ViewModel. For a user-selected
ordinary choice it invokes the existing `requestChange` mapping once; if the model
requires a warning, present its platform-owned dialog and preserve its native
positive/cancel behavior. No additional generic warning is needed for a choice
native Settings applies immediately. The dialog must hide non-system overlays,
use the same native strings and protected window handling, and recheck the exact
reviewed target before mutation. Rotation must preserve an already-dispatched
operation without replaying it. Return to the same OctoSense group page in a
separate recents-excluded task. Report only observed resulting state or a bounded
failure/uncertain outcome, never optimistic success.

Do not subclass the native handheld fragment to gain access to private fields.
The pinned tree uses `max35.LegacyAppPermissionFragment` / a feature-gated v36
fragment, while the original emulator APK uses `handheld.AppPermissionFragment`.
The stable model and confirmation interface are the reuse point.

## Exact-original APK feasibility

The preserved AE3A APK DEX contains these exact descriptors also found in the
pinned source:

- `AppPermissionGroupsViewModel(String, UserHandle, long)` and
  `getPackagePermGroupsLiveData()`.
- `AppPermissionViewModel(Application, String, String, UserHandle, long, String)`;
  `getButtonStateLiveData()`; `requestChange(...)`; `onDenyAnyWay(...)`.
- `ConfirmDialogShowingFragment.showConfirmDialog(ChangeRequest,int,int,boolean)`
  and `showAdvancedConfirmDialog(AdvancedConfirmDialogArgs)`.
- `ButtonState.isChecked/isEnabled/isShown`.

This supports an isolated compile/adapter-only DEX experiment; it does not prove
runtime behavioral compatibility. Not every Kotlin getter survives optimization
in the original APK (`getCustomRequest` is absent there), so bind only verified
members and default-device finite mappings. Resolve native resources in the
running controller; do not inline a newer build's resource IDs into the old APK.

Reuse the disposable5560 validation strategy: original controller DEX/resources
byte-identical, adapter-only added DEX with no duplicate or compile-only classes,
matching test signer, same package/version/data/UID. Original5558 and the physical
phone stay untouched. Stage only reviewed new adapter files/manifest nodes in
the production Permission source, then check against its pinned revision.

## Acceptance before calling the slice implemented

1. Pure model/contract checks: loading/stale versus denied, exact native choices,
   no actionable one-time observation, bounded group/permission pages, malformed
   enum/key rejection and no grant-on-read.
2. Native adapter tests: main-thread observer lifecycle/timeouts; native
   allow/revoke/ask/accuracy mappings; warning Cancel/positive; default/legacy,
   fixed-policy/restricted flags; same-incarnation and current-owner rechecks;
   duplicate, stale, expired and rotation-replayed tickets rejected.
3. Independent public-SDK fixture on5560 requesting camera/mic, coarse/fine and
   background location, contacts/calendar and selected extra groups. Check actual
   `checkSelfPermission`, permission flags and AppOps after each built-in choice;
   prove Ask every time versus persistent denial and approximate versus precise.
   Real sensor/location payload access is separate from permission readback.
4. A legacy/default-grant fixture exercises the required native warning with
   Cancel and positive confirmation; change policy or uninstall/reinstall while
   that warning is open and reject the old target. Preserve unrelated grant flags.
5. Ordinary5556 shows the controller service unavailable without inventing
   privileged choices or a complete permission inventory; native recovery works
   without new broad Home permissions. Add
   ordinary and unrelated-platform-signed caller probes for both new service
   transactions and the unexported Activity.
6. Stable keyboard/Back/theme/150% fonts, actual button-start swipes, stale
   accessibility nodes, process restart/readback and exact fixture cleanup.
   Preserve controller identity, all existing role holders and original grants.

The reviewed implementation follows this contract in2517. Native controller and
Agent compilation pass; corrected2521 source passes451 Rust and63 repository tests. Emulator mutation
acceptance remains in progress. The exported service adds no permission-grant
privilege to Home or Agent. Ordinary installations show service unavailable,
rather than claiming a complete public-API inventory. Parent-owned acceptance
uses only the disposable5560 clone and5556 ordinary UI; the original controller
on5558 and every physical phone remain untouched.

### Readiness and accessibility corrections

The2517/2518 diagnostics identified two independent failures. The native group
model produced its real choices while a static rationale source kept aggregate
`isStale` true; the corrected observer uses the native fragment's emitted-map
rule described above. In Home, a same-page Search acknowledgment preceded the
newly drawn accessibility tree, so a service could immediately see the previous
app row. Accepted actions now wait for a real draw/tree publication before their
success event; timers do not acknowledge undrawn state. Search keeps its semantic
identity during loading, while old catalog rows retire. Pending results are
bounded and retired with the Settings owner; actions are never replayed.

Production Home2519 passes the unchanged ordinary-emulator permission recovery
scenario (25 checks) without a navigation retry. The native model regression and
449 Rust/63 repository checks passed at that revision. Subsequent privileged
grant/warning checks used the corrected controller on the disposable5560 clone.

The corrected native observer passed a463-check real permission run covering nine
common groups and legacy Cancel/Back/rotation/reinstall/positive warning behavior.
An intermittent rejection remained after rotation. Diagnostic Home2520 identified
window focus, not consumed native authority, as the cause: Rust considered Home
resumed while Java correctly rejected its unfocused window. A second captured
return read likewise ran before focus and showed Restricted until the five-second
poll, although the service had already applied the native change.

Production2521 adds an observed focus callback without a runtime repin. Java
immediately clears accessibility nodes/actions on window-focus loss and suppresses
late layouts while unfocused. The permission host retires action authority, waits
for real focus, and reads immediately on regain. Captured focus edges plus the
initial ui_mode observation avoid guessing or fixed delays. Native required
warnings and all service checks remain intact. Successful permission/role flow
launches now describe native review instead of falsely saying Setting applied.
451 Rust/63 repository checks pass; emulator repetition and final11-group scope
acceptance are pending at this documentation checkpoint.

Ordinary5556 also passes2521 recovery (25 checks), seven cold launches (29 checks)
and accessibility/IME/lifecycle smoke (33 checks), with an empty crash buffer.
The first privileged2521 legacy repetition still rejected a queued choice while
rotation restoration was in flight. The fixture is being corrected to observe
actual display rotation and focused-window completion before the next action;
stored rotation preferences alone do not establish that completion. Production
focus checks remain intact. This receipt is not a final mutation-acceptance pass.

### Final2521 emulator acceptance

Three consecutive legacy runs pass104 checks each after the fixture waits for
actual display rotation and the focused recreated window. The full run passes521
checks across all11 common groups, including native Ask/Deny flags and AppOps,
location foreground/background/precision, legacy Cancel/Back/rotation/reinstall
rejection and positive native confirmation. SMS and call logs have the native
ADB installer exemption; no exemption, role or restriction setter was used to
force permission availability. All906 unrelated grant/flag records across183
packages survived every run. Final47 settings,40 role holders, three Assistant
rows, service identities, density420 and empty crash buffer match the baseline.

Receipts are archived under `out/home/settings-permission-focus/validation`.
The accepted product still rejects actions that lose window focus before native
dispatch; the fixture waits for rotation completion and never retries a mutation.
Specialized media, Health Connect, virtual devices, special access, managed users,
hardware and a full production-ROM boot remain separate parity gates.
