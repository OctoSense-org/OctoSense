---
name: app-studio
description: App Studio: write, check, preview, test and install an offline Splash script app (manifest.json plus main.splash) on this device with the studio tools. Read before writing any main.splash.
---

# App Studio

App Studio runs a small app you write, on this device, while developer mode
is on. An app is a **bundle**: a folder in your workspace holding
`manifest.json` and `main.splash`.

`main.splash` is **Splash**, OctoSense's own script and layout language. It is
not Swift, SwiftUI, Kotlin, JavaScript or Lua: no `scene`, `vstack`,
`state {}` or `.padding()`. Only what `reference.md` lists exists; invent
nothing. Read `reference.md` first, in parts
(`read_file` with `start_line` and `limit`, about 300 lines), and start from
`template/main.splash` and `template/manifest.json`, a working notes app.

## A bundle

- `manifest.json`: `schema` 1; `id` `dev.studio.<name>` (lowercase, digits,
  `.` `-` `_`, at most 100 characters; the last segment is not a native app's
  name: `dev.studio.tip-calculator`, not `dev.studio.calculator`); `name` at
  most 64 characters; `version` `0.1.0`; `capabilities` `["storage"]` when
  the app saves anything, else `[]`. Nothing else is accepted; leave
  `integrity` out.
- `main.splash`: non-empty, at most 64 KiB, offline: no `://`,
  `http_resource(` or `{{assets}}`. At most 128 files, 2 MiB, 512 KiB each.
- Pass the **folder** as `bundle_path`, never a file.

## The loop

1. Write both files with `write_file`.
2. `studio.bundle_check` on the folder; fix what the reply names until it
   shows the app id.
3. `studio.open` with `bundle_path`: a disposable preview; keep its
   `instance_id`.
4. `studio.inspect`: look at `path` with `view_image`; pass a selector exactly
   as printed to `studio.input` (a label's text is not an id); follow
   `next_offset` with `offset`.
5. `studio.input` to tap, type or scroll, then inspect again. Check light and
   dark appearance.
6. `studio.close`; when it works, `studio.install`. `studio.uninstall` removes
   your install (close it first).

A design picture is optional: `studio.image` with a `prompt` only. Call again
with the returned `job` alone, only while the reply says `state: "running"`.

## Tools

- `studio.bundle_check {bundle_path}`: stages a private copy and admits it; the reply has the app id, or the first error.
- `studio.open {bundle_path}` or `{app_id}`: opens a preview, or an install with its own state; returns `instance_id`.
- `studio.inspect {instance_id, offset?}`: PNG `path`, a page of exact selectors, `next_offset`, `snapshot_path` (full JSON).
- `studio.input {instance_id, widget_id, action, text?, delta_y?}`: one real `tap`, `text` or `scroll` on a visible, enabled widget.
- `studio.close {instance_id}`: closes your instance.
- `studio.install {bundle_path}`: installs the bundle under its `dev.studio.*` id.
- `studio.uninstall {app_id}`: removes one of your installs, data included.
- `studio.image {prompt, size?, seed?, class?}` or `{job}`: one image file in your workspace, or a `job` to poll; spends the person's image budget.
- `studio.render {source_path, data_path?, dark?}`: an L0 glance card source to a PNG. Not for app bundles.

## Examples

1. Check, preview, test and install the folder `my-notes` (ids and selectors
   come from the replies):
   `studio.bundle_check {"bundle_path": "my-notes"}`,
   `studio.open {"bundle_path": "my-notes"}`,
   `studio.inspect {"instance_id": "inst-1"}`,
   `studio.input {"instance_id": "inst-1", "widget_id": "root.entry", "action": "text", "text": "Buy milk"}`,
   `studio.input {"instance_id": "inst-1", "widget_id": "root.add", "action": "tap"}`,
   `studio.close {"instance_id": "inst-1"}`,
   `studio.install {"bundle_path": "my-notes"}`.
2. `studio.image {"prompt": "A one-screen tip calculator for a phone", "size": "1024x1024", "class": "fast"}`;
   if the reply is `{"job": "j-7f3a", "state": "running"}`, call
   `studio.image {"job": "j-7f3a"}` until `state` is `done`.
3. `studio.uninstall {"app_id": "dev.studio.my-notes"}` removes an install.
