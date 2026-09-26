# Home builds and ROM staging

Home is the Rust workspace in `home/`. Run commands below from the
`octosense-rom` root. Builds do not install an APK, flash a phone, change a Home
role or publish a release.

## Prepare source dependencies

```sh
python3 scripts/setup-home.py
python3 scripts/setup-home.py --check --cargo
```

Requires Python 3.9+, Git and Rust stable. The setup script prepares exact
revisions in ignored `.sources/`; it preserves unrelated local modifications.
`home/native-runtime.lock.json` selects the framework release and
`home/native-apps.lock.json` selects the OctoSense-System-Apps source that holds the system script apps (`apps/<name>/bundle/`, chosen by `home/system-apps.json`) and the Mail host service. App Hub
is App Hub's shared shell crate `octosense-app-hub-app` (OctoSense-App-Hub
`crates/app-hub-app`), a git dependency pinned in `home/Cargo.toml` and
`home/Cargo.lock` at the same revision as its backend crates. Its build packs
the system apps named by `OCTOSENSE_SYSTEM_APPS`, which `home/.cargo/config.toml`
sets to `home/system-apps.json`. The AppCard assistant (`octos-app`) is built
from the same pinned OctoSense-System-Apps checkout
(`.sources/system-apps/apps/appcard/app/app`).

The runtime's Makepad (main `1d3d383e`) has the isolate controls App Hub
requires, but not yet the contained script apps of makepad#30, so
`home/runtime-patches.lock.json` names one patch,
`patches/runtime/makepad-contained-apps.patch`. The lock records the exact
patch, its originating Makepad commit, SHA-256 and resulting Git tree; setup
applies it to the pinned checkout and leaves it staged, and `--check` accepts
only that exact tree. `--check` always rejects
staged, unstaged or untracked source changes.
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
scripts/build-home.sh --variant standalone --development \
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
scripts/build-home.sh --variant rom \
  --sdk /path/to/makepad-android \
  --android-sdk /path/to/android-sdk \
  --gradle-home /path/to/gradle-8.11.1 \
  --java-home /path/to/full-jdk \
  --sign-key /private/rom-keys/platform.pk8 \
  --sign-cert /private/rom-keys/platform.x509.pem
python3 scripts/stage-home.py
scripts/stage-forks.sh /path/to/lineage-tree
```

`stage-forks.sh` retains its existing reset of previously staged SystemUI and
Quickstep files in the OS tree. Run it only on the designated build tree with no
active OS build or unrelated edits in those paths. It now takes Home's sources
from this checkout, and no longer accepts a separate launcher checkout.
The Linux OS build still uses `scripts/build-rom.sh` / `run-rom-rootfs.sh` and
the supported LineageOS/device/vendor inputs; see [flashing](flashing.md).

Each APK build exports `OctoSenseHome.apk`, `OctoSenseBridge.apk` and `build.json`
under `out/home/standalone/` or `out/home/rom/`. Home and Bridge are signed
together and their certificate digests must match. The receipt records source
revision/dirty state, runtime/patch/native-app inputs, APK hashes and certificate
digests. Existing application IDs, signature guards and platform imports are
unchanged. The build does not create or migrate signing keys.

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

The same `home/` workspace also builds a normal OpenHarmony application. It
links the native modules and App Hub in process, because a phone cannot spawn
the desktop catalog's Cargo binaries. This does not grant Android's Home role,
replace the HarmonyOS system launcher, or make an Android ROM flashable on a
Huawei device.

Use an existing DevEco installation, compatible `cargo-makepad`, and an existing
device-authorized signing profile. Export the existing DevEco `signingConfigs`
array into a private JSON file outside the repository:

```sh
python3 scripts/build-home-ohos.py \
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
It then applies the product's `home/ohos/EntryAbility.ets` window policy:
HarmonyOS reserves the native status and navigation bars, and Home draws a
draggable floating ball over hosted content. Tapping it opens a compact panel
with **返回首页** and **最近应用**. Dragging docks it inside the nearest side;
tapping outside dismisses the panel without activating the content underneath.
The ball stays in the app window and reserves no content height.
The generated ArkTS bridge also receives `home/ohos/keyboard.patch` to coalesce
per-frame keyboard requests and serialize attach/show/hide while leaving the
pinned framework checkout intact. OpenHarmony uses only its native keyboard.
OpenHarmony does not recognize shell edge swipes or draw a second navigation
pill; those gestures remain available to the host OS. Home paging and the
central pull for the app library still work inside the content area.
Artifacts and source/hash receipts go to `out/home/ohos/`. The optional
`--remote-port` enables app-owned loopback inspection for validation and writes
to `out/home/ohos-validation/`; omit it for the normal package. Existing warnings
remain. Android device acceptance still requires an ordinary Android phone.
