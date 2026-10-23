# Task Planner: a fresh App Studio acceptance project

This is a design brief for the model authoring a new app. It contains no app
implementation. Passing a preview of an existing card does not satisfy it.
The acceptance run starts with an empty project in the test package, records
the model's tool calls and resulting bundle, and exercises that bundle through
Makepad's actual widget input path.

## Brief to give the authoring model

Create **Task Planner**, a small offline phone app for keeping a short personal
task list. Its app id is `dev.studio.task-planner`. Build a fresh contained Splash
app (`main.splash`), not an L0 glance card or a modified existing app example.
Use the installed Studio tool schemas exactly; do not invent tool names,
widget methods, syntax, host services or a successful tool result.

Design a calm, readable portrait screen with a warm neutral background, dark
text, one blue accent and generous spacing. Put the title and remaining-task
count at the top, a text field and Add button underneath, then All / Active /
Done filters and a scrollable task list. Use real editable and tappable
widgets. Show completion with both a readable label/state and a visual change,
not color alone. Keep controls at least 44 logical pixels high. The layout
must fit the current device, keep long titles readable and remain usable with
the keyboard open. Draw a simple original checklist icon as local vector
artwork. No generated raster design, borrowed screenshots, network artwork or
existing A2 app/card/assets are needed.

A new app opens with no tasks, `0 active`, All selected, and `No tasks yet`.
Typing a nonblank title and pressing Add creates one active task, clears the
field and updates the count. Blank input creates nothing and explains that a
title is required. Completion is reversible. All shows every task, Active
shows incomplete tasks, and Done shows completed tasks; a filter with no
matches shows a short empty-state message. Filters do not change task data.
Titles and completion survive closing and reopening the app, including a
restart of the test Home process. On reopen, select All. There is no login,
network request, AI feature in the resulting planner, or permission request.

The only app capability is `storage`. Keep task data in the app's own storage
jail, separate from the Studio project's source and the agent conversation.
Persist a versioned JSON document in `tasks.json`:

```json
{"schema":1,"next_id":1,"tasks":[]}
```

Each task is `{ "id": <positive integer>, "title": <string>, "completed":
<boolean> }`; increment `next_id` after insertion. The filter is session UI
state. Do not silently overwrite a malformed saved document: show a useful
storage error and preserve its bytes. Surface a failed save rather than
claiming that the change was persisted. Use only storage/error APIs supported
by the pinned runtime.

Give the input, Add button and filter controls stable source names:
`task_title`, `add_task`, `filter_all`, `filter_active`, `filter_done`.
Name the scrollable list `task_list`, and status and validation text
`task_count` and `task_error`. Task titles
must appear as native widget text. Each task has a visible `Complete` or
`Reopen` control associated with that task; the test can resolve its current
native widget id from inspection. These names are test selectors, not a
requirement to invent dynamic ids or custom runtime APIs.

Before implementation, write a short `DESIGN.md` outside `bundle/` stating the
screen layout, states, data ownership and supported runtime APIs. Then author
`bundle/manifest.json`, `bundle/main.splash` and `bundle/assets/icon.svg` from
scratch. Include other metadata only when the real local bundle checker
requires it. Do not invent publisher identity, signatures, public submission,
or platform claims. Keep source notes, logs, runtime state and test evidence
outside the bundle.

The local developer checker may compute integrity and stamp its own immutable
snapshot; it must leave the authored source unchanged. A developer check is
not publisher signing. Check the bundle, open it in Studio, inspect its real widget tree and image,
and drive its controls through Studio's actual input tool. Inspect after each
state change. `studio.inspect` returns a compact page in `snapshot.widgets`;
if `next_offset` is an integer, inspect the same instance with that `offset`
until it is null. Use the exact returned `selector` for `widget_id`; truncated
`text` or `value` is explicitly flagged and is not the complete field.
`snapshot_path` names the complete JSON inspection artifact relative to your
conversation workspace, and `path` names its PNG. The functional harness reads
the bounded full artifact so pagination cannot hide missing controls or
failed layout checks. The model should use selector pages for interaction;
reading a large artifact through a text tool is still subject to that tool's
output limit. Use `view_image` on the PNG for visual review.
Repair source errors or UX failures, recheck the new revision,
and reopen that revision before testing it. Preview state and installed app
state are separate; a preview does not prove installed-app persistence.
Leave final installation to the acceptance harness after your preview checks;
close the preview and return the final checked digest. The harness installs
into the developer test profile and exercises persistence through the same
Studio tools. Do not prepopulate installed task data. Never replace the
installed production Home or Bridge.

Report which checks ran and the final immutable bundle revision. Report
unavailable tools, errors and unfinished steps directly. A returned filename
or `settled: true` alone does not prove a readable or functional app. Leave
publication unsigned and local; the requested result is a working developer
app, not a public App Hub submission.

## Acceptance sequence

The machine-readable contract is
[`task-planner-acceptance.json`](../../tools/studio-flow/task-planner-acceptance.json).
Run it against the model's final bundle, not a separately hand-authored
replacement. Preserve the generation transcript and source revision with the
receipt; the functional harness cannot prove authorship by itself.

1. Inspect the empty screen and capture it. Check that the input, Add and three
   filters are visible, enabled where appropriate, and at least 44 logical
   pixels high. Blank Add must leave zero tasks and show validation.
2. Through native text input and button clicks, add `Buy oats`, `Call Sam`,
   and `Read ten pages`. Verify three task titles and `3 active`.
3. Complete `Call Sam`; verify `2 active`, All still contains all three,
   Active contains only Buy oats and Read ten pages, and Done only Call Sam.
4. Reopen Call Sam from Done; Done becomes empty and the count returns to
   three. Return to All, then complete Call Sam again for persistence testing.
5. Close and reopen the preview: its disposable storage must reset to the
   empty state. Developer-install the checked revision and open it by app id.
   Repeat the three-task sequence in the installed app, then close/reopen it
   and force-stop/relaunch the separate test Home package. Verify all three
   titles, Call Sam's completed state and `2 active` after each installed-app
   restart. Installed storage must not inherit preview test data.
6. Through native text input, add `测试中文任务：买燕麦`, reopen the installed
   app and verify the exact title and `3 active`. Capture the typed, added
   and reopened states for glyph review: correct widget text does not prove
   correct font rendering. Then add a long title, inspect wrapping and scroll
   to the final item. Verify
   typing remains possible with the keyboard open. Review actual native
   captures for clipping, overlap, contrast and top-to-bottom orientation.
7. In an isolated copy of the app data only, exercise malformed saved JSON and
   a save failure if the test API can create that condition. Preserve existing
   data and mark any unsupported fault-injection check unverified.

The overall result is incomplete if model authoring, actual input behavior,
installed-app persistence or visual review is missing. A developer bundle
check is not a full App Hub publishing approval.

## Existing flow and runtime boundaries

The procedure follows Design Flow's
[text-brief flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/0e59346e810ed694702b1df48f4283dc8104358c/flows/script-app/FLOW.md)
and [script API](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/0e59346e810ed694702b1df48f4283dc8104358c/docs/SCRIPT-API.md).
That revision pins the same OctoScript-Makepad release as this shell's
`native-runtime.lock.json`. Use the pinned implementation when a historical
API description disagrees with source.

Desktop tests use an owned hidden `card-host --remote` and real `/snap`,
`/click`, `/t` and `/g` operations. The current Android build has no HTTP
remote server. Its Studio integration must call Makepad's underlying widget
inspection and event dispatch on the UI thread; desktop captures do not count
as phone evidence. The earlier L0 `studio.render` probe verifies offscreen
rendering only.

The existing toolbox library runs declared `mod.research` methods. It does
not yet provide a `mod.studio` authoring adapter. This brief and test contract
are inputs to the Studio flow, not a claim that an executable OctoScript
workflow template has already been installed.
