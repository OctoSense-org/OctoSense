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
The final Android build and phone observations are recorded below.

## Device validation

Normal Home 2026100518 was installed on the assigned OnePlus 6 with its existing
signer and account. APK SHA-256:
`7d47741857cd227e5382145189f3e53235a9fc43f3641f289a19afff63ba843e`.
Compiled Shell source matches commit `78a3b67c`.

After Home opened, DeepSeek automatically published the previously blocked
fridge-delivery and shipping-event cards. Both `mail.publish_card` calls completed
successfully under `own_agent/events-mail`, and their native notification outbox
records were delivered. The queue drained to zero; collection and delivery
errors cleared. Six additional quiet skip receipts were present relative to the
pre-update snapshot. The person's resend was classified as `duplicate`; comparing
the two cached source bodies found only leading whitespace differences.
All eleven pre-update draft files remained byte-identical.

Codex navigated with ADB and reviewed platform screenshots. Six Mail cards were
visible across the scrollable feed (five active outbox records and an older
cached publication). Scrolling reached the lower cards and the shell footer.
DeepSeek generated the live Mail publications from existing automatic events;
Codex did not write their source or prompt their importance decisions. No SMTP
reply was sent.

For the mixed-publisher check, Codex asked the system agent to ask News to list
one cached story and publish a clearly labeled integration notice. Fresh audit
records show two successful News list/notify rounds using the same card ID, so
they produced one notice, not two. This used News's host notice template with
model-supplied content, not newly generated L0 source. The system agent's final
prose understated completion; host audit receipts and the visible notice are
the execution evidence.

The shared feed then contained seven cards: one News and six Mail. A vertical
swipe reached the seventh card and the shell footer. Opening that seventh card
showed the full Email/Chat workspace, saved reply and Review reply control.
Returning to the feed and opening the new delivery card showed its relevant
Chinese summary and saved reply with the same controls. Neither reply was
edited or sent during this check. The older saved cards were preserved; their
presence does not establish a fresh importance decision under the current policy.

This device check used light appearance and ADB/platform screenshots. It is not
a frame-timing benchmark, full editing/keyboard acceptance, or a fresh MiniMax
comparison. Temporary stay-awake was restored and ADB was left non-root.

This change is not an unlimited permanent card archive. Byte budgets, expiry,
and Mail's separately bounded durable publication/outbox storage remain. It
removes the small card-count barriers that blocked ordinary new-mail delivery.
