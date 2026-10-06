# Notes Rinx writer native soak

English | [简体中文](README.zh-CN.md)

Both runs passed their scripted task and retention checks: **36 cycles in
601.819 seconds**, followed by **12 cycles in 10.272 seconds**. They used a signed,
installed GitHub Notes bundle, the integrated native host, an isolated synthetic
GitHub provider, and Makepad instrument events on macOS. All 8 curated PNGs below
were inspected individually at original resolution. These are desktop windows
at phone width, not physical OnePlus 6 input or Android acceptance.

| Evidence | Long run | Final burst |
| --- | --- | --- |
| Receipt | [long-36-receipt.json](long-36-receipt.json) | [final-12-receipt.json](final-12-receipt.json) |
| Cycles | 36 | 12 |
| Time including startup/reopen | 601.819 s | 10.272 s |
| Idle/timer observation inside cycles | 572.209 s | 0 s |
| Exact review and cancel | 6 | 2 |
| Rich edit and undo | 6 | 2 |
| Cold reopen with exact full draft | 1 | 1 |
| Provider write attempts | 0 | 0 |

The long run **predates** the Notes Splash failed-load recovery guard. The final
burst follows that guard and the refreshed bundle listing. Both use native
executable `a4a11541…c75522`; the recorded Rust editor, host and driver source
hashes are unchanged. The bundle `main.splash` and `manifest.json` hashes differ,
so the receipts remain separate. The final burst repeats the ordinary journey;
it does not inject a failed remote load and is not, by itself, proof of that
fault-handling branch.

Every cycle writes alternating short/long fictional Markdown with accented
Latin, Chinese and Japanese text, scrolls and refocuses the source, enters the
secondary block editor, previews the document, and visits the repository screen
and returns. Every sixth cycle modifies a rich-input block, undoes it, checks the
complete persisted draft, opens the exact host review, scrolls to its end and
cancels. The final draft is checked again after a new native process opens the
installed app. All owned processes exited with code 0; the driver removed its
isolated profile. No real GitHub credentials or remote writes were used.

The numbers below are **instrument round trips**, including native frame waits,
IPC, polling and the listed operation's work. They are not FPS, touch-to-display
latency, or an idle-machine benchmark. Source edit/persist also includes the
app's 350 ms local-save debounce. Entering the block editor is a multi-step
interaction through Preview and Styles.

| Operation | Long run P95 / max | Final burst P95 / max |
| --- | --- | --- |
| Source edit and persisted readback | 485.74 / 711.74 ms | 441.43 / 441.46 ms |
| Source scroll and refocus | 11.31 / 13.20 ms | 7.65 / 7.67 ms |
| Enter block editor | 42.75 / 75.98 ms | 22.48 / 23.85 ms |
| Preview transition | 13.14 / 102.13 ms | 8.28 / 9.34 ms |
| Preview scroll pair | 21.45 / 42.07 ms | 15.88 / 20.73 ms |
| Repository and return | 83.68 / 236.39 ms | 50.77 / 57.27 ms |
| Open exact review | 196.21 / 212.62 ms | 125.65 / 126.39 ms |
| Cancel review | 14.84 / 15.95 ms | 11.92 / 12.16 ms |

Review timings have only 6 and 2 observations respectively. Installed-process
startup to the restored-status label took 409.15 ms and 360.82 ms, one observation
each; full draft equality was checked immediately afterward.

RSS increased over both finite runs. Across cycle samples, the long run moved
from **316.23 to 370.44 MiB** (last idle sample 364.42 MiB); its first/last five-cycle
medians were 326.14/363.64 MiB. The burst moved from **316.94 to 353.95 MiB**, with
first/last five-cycle medians of 321.62/341.27 MiB. RSS includes renderer caches,
editing history and screenshot allocations. These observations establish neither
a leak nor a leak-free steady state; the receipts preserve every numeric sample.

| Original capture | Observed result |
| --- | --- |
| [Long run: preview](long-36/cycle-01-preview-top.png) | Unicode text is readable; compact icon header and paper preview are visible. |
| [Long run: exact review](long-36/cycle-36-exact-review.png) | Repository, branch, path and final-cycle Markdown are visible. |
| [Long run: review bottom](long-36/final-review-scrolled-bottom.png) | Final marker, table/code source and both review actions are reachable. |
| [Long run: reopened source](long-36/final-reopen-retained-source.png) | Final-cycle source and filename return after restart. |
| [Final burst: preview](final-12/cycle-12-preview-top.png) | Final-cycle Unicode preview remains readable. |
| [Final burst: block editor bottom](final-12/long-write-scrolled-bottom.png) | The secondary block editor reaches the final table and code block. |
| [Final burst: review bottom](final-12/final-review-scrolled-bottom.png) | Final content and review actions remain visible. |
| [Final burst: reopened source](final-12/final-reopen-retained-source.png) | Final-cycle source and filename return after restart. |

Raw logs, widget snapshots and input-body dumps are excluded. Sanitized receipts
retain all recorded source hashes, installation metadata, content hashes, cycle
measurements, exit codes and original capture hashes. Published PNG bytes match
the original run receipts. The earlier [Rinx reference evidence](../rinx-writer-20261006/README.md)
and its separate [attribution correction](../rinx-writer-20261006/attribution-correction.json)
remain historical records. No numerical UX score, live-provider approval, physical
keyboard result or Android performance claim is inferred from these runs.
