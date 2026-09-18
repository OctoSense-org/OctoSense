# The OctoSense agent service (Phase 2)

One privileged, persistent process in the ROM that gives agent code the powers
an ordinary app cannot have, behind a single Binder surface, reached through
the same bridge contracts the launcher already uses.

## Shape

- Package `dev.makepad.octosense.agent`, platform-signed, `system_ext/priv-app`,
  `persistent`, `directBootAware`. It exposes `IAgentPlatform` (AIDL in
  `android/contracts`), and the System Bridge forwards to it so the launcher's
  capability model stays the one entry point: a capability is present when the
  service is bound and its permission is held, absent otherwise.
- Callers are allowlisted by signature (the platform key) and by package;
  every call is logged with the caller and the outcome.

## Capabilities, in the order they unblock agent ideas

| Capability | Privileged API | Agent use |
|---|---|---|
| `tasks` | `ActivityTaskManager.getTasks`, `REAL_GET_TASKS`, task snapshots | Recents with thumbnails; "what is open" |
| `screen` | `SurfaceControl` screenshot, `CAPTURE_VIDEO_OUTPUT`, no consent dialog | The agent sees the screen; UI understanding |
| `tree` | Accessibility node tree of the active window, system-wide | Structured reading without pixels |
| `input` | `InputManager.injectInputEvent`, `INJECT_EVENTS` | The agent taps, types, scrolls |
| `settings` | `WRITE_SECURE_SETTINGS`, `NETWORK_SETTINGS`, `BLUETOOTH_PRIVILEGED`, `DEVICE_POWER` | Direct radios, brightness, modes; no root adapter |
| `notifications` | `MANAGE_NOTIFICATIONS`, the listener the bridge already holds | Read, act, snooze, reply |
| `apps` | `INTERACT_ACROSS_USERS`, `START_TASKS_FROM_RECENTS`, `STOP_APP_SWITCHES` | Launch, switch, close on the agent's behalf |
| `statusbar` | `STATUS_BAR_SERVICE` | Own the shade and quick settings from the OctoSense panel |

## Safety rails

- A per-call budget and an on-screen indicator while `screen` or `input` is in
  use, like the camera and microphone dots.
- `input` refuses while the keyguard is showing and while a password field has
  focus.
- Everything degrades: the launcher's fallbacks remain, so the Home-app build
  on other phones keeps working with the bridge alone.

## Harness

`scripts/` on the host: build, flash the boot and system images, run the
device checklist, capture screens. Each capability lands with a checklist
entry that exercises it on the bench phone.

## Implementation (vendor/octosense/agent)

`OctoSenseAgent` builds in the ROM tree with `platform_apis: true`, so it uses the
hidden framework surface directly: `ActivityTaskManager.getTasks` and
`getTaskSnapshot` (tasks), `IWindowManager.captureDisplay` with a synchronous
`ScreenCapture` listener (screen, the path SystemUI's own screenshot takes),
`InputManager.injectInputEvent` (input), the three settings tables (settings),
`startActivityFromRecents` / `removeTask` / `forceStopPackage` (apps) and
`StatusBarManager`'s hidden expand and collapse (statusbar). `tree` is absent
until the accessibility bridge lands.

Callers must be platform-signed and one of the OctoSense packages; results are
Bundles with `ok` and a `reason` (`denied`, `keyguard`, `unavailable`, `failed`).
Input is refused while the keyguard is showing. The last 200 calls are kept in
an audit log (`getAuditLog`).

On the bench phone the service answers `dumpsys` too, so every capability can be
driven from adb without a client: `scripts/agent-test.sh`.

`ProvisionReceiver` runs once per user on boot and seeds the Material You
palette with the launcher's purple (0x6750A4, tonal spot), which SettingsProvider
has no default for. The same setting was applied by hand on the LineageOS phone
on 18 Sep 2026 and turned its shade purple immediately.
