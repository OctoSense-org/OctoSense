# Host API Lab on Android

English | [简体中文](ANDROID.zh-CN.md)

The [receipt](evidence/android/receipt.json) records **14/14 checks on a OnePlus 6 running Android 15**. The [native result](evidence/android/native-result.json) comes from the actual phone process. A signed ordinary bundle calls its own Splash tool, which discovers a host API and reads Android's camera permission status through the production service. The fixture rejects cross-account calls, undeclared capabilities/tools, invalid arguments, uncompiled Rust functions, background permission prompts and calls after the tool endpoint closes.

This proves the native app/tool/service path. It does not start a model or peer agent, capture a camera image, approve a permission, log in to a provider, or establish physical approval. The test package was stopped after completion. Normal Home and personal profiles were not changed.

Use an assigned phone and the existing Makepad Android SDK/JDK. From the repository root, prepare the pinned runtime and build its packager:

```sh
python3 tools/setup.py
cargo build --release --offline --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml
```

Set `JAVA_HOME` and `ANDROID_HOME` to existing tool installations. Set `PACKAGER` to that built `cargo-makepad` executable and `MAKEPAD_ANDROID_SDK` to the existing Makepad Android SDK. Build a separate debuggable package; `octosense-host-api-smoke` uses the same Rust source as the desktop `host-api-lab` example:

```sh
MAKEPAD_FORCE_DEBUGGABLE=1 "$PACKAGER" makepad android \
  --sdk-path="$MAKEPAD_ANDROID_SDK" --abi=aarch64 \
  --package-name=dev.makepad.octosense.hostapilab \
  --app-label='Host API Lab' \
  build -p octosense-host-api-smoke --release --locked --offline
```

Set `APK` to the aligned APK reported by the packager, `HUB` to the built compatible App Hub CLI, and choose a new `EVIDENCE` directory. Run the tested driver:

```sh
python3 tools/test-host-api-android.py \
  --adb "$ANDROID_HOME/platform-tools/adb" \
  --aapt2 "$ANDROID_HOME/build-tools/35.0.0/aapt2" \
  --apk "$APK" --hub "$HUB" --out "$EVIDENCE"
```

The driver checks the APK's package and debuggable flag, refuses an already installed package, copies only the public synthetic bundle, stamps its Android listing, and collects the native result. The phone host creates ephemeral signing keys and admits the bundle through the normal signed Store path. No grants or credentials are injected. If multiple devices are attached, supply `--serial` for the assigned phone; serials are omitted from evidence. For another run, choose a fresh `.hostapilab.<suffix>` package consistently in the build and driver's `--package` argument.

The driver force-stops only its own package after success or failure. It leaves that isolated test package/data available for inspection. The published evidence contains no device serial, personal account, credential or local checkout path. The commands above were exercised using existing local tool/cache paths; they install no build tools.
