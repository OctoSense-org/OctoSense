# octosense-octos-core: the shell's octos kernel

The [octos](https://github.com/octos-org/octos) agent kernel is a **shell
service**. The shell (OctoSense-ROM Home, OctoSense-Desktop) owns it; the
**AI providers** system app configures it through the `llm` host service;
**AppCard**, and next Rinx's native mini-app host, connect to it. This crate
is that service: one kernel per process, started on demand, shared,
restarted when the providers change.

It lives in `crates/` rather than `apps/` because it is not an app: it is
the shared runtime piece the shells, AppCard (`apps/appcard/app`) and the
`llm` service (`apps/ai-providers/host-service`, feature `octos-core`) link.

## What it does

| | |
|---|---|
| **Core dir** | octos's data dir: `<core_dir>/profiles/_main.json` is the profile the AI providers app writes. Resolved as: the shell's `Options::core_dir`, else `$OCTOS_APP_CORE_DIR`, else on Android/OpenHarmony `<app data dir>/octos-home/.octos` (the app-private octos home AppCard has always used), else `$HOME/octos-home/.octos` (`octosense_llm_config::profile::default_core_dir()`). |
| **One kernel, lazily** | The first `connect()` starts it; later ones share it. octos holds a single-writer lock on its data dir, so a second kernel on the same dir could not run anyway. |
| **Shared by frames** | A `Connection` carries UI Protocol (JSON-RPC) frames exactly as `octos serve --stdio` speaks them. Each consumer uses its own request ids and receives the replies to its requests and the notifications of the sessions it named (a notification for a session nobody named goes to every consumer). |
| **Restart** | `restart()` stops a running kernel (a no-op when none runs). Connections then end with `CloseReason::Restarted`; a consumer connects again, which starts a fresh kernel that reads the new profile. The next kernel starts only after the old one has exited and released its data dir. |
| **Idle stop** | When the last connection is dropped the kernel stops, as AppCard's own child used to. |
| **Shutdown** | `shutdown()` stops it and waits (5 s at most). |

How it starts, per platform (`src/launch.rs`):

- **Android**: `<nativeLibraryDir>/liboctos.so serve --stdio`, `HOME=<core
  dir's parent>` (the octos home), with AppCard's environment
  (`OCTOS_SKILLS_PATH`, `OCTOS_OMIT_WORKSPACE_HINT`, `RUST_LOG`, the
  `makepad.OCTOS_PROXY` proxy) and the kernel config's memory budget. The APK
  must bundle the kernel: `MAKEPAD_ANDROID_EXTRA_LIBS=liboctos.so=<octos>`
  (the shells' build scripts do it).
- **OpenHarmony**: the canonical core in-process,
  `octos_cli::embedded::serve_io(<octos home>, ..)`, on this crate's runtime
  with 8 MiB worker stacks (HAP native libraries may not exec).
- **Desktop**: `<program> serve --stdio --data-dir <core_dir>` (plus
  `--config <core_dir>/config.json` when that file exists) with
  `OCTOS_HOME=<core_dir>`; the program is the shell's `Options::program` or
  `$OCTOS_APP_CORE_BIN`. With neither there is no kernel: a developer's own
  `octos serve` is never touched.
- **iOS**: no kernel.

The kernel and the frame pump run on the crate's own Tokio runtime, so a
consumer may use any runtime or none.

## Using it

A shell, once at startup, before the first consumer:

```rust
octosense_octos_core::configure(
    octosense_octos_core::Options::default().app_data_dir(cx.get_data_dir()),
);
// The llm service (feature `octos-core`) writes under the same core dir
// and calls octosense_octos_core::restart() after every change.
octosense_llm_service::register_with(
    octosense_llm_service::Options::default().core_dir(octosense_octos_core::core_dir().unwrap()),
);
```

A consumer:

```rust
let mut conn = octosense_octos_core::connect()?;       // Err: no kernel here
conn.send(r#"{"jsonrpc":"2.0","id":"1","method":"session/open","params":{"session_id":"_main:api:x","profile_id":"_main"}}"#)?;
loop {
    match conn.recv().await {
        Ok(frame) => { /* a JSON-RPC frame for this consumer */ }
        Err(octosense_octos_core::CloseReason::Restarted) => { /* connect again, re-open sessions */ break }
        Err(other) => { /* the kernel stopped or could not start: tell the person */ break }
    }
}
```

AppCard's transport (`apps/appcard/app/crates/octos-app-transport`,
`kernel.rs`) is the reference consumer: on `Restarted` it fails the requests
still waiting, reconnects and opens its sessions again from their replay
cursors, so the app carries on.

**Rinx and other consumers.** A native mini-app host takes its own
connection (`connect()`), opens sessions with ids of its own (Rinx uses
`<profile>:api:rinx-mini-…`) and gets only its sessions' traffic, with no
coupling to AppCard's connection or UI queue. It must handle
`CloseReason::Restarted` by reconnecting.

Other functions: `core_dir()`, `home()`, `profile()`, `launch()` /
`is_available()` (whether and how a kernel would start), `status()`.

## Testing

```sh
cd crates/octos-core
cargo test                    # unit tests + the core against a stand-in kernel (python3)
# The real kernel: a profile written by octosense-llm-config, session/open,
# profile/llm/list, a provider change and a restart. Build octos at the rev
# AppCard pins, then:
OCTOS_CORE_TEST_KERNEL=/path/to/octos cargo test --test real_kernel -- --nocapture
```

Build the kernel for that test (and for an Android APK, with the NDK and
`--target aarch64-linux-android`) from octos-org/octos at the rev in
`apps/appcard/app/Cargo.toml`:

```sh
cargo build --release -p octos-cli --bin octos --no-default-features --features api,git,ast
```

CI: `.github/workflows/octos-core.yml` (this crate) and
`.github/workflows/appcard.yml` (AppCard, which links it).

## One octos

On OpenHarmony this crate links `octos-cli` from git octos-org/octos at the
one rev AppCard pins (`552767dd`). Move the two together. A workspace that
builds it for OpenHarmony also needs AppCard's `nix` patch (octos rev
`18fcd3f1`, see `apps/appcard/app/Cargo.toml`). On every other target it
links no octos crate at all: the kernel is a separate binary.
