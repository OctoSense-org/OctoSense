# octosense-l0-chat: the host side of an L0 card's in-card chat

English | [简体中文](README.zh-CN.md)

> **Where this fits.** An app's agent can put an L0 card on the glance screen, and the person can chat with that agent inside the card. The card only declares the chat; this crate is everything the host decides about it: who may chat, who wrote what, the limits, and where the transcript is kept. The shell's glance cards and card window (`crates/shell/src/glance_chat.rs`) and AppCard's L0 cards both use it. The agent behind the chat is the app's own (one host-owned octos peer per app and account, [`crates/app-peers`](../app-peers/README.md)); the overview is in the root README, [The system agent and the app agents](../../README.md#the-system-agent-and-the-app-agents).

A card declares a conversation with its app's agent (Octoscript profile §5.15,
`sys.chat` and `ChatEntry`, OctoScript#53):

```text
source convo sys.chat(app: "os.mail", thread: "ana-contract", fields: [entries, id, role, text])
event  send  { convo: append($value), draft: clear }
for m in convo.entries key m.id { ChatEntry(text: m.text, role: m.role) }
```

Octoscript's L0 checker admits the card. The rest is the host's, and is here
(`src/lib.rs`):

| Rule | How |
| --- | --- |
| **Ownership** | A card chats only with the app that published it. `check_publisher` refuses, at publish, a `sys.chat` whose `app` is not the publisher; `seed` and `perform` check again on every read and write, and a card naming another app reads an `unavailable` transcript and writes nothing. |
| **The transcript is the host's** | `seed` puts the host's own transcript under every `sys.chat` source's name, replacing whatever the publisher put in its `data`: no forged `model` entry, no other app's thread. |
| **Roles** | Entries are `{id, role, text, at}`, `role` one of `user`, `model`, `host`. A card's one write is `append`, recorded as a `user` entry only when its payload is what the person typed (`ValueOrigin::UserInput`). Only the host's `ChatStore::append_reply` writes `model` (the agent's answer, drawn AI-written) or `host` (a notice). A payload that looks like `{"role":"model",…}` is just the text of a `user` entry. |
| **Limits** | `TEXT_MAX_BYTES` 4 KiB per message after trimming, control characters dropped; one message per thread every `MIN_INTERVAL_MS` (2 s) and none while the agent is still answering; the last `RETENTION` (200) entries kept; a reply cut at `REPLY_MAX_BYTES` (16 KiB); app and thread ids at most `ID_MAX` (64), threads `[A-Za-z0-9_-]`. |
| **Storage** | `ChatStore::with_folder` keeps one JSON file per (app, thread) in the folder the host names; `ChatStore::in_memory` for tests. The shell names the app's account folder (ADR 0004 §11): `apps/<app>/accounts/<account>/chat/<thread>.json`, owner-only, for the account the app's agent acts for (`device` without one), so switching accounts switches transcripts; a reply is kept in the thread its message went to, even if the account switched while the agent answered. AppCard keeps its chats in `apps/appcard/accounts/device/chat/`. |
| **Stale** | Every change bumps `ChatStore::generation` and calls the host's change hook (`set_on_change`), so a surface re-seeds and re-lowers the card when the reply arrives. |

## Who answers: `Responder`

The host passes each accepted message to a `Responder` and appends what it
answers:

| Responder | Where | Answer |
| --- | --- | --- |
| `AgentResponder` | the shell (`crates/shell/src/glance_chat.rs`) | a turn in the app's conversation, the person's lane on the app's peer (`crate::agents::conversation`, `TurnTrigger::AppSaysPerson`), once the person allowed the app's agent; otherwise a `host` notice that says why |
| `Canned` | the shell under `OCTOSENSE_GLANCE_DEMO=mail` (Mail's demo cards, fake data) | a fixed `model` reply; no model is called |
| `NoAgent` | AppCard | a `host` notice: its agent does not answer inside a card yet |

Unbound cards retain their existing message-only adapter. For a host-bound Mail
card, `ContextBinding` supplies the original account, email context, durable draft
snapshot (including its revision), and chat thread. `seed_bound` and
`perform_bound` use the explicitly resolved account folder and reject a different
thread. The shell rejects a switched account or revoked assistant rather than
retargeting the card. Late replies stay in the original transcript.

`Request::agent_text` serializes that binding (at most 32 KiB), the last 20
history entries within 16 KiB, and the question. Email, draft and transcript text
are labelled untrusted data; they confer no tool grants or send approval. The
shell opens an account-bound conversation on the existing app peer and preserves
`TurnTrigger::AppSaysPerson`. This API does not by itself publish a composed card
or authorize sending.

## Model-written text in a card

The same Octoscript change lets an L0 card show text the model wrote, in
`copy` blocks of `class: model-copy`: it is drawn marked as AI-written, as
plain text, and never triggers, retargets or changes an action. Actions stay
the card's declared ones, which the host checks. That rule is the runtime's
(Octoscript's L0 profile); this crate adds only the chat.

## Testing

From the repository root:

```sh
cargo test --locked -p octosense-l0-chat
cargo clippy --locked -p octosense-l0-chat --all-targets --no-deps -- -D warnings
```

CI: the `services` job of `.github/workflows/apps.yml` runs both.
