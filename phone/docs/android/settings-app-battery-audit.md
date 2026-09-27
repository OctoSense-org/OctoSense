# Per-app battery policy

The built-in Apps → App details → Battery usage page implements the three native policies: Restricted, Optimized and Unrestricted. This is a finite package-policy API, not an arbitrary AppOps or allowlist editor. Implementation and unit checks are complete; ordinary Home 2026092525 emulator checks and privileged mutation acceptance now pass. No physical phone is part of this validation.

## Native behavior and authority

The audited native implementation is Settings commit `0f0669fc699f70adb20fe6ed4b2ff1c600da6f86`, particularly `BatteryOptimizationModePreferenceController`, `BatteryOptimizeUtils`, `BatteryUtils`, `PowerBackgroundUsageDetail`, and SettingsLib `PowerAllowlistBackend`. The Broker is the existing platform-signed, system-UID Settings component. Home and Agent gain no general AppOps or power-policy setters.

| Policy | RUN_ANY_IN_BACKGROUND | Power allowlist | Target SDK below 26 |
| --- | --- | --- | --- |
| Restricted | Ignored | Removed | RUN_IN_BACKGROUND also Ignored |
| Optimized | Allowed | Removed | RUN_IN_BACKGROUND also Allowed |
| Unrestricted | Allowed | Added | RUN_IN_BACKGROUND also Allowed |

Policy AppOps are observed with native `unsafeCheckOpRawNoThrow`; allowlist state uses DeviceIdleController plus the native exempt/default-app rules. The evaluated `checkOpNoThrow` API translates `MODE_FOREGROUND` according to process state and cannot identify the stored policy. A contradictory combination, unsupported raw mode or mismatched pre-O pair is displayed as a custom Android state, rather than inventing a selected policy. Choosing a finite policy explicitly repairs that state.

The native adapter reproduces framework restriction-reason notifications and the allowlist API's `appRestrictionsApi` condition. Pinned ActivityManagerService documents `noteAppRestrictionEnabled` as a telemetry operation that does not change app state; AppRestrictionController emits its stats event. OctoSense does **not** reproduce Settings-only BatteryDatabaseManager history, Settings usage-log records or Settings-local caches. Framework policy is authoritative; a full shared Settings telemetry implementation remains separate work.

Writes require the current unlocked owner/admin and exact trusted helper identity. Native immutable states remain read-only: system power allowlist, forced Settings resource lists, default SMS/dialer, active device administrator, system-exempt power AppOp, protected package state, and platform UIDs. Suspended/instant/uninstalled packages and `DISALLOW_APPS_CONTROL` cannot be changed. Shared UIDs are displayed read-only because a change can affect multiple packages. Missing native methods/resources or unknown authority disable writes.

The Broker binds its one-use key to the exact package UID, signer, install/update/version identity, source APK device/inode, current policies, raw AppOps and allowlist observation. Each multiwrite step re-resolves that identity and expected intermediate state. A failure after a write reports partial/unconfirmed state; no rollback, replay or automatic mutation retry runs. Successful transport alone never means the policy changed: the backend rereads every relevant field before reporting `app_battery_applied`.

## UI and boundary

The renderer is the existing Octoscript-Makepad Settings view. The page retains live theme, font scaling and Apps navigation state. Restricted requires an explicit local review explaining possible delayed notifications/background work. Cancel performs no write. The selected policy remains the observed one until native readback arrives. A stale review loses its confirmation capability, and a held button cannot change its package/key when a poll arrives.

Wire operations are `app_battery_snapshot` and `app_battery_set`, with `launcher.app_battery_state` schema 1 observations using `request_id`. Only a validated package, opaque 64-character key and the three-value mode enum cross the mutation boundary. The host requires the trusted Settings root, focused foreground page, current existing Apps detail target and matching request ID. Agent transactions 71–72 forward through `IAppBatterySettings`; the Broker exposes only snapshot/set behind the existing signature permission plus exact package check. Native app-info recovery is retained on ordinary installations.

## Validation

`tests/test_app_battery_backend.py` executes the real shared backend with deterministic native observations: all modes, pre-O coupling, custom states, expired/forged/replayed targets, package reincarnation, external changes, policy/shared-UID rejection, partial failure and pending readback. Rust tests cover strict decoding, foreground retirement, reviewed Restrict/Cancel, stale press/review authority, preserved observed state, live theme/150% font and nested Back.

The public Android accessibility fixture adds `app_battery` for the owned privileged emulator and `app_battery_unavailable` for the ordinary emulator. The supported scenario checks actual AppOps and DeviceIdleController state independently, including temporary modern/pre-O/shared-UID packages. It removes all synthetic packages and compares original power allowlist and both background AppOps inventories afterward. Emulator receipts belong in `out/home/settings-app-policy/validation`; platform acceptance and hardware limitations remain recorded separately in the platform validation ledger.

Home 2525 ordinary emulator5556 passed 24 battery-unavailable checks, 27 network-unavailable checks, eight cold entries/33 checks, and 33 accessibility/IME checks; the crash buffer was empty. Recovery was checked with a pre-existing native Settings task. The combined build passed 467 Rust tests and 69 repository tests. The separate disposable privileged clone passed 111 native checks after correcting raw AppOp observation: `checkOpNoThrow` evaluates a foreground-only raw mode to ignored for a background app, so the original reader incorrectly displayed Restricted. The corrected raw reader preserves Custom state and its observation key. All three modes, pre-O coupling, review Cancel, stale review denial, Home restart, shared/protected UID gates and native AppOps/allowlist readback passed; original AppOps/allowlist inventories were preserved and all fixtures removed.

Battery consumption, job delivery under real Doze, OEM power-management additions and shared-UID mutation are not established by these policy readbacks. This slice does not replace battery-usage statistics, app force-stop, storage clearing or other unfinished Settings areas.
