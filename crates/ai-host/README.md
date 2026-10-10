# octosense-ai-host: the shell's AI services

English | [简体中文](README.zh-CN.md)

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
  the same providers for admitted apps, with per-app budgets and active
  account scope. Omitting `model` from capabilities does not deny a call;
- **script apps' agents**: the `octos` host service (`src/contained.rs`,
  below), one host-owned peer `card.<app id>` per app;
- **native apps' assistant access** (Rinx ADR 0007): a scoped
  `crates/app-peers` service offered to each granted native module instance
  at creation (`offer`), and a module's own peer link
  (`module_peer::ModulePeerLink`). The shell's module host serves that link
  to every module that opens Makepad's `OctosPeer`: App Hub, Calculator,
  Clock, Notes, Reminders and Weather, and the Terminal when it runs
  in-process. In a checkout build on macOS or Windows the Terminal runs as its
  own process and reaches its agent over the hub instead.

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
(`Policy::shipped()` uses `native-apps.json`'s `agent.octos` to identify
native assistant offers through generated `src/native_agents.rs`; a
consented offer receives all four supported methods).

Features: `octos-core` (the kernel, app-peers broker, llm restart; native
mobile targets always have it — `cfg(kernel)`, set by build.rs), `llm`
(register the `llm` service; a shell's `app-hub` turns it on) and
`toolbox-peers` (below; off by default, turned on by the shell's feature of
the same name).

## Script apps' agents: the `octos` host service (`src/contained.rs`)

`start` registers `ContainedOctos` (family `octos`) in App Hub's host-service
registry where the shell hosts a kernel. It is how every script app, system
or store, can use an opted-in agent:

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
  the fixed public method set for any admitted, opted-in, consented agent.
  `set_caller_admitted` checks the current bundle and exact host profile;
  `set_declared` returns `None` for an app without an agent, including when
  the public API is available to every app. An agent block, admitted tools,
  or a supported assistant declaration opts an app in. An empty declaration
  list on an opted-in agent is allowed. Missing identity/consent callbacks
  fail closed. Text is limited to 32 KiB, replies to 2 MiB; arbitrary kernel
  methods and app-selected sessions, profiles or accounts remain refused.
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

| Script app's exact `agent.tools` request | Offered tool (risk), `app: "toolbox"` |
| --- | --- |
| no toolbox tool requested | nothing |
| `workflow.run` / `workflow.fork` | that requested tool only (read / act) |
| `toolbox.search` / `toolbox.web_read` | that requested tool only (read) |
| `toolbox.deep_crawl`, with positive `max_depth` and `max_pages` | `toolbox.deep_crawl` (read) |

- `catalog()`: every toolbox tool, `shareable`, owned by `toolbox`; the relay
  declares it once and grants each app its `ToolboxGrant::tools()`.
- `ToolboxGrant::for_manifest` reads exact shared tool requests from an
  admitted, digest-checked manifest's `agent.tools`. It ignores `capabilities`:
  omitted disclosures do not deny a requested tool, and a `research`/`crawl`
  disclosure alone grants none. The top-level `research` object still bounds
  resource use with octos's `Scope` fields. Store and system apps follow the
  same policy. Selecting search never also grants workflow writes or crawling.
- Native `for_module` retains the shell's compiled, reviewed family offer;
  `ToolboxGrant::new(app, declared, granted, scope)` intersects that host
  selection. Neither path bypasses agent consent or relay approval.

For example, this manifest excerpt requests only search and bounded crawling:

```json
{
  "capabilities": [],
  "agent": {"profile": "read-only", "tools": ["toolbox.search", "toolbox.deep_crawl"]},
  "research": {"max_depth": 2, "max_pages": 5}
}
```

- `ToolboxExecutor`: the relay's executor for the `toolbox` owner. It checks
  the calling app's grant again (a forged `toolbox.deep_crawl` is
  `not_granted`), runs the call with the app's `AppContext` (id, grants,
  octos `Scope`) on a worker thread per app, answers once, and never answers
  a cancelled call. Template model calls go through the `model` service's
  `ModelHost::complete`: the person's providers and the app's daily budget,
  in the same ledger as `model.complete`.
- Consent (the #120 first-use sheet) is the relay's: no toolbox tool is
  offered to an app, or run for it, before the person allowed its agent.

Kernel tools are separate from shared toolbox tools. Contained apps keep
only the kernel tool names the host contract permits (`ask_user_question`),
selected in `agent.tools`; toolbox access never adds arbitrary kernel tools.

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

## Host-provisioned app guidance

The shell can call `contained::set_guidance(app_id, account, TrustedGuidance)`
before preparing a contained app's peer or between turns. The broker snapshots
that app/account's instructions and named skill texts for each request-context,
conversation, and system `peer/input` turn. Updates affect the next turn without
recreating the peer or deleting its history. The combined text is limited to
16 KiB and 16 skills; the host must check consent and persist any overlay itself.
Account changes and revocation clear the in-memory guidance.

This supplies host-provisioned **text**, not kernel-native skill installation or
discovery. Guidance and request data are separately serialized ordinary text
inputs; they are not separate kernel system-message roles. Tool grants and the
original `TurnTrigger` remain the authorization boundary, including for incoming
email. Incoming text cannot replace the host's structured guidance fields.

The guidance tests use isolated host/data directories and fake peers. They
check account mismatch, payload limits and provisioning without preparing a
peer. The declaration regressions exercise omitted/subset declarations,
agent opt-in, host-profile mismatch, consent and signed-out accounts. These
are scripted checks; they do not verify a live provider or phone.
