# Composed Mail cards: draft, chat and host approval

English | [简体中文](mail-composable-cards.zh-CN.md)

The ADR0007 draft, chat and review paths are implemented. Both DeepSeek and
MiniMax have exercised repaired model-authored cards on a OnePlus 6 test build:
editing, live contextual chat, explicit suggestion acceptance, cancellation,
injected-approval rejection and restart restoration. Final build 0416 also
verified concise, plain-text contextual answers from both actual models without
changing the saved draft. **Physical human approval of a real send and the full
native UI matrix remain unverified.**
Android physical touchscreen input is the only positive approval route in this
iteration; desktop and accessibility approval are deferred requirements.

A generated card describes presentation. Rust host code owns the account,
original email, saved draft, revision and send operation. Editing a field changes
a durable draft; a generated Review reply chip only opens host review. Neither
the model nor a card-local `sent` state can authorize SMTP.

## Follow one reply through the code

1. The incoming dispatcher starts Mail's app agent under the signed-in account.
   The agent reads the message, then calls `mail.propose_reply` with `message`,
   optional `folder` and suggested `body`. [The Mail service](../apps/mail/host-service/src/drafts.rs)
   derives recipient/reply headers and returns `draft_id`, `revision` and
   `chat_thread`. `mail.draft` reads the saved revision;
   `mail.suggest_reply` creates a revision-bound suggestion without overwriting
   edits; `mail.propose_send` prepares an immutable proposal, not a send.
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
`27e9c1bfdbf6021bcad87214ae4ebbe6d683b406`. Both Mail sources require literal app
identity; this host additionally requires their literal draft ID to match the
trusted publication. Draft `to`, `subject`, `body` and `suggestion_body` remain
model-tainted for checker purposes: display/edit is allowed, direct reuse in
an action payload or source selector is not. Host approval authorizes the exact
stored message, without requiring a person to retype an unchanged AI draft.

The final device run below used build **0416** (`abf06d8f`), which
used Octoscript `9ca9545b` and Octoscript-Makepad `a950f7fb`. The current pins
only apply rustfmt to the same L0 implementation and propagate that revision
through the wrapper. This formatting-only follow-up was not rebuilt on the
phone; it does not add a new device-validation claim.

The [Makepad overlay](../tools/runtime-patches/makepad-trusted-user-input.patch)
keeps `trusted_user_input()` false by default. Android JNI checks a positive,
nonvirtual device, touchscreen source, unobscured flags and noncancelled input;
null objects or Java exceptions enqueue no trusted input. A scoped guard applies
only during the native TouchUpdate handler. Remote/nested synthetic dispatch,
deferred actions and script tasks do not inherit it. Host review requires a
matching trusted press and release. `with_untrusted_input` can only remove trust.
Keyboard/IME, long press, desktop and accessibility input currently cannot
approve a send. A compromised OS/root process impersonating hardware lies
outside this application-level boundary; this is not hardware attestation.

## Verification checkpoint

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
| Physical approval and real threaded reply | No real SMTP send performed. Physical human approval, provider acceptance and matching card/sheet receipts remain unverified. |
| Desktop/accessibility approval | Deferred; the current provenance boundary denies these routes. |

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
benchmark or model ranking. Simulated UI input remains distinct from trusted
physical approval. No real SMTP submission, provider acceptance or recipient
delivery is claimed; those gates remain open.
