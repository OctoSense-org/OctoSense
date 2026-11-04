# Connected app preview admission

English | [简体中文](README.zh-CN.md)

Version 0.1.1 has its own record: [connected-apps-0.1.1](../../docs/admissions/connected-apps-0.1.1/README.md).

Catalog sequence **7** lists all three independently installable macOS
developer-preview apps from ymote. [`admission.json`](admission.json) records
that sequences 5, 6 and 7 admitted them, one app each. The App Hub maintainer
authorized publication with an instruction to finish runtime delivery and
catalog acceptance. Separate reviewer agents examined the publisher packages,
capabilities, privacy disclosures and the `hub publish` compatibility fix
described below. This record covers the agents' review and the maintainer's
publication authorization; it does not claim that an independent human
completed live provider acceptance.

| App | Publisher commit | Submission |
| --- | --- | --- |
| GitHub Notes | `5f0c4c6b13bddae87a4945b2b76ca2a416793f14` | [#121](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/121) |
| Inbox Assistant | `28dd24a39a7e9668b877dd84d9577d10b3f87363` | [#122](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/122) |
| Google Calendar | `c0783291a528689008d7213f1b1163c9c15ac874` | [#123](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/123) |

[`admission.json`](admission.json) records the bundle digests, source tags,
public release verification and catalog identity. Each bundle, byte for byte
as its publisher signed it, passed `hub check`. `hub publish` then reran admission, copied the files
unchanged and signed the catalog with the existing certified working key. The
reviewers compared every artifact file by SHA-256 with its publisher source;
`admission.json` records each bundle's digest and file count, not those
per-file hashes. The index entries are exact copies of the admitted entries,
and the `.pack.json` files beside the bundle directories in `artifacts/`
contain the same bundles. This
directory holds no signing keys, provider credentials, private profiles or
raw runtime logs. The public trust anchor is unchanged.

Before admission, reviewer agents checked the publisher's scan answers on
these points: visible function; platform and category; least necessary grants;
misleading UI; agent instructions hidden in app content; abusive or private
content; and tool risk and routing. They approved the apps within the
disclosed macOS developer preview scope. Every tool that exposes private
provider data is non-shareable by default. GitHub Notes exports no write tool,
Inbox Assistant no send tool and Google Calendar no booking tool. The host
keeps account management and the approval of external actions. The
publishers' privacy policies and the Store summary explain the provider and
model traffic, with one gap: GitHub Notes ships tools without an `agent`
block, so the Store summary in OctoSense desktop 0.1.0-beta.2 says "Runs no
assistant." although the shell offers "Ask GitHub Notes". The publisher's
corrected privacy policy discloses that agent, and a store built from App Hub
`main` discloses it in the summary.

The compatibility review found that older strict catalog readers rejected
`host_method` in display metadata. The fixed `hub publish` omits only that
field from the catalog's tool summaries. The original signed `tools.json`
mappings stay byte-identical, and hosts load them for execution. Regression
tests verify old field-shape parsing, canonical signature bytes, unchanged
installed mappings, and the refusal of a tampered mapping or a forbidden
route. These tests check the legacy wire shape; they do not run an old desktop
binary.

These records establish admission and integrity. They do not establish a live
GitHub or Google connection, provider writes, physical approval, Android Google
authorization, Windows or Linux UX, or the absence of long-run memory growth.
The existing native, synthetic-provider and model evidence keeps its original
source hashes in the publisher repositories.

## Official Store acceptance

The [public HTTP check](official-http.json) fetched 21 URLs, each with status
200: `catalog.json`, the three index entries, the three `.pack.json` files and
14 of the 28 bundle files (each app's manifest, icon and screenshots). It
verified the default trust anchor and recorded bundle digests that match
`admission.json`.

The [macOS Apple silicon preview](https://github.com/OctoSense-org/OctoSense/releases/tag/desktop-v0.1.0-beta.2)
is built from source `84e3438b7ee25b43ba945bfe063db594a71e61ba`. Its packaged
application, run as shipped, searched, installed and opened all three apps
from the official catalog in a fresh isolated profile, with no catalog or
anchor override.
[Nine functional checks and six visual reviews of original pixels](native-store/receipt.json)
passed, including exact local draft restoration after a full process restart.
One reviewer agent, recorded as `calendar_sample`, performed all six visual
reviews. The receipt binds the runtime commit; the hashes of the executable,
the kernel and each app bundle; and the SHA-256 of the release's
`macos-aarch64-release-receipt.json`, recorded as `package_receipt_sha256`.
The receipt keeps the release candidate's record unchanged; publication added
no new test result.

| App | Restored state and original capture |
| --- | --- |
| GitHub Notes | [Unicode Markdown and rendered preview](native-store/notes-restored-preview.png) |
| Inbox Assistant | [Revision-2 fictional reply, kept across the Reply and Chat tabs](native-store/inbox-restored-reply.png) |
| Google Calendar | [Local event draft](native-store/calendar-restored-draft.png) and [timezone](native-store/calendar-restored-timezone.png) |

The [Store](native-store/official-store.png) and
[installed Library](native-store/installed-library.png) captures are also
original, unedited pixels.

The run recorded **14 read-only frame-capture errors** and no input errors or
replays. It stopped and resumed once, because the test driver used coordinates
from before a scroll had settled; the corrected test driver observed a new
frame before it computed the next target. The run proves the stated local workflows. It is
not an uninterrupted instrument run, a latency measurement or a clean UX soak
test. It used no OAuth connection, live provider data, model call or remote
write, and no physical approval of a send or a save.
