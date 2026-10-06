# Medical appointment live test and in-card Calendar navigation — 2026-10-06

English | [简体中文](mail-medical-calendar-2026-10-06.zh-CN.md)

The assigned OnePlus 6 received a real AgentMail email in its connected Gmail
account. The appointment was explicitly fictional: October 8, 2026,
10:30–11:00 AM, America/Los_Angeles, Demo Health Clinic. No real clinic was
contacted. Normal Home used DeepSeek V4 Flash with no fallback provider.

## Observed flow

1. AgentMail accepted one message at 07:29:46 UTC. The existing 30-second
   foreground collector received it; no per-message agent prompt triggered this
   stage and the stored importance policy was unchanged.
2. The incoming-event Mail lane read the source, proposed a saved reply and
   published a model-authored Mail card plus an Android notification. Two L0
   publications were rejected (constructor syntax, then an unread chat source).
   DeepSeek repaired both within the same turn; Codex did not edit its card.
3. The initial saved reply was noncommittal and no Calendar record was created.
   Codex opened this card’s native Chat on the phone and submitted the user’s
   authorized test request: confirm the stated time in the saved reply, add the
   appointment to Calendar and publish its card; leave the email unsent.
4. The Mail peer successfully called `calendar.events`, `mail.draft`,
   `mail.suggest_reply`, `calendar.add_event`, `calendar.events` and
   `calendar.notify`. The saved draft advanced from revision 1 to 2 with
   `applied:true`. Its body confirms October 8, 10:30–11:00 AM Pacific.
5. The owning Calendar service persisted one new event with start
   `2026-10-08T10:30`, end `2026-10-08T11:00` and timezone
   `America/Los_Angeles`. Its card routes to that same record. The draft remained
   `draft`; no SMTP approval or send was attempted.

This proves automatic email triage followed by user-directed confirmation and
cross-app Calendar booking. It does **not** claim autonomous confirmation from
email alone, a sent reply, Google Calendar sync, reminders, or MiniMax coverage.
The phone remains in its existing device timezone; the event stores Pacific time.

## Card navigation change

Calendar’s existing L0 event template now puts **Open Calendar** immediately
below the date/time tiles. The expanded workspace header no longer has this
button. `sys.link` can request only the current publication’s declared own-app
`app://<launcher>/<route>` destination. Other apps, routes and web URLs are
inert; withdrawn/replaced publications and unbound sessions cannot launch.
The shell reuses its Calendar focus route in both the panel and focused card.

Validation: 940 Shell tests and 7 Calendar tests passed. Both desktop checks,
phone check, both shell graphs, dependency pins, native catalog, private-path
and whitespace checks passed. An isolated hidden native release test passed
five navigation assertions in 2.01 seconds, 15 HTTP calls and one visual
capture: summary expansion, no premature app launch, button in the card below
the time, no header button, and navigation to the exact saved event.

The native test uses fictional local fixtures, not Gmail data. Its Splash
widget geometry required `all=1` and translation from isolate coordinates to
the sheet viewport. An initial harness query missed those widgets; it was
corrected without changing the implementation. Phone email/chat actions used
ADB because this pinned Makepad remote HTTP server is disabled on Android.
Private evidence includes source mail, tool records, before/after draft and
Calendar state, notification records and original platform captures.

The final bundle-admission check also caught the updated Mail guidance exceeding
its 6,800-byte allowance by 33 bytes. The wording was shortened; all 947 tests
passed again. The interrupted unsigned build was not installed.

Runtime source: `5d7e2f6a03b214e122b21cf5568359e47c080f20`.
APK SHA-256: `d86898286ea261bce66611fd362ca83edd27d725629d8ab5f9c9f3cf66fe4d6d`.

Normal Home **2026100601** was installed in place. All three Calendar events
remained byte-identical. Eighteen Mail/account files were unchanged; the test
draft received an intervening user-origin edit (revision 3, trailing sign-off
removed) and its prepared review was cancelled. That draft stayed unsent; no
automated retry or overwrite was performed. Calendar’s appointment card restored
with the new in-card button. The side-by-side time tile clips the final timezone
letters on this phone; the summary and Calendar detail retain the full time/zone.
This remaining visual issue prevents a complete UX acceptance claim.
