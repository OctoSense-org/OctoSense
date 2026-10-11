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

Source `9672046b1c67709e0682e875a746c774e3ee38db` passed 937 Shell tests and
55 Mail tests; two optional Mail tests remained ignored. Desktop checks with and
without mobile apps, phone checks, both dependency graphs, runtime pins, native
catalog, private-path and whitespace checks passed, as did the release Android
build. All five compiled lifecycle source hashes match that clean revision.

Normal OnePlus 6 Home **2026100523** is installed. Its APK SHA-256 is
`a0fa78ae46b76c27c2a1425d339a1fb50f17709796ef08da6b93c90a1f07e72a`.
The private baseline contained eight cached bound reply cards, three of which
had accepted send receipts. After upgrade, exactly those three publications
and their notification records were dismissed; the other five bound publications
remained unchanged. All eight model sources, data, bindings and timestamps were
preserved; completion changed only the three dismissal flags. All fifteen saved
draft files remained byte-for-byte identical.

ADB/platform captures showed the cleaned feed and an unfinished card's Email /
Chat workspace with its saved body and Review action. After stopping and
relaunching Home, the completed cards remained absent, scrolling reached the
remaining feed entries, and the same storage comparisons passed again. These
are phone lifecycle observations, separate from the Rust/native widget tests.
Temporary device settings were restored and ADB returned to shell UID 2000.

Codex authored the Rust change, drove ADB and inspected private captures/storage.
No text was entered, no send approval was activated and no model turn was
requested by the test driver. Existing completed receipts supplied the evidence;
this run did not exercise a new physical send, the live success-review transition,
removal of an already displayed Android notification, or a fresh incoming-mail
model decision. No new model-quality or complete UX score is claimed. Private
email and screenshots remain outside the public report.
