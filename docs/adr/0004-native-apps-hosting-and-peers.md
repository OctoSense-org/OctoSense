# ADR 0004: Native apps, app agents and cross-app work: one manifest, hosting per target, an agent for every app, approvals by the person

- **Date:** 2026-09-28
- **Status:** Proposed
- **Scope:** How OctoSense declares, links, hosts, isolates and trusts its bundled native Rust apps on each target; where every app keeps its data and what its agent may read; how every bundled app, native or script, owns an octos app agent; how the system agent and app agents work across apps; and how the person approves what they do, live or in advance.
- **Relates to:** [ADR 0001](0001-one-octosense-repository.md); [ADR 0002](0002-event-driven-app-agents.md) (app agents; amended by this ADR); [Home ADR 0002](home/0002-agentic-app-security-model.md) and [Home ADR 0004](home/0004-system-apps-are-contained-script-apps.md) (amended); Rinx [ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md); octos UPCR-2026-034 (host-owned app peers, merged), UPCR-2026-035 ([octos#2567](https://github.com/octos-org/octos/pull/2567), host tools per app peer, open) and UPCR-2026-036 (`serve --host-managed`, [octos#2591](https://github.com/octos-org/octos/pull/2591), merged); [octos#2601](https://github.com/octos-org/octos/pull/2601) (tool origin, per-connection ownership); [ADR 0003](0003-shared-octos-client-access.md) (Talk to Octos, OctoSense [#98](https://github.com/OctoSense-org/OctoSense/pull/98): external clients never reach app agents).

## Context

### Two kinds of bundled app

| | Native app | Script system app |
| --- | --- | --- |
| Today | App Hub (the store and the Card runner), Rinx, Terminal; opt-in Sheets, Reference, AppCard; native News, Maps and Photos crates kept "for comparison" | News, Maps, Photos, Camera, Mail, AI providers |
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

- **Peers are sessions, not processes.** One kernel process per shell; every peer is a session in it (tokio tasks) with its own workspace, memory namespace, transcript and model lane. Tools such as the shell run as short child processes in the peer's workspace.
- **No 8-peer ceiling.** `peer/prepare` stages 1–8 peers **per call** (`n`); the only total is a soft cap of 8192 live peer registrations. The model's `peer_handoff` tool is limited to 4 per turn; host-owned app peers are not created through it. OctoSense's broker prepares one peer per call.
- **Master and peer talk through an inbox and a file blackboard.** Master → peer: `brief.md` at creation, then `peer_send_input`, delivered as the peer's next user turn (originator only; in serve through a durable queue drained every 2–5 s, best-effort at-least-once). Peer → master: each turn writes `peers/<slug>/result.md` (+ `result-N.md`, `turns.txt`), read with `peer_gather` / `peer_list`, "the only cross-peer channel". A peer asks with `ask_user_question`; the master answers with `peer_respond`. Depth 1: peers cannot create, steer or close peers.
- **Host-owned app peers** (UPCR-2026-034): owned by the shell's system-agent session; exclusive workspace and memory namespace per (app, account); resumable with a host token; **request contexts** (`peer/context/open`) give an app's clients separate transcripts, folders and child memory namespaces. The system agent may answer an app peer's questions, never approve its tools.
- **Request contexts are fenced** (UPCR-2026-034): a context's workspace is `<peer workspace>/contexts/<id>/`; the peer's own folder is refused as a context workspace, so a context turn cannot read the files beside it. The peer's own session is bound to the peer's folder and reads everything under it, every context's folder included.
- **Closing is final** (UPCR-2026-034): a closed peer cannot be resumed, and it keeps its (app, account) namespace and workspace reservation ("their stores still hold data"), so no replacement can be created for them. octos has no suspend, reclaim or purge.
- **Host tools per app peer** (UPCR-2026-035, #2567, open): the host registers the app's `tools.json`; the kernel sends every call to the **host's connection** (`peer/tool/call`, carrying `context_id`), the host answers (`peer/tool/result`). A call carries `session_id`, `context_id` (null on the peer's own session) and `turn_id`, and no account or client field. A turn driven with a `generic_tools` list keeps only the peer-safe kernel tools (file reads, search, memory), and `ask_user_question` is not among them. Once-only execution, timeouts, audit, approvals for destructive and outward tools, budgets. The kernel never talks to an app. As agreed on #2567: registration is **additive** (the peer-safe generic tools always stay); ownership is **per connection** (the registering connection is the tool host and owns the peer's approvals); host-routed tools have their own origin and are excluded from Talk to Octos external turns (#2601's builtin-only allowlist); and the system agent's input to a host-owned peer is delivered to the host's driving connection as **`peer/input`**.
- **Host-managed serve** (UPCR-2026-036, OctoSense #98): opt-in websocket kernel with a host token and an external token. External clients (a web client, a TUI) get no peer methods and cannot name an app peer's session.

### Gaps

1. **A system-agent turn on an app peer got none of the app's context.** #2567 gives tools, memory and context only to turns driven by the host's connection, and `peer_send_input` used to arrive as a tool-less kernel continuation. Closed on #2567 by `peer/input` (section 6); the shell has to handle it.
2. **App tools are per peer.** An app agent cannot call another app's tool, and the system agent has no app tools; the system agent's turns instead get octos's full default tool set, shell included (section 12).
3. **Script apps have no agent on `main`.** [#106](https://github.com/OctoSense-org/OctoSense/pull/106) (open) adds the `octos` host service, one peer `card.<app id>` per app; it declines tool approvals (the Card runner has no approval sheet yet).
4. **Native apps in their own process cannot reach an agent**: `ai_host::offer` hands the peer to a module in-process only.
5. Only Rinx is granted an agent, in code (`Policy::shipped()`).
6. **Host-driven turns cannot ask questions**: a turn driven with a `generic_tools` list, which the shell sets from each app's manifest and grants (section 12), loses `ask_user_question`.
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

Three flows:

| Flow | Native module | Native app in its own process | Script app |
| --- | --- | --- | --- |
| 1. The app talks to its agent (UI, triggers) | the service injected at `create` | the **peer link** (section 5) | `host.request("octos.*")` (#106) |
| 2. The agent uses the app's tools with full context | the shell executes the call in-process | the peer link carries `peer/tool/call` in and the result out | the app's host service executes it |
| 3. The system agent talks to the agent | in the kernel; its input reaches the shell as `peer/input` and the shell drives the turn (section 6) | the same, relayed on the peer link | the same |

The shell is always the kernel's host connection: it holds the host token, registers the app's tools, drives the app peer's turns and routes tool calls.

**Triggers** (a button, a data event, a schedule, a file change) are the app's (ADR 0002 §2). The shell gates each: script triggers arrive as `host.request`, native ones on the peer link or at injection, under the same grants, budgets and background policy.

### 5. The peer link for process-hosted native apps

It rides the process's hub socket as its **own channel**, addressed to that app's peer and never registered with the AI services bus:

- up: `PeerRequest { req_id, method: "octos.session.open" | "octos.turn.start" | …, args }`, `PeerToolResult { call_id, ok, data | error | awaiting_confirmation }`;
- down: `PeerReply { req_id, … }` and streamed turn events, `PeerToolCall { call_id, name, args, risk, confirm_required, timeout_ms, account, context_id, client, caller }`, `PeerToolCancel { call_id }`.

**Identity travels with every call, stamped by the shell.** The socket identifies the app, not who inside it asked. When the app opens a request context (`octos.context.open { account, client }`, where `client` is the app's own label such as a Rinx mini app id), the shell records `context_id → (app, account, client)`. On each `peer/tool/call` it looks up the kernel's `context_id` (null: the agent's own session, `client` null) and the peer's account, and puts them on `PeerToolCall`, with **`caller`**: the app's own agent, or for a cross-app call the calling app or the system agent (section 7). The app never supplies them on a call, and a process may use only contexts it opened. The app checks its own grants against them (section 9), and a `confirm: app` sheet shows the caller (section 8). In-process modules and script apps' host services get the same fields.

The shell stamps the app's identity from the socket, enforces #2567's host obligations for the app (once per call, nothing after cancel, acknowledge before a confirmation sheet), and when the process dies fails its outstanding calls, closes its request contexts and keeps the peer. The client API lives in Makepad's `makepad-ai-services` (Makepad apps cannot depend on OctoSense crates) and serves in-process modules too, so an app does not know how it is hosted.

### 6. The system agent and app agents

- The system agent is the originator of every app peer. It briefs and asks with `peer_send_input`, reads answers on the blackboard, answers an app agent's questions (through `host.ask`, below, or `peer_respond` where octos keeps `ask_user_question`), and never approves an app agent's tools.
- **A turn the system agent starts on an app peer runs with the app's tools, memory and context: the `peer/input` event** (agreed on #2567). The system agent's `peer_send_input` to a host-owned peer is not run as a kernel continuation; octos delivers it to the host's driving connection as `peer/input {peer, session_id, input_id, turn_id, text}`, and the shell starts the turn on that connection (`turn/start` with the kernel's `turn_id`), so it is host-driven: it has the app's tools and memory, and its approvals are raised in the app's conversation. The system agent follows it on the blackboard. If no host connection holds the peer (never registered, or disconnected), octos runs and queues nothing and tells the system agent the app is not connected; the system agent waits for the app or reports the failure visibly.
- **Questions from host-driven turns go through a host tool.** The shell drives turns with a `generic_tools` list set from the app's manifest and grants (section 12), and octos drops `ask_user_question` from any such turn, so `peer_respond` has nothing to answer. The shell registers **`host.ask {question, options?}`** on every app peer and routes it: on a turn the system agent started, to the system agent, whose answer (or its own question to the person) becomes the tool result; on a turn the app or the person started, to the app's conversation. It is never an approval: approvals stay with the person (section 8). We also ask octos to keep `ask_user_question` on host-driven turns; `host.ask` remains the route while it is missing.
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
- **Who draws the sheet.** For a `confirm: host` tool (for example `mail.send`) the shell draws it, in the app's conversation or batched in the system chat, and a standing rule may answer it. For a `confirm: app` tool the shell hands the confirmation to the owning app, which shows its **own sheet** (for Rinx, its send sheet) for callers of every kind; the call carries who is calling (`caller`, with its context; section 5), so the app's sheet shows it. When the owning app is not running, or nobody is present, the call waits or is refused visibly, as octos#2567 specifies. **An agent's own text is never an approval surface.** Each line, on either sheet, shows the **owning app, the tool and the exact arguments**, and for a cross-app call the **calling app** (another app's agent, or the system agent).
- **Live.** Pending approvals surface on a shell-drawn sheet in the app's conversation (on the owning app's own sheet for a `confirm: app` tool), on its card and as a notification; the system agent may batch the `confirm: host` approvals of one request into one shell-drawn sheet in its chat (a plan plus its outward actions, with exact arguments). The shell answers each approval with the person's decision.
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
- Its AI may read (`read_screen`, `read_scrollback`) and **type commands**, in every hosting. Each typed command goes through the shell with a live approval showing the exact command, and is `auto_approvable: false`, so no standing rule approves it (consistent with makepad#41, which already requires confirmation for each run). Outside developer mode, that is: developer mode runs them without a sheet (section 13).
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

- `accounts`: the app keeps data per account (one agent per account); `false` means one `device` folder.
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
| **command execution** | apps that declare it and are granted it, and the system agent when the person grants it in Settings (below); each command approved live under section 8 | host tool (for example `terminal.run`) |

**The system agent's tool set is its grants, enforced by the shell (decided on #110).** Today the system agent's turns get octos's full default tool set, shell included, granted or not. The system agent gets an explicit list instead: its supervision tools (the peer tools, glance curation, policy and budgets), the toolbox tools it is granted, the cross-app tools granted to it (section 7), and command execution only when the person grants it in Settings, off by default, each command going through the shell with a live approval (section 8). The shell enforces the list (with octos's help where needed), and a test starts a system-agent turn and checks it is offered exactly that list (in particular, no `shell` unless granted). This is a plan step (step 4), and it lands before any cross-app grant reaches the system agent.

**Command execution is a grantable capability like any other, and the widest.** A shell escapes every other boundary: the workspace fence (`cat` reads anywhere, including `secrets/`), the network policy (`curl` reaches any host), budgets and audit (one call does anything), and it turns prompt injection into code execution. So the install-time grant says so plainly, and each destructive or outward call goes through section 8. A narrow tool often serves better (for example a sandboxed `toolbox.run_python`: no network, only its folder, time and memory limits). Running real commands on the person's machine is the Terminal app's shareable `terminal.run`: destructive, `auto_approvable: false`, each command approved live with the exact command shown, run in a terminal the person sees. Every live approval and `auto_approvable: false` in this section holds outside developer mode, which approves them all (section 13). octos refuses its own `shell` to app peers today (#2567's generic-tool allowlist) and to external clients (#98), so a granted app reaches command execution through a host tool.

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
3. It builds an L0 card bound to a host-held source (`sys.digest`, #87), renders and checks it, stores it with the email (`mail.save_card(message_id, card, context_id)`), and publishes it with `glance.publish`: glance screen and notification.
4. Tapping the card opens Mail at that email's fact-check thread: the card and a conversation on the same agent context, so follow-up questions see the email and the findings. The chat history is the context's transcript; Mail keeps the reference and reloads it with `octos.session.history`. Deleting the email closes its context.

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
- The system agent loses its default shell access and gets a tested tool set made of its grants; command execution only if the person grants it, each command approved live.
- New work: the generator; the system agent's tool set; `peer/input` in the shell; process sandboxes; the peer link; shell tool routing; the approval router and its Settings; consent at first use; release packaging of process apps; module panic containment.

## Plan

1. `native-apps.json` and `tools/native_apps.py` with `--check`; delete the native News, Maps and Photos crates. No behaviour change otherwise.
2. Terminal as a process on macOS and Windows, the only process app for now; release packages ship its `bin`. App Hub and Rinx stay in-process (section 2).
3. Land octos#2567 with the agreed design: additive registration, per-connection ownership, host-routed origin kept out of external turns (#2601), and `peer/input` (and the UPCR-2026-034 approval wording, section 8). Ask octos for `ask_user_question` on host-driven turns, a read-only view of the peer's folder for request contexts, and a host-only `peer/purge`.
4. **The system agent's tool set** (section 12): its grants (supervision tools, granted toolbox tools, granted cross-app tools; command execution only when the person grants it in Settings, each command approved live), enforced by the shell, and a test that a system-agent turn is offered exactly that list. Before any cross-app grant reaches the system agent.
5. Land #106 (script apps' agents) and #108 (toolbox tools); rebase #85 on #98.
6. #2567 in the shell: register app tools, execute or relay `peer/tool/call` with the stamped identity and caller (section 5), set each peer's `generic_tools` from its manifest and grants (section 12), handle `peer/input`, `host.ask` and the host read tools, suspend on sign-out (section 11); grant cross-app tools from manifests (at install for script apps, from `native-apps.json` for native apps), register them marked with the owning app, and check every call against the grants.
7. Shell-drawn approval sheets for `confirm: host` tools and the hand-off of `confirm: app` confirmations to the owning app with the caller (owning app, tool, exact arguments, calling app); the approval router, standing approvals keyed to (owning app, tool) and their Settings; consent at first use.
8. The peer link and its Makepad client API; process sandboxes. Then Rinx may move to a process by a reviewed `hosting` change.
9. Module panic containment.
10. The storage layout and contract (section 11): App Hub's `storage` fields; the host paths and secrets APIs; Rinx and the host services move under `apps/` and `secrets/` (the Rinx move, #101, and any App Hub pin move through the tagged-Rinx rule and the lockstep fix, hagency-org/Rinx#37); the startup check.
11. Developer mode (section 13): the developer profile, the grants, `dev.run`, the banner and audit, and a test that an external client gets neither developer grants nor `dev.run`; `terminal.run` as a confirmed shareable tool (section 12).

## Open questions

- D1 is decided on #110 (section 7): app agents and the system agent may call other apps' granted tools, and the shell authorizes each call.
- Linux without Vulkan: revisit once process hosting is measured there (readback cost for light apps).
- The exact `tools.json` fields rules can match (recipients, attachments, amounts), agreed with App Hub.
