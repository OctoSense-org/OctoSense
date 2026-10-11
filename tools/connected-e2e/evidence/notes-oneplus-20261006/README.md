# OnePlus 6 local Notes check — 2026-10-06

English | [简体中文](README.zh-CN.md)

The separate **OctoSenseNotesTest** APK passed local Markdown editing, actual
soft-keyboard input, hardware Enter, preview, exact cold-restart recovery and
edge Back to its own test Home. The regular Home was not replaced. The latest
APK and unchanged signed Notes bundle are identified in [receipt.json](receipt.json).
No GitHub connection, provider token or personal profile was copied.

The first APK lost the final composing word on hardware Enter, even after the
input had settled: `# OnePlus Notes test` became `# OnePlus Notes ` plus a
newline. [The original failure](01-before-hardware-enter.png) is retained.
The runtime patch finishes composition before inserting the newline; ordinary
IME commit replacement remains unchanged. The rebuilt APK passes the exact
same hardware sequence and the actual soft-keyboard path. The
[fixed keyboard](02-fixed-hardware-enter.png), [new-process recovery](03-fixed-cold-reopen.png)
and [preview](04-fixed-preview.png) originals were inspected individually.
Java/Rust regressions and patch-stack checks are recorded
[separately](../android-enter-validation.json).

This was an ADB device check, not the Mac Makepad instrument soak. Android's
pinned remote module is a no-op. [Reproduction](../../android-notes.md) describes
the signed private catalog, isolated package, contracts export and packaging.
The initial missing-contracts Java failure and incremental-install failure were
resolved by exporting the contracts and using streamed installation; raw build
logs and APKs remain private local artifacts.

Remaining phone polish is visible in the captures: the shell's floating menu
can cover the right edge of the editor with the keyboard open, and a Repository
explanatory line clips at phone width. No numeric UX score or full phone soak
pass is claimed. Live GitHub sign-in/read/write, physical approval and Android
background lifecycle remain unverified. The test app is left open for the
requested hands-on trial, with only a fictional local note.
