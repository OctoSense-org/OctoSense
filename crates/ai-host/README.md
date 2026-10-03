# octosense-ai-host: the shell's AI services

> **Where this fits.** This crate is the shell's side of the octos kernel: it owns the kernel service, offers each granted native module its `OctosAppService` (from `crates/app-peers`), and serves script apps' `host.request("octos.*")` through the `octos` host service. Every path from an app into octos goes through it; apps never talk to the kernel. Diagrams of the processes, an app agent's two lanes and a tool call with its approval: [How it fits together](../../README.md#how-it-fits-together); the details: [docs/architecture.md](../../docs/architecture.md) and [ADR 0004](../../docs/adr/0004-native-apps-hosting-and-peers.md).

One entry point for what every OctoSense shell (desktop/, phone/) hosts:

- **the octos kernel** (`crates/kernel`) as a shell service: configured once,
  started when a consumer (the system chat, an app's agent, Rinx, AppCard)
  first connects, restarted by the `llm` service after a provider change,
  stopped at shutdown;
- **the `llm` host service** (`apps/ai-providers/host-service`) the AI
  providers system app calls, with the platform's QR import (Android camera
  and image picker, desktop open panel and drops, elsewhere a pasted code);
- **the `model` host service** (`model.complete`, implemented in
  `apps/ai-providers/host-service/src/complete/`): one-shot model calls over
  the same providers for apps granted the `model` capability, with per-app
  budgets;
- **script apps' agents**: the `octos` host service (`src/contained.rs`,
  below), one host-owned peer `card.<app id>` per app;
- **native apps' assistant access** (Rinx ADR 0007): a scoped
  `crates/app-peers` service offered to each granted native module instance
  at creation (`offer`), and a module's own peer link
  (`module_peer::ModulePeerLink`, which no module uses yet).

```rust
use octosense_ai_host as ai_host;
// handle_startup:
ai_host::start(ai_host::Host::platform(cx.get_data_dir()));
// every event, early:
ai_host::handle_event(cx, event);
// desktop drag/drop routing (`app_at`: the app whose window is at a point):
if ai_host::handle_drop(event, &app_at) { return; }
// Android extension packet `qr.image.result`:
ai_host::qr_image_result(id, &status, &detail);
// module host, around `module.create`:
let offer = ai_host::offer(module, &scope);
let parts = module.create(vm, open, handles);
let assistant = offer.finish(); // Option<Assistant>; dropping it releases the instance's leases
// a module's own peer link (Makepad's `OctosPeer::open`), as frames for the shell's peer link:
let link = ai_host::module_peer::ModulePeerLink::new(parked_link);
let out = link.frames_down(); // hand to peer_link::module_connected
for frame in link.take_up() { /* peer_link::on_module_frame(...) */ }
// Event::Shutdown:
ai_host::shutdown();
```

`Host` fields: `data_dir`; `kernel: KernelSource` (`Bundled` on Android,
`InProcess` on OpenHarmony, `Env` = `$OCTOS_APP_CORE_BIN` or the packaged
`octos-kernel` on a desktop,
`Program(path)`, `None`; `KernelSource::platform()` picks); `qr_import:
QrImport` (`platform()` or `paste_only()`); `policy: Policy`
(`Policy::shipped()` grants Rinx the `octos.*` services).

Features: `octos-core` (the kernel, app-peers broker, llm restart; native
mobile targets always have it — `cfg(kernel)`, set by build.rs), `llm`
(register the `llm` service; a shell's `app-hub` turns it on) and
`toolbox-peers` (below; off by default, turned on by the shell's feature of
the same name).

## Script apps' agents: the `octos` host service (`src/contained.rs`)

`start` registers `ContainedOctos` (family `octos`) in App Hub's host-service
registry where the shell hosts a kernel. It is how every script app, system
or store, has an agent:

- **The peer.** One host-owned octos peer per app, `card.<app id>`
  (`PEER_PREFIX`, `peer_id`), launched through
  `octosense_app_peers::hosted::launch` and owned by the system agent. It acts
  for `device` (`ACCOUNT`), or, for an app whose manifest sets
  `storage.accounts`, for the account the shell reports (`set_account_of`;
  Mail's signed-in account); without one it answers `SIGN_IN`.
- **The gate.** `Policy::shipped()` reads `OCTOSENSE_CONTAINED_APPS`
  (`contained_gate_from`): unset is `ContainedGate::Consent` (each app once
  the person allowed its agent on the first-use sheet), `1` is `Everyone`
  (asks nobody, for development), `0` is `Off` (`TURNED_OFF` for every app).
  The service is registered even when off, so an app hears why.
- **The app's own calls.** `host.request("octos.session.open" |
  "octos.session.history" | "octos.turn.start" | "octos.turn.interrupt")`,
  only the names its manifest declares (`set_declared`, else
  `NOT_DECLARED`); text at most 32 KiB, replies at most 2 MiB. None of
  today's system apps with an agent (News, Mail, Calendar) declares one:
  their agents are driven by the shell.
- **The shell's calls.** `prepare` (the shell prepares every allowed app's
  peer at startup and when it is allowed, so the system agent's `peer_list`
  shows it), `conversation` (the person's lane, for the "Ask <app>" panel and
  a card's `sys.chat`), `revoke` (the agent turned off) and
  `account_changed`.

## The system toolbox for app agents (`toolbox-peers`)

ADR 0002 section 6 and ADR 0004 section 12. The toolbox is one more owner of
host-routed tools in the shell's host-tool relay (octos#2567's shell side,
`crates/shell/src/host_tools/`): the broker registers them after every
`peer/prepare` and reconnect, with the app's other tools, and the relay
authorizes each call and routes it to the toolbox's executor. This crate adds
only the toolbox's part (`src/toolbox_peers.rs`, over `crates/toolbox`'s
`peer` module):

| Declared and granted | Offered (risk), each `app: "toolbox"` |
| --- | --- |
| neither | nothing |
| `research` | `workflow.run` (read), `workflow.fork` (act), `toolbox.search` (read), `toolbox.web_read` (read) |
| `crawl`, with `max_depth` and `max_pages` above 0 in the scope | `toolbox.deep_crawl` (read) |

- `catalog()`: every toolbox tool, `shareable`, owned by `toolbox`; the relay
  declares it once and grants each app its `ToolboxGrant::tools()`.
- `ToolboxGrant`: what the app declares AND the person granted
  (`ToolboxGrant::new(app, declared, granted, scope)`). A native module's
  declared capabilities are reviewed with the shell (`for_module`). A script
  app's manifest (`for_manifest`: `research`/`crawl` in `capabilities`, the
  scope in octos's `Scope` shape under the top-level `research` object, App
  Hub #26's shape) is, **temporarily**, granted only to system apps (`os.*`)
  until the host reads App Hub's verified grant. The shells' App Hub pin
  (`0d5b47a2`) already includes #26; the code still keeps the `os.*` gate
  (`system_app_only`). No system app declares `research` or `crawl` yet.
- `ToolboxExecutor`: the relay's executor for the `toolbox` owner. It checks
  the calling app's grant again (a forged `toolbox.deep_crawl` is
  `not_granted`), runs the call with the app's `AppContext` (id, grants,
  octos `Scope`) on a worker thread per app, answers once, and never answers
  a cancelled call. Template model calls go through the `model` service's
  `ModelHost::complete`: the person's providers and the app's daily budget,
  in the same ledger as `model.complete`.
- Consent (the #120 first-use sheet) is the relay's: no toolbox tool is
  offered to an app, or run for it, before the person allowed its agent.

Nothing else is held back: octos's own generic tools (`deep_research` among
them) are the kernel's, and which of them a peer gets is its `generic_tools`
list: exactly the kernel tools its manifest names and the person granted,
which the broker sets with every registration.

Which shells build it: the phone's default features include `toolbox-peers`
(`phone/Cargo.toml`, generated from `native-apps.json`); the desktop's do
not (the desktop package has the feature, off by default). On the phone,
`src/webview_render.rs` lets the octos reader render pages it cannot read
over plain HTTP in hidden system WebViews.

Results are written to the host-owned `<apps root>/.host/toolbox/<app id>`
(`toolbox_folder`, always compiled; run results under
`toolbox/runs/<template>/<run>.json`, research items under `research/`),
outside the app's jail, where the glance screen's `sys.digest` (OctoSense
#87) reads them.

Tests: `cargo test -p octosense-ai-host --features octos-core,llm` (and
without features for a kernel-less desktop); `--features toolbox-peers` adds
the toolbox's grants and executor through the broker against a scripted
kernel with the toolbox's fixture backends, and, when
`OCTOS_APP_PEERS_TEST_KERNEL` names an `octos` binary at the pinned revision,
`tests/toolbox_real_kernel.rs` against the real kernel. The module-host tests that
create real instances (Rinx included) live with each shell's
`module_host.rs`.

The Android APK's kernel artifact (`liboctos.so`) is built by
`tools/kernel-artifact.py`; the graph guards are `tools/check-shell-graph.sh`.
