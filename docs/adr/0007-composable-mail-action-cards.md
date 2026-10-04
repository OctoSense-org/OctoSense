# ADR 0007: Composable Mail cards with editing, chat and approved actions

English | [简体中文](0007-composable-mail-action-cards.zh-CN.md)

- **Date:** 2026-10-04
- **Status:** Implementation in progress. DeepSeek and MiniMax have exercised phone editing, contextual chat, suggestion acceptance, cancellation, injected-approval rejection and restart with their repaired model-authored cards. Remaining native/device checks and physical human-approved sending are pending. Desktop and accessibility approval remain deferred.
- **Scope:** An LLM composes one L0 Mail card from reusable sections. A person reads the email, edits a reply, chats with Mail's agent, and approves sending inside that card. Drafts and outcomes survive view changes and restarts.
- **Relates to:** [ADR 0002](0002-event-driven-app-agents.md) (incoming events), [ADR 0004](0004-native-apps-hosting-and-peers.md) (peers and approvals), [ADR 0005](0005-app-contract.md) (admission), [ADR 0006](0006-app-studio-on-the-phone.md) (generation and evaluation), and the [historical Mail action-card plan](../../apps/mail/docs/2026-10-01-email-action-card-plan.md).

## Original context: sending from the card was still a demonstration

A shipping update can already trigger Mail's agent to generate a card and notification. The next interaction should be equally direct: open the card, ask the agent to draft a reply, edit it, review the actual message, and send it without opening a separate composer or approval app. Publishing a new layout must not erase the person's edits.

Here, **host** means OctoSense's trusted Rust shell and Mail service. The L0 card describes UI; the app agent proposes content and calls its allowed tools; the host owns account access, storage and execution.

The following table records the pre-implementation baseline, not current support. For current APIs, code paths and verification gates, see [Composed Mail cards](../mail-composable-cards.md).

The source review used OctoSense `127ae4bd5a5476f0b813179868e61f80748a044e`, with Octoscript pinned to `5991dfae9344589e732b2605b530f788e8bbcd11` and Octoscript-Makepad to `2cc5ef37d7d6a3d2992673389ce74488f7bb2d87`.

| Existing part | What it provides | What is missing |
| --- | --- | --- |
| [L0 components, events and slots](https://github.com/OctoSense-org/Octoscript/blob/5991dfae9344589e732b2605b530f788e8bbcd11/docs/ui-profile-l0.md) | Typed component props, parent event routes, named views and slots within one card. | Importing or embedding independently published cards is a separate capability. |
| [Mail request fixture](../../crates/shell/resources/glance/mail-request.card) | Summary, editable draft and chat views in one L0 card. | Its `send` event only sets local state to `sent`; it does not send mail. |
| [Glance runtime](../../crates/shell/src/glance_card.rs) | Input events, component state and `sys.chat` writes. | Other writes are not performed. Republishing source creates a new session; different surfaces have separate state. |
| [Card chat adapter](../../crates/shell/src/glance_chat.rs) and [chat store](../../crates/l0-chat/src/lib.rs) | Publisher checks, user-input provenance and persisted chat transcripts. | The adapter forwards only the question, without the request's history or an email/draft binding. Account selection uses the currently selected account. |
| [Mail service](../../apps/mail/host-service/src/lib.rs), [transport](../../apps/mail/host-service/src/network.rs) and [tools](../../apps/mail/bundle/tools.json) | Account-scoped reads and host/UI SMTP sending. The transport can emit reply headers. | No durable revisioned drafts, agent draft/send tools or send ledger. The service does not pass reply headers through to the transport. |
| [Approval router](../../crates/shell/src/approvals/router.rs) and [tool relay](../../crates/shell/src/host_tools/relay.rs) | The relay retains the exact tool arguments while approval is pending. | A mutable draft ID is not an immutable message snapshot. Mail has no registered app confirmation handler, and developer mode currently precedes app confirmation. |

The unchanged Mail fixture passed the pinned L0 checker with no diagnostics or lints. That establishes a composition baseline, not durable editing, contextual chat, native UI quality or actual sending. The [paired Android card test](../testing/mail-card-actions-2026-10-04.md) verified local Show code/Details/Back interactions; it did not verify remote actions.

## Decision

### 1. Compose reusable sections into one L0 card

Use L0's existing component declarations, typed props, event parameters, named views and slots. The model generates a single checked card containing summary, reply editor and chat sections, plus an entry point for the host's action review. The existing a2app L0 card examples supply reusable composition patterns and visual defaults; the model still authors the final source and text.

No new composition grammar is required. A build or generation step may include reviewed component definitions in the card's checked source; this ADR does not introduce runtime imports. A visual `Card` container does not embed another published card, merge its permissions or create another agent. Independent cross-app card embedding is outside this decision.

L0 remains the language capability level. Compact Glance, expanded card and full app are presentation depths, not the L0/L1/L2 language levels. The first implementation supports the compact and expanded surfaces; a standalone full Mail app remains a separate surface.

Glance shows a short summary, draft/send status and an entry to expand. The expanded card contains the editor, contextual chat and host review region. They may occupy sections or switch between named views within that card. Opening, closing or switching views must preserve the draft. Read-only status is consistent across both surfaces.

On the phone, the shell arrow on a host-bound Mail tile opens that exact published card in the expanded sheet, including the draft toolbar and host review. Closing it returns to Glance. Unbound cards keep their existing full-app shortcut; the Mail launcher still opens the separate Mail app. The bound-card route has been exercised on the OnePlus 6 test build. Build 0415 also verified that tapping a real notification opens the exact bound card; this navigation input does not authorize sending.

Component state is for temporary UI choices, such as which section is open. Business data must not depend on `InstanceStore` component keys: those keys can change when a model rearranges a view, and they do not provide a global namespace for independently published cards.

### 2. Bind the card to host-owned Mail data

The host attaches a trusted binding to the publication. These are **host binding fields**, carried out of band by the publication adapter, not model-provided L0 arguments. The implementation is `mail_card::Binding`:

| Field | Meaning and owner |
| --- | --- |
| `publisher` | Authenticated app identity, initially `os.mail`; supplied by the host. |
| `account` | The Mail account authorized when the card was created. |
| `source_message` | Account-local folder/message identity, including the mailbox generation where applicable; resolved by Mail. |
| `draft_id`, `draft_revision` | A host-created draft and the revision currently displayed. |
| `chat_thread` | A host-created conversation identity for this email/card context. |

Model text and card data cannot overwrite this binding. A generated source may refer to admitted data/action handles, but may not select arbitrary accounts or forge message identities. The host verifies every handle against the publisher and bound account on each operation. Model text remains plain, provenance-labelled content; accepting an AI draft for review does not convert it into approval.

Changing the currently selected account never retargets an old card. The shell must show the bound account clearly, and invalidate pending approval on an account switch. Further operations either explicitly continue under that still-authorized bound account or fail visibly; they never silently use the new selection. Sign-out/revocation blocks new reads, model dispatch and actions for that account. Late results stay with their original binding and cannot update another account's card.

### 3. Store drafts durably, with revisions

Mail's host service owns the authoritative draft store. A draft contains its account, source message, recipient, subject, body, reply headers, monotonic revision and lifecycle status. Initially support plain-text replies to one explicitly reviewed To address, matching the current SMTP transport. Multiple recipients, Cc/Bcc, Reply all, attachments, rich text and forwarding are deferred. If the requested action requires an unsupported feature, show that limit before approval; never silently drop recipients or attachments.

Creating a reply derives its recipient and `In-Reply-To`/`References` from the cached source email. The model may suggest text; it must not invent transport identifiers. Recipient or subject edits are allowed through the same reviewed draft update path.

An AI-written body enters storage through Mail's account-scoped draft-proposal tool as untrusted text, not through an L0 event writing `model-copy` into a host store. The card reads that draft through the admitted binding and retains its AI-written label. Keep L0's existing default-deny restrictions on model text in action targets, payloads and source arguments. Approval authorizes the host's exact stored snapshot; it does not reclassify model text as a trusted command or require the person to retype an unchanged draft.

Draft updates use an expected revision: an update succeeds only if the stored revision still matches. Each successful update increments it. A stale editor gets a conflict and keeps its unsaved text for recovery. Background agent rewrites remain proposals for explicit acceptance.

**Native chat exception (2026-10-04):** when a person requests a change in the native Mail chat, the host may issue a random, single-use edit capability for the exact account, draft and saved revision. `mail.suggest_reply` accepts this optional `edit_token` to save the requested body directly, recording `model_chat` provenance and returning `applied: true` with the new revision. The capability expires after five minutes and is revoked when the turn completes. It never authorizes sending, changing the recipient, or overwriting a newer revision. Generated NAV, reading a card and background turns do not issue it. Invalid capabilities fail without falling back to a proposal. This replaces the extra suggestion-acceptance step for user-requested chat changes.

The editor distinguishes unsaved, saving, saved and conflict/error states. Send is disabled until the displayed fields are durably saved and match the reviewed revision. Navigation and republishing must not discard dirty input; preserve it until saving succeeds or the person explicitly discards it. After a restart, restore the last acknowledged durable revision and report any interrupted send. Never label unacknowledged input as saved.

Glance and the expanded view subscribe to the same draft and send records. A source refresh may replace presentation, but retains the host binding and draft identity. Hiding or dismissing a card does not delete its draft. Account deletion uses Mail's data-removal policy; a card is not an extra copy of credentials or account storage.

### 4. Use a typed action reference and host-owned approval inside the card

Choose a **typed, host-issued action reference** for a proposed send. It names an immutable snapshot of the draft revision and its operation identity. Do not use an unguarded append to a generic outbox as authorization. This resolves the historical plan's Send design choice if this ADR is accepted.

The implemented source is `sys.mail_review(app, id, fields)`: `set` requests review of the bound draft and `clear` cancels pending review. `sys.mail_draft(app, id, fields)` reads draft state and permits single-field edits; the agent tools are `mail.propose_reply`, `mail.draft`, `mail.suggest_reply` and `mail.propose_send`. A card can request review of an admitted action reference; only the host can authorize execution. Add the necessary declared capability, checker validation and runtime handler together. Existing L0 composition syntax stays unchanged, and documentation must not advertise the new action until the handler exists. Native kit packs are currently rejected on Glance, so installing a kit alone cannot supply this feature.

The model can generate a Review reply entry, but the shell renders a distinct host-owned review region **inside the same expanded card surface**. It shows the sending account, complete recipients, subject and body being approved, followed by **Approve & Send** and Cancel. Long content is scrollable and available for inspection. The model cannot replace its contents, conceal the sender or relabel its final control. A summary of the message is insufficient for approval.

Only a trusted activation of that host control creates authorization. Generated chips, local `approved` state, chat text such as “yes”, tool calls, source replacement and simulated agent input do not. Accessibility activation must retain the same trusted route. The implementation must distinguish real user input from agent/instrument injection; ordinary script or navigation events are not proof of a person.

Authorization binds the publisher, account, draft ID, revision, complete outbound snapshot and operation ID. Editing any outbound field, changing accounts, cancelling or expiring the review invalidates it. The executor rechecks the binding and revision atomically when claiming the send. A stale approval fails rather than sending newer text.

This send path requires explicit human approval even in developer mode and under standing approval rules. If accepted, that is a narrow exception to ADR 0004's general developer-mode behavior. Merely setting `confirm: app` is insufficient with today's router ordering.

All UI, agent and service entry points for Mail sending must converge on this executor. The existing `host.request("mail.send", ...)` path must migrate to the same authorization contract; leaving it as an alternate unguarded route would violate the decision. The agent may create drafts and propose sends, but does not receive a tool that can mint approval or call the SMTP transport directly.

### 5. Record sending durably and represent uncertainty honestly

Approval authorizes one operation on one revision. Each submission attempt has a host-issued operation ID. The host durably records that ID, a stable Message-ID for the attempt and the immutable payload before invoking SMTP. An atomic claim prevents a second tap, another surface or a repeated tool request from starting the same attempt twice. Ordinary repeated propose/send requests for the same draft revision return the existing attempt and its status; issuing another action reference does not bypass deduplication.

| State | Meaning shown by the host |
| --- | --- |
| `draft` | Editable draft; no send authorized. |
| `awaiting_approval` | An immutable revision is available for review; nothing has been sent. |
| `sending` | Authorization was consumed and the operation was claimed. The payload is frozen. |
| `accepted` | SMTP accepted the message; this does not prove recipient delivery or reading. |
| `failed_before_delivery` | The host knows the submission did not reach acceptance; explicit retry creates a new attempt requiring another approval. |
| `outcome_unknown` | Acceptance may have happened. Do not automatically resend. |

An explicit Retry request may create a new attempt linked to the prior attempt and the same immutable draft revision, only when no attempt is active. It requires fresh approval. For `outcome_unknown`, it additionally requires the person to acknowledge the possible duplicate. Normal model or UI retries cannot create that new attempt. An `accepted` attempt is never retried; another reply is a new draft.

Once claimed, the payload cannot change. After an ambiguous transport error or a crash with an unresolved `sending` record, retain the original identity and reconcile with the provider/Sent folder where possible. Otherwise keep showing uncertainty; neither a timeout nor a missing Sent result proves failure. A stable Message-ID helps reconciliation; it does not make SMTP exactly once.

Cancelling before the claim prevents sending. Closing the card, cancelling an agent turn or signing out after SMTP submission begins cannot guarantee recall. The receipt must represent the transport's actual outcome, not the UI's local state. Keep the ledger authoritative even if publishing the updated card or saving a chat transcript fails.

### 6. Ground chat in the email and current draft

The chat request includes the trusted account/message binding, bounded thread history and a snapshot of the current durable draft revision. The native editor flushes staged changes before entering Chat; failed saves keep the editor open for resolution. The agent receives acknowledged text and, where applicable, the scoped edit capability above. A delayed answer may remain useful as chat, but its stale draft patch cannot apply automatically.

The phone opens a focused workspace with **Chat / Reply** tabs. [The native reply clip](../../crates/shell/src/mail_clip.rs) reads the authoritative draft, supports direct recipient/subject/body editing and original-email review, and opens the existing physical-approval surface. [The native chat](../../crates/shell/src/card_chat.rs) virtualizes the transcript and keeps its composer above the keyboard. A saved `model_chat` revision drives the host's “Reply updated · View reply” control; assistant prose cannot create that confirmation. Manual edits coalesce in memory for 500 ms, flush on navigation/review, and retain conflicting text. The model-authored L0 publication remains the Glance entry and does not need to be regenerated for an edit. The UX target is **9/10**, subject to functional, keyboard, performance and device evidence; a prototype or passing build alone does not establish it.

Use the existing broker: one app peer per `(app, account)`, with conversation contexts within that peer. Components, draft editors and chat sections do not start kernels or create new app agents. The adapter uses the label `card-chat`, while the broker gives each conversation a fresh context ID. The implementation adds explicit email/draft binding and bounded thread history; the original gap was this missing context, not a collision between those IDs. Context is reconstructed from the bound thread's bounded history rather than keeping an unbounded live turn.

Both the system agent's requests and the person's card chat reach Mail's own peer under their existing caller and lane policies. A system agent can ask Mail to prepare a reply and report the result; it cannot approve that reply for the person. Mail tools remain account-scoped, and their declared availability does not widen another app's grant.

Chat may explain a send receipt or propose a better draft. It is never the authoritative draft store, delivery ledger or approval channel.

## Event and interaction flow

```mermaid
sequenceDiagram
    participant I as Incoming Mail event
    participant A as Mail app agent
    participant H as Mail host service
    participant C as Composed L0 card
    participant P as Person
    participant R as Host review inside card
    participant S as SMTP provider
    I->>A: Bound event + host-provisioned instructions
    A->>H: Read message; propose reply draft
    H-->>A: Draft ID and revision
    A->>C: Publish checked L0 source + bound data; notify
    P->>C: Open, edit or chat
    C->>H: Save edit with expected revision
    H-->>C: Durable revision or conflict
    C->>A: Chat with email + draft + thread context
    A-->>C: Answer / proposed revision-bound edit
    P->>C: Review reply
    C->>R: Request review of host-issued action
    R->>H: Read immutable snapshot
    R-->>P: Exact account, recipients, subject and body
    P->>R: Approve & Send
    R->>H: Host authorization for this operation/revision
    H->>H: Validate, persist and atomically claim
    H->>S: Submit frozen message
    S-->>H: Accepted / failure / ambiguous outcome
    H-->>C: Persisted receipt for expanded card and Glance
```

The incoming event runs through Mail's opt-in dispatcher and host-provisioned instructions/skills. It does not need a per-email instruction from the system agent or a coding assistant. The model decides whether to publish or stay quiet; publishing never authorizes an external action.

## Ownership and implementation order

| Repository / area | Responsibility |
| --- | --- |
| OctoSense: `apps/mail/host-service` | Draft storage, reply headers, revision checks, action snapshots, authorization validation and send ledger/transport. |
| OctoSense: shared shell, `crates/l0-chat`, AI host/broker adapters | Publication binding, shared draft subscriptions, editor adapter, contextual chat, host review UI and trusted approval routing. No desktop/phone source copies. |
| Octoscript | Define and validate any new Mail/action source and event contracts in the L0 catalog/checker. Preserve model-text provenance and existing composition grammar. |
| Octoscript-Makepad | Shared input/render hooks needed by the checked components and host review boundary; it does not own Mail authorization. |
| OctoScript-App-Design-Flow / a2app examples | Reusable sections, generation guidance and model evaluation cases using implemented contracts. |
| OctoSense-App-Hub | Admission/schema changes only where the new declared capability requires them; reject unsupported capability versions. |
| octos | Keep the existing shared-kernel/peer model. This decision needs no new kernel per card or new agent topology. |

Implement in this order:

1. Agree the concrete draft/action schemas and authorization interface; implement the durable Mail store, reply headers and executor with a fake transport. Define the trusted input boundary before exposing Send.
2. Add checked L0 bindings and a composed card using the existing component grammar. Wire durable editing, conflicts and Glance/expanded synchronization.
3. Bind chat to the account, email, draft revision and thread. Route agent suggestions through the same revision API.
4. Add the host review region and migrate every send entry point. Admit draft/propose-action tools only with their handlers and caller grants in place.
5. Update a2app/generation guidance, then run paired model and native UI acceptance. Keep actual sending unavailable until all authorization and outcome gates pass.

## Acceptance criteria

**Host/service tests:**

- Correct reply recipient and headers; rejection of unsupported recipient/content combinations.
- Stale revisions; user edits while approval or an agent rewrite is pending; account switching and sign-out; attempts to retarget handles.
- Direct-service and developer-mode bypass attempts; cancellation before claim. Model copy cannot become approval, even when the person sends an unchanged AI-written draft.
- Two surfaces approving at once; repeated calls; explicit retry versus duplicate submission; storage failure before SMTP; restart during sending.
- SMTP acceptance versus uncertain delivery. No ambiguous result causes an automatic resend, and a new attempt requires the specified fresh approval.

**Native UI tests:** use Makepad's hidden-window instrument for the actual composed card and review surface. Exercise multiline and Unicode editing, quotes, cursor/selection, IME composition, keyboard/Back behavior, scrolling, focus and visible touch targets in light/dark themes. Republish and change views while editing, reopen the card and restart the shell. Verify stored values and action results as well as screenshots. Injection may test controls, but production must not treat agent/instrument input as human send authorization; transport fakes or an explicit test-only path cover that boundary.

**Live OnePlus 6 acceptance:** use a separate test package. For each configured DeepSeek and MiniMax model, send the same controlled shipping/request scenario through AgentMail to the connected test mailbox. The new email must automatically trigger Mail's agent, which authors the final L0 source/data and publishes the notification. The person opens it, edits the reply, chats about that email, reviews and approves inside the card, and verifies the actual threaded reply plus matching Glance/expanded receipt. Also cancel a review and verify that no message is submitted. The evaluator uses instrument/device evidence to give the model feedback; it does not substitute a hand-authored final card or prompt each email turn manually.

Report exact model IDs, provider configuration labels without secrets, repository/runtime pins, generated source hashes and test outcomes. Separate local controls, simulated transport outcomes and real delivery evidence. Record visual/interaction shortcomings even if the checker passes. The target for writing and UX review is at least 4.5/5 (A−), alongside the mandatory functional gates; a score cannot replace a failed gate.

**Historical acceptance status (0414–0416):** on test build 0414 (`57b711ae`), both models' repaired cards exercised editing, live email/draft-bound chat, explicit suggestion acceptance, cancellation, injected-approval rejection and restart. Separate native-shell checks also passed within their stated scope. Final agent reviews, including the 0416 phone answers, scored DeepSeek 4.1/5 visual and 4.4/5 scoped usability; MiniMax scored 4.2/5 and 4.4/5. Both remain below the target.

Build 0415 (`97a23a3f`) passed 888 shell tests, desktop/phone and dependency-graph checks. Its notification touch fix was exercised on the OnePlus 6: a real banner opened the exact bound card with the saved draft intact. Full phone IME, selection, theme, touch-target and failure-matrix coverage remains incomplete. The earlier Mail suite passed 50 tests with two existing live/environment tests ignored; pinned L0/portable UI suites passed 315/86.

Build 0416 (`abf06d8f`) adds presentation guidance to the bound card-chat request; 11 chat tests passed. Both actual models answered the same fresh read-only question concisely in plain text with the correct saved note, no Markdown or raw IDs, and no change to revision 44 or its body. Repeated injected approval left the operation awaiting approval; Cancel returned it to draft.

Real sending was unverified at that 0416 checkpoint. A later controlled shipping demo completed physical approval and a verified threaded reply. The new Chat / Reply workspace subsequently passed both actual models’ appointment-time edits, exact review and manual-edit readback; see the [current workspace validation](../mail-composable-cards.md#reply-workspace-design-and-current-validation) and its scoped 9.0/10 engineering review. The workspace tests did not repeat real SMTP. Only Android physical touchscreen input can authorize sending; desktop/accessibility approval remains deferred. See the [current checkpoint](../mail-composable-cards.md#historical-verification-checkpoint-04140416) for source hashes, failed attempts and operator-harness limits. Guidance and host fixes changed between providers, so these are engineering checks, not a controlled model benchmark.

## Consequences and alternatives

This keeps model-authored composition small and reusable while giving Mail a durable, auditable action path. It adds storage migrations, conflict handling and a trusted host UI boundary; those costs are necessary to make the visible draft match what is sent.

Concatenating complete published cards would lose clear ownership of state and permissions. Full nested-card hosting needs a separate identity and lifecycle contract and is deferred. A raw writable outbox or a generated Send chip alone cannot prove human approval. Redirecting every action to the full Mail app would avoid some in-card integration, but would not satisfy the requested editing/chat/approval experience.

If accepted, this ADR replaces the historical Mail plan's open Send design choice and transient-draft approach. It does not mark all of ADR 0002 implemented, grant general cross-app card composition, or claim that unrelated Calendar/payment actions are now available.
