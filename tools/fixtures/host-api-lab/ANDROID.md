# Host API Lab on Android

English | [简体中文](ANDROID.zh-CN.md)

**Public API batch (fixture 0.4):** [OnePlus 6 / Android 15 acceptance](evidence/public-api-v0.4/oneplus6.json) passed **44/44 checks**: the original 14 checks, ten OS checks and 20 Calendar/Mail/media discovery and refusal checks. The [Mac run](evidence/public-api-v0.4/macos.json) passed all 30 desktop checks. The phone APK was built from production source `13e3b21a`, runtime `fc938badf`, and has SHA-256 `2ad38099b4115bfa3b4c556cf181b7093ca1e672db73c48ea38d041b705f4dd1`. The build ended at `53bab40f`; that intervening change is test-only and excluded from the APK. Full source, runtime, SDK, adapter and artifact hashes are in the receipt. These results do not validate later Android Video Java changes.

The separate [regression record](evidence/public-api-v0.4/regression.json) passed 1,051 shared-shell tests, three packaging checks and the native fixture build at `53bab40f`. SDK 1.10.0 is published; [host distribution status](../../../docs/host-os-api-status.md) is separate. These historical receipts do not validate final Desktop RC2 packages or update the published Home beta.1. The earlier [24-check batch record](evidence/os-api-batch1/receipt.json) retains its then-pending phone status, and the original 14-check record below is preserved.

The historical [receipt](evidence/android/receipt.json) records **14/14 checks on a OnePlus 6 running Android 15**. The [native result](evidence/android/native-result.json) comes from the actual phone process. A signed ordinary bundle calls its own Splash tool, which discovers a host API and reads Android's camera permission status through the production service. That historical fixture rejected undeclared capabilities as well as cross-account calls, undeclared tools, invalid arguments, uncompiled Rust functions, background permission prompts and calls after the tool endpoint closed. The current fixture instead requires native microphone status to be readable without a declaration, with app consent still false, and requires exact background recording refusal. Fresh receipts are needed to validate these new semantics.

This proves the native app/tool/service path. It does not start a model or peer agent, capture a camera image, approve a permission, log in to a provider, or establish physical approval. The test package was stopped after completion. Normal Home and personal profiles were not changed.

Use an assigned phone and the existing Makepad Android SDK/JDK. From the repository root, prepare the pinned runtime and build its packager:

```sh
python3 tools/setup.py
cargo build --release --offline --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml
```

Set `JAVA_HOME` and `ANDROID_HOME` to existing tool installations. Set `PACKAGER` to that built `cargo-makepad` executable and `MAKEPAD_ANDROID_SDK` to the existing Makepad Android SDK. Build a separate debuggable package; `octosense-host-api-smoke` uses the same Rust source as the desktop `host-api-lab` example:

The workspace Cargo configuration selects the supported AWS-LC CC builder for both Android arm64 and an Apple arm64 build host. The Android packager otherwise forces CMake for host-side bundle validation too. This avoids an extra CMake installation without disabling cryptography or assembly. The equivalent target-scoped host environment override was exercised in the Android build before being added to the configuration.

```sh
python3 tools/build-host-api-android.py \
  --packager "$PACKAGER" --sdk "$MAKEPAD_ANDROID_SDK" \
  --package dev.makepad.octosense.hostapilab.publicapis1
```

The helper temporarily stages Home's exact `DeviceCalendarClient.java` in the
isolated fixture, uses its small native integration extension, and removes the
staged copy afterward. It installs no tools. The fixture queries permission
status only and never approves Calendar access. Use the same fresh package
with the driver (`--package dev.makepad.octosense.hostapilab.publicapis1`).

Set `APK` to the aligned APK reported by the packager, `HUB` to the built compatible App Hub CLI, and choose a new `EVIDENCE` directory. Run the tested driver:

```sh
python3 tools/test-host-api-android.py \
  --adb "$ANDROID_HOME/platform-tools/adb" \
  --aapt2 "$ANDROID_HOME/build-tools/35.0.0/aapt2" \
  --apk "$APK" --hub "$HUB" --out "$EVIDENCE" \
  --package dev.makepad.octosense.hostapilab.publicapis1
```

The driver checks the APK's package and debuggable flag, refuses an already installed package, copies only the public synthetic bundle, stamps its Android listing, and collects the native result. The phone host creates ephemeral signing keys and admits the bundle through the normal signed Store path. No grants or credentials are injected. If multiple devices are attached, supply `--serial` for the assigned phone; serials are omitted from evidence. For another run, choose a fresh `.hostapilab.<suffix>` package consistently in the build and driver's `--package` argument.

The driver force-stops only its own package after success or failure. It leaves that isolated test package/data available for inspection. The published evidence contains no device serial, personal account, credential or local checkout path. The build helper and 44-check driver were executed for the source/artifact hashes in the new receipt, using existing tools and cached dependencies. No build tools were installed by the helper. Calendar permission approval and event reads/writes, SMTP delivery, microphone capture/audio playback, native photo/share choosers and Video playback remain unverified by this Host API test; Video has a separate acceptance fixture.
