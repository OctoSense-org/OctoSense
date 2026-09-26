# octos-app

Native desktop client for [Octos](../octos) built on [Makepad](../aichat) with the Splash scripting layer.
Replaces the React frontend (`../octos-web`) for the chat / coding / studio workflows; reuses the
liquid-glass UI, streaming markdown pipeline, diagram/Splash renderers, and font-fallback work
already shipped in `aichat/examples/aichat`.

This directory is a **planning workspace**, not source code. The deliverable is the docs tree below.
Code lands later in a sibling repo (target name: `octos-app`, layout proposed in `01-ARCHITECTURE.md`).

## Why a native client

Octos is moving its interactive surface from REST/SSE to a single JSON-RPC-2.0-over-WebSocket
contract — `octos-ui/v1alpha1`, spec'd at `~/home/octos/api/OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md`,
draft Rust types at `~/home/octos/crates/octos-core/src/ui_protocol.rs`. A native Rust client can
import `octos-core` directly, share the protocol-client layer with `octos-tui`, and skip the
React/Vite/Node toolchain entirely.

Makepad gives us GPU-rendered chat, code, math, mermaid, and diagram-kit views that already work in
`aichat`. Splash gives the model a way to emit interactive UI inline. The combination is a better
fit for an "agentic OS" client than DOM/CSS.

## Docs map

| Doc | What it answers |
|-----|---|
| `00-CHARTER.md` | Vision, in-scope vs. out-of-scope, non-goals, success criteria |
| `01-ARCHITECTURE.md` | Crate layout, layered model, threading, persistence, failure model |
| `02-API-DRIFT.md` | Trust score for `OCTOS_WEB_REST_API.md`, drift list, what to lock vs. test |
| `03-PROTOCOL-CONTRACT.md` | UI Protocol v1 wire summary, reconnect rules, capability flags, M9 gates |
| `04-IA-AND-NAVIGATION.md` | Screen catalog, top-level shell, route map (Splash routes, not URL routes) |
| `05-AICHAT-REUSE-MAP.md` | Per-widget map of what lifts directly from `aichat/examples/aichat` |
| `06-WORKSTREAMS.md` | Master index, dependency DAG, milestone sequencing, agent-swarm parallelism plan |
| `workstreams/W01–W10` | One doc per workstream — scope, owner, deliverables, tests, exit criteria |

Read in order. The workstream docs assume you've internalized 00–05.

## Reference repos

- `~/home/octos/` — Rust backend ("Open Cognitive Tasks Orchestration System"). 91 REST endpoints,
  multi-tenant, multi-LLM. The new contract lives in `crates/octos-core/src/ui_protocol.rs`.
- `~/home/octos-web/` — React+TS+Vite frontend being replaced. Useful as a feature inventory and
  a record of which API endpoints are actually used in practice (~15 of 91).
- `~/home/aichat/` — Makepad fork hosting `examples/aichat`, the desktop chat UI we lift from.
- `~/home/octos/docs/OCTOS_TUI_ARCHITECTURE_2026-04-24.md` — sibling client, same protocol,
  inspires the layered architecture in `01-ARCHITECTURE.md`.

## Status

Drafted 2026-04-28. Reconnaissance complete. Workstream sequencing in `06-WORKSTREAMS.md`.
No code yet.
