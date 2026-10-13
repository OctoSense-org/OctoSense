# PDF Tools' Edit on a page as a web browser prints it — 2026-10-11

English | [简体中文](README.zh-CN.md)

Before Apply, the Edit panel says what an edit would do besides changing
the words (`pdf.edit_check`), and it refuses what the engine won't do in
plain words. A desktop shell built from this branch at `c50104da` showed
it with the real `pdf` engine, on the page `pdftools_fixture --edit-sample`
writes. That page has a title a web browser printed in five pieces, in
unnamed Type 3 fonts, and Chinese text in an embedded subset font.

The shell ran on the Linux build host, not on the Mac, because another
session was running OctoSense on the Mac at the time.

- **Build:** `cargo build --locked -p octosense --no-default-features --features app-hub,craft-engines`.
- **Display:** a private headless weston 14.0.2 (pixman renderer, never
  idle), with Makepad's Wayland backend.
- **Home:** a fresh `OCTOSENSE_HOME` with the sample alone in PDF Tools'
  library, plus `MAKEPAD_REMOTE` and `MAKEPAD_WM_TEST_APP=pdftools`.
- **Run:** one instance, driven over the remote bridge, ended with `/quit`.
  The driver used the Linux launch of [ui.py](../../ui.py) from #495 (its
  `Shell` and `click_page`) and isn't in the repository yet; its steps are
  below.

Frames are `/g` grabs at one pixel per point, cropped to the app's area
(1292 x 662, the shell's default window for PDF Tools). Every step passed
at the runtime's 64 ms script budget, with no script errors in the log.
[receipt.json](receipt.json) holds the commit, the binary's digest and what
each step found.

| Frame | What it shows |
| --- | --- |
| [e01-piece.png](e01-piece.png) | Edit, the title's first piece clicked ("Studio2026"): Paragraph 2, Font "Unnamed font · 27.8 pt", and "The PDF stores this line in 5 pieces: only this one changes." Its own text keeps its font, so Apply reads Apply |
| [e02-warned.png](e02-warned.png) | " Beta" typed: once typing pauses, "Set in Helvetica: this paragraph's font can't show all of this text." and "Longer than its box: it runs into 2 lines of text nearby." Apply reads "Apply anyway", and stays in view at the card's foot while the fields scroll |
| [e03-refused.png](e03-refused.png) | The piece 季 with 夏 typed (a character its font lacks), refused in red: "PDF Tools can't edit Chinese, Japanese or Korean text yet." |
| [e04-apply-refused.png](e04-apply-refused.png) | Apply then only repeats it: "Couldn't change the text: PDF Tools can't edit Chinese, Japanese or Korean text yet." Nothing changed (the status line still says Saved) |
| [de02-warned.png](de02-warned.png), [de03-refused.png](de03-refused.png) | The same two steps after the shell's switch to OctoSense dark |
| [e05-applied-anyway.png](e05-applied-anyway.png) | Apply anyway on "Studio2026 Beta": the engine's own result, in Helvetica over 春 and 季, as the panel said |
| [e06-applied-at-once.png](e06-applied-at-once.png) | An edit the check has nothing to say about, the Helvetica paragraph a sentence longer with nothing under it: Apply makes it at once |

What the run found, and what changed for it:

- **Apply out of view:** in the first run the notes pushed Apply and Cancel
  out of the card at this window size. Since `c50104da`, a card with notes
  takes the panel's height, its fields scroll, and Apply and Cancel stay at
  its foot. The notes are shorter too.
- **The edit box shows only one line:** the box over the page is sized for
  the paragraph's own text when it is clicked, and doesn't grow while you
  type, so a longer draft shows only its last line ("Beta" in e02). This was
  there before this change and is left as a follow-up.
