# Mail events and the app agent

English | [简体中文](mail-agent-events.zh-CN.md)

Mail can process new Inbox messages without a person sending a chat prompt for
each message. A person first connects an account on the host-owned sign-in
sheet, allows Mail's agent, and asks the system agent to configure the automation.
The system agent calls `agents.provision` with instructions, named skill text,
an enabled flag and a polling interval. `agents.status` reports the current
configuration and processing state. Provider credentials and mail passwords
stay with the host; message text read by the agent goes to the configured model.

```mermaid
sequenceDiagram
    participant Person
    participant System as System agent
    participant Host as Mail host and dispatcher
    participant Mail as Mail app agent
    participant UI as Glance and notification
    Person->>Host: Sign in and allow Mail's agent
    Person->>System: Configure new-email automation
    System->>Host: agents.provision(instructions, skills, enabled)
    Host->>Host: Sync Inbox; establish initial baseline
    loop New mail after baseline
        Host->>Host: Atomically save messages, cursor and pending event
        Host->>Mail: Incoming event with message IDs
        Mail->>Host: mail.peek(message)
        Host-->>Mail: Bounded plain-text message
        Mail->>Mail: Decide whether attention is needed
        opt A card is useful
            Mail->>Host: mail.publish_card(source, data, event ID)
            Host->>UI: Validate L0, publish as Mail, notify
        end
        opt No card needed
            Mail->>Host: mail.skip_event(event ID, reason)
        end
        Mail-->>Host: Complete turn
        Host->>Host: Check publication/skip receipt, then acknowledge
    end
```

The worker polls only the currently active, consented account with an enabled
provision. First sync and server UID resets establish a baseline without turning
old mail into notifications. Pending events survive restart. Failed turns remain
pending and retry with backoff. Acknowledgement requires both a successful turn
and a durable host receipt for publication or an explicit `mail.skip_event`
decision (`no_action`, `duplicate` or `outside_policy`). A final answer after a
failed tool call does not satisfy this check.

Read the implementation in this order:

1. [`incoming.rs`](../apps/mail/host-service/src/incoming.rs) serializes UI and
   background sync. The cursor, messages and pending event queue share an atomic
   mailbox save. Queue backpressure prevents advancing past undelivered events.
2. [`agent_events.rs`](../crates/shell/src/agent_events.rs) persists account-bound
   provisions outside the app workspace, starts incoming turns and checks
   cancellation, consent and account changes while waiting for completion.
   Status uses a nonblocking queue snapshot (`pending: null`, `queue_busy: true`
   during a sync). Completion checks the saved decision and acknowledges under
   one nonblocking service lock; a busy queue retries without freezing the UI.
3. [`script_apps.rs`](../crates/shell/src/host_tools/script_apps.rs) loads admitted
   `AGENT.md`/skill text and binds every Mail tool call to the broker's account.
   A model cannot choose another account through tool arguments. `mail.folders`
   and `mail.list` read caches; `mail.sync` refreshes a folder in bounded batches;
   `mail.peek` reads plain text without marking mail read. Neither the password
   vault nor the mailbox directory is mounted into the peer workspace.
4. [`guidance.rs`](../crates/app-peers/src/guidance.rs) snapshots host-provisioned
   instruction and skill **text** on each turn, including existing peers. The
   guidance and untrusted request occupy separate serialized text blocks. This
   does not create a kernel system-message role or install native skills; tool
   grants and account checks remain the enforceable authority boundary.
5. [`glance.rs`](../crates/shell/src/glance.rs) validates generated L0 source and
   named dataset objects before publishing with Mail's attribution. Each dataset
   requires a nonempty fields list and values for those fields; cyclic source
   dependencies are rejected. This detects missing bindings, not factual accuracy
   or visual quality. The model uses the stable incoming event
   ID as `card_id`. A recorded publication receipt suppresses repeat publication;
   a crash between publication and receipt persistence can still repeat a
   notification. The stable card ID replaces the card rather than adding another.
   Glance cards themselves are currently in memory; these receipts do not restore
   visible cards after a process restart.
6. [`mobile_app.rs`](../crates/shell/src/mobile_app.rs) remembers the exact card
   key behind each notification. A tap opens that full card over Glance; Back or
   the sheet's close button returns to Glance. A removed card falls back safely.
   The card's separate app-opening action still opens Mail.

An actionable email also needs usable controls inside its generated card.
Mail's admitted skill now requests declarative L0 `Chip` buttons, named events
and local view state: for example, Delivery details or Appointment details,
then Back. The button must reveal facts from that email and accurately name
its effect. A suggestion in a text paragraph, the shell's open-Mail control,
or a local state called “sent” does not implement an email action. The existing
[shipping](../crates/shell/resources/glance/mail-shipping.card) and
[request](../crates/shell/resources/glance/mail-request.card) fixtures demonstrate
the interaction syntax; their demo send/tracking/done states must not be
presented as real remote operations. Sending, booking and mailbox mutations
from generated cards remain outside this implementation. The earlier device
trial below established publication and opening, not in-card button behavior.
The subsequent [paired button test](testing/mail-card-actions-2026-10-04.md)
verified Show code → Back → Details → Back for both models in the phone's full
card and Glance, after preserving earlier failed layouts.

That shared scenario uses a pickup email with a code and no tracking URL:
Show code and Details reveal the supplied code, location,
deadline and photo-ID requirement, with Back from both views. These local
controls address a supported part of the
[email action-card plan](../apps/mail/docs/2026-10-01-email-action-card-plan.md).
The plan's external Track action remains separate work: although L0 catalogues
`sys.link`, this shell does not execute its writes. A missing URL must never
be replaced with an invented link or a fake tracking-success screen.

The current shell has a rendering limitation: an accepted `Chip(width: .fill)`
can collapse inside its Fit wrappers and become invisible. The admitted skill
therefore requests natural-width Chips, stacked vertically, with short labels.
This is a renderer limitation, not an invalid L0 token. The
[button follow-up](testing/mail-card-actions-2026-10-04.md) separates failed
layouts, actual phone taps and the remaining remote-action scope.

The Mail window can be closed during processing, but the OctoSense process must
remain alive. This implementation has no Android JobScheduler/WorkManager,
foreground service or Android NotificationManager integration. Notifications
appear in OctoSense's own shade, and cards appear on its Glance screen. Do not
promise delivery after Android suspends or kills the process.

Only Mail's `mail.messages.new` trigger is implemented here. Generic cron,
arbitrary app event routing, model selection per app, native skill discovery and
the remaining ADR 0002 budget/Settings UI are separate work. The system agent's
provision supplements admitted app guidance; email content cannot provision an
agent or expand its tools. Disabling the provision or revoking agent access
stops new turns and cancels the dispatcher's active context.

Verification: on a OnePlus 6, mail delivered through Gmail started real DeepSeek and MiniMax
Mail turns without a per-email prompt. Both models skipped a routine newsletter and published
readable shipping and appointment cards (MiniMax's opened from their notifications); a MiniMax card with
missing data bindings led to the generated-data gate. The [recorded test](testing/mail-events-2026-10-04.md)
has the builds, artifacts and limitations. The trial policy handles only subjects prefixed
`[OctoSense simulation]` and skips other messages without reading their bodies.
