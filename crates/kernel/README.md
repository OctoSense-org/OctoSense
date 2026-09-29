# octosense-kernel: the shell's octos kernel

English | [简体中文](README.zh-CN.md)

> Where this fits: see the [OctoSense architecture](../../docs/architecture.md) (with diagrams).

The [octos](https://github.com/octos-org/octos) agent kernel is a **shell
service**. The shell (Home in `phone/`, the desktop in `desktop/`) owns it; the
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
| **Core dir** | octos's data dir: `<core_dir>/profiles/_main.json` is the profile the AI providers app writes. Resolved as: the shell's `Options::core_dir`, else `$OCTOS_APP_CORE_DIR`, else `<app data dir>/octos-home/.octos`: OctoSense's own octos home (the platform's app data dir on a phone, the app-private home AppCard has always used; OctoSense's state dir `~/.octosense` on a desktop), else `$HOME/octos-home/.octos` (`octosense_llm_config::profile::default_core_dir()`, for a consumer that names nothing). A desktop OctoSense used to share `$HOME/octos-home/.octos` with the person's standalone octos; on first use it now copies only the provider and model settings (`llm`, `env_vars`) from there into its own profile, and never writes, moves or deletes anything there. |
| **One kernel, lazily** | The first `connect()` starts it; later ones share it. octos holds a single-writer lock on its data dir, so a second kernel on the same dir could not run anyway. |
| **Shared by frames** | A `Connection` carries UI Protocol (JSON-RPC) frames exactly as `octos serve --stdio` speaks them. Each consumer uses its own request ids and receives the replies to its requests and the notifications of the sessions it named (a notification for a session nobody named goes to every consumer). |
| **Restart** | `restart()` stops a running kernel (a no-op when none runs). Connections then end with `CloseReason::Restarted`; a consumer connects again, which starts a fresh kernel that reads the new profile. The next kernel starts only after the old one has exited and released its data dir. |
| **Idle stop** | When the last connection is dropped the kernel stops, as AppCard's own child used to (not while Talk to Octos is on, below). |
| **Talk to Octos** | Off by default. While the person has it on, the kernel is octos's host-owned loopback server and external clients can attach to the same kernel (below). |
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
- **Talk to Octos on** (desktop and Android): the same command with
  `--host 127.0.0.1 --host-managed` instead of `--stdio`, and the listener
  this process keeps passed as descriptor 3 (`--listen-fd 3`, Unix).

The kernel and the frame pump run on the crate's own Tokio runtime, so a
consumer may use any runtime or none.

## Using it

A shell, once at startup, before the first consumer:

```rust
octosense_kernel::configure(
    octosense_kernel::Options::default().app_data_dir(cx.get_data_dir()),
);
// The llm service (feature `octos-core`) writes under the same core dir
// and calls octosense_kernel::restart() after every change.
octosense_llm_service::register_with(
    octosense_llm_service::Options::default().core_dir(octosense_kernel::core_dir().unwrap()),
);
```

A consumer:

```rust
let mut conn = octosense_kernel::connect()?;       // Err: no kernel here
conn.send(r#"{"jsonrpc":"2.0","id":"1","method":"session/open","params":{"session_id":"_main:api:x","profile_id":"_main"}}"#)?;
loop {
    match conn.recv().await {
        Ok(frame) => { /* a JSON-RPC frame for this consumer */ }
        Err(octosense_kernel::CloseReason::Restarted) => { /* connect again, re-open sessions */ break }
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
`is_available()` (whether and how a kernel would start), `status()`, and the
Talk to Octos controls below.

## Talk to Octos

Talk to Octos lets a web client or a terminal UI talk to this device's
assistant. It is **off by default**; the kernel is then the private stdio
child above and nothing listens. **AI providers → Talk to Octos** turns it on
(`set_external_access(true)`), which:

- restarts the kernel as `octos serve --host-managed` (octos
  [`docs/HOST_MANAGED_SERVE.md`](https://github.com/octos-org/octos/blob/main/docs/HOST_MANAGED_SERVE.md));
  native consumers keep the same frames over its WebSocket with a host token
  that never leaves this process (the kernel gets both tokens on its stdin,
  never in its environment), and request octos's stdio feature set
  (`octos_core::ui_protocol::UI_PROTOCOL_STDIO_DEFAULT_FEATURES`);
- mints an **external token**. It opens `/api/ui-protocol/ws` and nothing
  else, and there only an allowlist of session, turn, answer and read-only
  status methods: no configuration (providers, keys, skills, snapshots), no
  `server/shutdown`, nothing on an app's assistant's sessions (host-owned app
  peers), and answers only in sessions it opened. Its turns get no tool that
  runs code, administers octos or reaches peers (octos UPCR-2026-036);
- keeps the listener in this process (Unix) and hands it to every kernel
  generation, so a restart keeps the port and no other app can take it in
  between. Elsewhere a restart reuses the port when it is free, and otherwise
  moves to a new port with a new external token;
- keeps the server up when native consumers leave, until it is turned off or
  the shell exits. The kernel's stdin is its lifeline: when the shell exits or
  crashes, the kernel sees EOF and stops.

How clients get in:

- **Web.** The sheet's **Pair a web client** enables octos's pairing
  (`pairing()`): an 8-character code, valid for five minutes and one claim,
  shown with a QR of the web client's link
  (`<web origin>/?octos=<server>&pair=<code>`). The code only works while
  that sheet is open (`end_pairing()` when it closes). The web origin saved on
  the sheet is the only browser origin the server trusts: `https`, or on a
  desktop also `http` for localhost, 127.0.0.1 or [::1] (Android: https
  only). A malformed saved origin counts as none; the kernel still starts.
- **Terminal.** The connection file `connection_file(core_dir)`
  (`<core_dir>/client-connection.json`, mode 0600; on Windows
  `%LOCALAPPDATA%\OctoSense\client-connection.json`, whose default ACL admits
  this user, SYSTEM and administrators) holds the endpoint and the external
  token for a client of this user. It is rewritten when the port or token
  changes and removed when the server stops. Terminal UI launch is
  **unverified**.
- **Revoke all clients** (`rotate_external_access()`) mints a new external
  token and restarts the server; **Turn off** (`set_external_access(false)`)
  stops it, forgets the token and port, and removes the connection file.

A computer reaches a phone's server through a tunnel that keeps the port
number, since the server only answers requests whose `Host` names its own
port (**unverified on a device**):

```sh
adb -s SERIAL forward tcp:PORT tcp:PORT
```

The system conversation is `_main:api:octosense#system` in profile `_main`;
its workspace is saved in `system-workspace.txt` so native opens and Web's
scoped session agree. OpenHarmony (embedded core) and iOS (no kernel) have no
Talk to Octos. See [ADR 0003](../../docs/adr/0003-shared-octos-client-access.md)
for the threat model.

## The system agent's tools

[ADR 0004](../../docs/adr/0004-native-apps-hosting-and-peers.md) §12: the
system agent's tool set is its grants. Its default octos tools are
`system_tools::SYSTEM_AGENT_TOOLS`: supervision (`peer_send_input`,
`peer_gather`, `peer_list`, `peer_respond`, `peer_close`), its workspace's
file tools (octos fences them to the session's working directory), memory,
`ask_user_question`, media viewing, octos's `web_search` / `web_fetch` (until
toolbox grants replace them, #108) and `tool_search`. Granted toolbox and
cross-app tools join it as host tools through `SystemAgentTools`. Command
execution is **planned, not available**: it is to be a host tool with a live
approval (`terminal.run`), off by default and turned on by a Settings
switch. `SystemAgentTools::grant_command_execution` is the seam, but no
Settings switch sets it yet, and the host tool cannot be registered on the
system session until octos can register tools per session (octos#2567).

**What the kernel enforces today.** octos has no tool list the host can set
for one session, so every start writes the `_main` profile's `tool_policy`
(`system_tools::tool_policy`): everything a grant can give, minus octos's own
shell (`group:runtime`: `shell`, `bash`, `exec_command`, `write_stdin`), the
one tool OctoSense never offers, since §12 grants command execution only as
a host tool. octos applies it to every turn of the profile, wake
continuations included. So:

- **§12's "exactly its grants" is not yet enforced for the system agent**:
  it gets no octos shell, but otherwise is bounded by the grantable ceiling,
  not its list, until octos#2567 adds session-targeted registration and tool
  lists (reviewer item M1; our item 5). The exact-list real-kernel test is
  ignored until then;
- app peers are narrowed to their grants by their turns' `generic_tools`
  (plan step 6);
- Talk to Octos external turns keep octos's own allowlist.

The policy is written only into OctoSense's own core dir, and only over a
policy OctoSense wrote (`"owner": "octosense"`): a foreign policy, or the
person's own `$HOME/octos-home/.octos`, is refused with a warning.

## Testing

From the repository root:

```sh
cargo test --locked -p octosense-kernel   # unit tests + the core against a stand-in kernel (python3)
# The real kernel: a profile written by octosense-llm-config, session/open,
# profile/llm/list, a provider change and a restart; Talk to Octos on and off
# (what the external token must not reach, pairing, restart and rotation);
# native and web clients on one system conversation; and a host killed with
# SIGKILL taking its kernel with it. Build octos at the rev the root
# Cargo.toml pins, then:
OCTOS_CORE_TEST_KERNEL=/path/to/octos cargo test -p octosense-kernel --test real_kernel -- --nocapture
```

Build the kernel for that test (and for an Android APK, with the NDK and
`--target aarch64-linux-android`) from octos-org/octos at the rev in the root
`Cargo.toml` `[workspace.dependencies]`:

```sh
cargo build --release -p octos-cli --bin octos --no-default-features --features api,git,ast
```

`python3 tools/kernel-artifact.py --host --plan` prints the same build of the
pinned revision for this desktop.

CI: `.github/workflows/apps.yml` (the `services` job tests this crate; the
`apps` job builds AppCard, which links it).

## One octos

This crate links `octos-core` (for the stdio feature set) and, on
OpenHarmony, `octos-cli`, from git octos-org/octos at the one rev the root
`Cargo.toml` pins for every octos crate. A workspace that builds it for
OpenHarmony also needs the `nix` patch (octos rev `18fcd3f1`, see the root
`Cargo.toml` `[patch.crates-io]`). Elsewhere the kernel is a separate binary
built from that same rev.
