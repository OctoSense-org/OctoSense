# octosense-ai-host: the shell's AI services

> Where this fits: see the [OctoSense architecture](../../docs/architecture.md) (with diagrams).

One entry point for what every OctoSense shell (desktop/, phone/) hosts:

- **the octos kernel** (`crates/kernel`) as a shell service: configured once,
  started when a consumer (AppCard, Rinx) first connects, restarted by the
  `llm` service after a provider change, stopped at shutdown;
- **the `llm` host service** (`apps/ai-providers/host-service`) the AI
  providers system app calls, with the platform's QR import (Android camera
  and image picker, desktop open panel and drops, elsewhere a pasted code);
- **apps' assistant access** (Rinx ADR 0007): a scoped `crates/app-peers`
  service offered to each granted native module instance at creation.

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
// Event::Shutdown:
ai_host::shutdown();
```

`Host` fields: `data_dir`; `kernel: KernelSource` (`Bundled` on Android,
`InProcess` on OpenHarmony, `Env` = `$OCTOS_APP_CORE_BIN` on a desktop,
`Program(path)`, `None`; `KernelSource::platform()` picks); `qr_import:
QrImport` (`platform()` or `paste_only()`); `policy: Policy`
(`Policy::shipped()` grants Rinx the `octos.*` services).

Features: `octos-core` (the kernel, app-peers broker, llm restart; native
mobile targets always have it — `cfg(kernel)`, set by build.rs), `llm`
(register the `llm` service; a shell's `app-hub` turns it on) and
`toolbox-peers` (below; off by default, turned on by the shell's feature of
the same name).

## The system toolbox for app agents (`toolbox-peers`)

ADR 0002 section 6 and ADR 0004 section 12. The toolbox is one more owner of
host-routed tools in the shell's host-tool relay (octos#2567's shell side,
`crates/shell/src/host_tools/`): main's broker registers them after every
`peer/prepare` and reconnect (`generic_tools` omitted), and the relay
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
  until the shells' App Hub pin includes #26 and the host reads its verified
  grant.
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
list, which the broker does not set.

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
