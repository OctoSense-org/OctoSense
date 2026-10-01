# Architecture decision records

English | [简体中文](README.zh-CN.md)

Decisions for the OctoSense repository: the shell, its services, the system apps and the desktop, phone and ROM packagings. New ADRs go here, numbered after the last one in this table.

How these decisions fit together in the code on `main`, and which parts are still planned: [OctoSense architecture](../architecture.md).

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-one-octosense-repository.md) | One OctoSense repository for the shell, its services, the system apps and both packagings | Accepted |
| [0002](0002-event-driven-app-agents.md) | Event-driven app agents: apps think on their own triggers and publish cards to the glance screen | Proposed |
| [0003](0003-shared-octos-client-access.md) | Talk to Octos: one kernel for native and external clients (opt-in) | Implemented; Android unverified |
| [0004](0004-native-apps-hosting-and-peers.md) | Native apps, app agents and cross-app work: one manifest, hosting per target, an agent for every app, approvals by the person | Implemented |
| [0005](0005-app-contract.md) | The app contract: one small, versioned interface between App Hub and every app | Implemented |

## Home (phone shell) decisions, 2026-09-16 to 2026-09-25

Written in OctoSense-ROM (retired; merged into this repository) `home/docs/adr/` before the repositories merged, and kept here unchanged as history under [`home/`](home/). They keep their own numbers; cite them as "Home ADR 0004". Where one names a path such as `home/src/` or `home/apps/`, read `crates/shell/src/` (the shell; Settings is in `phone/src/`) and `apps/` (see ADR 0001). Their status is as they recorded it.

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
