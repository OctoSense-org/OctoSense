# DND schedules, rules, and allowed interruptions

Status: implementation and emulator acceptance in progress on2026-09-25.
The initial read-only audit below established the native contract. The current
implementation adds built-in default interruption policy and Android time schedules.
Policy acceptance passes 837 emulator checks on Home 2524; schedule acceptance
passes 210, including native activation and exit with Home stopped. Native
compatibility findings and failed/corrected receipts are in
[the emulator validation record](settings-platform-emulator-validation.md).
This does not complete provider/calendar rules,
per-rule policy, effects, or physical-device validation.

Sources are the actual pinned ROM: Settings
`0f0669fc699f70adb20fe6ed4b2ff1c600da6f86`, frameworks/base
`ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b`. Paths below are relative to the ROM
checkout. Existing OctoSense references are relative to this repository.

## Recommendation

Extend the existing on-demand, system-UID Zen broker with a finite observed-state
controller. Implement a built-in **Do Not Disturb** page containing manual mode,
**Allowed interruptions**, and **Schedules and rules**. The first useful increment
should edit the manual/default interruption policy and Android's own time-schedule
rules. It should show other rule types honestly and retain their native/provider
configuration flow. Do not recreate rule scheduling in OctoSense, grant Home DND
policy access, or write raw `zen_mode` settings.

Keep policy editing and schedule editing separate. A saved policy is not proof
that a notification will be delivered: active rules, the interruption filter,
channel settings, people affinity and runtime notification permission also apply.
Show saved/manual policy separately from the system's consolidated current policy
and active rule observations. Off for the manual mode may leave another scheduled
mode active; do not silently disable those rules.

## Native contracts and compatibility

| Source | Observed contract and implication |
| --- | --- |
| `packages/apps/Settings/src/com/android/settings/notification/modes/ZenModesLinkPreferenceController.java:52` and `notification/zen/ZenModePreferenceController.java:70` | `Flags.modesUi()` chooses the newer Modes UI versus legacy Zen UI. API level alone cannot select semantics. |
| `frameworks/base/packages/SettingsLib/src/com/android/settingslib/notification/modes/ZenModesBackend.java` | Combines `getAutomaticZenRules()` with `getZenModeConfig()` to determine actual status; updates use `fromUser=true`; manual DND policy is distinct from automatic rule policies. |
| `.../SettingsLib/.../notification/modes/ZenMode.java` | Manual DND policy is not editable while a temporary Alarms-only/Total-silence manual filter is active. Automatic-rule policy customization can change its filter to Priority. Preserve this distinction instead of silently changing filters. |
| `packages/apps/Settings/src/com/android/settings/notification/zen/ZenModeBackend.java` | Legacy paths use `getNotificationPolicy`, finite category/sender changes, condition helpers, and `fromUser=true` only when `Flags.modesApi()` applies. Unedited categories, sender fields and visual effects must survive. |
| `frameworks/base/services/core/java/com/android/server/notification/NotificationManagerService.java:5806–6200,6381–6500` | Config observation, user-origin writes and rule ownership are service checked. Newer ordinary-app global-filter/policy calls may instead create an implicit app rule. UID1000 retains genuine system/global authority. |
| `.../notification/ZenModeHelper.java:1128` | System UID can manage every rule; ordinary ownership checks alone therefore do not constrain the broker. Its finite controller must add narrower allowed target/type checks. |
| `.../notification/ZenModeHelper.java:1330–1607` | User-origin changes track user-modified rule, policy and device-effect fields. Use native update APIs with the correct origin; do not reconstruct internal rule XML or clear user locks. |

The existing `OctoSenseZenSettingsService` already requires the exact Agent package
and matching signer, current unlocked owner0, system UID, admin status, and no
`DISALLOW_ADJUST_VOLUME` for writes. Its original `IZenSettings` transactions0/1 remain
snapshot/manual-mode only. The new typed transactions are appended without renumbering them;
keep Agent and Home ordinary UIDs and reuse the existing signature-gated service.
New Binder reads/mutations run off the UI thread. No new system-UID app, manifest
export or PermissionController adapter is needed for this slice.

Before implementing policy writes, verify the built broker's effective target SDK
and compile-time/native flags. NMS preserves compatibility behavior for callers
targeting before P/R, including alarm/media/system/conversation fields, even in the
policy conversion path. Do not assume that a public API35 SDK or the OS version
proves the broker's effective target SDK. Test the actual packaged broker.

## Built-in controls

### Allowed interruptions

Offer observed finite values, with independent capability and unknown states:

- Calls: anyone, contacts, starred contacts, no one.
- Messages: anyone, contacts, starred contacts, no one.
- Conversations: all, important, none where the native policy exposes it.
- Repeated callers; alarms; media; system/touch sounds; reminders; calendar events.

Read the actual repeat-call window from the native resource/service when showing
its explanation; `ZenModeFiltering.RepeatCallers` loads
`config_zen_repeat_callers_threshold`, so avoid hardcoding a duration.

Start with the manual/default policy. Preserve every unedited field, including
suppressed visual effects, conversation senders and newer priority-channel state.
Use a copy of the freshly re-read policy rather than a default constructor with
missing fields. `STATE_CHANNELS_BYPASSING_DND` is service-derived, and Modes has a
separate `STATE_PRIORITY_CHANNELS_BLOCKED`; never synthesize either from an app
count. UI labels must distinguish stored choices from the currently consolidated
policy. Match native Modes read-only behavior for a temporary special manual
filter. Do not switch to Priority just to enable a policy editor.

Per-rule custom policy, individual priority conversations, contact selection,
channel bypass and visual/device effects are separately scoped follow-ups. Merely
adding a link to those native pages does not complete their replacement.

### Time schedules

Use the native `ScheduleConditionProvider`, not Home timers or a new background
worker. Classify an editable time rule only when all of these match freshly:
Android ownership, the exact native schedule provider, a valid native schedule
condition, current owner user, and supported rule type/flags. A type label or a
parseable arbitrary URI alone is insufficient. Existing legacy time rules can lack
newer type metadata; derive them from the verified native owner and condition.

A built-in list/detail editor should support:

- Observe name, enabled, actual active/inactive/unknown state, and rule type.
- Create an explicitly reviewed time schedule; edit its name, days, start/end,
  exit-at-next-alarm preference and enabled state; delete after a local review.
- Start/end validated to00:00–23:59; locale-ordered weekday labels map to native
  `Calendar` day values1–7. Overnight and equal start/end use native next-day
  semantics. Represent empty days as inactive/not scheduled; never manufacture
  a weekday or enable a draft with no chosen days without an explicit decision.
- Keep a stable Save/Cancel draft. Changing an externally observed rule or policy
  invalidates Save while preserving the text for review. Deletion is one-shot.

`ZenModeScheduleRuleSettings` and the newer
`ZenModeSetSchedulePreferenceController` use
`ZenModeConfig.tryParseScheduleConditionId` / `toScheduleConditionId`, native hour
and minute validation, and the same next-day calculation. `ScheduleConditionProvider`
owns time-zone/clock/alarm reevaluation and snoozing. Copy the observed complete
`AutomaticZenRule` (its Parcel representation or verified copying builder) and
change only reviewed fields. Preserve owner/configuration activity, filter,
ZenPolicy, device effects, icon/type metadata and native user-modified state.
Do not turn a time edit into an unrelated policy or filter change.

Creation should select the exact native schedule owner and create with user-origin
semantics where supported. Seed an explicit policy choice according to the native
legacy/Modes path; do not copy the consolidated effective policy as if it were the
user's default. Native add/update return values are acceptance; require fresh
rule readback before showing Saved. Check native rule limits and availability
instead of promising unlimited schedules.

### Remaining native/provider flows

Calendar event rules require real calendar/account data, user-scoped selection
and native reply criteria. Include them as a distinct follow-up, with native
calendar/rule configuration until implemented. Do not add READ_CALENDAR to Home
merely to populate a schedule picker.

Third-party providers own their trigger configuration and authentication.
`AbstractZenModeAutomaticRulePreferenceController.getSettingsActivity` uses the
observed rule's configuration activity or provider metadata and checks that the
activity owner UID matches the rule owner UID. The OctoSense route must likewise
re-resolve an observed provider and target incarnation, verify exported/enabled
component and owner identity, and pass only the finite native rule-ID extra.
Keep raw component names, URIs and intent extras out of script requests. Use a
fresh recents-excluded task so Cancel/Back returns to the preserved picker.
Provider disappearance/reinstallation retires the route rather than launching
an unrelated replacement.

Granting/revoking notification-policy access remains the native special-access
consent flow, including its warning and rule-cleanup side effects. Listing a
provider does not authorize granting access. Managed/work-profile, bedtime,
driving, custom manual modes and provider-specific triggers remain native until
their individual policy and confirmation behavior is implemented and tested.

## Proposed finite observation and mutation boundary

Use a dedicated controller/model/host, sharing the existing Settings widget kit:

- `dnd_snapshot {id, offset, generation?}` → `launcher.dnd_state` with schema1, request ID, owner availability,
  native feature flags, manual/current mode, observed policy, consolidated policy,
  capabilities, bounded rule rows and selected schedule. Page20, at most256 rows;
  explicitly mark truncation/unsupported fields instead of inventing values.
- Each row has an opaque rule target, observed key, localized label and finite
  kind/status. No public raw rule ID, condition URI or arbitrary native payload.
  Manual mode has its own fixed identity, never an automatic-rule deletion target.
- `dnd_policy_set {key, field:<finite enum>, value:<finite enum/bool>}` edits exactly
  one reviewed setting. Re-read and preserve all other policy fields.
- `dnd_schedule_save {key, target?, name, days:[native weekday], start_minute,
  end_minute, exit_at_alarm, enabled}`; target absent is explicit creation against
  a current creation capability. Strict bounds/types, no raw owner/filter/policy.
- `dnd_rule_enabled {key,target,enabled}`, `dnd_rule_delete {key,target}` require observed per-row capabilities.
  Direct mutations initially apply only to verified Android time rules;
  unsupported/provider kinds remain observation/configuration handoffs.

Bind keys to current owner, rule incarnation/creation time and full observed native
state, rule/provider identity, policy/restrictions and feature flags. Keep the
catalog stable for paging, but mutation authority short-lived (for example20s),
one-use and refreshed only by a new observation. Re-resolve before each write and
check post-write state. Unknown native enum/fields disable the affected operation.
External policy/rule changes, expiry, lock/user change or replaced roots retire
confirmation and action authority. A changed active condition should refresh
status, not silently retarget a saved draft; separate status-only changes from
configuration revision where the native model permits that distinction.

Result reasons distinguish applied, requested/pending, changed target, restricted
and unavailable. Poll only while the trusted Settings page is visible; reads never
activate a mode, create a rule, grant access or scan providers for side effects.
Update controls in place and retain search/editor state. Publish new accessibility
targets before acknowledging same-page mutations, as in the corrected2519 host.

## Acceptance plan

1. Contract/model tests reject arbitrary field names, native URIs/IDs, malformed
   days/times, excessive names/pages, unsupported modes and forged/expired/replayed
   targets. Tests distinguish inherited per-rule policy from default and
   consolidated policies and preserve all unedited serialized native fields.
2. Real broker boundary tests extend both ordinary and unrelated platform-signed
   probes; exact Agent only, owner/admin/unlock/restriction rechecks, no Home
   `ACCESS_NOTIFICATION_POLICY` grant. Existing manual transactions remain intact.
3. Disposable emulator only: capture exact baseline Zen configuration/rules,
   policies, feature flags, app access, volumes and roles. Edit each global policy
   field, verify native readback and independent notification ranking. Use
   synthetic local notifications/contacts for sender/conversation effects; no
   real contacts, messages or calls. Readback alone is not delivery acceptance.
4. Create a uniquely named inactive native time rule; Save/Cancel/rename/days,
   overnight/equal endpoints, enable/disable, expired/replaced/deleted targets and
   explicit delete. Verify unrelated rule IDs, fields and policies remain intact.
5. Observe a real scheduled activation and exit with a short upcoming boundary
   rather than writing the current Zen mode from the test. A synthetic next-alarm
   fixture tests exit-at-alarm. Framework unit tests cover DST/time-zone edge
   cases; any emulator clock/time-zone manipulation needs an isolated baseline
   and exact restoration and does not replace physical-device acceptance.
6. Synthetic provider configuration: correct identity, pre-existing provider task,
   Cancel/Back, removed/reinstalled provider, native policy-access denial and
   consent. Never auto-grant access to make a test pass. Unknown/managed kinds
   expose no unsupported direct mutation.
7. Actual keyboard, narrow150% layout, button-start scroll, stable accessibility
   focus/semantic targets, stale-node rejection and cold entry. Ordinary5556 is
   honest unavailable/read-only with trusted native recovery. Actual privileged
   changes stay on the parent's disposable emulator.
8. Restore exact baseline policy/rule fields and remove every synthetic fixture;
   verify independent notifications, role/permission records, identity and crash
   buffer. Hardware call filtering, real calendar providers, secondary/work users
   and full Modes effects remain separate ledger gates.

## Implemented boundary and current validation

The shared `DndSettingsContract` / `DndSettingsBackend` supply the finite model,
with `DndPlatformSettings` in the existing system-UID SettingsBroker. Existing
IZenSettings transactions0/1 remain intact;2–6 implement snapshot, policy,
schedule save, enabled state, and delete. Agent transactions64–68 mirror that
boundary under `dnd_settings_v1`. No new exported service or Home privilege was
added. The actual validation broker targets SDK35 and observations carry native
Modes API/UI flags.

Home has dedicated typed model/host/UI modules and stable Octoscript-Makepad
pages. Policy changes preserve all six native Policy fields. Time edits copy the
whole native rule and preserve unknown condition query fields. Create uses native
NMS policy initialization, rather than copying a consolidated policy. Mutation
leases are one-use, expire after20s without observation, and retire on config,
owner/focus/policy changes. Save retains its draft while pending or rejected and
closes only after the backend confirms matching native readback. Reads never
create or enable rules.

Shared backend tests cover validation, expiry/replay, owner/policy/config changes,
unknown policy-bit preservation and applied-versus-requested outcomes. The pinned
Android15 Agent/Broker build passed; actual ordinary and unrelated platform-signed
caller probes passed37 each on the disposable emulator, with existing identities,
roles and permission records preserved. Home UI/native schedule acceptance is
still pending; do not infer delivery or scheduler behavior from those unit and
boundary checks alone.

The follow-up2523 source maps `ZEN_MODE_SETTINGS` and the finite custom `dnd`
route directly to the dedicated page. Preferred system navigation requires the
new alias capability metadata; old Home builds keep pinned native recovery.
The existing standard intent filter remains unchanged so an APK update does not
cap unrelated standard-action priorities. Native DND recovery uses a dedicated
recents-excluded task to preserve Back even with an existing native Settings task.
Actual2523 entry/recovery acceptance is pending. Other rule types
currently use the pinned native DND recovery screen, not a per-provider adapter.
Calendar/provider triggers, per-rule custom policy/effects, schedule alarm-exit
behavior, sender affinity, locale completeness, work/secondary users and production
ROM/hardware remain separate parity/acceptance gates.


### Native API35 compatibility found during acceptance

The first2522 privileged observation exposed an actual runtime difference despite
successful pinned compilation: the Lineage ZenRule has `isActive()`, while the
original AOSP API35 image has `isAutomaticActive()`. The direct call raised
NoSuchMethodError in the broker before any policy write. The finite adapter now
invokes the present native method; it never reconstructs snoozing/condition logic.
Missing or unsupported status is null. A specific LinkageError boundary returns
unavailable for unsupported framework linkage without swallowing VM failures.
The regression executes both native API shapes, unknown status and missing
internal linkage, and confirms OutOfMemoryError is not converted to unavailable.
The corrected native build and actual policy/schedule acceptance are pending.
