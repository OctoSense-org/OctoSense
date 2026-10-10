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

This isolated fixture explicitly selects its temporary legacy catalog before the Wasm worker rechecks admission. Normal OctoSense keeps the GitHub catalog default; this test does not establish production catalog publication or keyless App Hub UI acceptance. Apple arm64 Android build hosts use the workspace's target-scoped AWS-LC CC configuration described in the [Android build note](../host-api-lab/ANDROID.md).

`state.wat` is the source of `bundle/fns/state.wasm`. To reproduce the checked-in module:

```sh
cargo run --locked --offline -p octosense-wasm-host --example encode_phone_fixture --   tools/fixtures/wasm-phone-lab/state.wat tools/fixtures/wasm-phone-lab/bundle/fns/state.wasm
```

The guest deliberately remembers input if its instance is reused. Passing requires the host to create a new instance, including after success. Earlier 19-check evidence used explicit result fields and did not prove raw response forwarding; the repaired raw path adds that check, and the reproducible harness adds two compiled-identity checks. The [OnePlus 6 receipt](acceptance-oneplus6.json) records **22/22 checks passed** on Android 15, built from source `e67ce63ebca6054961899691e1644a54c2b4f081` with runtime tree `0fc8e29e2411fd7eb94d845ba16a261d2ebc9dfc`. The build and driver commands above were executed with local tool paths. Live model/peer relay, performance and release APK behavior remain unverified.

The listing image is a separate native macOS capture of this fixture after its signed app tool completed, made in a hidden Makepad window from source `d3535a95`. It is not a phone screenshot and does not extend the 22-check OnePlus receipt to the later screenshot-only change. The image and refreshed fixture digest replace the earlier unrelated Host API Lab image.

## Real GitHub shared-component acceptance

The same native host also accepts a mirror containing a genuine GitHub-attested
`catalog-v2.json`, its two reviewed consumer bundles and pinned shared components
from [the synthetic publisher](https://github.com/ymote/octosense-component-demo).
This mode uses the default GitHub trust channel and actual Store admission;
it creates no legacy signatures and grants no additional capability.
The first consumer has no capability declarations; the second declares `wasm`
and `storage`. Both call the same component bytes in their own retained instance
and private storage. Four alternating app-tool turns check counters, Markdown,
file separation, alias discovery and a real component-to-host `runtime.describe`
round trip. No model or personal account runs.

Prepare the mirror with the reviewed
[App Hub candidate helper](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/SHARED-COMPONENT-REHEARSAL.md)
and the protected admin workflow's `dry_run: true` envelope. The official catalog
is not changed. The following commands are **unverified until a matching receipt
is recorded**; run the hidden desktop check before touching the assigned phone.
Use new evidence directories, an explicitly assigned OnePlus 6 serial and your
local tool paths:

```sh
cargo build --locked -p octosense-wasm-phone-smoke
python3 tools/test-shared-components.py --host target/debug/octosense-wasm-phone-smoke \
  --mirror "$SHARED_MIRROR" --out "$SHARED_MAC_EVIDENCE"
MAKEPAD_FORCE_DEBUGGABLE=1 CARGO_TARGET_DIR="$WASM_TARGET" "$CARGO_MAKEPAD" makepad android \
  --sdk-path="$MAKEPAD_ANDROID_SDK" --abi=aarch64 --version-code=2026100901 \
  --package-name=dev.makepad.octosense.hostapilab.shared1 --app-label=OctoSenseSharedComponentTest \
  build -p octosense-wasm-phone-smoke --release --locked --offline
python3 tools/test-shared-components.py --adb "$ADB" --aapt2 "$AAPT2" --serial "$ONEPLUS_SERIAL" \
  --apk "$WASM_TARGET/makepad-android-apk/octosense_wasm_phone_smoke/apk/octo_sense_shared_component_test.apk" \
  --mirror "$SHARED_MIRROR" --out "$SHARED_PHONE_EVIDENCE"
```

The driver refuses an existing test package, binds its receipt to the clean
compiled source/runtime and removes only its own newly installed phone package.
The receipt excludes the device serial. The raw native receipt records the real
catalog payload digest, bundle/component identities and all four result objects.
The two modes test different contracts: core functions use fresh instances;
shared components retain app-private instances while sharing immutable bytes.
Neither mode is a performance benchmark or a release Home upgrade test.
