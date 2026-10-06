# Markdown editor

[简体中文](README.zh-CN.md)

`octosense-markdown-editor` makes Rinx's reusable article components available
to contained OctoSense apps as `MarkdownEditor`. It has no filesystem, network,
account or publication service. The app keeps drafts and requests a separate
host-reviewed GitHub save.

The workspace pins `article-core` and `article-makepad` to the same Rinx v1.1.0
revision already used by the native Rinx module. This reuses its document model,
lossless Markdown import, edit history, native rich input/IME/clipboard,
cross-block selection, presentation styles and complete Markdown renderer.
Unsupported visual structures remain editable Markdown blocks instead of being
discarded. Source, Write and Preview share one document. A document that cannot
be parsed remains visible in Source; switching to Write is refused until fixed.

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
}
```

After the UI exists, `ui.editor.load(markdown)` returns an empty string on
success, or a validation error without replacing the current document.
`ui.editor.text()` returns current Markdown. `on_change` uses the normal queued
widget callback and observes both source and rich edits. Loading programmatically
does not emit a user edit. Use `load`, not an unchecked generic `set_text`, when
importing a remote file.

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
`OctoScript-App-Design-Flow/examples/connected-apps/github-notes`. Actual OAuth,
host approval, installed-app grants and a GitHub commit require an integrated
OctoSense host; the fixture cannot validate them.
