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

Each run records source/binary/PNG hashes, snapshots, native logs and a receipt
under `target/connected-notes-e2e/run-*`, including failures. The driver closes
only its owned native processes and removes its temporary profile. Original
PNGs must be inspected separately before visual acceptance. `native.py` drives
Makepad instrument; these are synthetic input events, not physical approval.

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
| OnePlus 6 | [Local Notes device checks passed after the Enter fix](evidence/notes-oneplus-20261006/README.md): soft/hardware input, preview and exact cold recovery. [Reproduction](android-notes.md) uses a separate signed test APK. Phone UI polish and live provider acceptance remain open; shared Google native authorization is not implemented. |
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
installation. Each cycle checks exact persisted Markdown after switching Write,
Markdown and Preview, scrolling/refocusing and returning from Repository. Every
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
Flow: [Calendar](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/feat/connected-sample-apps/examples/connected-apps/google-calendar/ACCEPTANCE.md)
and [Inbox](https://github.com/OctoSense-org/OctoScript-App-Design-Flow/blob/feat/connected-sample-apps/examples/connected-apps/inbox/README.md).
They exercise different hosts: Calendar's provider host and Inbox's full Shell
with actual DeepSeek turns. Do not combine their latency or memory figures into
a single benchmark. The latest [three-bundle signed installation check](evidence/signed-install-after-soak.json)
includes Inbox's corrected monitoring-status bundle; all three passed reopening
and tamper refusal without changing the public catalog.
