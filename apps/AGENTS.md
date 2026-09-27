# Working in OctoSense's system apps (`apps/`)

> **Any coding agent, or none.** These instructions work the same for Codex, Claude Code, Cursor, Gemini CLI, GitHub Copilot or a person at a terminal: every step is a shell command or a file edit, and nothing here needs a particular agent, model or vendor. `AGENTS.md` is the one source of truth; `CLAUDE.md` and `GEMINI.md` only import it for agents that look for those names.

> octos appears below only as the runtime of the AppCard assistant, a product dependency. Changing or building the script apps and the Mail service does not need octos, and no step asks you to use octos as your coding agent.

These are shipping apps. Keep changes small, test them in a shell, and keep the
rules in README.md. The repository-wide rules in [../AGENTS.md](../AGENTS.md)
apply too; run every cargo command from the repository root (the root
workspace) after `python3 tools/setup.py`.

If you are building a new OctoSense app rather than changing these, you are in
the wrong place: follow OctoScript-App-Design-Flow's
[AGENTS.md](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/AGENTS.md)
and use the bundles here only as read-only examples.

- An app is `apps/<name>/bundle/`: `manifest.json` + `main.splash` (+ artwork).
  Learn the language, the APIs and the development loop from
  [OctoScript App Design Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)
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
- Mail's service: change `apps/mail/host-service` and run
  `cargo test --locked -p octosense-mail-service`.
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
- Native apps reach the assistant through `../crates/app-peers`
  (`octosense-app-peers`): the shell gives each app whose declared `octos.*`
  services host policy grants ONE peer owned by the system agent and injects
  a scoped service at module creation; apps never get raw kernel protocol.
  Test with `cargo test --locked -p octosense-app-peers --features octos-core,ws`
  (and `OCTOS_APP_PEERS_TEST_KERNEL=<octos> cargo test -p octosense-app-peers
  --features octos-core --test real_kernel`); see its README.
- Never add a password or one-time-code field to an app; secrets belong to a
  host service's sheet.

## AppCard (apps/appcard)

AppCard is the one native app: Rust crates, not a bundle. Its own rules
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
