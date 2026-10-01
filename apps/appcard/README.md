# AppCard

English | [简体中文](README.zh-CN.md)

The **AppCard assistant runtime** — the "Ask anything" tile: the `octos-app`
crate workspace and the card prompt corpora it compiles in. The OctoSense
shells host it in a tile: you type a request, a routing brain (the AMA) picks
or composes an app agent, and that agent generates a live interactive card. The
card is a Splash DSL card or a webview card, and it binds real data at render
time.

Unlike the other apps in `apps/`, AppCard is not (yet) a contained
script app with a `bundle/`. It is the one **native** app here: a Rust module
(`octos-app`) that the shells link in-process.

What lives here (paths relative to `apps/appcard/`):

```
app/              AppCard's crates (members of the repository's root workspace).
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
module/           octosense-appcard: the shell module that mounts octos-app in a tile.
tools/            setup-native.py (runtime check), octos macOS/OHOS runners,
                  build-android.sh, dev-goal bridges, llm-qr, splash-research.
docs/             Architecture, protocol, build and review notes.
```

## Octos

Every octos crate (`octos-core`, and on OpenHarmony `octos-cli` with the
~20 crates it pulls in) comes from **one** source: git
`https://github.com/octos-org/octos.git` at the single rev in the repository's
root `Cargo.toml` `[workspace.dependencies]` (today `4a3ec9f9`, octos main),
shared with `crates/kernel` and both shells. octos's OpenHarmony-safe `nix`
is patched in the root `[patch.crates-io]` (from octos `18fcd3f1`). There is
no octos submodule; check the graph keeps one octos with
`cargo tree --locked -p octos-app -i octos-core --target all --depth 0`.

The runners that build the kernel *binary* (`tools/build-android.sh`,
`tools/octos-ohos.py`, `tools/octos-macos.py`) use an octos checkout, by
default `octos/` in the framework workspace (`OCTOS_SOURCE` selects another);
`build-android.sh` refuses a checkout at any other rev. They read that rev
from `app/Cargo.toml`, which the move into the root workspace removed:
**unverified**, these runners likely need `OCTOS_SOURCE` and a fix before they
work again. The shells' Android APKs bundle the kernel through
`rom/scripts/build-home.sh` instead.

## Framework sources

The app does not carry its own Makepad. Its crates take Makepad, Octoscript
and Octoscript-Makepad with `workspace = true`, and the root `Cargo.toml`
patches them to the checkouts in `.sources/` at the repository root, at the
release `native-runtime.lock.json` selects (this directory's copy of the lock
names the same release). Prepare and verify them from the repository root:

```sh
python3 tools/setup.py                 # prepare .sources/ (makepad, octoscript, octoscript-makepad)
python3 tools/setup.py --check --cargo # verify, including one Makepad in the graph
```

`app/app/build.rs` embeds framework resources from `OCTOSENSE_WORKSPACE`,
which the root `.cargo/config.toml` sets to `.sources`. This directory's own
Python tools (`tools/setup-native.py`, `build-android.sh`, the octos runners)
read the same variable but, outside cargo, still default to the parent of the
repository root; set `OCTOSENSE_WORKSPACE=<repo>/.sources` when you run them.
For the original sibling layout, see [docs/NATIVE-WORKSPACE.md](docs/NATIVE-WORKSPACE.md).

## Build and test

From the repository root:

```sh
cargo clippy --locked -p octos-app -p octos-app-store -p octos-app-transport -p octos-app-render --all-targets --no-deps -- -D warnings
cargo test --locked -p octos-app-transport -p octos-app-store
cargo run -p octos-app                                          # standalone window
(cd apps/appcard && PYTHONPATH=tools python3 -m unittest core.test_native_runtime)
```

CI is the `apps` job of [.github/workflows/apps.yml](../../.github/workflows/apps.yml)
(changes under `apps/`, `crates/` and the workspace files): it prepares
`.sources/`, runs the runtime lock tests, clippy for the four crates and the
transport and store tests, and checks that the Cargo graph has one Makepad,
octos, App Hub and Rinx source. For Android, see [docs/BUILDING-ANDROID.md](docs/BUILDING-ANDROID.md)
and `tools/build-android.sh`. For OpenHarmony, see
[docs/BUILDING-OPENHARMONY.md](docs/BUILDING-OPENHARMONY.md).

## Consumers

A shell takes `octos-app` without its standalone entry points and mounts it
as a widget:

```toml
# inside this repository: a workspace dependency (as apps/appcard/module does)
octos-app = { workspace = true, default-features = false }
# outside it: a git dependency (Cargo finds the package inside the repository by name)
octos-app = { git = "https://github.com/OctoSense-org/OctoSense.git", rev = "<sha>", default-features = false }
```

An outside consumer must also patch Makepad, Octoscript and Octoscript-Makepad to the locked
release and set `OCTOSENSE_WORKSPACE` in its Cargo configuration (the build
embeds framework resources from that workspace; from a git checkout the
default does not exist). A dependency's `[patch]` sections do not apply to
its consumer, so a consumer that builds for OpenHarmony also patches `nix`
as the root `Cargo.toml` does.

The hosting API is in `app/app/src/host.rs`: call
`octos_app::register_script_mods(vm)`, then mount `AppShell::create(vm)`, a
widget that owns the app and draws `OctosAppBody` (the app's root without the
standalone `Window`). `AppShell::ask` submits text as if typed into the
composer; `AppShell::shutdown` runs before the host frees the isolate.

- **Both OctoSense shells** (desktop and Home) mount it through
  [`module/`](module) (`octosense-appcard`): an `AppCardModule` that
  implements the shell's `AppModule` trait around `AppShell`, opt-in with
  `--features app-appcard`. They build it from the same commit, so a change
  here reaches them in the same pull request.
- **Rinx** embeds the AppCard tile; its repin is a separate follow-up.

## Provenance

Part of the OctoSense-System-Apps repository until 2026-09-27, then imported
into OctoSense with its history. It moved into System-Apps from
OctoSense-org/OctoSense-AppCard
at commit `d0a836b8`, which had split it from
[OctoSense-org/OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow)
(`app/` at commit `cbbda4da`). The full history of these files is there.
