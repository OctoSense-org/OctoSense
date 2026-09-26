# Android build with AppCard's framework, buildtool and bundled kernel

OctoSense on the phone now runs the AppCard assistant module (`apps/appcard`,
which mounts `octos-app` from the pinned OctoSense-System-Apps checkout,
`../OctoSense-System-Apps/apps/appcard/app/app`) on the makepad fork's AppCard
framework line and needs three things the stock `cargo makepad` build does not
give it:

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
   nativeLibraryDir, so the kernel must ship as a "library"; the hosted app
   (`octos-app`'s `stdio_spawn`) finds it there and runs `octos serve --stdio`
   with `HOME=<files>/octos-home`.

## Step by step

```sh
# 0. Paths (adjust): a checkout of the fork, a checkout of octos-org/octos at
#    the rev octos-app pins for every octos crate (18fcd3f1, see
#    OctoSense-System-Apps apps/appcard/app/Cargo.toml), and the Android
#    toolchain dir that `cargo makepad android install-toolchain` produced.
FORK=/path/to/makepad-fork          # https://github.com/OctoSense-org/makepad.git
OCTOS=/path/to/octos                # https://github.com/octos-org/octos.git at 18fcd3f16e527d2b601d7ae244f056fb711bb6b8
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
( cd "$OCTOS" && cargo build --release --target aarch64-linux-android \
    -p octos-cli --bin octos --features api,git,ast )
KERNEL="$OCTOS/target/aarch64-linux-android/release/octos"   # ~130 MB

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
"$ADB" logcat -d -s Makepad | grep -E 'stdio:|kernel'          # the kernel's start
```

When the AppCard tile starts (the module delivers `Event::Startup` to the
hosted app on first contact), the app's agent spawns the kernel over stdio
and logs one of

- `stdio: octos=…/lib/arm64/liboctos.so HOME=…/files/octos-home` — the bundled
  kernel was found and started; the agent's `client_hello` follows;
- `stdio: bundled octos not found under …; using WebSocket transport` — the
  APK was built without `MAKEPAD_ANDROID_EXTRA_LIBS`; the app shows its
  login screen.

The kernel is a `kill_on_drop` child of the agent's runtime: the module's
`shutdown` stops the agent when the tile's instance is torn down, and the
child goes with it. The module spawns no kernel of its own.

On a desktop the app spawns a local kernel only when BOTH `OCTOS_APP_CORE_BIN`
(the `octos` binary) and `OCTOS_APP_CORE_DIR` (its data dir; the app runs
`serve --stdio --data-dir <dir> --config <dir>/config.json` with
`OCTOS_HOME=<dir>`) are set; otherwise it uses the WebSocket transport / login
screen, so a developer's own `octos serve` is never touched.

## Provisioning the LLM key

The quoting-safe way is `scripts/provision-appcard-llm.sh <family> <model> <key>`: it builds the JSON, ships it through both shells intact, restarts OctoSense with the extra, and waits for the app to log `provisioned LLM`. GLM keys come in two families: `zhipu` for a bigmodel.cn key (OpenAI-style endpoint) and `zai` for a z.ai key (Anthropic-style endpoint); using the wrong one yields HTTP 401 even with a valid key.


The kernel needs an LLM profile before a request can run. The buildtool
activity forwards launch-intent extras prefixed `makepad.` to the app's
environment, and the app reads `MAKEPAD_PROVISION_CONFIG` on startup and
writes the profile into the kernel's config:

```sh
"$ADB" shell "am start -S -n dev.makepad.octosense/.MakepadApp \
  --es makepad.PROVISION_CONFIG '{\"llm_family\":\"zai\",\"llm_model\":\"glm-5.2\",\"llm_key\":\"<key>\"}'"
"$ADB" logcat -d -s Makepad | grep -iE 'provision|turn/start|turn FAILED'
```

`provisioned LLM from intent: …` in logcat is the proof the profile landed;
the next request goes to the provider (a wrong key fails the turn with the
provider's auth error, not with `profile '_main' is not configured`).

## What the tile shows

The whole app: its sessions and composer over the bundled kernel (the login
screen when there is none). A request typed into the composer — or submitted
through the module's `ask` tool — is routed by the app's brain and its card
rendered by the L0 pipeline in a Splash isolate. With location granted, the
buildtool activity feeds the device's fix to `makepad_platform::gps`, and a
card whose place is blank has the engine reverse-geocode the fix
(photon.komoot.io), so it names the real place. Live values log
`Script data fetch: issuing …` / `loaded …` in logcat.

Two things learned verifying this on a OnePlus 6T:

- A Splash isolate is minted without the framework's `sys`/`agent` engine
  (`widgets::script_mod` installs it into the main VM only, and octos-app's
  `register_script_mods` does not install it), so it is NOT in scope inside
  a card's isolate. The module installs it with
  `register_splash_isolate_mod(register_agent_module)`; without that the
  body fails with "variable sys not found" and the live Splash keeps showing
  its previous (empty) view with no log line. The host test
  `appcard_isolates_carry_the_sys_engine_after_register` guards this.
- The phone is one shared device: a `cargo makepad android run` from another
  checkout reinstalls `dev.makepad.octosense` (PackageManager kills the
  running instance: `stop … due to installPackageLI`) and the first launch
  right after an install-over-a-running-app can come up with a black
  surface. Force-stop, then launch, before judging a capture.
