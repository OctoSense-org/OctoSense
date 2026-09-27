# ADR 0002: Event-driven app agents: apps think on their own triggers and publish cards to the glance screen

- **Date:** 2026-09-27
- **Status:** Proposed
- **Scope:** How the assistant works in OctoSense when no person is typing, for any app: which agent runs, what starts it, which tools and data it may use, how it gathers information, how it produces and checks a card, and where the card and what it learned go.
- **Relates to:** [ADR 0001](0001-one-octosense-repository.md) (one repository; [`crates/kernel`](../../crates/kernel), [`crates/app-peers`](../../crates/app-peers), the planned `crates/ai-host`); the phone shell's earlier ADRs, moving to `docs/adr/home/`: 0002 (agentic app security model), 0003 (App Hub), 0004 (system apps are contained script apps); [Rinx ADR 0007](https://github.com/hagency-org/Rinx/blob/main/docs/adr/0007-host-owned-octos-app-peers.md) (host-owned octos app peers); octos ADR "personal memory tiers" (octos-org/octos#2365); OctoScript [`docs/ui-profile-l0.md`](https://github.com/OctoSense-org/OctoScript/blob/main/docs/ui-profile-l0.md) (the L0/L1/L2 card levels).

## Context

OctoSense's premise is that the assistant is driven by time, events and changing data, not only by a person's questions. A new message, a moved meeting, a delayed flight, a weather warning, a burst of stories on a followed topic or a change in health data should make the relevant apps summarise and prepare. The person sees the result as cards and confirms only what matters.

What exists (2026-09-27):

- **One octos kernel per shell** ([`crates/kernel`](../../crates/kernel); crate `octosense-octos-core`, to be renamed `octosense-kernel`). It starts lazily, is restarted when the AI providers change, and speaks the UI Protocol over stdio.
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

The **system agent does not write prompts for app agents.** It is the supervisor: background-run policy, budgets, the kill switch, curating the glance screen, and the few cross-app insights no single app owns (for example, a delayed flight from one app combined with a meeting in another).

App agents are **peers inside the shell's one kernel**, not separate kernel processes. Peers already separate workspace, memory and history, so one kernel gives the isolation. It avoids starting a 100 MB+ kernel per app on a phone, avoids copying provider keys into every app, and lets the system agent see what apps publish without a cross-kernel protocol. A separate kernel for an untrusted third-party app may be added later as a policy option, not the default.

### 2. Triggers belong to the app

An app agent wakes on the app's own triggers, never waiting for a system-agent prompt:

| Trigger | Owner | Examples |
|---|---|---|
| **Schedule** | the app's peer, as a kernel loop or cron entry owned by the peer | a morning briefing; an evening wrap-up; a weekly review |
| **Data event** | the app's host service, which holds a background handle to its app's peer | new messages arrived; an event moved; a feed updated; a sensor crossed a threshold |
| **Person** | the app's UI | the person asks inside the app |

What the agent does when woken comes from the app's **agent spec**, shipped in the bundle next to `manifest.json` and pinned by App Hub's approval like the rest of the app. It is not a prompt composed at run time by another agent.

### 3. Deterministic collection, LLM thinking

Mechanical collection is **code, not a model**. Judgement is the model's.

- **Data services** (host services, native code, no model) collect the app's data on their own schedule or on the source's notifications into the app's folder, and keep a **ledger** of what was already seen. Examples: mail and calendar sync, feeds, a device's sensors or health store, files the person shared with the app. They keep working when the model, its provider or its quota is unavailable, and emit an event when something changed.
- **The app agent** (LLM) wakes on that event or on its schedule. It decides what is new and important, gathers more where needed, writes the result, produces the card, and **proposes changes to what the data service collects** (sources, topics, filters), stored as data for the next run.

### 4. External information: free sources first, APIs by choice, a browser for reading, no disguised search

When an agent needs information beyond its app's data, it uses legitimate channels, cheapest and most stable first:

1. Structured sources for the app's domain: feeds, public APIs and open datasets (for example RSS/Atom, RSSHub, Google News RSS or GDELT for news; official weather or transit APIs). These are free, predictable and often multi-language.
2. A self-hosted SearXNG, if configured, at personal volume.
3. A search API key the person chooses to add (free tier or paid). None is required.
4. A **real browser for reading, not searching.** Pages the agent will cite are rendered (the existing octos `browser` tool, or Playwright/CDP), then reduced to their main text, at a polite rate that respects robots.txt.

OctoSense does **not** make automated search pass as a person to get around a search engine's bot detection: no stealth fingerprinting, human-behaviour imitation or CAPTCHA solving, and no scraping of search results pages. The existing "hide automation" code in octos `deep-crawl` is reviewed against this rule.

Research tools return **structured items** (title, URL, source, language, date, summary, citations) written into the app's folder, not only a Markdown report.

### 5. Cards: L0, grounded, rendered and critiqued before publishing

Because no person is waiting, an app agent spends its time on quality:

1. **Content first.** The agent's findings become a **host-resolved source** the card binds to (for example `sys.digest(app: <app>, id: …)`), carrying provenance. L0's no-facts rule holds: a card states nothing it did not get from a declared source.
2. **Generate at L0** (L1 only where arithmetic is needed and declared). Try more than one layout or style.
3. **Render and inspect.** A render tool loads the card in a hidden `card-host --remote` at the target sizes (glance tile, phone, desktop) and returns the frame (`/g`), the widget snapshot (`/snap`), the log, the realize report (truncation) and the app's lint result.
4. **Critique and revise** against a rubric: measured checks (clipped or overflowing text, empty or failed data states, overlap, fits the tile) plus a vision model's judgement (legibility, hierarchy, balance, does it read as this app's card). Stop at a pass or at the run's budget.
5. **Admit** (level check, lint, approval pin) and **publish** through `glance.publish(card)`.

Heavy evaluation runs where it is cheap: on the desktop or a server, or on the phone only while charging. The phone always keeps the measured checks.

### 6. The glance screen is curated

App agents publish; the shell stores; the **system agent ranks and trims**. It sees published cards and shared facts, not app-private memory. It can merge related cards and defers low-value ones. Publishing is rate-limited and deduplicated per app.

### 7. Memory: private by default, promoted by rule

Each run records what it distilled into the **app's memory namespace** (octos Recall tier) through a memory ingestion call. Promotion into shared user memory (for example, an appointment other apps should know about) happens by an explicit rule in the app's agent spec, or with the person's approval.

### 8. What an autonomous app agent may do

Least privilege, declared by the app, checked by App Hub, granted by the host, enforced by the kernel on every call:

| Layer | Restriction | Enforced by |
|---|---|---|
| **Tools** | only the manifest's `agent.tools`; nothing else is visible to the model | kernel, per peer context |
| **Network** | only the manifest's declared hosts, plus external-information providers the host grants | host network policy on every fetch |
| **Files** | only the app's folder | app jail and the kernel's per-app workspace |
| **Memory** | only `app/<app>/…`; promotion by rule or approval | kernel memory namespaces |
| **Secrets** | none; keys and passwords stay in host services | host services |
| **Risk** | each tool declares Read, Act or Destructive. Read and in-app Act run unattended; anything outward (send, post, share, buy, delete) becomes a **confirmation card** | kernel approval gate |
| **Budgets** | tokens, run time, research depth, browser pages, runs per day, Wi-Fi/charging conditions | system agent and kernel limits |
| **Output** | cards only through `glance.publish`, only L0/L1, checked and pinned | shell |
| **Control** | background mode off per app; every run logged (trigger, tools, cost, what was published) | Settings and an audit log |

### Examples

The same loop serves every app; only the data service, the agent spec and the tools differ.

| App | Data service (no model) | Agent (on event or schedule) | Card | Needs confirmation |
|---|---|---|---|---|
| **News** | feeds, RSSHub, topic feeds, with a seen-items ledger | clusters new stories, researches the top topics across languages, writes a cited digest, proposes topics | morning/evening digest | none |
| **Mail** | mail sync | spots what needs a reply or a decision, drafts replies, extracts dates and tasks | "3 need you today" with drafts | sending a reply |
| **Calendar / travel** | calendar sync; flight or transit status | sees a delay or conflict, works out consequences | "Your 9:00 is at risk: flight +40 min" with options | changing or declining an event |
| **Weather** | forecast and warnings feed | relates warnings to the person's plans and places | "Storm at 17:00 near your commute" | none |
| **Health** | the device's health store | notices a trend against the person's baseline | weekly summary; a gentle flag | sharing with anyone |

```
data service (timer or notification, no model) ──▶ app folder + ledger
      │ event: something changed
      ▼
app agent (app peer)
  ├─ decide what matters; gather more from allowed sources
  ├─ write findings → sys.digest source; record in app memory
  ├─ L0 card, more than one style → render in card-host --remote → critique → revise
  ├─ admit → glance.publish (outward actions become confirmation cards)
  └─ propose changes to what is collected (data) → next run
system agent: policy and budgets; rank the glance screen; cross-app insights
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

1. **Kernel (octos):** enforce a peer's tool list and tool risk levels; peers own schedules; a host-authorised "wake peer with event" call; memory ingestion from runs.
2. **Shell (`crates/ai-host`, `crates/app-peers`):** background peer handles for host services under host policy; `events` from data services to peers; budgets, kill switch and audit log in Settings.
3. **App Hub:** the manifest's `agent` section gains the agent spec, tool risk levels and background permission; admission checks them.
4. **Data services:** a common shape (collect, ledger, emit events) and the first services for the system apps that need them.
5. **External information (octos):** structured JSON output from research, `lang`, `since` and per-domain limits; free structured providers and SearXNG; browser rendering and main-text extraction for pages to be cited; robots.txt; review of `deep-crawl`'s automation hiding.
6. **Cards:** a `card-studio` skill (render in `card-host --remote`, measured checks, vision critique, revise within budget); `glance.publish` and the glance screen's curation.
7. **A first app end to end** through steps 1–6, then the other system apps.

## Open questions

- Where render and critique run for a phone-only user with no desktop or server.
- How the person reviews and edits an app's agent spec and what its data service collects.
- Budget defaults per app, and how cost is shown.
- Whether a card may carry a short-lived action (Act) or only open its app.
