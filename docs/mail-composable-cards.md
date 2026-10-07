# Composed Mail cards: draft, chat and host approval

English | [简体中文](mail-composable-cards.zh-CN.md)

The native **Email / Chat** workspace shares one saved draft. In isolated OnePlus 6
phone tests, both actual `deepseek-v4-flash` and `MiniMax-M3.1-Flash-Preview`
changed an appointment time through chat, and the Reply pane and final review
showed that exact saved change. In the earlier controlled shipping demo, a
person approved the send with a physical press, and the threaded SMTP reply was
verified. These are separate checkpoints: the new workspace tests did not send
real email.

Sending needs a physical press on the host's review, either a tap on Android or
a click on macOS; synthetic and remote input are refused. The macOS path is
**unverified**: no real message has been sent from a Mac. Windows, Linux and
accessibility approval, and the complete phone UI matrix, remain deferred.

After a saved transport receipt confirms SMTP acceptance, the bound reply card
and notification leave Glance. The draft and send receipt remain stored, and an
open success review stays visible until Back. Failed or uncertain sends remain
actionable. New incoming messages can produce new important-mail cards; this
does not merge their chat threads. See [completion validation](testing/mail-completion-2026-10-05.md).

A generated card describes presentation. Rust host code owns the account,
original email, saved draft, revision and send operation. Editing a field changes
a durable draft; a generated Review reply chip only opens host review. Neither
the model nor a card-local `sent` state can authorize SMTP.

## Reply workspace design and current validation

The former interaction had two competing documents: the assistant's suggested
text and the saved reply. It could say “corrected” while review still read the
old body. Requested edits in native chat now update the saved draft through a
revision-bound host capability; successful prose alone cannot update the UI.

| Moment | What the person sees and does | Host behavior |
| --- | --- | --- |
| Open a Mail card | Summary first; tap expands to a full-screen Email / Chat workspace | Collapsed summaries run no generated UI; the workspace retains its own session |
| Ask for a change | Fixed, growing composer above the keyboard; transcript scrolls independently | Supplies the current saved draft; the edit tool saves only the matching revision |
| Model finishes | “Reply updated · saved” appears after a saved model edit | Uses authoritative draft state, never inferred success from assistant text |
| Read or edit | Body is directly editable; Details expands recipient/subject fields | Keystrokes stage locally; idle saves are coalesced; switching to Chat flushes edits |
| Compare with the email | View original email inside Email | Reads the bound original email without replacing the reply |
| Review and send | A separate final review shows From, To, subject and exact body | Only a physical press on Approve & Send authorizes that immutable payload |
| Save conflict | Unsaved text remains visible, with Use my edit / Use saved reply | Never silently overwrites newer data; blocks chat/review until resolved |
| Sending or uncertain outcome | Explicit sending, accepted, failed or unknown state | Keeps receipts and requires a fresh review/approval for an explicit retry |

The conversation uses a virtualized `PortalList`, paragraph rows and a separate
native `TextInput`. Reply has its own scrollable preview/editor. Neither typing
nor scrolling rebuilds an L0 conversation. Native Mail opening also defers
unused generated-layout lowering. Tabs and primary controls have at least
44-point targets; the editor gives the keyboard-adjusted space to the body.

Original, Details and Review now share one compact row. Recipient and subject
editing expand on demand, while the body keeps the remaining space. Selection
uses a muted blue-gray band without changing the editor background. See the
[compact-editor validation](testing/mail-compact-editor-2026-10-05.md) for the
current checks and limits; historical checkpoints below describe earlier builds.

**Paired model checkpoint (Lab build 0427):** fallback providers were disabled.
The existing DeepSeek-authored L0 source was preserved (SHA-256
`5186f14c6e692dfc63af20d788876630cdde6e381411404fd3a7d7d765e2ed30`).
Only the isolated test's authoritative email/draft was replaced with a fictional
appointment; no Gmail credentials were copied. Both models received the same
request to change Tuesday at 3:00 pm to Wednesday at 10:00 am and keep the rest.
Both actually called `mail.suggest_reply`, received `applied: true`, and saved
revision 2 with the exact expected body. The model did the rewrite. The evaluator
drove the phone UI and inspected storage, tool records and screenshots.

Both final-review payloads matched the saved revision-2 body; injected approval
was refused and cancellation returned to draft. DeepSeek's correction survived
restart. A subsequent native editor change saved revision 3; MiniMax correctly
read the new personal note from Chat without modifying or sending the email.
This proves the two-way workspace connection. It is not a new autonomous
incoming-mail test or a controlled comparison of model quality.

**Final layout/performance checkpoint (Lab build 0429):** actual phone captures
cover Chat, Reply and the composer above the keyboard. Short warm scrolling
windows measured frame-gap p95 of 17.5–18.4 ms; fully active windows ran around
57–59 fps. First-open/tab windows still included 300–380 ms pauses. Those cold
pauses remain a performance limitation; warm metrics do not describe cold start.
Lab build 0431 additionally checked long recipients and subjects; metadata now
wraps within two lines with explicit ellipsis. Secondary controls preserve their
palette when focused. The final separate user test package is build 0432, using the same production
code. Installation preserved the active publication, draft, transcript and agent policy byte for byte; no real draft edits or chat submissions were injected.

**Local verification:** 905 shell tests, 53 Mail-service tests and 12 contextual
chat tests passed; two pre-existing environment-dependent Mail tests remained
ignored. The native Makepad draw tests cover 200 transcript entries, keyboard
resizing, a visible composer/review action and deferred layout. Service tests
cover expired/replayed or mis-scoped capabilities, concurrent manual edits,
revoked reviews and zero SMTP from chat. Desktop default/mobile-apps checks,
phone checks and Android builds passed. These checks do not replace on-device
IME, accessibility, account-switch, storage-failure or uncertain-send acceptance.

**Navigation revision (Lab 0433):** real phone captures verify the grouped bottom controls in Reply, Chat and above the keyboard. The 905-test shell suite and desktop/phone checks passed. Build 0434 carries the same layout in the separate user test app; installation preserves the real card, draft, transcript and policy. This revision did not repeat model calls or SMTP.

**UX acceptance reopened; the previous 9.0/10 score is withdrawn.** The review
mistook functional correctness for usability and missed the frequent travel
between top mode tabs and bottom edit/review controls. The successful model and
storage tests remain valid; they do not establish a UX pass. No replacement
numeric score is claimed.

The current revision replaces the feed-sized expansion with a resident full-screen workspace. The summary supplies an animation origin; active layout uses the shell's safe viewport and never the feed header, margins or height cap. The feed stays compact and cannot receive input while covered. **Email / Chat** share one row. Email contains the original-message toggle and directly editable draft; Chat pins its composer and links to the actual saved update. Review occupies the workspace and still requires a physical press to approve the exact message. Back dismisses keyboard, review and workspace in that order. Collapse retains the session, unsent chat and widget state; three inactive clean workspaces are cached, and dirty human input is excluded from clean eviction. This retention is within the current process. Account invalidation and publication withdrawal retire the relevant view; pending review authority is never retained. Reduced Motion or a missing source rectangle uses an immediate transition. Notification entry opens above the current phone screen without first navigating through Glance. Native first-open/session costs remain separate from presentation animation and must be measured.

**Full-screen workspace checkpoint (Lab 0454/0456/0458, user test build 0459):**
The OnePlus 6 now draws the workspace at the root safe viewport (`384 × 758`
logical points in the recorded portrait layout), independently of the feed's
margins and former 620-point cap. Native captures verify Email/Chat, the original
message toggle, directly editable fields and the composer above the keyboard.
Unsent chat text survives keyboard dismissal, workspace collapse, opening a
different card and returning to the cached conversation. Exact review matches
the saved recipient, subject, body and revision; Back cancels review without
changing that draft. No send approval or SMTP was performed.

An initial Lab run exposed a missing Android extension for the isolated UX
package: system Back finished its activity and interrupted a request. The UX
package now delegates to Home's existing extension. After the fix, Back retains
the activity and kernel processes. Across six warm open/close cycles (12
transitions), 157 host frame intervals measured median **16.68 ms**, p95
**18.78 ms**, maximum **19.53 ms**, with none above 25 ms. This measures frames
between open/suspend and settled markers; it excludes first-open setup and is
not display-presentation or input-to-photon latency. It does not establish the
complete device or usability matrix.

The new DeepSeek edit request failed at network connection: Wi-Fi was enabled
but disconnected, DNS failed, and no saved network appeared in the scan. The
saved draft stayed unchanged. **Fresh paired-model validation is incomplete**;
MiniMax was not rerun while offline. The successful Lab 0427 model checkpoint
above is historical evidence, not a pass for this build. Local verification:
**912 shell tests**, 12 contextual-chat tests, final Mail pane draw tests,
desktop default/mobile-apps and phone checks, both shell graphs, runtime pins
and Android packaging passed. Build **0459** is installed in the separate user
test app; installation preserved its real publication, draft, transcript,
policy and provider configuration byte for byte. A final theme-refresh regression
resets cached child colors after script reapplication while preserving typed
text; Lab 0458 and the user test app use that fix. No numeric UX score is claimed.

The 0429 frame measurements above were for the **Chat pane**, not the Glance feed. They cannot substantiate feed performance.

**Historical Glance measurement (Lab 0439–0444):** 12 alternating 600 ms vertical swipes on the OnePlus 6, with four bound Mail publications and two shell items. Across 430 consecutive active frame intervals: median **16.74 ms**, p95 **17.62 ms**, maximum **35.67 ms**; nine exceeded 25 ms. These are host frame markers, including stalls and excluding inactive transitions, not display-presentation or input-to-photon latency. Three extra publications were explicitly labelled layout fixtures using the existing model-authored L0 source and fictional Lab draft; this did not generate emails or run a model. The preview no longer constructs or dispatches any generated UI. Lab 0443 verifies expansion, Details/Reply, exact review/back, and retirement of the workspace when entering Recents; 0441 also checks the focused native editor above the keyboard. An unsent Chat input survives switching to Reply and back. A review creates a cancelled attempt record when backed out; the draft revision, recipient, subject and body remain unchanged. Build **0444** was installed in the separate user test package; the real publication, draft, transcript and policy were byte-identical immediately after installation. The final **907 shell tests**, desktop default/mobile-apps and phone checks, both shell graphs, runtime pins and Android builds pass. This UI revision runs no new model turn or SMTP send. The final phone captures check summary, expansion, same-row modes, editor and keyboard composer; they do not establish a 9/10 usability pass.
Acceptance now needs the actual repeated task to work comfortably: ask for a
time change, switch to Reply, edit, switch back to Chat, then review and return.
Check hand travel, control grouping and visual hierarchy as well as saved data,
keyboard visibility and performance. Automated geometry checks establish where
controls are drawn; they do not establish comfortable one-handed operation or
replace feedback from the person using the phone.

## Shared workspaces for all card publishers

The full-screen transition is shared by every Glance publisher, including L0
and Splash cards. It does not launch the publisher's full app. Mail keeps its
specialized **Email / Chat** panes; other cards use their original generated UI
and **Card / Chat** when the publisher declares an app agent. A card without an
agent still opens full-screen, with no invented assistant or capabilities.

`glance::GlanceCard.account` records the host's account at publication. Opening
cannot rebind an old card to a new account. `glance_card::WorkspaceChat` provides
a separate host-owned conversation when the card has no explicit `sys.chat`;
it never inserts chat declarations into the model-authored source. Its
`ContextKind::Card` binding contains the publication and L0 local state, bounded
separately to 12 KiB. These are untrusted context, not tools or approval. The
existing app peer executes permitted tools. First-use **Enable assistant** opens
the existing host consent sheet. Mail's draft-edit lease requires
`ContextKind::Mail` and cannot be issued from this generic binding. Splash chat
receives publication context, not a snapshot of arbitrary isolate variables.

Local L0 changes, interacted Splash views and unsent native chat are protected
from the three-clean-workspace cache limit. Same-layout L0 republication keeps
local state; identical Splash bodies retain their isolate. A changed non-Mail
layout is deferred while that workspace contains local work, until withdrawal
or expiry; the feed can show the newer summary meanwhile. Retention is within
the process. Expiry, withdrawal and account invalidation retire workspaces.

**Six-family OnePlus 6 checkpoint (Lab 0460/0464/0466):** the existing Android
DeepSeek turn-12 and MiniMax turn-14 collections supplied six original L0
`glance.card` / `glance.data.json` pairs each. Source and data hashes were
recorded; no generated source was rewritten. All 12 published cards opened
from compact summaries into the root `384 × 758` logical-point workspace.
The evaluator drove ADB; models authored the templates and handled the separate
live chat requests. The developer fixture loader uses normal app admission.
Calendar is now included in the phone's system-app catalog.

| Family | Observed local interaction in both collections | Scope |
| --- | --- | --- |
| Mail | Reply view; unsent native chat retained after visiting five other cards; MiniMax's typed generated reply also retained | Prototype reply is local, separate from the bound Mail service test below |
| Calendar | RSVP selection: DeepSeek Maybe, MiniMax Going | No invitation response sent |
| News | Save | Local state, not a backend bookmark |
| Finance | Watch | Unprivileged `test.finance`; no shipping Finance app or agent |
| Photos | Favorite; retained on return | Synthetic metadata, no photo-library access |
| YouTube | Watch later | Local state, no playback or remote playlist update |

Actual `deepseek-v4-flash` and `MiniMax-M3.1-Flash-Preview` Calendar agents then
answered the same question about the event, current RSVP and whether anything
was sent. Both read their card data and actual local choice correctly, and both
said nothing was sent. Fallbacks were disabled; ledger terminal messages
confirm each provider. This compares context handling, not model performance:
the two original templates contain different event data. MiniMax's answer also
contained unnecessary implementation wording and literal Markdown emphasis.

**Fresh bound-Mail regression (Lab 0466):** DeepSeek called `mail.suggest_reply`
and saved Thursday at 2 pm as revision 4, preserving the personal note and other
text. MiniMax saved Wednesday at 10 am as revision 5, but introduced literal
backslash-n characters instead of paragraph breaks. After explicit feedback
through the same Chat pane, MiniMax saved real line breaks in revision 6; it
also removed one dangling comma. The first formatting attempt failed, and the
retry does not prove exact punctuation preservation. Both final Email views
and review payloads matched the saved recipient, subject, body and revision.
Back cancelled each review. No send approval or SMTP occurred, and no Gmail
credentials were copied into the isolated Lab. These fresh live calls supersede
the earlier offline model-validation gap; the failed 0459 checkpoint remains
historical evidence.

**Remaining acceptance:** DeepSeek Photos and YouTube captions still clip in
the unchanged authored templates. The new phone pass covers L0 cards; full
Splash bundles, the complete account/IME/accessibility matrix and cold-open
latency remain unverified by this pass. Existing frame measurements above are
historical; no new timing or numeric UX score is claimed. Local validation:
**916 shell tests**, **13 contextual-chat tests**, three final native-chat draw
tests, desktop default/mobile-apps and phone checks, both shell graphs, runtime
pins and Android packaging passed.

Lab **0468** also verifies declining first-use assistant consent: the Enable
assistant control disappears and chat remains unavailable. User test build
**0469** carries this final consent-state refresh. Installation preserves the
real publication, draft, transcript, policy and provider configuration byte for
byte; publication, draft, transcript and provider remain unchanged after startup.

## Follow one reply through the code

The native workspace now uses **Email / Chat**. Email reads the authoritative
draft through `mail_clip::MailClip`; typing stages in `mail_card::Session`, saves
after 500 ms idle, and flushes before Chat or review. It retains unsaved text on
conflict. The native chat can issue a five-minute, one-use body-edit capability
for the displayed account/draft/revision. With that `edit_token`,
`mail.suggest_reply` saves the requested change and returns `applied: true`.
Without it, background suggestions still need explicit acceptance. Only saved
host state drives “Reply updated”; model prose does not. Neither path sends mail.
See ADR0007's native-chat exception for the revised contract. Earlier device
checkpoints below concern the former suggestion-acceptance workflow.

1. The incoming dispatcher starts Mail's app agent under the signed-in account.
   The agent reads the message, then calls `mail.propose_reply` with `message`,
   optional `folder` and suggested `body`. [The Mail service](../apps/mail/host-service/src/drafts.rs)
   derives recipient/reply headers and returns `draft_id`, `revision` and
   `chat_thread`. `mail.draft` reads the saved revision;
   `mail.suggest_reply` creates a revision-bound suggestion or consumes the
   native chat's scoped edit token; `mail.propose_send` prepares an immutable
   proposal, not a send.
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
notification. Five cache tests and 27 Glance tests passed. The DeepSeek phone
checkpoint below also retained the saved body and card through process restart;
this does not establish every account-switch, expiry or dismissal path on-device.

Generated views use explicit enum comparisons (`when mode == .brief`), with a
visible initial branch and reachable Back actions. A bare `when mode` only
accepts boolean `true`. Chat typing updates local text; `on_commit` appends the
complete question. Appending on `on_change` sends partial keystrokes and can
hit the thread's rate/busy limits. Reply-body editing uses `on_change` without
`on_commit` for the shell's multiline editor.

An identical `mail.publish_card` payload reuses its durable receipt without
another notification. Corrected source/data under the same card and draft IDs
is revalidated and republished, honoring the requested `notify` value. An older
receipt without a payload fingerprint allows one validated refresh.


## Runtime contracts and input provenance

The [tool schemas](../apps/mail/bundle/tools.json) describe the four draft/proposal
tools. Catalog contracts are pinned to Octoscript
`2e37d9e657a246f16718d9a475e167ccd2d5b5fa`; Octoscript-Makepad is pinned to
`aa80f72c509c767faf04822b939d44b2f34fbc81`. Both Mail sources require literal app
identity; this host additionally requires their literal draft ID to match the
trusted publication. Draft `to`, `subject`, `body` and `suggestion_body` remain
model-tainted for checker purposes: display/edit is allowed, direct reuse in
an action payload or source selector is not. Host approval authorizes the exact
stored message, without requiring a person to retype an unchanged AI draft.

The historical device run below used build **0416** (`abf06d8f`), which
used Octoscript `9ca9545b` and Octoscript-Makepad `a950f7fb`. The current pins
only apply rustfmt to the same L0 implementation and propagate that revision
through the wrapper. The newer 0427–0432 workspace builds use the current pins; their validation
is recorded above, separately from those historical runs.

The [Makepad overlay](../tools/runtime-patches/makepad-trusted-user-input.patch)
keeps `trusted_user_input()` false by default. Android JNI checks a positive,
nonvirtual device, touchscreen source, unobscured flags and noncancelled input;
null objects or Java exceptions enqueue no trusted input. A
[second overlay](../tools/runtime-patches/makepad-desktop-trusted-input.patch)
trusts a macOS pointer press or release only when AppKit supplies a CGEvent
from the HID source with no posting process.

A scoped guard applies only while the native handler of a trusted touch
(`TouchUpdate`) or click runs. Remote and synthetic dispatch (with any native
callback nested inside it), deferred actions and script tasks do not inherit the
guard. Host review requires a matching trusted press and release.
`with_untrusted_input` can only remove trust. Keyboard/IME, long press,
accessibility input and any input on Windows or Linux cannot approve a send. A
compromised OS/root process impersonating hardware lies outside this
application-level boundary; this is not hardware attestation.

<a id="verification-checkpoint"></a>

## Historical verification checkpoint (0414–0416)

The paired-model phone flows below ran on code
`57b711ae9d8ce595d37a78c7bd0f2253bb3f962b`, Android test build **0414**.
Build **0415**, code `97a23a3f77ecda0426be3efc654cdbb1c9f20ba9`, adds native
notification-toast touch handling. A real banner tap opened the exact bound
card with the saved revision-44 body intact.
Build **0416**, code `abf06d8f`, adds response-presentation guidance to the bound
card-chat request; its 11 chat tests passed. With the same fresh read-only
question, both actual models correctly referenced the saved note in concise
plain text, without Markdown or raw IDs. Revision 44 and its body stayed
unchanged. Repeating injected approval left the operation awaiting approval;
Cancel returned it to draft. These builds use a separate test application on
the OnePlus 6. These are partial
acceptance results; installing the later build does not rerun the earlier flows.

| Gate | Observed result or remaining work |
| --- | --- |
| L0 catalog and provenance rules | 315 Octoscript tests passed, including five Mail-source tests. |
| Portable UI translation | 86 Octoscript-Makepad portable tests passed, including its doc test. |
| Integrated shell | 884 tests passed for 0414; 888 passed for 0415, including four new toast-touch regressions. |
| Mail service | 50 tests passed; two existing live/environment-dependent tests remained ignored. Fake transport tests do not prove live SMTP. |
| Packaging | Desktop and phone checks, dependency-graph checks and Android builds passed. Test builds 0414, 0415 and 0416 were installed on the OnePlus 6. |
| Bound chat presentation | 11 chat tests passed for 0416. Both actual providers answered the same fresh read-only question concisely in plain text, with the correct saved note and no draft mutation. |
| DeepSeek card | Native phone editing, context-bound chat, explicit suggestion acceptance, cancel and restart restoration exercised; details below. |
| MiniMax card | Phone editing, live contextual chat, explicit suggestion acceptance, injected-approval rejection, cancel and restart restoration exercised; scoped hidden-native checks also passed. |
| Remaining phone UI matrix | Full IME composition, cursor/selection, Unicode, themes, touch-target sizing, conflicts, account switching, expiry and retry coverage remains incomplete. Relevant local tests are not a substitute. |
| Notification-toast touch route | Four new regressions passed. On 0415, a visible notification followed by an injected Android tap opened the exact bound card and retained revision 44. This proves navigation, not send authorization. |
| Physical-press approval and real threaded reply | No real SMTP send performed. Approval with a physical press, provider acceptance and matching card/sheet receipts remain unverified. |
| Desktop/accessibility approval | Deferred at this checkpoint; the provenance boundary denied these routes. Later builds accept a click on macOS as a physical press (**unverified**); input on Windows or Linux, and accessibility input, still cannot approve. See [Runtime contracts and input provenance](#runtime-contracts-and-input-provenance). |

### Per-provider engineering review

A separate agent reviewed the actual generated cards through native Makepad
controls and captures. Scores apply to these repaired artifacts and exercised
paths, not general model capability. The final reviews include the 0416 live
answers. Neither final artifact reaches the 4.5/5 target.

| Model | Final source SHA-256 prefix | Visual / 5 | Scoped usability / 5 |
| --- | --- | --- | --- |
| `deepseek-v4-flash` | `00ddd83c` | 4.1 | 4.4 |
| `MiniMax-M3.1-Flash-Preview` | `b5252ee8` | 4.2 | 4.4 |

Both ran without model fallback. Both preserved the edited body until explicit
suggestion acceptance and rejected injected approval. Final chat answers resolved
the earlier verbose/raw-Markdown issue in the exercised question. DeepSeek still
has three repetitive AI badges, small secondary targets and nested scrolling;
MiniMax's Details labels remain confusing. Guidance and host fixes changed between
runs, so publication counts and scores are not an equal-budget benchmark.

### DeepSeek: what worked and what needed correction

An incoming test email autonomously triggered Mail's agent using
`deepseek-v4-flash` and creation of a reply draft. The model authored the card source. Its initial run made 13
publication attempts; the resulting syntactically admitted card showed only
a title because it used an enum as a bare boolean guard. Source inspection also
found chat wired to append on `on_change`, creating a per-keystroke submission
risk. The hidden editor/chat views prevented exercising that path.

Operator feedback asked the model to repair its own source. The first feedback
turn exposed a host bug: publication returned the old receipt for the same card
ID without applying corrected source. The revision above fixes that deduplication
boundary. The second feedback turn encountered five validator rejections and
was interrupted. The third made four publication calls: three rejections,
then success. The repaired checkpoint uses a `Section` component with typed
props and a slot, plus explicit enum guards. Its source SHA-256 is
`80b785bc22acf5acf5734e2685a17db1825ee9da091057a3480d58a119af48a3`.
The fourth feedback turn received seven publication rejections and was
interrupted without a successful replacement; the tested source above remained
in place. Possible input-harness truncation of that long feedback is under
investigation, so its counts must not be attributed solely to model capability.
The fifth feedback turn used a short prompt and the complete earlier source
as a file reference. DeepSeek published once successfully, with no rejection.
Its replacement source SHA-256 is
`00ddd83c82c46fd8b6a97b7718ed8b4fe86eadb1fc66985f869dfc5bf17b692c`.
The phone displayed compact Reply/Chat/Details controls; a visible notification
followed by an injected tap opened its bound sheet with revision 44 unchanged.
Its separate final review, including the 0416 answer, scored visual presentation
4.1/5 and scoped usability 4.4/5, as listed above. No operator hand-edited either
model-authored card.

A focused hidden-native check of the final `00ddd83c…` source passed navigation,
editing revision 44 to 45, exact review, injected-approval rejection and Cancel,
one Ask submission with retained question text, Clear and process restart. This
isolated rig had no live provider; the 0416 phone answers are separate evidence.

Operator-driven native UI checks on the earlier `80b785…` card established:

- Appending a test note in the editor durably advanced the draft from revision
  1 to 43 through field changes.
- Submitting chat with the injected UI's **IME Done** action reached DeepSeek.
  It read revision 43, quoted the exact test note and called
  `mail.suggest_reply` with `expected_revision: 43`. This was an IME action,
  not evidence that a raw ADB Enter key submits the field.
- The suggestion left the saved body unchanged until the host's **Use draft**
  action accepted it, producing revision 44.
- Process restart retained the saved body and the published card.
- Injected **Approve & Send** input was explicitly refused and left the
  operation awaiting approval. **Cancel** returned the draft to `draft`.
  No SMTP submission occurred.

### Separate native-shell review

A separate agent exercised the same DeepSeek source in a hidden native Home
shell using Metal, isolated host storage and demo transport, without a provider
or running kernel. It passed summary/details/reply/chat navigation, a durable
edit from revision 1 to 2, exact edited-message review, injected-approval
rejection, cancellation and process-restart restoration. Applying a reversible
immutable-file fault to the copied draft also preserved failed input without
changing saved bytes; **Restore saved** recovered the durable text. The fault
was removed afterward. These checks did not alter the original card source.

Typing in chat created no message until Return. Submission then produced one
user row and the expected unavailable-kernel error. That offline result earns
no credit for a model answer; the live DeepSeek answer above was observed
separately on the phone.

For that earlier source, the reviewing agent rated visual presentation **3.8/5** and usability in the
exercised offline scope **4.3/5**. Remaining issues include an undiscoverable
Return-only chat submission, repetitive AI badges, small generated secondary
targets and nested scrolling. One immediate post-edit native frame had unreadable
field colors before redraw; this is a retained capture caveat, not an established
phone defect. No overall acceptance or A− score is claimed.

### MiniMax: generation and repair checkpoint

After the system agent provisioned the policy once, a new incoming fixture
autonomously triggered **MiniMax-M3.1-Flash-Preview**, with no model fallback.
Its initial run made three publication calls: two rejections followed by
success. The resulting card had a clipped Ask control. The first feedback turn
again made three publication calls: two rejections followed by success. The
repaired card now visibly exposes compact Reply, Chat and Details controls;
its source SHA-256 is
`987622aa06fe5c6c14d062bd5af536cb8a7eedff32540fae4427ff081e474041`.
The model authored both versions without operator source edits. On the phone,
appending a note preserved the original body and advanced revision 1 to 43.
The visible **Ask** chip submitted once; MiniMax read revision 43, quoted the
exact note and called `mail.suggest_reply` with `expected_revision: 43`.
The saved body remained at revision 43 until **Use draft** accepted the exact
suggested body, producing revision 44 with the test note intact. This chat
submission used the visible control, not IME submission.

Host review showed revision 44. Injected **Approve & Send** displayed the explicit
automated-input rejection and left the ledger awaiting approval. **Cancel**
returned it to `draft` with a cancelled attempt, without sending. After process
restart, the saved revision-44 body matched exactly. These were operator-driven
phone checks, separate from the hidden-native checks below.

A separate agent then exercised the unchanged MiniMax card in the same isolated
hidden Makepad setup used for DeepSeek. Navigation, a durable revision-1-to-2
edit, exact edited-message review, injected-approval rejection, cancellation,
restart and failed-save/Restore saved checks passed. Chat created no row while
typing, submitted once through **Ask**, retained the question after the expected
missing-kernel failure, and cleared it through **Clear**. This harness had no live
model or SMTP transport; the actual model response came from the phone run.

Its earlier review scored **4.1/5 visual** and **4.4/5 scoped usability**. The final
review includes the improved 0416 phone answer and scores **4.2/5 visual** and
**4.4/5 scoped usability**. Details remained
repetitive, with a confusing "Replying to"/"Simulation notes" sequence before
the address. Small secondary Chips lack 44-point evidence, and nested scrolling
still complicates navigation. Passing the exercised controls is not complete
native UI acceptance.

Both providers' live answers were verbose and exposed raw Markdown. The
event-policy guidance did not reach the separate card-chat request, which
lacked concise plain-text response instructions. That host-guidance gap and
presentation shortcoming remain separate from the
verified email/draft binding. Build 0416 added those instructions to the actual
bound request; both fresh model answers then passed the concise plain-text check
described above. Android notification-toast activation also exposed
a missing native-touch handler. On corrected build 0415, a visible notification
and an injected Android tap opened the exact bound sheet with revision 44 intact.

During that check, MiniMax reported a missing tail in a long source reference
and reconstructed it, producing a different model-authored layout. A subsequent
protocol-ledger comparison confirmed that the intended 5,426-character prompt
became a 2,371-character exact prefix before reaching the model, losing 3,055
characters. The precise input/transport cause remains under investigation; this
loss is not a model error. The
navigation result stands, but the earlier visual score does not transfer to
that temporary layout. The model subsequently read its staged earlier source
and restored it, then reused its own shorter notification wording. The final
source SHA-256 is
`b5252ee8ee0702014aeace89e17fd85c428d171d7815561b4f49a118b4dc5ec5`:
it differs from the reviewed `987622…` source only by two spaces in a
`ChatEntry` argument list, with no semantic or UI change. Data is byte-identical,
and the saved draft stayed at revision 44. The final publication succeeded;
the observed toast activation was on the earlier temporary layout, not a fresh
capture of this final notification. No operator edited card source. These
input and recovery conditions prevent treating the attempt counts as model
performance measurements.

The provisioned grammar guidance improved between providers, including chat
input guidance. This is engineering validation, not a controlled performance
benchmark or model ranking. Synthetic UI input remains distinct from a trusted
physical press. No real SMTP submission, provider acceptance or recipient
delivery is claimed; those gates remain open.
