# Default-app roles: pinned platform audit

Status: adapter and OctoSense picker implemented; native Browser/Assistant consent and None emulator validated2515. Other role-specific behavior and full-ROM integration remain acceptance gates. Inspected
the actual ROM build checkout read-only on2026-09-25. Permission module revision:
`12d670229861f4ec3289418128589d751ac20c8c`. All source paths below are relative to
that checkout. This is separate from the completed app-notification controls.

## Existing entry points cannot confirm an arbitrary selected app

`packages/modules/Permission/framework-s/java/android/app/role/RoleManager.java`
creates `ACTION_REQUEST_ROLE` with only the role name and PermissionController
package. `RequestRoleActivity.java` in PermissionController's
`src/com/android/permissioncontroller/role/ui` assigns the target from
`getCallingPackage()`, not a caller-supplied package extra.

The deprecated SMS and Dialer compatibility actions do not provide a general
exception: they allow the caller itself or delegation by the current holder.
Their trusted calling-package extra comes from PermissionPolicyService. OctoSense
must not manufacture that identity or misuse these actions.

`DefaultAppActivity` is protected by `MANAGE_ROLE_HOLDERS`. It accepts role/user
and opens the native candidate picker; it has no reviewed target-package
confirmation interface. Linking it is a recovery path, not a built-in picker.

## Observation and mutation authority

| API | Authority / behavior |
| --- | --- |
| `RoleManager.isRoleAvailable` | Public availability; does not enumerate candidates or certify writable policy |
| `getRoleHolders[AsUser]` | `MANAGE_ROLE_HOLDERS`; current holder(s) |
| `isRoleVisible` / `isApplicationVisibleForRole` | `MANAGE_ROLE_HOLDERS`; native role availability, qualification and application visibility |
| `getDefaultApplication` / `setDefaultApplication` | API34+ system APIs requiring `MANAGE_DEFAULT_APPLICATIONS`; setter directly changes the holder, not a consent UI |
| `Role.getQualifyingPackagesAsUser` | PermissionController internal model; used by its actual role picker |

The pinned framework declares `MANAGE_ROLE_HOLDERS` as
`signature|installer|module` and `MANAGE_DEFAULT_APPLICATIONS` as
`signature|role`. Neither needs to be added to Home or ordinary OctoSense modules.
Never enable `BYPASS_ROLE_QUALIFICATION` or use role-holder test APIs.

Qualification alone is insufficient. Native selection also uses:

- `Role.getApplicationRestrictionIntentAsUser`: administrator restriction
  `DISALLOW_CONFIG_DEFAULT_APPS` and Android15 Enhanced Confirmation restrictions.
- `RoleUiBehaviorUtils.getConfirmationMessage`: role-specific consent text.
- `DefaultAppViewModel` / `ManageRoleHolderStateLiveData`: serialized role changes
  and asynchronous result handling.
- `Role.onHolderSelectedAsUser` (or `onNoneHolderSelectedAsUser` where actually
  offered): role-specific behavior after selection.

Those helpers live in the platform PermissionController. Copying just the generic
RoleManager setter into a Home picker would miss native policy and behavior.

## Recommended next increment

Keep the role overview and bounded candidate lists in Octoscript-Makepad. Add a
narrow ROM adapter inside PermissionController, alongside its existing role UI,
so it can reuse the actual qualification, restrictions, localized confirmation
and post-selection behavior. The pinned `PermissionController` module is
platform-signed and updatable; staging must preserve the original manifest,
resource IDs and role service, and compile against this exact revision.

The adapter should expose only a reviewed finite role set, owner-user observations
and a one-shot platform confirmation for an opaque observed candidate. No raw
role name, arbitrary package/UID/user or direct setter should be script-callable.
Gate Binder by the exact signed Agent package, current unlocked owner, and fresh
role/candidate state. Recheck package incarnation, current holder, qualification
and restriction after confirmation. Cancel is inert; an asynchronous callback is
not proof until the new actual holder is observed. Do not silently clear a role;
offer None only where the native role model supports it.

A missing adapter must show unavailable authority with a native recovery link.
Acceptance requires actual candidate filtering, Cancel/confirm, changed candidate
or holder rejection, administrator/Enhanced Confirmation denial, full readback,
role-specific effects, and restoration on the owned emulator. Credential flows
and platform confirmation remain native. No physical phone is involved.

## Concrete adapter boundary and insertion points

The first finite role set can cover Browser, Home, Assistant, Dialer, SMS, Call
screening, Call redirection and Wallet. Each still requires the actual native
role to be available, visible and exclusive. Do not invent candidates for
unsupported telephony/NFC capabilities. Emergency defaults, system-only roles,
profile management and non-role defaults remain separate scope.

Stage adapter files under PermissionController's
`src/com/android/permissioncontroller/octosense/` and add their AIDL source to
`PermissionController-lib`'s explicit sources. Java sources already use
`src/**/*.java`; the existing library compiles with `system_current`. `home/android/platform-build/stage-permissioncontroller.py` verifies the
exact source revision/predecessor state and add only those files plus two
manifest nodes: a bind-only service protected by
`dev.makepad.octosense.permission.BIND_AGENT_PLATFORM`, and an unexported,
recents-excluded confirmation Activity. No launcher or new broad Home/Agent
permission is needed. Agent gets a mirrored finite AIDL and append-only methods.

The service rechecks the exact Agent package, matching platform signer and
current unlocked owner for every call. It uses the existing `Roles`/`Role` model
for inventory and `RoleManager` for actual holders. Candidate pages are bounded
to20, with opaque keys binding role, current holder, package incarnation and
restriction state. The native target contains package UID, signer, version and
install/update identity; none of those become a caller-selected grant.

Selection produces an immutable one-shot PendingIntent for the unexported
Activity, not a direct mutation. A process-owned ticket binds the selected
candidate and expected holder; process death or an expired/missing ticket fails
closed. The Activity re-resolves authority/identity and uses the existing native
role confirmation text (`RoleUiBehaviorUtils.getConfirmationMessage`), native
role title/description where no special warning exists, and the platform
confirmation dialog. Its parent fragment implements the existing
`DefaultAppConfirmationDialogFragment.Listener`. Only positive confirmation
rechecks the ticket and calls `DefaultAppViewModel`; successful completion must
retain `Role.onHolderSelectedAsUser`. None, when supported, must use the native
None path. Restriction intents and enhanced confirmation stay inside the
platform component; no intent or permission grant payload enters Octoscript.

## Reversible emulator feasibility

Parent-captured AOSP5558 metadata identifies the original controller as:

- `/apex/com.android.permission/priv-app/PermissionController@AE3A.240806.019/PermissionController.apk`
- Package `com.android.permissioncontroller`, UID10142, no shared UID;
  version330000000, target35/min30, SYSTEM+PRIVILEGED.
- Public AOSP platform certificate SHA256
  `c8a2e9bccf597c2fb6dc66bee293fc13f2fc47ec77bc6b2b0d52c11f51192ab8`.

The actual APK's DEX was inspected locally. The needed `Role`/`Roles`,
`DefaultAppViewModel`, `DefaultAppConfirmationDialogFragment` and
`RoleUiBehaviorUtils` method descriptors are present, including native policy,
qualification and selection callbacks. This supports testing an adapter against
the original platform implementation. It does not justify replacing the entire
AE3A controller with a newer pinned-Lineage build: role data, QPR methods and
resources can differ.

For validation, prefer the exact original APK with its original DEX/resources
untouched, adding only adapter DEX and the two manifest nodes. Keep the original
package/version/SDK metadata and sign with the matching fixture key. A same-version
data-APK update can avoid changing the APEX image and preserve package identity;
installation and restoration must first be demonstrated on a disposable clone
of5558. Verify UID, data inode, firstInstall, roles, permissions and native
permission dialogs before and after. A rollback to the original APK as a data
update is different from restoring the APEX-base path; do not assume
uninstall-updates preserves data. The disposable5560 clone has accepted the exact original APK as a same-version data update, then the reviewed adapter: UID10142, both data inodes, firstInstall and all40 role holders were preserved. Its10,089 native classes and3,154 original payloads remain byte-identical; only26 adapter classes and two manifest nodes were added. Original5558 remains untouched. No APEX image was changed.

The isolated adapter compiled against the pinned system_current/PermissionController-lib boundary; no native or stub classes were packaged into the added DEX. Both ordinary and unrelated platform-signed callers failed all20 boundary probes. The stager passes read-only check against the pinned tree (7files) and local revision/dirty-source/build-lock tests. The real native-role backend passes deterministic observation, ticket lifetime, cancellation, incarnation, policy/owner and actual-holder-readback tests. Build2515 then passed83 actual UI/service checks on5560: browser Cancel and Back preserve the holder, both independent browser fixtures can be selected through native consent, reinstall invalidates an open old confirmation, and nested Back/native recovery preserve OctoSense navigation. An exported ACTION_ASSIST + CATEGORY_DEFAULT fixture qualifies through the native Assistant implementation; confirmed selection and native None both change the real holder. All40 original role holders and assistant/voice-interaction/voice-recognition settings were restored, and all fixtures removed. The complete Agent/controller boundary passes22 probes for each independent signer. Other role-specific native behaviors remain separate acceptance gates.


Ordinary Google-image recovery was verified separately on5556. Its controller is
`/apex/com.android.permission/priv-app/GooglePermissionController@350820360`,
package `com.google.android.permissioncontroller`, with Android SYSTEM provenance.
Its certificate SHA256 `828a93a07a08950b567c577192a949570a9dca5196dd4318154dc4348cb6ec96`
differs from framework-res `301aa3cb081134501c45f1422abc66c24224fd5ded5fdc8f17e697176fd866aa`.
Recovery therefore validates the exact public native class in either finite system
package, rather than incorrectly requiring mainline signing equality with the
framework. It rejects ordinary, disabled or unexported lookalikes and always uses
an explicit component in a separate task. This does not widen the ROM adapter's
AOSP-only Binder/creator gate. The original2514 unavailable fixture exposed its
own hardcoded AOSP assumption before product recovery;2515 includes the corrected
finite recovery path.
