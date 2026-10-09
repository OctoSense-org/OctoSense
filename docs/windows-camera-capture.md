# Windows local camera still capture

English | [简体中文](windows-camera-capture.zh-CN.md)

The Windows runtime overlay adds `CameraCaptureRequest::Photo` for an already
open Media Foundation camera. A contained app uses the existing
`CameraPreview.capture()` and receives `on_capture` only after a JPEG is saved.
It still needs the host's camera grant, OS camera access, and its storage jail.
The backend does not add permissions, select another app's storage, or open a
camera in response to a background tool call.

For this batch, declare **camera and storage without library**. The shared
widget currently turns a `library.write` grant into a request to export every
capture. Windows gallery export is unsupported and is refused before capture.
An unrestricted native CameraPreview may therefore request unsupported gallery
export by default. This change does not alter shared widget intent semantics.
Video recording, pause, resume and stop also report unsupported explicitly.

A lazy, long-lived worker accepts at most one still request per camera. The
Media Foundation callback copies only the next requested frame into a bounded
channel. JPEG decoding/encoding and file writes occur on the worker. Supported
inputs are validated MJPEG, even-sized NV12 and YUY2; YUV conversion follows the
existing reader's BT.709 interpretation. Other formats fail visibly.

Limits are 4096 pixels per dimension, 8 megapixels, 32 MiB for source/output and
a ten-second deadline. Closing or changing the camera cancels an uncommitted
request. Output uses `create_new`, so an existing file is never overwritten;
an unsuccessful write removes only the capture it created. Completion is posted
only after the write and final cancellation check. Cancellation after that
commit does not delete a completed photo.

Read ownership distinguishes idle, pending and processing states. The UI and
callback cannot both schedule the next read when the same camera is reopened.
A format change during an outstanding frame is refused; retry after the camera
is idle. Hardware acquisition, privacy indicators and disconnect/reconnect
behavior still require a real Windows camera run.

The existing shared widget checks whether the app has storage room before
requesting a capture. It does not pass the remaining quota to the native
backend; the limits above are not a reservation of an app's remaining quota.
This is a shared widget boundary, not a claim of complete capture quota enforcement.

## Validation

The portable actual-source harness passed **9/9 tests**: decoded NV12/YUY2 JPEG
pixels, strict MJPEG validation, no-clobber, busy refusal, cancellation/reuse,
close, timeout, and malformed/oversized input. It substitutes platform IDs and
action delivery, so it is not a Media Foundation test. Run
`tools/windows-camera-still/check.py` with `--output` naming a new directory;
the driver compiles the existing vendored codecs with optimized `rustc`, stores
logs and hashes, and retains failures. See its `validation.json`.

`cargo check --locked --offline --target x86_64-pc-windows-msvc -p makepad-platform -j1`
passed on the prepared runtime. This checks Windows source types, including the
Media Foundation integration, but does not link or execute a Windows program.
Existing workspace duplicate-target warnings, vendored Windows warnings and an
unused window DPI helper warning remain.

Actual camera execution is **unverified**. No Windows hardware or camera
permission was exercised on the Mac. A compilation pass alone must not be
described as a working Windows camera demo.
