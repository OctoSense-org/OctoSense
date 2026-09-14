# Android build with AppCard's framework, buildtool and bundled kernel

OctoSense on the phone now runs the Octoscript-AppCard module (`apps/appcard`)
on the makepad fork's AppCard framework line and needs three things the stock
`cargo makepad` build does not give it:

1. **The framework pin.** `Cargo.toml`, `apps/reference/Cargo.toml` and
   `apps/appcard/Cargo.toml` pin every makepad crate at the fork's
   `port/appcard-on-octoscript` branch (`ad8f3729d2c24ba5a3bda5c8865a2b8366480147`):
   the octoscript line plus the AppCard framework — the `sys.*` / `agent.*`
   engine installed into every Splash isolate, the fetch layer, `gps.rs`, the
   AppCard widget set, fonts and textures. Nothing to do at build time; it is
   the pinned rev.
2. **The buildtool `cargo-makepad`.** The fork's `port/appcard-on-octoscript-buildtool`
   branch (`1b11c4a395bc35af95162897293cc9ab1d3b791d`) is that framework plus
   AppCard's Java activity: GPS `LocationListener` → `makepad_platform::gps`,
   notifications, share and deep-link intents, WebView bridge, file picker,
   `downloadFile`, `MAKEPAD_ANDROID_EXTRA_LIBS` and `--min-api`. It also honours
   `resources/android/AndroidManifest.xml.template` (this repo ships one:
   AppCard's permissions and intent filters, label OctoSense, package
   `dev.makepad.octosense`, not persistent).
3. **The octos kernel**, cross-built for `aarch64-linux-android` and bundled
   into the APK as `liboctos.so`. Android lets an app exec only from its
   nativeLibraryDir, so the kernel must ship as a "library"; `apps/appcard/src/kernel.rs`
   finds it there and runs `octos serve --stdio` with `HOME=<files>/octos-home`.

## Step by step

```sh
# 0. Paths (adjust): a checkout of the fork and of Octoscript-AppCard (with its
#    `octos` submodule initialised), and the Android toolchain dir that
#    `cargo makepad android install-toolchain` produced earlier.
FORK=/path/to/makepad-fork          # https://github.com/OctoSense-org/makepad.git
APPCARD=/path/to/Octoscript-AppCard # octos submodule at deb433e9 or later
TOOLCHAIN=/path/to/android_33_macos_aarch64   # ndk/, platforms/, build-tools/, platform-tools/, openjdk/

# 1. The buildtool cargo-makepad. It looks for the toolchain relative to its
#    own source tree (tools/cargo_makepad/android_33_<host>), so link it in.
git -C "$FORK" fetch origin 'refs/heads/*:refs/remotes/os/*'
git -C "$FORK" worktree add "$FORK-bt" os/port/appcard-on-octoscript-buildtool
ln -s "$TOOLCHAIN" "$FORK-bt/tools/cargo_makepad/android_33_macos_aarch64"
( cd "$FORK-bt" && cargo build -p cargo-makepad )
CARGO_MAKEPAD="$FORK-bt/target/debug/cargo-makepad"

# 2. The kernel, exactly as AppCard's tools/build-android.sh builds it:
#    NDK clang for API 33, target aarch64-linux-android, features api,git,ast.
NDK=$(ls -d "$TOOLCHAIN"/ndk/* | sort -V | tail -1)
LLVM="$NDK/toolchains/llvm/prebuilt/darwin-x86_64/bin"
rustup target add aarch64-linux-android
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$LLVM/aarch64-linux-android33-clang"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_AR="$LLVM/llvm-ar"
export CC_aarch64_linux_android="$LLVM/aarch64-linux-android33-clang"
export CXX_aarch64_linux_android="$LLVM/aarch64-linux-android33-clang++"
export AR_aarch64_linux_android="$LLVM/llvm-ar"
export RANLIB_aarch64_linux_android="$LLVM/llvm-ranlib"
( cd "$APPCARD/octos" && cargo build --release --target aarch64-linux-android \
    -p octos-cli --bin octos --features api,git,ast )
KERNEL="$APPCARD/octos/target/aarch64-linux-android/release/octos"   # ~130 MB

# 3. Build, install and launch OctoSense with the kernel bundled. From this
#    repository's root, with the phone authorised over adb:
MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=$KERNEL" \
  "$CARGO_MAKEPAD" makepad android run -p octosense --release
```

The exact invocation used for the foundation PR, with the paths of that
session, for the record:

```sh
MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=/private/tmp/claude-505/-Users-user-home-splash/0bdb704b-c531-48c0-b6a5-009881090313/scratchpad/appcard-apk/octos/target/aarch64-linux-android/release/octos" \
  /private/tmp/claude-505/-Users-user-home-splash/0bdb704b-c531-48c0-b6a5-009881090313/scratchpad/bt-1b11c4a/target/debug/cargo-makepad \
  makepad android run -p octosense --release
```

## Checking the result

```sh
ADB="$TOOLCHAIN/platform-tools/adb"
"$ADB" shell pidof dev.makepad.octosense                       # the app is up
"$ADB" shell pm path dev.makepad.octosense                     # the installed APK...
unzip -l target/makepad-android-apk/octosense/apk/octosense.apk | grep liboctos   # ...carries the kernel
"$ADB" logcat -d | grep -E 'kernel:'                           # the probe's verdict
```

The AppCard tile's `create` probes the kernel: it spawns `liboctos.so serve
--stdio`, waits two seconds, and logs one of

- `kernel: ok (alive, silent until client_hello) [pid=… bin=…/lib/arm64/liboctos.so]` —
  the kernel started (it says nothing on stdout until a client speaks);
- `kernel: error (no liboctos.so under …)` — the APK was built without
  `MAKEPAD_ANDROID_EXTRA_LIBS`;
- `kernel: error (exited with Some(n): …)` — it started and died; the stderr
  excerpt says why.

The child is killed when the tile's instance is torn down. Nothing talks to it
yet — the transport is the shell track's job. `OCTOSENSE_APPCARD_KERNEL=0`
in the environment skips the probe (the host's module tests set it).

On a desktop the same code looks for `octos` on `PATH` (or `$OCTOS_BIN`) and
gives it `HOME=~/.octosense/appcard/octos-home`, so a developer's own
`octos serve` and its data-dir lock are never touched.

## What the tile shows

With location granted, the buildtool activity feeds the device's fix to
`makepad_platform::gps`; the module then leaves the card's place blank and
the engine reverse-geocodes the fix (photon.komoot.io), so the card names the
real place — a road or a district at city-scale rounding. Without a fix it
shows `DEFAULT_PLACE` (Cupertino) through open-meteo's gazetteer, as Phase A
did. Both paths log `Script data fetch: issuing …` / `loaded …` in logcat.

Two things learned verifying this on a OnePlus 6T:

- A Splash isolate strips injected globals when it is minted, so the
  framework's `sys`/`agent` engine (installed by `widgets::script_mod` into
  the main VM) is NOT in scope inside the card's isolate. The module
  re-installs it with `register_splash_isolate_mod(register_agent_module)`;
  without that the body fails with "variable sys not found" and the live
  Splash keeps showing its previous (empty) view with no log line. The host
  test `appcard_body_validates_in_an_isolate_with_the_engine_installed`
  guards this.
- The phone is one shared device: a `cargo makepad android run` from another
  checkout reinstalls `dev.makepad.octosense` (PackageManager kills the
  running instance: `stop … due to installPackageLI`) and the first launch
  right after an install-over-a-running-app can come up with a black
  surface. Force-stop, then launch, before judging a capture.
