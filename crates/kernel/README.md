# OctoSense's shared Octos server

English | [简体中文](README.zh-CN.md)

The shell owns one Octos agent runtime. Native apps use the scoped app-peer
broker; OctosCode TUI and Web attach to the same authenticated WebSocket server.
They do not start another kernel or contend for the same data directory.

## Talk to the system agent

1. Configure a model in **AI providers**, then select **Talk to Octos**.
2. The trusted sheet shows **Server origin**, **WebSocket endpoint**, and the
   system conversation, `_main:api:octosense#system` in profile `_main`.
3. For OctosCode Web, enter the origin where you host that client, such as
   `http://localhost:4173`, and select **Save origin and restart**. A changed
   origin interrupts active work. OctoSense does not bundle web-client assets.
4. Select **Copy access token**, then **Open web client**. Enter the sheet's
   server origin and paste the token into the web client's **Auth token** field.
   Select **Connect**, then **Open conversation** on its saved-link screen.
   The generated link selects the existing system conversation using the
   workspace confirmed by Octos. It contains no token. The same link can be
   opened on your computer.

The access token grants control of the agent, including its tools. It is copied
by native code at the user's request; neither the app script nor its service
reply receives it. Only use a web client you trust.

On desktop, launch OctosCode with its `--endpoint`, `--profile-id _main`, and
`--session '_main:api:octosense#system'` options and supply its auth token through
its environment/configuration. Leave stdio mode disabled. A bare `octoscode`
launch normally starts its own kernel and is not this connection. TUI launch
is **unverified** in this change; simultaneous native and browser clients were
verified against the real kernel with a scripted local model. A headless
Chromium run also opened the generated system link, sent a message and
displayed the reply.

### Computer connected to an Android device

The server binds only `127.0.0.1`; another APK on the device still needs the
token. A computer needs a tunnel. The ADB forwarding example below is
**unverified on a device in this change**. Replace `SERIAL` and `PORT` with the
authorized device and the port shown in the sheet; using the same port on both
ends keeps the displayed server address usable on the computer:

```sh
adb -s SERIAL forward tcp:PORT tcp:PORT
```

Serve the web client on the computer, allow that exact web origin in the
phone's sheet, open the system conversation link on the computer, and enter the
forwarded server origin and token there. No root, Termux, PRoot or Ubuntu is
required by this APK-bundled server. This does not add a Linux coding toolchain.
A browser may require permission to reach a local-network address.

## Runtime and lifetime

- Desktop and Android run `octos serve --host 127.0.0.1 --host-managed`.
  Android executes the APK's `liboctos.so`; desktop uses `OCTOS_APP_CORE_BIN`.
- The first native consumer or connection-sheet request starts the server.
  Port zero allocates a free port; only the pinned server's listener announcement
  is accepted. Native consumers retain their frame API and negotiate the
  capabilities previously enabled by stdio.
- The system workspace is resolved and saved in `system-workspace.txt`. Native
  system-session opens reuse it, including after Web scopes the session and
  after a full shell restart. App-peer workspaces remain independent.
- Provider/origin changes restart the server after the old process exits, keeping
  its port and token for reconnecting clients. Dropping the last native consumer
  leaves the server running. Closing the shell stops it. This is not a persistent
  Android foreground service; Android process death stops the agent too.
- `<core_dir>/client-connection.json` contains connection details and the secret
  token, is written atomically with mode `0600`, and is removed on orderly stop.
  Tokens rotate when the shell's kernel service is recreated. Do not publish this
  file or put the token in logs/command arguments.
- Core directory: explicit `Options::core_dir`, then `OCTOS_APP_CORE_DIR`, then
  `<app data dir>/octos-home/.octos` on phones or `~/octos-home/.octos` on desktop.
  AI providers writes `<core_dir>/profiles/_main.json`.
- OpenHarmony retains `octos_cli::embedded::serve_io`; external clients are
  unavailable there. iOS has no local kernel. `Options::stdio()` retains private
  pipe behavior for fixtures and embedding hosts, including stop when idle.

A shared server is not a shared conversation unless clients open the same
session. App peers keep their own scoped sessions. Browser-owned active turns
may still be interrupted when their WebSocket closes (upstream Octos issue
2167); this integration does not implement detached-turn ownership.

## Build and verify

The executable must contain the overlay in
[`octos-runtime-patches.lock.json`](../../octos-runtime-patches.lock.json).
It adds mandatory host-token authentication, disables password-free solo login
and loopback profile-header impersonation, and runs the profile in the server
process. Plain upstream binaries lacking `--host-managed` fail to start; there
is no unauthenticated fallback. The Octos pin and patch hash are checked before
application. Android packaging applies the overlay automatically.

The desktop plan and tests below were run from the repository root:

```sh
python3 tools/kernel-artifact.py --host --plan
python3 -m unittest discover -s tools -p 'test_kernel_artifact.py'
cargo test --locked -p octosense-kernel
```

`python3 tools/kernel-artifact.py --host` builds the release desktop executable
under `target/octos-kernel/target/release/octos` (**release command unverified**;
the pinned source plus overlay was built and exercised in debug mode). Set
`OCTOS_APP_CORE_BIN` to that binary when launching a shell.

Real integration tests use `OCTOS_CORE_TEST_KERNEL=<patched binary>` with
`cargo test --locked -p octosense-kernel --test real_kernel -- --nocapture`.
They check authentication, origin rejection, native/browser conversation
sharing, provider restart, token redaction and shutdown without external model
calls. The app-peer real-kernel suite also runs against this transport.

On OpenHarmony this crate links `octos-cli` from git octos-org/octos at the
one rev the root `Cargo.toml` pins for every octos crate (`3b5d17a4`). A
workspace that builds it for OpenHarmony also needs the `nix` patch (octos
rev `18fcd3f1`, see the root `Cargo.toml` `[patch.crates-io]`). On every other target it
links no octos crate at all: the kernel is a separate binary.

See [ADR 0003](../../docs/adr/0003-shared-octos-client-access.md). Android APK
packaging and physical-device behavior remain **unverified** in this change.
