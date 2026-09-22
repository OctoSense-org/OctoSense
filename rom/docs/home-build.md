# Home APKs and ROM staging

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
`home/native-apps.lock.json` selects Mail/AppCards and Camera sources. App Hub
client crates are pinned in `home/Cargo.toml` and `home/Cargo.lock`.

The imported Home runtime lacks the isolate controls required by App Hub.
`home/runtime-patches.lock.json` records the exact policy patch, its originating
Makepad commit, SHA-256 and resulting Git tree. Setup applies that patch to the
pinned Makepad checkout and leaves it staged; `--check` accepts only that exact
tree and rejects additional staged, unstaged or untracked source changes.
The separate Makepad/Octoscript repositories are dependencies, not vendored
copies of the launcher. No mobile repository or sibling-worktree name is used.

To compile or test on a desktop:

```sh
cd home
cargo check --locked --workspace --features mobile-apps
cargo test --locked --bin octosense --features mobile-apps
```

## Android builds

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

`publish-release.sh` defaults to the ROM output directory and verifies its
adjacent receipt before including Home in the existing ROM update feed. The
stager and publisher reject standalone/development receipts or changed APKs.
This is a build integrity check, not a replacement for Android's signature
verification. Signer inputs must still be the established ROM identity.

## App Hub behavior and remaining device validation

Both delivery modes link the same App Hub/store and card-host modules. Bundle
installation requires neither root nor Android package installation permission.
Installed apps get distinct launch/focus identities and appear after catalog
changes; native app IDs take precedence. The card host applies declared storage,
network, instruction and memory limits before evaluating downloaded content.
This does not give bundles access to the privileged Android agent or bridge.

The current public catalog is empty. Device acceptance needs a signed fixture,
real HTTPS delivery, install/launch/update/removal, reboot/offline tests and
interrupted-update recovery. The external App Hub installer still replaces a
bundle by deleting then copying; atomic replacement remains a production gate.
No general Android APK store or universal rooted-device support is introduced.

Source/build validation alone does not establish unrooted-device operation or a
new ROM's boot, radios, notifications, Recents, emergency access and OTA recovery.
Run those acceptance checks before retiring the old release path or changing
certificates. Compilation may embed source/resource paths; release artifact
review and the existing private-key publication check remain necessary.
