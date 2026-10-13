# PDF Tools v2: image prompts

App Flow step 2 (exact text to submit) for step 3 (image generation, a
**HUMAN** checkpoint for a paid generator). Built from `../BRIEF.md`.

Adaptation from App Flow's atlas route: these are desktop screens, so each
screen is its own image at the generator's largest landscape size, so that its
text stays sharp enough to measure. App Flow's atlas puts 8–12 phone screens in
one image instead. Consistency comes from the identical shared block in every
prompt; screen 1 is generated first and approved as the style reference
before the rest.

Each submitted prompt is the shared block followed by one screen block. The
exact text sent for screen N is saved as `NN-<name>.prompt.txt` beside its
untouched output `NN-<name>.png`. The generator, model and requested size go
in `generation.json`.

## Shared block (identical in every prompt)

```text
Create ONE original image: a single, complete desktop application screen for
"PDF Tools", a document app inside the OctoSense desktop.

Requested image size: 1536 × 1024 pixels, landscape. Requested quality: high.
Show only the application's own window content, edge to edge, flat and
straight on. No operating-system menu bar, no dock, no window title bar or
traffic lights, no device frame, no perspective, no drop shadow around the
screen, no captions or annotations outside the interface.

Visual language: an original, precise desktop document tool. The document is
the brightest thing on screen; the chrome is quiet, thin and well aligned.
Palette: a cool light grey desk behind the pages, white pages with a soft
shadow, graphite text, mid-grey secondary text, thin 1-pixel dividers in pale
grey, and ONE accent colour, a confident medium blue, used only for the
current mode, the selected item, focus and primary buttons. Yellow marks
highlighted text and search matches. Green means saved or done, red means an
error, both used sparingly. Corner radius about 8 px on panels, buttons and
fields; pages have square corners. Icons are simple outline glyphs with one
consistent stroke weight. Typography: a neutral sans-serif for the interface,
small and crisp (about 13–14 px), medium weight for panel titles; document
pages use a classic serif for body text and a sans-serif for their headings.
Do not imitate Adobe Acrobat's layout, icons, colours or branding, or any other
product's.

Window layout, the same in every screen:
- Top row: document tabs. Each tab shows a small PDF glyph, the file name and
  a close cross; the active tab is white, the others grey; a "+ Open" button
  follows the last tab.
- Second row, the mode bar: the modes "Read", "Comment", "Fill & Sign",
  "Pages", "Combine", "Edit" as text tabs, the current one in the accent
  colour with a short underline; on the right a search field with a magnifier
  and the placeholder "Search in document", then icon buttons for Undo and Redo,
  and a "Save" button.
- Left: a narrow icon rail with four icons and tooltips' labels shown under
  them in tiny text: "Pages", "Outline", "Comments", "Search". The selected
  icon has a tinted square behind it. Next to the rail, an open panel about
  240 px wide when the screen calls for it.
- Centre: the canvas, a light grey desk with the document's pages stacked
  vertically, centred, one page fully visible and the next starting below.
- Bottom centre of the canvas: a floating rounded pill with "‹", "3 / 24",
  "›", a divider, "−", "100%", "+", and two small icons for "Fit width" and
  "Fit page".
- Right: a panel about 300 px wide, only in screens that call for it.
- Bottom edge: a thin status line with "Saved" (with a small green dot) on the
  left and "14.6 of 64 MB used" on the right.

Use this exact fixture wherever it appears:
- Documents in the library: "Q3 2026 Board Report.pdf" (24 pages, 2.8 MB,
  opened today), "Riverside Lease 2026.pdf" (12 pages, 1.1 MB, yesterday),
  "Field Guide to Garden Birds.pdf" (7 pages, 16 KB, 3 Oct), "Invoice
  INV-2041.pdf" (1 page, 84 KB, 1 Oct), "Site Survey Photos.pdf" (18 pages,
  9.6 MB, 28 Sep).
- The board report belongs to the fictional "Northwind Cooperative". Its
  sections: Summary, Revenue, Operations, Outlook, Appendix. Page 3 is
  "Revenue" with a short paragraph that includes the sentence "Revenue grew
  12% year over year, led by the Northwest region." and a simple bar chart
  "Revenue by region, Q3 2026" with bars for Northwest, Coast, Valley, Metro.
- People: Maya Chen (the user), Jun Park and Ana Ruiz (reviewers).

Render every piece of interface text and every control sharply enough to be
read and measured later. Labels, buttons, fields and panels will be rebuilt as
native components from this image; page contents are sample document art.
```

## Screen blocks

### 01 Home (`01-home`)

```text
This screen: the Home tab is active (the first tab shows a house glyph and
the word "Home"; a second tab "Q3 2026 Board Report.pdf" is open behind it).
No mode bar, no rail, no canvas pill on this screen. Instead, a centred
content area: the title "PDF Tools", then a wide primary button "Open a PDF
from this device" with the caption "Up to 64 MB" beside it. Below, the heading
"Recent" and a grid of five document cards in two rows, one per fixture
document, each with its first page as a thumbnail, the file name, "N pages ·
size" and when it was opened. The status line shows only "14.6 of 64 MB
used" with a thin storage bar.
```

### 02 Reading (`02-reading`)

```text
This screen: mode "Read". Tab "Q3 2026 Board Report.pdf" active. The rail's
"Pages" icon is selected and its panel shows the title "Pages" and a vertical
column of page thumbnails labelled 1 to 6, thumbnail 3 outlined in the accent
colour. The canvas shows page 3, "Revenue", at 100% with its paragraph and the
bar chart, and the top of page 4 below it. The canvas pill reads "3 / 24" and
"100%". No right panel.
```

### 03 Find (`03-find`)

```text
This screen: mode "Read", with "revenue" typed in the search field. The rail's
"Search" icon is selected; its panel shows "12 matches on 7 pages" and a list
of results grouped by page ("Page 1", "Page 3", "Page 4", …), each with a
one-line snippet in which "revenue" is bold. The second result is selected.
On the canvas, page 3 shows every match highlighted in yellow and the selected
match in a stronger orange-yellow with an outline. A small bar above the canvas
reads "2 of 12" with up and down arrows and "Done".
```

### 04 Comment (`04-comment`)

```text
This screen: mode "Comment". Under the mode bar, a slim tool row: "Highlight"
(selected), "Underline", "Strike out", "Note", "Text box", and a row of four
colour dots. On page 3 the sentence "Revenue grew 12% year over year, led by
the Northwest region." is highlighted in yellow with a small speech-bubble
marker in the margin. The right panel is open with the title "Comments" and
"3": a thread on the highlight. Jun Park (initials avatar), "Today 10:12":
"Can we add the regional breakdown table here?" A reply from Maya Chen,
"Today 10:20": "Added on page 4." A status chip "Accepted". At the bottom of
the thread, a reply field "Reply…" and a "Post" button. Below the thread, a
second, collapsed comment by Ana Ruiz on page 7.
```

### 05 Pages (`05-pages`)

```text
This screen: mode "Pages". No left panel (the rail's "Pages" icon is still
selected). The canvas becomes a grid of large page thumbnails, five per row,
pages 1 to 15 visible, each with its number below. Pages 5 and 6 are selected
with an accent outline and a check badge. A toolbar row under the mode bar:
"2 selected", then "Rotate left", "Rotate right", "Delete", "Extract",
"Insert from file", and on the right "Drag pages to reorder". A thin
insertion marker shows between pages 9 and 10 where a page is being dragged.
```

### 06 Combine (`06-combine`)

```text
This screen: mode "Combine". The canvas shows a centred card titled "Combine
files" with three rows in order, each with a drag handle, the first page as a
small thumbnail, the file name, a page-range field and a remove cross:
"Q3 2026 Board Report.pdf" with range "All (24 pages)", "Appendix A -
Figures.pdf" with range "1–4, 9 (5 pages)", "Cover Letter.pdf" with range
"All (2 pages)". Below the rows: "+ Add files". A summary line "One PDF of 31
pages", a name field "Q3 2026 Board Pack", and a primary button "Combine". No
left panel.
```

### 07 Fill & Sign (`07-fill-sign`)

```text
This screen: mode "Fill & Sign". Tab "Riverside Lease 2026.pdf" active, page
1 of 12 on the canvas: a residential lease form whose fields are outlined in
pale blue. Filled fields: "Tenant name: Maya Chen", "Unit: 4B", "Start date:
1 November 2026". Empty fields: "Monthly rent" (marked required) and two
"Initials" boxes. A banner above the canvas: "This form has 6 fields. 2
required fields are empty." with a "Highlight fields" toggle. The right panel
"Fields" lists every field with its value or "Empty"; required ones have a red
asterisk. Below the list: "Add text", "Add date", "Add initials" buttons; an
initials box "MC" is being placed on the page.
```

### 08 Edit (`08-edit`)

```text
This screen: mode "Edit". Page 2 of the board report on the canvas, a page of
serif text. One paragraph is selected for editing with a blue outline and a
text cursor; it now reads "The board met on 21 September 2026 to review the
quarter's results." with the date just changed. The right panel "Edit text"
shows "Paragraph 3", font "Serif · 11 pt", alignment buttons (left selected),
colour "Black", and two buttons "Apply" (primary) and "Cancel". The status line
shows "Edited" with an amber dot instead of "Saved".
```

### 09 Dark reading (`09-dark-reading`)

```text
This screen: the same as "Reading" (mode "Read", page 3 of the board report at
100%), in the dark appearance: near-black graphite chrome, a dark desk, the
accent blue lighter for contrast, text light grey. The page itself stays white
with its content. The rail's "Outline" icon is selected; its panel shows the
title "Outline" and a bookmark tree: "Summary", "Revenue" (selected, with
"Revenue by region" nested under it), "Operations", "Outlook", "Appendix".
```
