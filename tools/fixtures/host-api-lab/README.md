# Host API Lab: native host API acceptance

English | [简体中文](README.zh-CN.md)

[Android reproduction and OnePlus 6 results](ANDROID.md): fixture 0.4 passed all **44 checks on OnePlus 6 / Android 15**, without a model, account login or permission approval. The original 14-check record remains historical evidence.

This development fixture shows an app's own Splash tool calling Rust code that is already compiled into OctoSense. The tool reads the real macOS camera permission status, updates the app's screen and returns a structured answer to its native caller. It also discovers the file/location APIs, writes and reads four synthetic bytes in its own storage jail, and verifies background refusals. It never captures media, starts location sampling, opens a file picker or approves device access. It is not an App Hub submission, and it is not a way to load arbitrary Rust libraries.

A call takes this path:

```text
temporary signed catalog → Store install / prepared launch
    → isolated app with verified bundle identity
    → apilab.inspect → app_tool(name, call_id)
    → host.request("camera.permission.status", ...)
    → Rust DeviceService → Makepad native permission query → macOS
    → callback → live app UI + mod.app_tools.complete(call_id, result)
```

The test host puts the call straight into the queue of authorized tool calls. A real agent's call must first pass the shell's relay, which checks the person's consent to the app's agent and the account and tool grants; this test does **not** exercise that model and peer path. It does exercise the real signed-bundle checks, the Splash isolate and the [device service](../../../crates/shell/src/platform_services/README.md).

## Public Calendar and Mail checks (fixture 0.4)

The current fixture adds the public `device_calendar` and `mail` APIs. It
requires the published [app-contract 1.10.0](https://crates.io/crates/octosense-app-contract/1.10.0)
declarations and a compatible host implementation, including the earlier `files`
capability. SDK installation alone does not provide that implementation. It checks four method descriptions, native Calendar permission
status, refusal to list calendar choices without this app's consent, refusal
of background permission/selection/event-write requests, and refusal of Mail
composition without an account and background sending. Mail uses a synthetic
transport in this fixture. It cannot deliver a message.

Nine further checks discover photo selection, text sharing, playback, recording and the Video controls runtime ABI,
then refuse background media requests, including recording. Microphone
permission status remains readable without a capability declaration or app consent. Sharing is advertised only on Android. No media device is opened.
Together, these twenty checks supplement the ten OS batch checks below. They do not
prove reading or writing real calendars, physical approval, or SMTP delivery.
The [Mac receipt](evidence/public-api-v0.4/macos.json) records all 30 checks passing; the [OnePlus 6 receipt](evidence/public-api-v0.4/oneplus6.json) records all 44 Android checks passing, including the original 14 checks. These records bind the source and runtime hashes they name. The previous 14- and 24-check receipts remain historical records.

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
- Without a `microphone` declaration, the app reads native microphone status while `app_consent` stays false. Recording from the app tool is refused by the foreground-only boundary.
- The tool's asynchronous host callback cannot open a permission sheet.
- File status/import/export and location sampling are discoverable; binary storage is advertised as a runtime ABI, not a `host.request` method.
- The live contained VM round-trips bytes `0, 127, 128, 255` through `fs.write_bytes` / `fs.read_bytes` and removes its temporary app file.
- File status reports the app's bounded storage availability and byte limit; background import and export are refused before opening native UI.
- The host binds the admitted app identity before evaluating its source. The live native storage handle exists for that identity and is refused for a different app. This adds `native_storage_identity_scoped` to the current check set; historical receipts above do not cover it.
- Location sampling from the tool's background callback returns the exact `location.sample is unavailable to agents/background surfaces` refusal. Its API descriptor also declares it foreground-only.
- Calls for the wrong account, for an undeclared tool or with invalid input are refused.
- After the app that owns the tool closes, a call fails with `app_not_running`.

From a host callback, which keeps the tool's background provenance, the tool deliberately calls `camera.permission.request` to prove that App Hub refuses it; nothing in the fixture can approve a permission. Verified app identity, the app's consent and the OS permission stay separate checks. Capability declarations describe intended use; they do not authorize device access. The Mac may already have granted OctoSense the camera permission, but the new profile must still report `app_consent: false`.

The current native location check proves that the host retains the callback's background provenance and refuses sampling before starting native work. Its receipt key remains `location_without_consent_refused` for compatibility, but its assertion now requires the exact background refusal above; the earlier `authorization_required` result belongs to the historical receipts below. The separate `platform_services::tests::location_sampling_broker_lifecycle` regression establishes app consent in an isolated test store and still requires that background refusal, with no queued request, permission check, consent review or running location sampler. That regression uses simulated permission results for its later lifecycle checks; neither check proves physical permission approval or a live location fix.

The 0.4 release-mode Mac run passed **30/30 named OS and public-service checks**, signed tool completion, live UI updates and native button interaction at source `53bab40f`, runtime `fc938badf`. All three native captures were inspected; both owned host processes exited cleanly. The [Mac receipt](evidence/public-api-v0.4/macos.json) binds the result to source, runtime and binary hashes. The [OnePlus 6 run](evidence/public-api-v0.4/oneplus6.json) passed **44/44 checks on Android 15**, using an APK built from production source `13e3b21a` with the same runtime; the intervening `53bab40f` change is test-only and excluded from that APK. Its isolated package was force-stopped afterward.

The separate [regression receipt](evidence/public-api-v0.4/regression.json) records **1,051/1,051 shared-shell tests, zero failures or ignored tests**, three packaging checks (desktop default/mobile and Home mobile), and the native fixture build at `53bab40f`. These receipts do not validate later Android Video Java changes or real-account/hardware actions. SDK 1.10.0 is published; [host distribution status](../../../docs/host-os-api-status.md) is separate. These historical receipts do not validate final Desktop RC2 packages or update the published Home beta.1.

The [earlier batch receipt](evidence/os-api-batch1/receipt.json) records ten OS checks at source `807f2bc8`; `evidence/android/` retains the original 14-check phone record. These historical results do not validate the current source. The current fixture replaces declaration-denial assertions with readable microphone status, absent app consent and exact background recording/location refusal; those semantics require a fresh receipt.

**Previously verified:** the `native-host-api` job of `.github/workflows/desktop.yml` ran these commands on a GitHub `macos-14` runner for the change that added this fixture, adding `--output` for its evidence directory, and they passed.

**Not covered by this macOS run:** real model reasoning, physical permission approval, camera capture, interactive file/photo/share choosers, live location sampling, native browser launch, Calendar event reads/writes, SMTP delivery, audio recording/playback, Android, Linux and Windows device services, and publishing a compatible host binary. Android has its [separate OnePlus 6 acceptance record](ANDROID.md). Separate runtime regression tests on the real Splash VM cover detached timers, paused tasks, HTTP and WebSocket callbacks, and the gates in the native device helpers; this fixture covers chained host callbacks.

## Reuse the pattern

Copy the tool declaration and the `app_tool` hook into your app, and describe the capabilities it uses. Device consent and OS permission are separate runtime requirements. The hook runs in the open app's live VM, so a closed app cannot run these tools. A `host_method` tool instead maps straight to an allowlisted Rust service method. Neither mechanism compiles new native code from the bundle. List the methods the app needs in `host_api.required`, so that App Hub keeps it off incompatible hosts, and use `host_api.optional` with `runtime.describe` to offer a fallback when a method is missing. App Hub's [script tool reference](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.md#script-tool-execution-script-tools-v1) documents the hook, and [ADR 0012](../../../docs/adr/0012-app-host-api-discovery.md) describes the host boundary.
