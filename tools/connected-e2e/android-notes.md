# Notes on a separate Android test app

English | [简体中文](android-notes.zh-CN.md)

The OnePlus 6 check uses **OctoSenseNotesTest**, package
`dev.makepad.octosense.connectednotes`. It keeps the installed Home, Mail and
personal accounts separate. This is local Markdown acceptance, not live GitHub
OAuth or repository-write acceptance. No provider credentials are copied.

Prepare the pinned sources with `python3 tools/setup.py`, then follow the
[Home build prerequisites](../../rom/docs/home-build.md) for the Android SDK,
full JDK 17, Gradle and pinned Android octos kernel. The commands below were run
with task-private absolute paths substituted for the variables. `NOTES_LAB`
must be a new private directory; do not reuse a normal OctoSense profile.

Export `JAVA_HOME`, `ANDROID_HOME`, `ANDROID_SDK_ROOT`,
`OCTOSENSE_GRADLE_HOME`, `NOTES_KERNEL` and `NOTES_LAB` for that setup. From the
repository root, first export the Java contracts:

```sh
(cd phone/android && ./gradlew --no-daemon :contracts:exportHomeContracts)
cargo build --release \
  --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml \
  --target-dir "$NOTES_LAB/packager"
```

Do not skip the contracts export: the first recorded build failed Java
compilation without it. Build the pinned packager from this checkout: its binary
stores the source-checkout path and reads the Java files during packaging.
Rerun APK packaging after runtime Java patches. From `phone/`:

```sh
MAKEPAD_FORCE_DEBUGGABLE=1 CARGO_BUILD_JOBS=4 \
MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=$NOTES_KERNEL" \
"$NOTES_LAB/packager/release/cargo-makepad" makepad android \
  --sdk-path="$ANDROID_HOME" --abi=aarch64 --version-code=2026100618 \
  --package-name=dev.makepad.octosense.connectednotes \
  --app-label=OctoSenseNotesTest \
  build -p octosense-home --release --locked --features dev-mode
```

From the root, use the [acceptance installer](README.md) to create a signed
private catalog with only the unchanged Notes bundle:

```sh
target/release/examples/connected-install \
  --keep-profile="$NOTES_LAB/apps" \
  ../OctoScript-App-Design-Flow/examples/connected-apps/github-notes/bundle
python3 tools/connected-e2e/android_notes.py \
  --apk phone/target/android/makepad-android-apk/octosense_home/apk/octo_sense_notes_test.apk \
  --profile "$NOTES_LAB/apps" \
  --build-tools "$ANDROID_HOME/build-tools/35.0.0" \
  --debug-keystore .sources/makepad/tools/cargo_makepad/debug.keystore \
  --out "$NOTES_LAB/packaged"
```

The packager checks the separate debuggable package and the Notes signed-install
receipt, refusing profiles with an existing `.host` directory. It retains only
typed public receipt fields and adds Android's supported
[debug startup wrapper](https://developer.android.com/ndk/guides/wrap-script),
aligns and signs the APK with the existing development key, and verifies its
signature. The wrapper sets only the app-private catalog paths and public trust
anchor. The normal host verifies catalog signatures at launch; the packaging
helper does not replace or bypass that gate. Its receipt describes packaging;
it does not claim device installation or UX success.

On the assigned device, install the result with ADB (`--no-incremental` avoids
the recorded incremental-install failure), copy that fresh `apps` directory to
the test package's `files/apps` using `run-as`, and launch `.MakepadApp` with the
`makepad.APP_CONFIG` string:

```json
{"test_actions":["launch-hub:org.octosense.samples.githubnotes"]}
```

Never install over the regular Home package or transfer an existing personal
profile. The pinned Makepad remote implementation is a no-op on Android;
`MAKEPAD_REMOTE` and ADB port forwarding do not enable it. Phone verification
uses ADB input and original device captures. The Mac soaks use Makepad
instrument and have separate receipts.

Check Write, Markdown, Preview, the actual soft keyboard and Repository/back.
Verify the exact latest `draft-a.json`/`draft-b.json` revision in the **test
package**, then stop only that process, relaunch and check its restored visible
content. Test hardware Enter separately from soft-keyboard Enter: the initial
build lost the active composing word on hardware Enter, which has its own
runtime patch and regression tests. Retain failed captures alongside the
retest; do not infer phone acceptance from desktop checks.
