# Native host API acceptance

English | [简体中文](README.zh-CN.md)

This development fixture shows an app's own Splash tool calling Rust code that is already compiled into OctoSense. The tool reads the real macOS camera permission status, updates the app's screen and returns a structured answer to its native caller. It never captures media and never approves device access. It is not an App Hub submission, and it is not a way to load arbitrary Rust libraries.

A call takes this path:

```text
temporary signed catalog → Store install / prepared launch
    → isolated app with admitted capabilities
    → apilab.inspect → app_tool(name, call_id)
    → host.request("camera.permission.status", ...)
    → Rust DeviceService → Makepad native permission query → macOS
    → callback → live app UI + mod.app_tools.complete(call_id, result)
```

The test host puts the call straight into the queue of authorized tool calls. A real agent's call first passes the shell's relay, the person's consent to the app's agent, and the account and tool grants; this test does **not** exercise that model and peer path. It does exercise the real signed-bundle checks, the Splash isolate and the [device service](../../../crates/shell/src/platform_services/README.md).

## Run it on macOS

You need a graphical macOS session. The windows stay hidden and never take focus. Run these commands from the OctoSense checkout; `tools/setup.py` prepares the pinned framework sources that the build uses:

```sh
python3 tools/setup.py
cargo build --locked --release -p octosense-shell --example host-api-lab --features acceptance-fixtures
# The Hub CLI, built from the App Hub revision this host pins:
cargo build --locked -p octosense-app-hub --bin hub
python3 tools/test-host-api-native.py --hub target/debug/hub
```

The script creates a private evidence directory and prints where it is; to choose the directory, pass `--output` with a path that does not exist yet. The script copies the fixture, captures its native preview as the listing screenshot, stamps the copy, signs it with keys that exist only in memory, installs it into a new profile and runs the native checks. It changes neither the source files nor any ordinary user profile. The signature serves only an isolated test trust anchor: it creates no publisher identity and does not touch the public catalog.

`result.json` records the outcome and what remains unverified, even when setup fails. `native-result.json` holds the native status and the tool's answer. The PNGs and widget snapshots show the real preview and the finished app. Logs and the private profiles stay in the evidence directory; review them before you share them. The script stops only the test processes it started.

## What it checks

- The signed app calls its own declared `implemented_by: "app"` tool.
- The tool reads a real native permission status and updates the live screen.
- A button on the app's screen reaches the same host service.
- `runtime.describe` finds a compiled API, and reports a custom function that the host lacks as unavailable, without running anything.
- Without the `microphone` capability, the app cannot read the microphone status.
- The tool's asynchronous host callback cannot open a permission sheet.
- Calls for the wrong account, for an undeclared tool or with invalid input are refused.
- After the app that owns the tool closes, a call answers `app_not_running`.

The tool deliberately requests a permission from the background to prove that the request is refused; it cannot approve it. The capability, the app's consent and the OS permission stay separate checks. The Mac may already have granted OctoSense the camera permission, but the new profile must still report `app_consent: false`.

**Verified:** these commands pass in the `native-host-api` job of `.github/workflows/desktop.yml` on a GitHub `macos-14` runner.

**Not covered:** real model reasoning, approving a permission with a physical press, camera capture, Android, the device services on Linux and Windows, and publishing a compatible host binary. Separate runtime regression tests on a real VM cover detached timers, paused tasks, HTTP and WebSocket callbacks, and the gates in the native device helpers; this fixture covers chained host callbacks.

## Reuse the pattern

Copy the tool declaration and the `app_tool` hook into your app, and request only the capabilities it really uses. The hook runs in the open app's live VM, so a closed app cannot run these tools. A `host_method` tool instead maps straight to an allowlisted Rust service method. Neither mechanism compiles new native code from the bundle. Use `host_api.required` to refuse incompatible hosts, and optional discovery to offer a manual fallback. App Hub's [script tool reference](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.md#script-tool-execution-script-tools-v1) documents the hook, and [ADR 0012](../../../docs/adr/0012-app-host-api-discovery.md) describes the host boundary.
