# Working in OctoSense

> **Any coding agent, or none.** These instructions work the same for Codex, Claude Code, Cursor, Gemini CLI, GitHub Copilot or a person at a terminal: every step is a shell command or a file edit, and nothing here needs a particular agent, model or vendor. `AGENTS.md` is the one source of truth; `CLAUDE.md` and `GEMINI.md` only import it for agents that look for those names. Directories with their own rules (`apps/AGENTS.md`, `apps/appcard/AGENTS.md`) add to these.

**Building a new OctoSense app?** You are in the wrong repository. Follow OctoScript-App-Design-Flow's [AGENTS.md](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/AGENTS.md) and publish through [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub). Read the system apps here (`apps/<name>/bundle/`) only as examples.

## Where things live

| To change | Edit | Check with |
| --- | --- | --- |
| The shell: window manager, styles, hosting, App Hub, the phone layer, themes and wallpapers | `crates/shell/` (package `octosense-shell`; never copy a shell file into `desktop/src` or `phone/src`) | both packagings' checks below, and from `phone/`: `cargo test --locked --features mobile-apps -p octosense-shell`; `desktop.yml`, `phone.yml` |
| The desktop packaging: entry point, catalogs, WM sync | `desktop/` | `cargo check --locked -p octosense` (and `--features mobile-apps`), `bash tools/check-shell-graph.sh -p octosense`, `desktop.yml` |
| Home's packaging: Settings, Android/OpenHarmony/iOS packaging, the phone side of the bridge | `phone/` (run cargo from `phone/`) | `cargo check --locked -p octosense-home --features mobile-apps`, `bash ../tools/check-shell-graph.sh -p octosense-home`, `phone.yml` |
| The OnePlus 6 image: vendor, patches, flash/OTA, APK build scripts, web installer | `rom/` | from `rom/`: `python3 -m unittest discover -s tests`, `rom.yml`; image builds are not in CI |
| The shell's AI services (kernel start, `llm` service, QR import, app peers) | `crates/ai-host/` | `cargo test --locked -p octosense-ai-host --features octos-core,llm`, `apps.yml` |
| The octos kernel service | `crates/kernel/` | `cargo test --locked -p octosense-kernel`, `apps.yml` |
| The octos kernel an Android APK bundles | `tools/kernel-artifact.py` | `python3 -m unittest discover -s tools -p 'test_*.py'` |
| Apps' access to the assistant | `crates/app-peers/` | its README, `apps.yml` |
| The system toolbox's workflow templates (library, runner, forks, evaluation, `mod.research`) | `crates/toolbox/` (templates in `crates/toolbox/templates/<id>/`) | `cargo test --locked -p octosense-toolbox`, its README, `apps.yml` |
| A system app | `apps/<name>/bundle/` | App Hub's `card-host --bundle apps/<name>/bundle --system`; then in a shell |
| A host service (`mail`, `llm`) | `apps/mail/host-service/`, `apps/ai-providers/` | `cargo test --locked -p octosense-mail-service -p octosense-llm-service` |
| AppCard (opt-in) | `apps/appcard/` | [apps/appcard/AGENTS.md](apps/appcard/AGENTS.md), `apps.yml` |
| An external pin (Makepad, OctoScript, App Hub, octos, Rinx) | root `Cargo.toml` `[workspace.dependencies]`, `native-runtime.lock.json`, `runtime-patches.lock.json` | `python3 tools/setup.py --update`, then `--check --cargo` |
| A decision | `docs/adr/` (next free number) | — |

Start every session with `python3 tools/setup.py` (it prepares `.sources/`, and changes nothing that is already right).

## Rules

1. **One change, one pull request.** Branch from `main`; the shell, its services and the apps change together, so there are no internal pins to move. Never push to or force-push `main`.
2. **One revision per external dependency.** Pins live only at the root. Members inherit them with `workspace = true`; never add a second Makepad, App Hub, octos or Rinx source, a `[patch]` to a moving branch, or a path to a checkout outside `.sources/`. `python3 tools/setup.py --check --cargo` must pass.
3. **Secrets are the host's.** No script app collects a password, PIN, key or one-time code, and none gets a field for one: a person types secrets only on a host-owned sheet (the `<family>.sheet.*` methods of a host service), the runtime makes password fields inert in a contained app, and the App Hub gate refuses bundles that declare them. Apps see masked status, never the secret.
4. **One kernel per process.** The kernel service starts octos on the first consumer and shares it; consumers never spawn their own. Apps reach the assistant only through `crates/app-peers`, never through the raw kernel protocol.
5. **System apps ship inside the shells.** Their ids start with `os.`, they are packed by digest from each packaging's `system-apps.json`, and they are not released separately or through the store.
6. **Test UI without taking over the screen.** Run hosted apps and shells with hidden windows and drive them over the local control surface: `MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE=<port>` (routes under `/help`; end with `/quit`), or `makepad_test`. Use `--test-action`, `MAKEPAD_WM_TEST_APP` and `--module` to reach a state without clicking.
7. **Devices.** Use only a device you were given for the task. Install test builds under a separate package name; never replace the installed Home or flash an image unless asked. Anything not run on a device is **unverified**, and says so.
8. **Commit with a public identity.** Author and committer are your GitHub noreply address (for the maintainer, `ymote <151983+ymote@users.noreply.github.com>`) or another address you mean to publish; never a work or machine-local address. Check `git config user.email` before the first commit in a clone: a repository-local identity on a shared machine leaked a work address into this project's history once, and removing it took a history rewrite.
9. **Keep docs honest and bilingual.** Every command in a doc was run; anything not run is marked **unverified**. User-facing docs are `README.md` plus `README.zh-CN.md`, linked by the single switcher line under the title (`English | [简体中文](README.zh-CN.md)`), and change together. `AGENTS.md` files are English only.
10. **Keep signing keys, keystores, tokens and personal paths out of the repository.** `rom/tests/test_no_local_paths.py` checks for local paths.
