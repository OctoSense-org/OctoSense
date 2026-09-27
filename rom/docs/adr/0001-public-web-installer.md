# ADR 0001: Public browser installer

- **Date:** 2026-09-23
- **Status:** Accepted
- **Implementation:** First local-preview milestone implemented; public
  release work remains in progress. This does not qualify a public ROM release.
- **Scope:** OctoSense ROM installation, update, recovery and release delivery.
- **Initial device:** OnePlus 6 (`enchilada`); OnePlus 6T requires its own recipe.

## Context

The current single-page WebUSB installer is a useful bench tool. The
[release review](../web-installer-public-release-plan.md) reproduced acceptance
of corrupt 64 MiB images, success with an empty manifest, conflicting controls
during installation, and a partial write before a later download failure.
The bundled fastboot library also automatically accepts another device on
reconnection. A public installer must prevent these failures and explain how
to recover from interruptions.

The existing website and OTA releases do not yet provide a public browser
installation path. The current display patch is still under evaluation, so
installer quality alone cannot qualify the ROM as stable.

## Decision

Ship **OctoSense Installer**, with Install, Update and Recover workflows.
Keep temporary kernel boot and optional root tools in a separate developer
area. Bootloader unlocking and Android root are different capabilities; normal
installation must not require Termux, a Linux environment or a root grant.

### Browser architecture

Use small ES modules, with no framework or build step required to serve the
installer. Separate the UI, release contracts, download/storage worker,
installation session and fastboot adapter. Vendor and pin browser runtime
dependencies with licenses and integrity receipts. Never fetch executable
dependencies from a CDN at runtime.

1. Validate a versioned manifest and exact required image set before exposing
   an installation action. Constrain partition names, file locations, sizes,
   hashes and the device recipe. Packaging must fail on incomplete input.
2. Download every required image into origin-private browser storage and
   verify its complete SHA-256 incrementally in a worker before any partition
   write. Check storage quota, response status, size and content; propagate
   download, storage and verification failures without touching partitions.
3. One session serializes operations. Use an origin-wide Web Lock as well as
   in-process guards; freeze the reviewed device, slot, release and data-loss
   choice. Disable conflicting controls while an operation is active.
4. Bind the USB connection to the selected physical device. Disconnection
   invalidates the session; never automatically switch to a newly attached
   device or blindly replay a failed partition write. Recheck identity and
   slot before writes. An uncertain write requires recovery review.
5. Persist write intent and completed steps before proceeding. After reload,
   an unfinished write blocks another ordinary installation. A local journal
   helps diagnosis but does not prove the contents of a phone's partitions.
6. Distinguish image-write completion, restart request and verified Android
   boot. Sending a reboot command must not produce an “installation verified”
   claim.

The OnePlus 6 recipe must identify the device independently of the shared
`sdm845` product string and validate bootloader mode, slot, unlock state and
partition capabilities. Uncertain identity blocks writes. In particular,
selecting “OnePlus 6” in the UI is not sufficient evidence. The first milestone
may remain read-only on real phones until an exact identification route has
been qualified.

### Release trust and distribution

The public installer requires authenticated release metadata, immutable image
URLs, pinned verification keys and explicit channel promotion. A manifest's
own checksum field is not proof of publisher identity. Manifest signing and
key rotation must be implemented before public-origin writes are enabled.
Until then, the unsigned development format is accepted for local preview
only; public origins can present diagnostics but cannot flash.

Publish the web app over HTTPS and ROM artifacts through object storage/CDN
with tested CORS and range behavior. Link from the existing bilingual website.
Keep build/packaging/publication separate from bench commands that reboot
phones or terminate local tools. CI must reject failed builds, incomplete or
stale artifacts, and bench ADB authorizations in public images.

Use the existing signed OTA path for qualified updates. A browser's raw-image
flash is not a generic data-preserving upgrade or rollback mechanism. The
initial local preview supports an explicitly acknowledged fresh installation;
recovery and upgrade recipes are separate milestones. Do not offer bootloader
relocking until this ROM/device/signing combination is validated.

### User experience

Guide users through Connect → Compatibility → Download and verification →
Data-change review → Installation → Boot verification. Provide English and
Chinese instructions, accessible progress, platform-specific USB help and a
redacted support report. Technical partition logs remain expandable.
Developer actions must distinguish **Boot once** from **Install permanently**.

## Alternatives considered

- **Polish the existing page only:** rejected; it preserves the unsafe
  operation sequence and does not address release or recovery integrity.
- **Desktop application first:** deferred. It may be useful for unsupported
  browsers or recovery modes, but adds distribution and update work before
  the supported WebUSB path is qualified.
- **Generic multi-device/root flasher:** deferred. Device-specific identities,
  bootloader constraints and recovery recipes are prerequisites for support.
- **Hash whole images with a single WebCrypto ArrayBuffer:** rejected for the
  production path because multi-GB images create avoidable memory pressure.
  Use a pinned incremental implementation and bounded reads instead.

## Consequences and release gates

The first milestone deliberately fails closed where the previous bench page
guessed compatibility. Legacy unversioned manifests must be regenerated.
Runtime remains a static web application; browser-managed storage and Web
Locks become prerequisites for writing.

Implementation is complete only after:

- Negative manifest, corruption, interruption, conflicting-operation and
  wrong-device tests pass without unintended writes.
- Signed public releases, exact OnePlus 6 identification, recovery and boot
  verification have implementation and hardware evidence.
- The deployed site passes download/permission tests on its supported
  browser/host matrix.
- Dedicated phones pass install/update/recovery and hardware qualification,
  including the previously observed display blackout scenario.

The initial milestone is a developer preview. It is not a stable-ROM release,
an automatic flash of the connected user's phone, or evidence that the display
driver issue has been resolved.

## Implementation record — 2026-09-23

The initial implementation uses flat modules under `web-installer/src/` and
retains static hosting. It adds complete schema validation, incremental
full-image hashing in a worker, OPFS staging, sparse-frame validation, explicit
fresh-install consent, origin/device operation guards, pinned slot writes,
an interruption journal and separate restart/user-confirmed-boot states.
Packaging now requires all images, and `release.sh` no longer performs phone
operations. A dedicated CI workflow runs unit, Chromium and packaging tests.

Local validation on this date passed 66 Node unit/adapter tests, 13 Chromium
integration tests and eight Python packaging tests, plus shell syntax and
diff whitespace checks. Chromium exercised the actual download worker and
OPFS, including corruption in the final byte of a 64 MiB image. These results
do not substitute for physical USB or ROM hardware qualification.

The regression suite uses simulated devices; no physical flashing or
publication was performed for this milestone. Exact bootloader identification,
signed public metadata, firmware qualification, automated recovery, verified
running-build checks, Chinese localization and public distribution remain
required follow-up work. Public-origin writes remain disabled.
