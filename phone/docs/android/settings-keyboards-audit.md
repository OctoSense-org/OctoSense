# Built-in keyboards: implementation candidate

System → Keyboards now has production source for a bounded inventory, observed
current/default state, provider details, enable/disable, the native current-keyboard
picker, provider-owned settings, and a clearly labelled Android subtype-editor
handoff. The custom `keyboards` entry and local search are finite navigation.
Android's standard input-method action remains native for this increment.
The Home test build and full Rust suite pass. Native/Android APK compilation and
emulator acceptance remain pending; this is not an accepted device feature yet.

## Native policy and consent

The adapter links the pinned SettingsLib implementation from Settings
`0f0669fc699f70adb20fe6ed4b2ff1c600da6f86` / framework
`ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b`. It uses
InputMethodSettingValuesWrapper for actual enabled state and required/last-keyboard
rules, InputMethodPreference for native security/Direct Boot warnings, and
InputMethodAndSubtypeUtilCompat.saveInputMethodSubtypeListForUser for the linked
secure-provider update. It does not duplicate secure-row serialization.

The UID1000 Broker requires the current unlocked owner and WRITE_SECURE_SETTINGS.
The finite service retains the signature permission and exact Agent package/signer
caller gate. Home and Agent gain no secure-settings privilege. Organization
permitted-input-method rules follow the native page, including the ability to
disable an already-enabled provider that the organization no longer permits.
The native wrapper excludes virtual-device-only IMEs and preserves its required
ASCII-capable system-keyboard rule. Unknown/overflowed observations fail closed.

Native model access is serialized on the main thread because SettingsLib's wrapper
is a UI-thread model. Binder worker requests have a bounded wait; Home dispatches
on its existing worker. Native consent runs in an unexported Broker Activity
reached only by an immutable one-shot PendingIntent with an opaque ticket and a
fresh task. Home verifies creator package, SYSTEM_UID, and immutability.

The native security warning calls its save listener with checked=false on Cancel.
Only the intended checked state may claim a change. Direct Boot's native dialog
has no cancel callback, so the exact linked InputMethodPreference.mDialog field
is observed only to retire the host on dismiss/Back. This hook never authorizes a
write or supplies warning text. A replacement second native dialog receives the
same exit hook. Handled configuration changes retain the actual dialog;
process restoration does not replay a prior operation.

Before displaying a warning and again at claim, the adapter reobserves owner,
policy, raw enabled/default/subtype/disabled-system rows, hardware keyboard state,
and package/service identity. Identity includes UID, first/last install time,
version, signer, APK inode/path, service metadata, and subtype definitions.
Replacement or any relevant state change retires the review without a mutation
retry. The key cannot make the final check/native multi-row save atomic; this
remaining framework race is explicit. Completion observes the target enabled
state and rejects a disabled target still reported as default. Opening the flow
is reported as opened, never applied, and Home refreshes actual state on return.

Provider settings resolve only the freshly observed advertised exported/enabled
Activity in the same installed package, including the required permission check.
Subtype navigation pins trusted Android Settings and carries only an observed
IME identifier. The native default picker accepts no caller-supplied IME ID.

## Interface and UI

Agent84 getKeyboardsSnapshot(request_id, query, offset) returns Bundle{ok,json}.
Agent85 prepareKeyboardFlow(request_id,key,target,operation) returns Bundle{ok,flow}.
Broker has matching finite snapshot/prepare methods. Operations are enable,
disable, provider_settings, subtypes, and choose_default. The default picker
requires an empty target; other operations require an observed opaque target.

The snapshot is schema1, with availability/reason/key/default_id,
can_choose_default, query/offset/total/page_size20 and rows containing target,
id/package/label/summary/restriction, enabled/selected/system/direct_boot and
separate enable/disable/settings/subtype capabilities. The complete native list
is bounded at 128; filters at 80 Unicode code points. Catalog lifetime is fixed
at ten minutes, displayed targets at 30 seconds, and native reviews at two minutes.
A fresh user flow can replace an abandoned unclaimed review; its old callback
cannot claim the replacement.

The compiled themed pages are “Keyboards” and “Keyboard details”. Labels are
“Choose current keyboard”, “Refresh keyboards”, “Filter keyboards”, “Search
keyboards”, “Clear keyboard filter”, “Previous keyboards”, “Next keyboards”,
“Enable keyboard”, “Disable keyboard”, “Keyboard provider settings”, “Keyboard
languages and subtypes”, “Refresh keyboard”, and “Android keyboard settings”.
Semantic accessibility identities bind the exact observed row/action. Held
presses cannot activate replacement targets. Nested Back preserves the list and
unsubmitted filter. Changed detail targets require a fresh observation/review.

## Current checks and remaining gate

The production Java authority/client suite passes (40 authority assertions and
9 foreground-client assertions); Broker manifest/AIDL boundary tests pass five
cases, and the entry-contract test passes. Shared Java/client compile against the
available SDK35 passes. The attempted SDK33 check had an absent local jar, so it
is not an API33 success claim. The focused Rust filter passes 11 tests, including four new model and four new
UI tests. The full Home suite passes 526 tests with `mobile-apps`. New coverage
includes native-capability gating, retained filter/Back state, held-press target
changes, live theme identity, and retired accessibility actions at 150% text scale
with a narrow scrollbar gutter. The first accessibility test failed because its
fixture omitted pass-sized clipping; the corrected fixture and full suite pass.
Receipts and the native source review hashes are in
`out/home/settings-keyboards/validation/`.

The actual AOSP Settings APK's relevant public SettingsLib descriptors were read
from its DEX, complementing pinned source inspection. Receipt:
`/tmp/octosense-keyboard-settingslib-abi.json`. The linked production SettingsLib
still requires its real pinned Soong compile. Its optional AvatarPicker Activity
is explicitly removed by manifest-merger directive. The compiled manifest must
be checked for the complete permission/component delta, including SettingsLib's
READ_DEVICE_CONFIG dependency; source-manifest tests alone do not prove this.

Parent-owned independent aware/unaware IME and editor fixtures will validate
native warning order/Cancel/Back, default picker and actual synthetic input,
last-keyboard policy, enable/disable/current-provider semantics, stale reinstall,
configuration changes, provider settings, and exact original raw state restoration.
Only synthetic editor text is permitted. No keyboard has been enabled, disabled,
or selected by this candidate yet. Ordinary validation belongs only to emulator
5556; privileged validation belongs to parent-owned disposable5560. No phone use.

Built-in subtype editing, physical keyboard layouts, provider-private settings,
work-profile behavior, pointer speed, and text-to-speech remain separate parity
work. Native handoffs do not count as completed built-in subtype controls.
