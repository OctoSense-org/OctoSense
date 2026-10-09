---
name: sheet-engine
description: Spreadsheet formulas (Excel-compatible, hundreds of functions) and open workbooks: calculate with sheets.eval, read cells with sheets.get. Read before spreadsheet math or an xlsx task.
---

# Sheet engine

gridcraft is a headless spreadsheet engine: Excel-compatible formulas over
hundreds of worksheet functions (financial, date and time, lookup,
statistical, text, engineering, dynamic arrays, LET and LAMBDA), multi-sheet
workbooks, and xlsx in and out.

## Tools

- `sheets.eval {formula, book?, sheet?, at?}`: the value of one formula, nothing stored. Without `book` it is a plain calculation against an empty workbook; with one, it can refer to that workbook's cells.
- `sheets.get {book, range, sheet?}`: the computed values of an open workbook, row-major, for a range like `A1:C9` or one cell like `B2`.

The Sheets app's own agent has the rest: `sheets.new`, `sheets.open`,
`sheets.set`, `sheets.fill`, `sheets.recalc`, `sheets.export` and
`sheets.close`. To build or change a workbook, ask it with `agents.ask`; it
answers with the workbook's `book` number, which `sheets.get` and
`sheets.eval` then take.

## Files

Workbooks are files in the sheet engine's own folder, relative paths inside
it; only `sheets.open` and `sheets.export` touch it. Your own workspace and
the person's files are outside it. `sheets.eval` without a workbook needs no
file at all, so prefer it for arithmetic you would otherwise do in your
head.

## Examples

1. A monthly mortgage payment: `sheets.eval {"formula": "=PMT(6.5%/12, 30*12, -400000)"}` gives about 2528.27.
2. Working days in the last quarter of 2026:
   `sheets.eval {"formula": "=NETWORKDAYS(DATE(2026,10,1), DATE(2026,12,31))"}` gives 66.
3. A dynamic array: `sheets.eval {"formula": "=SUM(SEQUENCE(100))"}` gives 5050.
4. Read back what the Sheets agent built in workbook 3: `sheets.get {"book": 3, "range": "A1:D12"}`.

## Functions

`functions.md` in this skill's folder lists every worksheet function with
its signature and a one-line description, by category (`## Financial`,
`## Statistical`, ...), and the forms the calculator evaluates itself (LET,
LAMBDA, INDIRECT, OFFSET, ...). Grep it for a name before you write a
formula you are unsure of (`grep -i xlookup functions.md`).
