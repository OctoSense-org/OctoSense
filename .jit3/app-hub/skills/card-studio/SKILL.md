---
name: card-studio
description: Inspect a generated L0 card before publishing it to the Glance screen — render at glance, phone and desktop sizes, measured checks, vision critique payload. Triggers: card render, check my card, critique card, glance card, before glance.publish, card-studio.
version: 0.1.0
author: OctoSense
always: false
---

# card-studio

Under OctoSense ADR 0002 section 7, an app agent renders and critiques its card before
it publishes it. This skill renders and measures the card, and prepares the
vision critique as a request for you to send.

## Render, check and revise

1. Write the card (L0) and its data: the host-resolved sources, such as the
   `sys.digest` payload. Try two styles when the run's budget allows.
2. Call `card_render {card, data}`. It renders each size in a hidden
   `card-host --remote` and returns the report. Read `summary.pass` and every
   finding with `severity: "error"`:
   - `text_truncated`, `text_hidden`, `text_clipped` or `overflow` on text:
     the text is cut. Shorten it, drop a row or let it wrap.
   - `does_not_fit` at the glance size: the glance tile is 350x160pt and
     cannot scroll. Show fewer items.
   - `empty_card`: the card drew nothing; fix the card or its data.
   - `source_failed`, `lint`, `realize`, `realize_truncated`, `lower_failed`,
     `log_error`: fix the card or its data.

   Revise for these warnings when the budget allows: `state_visible`,
   `missing_value`, `overlap`, `source_pending`, `hidden`,
   `outside_viewport` and `does_not_fit` at the phone or desktop size.
3. Revise and render again until no error remains or the budget is spent.
4. Call `card_critique_payload {report, rubric, inline: true}`. Send a vision
   model the payload's `prompt`. Then, for each entry of `sizes`, send a text
   line with the size's name, `widgets` and `findings`, followed by that
   size's image. Keep the model's answer with the run as the critique record.
   If the verdict is `revise` with concrete `fix` entries, revise again while
   the budget allows.
5. Publish, with `glance.publish`, only a card whose last render passed, and
   write it at L0. The shell admits it only when the app holds the `glance`
   capability and `check_ui_l0` finds the card valid. OctoSense desktop
   0.1.0-beta.2 also admits a card valid at L1; OctoSense desktop 0.1.0-rc.1
   refuses an agent's L1 card and any `script` card.

## Install

Build `card-host` and `card-studio`, then copy the skill into the octos
profile:

```sh
cargo build --release -p octosense-card-host -p octosense-card-studio
dest=<octos data dir>/profiles/<profile>/data/skills/card-studio
mkdir -p "$dest" && cp skills/card-studio/{SKILL.md,manifest.json,rubric.md} "$dest/"
cp target/release/card-studio "$dest/main"   # card-host may stay where it was built
```

If the build fails with `no variant … TextInputStateQuery`, see [`card-host` fails to build](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/DEVELOPMENT.md#card-host-fails-to-build).

The skill runs `main` (the `card-studio` binary) in the session workspace,
with a filtered environment. It reads its defaults from `card-studio.json`
next to `main`, so write that file into `$dest`:

```json
{"card_host": "<absolute path>/card-host", "kit": "<absolute path>/octoscript-makepad/components/l0", "out_dir": "<absolute path>/card-studio-runs"}
```

Rendering needs a graphical session (the window is hidden, not headless): run
it on a desktop, or on a server with a display. The measured checks alone run
anywhere (`card-studio check`). Not yet: rendering on the phone, where OctoSense
ADR 0006 plans an in-process renderer.
