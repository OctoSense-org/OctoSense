# Settings emulator acceptance fixture

`../../scripts/run-settings-accessibility-probe.py` builds, installs and removes
a disposable independently signed instrumentation APK. It uses public Android
accessibility APIs to interact with the actual built-in Settings widgets. The
runner refuses physical devices and refuses to overwrite an existing fixture.
Use one UI-driving process per dedicated emulator.

Scenarios:

- `inspect`: visible bounds, text and scroll hierarchy.
- `smoke`: overview navigation, Search editing, keyboard/focus, retired nodes,
  disabled actions and scrolling. Start on the Settings overview.
- `entries` / `cold_entries`: finite custom and standard Android entry routes,
  process startup, nested Back and native recovery. `entries` currently covers
  23 custom routes and 21 standard Android actions; the package-specific
  22nd action is covered by `notification_entries`.
- `rom_defaults` / `preferred_fallback`: privileged ROM default resolution and
  real SystemUI gear, or ordinary-install native fallback. These require the
  corresponding documented package state; the fixture does not grant it.
- `density`: changes and restores an emulator's display override while checking
  editor identity, focus, process, physical-coordinate touch and exact typing.
- `display_controls`: real Save/Cancel, density/default, Night Light color
  service/compositor, warmth, schedule validation and Location gate. Requires
  the trusted platform helper and the documented
  [Night Light capability fixture](../night-light-capability/README.md). All
  tested writes use the visible OctoSense controls. Exact secure-row and Location
  restoration runs in `finally`; only cleanup temporarily adopts shell write
  permission. The fixture tests on a default-density emulator. Return warmth
  through the UI before deleting its raw row, because ColorDisplay caches the
  active tint temperature. After an interrupted test, restore the saved raw
  rows and reboot only the dedicated emulator to reset service caches.
- `night_unavailable`: after removing that capability overlay and rebooting,
  verifies that density still works while all unsupported Night Light actions
  are disabled and rejected.
- `sound_feedback`: on the documented helper-equipped AOSP emulator with
  one native vibration intensity level and Medium defaults, checks sound
  preferences, the master dependency, real VibratorManager intensity state,
  legacy ring/touch companion settings, and missing keyboard capability. Exact
  presence/values of all 14 touched System/Secure rows are restored in `finally`
  using shell settings commands with constant keys and prevalidated values. A
  writable observed choice marks readiness; Refresh alone can precede the new
  snapshot and dependent row layout.
  This does not establish physical haptic strength or audible charging playback.
- `notification_channels`: installs two additional independently signed
  disposable publishers. The modern app has 24 channels, with 23 in two groups
  and one ungrouped (Android's synthetic null-ID container is not a real group); the
  target-25 app uses Android's sole legacy default channel. The probe drives
  app/group/channel on/off, importance review and Cancel, then checks actual
  delivery, permission state, native legacy coupling, unrelated channel fields,
  user-lock bits, pagination, retired reviews and nested Back. Only synthetic
  publisher state is inspected; both publishers are removed in the runner's
  cleanup. This requires the notification-capable Agent/Broker and Home build.
- `notification_unavailable`: ordinary-install path without the ROM helper.
  Opens Home's app details and the built-in notification pane, checks unavailable
  state and disabled mutation rejection, then tests native recovery and nested
  Back with the app filter preserved. It changes no notification preference.
- `notification_entries`: cold/warm standard per-app entries, fresh package
  selection, missing-app denial, invalid selector/data/custom-route rejection,
  ignored UID/channel/mutation extras, no replay after Back, and native recovery
  with an existing Settings task. It changes no notification preference.
- `roles_unavailable`: ordinary Home without the role adapter/helper, disabled
  choices, explicit native Defaults recovery with an older controller task,
  and Back to the built-in page and Apps. It changes no role assignment.
- `roles_entries`: cold/warm standard Default Apps navigation, ignored
  role/package/user/UID/target/value extras, no replay after Activity return,
  malformed data/type/custom-route denial, and unchanged role holders.
- `default_roles`: requires the disposable `OctoSense_ROM_Roles_API35` clone
  and the role-capable Controller/Agent/Home build. Two independently signed
  local-only browser fixtures and an exported ASSIST Activity exercise native qualification, Cancel/Back,
  confirmed selection, public role readback, removal/reinstallation during an
  open review, the native Assistant None flow, and nested Back/filter preservation.
  The original browser is restored through the visible confirmation flow;
  no shell role setter is used. The runner compares all original role holders
  and removes all three fixtures and the temporary reinstall APK. It also
  restores exact raw presence/values of the three legacy Assistant/voice-service
  Secure rows, only after the UI restores the original role assignments.
- `runtime_permissions`: requires the disposable `emulator-5560` clone and
  permission-capable Controller/Agent/Home builds. A modern app reports its own
  grants and raw AppOps after common-group changes, including Ask versus denial,
  approximate/precise location and foreground/background access. A legacy app
  exercises native revoke warnings, Cancel/Back, positive consent and target
  removal/reinstallation during a warning. Legacy denial is checked using
  AppOps and `REVOKED_COMPAT`, because its raw permission grant bit stays true.
  Only these independently signed synthetic apps are changed and both are
  removed afterward. The runner also verifies every existing package's original
  grant/flag records and every original role holder after cleanup. Native recovery,
  nested Back, retained filter and retired/selected actions are checked through
  the visible built-in widgets.
- `permissions_legacy`: the same legacy warning cases extracted for focused
  return-path diagnosis, including rotation, Cancel/Back, reinstall protection
  and positive consent. It retains the same disposable-emulator guard, synthetic
  apps, grant/role preservation checks and cleanup. Phase logs contain fixed
  scenario labels only; failed mutations are never automatically retried.
  Rotation waits for the actual display and replacement focused warning window;
  restoration waits for a focused Home laid out in the restored orientation.
  Stored rotation preferences alone do not establish transition completion.
- `permissions_unavailable`: ordinary Home without the native permission
  service/helper shows the unavailable state without actionable grants, preserves
  Home's permissions, and supports Android app-info recovery and nested Back.
- `dnd_unavailable`: ordinary Home shows unavailable policy and schedule controls,
  rejects disabled actions and returns from native DND recovery and nested Back.
- `dnd_entries`: cold custom and standard DND entries, ignored mutation extras,
  no replay after a native Activity hop, and recovery with an older Settings task.
  It does not change policy, mode or schedules.
- `dnd_observer`, `dnd_policy`, `dnd_schedules`: run through
  `run-dnd-observer-probe.py --ui-scenario`, which installs the independently
  signed native observer and synthetic ranking publisher. The observer scenario
  verifies nested instrumentation leaves the UI automation connection intact.
  Policy acceptance checks all nine fields and actual notification ranking under
  Priority mode, then restores the complete policy and original rules. Schedule
  acceptance only creates new names prefixed `OctoSense validation `; it checks
  editing, Cancel, delete review, empty days and stale drafts, and waits for a
  future native schedule to start/end while Home is force-stopped. One controlled
  external removal of a newly created disabled synthetic rule simulates another
  Settings client deleting an open draft. Existing rules are never removed or
  edited. The runner records complete before/after observations and removes its
  notification-listener grant and both fixture packages, including on failure.
  Policy choices wait in place for selected readback after a click; searching by
  scrolling at that point could skip an asynchronously updated row. Manual mode
  also waits for its separate capability observation. No mutation is retried.
  The result retains the primary failure if cleanup itself fails.

Scenario availability is not an acceptance claim; see the validation record for
the exact builds and scenarios actually run.

App-row navigation distinguishes queued accessibility actions from completed
navigation. If a filtered catalog retires the clicked node before processing,
the fixture permits one new navigation attempt only after that node no longer
refreshes and a new semantic node for the same exact package appears. A current
node is never retried, and mutation actions are never replayed. Any such retry
is reported in the acceptance result.

These checks do not establish physical-panel behavior, full TalkBack/switch
access, a production ROM boot, or parity for unmigrated Settings features.
See [the platform validation record](../../../docs/android/settings-platform-emulator-validation.md).
