# Completed Mail cards — 2026-10-05

English | [简体中文](mail-completion-2026-10-05.zh-CN.md)

Completed work should leave Glance. This change implements that lifecycle for
host-bound Mail reply cards: both the draft and latest attempt must be accepted,
with a positive saved transport receipt. SMTP acceptance is not recipient delivery.

The host persists dismissal in the publication cache and Android notification
outbox, then removes the matching account/draft/source publication from the feed.
It retains the draft and receipt, preserves the person's existing Undo action,
and leaves an active success review visible until Back. Restore, Undo and model
republishing cannot revive an accepted bound reply. Reading, editing, cancellation,
failed sends and uncertain outcomes do not complete a card.

Completion is checked when the saved-draft generation changes, on cold restore
and periodically to retry transient reads. New incoming messages retain their
own event identities and importance triage. Conversation-wide merging, replies
sent outside this bound workspace, and automatic completion for other app types
are outside this change.

## Validation

The new regressions cover authoritative acceptance versus failed/unknown or
missing receipts, stale accepted attempts, exact account/draft/source matching,
idempotent retirement, retained newer-message cards and unchanged Undo. Existing
cache tests cover persisted dismissal and account isolation.

Device acceptance and final build/check results are pending. The private baseline
contains three cached reply cards with accepted send receipts. Codex authored
the Rust lifecycle change; live model-authored L0 source is preserved. No model
quality comparison or fresh send is required for this host lifecycle check.
