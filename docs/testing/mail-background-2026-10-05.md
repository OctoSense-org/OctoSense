# Mail collection and Android background validation — 2026-10-05

English | [简体中文](mail-background-2026-10-05.zh-CN.md)

Scope: independent collection/delivery, selective model decisions, and [ADR 0008](../adr/0008-quiet-android-mail-jobs.md). Tests used an assigned OnePlus 6, Android API 35, the separate `dev.makepad.octosense.studio` (OctoSenseMailTest) package and its existing DeepSeek `deepseek-v4-flash` profile. This initial test phase did not replace the installed Home or ROM. The later, user-requested normal Home deployment is recorded in the [shared Glance follow-up](shared-glance-home-2026-10-05.md). Gmail and provider credential-file hashes remained unchanged across upgrades. No reply was sent by the test driver.

## Local checks

| Check | Result |
| --- | --- |
| From `phone/`: `cargo test --locked --features mobile-apps -p octosense-shell -p octosense-mail-service` | 932 shell + 55 Mail tests passed; 2 optional Mail tests ignored |
| From `phone/`: `cargo check --locked -p octosense-home --features mobile-apps` | Passed |
| Root: `cargo check --locked -p octosense`, with and without `--features mobile-apps` | Passed |
| Root: `bash tools/check-shell-graph.sh -p octosense`; phone equivalent with `-p octosense-home` | Passed; one pinned octos revision |
| `python3 tools/setup.py --check --cargo`; `python3 tools/native_apps.py --check` | Passed |
| `python3 -m unittest discover -s rom/tests -p test_no_local_paths.py` | Passed |
| Separate debuggable Android APK, preserved account data and bundled kernel hash | Built through 2026100515; device acceptance through 2026100513 |

Tests ran with isolated `OCTOSENSE_HOME` and `RINX_DATA_DIR` directories for the final checks. The two ignored tests require a real Gmail credential failure or macOS keychain. Fake context/transport tests establish scheduling, receipt, cancellation and persistence boundaries; they do not establish real model behavior. Real DeepSeek/device evidence is listed separately below.

## Device observations

| Scenario | Observed result |
| --- | --- |
| New Inbox collection with existing pending events | Inbox grew while the queue was nonempty; the new collector no longer waited for the failed head event |
| Importance policy | System agent saved the user's selective healthcare/shipping/schedule/school/work/family policy. Repeated ordinary incoming events produced successful `mail.peek` → `mail.skip_event` receipts without native notifications |
| Network diagnosis | App-UID HTTPS initially failed while shell-UID HTTPS worked. Android reported effective `APP_BACKGROUND` blocking. Restricted networking was also enabled, but this app was exempt from that particular rule; it was not the effective blocker |
| User-authorized setting change | Global restricted networking was turned off as requested. The temporary USB/AC stay-awake change was restored to its prior value |
| Job registration | Android reported a persisted 15-minute periodic job, 5-minute flex, and a network constraint |
| Background execution | Forced JobScheduler runs advanced collection and DeepSeek decisions with the Activity closed/screen asleep. Network policy showed effective blocking `NONE` during the job |
| Cold process | Updating/killing the test process followed by an OS job started the host without opening the Activity; collection and decisions continued under the original profile |
| Job deadline and interruption | The four-minute lease ended; the active event stayed pending and status recorded cancellation. Outside the job, Android restored its ordinary background network restriction. An explicit JobScheduler timeout also stopped the job |
| Native notification | Real incoming events reached model-authored `mail.publish_card` through caller `own_agent/events-mail:…`, then the private outbox and an Android notification. The model repaired rejected card source through its own tool calls |
| Notification after process death | After `am kill` showed no test-app PID, tapping the real Android notification started a new PID and opened the correct full-screen card, including its saved draft, Email/Chat tabs and Review reply |
| Edge Back | The edge gesture returned from the card to Home while retaining the test-app process; the manifest extension was loaded |
| Restart duplication | A previously tapped/delivered notification was not posted again after the APK upgrade; its private outbox record remained delivered |

Native publication was observed on versions 2026100509 and 2026100512. The first full notification → process restart → card/Back sequence used 2026100509. The final version additionally tightens notification-permission handling, silent republish persistence and publication version precedence.

## Compose reply regression

The person's preference was saved by the system agent and read back from the
host provision: replyable important mail may receive automatic drafts;
automated/no-reply mail waits for an explicit Compose reply request. The existing
importance gate and physical SMTP approval requirement remain in that provision.

On build 2026100513, a real informational incoming card initially had no draft
binding. Its original model source remained unchanged when the host added
Compose reply. Codex injected the navigation/action through ADB; DeepSeek
`deepseek-v4-flash` read the host-resolved email, proposed a draft and repaired two
rejected publications before successfully republishing the same card with its
saved draft and `notify:false`. The open workspace changed from Card/Chat to
Email/Chat and displayed the native editor and Review reply. No SMTP send occurred.

A subsequent native Chat request asked for a proposed 6:15 PM arrival, explicitly
without confirming plans or sending. DeepSeek used the turn's edit lease: the
saved draft advanced from revision 1 to 2 with `body_origin:model_chat`, and the
requested time appeared in the Email editor. The sender address was unchanged.
The recipient warning was visible but clipped at its end. Asking the agent to
shorten it exposed missing publication context: the agent searched its workspace
and exhausted its tool budget. Build 2026100515 supplies the current card id and
bounded source/data in Mail chat; its 32 KiB context-budget regression test passes.
That final model repair is **not yet device-verified**.

The direct-edit/review round trip remains **unverified**: Android's `input text`
rejected a Unicode dash, and additional draft edits/review requests occurred
during the run. Phone input was paused to avoid overwriting the person's work.
The observed review attempts were cancelled, with no approved/send attempt.
This report does not award a UX score or treat publication as full UX acceptance.
Captures are ADB/platform captures; Rust widget tests are separate from Makepad
instrument geometry or presented-frame measurements.

One existing concurrency test timed out while the APK and tests compiled in
parallel. The complete unchanged suite passed after compilation finished:
932 shell + 55 Mail tests, with the same two optional tests ignored. Build
2026100514 only shortened the Compose hint; 2026100515 also includes publication
context. The final APK is built; the last installed build in this record is
2026100513. An explicitly forced job schedule was reset after test forcing had
advanced its next window; Android again reported the normal 15-minute period.

## Remaining checks and limits

- The initial everyday Home → swipe right → Mail card check **failed**. A follow-up device check found that `dev.makepad.octosense` build 2026100216 still held the Home role, while the configured Gmail account and three undismissed, unexpired Mail publications were in `dev.makepad.octosense.studio` build 2026100513. These packages do not share the Glance store. The earlier notification/card/Back test does not establish integration with the default Home. The [follow-up](shared-glance-home-2026-10-05.md) records the corrected normal Home deployment and mixed Mail/News feed, with Gmail reauthentication still pending.
- The shipping simulation reached the Inbox and is pending behind the existing queue. Its final card outcome is still under observation.
- AgentMail accepted a separately labeled no-action control. Its arrival in the monitored Inbox has not yet been established; it must not be counted as an agent skip.
- The exercised jobs were explicitly forced through Android's test command. A natural periodic/Doze/reboot cycle is **unverified** at this point; registration does not prove its delivery latency.
- Notification permission/channel denial, account removal during a job, and other Android versions are not all device-tested in this change. Existing consent/account tests and the new outbox scope/expiry tests cover host-side boundaries.
- A large backlog, a slow model or Mail's four-live-card limit can delay later decisions. The queue remains bounded at 128. Model importance judgments are not guaranteed to match every human preference.

Screenshots, model/audit correlation, sanitized count/time snapshots and build hashes are retained locally. Raw mail, drafts, account identifiers, provider configuration and private artifact paths are excluded from this public record.
