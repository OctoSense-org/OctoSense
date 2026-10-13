# Code walkthrough: from an app window to an agent turn

English | [简体中文](architecture-walkthrough.zh-CN.md)

This is the reading path through OctoSense's agent code. It follows one question from the window where the person types it, through the shell and the octos kernel, to the Rust that answers it. Each step names the file and symbol to open, what to look for, and why it matters. The concepts are in the README's [key concepts](../README.md#key-concepts); the full reference is [architecture.md](architecture.md).

The question is **"What is on my calendar today?"**, and Calendar is a system script app. The person can ask the system agent, which delegates it to Calendar's agent in the **system agent's lane**, or ask Calendar's agent directly, in its "Ask Calendar" panel or a Calendar card's Chat tab, in the **person's lane**. Both routes can end in the same tool call, `calendar.events`, and each answer returns to the conversation that asked. Either needs a kernel, a provider and the person's consent.

## 1. Start at the executable

[desktop/src/main.rs](../desktop/src/main.rs) only calls `octosense_shell::octosense_main!()`; [phone/src/main.rs](../phone/src/main.rs) makes the same call around an `App` that adds the Settings app. In [crates/shell/src/lib.rs](../crates/shell/src/lib.rs), `App::handle_startup` sets up the agent machinery in a fixed order:

| Call | What it sets up |
| --- | --- |
| `app_storage::init` | Every app's folders and the secrets root ([§8](#8-where-the-data-lives)) |
| `ai_host::start` | The kernel's configuration and the AI services ([§3](#3-find-who-owns-the-kernel)) |
| `dev_mode::init`, `approvals::init` | Developer mode, then the approval router |
| `host_tools::init` | The shell as every broker's tool host, with the relay ([§7](#7-trace-a-tool-to-rust-code)) |
| `system_chat::init` | The system agent's grants, before the kernel first starts |
| `agents::start` | A thread that prepares the peer of every script app the person allowed ([§5](#5-prepare-a-peer-and-give-it-two-lanes)), and Mail's collection/delivery threads |

The order is the point: the router and the relay exist before any agent can call a tool. If the person allowed Calendar's agent in an earlier run, `agents::start` prepares its peer at once, and that first connection starts the kernel.

## 2. See how an app is hosted

Where an app runs decides how it reaches its agent and where its tools execute. [native-apps.json](../native-apps.json) declares each native app's hosting, and `AppRegistry::hosting` in [apps.rs](../crates/shell/src/apps.rs) resolves it for each launch.

| Kind | Open | Look for |
| --- | --- | --- |
| Native module (Rinx, Notes, Calculator, …) | [module_host.rs](../crates/shell/src/module_host.rs) | One Splash isolate per instance. Every call into a module runs under `catch_unwind` (`contain`), so a panic closes the app, not the shell. |
| Process app (the Terminal and Task, in a checkout build) | [clients.rs](../crates/shell/src/clients.rs), [hub.rs](../crates/shell/src/hub.rs) | The hub admits a child's socket only with the secret its launch read on stdin; `sandbox_policy` builds the OS sandbox. |
| Script app (Calendar, Mail, every store app) | `system_card_apps` in `apps.rs`, then App Hub's `CARD_MODULE` | The `card` module, the Card runner, hosts every system and installed app, one isolate per instance. |

`system_card_apps` makes Calendar's launcher row and, the first time, registers the shell's host services (`register_host_services`). Calendar's `calendar` declaration describes its service use; the contained UI calls as the admitted app under the service's actual identity and data rules. Month/day views, the editor and Calendar-owned tools all read and write `.host/calendar/events.json`. The app's Glance event card is a projection of that record; its in-card **Open Calendar** action uses the publication-bound `event/<id>` route to open that same event in Calendar. Cross-app agent calls still require the separate grants described below.

To run the desktop, follow its README's [Build and run](../desktop/README.md#build-and-run), which stages the pinned kernel with `python3 tools/kernel-artifact.py --host --stage target/release`.

Ordinary connected apps use the same Card runner. GitHub Notes, Inbox Assistant and Google Calendar declare `auth`, the data family they use (`github`, `gmail` or `gcalendar`) and `storage.accounts: true`. The host binds each app's peer to its active connection, which the app sees only as an opaque handle. Follow the [OAuth guide](../crates/oauth-service/README.md) for sign-in, explicit `host_method` tool mappings, durable Gmail events and admitted Glance templates. Google Calendar reads the person's Google calendar, not the built-in Calendar's `events.json`. Live GitHub and Google sign-in has passed on macOS; GitHub writes, Gmail sends and device acceptance are still pending.

## 3. Find who owns the kernel

One service owns the kernel, and every consumer connects to it. Read these in order:

1. **`ai_host::start`** ([ai-host/src/lib.rs](../crates/ai-host/src/lib.rs)) configures the kernel and starts nothing. It also registers the `octos` service, which gives script apps their peers, and `model`, which makes one-shot model calls with no peer or tools.
2. **`Core::connect`** ([kernel/src/lib.rs](../crates/kernel/src/lib.rs)): the first connection starts a kernel *generation*, and later ones attach to it. When the last one detaches the kernel stops, unless Talk to Octos is on. After a provider change, `restart` ends the generation and consumers reconnect.
3. **`launch::resolve`** ([launch.rs](../crates/kernel/src/launch.rs)): a `serve --stdio` child on the desktop (`OCTOS_APP_CORE_BIN`, or the packaged `octos-kernel` whose receipt names the pinned revision) and on Android (`liboctos.so`), `serve_io` inside the shell on OpenHarmony, nothing on iOS.
4. **`supervise`** ([kernel.rs](../crates/kernel/src/kernel.rs)): one task per generation owns the process and the frame pump. Before any consumer's frame, it sets the system agent's exact tool list (`session/tool_list/set`).
5. **`Router`** ([router.rs](../crates/kernel/src/router.rs)): consumers, such as the system chat and Calendar's broker, share one stream. Each request gets a kernel-unique id and its response goes to its sender only; each notification goes to the consumers that opened its session.

## 4. Follow a system-chat message

Open [system_chat/mod.rs](../crates/shell/src/system_chat/mod.rs), then [session.rs](../crates/shell/src/system_chat/session.rs) and [link.rs](../crates/shell/src/system_chat/link.rs):

- `spawn` starts the `system-chat` thread, which owns a `session::Driver`. The UI thread sends it `Command`s; the thread wakes the UI with `SignalToUI` when the chat changes.
- The `Driver` opens `SYSTEM_SESSION` (`_main:api:octosense#system`) and sends each message as `turn/start`. After every connect it registers the system agent's host tools on its link (`peer/tools/register` without a `peer`).
- `link::poll_for` polls the kernel with a waker that unparks the thread, so the chat needs no Tokio runtime. It stays connected only while its pane is open or a turn runs.

The system agent's kernel tools (`SYSTEM_AGENT_TOOLS` in [kernel/src/system_tools.rs](../crates/kernel/src/system_tools.rs)) do not read a calendar. Its separately registered host tools now include explicitly granted `calendar.events`, `calendar.add_event` and `calendar.notify`, so a bounded request can run directly. For work needing Calendar's own reasoning/context, it can still delegate. Two host tools from [agents.rs](../crates/shell/src/agents.rs), answered by the system chat itself (`agents::call`), get it there:

- `agents.list` returns every app with an agent, whether the person allowed it, and its peer slug.
- `agents.ask` shows Calendar's first-use sheet if the person has not decided, and holds the call until they answer and the peer is ready; then it returns the slug.

The system agent passes the slug, never the app id `os.calendar`, to `peer_send_input`. Its approvals go to the router, batched per turn; the pane never approves anything.

## 5. Prepare a peer and give it two lanes

Read [app-peers/src/contract.rs](../crates/app-peers/src/contract.rs), everything an app sees, before the much larger [broker.rs](../crates/app-peers/src/broker.rs):

- `OctosAppService`: one app instance's scoped service. `open_conversation` opens the person's lane, `open_context` a private context (Rinx's mini apps).
- `OctosContext::call(ContextOp, EventSink)`: one operation, whose events end with one `Complete`.
- `ContextSpec` (the account and granted services) and `TurnTrigger` (what started a turn), both stamped by the host ([§6](#6-where-the-person-talks)).

**Calendar's peer.** A script app's peer belongs to the `octos` host service in [ai-host/src/contained.rs](../crates/ai-host/src/contained.rs). `contained::prepare` (from `agents::prepare`) and `contained::conversation` (from the "Ask Calendar" panel) share one broker, `card.os.calendar`, built by `hosted::launch` ([hosted.rs](../crates/app-peers/src/hosted.rs)). Calendar keeps no accounts, so the broker acts for `device`. `Broker::ensure_peer` then:

1. sends `peer/prepare` with the memory namespace `app/card.os.calendar/acct-<tag>`, `resume: true` and, for a new peer, the account folder as its workspace (`ToolHost::agent_workspace`);
2. refuses a kernel that did not honor that namespace;
3. keeps the peer's host token in a `PeerRecord` and opens the peer session, `…#peer-<slug>`: the system agent's lane;
4. registers the app's tools (`register_tools`, [§7](#7-trace-a-tool-to-rust-code)). A peer whose registration fails runs no turn.

When a native app has two windows, only the oldest instance's broker drives the peer (`Broker::drives`; `take_over` when it closes). Calendar's one broker always drives: it registers the tools and takes the system agent's inputs. Its `on_peer_input` checks the account and consent (`ToolHost::admit_input`), answers `peer/input/reject` when it refuses, and otherwise starts the turn on the peer session, one at a time.

**The person's lane.** Each conversation handle gets a new request context, opened with `share_history` (`open_handle`), plus `read_parent` when the app declares `storage.agent_workspace: "account"` (`context_reads_account`).

```mermaid
sequenceDiagram
    actor P as Person
    participant S as System agent
    participant K as octos kernel
    participant B as Calendar's broker
    participant L1 as System agent's lane
    participant L2 as Person's lane
    S->>K: peer_send_input(slug, question)
    K->>B: peer/input
    B->>B: check account and consent, queue it
    B->>K: turn/start on the peer session
    K->>L1: run the system agent's turn
    P->>B: Ask Calendar, TurnFrom with TurnTrigger::Person
    B->>K: peer/context/open with share_history, turn/start
    K->>L2: run the person's turn, in parallel
    L1-->>K: turn completed
    K-->>S: result on the blackboard, read with peer_gather
    L2-->>B: streamed events, completion
    B-->>P: reply in the Ask Calendar panel
```

**Native apps** reach a broker of the same kind through the [peer link](../crates/shell/src/peer_link/mod.rs), over a process app's hub socket or the channel a module's `OctosPeer::open` parked (`claim_peer_links`). The shell takes the app's identity from the socket or instance, never from a frame.

## 6. Where the person talks

Every surface but the system chat opens the person's lane:

| Where the person types | Code to open |
| --- | --- |
| System chat | `system_chat/session.rs`: the system session ([§4](#4-follow-a-system-chat-message)) |
| "Ask &lt;app&gt;" panel | [app_chat/mod.rs](../crates/shell/src/app_chat/mod.rs): `agents::conversation`, then `ContextOp::TurnFrom { trigger: TurnTrigger::Person }` |
| A native app's own chat, through the injected service | `OctosAppService::open_conversation`. Rinx uses only `open_context`, for its mini apps' private contexts. |
| Another native app's chat | Makepad's `OctosPeer` over the peer link: `octos.session.open` without a `client` ([peer_link/link.rs](../crates/shell/src/peer_link/link.rs)) |
| A script app's own chat | `host.request("octos.turn.start")`, served by `contained.rs` for the admitted app with agent consent and current account checks |
| A card's Chat tab, or a card that declares `sys.chat` | [glance_chat.rs](../crates/shell/src/glance_chat.rs) and [l0-chat](../crates/l0-chat/src/lib.rs) |

**Card workspaces** ([in-card chat](../README.md#in-card-chat)). [glance_sheet.rs](../crates/shell/src/glance_sheet.rs) shows an opened card full screen on the phone, centred on the desktop. If the publisher has an agent and the card declares no chat, `L0Session::for_card` ([glance_card.rs](../crates/shell/src/glance_card.rs)) adds a host-owned `WorkspaceChat` behind Card / Chat tabs. `chat_submit` sends each turn through `glance_chat::perform_bound` to the account that published the card (`agents::conversation_for_account`), with the card's data and local state as context (`ContextKind::Card`), never as tools. Mail's reply cards show Email / Chat over one saved draft; a Chat turn carries a one-use token (`drafts::issue_chat_edit`) that lets `mail.suggest_reply` save the edit ([Composed Mail cards](mail-composable-cards.md)).

Not yet: no shipped app opens the person's lane from its own UI; the shell's panel and cards are the way in.

The trigger decides how far approvals trust a turn. Only the shell's own surfaces, the "Ask &lt;app&gt;" panel and the system chat, stamp `TurnTrigger::Person`; an in-process module could through the injected service, but none does. An app's `"trigger": "person"` becomes `AppSaysPerson` (`TurnTrigger::from_args`), which the relay hands the router as an app run (`trigger_of` in `host_tools/relay.rs`); a card's chat gets the same stamp. Rinx's mini apps send a bare `ContextOp::Turn`, which is `Unknown`; standing rules skip it. Mail's new-mail events ([agent_events.rs](../crates/shell/src/agent_events.rs)) run in this lane as `TurnTrigger::Incoming`, and so do an installed Gmail app's new-mail events ([connected_events.rs](../crates/shell/src/connected_events.rs)). The shell delivers an installed app's events only when all of these hold:

- the person allowed the app's agent;
- its installed release is still admitted in the current signed local catalog (see the withdrawal checks in [section 7](#7-trace-a-tool-to-rust-code));
- its admitted `agent` block sets `background: true` and lists the trigger `<app namespace>.new_message`, where the app namespace is the last segment of the app id (Inbox Assistant's trigger is `inbox.new_message`);
- the collector verifies the admitted app; `auth` and `gmail` disclose usage rather than authorize delivery;
- its active Google connection can read Gmail.

No other app has events yet.

For "Ask Calendar", the reply streams back through the context's `EventSink`, and the panel's follower (`OctosContext::subscribe`) hears both lanes. Not yet: a script app gets no pushed events; its `octos.turn.start` returns the finished reply.

Stop works per lane: the panel's Stop ends only the person's turn (`app_chat::stop`), and the system agent's turn has its own control (`stop_system_agent`). `ContextOp::Interrupt` on a conversation ends both, since the person owns the device.

## 7. Trace a tool to Rust code

Follow `calendar.events` from its declaration to the file it reads:

1. **Declared** in [apps/calendar/bundle/tools.json](../apps/calendar/bundle/tools.json): its schemas, `risk: "read"`, `implemented_by: "host-service"`, and `shareable: true` (a caller still needs an explicit grant).
2. **Loaded** by `from_bundle` in [host_tools/script_apps.rs](../crates/shell/src/host_tools/script_apps.rs), through App Hub's digest-checking loader. `install` adds the tools to the relay's catalog, with a `HostServiceExecutor` for `os.calendar`.
3. **Registered** by `register_tools` in `broker.rs`, with what `ShellToolHost::declarations` ([host_tools/mod.rs](../crates/shell/src/host_tools/mod.rs)) returns.
4. **Called.** octos sends `peer/tool/call` on the registering link. The broker stamps the account, context and caller into a `HostToolCall` ([app-peers/src/host_tools.rs](../crates/app-peers/src/host_tools.rs)), which `ShellToolHost::tool_call` queues for the host relay. The UI normally pumps it; Android Mail jobs can pump the same synchronized relay without a window.
5. **Checked** by `Relay::handle` ([relay.rs](../crates/shell/src/host_tools/relay.rs)): the grant, consent, a signed-out account, the arguments' size and `input_schema`, then the caller's budget (by default 32 calls a turn and 1000 a day).
6. **Run.** `HostServiceExecutor::execute` dispatches a `ServiceCall` to App Hub's service registry as `os.calendar`, with no sheet. `CalendarService::call` ([apps/calendar/host-service/src/lib.rs](../apps/calendar/host-service/src/lib.rs)) loads `<apps root>/.host/calendar/events.json` (`<apps root>` is the `apps/` folder in the OctoSense home) and filters it by `from`, `to` and `limit`.
7. **Answered.** `script_apps::poll` takes the reply from App Hub's queue, `checked_reply` (in `relay.rs`) checks it against `output_schema` and a size cap, and the `ToolReply` sends `peer/tool/result` once.

`calendar.events` only reads, so nobody is asked. `calendar.remove_event` (`destructive`, `confirm: host`) is gated in octos first: the kernel raises a `host_tool` approval, which the broker hands to `ToolHost::host_tool_approval`, and only an approved call arrives.

For agent calls resolving to `glance.publish`, including aliases such as `inbox.notify`, the executor refuses raw `script`, mixed template/source payloads and executable or L1 source. An agent can select a reviewed bundle template with an `initial` data object, or supply valid declaration-only L0. Glance still checks the template's admitted bundle, publisher/account identity and resource limits before rendering; capability families are descriptive. This restriction applies to model-authored publications; an admitted foreground app retains its own reviewed Splash implementation.

An installed app's agent also depends on its exact release remaining admitted. Guidance, tool offers and system-agent input read the current signed local catalog. The broker also checks `ToolHost::admit_turn` immediately before every actual `turn/start`, including cached conversations, queued input and retries; the relay checks both tool owner and calling app again before execution, including after a pending approval. A withdrawal takes effect after the next catalog fetch, even for cached peers. The Gmail dispatcher then releases the unavailable peer and retains its unfinished event for a later authorized retry. Saved user consent is unchanged; an unavailable app is not treated as a user denial.

The App Hub UI, agent admission, Glance isolates and backend registration watcher
use the host-selected `CatalogChannel`. Legacy catalogs use `catalog.json`;
the GitHub-attested channel uses `catalog-v2.json` and verifies its proof before
admitting an app. An existing v2 cache keeps that library on v2: malformed proof
or a missing selected cache is an error, never a reason to use the legacy file.
The default remains legacy until official v2 acceptance; the operator can select
`OCTOSENSE_HUB_CATALOG=github-v2`. Both cache names and their `.lock` counterparts
are host-owned names, not app storage IDs. A new channel or changed cache causes
the backend watcher to recheck connected apps; per-call admission still applies.
If a newer verified catalog cannot be saved, agents and Glance also honor the
process's highest verified sequence and refuse the older cache until it is saved.

The relay routes every call by the tool's owner:

| Owner | Executor |
| --- | --- |
| Script app, `implemented_by: "host-service"` | `HostServiceExecutor`: the app's host service (`calendar`, `mail`, `news`), the shell's notice service for `<app>.notify`, or the shared service a store app's tool names in `host_method` (`github`, `gmail`, `gcalendar`, `glance`). For `github`, `gmail` and `gcalendar` it injects the app's active connection. |
| Script app, `implemented_by: "app"` | `ScriptAppExecutor` queues the call to App Hub's admitted full-app runner. Its `app_tool(name, call_id)` hook runs on the UI thread in the existing Splash VM and storage jail. A closed app returns `app_not_running`. |
| Native app | Its open instance: `OctosPeer::serve_tools` on its peer link, else its AI bus service ("Open … first" when closed). An executor from `OctosAppService::set_tool_executor` comes first. |
| `terminal.run` (system agent only) | The visible Terminal, over the AI bus, after a sheet with the exact command |
| `files.list`, `files.read`, `files.search`, `dev.run` | The shell itself |
| The toolbox (feature `toolbox-peers`) | The toolbox's executor |

Script tool bundles declare `requires: ["script-tools-v1"]`. The caller never
selects the VM, filesystem path, app identity, or owner account in its arguments.
The relay validates grants and schemas; the runner checks the actual running
bundle's declarations again. Only a full-app instance owns these tools, not
its Glance copy. Closing, cancellation, account changes and the bounded deadline
invalidate pending results. The handler can use `mod.app_tools.request`,
`complete`, `fail`, and `active`; see App Hub's
[script ABI](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.md#script-tool-execution-script-tools-v1).
This first version does not start closed apps or background VMs and cannot
synthesize native confirmation. It preserves host-service execution for the
existing system apps. Phone/model acceptance remains unverified until run.

### Approval order

First-use consent, a tool grant and a per-call approval are separate checks. For an approval, `Router::request` in [approvals/router.rs](../crates/shell/src/approvals/router.rs) tries, in order:

1. the external client's own prompt, for that client's turns;
2. developer mode, for the apps it covers;
3. the owning app's sheet, for a `confirm: app` tool (refused after `app_wait_s`, 120 seconds, if the app registers none);
4. a live sheet, for calls that must always ask: `auto_approvable: false`, an unknown outcome, an external connection;
5. the person's standing rules;
6. otherwise, a shell sheet with the exact arguments.

Decisions are audited in `logs/approvals-audit.jsonl`, and the deadlines are `DEFAULT_PROMPT_DEADLINE` (10 minutes) and `EXPIRY_GRACE` (30 seconds) in `app-peers/src/host_tools.rs`. The system agent cannot approve: octos refuses its `peer_respond` for approvals. What each step means is in [architecture.md §5](architecture.md#5-approvals).

Sending mail never reaches this router. `mail.propose_send` only prepares the exact message; the host's review in the card (`mail_review.rs`) calls `drafts::approve_and_send` only after a physical press (a tap on Android or a click on macOS) whose down and up events are both trusted (`trusted_user_gesture`). Synthetic and remote input are refused, and developer mode cannot stand in for that press. The macOS path is **unverified**: no real message has been sent from a Mac.

## 8. Where the data lives

An app's data lives in several stores, each with one owner:

| Data | Where | How an agent reaches it |
| --- | --- | --- |
| The account folder | `apps/<app id>/accounts/<account hash>/`, or `accounts/device/` ([app_storage/mod.rs](../crates/shell/src/app_storage/mod.rs)) | It is the peer's workspace, fixed when the peer is created. The person's lane runs in its own `contexts/<id>/` and reads the folder through `read_parent` or the host's `files.*` tools ([files.rs](../crates/shell/src/host_tools/files.rs): Unix only; 128 KiB a read, 500 entries, 100 matches). |
| A host service's data | App Hub's host directory, `<apps root>/.host/`: Calendar's events, Mail's messages and reply drafts (`drafts.rs`); under `oauth/`, the connected accounts' metadata (`connections.json`), Gmail drafts and event state, Google Calendar caches, and the operator's optional registration files: `clients.json`, which replaces the build's OAuth client registrations, and `backends.json`, which registers apps' own backends | Only through that service's tools (none exposes the registration files); no workspace includes it. |
| Transcripts and memory | octos, under the namespace `app/<app>/acct-<tag>` | The agent's own. `<tag>` is an FNV-1a hash (`account_tag`); the folder name is a different, SHA-256 hash (`account_hash`). |
| Secrets | App secrets: the keychain on macOS and iOS (indexed in `<home>/secrets/<app id>/`), elsewhere files there. AI provider keys: the macOS keychain, elsewhere files under the kernel's core directory (owner-only, except on Windows). OAuth tokens: the keychain on macOS and iOS, files encrypted with an Android Keystore key on Android, the OS credential service on Windows and Linux (`oauth-service/src/host.rs`) | Never. `app_storage::check` refuses a workspace that contains or links to them. |

`agent_workspace_in` ([host_tools/mod.rs](../crates/shell/src/host_tools/mod.rs)) gives the folder to every native app with `octos.*` services, even one that declares `storage.agent_workspace: "none"`, and to every script app that does not. Calendar's agent gets `apps/os.calendar/accounts/device/`, but its events are not there: `calendar.events` reads them from `.host/calendar/events.json` and hands the model JSON.

Signing out suspends the peer, whose calls are then answered `signed_out`. Removing the account or uninstalling the app erases it with `peer/purge` ([purge.rs](../crates/app-peers/src/purge.rs)).

## 9. Cross-app work and asking for help

Three operations are easy to confuse.

**Delegation.** The system agent asks an app's agent with `peer_send_input` ([§4](#4-follow-a-system-chat-message)), and the shell starts that turn on the existing peer. No process is spawned.

**Calling another app's tool directly.** This skips the owning app's model, so the relay allows it only when all of these hold:

1. The owner declares the tool `shareable: true` and has an executor for it.
2. The caller is granted it: by a dotted name in a script app's `agent.tools`, or by `agent.grants` in a native app's `native-apps.json` entry. `Catalog::may_call` checks this.
3. For a script bundle, App Hub's admission offers the name (`HostLimits.offered_tools`).
4. `Catalog::owner_of` finds the owner from the namespace: the toolbox, the native app of that id, or else the system app `os.<namespace>`.

Calendar shares `calendar.events`, `calendar.add_event` and `calendar.notify`; Mail requests exactly those in its manifest and the system agent has a separate explicit grant. Loading Mail also loads Calendar's admitted catalog and executor, without launching a Calendar peer or window. Mail reads the confirmed email, resolves its date/timezone, reads Calendar, adds with a stable retry key, verifies the saved event and publishes a Calendar-owned card. Scheduling requires a human request or explicit provisioned policy. Named timezones survive device timezone differences; this writes local Calendar storage, not Google Calendar. News shares `news.list` and `news.read`. Not yet: `owner_of` never resolves to a store app, so store apps cannot share tools.

**Asking for help.** [questions/mod.rs](../crates/shell/src/questions/mod.rs) routes an agent's `ask_user_question` by the turn's origin: to the system chat for a `peer/input` turn, to the app's conversation otherwise. Only the person answers, on a shell surface. System facilities reach an app's agent only as granted tools, such as the [toolbox](../crates/toolbox/README.md)'s workflows. Not yet: an app cannot start a conversation with the system agent; `OctosAppService` has no call for it.

## 10. Map the architecture to Rust execution

A peer is stored state and a turn is a group of Tokio tasks in octos; threads belong to services, not agents ([why it stays light](../README.md#why-it-stays-light-on-memory-and-cpu)).

| Layer | How it runs | Where to look |
| --- | --- | --- |
| Shell UI | The Makepad UI thread: drawing, events, and `host_tools::pump` with the relay | `lib.rs`, `host_tools/mod.rs` |
| Mail events | Two `std::thread`s: independent collection and serialized delivery with per-event retries. Android permits them while foregrounded or inside a bounded OS job. | `agent_events.rs`, `mail_background.rs` |
| Connected Gmail events | One `std::thread`, `connected-inbox-events`, which Android permits only in the foreground or inside a bounded OS job. It polls each allowed (app, connection) every 300 seconds, every 2 seconds while events are pending, and 60 seconds after a failure. Each event's turn runs on the app's peer, which gets 180 seconds to finish it. | `connected_events.rs` |
| Android Mail job | A Java JobService worker loads the same Rust host without an Activity and pumps its synchronized relay; one network-constrained periodic job, no second kernel or peer. | `phone/src/android_mail.rs`, `MailJobService.java`, `runtime_host.rs` |
| System chat | One `std::thread`, polling the kernel with `link::poll_for` | `system_chat/mod.rs`, `link.rs` |
| Kernel service | One Tokio runtime, built on first use: 2 workers, 8 MiB stacks; one supervisor task per generation | `kernel/src/lib.rs` `Inner::runtime`, `kernel.rs` `supervise` |
| App broker | A runtime per `Broker::new`, with 1 worker: the link loop, requests, retries, deadlines | `app-peers/src/broker.rs` |
| Host services | Called on the caller's thread by App Hub's `services::dispatch`: for a tool call, the relay pump caller (UI or Android Mail job), where Calendar answers. Mail (`work` threads, `mail-fetch`) and News (`news-fetch`) run network work on their own threads. The connected-account services (`auth`, `github`, `gmail`, `gcalendar`) start a thread for each request's network work. | `script_apps.rs`, `apps/*/host-service/`, `crates/oauth-service/` |
| octos, desktop and Android | Its own process, on Tokio's default runtime: one worker per CPU core (`ServeCommand::execute`) | octos `crates/octos-cli/src/commands/serve.rs` |
| octos, OpenHarmony | `serve_io` on the kernel service's runtime, over `tokio::io::duplex` | `kernel.rs` `start` |
| An octos turn | A spawned task behind a `oneshot` start barrier, then `run_standalone_turn` and its own tasks | octos `crates/octos-cli/src/api/ui_protocol_transport.rs` |

```mermaid
flowchart LR
    UI["Makepad UI thread<br/>relay, host services"] --> CMD["system-chat thread"]
    UI --> B["Broker runtime, 1 worker<br/>link loop, requests"]
    CMD --> C["Kernel Connection channels"]
    B --> C
    C --> SUP["Kernel service runtime, 2 workers<br/>generation supervisor task"]
    SUP <-->|"stdio, or the host WebSocket"| OUP["octos protocol dispatcher"]
    OUP --> T1["System agent's lane<br/>turn task"]
    OUP --> T2["Person's lane<br/>turn task"]
    T1 --> TOOL["Tool future waits<br/>for peer/tool/result"]
    T2 --> TOOL
    TOOL -->|"peer/tool/call"| B
    B -->|"host_tools::submit"| UI
```

For "Ask Calendar", octos's `handle_turn_start_with_accept` spawns the person's turn task, and its `calendar.events` future waits while the call crosses to the UI thread and back.

Three channel kinds recur: `oneshot` for one answer (a request's reply, a turn's start), `mpsc` for a mailbox (the supervisor's `Ctl` messages) and `watch` for the latest state (a generation's readiness). The broker's link loop and the supervisor are both `tokio::select!` loops, and the account generation and `link_epoch` checks drop replies meant for an old account or connection.

`Broker::bind`, `Broker::host_request` and `OctosAppService::prepare` block their caller for up to a minute, so call them off the UI thread.

### Follow App Studio from the agent to a working app

App Studio adds shell-owned tools to the existing system or app agent while developer mode covers that caller. Their [declarations and executor](../crates/shell/src/host_tools/studio.rs) use the same relay, audit and cancellation path as other host tools. A system call uses the workspace confirmed by `session/open`; an app call uses the broker-confirmed peer workspace, narrowed to its own `contexts/<id>/` for a human conversation. Arguments cannot replace that identity or choose an output directory.

An agent can write a fresh `manifest.json` and `main.splash` with its normal file tools, then follow this sequence:

| Tool | What it does |
| --- | --- |
| `studio.bundle_check {bundle_path}` | Copies bounded files from the conversation workspace, computes the digest and admits a private developer snapshot. It leaves the author's files unchanged. |
| `studio.open {bundle_path}` | Opens a visible preview with disposable app state; returns `instance_id`. |
| `studio.inspect {instance_id, offset?}` | Returns a PNG `path`, a compact page of widget selectors and checks, and `snapshot_path` for full diagnostic JSON. |
| `studio.input {instance_id, widget_id, action, …}` | Sends a real `tap`, `text` or `scroll` event to an inspected widget. Text uses `text`; scrolling uses `delta_y`. |
| `studio.close {instance_id}` | Closes the app and discards preview state. |
| `studio.install {bundle_path}` | Records a local developer install, visible in Home. Open it with `studio.open {app_id}` or its launcher tile; its own state survives closing and reopening. |
| `studio.uninstall {app_id}` | Removes one of the caller's own developer installs: receipt, snapshot, app data and owner record. Refused while an instance of it is open. |

The first full-app path accepts offline, storage-only `main.splash` bundles under `dev.studio.*`. These apps have no agent of their own, account access or `net` module, and every `host.request` is refused; the admitted instruction budget is a declaration, while the jail, quota and memory cap are enforced. Admission also refuses a `main.splash` whose text names a URL or the `{{assets}}` route; that scan is a lint, since Splash builds strings at runtime, and the offline guarantee is the isolate itself, which runs with no network hosts and no host capabilities. A bundle may contain an original launcher icon, but in-screen resource routes are not supported yet. In [studio_bundles.rs](../crates/shell/src/host_tools/studio_bundles.rs), admission limits the bundle to 128 files/directories, eight directory levels and 2 MiB total, with at most 512 KiB per file and 64 KiB for `main.splash`. The resolved policy allows at most 1 MiB of private app storage, five million script instructions and a 16 MiB heap; a lower manifest limit remains lower.

A developer install is separate from App Hub's signed catalog. Its host-private receipt binds the admitted bytes to the authoring app, account, session, context and `DevTag`. The tag identifies the developer profile and the activation that authorized it. Opening an installed app rechecks its receipt and digest. Ending that grant removes its launcher availability and stops its running instance. Another conversation cannot inspect, control or replace it. One installed app runs in one instance at a time, avoiding concurrent writes to its state.

Now follow the Rust execution boundaries:

1. The host executor runs bounded file reads and admission on a worker thread, then queues an `OpenSpec` or instrument request. It waits for a reply channel without blocking Makepad's UI thread.
2. The shell drains the launch queue into a normal window-manager client. [StudioModule](../crates/shell/src/studio/module.rs) creates the [StudioApp](../crates/shell/src/studio/apps.rs) widget and owns its shutdown. Splash evaluates the source after the private jail and admitted limits are installed. Preview writes stay in a disposable jail; installed writes stay in that app's persistent jail.
3. The UI thread builds a Makepad `WidgetTree` rooted at that app alone. Inspection reads its real rectangles and control state; input resolves a visible, enabled widget and follows the event path. Duplicate names have unique `selector` values. GPU readback uses the shell's shared ticket router; PNG compression runs on the heavy worker pool. The reply returns to the same agent call.

The model receives at most 3,800 UTF-8 bytes from `studio.inspect`. `snapshot.widgets` lists visible non-Splash widgets with exact `selector` values; pass one as `studio.input.widget_id`, rather than guessing a button's painted label. Long text/value fields carry `text_truncated` or `value_truncated`. If `next_offset` is an integer, call inspect again with that `offset` to read the next page. Each call observes the current UI, so page through a stable app state. `snapshot.checks` summarizes the pass result and finding/error counts.

The `snapshot_path` file contains the full original result, including all widgets, rectangles, geometry, tree and findings, plus the PNG path. This pretty-printed JSON is limited to 1 MiB and stored alongside the PNG in the caller's conversation workspace; `read_file` can retrieve bounded line ranges. Full diagnostics stay available without pushing selectors beyond the kernel's 4 KiB model-visible tool-output limit.

The Android HTTP remote instrument is unavailable at this pin. Studio uses its underlying Makepad widget APIs in process, scoped to its own app. The current checks detect empty geometry, clipped text and small buttons; they do not prove the app's behavior or overall UX. App inspection returns `settled: false`: it captures a current frame without claiming that an arbitrary interactive script has finished changing. The agent can call `view_image` on the returned PNG and then test the app's behavior with further inputs.

`studio.render` remains the separate L0 glance path: relative `source_path`, optional `data_path` and `dark`, with 16/32 KiB source/data limits. It uses the actual glance width and 72–440 point height bounds, a zero-quota disposable jail, and no network or host capabilities. Three matching readbacks after shader readiness produce `settled: true`; chat and image resources are explicitly unsupported. Both capture paths cap PNGs at 5 MiB and check cancellation, foreground state and the developer grant. UI requests have a 20-second deadline inside the host's 25-second wait and normal 30-second kernel call deadline.

None of this creates an agent peer or assigns one Tokio task to each app. The existing agent invokes a host tool; Rust workers handle files and encoding, and the UI thread owns widgets and GPU submission.

A fresh Task Planner authored by DeepSeek V4 Flash passed **129 tool calls on a physical OnePlus 6**: task entry/completion/filtering, disposable preview state, separate installed state, close/reopen, process restart, exact Chinese text input and scrolling. The harness did not modify app source or storage directly. Portrait app and keyboard visual review passed, with generous spacing and separate shell overlay/status-bar observations; malformed-storage/save-failure fault injection remains pending. See the [validation report](studio/oneplus6-validation.md), [ADR 0006](adr/0006-app-studio-on-the-phone.md) and the [fresh-app brief](studio/task-planner-brief.md). The `mod.studio` toolbox adapter, image generation/comparison, richer asset routes and public publishing are not implemented by this slice. After the merges of `main`, a fresh Task Planner authored by Claude passed the same acceptance flow on a stock Xiaomi (134 tool calls on the final head), and a second spool-driven check covered owned app data, `studio.uninstall` and an ended developer grant ([re-validation](studio/xiaomi-revalidation.md)).

## 11. Tests

[crates/app-peers/tests/broker.rs](../crates/app-peers/tests/broker.rs) drives the broker against a scripted kernel; its test names state the protocol's rules. Start with `a_persons_message_runs_while_the_system_agents_input_runs`, `a_lane_stop_leaves_the_other_lane_running`, `a_kernel_without_shared_history_is_refused_for_the_conversation` and `removing_an_account_purges_its_recorded_peer_and_drops_the_record`. [host_tools/scenario_tests.rs](../crates/shell/src/host_tools/scenario_tests.rs) runs the same two-lane story with a test News app end to end against the pinned octos and a scripted model; its header says how to run it.

Run the unit and scripted-connector tests from the repository root:

```sh
cargo test --locked -p octosense-kernel -p octosense-app-peers \
  --features octosense-app-peers/octos-core,octosense-app-peers/ws
```

The real-kernel tests (`crates/app-peers/tests/real_kernel.rs` and the scenario tests) print a note and pass when no kernel binary is set, so a green run is not integration evidence; the [app-peers README](../crates/app-peers/README.md#testing) shows how to give them one. Visible UI, real providers and devices need runs of their own.

Walkthroughs of the other repositories, at fixed revisions: [OctoSense App Flow](https://github.com/OctoSense-org/OctoSense-App-Flow/blob/218b25d2460d64f843932f67d419467618464fb9/docs/CODE-WALKTHROUGH.md) (formerly Design Flow), [App Hub](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/d2ca3a30ce06b0b1390cff305520962731baa1f8/docs/CODE-WALKTHROUGH.md), [OctoScript-Makepad](https://github.com/OctoSense-org/OctoScript-Makepad/blob/2cc5ef37d7d6a3d2992673389ce74488f7bb2d87/docs/architecture-walkthrough.md) and [octos](https://github.com/octos-org/octos/blob/056173e85b150e387805fc307fe231064ac1ed35/docs/octosense-integration-walkthrough.md).
