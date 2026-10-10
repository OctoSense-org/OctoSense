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
is not changed. The following build and driver commands were executed with
local tool paths, desktop first and the assigned OnePlus 6 last. To repeat them,
use new evidence directories, an explicitly assigned OnePlus 6 serial and your
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

The earlier completed run is bound to clean source
`c5f0c5c1948d9b730c707b409e9713af49e3e614` and Makepad runtime tree
`a6fae94aa4d233503b14bc6d0a36bcbc2b264b44`. The two components and two apps
were built and attested by [publisher workflow 38024551136](https://github.com/ymote/octosense-component-demo/actions/runs/38024551136)
for [v0.1.0](https://github.com/ymote/octosense-component-demo/releases/tag/v0.1.0).
[Protected admin workflow 38026083032](https://github.com/OctoSense-org/OctoSense-App-Hub/actions/runs/38026083032)
produced the real sequence-16 catalog envelope with `dry_run: true`, payload
SHA-256 `87bde245807a5ff6a1b3297c409d4ef6684414e47b038519a196feab29f42a7e`.
Its proof and all artifact digests were verified before either native run.
The public catalog stayed unchanged.

| Platform | Native component assertions | Driver checks | Evidence |
| --- | --- | --- | --- |
| macOS, hidden Makepad window | 28/28 | 12/12, including the final app and counter visible | [Driver](evidence/shared-components/macos.json), [raw native results](evidence/shared-components/macos-native.json), [native capture](evidence/shared-components/completed.png) |
| OnePlus 6 | 28/28 | 13/13, including the assigned device, fresh isolated package and compiled identities | [Driver](evidence/shared-components/oneplus6.json), [raw native results](evidence/shared-components/oneplus6-native.json) |

The receipts are copied without changes. They show actual Store installation,
Splash app-tool calls, alias discovery, a component-to-host call, deduplicated
read-only component bytes, and retained counters and files isolated per app.
The OnePlus test package was removed. No personal accounts or models were used,
and the installed Home was untouched. These are development-host receipts,
not released-binary acceptance. Live model relay, performance, OpenHarmony
device execution and a production Home upgrade remain outside this run.

## Final source acceptance

Source `8b09e05d1cb41b677340fa3f9415ee37ac88eace` was merged by
[#457](https://github.com/OctoSense-org/OctoSense/pull/457) as `40ca21da`.
Both have tree `703c30ecfdc6f2fa2db6323dafd526c9e0c9610c`. The final run used
the same genuinely attested private catalog above; the public catalog remained
unchanged. The earlier `c5f0c5c1` receipts remain historical and are not relabeled.

| Surface | Result | Evidence |
| --- | --- | --- |
| Full macOS desktop, fresh profile | 11/11 driver checks: App Hub search/install, app launch, component execution and cold restart | [Development-binary receipt](evidence/final-8b09/macos-desktop-rehearsal.json) |
| OnePlus 6, isolated component host | 28/28 native assertions and 13/13 driver checks | [Driver](evidence/final-8b09/oneplus6.json), [native results](evidence/final-8b09/oneplus6-native.json) |

The Mac receipt explicitly records `release_archive_tested: false`; it is not
RC4 archive acceptance. The phone receipt embeds its clean source/runtime and
APK identity, and records removal of the newly installed test package. Neither
test used a personal account or live model, modified the public catalog or
upgraded Home. Home beta.2 is older. Live-model relay, performance and
OpenHarmony device execution remain **unverified**. Final OS API results are
recorded separately in [Host API Lab](../host-api-lab/README.md#final-source-acceptance).

## Windows cache validation

At `a8e170d4`, [Windows CI](https://github.com/OctoSense-org/OctoSense/actions/runs/38066866571)
passed all 42 runtime tests (7 unit, 14 component, 7 guest, 14 runtime; none
skipped), then checked the shell with the Wasm service. Exact-head local CI
passed 52/52 steps without failures or skips. [#467](https://github.com/OctoSense-org/OctoSense/pull/467)
merged as `af205d9c` with the same tree.

The change removes a fixture's assumption that a cache hit must finish within
50 ms. Strict internal tests still require real cached module/component
deserialization and execution, corruption repair and complete concurrent
publication. The earlier [Windows failure](https://github.com/OctoSense-org/OctoSense/actions/runs/38065611111)
establishes a successful source-compilation fallback, not its exact cause.

The [derived validation summary](evidence/windows-cache-tests/validation.json)
records the source comparison: after excluding the new `cfg(test)` field,
branch and test module, production cache source is unchanged from RC4 source
`9266b008`; the other changes are integration tests. The RC4 tag and archive
source remain `9266b008` and **do not include #467**. This is not rebuilt-binary
equivalence, release archive acceptance or an installed Windows GUI test.
The original local CI receipt remains private; this summary contains only
public source identities and aggregate results.

## Unpublished RC3 candidate

The `desktop-v0.1.0-rc.3` [release workflow](https://github.com/OctoSense-org/OctoSense/actions/runs/38031827821)
ran at `40ca21da`. Mac and Windows packaging passed; Linux compiled but its
package job failed the privacy scan on a public-string false positive. Signing
and draft-release jobs were skipped. [#466](https://github.com/OctoSense-org/OctoSense/pull/466)
merged the scanner correction as `9266b008`. The new immutable tag
`desktop-v0.1.0-rc.4` points to that commit (tree
`eade406807c20587801a5bfb2169f454dc4e43f7`); its
[workflow](https://github.com/OctoSense-org/OctoSense/actions/runs/38065627444) later succeeded.
Only the scanner and its tests changed after the `40ca21da` runtime baseline.

The unpublished Mac app ZIP from that run passed 11/11 App Hub/component archive
checks in a fresh profile. ZIP and mounted DMG contents also passed the privacy
scan. The binary has an ad-hoc linker signature, **not** a Developer ID signature
or notarization. The first acceptance attempt failed at its initial screenshot
before any checks; the successful attempt retained its blank first frame, then
used extra read-only observations to see the real consent sheet before refusing
the agent. The unchanged driver completed all 11 checks. No consent was preseeded.

- [Original driver receipt](evidence/rc3-unpublished-macos/receipt.json).
- [Artifact, source, signing and attempt provenance](evidence/rc3-unpublished-macos/source-provenance.json).

These are unchanged copies. Here, `release_archive_tested: true` means the driver
tested an app ZIP; the accompanying provenance explicitly records that the
workflow failed and no release was published. It is **not RC4 acceptance**.
RC4 candidate acceptance and its later publication binding are recorded separately below.

## RC4 Mac candidate (unpublished)

The unsigned Mac Actions candidate from [workflow 38065627444](https://github.com/OctoSense-org/OctoSense/actions/runs/38065627444),
source `9266b008`, passed **11/11 archive checks on the first attempt** in a
fresh profile. The actual consent screen was readable before refusal, both
apps installed through App Hub, and their component calls updated isolated
state and rendered notes. No retries, extra captures, external cache warming
or deadline changes were used. The app ZIP and mounted DMG passed the full
privacy scan: 578 files, 8 patterns. The binary has a linker ad-hoc signature,
with **no Developer ID signature or notarization**.

- [Original driver receipt](evidence/rc4-macos/receipt.json).
- [Candidate provenance and acceptance details](evidence/rc4-macos/candidate-provenance.json).

Both records are copied unchanged. This is an **unpublished Actions candidate**:
`release_archive_tested: true` records the app ZIP transport, not a published
release. The later [publication evidence](#rc4-release-evidence) binds these unchanged records to the final bytes. The
authentic GitHub-attested rehearsal catalog did not modify the public catalog;
no personal account, live model or Home upgrade was involved. RC3's earlier
capture failure and retry evidence remain separate.

## RC4 release evidence

[Desktop RC4](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-rc.4)
uses source `9266b0083544d86bd7636543b5ff60c61b26460f`, tree
`eade406807c20587801a5bfb2169f454dc4e43f7`, from
[workflow 38065627444](https://github.com/OctoSense-org/OctoSense/actions/runs/38065627444).
Published on 10 October 2026 at 18:25:40 UTC as a prerelease, with nine assets.
The unchanged [asset verification](evidence/rc4-macos/verified-assets.json) and
[download provenance](evidence/rc4-macos/download-provenance.json) record full
checksum validation before publication; their draft URLs are historical.
[Public reachability](evidence/rc4-macos/public-download-verification.json)
then passed 9/9: it downloaded `SHA256SUMS` fully and only the first byte of
each other asset, not all public packages in full.
The Mac app ZIP passed **11/11** checks on its first candidate run. Its SHA-256
is `a58fe9efdece0fa6a58c51a5767b368127a9713a1c7048509366427eead2837e`.
The additive [final byte binding](evidence/rc4-macos/final-byte-binding.json)
proves the final ZIP, DMG and package receipt are byte-identical to that candidate;
the UI test was not rerun. Candidate receipts remain unchanged, including their
unpublished status at the time of testing.
[Signing and scan evidence](evidence/rc4-macos/signing-and-scans.json) records
clean package scans for all three platforms and a final scan of 9 artifacts,
1,205 files and 6 patterns. The Mac ZIP and mounted DMG also passed the separate
578-file, 8-pattern scan. macOS has only a linker ad-hoc signature, without
Developer ID or notarization; the Windows installer has no Authenticode
signature. Windows/Linux installed GUI behavior and OS upgrade paths were not
covered by this Mac archive run.

The source/device results above use the development host and isolated Android
fixtures. They did not upgrade installed Home. Windows cache validation at
`a8e170d4`/`af205d9c` is separate: RC4 does not include the later test-only fix,
and source equivalence is not rebuilt-binary equivalence. Live-model reasoning,
real account writes and OpenHarmony device execution remain unverified by
these checks. Shared-component catalog publication is separate from host
compatibility; the attested dry-run entries were not added to the public catalog.
