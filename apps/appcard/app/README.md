# octos-app workspace

English | [简体中文](README.zh-CN.md)

The Cargo workspace of the AppCard assistant: a native Makepad and Splash
client for the [octos](https://github.com/octos-org/octos) agent kernel. The
OctoSense shells mount it as a widget; it also builds as a standalone app.
How it fits into this repository, and the sibling runtime it needs, is in
[../README.md](../README.md).

## Crates

| Path | Crate | Role |
| --- | --- | --- |
| `app/` | `octos-app` | the app: routing brain (router + composer), multi-agent dispatch, Splash card renderer and post-generation validator, L0 card generation, WebView overlay; `src/host.rs` is the hosting API (`register_script_mods`, `AppShell`, `OctosAppBody`) |
| `crates/octos-app-store/` | `octos-app-store` | `AppState` reducer and selectors, no Makepad |
| `crates/octos-app-transport/` | `octos-app-transport` | WebSocket and REST transport for the octos UI Protocol v1 |
| `crates/octos-app-render/` | `octos-app-render` | streaming-markdown renderer wrappers |

Other directories: `splash-native/` (research: Splash cards rendered as
native Android views), `scripts/smoke-live.sh`.

`octos-app` has one feature, `standalone` (default): `fn main`, the Android
entry points and the dev monitor. A host that mounts `AppShell` builds with
`default-features = false`.

## Build, test, run

Prepare the sibling runtime first (from `apps/appcard`:
`python3 tools/setup-native.py`). Then, here:

```sh
cargo check
cargo test --workspace
cargo clippy -p octos-app -p octos-app-store -p octos-app-transport -p octos-app-render --all-targets --no-deps -- -D warnings
cargo run -p octos-app
```

The `Makefile` wraps the same commands (`make check`, `test`, `run`,
`clippy`, `fmt`) and adds `make smoke-live`, the ignored live transport test
against `OCTOS_LIVE_URL` (default `http://127.0.0.1:56831`). It reads a local
`.env` if present.

How the standalone app reaches octos, in order:

1. the shell's octos kernel (`octosense-octos-core`, repository
   `crates/octos-core`), when one can run: on Android the APK's bundled
   `liboctos.so`, on OpenHarmony always (linked in), on a desktop when a
   kernel binary is configured by the shell or `OCTOS_APP_CORE_BIN` (its data
   dir: `OCTOS_APP_CORE_DIR`, else `~/octos-home/.octos`;
   `../tools/octos-macos.py` sets both, see `../tools/OCTOS-MACOS.md`). The
   kernel is shared with the shell's other consumers and restarted when the
   AI providers change; the transport then reconnects and re-opens its
   sessions;
2. `~/.config/octos-app/server.json` (server URL and profile), with the
   bearer from `OCTOS_APP_TOKEN` or the OS keychain;
3. otherwise `OCTOS_BASE_URL`, `OCTOS_BEARER` and `OCTOS_PROFILE_ID`
   (default `https://localhost:8080`).

Never commit tokens or a `.env` file.

## CI

CI is the repository's
[.github/workflows/appcard.yml](../../../.github/workflows/appcard.yml). The
`.github/workflows/` directory inside this workspace came with the code from
its earlier repository; GitHub does not run it here.
