# Plan: Mail's email action card (desktop validation)

- **Date:** 2026-10-01
- **Status:** Plan, not started. Two decisions are open (end of this page).
- **Relates to:** [ADR 0002](../../../docs/adr/0002-event-driven-app-agents.md) (app agents, triggers, cards; Mail's row), [ADR 0004](../../../docs/adr/0004-native-apps-hosting-and-peers.md) (one agent per app and account, host tools, approvals, interactive cards), [ADR 0005](../../../docs/adr/0005-app-contract.md) (the app contract), the L0 card language (Octoscript `docs/ui-profile-l0.md`) and the a2app L0 cards (`apps/appcard/a2app-l0/`).

## What we want to show

On the OctoSense **desktop**:

1. The person adds their email account in Mail (IMAP/POP3 + SMTP; Gmail with an app password, since there is no OAuth yet).
2. A new email arrives. **Mail's own app agent** reads it, decides whether it needs the person, and if so writes a short summary, a suggested action and, where a reply makes sense, a draft.
3. A notification appears on the desktop ("Mail · Ana Lee: Contract question").
4. Clicking it opens **that email's card**, App Clip style, where the person can:
   - approve the suggested action;
   - close the card;
   - edit the draft and **send a real email**;
   - chat with Mail's agent about the thread.

## Who does the work: Mail's app agent

Mail gets its own app agent, as ADR 0004 gives every app: a host-owned octos peer `card.os.mail`, one per signed-in account (#233), with:

- **its own workspace:** Mail's account folder;
- **its own memory:** octos memory namespace `app/os.mail/acct-<hash>`, private to Mail, per account;
- **its own instructions:** `AGENTS.md`;
- **its own skills:** `triage`, `email-card`.

The system agent stays separate. It may ask Mail's agent things ("anything from the school today?"), but triage and cards are Mail's.

## What matters: the agent's skills and memory, not a settings screen

There is no "what matters to me" setting. The agent knows from two places:

- **The `triage` skill, shipped in Mail's bundle.** It defines the categories and how to recognise them:
  - **school:** teachers, the school office, class or district updates, permission slips, schedule changes;
  - **service appointments:** confirmations, reminders and reschedules from clinics, garages, home services;
  - **shipping and delivery tracking:** carriers, "shipped", "out for delivery", tracking numbers;
  - **direct personal requests:** a real person asking something, with a deadline;
  - **noise:** newsletters, promotions, social notifications, automated digests.

  Rules: a real person writing directly outranks automation; a deadline within 48 hours raises priority. For each category the skill says whether it deserves a card, and which card shape.
- **The agent's memory,** built from what the person does:
  - each card's outcome (sent, marked done, closed unread);
  - what the person says in a card's chat ("don't show me these", "this sender matters");
  - a short per-sender profile (who they reply to, and how fast).

  At triage the agent reads the skill, then checks memory for anything that overrides it.

Two stages keep it cheap. Each new email gets a short triage call (`{important, category, why}`); only important mail gets a card. The model is DeepSeek V4 Flash, chosen in AI providers (see open decisions).

**Privacy:** the text of each new email goes to the model provider for triage. The agent's memory can hold "never send mail from <sender> to the model", and the host then skips those messages before any model call.

## The card: the LLM writes L0 only

The card follows the L0 design. The model writes **only an L0 card** (declarative: sources, state, events, copy, view), checked by `check_ui_l0` with up to three repair rounds, as in AppCard (`L0_REPAIR_BUDGET`). It writes no L1 (arithmetic) and never L2 (imperative). The trusted theme kit (`Octoscript-Makepad/components/l0/_kit.octoscript`) turns roles into widgets.

- **A new a2app L0 app:** `apps/appcard/a2app-l0/apps/email/{app.md, exemplar.card}`, with one exemplar per category shape (in the `email-card` skill), registered like the other L0 apps.
- **New `sys.mail_*` sources:** declared in Octoscript's `docs/ui-l0-constructors.toml` (then the catalog is regenerated):
  - `sys.mail_message(id, fields)`, read-only: from, subject, date, body;
  - `sys.mail_draft(id, fields:[body])`, writable (`set`/`clear`): the draft, which the agent fills **as host data** through a Mail tool, because a draft written as model copy is refused by the checker;
  - **Send**, as a real action that sends real email: see open decision 1.
- **A multi-line text field** for the draft body: a new argument or role in the constructors TOML, plus its kit piece and widget.
- **Before typed text may drive a send:** the 2026-09-04 L0 review found that `on_change`/`on_return` targets are built by unescaped string concatenation (typed text could redirect a tap). That fix comes first.

### Card states (visual design: the first deliverable)

```
┌──────────────────────────────────────────┐
│ ✉ Mail                          10:42  ✕ │
│ Ana Lee · Contract question              │
├──────────────────────────────────────────┤
│ ✦ Summary                                │
│ Asks whether we can sign by Friday and   │
│ wants the revised payment terms.         │
│ Suggested: Reply — confirm Friday        │
├──────────────────────────────────────────┤
│ [ Reply ✎ ]   [ Mark done ✓ ]  [ Ask ✦ ] │
└──────────────────────────────────────────┘
Reply:  To/Re header · editable draft · [ Cancel ] [ Send ➤ ]
Ask:    transcript (final answers) · [ Ask about this email… ] [➤]
```

The shape follows the category:

| Category | The card offers |
| --- | --- |
| school | summary, key dates, Reply |
| service appointment | when and where, Confirm (reply), Reschedule (draft), Mark done |
| shipping and tracking | carrier, status and ETA, Track (opens the link); no reply |
| personal or work request | summary, draft reply, Send, Ask |

## Trigger: new mail to an agent run

Nothing fires today: Mail syncs only while its app is open (`mail.sync` on open and Refresh), and no app trigger is wired.

1. **Mail host service, background sync:** polls every **2 minutes**, configurable, and keeps a ledger of handled message ids so each email is triaged once.
2. **A new-mail event** from the host service, like News's `on_fetch` hook.
3. **The shell starts a turn** on Mail's agent with `trigger: incoming`. Per ADR 0004, incoming-triggered runs never use standing approval rules.

## Mail's agent declaration (`apps/mail/bundle/`)

- **`manifest.json`:** adds the `octos.*` services, `glance`, and an agent block (today it has only `storage` and `mail`).
- **`tools.json`:** `mail.list` and `mail.message` (read). It also adds two new tools:
  - `mail.draft_reply`: stores a draft as host data; never sends;
  - `mail.publish_card`: publishes the generated L0 card for a message, with `notify: true`.

  The agent has **no send tool**: only the person sends, from the card.
- **`AGENTS.md`:** the run (triage → if important: read, draft, generate the L0 card, publish), recording each card's outcome in memory, and how to answer questions about a thread.
- **`skills/triage/SKILL.md`** and **`skills/email-card/`:** shipped in the bundle. The host copies them into the agent's workspace, because octos installs skills per profile, not per app; a per-app skill install in octos is a later follow-up. octos auto-discovers `AGENTS.md` in the agent's working directory; that this applies to host-owned app peers is to be verified.

## Notification, then the card

Today the desktop shows toasts only (no notification centre), and clicking one opens the whole glance panel, where tiles are clipped at 260 pt. To build:

1. Clicking the toast opens **that** card. `GlanceNote` already holds the key.
2. A full-size **card window** (the App Clip feel) instead of the clipped tile.
3. A small notification history on the desktop, so missed cards aren't lost.
4. **Glance cards carry out L0 source writes.** Today only the AppCard app dispatches L0 writes; glance cards render L0 for presentation only.

## Actions in the card

- **Send:** a real email through Mail's host service (`mail.send`, SMTP), with the account the person signed in to. The person's tap on Send, after reviewing the draft, is the person's own action, as in Mail itself. The agent cannot send.
- **Mark done:** marks the message read (archive later). **Close:** dismisses the card. Both outcomes go to the agent's memory.
- **Ask:** the card asks Mail's agent in the person's lane (`octos.turn.start`, answers through `octos.session.history`), scoped to the thread. The agent may re-read it with `mail.message`. Answers arrive whole (no streaming) for now.
- **Fix needed:** sheets a card raises (e.g. a sign-in) aren't shown today; a request waiting on one would hang.

## Validation

1. **Design review:** render the card states (each category) for the person.
2. **Hidden-window run with Mail's demo mailbox** (`mail_demo`), automated:
   - a new demo message → triage → an L0 card → a toast;
   - the card opens → Reply → Send to the demo outbox;
   - Ask → an answer.
3. **The person's real account,** on their visible desktop: a test email from a second address gives a card within about 2 minutes; a real reply sent from the card; a chat about the thread.
4. **Edge cases:**
   - a newsletter gets no card;
   - two emails at once;
   - a shipping card offers no reply;
   - closing a card is remembered;
   - sign-out pauses the agent;
   - a blocked sender never reaches the model.

## Build order

| # | Work | Where |
| --- | --- | --- |
| 1 | Card visual design: L0 exemplars per category, rendered for review | `apps/appcard/a2app-l0/apps/email/` |
| 2 | Fix the L0 typed-text target escaping (the review finding) | AppCard / Octoscript-Makepad kit |
| 3 | `sys.mail_*` sources; the multi-line field; the send action (decision 1) | Octoscript TOML, Octoscript-Makepad kit, OctoSense host |
| 4 | Mail host service: background sync, the new-mail event and ledger, `mail.draft_reply`, `mail.publish_card` | `apps/mail/host-service` |
| 5 | Mail's manifest, `tools.json`, `AGENTS.md`, skills; skills copied into the workspace; the shell's `incoming` trigger | `apps/mail/bundle`, `crates/shell` |
| 6 | Glance: write dispatch for L0 cards; toast opens the card; card window; history | `crates/shell` (glance, notifications) |
| 7 | Card Ask chat; outcomes to memory | template, `AGENTS.md` |
| 8 | Validation (above) | — |

## Open decisions

1. **How Send is expressed in L0.**
   - **(B, recommended) a typed action reference:** a new L0 construct naming a stored action and the expected draft revision, which the host executes. This is the design review's recommendation (`layered-architecture-assessment.md` §9.3–9.4), and it covers later send/book/pay. It's a grammar change in Octoscript plus the checker, kit and host.
   - **(A, quicker) a writable outbox source:** `sys.mail_outbox` with `append`. A workaround that reuses collection writes.
2. **The model id.** The catalog has DeepSeek **V4 Flash** (`deepseek/deepseek-v4-flash`). If "v4.1 Flash" is a distinct model id, add it to `apps/ai-providers/config/data/model_catalog.json`.
