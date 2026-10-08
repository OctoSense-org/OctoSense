# Wasm phone acceptance

English | [简体中文](README.zh-CN.md)

This synthetic app exercises the signed Store → app tool → `host.request` → Wasm service path. It uses no personal account or model. It checks fresh state after a successful call, guest error, trap and deadline; raw native responses must survive bounded JSON conversion. It also checks account scope, undeclared tools, invalid arguments, closed endpoints and absence of an approval sheet.

Use a clean committed checkout and an assigned Android device. The fixture embeds its source commit and runtime tree when compiled; the driver refuses stale or dirty builds. It installs only a fresh `dev.makepad.octosense.hostapilab.*` package and stops that package afterward. It does not use or replace Home. The APK is a debuggable test build, not a release or performance benchmark.

Use your existing Android SDK, JDK, ADB and pinned Makepad packager. Set `CARGO_MAKEPAD`, `MAKEPAD_ANDROID_SDK`, `ADB`, `AAPT2`, `HUB`, `WASM_TARGET` and `WASM_EVIDENCE` to your local paths; the evidence directory must not exist. From the repository root:

```sh
python3 tools/setup.py --check --cargo
MAKEPAD_FORCE_DEBUGGABLE=1 CARGO_TARGET_DIR="$WASM_TARGET" "$CARGO_MAKEPAD" makepad android   --sdk-path="$MAKEPAD_ANDROID_SDK" --abi=aarch64 --version-code=2026100703   --package-name=dev.makepad.octosense.hostapilab.wasm1 --app-label=OctoSenseWasmIsolationTest   build -p octosense-wasm-phone-smoke --release --locked --offline
python3 tools/test-wasm-phone.py --adb "$ADB" --aapt2 "$AAPT2" --hub "$HUB"   --apk "$WASM_TARGET/makepad-android-apk/octosense_wasm_phone_smoke/apk/octo_sense_wasm_isolation_test.apk"   --out "$WASM_EVIDENCE"
```

The driver stamps a temporary copy. The native host creates ephemeral signing keys in memory and uses normal signature, digest, capability and tool admission. Keys and provider credentials are not published. Receipts record APK/source/runtime identities and individual results, without device serials or SDK paths. Keep an earlier receipt separate; choose a new package suffix for another install or remove only your previous test package.

`state.wat` is the source of `bundle/fns/state.wasm`. To reproduce the checked-in module:

```sh
cargo run --locked --offline -p octosense-wasm-host --example encode_phone_fixture --   tools/fixtures/wasm-phone-lab/state.wat tools/fixtures/wasm-phone-lab/bundle/fns/state.wasm
```

The guest deliberately remembers input if its instance is reused. Passing requires the host to create a new instance, including after success. Earlier 19-check evidence used explicit result fields and did not prove raw response forwarding; the repaired raw path adds that check, and the reproducible harness adds two compiled-identity checks. Actual device results are recorded separately from unrun instructions.
