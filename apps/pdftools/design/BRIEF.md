# PDF Tools v2: product requirements

Status: draft for review, 10 Oct 2026. This brief starts the OctoSense App
Flow for a new design. The order is: product requirements (this file), UX
design by image generation (`source/`), pixel mapping to Splash widgets,
then the logic wired to the pdf engine. It replaces v1's design; v1 stays on
`main` until v2 is accepted.

## Why a new design

v1 (#430) is a phone-style flow on a desktop: a library, then one page at a
time, then step-by-step merge and split. It reaches 5 of the engine's 132
commands, and a page is a static picture. Someone used to a desktop PDF app
finds nothing they expect: no document view to scroll and zoom, no search
highlights, no comments, no page organizing, no forms.

## Who it is for, and what they do

People who receive, read and send PDFs on an OctoSense desktop. Their jobs,
in order of frequency:

1. **Read** a long document: scroll through it, zoom, jump to a page or a
   section, keep their place.
2. **Find** a word or phrase and see every match on the page.
3. **Review**: highlight, underline and comment, reply to others' comments,
   mark them resolved.
4. **Fill and sign** a form: type into its fields, or type text, a date and
   their initials onto a form that has no fields.
5. **Organize pages**: rotate, delete, reorder, insert pages from another PDF,
   extract pages into a new file.
6. **Combine** several PDFs into one, choosing the pages from each.
7. **Edit** a line or paragraph of existing text (a typo, a date).
8. **Export** pages as images or the text as a file, inside the app's storage.

## Platform and constraints

- OctoSense desktop only: the pdf engine is behind the desktop-only
  `craft-engines` feature. A contained Splash system app, id `os.pdftools`,
  declaring `storage`, `files` and `pdf`.
- Files live in the app's own storage (64 MiB). A PDF comes in through
  `files.import` (up to 64 MiB, titled with its own name). Exports stay in the
  app's storage for v2's first release (`files.export` still reads at most
  1 MiB).
- Engine calls run on the shell's UI thread until #399 lands. The design
  must stay usable with that: pages render one at a time at screen
  resolution and are cached, thumbnails fill in progressively, and nothing
  re-renders the whole document on a change.
- No network, no device features. A document's own scripts never run.
- Password-protected PDFs: OctoSense rule 3 keeps secrets on host-owned
  sheets, and there is no PDF password sheet yet. v2 shows "This PDF is
  protected with a password" and does not open it.

## Window and layout

The page is the hero; tools appear where they act. Regions, top to bottom
and left to right:

| Region | What it holds |
| --- | --- |
| Document tabs | One tab per open PDF, its title and a close button; "Open" at the end |
| Mode bar | Read · Comment · Fill & Sign · Pages · Combine · Edit, the current mode marked; search field; Undo, Redo, Save |
| Left rail and panel | Rail icons: Pages (thumbnails), Outline (bookmarks), Comments, Search results. The panel shows the selected one and can collapse |
| Canvas | The document as white pages on a quiet desk, scrolled continuously; the page under the pointer is the current page |
| Canvas controls | A floating pill: page "3 / 24", previous and next, zoom out, zoom "100%", zoom in, fit width, fit page |
| Right panel | Appears only when a mode needs it: a comment thread, the form's field list, tool options, document properties |
| Status line | Save state ("Saved", "Edited"), storage used ("18.4 of 64 MB") |

Keyboard: ⌘F find, ⌘+ and ⌘− zoom, ⌘0 fit page, arrows and Page Up and Down
to move, ⌘Z and ⇧⌘Z undo and redo, ⌘S save.

## Screens

1. **Home**: recent PDFs as cards with their first page, pages and size;
   "Open a PDF from this device (up to 64 MB)"; storage used; empty state for
   a first run.
2. **Reading**: thumbnails panel, a 24-page report at 100%, canvas controls.
3. **Find**: a query, "12 matches on 7 pages", the results list with
   snippets, matches highlighted on the page, the current one stronger.
4. **Comment**: highlight tool active, a highlighted sentence, the right
   panel showing a thread with two replies, a reply box and a status menu.
5. **Pages**: every page as a large thumbnail; two selected; actions Rotate,
   Delete, Extract, Insert from file, and drag to reorder.
6. **Combine**: three PDFs in order with drag handles and page ranges
   ("all", "1–4, 9"), "One PDF of 31 pages", Combine.
7. **Fill & Sign**: a lease with its fields outlined, some filled; the right
   panel lists the fields with required ones marked; "Add initials" places
   typed initials.
8. **Edit**: one paragraph selected for editing, its new text typed in place,
   font and size shown in the right panel.
9. **Dark appearance** of Reading, with the Outline panel open.

States each screen must design for: empty library; a page still rendering;
a damaged PDF ("Couldn't open this PDF" and the engine's reason); a
protected PDF; storage full ("Remove a PDF to make room"); unsaved changes
when closing a tab.

## Actions and the engine

| Action | Engine command | Class | App method to add |
| --- | --- | --- | --- |
| Open, page sizes, outline, fields, properties | `doc_open` (scripts off), `doc_info`, `bookmark_list` | safe after open | `pdf.open`, `pdf.info` (extend) |
| Render a page at screen resolution | `page_render {dpi}` | safe | `pdf.page` |
| Find with match rectangles | `text_find` | safe | `pdf.find` |
| Text lines and paragraphs (selection, edit) | `text_lines`, `text_paragraphs` | safe | `pdf.lines` |
| Comments: list, add, edit, delete, reply, status | `comment_*` | safe | `pdf.comments`, `pdf.comment` |
| Fill fields, Fill & Sign text, date, initials | `form_fields`, `form_fill`, `fill_sign_add` | file/code, needs review | `pdf.fields`, `pdf.fill` |
| Rotate, delete, move, duplicate pages | `page_rotate`, `page_delete`, `page_move`, `page_duplicate` | safe | `pdf.pages` |
| Insert from file, extract | `page_insert_file`, `page_extract` | file (inside storage) | `pdf.pages` |
| Edit a line or paragraph | `text_edit` | safe | `pdf.edit_text` |
| Undo, redo, save | `edit_undo`, `edit_redo`, `doc_save` | file (inside storage) | `pdf.undo`, `pdf.redo`, `pdf.save` |
| Combine with page ranges | `pdf.merge` (extend with ranges) | file | `pdf.merge` |
| Export images, text | `doc_export_images`, `doc_export_text` | file (inside storage) | `pdf.export` |

The app's service grows with the design: each method is reviewed the way the
system agent's doors were, works in the app's storage, and keeps a document
open across calls (an open-document id per tab), so an edit doesn't reopen
the file.

## Data

- The library index in the app's storage: file, title, pages, size, last
  opened, last page and zoom.
- Comments, form values and page changes live in the PDF itself, written by
  Save. Unsaved edits stay in the engine's open document; Undo and Redo use the
  engine's history.

## Visual direction for the image prompts

A precise, calm desktop tool that looks like it belongs to OctoSense: the
document is the brightest thing on screen, chrome is quiet and thin, and one
accent marks the current mode and primary actions. Neutral greys with a slight
cool bias, white pages with a soft shadow on a light grey desk, generous but
not loose spacing, crisp small type in the chrome and real document content on
the pages. Highlights use the familiar yellow; status uses green and red
sparingly. A dark appearance with the same structure. It must not copy Adobe
Acrobat's look or iconography.

## Out of scope for v2's first release

OCR, redaction, digital signatures with certificates, comparing documents,
accessibility fixing, printing, password-protected PDFs, measuring, stamps,
links, cloud services and sharing outside the app.

## Done when

- Every job above works end to end with the real engine in the hidden
  desktop shell, in light and dark, with the states above.
- A 100-page PDF opens to its first page in under a second, and scrolling never
  freezes the shell for more than a page render. A smooth canvas waits for
  #399.
- Side-by-side comparisons with the approved generated designs pass a
  person's visual review.
- Rebuilt from the approved images through the App Flow pixel mapping, not
  hand-drawn in another style.

## Open questions

1. Is "Edit text" in v2's first release, or the next one?
2. Should Combine and Export also offer saving outside the app once
   `files.export` takes large files?
