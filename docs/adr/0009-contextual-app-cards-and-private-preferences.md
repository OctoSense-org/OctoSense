# ADR 0009: Contextual app cards and private preference memory

English | [简体中文](0009-contextual-app-cards-and-private-preferences.zh-CN.md)

Status: accepted; implemented in this change (2026-10-06). Device/model acceptance
is recorded separately; this decision is not a claim that every journey passed.

## Requested behavior

News, Photos and YouTube publish app-owned Glance cards with a compact summary,
focused expanded workspace and native Card/Chat. Each card keeps an item identity
and context across expansion; other feed cards do not constrain the active task.
Other app agents use explicitly shared owner tools through the existing admission,
grant and executor relay, never direct access to another app's files.

News exposes a Research topic action per story. Both the app UI and the News
agent call the same owner service, which uses the existing octos research
toolbox with a bounded source/read/model budget and publishes a
source-attributed research card, and exposes the complete summarized result when
expanded. Research runs on request, not automatically for every fetched headline.
A failed or partial research run must remain visibly failed or partial.

Photos exposes the library/collections the app actually owns, with stable photo
and memory identifiers. Its current bundled sample library must remain labeled
as sample data; this work cannot imply access to the Android camera roll without
an implemented import/permission path. Photo cards open their corresponding
collection and support contextual chat. No image pixels are sent to a provider
unless the implemented feature and consent actually authorize it.

YouTube produces quiet music recommendations for morning, noon, afternoon
relaxation, dinner and sleep, using the device timezone and configurable slots.
Cards contain actual resolved video results and open playback only after a tap.
Do not autoplay on a timer, fabricate video ids, claim guaranteed playback, or
publish a new notification for every retry. Explain unavailable network/content.
Use the existing player and retain its playback restrictions.

The first implementation has five configurable device-local slots, an explicit
Daily music opt-in and agent consent. Its timer runs in the active host process;
Android may suspend that process. It is not an Android background job. The
service searches public YouTube results and publishes a real resolved video;
it does not claim that a model selected every scheduled recommendation. The
app agent can choose, refine and publish through the same owner APIs.

## Agent and memory boundaries

Host services own business state and expose narrow declared tools. Update the
owner declaration, cross-app requests, App Hub admission offer, caller checks and
cold service registration together. Use the existing app peer per account and
host-owned card context; a card cannot manufacture a grant or approval.

After a successful human card-chat turn, summarize useful preferences for the
system agent's private user memory. Summaries must be grounded in actual user
messages, distinguish an expressed preference from an inference, and retain app,
account and context provenance. Do not copy full transcripts, credentials,
article instructions, photo metadata or the assistant's guesses into a durable
user preference. Revoked consent/account changes must stop pending writes.
Provide inspection and deletion, prevent duplicates on retries, and keep memory
private to the user/system agent; another app does not gain global memory access.

The initial extractor is deliberately limited to short, explicit statements
from the latest human message. A model selects exact statements; host validation
rejects paraphrases, sensitive facts and ungrounded inferences. A bounded queue
and model budget prevent chat from spawning unbounded work. This is best-effort
preference capture, not a durable summary of every conversation.

The host stores preferences under `apps/.host/system-memory/`, outside app jails,
and exposes `preferences.list/forget/configure` only to the system session.
Mutation requires a human-triggered system turn. The pinned octos Recall API
has no delete operation; copying preferences there would defeat deletion, so
the system agent reads this authoritative store when personalizing instead.
Forget invalidates pending writes and tombstones the statement; it does not
delete the original conversation or unrelated kernel memories.

Cross-app admission offers are registered before any cold bundle/agent discovery.
Host services resolve stored story/photo/video identities and publish under
their owner app. Research completion uses an exact-publication guard so a
delayed result cannot resurrect a dismissed card or overwrite a newer request.
App Hub prepares system bundles in admitted staging directories before atomic
installation; concurrent discovery must never see a partial extraction.

## Acceptance

Complete a News vertical slice first, then apply the same host pattern to Photos
and YouTube. Test cold cross-app calls, denied callers, stable item routing,
summary expansion, full research text and citations, native chat keyboard/state,
quiet time-slot replacement, tap-to-play and preference provenance/deletion.
Use owned hidden Makepad instances for desktop iteration and the assigned
OnePlus 6 for phone acceptance. Use fictional content for committed evidence.
Codex authors runtime/tool integration; label model-generated content and keep
model/source receipts. Do not describe operator tests as autonomous agent tests.

No current UX score or phone/model pass is claimed by this ADR. Record final
source revisions, independent checks and unverified behavior before handoff.
