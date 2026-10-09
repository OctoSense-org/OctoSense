# Contained file transfer
English | [简体中文](README.zh-CN.md)

`files` connects a foreground app to the native document picker. An import copies
one selected document into that app's existing Splash filesystem; an export saves
a snapshot of an existing app file. Apps receive an app-relative path and byte
count, never a host path or Android provider URI. No separate blob store is created.

| Method | Arguments | Result |
| --- | --- | --- |
| `files.status` | `{}` | `import_supported`, `export_supported`, `storage_granted`, `max_file_bytes`, `foreground_required` |
| `files.import` | `{"path":"/documents/report.pdf"}` | `{"cancelled":false,"path":"/documents/report.pdf","bytes":123}` |
| `files.export` | `{"path":"/documents/report.pdf","name":"Report.pdf"}` | The same success fields; `name` is an optional basename suggested to the native dialog |

Cancelling a dialog returns `{"cancelled":true}`. Read failures, unsupported
platforms, missing grants, full storage, and busy transfers are errors. Imports
require a new destination: they cannot overwrite an existing app document. The
app can read imported bytes with `fs.read_bytes(path)`, pass that array to an image
widget, or copy it with `fs.write_bytes(other_path, fs.read_bytes(path))`.
`fs.write_bytes` accepts a typed U8 array and applies the same limits as text writes.

The manifest must request `files` and `storage` for transfer; status only needs
`files`. Import/export are foreground-only, including through agent tool wrappers.
Both the admitted manifest and the request's live isolate are checked before the
dialog and again on completion. Native loading checks authorization before IO and
before delivering bytes; export checks it before desktop rename and before replying. Native code obtains the jail by the authenticated
request's isolate key and verifies its host-assigned app tag. Neither the app id,
jail root, quota, native destination, nor URI is taken from request arguments.

Imports use the existing 1 MiB per-file limit, granted whole-jail byte quota, and
256-entry cap. Paths reject traversal, symlinks, drive prefixes, alternate data
streams, and Windows device names. Import commit runs between script turns,
sharing the existing filesystem quota writer. Native selection loading and export
destination IO run on the existing bounded task pool. A single transfer reservation
limits concurrent dialogs and export snapshots; a blocked export provider retains
that reservation until its worker returns.

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
platform service event handler in `lib.rs`, outside the App Hub service mutex.
The App Hub contract must know `files` and provide `Replier::isolate_key`; Makepad
must include the corresponding storage/dialog adapter. Runtime discovery can
advertise `storage.binary_write@1` for the native `fs.write_bytes` method.

Validation: five native storage unit tests passed, covering ownership, quotas,
overwrite refusal, traversal, entry limits, and symlinks. Five native dialog tests and the Android-target Makepad platform Rust check also
passed. Existing unrelated compiler warnings remain. Host-service
tests require the aggregate shell integration. Interactive dialogs and Android
provider/device behavior remain **unverified**; the local JDK was
repaired; combined Android Java template compilation passed with 23 existing
deprecation warnings.
