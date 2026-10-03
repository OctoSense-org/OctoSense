# ADR 0006: App Studio on the phone: make apps and cards from an image, render, check and refine them on the device, with no compile and no Mac

- **Date:** 2026-10-02
- **Status:** Proposed
- **Scope:** How an agent on the phone turns an image (a generated design or a screenshot of an existing app) into an OctoSense app or glance card, looks at its own result and improves it, entirely on the phone, at first in developer mode. Covers the inputs, the in-process renderer, the checks, the rules that can change without a build, the tools agents get, and what of the OctoScript App Design Flow moves to the phone.
- **Relates to:** [ADR 0002](0002-event-driven-app-agents.md) (§7 a card is rendered, critiqued and revised before it is published; milestone M6); [ADR 0004](0004-native-apps-hosting-and-peers.md) (app agents, host tools, approvals, developer mode); [ADR 0005](0005-app-contract.md) (the app contract and bundles); [Home ADR 0004](home/0004-system-apps-are-contained-script-apps.md) (contained script apps); App Hub's [`card-studio`](https://github.com/OctoSense-org/OctoSense-App-Hub/tree/main/crates/card-studio) crate and [skill](https://github.com/OctoSense-org/OctoSense-App-Hub/tree/main/skills/card-studio); the [OctoScript App Design Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) (`flows/image-to-card`, `flows/image-lib`); octos issue #1149 (a backend for the `image_generation` tool).

## Context

Making an OctoSense app or card from a picture works today only on a desktop:

- **The OctoScript App Design Flow** is about 12,700 lines of Python tooling, plus tests and per-app examples. It runs on a Mac with cargo, Makepad Studio and App Hub's `card-host --remote`. Its two image paths are the Sketch kit (`flows/kits/sketch`, which needs `sketchtool`, Swift and a licensed kit) and image-to-card (`flows/image-to-card`, `flows/image-lib`): crop scenes from an image, map regions to widgets, generate L0, render and compare.
- **App Hub's `card-studio`** renders a card in a hidden `card-host --remote`, grabs a PNG, runs measured checks (hidden, clipped or truncated text, overflow, overlap, empty or failed states, fit, lint, realize) and builds a critique payload for a vision model. The checks and the payload are plain Rust with only serde as a dependency. The rendering needs the remote instrument and a separate process.

On the phone none of that runs:

- There is no Python.
- Makepad's remote instrument is compiled out on Android (`platform/src/remote.rs`, the `target_os = "android"` stub), and there is no second process to host a card.
- Agents cannot reach a loopback port anyway: octos's `web_fetch` and MCP over HTTP refuse private addresses.

What the phone already has:

- **Offscreen rendering and pixel readback.** Makepad's GLES backend, which the Home APK uses, draws a pass into a texture of any size and reads a `RenderBGRAu8` texture back asynchronously. The shell already uses that chain on Android for its `capture:`/`record:` test actions, encoding PNGs with the platform's built-in encoder. Capturing the whole window, which AppCard's monitor relied on, is not implemented on Android.
- **The real lowering.** [`glance_card.rs`](../../crates/shell/src/glance_card.rs) lowers an L0 card the way the glance tile shows it (the shell's kit, theme fonts, multi-line fields, height `Fit`) and runs it in a Splash isolate under the app's policy. A card bundle lowers with its own `kit/`. App Hub's `card-studio` uses neither path exactly: its glance size is a provisional 350×160, while the tile is the page width minus 40 pt wide and 72–440 pt tall.
- **Agent tools.**
  - The system agent has `view_image` ([`system_tools.rs`](../../crates/kernel/src/system_tools.rs)).
  - App agents in developer mode get `write_file`, `edit_file`, `apply_patch`, `view_image` and `dev.run` ([`relay.rs`](../../crates/shell/src/host_tools/relay.rs)).
  - Host-tool results are text only, so an image reaches the model only through a file in the agent's workspace and `view_image`.
- **Rules that change without a build.** The [system toolbox](../../crates/toolbox) runs Octoscript templates: built-in ones locked by digest, and editable copies under the app's folder that are re-checked on load and can never widen their modules or budgets.
- **Screen access on the OctoSense ROM.** The platform-signed agent service ([`IAgentPlatform.aidl`](../../rom/vendor/octosense/agent/src/dev/makepad/octosense/agent/IAgentPlatform.aidl)) can:
  - capture the screen (`captureScreen`);
  - list tasks and return a task's last snapshot (`getTasks`, `getTaskSnapshot`);
  - start an app (`startActivity`, `startTask`);
  - inject input (`tap`, `swipe`, `typeText`, `pressKey`), which it refuses while the keyguard is showing.
- **No image generation yet.** octos registers an `image_generation` tool but binds no backend, so every call returns "unsupported" (issue #1149).
- **Developer mode only in developer builds.** Developer mode exists but is honoured only by a development build (`cfg(dev_mode)`; [`dev_mode.rs`](../../crates/shell/src/dev_mode.rs)). The Home APK is built `--release` without the feature, so a phone cannot turn it on today.

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

- **Generated designs.** The person, or the agent, asks for a design. Generation goes through a provider the person configured in AI providers, with the key held by the host as for every provider. The PNG lands in the studio project's folder. octos's `image_generation` tool gets this backend (#1149) instead of a separate OctoSense tool, so every client sees one tool.
- **Screenshots of existing apps, to clone them.**
  - On every phone, the person picks screenshots from the gallery (the host's image picker, as AI providers' QR import uses it) or shares them to OctoSense.
  - On the OctoSense ROM, in developer mode and after the person approves a capture session, the studio drives the agent service. It opens the app (`startActivity`), walks its screens (`tap`, `swipe`) and captures each one (`captureScreen`).
  - On stock Android, a capture session through MediaProjection, with Android's own consent prompt, comes later.

The Sketch kit stays a desktop tool and is not ported.

### 3. The output is a glance card or a contained script app

- **A glance card:** L0 only, as for every generated card, published through `glance.publish` as the app.
- **A script app:** a bundle per [ADR 0005](0005-app-contract.md), with `main.splash`, Octoscript controllers, its kit and its data. In developer mode it installs locally as a stamped developer bundle. Publishing goes through App Hub's usual gate and signing.

A cloned app is the person's own prototype. Section 9 says what may and may not be copied.

### 4. The shell renders in process, on the target surface's own path

A new renderer in the shell (`crates/shell/src/studio/`) renders a card or one screen of an app into an offscreen pass. It uses exactly the lowering and size of the surface it is meant for:

- the glance tile through `glance_card::lower` at the tile's real width and height bounds, in light or dark mode;
- a bundle's screen through the bundle's own kit at the phone's window size.

It then:

1. waits until the render settles: no resource still loading for that isolate, the draw list quiet for a few frames, and a hard timeout (an animated card reports `settled: false`);
2. reads the pixels back, composes them onto the surface's backdrop and writes an opaque PNG;
3. builds the widget-tree capture the checks read.

There is no remote instrument, no `card-host` process and no window grab. Studio renders never go through the glance store, so its publish rate and card limits do not apply. Script cards render under a no-side-effect policy: a `host.request` that would change something is refused during a studio render.

### 5. Checks and comparison are Rust, shared by phone and desktop

App Hub's `card-studio` becomes the one implementation of checks, comparison and the critique payload. Both the desktop `card-host` path and the phone's in-process renderer feed it.

- Its process-and-remote rendering goes behind a desktop feature.
- `realize_report` moves out of `card-host`, so both renderers write the same report.
- Check thresholds move into a JSON config, so they can change without a build.
- **New checks** computed from the read-back pixels: text contrast per text rectangle, and empty or failed images.
- **Comparison with the source image:** layout bands, colour and fill, and crops of disputed regions. These are ported from the design flow's `compare_screens`, `gate_fill` and `visual_evidence`.

### 6. The rules and the loop are Octoscript, editable without compiling

The system toolbox's runner becomes the studio runner.

- **Templates:** built-in templates are locked by digest. A project's copies are edited by the agent with `write_file` and re-checked on load. They cannot widen their modules or budgets.
- **A new `mod.studio` host module** gives templates:
  - files confined to the project folder;
  - `sha256` and the clock;
  - render, check, compare and critique-payload calls;
  - L0 check and realize as JSON;
  - SVG checks and image size.

  Large results are written to files and passed back as paths, because Octoscript's JSON values are capped at 64 KiB.
- **What moves from the design flow to Octoscript** (rules and text, not pixels):
  - the role-first mapping policy (`semantics`, `policy`);
  - the composition gate and the visual-review gate (`gate_composition`, `gate_visual`);
  - review records;
  - the code generation of image-to-card (`compile`, `extract`, `register`);
  - repair plans.
- **What moves to Rust** (in `card-studio`):
  - pixel work;
  - the geometry comparison of `gate_structure`;
  - the journaled multi-file commit of a repair;
  - bundle export.
- **Octoscript itself gains** `sort` and number formatting. It deliberately has no regex, sorting or file access today; the studio does not need regex.

### 7. Agents drive the loop through host tools

Host tools on the shell, declared `risk: read` and `background: true`:

- `studio.render`
- `studio.check`
- `studio.compare`
- `studio.critique_payload`
- `studio.bundle_check`
- `studio.install`, for a developer bundle

Every tool writes its results into the calling agent's workspace and returns their paths. The arguments name files in that workspace (host-tool arguments are capped at 64 KiB), so iterations are `edit_file` calls on the project's sources.

The agent looks at renders with `view_image` and critiques with its own model. On the phone that model is DeepSeek V4 Flash, which takes images; this is to be verified on the device. `model.complete` takes no images and is not used for critique.

Who gets the tools:

- **The system agent:** through the same interception as `agents.*`.
- **App agents:** in developer mode first.

An octos change lets a host-tool result carry an image (`model_media`), which saves the second call. Until then `view_image` on the returned path is the route.

### 8. Developer mode is turned on in a developer build

- The phone gets a developer build: Home built with `--features dev-mode`, through a new option of [`build-home.py`](../../rom/scripts/build-home.py), which has none today.
- Settings gets a row that creates the persistent developer profile, so developer mode does not end after 8 hours.
- What developer mode grants is unchanged (ADR 0004).
- On top of that, the studio's capture sessions on the ROM ask the person once per session, even in developer mode, because they see other apps' screens.

### 9. Privacy, safety and other people's work

- **Screenshots can hold personal data.** They stay in the project's folder. They are sent only to the model provider the person chose, and they are deleted with the project.
- **Other apps are captured only in an approved session** started by the person. A capture session is visible on screen and never runs in the background.
- **A clone is for the person's own use or as a prototype.** Studio projects never copy logos, brand marks, icons or copyrighted images into a bundle. The studio regenerates or draws artwork instead. Publishing a clone needs the publisher's own assets and goes through App Hub's review.
- **Generated cards stay L0** and script apps stay contained. The studio widens no policy.

### 10. Limits on the device

- **The screen must be on.** Makepad paints only while Home has a drawable surface, so a render needs Home in front with the screen on, and otherwise fails fast with `not_foreground`. Rendering in the background needs a surfaceless or pbuffer EGL context; that stays an open question.
- **Size and memory.** A tile is about 2–3 MB of RGBA, well under the 32 MiB readback limit. PNGs for `view_image` stay under 5 MiB.
- **Vulkan.** Readback is implemented for GLES only. A Vulkan build of Home would need Vulkan readback first.

## What changes where

| Where | Change | Size |
| --- | --- | --- |
| makepad | Nothing for the first release. Later: Vulkan readback; optionally the remote instrument on Android, for debugging from a desktop | — |
| App Hub `card-studio` | Library split; shared `realize_report`; thresholds as JSON; contrast, image and comparison checks | M |
| OctoSense shell | `studio/` renderer and one router for texture readbacks; the host tools; capture sessions on the ROM; developer installs | L |
| OctoSense toolbox | Generalised into the studio runner; `mod.studio`; the first templates | M |
| Octoscript | `sort`, number formatting | S |
| octos | An `image_generation` backend (#1149); images in host-tool results | M |
| Phone packaging | The developer build and its Settings row | S |
| App Design Flow | Sketch stays on the desktop; image-to-card Python retires as each part lands on the phone | — |

## Milestones

1. **Render on the phone.** Renderer, readback router, `studio.render`, developer build. Verified on a device: pixels and orientation, settle timing, cost of the repaint, DeepSeek V4 Flash accepting the image.
2. **Check and compare.** `card-studio` as a library, the new checks, comparison with a screenshot. End-to-end: a screenshot of an app becomes a glance card refined in three iterations.
3. **Rules in Octoscript.** The studio runner and `mod.studio`; the `card-refine` template; image-to-card code generation; script-app output and developer install.
4. **Images in.** The image-generation backend; capture sessions on the ROM; clone a multi-screen app from captured screens.
5. **Normal mode.** Approvals for capture and install; MediaProjection on stock Android; background rendering if it is solved.

## Consequences

- One implementation of checks and comparison for desktop and phone, so a card judged on a Mac and on a phone gets the same report.
- The design flow's Python shrinks to the Sketch kit and desktop-only tools as the image-to-card parts move to Octoscript and Rust.
- The shell gains a renderer that can show any card offscreen. It is a new attack surface for script cards, which is why studio renders run without side effects.
- Rules and templates change on the phone without a build, under the toolbox's digest and budget rules.

## Open questions

- Which image-generation providers and models to offer first, and what a generation costs.
- Whether DeepSeek V4 Flash's vision is good enough to critique UI, and which model requirement a studio project declares otherwise.
- How App Hub treats a published app that began as a clone.
- Background rendering without a surface.
- The comparison thresholds that mean "close enough" to a screenshot.
