# ADR 0006: App Studio on the phone

- **Date:** 2026-10-02
- **Status:** Accepted (2026-10-03); amended 2026-10-10, amendment accepted 2026-10-12: the route through components is accepted and assigned (see the amendment at the end). Implementation in progress: a fresh model-authored app passed physical-device functional checks on a OnePlus 6 (3 October) and, after the rebase on `main`, on a stock Xiaomi ([re-validation](../studio/xiaomi-revalidation.md), 10 October); portrait visual review passed; fault injection and the full image/workflow pipeline remain pending.
- **Scope:** How an agent on the phone turns an image (a generated design or a screenshot of an existing app) into an OctoSense app or glance card, looks at its own result and improves it, entirely on the phone, at first in developer mode. Covers the inputs, the in-process renderer, the checks, the rules that can change without a build, the tools agents get, and what of the OctoScript App Design Flow moves to the phone. There is no compile in the loop and no Mac.
- **Relates to:** [ADR 0002](0002-event-driven-app-agents.md) (§6 the `card_render` and `card_critique_payload` toolbox tools; §7 a card is rendered, critiqued and revised before it is published; milestone M6); [ADR 0004](0004-native-apps-hosting-and-peers.md) (app agents, host tools, approvals, §13 developer mode); [ADR 0005](0005-app-contract.md) (the app contract and bundles); [Home ADR 0004](home/0004-system-apps-are-contained-script-apps.md) (contained script apps); [Home ADR 0005](home/0005-settings-octoscript-controller.md) and [Home ADR 0006](home/0006-builtin-settings.md) (Settings and its developer options); App Hub's [`card-studio`](https://github.com/OctoSense-org/OctoSense-App-Hub/tree/main/crates/card-studio) crate and [skill](https://github.com/OctoSense-org/OctoSense-App-Hub/tree/main/skills/card-studio); the [OctoScript App Design Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) (`flows/image-to-card`, `flows/image-lib`); octos issue #1149, closed, which added the `image_generation` stub (its backend needs a new issue).

## Implementation status

The implementation now covers L0 glance rendering and a local, offline script-app authoring path. A fresh Task Planner authored by **DeepSeek V4 Flash passed 129 instrument-driven tool calls on a physical OnePlus 6**. This establishes the functional path from a new design to a working local app. Portrait app and keyboard visual review also passed; fault injection remains pending. A fixture PNG, a passing unit test or a successful build alone would not establish that result.

The shell registers the following tools only while developer mode covers the calling system or app agent:

| Implemented tool | Current behavior |
| --- | --- |
| `studio.render` | L0 glance source/data → PNG, using the real glance width and height bounds. Arguments are relative `source_path`, optional `data_path` and `dark`. |
| `studio.bundle_check` | Copies a workspace-relative bundle into host-private staging, computes its digest and admits that copy. The author's source is unchanged. |
| `studio.open` | Opens a visible script app from either `bundle_path` (disposable preview) or `app_id` (local developer install with persistent state). |
| `studio.inspect` | Captures app pixels and scoped Makepad diagnostics. Returns PNG `path`, compact `snapshot.widgets`/check summary, and a full diagnostic JSON `snapshot_path`; optional `offset` follows `next_offset` pagination. |
| `studio.input` | Sends tap, text or scroll events to visible, enabled widget selectors within the caller's own instance. |
| `studio.close` | Closes that instance; preview state is discarded and installed state is retained. |
| `studio.install` | Records a local developer install under `dev.studio.*`, available from Home while its `DevTag` is valid. |
| `studio.uninstall` | Removes one of the caller's own developer installs: receipt, snapshot, app data and owner record. Refused while an instance of it is open. |

Inspection replies are bounded to 3,800 UTF-8 bytes so actionable selectors survive the pinned kernel's 4 KiB model-output limit. Pages retain exact selectors, report `total_widgets` and `next_offset`, and explicitly mark shortened text/value fields. Pass the returned selector to `studio.input`; a painted label is not an identity. Each page observes the current UI. The full original result is written as pretty-printed JSON (at most 1 MiB) in the caller's workspace, retaining all widget rectangles, geometry, tree and findings. `read_file` can read bounded line ranges through `snapshot_path`. Both artifacts are removed if completion fails, the grant expires or the workspace scope changes.

Tools are not shareable or available to background turns. The relay supplies the trusted caller and workspace: the system session's kernel-confirmed root, or the app peer's confirmed root and own conversation context. The model supplies relative input paths and instance ids, never another app's identity or an output directory. `studio.open`, `studio.input`, `studio.install` and `studio.uninstall` are `act` tools. A private install receipt binds the authoring app/account/session/context, developer activation, manifest and digest. Launch revalidates it; revocation closes the app and removes launcher availability. A second instance of the same installed app is refused to prevent concurrent state writes.

The first script-app path accepts `main.splash` with storage-only permissions: no app agent, accounts, `net` module or in-screen resource loader, and every `host.request` is refused. Admission also refuses a `main.splash` whose text names a URL or the `{{assets}}` route. That scan is a lint, since Splash builds strings at runtime; the offline guarantee is the isolate itself, which runs with no network hosts and no host capabilities, so a load from a URL is refused when it happens. An app's data folder carries an owner record beside it; another conversation that installs the same id starts with an empty folder. Since the ruling of 8 October 2026 the admitted instruction budget is a declaration; the storage jail, its quota and the memory cap are the enforced limits, and a preview fails closed on device consent. An original launcher icon may be present in the bundle; rich artwork routes are still future work. Staging permits 128 files/directories, eight directory levels, 2 MiB total, 512 KiB per file and 64 KiB for `main.splash`. The manifest's resolved ceilings are at most 1 MiB app storage, five million script instructions and a 16 MiB heap. These limits are installed before evaluation. Private preview writes are allowed for functional tests and discarded on close; developer installs write only their own persistent jail. No signed catalog or publisher identity is fabricated, and local admission does not count as App Hub publishing approval.

L0 glance rendering remains narrower: 16 KiB source, 32 KiB JSON data, zero storage quota, no chat or image resources. It reports `settled: true` after three matching pixel samples with shaders ready. Interactive app inspection returns `settled: false`, since a current frame does not prove an arbitrary script has stopped changing. PNGs are limited to 5 MiB. UI/render requests have a 20-second deadline inside a 25-second host wait and the normal 30-second kernel call deadline; cancellation, foreground state and the developer grant are checked through readback and encoding.

Android still has no HTTP remote instrument at the pinned Makepad revision. Studio calls Makepad's underlying widget instrument **in process**, rooted only at its own app; it does not inspect other apps or the shell's controls. Current checks report empty geometry, clipped text and small buttons. The shared `card-studio` check/critique port, image comparison, `mod.studio` toolbox adapter, editable workflow templates, image generation and capture sessions remain unimplemented.

Start at [host tools and admission](../../crates/shell/src/host_tools/studio_bundles.rs), then [the visible app runner](../../crates/shell/src/studio/apps.rs) and [the bilingual architecture walkthrough](../architecture-walkthrough.md#follow-app-studio-from-the-agent-to-a-working-app). The [fresh-app brief](../studio/task-planner-brief.md) and [functional device harness](../../tools/studio-flow-device-test.py) define acceptance against a newly authored project. The older [L0 device probe](../../tools/studio-device-probe.py) covers denial, palette/PNG behavior and background cancellation only. The Android wrapper's `--dev-mode` enables developer options; `--development` selects signing and `--package-name` selects an isolated test identity. The physical functional result is recorded below, separately from build and unit results.

### Physical functional evidence

The `adr0006-task-planner-acceptance-v2` run used the isolated `dev.makepad.octosense.studio` test package. Its generation receipt identifies `deepseek/deepseek-v4-flash` and binds the model-authored design, manifest, `main.splash` and icon to bundle digest `7892501e8b69d752b002130879dde11af8f7c7e7b7e620a6854b97892fc95f3b`. The model called `view_image`; its tool result reported `shown_to_model: true`, and the model continued its turn. This records image delivery and continuation, not an independent guarantee that every visual detail was understood.

The 129-call harness reported `functional_passed_visual_review_required`. It exercised task entry, completion and All/Active/Done filters; discarded preview state; verified separate installed storage; and retained installed state through close/reopen and process restart. It also verified exact Chinese text after native input and reopening, and scrolled through seven additional rows. The harness made no direct source or app-storage writes.

Subsequent manual review passed portrait readability, long-title wrapping, bottom-row reachability, Chinese glyphs, app contrast and absence of unintended app overlap. An Android-wide screenshot also confirmed keyboard presentation and correct viewport shrinkage; app-texture PNGs alone exclude keyboard pixels. Generous spacing leaves about one row above the keyboard. A shell floating overlay and low-contrast status bar remain separate observations; rotation/landscape was not established. See the [validation report](../studio/oneplus6-validation.md) for provenance and limits. Malformed saved JSON preservation and reported save failures were **not tested** because they require isolated fault injection. The full image-generation/refinement workflow and `mod.studio` adapter remain unimplemented.

The remaining decision describes the complete intended design. Some local app support from milestone 3 is implemented ahead of the shared checks and workflow adapter; the milestones as a whole are not complete.

## Context

At the time of the proposal, the image-driven app/card flow was provided by these desktop tools. The phone implementation above does not yet port that image pipeline:

- **App Flow** is about 12,150 lines of Python tooling (about 16,000 with its tests), plus per-app examples. It runs on a Mac with cargo, Makepad Studio and App Hub's `card-host --remote`, and also needs Python 3.12 with numpy, OpenCV, Pillow and fontTools, Swift and Node. Its two image paths are:
  - the Sketch kit (`flows/kits/sketch`), which needs `sketchtool`, Swift and a licensed kit, and has its own gates (`gate_structure`, `gate_composition`, `gate_fill`, `gate_visual`);
  - image-to-card (`flows/image-to-card`, `flows/image-lib`): crop scenes from an image, read their text with Apple Vision OCR (through Swift), map regions to widgets, generate L0, render and compare. Its gate is `flows/image-lib/gate.py`: native geometry, OCR text, ink and colour checks, and the visual-review receipt.
- **App Hub's `card-studio`** renders a card in a hidden `card-host --remote`, grabs a PNG, runs measured checks (hidden, clipped or truncated text, overflow, overlap, empty or failed states, fit, lint, realize) and builds a critique payload for a vision model. The checks and the payload are plain Rust that depends only on serde and serde_json. They read the remote instrument's snapshot and dump formats and card-host's widget ids, and the critique prompt is written for an L0 glance card. The rendering needs the remote instrument and a separate process.

The desktop process/HTTP pipeline cannot run unchanged on the phone:

- There is no Python, and no Apple Vision for OCR.
- Makepad's remote instrument is compiled out on Android (`platform/src/remote.rs`, the `target_os = "android"` stub), and there is no second process to host a card.
- Agents cannot reach a loopback port anyway: octos's `web_fetch` and MCP over HTTP refuse private addresses.

What the phone already has:

- **Offscreen rendering and pixel readback.** Makepad's GLES backend, which the Home APK uses, draws a pass into a texture and reads it back asynchronously (`Texture::read_back`, for `RenderBGRAu8` textures; on GLES the bytes come back as premultiplied RGBA with a top-left origin). The shell already uses that chain on Android for its `capture:`/`record:` test actions: it reads back the focused app's offscreen frame and encodes the PNG with makepad's own encoder (`Cx::encode_rgba_as_png`). Capturing the whole window, which AppCard's monitor relied on, is not implemented on Android's GLES backend.
- **The real lowering.** [`glance_card.rs`](../../crates/shell/src/glance_card.rs) lowers an L0 card the way the glance tile shows it (the shell's kit, theme fonts, multi-line fields, height `Fit`) and runs it in a Splash isolate under the app's policy. A published tile first passes `check_level`, `resolve_digests` and the chat seed, and light or dark comes from a process-wide flag. A card bundle lowers with its own `kit/` in App Hub's Card runner. App Hub's `card-studio` uses neither path exactly: its glance size is a provisional 350×160, while the tile is the page width minus 40 pt wide and 72–440 pt tall.
- **Agent tools.**
  - The system agent has `view_image` ([`system_tools.rs`](../../crates/kernel/src/system_tools.rs)).
  - App agents in developer mode get a fixed set of kernel tools, among them `write_file`, `edit_file`, `apply_patch` and `view_image`, plus `dev.run` and every app's shareable tools ([`relay.rs`](../../crates/shell/src/host_tools/relay.rs)). Developer mode also answers every approval itself ([`dev_mode.rs`](../../crates/shell/src/dev_mode.rs)).
  - Host-tool results are text only, so an image reaches the model only through a file in the agent's workspace and `view_image`.
- **Rules that change without a build.** The [system toolbox](../../crates/toolbox) runs Octoscript templates: built-in ones locked by digest, and editable copies under the app's folder that are re-checked on load and can never widen their modules or budgets.
- **Screen access on the OctoSense ROM.** The agent service ([`IAgentPlatform.aidl`](../../rom/vendor/octosense/agent/src/dev/makepad/octosense/agent/IAgentPlatform.aidl)) accepts only platform-signed OctoSense packages, Home among them, and can:
  - capture the screen (`captureScreen`); with the default capture arguments, windows an app marks secure are left out;
  - list tasks and return a task's last snapshot (`getTasks`, `getTaskSnapshot`);
  - start an app (`startActivity`, `startTask`);
  - inject input (`tap`, `swipe`, `typeText`, `pressKey`), which it refuses while the keyguard is showing. `captureScreen` and `startActivity` do not check the keyguard.
- **No image generation yet.** octos registers an `image_generation` tool but binds no backend, so every call fails with `no_backend_bound`. The tool is on no client's tool list: not octos's external clients', not the system agent's, not the developer app agents'.
- **Phone developer mode.** A development build (`cfg(dev_mode)`) honours it from Settings; an ordinary release build only with the `--dev-grant-all` launch flag, which a phone cannot pass. The Home APK still defaults to `--release` without the feature; the milestone 1 `--dev-mode` build option includes the existing feature while retaining release optimization. In a home with real accounts developer mode ends after 8 hours; only a home marked as a developer profile keeps it ([`dev_mode.rs`](../../crates/shell/src/dev_mode.rs)). A phone has one home.

For many people the phone is the only computer they have. An agent that can make and refine its own apps there should not need a desktop or a build/render server.

## Decision

### 1. An App Studio that runs on the phone, developer mode first

The authoring workspace and rendering loop live on the device:

1. **Take an image.**
2. **Draft** an app or card.
3. **Render** it the way its real surface will show it.
4. **Check** it.
5. **Compare** it with the image.
6. **Critique** it with the agent's own vision.
7. **Edit** it, then render again.

There is no compile in the loop and no desktop or build/render server is required. Generation and vision still call the model providers configured by the person. The first release is gated to developer mode (section 8). Normal mode follows once approvals cover screen capture and installs.

### 2. The input is an image; there is no Sketch path on the phone

An image comes from one of two places.

- **Generated designs.** The person, or the agent, asks for a design. Generation goes through a provider the person configured in AI providers, and the PNG lands in the studio project's folder. octos's `image_generation` tool gets this backend instead of a separate OctoSense tool, and joins the tool lists that need it (octos's external clients, the system agent, developer app agents), so every client sees one tool. The person types the image provider's key on AI providers' host sheet, and it is kept like every provider key, where the kernel reads it: in the profile's `env_vars` on a phone (app-private, mode 0600), behind a `keychain:` marker on a desktop. octos's backend resolves it from there as it resolves an LLM key, and no app ever sees it.
- **Screenshots of existing apps, to clone them.**
  - On every phone, the person picks screenshots with the system document picker, as AI providers' QR import does (one PNG or JPEG at a time today; picking several is new), or shares images to OctoSense (Home's share target accepts only text today; image shares are new).
  - On the OctoSense ROM, in developer mode and after the person approves a capture session, the studio drives the agent service. It opens the app (`startActivity`) and captures its screens (`captureScreen`); automated input has the additional guard below. A session is bound to one app and ends when another app comes to the front. It refuses to start or continue while the phone is locked and shows a stop control the person can use at any time. Coordinates alone cannot distinguish navigation from sending, purchasing or changing a setting. Autonomous `tap`/`swipe` stays deferred until a trusted host guard can identify the target action and require the person’s approval for actions that change external state; omitting `typeText` is insufficient. Until that guard exists, the person navigates the target app and the approved session only captures its screens.
  - The ROM capture transport must also change before these sessions ship: full PNGs must travel through a file descriptor or another bounded transfer, rather than an unbounded Binder byte-array reply. The host checks dimensions and byte limits, copies the image into the project folder, and closes or cancels the transfer on timeout or session end. This is future ROM/client work, not a milestone 1 dependency.
  - On stock Android, a capture session through MediaProjection, with Android's own consent prompt, comes later.

The Sketch kit, its gates included, stays a desktop tool and is not ported.

### 3. The output is a glance card or a contained script app

- **A glance card:** L0 only, as for every generated card, published through `glance.publish` as the app.
- **A script app:** a bundle per [ADR 0005](0005-app-contract.md), with `main.splash`, Octoscript controllers, its kit and its data. In developer mode it installs locally as a developer bundle. App Hub's ordinary install path remains tied to its signed catalog. Studio now adds a separate local path for offline, storage-only `main.splash` apps: a developer bundle carries developer mode's `DevTag`, runs only while that tag is valid, and is never offered through the catalog. Broader bundle support remains planned. Publishing goes through App Hub's usual gate and signing.

A cloned app is the person's own prototype. Section 9 says what may and may not be copied.

### 4. The shell renders in process, on the target surface's own path

A new renderer in the shell (`crates/shell/src/studio/`) renders a card or one screen of an app into an offscreen pass. It uses exactly the lowering and size of the surface it is meant for:

- the glance tile through the tile's whole path (`check_level`, `resolve_digests`, the chat seed, then `glance_card::lower`), at the tile's real width and height bounds, in light or dark mode. Lowering takes the mode as a parameter instead of reading the shell's process-wide dark flag, so a studio render never re-lowers live tiles;
- a bundle's screen through the bundle's own kit, as App Hub's Card runner lowers it, at the phone's window size.

It then:

1. waits until the render settles: no resource still loading for that isolate, every shader compiled (`is_draw_shader_window_ready`), the draw list quiet for a few frames after a repaint the renderer requests itself, and a hard timeout (an animated card reports `settled: false`);
2. reads the pixels back, one readback at a time (makepad's 32 MiB readback budget is shared by every pending readback), composes the premultiplied RGBA onto the surface's backdrop and writes an opaque PNG;
3. builds the widget-tree capture the checks read, in the capture format `card-studio` reads (section 5).

There is no HTTP remote server, `card-host` process or whole-window grab. The local app path uses Makepad's in-process widget instrument, scoped to its own root. Studio renders never go through the glance store, so its publish rate and card limits do not apply. Every preview uses a policy that confines side effects before source evaluation: a fresh disposable storage jail (or a bounded snapshot copied into it), no live app/account files, and disabled external network access. Bundle artwork is staged by the host through a bounded local asset path; service data comes from explicit preview fixtures. Mutating `host.request` calls are refused, but that gate alone is insufficient: Splash’s direct `fs.write`/`append`/`remove` and network APIs must remain confined or disabled too. A glance render discards its state on completion or cancellation. An interactive preview may write within its disposable jail for functional tests, then discards that state on close; an installed developer app keeps its own private state. The target’s layout and lowering are reused; its live account storage and service authority are not.

### 5. Checks and comparison are Rust, shared by phone and desktop

App Hub's `card-studio` becomes the one implementation of checks, comparison and the critique payload. Both the desktop `card-host` path and the phone's in-process renderer feed it.

- Its process-and-remote rendering goes behind a desktop feature.
- Its capture layer, which today parses the remote instrument's snapshot and dump and card-host's widget ids, becomes an interface both renderers fill.
- `realize_report` moves out of `card-host`, so both renderers write the same report.
- Check thresholds move into a JSON config, so they can change without a build.
- The critique payload gets a prompt for app screens beside the one for glance cards.
- **New checks** computed from the read-back pixels: text contrast per text rectangle, and empty or failed images.
- **Comparison with the source image:** layout bands, colour and fill, and crops of disputed regions. These are ported from image-to-card's `compare_screens` and image-lib's `gate.py` (native geometry, ink and colour), with bands sized for each surface instead of one 812×1552 page.
- **Text in the source image** needs OCR, which the phone lacks. Until an on-device OCR is chosen (open questions), text checks against the source image are skipped and the agent's own critique covers text.

### 6. The rules and the loop are Octoscript, editable without compiling

The system toolbox's runner becomes the studio runner.

- **Templates:** built-in templates are locked by digest. A project's copies are edited by the agent with `write_file` and re-checked on load. They cannot widen their modules or budgets.
- **A new `mod.studio` host module** gives templates:
  - files confined to the project folder;
  - `sha256` and the clock;
  - render, check, compare and critique-payload calls;
  - L0 check and realize as JSON;
  - SVG checks, image size, cropping and PNG encoding.

  Large results are written to files and passed back as paths, because Octoscript's JSON values are capped at 64 KiB.
- **What moves from App Flow to Octoscript** (rules and text, not pixels):
  - the role-first mapping policy (`semantics`, `core/policy`);
  - review records;
  - the image-to-card code generation (`flows/image-lib/compile.py`, `flows/image-lib/register.py` and `flows/image-to-card/extract.py`), without its desktop assumptions: fonts read from a `splash-makepad` checkout, artwork fetched from a local server, and the single 406×776 artboard;
  - repair plans.
- **What moves to Rust** (in `card-studio`):
  - pixel work, including `extract`'s crop and `core/policy`'s crop comparison;
  - the geometry, ink and colour checks of `image-lib/gate.py`;
  - the journaled multi-file commit of a repair (`core/repair`);
  - bundle export.
- **What stays on the desktop:** the Sketch kit's gates (`gate_structure`, `gate_composition`, `gate_fill`, `gate_visual`, and `visual_evidence`, which `gate_visual` uses).
- **Octoscript itself gains** `sort` and number formatting for scripts. L0 cards already format numbers with `format:`. Regex and file access stay out, as Octoscript's security model has them; the five simple patterns in the ported code become character checks.

### 7. Agents drive the loop through host tools

The complete design includes these host tools; the implementation table above identifies the current subset:

- `studio.render`, `studio.check`, `studio.compare`, `studio.critique_payload` and `studio.bundle_check`, declared `risk: read`;
- `studio.install`, for a developer bundle, declared `risk: act`, because it changes what is installed;
- the implemented interactive app path also has `studio.inspect` and `studio.close` (`read`), plus `studio.open`, `studio.input` and `studio.uninstall` (`act`), for stateful behavior tests within the caller's own app and for removing the caller's own installs.

None is a background tool, because a render needs Home in front (section 10). Each registers a call timeout that covers settle and readback; octos's default is 30 s.

Every tool writes its results into the calling context's folder and returns their paths. For an app agent in a conversation context that is `contexts/<id>/`, the only folder its `view_image` can read. The arguments name files in that folder (host-tool arguments are capped at 64 KiB), so iterations are `edit_file` calls on the project's sources.

The agent looks at renders with `view_image` and critiques with its own model:

- It views a render and its source in the same step, because a viewed image reaches the model for one request only.
- If an image does not reach the model, the loop stops and says so instead of critiquing blind. That happens when `view_image` returns `shown_to_model: false`, or when the provider refuses images, after which octos tells the model the image could not be shown.
- On the phone the model is DeepSeek V4 Flash. Its API takes images: on 2026-10-03 a PNG sent as an `image_url` part was described correctly, for about 200 prompt tokens at 64×64. Milestone 1 tests whether octos passes a viewed image through to it (`shown_to_model`) and whether its vision is good enough to critique UI.
- `model.complete` takes no images and is not used for critique.

Who gets the tools, in the first release only while developer mode is on:

- **The system agent:** through the same interception as `agents.*`.
- **App agents:** as a new developer-mode grant (section 8).

An octos change adds a media field to `peer/tool/result`, mapped onto octos's internal `ToolResult::model_media`, so a host-tool result can carry an image and the second call goes away. Until then `view_image` on the returned path is the route.

### 8. Developer mode for the studio

- **The developer build.** Home is built with `--features dev-mode`, through the `--dev-mode` option of [`build-home.py`](../../rom/scripts/build-home.py). Its existing `--development` option only picks the signing keystore; ordinary release builds keep developer options disabled. Standalone tests use a separate `--package-name`, such as `dev.makepad.octosense.studio`, without renaming or replacing the installed Bridge. Build receipts record both choices, ROM staging rejects developer-enabled receipts, and the build script refuses `--dev-mode` for the ROM variant, and for the installed Home's identity unless `--development` is given: a developer build is always a standalone APK, signed with the development keystore or built under a test package name.
- **The person's own home keeps its 8-hour limit.** The studio never marks a home that holds real accounts as a developer profile. A persistent developer profile on the phone needs a second home with test accounts, as ADR 0004 §13 intends; how the phone hosts one is open.
- **One new grant.** Developer mode gives app agents the `studio.*` tools.
- **Person-only approvals amend ADR 0004 §13.** Developer mode never answers a capture-session approval by itself. The person answers once per session because the session sees other apps' screens. If the future trusted input guard permits an autonomous action that changes external state, its action-specific approval must also come from the person; the capture-session approval does not authorize that action. Both answers go to the developer-mode audit log. Until that guard and approval route exist, the person operates the target app.

### 9. Privacy, safety and other people's work

- **Screenshots can hold personal data.** They stay in the project's folder and are sent only to the model provider the person chose. Deleting a project deletes its folder and purges the conversation sessions that viewed its images, because octos keeps viewed images as message media.
- **Other apps are captured only in an approved session** started by the person (section 2). A session is visible on screen, never runs in the background, and leaves secure windows out.
- **A clone is for the person's own use or as a prototype.** Studio projects never ship logos, brand marks, icons or copyrighted images in a bundle. The planned image workflow records where every image came from and adds a provenance gate refusing raster assets derived from captured screenshots. The current `studio.bundle_check` checks local developer admission and resource limits; it does not establish image provenance. The studio regenerates or draws artwork instead. Publishing a clone needs the publisher's own assets and goes through App Hub's review.
- **Generated cards stay L0** and script apps stay contained. The studio widens no policy.

### 10. Limits on the device

- **Home must have a drawable surface.** Makepad paints only while Home has a drawable surface, and without one a readback does not fail; it waits. The renderer watches for Home going to the background, cancels the readback and fails the render with `not_foreground`. Rendering in the background needs makepad changes, a surfaceless or pbuffer EGL context and its drawable-surface checks; that stays an open question.
- **Size and memory.** A glance tile reads back from under 1 MB to about 8 MB of RGBA, depending on its height and the screen's density. A full app screen reads back 10–18 MB, which reserves 19–25 MiB of the 32 MiB readback budget once rows are padded, so renders run one at a time. PNGs for `view_image` stay under 5 MiB and are scaled down if needed.
- **Vulkan.** On Android, readback is implemented for GLES only. A Vulkan build of Home would need Vulkan readback first.

## What changes where

| Where | Change | Size |
| --- | --- | --- |
| makepad | Nothing for the first release. Later: Vulkan readback; background rendering; optionally the remote instrument on Android, for debugging from a desktop | — |
| App Hub `card-studio` | Library split; a capture interface both renderers fill; shared `realize_report`; thresholds as JSON; an app-screen critique prompt; contrast, image and comparison checks | M |
| App Hub installs | A local developer-install path for `DevTag` bundles | S |
| OctoSense shell | `studio/` renderer and one router for texture readbacks; lowering that takes light or dark as a parameter; the host tools; capture sessions on the ROM and their approval kind; developer installs; image shares and picking several images | L |
| OctoSense toolbox | Generalised into the studio runner; `mod.studio`; the first templates | M |
| Octoscript | `sort`, number formatting for scripts | S |
| octos | An `image_generation` backend (a new issue; #1149 is closed); the tool on the external-client list; a media field on `peer/tool/result` | M |
| Phone packaging | The developer build (`--dev-mode`), standalone only: never the ROM variant, never the installed Home's identity under the release signer | S |
| App Flow | The Sketch kit and its gates stay on the desktop; image-to-card Python retires as each part lands on the phone | — |

## Milestones

1. **Render on the phone.** Renderer, readback router, `studio.render`, developer build. Acceptance requires device evidence for: pixels and orientation, settle timing, cost of the repaint, the readback when Home leaves the front, and an image reaching DeepSeek V4 Flash (`shown_to_model`).
2. **Check and compare.** `card-studio` as a library, the new checks, comparison with a screenshot, the OCR choice. End-to-end: a screenshot of an app becomes a glance card refined in three iterations.
3. **Rules in Octoscript.** The studio runner and `mod.studio`; the `card-refine` template; image-to-card code generation. Local offline script-app output, instrumentation and developer install now have a Rust path; the template/adapter port and fault injection remain pending. Fresh-app functional and portrait visual checks have passed on OnePlus 6.
4. **Images in.** The image-generation backend; capture sessions on the ROM; clone a multi-screen app from captured screens.
5. **Normal mode.** Approvals for capture and install; MediaProjection on stock Android; background rendering if it is solved.

## Consequences

- One implementation of checks and comparison for desktop and phone, so a card judged on a Mac and on a phone gets the same report.
- App Flow's Python shrinks to the Sketch kit and desktop-only tools as the image-to-card parts move to Octoscript and Rust.
- The shell gains a renderer that can show any card offscreen. It is a new attack surface for script cards, which is why studio renders run without side effects: preview effects stay in disposable storage with external services disabled.
- Rules and templates change on the phone without a build, under the toolbox's digest and budget rules.
- Developer mode gains the `studio.*` tools, with person-only approvals for capture sessions and any future guarded input that changes external state.
- ADR 0002 is amended (its amendment of 2026-10-03): its §6 toolbox tools `card_render` and `card_critique_payload` become the `studio.*` host tools, and its §7 rule that the phone evaluates only while charging does not apply to renders the person starts.

## Open questions

- Which image-generation providers and models to offer first, and what a generation costs.
- Whether DeepSeek V4 Flash's vision is good enough to critique UI, and which model requirement a studio project declares otherwise.
- Which OCR reads a source image's text on the phone.
- How the phone hosts a second home with test accounts, for a persistent developer profile.
- How App Hub treats a published app that began as a clone.
- Background rendering without a surface.
- The comparison thresholds that mean "close enough" to a screenshot.

## Amendment, 2026-10-10, accepted 2026-10-12 (components carry the pipeline; where the studio stands)

Two things changed after this ADR was accepted on 3 October, and a review of
the flow on 10 October listed what was still missing. On 12 October the
product owner accepted the route below and assigned its slices: the
amendments to sections 5 to 7 are the accepted architecture, and the slices
are assigned in order. The state table records 11 October, the day slice 1
landed (#477).

- **An app's Rust runs as WebAssembly components.** [ADR 0014](0014-app-components-in-webassembly.md)
  is accepted and merged (OctoSense #436, #451, #452, #454, #455). The `wasm`
  service loads a component from a bundle's `fns/`, keeps its instance between
  calls, gives it the app's storage folder with `storage`, HTTP to any host
  with `net`, and the app's host services through `octosense:host`. Shared
  components (App Hub ADR 0003, OctoSense #454) let apps pin reviewed
  components from the catalog. Desktop 0.1.0-rc.4 (10 October 2026) ships all
  of it; Home 0.1.0-beta.2 ships core modules only, and a component has run on
  a phone so far only in the isolated OnePlus 6 acceptance packages, not in a
  Home release.
- **Capabilities are declarations.** The per-app runtime gates are gone from
  the Makepad fork (makepad#117, makepad#118), a component's host call no
  longer checks the declared families (OctoSense #452), and OctoSense #457
  removed the shell's own declared-family checks: an admitted app calls a
  public host API whether or not its manifest lists the family, in Splash, in
  an agent tool and in a component. Identity, device consent, OS permissions,
  account scope, human write review, the storage jail and tool-sharing
  approvals still apply. Desktop 0.1.0-rc.4 carries this; earlier releases
  enforce the old gates.
- **The studio itself.** [OctoSense #313](https://github.com/OctoSense-org/OctoSense/pull/313),
  opened on 3 October, implements the renderer and tools of milestone 1 for
  developer builds: `studio.open`, `studio.render`, `studio.input`,
  `studio.inspect`, `studio.close`, `studio.bundle_check` and `studio.install`,
  verified on a OnePlus 6 with a Task Planner that DeepSeek V4 Flash wrote and
  previewed on the phone and that a test harness installed. It was rebased,
  reviewed and merged as [#477](https://github.com/OctoSense-org/OctoSense/pull/477)
  on 11 October and re-validated on a stock Xiaomi, with `studio.uninstall`
  added.
  Nothing else in this ADR has started: there is no image tool for agents, no
  measurement or comparison on the phone, and none of App Flow's image-to-card
  Python is ported; a developer build of Home exists through
  `build-home.py --dev-mode`, and no shipped APK carries it.

### The studio is a system app (accepted 12 October 2026)

The studio ships as a contained script app, `os.studio` (Home ADR 0004), with:

- a Splash UI: the brief, the design gallery, the review screen where the
  design and its map are shown together and corrected, the preview and
  comparison view, and publishing;
- its own agent, with the `studio.*` tools of section 7 and its components'
  functions as tools (the amendment to section 5);
- its own components in `fns/`, which carry the pixel work of section 5;
- its Octoscript rules and loop of section 6, in its bundle.

It uses services Home already has: `files.pick_photo` for reference images
(Home 0.1.0-beta.2 serves it, one image of up to 1 MiB; on `main`,
`files.import` takes 16 MiB on Android), `model.image` for generated designs,
the agent's own vision for critique, and the `github` service for publishing
once it can write (slice 5). Section 2's image
share and multi-image picker become optional: the picker already brings one
image in.

### Amendment to section 5: the pixel work ships as the studio's components

Section 5 chose Rust shared by phone and desktop and put it in App Hub's
`card-studio`, compiled into Home. The decision stands; the packaging
changes. The checks, the measurement and the comparison are Rust crates that
build two ways: natively into `card-studio` on the desktop, and with App Flow's
`tools/octo wasm build` into components in the studio's bundle on the phone.
One implementation, as section 5 wanted, and no shell rebuild to change a
threshold or a check: the studio updates like any system app.

What becomes a component, each a crate the desktop also links:

| Crate | Replaces | Notes |
| --- | --- | --- |
| intake | `atlas.py`, `prepare.py`, the crop checks | Image decoding with `image`; crops as files in the studio's storage folder |
| ocr | Apple Vision through Swift | `ocrs` with `rten`, pure Rust and already in the shell's lock file through the PDF engine's OCR, unused by anything else; the vision model is the fallback where it does not build or read well enough |
| measure | the OpenCV surface, colour and divider measurement | `imageproc` or hand-written passes; thresholds as JSON in the bundle |
| compare | `compare_screens`, `image-lib/gate.py` | Layout bands, colour and fill, crops of disputed regions, sized per surface |
| checks | `card-studio`'s measured checks | Contrast per text rectangle, empty or failed images, overflow and truncation from the capture |

How they run on the phone, within the `wasm` service's limits
([WebAssembly in OctoSense](../wasm.md)): one instance per component kept
between calls, 256 MiB of linear memory, 16 MiB out and 8 MiB of serialized
input per call, and 2 s a call (10 s for a component that imports
`wasi:http`). Images pass as files in the storage folder, which the
component sees as `/`, not as base64 in JSON: a 1 MiB `list<u8>` costs about
18 ms there and back, and a full screen readback is 10 to 18 MB. The studio's
script calls them as `wasm.<function>`, and its agent as tools with
`host_method: "wasm.<function>"`.

What stays shell code, because only the shell can do it: the renderer and
readback (section 4, #477), driving and inspecting a running draft, the
developer install, exposing the App Hub gate on a local folder, and the
`model.image` and image-share plumbing.

### Amendment to section 6: what the toolbox module still does

The rules and the loop stay Octoscript, in the studio's bundle, editable
without a build. `mod.studio` keeps its file access (still confined to the project folder),
`sha256` and the clock, the L0 check and realize, and the SVG, image-size,
cropping and PNG helpers; its render, check, compare and critique-payload
calls become section 7's tools and the components' functions. Large results
are still written to files and passed as paths.

### Amendment to section 7: the tools as built

OctoSense #477 (#313 rebased) ships `studio.open`, `studio.render`, `studio.input`,
`studio.inspect`, `studio.close`, `studio.bundle_check`, `studio.install` and `studio.uninstall`.
`studio.check`, `studio.compare` and `studio.critique_payload` are not host
tools: they are the studio's own functions, called through `wasm.<function>`.
The grant rules of section 8 are unchanged, and after #457 a tool call no
longer needs the family declared; `studio.*` tools remain developer-mode
tools because of what they do, not because of a declaration. A host-tool
reply is capped at 256 KiB of text, so an image a tool produces goes to the
workspace as a file, as section 7 already requires.

### Where things stand, 12 October 2026

| Step of the flow | State |
| --- | --- |
| Brief, and writing `main.splash` | Works: the system agent writes files into its workspace |
| Generated designs | Merged, no live call recorded yet: `studio.image` (slice 2) generates one image through `model.image` under the calling agent's identity and writes it into the agent's workspace; the system agent is admitted while covered by developer mode, and the host still sets the model (`gpt-image-1`, `gpt-image-1-mini` or MiniMax `image-01`); octos's own `image_generation` tool still has no backend (`no_backend_bound`) |
| Reference images in | `files.pick_photo` on Home 0.1.0-beta.2, one image, 1 MiB; no image share target; no capture of other apps |
| Measurement, OCR, mapping | Not ported; `ocrs` and `rten` are in the lock file and unused outside the PDF engine |
| Review screen | Not built |
| Render, drive, inspect, install | Merged in #477 on 11 October, developer builds only; `studio.uninstall` added |
| Checks and comparison | Not built anywhere on the phone; App Hub's checker needs a desktop |
| Gate on a local folder | The gate code is inside Home and not exposed as a tool |
| Publish | Creating the repository, committing and tagging can be done from a phone in GitHub's own web UI; the review evidence needs desktop App Hub tools |
| Developer mode | Not in the shipped Home APK; `build-home.py --dev-mode` builds a developer Home under a test package (#477), and release builds compile the studio out |

### The route (accepted and assigned, 12 October 2026)

Each slice is usable on its own.

1. **Render and install, developer builds.** Done on 11 October: #313 rebased,
   reviewed and merged as #477, `--dev-mode` in the Home build, and a developer
   APK re-validated on a stock Xiaomi. Gives brief → `main.splash` → preview →
   install on the phone.
2. **Inputs.** An agent tool that calls `model.image` and saves the PNG into
   the agent's workspace, with the provider the person chose and the model the
   host sets for it; which to use first is an open question (below). Merged in
   slice 2's PR as `studio.image`; no live call is recorded yet. Reference
   images through `files.pick_photo`.
3. **The pipeline as components.** The five crates above, developed on the
   desktop with OctoSense's `wasm_call` example (OctoSense #455) and App Flow's
   `tools/octo wasm call` (App Flow #184, merged), shipped in the studio's
   bundle; `card-studio` links the same crates. First tasks: run a component
   in Home on a phone, and build `rten` for `wasm32-wasip2`. If OCR does not
   build or read well enough, the vision model reads the text.
4. **Rules, loop and review screen.** The mapping rules and the fix loop as
   Octoscript in the bundle; the review screen in the studio's UI.
5. **Gate and publishing from the phone.** The gate as a tool on a local
   folder; repository, push, tag and submission through the `github` service
   once it can write, with the review evidence produced on the phone. App Hub's
   submission guide accepts that evidence.
6. **Normal mode.** As before: approvals for capture and install, and the
   policy decision on developer mode in release builds. Capture sessions on
   the ROM (section 2, original milestone 4) follow this slice; they are
   deferred, not dropped.

The smallest end-to-end path is slices 1 and 2 plus the vision model in place
of OCR and the compare component: brief → generated design → built app → preview →
comparison with the design → install, all on the phone.

### What a phone will not do

New Rust service code, new system apps, and building WebAssembly: the
studio's components are built on a desktop and ship with it, and an app made
in the studio is Splash on the services Home already has. A PDF tool built
with a new engine is still built on a desktop.

### What changes where

| Where | Change |
| --- | --- |
| App Hub `card-studio` | Becomes a set of crates that also build as components; the library split, the capture interface, `realize_report` and JSON thresholds as before |
| OctoSense shell | The renderer, host tools and developer install (#477); the `model.image` agent tool; the gate exposed on a local folder; no pipeline code |
| OctoSense studio app | New: `os.studio` with its UI, agent, components and Octoscript rules |
| App Flow | Image-to-card Python retires as each crate lands; the Sketch kit stays on the desktop; the SDK and `tools/octo wasm` are the build path for the components |
| Phone packaging | The developer build, as before |

### Open questions, added

- Whether `rten` builds and runs for `wasm32-wasip2` within 256 MiB and the 2 s call deadline, whether `ocrs` reads UI text well enough, and, if it runs but not within 2 s, whether the studio needs a longer deadline for its components.
- Which Home release ships components. A component has run in Home on a phone only in the isolated acceptance packages, and the first Home release with components is the first that can run the studio's pipeline.
- Which image model and provider first, and who approves the paid calls.
