# Quick Deck's inspector states — 2026-10-11

English | [简体中文](README.zh-CN.md)

The deck service now reports what the inspector shows (#507, SERVICE.md
"Inspector states"), and Quick Deck shows it: a box's anchor and autofit, a
chart's legend, data labels, gridlines and palette, a table's options, every
colour set's colours, the deck's colour set, font pair and footer. After an
Undo or a Redo the controls show the deck's own again, without reopening.
[ui.py](../../ui.py) ran it on this branch, and
[receipt.json](receipt.json) holds the digests, the commands, what each run
checked, the rounds that failed before these and what was not verified.

**App Hub's `card-host`** (macOS 26.6.2, Apple silicon, at the runtime's
64 ms budget), on the dev fixture, which answers as the service does:

- [fixture/](fixture): the designs beside the app for the screens that
  change, light, then dark (`d`-prefixed; only 10 has a dark design): 02
  (the box's shrink switch on and its top alignment chosen, as the design
  draws them), 03 (the chart's title, data labels and gridlines on, its
  legend off, its colourful palette) and 05 (Harbor's colour set and Classic's
  font pair marked; every colour set with its colours).
- [before-after/](before-after): the app on main (left) beside this branch
  (right), on the same fixture, for 02, 03, 05, the table's options (11) and
  the footer (12), light and dark. On main, the box's switches show off, the
  chart's legend shows on although the chart has none, the table shows three
  of its six options at their defaults, the footer's slide numbers show off
  and its text is empty, and the colour set and font pair are guessed from
  the theme.
- The runs also checked each switch and mark against the stand-in, and a
  table option and the footer's date against it after a change and its
  Undo (their frames are not kept: they look like 11 and 12).

**A hidden desktop shell with the real `deck` engine** (Ubuntu 26.04 LTS,
x86_64, 72 CPUs, no GPU; headless weston, as before): the shell was built
from this branch's tree on main eb2fd5d1, which has the service of #503 and
#507. Each frame is the app's window, 1292 x 662 points, at one pixel per
point. In the journey's light pass, each state was read, changed, read back
from the engine (the shape chosen again, or the deck's design asked again)
and undone, and read back again ([linux/states.json](linux/states.json)
records what the engine first reported):

- [linux/10-box.png](linux/10-box.png), [10b-box-undone.png](linux/10b-box-undone.png):
  the Agenda box, which the engine draws at the top and shrinking its text
  to fit, set to the bottom with shrink off (the engine moves its text
  down), then both undone.
- [11-chart.png](linux/11-chart.png), [11b-chart-undone.png](linux/11b-chart-undone.png):
  a new chart's data labels on (the engine draws the values over the bars),
  then undone.
- [12-table.png](linux/12-table.png), [12b-table-undone.png](linux/12b-table-undone.png):
  a new table's total row on, then undone.
- [13-colours.png](linux/13-colours.png), [13b-colours-undone.png](linux/13b-colours-undone.png):
  the Ocean colour set, then undone: Ember's own set is marked again.
- [14-footer.png](linux/14-footer.png), [14b-footer-undone.png](linux/14b-footer-undone.png):
  slide numbers on (the slide shows its number), then undone.

The rest of the journey (dark, restart) passed as in
[v2-20261011](../v2-20261011/README.md).

Every frame was inspected.
