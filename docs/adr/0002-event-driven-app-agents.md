# ADR 0002: Event-driven app agents: apps think on their own triggers and publish cards to the glance screen

- **Date:** 2026-09-27
- **Status:** Proposed
- **Scope:** How the assistant works in OctoSense when no person is typing, for any app: which agent runs, what starts it, which tools and data it may use, how it gathers information, how it produces and checks a card, and where the card and what it learned go.
- **Relates to:** [ADR 0001](0001-one-octosense-repository.md) (one repository; [`crates/kernel`](../../crates/kernel), [`crates/app-peers`](../../crates/app-peers), [`crates/ai-host`](../../crates/ai-host), [`crates/shell`](../../crates/shell)); [Home ADR 0002](home/0002-agentic-app-security-model.md) (agentic app security model), [Home ADR 0003](home/0003-app-hub-and-store.md) (App Hub), [Home ADR 0004](home/0004-system-apps-are-contained-script-apps.md) (system apps are contained script apps); [Rinx ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md) (host-owned octos app peers); octos ADR "personal memory tiers" (octos-org/octos#2365); OctoScript [`docs/ui-profile-l0.md`](https://github.com/OctoSense-org/OctoScript/blob/main/docs/ui-profile-l0.md) (the L0/L1/L2 card levels).

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
- **Callers.** The app's own agent always. The **system agent** and **other apps' agents** only where the host grants it, and only the tools the app marks as shareable (for example a Calendar `free_busy` Read tool for a travel app). The person's "Ask anything" assistant calls them the same way, so a request and a background run use one surface.
- **Risk decides supervision.** Read and in-app Act run unattended. An outward or destructive tool (send, post, share, buy, delete) called without a person present does not run: it becomes an **approval request** in the app's conversation (section 10), with the exact arguments, and runs only when the person approves there.

### 5. Deterministic collection, LLM thinking

Mechanical collection is **code, not a model**. Judgement is the model's.

- **Data services** (host services, native code, no model) collect the app's data on their own schedule or on the source's notifications into the app's folder, and keep a **ledger** of what was already seen. Examples: mail and calendar sync, feeds, a device's sensors or health store, files the person shared with the app. They keep working when the model, its provider or its quota is unavailable, and emit an event when something changed.
- **The app agent** (LLM) wakes on that event or on its schedule. It decides what is new and important, gathers more where needed, writes the result, produces the card, and **proposes changes to what the data service collects** (sources, topics, filters), stored as data for the next run.

### 6. The system toolbox: research and crawling are granted system services

Gathering information beyond an app's own data (searching, deep research, crawling a site, reading a page) is done by a **system toolbox** that the host and the system agent own and run. An app agent does not search, crawl or drive a browser itself; it is **granted** toolbox tools and calls them, and the host executes them outside the app and its peer.

**The toolbox:**

| Tool | What it does | Capability |
|---|---|---|
| `search` | one query through the provider chain (below); structured results | `research` |
| `deep_research` | a planned, multi-source, multi-language investigation of a topic: sub-queries, reading, cross-checking, synthesis with citations | `research` |
| `web_read` | render one page with a real browser and return its main text | `research` |
| `deep_crawl` | crawl one site within limits (same site, depth, page count, path prefix) | `crawl` |
| `card_render`, `card_critique_payload` | render and measure a card (card-studio) | granted to every app with an agent |
| `glance.publish` | publish a card to the glance screen | `glance` |
| memory search and recall | the app's own memory namespace | granted to every app with an agent |

**Granted per app, scoped.** An app asks for `research` and/or `crawl` in its manifest, with a scope: languages, regions, allowed or denied domains, maximum depth and pages, recency. App Hub checks and pins the request; the person or the store grants it, possibly narrower. The kernel offers the app's peer only the granted toolbox tools (the peer's registered tool set), and the host checks every call against the app's scope before running it.

**Executed by the host, charged to the app.** Toolbox calls are host-routed tools: the kernel sends the call to the host, which runs the engine with the app's scope and budget and writes the results as **structured items** (title, URL, source, language, date, summary, citations) into the **calling app's folder**, returning a summary and item references to the agent. The system agent charges the work to the app's budget, runs heavy jobs when it is cheap (charging, Wi-Fi), and can queue or batch jobs across apps.

**One policy, enforced once.** Because every app goes through the toolbox, the rules below are implemented in one place and cannot be bypassed by an app:

1. **Provider chain, free first:** structured sources for the domain (feeds, public APIs and open datasets, for example RSS/Atom, RSSHub, Google News RSS or GDELT for news; official weather or transit APIs); then a SearXNG instance if configured; then a search API key the person chose to add. None is required.
2. **A real browser for reading, not searching.** Pages to be cited are rendered and reduced to their main text, at a polite rate, respecting robots.txt, with an honest user agent.
3. **No disguised search.** OctoSense does not make automated search pass as a person to get around a search engine's bot detection: no stealth fingerprinting, human-behaviour imitation, spoofed browser user agents or CAPTCHA solving, and no scraping of search results pages by default. Results-page scrapers exist only behind an explicit, off-by-default operator flag.
4. **Provider keys and configuration live in the host** (like the AI providers' keys), never in apps or agents.

Improving the toolbox (a better provider, better extraction, a new language) improves every app at once, and the outer loop (section 11) can tune how each app uses it.

### 7. Cards: L0, grounded, rendered and critiqued before publishing

Because no person is waiting, an app agent spends its time on quality:

1. **Content first.** The agent's findings become a **host-resolved source** the card binds to (for example `sys.digest(app: <app>, id: …)`), carrying provenance. L0's no-facts rule holds: a card states nothing it did not get from a declared source.
2. **Generate at L0** (L1 only where arithmetic is needed and declared). Try more than one layout or style.
3. **Render and inspect.** A render tool loads the card in a hidden `card-host --remote` at the target sizes (glance tile, phone, desktop) and returns the frame (`/g`), the widget snapshot (`/snap`), the log, the realize report (truncation) and the app's lint result.
4. **Critique and revise** against a rubric: measured checks (clipped or overflowing text, empty or failed data states, overlap, fits the tile) plus a vision model's judgement (legibility, hierarchy, balance, does it read as this app's card). Stop at a pass or at the run's budget.
5. **Admit** (level check, lint, approval pin) and **publish** through `glance.publish(card)`.

Heavy evaluation runs where it is cheap: on the desktop or a server, or on the phone only while charging. The phone always keeps the measured checks.

### 8. The glance screen is curated

App agents publish; the shell stores; the **system agent ranks and trims**. It sees published cards and shared facts, not app-private memory. It can merge related cards and defers low-value ones. Publishing is rate-limited and deduplicated per app.

### 9. Memory: private by default, promoted by rule

Each run records what it distilled into the **app's memory namespace** (octos Recall tier) through a memory ingestion call. Promotion into shared user memory (for example, an appointment other apps should know about) happens by an explicit rule in the app's `AGENT.md`, or with the person's approval.

### 10. People talk to an app's agent inside the app

Every app with an agent gets a **built-in conversation with its own agent**, drawn by the shell (the same component in every app, like a host sheet) and backed by the app's peer: the same context, history, memory, `AGENT.md`, skills and tools. It is where the person steps into the loop:

- **Approvals.** When a background run reaches an outward or destructive tool, it pauses and posts an **approval request** into the app's conversation: what it wants to do, the exact arguments (the reply text, the event change, the recipients), why, and what happens if the person declines. The person can **approve, edit the arguments, or decline**; the run continues from where it paused, or stops and records the decision. Requests expire, and an expired request is declined.
- **Questions.** An agent that cannot decide on its own (which of two meetings to keep, which topic the person meant) asks in the conversation instead of guessing, and the run waits or continues without that step, as its `AGENT.md` says.
- **Follow-ups and steering.** The person can ask about a card ("why is this here?", "tell me more"), correct the agent ("less of this topic"), or change what it collects. Corrections become data (topics, filters, preferences) or memory, and feed the system agent's tuning of the app agent (section 11); they never edit the pinned `AGENT.md`.
- **One thread per run.** Each background run that needs the person has its own thread, so its card, the approval and the outcome stay together.

**Where requests surface.** A pending approval shows on the app's glance card ("Needs you: approve reply to Alice") and as a notification; tapping either opens the app at that thread. Approving in the conversation is the only way an outward or destructive tool runs; a card or notification never approves on its own. The system agent may batch and order pending requests across apps, but it never approves on the person's behalf.

**Person present versus absent.** With the person in the conversation, the agent may propose an outward action and run it after the person's explicit confirmation there. Without the person, it only queues the request. The approval record (request, arguments, decision, time) goes into the run's audit log.

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
| **Approvals** | the shell's shared conversation component in the app | the module's own conversation UI (for Rinx, its chat) |
| **Data service** | a host service (e.g. the News service) | the module's own sync (for Rinx, Matrix sync stays Rinx's code); the shell supplies the wake and the schedule |

**Who confirms a destructive tool.** `risk` and `confirm` are separate fields; confirmation is never derived from risk:

- `risk: destructive`, `confirm: host`: the kernel's approval path. Person present or absent, the call waits for approval in the owning app's conversation.
- `risk: destructive`, `confirm: app`: the app confirms it itself. With the person present, the app's own confirmation sheet is the only confirmation (for example, Rinx's send-message sheet), and the kernel does not prompt again. With the person absent, it becomes an approval request in the owning app's conversation. The person is never asked twice.

In every case approvals go only to the owning app's conversation through the kernel's existing approval path, deduplicated by `<session>/<turn>/<tool_call_id>`, and the system agent cannot answer approvals for host-bound peers (octos#2560).

**Cards.** Anything a native module publishes to the glance screen goes through `glance.publish` as an L0 card with the same `card-studio` checks. A native module's own in-app surfaces (for example, Rinx's in-room mini apps) stay under that module's authority model (Rinx ADRs 0002 and 0005) and are not routed through the glance render and critique unless they are published as glance cards.

**Follow-ups outside this ADR's first slice.**

- **Background wake for native modules.** A native module that declares `background: true` needs the shell to wake it on its schedule and run a catch-up (today Rinx stops Matrix sync when backgrounded and gets no background signal). The shell provides the wake and schedule; the module runs its own sync and emits events to its peer like a data service. This comes after the News slice.
- **Secrets in native modules.** Native modules keep secrets in the platform vault through the host, like Mail and AI providers. For Rinx this is [hagency-org/Rinx#29](https://github.com/hagency-org/Rinx/issues/29) (the Matrix access token and database passphrase move out of the session file).

### 13. What an autonomous app agent may do

Least privilege, declared by the app, checked by App Hub, granted by the host, enforced by the kernel on every call:

| Layer | Restriction | Enforced by |
|---|---|---|
| **Tools** | only the app's own `tools.json` plus the generic tools its manifest names (`agent.tools`); tools of other apps only where granted and marked shareable; nothing else is visible to the model | kernel, per peer context (script apps and native modules alike) |
| **Network** | only the manifest's declared hosts; wider information only through granted system toolbox tools (`research`, `crawl`), within the app's declared scope | host network policy on every fetch; the toolbox checks every call against the scope |
| **Files** | only the app's folder | app jail and the kernel's per-app workspace |
| **Memory** | only `app/<app>/…`; promotion by rule or approval | kernel memory namespaces |
| **Secrets** | none; keys and passwords stay in host services | host services |
| **Risk** | each tool declares Read, Act or Destructive. Read and in-app Act run unattended; anything outward (send, post, share, buy, delete) pauses as an **approval request** in the app's conversation, surfaced on its card and as a notification | kernel approval gate; the app's conversation |
| **Model** | chosen by the host from the person's providers to meet the app's declared requirements; policy may lower the tier or force local models; the person may override | host, system agent, Settings |
| **Budgets** | tokens, run time, research depth, browser pages, runs per day, Wi-Fi/charging conditions | system agent and kernel limits |
| **Output** | cards only through `glance.publish`, only L0/L1, checked and pinned | shell |
| **Control** | background mode off per app; every run logged (trigger, tools, cost, what was published) | Settings and an audit log |

### Examples

The same loop serves every app; only the data service, `AGENT.md` and skills, the model requirements and the app's tools differ.

| App | Data service (no model) | App tools (examples) | Agent (on event or schedule) | Card | Needs confirmation |
|---|---|---|---|---|---|
| **News** | feeds, RSSHub, topic feeds, with a seen-items ledger | `news.list`, `news.read`, `news.topics.set`, `news.digest.write` | clusters new stories, researches the top topics across languages, writes a cited digest, proposes topics | morning/evening digest | none |
| **Mail** | mail sync | `mail.list`, `mail.read`, `mail.draft_reply`, `mail.send` (Destructive) | spots what needs a reply or a decision, drafts replies, extracts dates and tasks | "3 need you today" with drafts | sending a reply |
| **Calendar / travel** | calendar sync; flight or transit status | `calendar.list`, `calendar.free_busy` (shareable), `calendar.move` (Act, confirm) | sees a delay or conflict, works out consequences | "Your 9:00 is at risk: flight +40 min" with options | changing or declining an event |
| **Weather** | forecast and warnings feed | `weather.forecast`, `weather.alerts` | relates warnings to the person's plans and places | "Storm at 17:00 near your commute" | none |
| **Health** | the device's health store | `health.summary`, `health.trend` (Read, never shareable) | notices a trend against the person's baseline | weekly summary; a gentle flag | sharing with anyone |
| **Rinx** (Matrix; native module) | Rinx's own Matrix sync, woken by the shell (follow-up) | `rinx.rooms.list`, `rinx.unread`, `rinx.message.draft`, `rinx.message.send` (Destructive, `confirm: app`) | summarizes mentions and decisions, drafts replies | "3 mentions need you" | sending (Rinx's own sheet if present, else an approval request in Rinx) |

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
- **Drive search engines with a disguised browser.** Against the engines' terms, and unreliable because blocking escalates. A browser is used for reading pages only.
- **Keep the one-pass A2App router for background cards.** It was designed for latency; without a waiting person the constraint is gone.

## Implementation

In order; each step usable on its own.

1. **Kernel (octos):** host-registered tools per peer (schema, risk, routing to the host); enforce a peer's tool list and tool risk levels; peers own schedules; a host-authorised "wake peer with event" call; memory ingestion from runs.
2. **Shell (`crates/ai-host`, `crates/app-peers`):** the built-in per-app conversation (threads per run; approvals with approve, edit and decline; questions; expiry; deep links from cards and notifications); installing each app's `AGENT.md` and skills into its peer workspace; selecting each peer's model from the person's providers under policy (`peer/model/set`), with per-app override in Settings; background peer handles for host services under host policy; `events` from data services to peers; registering each app's tools with its peer and routing calls to the host service or the app; budgets, kill switch and audit log in Settings.
3. **App Hub:** the bundle gains `AGENT.md`, the app's skills, its model requirements and `tools.json` (schemas, risk, `confirm`, background and shareable flags), and the manifest a background permission; admission checks and pins them. The same parser and checks are a library the app-peers broker uses for native modules' `tools.json`.
4. **Data services:** a common shape (collect, ledger, emit events) and the first services for the system apps that need them.
5. **System toolbox:** the research engine in octos (structured items, `lang`, `since`, per-domain limits, free providers and SearXNG, browser reading, robots.txt, no disguised scraping; octos#2568) exposed as host-executed toolbox tools (`search`, `deep_research`, `web_read`, `deep_crawl`); App Hub `research` and `crawl` capabilities with a scope; per-app budgets and queueing by the system agent.
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
| **M5: System toolbox** | The research engine as host-executed toolbox tools (`search`, `deep_research`, `web_read`, `deep_crawl`) granted to News by the `research` capability with a scope; structured items with citations in News's folder; free providers first, SearXNG if configured, pages read with a browser; the digest gains citations. | octos research engine, `crates/ai-host`, App Hub |
| **M6: Render and critique** | A `card-studio` skill: render in `card-host --remote`, measured checks, vision critique, two styles, revise within budget, then publish. | octos skill, App Hub `card-host` |
| **M7: Conversation and memory** | News's in-app conversation ("why this story?", "less of this topic" → topics); digest items recorded in the app's memory namespace. | `crates/shell`, octos memory |
| **M8: Outer loop, first version** | Run metrics (critique score, cards opened or dismissed, topic corrections); a News `AGENT.md` overlay proposed by the system agent, evaluated by replay, shown in Settings. | `crates/ai-host`, Settings |

Mail follows as the first app with approvals (`mail.send`), reusing M2–M7.

## Open questions

- Where render and critique run for a phone-only user with no desktop or server.
- How the person reviews an app's `AGENT.md`, its overlay and what its data service collects.
- Which metrics define a better app agent per app, and how much evaluation (replays, trial share) the outer loop may spend.
- How model requirements are expressed (a small closed vocabulary, or octos's model hints) and how the host breaks ties between providers.
- Budget defaults per app, and how cost is shown.
- Whether a card may carry a short-lived action (Act) or only open its app's conversation.
- How approvals behave across devices (approve on the phone a run that happened on the desktop).
