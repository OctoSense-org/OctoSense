# Working in OctoSense System Apps

> **Any coding agent, or none.** These instructions work the same for Codex, Claude Code, Cursor, Gemini CLI, GitHub Copilot or a person at a terminal: every step is a shell command or a file edit, and nothing here needs a particular agent, model or vendor. `AGENTS.md` is the one source of truth; `CLAUDE.md` and `GEMINI.md` only import it for agents that look for those names.

> octos appears below only as the runtime of the AppCard assistant, a product dependency. Changing or building the script apps and the Mail service does not need octos, and no step asks you to use octos as your coding agent.

These are shipping apps. Keep changes small, test them in a shell, and keep the
rules in README.md.

- An app is `apps/<name>/bundle/`: `manifest.json` + `main.splash` (+ artwork).
  Learn the language, the APIs and the development loop from
  [OctoScript App Design Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)
  (`docs/QUICKSTART.md`, `docs/SCRIPT-API.md`). Do not invent APIs: if a
  widget or call is not documented there or used by another app here, check the
  runtime source before using it.
- Run a bundle on a desktop with App Hub's `card-host --bundle apps/<name>/bundle
  --system` (add `MAKEPAD_REMOTE=<port>` to drive it over HTTP; Photos also
  takes `--static photos=<dir>`). `--system` lets an `os.*` id and an empty
  digest through, as the shell does. These flags are on App Hub's
  `apps/script-and-system-apps` branch (OctoSense-App-Hub#4) until it merges.
  `card-host` registers no host services: run Mail in a shell with
  `MAKEPAD_APP_CONFIG='{"mail_demo":true}'`.
- Validate on a phone through the ROM's Home as a separate test package; never
  replace the device's installed Home.
- Mail's service: change `apps/mail/host-service` and run
  `cargo test -p octosense-mail-service` from a shell workspace that links it
  (the ROM's `home/`, or OctoSense-Desktop).
- After a change, bump the shells' pins (`home/native-apps.lock.json` in the
  ROM, `native-apps.lock.json` in OctoSense-Desktop) in a pull request there.
  That wiring lands with OctoSense-ROM#18 and OctoSense-Desktop#36.
- Never add a password or one-time-code field to an app; secrets belong to a
  host service's sheet.

## AppCard (apps/appcard)

AppCard is the one native app: a Cargo workspace, not a bundle. Its own rules
are in [apps/appcard/AGENTS.md](apps/appcard/AGENTS.md); in short:

- Prepare the sibling runtime from `apps/appcard`: `python3 tools/setup-native.py`
  (Makepad, Octoscript, Octoscript-Makepad land beside this repository, at the
  release `native-runtime.lock.json` selects). Never vendor them here.
- Build and test in `apps/appcard/app`: `cargo check`, `cargo test --workspace`,
  and `cargo clippy -p octos-app -p octos-app-store -p octos-app-transport
  -p octos-app-render --all-targets --no-deps -- -D warnings`. If you touched
  `apps/appcard/tools/core` or `setup-native.py`, also run
  `PYTHONPATH=tools python3 -m unittest core.test_native_runtime` from
  `apps/appcard`.
- Relative paths out of `apps/appcard` (Cargo `[patch]`, `build.rs`,
  `tools/core/native_paths.py`) assume the sibling workspace is the parent of
  the repository root. Keep them consistent if anything moves.
- Octos is one git source at one rev (`apps/appcard/app/Cargo.toml`); do not
  add an octos submodule or path dependency.
- CI for it is `.github/workflows/appcard.yml`, filtered to `apps/appcard/**`.
