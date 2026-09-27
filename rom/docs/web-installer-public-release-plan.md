# Public OctoSense installer: review and release plan

Reviewed September 23, 2026 against the local working tree based on `b31cbe5`.
This is an assessment and implementation plan, not a completed public release.

Implementation has started under [ADR 0001](adr/0001-public-web-installer.md).
The local developer preview now has complete pre-flash verification,
strict manifest/device checks, serialized operations, a recovery journal and
browser regression tests. See the [installer README](../web-installer/README.md)
for current behavior and limitations. The findings below describe the
**pre-change prototype**; source line numbers refer to that review snapshot.

The browser approach is viable. The existing `web-installer/` is a bench
flasher with useful WebUSB transport, sparse-image splitting, progress output,
and some device/integrity checks. Public release requires a reliable install
transaction and recovery path as well as a clearer interface. The current ROM
also needs hardware qualification: the display patch remains a candidate fix,
and the intermittent blackout has not been proven resolved.

## Product scope

Use **OctoSense Installer** as the public product, reachable from an **Install**
link on the existing website. Start with OnePlus 6 (`enchilada`) only. OnePlus
6T is a separate device target and must not inherit support from its chipset.

Offer three user goals:

1. **Install OctoSense**: supported-device check, preparation, release selection,
   backup/data-erasure review, installation, and first-boot verification.
2. **Update OctoSense**: use the existing signed OTA mechanism for supported
   upgrades, with data preservation conditional on a validated upgrade path.
3. **Recover a phone**: identify its available boot mode and offer a tested
   recovery recipe, explicitly stating when a wipe is necessary.

Read-only diagnostics should be available before a write operation. An
optional **Developer tools** area can later expose temporary kernel boot and
root-related workflows. Root is not a prerequisite for the normal browser
installer: fastboot flashing relies on bootloader authorization. Browser
permissions cannot bypass an OEM/carrier restriction or replace unlocking.

If Magisk support is added, treat it as a separate, explicit workflow using
the correct image for the installed build and patching on that same device.
Do not distribute a generic pre-rooted image as the default install.
[Magisk's installation guide](https://topjohnwu.github.io/Magisk/install.html)
describes why image selection and device-specific patching matter.

## Confirmed implementation gaps

Line references are to the reviewed working tree.

| Priority | Evidence | Required behavior |
| --- | --- | --- |
| P0 | `web-installer/index.html:114`: images **at or above 64 MiB** are checked only by size. A deliberately wrong checksum was accepted in the simulated browser. | Verify the entire contents of every image against authenticated release metadata, including large boot/system/vendor images. Use incremental hashing without loading a multi-GB image into an ArrayBuffer. |
| P0 | `index.html:159` and `scripts/make-manifest.py:8`: eligibility checks only the bootloader product, defaulting to `sdm845`. | A device-specific recipe must establish model/codename, partition layout, required firmware, mode, slot and unlock state. An ambiguous chipset result must block automatic installation. A user selection alone is not hardware verification. |
| P0 | `index.html:160`: Install is enabled for a matching product even with `unlocked=no` or unknown. `:161` also enables Unlock for an unsupported product. | Check all required preconditions and gate every destructive operation, including unlock. Unknown state must not count as permission to write. |
| P0 | `index.html:175`: only Install is disabled during the operation. The simulation accepted a concurrent Reboot; Connect and the wipe checkbox stayed active. | One serialized operation owns the device. Freeze the reviewed device, release, wipe choice and plan. Lock conflicting controls and prevent another tab/session taking ownership. |
| P0 | `make-manifest.py:15` skips missing images; `index.html:177` accepts an empty image list and reports success after reboot. | Versioned manifest schema, required artifact set, partition allowlist, validated URLs/sizes/hashes, and a packaging failure when any required artifact is missing. Reject an empty or malformed manifest before any device command. |
| P0 | `index.html:177` downloads and writes one image at a time. Simulation wrote boot before a later system download failed. | Download and verify the complete required release before the first partition write. Reserve sufficient local browser storage first. |
| P0 | `fastboot.mjs:8544` selects the current slot. The wrapper provides no durable journal or proven rollback procedure. | Use a tested per-device installation/recovery recipe with explicit slot handling. Prefer the existing OTA inactive-slot mechanism for updates. Account for shared partitions such as this device's vbmeta; A/B does not make every operation reversible. |
| P0 | `fastboot.mjs:8283` waits for the next device, explicitly regardless of whether it is the original device. `:8333` replaces the selected USB device on connection. | Bind reconnection to the authorized device and operation. Recheck identity, mode, slot and plan before continuing; never resume a write on another attached phone. |
| P1 | `index.html:132` retries the whole `flashBlob` call. The README claims per-piece retries, but the split loop at `fastboot.mjs:8582` has no such retry handler. | Classify transport versus semantic failures, add timeouts, and specify a safe restart boundary. Do not promise arbitrary chunk resume without proving sparse-image replay behavior. Respect the device's transfer limit rather than unconditionally overriding a private method. |
| P1 | `index.html:187` reports installation complete immediately after sending reboot. | Separate “images written,” “restart requested,” and “Android boot verified.” Verify the running build through authorized ADB when available; otherwise ask for a clear on-phone confirmation and mark verification pending. |

These are observed gaps in the installer, not claims that the simulated run
damaged hardware or that a bootloader would accept every attempted command.
The bootloader may reject a locked-device write; the frontend should prevent
the invalid attempt and explain the required next step.

## Release and availability gaps

Read-only public checks on the review date found:

- The [ROM repository](https://github.com/OctoSense-org/OctoSense-ROM) is public;
  its GitHub API reports `has_pages=false`.
- The [OctoSense website](https://octosense-org.github.io/) responds, but
  `https://octosense-org.github.io/install/` returns HTTP 404.
- Latest public release `20260919-j` contains an OTA ZIP, Home APK and
  `update.json`. It does not contain the web installer's manifest or partition
  image set. That release is not marked as a prerelease.
- `scripts/release.sh` is a Mac bench helper: it fetches from a configured SSH
  build host, creates local symlinks, starts a local server, kills fastboot
  processes and can reboot the attached bench phone. It is not a public
  publication pipeline and must be split into packaging and optional local
  testing commands.
- `scripts/publish-release.sh:79` publishes the OTA assets, then unconditionally
  marks the release latest. Stable, beta and internal-test feeds need distinct
  promotion rules.
- `scripts/build-rom.sh:5` lacks `pipefail`; its build pipeline pipes the build
  into `tail`. A build failure can therefore be masked. Publication must require
  a successful build and artifacts from that exact build, not leftover files.
- `vendor/octosense/octosense.mk:24` conditionally preauthorizes a bench ADB
  key. Its comment excludes this from public releases, but an artifact check
  must enforce that rule. This review has **not** established that a published
  image contains such a key.
- The current CI checks Home/product code but has no browser installer test
  suite or installer deployment job.

## User experience

Use one primary action at each step, with a persistent device/release summary:

```text
Choose Install / Update / Recover
  → Check browser and computer
  → Connect and identify phone
  → Show compatibility and release status
  → Download and verify release
  → Review data changes and unlock if needed
  → Reconnect the same phone and recheck the installation plan
  → Install
  → Verify boot and guide first setup
```

- Show **OnePlus 6**, **Android version**, **release channel**, **download size**
  and expected data changes. Keep partition names and raw logs in expandable
  technical details.
- Provide English and Chinese copy, keyboard navigation, visible focus,
  accessible progress announcements and responsive instructions. The initial
  supported flashing environment should be desktop Chrome/Edge on operating
  systems actually tested; detect WebUSB and secure-context capability.
- Give platform-specific USB guidance: permission denial, cable/port problems,
  Windows driver setup and Linux permissions require different recovery steps.
- Explain data erasure at the point where unlock/format is selected. Never
  advertise a full encrypted-app-data backup unless that backup is actually
  supported and verified.
- Show real download bytes, hash-verification progress, current installation
  step, and elapsed time. Offer pause/cancel only at safe boundaries; an
  in-progress partition write cannot be treated like an ordinary canceled
  download.
- For a disconnected cable, say what completed and how to reconnect the same
  phone. After a page reload, inspect the real device and journal before any
  resumption. Never infer successful writes solely from localStorage.
- In Developer tools, clearly distinguish **Boot once** from **Install
  permanently**. The current display test illustrates why users need to know
  whether a kernel will survive a restart.
- Offer a downloadable, redacted support report containing release ID,
  installer version, operation ID and stage/error information. Keep serials
  private by default and show the report before any optional upload.

The [Android Flash Tool](https://source.android.com/docs/setup/test/flash) and
[GrapheneOS installer](https://grapheneos.org/install/web) provide useful
examples of preparation, reconnect guidance and post-install checks. Their
device support and bootloader-locking procedures must not be assumed to apply
to the OnePlus 6. Relocking must remain unavailable until this exact device,
ROM signing and verified-boot configuration have been tested together.

## Implementation structure

Keep the browser app small and independent from the ROM build environment:

```text
web-installer/
  src/ui/              guided steps and localized user-facing errors
  src/session/         operation state, ownership and recovery journal
  src/transport/       audited/pinned fastboot adapter; optional WebADB
  src/releases/        schema, signature checks, storage and image hashing
  src/devices/         enchilada identification and tested operation recipes
  tests/               simulated USB, interrupted operation and UI tests
```

The device recipe defines required artifacts, firmware prerequisites,
partition/slot rules, unlock and wipe behavior, recovery options and supported
post-install verification. Unsupported or uncertain devices get instructions,
not a force-install escape hatch in the normal flow.

Store downloaded artifacts in browser-managed persistent storage with quota
checks; perform incremental hashing in a worker. Separate verified download
state from device-write state. A signature on the release manifest protects
artifact distribution when the installer and its public key are trusted; it
does not independently protect against a compromised installer JavaScript
origin. Protect that origin and its publishing credentials accordingly.

Use explicit release identity and immutable artifact URLs. The manifest should
carry schema/version, channel, build/source identity, device recipe version,
firmware requirements, image sizes and SHA-256 values, signature/key ID,
allowed upgrade paths, and known issues. Pin the release for an installation
instead of resolving `latest` again midway through it.

## Public hosting and publishing

1. Add `/install/` and `/cn/install/` to the existing website. The guide can
   remain on the marketing site; the USB-enabled app can run on a dedicated
   installer origin with controlled security headers.
2. Host the small web app on HTTPS static hosting. Keep multi-GB ROM artifacts
   in versioned object storage/CDN with tested CORS, range requests and correct
   cache behavior. GitHub Releases can remain a release index or mirror, but
   browser fetch behavior through redirects must be tested from the deployed
   installer origin. Do not put ROM images in the Pages site bundle; GitHub
   [limits published Pages sites to 1 GB](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits).
3. Build from pinned source/dependency revisions. Verify build status, the
   complete artifact set, image hashes, signatures, release identity and absence
   of bench credentials/debug authorizations. Publish provenance and third-party
   notices alongside the supported-device and known-issues pages.
4. Create a draft/beta release, run artifact and deployed-origin download tests,
   then hardware qualification. Promote by channel only after those checks;
   retain prior supported recovery artifacts.
5. Provide a support link and status page. An unavailable release/CDN must stop
   before writing rather than leave a phone with a partially downloaded build.

The browser-to-phone transport is local USB; a user account or cloud relay is
not required for the first public installer. The server distributes software
and metadata. Rooting and publishing users' diagnostic data are not implicit
parts of installing OctoSense.

## Delivery order and release gates

| Phase | Concrete deliverable | Exit requirement |
| --- | --- | --- |
| 1 | Typed manifest/device contracts and serialized install engine, complete verification, reconnection and recovery handling | Every P0 case above has a regression test; incorrect device, signature, image or state produces no write commands. Packaging rejects incomplete builds. |
| 2 | Guided bilingual UI and actionable errors | A new user can complete preparation and recover from permission/download/disconnection failures in simulated browser tests. No success claim before the specified verification stage. |
| 3 | Public website integration, immutable artifacts, channels and reproducible deployment | Deployed-origin smoke tests pass; a known signed release can be downloaded and verified with no developer SSH access, local symlinks or developer credentials. |
| 4 | Opt-in OnePlus 6 beta, then stable promotion | Dedicated test phones pass clean install, update, recovery, interrupted-transfer and reconnect cases on the published host/browser matrix. Display, camera, sleep/wake, basic Wi-Fi and other advertised functions pass hardware qualification. |

For the current display issue, repeat the original failure scenario and test
the relevant framebuffer-removal path; passing ordinary app cycles alone did
not exercise it. A longer mixed-use soak on multiple OnePlus 6 units is also
needed before stable promotion. The `fw-internet` account-specific rejection
should be documented separately from general Wi-Fi qualification; it is not
yet evidence of a radio-driver defect.

## Review validation

`out/web-installer-review/audit.mjs` ran the real HTML in installed headless
Chromium with a simulated `FastbootDevice` and intercepted downloads. All
other browser requests were blocked; no phone commands were issued.
`out/web-installer-review/findings.json` records eight scenarios:

- Small corrupt image: correctly rejected (control case).
- Corrupt image at exactly 64 MiB: accepted and reported installed.
- Locked or unknown unlock state: Install enabled (two scenarios).
- Unsupported product: Install blocked, but Unlock enabled.
- Pending installation: Connect, Reboot and wipe control remain active;
  concurrent Reboot accepted by the frontend.
- Empty manifest: reported installed and requested reboot.
- A later image download fails: boot was already sent to the simulated flasher.

These are diagnostic reproductions of the pre-change implementation, not passing
acceptance tests for a completed fix. Dependency-level reconnect/slot behavior
was reviewed in source; browser and hardware qualification of the replacement
behavior remain implementation work.

Additional primary reference: [Chrome WebUSB capabilities and permissions](https://developer.chrome.com/docs/capabilities/usb).
