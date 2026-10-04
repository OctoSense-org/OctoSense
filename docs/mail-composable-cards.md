# Composed Mail cards: draft, chat and host approval

English | [简体中文](mail-composable-cards.zh-CN.md)

The ADR0007 draft, chat and review paths are implemented in source. **The complete
paired-model OnePlus 6 flow and a person's final send approval remain unverified.**
Android physical touchscreen input is the only positive approval route in this
iteration; desktop and accessibility approval are deferred requirements.

A generated card describes presentation. Rust host code owns the account,
original email, saved draft, revision and send operation. Editing a field changes
a durable draft; a generated Review reply chip only opens host review. Neither
the model nor a card-local `sent` state can authorize SMTP.

## Follow one reply through the code

1. The incoming dispatcher starts Mail's app agent under the signed-in account.
   The agent reads the message, then calls `mail.propose_reply` with `message`,
   optional `folder` and suggested `body`. [The Mail service](../apps/mail/host-service/src/drafts.rs)
   derives recipient/reply headers and returns `draft_id`, `revision` and
   `chat_thread`. `mail.draft` reads the saved revision;
   `mail.suggest_reply` creates a revision-bound suggestion without overwriting
   edits; `mail.propose_send` prepares an immutable proposal, not a send.
2. `mail.publish_card` accepts that `draft_id` alongside model-authored L0
   source/data. [The publication route](../crates/shell/src/glance.rs) attaches
   trusted Mail metadata out of band and refuses retargeting an existing bound
   card to another account, email or draft. Ordinary `glance.publish` cannot
   supply this binding.
3. [The card adapter](../crates/shell/src/mail_card.rs) answers
   `sys.mail_draft(app, id, fields)` from host storage. A written source declares
   exactly one of `to`, `subject`, `body`; a direct `Field(text: draft.body,
   on_change: save)` with `draft: set($value)` carries Field UserInput. The
   adapter checks the displayed revision before saving. Conflicting or failed
   edits remain unsaved and visible; they are not silently accepted.
4. `sys.chat(app, thread, fields)` uses the returned host thread.
   [The chat adapter](../crates/shell/src/glance_chat.rs) supplies the bound email,
   durable draft revision and bounded thread history to Mail's existing peer.
   Unsaved editor text is excluded. A stale agent suggestion cannot overwrite
   later edits. This is a conversation context in the same account's peer,
   not another kernel or app agent.
5. `sys.mail_review(app, id, fields)` accepts `set` to request review and `clear`
   to cancel it. [The review UI](../crates/shell/src/mail_review.rs), mounted by
   [the expanded sheet](../crates/shell/src/glance_sheet.rs), shows the exact
   account, recipient, subject and body. Only its trusted Approve & Send gesture
   passes an opaque review capability to the service executor. The agent has
   neither `mail.send` nor `mail.approve`. The legacy full-app send path also
   requests host review; developer mode does not bypass this check.
6. The service claims the immutable operation and records its identity before
   SMTP. Repeated requests share its result. `accepted` means SMTP acceptance,
   not recipient delivery. An uncertain outcome never causes automatic resend;
   explicit retry requires a new approved attempt and duplicate-risk
   acknowledgement where applicable. Card/chat publication failures cannot
   turn a local state change into a delivery receipt.

Read [ADR0007](adr/0007-composable-mail-action-cards.md) for the full decision and
mandatory acceptance gates. Initial replies support one reviewed To address;
Cc/Bcc, Reply all, attachments, rich text and forwarding remain unsupported.

Drafts and attempt receipts persist; review capabilities deliberately do not.
Reopening requires a fresh review and approval. The full composer retains its
compose ID during its live session, but currently has no saved-draft list for
reopening an arbitrary draft after restart. The host-private publication cache
restores model-authored Glance source/data only for the active, signed-in account
and an authoritative saved draft. It preserves original expiry (which a digest
expiry may shorten) and durable dismissal/undo state, without sending a new
notification. Five cache tests and 27 Glance tests passed; actual process-restart
and phone restoration remain unverified.

## Runtime contracts and input provenance

The [tool schemas](../apps/mail/bundle/tools.json) describe the four draft/proposal
tools. Catalog contracts are pinned to Octoscript
`9ca9545b6cba489ab72ec988dfd649fb7c13ce17`; Octoscript-Makepad is pinned to
`a950f7fb7c5560eb0583d643ea1cf11d4558e6d8`. Both Mail sources require literal app
identity; this host additionally requires their literal draft ID to match the
trusted publication. Draft `to`, `subject`, `body` and `suggestion_body` remain
model-tainted for checker purposes: display/edit is allowed, direct reuse in
an action payload or source selector is not. Host approval authorizes the exact
stored message, without requiring a person to retype an unchanged AI draft.

The [Makepad overlay](../tools/runtime-patches/makepad-trusted-user-input.patch)
keeps `trusted_user_input()` false by default. Android JNI checks a positive,
nonvirtual device, touchscreen source, unobscured flags and noncancelled input;
null objects or Java exceptions enqueue no trusted input. A scoped guard applies
only during the native TouchUpdate handler. Remote/nested synthetic dispatch,
deferred actions and script tasks do not inherit it. Host review requires a
matching trusted press and release. `with_untrusted_input` can only remove trust.
Keyboard/IME, long press, desktop and accessibility input currently cannot
approve a send. A compromised OS/root process impersonating hardware lies
outside this application-level boundary; this is not hardware attestation.

## Verification gates

| Gate | Current evidence/status |
| --- | --- |
| L0 catalog and provenance rules | 315 Octoscript tests passed, including five Mail-source tests. |
| Portable UI translation | 86 Octoscript-Makepad portable tests passed, including its doc test. |
| Input metadata and scoped guard | Three focused tests passed; these do not emulate Android JNI or prove device compatibility. |
| Integrated service, shell and Android build | Validation in progress; record results for the final source revision. |
| Two model-authored cards: edit, chat, conflict, cancel, reopen/restart | Unverified for this integrated feature. Earlier pickup-code cards tested local views only. |
| Physical approval and actual threaded reply, matching Glance/sheet receipt | Unverified; automated input must not substitute for a person's approval. |
| Desktop/accessibility approval | Deferred; denied by the current provenance boundary. |

No new launch or sending recipe is claimed verified here. Keep transport-fake,
native-instrument and real-device results separate. Passing syntax or scoring
4.5/5 cannot replace a failed authorization, persistence or delivery gate.
