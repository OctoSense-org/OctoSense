# Calendar app and Glance validation — 2026-10-05–06

English | [简体中文](calendar-ui-2026-10-06.zh-CN.md)

The Calendar bundle previously displayed instructions instead of its saved events.
Mail could book an event through Calendar's tools while the person saw no marked
day in the app. Calendar now reads and edits the same host-owned event store,
shows a month grid and selected-day agenda, and publishes its existing event
card template. The card header opens the owning Calendar app at `event/<id>`.
It does not create another event or a separate Calendar data store.

This is a follow-up to [the live Mail→Calendar run](mail-calendar-2026-10-05.md).
That report retains its observed duplicate banners and lost card after restart.
The new saved-event publication registry addresses those two limitations: exact
active retries reuse the card without another notification, edits refresh it
quietly, and restart restores the original publication and expiry. Dismissal and
Undo persist; deleting the event withdraws its cards. Ad-hoc notices and agenda
cards are outside this restoration scope.

## Source and checks

Runtime source: `4499b2ff7aa4cd0da1748cb7b8534cc6acde6b2b`.
App Hub source: `7bb63ff925d19f9aecbc72b8946679320e53cfa1`.
The latter adds the explicit `calendar` UI capability and permission wording at
contract 1.4.0; that version is not published to crates.io. The consumer pins all
Hub crates and its contract override to this same Git revision.

The shared Rust run at `22c26301` passed **1,206 tests**, with two optional Mail tests ignored:
939 Shell, 55 Mail and seven Calendar unit tests, plus Appstore, Contract,
Policy, Hub and documentation tests. The first run found an outdated Calendar
tool-list assertion; the corrected test also checks that editing remains
owner-only. Narrow Mail/system grants still permit only read/add/notify.

The nine packaging, graph, pin, catalog, private-path and whitespace checks
passed, as did 114 build-tool tests. The final runtime change only clarified the
blank-time placeholder; release desktop/Android builds and native/device
rechecks used the final revision above.
The test suite does not substitute for real model evidence. The existing live
DeepSeek tool audit belongs to the earlier report; no fresh DeepSeek or MiniMax
booking is claimed here.

## Native UI acceptance

Codex authored and drove this implementation using Makepad's native remote
instrumentation on an owned hidden release Home instance, at 412 × 892 logical
pixels. Events and publication records were fictional, in isolated storage.
The driver inspected pixels and the saved store, not only tool return messages.

- The month marks October 6 and lists both events with their distinct times.
- A saved event opens details; publishing uses Calendar's existing template.
- Changing the delivery fixture's time updates the same event id and card.
  The final fixture showed **11:30 PDT** in Glance and the routed Calendar detail.
- Restart restores the card without replaying its notification. “Open Calendar”
  selects that exact event in the actual app; its editor shows the saved time.
- Empty-title Save leaves storage unchanged and shows the error above the form.
  Temporary events can be created and deleted, with a second confirmation tap.
- The editor scrolls, retains an unspecified end, and separates date, time and
  timezone fields. Date-only input normalizes to midnight in the existing
  backend; this is not a distinct all-day event mode.

Earlier iterations exposed missing optional response properties and a validation
message below the visible area; both were repaired and rechecked. An immediate
capture during the keyboard's closing animation was not a stuck-keyboard failure.
The test driver also retried a navigation click made before expansion settled.
Original captures and source/hash receipts are retained privately. All owned
native instances were closed. No generated card is attributed to an LLM here.

An additional instrument-only run on the final revision passed **five UI/state
assertions in 1.17 seconds**, with 36 HTTP calls and **zero PNG captures**. It
checked the marked date, saved detail, visible validation, saving the same event
and agenda at 12:05 PDT, and synchronization of the existing Calendar card. It
used `/snap`, `/click`, `/k` and `/t`, then `/quit`; data stayed in the fictional
desktop fixture. This is an automation-loop measurement, not phone latency.

## OnePlus 6

Normal Home was upgraded in place to **2026100528**, from the runtime revision
above. APK SHA-256:
`8e55f4405a23749050e281324bfb47edb6314a46bda31d63ad61bf7bd98b1a63`.
It is a release APK under the existing package, not a new Calendar test app.

Android/ADB platform captures verify October 6 and 12 are marked in the actual
Calendar app. Selecting October 6 shows the pre-existing delivery at **09:00
PDT**; its detail and editor show the same date, `America/Los_Angeles`, location
and notes, with no end time. The editor was inspected and cancelled without
changing the real appointment. Calendar's “Show in Glance” publishes its event
card into the shared feed alongside Mail. Opening that card and using its header
button reaches the same event in the Calendar app.

After a stopped-process restart, the Calendar card remained in Glance without
another visible notification. The card's Open Calendar route also selected the
correct event when Calendar had not yet been opened in the new process. The
driver retried an early navigation tap after the card expansion settled; no
transition-latency benchmark is claimed.

A private before/after comparison found both existing events byte-identical,
all **17 draft files** unchanged, and the Mail account file unchanged. Exactly
one saved Calendar publication points to the existing delivery id. No email was
sent, no booking was duplicated, and no real event was edited or deleted.
The device timezone was left unchanged. ADB was returned to its ordinary shell
identity, and the user's view-only mirror was restored with its existing
plugged-in stay-awake wrapper. Private mail, addresses, accounts, logs, provider
configuration and phone screenshots are excluded from the repository.

## Scope

This is OctoSense's local Calendar, without Google Calendar synchronization,
invitation email or reminder alarms. Mail calls Calendar's admitted tools as
Mail; it does not gain the UI capability or raw directory access. Editing uses
an expected saved snapshot and refuses stale writes. Calendar's update tool is
available to its own agent, not newly granted to Mail or the system agent.

Phone keyboard editing, accessibility, multi-day calendar layout, background-job
Calendar publication, and a fresh model benchmark remain unverified in this
follow-up. No numeric UX score or complete scheduling-product claim is made.

Follow-up: [in-card Open Calendar and a live medical email test](mail-medical-calendar-2026-10-06.md) supersede the header placement described in this historical build.
