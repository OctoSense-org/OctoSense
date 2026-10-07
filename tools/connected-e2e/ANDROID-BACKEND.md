# Android backend login acceptance

English | [简体中文](ANDROID-BACKEND.zh-CN.md)

This is a separate debug-package test of a developer backend, not Google login. It uses the actual backend HTML form, one-time authorization code, PKCE, native host callback, token exchange, identity endpoint and Android vault. No logged-in connection or token is injected. The form alone owns the fictional password.

Use the assigned test phone, an existing JDK 17, Android SDK/build tools 35 and the repository's existing Makepad Android toolchain. Do not replace Home or another test app. All output belongs in a new private directory. Commands below use paths supplied by the tester; no private path is required by the source.

1. Build `cargo-makepad` from `.sources/makepad/tools/cargo_makepad`, and export the phone contracts with `phone/android/gradlew --offline --no-daemon :contracts:exportHomeContracts`. Build the APK from `phone/` with `MAKEPAD_FORCE_DEBUGGABLE=1`, `--package-name=dev.makepad.octosense.backendlogin`, `--app-label=OctoSenseBackendLoginTest`, `--abi=aarch64`, and `build -p octosense-home --release --locked --offline --no-default-features --features dev-mode,app-hub,octosense-shell/acceptance-fixtures`. Supply the existing `--sdk-path`. This auth-only build does not bundle a model or kernel.
2. Run `python3 tools/backend-login-fixture.py --directory "$LAB/server"` in a separate terminal. Keep it running. Its metadata contains public local endpoints only. Do not publish the private run directory.
3. Build the ordinary Hub CLI and the `connected-install` shell example. Prepare the exact two test app IDs through the normal signed Store path:

```sh
python3 tools/connected-e2e/android_backend_lab.py prepare \
  --hub "$HUB" --installer "$INSTALLER" \
  --server-metadata "$LAB/server/metadata.json" --out "$LAB/prepared"
python3 tools/connected-e2e/android_backend.py \
  --apk "$INPUT_APK" --profile "$LAB/prepared/apps" \
  --build-tools "$BUILD_TOOLS" --debug-keystore .sources/makepad/tools/cargo_makepad/debug.keystore \
  --out "$LAB/packaged"
python3 tools/connected-e2e/android_backend_lab.py deploy \
  --adb "$ADB" --packaged "$LAB/packaged" --profile "$LAB/prepared/apps" \
  --registrations "$LAB/prepared/backend-registrations.json"
```

`JAVA_HOME` must point to the existing JDK for `apksigner`. For multiple attached devices, supply the assigned `--serial` to deploy; never publish it. Deployment refuses an already installed package. It transfers only the fresh signed fixture profile, configures one ADB loopback reverse, and passes the one-shot `makepad.APP_CONFIG` Intent extra. Android intentionally ignores a startup environment-only app config. A fresh suffix can be used consistently at APK build, packaging and deployment.

The tracked `backend-login/glance.splash` is embedded by the preparation helper. Its publication is subject to the normal app's Glance grant. This source is test artwork and fixture UI, not a production listing.

Native checks, using current visible controls and no input replay:

- Decline unrelated location/media permissions only when the prompt names this test app. Do not touch another app's prompt.
- Choose **Connect in WebView**, review the host consent, then use the real registration/sign-in form with an invented `example.invalid` address and fictional password. Confirm callback completion and **Protected identity**. Never use a personal account.
- Reopen sign-in: check wrong-password rejection, keyboard reachability, native Back and Cancel, same-origin informational navigation, and return to the app. Cancelling must create no account. A second attempt must start with no first-attempt session cookie; check the fixture journal's boolean cookie status, never cookie values.
- Force-stop only this test package and restart it with the same one-shot fixture action/config (the following command is the default package example):

```sh
"$ADB" shell am force-stop dev.makepad.octosense.backendlogin
"$ADB" shell am start -n dev.makepad.octosense.backendlogin/.MakepadApp \
  --es makepad.APP_CONFIG '"{\"test_actions\":[\"backend-auth-fixture\",\"launch-hub:org.octosense.samples.backend\"]}"'
```

 **Load active account** and **Protected identity** must use the persisted native vault connection. **Disconnect** must revoke it. Repeat with the second signed test app to check account isolation; a handle from one app must be refused by the other.
- **Show login in Glance** publishes the signed fixture's card. Open it from the normal Glance page, choose **Sign in from Glance**, test Cancel and a completed form callback. This validates the app/host sheet's Android pause handoff; opening the ordinary app alone does not cover it.

The native auth Activity uses `FLAG_SECURE`; screenshots of that screen are intentionally unavailable. ADB UI hierarchy can establish control labels/bounds and form interactions, but is not original-pixel visual acceptance. Capture only the test app after returning from auth. On this OnePlus Android 15 build, ADB can still return a black image after successful keyboard-bearing sign-in. The existing dev-mode `capture:<private-file>` action reads only the Makepad application texture; pairing its rendered output with an ADB black frame distinguishes app rendering from OS capture. It does not capture the protected native WebView and does not prove what a person saw on the physical display. Do not capture unrelated apps, notifications or keyboard suggestions. Log only event names/status and source/APK hashes; omit account/password/token/state/callback values, device IDs, private paths and ephemeral ports.

Stop the owned server and test package, and remove only its ADB reverse mapping. Retain private failure receipts separately. Setup/build success alone is not a device acceptance pass; the accompanying sanitized result must enumerate which native cases actually completed.

The [2026-10-07 OnePlus result](evidence/backend-android-20261007/README.md) binds the exact APK/source hashes and separates final checks from earlier failures and unrun cases.
