# Markdown editor

English | [简体中文](README.zh-CN.md)

`octosense-markdown-editor` makes Rinx's reusable article components available
to contained OctoSense apps as `MarkdownEditor`. It has no filesystem, network,
account or publication service. The app keeps drafts and requests a separate
host-reviewed GitHub save.

The workspace pins `article-core` and `article-makepad` to the same Rinx v1.1.0
revision already used by the native Rinx module. This reuses its document model,
lossless Markdown import, edit history, native rich input/IME/clipboard,
cross-block selection, presentation styles and complete Markdown renderer.
The primary view follows Rinx's actual article writer: an icon header, Markdown
source, desktop split preview, per-block paper preview, desktop formatting bar,
and a phone bottom formatting bar. At widths below 960 points, source and preview
alternate. Hover or long-press an icon for its purpose. The style inspector (desktop)
or bottom sheet (phone preview) contains presentation themes and secondary Block
editor, Undo and Redo controls. The native table-size picker is also reused.

Source, split preview and the Rinx block editor share the same Markdown. Unsupported
visual structures remain editable Markdown blocks. Unparseable source remains the
authoritative draft; entering the block editor is refused until it is valid. Mode
changes, theme previews and filename captions never rewrite file contents. Source
formatting edits only the selected text or lines; undo retains exact source bytes.
Themes are local presentation state, not metadata injected into a GitHub file.

The SVGs and writer/table-picker adaptation are attributed in
[resources/NOTICE.md](resources/NOTICE.md). The standalone `article-makepad` demo
is not used as the visual reference: that demo has a different control layout.

This is not Rinx's Matrix-specific article publication controller. Its image
picker, binary asset upload, Matrix author/room fields and publication workflow
are not included. Image URL syntax survives editing; remote images are not
fetched by this widget. GitHub Notes currently accepts Rinx's 512 KiB document
limit even though the host GitHub reader can return files up to 1 MiB.

## Host integration

Call `register()` once on the UI thread before creating any app isolates. It
installs only this composite widget into the public Splash prelude. Native draw
and input explicitly enter the widget's owning isolate: Rinx's presentation
helpers lazily evaluate styles and must never read the outer host's heap.

```splash
editor := MarkdownEditor {
    width: Fill height: Fill
    on_change: |markdown| save_local_draft(markdown)
    on_repository: || choose_repository_and_file()
    on_save: || request_host_review()
}
```

The editor owns its complete workspace; do not add a second title, save toolbar
or mode-tab row around it. `ui.editor.set_destination(filename)` sets the read-only
filename caption. `ui.editor.set_status(message)` shows short routine statuses
in the saved label and wraps longer notices below the content. These setters have
no file or provider effects. Back/repository and send/save icons only invoke the
embedding app callbacks; the host still reviews and authorizes a remote write.

After the UI exists, `ui.editor.load(markdown)` returns an empty string on
success, or a validation error without replacing the current document.
`ui.editor.text()` returns current Markdown. `on_change` uses the normal queued
widget callback and observes both source and rich edits. Loading programmatically
does not emit a user edit. Use `load`, not an unchecked generic `set_text`, when
importing a remote file.

## Document page

The crate also holds `DocumentEditor`, the page Writer shows on a Mac, and
`DocumentScroller` and `DocMark` for its read-only preview. `DocumentEditor` reuses
Rinx's rich input and cross-block selection, one input per line, and shows a line's
heading, list number, quote or emphasis as it will print. Its text stays the app's
line format (`# Title`, `- item`, `1. item`, `> quote`, `**bold**`, `*italic*`); only
the lines a person changes are written again, so a document's saved form doesn't
move on its own. `MarkdownEditor` isn't used for this: its model has no first-level
heading, an edit writes the whole document again, and its chrome is its own.

```splash
editor_text := DocumentEditor {
    width: Fill height: Fill
    on_change: |text| draft_changed(text)
    on_format: |style, bold, italic, line, selected| format_changed(style, bold, italic, line, selected)
    on_save: || save_doc()
}
```

`ui.editor_text.text()` and `set_text(s)` read and load the text; `headings()` answers
JSON `[{level, text, line}]`; `bold()`, `italic()`, `set_style(id)`, `go_to_line(n)`,
`focus()`, `set_active(on)`, `cut()`, `copy()` and `paste()` act on the caret or the
selection. `find(words, step)` selects a place the words appear, case aside, and brings it
into view while the keyboard stays where it is: `step` 0 takes the first at or after the
selection, 1 the next and -1 the one before, round the document. It answers JSON
`{index, count}`, `index` counting from 1, and `{0, 0}` when the words aren't there.
`find_done()` gives the keyboard back to the page, on the match. `replace_found(words, with, all)` puts `with` in place
of the selected match and goes on to the next, or in place of every match when `all` is true (one change to undo); the
words take the bold and italic of what they replace, and it answers JSON `{index, count, replaced}`. For the rest of a
Mac's Edit menu: `on_format`'s `selected` says whether there is a selection (a menu's rows read it from the
script's state, not from the editor), `delete_selected()` removes it and leaves the
pasteboard alone, `paste_matching()` is Paste and Match Style (the pasteboard's words as if typed there: Markdown in
them stays characters, and each further line is a row as Return would make it; Option-Shift-Command-V on the page), and `change_case(kind)` is
Transformations, `"upper"`, `"lower"` or `"capitalize"` (a capital to each word, a word running to a space), keeping
the selection and its bold and italic. A right click off the selection selects the word under it, or puts the caret
there, before a menu around the page opens. `paper := Sheet{...}` is the sheet behind the lines and `page_gap` the room above it:
the top edge goes up with the first line, as a page's does.
`DocumentScroller{on_mark: |k| ...}` scrolls a page holding
`DocMark{mark: k}` headings with `scroll_to_mark(k)` and `scroll_to_top()`, brings a
mark just into sight with `reveal_mark(k)` (going up, a mark in the first screenful takes the page to its top, so
what's above it shows too), and says which mark has reached the top. A page ends in `DocMark{end: true}`:
`on_near_end: || ...` asks for the next step of a long page as scrolling comes near it, and over the last screenful the
last marks are reached in turn, though they can't scroll up to the top.

## Verification

```sh
cargo test --locked -p octosense-markdown-editor --lib
cargo check --locked -p octosense-markdown-editor --all-targets
cargo build --locked --release -p octosense-markdown-editor --example editor-host
MAKEPAD_HIDE_WINDOWS=1 target/release/examples/editor-host \
  --source=/path/to/github-notes/bundle/main.splash \
  --app-data=/path/to/temporary-fixture-state --remote
```

`editor-host` is a native UI fixture, not an App Hub admission or provider test.
It has no OAuth account and rejects provider operations. Add `--fixture-provider`
to expose two explicitly fictional repositories and Markdown files for UI tests;
all remote writes still fail. Do not place real account configuration in this
test directory. Close the owned process through its printed `/quit` endpoint.

The companion sample and acceptance record live in
`OctoSense-App-Flow/examples/connected-apps/github-notes`. Actual OAuth,
host approval, installed-app grants and a GitHub commit require an integrated
OctoSense host; the fixture cannot validate them.

Instrument selectors use IDs rather than empty icon labels: `repository_button`,
`save_button`, `source_mode`, `split_mode`, `preview_mode`, and `markdown`.
`palette_button` opens desktop styles; on a narrow view, `style_fab` is available
in Preview. Secondary controls are `rich_mode`/`undo_button`/`redo_button` on
desktop and `mobile_rich_mode`/`mobile_undo_button`/`mobile_redo_button` on phone.
The example accepts `--wide` for the 1200×820 desktop reference; its default is
430×850. These desktop window sizes do not prove Android IME behavior.

The provider-free native regression was run with:

```sh
python3 crates/markdown-editor/tests/native-writer.py --output target/markdown-writer-native-final
```
