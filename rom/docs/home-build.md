# Home builds and ROM staging

Home is the `phone/` package (`octosense-home`) of the OctoSense workspace.
Run commands below from the repository root unless noted (the ROM scripts
live in `rom/scripts/`). Builds do not install an APK, flash a phone, change a Home
role or publish a release.

## Prepare source dependencies

```sh
python3 tools/setup.py
python3 tools/setup.py --check --cargo
```

Requires Python 3.9+, Git and Rust stable. The setup script prepares exact
revisions in ignored `.sources/`; it preserves unrelated local modifications.
`native-runtime.lock.json` selects the framework release. The system script
apps (`apps/<name>/bundle/`, chosen by `phone/system-apps.json`) and the Mail
and `llm` host services are in this repository's `apps/`. App Hub
is App Hub's shared shell crate `octosense-app-hub-app` (OctoSense-App-Hub
`crates/app-hub-app`), a git dependency pinned once in the root `Cargo.toml` and
`Cargo.lock` at the same revision as its backend crates. Its build packs
the system apps named by `OCTOSENSE_SYSTEM_APPS`, which `phone/.cargo/config.toml`
sets to `phone/system-apps.json`. The octos kernel service
(`octosense-kernel`, `crates/kernel`, started through the shell's AI
services in `crates/ai-host`) is in every standard build (feature `octos-core`, on by
default and always on for Android, iOS and OpenHarmony). The AppCard
assistant (`octos-app`, `apps/appcard/app/app`) is built
only with `--features app-appcard`: it is not shipped for now, so default,
`mobile-apps` and native mobile builds leave its UI out.

The runtime's Makepad (main `32d6415f`, OctoScript-Makepad `33dea2f1`)
includes the contained-app and isolate controls from makepad#30, the camera
QR scanner AI providers uses (makepad#31), Splash `reapply_text`
(makepad#35) with stateful mini-app inputs (OctoScript-Makepad#46) for Rinx,
self-confirmed assistant tools (makepad#36), the one-call-site Slug text,
cursor and glass shaders (makepad#37, #39) and the in-process terminal
module whose `run` the person confirms (makepad#40, #41).

Rinx (`app-rinx`, in the default and `mobile-apps` builds) is linked as a
native module with `octosense-module` only; CI checks that its standalone
entry and local kernel stay out of every graph. Its assistant is Home's:
the shell's AI services (`octosense-ai-host`, `crates/ai-host`) give each module whose declared `octos.*` services
host policy grants a scoped service from `octosense-app-peers`
(`crates/app-peers`): one octos peer per app and
account, owned by the system agent `_main:api:octosense#system`, on the
shell's kernel. A module without granted assistant services gets no peer. `runtime-patches.lock.json`
records only the Settings overlay, `tools/runtime-patches/makepad-settings.patch`,
for Android input, accessibility and renderer integration.
The lock records the pinned base, patch SHA-256 and resulting Git tree; setup
applies it to the pinned checkout and leaves it staged. `--check` accepts only
that exact tree and rejects additional staged, unstaged or untracked source changes.
The separate Makepad/Octoscript repositories are dependencies, not vendored
copies of the launcher. No mobile repository or sibling-worktree name is used.

To compile or test on a desktop:

```sh
cd home
cargo check --locked --workspace --features mobile-apps
cargo test --locked --bin octosense --features mobile-apps
```

## Android builds

Android and OpenHarmony share app-local floating navigation: tap the ball for
**返回首页** or **最近应用**, drag it to dock on either side, and tap outside to
collapse the panel. It reserves no content height and stays clear of native
gesture edges and the system keyboard. Native applications launched outside
Home retain their own windows; the ball is not a system-wide overlay.

Use an existing cargo-makepad SDK/NDK directory, Android SDK platform 35 with
build-tools 35.0.0, full JDK 17+ and Gradle 8.11.1. The scripts do not install
these tools. Makepad's trimmed JDK may lack Gradle's instrumentation support;
pass a full JDK through `--java-home` or `JAVA_HOME`.

Standalone development pair, signed with the existing Makepad development key:

```sh
rom/scripts/build-home.sh --variant standalone --development \
  --sdk /path/to/makepad-android \
  --android-sdk /path/to/android-sdk \
  --gradle-home /path/to/gradle-8.11.1 \
  --java-home /path/to/full-jdk
```

For a standalone release, replace `--development` with `--sign-key` and
`--sign-cert` pointing to the existing application signer outside the checkout.
Do not use the ROM platform key for ordinary distribution.

ROM Home and Bridge, signed with the existing ROM platform identity:

```sh
rom/scripts/build-home.sh --variant rom \
  --sdk /path/to/makepad-android \
  --android-sdk /path/to/android-sdk \
  --gradle-home /path/to/gradle-8.11.1 \
  --java-home /path/to/full-jdk \
  --sign-key /private/rom-keys/platform.pk8 \
  --sign-cert /private/rom-keys/platform.x509.pem
python3 rom/scripts/stage-home.py
rom/scripts/stage-forks.sh /path/to/lineage-tree
```

A ROM Home is published, so it must not carry the builder's paths. The build
remaps the checkout, `CARGO_HOME` and the home directory out of panic
locations and `file!()` (`--remap-path-prefix`), but every `script_mod!`
compiles in `env!("CARGO_MANIFEST_DIR")`, which no remap reaches. Build a ROM
Home from a checkout and a `CARGO_HOME` outside any home directory (on a Mac,
outside `/Users`, e.g. under `/private/var/tmp`); `--variant rom` refuses to
sign an APK whose native libraries contain `/Users/` or the home directory.

`stage-forks.sh` resets previously staged SystemUI and Quickstep files and the
PermissionController integration paths in the OS tree. Run it only on the designated build tree with no
active OS build or unrelated edits in those paths. It now takes Home's sources
from this checkout, and no longer accepts a separate launcher checkout.
The Linux OS build still uses `scripts/build-rom.sh` / `run-rom-rootfs.sh` and
the supported LineageOS/device/vendor inputs; see [flashing](flashing.md).

Each APK build exports `OctoSenseHome.apk`, `OctoSenseBridge.apk` and `build.json`
under `out/home/standalone/` or `out/home/rom/`. Home and Bridge are signed
together and their certificate digests must match. The receipt records source
revision/dirty state, runtime/patch/native-app inputs, APK hashes and certificate
digests. The default application IDs, signature guards and platform imports are
unchanged; standalone Home can use the test identity described below. The build does not create or migrate signing keys.

**Developer options and a separate test Home.** `--development` selects only
the signing keystore. `--dev-mode` separately compiles the existing Home
`dev-mode` feature, enabling Settings to offer developer options; it does not
activate them or change release optimization. Default builds omit that feature.
Both values are recorded in `build.json`. Developer-enabled builds default to
`out/home/<variant>-dev-mode/`, and ROM staging/publication refuses them.

For testing beside the installed Home, add
`--package-name dev.makepad.octosense.studio` to a standalone build. The wrapper
forwards this identity to the packager and verifies the resulting APK identity.
The default output then has an additional `dev.makepad.octosense.studio/`
subdirectory. Only Home is renamed: the Bridge APK keeps its existing ID and
must not replace the phone's installed Bridge. A test Home does not acquire
ROM services merely by changing its package name; their signature and package
guards remain in force. A ROM build cannot override its Home identity.

These options have unit and dry-run coverage. An APK build and assigned-device
validation are separate checks; [ADR 0006](../../docs/adr/0006-app-studio-on-the-phone.md)
tracks the in-progress studio milestone and its remaining device acceptance.

**The octos kernel.** Every APK carries Home's octos kernel as
`lib/arm64-v8a/liboctos.so` (an Android app may exec only from its native
lib dir). By default the script cross-builds it: it checks out
`https://github.com/octos-org/octos.git` at the revision `Cargo.lock`
pins for `octos-cli` into `.sources/octos`, builds `cargo build --locked
--release --target aarch64-linux-android -p octos-cli --bin octos
--no-default-features --features api,git,ast` with the NDK clang from
`--sdk` (API 33), and hands it to the packager as
`MAKEPAD_ANDROID_EXTRA_LIBS=liboctos.so=<octos>`. `--octos-kernel <path>`
bundles a prebuilt aarch64-linux-android `octos` instead; `--no-octos-kernel`
builds an APK without one (the phone then runs no kernel; the AI providers
are still saved). The receipt records the kernel's source and SHA-256
(`octos_kernel`). The kernel adds about 137 MB (unstripped) to the APK.

**The kernel on a ROM.** A system app that has not been updated never gets its
native libraries extracted: Android loads them from inside the APK, so a ROM
Home would have no kernel file to exec. `stage-home.py` therefore stages Home
without its `lib/` entries and puts `libmakepad.so` and `liboctos.so` in
`vendor/octosense/prebuilt/lib/arm64/`; `vendor/octosense/Android.mk` installs
them into `/system_ext/priv-app/OctoSenseHome/lib/arm64/`, which the package
manager then uses as Home's native library dir (the AOSP layout for system apps
with native code), and `vendor/octosense/config.fs` makes `liboctos.so`
executable (0755). The build re-signs the APK with the platform certificate. A
Home update installed over the system app (`adb install -r`) has its libraries
extracted under `/data/app` as usual. The kernel runs in Home's `platform_app`
domain; `sepolicy/private/octosense_kernel.te` lets it bind its goal
operator-control socket in Home's data dir and keeps three harmless probes (the
linker's config dirs, `/proc/stat`, `/postinstall`) out of the audit log.

Options: `--dry-run` prints the plan; `--offline` uses cached dependencies;
`--version-code` overrides the automatic code; `--output` selects an artifact
directory; `--packager` uses an already built compatible `cargo-makepad` instead
of compiling the pinned packager. Such an override is recorded in the receipt.
Compiling the pinned packager currently fails: it runs `cargo build --locked`
in `.sources/makepad`, which has no `Cargo.lock`. Build it with
`cargo build --release --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml`
and pass `--packager .sources/makepad/target/release/cargo-makepad`.

`publish-release.sh` defaults to the ROM output directory and verifies its
adjacent receipt before including Home in the existing ROM update feed. The
stager and publisher reject standalone/development receipts or changed APKs.
This is a build integrity check, not a replacement for Android's signature
verification. Signer inputs must still be the established ROM identity.

## App Hub behavior and remaining device validation

Both delivery modes link the same App Hub and card-host modules from the
shared `octosense-app-hub-app` crate (OctoSense-App-Hub `crates/app-hub-app`, enabled by the
default `app-hub` feature, which `mobile-apps` includes), built on the pinned
OctoSense-App-Hub backend. Bundle installation requires neither root nor
Android package installation permission.
Installed apps get distinct launch/focus identities and appear after catalog
changes; native app IDs take precedence. The card host applies declared storage,
network, instruction and memory limits before evaluating downloaded content.
This does not give bundles access to the privileged Android agent or bridge.

The current public catalog is empty. Device acceptance needs a signed fixture,
real HTTPS delivery, install/launch/update/removal, reboot/offline tests and
interrupted-update recovery. The backend installer still replaces a bundle by
deleting then copying, so App Hub runs it in a staging root and publishes the
verified bundle by renaming, keeping the previous bundle until the new one is
in place.
No general Android APK store or universal rooted-device support is introduced.

Source/build validation alone does not establish unrooted-device operation or a
new ROM's boot, radios, notifications, Recents, emergency access and OTA recovery.
Run those acceptance checks before retiring the old release path or changing
certificates. Compilation may embed source/resource paths; release artifact
review and the existing private-key publication check remain necessary.

## OpenHarmony Home

The same `phone/` package also builds a normal OpenHarmony application. It
links the native modules and App Hub in process, because a phone cannot spawn
the desktop catalog's Cargo binaries. This does not grant Android's Home role,
replace the HarmonyOS system launcher, or make an Android ROM flashable on a
Huawei device.

Use an existing DevEco installation, compatible `cargo-makepad`, and an existing
device-authorized signing profile. Export the existing DevEco `signingConfigs`
array into a private JSON file outside the repository:

```sh
python3 rom/scripts/build-home-ohos.py \
  --deveco-home /Applications/DevEco-Studio.app/Contents \
  --packager /path/to/cargo-makepad \
  --signing-config /private/home-signing.json \
  --bundle-id dev.makepad.octosense
```

The bundle ID must match the signing profile. The Mate 70 development profile
currently authorizes the existing Home prototype identity
`com.example.myapplication`; device validation explicitly uses that ID. A
production Home identity requires its own profile. The builder never creates
keys or silently substitutes an application's identity.

The builder uses DevEco's existing CMake and Java, resets generated ArkTS files
from the pinned framework template, supplies missing permission descriptions,
and removes signing credentials from the generated project after packaging.
It then applies the product's `phone/ohos/EntryAbility.ets` window policy:
HarmonyOS reserves the native status and navigation bars, and Home draws a
draggable floating ball over hosted content. Tapping it opens a compact panel
with **返回首页** and **最近应用**. Dragging docks it inside the nearest side;
tapping outside dismisses the panel without activating the content underneath.
The ball stays in the app window and reserves no content height.
The generated ArkTS bridge also receives `phone/ohos/keyboard.patch` to coalesce
per-frame keyboard requests and serialize attach/show/hide while leaving the
pinned framework checkout intact. OpenHarmony uses only its native keyboard.
OpenHarmony does not recognize shell edge swipes or draw a second navigation
pill; those gestures remain available to the host OS. Home paging and the
central pull for the app library still work inside the content area.
Artifacts and source/hash receipts go to `out/home/ohos/`. The optional
`--remote-port` enables app-owned loopback inspection for validation and writes
to `out/home/ohos-validation/`; omit it for the normal package. Existing warnings
remain. Android device acceptance still requires an ordinary Android phone.
