# ADR 0006: App Studio on the phone

- **Date:** 2026-10-02
- **Status:** Accepted (2026-10-03); amended 2026-10-10 (components carry the pipeline; where the studio stands; see the amendment at the end). The phone studio work is deferred by the product owner (2026-10-10); milestone 1's renderer and tools are in [#313](https://github.com/OctoSense-org/OctoSense/pull/313), unreviewed and unmerged.
- **Scope:** How an agent on the phone turns an image (a generated design or a screenshot of an existing app) into an OctoSense app or glance card, looks at its own result and improves it, entirely on the phone, at first in developer mode. Covers the inputs, the in-process renderer, the checks, the rules that can change without a build, the tools agents get, and what of OctoSense App Flow (formerly Design Flow) moves to the phone. There is no compile in the loop and no Mac.
- **Relates to:** [ADR 0002](0002-event-driven-app-agents.md) (§6 the `card_render` and `card_critique_payload` toolbox tools; §7 a card is rendered, critiqued and revised before it is published; milestone M6); [ADR 0004](0004-native-apps-hosting-and-peers.md) (app agents, host tools, approvals, §13 developer mode); [ADR 0005](0005-app-contract.md) (the app contract and bundles); [Home ADR 0004](home/0004-system-apps-are-contained-script-apps.md) (contained script apps); [Home ADR 0005](home/0005-settings-octoscript-controller.md) and [Home ADR 0006](home/0006-builtin-settings.md) (Settings and its developer options); App Hub's [`card-studio`](https://github.com/OctoSense-org/OctoSense-App-Hub/tree/main/crates/card-studio) crate and [skill](https://github.com/OctoSense-org/OctoSense-App-Hub/tree/main/skills/card-studio); [App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow) (`flows/image-to-card`, `flows/image-lib`); octos issue #1149, closed, which added the `image_generation` stub (its backend needs a new issue).

## Context

Making an OctoSense app or card from a picture works today only on a desktop:

- **App Flow** is about 12,150 lines of Python tooling (about 16,000 with its tests), plus per-app examples. It runs on a Mac with cargo, Makepad Studio and App Hub's `card-host --remote`, and also needs Python 3.12 with numpy, OpenCV, Pillow and fontTools, Swift and Node. Its two image paths are:
  - the Sketch kit (`flows/kits/sketch`), which needs `sketchtool`, Swift and a licensed kit, and has its own gates (`gate_structure`, `gate_composition`, `gate_fill`, `gate_visual`);
  - image-to-card (`flows/image-to-card`, `flows/image-lib`): crop scenes from an image, read their text with Apple Vision OCR (through Swift), map regions to widgets, generate L0, render and compare. Its gate is `flows/image-lib/gate.py`: native geometry, OCR text, ink and colour checks, and the visual-review receipt.
- **App Hub's `card-studio`** renders a card in a hidden `card-host --remote`, grabs a PNG, runs measured checks (hidden, clipped or truncated text, overflow, overlap, empty or failed states, fit, lint, realize) and builds a critique payload for a vision model. The checks and the payload are plain Rust that depends only on serde and serde_json. They read the remote instrument's snapshot and dump formats and card-host's widget ids, and the critique prompt is written for an L0 glance card. The rendering needs the remote instrument and a separate process.

On the phone none of that runs:

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
- **No developer mode on a phone today.** A development build (`cfg(dev_mode)`) honours it from Settings; a release build only with the `--dev-grant-all` launch flag, which a phone cannot pass. The Home APK is built `--release` without the feature. In a home with real accounts developer mode ends after 8 hours; only a home marked as a developer profile keeps it ([`dev_mode.rs`](../../crates/shell/src/dev_mode.rs)). A phone has one home.

For many people the phone is the only computer they have. An agent that can make and refine its own apps there should not need a Mac or a server.

## Decision

### 1. An App Studio that runs on the phone, developer mode first

The whole loop runs on the device:

1. **Take an image.**
2. **Draft** an app or card.
3. **Render** it the way its real surface will show it.
4. **Check** it.
5. **Compare** it with the image.
6. **Critique** it with the agent's own vision.
7. **Edit** it, then render again.

There is no compile in the loop and no Mac or server anywhere. The first release is gated to developer mode (section 8). Normal mode follows once approvals cover screen capture and installs.

### 2. The input is an image; there is no Sketch path on the phone

An image comes from one of two places.

- **Generated designs.** The person, or the agent, asks for a design. Generation goes through a provider the person configured in AI providers, and the PNG lands in the studio project's folder. octos's `image_generation` tool gets this backend instead of a separate OctoSense tool, and joins the tool lists that need it (octos's external clients, the system agent, developer app agents), so every client sees one tool. The person types the image provider's key on AI providers' host sheet, and it is kept like every provider key, where the kernel reads it: in the profile's `env_vars` on a phone (app-private, mode 0600), behind a `keychain:` marker on a desktop. octos's backend resolves it from there as it resolves an LLM key, and no app ever sees it.
- **Screenshots of existing apps, to clone them.**
  - On every phone, the person picks screenshots with the system document picker, as AI providers' QR import does (one PNG or JPEG at a time today; picking several is new), or shares images to OctoSense (Home's share target accepts only text today; image shares are new).
  - On the OctoSense ROM, in developer mode and after the person approves a capture session, the studio drives the agent service. It opens the app (`startActivity`), walks its screens (`tap`, `swipe`) and captures each one (`captureScreen`). A session is bound to one app and ends when another app comes to the front. It only navigates, never types (`typeText` is not used), refuses to start or continue while the phone is locked, and shows a stop control the person can use at any time.
  - On stock Android, a capture session through MediaProjection, with Android's own consent prompt, comes later.

The Sketch kit, its gates included, stays a desktop tool and is not ported.

### 3. The output is a glance card or a contained script app

- **A glance card:** L0 only, as for every generated card, published through `glance.publish` as the app.
- **A script app:** a bundle per [ADR 0005](0005-app-contract.md), with `main.splash`, Octoscript controllers, its kit and its data. In developer mode it installs locally as a developer bundle. App Hub installs only from the signed catalog today, so this adds a local install path: a developer bundle carries developer mode's `DevTag`, runs only while that tag is valid, and is never offered through the catalog. Publishing goes through App Hub's usual gate and signing.

A cloned app is the person's own prototype. Section 9 says what may and may not be copied.

### 4. The shell renders in process, on the target surface's own path

A new renderer in the shell (`crates/shell/src/studio/`) renders a card or one screen of an app into an offscreen pass. It uses exactly the lowering and size of the surface it is meant for:

- the glance tile through the tile's whole path (`check_level`, `resolve_digests`, the chat seed, then `glance_card::lower`), at the tile's real width and height bounds, in light or dark mode. Lowering takes the mode as a parameter instead of reading the shell's process-wide dark flag, so a studio render never re-lowers live tiles;
- a bundle's screen through the bundle's own kit, as App Hub's Card runner lowers it, at the phone's window size.

It then:

1. waits until the render settles: no resource still loading for that isolate, every shader compiled (`is_draw_shader_window_ready`), the draw list quiet for a few frames after a repaint the renderer requests itself, and a hard timeout (an animated card reports `settled: false`);
2. reads the pixels back, one readback at a time (makepad's 32 MiB readback budget is shared by every pending readback), composes the premultiplied RGBA onto the surface's backdrop and writes an opaque PNG;
3. builds the widget-tree capture the checks read, in the capture format `card-studio` reads (section 5).

There is no remote instrument, no `card-host` process and no window grab. Studio renders never go through the glance store, so its publish rate and card limits do not apply. Script cards render under a no-side-effect policy: a `host.request` that would change something is refused during a studio render.

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
  - the code generation of image-to-card (`compile`, `extract`, `register`), without its desktop assumptions: fonts read from a `splash-makepad` checkout, artwork fetched from a local server, and the single 406×776 artboard;
  - repair plans.
- **What moves to Rust** (in `card-studio`):
  - pixel work, including `extract`'s crop and `core/policy`'s crop comparison;
  - the geometry, ink and colour checks of `image-lib/gate.py`;
  - the journaled multi-file commit of a repair (`core/repair`);
  - bundle export.
- **What stays on the desktop:** the Sketch kit's gates (`gate_structure`, `gate_composition`, `gate_fill`, `gate_visual`, and `visual_evidence`, which `gate_visual` uses).
- **Octoscript itself gains** `sort` and number formatting for scripts. L0 cards already format numbers with `format:`. Regex and file access stay out, as Octoscript's security model has them; the five simple patterns in the ported code become character checks.

### 7. Agents drive the loop through host tools

Host tools on the shell:

- `studio.render`, `studio.check`, `studio.compare`, `studio.critique_payload` and `studio.bundle_check`, declared `risk: read`;
- `studio.install`, for a developer bundle, declared `risk: act`, because it changes what is installed.

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

- **The developer build.** Home is built with `--features dev-mode`, through a new `--dev-mode` option of [`build-home.py`](../../rom/scripts/build-home.py). Its existing `--development` option only picks the signing keystore. On the ROM the developer build is platform-signed, because the agent service accepts only platform-signed OctoSense packages.
- **The person's own home keeps its 8-hour limit.** The studio never marks a home that holds real accounts as a developer profile. A persistent developer profile on the phone needs a second home with test accounts, as ADR 0004 §13 intends; how the phone hosts one is open.
- **One new grant.** Developer mode gives app agents the `studio.*` tools.
- **One exception to ADR 0004 §13.** A capture session's approval is a new approval kind that developer mode never answers by itself. The person answers it once per session, because the session sees other apps' screens, and the answer goes to the developer-mode audit log.

### 9. Privacy, safety and other people's work

- **Screenshots can hold personal data.** They stay in the project's folder and are sent only to the model provider the person chose. Deleting a project deletes its folder and purges the conversation sessions that viewed its images, because octos keeps viewed images as message media.
- **Other apps are captured only in an approved session** started by the person (section 2). A session is visible on screen, never runs in the background, and leaves secure windows out.
- **A clone is for the person's own use or as a prototype.** Studio projects never ship logos, brand marks, icons or copyrighted images in a bundle. The studio records where every image came from, and `studio.bundle_check` refuses any raster asset derived from a captured screenshot. The studio regenerates or draws artwork instead. Publishing a clone needs the publisher's own assets and goes through App Hub's review.
- **Generated cards stay L0** and script apps stay contained. The studio widens no policy.

### 10. Limits on the device

- **The screen must be on.** Makepad paints only while Home has a drawable surface, and without one a readback does not fail; it waits. The renderer watches for Home going to the background, cancels the readback and fails the render with `not_foreground`. Rendering in the background needs makepad changes, a surfaceless or pbuffer EGL context and its drawable-surface checks; that stays an open question.
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
| Phone packaging | The developer build (`--dev-mode`), platform-signed on the ROM | S |
| App Flow | The Sketch kit and its gates stay on the desktop; image-to-card Python retires as each part lands on the phone | — |

## Milestones

1. **Render on the phone.** Renderer, readback router, `studio.render`, developer build. Verified on a device: pixels and orientation, settle timing, cost of the repaint, the readback when Home leaves the front, and an image reaching DeepSeek V4 Flash (`shown_to_model`).
2. **Check and compare.** `card-studio` as a library, the new checks, comparison with a screenshot, the OCR choice. End-to-end: a screenshot of an app becomes a glance card refined in three iterations.
3. **Rules in Octoscript.** The studio runner and `mod.studio`; the `card-refine` template; image-to-card code generation; script-app output and developer install.
4. **Images in.** The image-generation backend; capture sessions on the ROM; clone a multi-screen app from captured screens.
5. **Normal mode.** Approvals for capture and install; MediaProjection on stock Android; background rendering if it is solved.

## Consequences

- One implementation of checks and comparison for desktop and phone, so a card judged on a Mac and on a phone gets the same report.
- App Flow's Python shrinks to the Sketch kit and desktop-only tools as the image-to-card parts move to Octoscript and Rust.
- The shell gains a renderer that can show any card offscreen. It is a new attack surface for script cards, which is why studio renders run without side effects.
- Rules and templates change on the phone without a build, under the toolbox's digest and budget rules.
- Developer mode gains the `studio.*` tools and one approval it never answers by itself.
- ADR 0002 is amended (its amendment of 2026-10-03): its §6 toolbox tools `card_render` and `card_critique_payload` become the `studio.*` host tools, and its §7 rule that the phone evaluates only while charging does not apply to renders the person starts.

## Open questions

- Which image-generation providers and models to offer first, and what a generation costs.
- Whether DeepSeek V4 Flash's vision is good enough to critique UI, and which model requirement a studio project declares otherwise.
- Which OCR reads a source image's text on the phone.
- How the phone hosts a second home with test accounts, for a persistent developer profile.
- How App Hub treats a published app that began as a clone.
- Background rendering without a surface.
- The comparison thresholds that mean "close enough" to a screenshot.

## Amendment, 2026-10-10 (components carry the pipeline; where the studio stands)

Two things changed after this ADR was accepted on 3 October, a review of the
flow on 10 October listed what is still missing, and on the same day the
product owner deferred the phone studio work ("no phone app for now").
This amendment records the state and the route for when the work resumes.
Nothing in it is assigned.

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
  previewed on the phone and that a test harness installed. It is unreviewed,
  unmerged and conflicts with `main`. Nothing else in this ADR has started:
  there is no image tool for agents, no measurement or comparison on the
  phone, no developer build of Home in the shipped APK, and none of App Flow's
  image-to-card Python is ported.

### The studio is a system app

The studio ships as a contained script app, `os.studio` (Home ADR 0004), with:

- a Splash UI: the brief, the design gallery, the review screen where the
  design and its map are shown together and corrected, the preview and
  comparison view, and publishing;
- its own agent, with the `studio.*` tools of section 7 and its components'
  functions as tools (section 5, amended);
- its own components in `fns/`, which carry the pixel work of section 5;
- its Octoscript rules and loop of section 6, in its bundle.

It uses services Home already has: `files.pick_photo` for reference images
(Home 0.1.0-beta.2 serves it, one image of up to 1 MiB; on `main`,
`files.import` takes 16 MiB on Android), `model.image` for generated designs,
the agent's own vision for critique, and the `github` service for publishing
once it writes (slice 5). Section 2's image
share and multi-image picker become optional: the picker already brings one
image in.

### Section 5, amended: the pixel work ships as the studio's components

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
readback (section 4, #313), driving and inspecting a running draft, the
developer install, exposing the App Hub gate on a local folder, and the
`model.image` and image-share plumbing.

### Section 6, amended: what the toolbox module still does

The rules and the loop stay Octoscript, in the studio's bundle, editable
without a build. `mod.studio` keeps its file access confined to the project folder,
`sha256` and the clock, the L0 check and realize, and the SVG, image-size,
cropping and PNG helpers; its render, check, compare and critique-payload
calls become section 7's tools and the components' functions. Large results
are still written to files and passed as paths.

### Section 7, amended: the tools as built

OctoSense #313 ships `studio.open`, `studio.render`, `studio.input`,
`studio.inspect`, `studio.close`, `studio.bundle_check` and `studio.install`.
`studio.check`, `studio.compare` and `studio.critique_payload` are not host
tools: they are the studio's own functions, called through `wasm.<function>`.
The grant rules of section 8 are unchanged, and after #457 a tool call no
longer needs the family declared; `studio.*` tools remain developer-mode
tools because of what they do, not because of a declaration. A host-tool
reply is capped at 256 KiB of text, so an image a tool produces goes to the
workspace as a file, as section 7 already requires.

### Where things stand, 10 October 2026

| Step of the flow | State |
| --- | --- |
| Brief, and writing `main.splash` | Works: the system agent writes files into its workspace |
| Generated designs | Not available to the agent: octos's `image_generation` tool still has no backend (`no_backend_bound`); the `model.image` host method exists for script apps, with the model set by the host (`gpt-image-1`, `gpt-image-1-mini` or MiniMax `image-01`), and no live call is recorded |
| Reference images in | `files.pick_photo` on Home 0.1.0-beta.2, one image, 1 MiB; no image share target; no capture of other apps |
| Measurement, OCR, mapping | Not ported; `ocrs` and `rten` are in the lock file and unused outside the PDF engine |
| Review screen | Not built |
| Render, drive, inspect, install | Built in #313, developer builds only, unreviewed and unmerged |
| Checks and comparison | Not built anywhere on the phone; App Hub's checker needs a desktop |
| Gate on a local folder | The gate code is inside Home and not exposed as a tool |
| Publish | Creating the repository, committing and tagging can be done from a phone in GitHub's own web UI; the review evidence needs desktop App Hub tools |
| Developer mode | Not in the shipped Home APK, whose build script has no `--dev-mode` option; release builds answer `Developer mode needs a development build of OctoSense.` |

### The route when the work resumes (proposed, unassigned)

Each slice is usable on its own.

1. **Render and install, developer builds.** Rebase, review and merge #313;
   add `--dev-mode` to the Home build and produce a developer APK. Gives
   brief → `main.splash` → preview → install on the phone.
2. **Inputs.** An agent tool that calls `model.image` and saves the PNG into
   the agent's workspace, with the provider the person chose and the model the
   open questions leave to choose; the first live call. Reference images
   through `files.pick_photo`.
3. **The pipeline as components.** The five crates above, developed on the
   desktop with OctoSense's `wasm_call` example (OctoSense #455) and App Flow's
   `tools/octo wasm call` once App Flow #184 merges, shipped in the studio's
   bundle; `card-studio` links the same crates. First tasks: run a component
   in Home on a phone, and build `rten` for `wasm32-wasip2`. If OCR does not
   build or read well enough, the vision model reads the text.
4. **Rules, loop and review screen.** The mapping rules and the fix loop as
   Octoscript in the bundle; the review screen in the studio's UI.
5. **Gate and publishing from the phone.** The gate as a tool on a local
   folder; repository, push, tag and submission through the `github` service
   once it writes, with the review evidence produced on the phone. App Hub's
   submission guide accepts that evidence.
6. **Normal mode.** As before: approvals for capture and install, and the
   policy decision on developer mode in release builds. Capture sessions on
   the ROM (section 2, original milestone 4) follow this slice; they are
   deferred, not dropped.

The smallest end-to-end slice is 1, 2, the vision model in place of OCR and
the compare component: brief → generated design → built app → preview →
comparison with the design → install, all on the phone.

### What a phone will not do

New Rust service code, new system apps, and building WebAssembly: the
studio's components are built on a desktop and ship with it, and an app made
in the studio is Splash on the services Home already has. A PDF tool built
with a new engine is still built on a desktop.

### What changes where, amended

| Where | Change |
| --- | --- |
| App Hub `card-studio` | Becomes a set of crates that also build as components; the library split, the capture interface, `realize_report` and JSON thresholds as before |
| OctoSense shell | The renderer, host tools and developer install (#313); the `model.image` agent tool; the gate exposed on a local folder; no pipeline code |
| OctoSense studio app | New: `os.studio` with its UI, agent, components and Octoscript rules |
| App Flow | Image-to-card Python retires as each crate lands; the Sketch kit stays on the desktop; the SDK and `tools/octo wasm` are the build path for the components |
| Phone packaging | The developer build, as before |

### Open questions, added

- Does `rten` build and run for `wasm32-wasip2` within 256 MiB and the 2 s call deadline, and does `ocrs` read UI text well enough? If it runs but not within 2 s, does the studio need a longer deadline for its components?
- Which Home release ships components. A component has run in Home on a phone only in the isolated acceptance packages, and the first Home release with components is the first that can run the studio's pipeline.
- Which image model and provider first, and who approves the paid calls.
