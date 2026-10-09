# Working in OctoSense's system apps (`apps/`)

> **Any coding agent, or none.** These instructions work the same for Codex, Claude Code, Cursor, Gemini CLI, GitHub Copilot or a person at a terminal: every step is a shell command or a file edit, and nothing here needs a particular agent, model or vendor. `AGENTS.md` is the one source of truth; `CLAUDE.md` and `GEMINI.md` only import it for agents that look for those names.

> octos appears below only as a product dependency: the runtime of the apps' own agents and of the AppCard assistant. Changing or building the script apps and their host services does not need octos, and no step asks you to use octos as your coding agent.

These are shipping apps. Keep changes small, test them in a shell, and keep the
rules in README.md. The repository-wide rules in [../AGENTS.md](../AGENTS.md)
apply too; run every cargo command from the repository root (the root
workspace) after `python3 tools/setup.py`.

If you are building a new OctoSense app rather than changing these, you are in
the wrong place: follow the
[AGENTS.md](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/AGENTS.md)
of OctoSense App Flow (formerly Design Flow), and use the bundles here only as
read-only examples. For AI in an app (the
`octos.*` and `model` capabilities, why `llm` is for system apps only, an
app's own agent and `tools.json`, the system toolbox, `glance.publish` and
`sys.digest`, and which of these are available or still coming), read its
[AI in your app](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/main/docs/AI-SERVICES.md).

- An app is `apps/<name>/bundle/`: `manifest.json` + `main.splash` (+ artwork).
  Learn the language, the APIs and the development loop from
  [App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow)
  (`docs/QUICKSTART.md`, `docs/SCRIPT-API.md`). Do not invent APIs: if a
  widget or call is not documented there or used by another app here, check the
  runtime source before using it.
- Run a bundle on a desktop with App Hub's `card-host --bundle apps/<name>/bundle
  --system` (add `MAKEPAD_REMOTE=<port>` to drive it over HTTP; Photos also
  takes `--static photos=<dir>`). `--system` lets an `os.*` id and an empty
  digest through, as the shell does. These flags are on App Hub `main`.
  `card-host` registers no host services: run Mail in a shell with
  `MAKEPAD_APP_CONFIG='{"mail_demo":true}'`.
- Validate on a phone through Home (`phone/`) built as a separate test
  package; never replace the device's installed Home.
- Text on a page follows the host's appearance. On a phone, a page drawn by a
  plain `View` shows the host's light or dark background, not the app's own
  `ground` colour (**unverified** on the desktop), so a Label on it uses
  `theme.color_text`. Keep a fixed dark colour (the apps' `ink`) for text on a
  surface the app paints itself: a `SolidView` page like Mail's, a text field,
  a white card or the tab bar. News's title and then Photos' headings
  shipped in a fixed `ink` and vanished in dark mode, so check every page you
  change in dark mode as well as light.
- Mail's service: change `apps/mail/host-service` and run
  `cargo test --locked -p octosense-mail-service`. Calendar's and News's:
  `apps/calendar/host-service`, `apps/news/host-service`, and
  `cargo test --locked -p octosense-calendar-service -p octosense-news-service`.
  The sheet engine's (gridcraft behind `sheet.*`, ADR 0013, no bundle yet):
  `apps/sheets/host-service` and `cargo test --locked -p octosense-sheets-service`.
  The photo engine's (photocraft behind `photo.*`, same ADR, no bundle yet):
  `apps/photo/host-service` and `cargo test --locked -p octosense-photo-service`.
  The word engine's (wordcraft behind `word.*`, same ADR, no bundle yet):
  `apps/word/host-service` and `cargo test --locked -p octosense-word-service`.
  The deck engine's (deckcraft behind `deck.*`, same ADR; its app is Quick
  Deck, `apps/quickdeck/bundle`, desktop only, whose own `deck.*` calls work
  in its storage; its dev fixture is in apps/README.md):
  `apps/deck/host-service` and `cargo test --locked -p octosense-deck-service`.
  The cad engine's (cadcraft behind `cad.*`, same ADR, no bundle yet):
  `apps/cad/host-service` and `cargo test --locked -p octosense-cad-service`.
  The light engine's (lightcraft behind `light.*`, same ADR, no bundle yet):
  `apps/light/host-service` and `cargo test --locked -p octosense-light-service`.
  The sound engine's (soundcraft behind `sound.*`, same ADR, offline only,
  no bundle yet): `apps/sound/host-service` and
  `cargo test --locked -p octosense-sound-service`.
  The design engine's (designcraft behind `design.*`, same ADR, no bundle
  yet): `apps/design/host-service` and
  `cargo test --locked -p octosense-design-service`.
  The film engine's (filmcraft behind `film.*`, same ADR, offline only, no
  bundle yet): `apps/film/host-service` and
  `cargo test --locked -p octosense-film-service`.
  The effect engine's (effectcraft behind `effect.*`, same ADR, no bundle yet):
  `apps/effect/host-service` and `cargo test --locked -p octosense-effect-service`.
  The vector engine's (vectorcraft behind `vector.*`, same ADR, no bundle yet):
  `apps/vector/host-service` and `cargo test --locked -p octosense-vector-service`.
  The pdf engine's (pdfcraft behind `pdf.*`, same ADR, no bundle yet):
  `apps/pdf/host-service` and `cargo test --locked -p octosense-pdf-service`.
- Declare an app's agent in its manifest and `bundle/tools.json`. Keep the
  input/output schemas consistent with the executor (octos requires an object
  output schema), and select the actual risk, sharing and confirmation policy.
  Add the implementation before adding a tool declaration. A host-service tool
  needs its handler in the host service. An `implemented_by: "app"` tool needs
  the app's `app_tool(name, call_id)` hook and `"requires": ["script-tools-v1"]`
  in the manifest; it runs only while the app is open
  ([ADR 0012](../docs/adr/0012-app-host-api-discovery.md)).
- For a notification tool, follow `../crates/shell/src/glance_notice.rs`.
  Mail/News/Photos install `on_notify` callbacks; the shell's `NoticeService`
  serves Maps, YouTube and Camera. The fixed notice template lives in
  `../crates/shell/resources/glance/notice.card`; Calendar keeps its own event
  and agenda templates. Grant `glance` in the manifest and publish as the app.
- For richer app-owned cards, use `glance.publish` with either L0 `source` and
  optional `data`, or a Splash `script`. Preserve app attribution, policy and
  the distinction between app UI actions and agent tool calls. See
  [App agents](README.md#app-agents) and the shell's glance tests.
- There are no pins to bump: both shells pack `apps/` from the same commit
  (`desktop/system-apps.json`, `phone/system-apps.json`), so one pull request
  carries a change to every shell. App Hub, octos and the runtime are pinned
  once in the root `Cargo.toml`; the host services inherit them. Never give a
  crate here its own App Hub or octos source.
- AI providers: `apps/ai-providers/{config,host-service}` test with
  `cargo test --locked -p octosense-llm-config -p octosense-llm-service` (and
  `--features octosense-llm-service/octos-core`, the shells' build); AppCard
  links the config crate, so run AppCard's checks too when it changes.
- The octos kernel is a shell service, `../crates/kernel`
  (`octosense-kernel`): one kernel per process, started on the first
  `connect()`, shared by AppCard and other consumers, restarted by the `llm`
  service after a provider change. Test it with
  `cargo test --locked -p octosense-kernel` (and
  `OCTOS_CORE_TEST_KERNEL=<octos> cargo test -p octosense-kernel --test
  real_kernel` with a real kernel); AppCard and the `llm` service link it, so
  run their checks too. Consumers never spawn a kernel of their own.
- Apps reach the assistant through `../crates/app-peers`
  (`octosense-app-peers`): the shell gives each app with an agent ONE peer
  per account, owned by the system agent: a native app whose declared
  `octos.*` services host policy grants gets a scoped service injected at
  module creation; a script app gets `card.<app id>` from the shell's
  `octos` host service (`../crates/ai-host/src/contained.rs`). Apps never
  get raw kernel protocol.
  Test with `cargo test --locked -p octosense-app-peers --features octos-core,ws`
  (and `OCTOS_APP_PEERS_TEST_KERNEL=<octos> cargo test -p octosense-app-peers
  --features octos-core --test real_kernel`); see its README.
- Never add a password or one-time-code field to an app; secrets belong to a
  host service's sheet.
- Launcher artwork follows [the icon guidelines](README.md#launcher-icon-artwork):
  use a square canvas and keep essential marks inside the central safe area.
  The shell applies the active platform's shape to both PNG and SVG bundle
  icons. Do not bake rounded corners, circular masks or outer shadows into
  new artwork, or add an app-specific rendering path to bypass that policy.
  Review the icon in Android, macOS and iOS styles, on light and dark grounds.

## AppCard (apps/appcard)

AppCard is an opt-in native assistant: Rust crates, not a bundle. Reference
(`apps/reference`) is another native app; other native apps are linked from
external crates through `native-apps.json`. Its own rules
are in [appcard/AGENTS.md](appcard/AGENTS.md); in short:

- Its crates (`apps/appcard/app/app`, `apps/appcard/app/crates/*`,
  `apps/appcard/module`) are members of the root workspace. Makepad,
  Octoscript and Octoscript-Makepad come from `.sources/` at the repository
  root, prepared by `python3 tools/setup.py`; the root `.cargo/config.toml`
  sets `OCTOSENSE_WORKSPACE=.sources` for cargo. Never vendor them here.
- Build and test from the root: `cargo clippy --locked -p octos-app
  -p octos-app-store -p octos-app-transport -p octos-app-render --all-targets
  --no-deps -- -D warnings` and `cargo test --locked -p octos-app-transport
  -p octos-app-store`. If you touched `apps/appcard/tools/core` or
  `setup-native.py`, also run `PYTHONPATH=tools python3 -m unittest
  core.test_native_runtime` from `apps/appcard`.
- AppCard's own Python tools (`tools/setup-native.py`, `build-android.sh`,
  the octos runners) find the framework checkouts through
  `OCTOSENSE_WORKSPACE`; outside cargo its default is still the parent of
  the repository root, so set `OCTOSENSE_WORKSPACE=<repo>/.sources` when you
  run them (**unverified** after the move). Keep `tools/core/native_paths.py`,
  `build.rs` and the root `.cargo/config.toml` consistent if anything moves.
- Octos is one git source at one rev, in the root `Cargo.toml`
  `[workspace.dependencies]`; do not add an octos submodule or path
  dependency.
- CI for it is the `apps` job of `.github/workflows/apps.yml`, which runs on
  changes under `apps/`, `crates/` and the workspace files.

## Changing tools and data access

Trace each tool from `bundle/tools.json` through
`../crates/shell/src/host_tools/script_apps.rs` to its executor. Test schemas,
caller identity, approval behavior and results at that boundary. Keep UI API
methods separate from the tools actually declared for the agent: Mail exposes
account-scoped accounts/folders/sync/list/peek and notify/publish_card; its
credentials, send and mark-read APIs remain host/UI-only. News exposes list/read/notify; Photos notify and
info (its `photos` service: `photos.info` on the photo engine, ADR 0013); Maps, YouTube
and Camera expose notify only. AI providers declares no app agent. The native
Sheets app declares `sheets.*`, which the shell's engine executor
(`../crates/shell/src/host_tools/engines.rs`) runs on the sheet engine.

The ten craft engines (word, deck, cad, light, sound, design, film, effect,
vector, pdf; ADR 0013) declare their tools in `<family>/host-service/tools.json`,
which each crate's tests load with App Hub's own loader. No app ships them, so
the shell declares them under a virtual owner `os.<family>` (no bundle, no app
agent, no Settings row) and runs them on the engine's service as that system
identity. Only the system agent gets them: `ENGINE_TOOLS` in
`../crates/shell/src/system_chat/grants.rs` grants every read tool and each
act tool that only creates new files, never the command doors `vector.run`
and `effect.run` (held for their own review). No app's agent gets an engine
tool: none is shareable. An engine works in its caller's own folder (ADR
0013, 9 Oct 2026; `../crates/shell/src/host_tools/areas.rs` decides it):
for the system agent, its workspace, the folder its own file tools see, so
it hands an engine a file by placing it there and reads the result back.
Sheets' `sheets.*` and Photos' `photos.info` work in their own app's agent
folder. Every path is relative to that folder and kept inside it, links
resolved; an agent's write never replaces an existing file, and output into
an app's jail must fit what is left of its storage quota. These rules live
once in `../crates/engine-area` (`cargo test --locked -p
octosense-engine-area`); each service applies them, and fences the paths
written inside the documents it opens (design, vector, effect, film,
light). Change a tool's schema and the service together, keep both schemas
objects (octos takes no other), and add a new tool to `ENGINE_TOOLS` only
after reviewing it; the shell's `host_tools` tests run every granted tool
against its declared result.

Every engine (sheet and photo too) ships the system agent's skill for it in
`<family>/host-service/skill/`, embedded by `src/skill.rs`. `SKILL.md` is
written by hand: its `## Tools` must list exactly the engine tools the
system agent is granted, and its `## Examples` calls must match the tools'
schemas (the shell's `system_chat` tests check both, so a grant or schema
change fails until the skill follows). `commands.md` and `safety.json` are
generated from the pinned engine and `skill/safety-rules.json`, the
hand-written classification of every engine command (safe, file, code,
network, device, host), made from reading the command implementations;
regenerate with `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p
octosense-<family>-service --test skill` after an engine pin moves, and
review the classes of new ids before committing.

Use the [product walkthrough](../desktop/docs/code-walkthrough.md) for the data
and notice paths. For a cross-app tool, update the owner's shareable declaration,
requesting app's grant and App Hub admission offer together. Keep credentials in
the host service; expose business data through a narrow method or tool.
For shipped bundles, register the per-app host offer with
`octosense_appstore::system::set_agent_tool_offer` before `system::prepare`.
`AgentBundle::load` alone does not verify the host's admission offer. Test a
cold process: Mail must load Calendar's granted executor and register its host
service without opening Calendar or preparing its peer. Calendar scheduling
uses an explicit event timezone and a stable retry key; unknown end times stay
omitted. A human request or provisioned scheduling policy supplies intent,
never instructions inside the email itself.

Update both README languages when declarations, storage or runtime support
change. Mail's opt-in dispatcher is `crates/shell/src/agent_events.rs`: an
initial baseline, durable pending events, incoming-trigger turns, successful-turn
acknowledgment and bounded retries. News's fetch timer still collects data
without starting an LLM turn. Host-provisioned skill text is not kernel-native
skill installation; never claim the general ADR 0002 scheduler is complete.
Android's Mail-only JobService adapter is in `phone/src/android_mail.rs` and
`phone/resources/android/java/dev/makepad/octosense/MailJobService.java`;
`runtime_host` initializes the same host once, and `mail_background` owns bounded
execution leases and account-scoped notification restoration. A Rust worker
thread alone is not Android background execution. Test a cold process and a
stopped job, distinguish forced from natural scheduling, and preserve physical
send approval. See ADR 0008.

Calendar UI acceptance uses the same `.host/calendar/events.json` as the tools.
Exercise month/day markers, a saved-event card’s `event/<id>` navigation, direct
editing with stale-snapshot refusal, quiet card refresh, restart restoration and
dismissal. `calendar.view` is UI-only; `calendar.update_event` belongs to Calendar’s UI
and own agent, not new Mail/system grants. Keep App Hub’s explicit `calendar` capability, permission
wording and the consumer’s single contract source aligned.
