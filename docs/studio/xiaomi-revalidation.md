# App Studio re-validation on a stock Android phone

The rebased App Studio (branch `studio/313-rebase`, after the merges of `main`
`3bd15bad` and `5c28da7d` and two review batches) was re-run on a Xiaomi
23021RAAEG, Android 15, stock launcher, with OctoSense Home already installed
from the store beside it. Only the separate developer test package
`dev.makepad.octosense.studio` was installed and used; the installed Home, its
Bridge and the default launcher were untouched. The OnePlus 6 evidence in
[oneplus6-validation.md](oneplus6-validation.md) belongs to the pre-rebase
build and does not transfer; this run replaces it for the rebased code.

An earlier run on this phone (source `cde02171`, before the second review
batch) passed the same probe and a 129-call acceptance flow; the figures
below are from the final run.

## Build

| Item | Value |
| --- | --- |
| Source | `c34dc7a5` (clean tree), built with `--dev-mode --development --package-name dev.makepad.octosense.studio` and `MAKEPAD_FORCE_DEBUGGABLE=1`, so `run-as` works on a stock phone |
| Home APK, SHA-256 | `3d6575af4a227e2179cbced786b1103a4b3c78613cfd5d4ac7098089d231e755` (version code 2026101103, debuggable; the installed Home kept its own version) |
| Developer mode | Provisioned in the test profile only (`files/.octosense/developer-profile` and `dev-mode.json`, scope `all`); the probe restores the previous state when it finishes, and the other checks remove it |
| Fixtures | Written only under the package's private `files/studio-fixture/`, the one folder the `studio-render:` and `studio-flow:` launch fixtures accept |

## L0 render probe (`tools/studio-device-probe.py`)

All four cases passed: `denied` (no developer grant: refused), `light` and `dark`
(970×575 PNGs with different content, `dbd044e3…` and `0621af7c…`), and
`background`. This is the milestone 1 render evidence the earlier report did
not include.

## Task Planner acceptance (`tools/studio-flow-device-test.py`)

The authoring model was Claude Fable 5.1, run as a subagent of this session
from [task-planner-brief.md](task-planner-brief.md) with the acceptance
contract and App Flow's Splash documentation as its only references; the
Codex session's DeepSeek bundle of 3 October was not available on this machine.
The model's transcript and the bundle are kept outside the repository; their
hashes bind the run.

| Item | SHA-256 |
| --- | --- |
| Bundle digest (`studio.bundle_check`, BLAKE3 as App Hub computes it) | `72af424c6cc5f1fa1f665074df68a8e473ae530fa588e716e5badb8f77922ae7` |
| `main.splash` (10,251 bytes) | `cddbd51e4baf590e…` |
| Authorship transcript | `2b6e0f872d318643d612a13783dd144b9093fa97d3fc6ddca06c9415ab65ab02` |
| Generation receipt | `5435402c9b2d1df0a4e2584199cfb47238866fdc5128778ec1c342d14eaee233` |
| Acceptance contract | `58a2091f2593a231982f755f643559b9dc29649b8d657b67c4edc3d3d3cbf715` |

Result: `functional_passed_visual_review_required`, 103 seconds, **134 tool
calls** (`studio.bundle_check` 1, `studio.uninstall` 1, `studio.open` 6,
`studio.inspect` 63, `studio.input` 57, `studio.close` 5, `studio.install` 1);
every call was answered, and the one refusal was expected: the harness now
begins by removing a previous developer install of the app, and there was
none. Checks passed: model bundle digest binding; preview interactions (empty
state, blank-title validation, three tasks added through native text input,
complete, All/Active/Done filters, reopen, controls at least 44 logical pixels
high); preview storage discarded on close and reopen; developer install with
the checked digest preserved; the installed app's interactions with storage
separate from the preview; close and reopen; persistence across a process
restart; the Chinese title `测试中文任务：买燕麦` entered natively and present
after reopen; native scrolling with eight extra rows; the long title. The
harness did not modify app source or app storage directly. The keyboard was observed
open and owned by the test package. The two fault checks (malformed
`tasks.json` preserved, save failure reported) were not run; they need
isolated fault injection.

Visual review of the captures (readability, clipping, overlap, contrast,
orientation, multilingual glyphs, keyboard): passed. The screens are readable
with no clipping or overlap, portrait, the CJK title wraps over two lines, the
completed row shows a filled disc, a "Done" caption, a muted title and a
"Reopen" control, and with the keyboard open the field stays visible and the
list scrolls. One observation belongs to the shell, not the app: the bold
capital "A" ("Add", "All", "Active") renders with a small artifact at its
baseline on this device.

![Empty state](evidence/xiaomi-empty.png)
![Installed app after reopen, with the Chinese title](evidence/xiaomi-multilingual.png)
![Keyboard open](evidence/xiaomi-keyboard.png)

## Ownership and revocation of a developer install

A second spool-driven check (17 tool calls, all answered) exercised the owner
record beside an installed app's data and the end of a developer grant.
Developer mode was provisioned twice, with a later `since` the second time, so
the first grant's `DevTag` no longer matched.

| Step | Result |
| --- | --- |
| Install, open the installed app, inspect it | ok |
| `studio.uninstall` while the instance is open | refused: "close the app's open instance first" |
| Close, reopen, close, `studio.uninstall` | ok (`uninstalled: true`) |
| `studio.open {app_id}` after the uninstall | refused: no developer install |
| Install again under the first grant | ok |
| Under the second grant, `studio.open {app_id}` of that install | refused: "developer grant expired" |
| Install again under the second grant, open, inspect, close, uninstall | ok |

Revocation of a *running* app was not driven on the device: a developer
profile's grant never expires, and only the Settings and banner gestures end
it; the unit tests cover that path (the app closes and its launcher row is removed).

## Not covered

Fault injection; landscape; an app-peer caller (the harness runs as the system
session); the ROM capture path; a release build of Home on a phone; revocation
of a running app on the device.
