# Glance scrolling and publication capacity — 2026-10-05

English | [简体中文](glance-scroll-2026-10-05.zh-CN.md)

The [normal Home test](shared-glance-home-2026-10-05.md) received the person's
new delivery email, but publication attempts failed because Mail already owned
four cards. This was a host admission failure, not an importance decision.
The phone additionally truncated its feed to six cards, despite having a
working vertical scroller.

## Change

- Remove the per-publisher four-card rejection, the phone's six-card truncation
  and the store's fixed 32-card retention count. Phone and desktop expose all
  retained publications in priority/recency order.
- Keep storage protection separate: source/data/lowered-body payload budgets
  are 8 MiB per publisher and 32 MiB overall. A valid new publication stays;
  payload pressure retires lower-priority older cards. These are payload
  accounting budgets, not a guarantee about total process RSS. Mail drafts
  and source messages remain owned by the Mail service.
- Apply the same policy to cold restoration, preserving newer versions and
  account validation. Retiring a card also retires its native notification
  record so the outbox cannot resurrect it on the next scan.
- Paint only visible summaries and avoid cloning the whole feed on every frame.
  Opening a card still uses its retained workspace, original app/account and
  host-owned approval path. Publication rate limits and expiry still apply.

## Local validation

The full phone-feature suite passed: **935 Shell tests and 55 Mail tests**, with
the same two optional Mail tests ignored. Regressions cover publishing forty
cards from one app, restoring forty cards, retaining older cards beyond the
six-row cutoff, finger-dragging to the end of a forty-card feed, payload pressure,
new-publication retention and deterministic restoration. Fixtures are local
host tests; they are not model-authored phone publications.

Desktop checks with and without `mobile-apps`, the phone check, both dependency
graph checks, runtime pins, native-app catalog and private-path check passed.
The final Android build and phone observations are recorded below when verified.

## Device validation

Pending deployment of normal Home 2026100518 on the assigned OnePlus 6. The
existing Gmail account, saved policy and incoming-event queue will be retained.
DeepSeek must generate any live Mail card from its existing automatic event;
no hand-written replacement card or prompt to choose an importance outcome is
part of this check. No SMTP send is authorized by the test driver.

This change is not an unlimited permanent card archive. Byte budgets, expiry,
and Mail's separately bounded durable publication/outbox storage remain. It
removes the small card-count barriers that blocked ordinary new-mail delivery.
