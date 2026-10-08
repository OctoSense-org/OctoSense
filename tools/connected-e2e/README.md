# Connected app native acceptance

English | [简体中文](README.zh-CN.md)

These drivers install the unchanged public sample bundle through a private,
ephemerally signed App Hub catalog and run its verified installed copy with the
real widget, storage, account authorization, service dispatch and review sheet.
Only the provider HTTP transport and credential vault are synthetic. No real
provider registration, token, repository or calendar is used.

From the OctoSense repository:

```sh
cargo build --locked --release -p octosense-shell \
  --features mobile-apps,acceptance-fixtures \
  --example connected-app-host --example connected-install \
  --example connected-inbox-e2e
python3 tools/connected-e2e/notes.py \
  --bundle ../OctoScript-App-Design-Flow/examples/connected-apps/github-notes/bundle
python3 ../OctoScript-App-Design-Flow/examples/connected-apps/google-calendar/scripts/verify-installed.py \
  --host target/release/examples/connected-app-host
```

`notes.py` creates a new temporary profile, signs a **copy** of the stamped
bundle with memory-only keys, installs it using `Store.install_staged`, then
opens it through `prepare_launch` and `validate_prepared_launch`. Restart reuses
that exact private catalog/profile. `connected_support` refuses an existing
nonempty installation root. Source bundles are not signed or changed in place.

A manual isolated launch uses:

```sh
target/release/examples/connected-install \
  --keep-profile=/absolute/new-empty-test-root/apps \
  ../OctoScript-App-Design-Flow/examples/connected-apps/github-notes/bundle
MAKEPAD_HIDE_WINDOWS=1 target/release/examples/connected-app-host \
  --installed-app=org.octosense.samples.githubnotes \
  --app-data=/absolute/new-empty-test-root/apps --provider-fixture=github --remote
```

The `acceptance-fixtures` feature is non-default. `--provider-fixture` cannot
turn it on in an ordinary binary. Native fixture registration requires an
isolated, marked profile with empty provider registration and synthetic account
metadata. It refuses ordinary profiles. The exact-root in-memory dependencies
do not apply to another app-data directory. Provider fixtures do not change
service capability, account, app identity or review checks. Do not copy GitHub/Google provider credentials into these fixture profiles.
A real model journey uses a separately configured private kernel profile; that
configuration is never public evidence.

Notes covers selected accounts, Unicode edit/preview, restart, paginated and
empty repositories, directory/second-file selection, dirty replacement refusal,
exact host review/cancellation, existing and new-file commits, SHA conflict,
lost response with no automatic retry, and offline restart. Four explicit
synthetic write attempts are checked against the provider journal. The final
lost-response case deliberately models a provider commit whose response was
lost; the app must keep the dirty draft until the user reconciles it.
These write checks predate [#356](https://github.com/OctoSense-org/OctoSense/pull/356).
On `main`, the native review accepts **Approve & Save** only from a physical
press, so the driver's synthetic clicks cannot approve a commit there. This
follows from the code; the driver has not been run on `main`.

Each run records source/binary/PNG hashes, snapshots, native logs and a receipt
under `target/connected-notes-e2e/run-*`, including failures. The driver closes
only its owned native processes and removes its temporary profile. Original
PNGs must be inspected separately before visual acceptance. `native.py` drives
Makepad instrument; these are synthetic input events, not a physical press.

This small host does not boot the production agent kernel, event collector or
Glance. Inbox/Calendar shell journeys use `connected-inbox-e2e`, which embeds the
real Shell. Neither provider fixture proves live OAuth, provider delivery,
Android, Windows, Linux, public catalog publication or a human approval tap.

## Platform evidence (2026-10-06)

| Check | Result and boundary |
| --- | --- |
| Final macOS unit/build checks | [48 OAuth tests, 953 shell tests, desktop/Home builds and both source-graph checks passed](evidence/final-local-checks.json). The separately ignored real-vault test was explicitly run and passed too. |
| macOS OS credential adapter | [One explicit Keychain test passed](evidence/macos-vault.json): store/reopen/logical revocation and no plaintext credential in profile files. No live OAuth. |
| Linux protocols and host adapter | [43 protocol tests and host compilation passed](evidence/linux-provider.json). The explicit vault test failed because the build host had no usable unlocked Secret Service. No GUI/display was available. |
| Windows | [Cross-compile attempt blocked before the host crate](evidence/windows-unverified.json) by missing Windows SDK headers on macOS. Native execution remains unverified. |
| OnePlus 6 | [Local Notes device checks passed after the Enter fix](evidence/notes-oneplus-20261006/README.md): soft/hardware input, preview and exact cold recovery. [Reproduction](android-notes.md) uses a separate signed test APK. The [Rinx writer update](evidence/notes-rinx-phone-20261006/README.md) verifies icon controls and hides floating navigation while typing. Live provider acceptance remains open; shared Google native authorization is not implemented. |
| Post-soak fixes | [954 shell tests and all desktop/Home build/graph checks passed](evidence/glance-modal-validation.json) for exclusive host-review rendering. The later Java-only Enter fix passed [96 ROM tests, 18 setup tests and source-stack validation](evidence/android-enter-validation.json), then the separate phone retest above. |

For shell unit tests that instantiate optional native apps, set `RINX_DATA_DIR`
to a fresh private absolute directory before starting the process. Rinx caches
its root on first access; changing it later cannot isolate an already running
process. Do not run acceptance against the developer's normal Matrix profile.
Do not export raw shell logs or model profiles as public evidence.

## Repeated Notes UX soak

```sh
python3 tools/connected-e2e/notes_soak.py \
  --bundle ../OctoScript-App-Design-Flow/examples/connected-apps/github-notes/bundle \
  --cycles 36 --duration-seconds 600
```

The soak alternates short and long Unicode documents in an isolated signed
installation. The driver targets icon widget IDs: Source, Preview and the
secondary Block editor in the style palette. Each cycle checks exact persisted
Markdown after switching modes, scrolling/refocusing and returning from Repository. Every
sixth cycle edits the native rich input, undoes that edit, opens the exact host
review and cancels. Timed idle windows verify that callbacks do not change the
draft; the final process restart must restore the same note and destination.
The provider journal must contain no write attempts.

Receipts keep every cycle's content hash, persisted revision, active duration,
idle duration and process RSS, plus p50/p95/max operation timing. These timings
are Makepad instrument round trips with native frame waits and polling, not FPS
or measured display latency. Memory trends cover a finite session and include
editor history, renderer caches and native screenshot allocations; they do not
alone establish a leak or prove its absence. Original PNGs and first failures
remain under `target/connected-notes-soak/run-*` for separate visual review.

The [2026-10-06 recorded Notes soak](evidence/notes-soak-20261006/README.md)
passed 36 cycles over ten minutes and a separate 120-cycle burst. It records
exact draft retention, native pixel review, timing boundaries and memory growth.

Calendar and Inbox have their own reusable soaks and evidence in App Design
Flow: [Calendar](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/examples/connected-apps/google-calendar/ACCEPTANCE.md)
and [Inbox](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/main/examples/connected-apps/inbox/README.md).
They exercise different hosts: Calendar's provider host and Inbox's full Shell
with actual DeepSeek turns. Do not combine their latency or memory figures into
a single benchmark. The latest [three-bundle signed installation check](evidence/signed-install-after-soak.json)
includes Inbox's corrected monitoring-status bundle; all three passed reopening
and tamper refusal without changing the public catalog.

## Rinx writer and rejected-draft recovery

The [original native reference](evidence/rinx-writer-20261006/README.md) compares
Rinx's real article writer with the reusable editor at matching wide/narrow sizes.
The new editor removes the outer Notes button rows; the back/file and paper-plane
icons call repository settings and exact host review. All provider gates remain.

```sh
python3 tools/connected-e2e/notes_recovery.py \
  --bundle ../OctoScript-App-Design-Flow/examples/connected-apps/github-notes/bundle
```

This native test seeds a fictional saved draft exceeding the Rinx parser limit,
reopens it through signed installation, refuses an editable blank replacement,
and checks that explicit valid-file replacement preserves the exact recovery
copy. No provider write occurs. Normal Notes admission/provider and soak drivers
remain the commands above.

Android developer-backend login has its own isolated package and real-form procedure: [Android backend acceptance](ANDROID-BACKEND.md). It does not authorize Google embedded login.

## Native backend browser acceptance (2026-10-08)

`backend_login.py` uses a real host, browser and operating-system credential
vault. Only the backend HTTP server and its fictional users are synthetic; no
account or token is injected into the host. Linux and Windows use the supported
external-browser route with an ephemeral loopback callback. Embedded WebReader
acceptance is a separate test and does not establish backend sign-in.

The [Linux receipt](evidence/linux-backend-native.json) records seven successful
behaviors: browser registration/sign-in and PKCE callback, protected identity,
rotated refresh recovery, native restart, logout, repeat login and isolation
between two signed apps. MiniBrowser/WebKitGTK 2.52.6 and native Secret Service
ran in an isolated display/session; all owned processes and fictional
connections were cleaned up. The receipt binds the actual binary and source
hashes. Its archived Cargo lock predates the later Wasm integration; the 35
other fixture/OAuth source files match this acceptance change. This is not an
exact-head whole-shell build claim.

Build the host/installer as above and the pinned Hub CLI with
`cargo build --locked --release -p octosense-app-hub --bin hub`. With an existing
isolated WebKitWebDriver, invoke `backend_login.py` with `--binary`, `--installer`,
`--hub`, `--webdriver http://127.0.0.1:PORT` and a new private `--out` directory.
The Linux run used that interface with the real engine and normal host vault.
`--chrome PATH` instead uses an existing Chromium browser through Playwright.
The `Platform accounts` workflow exercises the Windows native vault and this
same browser/callback journey using the runner's existing Edge installation.
The vault job passed; the added Windows browser journey is pending execution.

The acceptance host copies its own consent-sheet link into a private test
file, and the harness opens that exact URL in a fresh browser. **The OS default
browser link click is not tested.** Synthetic Makepad clicks cannot approve a
business write; these tests do not prove a physical press or OS-authenticated
approval. The disposable fictional account is not Google/GitHub provider
acceptance. Keep the entire run directory private: raw callback URLs and
fictional tokens are not suitable for public artifacts. Publish only reviewed
receipts.

The Linux test did not install packages systemwide. Its disposable bwrap mount
namespace overlays the owned extracted browser/vault package trees onto `/usr`,
read-only. Therefore WebDriver's MiniBrowser path resolves to the extracted
engine, not another system browser. The receipt includes both engine hashes.
Linux/Windows account, read and local-draft methods are now advertised in runtime
discovery; native write-review methods retain their physical-approval platform
limits. `auth.backend.request` supports declared GETs there, while mutations
still require the separately available native approval. Google authorization on
Android remains unsupported by the current provider adapter; the OnePlus 6 test
device has no Play Services. A method's platform support does not configure an
OAuth client or approve a remote write.
