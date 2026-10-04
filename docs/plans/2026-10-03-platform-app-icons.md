# Platform app icons implementation plan

**Goal:** Use OctoSense's existing eight-petal green mark in installed mobile apps and desktop builds.

**Architecture:** Extend the deterministic, standard-library icon generator in `desktop/packaging/make_icons.py`. Keep desktop release artwork intact, generate assets at the paths the pinned Makepad packager reads, and overlay OpenHarmony's template icons in the product build wrapper. Commit generated assets so normal builds require no image tools.

**Tech stack:** Python, PNG/ICO/ICNS, Android resources, Apple asset catalogs, Cargo environment configuration.

## Tasks

1. Add packaging-contract regressions in `tools/test_app_icons.py`: Android manifest/resource resolution and density sizes, iOS catalog slot dimensions and RGB opacity, desktop icon discovery and environment paths. Add an OpenHarmony staging test in `rom/tests/test_home_build.py`. Run these before implementing to reproduce missing branding.
2. Extend `desktop/packaging/make_icons.py` with mobile rendering, adaptive/themed Android XML, correctly sized iPhone/iPad/App Store PNGs, desktop ICNS and per-crate discovery assets, and a non-mutating `--check`. Preserve existing desktop output bytes.
3. Wire `phone/resources/android/AndroidManifest.xml.template`, `.cargo/config.toml`, and `rom/scripts/build-home-ohos.py` to the assets. Keep framework pins unchanged.
4. Verify generated files, compile Android resources with the installed SDK and iOS catalogs with `actool`, and inspect representative images. Run existing desktop packaging and ROM tests, tools tests and the repository graph checks; check both desktop and Home Cargo builds after environment changes.
5. Document regeneration, coverage and limitations in both languages of the desktop and phone READMEs. Review the diff and commit on `feat/platform-app-icons`.

## Design decisions

- Reuse the established website/desktop mark rather than introduce a second logo.
- Use opaque square mobile art with platform-applied masks; keep the desktop tile's transparent inset.
- Supply a complete iOS catalog instead of Makepad's fallback that reuses a 1024 image for differently sized slots.
- Supply Android legacy density PNGs, adaptive foreground/background and Android 13 monochrome art.
- Use the existing OpenHarmony build wrapper's product overlay rather than change the pinned framework.
- Initial implementation required no device installation. The user subsequently authorized a fresh installation on the connected Android; its evidence is recorded below.

## Verification record

- Baseline: desktop packaging 10 tests and Home packaging 20 tests pass.
- Regression: the three platform-icon tests failed before implementation (missing manifest references, catalogs and Cargo environment); the OpenHarmony staging test failed for the missing product overlay. All four now pass.
- All 113 tools tests, 10 desktop packaging tests and 21 Home packaging tests pass. `make_icons.py --check` verifies 74 generated assets; the existing desktop release PNG/ICO bytes are unchanged.
- Android SDK 33 `aapt package` compiled both actual product manifests with the framework/product resource overlays, and `aapt dump badging` resolved the application icon. This validates resources, not a complete APK or installation.
- Xcode `actool` compiled the iOS catalog to `Assets.car` and produced iPhone/iPad primary-icon metadata. It exited successfully despite sandbox CoreSimulator-service warnings. `iconutil` decoded the generated ICNS family.
- Desktop `cargo check --locked -p octosense` passes with and without `--features mobile-apps`; Home `cargo check --locked -p octosense-home --features mobile-apps` passes from `phone/`. Both shell graph checks, `tools/setup.py --check --cargo`, and `tools/native_apps.py --check` pass.
- Full ROM suite: all 95 tests pass with `ANDROID_SDK_ROOT` and `ANDROID_HOME` pointing to the prepared Android 33 SDK. Initially, the Settings-routing test selected the first sorted SDK jar (Android 28 on this host), which lacks four Settings symbols. The initial failure reproduced on unchanged main d93217cb and was resolved solely by selecting the current SDK; no unrelated code or tests changed.
- All 28 local-CI runner tests pass after adding icon checks to the desktop and phone workflows.
- Independent read-only packaging review found no actionable issues. Python syntax checks and whitespace checks pass.
- iOS/OpenHarmony installed-device appearance, Android themed-icon mode, a complete OpenHarmony HAP build, and native Windows/Linux packaging remain **unverified**.

## Android installation follow-up

- Device: Pixel 7 Pro, Android 17, arm64. Fresh development installation of source commit `5fabcc57` as `dev.makepad.octosense.icontest`, label `OctoSense Icons`, version code 1.
- The user independently confirmed the device result. The test label came solely from `--app-label='OctoSense Icons'`; production naming metadata is unchanged from the base branch: Android/iOS/desktop use `OctoSense`, and OpenHarmony retains `OctoSense Home`.
- Built from `phone/` with its system-app selection and the pinned Makepad packager. Included the octos kernel at `056173e85b150e387805fc307fe231064ac1ed35`, verified against its build receipt. The APK passed `apksigner verify` and Android reported installation success.
- APK SHA-256: `70e7d218227d0f2f8af5b754b0f1dbcd3153f099eed50b2aaf444b4603a9dc17`.
- Android App info visibly renders the green eight-petal adaptive icon. The app launches and renders Home. Declined the first-run location request; the captured process log contains no fatal exception, fatal signal, panic or ANR marker.
- Existing `dev.makepad.octosense` and its data were preserved; Pixel Launcher remains the default Home.
- Local evidence is under ignored `target/app-icons-validation/`: `android-installed-icon.png`, `android-first-launch.png`, `android-startup.log` and `android-build.log`. The APK is `target/android/makepad-android-apk/octosense_home/apk/octo_senseicons.apk`.
