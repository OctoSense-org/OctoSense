# ADR 0004: Native apps, app agents and cross-app work: one manifest, hosting per target, an agent for every app, approvals by the person

- **Date:** 2026-09-28
- **Status:** Accepted (2026-09-28); amended 2026-09-29 (section 6: `host.ask` dropped for octos's `ask_user_question`; sections 10 and 12: `terminal.run` only where the Terminal runs as a process; factual corrections from the gap review, marked where made; sections 4 and 8: interactive app cards; section 6: one shared conversation per app agent, then the same day two parallel lanes that share history; follow-ups from the 2026-09-29 code review; section 4: which apps have an agent, peers prepared at consent, the shell's "Ask <app>" panel); **Implemented** (2026-09-29): every plan step and every gap from the 2026-09-28 review is on `main`, and the full CI (Desktop, Phone, Apps and services, ROM) passed on `74e1d713`. The follow-ups below remain open work, not part of this decision's implementation.
- **Scope:** How OctoSense declares, links, hosts, isolates and trusts its bundled native Rust apps on each target; where every app keeps its data and what its agent may read; how every bundled app, native or script, owns an octos app agent; how the system agent and app agents work across apps; and how the person approves what they do, live or in advance.
- **Relates to:** [ADR 0001](0001-one-octosense-repository.md); [ADR 0002](0002-event-driven-app-agents.md) (app agents; amended by this ADR); [Home ADR 0002](home/0002-agentic-app-security-model.md) and [Home ADR 0004](home/0004-system-apps-are-contained-script-apps.md) (amended); Rinx [ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md); octos UPCR-2026-034 (host-owned app peers, merged), UPCR-2026-035 ([octos#2567](https://github.com/octos-org/octos/pull/2567), host tools per app peer, merged; OctoSense pins its follow-ups, now octos fe08d8e6 with octos#2621, #2626, #2636 and #2644) and UPCR-2026-036 (`serve --host-managed`, [octos#2591](https://github.com/octos-org/octos/pull/2591), merged); [octos#2601](https://github.com/octos-org/octos/pull/2601) (tool origin, per-connection ownership); [ADR 0003](0003-shared-octos-client-access.md) (Talk to Octos, OctoSense [#98](https://github.com/OctoSense-org/OctoSense/pull/98): external clients never reach app agents).

## Context

### Two kinds of bundled app

| | Native app | Script system app |
| --- | --- | --- |
| Today | App Hub (the store and the Card runner), Rinx, Terminal; opt-in Sheets, Reference, AppCard; the native News, Maps and Photos crates once kept "for comparison" were deleted by [#113](https://github.com/OctoSense-org/OctoSense/pull/113) (the script apps are the only versions) | News, Maps, Photos, Camera, Mail, AI providers |
| Source | a crate pinned in the root `Cargo.toml` | `apps/<name>/bundle/`, packed from the same commit (`desktop/system-apps.json`) |
| Code | Rust: full process rights, `unsafe`, files, PTYs, threads | OctoScript in App Hub's Card runner; reaches the shell only through `host.request` for families its manifest was granted |
| Runs | in the shell process, one splash isolate per instance (`crates/shell/src/module_host.rs`); the isolate separates the script heap only | in the shell process, a nested isolate per instance with `mod.res` and `mod.run` stripped |
| A bug can | take the whole shell down | fail inside its isolate |

**Adding a native app takes five hand edits** (the pin in the root `Cargo.toml`, an optional dependency and an `app-<id>` feature in `crates/shell/Cargo.toml`, the default in `desktop/` or `phone/`, a `#[cfg]` line in `linked_modules()`, and `Cargo.lock`), and nothing checks that they agree.

**A native crash is a shell crash.** On 2026-09-27 a bitmap-only font chosen in the terminal made Makepad's font loader panic; the recovery path panicked again and aborted the OctoSense process, taking Rinx with it. Fuzzing then found three more panics on corrupt fonts in the vendored parser and shaper (fixed in the Makepad fork).

**In-process means no boundary, only trust.** An in-process native app shares the shell's memory: its Rust code can read anything the shell holds (the host token for the kernel, other apps' data in memory) and open any file the shell can. Splash isolates do not contain native code.

### Process hosting on the desktop

`Hosting::Process` starts a Makepad app as a child (`--stdin-loop`, `crates/shell/src/clients.rs`) connected to the shell's in-process hub (`hub.rs`); a per-app override switches a module to a process. How its frames reach the compositor (Makepad `platform/src/os`):

| OS | Frames | Needs |
| --- | --- | --- |
| macOS | Metal IOSurface, zero copy | nothing more |
| Windows | D3D11 shared handles | nothing more |
| Linux, Vulkan build | DMA_BUF / OPAQUE_FD, zero copy | `MAKEPAD=vulkan` and a Wayland session (Vulkan windowing panics on X11) |
| Linux, OpenGL build | CPU readback of every frame | works; costly |

`host::processes_available()` is false for native mobile targets and wasm. Mobile GL contexts are per process. Process apps are started with `cargo run` from a checkout, else the sibling binary; release packaging does not ship them yet ([#94](https://github.com/OctoSense-org/OctoSense/pull/94) is open).

### Two AI models

- **Makepad's AI services bus** (`libs/ai/services`; the shell's half is `crates/shell/src/ai_bus.rs`): one central conversation calls typed tools that apps publish with a risk level. Apps offer a narrow API; the central agent holds all context; it runs one way.
- **App agents** (Rinx ADR 0007, ADR 0002): each app owns an octos agent with the app's full context: its workspace, memory namespace, history and tools. The system agent supervises and talks to app agents; it is not limited to an app's tool API.

### What octos provides (octos `main` e6223ef, verified)

Verified at e6223ef when this ADR was written. OctoSense now pins octos fe08d8e6 (the merge of octos#2645; it holds #2644, a running turn shown in the other lane's shared history, and #2636, the parallel person context with shared history; after #2626, the shared peer conversation, #2630, `peer_gather` accepting the kernel's own `result.md`, #2621, `peer/input/reject`, and #2616, the #2567 follow-ups); the corrections dated 2026-09-29 below were checked against c608384d, the pin before it.

- **Peers are sessions, not processes.** One kernel process per shell; every peer is a session in it (tokio tasks) with its own workspace, memory namespace, transcript and model lane. Tools such as the shell run as short child processes in the peer's workspace.
- **No 8-peer ceiling.** `peer/prepare` stages 1–8 peers **per call** (`n`); the only total is a soft cap of 8192 live peer registrations. The model's `peer_handoff` tool is limited to 4 per turn; host-owned app peers are not created through it. OctoSense's broker prepares one peer per call.
- **Master and peer talk through an inbox and a file blackboard.** Master → peer: `brief.md` at creation, then `peer_send_input`, delivered as the peer's next user turn (originator only; in serve through a durable queue drained every 2–5 s, best-effort at-least-once). Peer → master: each turn writes `peers/<slug>/result.md` (+ `result-N.md`, `turns.txt`), read with `peer_gather` / `peer_list`, "the only cross-peer channel". A peer asks with `ask_user_question`; the master answers with `peer_respond`. Depth 1: peers cannot create, steer or close peers.
- **Host-owned app peers** (UPCR-2026-034): owned by the shell's system-agent session; exclusive workspace and memory namespace per (app, account); resumable with a host token; **request contexts** (`peer/context/open`) give an app's clients separate transcripts, folders and child memory namespaces. The system agent may answer an app peer's questions, never approve its tools.
- **Request contexts are fenced** (UPCR-2026-034): a context's workspace is `<peer workspace>/contexts/<id>/`; the peer's own folder is refused as a context workspace, so a context turn cannot read the files beside it. The peer's own session is bound to the peer's folder and reads everything under it, every context's folder included.
- **Closing is final** (UPCR-2026-034): a closed peer cannot be resumed, and it keeps its (app, account) namespace and workspace reservation ("their stores still hold data"), so no replacement can be created for them. octos has no suspend, reclaim or purge.
- **Host tools per app peer** (UPCR-2026-035, #2567, merged): the host registers the app's `tools.json`; the kernel sends every call to the **host's connection** (`peer/tool/call`, carrying `context_id`), the host answers (`peer/tool/result`). A call carries `session_id`, `context_id` (null on the peer's own session) and `turn_id`, and no account or client field. `generic_tools` is an exact list the host sets per peer: a turn keeps exactly the listed kernel tools that the session has (an empty list keeps none, and `ask_user_question` stays when it is listed); with `generic_tools` omitted the peer keeps its whole kernel roster, `ask_user_question` and `shell` included. *(Corrected 2026-09-29: this bullet said a `generic_tools` turn keeps only peer-safe tools and never `ask_user_question`; octos tests `should_offer_exactly_the_generic_tools_the_host_sets`, `should_keep_the_peers_kernel_tools_when_it_registers_an_empty_set` and `should_offer_ask_user_question_on_a_peer_input_turn_when_the_host_lists_it` show otherwise.)* Once-only execution, timeouts, audit, approvals for destructive and outward tools, budgets. The kernel never talks to an app. As agreed on #2567: registration is **additive** (with `generic_tools` omitted, the peer's kernel tools all stay); ownership is **per connection** (the registering connection is the tool host and owns the peer's approvals); host-routed tools have their own origin and are excluded from Talk to Octos external turns (#2601's builtin-only allowlist); and the system agent's input to a host-owned peer is delivered to the host's driving connection as **`peer/input`**.
- **Host-managed serve** (UPCR-2026-036, OctoSense #98): opt-in websocket kernel with a host token and an external token. External clients (a web client, a TUI) get no peer methods and cannot name an app peer's session.

### Gaps

1. **A system-agent turn on an app peer got none of the app's context.** #2567 gives tools, memory and context only to turns driven by the host's connection, and `peer_send_input` used to arrive as a tool-less kernel continuation. Closed on #2567 by `peer/input` (section 6); the shell has to handle it.
2. **App tools are per peer.** An app agent cannot call another app's tool, and the system agent has no app tools; the system agent's turns instead get octos's full default tool set, shell included (section 12). *(Since [#117](https://github.com/OctoSense-org/OctoSense/pull/117), 2026-09-28, the `_main` profile's `tool_policy` denies octos's shell; the rest of the default set remains.)*
3. **Script apps have no agent on `main`.** [#106](https://github.com/OctoSense-org/OctoSense/pull/106) (merged 2026-09-28) adds the `octos` host service, one peer `card.<app id>` per app; it declines tool approvals (the Card runner has no approval sheet yet).
4. **Native apps in their own process cannot reach an agent**: `ai_host::offer` hands the peer to a module in-process only.
5. Only Rinx is granted an agent, in code (`Policy::shipped()`).
6. ~~**Host-driven turns cannot ask questions**~~. *(Withdrawn 2026-09-29: the premise was false. octos keeps `ask_user_question` on host-driven turns: `generic_tools` is an exact list the host chooses, and omitted it keeps the whole roster. What is missing is on the shell's side: the broker does not forward a peer's `user_question/requested` to any conversation yet. See the decision in section 6.)*
7. **Request contexts cannot read their account's data** (the fence above).
8. **An agent cannot be paused or erased**: signing out cannot use `peer_close`, and removing an account or the app cannot free or erase its agent.

## Decision

### 1. Two manifests, one per kind

Script system apps stay in `desktop/system-apps.json` and `phone/system-apps.json`. Native apps are declared only in **`native-apps.json`** at the repository root:

```json
{
  "schema": 1,
  "apps": [
    {
      "id": "terminal",
      "crate": "makepad-terminal",
      "source": { "git": "https://github.com/OctoSense-org/makepad.git", "rev": "<sha>", "local": ".sources/makepad/apps/terminal" },
      "module": "makepad_terminal::TERMINAL_MODULE",
      "bin": "terminal",
      "crate_features": [],
      "hosting": { "macos": "process", "windows": "process", "linux": "process-if-vulkan", "android": "module", "ios": "module", "ohos": "module" },
      "shells": { "desktop": "default", "phone": "off" },
      "sandbox": { "network": "any", "processes": true },
      "storage": { "accounts": false, "agent_workspace": "none", "external": ["home:rw"] },
      "agent": { "octos": [], "tools": null }
    }
  ]
}
```

`tools/native_apps.py` generates every derived place (marked blocks in the root, shell, desktop and phone `Cargo.toml`s; `crates/shell/src/native_apps.rs`, included by `linked_modules()`; the agent grants) and updates `Cargo.lock` for changed pins; `--check` fails CI on drift. It refuses `process` on mobile or wasm, and `process` without `bin`.

**The native News, Maps and Photos crates are deleted** (`apps/{news,maps,photos}/native`, their `app-*` features, their place in `mobile-apps`). The script apps are the only versions.

### 2. Hosting per target

| Target | Native app |
| --- | --- |
| macOS, Windows | own process where its `hosting` says so (for now only the Terminal, below) |
| Linux with a Vulkan build and a Wayland session | own process where its `hosting` says so (for now only the Terminal) |
| Linux without them (OpenGL, X11) | **in-process for every app**, for now |
| Android, iOS, OpenHarmony, wasm | in-process only |

**Where the process apps start (decided on #110).** The **Terminal is the only process app for now.** The other native apps stay in-process on every target until their reason is gone:

- **App Hub stays in-process** because it hosts the Card runner that every script app runs in.
- **Rinx stays in-process** until the peer link (section 5) and the process sandboxes (section 3) exist; it then moves by a reviewed change to its `hosting` in `native-apps.json`.

A process app that dies takes only itself down: its tile shows it closed with a restart; the shell and other apps keep running. Desktop release packages ship each process app's `bin`.

In-process modules (all of mobile, and Linux without Vulkan) must meet the bar a process does not: fuzzed parsing of outside data, no panics on it, and panic containment at the module boundary (a second panic must not abort the shell).

### 3. Trust boundary

- **Native apps are first-party and reviewed only.** A native app is added by a reviewed change to `native-apps.json`. Anything installable from the store is a script app.
- **Process apps on the desktop run under an OS sandbox** built from the manifest's `sandbox` and `storage` (files: the jail, its secrets and `external`; network; child processes): macOS sandbox profile, Linux Landlock and seccomp, Windows AppContainer. The terminal's is necessarily broad; most apps' are narrow.
- **A process app reaches its agent only through the shell.** It never connects to the kernel (an external client is barred from app agents by UPCR-2026-036) and never sees the host token.
- **The shell checks what it relays**: grants and consent; budgets, rate limits and background policy; each tool call's name and arguments against the declared `tools.json`; each result against its declared schema and size; its own audit log; crash cleanup.
- **What no one can check**: what a native app's code does inside its own tool or why it starts a turn. The sandbox and review are the controls.

### 4. Every app can own an app agent

An app that declares `agent.octos` (native) or `octos.*` capabilities (script) and is granted them gets ONE host-owned app peer on the shell's one kernel, per (app, account). No app starts a kernel of its own. **The person consents at first use**: the first time an app asks for its agent, the shell shows what the agent may read and use and where the model runs; Settings lists every app's agent with an off switch.

**Which apps have an agent (amended 2026-09-29).** From the manifests and grants, never from names (`crates/shell/src/agents.rs`, `apps::agent_apps`): a native app whose `native-apps.json` entry grants `agent.octos`; a script app whose manifest declares `octos.*` or an `agent` block, **or whose admitted bundle ships `tools.json`** (App Hub's `AgentBundle::load`). News ships `tools.json` (`news.list`, `news.read`) and declares no `octos.*`: it has an agent. Settings › Assistant › Approvals lists every one of them ("Not asked yet", on, off) before any has asked. An app's own `host.request("octos.*")` stays limited to the services its manifest declares.

**Peers exist once the person allows them.** The first use that asks may be the person opening the app's "Ask <app>" panel, the app's own `octos` call, or the system agent's `agents.ask` (the first-use sheet then shows over the system chat); developer mode asks nobody (section 13), and there is no other bypass. From the moment an agent is allowed (and at startup for the ones allowed before), the shell **prepares its peer**: `peer/prepare`, its tools and `generic_tools` registered (G3), its session open, so the system agent's `peer_list` lists it and `peer_send_input` reaches it without the app ever calling `octos`. A script app's peer is the contained service's `card.<app id>`, one per app, shared by the app, its cards, the panel and the system agent. A native app's peer is its instance's and exists while the app is open (a native app with accounts cannot be prepared without its signed-in account). Turning the agent off releases the peer and its contexts at once; the shell refuses the system agent's input to it (the kernel may still list the retired peer). octos lists only prepared peers and offers no hook for an app whose agent is not allowed, so the shell tells the system agent itself: a short note with the system chat's turn whenever the apps' agents changed (each app, and whether it is allowed, not yet allowed or off), and two host tools on the system session, `agents.list` and `agents.ask`, answered by the shell (registered once a host credential exists, that is, once one app's agent has been prepared).

**"Ask <app>" (the person's lane, drawn by the shell).** For every app with an agent the shell draws a panel with the app's conversation (section 6): it opens a sharing context with `open_conversation` (the contained service's for a script app), shows both lanes merged with their speakers ("You", "System agent", "<App>'s agent", "<App>'s agent, to the system agent"), has a composer whose Send stays available whenever the person's own lane is idle (the lanes are independent, so the person can send while the system agent's turn runs) and whose Stop stops only the person's own running turn; the system agent's running turn has its own row with an explicit "Stop the system agent's task" (`broker::interrupt_lane_where`), never in the Send button's place. It shows and answers the app's own questions (G11), and leaves approvals to the shell's sheets. It opens from the bar's "Ask <app>" beside the focused window's title, Shift+F8, or Setup › Assistant › Ask this app's agent; on a desktop it stands beside the system chat, on a phone it is a full-screen sheet. It is the system chat's pane (`crates/shell/src/system_chat/view.rs`) in its app mode (`crates/shell/src/app_chat/`).

Three flows:

| Flow | Native module | Native app in its own process | Script app |
| --- | --- | --- | --- |
| 1. The app talks to its agent (UI, triggers): the person's lane of its conversation (section 6) | the service injected at `create` (`open_conversation`) | the **peer link** (section 5; `octos.session.open` without a `client`) | `host.request("octos.*")` (#106) |
| 2. The agent uses the app's tools with full context | the shell executes the call in-process | the peer link carries `peer/tool/call` in and the result out | the app's host service executes it |
| 3. The system agent talks to the agent: the system agent's lane, in parallel with flow 1, each lane seeing the other's recent turns | in the kernel; its input reaches the shell as `peer/input` and the shell drives the turn (section 6) | the same, relayed on the peer link | the same |
| 4. The person acts in one of the app's cards (glance screen, notification) | as in the app's UI: the card runs under the app's own policy and its calls take the app's path of flows 1 and 2 (the person's lane) | the same | the Card runner's service gate, as in the app's UI (`host.request` under the manifest's grants, then each host service's own checks) |

**App cards are fully interactive (decided 2026-09-29: "the app card can do everything, for now").** An app's cards on the glance screen and behind notifications run under the publishing app's own policy, the same as the app's UI in the Card runner: typing, buttons, `host.request`, chatting with the app's agent (section 6). For example, the agent drafts an email and publishes it as an editable Mail card; the person edits it and taps Send in the card. A card's calls go through the app's existing path, not the host-tool relay, so the relay's schema checks and budgets (section 3) do not cover them. What the person does in a card is the app's own action (section 8). *(Until 2026-09-29 a glance card was contained: its isolate ran under an empty policy with no host requests, took no input, and the whole tile was one shell tap target that opened the app (`crates/shell/src/glance_card.rs`). Since the branch `feat/interactive-glance-cards`, a tile runs under the publishing app's policy and takes input, an app may publish a `script` card (a Splash program) besides an L0 `source` card, and `glance.publish` with `notify: true` posts a notification that opens the card.)*

The shell is always the kernel's host connection: it holds the host token, registers the app's tools, drives the app peer's turns and routes tool calls.

**Triggers** (a button, a data event, a schedule, a file change) are the app's (ADR 0002 §2). The shell gates each: script triggers arrive as `host.request`, native ones on the peer link or at injection, under the same grants, budgets and background policy.

### 5. The peer link for process-hosted native apps

It rides the process's hub socket as its **own channel**, addressed to that app's peer and never registered with the AI services bus:

- up: `PeerRequest { req_id, method: "octos.session.open" | "octos.turn.start" | …, args }`, `PeerToolResult { call_id, ok, data | error | awaiting_confirmation }`;
- down: `PeerReply { req_id, … }` and streamed turn events, `PeerToolCall { call_id, name, args, risk, confirm_required, timeout_ms, account, context_id, client, caller }`, `PeerToolCancel { call_id }`.

**Identity travels with every call, stamped by the shell.** The socket identifies the app, not who inside it asked. When the app opens a request context (`octos.session.open { account, client }`, where `client` is the app's own label such as a Rinx mini app id), the shell records `context_id → (app, account, client)`. On each `peer/tool/call` it looks up the kernel's `context_id` (null: the agent's own session, `client` null) and the peer's account, and puts them on `PeerToolCall`, with **`caller`**: the app's own agent, or for a cross-app call the calling app or the system agent (section 7). The app never supplies them on a call, and a process may use only contexts it opened. The app checks its own grants against them (section 9), and a `confirm: app` sheet shows the caller (section 8). In-process modules and script apps' host services get the same fields.

The shell stamps the app's identity from the socket, enforces #2567's host obligations for the app (once per call, nothing after cancel, acknowledge before a confirmation sheet), and when the process dies fails its outstanding calls, closes its request contexts and keeps the peer. The client API lives in Makepad's `makepad-ai-services` (Makepad apps cannot depend on OctoSense crates) and serves in-process modules too, so an app does not know how it is hosted.

### 6. The system agent and app agents

- The system agent is the originator of every app peer. It briefs and asks with `peer_send_input`, reads answers on the blackboard, answers an app agent's questions (its `ask_user_question`, below), and never approves an app agent's tools.
- **A turn the system agent starts on an app peer runs with the app's tools, memory and context: the `peer/input` event** (agreed on #2567). The system agent's `peer_send_input` to a host-owned peer is not run as a kernel continuation; octos delivers it to the host's driving connection as `peer/input {peer, session_id, input_id, turn_id, text}`, and the shell starts the turn on that connection (`turn/start` with the kernel's `turn_id`), so it is host-driven: it has the app's tools and memory, and its approvals are raised in the app's conversation. The system agent follows it on the blackboard. If no host connection holds the peer (never registered, or disconnected), octos runs and queues nothing and tells the system agent the app is not connected; the system agent waits for the app or reports the failure visibly.
- **Two parallel lanes that share history, per app agent (decided 2026-09-29).** Each app agent (a host-owned app peer) has two sessions that run **in parallel** and **share history read-only**. *(This supersedes, the same day, the single shared conversation of OctoSense [#166](https://github.com/OctoSense-org/OctoSense/pull/166) and octos [#2626](https://github.com/octos-org/octos/pull/2626): one peer session that the person's turns and the system agent's `peer/input`s both drove, queued one turn at a time. The system agent's long work blocked the person, and the person's chat blocked the system agent.)*
  - **The system agent's lane** is the peer's own session (`…#peer-<slug>`), driven by `peer/input` (the system agent's `peer_send_input`) as before, with its own queue.
  - **The person's lane** is a request context (`…#peerctx-<slug>.<id>`) that the app opens with `share_history` (octos UPCR-2026-034, "Parallel person context with shared history", [#2636](https://github.com/octos-org/octos/pull/2636), pinned e045c727). A native module opens it with `OctosAppService::open_conversation`, a process app with `octos.session.open` over the peer link, and a script app or card with `host.request("octos.session.open")`. No system agent is involved. The broker opens a new context id for every handle (a closed id is never reopened) and starts the person's message there with `origin: {kind: "person", label: <the app's name>}`, or `app` when the app says it started the run itself (G2's `trigger: app | schedule | background | incoming`). G2's trigger stays the authority for approval rules: an unsaid turn is `person` for the label and `Unknown` for rules. A person-said turn keeps its `Person` trigger, so its questions go to the app's conversation. A person's turn waits only for its own context's previous turn.
  - **Shared history, read-only.** At the start of each turn in one lane, the kernel shows the model the other lane's last N user and assistant text rows as a labelled, read-only block: "Recent turns in the system agent's conversation with this app" or "Recent turns in the person's conversation with this app". The defaults are N = 20 (at most 50) within 16 KiB. The block is read under the other session's persist lock, drops tool rows, names each row's speaker and is never written into the reader's transcript. A turn still running in the other lane is shown too, after the finished rows (octos [#2644](https://github.com/octos-org/octos/pull/2644)): its request with its speaker, the text it has streamed so far (`[the app agent] (in progress) …`) and a `[turn status]` line (`still running`, `waiting for approval: <tool>` or `waiting for an answer`, never tool arguments). **No shared transcript**: two writers on one transcript are rejected, because they break tool-call pairing and compaction.
  - **What the system agent sees.** Each of the person's turns also leaves a round on the peer's blackboard (`result-<n>.md` with `origin: person` and `context: <id>`), so `peer_gather` shows it. A person's round alone never fires a fleet synthesis.
  - **Who speaks, and what the app sees.** #166's speaker labels stay. The kernel's `[from the person: Rinx] …` or `[from the system agent] …` marker leads the user message. The app's follower (`OctosContext::subscribe`, or `conversation` frames on the peer link) gets both lanes' events: its own context's and the system agent's turns on the peer session. Each event carries its `lane` (`person` or `system_agent`), its `speaker`, and a user message's `display_text`; a turn's caller hears only its own turn. `octos.session.history` returns both transcripts merged by `persisted_at`, each row with `lane` and, for a marked user message, `speaker` and `display_text`. Approvals and questions keep going to the shell's router and request model, routed by the turn's origin: the person's and the app's turns to the app's conversation, the system agent's to the system chat.
  - **Which API opens which.** `open_conversation` opens the person's lane. `open_context(client)` stays a plain request context, with no shared history, no origin and no blackboard round: Rinx's mini apps, and `octos.session.open` with a `client`.
  - **Nothing blocks a lane for long, and Stop covers both.** #167's deadlines hold in both lanes. An approval or question nobody answers expires at the prompt deadline (section 8), and a turn still running 30 s later is interrupted (its late calls refused); on the peer session, the next queued input then starts. **Stop from the app stops both lanes**: the person's running turn and the system agent's (the person owns the device, and one Stop is the simplest clear behaviour). This is `octos.turn.interrupt` (`ContextOp::Interrupt`) on a conversation, and the reply lists each stopped turn with its lane. "Stop <app>'s agent" on the shell's app-conversation sheet and question card (`broker::interrupt_where`) does the same. The shell's "Ask <app>" panel is different: its Stop is the person's own lane only, and the system agent's turn is stopped only from its own row (see "Ask <app>" above).
  - **Accepted hazards.** The two lanes run at the same time on one peer:
    - File edits are not locked across lanes. Each lane edits its own folder (a context works in `<peer cwd>/contexts/<id>`), and a lane that writes the other's folder can race it.
    - The unknown-outcome marks of host tool calls are shared by both lanes.
    - The peer's token budget is checked at turn start, so two parallel turns can overshoot it slightly.
  - When the host refuses a `peer/input`, the system agent is told so visibly (`peer/input/reject`, octos [#2621](https://github.com/octos-org/octos/pull/2621)).
  - *(Until 2026-09-29 the person's turns ran in request contexts (`peerctx-…` sessions) and the system agent's on the peer's own session, so each was blind to the other's exchanges.)*
- **Questions go through octos's own `ask_user_question` (decided 2026-09-29; there is no `host.ask`).** App agents and the system agent ask with `ask_user_question`, which octos keeps on host-driven turns (`generic_tools` is an exact list the shell sets, and omitted keeps the whole roster). The shell routes each `user_question/requested` to the right conversation: on the app's peer session or one of its request contexts, to the app's own conversation; on a system-agent turn, including a `peer/input` turn the system agent started on an app peer, to the system chat, where the system agent (or the person) answers. It is never an approval: approvals stay with the person (section 8). *(Until 2026-09-29 this section had the shell register a `host.ask` host tool on every app peer, on the premise that octos drops `ask_user_question` from host-driven turns; the premise was false and `host.ask` is dropped. No `host.ask` is registered on `main`; any remaining shell code for it from [#145](https://github.com/OctoSense-org/OctoSense/pull/145) is being removed separately.)*
- **The system agent may also call granted tools of apps directly** (section 7), under its defined tool set (section 12).
- In OctoSense the AI services bus is not the system agent's channel to apps. It stays for upstream Makepad apps.

### 7. Cross-app work

**Cross-app tools, by grant (decided on #110; replaces D1).** App agents **and the system agent** may call another app's tools when the calling app's manifest asks for them and they are granted. This supersedes the earlier rule that the system agent never holds app tools.

- **Granted.** An app declares in `tools.json` which of its tools are shareable; a calling app's manifest asks for others' shareable tools. The shell grants them when a script app is installed; for a native app the grant is part of its reviewed entry in `native-apps.json`. The system agent's grants are part of its defined tool set (section 12).
- **Registered, routed and checked by the shell.** The shell registers each granted tool in the caller's tool list as a host-routed tool (as `mail.send`, marked as Mail's), receives the call like any other, checks the grant on every call and hands it to the owning app's host service or process. It uses #2567's additive registration and host-routed origin, so no granted tool reaches a Talk to Octos external client; it may move further into octos once proven.
- **Authorized by the shell.** A call from app A's agent, or from the system agent, to app B's tool is checked by the shell against the manifest grants; no second, agent-level consent is needed. Outward and destructive tools still follow section 8: a live approval (on the shell's sheet, or on the owning app's own sheet for a `confirm: app` tool), or a standing rule keyed to (owning app, tool), with the calling app shown on the sheet.
- **Who acts.** The system agent plans. A bounded call it is granted (a read such as "what's on my calendar today?", or a single action) it may make directly. Work that needs an app's own context and judgement goes to that app's agent through `peer/input`: "schedule a meeting" goes to Calendar, which calls Mail's `mail.send` for the invitations and records them on the event.
- **Partial failure is reported, not hidden**: which step succeeded, which did not. A call whose outcome is unknown is never retried without the person (octos marks it `outcome_unknown`).
- **Ambiguity is asked**, not guessed (two Edwards, no free slot).
- **Completion** is announced by the system app ("booked; invites sent to 4"); each app's own UI shows the change because its data changed.

### 8. Approvals: by the person, live or in advance

Only the person approves. The system agent never does, and cannot be talked into it: it reads untrusted text, and rules are checked mechanically. Everything in this section holds **outside developer mode**; developer mode overrides every approval, `auto_approvable: false` included (section 13).

- **The shell is the single path and the authority for every tool call (decided on #110).** Every app tool call, whoever makes it (the owning app's own agent, another app's agent or the system agent), goes kernel `peer/tool/call` → the shell's host connection → the owning app's executor; an app's agent has no direct way to call its own tools. So the shell sees, authorizes (grants, budgets), audits and routes every call.
- **Agent tool calls, not the person's own actions (decided 2026-09-29).** The approval router and the shell's sheets govern **agent tool calls** (`peer/tool/call`). **Actions the person takes in an app's own UI, its cards included (section 4), are the app's own actions**: they run under the app's policy and host services' own checks, and the shell draws no extra approval for them. An app can draw something that looks like a shell approval card, but a fake approves nothing: only the shell's host connection answers `approval/respond` (octos approvals are once-only and owned per connection).
- **Hardening deferred (2026-09-29).** OctoSense is at a very early stage; we see how interactive cards work and improve security later. Deferred: a user-gesture check on card actions; letting only the owning app's card act on its tools; a shell-only "Needs you" area; "from <app>" labels on app tiles; a personal anti-phishing mark; filtering mirrored Android notification actions named like approvals; budgets on card-initiated actions.
- **Who draws the sheet.** For a `confirm: host` tool (for example `mail.send`) the shell draws it, in the app's conversation or batched in the system chat, and a standing rule may answer it. For a `confirm: app` tool the shell hands the confirmation to the owning app, which shows its **own sheet** (for Rinx, its send sheet) for callers of every kind; the call carries who is calling (`caller`, with its context; section 5), so the app's sheet shows it. When the owning app is not running, or nobody is present, the call waits or is refused visibly, as octos#2567 specifies. **An agent's own text is never an approval surface.** Each line, on either sheet, shows the **owning app, the tool and the exact arguments**, and for a cross-app call the **calling app** (another app's agent, or the system agent).
- **Live.** Pending approvals surface on a shell-drawn sheet in the app's conversation (on the owning app's own sheet for a `confirm: app` tool), on its card and as a notification; the system agent may batch the `confirm: host` approvals of one request into one shell-drawn sheet in its chat (a plan plus its outward actions, with exact arguments). The shell answers each approval with the person's decision.
- **A deadline, never a silent yes (2026-09-29).** An approval or question the shell holds for an app peer's session or its request contexts expires after 10 minutes (`OCTOSENSE_PROMPT_DEADLINE_SECS` overrides it): the approval router denies it and the request model declines the question (free text saying it expired; octos has no other cancel), both with the reason ("no answer in 10 min"), audited, and the sheet, card or notification turns into "Expired: no answer in 10 min" until dismissed. Nothing is approved on expiry, and an external client's prompts never expire in the shell (octos#2624).
- **In advance: standing approvals.** The person sets rules; the shell's **approval router** evaluates each approval request against them on the exact arguments and answers it, or surfaces it:
  - scope: keyed to **(owning app, tool)**, whoever calls it, so a cross-app call is covered by the owning app's rule and the sheet or notification shows the calling app; with conditions (recipients in contacts or in the thread, no attachments, triggered by the person, amount or count limits) and a daily cap;
  - created in Settings → Assistant → Approvals, or from an approval sheet ("always for people in my contacts", "allow for 1 hour"); the system agent may suggest a rule, only the person creates one;
  - **no "everything, forever" rule**: the broadest rule is time-boxed ("approve everything this app asks for the next hour"), with a visible indicator;
  - excluded always: tools that declare `auto_approvable: false` (permanent deletion, payments, sharing outside the device, account and security changes) and calls whose outcome is unknown;
  - excluded by default: runs started by incoming content (an email or message someone else sent);
  - after the fact: every auto-approval is notified and audited (rule, arguments, time, result); a send queue gives an undo window where the app supports it; one tap turns every rule off.

Octos needs no change: the host already answers approvals, and it answers each one (octos approvals are once-only).

### 9. Rinx

One agent for Rinx and one request context per mini app is enough while mini apps are clients, not agents (Rinx ADR 0007). Because #2567 registers tools per peer, **Rinx checks each tool call's `client` (the mini app, stamped by the shell with its `context_id`, section 5) against that mini app's grants** (rooms, Matrix actions) and shares one budget fairly between contexts. If mini apps ever become autonomous (their own triggers, instructions, model or budget), each needs its own peer, and the shell creates it for Rinx (peers cannot create peers). Rinx's send sheet confirms `rinx.message.send` (`confirm: app`) for every caller and shows who is calling (section 8). Its agent reads the files Rinx writes into its account folder (section 11); the Matrix store and keys live under `secrets/rinx/`, and room data the agent needs beyond those files comes through Rinx's tools, with its per-room checks.

### 10. The terminal

- Desktop: its own process on macOS and Windows, and on Linux with Vulkan and Wayland (in-process otherwise, section 2).
- Its AI may read (`read_screen`, `read_scrollback`) in every hosting, and **type commands only when the Terminal runs as its own sandboxed process**: macOS, Windows, and Linux with Vulkan and Wayland. The in-process Terminal (Linux without Vulkan, phones) is read-only: its module offers only the read tools (Makepad `apps/terminal/src/module.rs`, `read_only_manifest`). *(Decided 2026-09-29; this bullet said "type commands, in every hosting".)* Each typed command goes through the shell with a live approval showing the exact command, and is `auto_approvable: false`, so no standing rule approves it (consistent with makepad#41, which already requires confirmation for each run). Outside developer mode, that is: developer mode runs them without a sheet (section 13).
- An agent for the terminal is optional and later. Its output is untrusted text: anything that types into a shell stays behind the person's confirmation and is `auto_approvable: false`.
- `terminal-ctl`, its control socket for OctoLoop, is off unless the person enables it and is unrelated to the agent.

### 11. App storage: one layout, declared in the manifest, the same for every app

Every app, script or native, stores its data in one host-owned layout, and its manifest declares it. The agent reads the app's files **directly from disk** (its workspace is the app's own account folder); tools remain for actions and for data that needs the app's own permission checks.

```
<octosense home>/apps/<app id>/            the app's jail: App Hub's jail root; a native app's sandbox root
    accounts/<account hash>/               one per signed-in account ("device" when the app has none):
                                            the app's data for that account = that account's agent workspace
    common/                                app data not tied to an account (settings, catalogs)
    cache/                                 evictable, not backed up
<octosense home>/secrets/<app id>/         host-owned: tokens, keys, passwords, encryption stores
```

Rules:

- **The agent's workspace is its account's folder** (`peer/prepare` `cwd`), matching octos's one peer per (app, account) and its memory namespace `app/<app>/acct-<hash>`. An agent never sees another account's folder; octos refuses overlapping workspaces, so this holds by construction. The kernel's file tools read and write there, fenced to it.
- **Request contexts see their own folder only**: octos fences a context to `contexts/<id>/` inside the account folder and refuses the account folder itself. The shell always lets octos choose that default (never an explicit context `cwd`, which octos checks only for being inside the peer's folder). A context turn reads account data through **host read tools** the shell registers on every app peer, `files.list`, `files.read` and `files.search`, executed by the shell over the account folder (never another context's folder, never outside the account folder) and narrowed by the app's per-client grants where it declares them (a Rinx mini app sees its own rooms' exports only). The agent's own session reads the folder directly. We also ask octos for an optional read-only view of the peer's folder for contexts; the host tools stay for per-client narrowing.
- **Secrets are never inside an app's jail.** Script apps reach theirs only through host services (Mail's passwords already live on host sheets). Native apps use a host secrets API (the OS keychain where the platform has one) that returns them to the app's code and never writes them under `apps/`. The shell checks at start that no agent workspace contains a `secrets/` path.
- **What the agent should not read raw stays out of its folder**: an app keeps encrypted or internal stores (Rinx's Matrix crypto store and tokens) under `secrets/<app id>/`, and writes into its account folder what its agent should read (exported threads, shared attachments, the agent's own notes and results). Rinx, today in `~/.local/share/rinx` (#101), moves under this layout. That move, and any App Hub pin move that comes with it, goes through the tagged-Rinx rule (OctoSense pins only Rinx release tags) and the App Hub lockstep fix, both in [hagency-org/Rinx#37](https://github.com/hagency-org/Rinx/issues/37).
- **Paths come from the host**, never hard-coded: script apps get the jail from the Card runner, native apps from the host API that also gives their account folder.
- **Signing out suspends the account's agent; it never closes it.** octos cannot resume a closed peer or create a replacement for its (app, account), so the shell never calls `peer_close` for sign-out. It closes the account's request contexts (a closed context id is never reopened), answers every `peer/tool/call` for it with `signed_out`, starts no turn for it (the app's, a trigger's, or a `peer/input` from the system agent, which is told the account is signed out), and stops listing it to the system agent. Signing in again resumes the same peer with the host token and opens new contexts.
- **Removing an account or uninstalling the app** deletes its folders (`accounts/<hash>/`, or `apps/<app id>/` and `secrets/<app id>/`) and suspends its agent as above. Its transcript and memory stay in octos until octos can reclaim a peer: we ask for a host-only `peer/purge` that erases a peer's stores and releases its namespace and workspace. Until then Settings says the agent's memory remains, and adding the account again resumes the same agent.

The contract, one block in both manifests (App Hub's `manifest.json` extends its existing `storage.max_bytes`; `native-apps.json` carries the same block):

```json
"storage": {
  "max_bytes": 536870912,
  "accounts": true,
  "agent_workspace": "account",
  "cache_max_bytes": 1073741824,
  "external": []
}
```

- `accounts`: the app keeps data per account (one agent per account); `false`, the default, means one `device` folder (an app with accounts declares `true`).
- `agent_workspace`: `"account"` (default: the account folder) or `"none"` (the agent reads no files; tools only).
- `external`: native apps only, paths outside the jail the app needs, reviewed in `native-apps.json` (the terminal's `home:rw`). They are part of its OS sandbox and are **never** in an agent's workspace.

Enforcement:

| Area | Script app | Native app, own process | Native app, in-process |
| --- | --- | --- | --- |
| The app stays in its jail | the isolate's jail and quota (App Hub, today) | the OS sandbox allows the jail, its secrets and `external` only | review; the host API is the only source of paths |
| The agent stays in its account folder | octos workspace fence | octos workspace fence | octos workspace fence |
| Secrets out of reach of the agent | host services hold them | host secrets API; startup check | host secrets API; startup check |
| Quotas | the isolate's quota | the shell measures and warns; the sandbox cannot count bytes | the shell measures and warns |

### 12. Agent tools: what the manifest declares and the person grants

An app agent gets **whatever tools its manifest declares, once the person grants them at install**: its own tools, octos's generic tools (`web_search`, `deep_search` and the rest), toolbox tools, other apps' shareable tools (section 7) and command execution. OctoSense hard-codes no exclusions. The shell registers them (host-routed tools additively on the peer, octos's generic tools through the turn's `generic_tools` list), authorizes and audits every call, and a destructive or outward call still needs the person, live or by a standing rule (section 8). Talk to Octos external clients still get only their fixed allowlist ([ADR 0003](0003-shared-octos-client-access.md)). Toolbox tools follow ADR 0002 §6 (the registration in [#108](https://github.com/OctoSense-org/OctoSense/pull/108)):

| Tool | Who gets it | Runs |
| --- | --- | --- |
| web search and page reading (`toolbox.search`, `toolbox.web_read`; deep research as a template through `workflow.run`) | apps granted `research` | host |
| crawling (`toolbox.deep_crawl`, with depth and page limits) | apps granted `crawl` | host |
| the app's own files | every app agent | octos, fenced to its account folder |
| memory | every app agent, its own namespace | octos |
| one-shot model calls (`model`) | apps granted `model` | host service |
| octos's generic tools (`web_search`, `deep_search`, `deep_research` and the rest) | apps that declare them and are granted them | octos |
| **command execution** | apps that declare it and are granted it, and the system agent when the person grants it in Settings (below); each command approved live under section 8 | host tool (for example `terminal.run`, only where a process Terminal exists: section 10) |

**The system agent's tool set is its grants, enforced by the shell (decided on #110).** Today the system agent's turns get octos's full default tool set, shell included, granted or not. *(Since [#117](https://github.com/OctoSense-org/OctoSense/pull/117), 2026-09-28: the shell writes a `tool_policy` denying `group:runtime` into the `_main` profile, so octos's shell is gone; the rest of the default set remains, a ceiling rather than the exact list; see the follow-ups.)* The system agent gets an explicit list instead: its supervision tools (the peer tools, glance curation, policy and budgets), the toolbox tools it is granted, the cross-app tools granted to it (section 7), and command execution only when the person grants it in Settings, off by default, each command going through the shell with a live approval (section 8). The shell enforces the list (with octos's help where needed), and a test starts a system-agent turn and checks it is offered exactly that list (in particular, no `shell` unless granted). This is a plan step (step 4), and it lands before any cross-app grant reaches the system agent.

**Command execution is a grantable capability like any other, and the widest.** A shell escapes every other boundary: the workspace fence (`cat` reads anywhere, including `secrets/`), the network policy (`curl` reaches any host), budgets and audit (one call does anything), and it turns prompt injection into code execution. So the install-time grant says so plainly, and each destructive or outward call goes through section 8. A narrow tool often serves better (for example a sandboxed `toolbox.run_python`: no network, only its folder, time and memory limits). Running real commands on the person's machine is the Terminal app's shareable `terminal.run`: destructive, `auto_approvable: false`, each command approved live with the exact command shown, run in a terminal the person sees. `terminal.run` exists only where the Terminal runs as its own sandboxed process (macOS, Windows, Linux with Vulkan and Wayland; section 10), so a grant of command execution, the system agent's Settings switch included, takes effect only there; on Linux without Vulkan and on phones it has no target (decided 2026-09-29). Every live approval and `auto_approvable: false` in this section holds outside developer mode, which approves them all (section 13). octos refuses its own `shell` to external clients (#98). For app peers and the system agent, octos's `shell` is kept out by the `_main` profile's `tool_policy`, which denies `group:runtime` (#117), so a granted app reaches command execution through a host tool. *(Corrected 2026-09-29: this sentence said octos refuses `shell` to app peers through #2567's generic-tool allowlist. It does not: with `generic_tools` omitted, as the broker registered every peer until G3, a peer keeps octos's whole roster, `shell` included (octos test `should_offer_exactly_the_registered_tools_and_refuse_an_unlisted_one`); the profile's `tool_policy` is the only barrier (and it failed open until G13: now a policy that cannot be enforced starts no kernel) until each peer's `generic_tools` is set from its grants, plan step 6 (done: each peer is registered with exactly its granted `generic_tools`).)*

### 13. Developer mode: everything granted, for building apps fast

Developers need to try an agent with every capability without granting, consenting and approving each step. **Developer mode** does that in one switch:

- **Grants**, for all apps or chosen ones: every capability an app could declare, every app's shareable tools to every other app, and a host tool **`dev.run {command, cwd?, timeout_ms?}`** that the shell executes (in the agent's workspace by default, with an output cap, every command logged). octos needs no change: `dev.run` is a host-registered tool like any app tool, and it is withdrawn the moment developer mode ends.
- **Approvals: none (decided on #110).** Developer mode **overrides every approval**: the shell answers each one itself, with no live sheet, no standing rule and no app's own `confirm: app` sheet, including destructive, outward and `auto_approvable: false` tools (Terminal commands, granted command execution, `dev.run`, deletes, payments); the cap of section 8 does not apply. The live approvals and `auto_approvable: false` rules of sections 8, 10 and 12 hold only outside developer mode.
- **What stays in place**: the developer banner, the audit of every call and command, the exclusion of Talk to Octos external clients, the development-build or explicit-flag gate, and the developer profile, each below.
- **Consent**: no first-use prompts.

How it is turned on and kept from leaking into normal use:

- **Development builds** carry it (a `dev-mode` feature). A release build honours it only with an explicit launch flag, never from Settings alone. Store builds cannot turn it on.
- **Only the person** turns it on: Settings → Developer options (a confirmation phrase on a desktop; the familiar developer-options gesture on a phone), or for headless runs and CI `OCTOSENSE_DEV_MODE=all` / `--dev-grant-all` on a development build. No agent, app or rule can turn it on, and the system agent does not suggest it.
- **A developer profile** (its own OctoSense home, test accounts) is the intended place: there developer mode **stays on until turned off**, with no expiry and no prompts. With real accounts signed in, the shell warns, offers to switch to a developer profile, and if the person continues, developer mode ends by itself after 8 hours or at restart. Grants and rules made in developer mode never carry over to another profile.
- **Always visible**: a banner ("Developer mode: all apps have full access") with a one-tap off.
- **Always audited**: every tool call, `dev.run` command and automatic approval, which doubles as a debugging trace.
- **Never for external clients (decided on #110).** Developer mode stays, to speed up building a system this complex, but Talk to Octos external clients ([ADR 0003](0003-shared-octos-client-access.md)) never get developer grants or `dev.run`: they are grants to app agents on the shell's host connection only. `dev.run` is host-routed, so octos already keeps it out of an external turn (#2601); the shell also never registers it on a session an external client can reach.

## Worked examples

### Mail: "Verify facts"

1. The person taps **Verify facts** on an email. Mail calls `host.request("octos.turn.start", …)` with a structured request ("run the fact-check skill on message <id>"), not free text.
2. Mail's agent (instructions in Mail's `AGENT.md`, procedure in a `fact-check` skill) calls `mail.read(id)` (Mail's host service), extracts the **claims**, and searches them with the system toolbox (`toolbox.search`, `toolbox.web_read`; the `research` grant, #108). It searches claims, not the whole email.
3. It builds an L0 card bound to a host-held source (`sys.digest`, #87), renders and checks it, stores it with the email (`mail.save_card(message_id, card)`), and publishes it with `glance.publish`: glance screen and notification.
4. The person acts in the card itself (section 4): asks a follow-up in it, which goes to Mail's agent in the person's lane of its conversation (section 6), which sees the recent turns of the system agent's lane, so it sees the email and the findings; or taps the card's own actions, which are Mail's own actions (section 8). Opening Mail at that email's fact-check thread is still possible, and shows the same conversation, the system agent's exchanges included. *(Until 2026-09-29 tapping the card was the only action: it opened Mail at a thread on a request context per email, whose transcript Mail reloaded with `octos.session.history`, step 3 stored that context's id with the card, and deleting the email closed that context.)*

### Calendar and Mail: "set up a group meeting plus Edward"

1. The person asks the system agent (chat or voice). It asks Calendar's agent who "the group" is and when they are free; Calendar resolves Edward (from contacts, or Mail's shareable `mail.lookup_contact`).
2. The system agent proposes "Tue 3–4 pm, Ana, Bo, Chen and Edward (edward@…); book and send invitations?"
3. On yes, it delegates to Calendar's agent, which calls `calendar.create_event` and Mail's `mail.send` four times (routed by the shell).
4. The four sends are outward, and `mail.send` is `confirm: host`: one batched sheet, drawn by the shell in the chat, shows each invitation's owning app and tool (Mail, `mail.send`), the calling app (Calendar) and the exact arguments, unless a standing rule on (Mail, `mail.send`) ("send to my contacts") answers them. The shell had already checked that Calendar's manifest was granted `mail.send`; no agent is asked to consent.
5. Calendar's UI shows the event; the system app notifies "Meeting booked Tue 3 pm; invitations sent to 4." A failed invitation is named and not retried without the person.

## Consequences

- One manifest entry adds or updates a native app; CI keeps the derived places in step.
- On macOS, Windows and Vulkan Linux a native crash no longer takes the shell down; mobile and non-Vulkan Linux depend on the robustness bar.
- Every app can have an agent with its full context; the system agent delegates to it through `peer/input`, and may call the tools it is granted.
- Cross-app tools are granted by manifest and authorized by the shell on every call, for app agents and the system agent alike; octos supplies the additive, host-routed registration (#2567, #2601).
- Every app tool call, whoever makes it, goes through the shell, which authorizes, audits and routes it. The person decides every outward action, on the shell's sheet or, for a `confirm: app` tool, the owning app's own sheet showing the caller, now or through narrow, time-boxed, audited rules keyed to (owning app, tool).
- The system agent loses its default shell access and gets a tested tool set made of its grants; command execution only if the person grants it, each command approved live, and only where a process Terminal exists (section 10).
- An app's cards can do anything its UI can, under its own policy; the shell's approvals cover agent tool calls, not the person's actions in an app's UI or cards, and card hardening is deferred (section 8). The person and the system agent each talk to an app agent in their own lane, in parallel, and each lane sees the other's recent turns (section 6).
- New work: the generator; the system agent's tool set; `peer/input` in the shell; process sandboxes; the peer link; shell tool routing; the approval router and its Settings; consent at first use; release packaging of process apps; module panic containment.

## Plan

1. `native-apps.json` and `tools/native_apps.py` with `--check`; delete the native News, Maps and Photos crates. No behaviour change otherwise.
2. Terminal as a process on macOS and Windows, the only process app for now; release packages ship its `bin`. App Hub and Rinx stay in-process (section 2).
3. Land octos#2567 (merged) with the agreed design: additive registration, per-connection ownership, host-routed origin kept out of external turns (#2601), and `peer/input` (and the UPCR-2026-034 approval wording, section 8). Ask octos for a read-only view of the peer's folder for request contexts, and a host-only `peer/purge`. *(2026-09-29: the earlier ask for `ask_user_question` on host-driven turns is closed as already met: octos keeps it; see section 6.)* Ask octos for one shared conversation per app peer, with each turn's `origin` (section 6; octos [#2626](https://github.com/octos-org/octos/pull/2626)); then, the same day, for the person's lane in parallel with shared history (`peer/context/open` `share_history`, octos UPCR-2026-034, [#2636](https://github.com/octos-org/octos/pull/2636); section 6).
4. **The system agent's tool set** (section 12): its grants (supervision tools, granted toolbox tools, granted cross-app tools; command execution only when the person grants it in Settings, each command approved live), enforced by the shell, and a test that a system-agent turn is offered exactly that list. Before any cross-app grant reaches the system agent.
5. Land #106 (script apps' agents; merged 2026-09-28) and #108 (toolbox tools); rebase #85 on #98.
6. #2567 in the shell: register app tools, execute or relay `peer/tool/call` with the stamped identity and caller (section 5), set each peer's `generic_tools` from its manifest and grants (section 12), handle `peer/input`, route agents' `ask_user_question` to the right conversation (section 6), handle the host read tools, suspend on sign-out (section 11); grant cross-app tools from manifests (at install for script apps, from `native-apps.json` for native apps), register them marked with the owning app, and check every call against the grants.
7. Shell-drawn approval sheets for `confirm: host` tools and the hand-off of `confirm: app` confirmations to the owning app with the caller (owning app, tool, exact arguments, calling app); the approval router, standing approvals keyed to (owning app, tool) and their Settings; consent at first use.
8. The peer link and its Makepad client API; process sandboxes. Then Rinx may move to a process by a reviewed `hosting` change.
9. Module panic containment.
10. The storage layout and contract (section 11): App Hub's `storage` fields; the host paths and secrets APIs; Rinx and the host services move under `apps/` and `secrets/` (the Rinx move, #101, and any App Hub pin move through the tagged-Rinx rule and the lockstep fix, hagency-org/Rinx#37); the startup check.
11. Developer mode (section 13): the developer profile, the grants, `dev.run`, the banner and audit, and a test that an external client gets neither developer grants nor `dev.run`; `terminal.run` as a confirmed shareable tool (section 12).

### Follow-ups

- [x] **An agent for every app that declares one** (section 4, 2026-09-29): a live run on `625d6cc` found News (tools.json, no `octos.*`) without a peer, an empty `peer_list` for the system agent, Settings listing only Rinx, and no place for the person to talk to an app's agent. Now the shell lists every app with an agent, prepares a consented app's peer, tells the system agent about the ones it cannot list, and draws "Ask <app>". Still open: a native app's peer exists only while the app is open; the speaker label of a script app's person turns is its app id (the peer's name, kept for resume), not its display name; prepared peers hold a kernel connection, so the kernel's idle stop waits for them.
- [x] **The "Ask <app>" panel's live run, follow-ups** (2026-09-29, [#189](https://github.com/OctoSense-org/OctoSense/pull/189) and the App Hub pin after it): the panel orders both lanes by one clock (each lane's `cursor.seq` is its own; a turn's request line stands where the turn started, handed over by the broker with `turn/started`, since octos writes a turn's user message only when it ends and never for a stopped turn); Stop says "interrupted" once; the system agent is given each peer's slug for `peer_send_input`; and a script app's agent may keep octos's `ask_user_question` (App Hub's one kernel tool for contained apps, `KERNEL_TOOLS`, "Ask you questions" in the store), handed to its peer's `generic_tools` by G3. News's agent keeps it, and its questions of the person's lane are asked in "Ask News".

*Still open after the implementation (2026-09-29):* the Windows AppContainer sandbox ([#137](https://github.com/OctoSense-org/OctoSense/issues/137)); the Linux sandbox has run only in CI; process apps see the system agent's lane only through history until Makepad's peer client reads `conversation` frames; a second message on one conversation handle is refused rather than queued; Rinx still has to send turn triggers and hand over its send sheet; the OpenHarmony build with octos#2619's BoringSSL (`btls-sys`) is untested.

Gaps between this decision and `main` found after it was accepted (2026-09-28; see [the architecture overview](../architecture.md#where-the-code-and-the-adrs-disagree)):

- [x] **`dev.run` registration** (section 13): done (2026-09-30). The shell offers `dev.run` to the peer of every app developer mode covers, as that app's own host tool (`destructive`, `confirm: host`), registered by the brokers on the shell's host connection only and never on the system agent's session; the relay runs it itself (`crates/shell/src/host_tools/dev_run.rs`: `sh -c` in the agent's workspace unless `cwd` names another directory, output capped at 64 KiB per stream, killed at its timeout) and refuses it to any other caller and to an app developer mode no longer covers; its approval is a command the router answers under developer mode; every command is written to the developer audit (`dev_run`). When the mode turns on, off or expires every live peer registers its tools again (`broker::reregister_tools_where`), so `dev.run` is withdrawn at once. Tests: `host_tools::tests::dev_run_*`, `host_tools::dev_run::tests`, and the existing `dev_mode::tests::external_clients_never_get_developer_grants_or_dev_run`.
- **Developer mode per app** (section 13): choosing which apps it covers in Settings (today only through `OCTOSENSE_DEV_MODE`), and the phone gesture that turns it on.
- **The system agent's exact tool list** (section 12, step 4): the `_main` profile's `tool_policy` is a ceiling, not the list. octos#2567's host session set can narrow the session with `generic_tools`, but that list narrows every client's turns on it, so the shell registers without it (as agreed on #2567); the exact list waits for a durable host-only list (octos#2605).
- [x] **The Settings switch for the system agent's command execution** (section 12): done. Setup → Assistant → Command execution sets `SystemAgentTools::grant_command_execution` with the person's typed confirmation, applies from the next kernel start (with a restart offered), and routes each command through the approval router as `auto_approvable: false`. While it is on, the system chat registers `terminal.run` on the system session (octos#2567's host session target; withdrawn when it goes off), and each approved call is typed into the Terminal the person sees.
- **The agent workspace is the account folder** (section 11): partly done. A peer created since the octos 665209e5 pin gets its account folder as the `peer/prepare` `cwd`, recorded beside its host token; a peer created before keeps the kernel-provisioned workspace, because octos resumes a peer only under the workspace it was made with. The account folder name (SHA-256, `app_storage`'s `account_hash`) and the memory namespace tag (FNV-1a, `account_tag` in the `crates/app-peers` broker) are two different hashes of one account; one should derive from the other. Changing the memory tag re-keys every app's memory, so it needs a migration (or the folder name follows the tag).
- [x] **octos#2567 in the shell** (plan step 6, in part): the broker registers every app peer's tools after each prepare and reconnect (with exactly its granted `generic_tools` since G3), takes `peer/tool/call` (stamped, once per occurrence, nothing after a cancel, calls of an interrupted turn refused), starts `peer/input` turns and hands `host_tool` approvals to the router; the relay (`crates/shell/src/host_tools/`) authorizes each call by (owning app, tool) and caller and routes it to the owning app. Done since (G3, G8): apps declare tools (`native-apps.json` `agent.tools`, a script app's `tools.json` loaded with App Hub's loader), cross-app grants come from `agent.grants` and a script app's manifest at install, each peer is registered with exactly its granted `generic_tools` (never octos's shell), script apps' host services have an executor (`HostServiceExecutor`), and the relay checks arguments and results against the declared schemas and budgets each agent. Still open: Rinx must hand its send sheet over (`OctosAppService::set_confirm_sheet`) to be the `rinx.message.send` confirmation; the host read tools are not registered; a peer's `user_question/requested` is forwarded to the shell's request model (`crates/shell/src/questions/`) and asked in the app's conversation or, for a system-agent turn, the system chat, answered only by the person on a shell surface (section 6; done 2026-09-29); a refused `peer/input` is now refused to the kernel with its reason (`peer/input/reject`, octos#2621); the person's turns run with their `origin` in the person's lane, a sharing request context in parallel with the system agent's lane (section 6; octos UPCR-2026-034 `share_history`; before that, one queued conversation, octos#2626 and OctoSense #166).
- **Gaps from the 2026-09-29 code review** (octos acffad3b, OctoSense `main` ecb3583):
  - **Kernel-tool approvals on a `peer/input` turn are dropped.** The broker's event filter for the peer's own session (`crates/app-peers/src/broker.rs`, `inbound`) hands only `host_tool` approvals to the router and returns on every other event of that session, so an `approval/requested` for a kernel tool on a `peer/input` turn reaches no one and nobody can answer it.
  - **Contained script apps' cards** (`crates/ai-host/src/contained.rs`): streamed text is dropped (the app gets only the final answer, and reads the rest of the conversation, both lanes merged, with `octos.session.history`: App Hub's service calls push no events), an approval the router does not take is declined (listed in `denied_approvals`), and every peer uses the fixed account `device`. *(Until #166 each app had one request context shared by all its cards and UI; #166 moved them to the peer's shared conversation; now they talk in the person's lane, a sharing context per handle generation, section 6.)*
  - [x] **`user_question/requested` for cards**: by design a card never answers its agent's question; the broker hands every question on the peer's session or a context to the shell's request model (`crates/shell/src/questions/`), routed by the turn's origin, and the person answers on a shell surface (the app's conversation, or the system chat).
  - **A failed host `turn/start` for a `peer/input` is only logged** (`start_input` in `broker.rs`); the system agent is not told. Since #166 a start refused `turn_in_progress` is retried with the same turn id; any other refusal is still only logged.
  - **The two lanes, left open**: the Makepad peer client (`makepad_ai_services::peer`) does not read the `conversation` frames yet (it drops events of a request that has answered), so a process app shows the system agent's turns only through `octos.session.history` until it does; a conversation handle runs one turn of its own at a time (a second message on the same handle is refused, not queued); the shared history block is the kernel's defaults (the shell sends `share_history: {}`); and the accepted hazards of section 6 (unlocked file edits across lanes, shared unknown-outcome marks, a slight budget overshoot). *(octos [#2627](https://github.com/octos-org/octos/issues/2627), `peer_gather` refusing the kernel's own `result.md`, is fixed by octos #2630.)*
  - **The `peer/input` route belongs to whichever connection registered the peer's tools last**: octos `set_host_route` (`crates/octos-cli/src/peers/host_tools.rs`) replaces any earlier route. Untested with more than one instance of an app.
- [x] **A shell-native chat with the system agent**: done. `crates/shell/src/system_chat/` (Setup → Assistant → Assistant chat, F8; full screen on a phone) talks to `_main:api:octosense#system` over the shell's own kernel connection, and its approvals go through the approval router, batched per request.

## Open questions

- D1 is decided on #110 (section 7): app agents and the system agent may call other apps' granted tools, and the shell authorizes each call.
- Linux without Vulkan: revisit once process hosting is measured there (readback cost for light apps).
- The exact `tools.json` fields rules can match (recipients, attachments, amounts), agreed with App Hub.
