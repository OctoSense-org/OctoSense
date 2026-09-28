# octosense-ai-host: the shell's AI services

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
`toolbox-peers` (below; off by default).

## The system toolbox for app agents (`toolbox-peers`)

ADR 0002 section 6 and News M5: the toolbox's tools are registered with each
app's peer (octos#2567, host-registered peer tools) and run by the host
(`src/toolbox_peers.rs`, over `crates/toolbox`'s `peer` module).

| The app's grant | Registered tools (risk) |
| --- | --- |
| none | the empty set (registration is mandatory) |
| `research` | `workflow.run` (read), `workflow.fork` (act), `toolbox.search` (read), `toolbox.web_read` (read) |
| `crawl`, with `max_depth` and `max_pages` above 0 in the scope | `toolbox.deep_crawl` (read) |

octos's `deep_research` is never offered; `workflow.list` and
`workflow.evaluate` are not either (the run tool's description lists the
templates). All are `background`, none `outward`, `confirm: host`: none is
gated by the kernel. A call is checked again against the grant (a forged
`toolbox.deep_crawl` is refused), runs with the app's `AppContext` (id,
grants, octos `Scope`, its own budget) on the octos research engine, and its
model calls go through the `model` service's `ModelHost::complete`: the
person's providers and the app's daily budget, in the same ledger as
`model.complete`.

Results are written to the host-owned `<apps root>/.host/toolbox/<app id>`
(`toolbox_folder`, always compiled; run results under
`toolbox/runs/<template>/<run>.json`, research items under `research/`),
outside the app's jail, where the glance screen's `sys.digest` (OctoSense
#87) reads them.

**Grants.** `research`/`crawl` in the manifest's `capabilities`, the scope
in octos's `Scope` shape under the manifest's `research` object
(provisional: App Hub has no such capability yet). **Temporary:** until App
Hub verifies these capabilities only system apps (`os.*`) get what they
declare; any other app's declaration is ignored.

**Turning it on** needs octos#2567 on the workspace's octos pin: a kernel
without it refuses `peer/tools/register`, and the broker then starts no
turns for any app, Rinx included. It also changes what a registered peer's
turns get (see `crates/app-peers`'s README).

Tests: `cargo test -p octosense-ai-host --features octos-core,llm` (and
without features for a kernel-less desktop); `--features toolbox-peers` adds
the toolbox's registration and routing against a scripted kernel with the
toolbox's fixture backends, and, when `OCTOS_APP_PEERS_TEST_KERNEL` names an
`octos` built with octos#2567, `tests/toolbox_real_kernel.rs` against the
real kernel. The module-host tests that
create real instances (Rinx included) live with each shell's
`module_host.rs`.

The Android APK's kernel artifact (`liboctos.so`) is built by
`tools/kernel-artifact.py`; the graph guards are `tools/check-shell-graph.sh`.
