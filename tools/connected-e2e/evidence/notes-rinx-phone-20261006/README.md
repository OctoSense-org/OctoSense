# Rinx writer on OnePlus 6

English | [简体中文](README.zh-CN.md)

The separate **OctoSenseNotesTest** package was updated with the Rinx writer
layout, using a signed private App Hub catalog. The installed normal Home and
its account/profile were not replaced. No provider credentials were copied.

Original ADB screenshots show the [editor](01-updated-source.png),
[native keyboard](02-native-keyboard.png), [temporary edit](03-keyboard-edited.png),
[keyboard dismissed](04-keyboard-dismissed.png), [cold restart](05-cold-restart.png)
and [preview](06-preview.png). All show the fictional test note and were opened
individually for visual review. The formatting bar remains above the IME; the
floating navigation controls are absent during typing and return afterward.

The test pressed the actual soft Enter key, typed a temporary marker, verified
the exact newline/text in the app's saved draft, then removed the marker through
native Backspace. A new process restored the exact original draft after restart.
The original draft also survived the APK/catalog update. No GitHub write ran.

[receipt.json](receipt.json) binds the APK, bundle, source files, checks and
original PNG hashes. [shell-validation.json](shell-validation.json) records
956 passing shell tests and both packaging source graphs. This validates local
Android editing and IME behavior; live GitHub authorization/read/commit, physical
input, background lifecycle and a whole-product UX score remain unverified.

Reproduction uses the [separate Android lab procedure](../../android-notes.md)
with the current source and a new signed fixture catalog. On the phone: open the
Notes sample, tap its source area, type with the Android keyboard, dismiss with
Back, switch with the pencil/eye icons, and reopen after stopping the test app.
Keep existing draft storage while replacing the catalog; never clear normal Home.

The [final update](final-update.json), version2026100620, adds the rejected-load
guard and refreshed listing. Its [source](07-final-update-source.png) and
[keyboard](08-final-update-keyboard.png) were checked again; the original draft
is unchanged. Replace the managed signed bundle directory during catalog update:
a plain overlay can retain removed files and correctly fail digest verification.
The initial overlay failure remains recorded separately from the successful retry.
