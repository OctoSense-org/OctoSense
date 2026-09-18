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
