# AppCard

English | [简体中文](README.zh-CN.md)

The **AppCard assistant runtime** — the "Ask anything" tile: the `octos-app`
crate workspace and the card prompt corpora it compiles in. The OctoSense
shells host it in a tile: you type a request, a routing brain (the AMA) picks
or composes an app agent, and that agent generates a live interactive card. The
card is a Splash DSL card or a webview card, and it binds real data at render
time.

Unlike the other apps in this repository, AppCard is not (yet) a contained
script app with a `bundle/`. It is the one **native** app here: a Rust module
(`octos-app`) that the shells link in-process.

What lives here (paths relative to `apps/appcard/`):

```
app/              Cargo workspace root.
  app/            octos-app: routing brain (router + composer), multi-agent
                  dispatch, Splash card renderer + post-generation validator,
                  L0 card generation, WebView overlay for webview cards.
  crates/
    octos-app-store/      AppState reducer + selectors (Makepad-free).
    octos-app-transport/  WebSocket + REST transport speaking octos UI Protocol v1.
    octos-app-render/     streaming-markdown renderer wrappers.
a2app/            App-card memory for Splash cards: requirements-only specs,
                  widget patterns, live-data helper docs and per-app lint rules.
                  Compiled into octos-app via include_str!.
a2app-l0/         L0 card corpora: framework, catalog and per-app exemplars that
                  L0 card generation is prompted with. Also compiled in.
personal-data/    octos skill: read-only search over Mail/Calendar data.
vendor/           Vendored third-party crates (see the repository NOTICE).
tools/            setup-native.py (sibling runtime), octos macOS/OHOS runners,
                  build-android.sh, dev-goal bridges, llm-qr, splash-research.
docs/             Architecture, protocol, build and review notes.
```

## Octos

Every octos crate (`octos-core`, and on OpenHarmony `octos-cli` with the
~20 crates it pulls in) comes from **one** source: git
`https://github.com/octos-org/octos.git` at the single rev in `app/Cargo.toml`
(today `18fcd3f1`, branch `appcard/mate70-on-main`, until its commits land on
octos main). octos's OpenHarmony-safe `nix` is patched from the same rev. There
is no octos submodule. A shell that also depends on octos must use the same
rev, so its graph keeps one octos; check with
`cargo tree -i octos-core --target all`.

The runners that build the kernel *binary* (`tools/build-android.sh`,
`tools/octos-ohos.py`, `tools/octos-macos.py`) use an octos checkout at that
rev, by default `octos/` beside this repository (`OCTOS_SOURCE` selects
another); `build-android.sh` refuses a checkout at any other rev.

## Sibling workspace

The app does not carry its own Makepad. `app/Cargo.toml` patches Makepad,
Octoscript and Octoscript-Makepad to checkouts beside this repository
(`../../../../makepad`, `../../../../octoscript`,
`../../../../octoscript-makepad` from `app/`). They must be at the release
that `native-runtime.lock.json` selects. Lay the workspace out like this:

```
octosense-org/
  OctoSense-System-Apps/   this repository; AppCard is apps/appcard/
  octoscript-makepad/      shared UI framework; its runtime.json pins the engines
  octoscript/
  makepad/
```

Prepare and verify it from `apps/appcard/`:

```sh
python3 tools/setup-native.py          # clone/prepare the siblings at the locked release
python3 tools/setup-native.py --check --cargo-manifest app/Cargo.toml
```

`--update` moves clean sibling checkouts to a new release. Dirty trees are left
alone. `OCTOSENSE_WORKSPACE` selects a workspace other than the parent of this
repository. For details, see [docs/NATIVE-WORKSPACE.md](docs/NATIVE-WORKSPACE.md).

## Build and test

```sh
cd app
cargo check
cargo test --workspace
cargo clippy -p octos-app -p octos-app-store -p octos-app-transport -p octos-app-render --all-targets --no-deps -- -D warnings
PYTHONPATH=tools python3 -m unittest core.test_native_runtime   # from apps/appcard
```

CI ([.github/workflows/appcard.yml](../../.github/workflows/appcard.yml), run
only for changes under `apps/appcard/`) prepares the locked runtime, runs the
runtime lock tests and clippy, and checks that the Cargo graph has one Makepad
source. For Android, see [docs/BUILDING-ANDROID.md](docs/BUILDING-ANDROID.md)
and `tools/build-android.sh`. For OpenHarmony, see
[docs/BUILDING-OPENHARMONY.md](docs/BUILDING-OPENHARMONY.md).

## Consumers

A shell takes `octos-app` without its standalone entry points and mounts it
as a widget:

```toml
# as the shells do it: a path dependency on their pinned checkout of this repository
octos-app = { path = "<checkout>/apps/appcard/app/app", default-features = false }
# or a git dependency (Cargo finds the package inside the repository by name)
octos-app = { git = "https://github.com/OctoSense-org/OctoSense-System-Apps.git", rev = "<sha>", default-features = false }
```

It must also patch Makepad, Octoscript and Octoscript-Makepad to the locked
release and set `OCTOSENSE_WORKSPACE` in its Cargo configuration (the build
embeds framework resources from that workspace; from a git checkout the
default, the parent of this repository, does not exist). A dependency's
`[patch]` sections do not apply to its consumer, so a shell that builds for
OpenHarmony also patches `nix` from the same octos rev, as `app/Cargo.toml`
does.

The hosting API is in `app/app/src/host.rs`: call
`octos_app::register_script_mods(vm)`, then mount `AppShell::create(vm)`, a
widget that owns the app and draws `OctosAppBody` (the app's root without the
standalone `Window`). `AppShell::ask` submits text as if typed into the
composer; `AppShell::shutdown` runs before the host frees the isolate.

- **OctoSense ROM**, `home/apps/appcard`, and **OctoSense-Desktop**,
  `apps/appcard`: an `AppCardModule` that implements the shell's `AppModule`
  trait around `AppShell`. Both build it from the System-Apps revision their
  `native-apps.lock.json` pins (since
  [OctoSense-ROM#18](https://github.com/OctoSense-org/OctoSense-ROM/pull/18) and
  [OctoSense-Desktop#36](https://github.com/OctoSense-org/OctoSense-Desktop/pull/36)),
  with octos `18fcd3f1`; a change here reaches them when they move that pin.
- **Rinx** embeds the AppCard tile; its repin is a separate follow-up.

## Provenance

Moved here from
OctoSense-org/OctoSense-AppCard
at commit `d0a836b8`, which had split it from
[OctoSense-org/OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)
(`app/` at commit `cbbda4da`). The full history of these files is there.
