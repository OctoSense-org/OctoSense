# Reproduce the live Mail → card → Calendar demo

English | [简体中文](README.zh-CN.md)

Use your own mailbox and model account. New mail triggers the Mail agent;
the model decides whether to publish a card. You can chat about its saved reply,
edit it, review it and optionally approve sending. An explicit Chat request lets
Mail call Calendar's tools, create a local event and publish Calendar's own card.
**Open Calendar** inside that card opens the same event in the Calendar app.

This is the real system-app workflow. The hackathon examples in OctoSense App
Flow (formerly Design Flow) use fictional local outboxes and calendars; they do
not reproduce Gmail delivery.

## 1. Get the complete source

The runtime checkpoint is OctoSense commit
`4081c30e432ad0c3d0260c90be804d5e8aa5a9a1` (PR #342), which contains the earlier
Mail/card/Calendar changes. Use this guide's branch, or a main revision containing
it, to get the input generator too. Do not cherry-pick only the final UI commit.

| Dependency | Revision selected by this checkpoint |
| --- | --- |
| App Hub | `7bb63ff925d19f9aecbc72b8946679320e53cfa1`, merged in PR #93 |
| Octoscript-Makepad | `27e9c1bfdbf6021bcad87214ae4ebbe6d683b406` |
| Makepad base | `c155f61d0e1600d2ec474209374444a38a09a470` plus committed `runtime-patches.lock.json` patches |
| Octoscript | `2e37d9e657a246f16718d9a475e167ccd2d5b5fa` |
| octos kernel | `056173e85b150e387805fc307fe231064ac1ed35` |

Clone [OctoSense](https://github.com/OctoSense-org/OctoSense), check out the guide's
revision, then run from its root:

```sh
python3 tools/setup.py
python3 tools/setup.py --check --cargo
python3 tools/native_apps.py --check
```

If you already have dependency clones, configure the source hub as described in
[Set up](../../../README.md#set-up) before setup. It creates the pinned worktrees
and applies tracked patches. No developer's dirty dependency checkout is needed.
Cargo fetches the App Hub and kernel revisions; separate legacy AppCard,
OctoSense-mobile or App Flow checkouts are not required for this live demo.

## 2. Build one Android package with its kernel

Use the prerequisites in [Phone build](../../../phone/README.md#build-and-run).
For an existing Home installation, prefer a separate package. Set
`MAKEPAD_ANDROID_SDK` to your cargo-makepad SDK directory and `ADB` to your Android
platform-tools executable. Set `DEMO_SERIAL` from your own `adb devices` output.
These paths and device ids stay local.

From the repository root, build the patched packager, then use the kernel wrapper:

```sh
cargo build --release --manifest-path .sources/makepad/tools/cargo_makepad/Cargo.toml
mkdir -p target/mail-calendar-demo
cd phone
python3 ../tools/kernel-artifact.py --sdk "$MAKEPAD_ANDROID_SDK" \
  --receipt ../target/mail-calendar-demo/kernel.json -- \
  ../.sources/makepad/target/release/cargo-makepad makepad android \
  --sdk-path="$MAKEPAD_ANDROID_SDK" \
  --package-name=dev.makepad.octosense.mailcaldemo \
  --app-label='OctoSense Mail Calendar Demo' \
  build -p octosense-home --release --features mobile-apps
cd ..
```

Set `DEMO_APK` to the APK path printed by the packager. Confirm it includes the
agent kernel before installing:

```sh
python3 - "$DEMO_APK" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as apk:
    assert 'lib/arm64-v8a/liboctos.so' in apk.namelist(), 'Missing agent kernel'
print('Agent kernel is bundled')
PY
"$ADB" -s "$DEMO_SERIAL" install -r "$DEMO_APK"
"$ADB" -s "$DEMO_SERIAL" shell am start \
  -n dev.makepad.octosense.mailcaldemo/.MakepadApp
```

These are portable build instructions traced to the shipped scripts. This new
package name has **not** had a fresh-account phone run. The recorded live run used
normal Home on the OnePlus 6; its [dated evidence](../../testing/mail-medical-calendar-2026-10-06.md)
states the tested source and APK hash. Signing changes binary hashes between builds.

## 3. Configure the same installed package

1. Configure a working provider in **AI providers** using your own credentials.
   The medical demo used DeepSeek `deepseek-v4-flash`. Model access is required;
   standalone `card-host` cannot substitute for a live agent. MiniMax was tested
   on earlier Mail/card flows, but the latest medical-to-Calendar sequence was
   validated with DeepSeek only.
2. Open **Mail** and sign in on its host-owned sheet. Use your provider's supported
   IMAP/SMTP credential; Gmail's form offers an app-password field. Do not put
   credentials in chat, scripts, Git or the input files below. Keep demo mode off.
3. Allow Mail's app agent when prompted. Open **Assistant** and ask it to enable
   the new-mail automation with the exact `agents.provision` payload below.
   Provisioning does not bypass app-agent consent.
4. Allow Android notifications for this package and its Mail channel. Keep network
   access enabled. For the first test, keep OctoSense open.

**Account, consent, policy and Glance are per installed package.** If you configure
the separate demo APK, open its Home/Glance page to see its cards. Swiping the
phone's default launcher does not show another package's cards. To test the system
Home swipe, explicitly select this package as Home in Android Settings; no ROM
flash is needed. The normal `dev.makepad.octosense` package also supports this
workflow when configured itself.

## 4. Provision selective triage and establish the baseline

Choose a future date, then generate five fictional input files (this command
never sends mail or reads accounts):

```sh
python3 tools/mail-calendar-demo.py --date 2026-11-12 \
  --timezone America/Los_Angeles --output target/mail-calendar-demo/inputs
```

Use a fresh output directory for each run. In **Assistant**, paste `policy.json`
and say: “Call `agents.provision` with exactly this JSON. Then call `agents.status`
with `{"app":"os.mail"}` and show the tool result. Do not generate a card.”
The policy uses the shipped Mail guidance and triage skill (`skills: []` adds no
override); it selects relevant actionable mail, creates drafts, and requires a
separate human request before booking Calendar.

Wait until status reports `configured`, `enabled`, `consent`, `admitted` and
`runtime.baseline_ready` all true, a recent `runtime.last_collection_success_at`,
and no collection error. The initial Inbox sync establishes a baseline and does
**not** generate cards for existing mail. Send the test messages only afterward.
Keep `account` and any raw status/log output private.

## 5. Send new email and verify the model's decision

Use your own second mailbox or AgentMail sender to send the subject and body from
`appointment.txt` to the account you connected inside OctoSense. The file contains
no recipient: supply your own address privately. AgentMail is an optional real
sender, not a runtime dependency; use your own AgentMail account/key if choosing it.
Send `quiet-newsletter.txt` separately as a negative control. Preserve each unique
demo reference and use a replyable sender you control.

Do not ask Assistant to produce either card. With the 30-second foreground policy,
allow one collection interval plus model execution/backlog time. Mail should
read the appointment, save a draft and publish a relevant card/notification. It
should quietly skip the general newsletter. This is a model decision, not a
guaranteed keyword classifier; a wrong decision fails this acceptance test.

Read `agents.status` to distinguish successful collection from delivery. The
`pending` count should drain; `last_receipt` records publication or skip. If both
arrive quickly, the last receipt can refer to either message. Check the visible
cards and individual private tool history before attributing a result.

## 6. Human Chat, draft edits and the real Calendar

1. Tap the notification or expand the appointment summary in this package's
   Glance. The card opens a focused workspace with **Email / Chat** over one
   saved draft; it must remain scrollable with a usable keyboard/composer.
2. In **Chat**, submit `chat-request.txt`. This explicitly authorizes one fictional
   local Calendar booking. The Mail peer should read `calendar.events`, save the
   reply with `mail.suggest_reply` (`applied:true`), call `calendar.add_event`,
   read back the event, and call `calendar.notify`. A real conflict must stop the
   booking and prompt you. The incoming email alone did not authorize it.
3. Switch to **Email**: the actual saved body, not only the chat answer, must show
   the appointment date, time and timezone. Then ask Chat to add “Please confirm
   the room number.” Verify the saved body changes. Make a manual edit, switch
   tabs and verify it persists. Do not treat an unapplied suggestion as a save.
4. Expand Calendar's card in Glance and tap its in-card **Open Calendar** button.
   It must open the same saved event in the real Calendar app. Compare against
   `expected.json`. Repeat the exact booking request and verify there is still
   one event; changed times require a new explicit decision, not a false success.
5. Tap **Review reply**, inspect the exact recipient/subject/body, then cancel
   for the default unsent demo. To test real sending, physically approve on the
   phone yourself and verify delivery in the sender inbox. ADB, chat text and
   model tools cannot approve SMTP. The medical checkpoint did not test sending.

Calendar storage is local OctoSense storage; this does not sync Google Calendar,
send invitations or schedule Android reminders. Restart the app and verify the
event and saved draft survive. Card expiry/completion can retire a Glance item
without deleting its source mail or Calendar event.

## 7. Background and failure checks

| Symptom | Check |
| --- | --- |
| Mail visible but no card | Initial baseline, agent consent, provision enabled, pending events, model delivery error, or an intentional skip |
| Card inside test app but absent on normal Home | Different installed package/account/Glance store |
| Card but no native notification | `notify`, Android permission/channel, quiet retry or expiry |
| Chat says “updated” but Email unchanged | Saved draft revision and `mail.suggest_reply` result; `applied:false` is not a successful edit |
| Calendar permission error | Correct pinned App Hub, Mail's admitted cross-app grants, tool registration; do not grant arbitrary tools to work around it |
| Slow/absent delivery | Network policy, provider access, collection versus delivery errors, queue/backoff; inspect local status |
| Background stopped after force-stop | Reopen the app; Android blocks jobs after force-stop |

When the app is closed normally, quiet background checks have a 15-minute period
and a 5-minute flex window. Android can delay them; this is polling, not instant
push. Only model-approved important notifications appear. Test foreground first,
then close without force-stop and send another fresh-reference email.

## Validation and privacy

The October 6 run observed automatic DeepSeek triage, human-directed draft
confirmation, Calendar persistence and in-card navigation. It left email unsent.
The recorded time tile still clips the end of the timezone label on OnePlus 6;
this guide does not claim complete visual acceptance or identical model prose.

For local regressions from `phone/`:

```sh
cargo test --locked --features mobile-apps \
  -p octosense-shell -p octosense-mail-service -p octosense-calendar-service
```

Rechecked on 2026-10-06: **1,002 tests passed**, with two optional Mail tests
ignored; dependency graph/pins, native catalog, fictional-input generation and
changed Markdown links passed. The Android kernel build plan was checked. A new
APK build/install, fresh-account login and live provider/SMTP run were not repeated
for this documentation change.

Keep only sanitized counts, pass/fail outcomes and source hashes in a shared
report. Never commit account/provider files, actual mail/drafts/transcripts,
raw status, logs, screenshots, device serials or signing keys. Use the fictional
input generator for shared examples; it embeds no addresses or credentials.
