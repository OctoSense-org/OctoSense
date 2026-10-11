# Native Rinx writer reference

English | [简体中文](README.zh-CN.md)

`rinx-desktop.png` and `rinx-phone.png` are original native GPU captures of Rinx's
actual `ArticlePanel` writer. A diagnostic entry loads fictional Markdown and
opens its existing `Page::Write`; the original UI definitions are unchanged.
No account, Matrix session, model, repository or publication was used.

`reference-receipt.json` binds the exact Rinx revision, original UI source,
diagnostic patch, executable and captures. `notes_writer_reference.rs` is the
small native host; `diagnostic-entry.patch` is its only library change. They are
reference-only artifacts, not application code or an authentication bypass.
The isolated worktree/profile and test windows were separate from the developer's
Rinx profile. All owned native processes stopped after capture.

To reproduce the reference, create a disposable worktree at the revision in the
receipt, apply the diagnostic patch, copy the host into `examples/`, and build
`cargo build --locked --release --example notes_writer_reference`. Start the
binary with a fresh `RINX_DATA_DIR` and `MAKEPAD_HIDE_WINDOWS=1`; `--remote` exposes
the instrument endpoint. Default size is 1200×820; `--phone` selects 430×850.
Use `/g` for the native capture and `/quit` to stop. This equivalent workflow was
executed with local temporary paths. Do not run it against an existing profile.

`notes-*.png` show the reusable Notes writer on the same window sizes.
`editor-receipt.json` binds its code, assets and executable and records the native
Unicode, selection formatting, undo/redo, view retention, rich-input and table
picker checks. The fixture source is `reference.splash`. The native test command
is recorded in the receipt; it exercises the actual widget with a provider-free
fixture. Installed App Hub admission, OAuth, remote GitHub review and Android
keyboard behavior are separate checks, not claims made by this reference run.

The adapted writer preserves GitHub file bytes: its title position displays a
read-only filename; the publish-position icon calls the app's host-review
callback. Styles are local presentation state. The secondary style panel contains
named theme tiles and Block editor/history controls. The Rinx library pin remains
v1.1.0; the actual visual reference is the newer Rinx revision recorded separately.
No pixel-identical or whole-product UX score is asserted.

The resource notices were corrected after this run to preserve upstream mixed
icon terms, with no Rust, fixture or SVG changes. The original receipt retains
its historical notice hash; [attribution-correction.json](attribution-correction.json)
records the documentation-only correction.
