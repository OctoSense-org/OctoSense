# Contained file transfer
English | [简体中文](README.zh-CN.md)

`files` connects a foreground app to the native document picker. An import copies
one selected document into that app's existing Splash filesystem; an export saves
a snapshot of an existing app file. Apps receive an app-relative path and byte
count, never a host path or Android provider URI. No separate blob store is created.

| Method | Arguments | Result |
| --- | --- | --- |
| `files.status` | `{}` | `import_supported`, `export_supported`, `photo_pick_supported`, `text_share_supported`, `storage_granted`, `max_file_bytes`, `max_import_bytes`, `max_share_text_bytes`, `foreground_required` |
| `files.import` | `{"path":"/documents/report.pdf"}` | `{"cancelled":false,"path":"/documents/report.pdf","bytes":123,"name":"Q3 report.pdf"}` |
| `files.pick_photo` | `{"path":"/photos/new.png"}` | Choose one PNG/JPEG/WebP; returns the import fields plus its signature-detected `mime` |
| `files.share` | `{"text":"Good morning 🌅"}` | Android only: `{"handoff":"chooser_opened","delivery":"unknown"}` after native chooser dispatch |
| `files.export` | `{"path":"/documents/report.pdf","name":"Report.pdf"}` | The same success fields; `name` is an optional basename suggested to the native dialog |

`pick_photo` uses an image-filtered document chooser; it does not grant photo-library
access. Only PNG, JPEG and WebP signatures are accepted, independently of the filename
or provider MIME label. It does not decode, resize, transcode or strip metadata. The
1 MiB limit still applies: many original phone photos exceed it and are explicitly
refused. Large originals and opaque large-blob handles remain a separate work item.

`share` requires `files` but not `storage`, accepts only 1–8192 UTF-8 bytes of text,
and rejects recipient/path/attachment fields. Android opens its existing system share
chooser; the result confirms OS handoff only. Recipient selection, cancellation after
handoff and delivery are unknown. The app must never label this result “sent”. Unicode
(including emoji) is preserved through UTF-16 JNI. Sharing on other platforms reports
unsupported and is absent from discovery. File/image attachments remain unsupported.
The request expires after 20 seconds; a missing reply is an uncertain handoff, not a
reason to automatically open another chooser. Closing the app cannot recall an OS
chooser that has already opened. The native result uses a unique per-request
correlation value and a bounded reply; stale replies are discarded.

`name` is the chosen document's display name, for the app to show: the file
name without folders, control characters or invisible direction marks, at most 128
bytes. The app still decides where the copy is stored (`path`). It is absent when
nothing is left after cleaning, and on Android, whose document loader does not
query a display name yet.

Cancelling a document/image dialog returns `{"cancelled":true}`. Read failures, unsupported
platforms, missing grants, full storage, and busy transfers are errors. Imports
require a new destination: they cannot overwrite an existing app document. The
app can read imported bytes with `fs.read_bytes(path)`, pass that array to an image
widget, or copy it with `fs.write_bytes(other_path, fs.read_bytes(path))`.
`fs.write_bytes` accepts a typed U8 array and applies the same limits as text writes.
A document above 1 MiB is meant for a host service that reads it natively, such
as an engine: `fs.read_bytes` loads all of it into the app's heap, and
`fs.write_bytes` refuses to copy it.

The manifest must request `files` and `storage` for transfer; status only needs
`files`. Import, export, photo selection and text sharing are foreground-only, including through agent tool wrappers.
Both the admitted manifest and the request's live isolate are checked before the
dialog and again on completion. Queued calls also recheck the current app, live surface,
and native window foreground before opening UI; an earlier foreground callback
cannot open a chooser after an app switch or suspension. An already-open chooser
may take focus, so completion checks lifetime and admission without requiring
the app window to regain focus. Native loading checks authorization before IO and
before delivering bytes; export checks it before desktop rename and before replying. Native code obtains the jail by the authenticated
request's isolate key and verifies its host-assigned app tag. Neither the app id,
jail root, quota, native destination, nor URI is taken from request arguments.

An import takes one document of up to 64 MiB (16 MiB on Android) and never more
than the app's granted whole-jail byte quota; it also counts against the 256-entry
cap. `files.status` reports the bound as `max_import_bytes`. The native loader
reads at most that much. A worker then stages the bytes beside the jail, where the
app cannot reach them, and the UI thread links the file in between script turns,
after checking the live jail, its current quota and that the destination is still
new. The bytes never pass through the script heap. Android's document loader holds
a selection in the Java heap and copies it again into native memory, hence its
smaller bound. Photo picks, exports and script writes keep the 1 MiB per-file
limit (`max_file_bytes`). Paths reject traversal, symlinks, drive prefixes,
alternate data streams, and Windows device names. Native selection loading, import
staging and export destination IO run on the existing bounded task pool. A single
transfer reservation limits concurrent dialogs, staged imports and snapshots; a
blocked import or export provider retains that reservation until its worker
returns, even after the app closes or times out.

Requests expire after five minutes. Closing an app invalidates its request, and a
late dialog result cannot import or start an export. A native dialog already on
screen may still need to be dismissed. Once an export's native write has started,
closing the app cannot undo it; Android providers do not offer an atomic rollback.
Replies for closed or expired requests are discarded. Desktop exports use a
temporary sibling and rename; Android uses ContentResolver's selected-document
stream. Contained Android picks do not retain persistent URI grants.

Desktop macOS/Windows and Android have byte adapters. Linux additionally needs an
executable zenity, qarma, matedialog, or kdialog in PATH. Unsupported hosts report
false and omit import/export from discovery. iOS, OpenHarmony, web, and direct
framebuffer Linux are currently unsupported by this service. A supported adapter
does not assert that a document provider is installed or that device UI was tested.

Shell integration: register `files_service::register()` beside platform service
registration in `apps.rs`; call `files_service::handle_event(cx, event)` beside the
platform service event handler in `lib.rs`, outside the App Hub service mutex. Feed `files_service::set_foreground_app` from the
same host-owned focused-client/expanded-Glance identity used for native audio.
The App Hub contract must know `files` and provide `Replier::isolate_key`; Makepad
must include the corresponding storage/dialog adapter. Runtime discovery can
advertise `storage.binary_write@1` for the native `fs.write_bytes` method.

Validation: six native storage tests passed, covering ownership, quotas,
overwrite refusal, traversal, entry limits, symlinks and preserving legacy camera
reads above the transfer limit. Five native dialog tests, four integrated
file-service tests and the Android-target Makepad platform Rust check also passed.
JNI transfer failures now use checked allocations and bounded copies, clear Java
exceptions and release their worker references/attachment on every return path.
Existing unrelated compiler warnings remain. Interactive dialogs and Android
provider/device behavior remain **unverified**. All 16 combined Android Java
templates compiled with 23 existing deprecation warnings.

Photo/share validation is tracked separately from the older transfer checks above.
Source tests cover actual signatures, traversal, explicit text limits, rejected
attachment/recipient fields and handoff wording. A compiler check is not a real
photo-picker or recipient-delivery test; those interactions remain **unverified**
until their dedicated native/device acceptance run.
