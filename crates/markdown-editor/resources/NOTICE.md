# Rinx writer components and icon attribution

The writer layout/formatting behavior and table picker are adapted from
[Rinx](https://github.com/hagency-org/Rinx), revision
`4b89097d8791a7190d01de1c576979c93df0013d` (v1.1.0),
`apps/article-editor/native/ui.rs` and
`apps/article-editor/native/table_picker.rs`. Rinx's code is distributed under
Apache-2.0; its inherited Robrix portions retain their MIT notice. The original
upstream notices are preserved here without changes:

| Local file | Exact path at the pinned upstream revision |
| --- | --- |
| [LICENSE-RINX](LICENSE-RINX) | `LICENSE` |
| [NOTICE-RINX](NOTICE-RINX) | `NOTICE` |
| [LICENSE-RINX-MIT](LICENSE-RINX-MIT) | `LICENSE-MIT` |
| [ATTRIBUTIONS-RINX.md](ATTRIBUTIONS-RINX.md) | `licenses/ATTRIBUTIONS.md` |

Every SVG in `icons/` is an unchanged copy of the same filename under upstream
`resources/icons/`. **The code's Apache-2.0 license is not a blanket license for
these assets.** Upstream `NOTICE` retains separate third-party asset terms;
`licenses/ATTRIBUTIONS.md` and `packaging/debian-copyright` describe the icon
collection as using mixed SVG Repo source licenses. Preserve the source metadata
in the SVGs and consult each icon's upstream source for its applicable terms.
This record makes no additional per-icon license claim. The original attribution
file also describes assets outside this copied subset; for example,
`add_wallet.svg` is not included here.

The embedding editor excludes the Matrix account, storage and publication
controller. Repository selection and reviewed save belong to its host.
