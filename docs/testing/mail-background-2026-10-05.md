# Mail collection and Android background validation — 2026-10-05

English | [简体中文](mail-background-2026-10-05.zh-CN.md)

Scope: independent collection/delivery, selective model decisions, and [ADR 0008](../adr/0008-quiet-android-mail-jobs.md). Tests used an assigned OnePlus 6, Android API 35, the separate `dev.makepad.octosense.studio` (OctoSenseMailTest) package and its existing DeepSeek `deepseek-v4-flash` profile. The installed Home and ROM were not replaced. Gmail and provider credential-file hashes remained unchanged across upgrades. No reply was sent by the test driver.

## Local checks

| Check | Result |
| --- | --- |
| From `phone/`: `cargo test --locked --features mobile-apps -p octosense-shell -p octosense-mail-service` | 930 shell + 54 Mail tests passed; 2 optional Mail tests ignored |
| From `phone/`: `cargo check --locked -p octosense-home --features mobile-apps` | Passed |
| Root: `cargo check --locked -p octosense`, with and without `--features mobile-apps` | Passed |
| Root: `bash tools/check-shell-graph.sh -p octosense`; phone equivalent with `-p octosense-home` | Passed; one pinned octos revision |
| `python3 tools/setup.py --check --cargo`; `python3 tools/native_apps.py --check` | Passed |
| `python3 -m unittest discover -s rom/tests -p test_no_local_paths.py` | Passed |
| Separate debuggable Android APK, preserved account data and bundled kernel hash | Built and installed; latest test version 2026100512 |

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

## Remaining checks and limits

- The shipping simulation reached the Inbox and is pending behind the existing queue. Its final card outcome is still under observation.
- AgentMail accepted a separately labeled no-action control. Its arrival in the monitored Inbox has not yet been established; it must not be counted as an agent skip.
- The exercised jobs were explicitly forced through Android's test command. A natural periodic/Doze/reboot cycle is **unverified** at this point; registration does not prove its delivery latency.
- Notification permission/channel denial, account removal during a job, and other Android versions are not all device-tested in this change. Existing consent/account tests and the new outbox scope/expiry tests cover host-side boundaries.
- A large backlog, a slow model or Mail's four-live-card limit can delay later decisions. The queue remains bounded at 128. Model importance judgments are not guaranteed to match every human preference.

Screenshots, model/audit correlation, sanitized count/time snapshots and build hashes are retained locally. Raw mail, drafts, account identifiers, provider configuration and private artifact paths are excluded from this public record.
