# Notes native UX soak — 2026-10-06

English | [简体中文](README.zh-CN.md)

Both recorded journeys passed their functional assertions: 156 cycles in total,
26 native rich-edit/Undo and exact host-review/cancel rounds, and two installed
restarts. Every cycle checked the authoritative persisted Unicode Markdown.
There were no crashes, native hard-error logs, lost drafts or provider write
attempts. All four owned processes exited normally; temporary profile absence
was checked. No application or runtime changes were required for these runs.

| Run | Cycles | Elapsed | RSS first → final / maximum | Last-five median |
| --- | ---: | ---: | ---: | ---: |
| [Timed soak](soak36/receipt.json) | 36 | 601.465 s | 323.2 → 362.1 MiB | 360.7 MiB |
| [Follow-up burst](burst120/receipt.json) | 120 | 80.966 s | 317.4 → 379.3 MiB | 371.4 MiB |

The timed soak includes idle windows with repeated exact-draft checks; the burst
has no scheduled idle. Both use the same executable and sources, recorded in
[summary.json](summary.json). GitHub transport/vault are synthetic, inside a
fresh marked, signed installation. There was no live OAuth or repository write.

Observed operation timing from the 36-cycle soak:

| Native instrument operation | p50 | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Batch source edit through durable-save readback | 426.34 ms | 434.40 ms | 445.96 ms |
| Switch to Write | 2.55 ms | 3.56 ms | 4.54 ms |
| Switch to Preview | 3.98 ms | 7.79 ms | 13.26 ms |
| Repository, scroll and return | 20.61 ms | 29.86 ms | 49.20 ms |
| Open exact host review | 109.56 ms | 122.44 ms | 124.49 ms |
| Cancel review and verify retained source | 5.16 ms | 9.59 ms | 9.82 ms |

These are HTTP instrument round trips with native frame submission waits,
controller polling and, for source edits, the save debounce. They are not FPS,
per-keystroke latency or measured input-to-display latency. The Mac concurrently
ran Inbox/Calendar soaks and an Android cross-compile with four build jobs.

Memory was not flat. During the burst, median RSS for cycles 56–60 was 359.2 MiB,
86–90 was 360.9 MiB, and 111–119 was 371.4 MiB. The final screenshots add another
allocation step. The longer run exceeds Rinx's 100-entry document Undo history,
but retained history, allocator/renderer caches and sheet/capture allocations
were not profiled separately. These measurements establish neither a leak nor
leak freedom. Raw cycle measurements remain in each run's `cycles.csv` and
receipt; the growth is retained as a follow-up profiling concern.

Original app-owned 860 × 1700 PNGs were visually checked: all ten timed-soak
images and six distinct burst images were opened individually; the other four
burst images match inspected originals byte-for-byte. See the separate
[soak review](soak36/manual-review.json) and [burst review](burst120/manual-review.json).
Long [Write](soak36/long-write-scrolled-bottom.png) and
[Preview](soak36/long-preview-scrolled-bottom.png) views reach the final table/code
block. Review can reach its [last paragraph and Cancel](burst120/final-review-scrolled-bottom.png),
and [restart retains the note and destination](burst120/final-reopen-retained-source.png).

The first pilot stopped before editing because the driver treated the text
instrument help route as JSON. That failure and its capture remain under the
ignored local output directory; the corrected driver passed a six-cycle pilot
before these runs. This evidence covers hidden native macOS UI with instrument
input. Phone keyboard/lifecycle, live GitHub operations, physical human approval
and broader UX scoring are separate checks.
