# OctoSense system apps

English | [简体中文](README.zh-CN.md)

**New to the code?** Read the [desktop, Home, ROM and system-app walkthrough](../desktop/docs/code-walkthrough.md), then the [agent and Tokio walkthrough](../docs/architecture-walkthrough.md). The first follows launch, native hosting, script bundles, app data and Android platform boundaries.

> **Where this fits.** System apps run in App Hub's Card runner. News, Mail, Calendar, Photos, Maps, YouTube and Camera declare app agents; AI providers configures the host and declares none. The shell gives each enabled app/account its own peer and drives its conversations for the system agent, the “Ask <app>” panel and in-card chat. Declared tools pass through the shell's relay and approval router to a host service; the shell's shared notice service handles apps that only expose `<namespace>.notify`. See [App agents](#app-agents) for the exact tools and [architecture](../docs/architecture.md) for the two lanes and trust boundaries. Glance accepts L0 and Splash cards under the publishing app's policy.

The first-party apps that ship with [OctoSense](https://github.com/OctoSense-org),
the agent shell on top of your operating system, and the host services behind
them. They live in `apps/` of the [OctoSense repository](../README.md); until
2026-09-27 they were the OctoSense-System-Apps repository (archived).

- **News, Photos, Maps, Camera, Mail, Calendar, AI providers and YouTube** are *contained script apps*. Each is
  a Makepad Script/Splash program in a `bundle/`, run by App Hub's Card runner
  in its own isolate, under the permissions admitted from its `manifest.json`. That is the same containment a store app gets. They are also worked
  examples of the app shape any developer publishes through the App Hub.
- **Mail's host service** (`mail/host-service`) is the Rust half of Mail:
  IMAP/POP3/SMTP, the account store and the sign-in sheet, run by the shell.
  The app gets mail, never a password or a socket. It also runs Mail's
  agent's account-scoped read/sync tools and `mail.notify`/`mail.publish_card`, which publish Mail cards on the glance screen.
- **Calendar's host service** (`calendar/host-service`) keeps Calendar's
  events in the host's directory and runs Calendar's agent's tools:
  `calendar.events`, `add_event`, `remove_event`, and `notify` and `agenda`,
  which put an event or agenda card on the glance screen. Calendar ships in both shells
  (`desktop/system-apps.json` and `phone/system-apps.json`).
- **News's host service** (`news/host-service`) collects News's stories on a
  timer, with no model, and runs News's agent tools `news.list`, `news.read`
  and `news.notify` (the shell draws the notice).
- **The word engine service** (`word/host-service`, ADR 0013) is wordcraft's document engine behind typed `word.*` methods: document info, plain-text extraction, structure inspection, conversion between docx, md, html, rtf, odt, txt and pdf, and writing a minimal new document, all inside the caller's `word/` host area. No bundle yet.
- **The `llm` host service** (`ai-providers/host-service`) is the Rust half
  of AI providers: the assistant's LLM providers over octos's model catalog,
  keys in the platform secret store, Test connection, and moving providers
  between devices by a PIN-protected `OCTOS1E` QR (camera, image or paste).
  Keys are typed and QRs drawn only on the host's own sheets; the app sees
  masked status.
- **The `model` media APIs** use those host-held providers for bounded image,
  speech, embedding and asynchronous video requests. Apps declare `model` and
  exact host API versions; provider entitlement is checked when used. See the
  [media contract and validation limits](ai-providers/host-service/MEDIA.md).
- **The `deck` host service** (`deck/host-service`, ADR 0013) is the deckcraft presentation engine behind `deck.*`: decks inspected, read as outline text, rendered to PNG, created from titles and bullets, and converted (pptx, native, outline, PDF), confined to the host directory's own `deck/` corner; no bundle yet.
- **The `cad` engine service** (`cad/host-service`): the cadcraft drafting engine behind `cad.*` (ADR 0013, no bundle yet) — inspect, query, measure, render and convert DXF/DWG drawings, every path inside the host's own `cad` directory.
- **The `light` engine service** (`light/host-service`) is lightcraft's RAW
  develop engine behind `light.*` (ADR 0013): EXIF/XMP metadata, the develop
  control catalog, and single or batch parametric develop, for system apps
  only and contained to the `light/` area of the caller's host directory. No
  bundle yet.
- **The `sound` host service** (`sound/host-service`) is soundcraft's audio engine behind `sound.*` (ADR 0013), offline file processing only — info, convert, trim, mix and waveform peaks inside the host directory's `sound/` area; it never opens an audio or MIDI device, and no bundle exists yet.
- **The `design` host service** (`design/host-service`) is designcraft's page-layout engine (ADR 0013) behind `design.*`: document info, page renders to PNG and PDF/IDML/EPUB export, with every path inside its own `design/` area of the host directory; no bundle yet.
- **The `film` host service** (`film/host-service`) is the pinned filmcraft
  video engine (ADR 0013) behind `film.*`: probe, frame-to-PNG and bounded
  exports through its own pure-Rust codecs — headless, offline, no bundle yet.
- **The `effect` host service** (`effect/host-service`) is the effectcraft
  motion-graphics engine (ADR 0013) behind `effect.*` for system apps:
  project info, comp frames rendered to PNG, the engine's command catalog
  and Lottie import/export, every file under the caller's `effect/` area;
  no bundle yet.
- **AppCard** (`appcard`) is an optional native app: the "Ask anything"
  assistant, a Rust module (`octos-app`) that the shells link in-process and
  that runs on the shell's octos kernel. It is **opt-in**: both shells link
  it only with `--features app-appcard`, and it is not shipped by default.
- **Reference** (`reference`): a Rust module the shells link behind
  `app-reference` (always on phones). Every native app the shells link is
  declared in [`../native-apps.json`](../native-apps.json). News, Photos and
  Maps are script apps only: their earlier native modules were deleted
  (native-apps ADR 0004 §1, [#113](https://github.com/OctoSense-org/OctoSense/pull/113)). Photos' sample library, which Home mounts, is in
  `photos/resources/`.

The shell services these apps rely on are next door:
[`../crates/kernel`](../crates/kernel) (the octos kernel service, see
[The octos kernel](#the-octos-kernel)) and
[`../crates/app-peers`](../crates/app-peers) (apps' access to the assistant).

Rules for agents working here are in [AGENTS.md](AGENTS.md) and
[appcard/AGENTS.md](appcard/AGENTS.md), on top of the repository's
[AGENTS.md](../AGENTS.md).

**Building your own app?** You do not need to build or change this
repository. Start at the [OctoSense-org profile](https://github.com/OctoSense-org)'s
reading list (OctoScript-App-Design-Flow's `AGENTS.md`, then
`docs/QUICKSTART.md`), and read the bundles here as worked examples
(`apps/<name>/bundle/main.splash`). To run one next to your app, clone the
OctoSense repository into the same workspace and, from
OctoScript-App-Design-Flow:
`tools/octo run ../OctoSense/apps/photos/bundle --system --no-stamp --app-data /tmp/sys-apps`
(`--no-stamp` leaves the checkout unmodified; Mail needs a shell, see below).

## The apps

### Shared appearance

News, Photos, Mail, Calendar, Maps, AI providers and YouTube share the interface
in [`interface.splash`](interface.splash): theme-aware page/card/field surfaces,
readable secondary text, 44-point actions and 48-point inputs. Desktop content
has a maximum width; narrow windows keep the same actions. Camera and media
viewers retain a dark viewing surface. App Hub uses the same spacing and action
hierarchy in its native module. Expanded Glance cards and in-card chat use the
shared shell on both desktop and phone.

When a keyboard shrinks a focused hosted app, the shell reveals the active
editor through the app’s own scrolling container and keeps fixed actions in
place. Maps switches its tiles with the host appearance. Camera controls use
a dark scrim to remain readable over a bright preview; Android waits for the
first-use permission answer before preparing that preview. A permission dialog
pauses only a playing video; it cannot turn an unprepared camera into a fake
playing state when the app resumes. The preview uses its resolved on-screen
size for aspect-preserving fill, including `Fill` layouts and resized windows;
a rotated camera frame is cropped at the edges instead of stretched. AI providers’
Android QR scanner chooses a preview/YUV size advertised by the camera before
opening it; the preview keeps that aspect ratio, and callbacks from an earlier
scanner session cannot close a newly opened one.

After editing the interface, run `python3 tools/sync-app-interface.py` from the
repository root. It embeds the exact prelude in each contained bundle;
`python3 tools/sync-app-interface.py --check` detects drift in CI.
The prelude resolves the current host theme on restyle, without replacing
unsaved input. [Local native UX checks](../tools/app-ux/README.md) exercise
editing, persistence, appearance switching, browsing and search in isolated
profiles. Phone-sized desktop captures are not physical-device verification.

| App | Id | What it does | Capabilities (manifest) | Network hosts (manifest) | Host services |
| --- | --- | --- | --- | --- | --- |
| [News](news/bundle) | `os.news` | Hacker News, TechMeme and Google News feeds in tabs (Today, HN, TechMeme, Google, Saved), with a reader for stories | `storage`, `net`, `images`, `web`, `news`, `glance` | `hn.algolia.com`, `www.techmeme.com`, `news.google.com`, `api.gdeltproject.org`, `feeds.bbci.co.uk`, `feeds.npr.org`, `www.theguardian.com`, `feeds.arstechnica.com` | [`news`](news/host-service) |
| [Photos](photos/bundle) | `os.photos` | A sample library with AI-curated Memories, optional story prompts, saved stories and slideshows; moments, albums, people, favorites, a grid with selection, a full-screen viewer | `storage`, `glance`, `model` | none (the host calls the model) | `model.complete`; its own `photos` service: `photos.notify` through the shell's notice hook, `photos.info` on the photo engine (full-size photos use the asset mount) |
| [Maps](maps/bundle) | `os.maps` | `MapView` map, place search, places, routes with a changeable start and up to two stops, and a drive mode with turn-by-turn and a 2D/3D view; starts at the device's GPS fix when there is one; the search and route map draws makepad's pre-baked world map (`makepad.nl`), the drive maps still read OpenStreetMap through Overpass | `storage`, `net`, `location`, `glance` | `photon.komoot.io`, `router.project-osrm.org`, `overpass-api.de`, `overpass.kumi.systems`, `maps.mail.ru`, `overpass.openstreetmap.fr`, `makepad.nl` | `maps.notify` via the shell notice service |
| [Camera](camera/bundle) | `os.camera` (Home) | Photo and video over the runtime's `CameraPreview` widget, flash and zoom, a thumbnail of the last shot and a viewer | `storage`, `camera`, `microphone`, `library`, `glance` | none | `camera.notify` via the shell notice service |
| [Mail](mail/bundle) | `os.mail` | Accounts, folders, message list, reader (HTML rebuilt by the service) and composer; its agent puts notice cards on the glance screen (`mail.notify`) | `storage`, `mail`, `glance` | none (the service connects, not the app) | [`mail`](mail/host-service) |
| [AI providers](ai-providers/bundle) | `os.ai-providers` | The assistant's LLM providers: a primary and fallbacks, each with a model pull-down from octos's catalog and Test connection; an add wizard (family, model, route, key, test); Show QR for phone and import by camera, image or paste | `storage`, `llm` | none (the service connects, not the app) | [`llm`](ai-providers/host-service) |
| [YouTube](youtube/bundle) | `os.youtube` | YouTube search (the runtime's keyless `sys.video`, which reads YouTube's own results page), result rows with thumbnails and LIVE or length badges, topic chips, playback of YouTube's mobile watch page in `WebReader`, and a history of what was played on this device | `storage`, `net`, `glance` | `www.youtube.com`, `m.youtube.com`, `i.ytimg.com` | `youtube.notify` via the shell notice service |
| [Calendar](calendar/bundle) | `os.calendar` | Month/day calendar, event details and editor; app-owned event/agenda cards in Glance, with saved-event navigation | `calendar`, `glance` | none | [`calendar`](calendar/host-service) (Calendar-owned executor; granted cross-app tools) |
| [AppCard](appcard) | native, opt-in | The AppCard assistant: a routing brain picks or composes an app agent, which generates a live Splash or webview card. Shells link it only with `app-appcard`; not shipped by default | n/a (not a bundle) | n/a | the shell's octos kernel |

What each capability means is defined by the shared `octosense-app-contract` 1.x
crate (`KNOWN_CAPABILITIES` in App Hub's `crates/app-contract/src/manifest.rs`): `images` shows
pictures from any public https host, `web` opens a page in the system WebView,
`library` offers captures to the system photo library, `mail` reaches the
host's mail service, `llm` reaches the host's LLM-provider service, `news`
reads the host's news service, `glance` publishes cards to the glance
screen, and `model` requests bounded one-shot model completions. `net`
reaches only the hosts the manifest lists.

### Status and known gaps

- **YouTube**: on the OnePlus 6 (2026-09-27) search, results, playback and
  history worked; closing the player ends the page (makepad#43, in the
  runtime). Playback opens YouTube's mobile watch page, which autoplays muted
  and shows its own "Open App" prompt. Search reads YouTube's results page and
  depends on its layout.
- **Camera**: the Redmi Note 12 test package (2026-10-07) passed delayed
  first-use permission, front/back switching, Photo/Video preview selection and
  background resume. The stretched `Fill` preview is corrected; native GPU
  circle measurements pass, while a clear-scene visual comparison on the phone
  is still pending. The OnePlus 6 black preview reported on 2026-09-25 has not
  been retested. Still capture was not exercised in this run; Android video
  recording remains unimplemented.
- **AI providers camera import**: the Redmi Note 12 scanner opens with supported
  preview buffers and passed cancel, system Back, background interruption and
  reopening. QR decoding and importing a provider were not exercised in this run.
- **Photos Memories**: open **Memories → Create memories**, optionally entering
  a theme such as “summer with family.” The host's `model.complete` uses the
  provider configured in **Settings → AI providers** to curate up to three
  stories with titles, short narratives and ordered slideshows. It receives
  catalog metadata (dates, places, names, tags, titles and favorite flags),
  never image bytes or credentials. This release uses the sample catalog;
  it does not import the device library or analyze image pixels. Generation
  runs only when requested. The latest 12 stories are kept in
  alternating `accounts/device/memories.json` and `memories-backup.json`
  snapshots, separately from albums and favorites; a failed write leaves
  the previous snapshot recoverable.
  Local moments and saved stories work without AI. Errors, invalid photo IDs,
  missing providers and budget limits preserve saved stories. The model service
  allows 270 seconds for its provider attempts; Photos clears its loading state
  on timeout and allows retry, with a 300-second fallback if no callback arrives. **Stop waiting**
  discards a late reply; the host request may still finish and count toward
  its budget. A standalone `card-host` has no model service.
- **Photos images**: the bundle ships only 75 thumbnails (`bundle/thumbs/`, about
  2 MB). Its Home preview keeps three columns and fits one, two or three rows
  to the card's available width and height, with favorites first. Photos fill
  the card edge to edge; the title overlays the lower-left corner instead of
  reserving a header row. The shell's
  in-process card host calls the optional `on_app_resize(width, height)` hook
  in the app's own isolate when its slot changes size or the card reloads.
  It applies the queued UI updates before drawing the Home preview frame.
  The full-size files the viewer shows are served at
  `{{assets}}/photos/...` only when a shell mounts them: Home mounts
  `photos/resources/photos` (about 87 MB, `phone/system-apps.json`);
  the desktop mounts nothing (`desktop/system-apps.json`), so the viewer has
  no full-size image there. The viewer keeps a thumbnail fallback visible.
  Memories on physical phones and with a live AI provider are **unverified**.
- **Maps**: on the OnePlus 6 (2026-09-27) search, place, route, adding and
  removing a stop, driving with turn-by-turn and the 2D view worked. The 3D
  drive view draws the route but no map tiles, on the phone and on the
  desktop, before and after the stops change.
- **News**: runs in `card-host` during development, but not exercised
  end to end in the shell PRs' test runs (the test phone had no network).
- **Mail**: verified with the demo mailbox on desktop and on the OnePlus 6.
  Mail's and the `llm` host services use the App Hub revision selected by the root
  `Cargo.toml`, shared with the shells, so a build has one `octosense-appstore` and
  one host-service registry. Manifest and policy validation use the shared
  versioned `octosense-app-contract` crate.
- **Script bundle checks are partial.** [`apps.yml`](../.github/workflows/apps.yml)
  runs Photos' app-contract admission, Splash memory logic and model-schema checks as part of
  `cargo test --locked -p octosense-llm-service`, and News has script tests.
  There is no comprehensive automated UI check for every bundle.
- **AppCard `personal-data` skill** reads the old native Mail module's
  `mailbox-*.json` files. The script Mail app's mail now lives in the host
  service's own directory (`<host_dir>/mail/box-*.json`), so the skill
  probably no longer sees it; not verified.
- **Calendar** (2026-10-01, desktop, hidden `--remote` run with a live
  model): the system agent asked Calendar's agent for a card; the first-use
  sheet came up, the agent added an event and its card opened the glance
  panel. Not packed on the phone. Its host service's tests are not in
  `apps.yml` yet.
- Camera and AI providers ship PNG launcher artwork; YouTube ships SVG
  artwork. The shell frames bundle icons for the selected platform style.

## How the shells pack them

Both shells in this repository ship the system apps: the desktop
([`../desktop`](../desktop/README.md)) and Home ([`../phone`](../phone/README.md),
standalone launcher and ROM image). Each packaging:

1. Lists the apps in its `system-apps.json` (`desktop/system-apps.json`,
   `phone/system-apps.json`), found through `OCTOSENSE_SYSTEM_APPS`: the root
   `.cargo/config.toml` points at the desktop's, `phone/.cargo/config.toml`
   at the phone's (so run phone builds from `phone/`). App Hub's shell crate
   `octosense-app-hub-app` reads that file at build time, packs each
   `apps/<name>/bundle/` into the binary and fills in its digest. `assets`
   maps extra directories into an app's `{{assets}}` (Photos, on the phone):

   ```json
   {
     "schema": 1,
     "source": "../apps",
     "apps": ["news", "photos", "maps", "camera", "mail", "ai-providers"],
     "assets": { "photos": { "photos": "../apps/photos/resources/photos" } }
   }
   ```

2. Links the host services `octosense-mail-service`,
   `octosense-calendar-service`, `octosense-news-service`,
   `octosense-llm-service` and the engine services `octosense-sheets-service`
   and `octosense-photo-service` (workspace path dependencies) through the shell,
   [`crates/shell`](../crates/shell) (its `app-hub` feature), and registers them at startup (`crates/shell/src/apps.rs`, `register_host_services`): Mail with `register()` for real accounts, or `register_demo()`
   when the shell's app config has `mail_demo: true`. Mail, News and Photos install
   `on_notify` callbacks to the shell's common notice renderer; Calendar
   installs its card publisher. The shell registers `NoticeService` for
   remaining system-app namespaces; `llm` with the octos
   kernel's core dir and the shell's QR scanner and image picker (see
   [the `llm` service](#the-llm-service)). App Hub is pinned once, in the root
   `Cargo.toml`, so there is one host-service registry.
3. Starts the shell's AI services through one entry point,
   [`crates/ai-host`](../crates/ai-host/README.md) (`octosense-ai-host`): it
   links `octosense-kernel` from `../crates/kernel` (feature `octos-core` in
   both shells, on by default), configures the kernel at startup and
   registers the `llm` service (with its `octos-core` feature) on the
   kernel's core dir, so a provider change restarts the kernel. See
   [The octos kernel](#the-octos-kernel).
4. Optionally (opt-in `app-appcard`) links AppCard's `octos-app` with
   `default-features = false` and mounts it through its `AppShell` widget
   (see [AppCard](#the-appcard-assistant)); it connects to the same kernel.

There are no pins to move: a change here reaches both shells in the same pull
request.

## Layout

```
<name>/bundle/               a contained script app: manifest.json, main.splash, artwork
photos/resources/            Photos' sample library, which Home mounts
mail/host-service/           octosense-mail-service, the `mail` host service (Rust); notice callback to the shell
mail/docs/                   Mail's plans (the email action card)
calendar/host-service/       octosense-calendar-service, the `calendar` host service; resources/event.card, agenda.card
news/host-service/           octosense-news-service, the `news` host service (News's data service)
<name>/bundle/tools.json     app tools: News, Mail, Calendar, Photos, Maps, YouTube, Camera
../crates/shell/src/glance_notice.rs   shared notice service; ../crates/shell/resources/glance/notice.card
ai-providers/                the `llm` host service (host-service/) and octosense-llm-config (config/:
                             octos's model catalog and provider registry, the profile merge, OCTOS1/OCTOS1E QR)
reference/                   the reference module
appcard/                     the native AppCard assistant
  app/                       octos-app + store/transport/render crates (members of the root workspace)
  module/                    octosense-appcard: the shell module that mounts it
  a2app/                     Splash card memory (specs, widget patterns, lint rules), compiled in
  a2app-l0/                  L0 card framework, catalog and per-app exemplar cards, compiled in
  personal-data/             octos skill: read-only search over Mail and Calendar data
  vendor/                    vendored third-party crates (rustyline, mmap-rs; see NOTICE)
  tools/                     setup-native.py, octos macOS/OpenHarmony runners, build-android.sh, ...
  docs/                      architecture, build and review notes
  native-runtime.lock.json   the Octoscript-Makepad release AppCard builds against (the same as the root's)
../crates/shell/             octosense-shell: the one shell both packagings link
../crates/ai-host/           octosense-ai-host: the shell's AI services (kernel, `llm`, app peers), one entry point
../crates/kernel/            octosense-kernel: the shell's octos kernel (one per process, shared)
../crates/app-peers/         octosense-app-peers: the app-agent broker, one peer per (app, account)
../crates/l0-chat/           octosense-l0-chat: the host side of a card's in-card chat (sys.chat)
../.github/workflows/apps.yml   CI for the host services, AppCard and the shell services
```

## A system app bundle

```
apps/<name>/bundle/
  manifest.json     id, version, name, capabilities, network.hosts, integrity
  main.splash       the program
  icon.png|svg      optional launcher artwork; the shell owns the outer shape
  thumbs/ ...       any other files the app loads, as {{assets}}/<path>
```

`main.splash` refers to its own files through the `{{assets}}` placeholder,
which the runner replaces with the origin it serves the bundle from (Photos:
`let assets = "{{assets}}"`, then `assets + "/thumbs/" + id + ".jpg"`).

A system app has the same shape as a store app, with these differences:

| | System app (this repo) | Store app (App Hub) |
| --- | --- | --- |
| Id | `os.<name>`. `os.` is reserved: `hub check` refuses it and no device installs one from a store | any other id |
| Delivery | packed into the shell binary at build time from `system-apps.json` | downloaded from the signed catalog |
| Admission | by digest only (`HostLimits::system()`); the source manifest leaves `integrity.bundle_blake3` empty and the build fills it | digest plus publisher signature |
| Ceilings | `HostLimits::system()`: 64 MB storage, 128 MB memory, a larger instruction budget, since the app lives as long as it is open | `HostLimits::default()`: sized for a card |
| Extra files | a shell can mount directories into `{{assets}}` | only what is in the bundle |

Everything else is identical: the same isolate, the same capability checks,
the same network allowlist. How to write such an app (language, APIs, the
`octo` CLI) is in
[OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)
(`docs/QUICKSTART.md`, `docs/SCRIPT-API.md`).

## Launcher icon artwork

App identity belongs to the app; the outer shape belongs to the selected
shell style. Home, the dock, App Library, group previews and shell app badges
use the shared `octosense::style::AppIconDraw` renderer. This also covers apps
installed from App Hub. Do not render bundle images directly on these surfaces.

| Shell style | Outer shape |
| --- | --- |
| Android | Circle, matching OctoSense's existing Android icon set |
| macOS / OctoSense | Rounded square with the macOS icon set's inset |
| iOS | Rounded square with the iOS icon set's inset |
| Windows | Softly rounded tile |
| NextStep | Square tile |
| Omarchy / Windows 2000 | Keep the theme's freeform artwork convention |

Native Android package icons are already drawn by Android with its device
mask and user/profile badges. The shell preserves those pixels. Android does
not require a circle on every device: [adaptive icon masks vary by
OEM](https://source.android.com/docs/core/display/adaptive-icons).
[Apple's app icon guidance](https://developer.apple.com/design/human-interface-guidelines/app-icons)
uses rounded rectangles for iOS, iPadOS and macOS.

For new bundle icons, supply a square PNG or supported SVG with a full-bleed
background. Keep essential marks inside the central 66% of the canvas so
platform corners cannot cut them off. Do not bake a circle, rounded corners
or an outer shadow into the source. Transparent artwork is supported and
receives a neutral backing in tile-based styles. Keep the logo and brand
colours; adapting the frame should not redesign the identity.

Shell-owned SVG icons use `icon_frame::styled_svg` and an explicit background
element. The same frame policy masks bundle PNG/SVG artwork at draw time, so
switching styles updates installed icons too. Review Android, macOS and iOS
at launcher and badge sizes, on light and dark backgrounds. The shell's
icon-frame/style tests run in the Phone workflow.

For a visual regression check, build the `icon_shapes` example from `phone/`
with `mobile-apps`. Run it with an isolated `OCTOSENSE_HOME`, hidden windows
and the Makepad remote control surface (see the root `AGENTS.md`). It shows
all seven styles, bundled and store PNG/SVG artwork, transparent backgrounds,
small badges and half-opacity icons. Capture `/g?scale=1` and check the PNG
with `tools/check_icon_shape_preview.py` (optional dependency: Pillow).
The default capture offset is the macOS caption; use `--body-y` for another
backend. Native Android package icons must still be checked on a device.

## Running a bundle during development

App Hub's `card-host` runs one bundle under the policy its manifest resolves
to, with the same admission order a device uses. The `--system` and `--static`
flags, and host-service support, are on App Hub `main` (since
[OctoSense-App-Hub#4](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/4)).

```sh
# in an OctoSense-App-Hub checkout; <OctoSense> is a checkout of this repository
cargo build --release -p octosense-card-host --bin card-host

card-host --bundle <OctoSense>/apps/news/bundle --system
card-host --bundle <OctoSense>/apps/photos/bundle --system --static photos=<dir of full-size photos>
```

| Flag | Effect |
| --- | --- |
| `--bundle <dir>` | the bundle (default: current directory) |
| `--system` | admit as a system app: digest only, system ceilings; an empty digest is filled in memory |
| `--static <prefix>=<dir>` | serve `<dir>` at `{{assets}}/<prefix>/...`, as a shell serves a mounted directory |
| `--app-data <dir>` | where the app's storage jail is made (default `$TMPDIR/octosense-card-apps`) |
| `--allow-unsigned`, `--stamp` | for store bundles; not needed with `--system` |

The log line `card-host: <id> <version> admitted — capabilities …, hosts …`
shows what the app got; `card-host: refused: …` means nothing is drawn.

Set `MAKEPAD_REMOTE=<port>` to drive the window over localhost HTTP
(`/snap`, `/click?x=..&y=..`, `/g` for a screenshot, `/quit`); App Hub's
`docs/DEVELOPMENT.md` lists the routes.

**Mail** needs its host service, and `card-host` registers none. Run Mail in
a shell build that links the service, with the demo mailbox (any address,
password `demo`, sample messages, sends that go nowhere):

```sh
# the desktop, from the repository root
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release -p octosense
# Home in a phone-sized window, from phone/
MAKEPAD_APP_CONFIG='{"mail_demo":true}' cargo run --release -p octosense-home --features mobile-only
```

The demo keeps its password in a file, so no keychain prompt appears.

## Host services and sheets

Some work needs something a contained app must never hold: a socket, a
credential, a device. A **host service** does that work in the shell, in
Rust. The app calls it with `host.request("<family>.<method>", args, fn(r){…})`;
the isolate refuses the call unless the manifest grants the family (`mail`),
and the service answers with data, never the means. The runtime side lives in
App Hub (`crates/appstore/src/services.rs`).

When the person has to act (type a password, approve an account), the service
raises a **sheet**: a host-owned Splash surface drawn over the app, in its own
isolate under no app's policy. Calls from the sheet arrive marked
`from_sheet`.

**Secrets are the host's.** No app collects a password, PIN or one-time code:

- a password field in a contained app takes no input;
- methods that carry a secret live under `<family>.sheet.*`
  (`mail.sheet.submit`, `mail.sheet.cancel`) and are dispatched only when
  they come from the sheet, before any service sees them;
- only a service can open a sheet; an app cannot.

### The `mail` service

`octosense-mail-service` (`apps/mail/host-service/src/`):

| File | Role |
| --- | --- |
| `lib.rs` | the service: `mail.accounts`, `add_account` (raises the sign-in sheet), `remove_account`, `folders`, `sync`, `list`, `message`, `mark_read`, `send`, `notify` (the agent's notice: handed to the shell, which publishes its notice card as Mail, `on_notify`); `register()`, `register_demo()`, `register_with*()`; the `Transport` trait; account events for the shell (`on_account_event`) |
| `imap.rs` | IMAP client (folders, read flag back to the server) |
| `network.rs` | POP3 and SMTP, MIME decoding; credentials never appear in errors |
| `html.rs` | rebuilds a message as the few tags Mail's `Html` view draws, with nothing remote in it |
| `vault.rs` | where passwords go: macOS/iOS Keychain, Android (a file sealed with an Android Keystore key), owner-only file elsewhere, the files in the host's secrets folder `<home>/secrets/os.mail/`; `OCTOSENSE_MAIL_VAULT=file` forces the file store for unsigned dev builds |
| `contacts.rs` | the addresses the person's accounts sent mail to, for the approval rule "recipients in my contacts" (off until the person turns it on in Settings) |

Account metadata (no passwords) and fetched mail live under the host's own
directory (`<host_dir>/mail`), outside every app's jail. Each account is
granted only to the apps that added it. The service tests an account before
keeping it.

Mail’s Inbox offers **Reconnect account**, which opens the same host-owned
sign-in sheet. Enter the same account address, username and incoming-server settings to
update its credentials without removing its cached messages or saved drafts.
On Android, the encrypted password file is tied to the installed package’s
Keystore key: copying it from a test package to Home cannot restore sign-in.
Reconnect inside the destination package; never use Remove account as a
credential-reset workaround.

### The `calendar` service

`octosense-calendar-service` (`apps/calendar/host-service/src/lib.rs`) answers
as Calendar (`os.calendar`). Its executor owns the service call even when Mail
or the system agent is the caller. Both have explicit grants for the shareable
`calendar.events`, `calendar.add_event` and `calendar.notify` tools. The relay
checks the caller before routing. Calendar’s own UI separately requests the
`calendar` capability; the service checks `os.calendar` identity. Removal, update, UI
view and agenda are not included in these cross-app grants.

| Method | Args | Answer |
| --- | --- | --- |
| `calendar.view` (UI only) | `{month?, day?, direction?, take_focus?}` | Month grid, marked days, selected day’s events, and pending saved-event navigation |
| `calendar.update_event` (Calendar UI/agent only) | `{id, expected, title, start, end?, timezone?, location?, notes?}` | Saved event; stale expected snapshots are refused; existing event cards refresh quietly |
| `calendar.events` | `{from?, to?, limit?}` | `{events: [{id, title, start, end, location, notes, timezone?, request_id?}]}`, soonest first |
| `calendar.add_event` | `{title, start, end?, location?, notes?, timezone?, request_id?}` | `{id, start, reused?}` |
| `calendar.remove_event` | `{id}` | `{removed}` |
| `calendar.notify` | `{event}` or `{title, when, location?, notes?}`, and `{card_id?, priority?}` | `{card_id, replaced, expires_at}` once an event card (`resources/event.card`) is on the glance screen, with a notification |
| `calendar.agenda` | `{days?}` | the same, for the agenda card (`resources/agenda.card`): the next three events within `days` (7) |

Events live in `<host_dir>/calendar/events.json`, outside every app's jail.
The app’s month/day list and editor use this same store. A saved event’s card
keeps its id in `open.route = "event/<id>"`. **Open Calendar** sits inside the
card, below its date/time, and opens that event in the actual app. Its L0
`sys.link` action uses `app://calendar/event/<id>`; the host accepts only the
current publication’s declared own-app destination. Other URLs or routes do
not launch anything. `calendar/cards.json` records saved-event publications, their
original expiry and dismissals. Restart restores active cards without a new
notification, identical live notify retries reuse the card, and edits refresh
its data. Ad-hoc notices and agenda cards are not durable saved-event records.
An explicit IANA `timezone` retains the event wall time and shows its zone on
the card; omitted zones retain legacy device-local behavior. Ambiguous or
missing daylight-saving times are refused. Use RFC3339 offsets for precise
`from`/`to` filters. A stable `request_id` reuses an exact saved request; changed
fields with the same key are refused. Omit an unknown end time.

Mail schedules only on a human request or an explicit provisioned policy, reads
the calendar first, verifies the saved event and then publishes its Calendar
card. Cold Android Mail jobs register Calendar's service and load its granted
executor without opening Calendar or preparing another agent. These are local
events, not Google Calendar sync, invitations or scheduled reminder alarms.

Validation: [Mail → Calendar checks and phone evidence](../docs/testing/mail-calendar-2026-10-05.md).

### The `llm` service

`octosense-llm-service` (`apps/ai-providers/host-service`) is the Rust half
of AI providers. It keeps the octos kernel's LLM providers in the kernel's
profile, `<core_dir>/profiles/_main.json` (`octosense-llm-config` merges
`config.llm` and `config.env_vars`, keeping every other key), and the keys
where octos reads them: the macOS keychain `octos` service behind a
`keychain:` marker, `<core_dir>/secrets/` on desktop Linux, the app-private
profile itself elsewhere (Android, and HarmonyOS, whose embedded kernel cannot
read the secrets folder). Keys are typed, QRs shown and codes scanned only
on the host's sheets; the app sees masked status. Built with its `octos-core`
feature (the shells' default), it writes under
`octosense_kernel::core_dir()` and calls `octosense_kernel::restart()`
after every change, so the running kernel picks up the new providers. The
method table and registration are in its
[README](ai-providers/host-service/README.md).

**Talk to Octos** (off by default): **AI providers → Talk to Octos** turns on a loopback server so a web client or a terminal UI can talk to this device's assistant. While it is on, the kernel runs as `octos serve --host-managed` instead of `--stdio` and native apps keep working over its WebSocket; external clients get a separate token that opens the UI Protocol socket and nothing else. A web client pairs with a one-time code or the QR of its link; a terminal client of this user reads the private connection file. The server stays up when native apps close, until it is turned off or the shell exits. See [ADR 0003](../docs/adr/0003-shared-octos-client-access.md) and the [kernel guide](../crates/kernel/README.md).

## App agents

An app agent is the app's own octos peer, owned by the system agent: its own
workspace (the app's account folder, `apps/<id>/accounts/<account>/`), memory,
model lane and tools. Which system apps have one, and how
([`../crates/shell/src/apps.rs`](../crates/shell/src/apps.rs) `agent_apps`,
[`../crates/shell/src/host_tools/script_apps.rs`](../crates/shell/src/host_tools/script_apps.rs)):

| App | `manifest.json` | `tools.json` | Cards |
| --- | --- | --- | --- |
| News | `agent` block, `glance` | `news.list`, `news.read` (read, shareable), `news.notify` (act, background) | the shell's notice card |
| Mail | `agent` block, `glance`, `storage.accounts` (the agent acts for the signed-in account) | `mail.accounts`, `mail.folders`, `mail.sync`, `mail.list`, `mail.peek`, `mail.draft` (read); `mail.notify`, `mail.publish_card`, `mail.skip_event`, `mail.propose_reply`, `mail.suggest_reply`, `mail.propose_send` (act, background) | L0 card or the shell's notice card |
| Calendar | `agent` block, `glance` | `calendar.events` (read), `calendar.add_event` (act), `calendar.remove_event` (destructive, `confirm: host`), `calendar.notify`, `calendar.agenda` (act) | `event.card`, `agenda.card` |
| Photos | `agent` block, `glance` | `photos.notify` (act, background), `photos.info` (read: the photo engine inspects a file in its workspace, [ADR 0013](../docs/adr/0013-craft-engines-as-pinned-services.md)) | the shell's notice card |
| Maps, YouTube, Camera | `agent` block, `glance` | `maps.notify`, `youtube.notify`, `camera.notify` (act, background) | the shell's notice card |
| AI providers | none | none yet: App Hub takes a tool namespace only as `[a-z0-9_]` (and octos a tool name's segments only as `[a-z][a-z0-9_]`), so `ai-providers.notify` is refused | – |

**Mail card reply modes.** The system agent can provision automatic drafts for replyable important mail and Compose reply on request for automated/no-reply mail. The host adds Compose reply to informational incoming-email cards, verifies the original message and asks the Mail agent to create a draft. The same card becomes Email/Chat with saved editing and host review. See [Mail events](../docs/mail-agent-events.md).

**A service API is not automatically an agent tool.** Mail explicitly declares
account-scoped read/sync, publication, event-decision and draft/proposal tools.
`mail.peek` does not mark read; `mail.message` remains a UI API. The UI
`mail.send` path now prepares a host review, not an unapproved SMTP call. No
agent tool can approve or send. [Composed Mail cards](../docs/mail-composable-cards.md)
traces durable editing, contextual chat and the approval boundary: only a
physical press (a tap on Android or a click on macOS) sends, and synthetic
or remote input is refused. The macOS path and integrated paired-model phone
acceptance remain **unverified**. The peer's workspace does not mount Mail's
host database or credential vault. Calendar
is a working example of an agent reading/writing its app data through declared
Rust tools; its script window currently only explains how to ask the agent.
See the [data-access walkthrough](../desktop/docs/code-walkthrough.md#4-follow-a-tool-into-app-data-and-glance).

- **Declaring one.** The manifest's `agent` block names the kernel tools the
  agent may use (`"tools": ["ask_user_question"]`; a dotted name there asks
  for another app's shareable tool), and `bundle/tools.json` declares the
  app's own tools: `<app>.<tool>`, `input_schema`, `output_schema`, `risk`
  (`read`, `act`, `destructive`), `background`, `confirm` (`host` or
  `app`), `shareable` and `implemented_by: "host-service"`. App Hub admits
  and pins both. The `agent` block's `profile` and `model` are admitted but
  not used by the shell yet.
- **Running one.** Nothing runs before the person allows the agent on the
  first-use sheet (shown when the person opens the shell's "Ask <app>"
  panel, with the bar's "Ask <app>", Shift+F8 or the menu row "Ask this
  app's agent", or when the system agent asks it with `agents.ask`). Then
  the shell prepares the peer, so the system agent can reach it with
  `peer_send_input`. `agents.ask` waits for the person's answer and the
  peer (it is declared `outward` with `confirm: app`, so the kernel holds
  it as long as an approval, not a read tool's 30 s), then gives the system
  agent the peer's slug, so the request goes on in the same turn. Mail also
  supports opt-in `mail.messages.new` events configured by `agents.provision`: a
  durable queue starts incoming turns while OctoSense is alive. Inbox collection
  runs independently of those turns, and failed events retry individually so
  one failure cannot block all later mail. Successful host
  publication or explicit skip plus turn completion is required before ack.
  General app triggers/cron remain planned. See [Mail events](../docs/mail-agent-events.md).
- **Talking to it yourself.** The person can chat with the app's agent
  directly, not only through the system agent: in the "Ask <app>" panel,
  which the shell draws for every app with an agent (none of these apps
  draws a chat of its own). Those turns run in the person's lane, beside
  the system agent's, with the app's tools. For the separate `sys.chat`
  feature, see [in-card chat availability](../README.md#in-card-chat).
  The panel's Stop stops only the person's own turn. On the phone no touch
  control opens the panel yet. See the root
  [README](../README.md#talking-to-an-apps-agent-yourself).
- **Its tools run on the app's host service**, as the app, after the
  shell's relay checked the grant, the schema and the budget. destructive and outward calls (here `calendar.remove_event`) enter the
  approval router. It applies the person's standing rules, host or registered
  app confirmation sheets, and user-enabled developer mode; decisions are
  audited. See [Approvals](../docs/architecture.md#5-approvals).
- **Cards.** `<app>.notify {title, body, card_id?, priority?}` puts a
  notice on the glance screen, with a notification: one fixed L0 card for
  every app, which the shell ships
  ([`../crates/shell/resources/glance/notice.card`](../crates/shell/resources/glance/notice.card),
  filled by [`../crates/shell/src/glance_notice.rs`](../crates/shell/src/glance_notice.rs)),
  with the app's icon and name, the time, and the agent's title (at most 80
  characters) and text (at most 600); the same `card_id` replaces the app's
  earlier notice. Mail's, News's and Photos' services hand `notify` to the
  shell (Photos' `photos` service also answers `photos.info` on the photo
  engine); Maps, YouTube and Camera have no service of their own, so the
  shell's notice service answers it. `calendar.notify` and
  `calendar.agenda` fill Calendar's own event and agenda cards. Every card
  is published with `notify` through the shell's `glance` service as the
  app (the app needs the `glance` capability). These fixed-template tools take
  model-supplied text; `mail.publish_card` additionally takes model-authored L0
  source validated by the host.
- **Trying it** on the desktop: open the assistant (F8) and ask the system
  agent to have an app's agent (Mail, Calendar, News, Photos, Maps or
  YouTube) put a card on the glance screen;
  allow the agent on the sheet that comes up. Mail needs a signed-in
  account (the demo mailbox, below, will do). Mail's richer action card
  ([the plan](mail/docs/2026-10-01-email-action-card-plan.md)) is so far a
  demo with fake data: `OCTOSENSE_GLANCE_DEMO=mail` publishes it at
  startup. **Unverified recipe.**

## The octos kernel

The octos agent kernel is a **shell service**, not part of any app.
[`crates/kernel`](../crates/kernel) (`octosense-kernel`) is that
service; the shells link it by default (cargo feature `octos-core`, also on
in `mobile-apps` and native mobile builds):

- **One per process, on demand.** The first consumer's `connect()` starts it:
  `octos serve --stdio` as a child on desktop and Android (on Android the
  APK's bundled `liboctos.so`), the canonical core in-process on
  OpenHarmony. Later consumers share it; each gets only the replies to its
  own requests and its own sessions' notifications. It stops when the last
  consumer leaves.
- **Configured by AI providers.** The `llm` host service writes the kernel's
  profile, `<core_dir>/profiles/_main.json`, and keys (macOS keychain `octos`
  service behind `keychain:` markers, `<core_dir>/secrets/` on desktop Linux,
  the profile itself elsewhere, HarmonyOS included), then calls `restart()`: a running kernel stops,
  its consumers reconnect and a fresh kernel reads the new providers.
- **Consumers.** AppCard (opt-in) connects through its transport's `kernel`
  module; Rinx's native mini-app host can take its own connection the same
  way instead of sharing AppCard's.
- **The core dir.** The shell's choice, else `$OCTOS_APP_CORE_DIR`, else on a
  phone `<app data dir>/octos-home/.octos`, else `$HOME/octos-home/.octos`.
  On a desktop a kernel runs only when a binary is configured (the shell's,
  or `$OCTOS_APP_CORE_BIN`); without one the providers are still saved.

Tests, from the repository root: `cargo test --locked -p octosense-kernel`;
with a built `octos`,
`OCTOS_CORE_TEST_KERNEL=<octos> cargo test --locked -p octosense-kernel --test real_kernel` starts a real
kernel on a profile written by `octosense-llm-config` and restarts it after a
provider change. Details in [crates/kernel/README.md](../crates/kernel/README.md).

## The AppCard assistant

The "Ask anything" tile. You type a request; a routing brain (the AMA) picks
or composes an app agent; the agent generates a live card, Splash or
webview, that binds real data at render time. It talks to octos over the
octos UI Protocol v1.

- **Code**: `apps/appcard/app`, crates in the root workspace: `octos-app` (router,
  composer, multi-agent dispatch, Splash renderer and validator, L0 card
  generation, WebView overlay), `octos-app-store` (state reducer, no
  Makepad), `octos-app-transport` (the octos UI Protocol over the shell's
  kernel, a WebSocket or REST) and `octos-app-render` (streaming-markdown
  renderer).
- **octos**: every octos crate comes from git `octos-org/octos` at the one
  rev in the root `Cargo.toml` `[workspace.dependencies]` (see that file for the selected revision), shared with `crates/kernel` and the shells. AppCard starts no kernel of its
  own: it connects to the shell's ([The octos kernel](#the-octos-kernel)).
- **Makepad**: not vendored. Makepad, Octoscript and Octoscript-Makepad are
  the checkouts in `.sources/` at the repository root that `tools/setup.py`
  prepares, at the release `native-runtime.lock.json` selects; the root
  `.cargo/config.toml` sets `OCTOSENSE_WORKSPACE=.sources`, so AppCard's
  build embeds its framework assets from there.

Build and test from the repository root (details in [appcard/README.md](appcard/README.md)):

```sh
python3 tools/setup.py                               # prepare .sources/
(cd apps/appcard && PYTHONPATH=tools python3 -m unittest core.test_native_runtime)
cargo clippy --locked -p octos-app -p octos-app-store -p octos-app-transport -p octos-app-render --all-targets --no-deps -- -D warnings
cargo test --locked -p octos-app-transport -p octos-app-store
cargo run -p octos-app                               # standalone window (default feature `standalone`)
```

The standalone app reaches octos through `~/.config/octos-app/server.json`,
or `OCTOS_BASE_URL`/`OCTOS_BEARER`/`OCTOS_PROFILE_ID`, or a local core
binary via `OCTOS_APP_CORE_BIN` and `OCTOS_APP_CORE_DIR`
(`tools/octos-macos.py` sets these up; see `tools/OCTOS-MACOS.md`). Android
and OpenHarmony builds: `docs/BUILDING-ANDROID.md`,
`docs/BUILDING-OPENHARMONY.md`.

**How shells embed it.** A shell depends on `octos-app` with
`default-features = false` (no `fn main`), calls
`octos_app::register_script_mods(vm)`, and mounts `AppShell::create(vm)`: a
widget that owns the app and draws `OctosAppBody`, the app's root without
the standalone `Window`. `AppShell::ask` submits text as if typed. In both
shells this sits in an `AppCardModule` that implements the shell's
`AppModule` trait ([`appcard/module`](appcard/module), package
`octosense-appcard`).

**CI**: [.github/workflows/apps.yml](../.github/workflows/apps.yml) runs on
changes under `apps/`, `crates/`, the workspace files and `tools/setup.py`.
Its macOS job prepares `.sources/`, runs the Mail and `llm` host-service
tests, the AppCard runtime-lock tests, clippy for AppCard's four crates
(which compiles the whole app), AppCard's transport and store tests, and
checks the graph has one octos, one Makepad, one App Hub and one Rinx
source. Its Ubuntu job tests `crates/kernel`, `crates/app-peers` and
`octosense-llm-config`. `apps/appcard/app/.github/workflows/` is left over
from the original repository and does not run.

## Changing an app

1. Edit `apps/<name>/bundle/`. Use only APIs documented in
   OctoScript-App-Design-Flow's `docs/SCRIPT-API.md` or already used by
   another app here; check the runtime source before using anything else.
2. Ask only for what the app uses. A new network host goes in
   `network.hosts`; a new capability must exist in App Hub's
   `KNOWN_CAPABILITIES`.
3. Never add a password or code field. If the app needs a secret, a host
   service and its sheet handle it.
4. Run it with `card-host --system` (Mail: in a shell with the demo). Test on
   a phone through Home built as a separate test package, never by
   replacing the device's installed Home.
5. Open one pull request. The shells pack `apps/` directly, so there is no
   pin to bump.

A **new** system app is a new `apps/<name>/bundle/` with an `os.<name>` id,
plus an entry in each shell's `system-apps.json`.

## Testing

| What | How |
| --- | --- |
| Mail service | `cargo test --locked -p octosense-mail-service` from the repository root. The keychain test is ignored by default: `cargo test -p octosense-mail-service -- --ignored keychain` |
| Calendar and News services | `cargo test --locked -p octosense-calendar-service -p octosense-news-service` (Calendar's are not in `apps.yml` yet) |
| A card's in-card chat | `cargo test --locked -p octosense-l0-chat` |
| octos kernel service | `cargo test --locked -p octosense-kernel` (a stand-in kernel); `OCTOS_CORE_TEST_KERNEL=<octos> cargo test -p octosense-kernel --test real_kernel` (a real one) |
| `llm` service and config | `cargo test --locked -p octosense-llm-service -p octosense-llm-config`; add `--features octosense-llm-service/octos-core` for the shells' build |
| AppCard | the commands above |
| CI | all of the above except the real-kernel and keychain tests: [apps.yml](../.github/workflows/apps.yml) |
| Script bundles | by hand in `card-host` and in a shell, driven over `MAKEPAD_REMOTE`. No automated UI tests here yet |

## Related repositories

| Repository | Role |
| --- | --- |
| [OctoSense](../README.md) (this repository) | the shells that ship these apps: [`desktop/`](../desktop/README.md) and Home in [`phone/`](../phone/README.md) (standalone launcher or preinstalled by the [`rom/`](../rom/README.md) image); the shell services in `crates/` |
| [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) | catalog, gate (`hub stamp`, `check`, `scan`, `sign-manifest`, `publish`), `card-host`, the Card runner and host-service registry, and `octosense-app-hub-app`, the crate every shell links |
| [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) | how to design, build, check and publish an app |
| [OctoScript](https://github.com/OctoSense-org/OctoScript), [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad), [makepad](https://github.com/OctoSense-org/makepad) | the language and runtime |
| [Rinx](https://github.com/hagency-org/Rinx) | Matrix chats and mini apps, a native module; reaches the assistant through `crates/app-peers` |
| [octos](https://github.com/octos-org/octos) | the agent kernel: run as a shell service by `crates/kernel`, configured by AI providers, used by AppCard and other consumers (one revision selected by the root `Cargo.toml`) |

## Contributing

- Pull requests against `main`; never force-push `main`.
- Keep changes small and test them in a shell. Follow [AGENTS.md](AGENTS.md).
- Changes under `apps/` must pass `apps.yml`, and the shells' `desktop.yml` and `phone.yml`.

## History and license

This directory was the OctoSense-System-Apps repository until 2026-09-27,
imported here with its history. The bundles and the Mail service were first
written in OctoSense-mobile (archived) and OctoScript-App-Design-Flow (formerly Octoscript-AppCard),
where their history remains. AppCard came from
OctoSense-org/OctoSense-AppCard (`d0a836b8`), split from
OctoScript-App-Design-Flow's `app/` at `cbbda4da`.

Apache-2.0 ([LICENSE](LICENSE)). Third-party components are listed in
[NOTICE](NOTICE).
