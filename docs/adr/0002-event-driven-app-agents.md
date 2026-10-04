# ADR 0002: Event-driven app agents: apps think on their own triggers and publish cards to the glance screen

- **Date:** 2026-09-27
- **Status:** Proposed; amended 2026-09-28 by [ADR 0004](0004-native-apps-hosting-and-peers.md) (sections 1, 4, 6, 10, 12 and 13; see the amendment at the end); notes dated 2026-09-29 record what has since changed; amended 2026-10-03 by [ADR 0006](0006-app-studio-on-the-phone.md) (sections 6 and 7; see the amendment at the end)
- **Scope:** How the assistant works in OctoSense when no person is typing, for any app: which agent runs, what starts it, which tools and data it may use, how it gathers information, how it produces and checks a card, and where the card and what it learned go.
- **Relates to:** [ADR 0001](0001-one-octosense-repository.md) (one repository; [`crates/kernel`](../../crates/kernel), [`crates/app-peers`](../../crates/app-peers), [`crates/ai-host`](../../crates/ai-host), [`crates/shell`](../../crates/shell)); [Home ADR 0002](home/0002-agentic-app-security-model.md) (agentic app security model), [Home ADR 0003](home/0003-app-hub-and-store.md) (App Hub), [Home ADR 0004](home/0004-system-apps-are-contained-script-apps.md) (system apps are contained script apps); [Rinx ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md) (host-owned octos app peers); octos ADR "personal memory tiers" (octos-org/octos#2365); OctoScript [`docs/ui-profile-l0.md`](https://github.com/OctoSense-org/OctoScript/blob/main/docs/ui-profile-l0.md) (the L0/L1/L2 card levels).

## Implementation amendment: Mail incoming events (2026-10-03)

The [Mail event walkthrough](../mail-agent-events.md) records the implemented
slice and verification limits. Mail has account-scoped read/sync tools, a durable
new-Inbox-message queue and an opt-in host dispatcher. The system agent can call
`agents.provision` to supplement admitted instructions with account preferences
and named skill text, and enable/disable processing. This amends section 1's
blanket statement that the system agent never writes app-agent prompts: it can
provision policy once, but it does not prompt Mail for each incoming message.

Instructions and skill text are supplied as separate host-guidance text on every
turn. This does **not** install kernel-native skills or implement the general
scheduler/model-selection/budget UI described below. New mail stays an
`Incoming` trigger; it is never relabeled as a person's request. The initial sync
is a baseline. Pending events are acknowledged only after a completed turn and
host-recorded publication or explicit no-action decision. Android process
suspension and OS notification delivery remain outside this slice.

## Context

OctoSense's premise is that the assistant is driven by time, events and changing data, not only by a person's questions. A new message, a moved meeting, a delayed flight, a weather warning, a burst of stories on a followed topic or a change in health data should make the relevant apps summarise and prepare. The person sees the result as cards and confirms only what matters.

What exists (2026-09-27):

- **One octos kernel per shell** ([`crates/kernel`](../../crates/kernel), crate `octosense-kernel`, reached through [`crates/ai-host`](../../crates/ai-host)). It starts lazily, is restarted when the AI providers change, and speaks the UI Protocol over stdio.
- **App peers** ([`crates/app-peers`](../../crates/app-peers), Rinx ADR 0007). The shell's system agent session `_main:api:octosense#system` owns one octos peer per granted app. Each peer has its own workspace, contexts and history, and its own memory namespace `app/<app>/acct-<hash>`, and it never sees provider keys. Today only a **running native module** can open contexts on its peer. Nothing wakes an app's agent while the app is closed, and contained script apps cannot reach their peer at all.
- **App Hub manifests** already declare an app's `agent`: its tools and a permission profile (`ReadOnly`, `WorkspaceWrite`, `WorkspaceWriteNeverAsk`; full access cannot be named). The kernel does not yet enforce that list per peer.
- **Host services** (`mail`, `llm`) run native code on an app's behalf and keep secrets out of apps.
- **The A2App card corpora** ([`apps/appcard/a2app`](../../apps/appcard/a2app), [`a2app-l0`](../../apps/appcard/a2app-l0)) were built for the "Ask anything" tile, where a person waits a couple of seconds. They route a request to an app type and emit one card in one pass. That trades quality for latency: the model cannot research, cross-check, try styles or look at its result. The L0 profile makes a generated card safe to render: declarations only, data from catalogued `sys.*` sources resolved by the host, no expressions, no calls. L1 adds arithmetic; L2 (imperative Splash) is refused for generated cards.
- **Research in octos** (`web_search`, `search`, the `deep-search` and `deep-crawl` skills, the `deep_research` pipeline) writes a cited Markdown report, not structured items. Result pages are fetched over plain HTTP without JavaScript; the only browser use is scraping a Bing results page when API providers fail. It has no language, recency or per-domain controls and no memory of earlier runs.
- **The makepad remote instrument** (`--remote`: `/g` frame grabs, `/snap` widget text and rectangles, `/d`, `/log`) already drives App Hub's `card-host` headlessly in OctoSense's end-to-end tests.
- **Upstream makepad's aichat** (`apps/aichat`, `libs/ai/services`) shows a proven app-to-assistant bus: apps publish a manifest of typed tools with a risk level (Read, Act, Destructive) and topics; a Destructive call waits for a Run/Cancel confirmation. Its engine is prompt-driven: it runs when someone chats.

## Decision

### 1. Each app has its own agent; the system agent supervises

Every app that asks for one gets **its own octos agent**: its app peer, with private context, history, workspace and memory. The app's agent does the thinking about that app's data and publishes that app's cards.

The **system agent does not write prompts for app agents.** It is the supervisor and the outer loop: background-run policy, budgets, the kill switch, curating the glance screen, the few cross-app insights no single app owns (for example, a delayed flight from one app combined with a meeting in another), and **improving each app agent over time** from how it performs (section 11).

**Cross-app tools, by grant** *(amended 2026-09-28, [ADR 0004](0004-native-apps-hosting-and-peers.md) §7)*. App agents **and the system agent** may call another app's tools when the calling app's manifest asks for them and they are granted. This replaces the earlier rule that the system agent never holds app tools.

- **Granted by the shell.** For a script app the shell grants them at install, from its manifest; for a native app the grant is part of its reviewed entry in `native-apps.json`. The system agent's own grants are part of its defined tool set (section 12).
- **Registered, routed and checked by the shell.** The shell registers each granted tool in the caller's tool list as a host-routed tool, marked with its owning app (`mail.send`, Mail's), routes each call to the owning app's host service or process, and checks the grant on every call.
- **Authorized by the shell, once.** A call from app A's agent, or from the system agent, to app B's tool is checked against the manifest grants; no second, agent-level consent is asked. Outward and destructive tools still need the person: a live approval (on the shell's sheet, or on the owning app's own sheet for a `confirm: app` tool, section 12), or a standing rule keyed to (owning app, tool) with the calling app shown (section 10).

The system agent still prefers to **delegate** work that needs an app's own context and judgement to that app's agent (section 6 names the `peer/input` event that carries it); a granted tool is for a bounded call, not for taking over an app's thinking.

App agents are **peers inside the shell's one kernel**, not separate kernel processes. Peers already separate workspace, memory and history, so one kernel gives the isolation. It avoids starting a 100 MB+ kernel per app on a phone, avoids copying provider keys into every app, and lets the system agent see what apps publish without a cross-kernel protocol. A separate kernel for an untrusted third-party app may be added later as a policy option, not the default.

### 2. Triggers belong to the app

An app agent wakes on the app's own triggers, never waiting for a system-agent prompt:

| Trigger | Owner | Examples |
|---|---|---|
| **Schedule** | the app's peer, as a kernel loop or cron entry owned by the peer | a morning briefing; an evening wrap-up; a weekly review |
| **Data event** | the app's host service, which holds a background handle to its app's peer | new messages arrived; an event moved; a feed updated; a sensor crossed a threshold |
| **Person** | the app's UI | the person asks inside the app |

What the agent does when woken comes from the app's **`AGENT.md`** and skills (section 3), shipped in the bundle and pinned by App Hub's approval like the rest of the app. It is not a prompt composed at run time by another agent.

### 3. Each app ships its own agent: `AGENT.md`, skills and model requirements

An app's agent is defined by the app, in its bundle, next to `manifest.json`, and pinned by App Hub with the rest of the app:

- **`AGENT.md`**: the agent's role and instructions: what to do on each trigger, what matters in this app's data, the rubric its cards must meet, and its rules for promoting memory. Written by the app's author and pinned; the system agent never edits it, but may add a local overlay on top of it (section 11).
- **Skills**: octos skills (`SKILL.md` plus manifest) that package the app's multi-step procedures (for example "write a cited digest" or "triage the inbox"). They are installed into **that app's peer workspace only**, and a skill can use only the tools the app's manifest grants; it never widens them.
- **Model requirements, not model names**: what the agent needs (tool calling, vision for card critique, long context, reasoning depth, cost tier, and *local only* for private data), optionally per task (a fast model for triage, a strong one for synthesis).

**The host selects the model** for each app peer from the providers the person configured in AI providers (octos `peer/model/set`), matching the declared requirements. The **system agent applies policy** on top: lower tiers when over budget or on battery, local-only models where an app or the person requires it, pausing. **The person can override** the choice per app in Settings. An app never names a provider or model, because it cannot know which ones the person has, and never sees keys.

### 4. Apps expose their own tools

Every app publishes a **tool manifest**: the operations that make sense for that app, typed and described so a model can use them well, much like the service manifests in upstream makepad's aichat. An app's agent works through **its app's tools**, not through raw files, sockets or generic scraping, and the same tools serve every caller.

- **Declared in `tools.json`**, next to `AGENT.md`: for each tool a name in the app's namespace (`<app>.<tool>`), a description, a JSON Schema for its input and output, a **risk level** (Read, Act or Destructive), a **confirmation owner** (`confirm`: `host` or `app`, section 12), whether it may run in the background, and whether it is shareable. It is the one source for every kind of app (section 12); App Hub, or the shell build for native modules, checks the declarations and pins them with the app.
- **Implemented where the capability lives.** Tools that need data, devices, network or secrets are implemented by the app's **host service** (native code; for example Mail's `list`, `read`, `draft_reply`, `send`), so a secret never reaches the model or the script. Tools that only reshape the app's own data may be implemented by the app itself.
- **Registered with the kernel for the app's peer.** The kernel offers the model exactly these tools plus the **system toolbox tools the app was granted** (section 6), and routes each call to its implementation, with the calling peer's identity. Results are structured, size-capped and recorded in the run's audit log.
- **Callers.** The app's own agent always. The **system agent** and **other apps' agents** only when the calling app's manifest (for the system agent, its defined tool set) asks for the tool and the shell granted it, and only the tools the app marks as shareable (for example a Calendar `free_busy` Read tool for a travel app). The shell registers, routes and checks each such call (section 1). The person's "Ask anything" assistant calls them the same way, so a request and a background run use one surface.
- **Risk decides supervision.** Read and in-app Act run unattended. An outward or destructive tool (send, post, share, buy, delete) called without a person present does not run: it becomes an **approval request** in the app's conversation (section 10), with the exact arguments, and runs only when the person approves there.

### 5. Deterministic collection, LLM thinking

Mechanical collection is **code, not a model**. Judgement is the model's.

- **Data services** (host services, native code, no model) collect the app's data on their own schedule or on the source's notifications into the app's folder, and keep a **ledger** of what was already seen. Examples: mail and calendar sync, feeds, a device's sensors or health store, files the person shared with the app. They keep working when the model, its provider or its quota is unavailable, and emit an event when something changed.
- **The app agent** (LLM) wakes on that event or on its schedule. It decides what is new and important, gathers more where needed, writes the result, produces the card, and **proposes changes to what the data service collects** (sources, topics, filters), stored as data for the next run.

### 6. The system toolbox: research and crawling are granted system services

Gathering information beyond an app's own data (searching, deep research, crawling a site, reading a page) is done by a **system toolbox** that the host and the system agent own and run. An app agent is **granted** toolbox tools and calls them, and the host executes them outside the app and its peer. The toolbox is the scoped, budgeted route, not the only one *(amended 2026-09-28, [ADR 0004](0004-native-apps-hosting-and-peers.md) §12)*: an app agent gets whatever tools its manifest declares and the person grants at install, octos's own `web_search` and `deep_search` included, with no fixed exclusions.

**The toolbox:**

| Tool | What it does | Capability |
|---|---|---|
| `toolbox.search` | one query through the provider chain (below); structured results | `research` |
| `toolbox.web_read` | render one page with a real browser and return its main text | `research` |
| `toolbox.deep_crawl` | crawl one site within limits (same site, depth, page count, path prefix) | `crawl` |
| deep research | a planned, multi-source, multi-language investigation of a topic: sub-queries, reading, cross-checking, synthesis with citations. It is a workflow template run with `workflow.run` (below), not a tool of its own, and not octos's `deep_research` pipeline | `research` |
| `card_render`, `card_critique_payload` | render and measure a card (card-studio). *Amended 2026-10-03: these are the `studio.*` host tools of [ADR 0006](0006-app-studio-on-the-phone.md) §7.* | granted to every app with an agent |
| `glance.publish` | publish a card to the glance screen | `glance` |
| `workflow.run`, `workflow.fork` | run a toolbox workflow template with parameters, or fork it into the app's own variant (below) | granted with the capabilities its steps use |
| memory search and recall | the app's own memory namespace | granted to every app with an agent |

**Granted per app, scoped.** An app asks for `research` and/or `crawl` in its manifest, with a scope: languages, regions, allowed or denied domains, maximum depth and pages, recency. App Hub checks and pins the request; the person or the store grants it, possibly narrower. The kernel offers the app's peer only the granted toolbox tools (the peer's registered tool set), and the host checks every call against the app's scope before running it.

**The scope is octos's `Scope`.** The grant's scope schema is exactly `octos_research::toolbox::Scope` (octos#2585), with its field names and units; OctoSense has no parallel definition. Unknown fields are refused. Empty lists mean no restriction.

| Field | Meaning |
|---|---|
| `langs` | BCP-47 languages the app may search in (normalized on load) |
| `regions` | ISO 3166-1 alpha-2 regions |
| `domains_allow`, `domains_deny` | domains results and reads must stay inside, and must avoid (subdomains included) |
| `max_age_days` | oldest material the app may ask for, in days; a search asking for older is clamped |
| `categories` | metasearch categories (`news`, `general`, `science`, `it`, `social`) |
| `max_results` | most results per search (default 20) |
| `max_depth`, `max_pages` | the `crawl` capability's limits for one `deep_crawl`; 0 means crawling is not granted |

The host parses a grant with `Scope::from_grant` and narrows every call with its methods (`search_args`, `research_args`, `crawl_args`, `check_domain`), the same rules octos applies to its own toolbox tools. **How much one run may do is not part of the grant:** a run's budget (articles read, `max_reads`; host calls; model calls; wall-clock time; concurrency) belongs to the workflow template, narrowed by the app's own budget. `max_pages` in the scope counts pages of one crawl, never articles a template reads.

**Executed by the host, charged to the app.** Toolbox calls are host-routed tools: the kernel sends the call to the host, which runs the engine with the app's scope and budget and writes the results as **structured items** (title, URL, source, language, date, summary, citations) into the **calling app's folder**, returning a summary and item references to the agent. The system agent charges the work to the app's budget, runs heavy jobs when it is cheap (charging, Wi-Fi), and can queue or batch jobs across apps.

**How toolbox tools reach agents** *(amended 2026-09-28, [ADR 0004](0004-native-apps-hosting-and-peers.md) §6 and §12)*. They follow the same model as other apps' granted tools (section 1): the shell registers them as host-routed tools on the agent's peer through octos's host tools per app peer (UPCR-2026-035, octos#2567), executes each call and checks it against the grant.

- `workflow.run` and `workflow.fork` for every app agent granted the capabilities the template's steps use; `toolbox.search` and `toolbox.web_read` with `research`; `toolbox.deep_crawl` with `crawl`.
- **octos's `deep_research` by grant, not by default.** Deep research in the toolbox is a workflow template, run under the app's scope and budget. octos's own pipeline runs outside that scope and budget; an app agent gets it, like any other tool, when its manifest declares it and the person grants it.
- **Registration is additive.** Registering app and toolbox tools adds them to the peer's turns; it never removes octos's peer-safe generic tools (workspace read fenced to the app's folder, the app's memory; #2567's allowlist). The shell sets that allowlist (`generic_tools`) from the app's manifest and install-time grant: octos's generic tools, its search tools included, are on it when declared and granted and off it otherwise; OctoSense adds no fixed exclusions.
- **Host-routed, never external.** Host-routed tools carry their own origin in octos (`ToolOrigin::HostRouted`, octos#2601), and a Talk to Octos external turn ([ADR 0003](0003-shared-octos-client-access.md)) keeps only builtin tools, so no toolbox or app tool reaches an external client whatever its name.
- **The system agent may receive granted tools too**, within its defined tool set (section 12).
- **Ownership is per connection.** The connection that registers a peer's tools is its tool host: it receives every `peer/tool/call` and alone answers that peer's approvals. In OctoSense that is always the shell.
- **The system agent's input to an app runs with the app's tools.** When the system agent sends input to a host-owned app peer (`peer_send_input`), octos delivers it to the shell's driving connection as a **`peer/input`** event, and the shell starts the turn with the app's tools and memory, so the app's agent answers with its full context. If the shell is not connected for that peer, octos runs nothing and says the app is not connected; the system agent waits or reports the failure visibly, never runs the turn without the app's context.

**One policy, enforced once.** Because every app goes through the toolbox, the rules below are implemented in one place and cannot be bypassed by an app:

1. **Provider chain, free first:** structured sources for the domain (feeds, public APIs and open datasets, for example RSS/Atom, RSSHub, Google News RSS or GDELT for news; official weather or transit APIs); then a SearXNG instance if configured; then a search API key the person chose to add; then search engines' own results pages for general web search (DuckDuckGo, Bing, Google; rule 3). None needs a key.
2. **The person's browser for reading and for results pages.** Pages to be cited, and results pages that need JavaScript, are loaded in the person's own browser (their profile, so their sign-ins and consent choices apply) and reduced to their main text, at a human pace.
3. **Search the web the way SearXNG does, with no person in the loop.** An agent acting for one person searches the web including search engines' own results pages (DuckDuckGo, Bing, Brave, Google) for general queries. This is **on by default**; an operator can turn it off. The results-page engines fetch those pages the way SearXNG does: where a page answers only a browser-like client (Google's page for simple phones), the engine presents one (a feature-phone user agent over a Chrome TLS fingerprint); the others identify as OctoSense. No person is asked: when a site challenges (a CAPTCHA or "unusual traffic" page), that engine is suspended for a while and the other engines answer. What is never done: automatic CAPTCHA solving and imitation of human input. Searching in the person's own browser profile remains available, off by default. Requests run at a human pace per site. (Amended 2026-09-28 and 2026-09-29; see the amendments at the end.)
4. **Provider keys and configuration live in the host** (like the AI providers' keys), never in apps or agents.
5. **A personal assistant, not a crawler: robots.txt is not applied by default.** OctoSense agents fetch on one person's behalf, so feeds, reads the person started and autonomous research all run without consulting robots.txt. What always applies instead: the identity rule of rule 3 (the person's browser identity for pages read on their behalf; calls to APIs, feeds and datasets identify OctoSense, as those services ask), polite per-host rates with backoff on 429 and 503 honouring `Retry-After`, timeouts and size caps, no bypassing paywalls or logins, and blocking of private, loopback, link-local and metadata addresses on every fetch and redirect. A deployment may turn robots.txt on as an operator setting.

Improving the toolbox (a better provider, better extraction, a new language) improves every app at once, and the outer loop (section 11) can tune how each app uses it.

**Workflow templates: procedures in the toolbox, used as-is or forked.** Besides single tools, the toolbox offers a **library of OctoScript workflow templates**: fixed, bounded procedures such as "news digest" (translate the query if needed, discover sources, read articles concurrently, summarize once with citations), weather, market, briefing, compare and travel. The first ones come from the AppCard research experiment ([`apps/appcard/tools/splash-research`](../../apps/appcard/tools/splash-research), measured in [`apps/appcard/docs/reviews/splash-research-20260909`](../../apps/appcard/docs/reviews/splash-research-20260909/README.md)): selecting and parameterizing a template takes one model call instead of a multi-call tool loop, and the host runs its independent steps concurrently (2.4–3.6× faster, 2–3 model calls down to 1).

- **Shape.** A template is OctoScript source plus a manifest: parameters and their schema, the host modules it calls (for example `mod.research` with `query`, `search`, `article`, `digest`, backed by the toolbox engine), a budget, and an output schema (structured items with sources). It runs in OctoScript's bounded evaluator; it can reach only host modules, and the host executes each call under the calling app's grants, scope and budget, with provenance (source URLs, timestamps, evidence hashes) kept by the host, never generated by the model.
- **Use as-is.** An app agent picks a template and fills its parameters; the host runs it and writes the result into the app's folder.
- **Fork to improve.** An app may copy a template into its bundle, or its agent may draft a local variant (different sources, steps, prompts or output). A fork never gains capabilities beyond the app's grants; it passes the OctoScript checker before it runs; it is pinned (by App Hub when shipped in the bundle, or as a versioned local fork like an outer-loop overlay, section 11); and it can be evaluated against the template it came from on the same inputs, and adopted only when it scores better.
- **Back upstream.** With the person's consent, a fork that keeps winning can be offered back to the toolbox library, so every app benefits.


### 7. Cards: L0, grounded, rendered and critiqued before publishing

Because no person is waiting, an app agent spends its time on quality:

1. **Content first.** The agent's findings become a **host-resolved source** the card binds to (for example `sys.digest(app: <app>, id: …)`), carrying provenance. L0's no-facts rule holds: a card states nothing it did not get from a declared source.
2. **Generate at L0** (L1 only where arithmetic is needed and declared). Try more than one layout or style.
3. **Render and inspect.** A render tool loads the card in a hidden `card-host --remote` at the target sizes (glance tile, phone, desktop) and returns the frame (`/g`), the widget snapshot (`/snap`), the log, the realize report (truncation) and the app's lint result.
4. **Critique and revise** against a rubric: measured checks (clipped or overflowing text, empty or failed data states, overlap, fits the tile) plus a vision model's judgement (legibility, hierarchy, balance, does it read as this app's card). Stop at a pass or at the run's budget.
5. **Admit** (level check, lint, approval pin) and **publish** through `glance.publish(card)`.

Heavy evaluation runs where it is cheap: on the desktop or a server, or on the phone only while charging. The phone always keeps the measured checks. *(Amended 2026-10-03, [ADR 0006](0006-app-studio-on-the-phone.md): on the phone a card renders in the shell's own process, not in `card-host --remote`, and renders the person starts are not limited to charging.)*

### 8. The glance screen is curated

App agents publish; the shell stores; the **system agent ranks and trims**. It sees published cards and shared facts, not app-private memory. It can merge related cards and defers low-value ones. Publishing is rate-limited and deduplicated per app.

**Cards are interactive** *(amended 2026-09-28)*. A glance tile runs its card under the publishing app's own resolved policy, the one the app's Card runner applies, and takes input: the person types into it, taps its buttons and sends from it as inside the app. Besides an L0/L1 `source` card, an app may publish a `script` card (a Splash program, like a script app's `main.splash`), whose `host.request` calls go out through the Card runner's path: the app's manifest grants, then each host service's own checks. `notify: true` also posts a notification that opens the card. Security hardening of this surface is deferred while the project is early (`crates/shell/src/glance.rs`, `glance_card.rs`).

### 9. Memory: private by default, promoted by rule

Each run records what it distilled into the **app's memory namespace** (octos Recall tier) through a memory ingestion call. Promotion into shared user memory (for example, an appointment other apps should know about) happens by an explicit rule in the app's `AGENT.md`, or with the person's approval.

### 10. People talk to an app's agent inside the app

Every app with an agent gets a **built-in conversation with its own agent**, drawn by the shell (the same component in every app, like a host sheet) and backed by the app's peer: the same context, history, memory, `AGENT.md`, skills and tools. It is where the person steps into the loop:

- **Approvals.** When a background run reaches an outward or destructive tool, it pauses and the shell posts an **approval request** into the app's conversation (for a `confirm: app` tool, the owning app raises its own sheet instead; section 12): what it wants to do, the exact arguments (the reply text, the event change, the recipients), why, and what happens if the person declines. The person can **approve, edit the arguments, or decline**; the run continues from where it paused, or stops and records the decision. Requests expire, and an expired request is declined.
- **Questions.** An agent that cannot decide on its own (which of two meetings to keep, which topic the person meant) asks in the conversation instead of guessing, and the run waits or continues without that step, as its `AGENT.md` says.
- **Follow-ups and steering.** The person can ask about a card ("why is this here?", "tell me more"), correct the agent ("less of this topic"), or change what it collects. Corrections become data (topics, filters, preferences) or memory, and feed the system agent's tuning of the app agent (section 11); they never edit the pinned `AGENT.md`.
- **One thread per run.** Each background run that needs the person has its own thread, so its card, the approval and the outcome stay together.

**The shell is the single path for every tool call; it draws every approval except `confirm: app`** *(amended 2026-09-28, [ADR 0004](0004-native-apps-hosting-and-peers.md) §8)*. Every app tool call, whoever makes it (the owning app's own agent, another app's agent or the system agent), goes from the kernel's `peer/tool/call` to the shell's host connection and on to the owning app's executor; an app's agent has no direct way to call its own tools. So the shell sees, authorizes (grants, budgets), audits and routes every call. For a `confirm: host` tool the shell draws the approval sheet, in the app's conversation or batched in the system chat. For a `confirm: app` tool the shell hands the confirmation to the owning app, which shows its **own sheet** (for Rinx, its send sheet) for callers of every kind; the request says who is calling (which app or agent, and its context), so the app's sheet can show it (section 12). **An agent's own text is never an approval surface**: a "yes" typed to an agent that asked in prose approves nothing. Each line of a sheet, the shell's or the app's, shows the **owning app, the tool and the exact arguments**; for a cross-app call it also shows the **calling app** (or the system agent).

**Where requests surface.** A pending approval shows on the app's glance card ("Needs you: approve reply to Alice") and as a notification; tapping either opens the app at that thread. An outward or destructive tool runs only on the person's decision: a **live approval** on a shell-drawn sheet (the owning app's own sheet for a `confirm: app` tool), or a **standing rule** the person made in advance, keyed to (owning app, tool) and evaluated by the shell on the exact arguments (ADR 0004 §8). A card or notification never approves on its own. All of this holds outside developer mode, which overrides every approval, `auto_approvable: false` included (ADR 0004 §13). The system agent may batch one request's pending `confirm: host` approvals into one shell-drawn sheet in its chat and order them across apps, but it never approves on the person's behalf.

**Person present versus absent.** With the person in the conversation, the agent may propose an outward action and run it after the person's explicit confirmation on the shell's sheet there, or on the owning app's own sheet for a `confirm: app` tool. Without the person, it only queues the request; a `confirm: app` call waits or is refused visibly, as octos#2567 specifies. The approval record (request, arguments, decision, time) goes into the run's audit log.

**Built on the UI Protocol.** The conversation uses the kernel's existing approval and question messages over the app peer's context, so octos, the shell's conversation component and other clients (octoscode-web, the TUI) handle approvals the same way.

### 11. The system agent improves app agents (the outer loop)

The system agent observes how each app agent performs and **tunes it**, without touching what App Hub pinned:

- **Base and overlay.** The app's `AGENT.md` and skills stay the pinned **base**. The system agent maintains a per-app, per-device **overlay**: extra or refined instructions, examples, rubric weights, and adjusted skill variants, applied on top of the base when the app's peer runs. The overlay is versioned.
- **What an overlay can never change:** tools, hosts, permissions, risk levels, budgets or the model policy. Those come from the manifest and host policy and are enforced by the kernel whatever the instructions say. An overlay also cannot remove the base's safety rules.
- **What it learns from:** the run log (cost, time, failures, tool errors), card critique scores, and the person's behaviour: approvals and declines, edited arguments, questions asked, corrections ("less of this"), cards opened, kept or dismissed.
- **Evaluate before adopting.** A proposed change is a diff against the current overlay. It is evaluated first, by replaying recent runs or by trying it on a share of new runs, and adopted only if its scores improve without raising cost beyond budget. Adopted changes can be rolled back automatically when later scores fall.
- **Visible and reversible.** The person sees each app's overlay history in Settings and can revert or freeze it; larger changes (a new skill variant, a changed card rubric) can require the person's approval in the app's conversation.
- **No instructions from data.** App data, web pages and messages are untrusted. The system agent writes overlays from metrics and the person's feedback, never by copying text from what the app agent read, so content cannot plant instructions (prompt injection).
- **Upstream, optionally.** With the person's consent, an improvement can be offered to the app's author as a suggestion for the next version of the pinned base.

### 12. Native modules and script apps follow one model

Everything above applies the same way to **contained script apps** (run by App Hub's Card runner, e.g. News, Mail) and **native modules** (Rust modules linked into the shell, e.g. Rinx). Only where the rules are enforced differs.

| | Contained script app | Native module |
|---|---|---|
| **Declarations** (`AGENT.md`, `skills/`, `tools.json`, agent fields) | in the bundle, pinned by App Hub | the same files as module resources, pinned by the shell build |
| **Tools** | `tools.json`; implemented by the app's host service or the app | `tools.json` is the only declaration. The app-peers broker loads it and builds its tool definitions from it; the module's Rust code only implements executors keyed by tool name. A build check requires every declared tool to have an executor and every executor to be declared. |
| **Registration with the kernel** | the shell (`crates/ai-host`) calls `peer/tools/register` for the app's peer | the broker, which owns the host-owned peer (Rinx ADR 0007), calls `peer/tools/register`; the module never talks to the kernel |
| **Enforcement** | kernel per peer, plus the Card runner's isolate and the app's grants | kernel per peer, plus the module's own code (trusted tier) |
| **Approvals** | the shell's approval sheet, in the shell's shared conversation component in the app | the shell's approval sheet, shown in the module's conversation (for Rinx, its chat); for a `confirm: app` tool (below), the module's own sheet, whoever calls |
| **Data service** | a host service (e.g. the News service) | the module's own sync (for Rinx, Matrix sync stays Rinx's code); the shell supplies the wake and the schedule |

**Who confirms a destructive tool.** `risk` and `confirm` are separate fields; confirmation is never derived from risk:

- `risk: destructive`, `confirm: host` (for example `mail.send`): the kernel's approval path and the shell's sheet, which may batch it and which a standing rule may answer. Person present or absent, the call waits for approval in the owning app's conversation (or batched in the system chat).
- `risk: destructive`, `confirm: app`: the owning app confirms it itself, whoever calls it: its own agent, another app's agent or the system agent. The shell authorizes the call as for any other, then hands the confirmation to the owning app with who is calling (which app or agent, and its context); the app's own confirmation sheet (for example, Rinx's send-message sheet) shows the caller and is the only confirmation, and the kernel does not prompt again. The person is never asked twice. The app's own sheet is the app's UI drawn by its trusted code, never the agent's text. When the owning app is not running, or nobody is present, the call waits or is refused visibly, as octos#2567 specifies.

Outside developer mode (ADR 0004 §13, which overrides every approval), in every case approvals go through the kernel's existing approval path and the shell, which draws the sheet for `confirm: host` (in the owning app's conversation or batched in the system chat) and hands `confirm: app` to the owning app (section 10), deduplicated by `<session>/<turn>/<tool_call_id>`, and the system agent cannot answer approvals for host-bound peers (octos#2560).

**The system agent's tool set is defined and enforced** *(amended 2026-09-28, [ADR 0004](0004-native-apps-hosting-and-peers.md) §12)*. The system agent is held to the same model as app agents: an explicit list of tools, in octos or in the shell's host configuration, made of its supervision tools (the peer tools, glance curation, policy and budgets), the toolbox tools it is granted, and the cross-app tools granted to it (section 1). Command execution is grantable to it as to app agents: off unless the person grants it in Settings, and each command still goes through the shell with a live approval (section 10) outside developer mode (ADR 0004 §13). Its tool set is exactly its grants, enforced by the shell, and a test starts a system-agent turn and checks that it is offered exactly that list. **Today it is not:** the system agent's turns get octos's full default tool set, shell included. That must change before cross-app grants reach it. *(2026-09-29: partly stale. Since [#117](https://github.com/OctoSense-org/OctoSense/pull/117) the shell writes a `tool_policy` denying `group:runtime` (octos's shell) into the `_main` profile before every kernel start, so the system agent no longer has octos's shell; the rest of the default set remains, a ceiling rather than the exact list, which waits for octos#2605. See the amendment of 2026-09-29.)*

**Cards.** Anything a native module publishes to the glance screen goes through `glance.publish` as an L0 card with the same `card-studio` checks. A native module's own in-app surfaces (for example, Rinx's in-room mini apps) stay under that module's authority model (Rinx ADRs 0002 and 0005) and are not routed through the glance render and critique unless they are published as glance cards.

**Follow-ups outside this ADR's first slice.**

- **Background wake for native modules.** A native module that declares `background: true` needs the shell to wake it on its schedule and run a catch-up (today Rinx stops Matrix sync when backgrounded and gets no background signal). The shell provides the wake and schedule; the module runs its own sync and emits events to its peer like a data service. This comes after the News slice.
- **Secrets in native modules.** Native modules keep secrets in the platform vault through the host, like Mail and AI providers. For Rinx this is [hagency-org/Rinx#29](https://github.com/hagency-org/Rinx/issues/29) (the Matrix access token and database passphrase move out of the session file).

### 13. What an autonomous app agent may do

Least privilege, declared by the app, checked by App Hub, granted by the host, enforced by the kernel on every call:

| Layer | Restriction | Enforced by |
|---|---|---|
| **Tools** | only the app's own `tools.json` plus the generic tools its manifest names (`agent.tools`) and the person granted at install, with no fixed exclusions; tools of other apps only where the manifest asks, the shell granted them and the owning app marks them shareable, registered by the shell and marked with the owning app; nothing else is visible to the model | kernel, per peer context (script apps and native modules alike); the shell checks every cross-app call against the grants |
| **Network** | only the manifest's declared hosts; wider information only through granted tools: the system toolbox (`research`, `crawl`) within the app's declared scope, or octos's own search tools where declared and granted | host network policy on every fetch; the toolbox checks every call against the scope |
| **Files** | only the app's folder | app jail and the kernel's per-app workspace |
| **Memory** | only `app/<app>/…`; promotion by rule or approval | kernel memory namespaces |
| **Secrets** | none; keys and passwords stay in host services | host services |
| **Risk** | each tool declares Read, Act or Destructive. Read and in-app Act run unattended; anything outward (send, post, share, buy, delete) pauses as an **approval request** on a shell-drawn sheet in the app's conversation (the owning app's own sheet for a `confirm: app` tool), surfaced on its card and as a notification, unless a standing rule keyed to (owning app, tool) answers it | kernel approval gate; the shell's sheets and approval router |
| **Model** | chosen by the host from the person's providers to meet the app's declared requirements; policy may lower the tier or force local models; the person may override | host, system agent, Settings |
| **Budgets** | tokens, run time, research depth, browser pages, runs per day, Wi-Fi/charging conditions | system agent and kernel limits |
| **Output** | cards only through `glance.publish`, only L0/L1, checked and pinned | shell |
| **Control** | background mode off per app; every run logged (trigger, tools, cost, what was published) | Settings and an audit log |

### 14. Direct one-shot model calls (`model.complete`)

*Addendum, 2026-09-27 (decision "C").* An app's AI keeps its **main path** through its own octos agent and the toolbox's workflow templates (sections 1–6): that is where tools, memory, research, budgets per run and approvals live. In addition, a contained script app may make a **narrow, direct, one-shot model call** for a bounded job that needs no agent: classify this item, title this note, pull these fields out of this message, summarize this text into three lines. Without it, such an app either wakes a whole agent for one sentence or cannot use a model at all.

**The call.** `host.request("model.complete", {task, input, schema, class?, allow_urls?})`, served by the shell's `model` host service ([`apps/ai-providers/host-service/src/complete`](../../apps/ai-providers/host-service/src/complete/mod.rs), registered by [`crates/ai-host`](../../crates/ai-host) with the `llm` service).

| Field | Meaning |
|---|---|
| `task` | what to do, in words (at most 4 KiB) |
| `input` | what to do it on: any JSON, at most 32 KiB; sent as data, and the host's instructions tell the model to follow no instruction inside it |
| `schema` | **required**: a JSON Schema (Octoscript's bounded subset: `type`, `properties`, `required`, `additionalProperties`, `items`, `minItems`, `maxItems`, `minLength`, `maxLength`, `minimum`, `maximum`, `enum`; at most 8 KiB). Any other keyword is refused, so an app never believes a rule is enforced when it is not |
| `class` | `"fast"` (default) or `"strong"`: a model **class** from octos's catalog types (`model_catalog.json`), never a provider or a model id |
| `allow_urls` | default `false`: see *Model text is data* below |

The answer is `{output, meta}`: `output` is the validated JSON; `meta` is `{class, requested, attempts, usage: {input_tokens, output_tokens, estimated}, budget}`. `model.budget` returns the caller's `budget` alone.

**The rules.**

- **A new capability, `model`, not `llm`.** `llm` stays the provider- and key-management service for `os.*` apps. `model` is its own App Hub capability ([App Hub #24](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/24)); the Card runner's isolate refuses `model.*` to an app whose policy lacks it, and the service checks the app's verified manifest again behind it.
- **The host picks the model.** It takes the person's providers in their own order (primary, then fallbacks), those whose model is of the requested class first, then the rest, and passes over a provider that fails (network, HTTP error, no text) for the next. The app never sees the provider, the model id, the route or the key. `meta.class` says which class answered; the model id is **not** shown: an app should depend on a class, not on a model, and the id stays in the host for the log and for Settings.
- **One shot.** No tools, no memory, no browsing, no conversation history: the model sees the host's fixed instructions, the task, the schema and the input, nothing else. This is not a chat and not an agent. Anything that needs tools, research, memory or approvals goes through the app's agent.
- **The reply must validate.** A reply that is not JSON, fails the schema, carries a URL where none is allowed, or exceeds the byte cap is retried **once** on the same provider, with the reason; a second failure is an error the app can show.
- **No output-token cap is sent** (the standing rule: a reasoning model spends any cap on its thinking first). The output is bounded by the schema and by a **hard cap of 16 KiB** on the reply the host accepts; the host reads at most 1 MiB of a provider's answer. Anthropic's Messages API requires `max_tokens`, so there the host sends the model's own catalog maximum, which is no cap of the host's.
- **Model text is data, never markup.** Replies an app renders go into L0/L1 card data, where they are text. With the toolbox's rule (`validate_digest`), any string in the reply containing `http://`, `https://` or `www.` refuses it, and the instructions tell the model so. An app that must return URLs (one that extracts links from its input) sets `allow_urls: true`; they are still data, and opening one still needs the `web` capability.
- **Refusals name the reason.** An error is `"<code>: <sentence>"`, with `code` one of `capability`, `no_provider`, `rate`, `budget`, `bad_request`, `invalid_output`, `too_large`, `provider`. The sentence can be shown to the person and never names a provider or a model.

**Budget.** Each app has a rate limit and a daily budget, kept by the host in a ledger (`<apps root>/.host/model/ledger.json`, outside every app's jail, so reopening an app or restarting the shell does not refill it). Defaults: **6 calls a minute, 100 calls and 100,000 tokens (input plus output, as the provider reports them) per UTC day**; at DeepSeek V4 Flash prices 100,000 tokens cost about US$0.03. A call is admitted only if its estimated input fits in what is left; both attempts of a retried call are charged. Every answer carries the budget (`calls_today`, `calls_per_day`, `tokens_today`, `tokens_per_day`, `tokens_left`, `per_minute`, `resets_at`). Per-app limits are overrides in the same ledger, and the service already lists every app's use (`ModelHost::usage`) and changes a limit (`ModelHost::set_limits`): that is what a Settings page will show and edit. The Settings page itself is a follow-up.

**Consent and privacy.** The app's inputs go to the AI provider the person configured, which is the point of the call. App Hub's consent line for `model` says so: "Sends what you give it to the AI provider you configured, for one-off answers within a daily budget; it never sees your API keys."

**One accounting path.** The toolbox's `ModelClient` (#82, `crates/toolbox/src/research/mod.rs`) will call the same host (`octosense_llm_service::complete::host()` → `ModelHost::complete`), so budgets live in one place. Its `ModelRequest {task, system, user, output_schema}` maps onto a `complete::Request` with the host-only `system` set to the template's prompt, `input` its user document, `schema` its output schema and `allow_urls: true` (the toolbox's own `validate_digest` then checks the result): the adapter is a few lines. `system` is settable only from Rust, never through `host.request`.

**Why C: the agent is the main path; a direct call for bounded one-shot jobs.** Routing every model use through the agent would make the smallest job wait for a peer, a workspace and a run, and leave contained apps without a model until their agent path exists. Making direct calls the main path would bring back per-app prompt plumbing, tool loops in scripts and spending outside the agent's accounting. The direct call adds only what small jobs need, bounded by a schema, a byte cap and a budget, and nothing an agent should do.

### Examples

The same loop serves every app; only the data service, `AGENT.md` and skills, the model requirements and the app's tools differ. A shareable tool in the table (for example `calendar.free_busy`) may also be called by another app's agent or by the system agent when it is granted to that caller (section 1); an outward one (`mail.send`, `rinx.message.send`) still waits for the person's approval, on the shell's sheet or, for a `confirm: app` tool such as `rinx.message.send`, the owning app's own sheet, which then names the calling app.

| App | Data service (no model) | App tools (examples) | Agent (on event or schedule) | Card | Needs confirmation |
|---|---|---|---|---|---|
| **News** | feeds, RSSHub, topic feeds, with a seen-items ledger | `news.list`, `news.read`, `news.topics.set`, `news.digest.write` | clusters new stories, researches the top topics across languages, writes a cited digest, proposes topics | morning/evening digest | none |
| **Mail** | mail sync | `mail.list`, `mail.read`, `mail.draft_reply`, `mail.send` (Destructive, `confirm: host`) | spots what needs a reply or a decision, drafts replies, extracts dates and tasks | "3 need you today" with drafts | sending a reply |
| **Calendar / travel** | calendar sync; flight or transit status | `calendar.list`, `calendar.free_busy` (shareable), `calendar.move` (Act, confirm) | sees a delay or conflict, works out consequences | "Your 9:00 is at risk: flight +40 min" with options | changing or declining an event |
| **Weather** | forecast and warnings feed | `weather.forecast`, `weather.alerts` | relates warnings to the person's plans and places | "Storm at 17:00 near your commute" | none |
| **Health** | the device's health store | `health.summary`, `health.trend` (Read, never shareable) | notices a trend against the person's baseline | weekly summary; a gentle flag | sharing with anyone |
| **Rinx** (Matrix; native module) | Rinx's own Matrix sync, woken by the shell (follow-up) | `rinx.rooms.list`, `rinx.unread`, `rinx.message.draft`, `rinx.message.send` (Destructive, `confirm: app`) | summarizes mentions and decisions, drafts replies | "3 mentions need you" | sending (Rinx's own sheet, whoever calls; waits or is refused visibly when Rinx is not running or nobody is present) |

```
data service (timer or notification, no model) ──▶ app folder + ledger
      │ event: something changed
      ▼
app agent (app peer)
  ├─ decide what matters, using the app's own tools; gather more from allowed sources
  ├─ write findings → sys.digest source; record in app memory
  ├─ L0 card, more than one style → render in card-host --remote → critique → revise
  ├─ admit → glance.publish (outward actions pause as approval requests in the app's conversation)
  └─ propose changes to what is collected (data) → next run
system agent: policy and budgets; rank the glance screen; cross-app insights;
              delegate to app agents (peer/input) or call its granted app tools (shell-routed);
              observe runs → evaluate → adopt overlay changes to AGENT.md/skills
```

## Consequences

- Cards get better where it matters: grounded, cross-checked, cited, in more than one style, and inspected before anyone sees them. The "Ask anything" tile remains for quick requests.
- Collection keeps working without a model; model runs happen only on change or on schedule, which bounds cost and battery.
- A generated card still cannot do anything its app cannot: L0 has no calls, and its data comes from host-resolved sources under the app's grants.
- The person can see, limit and switch off each app's background work.
- More moving parts: a data service per app, an event path, render workers, budgets and an audit log.
- Background work spends money only when the person configured a paid provider; the default works with free sources.

## Rejected alternatives

- **A separate octos kernel per app.** Too much memory per app on a phone, provider keys copied into every app, no shared view for curation. Peers give the same isolation.
- **The system agent prompts every app.** It centralises knowledge that belongs to each app, makes the system agent a bottleneck and a single point of failure, and hides what an app will do from its own manifest.
- **Let the LLM do all collection itself (for example through web search).** Slow, costly, unpredictable and often blocked; code does the mechanical part better.
- **Get past search engines' bot checks by solving them.** Automatic CAPTCHA solving and imitated human input: an arms race that escalates blocking (sometimes for the person's whole network), and the conduct search engines litigate over. Presenting a browser-like client where a results page answers only one, as SearXNG does, was adopted on 2026-09-29 (rule 3); a challenge still suspends the engine and is never solved.
- **Hand every challenge to the person** (the 2026-09-28 amendment). Rejected on 2026-09-29: it puts a person in the loop of every search, which background agents cannot have, and a cleared challenge did not stick on networks that leave through several addresses.
- **Keep the one-pass A2App router for background cards.** It was designed for latency; without a waiting person the constraint is gone.

## Implementation

In order; each step usable on its own.

1. **Kernel (octos):** host-registered tools per peer (schema, risk, routing to the host; additive, with their own origin kept out of external turns; octos#2567, #2601); the system agent's input to a host-owned peer delivered to the host as `peer/input`; enforce a peer's tool list and tool risk levels; the system agent's defined tool set (section 12); peers own schedules; a host-authorised "wake peer with event" call; memory ingestion from runs.
2. **Shell (`crates/ai-host`, `crates/app-peers`):** the built-in per-app conversation (threads per run; approvals with approve, edit and decline; questions; expiry; deep links from cards and notifications); installing each app's `AGENT.md` and skills into its peer workspace; selecting each peer's model from the person's providers under policy (`peer/model/set`), with per-app override in Settings; background peer handles for host services under host policy; `events` from data services to peers; registering each app's tools with its peer and routing calls to the host service or the app; registering, routing and checking granted cross-app tools, for app agents and the system agent; handling `peer/input`; drawing every `confirm: host` approval sheet and handing `confirm: app` confirmations, with the caller, to the owning app; budgets, kill switch and audit log in Settings.
3. **App Hub:** the bundle gains `AGENT.md`, the app's skills, its model requirements and `tools.json` (schemas, risk, `confirm`, background and shareable flags), and the manifest a background permission; admission checks and pins them. The same parser and checks are a library the app-peers broker uses for native modules' `tools.json`.
4. **Data services:** a common shape (collect, ledger, emit events) and the first services for the system apps that need them.
5. **System toolbox:** the research engine in octos (structured items, `lang`, `since`, per-domain limits, free providers and SearXNG, browser reading, robots.txt as an off-by-default operator setting, results-page search as the person, on by default, no circumvention; octos#2568) exposed as host-executed toolbox tools (`toolbox.search`, `toolbox.web_read`, `toolbox.deep_crawl`, and deep research as a template through `workflow.run`; octos's `deep_research` only where declared and granted); App Hub `research` and `crawl` capabilities with a scope; per-app budgets and queueing by the system agent.
6. **Outer loop:** per-app overlays for `AGENT.md` and skills (versioned, applied on top of the pinned base), run metrics and feedback signals, offline evaluation by replay or split trials, adoption and rollback, and the overlay history in Settings.
7. **Cards:** a `card-studio` skill (render in `card-host --remote`, measured checks, vision critique, revise within budget); `glance.publish` and the glance screen's curation.
8. **A first app end to end** through steps 1–7 (News; see *First slice: News*), then the other system apps.

## First slice: News

News drives the implementation because it needs no approvals, has free and stable data, and shows every other piece. Each milestone is usable on its own and testable on the desktop with `--remote` and on the phone.

| Milestone | What | Where |
|---|---|---|
| **M1: News data service** | A `news` host service fetches on a timer, with no model: the current feeds (HN, TechMeme, Google News), curated RSS/Atom lists, Google News RSS topic feeds and GDELT, per language. Normalized items are written to the app's folder with a seen-items ledger. Tools: `news.list`, `news.read`, `news.topics.get`, `news.topics.set`. The News bundle reads from the service instead of fetching in its script, so it opens from cache. | `apps/news/host-service`, News bundle |
| **M2: Tools and a peer for a contained app** | The bundle's tool manifest (schemas, risk, background, shareable), `AGENT.md` and model requirements. `os.news` gets an app peer; its `news.*` tools are registered with the kernel and routed to the host service. | App Hub, `crates/app-peers`, `crates/ai-host`, octos |
| **M3: Trigger and run** | Feed events ("N new items") and digest times wake the News peer. It clusters, ranks and writes a structured digest (`news.digest.write`) using only its tools, within budget, with an audit entry. | octos, `crates/ai-host`, News `AGENT.md` |
| **M4: Digest card on the glance screen** | A `sys.digest(app: news)` source; an L0 digest card spec; `glance.publish` wired to the glance feed (today `GlanceFeed::push`, marked "nothing is wired yet"), with a new glance item that renders an L0 card through the Card runner; a glance panel on the desktop. | `phone/`, `desktop/`, `crates/shell`, App Hub |
| **M5: System toolbox** | The research engine as host-executed toolbox tools (`toolbox.search`, `toolbox.web_read` and `workflow.run` for the deep-research template, granted to News by the `research` capability with a scope; `toolbox.deep_crawl` by `crawl`), registered additively as host-routed tools; structured items with citations in News's folder; free providers first, SearXNG if configured, pages read with a browser; the digest gains citations. | octos research engine, `crates/ai-host`, App Hub |
| **M6: Render and critique** | A `card-studio` skill: render in `card-host --remote`, measured checks, vision critique, two styles, revise within budget, then publish. | octos skill, App Hub `card-host` |
| **M7: Conversation and memory** | News's in-app conversation ("why this story?", "less of this topic" → topics); digest items recorded in the app's memory namespace. | `crates/shell`, octos memory |
| **M8: Outer loop, first version** | Run metrics (critique score, cards opened or dismissed, topic corrections); a News `AGENT.md` overlay proposed by the system agent, evaluated by replay, shown in Settings. | `crates/ai-host`, Settings |

Mail follows as the first app with approvals (`mail.send`, on the shell's sheet), reusing M2–M7.

## Open questions

- Where render and critique run for a phone-only user with no desktop or server.
- How the person reviews an app's `AGENT.md`, its overlay and what its data service collects.
- Which metrics define a better app agent per app, and how much evaluation (replays, trial share) the outer loop may spend.
- How model requirements are expressed (a small closed vocabulary, or octos's model hints) and how the host breaks ties between providers.
- Budget defaults per app, and how cost is shown.
- Whether a card may carry a short-lived action (Act) or only open its app's conversation.
- How approvals behave across devices (approve on the phone a run that happened on the desktop).

## Amendment, 2026-09-28 ([ADR 0004](0004-native-apps-hosting-and-peers.md))

ADR 0004 supplies the hosting and transport this ADR assumes and changes it in these places:

- **§1 (system agent and app agents).** The system agent talks to app agents through octos's peer mechanism (`peer_send_input`, the blackboard, `peer_respond`). A turn it starts on an app peer runs with the app's tools, memory and context: octos delivers that input to the shell's driving connection as `peer/input`, and the shell starts the turn (octos#2567); the app agent's questions on such turns reach the system agent through the shell's `host.ask` tool (ADR 0004 §6). *(2026-09-29: `host.ask` is dropped; the app agent asks with octos's own `ask_user_question` and the shell routes the question. See the amendment of 2026-09-29.)*
- **§1 and §4 (cross-app tools; decided on OctoSense #110).** App agents **and the system agent** may call another app's tools when the calling app's manifest asks for them and they are granted: by the shell at install for script apps, in the reviewed `native-apps.json` for native apps. This replaces "the system agent never holds app tools". The shell registers and routes each granted tool, marks it with its owning app, and checks it on every call; it authorizes cross-app calls against the grants, with no second agent-level consent. ADR 0004 §7.
- **§6 (toolbox).** Toolbox tools reach agents as host-routed tools under that model: `workflow.run` and `workflow.fork`; `toolbox.search` and `toolbox.web_read` with `research`; `toolbox.deep_crawl` with `crawl`. Beyond the toolbox, an app agent gets whatever its manifest declares and the person grants at install (octos's `web_search`, `deep_search` and other generic tools, other apps' tools, command execution), with no fixed exclusions (ADR 0004 §12). Registration is additive, host-routed tools are kept out of external turns, and the system agent may receive granted tools too.
- **§7 and §8 (cards).** App agents publish with `glance.publish` themselves; the system agent curates and announces completed cross-app requests. A card and its follow-up conversation are kept by reference (card and context id) by the app's host service.
- **§10 (approvals).** "Only the person approves" becomes "only the person approves, **live or in advance through standing rules the shell enforces**; never the system agent". Every app tool call, from the owning app's agent, another app's agent or the system agent, goes through the shell, which authorizes, audits and routes it. The shell draws the sheet for `confirm: host` tools, in the app's conversation or batched in the system chat; for `confirm: app` tools it hands the confirmation, with who is calling, to the owning app, which shows its own sheet for every caller (section 12); when the app is not running or nobody is present the call waits or is refused visibly (octos#2567). An agent's own text is never an approval surface; each line shows the owning app, the tool and the exact arguments, and for a cross-app call the calling app. The system agent may batch one request's `confirm: host` approvals into one sheet in its chat. Standing rules are keyed to (owning app, tool), conditioned, capped, time-boxed at their broadest, notified and audited; tools marked `auto_approvable: false`, unknown outcomes and (by default) runs started by incoming content always ask. All of this holds outside developer mode, which overrides every approval, `auto_approvable: false` included, behind its banner, audit, gate and developer profile, and never for external clients (ADR 0004 §13). ADR 0004 §8.
- **§12 (native modules and script apps).** How each hosting kind reaches its agent (injection, the peer link, `host.request`) is ADR 0004 §4 and §5; every app's agent needs the person's consent at first use. The system agent's tool set is its grants (command execution only if the person grants it in Settings, each command approved live), enforced by the shell, with a test; today its turns get octos's full default tool set, shell included, which must change (ADR 0004 §12). *(2026-09-29: octos's shell is denied since #117; see the amendment of 2026-09-29.)*
- **§13 (autonomy).** Adds the `auto_approvable` flag and the incoming-content rule.

## Amendment, 2026-09-28 (search the web as the person)

The maintainer changed §6 rules 1, 2, 3 and 5 on 2026-09-28, in two steps:

1. **Results pages are on by default.** Search engines' results pages (DuckDuckGo, Bing, Google) are the last tier of the free provider chain for general web search; an operator can turn them off (octos: `OCTOS_ALLOW_SERP_SCRAPE=0`). Without them app agents had no general web search: the free, key-less sources cover news, papers, code, Q&A, encyclopedias and social posts, not "the web".
2. **The agent acts as the person, not as a robot.** An agent has no identity of its own; it acts for one person. Searches and page reads made for the person therefore carry the person's browser identity: they run in the person's own browser (profile, user agent, cookies, sign-ins), not as a separately labelled crawler. Machine-to-machine calls to APIs, feeds and datasets keep identifying OctoSense, as those services ask.

What stays excluded is circumvention: forged fingerprints, hidden browser automation, imitated human input and automatic CAPTCHA solving. A challenge goes to the person to solve in their browser, or the attempt ends when nobody is present. Requests keep a human pace per site. A search engine's terms may not allow automated queries; like robots.txt (rule 5), that is weighed against OctoSense acting for one person, and an operator who needs stricter behaviour turns results-page search off.

Implementation in octos: results-page search on by default (octos#2607), then searches and page reads through the person's browser profile with challenges handed to the person, and clean-room DuckDuckGo, Bing and Google engines in the octos metasearch under these rules. *(2026-09-29: step 2 and "a challenge goes to the person" are superseded for results pages by the amendment "search the way SearXNG does" below.)*

## Amendment, 2026-09-29 (ADR 0004 gap review)

Facts that changed after the 2026-09-28 amendment, and two decisions made in ADR 0004 on 2026-09-29:

- **§10 and §12 (the system agent's tool set).** Since [#117](https://github.com/OctoSense-org/OctoSense/pull/117) the shell writes a `tool_policy` denying `group:runtime` (`shell`, `bash`, `exec_command`, `write_stdin`) into the `_main` profile before every kernel start (`crates/kernel/src/system_tools.rs`, `enforce`). The system agent and the app peers (sessions on the same profile) therefore no longer get octos's shell (when the policy is written: on a foreign policy or the person's own octos home `enforce` refuses and the kernel still starts without it, a known gap). The policy is the ceiling; since the octos 4a3ec9f9 pin every kernel start also sets the system session's exact kernel tool list (`session/tool_list/set`, octos#2648), and the exact-list real-kernel test runs. Command execution for the system agent is the Setup → Assistant → Command execution switch ([#132](https://github.com/OctoSense-org/OctoSense/pull/132)), which registers the host tool `terminal.run`, each command approved live.
- **§1 (questions).** `host.ask` is dropped (ADR 0004 §6, decided 2026-09-29). octos keeps `ask_user_question` on host-driven turns, so app agents and the system agent ask with it, and the shell routes each question to the app's own conversation, or to the system chat for system-agent turns (including `peer/input` turns).
- **§10 (command execution).** `terminal.run` exists only where the Terminal runs as its own sandboxed process: macOS, Windows, and Linux with Vulkan and Wayland. The in-process Terminal is read-only (ADR 0004 §10, decided 2026-09-29).

## Amendment, 2026-09-29 (search the way SearXNG does)

The maintainer changed §6 rule 3 again on 2026-09-29: **search exactly the way SearXNG does, with no person in the loop.** This replaces the 2026-09-28 step "the agent acts as the person" for results pages and its rule that challenges go to the person.

- **Browser-like client where a page requires one.** Google's results page answers plain clients (an honest user agent, curl, a current browser fingerprint) with an "unusual traffic" page. SearXNG gets results from Google's page for simple phones (`/wml/search`) with a feature-phone user agent, Chrome-style request headers and a Chrome TLS fingerprint. octos does the same for its Google engine (octos#2619, the `legacy_mobile` client). How SearXNG does it was learned black-box, from its network traffic and its installed HTTP client; no SearXNG code was read, so the metasearch stays clean-room. The other results-page engines and all API, feed and page-read requests keep identifying OctoSense.
- **No person in the loop.** A challenge suspends that engine for a while; the other engines answer. Nothing is shown to the person and nothing is solved.
- **The person's browser profile is opt-in.** Searching and reading in the person's browser (octos#2615) stays available for engines that need a real browser, off by default (`OCTOS_BROWSER`). When used, results say it and how to turn it off.
- **Still excluded:** automatic CAPTCHA solving and imitated human input.
- **Terms.** Search engines' terms may not allow automated queries; Google's and Bing's are the highest risk. As with robots.txt (rule 5), that is weighed against OctoSense acting for one person. The disclosure travels with the feature (octos's docs, the default-on notice and each engine's manifest), and an operator who needs stricter behaviour turns results-page search off (`OCTOS_ALLOW_SERP_SCRAPE=0`).

## Amendment, 2026-10-03 ([ADR 0006](0006-app-studio-on-the-phone.md))

ADR 0006 brings rendering, checking and critique onto the phone and changes this ADR in two places:

- **§6 (toolbox).** `card_render` and `card_critique_payload` become the shell's `studio.*` host tools (`studio.render`, `studio.check`, `studio.compare`, `studio.critique_payload`, `studio.bundle_check`, `studio.install`). In their first release they are granted only while developer mode is on. App Hub's `card-studio` stays the one implementation of the checks and the critique payload, now as a library both renderers feed.
- **§7 (cards).** On the phone, step 3 renders in the shell's own process, through the target surface's own lowering and size, instead of a hidden `card-host --remote`. The rule that heavy evaluation runs on the phone only while charging does not apply to renders the person starts.

