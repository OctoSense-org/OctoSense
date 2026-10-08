# Architecture decision records

English | [简体中文](README.zh-CN.md)

Decisions for the OctoSense repository: the shell, its services, the system apps and the desktop, phone and ROM packagings. New ADRs go here, numbered after the last one in this table.

How these decisions fit together in the code on `main`, and which parts are still planned: [OctoSense architecture](../architecture.md).

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-one-octosense-repository.md) | One OctoSense repository for the shell, its services, the system apps and both packagings | Accepted |
| [0002](0002-event-driven-app-agents.md) | Event-driven app agents: apps think on their own triggers and publish cards to the glance screen | Proposed; partly implemented |
| [0003](0003-shared-octos-client-access.md) | Talk to Octos: one kernel for native and external clients (opt-in) | Implemented; Android unverified |
| [0004](0004-native-apps-hosting-and-peers.md) | Native apps, app agents and cross-app work: one manifest, hosting per target, an agent for every app, approvals by the person | Implemented; three plan items open |
| [0005](0005-app-contract.md) | The app contract: one small, versioned interface between App Hub and every app | Implemented |
| [0006](0006-app-studio-on-the-phone.md) | App Studio on the phone | Accepted |
| [0007](0007-composable-mail-action-cards.md) | Composable Mail cards with editing, chat and approved actions | Implementation in progress; phone acceptance pending |
| [0008](0008-quiet-android-mail-jobs.md) | Quiet Android Mail jobs and native card notifications | Implemented in this change; device acceptance in progress |
| [0010](0010-shared-oauth-and-connected-apps.md) | Shared OAuth and independently installed connected apps | Implementation in progress; live sign-in passed on macOS; GitHub writes, Gmail sends and device acceptance pending |
| [0012](0012-app-host-api-discovery.md) | Discoverable host APIs, signed backend operations and live script tools | Implemented in source; contract 1.6.0 published; host release and phone acceptance pending |

## Home (phone shell) decisions, 2026-09-16 to 2026-09-25

Written in OctoSense-ROM (retired; merged into this repository) `home/docs/adr/` and kept here as history under [`home/`](home/); on 2026-09-28 Home 0002 and 0004 gained dated amendments and Home 0004's implementation-status line and last Consequences bullet were updated; on 2026-10-04 Home 0001 and 0002 gained dated notes. Cite them as "Home ADR 0004"; inside them "ADR 000N" means a Home ADR, except in those 2026-09-28 additions (this repository's ADR 0004). Paths are relative to the old `home/` or belong to other repositories: `src/` → `crates/shell/src/` (Settings: `phone/src/`), `resources/` and `android/` → `phone/resources/` and `phone/android/`, `octosense-rom/` → `rom/`, OctoSense-System-Apps `apps/` → `apps/` (ADR 0001); `crates/app-policy` and `crates/app-hub-app` are App Hub's; `apps/calendar/cards/` was Octoscript-AppCard's. Their status is as they recorded it.

| Home ADR | Title | Date | Status |
| --- | --- | --- | --- |
| [0001](home/0001-hybrid-android-launcher-and-system-bridge.md) | Hybrid Android launcher and system bridge | 2026-09-16 | Accepted |
| [0002](home/0002-agentic-app-security-model.md) | Agentic app security model | 2026-09-19 | Proposed |
| [0003](home/0003-app-hub-and-store.md) | The app hub, its signatures, and the store app | 2026-09-19 | Proposed |
| [0004](home/0004-system-apps-are-contained-script-apps.md) | First-party system apps ship as contained script apps | 2026-09-25 | Proposed |
| [0005](home/0005-settings-octoscript-controller.md) | Settings application logic in Octoscript | 2026-09-25 | Implemented in source; emulator acceptance pending |
| [0006](home/0006-builtin-settings.md) | Built-in OctoSense Settings | 2026-09-24 | Accepted; full replacement in progress |

## ROM image decisions

The OnePlus 6 image and its delivery have their own records in [`rom/docs/adr/`](../../rom/docs/adr/README.md): 0001 public browser installer, 0002 shared phone themes.

## Elsewhere

- The app-agent broker (`crates/app-peers`) follows Rinx [ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md) (host-owned octos app peers).
- The App Hub, its catalog and the admission gate: [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub).
