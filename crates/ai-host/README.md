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
mobile targets always have it — `cfg(kernel)`, set by build.rs) and `llm`
(register the `llm` service; a shell's `app-hub` turns it on).

Tests: `cargo test -p octosense-ai-host --features octos-core,llm` (and
without features for a kernel-less desktop). The module-host tests that
create real instances (Rinx included) live with each shell's
`module_host.rs`.

The Android APK's kernel artifact (`liboctos.so`) is built by
`tools/kernel-artifact.py`; the graph guards are `tools/check-shell-graph.sh`.
