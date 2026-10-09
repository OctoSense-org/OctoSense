# Host API Lab: native host API acceptance

English | [简体中文](README.zh-CN.md)

[Android reproduction and OnePlus 6 results](ANDROID.md): the original fixture completed all 14 phone checks, without a model, account login or permission approval.

This development fixture shows an app's own Splash tool calling Rust code that is already compiled into OctoSense. The tool reads the real macOS camera permission status, updates the app's screen and returns a structured answer to its native caller. It also discovers the file/location APIs, writes and reads four synthetic bytes in its own storage jail, and verifies background refusals. It never captures media, starts location sampling, opens a file picker or approves device access. It is not an App Hub submission, and it is not a way to load arbitrary Rust libraries.

A call takes this path:

```text
temporary signed catalog → Store install / prepared launch
    → isolated app with admitted capabilities
    → apilab.inspect → app_tool(name, call_id)
    → host.request("camera.permission.status", ...)
    → Rust DeviceService → Makepad native permission query → macOS
    → callback → live app UI + mod.app_tools.complete(call_id, result)
```

The test host puts the call straight into the queue of authorized tool calls. A real agent's call must first pass the shell's relay, which checks the person's consent to the app's agent and the account and tool grants; this test does **not** exercise that model and peer path. It does exercise the real signed-bundle checks, the Splash isolate and the [device service](../../../crates/shell/src/platform_services/README.md).

## Run it on macOS

You need a graphical macOS session. The windows stay hidden and never take focus. Run these commands from the OctoSense checkout; `tools/setup.py` prepares the pinned framework sources that the build uses:

```sh
python3 tools/setup.py
cargo build --locked --release -p octosense-shell --example host-api-lab --features acceptance-fixtures
# The Hub CLI, built from the App Hub revision this host pins:
cargo build --locked -p octosense-app-hub --bin hub
python3 tools/test-host-api-native.py --hub target/debug/hub
```

The script creates a private evidence directory; to choose it, pass `--output` with a path that does not exist yet. Its last output line is a JSON summary. On success, that line is `{"result": "pass", "evidence": "<evidence directory>", "error": null}` and the script exits with status 0. On failure, `result` is `"failed"`, `error` says what failed, and `preview.log` and `signed.log` in the evidence directory hold the host's output.

The script copies the fixture, captures its native preview as the listing screenshot, stamps the copy, signs it with keys that exist only in memory, installs it into a new profile and runs the native checks. It changes neither the source files nor any ordinary user profile. Only the run's isolated test trust anchor accepts the signature, which creates no publisher identity and does not touch the public catalog.

`result.json` records the outcome and what remains unverified, even when setup fails. `native-result.json` holds the native status and the tool's answer. The PNGs and widget snapshots show the real preview and the finished app. Logs and the private profiles stay in the evidence directory; review them before you share them. The script stops only the test processes it started.

## What it checks

- The signed app calls its own declared `implemented_by: "app"` tool.
- The tool reads a real native permission status and updates the live screen.
- A button on the app's screen reaches the same host service.
- `runtime.describe` finds a compiled API, and reports a custom function that the host lacks as unavailable, without running anything.
- Without the `microphone` capability, the app cannot read the microphone status.
- The tool's asynchronous host callback cannot open a permission sheet.
- File status/import/export and location sampling are discoverable; binary storage is advertised as a runtime ABI, not a `host.request` method.
- The live contained VM round-trips bytes `0, 127, 128, 255` through `fs.write_bytes` / `fs.read_bytes` and removes its temporary app file.
- File status reports the storage grant and byte limit; background import and export are refused before opening native UI.
- Without app consent, location sampling returns the exact `authorization_required` refusal from the runtime's consent gate. Its API descriptor also declares it foreground-only.
- Calls for the wrong account, for an undeclared tool or with invalid input are refused.
- After the app that owns the tool closes, a call fails with `app_not_running`.

From a host callback, which keeps the tool's background provenance, the tool deliberately calls `camera.permission.request` to prove that App Hub refuses it; nothing in the fixture can approve a permission. The capability, the app's consent and the OS permission stay separate checks. The Mac may already have granted OctoSense the camera permission, but the new profile must still report `app_consent: false`.

The native location check proves refusal **without app consent**; it does not reach the host's foreground gate. The separate `platform_services::tests::location_sampling_broker_lifecycle` regression creates app consent in an isolated test store, dispatches a background sampling request through the real service broker, and requires the exact background refusal, with no queued request, permission check, consent review or running location sampler. That regression uses simulated permission results for its later lifecycle checks; it does not prove physical permission approval or a live location fix.

The native receipt includes ten additional boolean `checks`; both drivers require the complete set and every value to pass. All ten passed in the release-mode Mac run at source `807f2bc8`, along with the signed tool completion, live UI update and native button interaction. See the [batch receipt](evidence/os-api-batch1/receipt.json). Android retains its original 14 checks, for 24 checks total; its new APK built, but device execution is **pending connection of the assigned OnePlus 6**. These checks do not exercise interactive file selection, provider writes or a live location fix. Historical receipts under `evidence/android/` remain the original 14-check record.

**Previously verified:** the `native-host-api` job of `.github/workflows/desktop.yml` ran these commands on a GitHub `macos-14` runner for the change that added this fixture, adding `--output` for its evidence directory, and they passed.

**Not covered by this macOS run:** real model reasoning, approving a permission with a physical press, camera capture, Android, the unsupported-platform answers on Linux and Windows, and publishing a compatible host binary. Android has its [separate OnePlus 6 acceptance record](ANDROID.md). Separate runtime regression tests on the real Splash VM cover detached timers, paused tasks, HTTP and WebSocket callbacks, and the gates in the native device helpers; this fixture covers chained host callbacks.

## Reuse the pattern

Copy the tool declaration and the `app_tool` hook into your app, and request only the capabilities it uses. The hook runs in the open app's live VM, so a closed app cannot run these tools. A `host_method` tool instead maps straight to an allowlisted Rust service method. Neither mechanism compiles new native code from the bundle. List the methods the app needs in `host_api.required`, so that App Hub keeps it off incompatible hosts, and use `host_api.optional` with `runtime.describe` to offer a fallback when a method is missing. App Hub's [script tool reference](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.md#script-tool-execution-script-tools-v1) documents the hook, and [ADR 0012](../../../docs/adr/0012-app-host-api-discovery.md) describes the host boundary.
