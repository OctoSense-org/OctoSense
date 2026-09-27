# Native OctoSense SystemUI build

**Build completed; phone replacement blocked by incompatible release signing.**

The user requested an OctoSense replacement for the remaining system interface,
after the global notification/controls panel had been deployed. This work builds
a native fork of the ROM-matched SystemUI package. It keeps the package/process
`com.android.systemui`, shared identity `android.uid.systemui`, and Android's
existing keyguard, navigation, notification, privacy, emergency and Recents
services. The installed global panel and normal Home/bridge remain the current
phone interface until a compatible native build can be deployed.

## Implementation

`android/platform-build/stage-systemui.py` applies an exact, reviewed transform
to framework revision `ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b`.
It adds the `OctoSenseSystemUI` platform application target, preserving the
upstream target for its test dependencies. The candidate includes the native
SystemUI implementation and an OctoSense interface layer:

- Light/dark OctoSense surfaces and accents for native quick settings and dialogs,
  a coordinated power-menu palette, status-bar clock typography, and new Home
  and Recents glyphs. Native navigation hit targets, long presses, accessibility
  behavior, contrast and RTL handling remain owned by Android's controllers.
- A wallpaper-aware OctoSense brand label within the lock-screen clock container,
  retaining the original clock/media IDs and the container's movement. Existing
  credential, biometric, emergency and lockout handling is unchanged.
- An OctoSense native Quick Settings tile and a device-controls activity, started
  through `ActivityStarter.postStartActivityDismissingKeyguard`. The activity is
  not exported, never shows above keyguard, and closes on screen-off.
- Observed connectivity, media volume, manual brightness and rotation. Automatic
  brightness remains automatic; the page directs that case to Display settings.
  Settings observers and a network callback refresh the page while resumed and
  detach while paused. Network setup, pairing, hotspot, VPN, battery, display,
  sound, DND, accessibility and OctoSense permission setup have fixed routes.
  The page reads no SSIDs, credentials or notification content.

Native SystemUI notifications and system dialogs keep their existing service
implementations and security decisions. The current Quickstep-owned global
panel can coexist with this candidate; turning off that panel exposes the
native fork's own quick settings and notifications. This initial fork does not
establish that every modern/Compose SystemUI surface has been redesigned.

## Build and source controls

The existing Linux build host and installed root filesystem compile the APK with
`lineage_gsi_arm64-bp1a-userdebug`, API 35, APK-only dexpreopt configuration and the
same bounded resources used for native Quickstep. This produces an APK, not a
OnePlus ROM image. No software or new build host was installed for this work.

The build checks all 1,145 installed-ROM project identities and revisions, the
existing Soong memory-control patch, the exact Quickstep staging, and every
SystemUI input/staged file. Unrelated framework changes are rejected. Staging
and compilation use the shared platform build lock. Evidence, original installed
APK, manifests, generated source and build receipts are under
`target/android/adr-0001-artifacts/systemui-native/`.

The first wrapper attempt stopped on an unset optional variable in Android's
`envsetup.sh`. The corrected wrapper turns off nounset before sourcing that
script; its failed attempt is retained separately. No APK from that attempt was
deployed.

The corrected native build completes all 1,128 actions in 811 seconds with zero
OOM events. Its development-signed APK SHA-256 is
`417fe0765cfd43f8450a73ef9a126a46586ff43fe016bf481127314b78b8610b`.
`native-build/systemui-result.json` and `native-build/source-verification.json`
record compilation and source verification. Upstream deprecation/compiler
warnings remain in the native log.

The final APK inspection passes against all 92 installed components and all 204
requested permissions, with no additions/removals to that permission set.
`packaging-checks.json` verifies 16 KB alignment and actual DEX definitions for
SystemUIService, KeyguardViewMediator, NavigationBarView, OverviewProxyService,
the new tile and device activity. `module-gate/module-result.json` confirms the
release-signer mismatch is rejected and no deployment ZIP is created.
`phone-preserved.json` verifies the original SystemUI APK/running process,
Home/Recents and everyday bridge permissions remain intact after preview cleanup.

`android/systemui-preview` compiles the **same device-controls Java source and
resources** under the separate application ID
`dev.makepad.octosense.systemuipreview`. It has public network-observation/audio
permissions only, no shared UID or signature privilege. The preview is a
temporary UI test, not a SystemUI replacement. Its brightness/rotation controls
must be disabled without settings access. The phone harness restores original
volume and uninstalls the preview in cleanup. It does not test native keyguard,
status/navigation bars, power dialogs or SystemUI process startup.

The preview build and lint pass with zero errors and six packaging/unused-resource
warnings (some shared resources are consumed only by native SystemUI).
`preview-ui/results.json` passes nine phone checks: disabled ungranted controls,
real media-volume changes and exact restoration, Internet/Wi-Fi/Bluetooth/setup
destinations, activity restart and Close. Cleanup removes the preview and restores
the original volume of 6. No additional phone permission was granted.

## Signing and deployment boundary

The phone's installed SystemUI APK SHA-256 is
`cf934c396775e84e10354fc787b40f756ce7abecd079b72558f8eebbc0ab55f7`.
Its verified LineageOS release certificate is
`59988fff31e2f85fbaddc5b37704be97d1c5b7db72a4fb2ed5f07b58ccf20ccf`.
The available platform development certificate is
`c8a2e9bccf597c2fb6dc66bee293fc13f2fc47ec77bc6b2b0d52c11f51192ab8`.
They do not match. A development-signed APK cannot be substituted into this
existing shared system identity as a compatible update.

`inspect-systemui-apk.py` compares the built APK with the retained installed APK:
identity, SDK, persistent/direct-boot application, existing components and their
export/permission boundaries, permission sets, private device page, native marker,
DEX and verified signing certificate. `--require-installed-signer` rejects a
mismatch with exit status 2. A packaging pass is not runtime proof.
`package-systemui-module.py` applies that gate before creating any module archive,
then scopes a compatible archive to the exact original APK, framework hash and
OnePlus ROM. It provides no signing-bypass option. A rejected build produces a
blocked receipt and no installable ZIP. The compatible-signature packaging branch
still requires its own validation before future deployment.

Deployment requires either signing compatible with the installed release, or a
separately reviewed, consistently signed device ROM migration. This work does
not bypass package signature verification, remove the installed SystemUI, alter
its shared UID or grant new privileges to ordinary OctoSense apps. A full ROM
migration, including its data/recovery implications, is outside this APK build.

Before a compatible build can replace the phone's SystemUI, preserve the exact
original APK/module state and validate rollback, boot, unlock/emergency access,
notifications/replies, app navigation, IME, power/volume dialogs, privacy indicators,
user/display changes and the Quickstep connection on the target device.
The existing [global panel recovery](systemui-shade-replacement.md#deployment-and-recovery)
still applies to the currently installed launcher integration.
