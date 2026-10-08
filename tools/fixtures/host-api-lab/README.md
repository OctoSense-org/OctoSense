# Native host API acceptance

English | [简体中文](README.zh-CN.md)

[Android reproduction and OnePlus 6 results](ANDROID.md): the same native host completed all 14 phone checks, without a model, account login or permission approval.

This development fixture demonstrates an app-owned Splash tool calling Rust
already compiled into OctoSense. It reads the real macOS camera permission
status, updates the same app UI, and returns a structured answer to its native
caller. It never captures media or approves device access. It is not an App Hub
submission or a way to load arbitrary Rust libraries.

The path is:

```text
temporary signed catalog → Store install / prepared launch
    → isolated app with admitted capabilities
    → apilab.inspect → app_tool(name, call_id)
    → host.request("camera.permission.status", ...)
    → Rust DeviceService → Makepad native permission query → macOS
    → callback → live app UI + mod.app_tools.complete(call_id, result)
```

The native fixture enters the authorized tool queue directly. A production
agent first passes the shell's relay, app-agent consent, account and tool
grants. This test does **not** exercise that upstream model/peer flow. It does
exercise the actual signed bundle checks, Splash isolate and device handler.

## Run on macOS

A graphical macOS session is required. The windows remain hidden and never
take focus. Use the compatible source graph prepared by `tools/setup.py`.
From the OctoSense checkout:

```sh
python3 tools/setup.py
cargo build --locked --release -p octosense-shell --example host-api-lab --features acceptance-fixtures
# Build the Hub CLI from this host's pinned dependency:
cargo build --locked -p octosense-app-hub --bin hub
python3 tools/test-host-api-native.py --hub target/debug/hub
```

The script creates a private evidence directory and prints its location. It
copies the fixture, captures its native preview for the listing, stamps the
copy, signs it with ephemeral keys, installs it into a new profile, and runs
the native probes. Source files and ordinary user profiles are unchanged.
Signing here is only for an isolated test trust anchor; it does not create a
publisher identity or modify the public catalog.

`result.json` records pass/failure and unverified paths even when setup fails.
`native-result.json` contains the native status and tool answer. The PNGs and
widget snapshots show the actual preview and completed app. Logs and private
profiles stay in that evidence directory; inspect locally before sharing.
The driver shuts down only its own test processes.

## What is checked

- The signed app calls its own declared `implemented_by: "app"` tool.
- The tool reads a real native permission status and updates the live UI.
- `runtime.describe` finds a compiled API and reports an unavailable custom
  function without executing it.
- No microphone capability means no microphone status access.
- A tool's asynchronous host callback cannot open a permission sheet.
- Wrong-account, undeclared-tool and invalid-input calls are refused.
- Unmounting the tool owner produces `app_not_running`.

The tool deliberately tries a background permission request to test refusal;
it cannot approve it. Capability declaration, app consent and OS authorization
remain separate. A previously granted OS permission is acceptable evidence,
but this fresh profile must still have `app_consent: false`.

**Not covered:** actual model reasoning, physical permission approval, camera
capture, Android execution, platform services on Linux/Windows, and publishing
a compatible host binary. Separate real-VM runtime regressions cover detached
timers, paused tasks,
HTTP/WebSocket callbacks and native device-helper gates. This fixture tests
chained host callbacks.

## Reuse the pattern

Copy the declaration and `app_tool` pattern into a developer app, then request
only its actual capabilities. The hook executes in the open app's live VM;
closed apps cannot run these tools. A `host_method` tool instead maps directly
to an allowlisted Rust service method. Neither mechanism compiles a new native
function from the bundle. Use `host_api.required` to refuse incompatible hosts
and optional discovery for a manual fallback. See
[ADR 0012](../../../docs/adr/0012-app-host-api-discovery.md) for the host boundary.
