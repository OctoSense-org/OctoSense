# Updating OctoSense

English | [简体中文](updater.zh-CN.md)

Open **OctoSense Updates** in the desktop or Home launcher. Android Home also
offers **Settings → Updates → Open OctoSense app updater**. Choose a channel,
then **Check for updates**. Review the offered version and download size, choose
**Download update**, and open the installer separately after verification.

| Channel | Eligible releases |
| --- | --- |
| Stable | Stable releases only. |
| Release candidate | Stable releases and `rc` prereleases; excludes beta, alpha and nightly releases. |
| Preview | Stable releases and all semantic prereleases. |

An installed RC defaults to the release-candidate channel. Versions compare
numerically: `rc.10` follows `rc.9`, and the corresponding stable version follows
its RCs. Switching channels does not permit a downgrade.

This updater is introduced by this source change. Published desktop
`0.1.0-rc.2` and Home `0.1.0-beta.2` do not contain it; install a newer release
containing the updater once to get this interface. Unversioned development
builds do not know their installed release and cannot offer an update.

## Installation

| Platform | Handoff | What the user finishes |
| --- | --- | --- |
| macOS, Apple silicon | Opens the verified `.dmg` | Quit OctoSense, drag the new app into Applications, confirm replacement, then reopen it. |
| Windows, x64 | Starts the verified NSIS `.exe` directly | Follow its prompts; close OctoSense when requested. |
| Linux, x86_64, Debian package | Opens the verified `.deb` with the desktop's package handler | Approve the package installation, then restart. Without a desktop package handler, use the distribution's package manager. |
| Linux, x86_64, AppImage | Opens the download folder | Quit OctoSense, replace the old AppImage, allow execution in its file properties, then reopen it. The updater does not overwrite the running image. |
| Android, ARM64 | Checks the verified APK against installed Home and stages it with Android's package installer | Review and confirm the operating system's installation prompt. |

Opening an installer does **not** report a completed installation. This first
version does not check periodically, silently quit, elevate itself, overwrite
the running application, or remove the user's profile. OS installation and
signature checks still apply.

Android 13 (API 33) or newer is required. Android updates **standalone Home** only. System Bridge and ROM updates remain
separate. The installed package and candidate must have the public standalone
Home signing identity; system-installed Home, ROM signatures, isolated test
packages and other signers are refused. The candidate must have a higher Android
version code and a compatible minimum Android version. If Android has not
allowed OctoSense to install packages, the user can open its permission settings,
return, and explicitly try installation again. No automatic installation resumes
after changing that permission. An APK is never installed by first uninstalling
Home to bypass a signer mismatch.

## Verification and privacy

The host reads public releases from `OctoSense-org/OctoSense` on GitHub. Desktop
selects `desktop-v…` tags; Home selects `home-v…` tags. Semantic version and the
exact platform asset name decide the offer; release publication order does not.

Both GitHub's SHA-256 asset digests and `SHA256SUMS` are required. The updater
checks the checksum file's own digest, requires its installer hash to match
GitHub's metadata, then verifies the complete downloaded installer and declared
size. Downloads use private staging and are rechecked before OS handoff.
Malformed metadata, unexpected URLs, symlinks, incomplete or modified files are
refused. This trusts the project's GitHub release account over HTTPS; a checksum
is not an independent publisher signature.

Desktop downloads live in the shell's `updates/` directory (`OCTOSENSE_HOME` if
set, otherwise `~/.octosense/updates`). Android uses app-private cache under
`octosense-updates/`. Checking releases needs no OctoSense, GitHub or Google
login. No mail, app contents, model credentials or agent conversations are sent.
The updater is a native host interface, not an App Hub app or an agent tool.

## Code and release identity

- [`crates/updater`](../crates/updater/src/lib.rs) owns channels, release
  selection, bounded downloads, cancellation and verified artifacts.
- [`crates/updater-ui`](../crates/updater-ui/src/lib.rs) owns the native module
  and explicit user actions. Network work and hashing run off the drawing thread.
- [`install.rs`](../crates/updater-ui/src/install.rs) uses literal arguments
  for desktop installer handoff; release data never becomes a shell command.
- Android's native extension independently checks package, version, signer and
  bytes before staging a session that requires OS user confirmation.
- [`native-apps.json`](../native-apps.json) registers the module in desktop and
  Home, without an agent update tool.

Cargo's common `0.1.0` package version is not the installed release identity.
Packaging embeds `OCTOSENSE_RELEASE_TAG` instead. The
[desktop packager](../desktop/scripts/package.py) derives it from an explicit
`--version-from-tag` or `--version`. Tagged `--skip-build` packaging requires a
matching tag and binary SHA-256 receipt from an earlier build, preventing a stale
binary from being relabeled. Generic builds clear inherited tags. The
desktop build and packager explicitly select the native Rust target; the binary
and identity receipt come from `target/<host-triple>/release/`. Inherited
`CARGO_TARGET_DIR`, `CARGO_BUILD_TARGET` or Cargo configuration cannot redirect
the build while leaving packaging to read a stale `target/release` executable.
The [Home builder](../rom/scripts/build-home.py) sets the tag only for a versioned,
signed standalone build; development and ROM builds leave it empty.

## What we learned from OctosCode

The native OctosCode app at `d47fade94498` documents a manual RC download; it
does not contain the CLI updater. The separate
[`octoscode update` CLI](https://github.com/octos-org/octoscode/blob/8722b551a7ee653c2457f1152a636160351b9b1f/src/cmd/update.rs)
uses `axoupdater` for receipt-owned cargo-dist installations and defers to
Homebrew, npm or Cargo for installations they own. Its prerelease option resolves
an exact channel tag and tests numeric RC ordering. OctoSense follows those
ownership and explicit-channel principles while handing complete app packages
to the native installer. Unlike that CLI's explicit prerelease channel switch,
this interface does not offer a stable-to-RC downgrade.

The [octos kernel updater](https://github.com/octos-org/octos/blob/084baa522a508cf3af7d7ce9585868ac710eb1d9/crates/octos-services/src/updater.rs)
backs up and replaces a bounded set of CLI binaries with rollback. It is not a
full Makepad app-bundle or Android installer. OctoSense also requires checksums
even for older releases, rather than allowing the kernel updater's legacy
missing-checksum fallback.

## Validation scope

Desktop handoff tests check executable names and literal arguments without
launching installers. Packaging tests verify identities, stale-binary refusal
and build planning without producing release packages.

A hidden native macOS UI checked the live RC1 → RC2 offer and downloaded and
verified its 297 MB disk image. A deliberate change to that isolated download
made **Install** reject it before any installer opened. Channel changes and
download cancellation were exercised; the actual desktop shell also opened the
Updates module and checked the live RC2 offer with an isolated profile.
Replacing the installed macOS app remains **unverified**, as do real Windows
and Linux installations.

On an owned API 35 Android emulator, a fixture using the actual Java updater
adapter and OS PackageInstaller passed 25 acceptance checks, including cleanup: permission changes
did not replay installation automatically; wrong signatures, packages, hashes,
version codes and minimum SDK levels were rejected; the visible OS confirmation
could be cancelled and retried; version code 1000 → 1001 retained the app UID
and synthetic app data, with status recovery after replacement. This exercised
the native adapter, **not** the complete Rust Home binary. The physical phone
was untouched; end-to-end installation from a released Home build remains
**unverified**.
