# design engine commands

The designcraft engine's command catalog at revision 14e677b24216: 444 commands, one per line as `id` label: params.
Generated from the engine by `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-design-service --test skill`; do not edit.

A tag after the id marks a command that reaches past the open document (safety.json has every id's class): [file] reads or writes a path, [code] installs or runs code or other commands, [network] uses the network, [device] uses hardware, [host] changes windows, views, preferences, the clipboard or other app state. An untagged command works on the open document only.

## anchor

- `anchor.create` New Hyperlink Destination…: {name} — a text anchor at the insertion point
- `anchor.list` Text Anchors: {} → [{id, name, story, pos, page}]

## anchored

- `anchored.insert` Insert Anchored Object…: {ids, story?, pos? (default: the text insertion point), position?: inline|aboveLine|custom, yOffset?, align?: left|center|right, spaceBefore?, spaceAfter?, xRelative?/yRelative?: anchor|textFrame|columnEdge|pageMargin|pageEdge, xOffset?, objectPoint?/refPoint?: 0..8, keepWithinColumn?} — moves the items into the text
- `anchored.list` Anchored Objects: {story?} → [{story, index, pos, position, size}]
- `anchored.options` Anchored Object Options…: {story?, index? (default: the object at the insertion point), position?: inline|aboveLine|custom, yOffset?, align?: left|center|right, spaceBefore?, spaceAfter?, xRelative?/yRelative?: anchor|textFrame|columnEdge|pageMargin|pageEdge, xOffset?, objectPoint?/refPoint?: 0..8, keepWithinColumn?}
- `anchored.release` Release: {story?, index? (default: the object at the insertion point)} — puts the object back on the page where it is now and removes it from the text

## app

- `app.links` [host] Community & Project Links: {} → {discord, website, appPage, github, issues} (ArtCraft Discord, website, DesignCraft page and repository)

## article

- `article.add` Add Selection to Article: {name, ids? (default: the selection)} — appended in order
- `article.delete` Delete Article: {name}
- `article.list` Articles: {} → [{name, items, export}]
- `article.new` New Article: {name?, ids? (default: the selection)} → {name}
- `article.options` Article Options: {name, to?: new name, export?: bool, order?: [ids] (reorder)}
- `article.remove` Remove from Article: {name, ids}

## book

- `book.add` [file] Add Document: {path, at?: index}
- `book.close` Close Book: {}
- `book.exportPdf` [file] Export Book to PDF…: {path?} — every document in order as one PDF → {path, bytes, pages} (no path: {base64, …})
- `book.list` [file] Book: {} → {path, styleSource, documents: [{path, pages, firstPage}]}
- `book.new` [file] New Book: {path} — an empty book file (.dcbook)
- `book.open` [file] Open Book: {path} → {documents}
- `book.paginate` [file] Update Numbering: {} — each document starts numbering where the previous one ended (files are saved)
- `book.remove` [file] Remove Document: {index}
- `book.styleSource` [file] Style Source: {index}
- `book.syncStyles` [file] Synchronize Book: {} — paragraph and character styles and swatches of the style source go into every document (by name)

## bookmark

- `bookmark.add` New Bookmark: {name?, page? (1-based; default: current selection's page or 1)}
- `bookmark.delete` Delete Bookmark: {index}
- `bookmark.list` Bookmarks: {}
- `bookmark.rename` Rename Bookmark: {index, name}

## button

- `button.clear` Convert to Object: {ids?}
- `button.list` Buttons: {} → [{id, name, action}]
- `button.set` Convert to Button: {ids?, action: page|firstPage|lastPage|nextPage|previousPage|url|none, page?, url?} — the selected objects act on release in interactive PDF

## changes

- `changes.accept` Accept Change: {story, start} — the change at that position
- `changes.acceptAll` Accept All Changes: {story?} — deleted text goes, added text stays
- `changes.list` Changes: {} → [{story, start, end, kind: inserted|deleted, text}]
- `changes.reject` Reject Change: {story, start} — the change at that position
- `changes.rejectAll` Reject All Changes: {story?} — added text goes, deleted text comes back
- `changes.track` Track Changes: {on?: bool (default: toggle)}

## color

- `color.convert` Convert Color: {color: {model: rgb, r, g, b} | {model: cmyk, c, m, y, k} | {model: gray, k}, to: rgb|cmyk|gray|lab} — through the working spaces
- `color.loadProfile` [host] Load Profile…: {path} — an ICC profile (.icc/.icm) to use as a working space or proof target
- `color.settings` [host] Color Settings: {rgb?: working RGB profile, cmyk?: working CMYK profile, intent?: perceptual|relative|saturation|absolute, bpc?: bool} → {rgb, cmyk, intent, bpc, profiles}

## condition

- `condition.apply` Apply Condition: {name, on?: bool (default true), only?: bool (remove the others)} — to the selected text; `name: null` with `only` removes all
- `condition.delete` Delete Condition: {name} — the text it was applied to stays (shown)
- `condition.list` Conditions: {} → [{name, color, visible}]
- `condition.new` New Condition…: {name, color?: [r, g, b], visible?} → {name}
- `condition.options` Condition Options…: {name, to?: new name, color?: [r, g, b], visible?: bool}

## conveyor

- `conveyor.clear` Clear Conveyor: {}
- `conveyor.collect` Collect: {ids? (default: the selection)} — each object onto the Content Collector conveyor → {count}
- `conveyor.list` Conveyor: {} → [name]
- `conveyor.place` Place: {index? (0), spread?, x?, y?, keep?: bool (stay on the conveyor)} — the Content Placer: the collected object at x/y (top-left)

## data

- `data.fields` [file] Data Fields: {csv? | rows? | path? | bytes?, name?} → [{name, kind, uses}]; no params reads the linked source and does not attach one
- `data.merge` [file] Create Merged Document…: {csv? | rows? | path? | bytes?, name?, records?, one?, range?, perPage?, arrange?, insets?, columnSpacing?, rowSpacing?, fitting?, center?, linkImages?, limit?} → {records, pages, missingImages, oversetStories, warnings}
- `data.options` Data Merge Options: {records?, one?, range?, perPage?, arrange?, insets?, columnSpacing?, rowSpacing?, fitting?, center?, linkImages?, limit?}
- `data.placeholder.add` Insert Field: {field, role?: text|image|qr|hyperlink, story?, at?, end?, item?} → {id}
- `data.placeholder.remove` Remove Field: {id}
- `data.preview` [file] Preview Record: {record?: n} → fill the template with that record (session only)
- `data.preview.stop` Stop Preview: {} → restore the unfilled template
- `data.source.remove` Remove Data Source: {} → drop the source; placeholders stay
- `data.source.select` [file] Select Data Source…: {path | bytes|base64, name, delimiter?: comma|tab|semicolon, sheet?} → {fields, records, warnings}
- `data.source.update` [file] Update Data Source: {} → re-read the linked file

## document

- `document.history` History: {}
- `document.inspect` Inspect Document: {} → pages, spreads, items, stories (with overset), styles, swatches, selection
- `document.list` List Documents: {}
- `document.preferences` Document Preferences: {horizontalUnits?, verticalUnits?: points|picas|inches|millimeters|…, keyboardIncrement? (pt), baselineGrid?: {start, increment, relativeTo, color, viewThreshold}, grid?: {horizontal, vertical, subdivisions, color, inBack}, pasteboard?: [h, v], marginColor?, columnColor?, bleedColor?, slugColor?: [r, g, b], advancedType?: {superscriptSize, superscriptPosition, subscriptSize, subscriptPosition} (%), overprintBlack?, glyphFallback? (draw characters the font lacks from fallback fonts; off: as the font's missing-glyph box)} → those settings

## edit

- `edit.clear` Clear: {ids?} — delete selected items (or the selected text)
- `edit.copy` Copy: {} → {text} when text is selected (with formatting, footnotes, markers)
- `edit.cut` Cut: {}
- `edit.deselectAll` Deselect All: {}
- `edit.duplicate` Duplicate: {}
- `edit.paste` Paste: {inPlace?: bool, text?: the system clipboard's text (pastes it unless it is what was copied here)}
- `edit.pasteInPlace` Paste in Place: {}
- `edit.pasteInto` Paste Into: {id?} — the copied objects become the content of the selected frame (or `id`), clipped by it
- `edit.pasteWithoutFormatting` Paste without Formatting: {text?} — plain text at the insertion point, in the format there
- `edit.redo` Redo: {}
- `edit.selectAll` Select All: {} — all items on the active spreads, or all text in the story
- `edit.stepAndRepeat` Step and Repeat…: {count?: 1, dx?, dy?, rows?, columns?, ids?} — copies of the selection (or ids) offset by (dx, dy); with rows/columns, a grid
- `edit.transparencyBlendSpace` Transparency Blend Space: {space: cmyk|rgb} — the colour space transparency is composited in for output
- `edit.undo` Undo: {}

## endnote

- `endnote.delete` Delete Endnote: {story, id} — removes the reference and its text
- `endnote.edit` Edit Endnote: {story, id, text}
- `endnote.insert` Insert Endnote: {text?} — a reference at the insertion point; the first one makes the endnote frame on a new last page → {story, id, frame}
- `endnote.list` Endnotes: {} → [{story, id, number, text}] in numbering order
- `endnote.options` Document Endnote Options…: {style?: arabic|upperRoman|lowerRoman|upperLetters|lowerLetters, startAt?, prefix?, suffix?, heading?, headingStyle?, paraStyle?, separator?} → the options

## file

- `file.activate` Activate Document: {index}
- `file.close` [file] Close: {index?}
- `file.exportEpub` [file] Export EPUB (Reflowable)…: {path?, title?, author?, language?: "en", cover?: bool (the first page as the cover image), fixedLayout?: bool (pre-paginated: each page as an image with its text)} → {path, bytes} (no path: {base64, bytes})
- `file.exportFixedEpub` [file] Export EPUB (Fixed Layout)…: {path?, title?, author?, language?} — pre-paginated EPUB → {path, bytes} (no path: {base64, bytes})
- `file.exportHtml` [file] Export HTML…: {path?, title?, language?} — one self-contained page (styles inline, images embedded), stories and graphics in reading order → {path, bytes} (no path: {text, bytes})
- `file.exportIdml` [file] Export IDML…: {path?, embedImages?: true} — writes an IDML package to `path`, or returns {base64} without a path
- `file.exportPdf` [file] Export PDF…: {path?, pages?: "1-3,5" | [1,3] (1-based positions; default all), flatten?: high|medium|low|ppi (Transparency Flattener: spreads with transparency are rasterised), spreads?: bool, fullScreen?: bool, bookmarksPanel?: bool, pageLayout?: single|continuous|twoUp|twoUpCover|twoUpContinuous, view?: fitPage|fitWidth|actual, advanceSeconds?: number (interactive PDF), bleed?: bool (document bleed), marks?: bool | {crop?, bleed?, pageInfo?, weight?, offset?}, standard?: "none"|"x4"|"a2b", compressImages?: bool, tagged?: bool (structure tree: stories as paragraphs, figures with alt text), media?: bool (interactive: embed placed video and sound), title?, author?} → {path, bytes, pages, warnings} (no path: {base64, …})
- `file.exportText` [file] Export Text…: {path?, format?: "txt"|"rtf"|"tagged" (Tagged Text; default from the path, else txt), story?, frame?} — the story being edited or of the selected frame → {path, bytes} (no path: {text, bytes})
- `file.exportXml` [file] Export XML…: {path?} — the tagged content in reading order → {path, bytes} (no path: {text})
- `file.importXml` [file] Import XML…: {path | text} — each element's content goes into the next text frame with that tag (child elements of mapped tags become paragraphs in their style) → {placed, unmatched}
- `file.new` Document…: {preset?: "Letter"|"A4"|…, width?, height?, pages?, facingPages?, columns?, gutter?, margins?: number|{top,bottom,inside,outside}, bleed?, title?}
- `file.newSample` Sample Document: {} — a multi-page magazine sample
- `file.open` [file] Open…: {path} — .designcraft or .idml; the fonts in a `Document Fonts` folder beside it load first → {index, documentFonts: faces loaded, warnings: font files skipped}
- `file.openBytes` [file] Open Bytes: {name, base64} — DesignCraft JSON or an IDML package
- `file.openIdml` [file] Open IDML: {path | base64, name?} — opens an IDML package as a new document (linked images are read next to the file or from its Links/ folder; the fonts in a `Document Fonts` folder beside it load first) → {index, documentFonts, warnings}
- `file.package` [file] Package…: {dir: folder to create, idml?: true, pdf?: false, instructions?: text} — the document (its placed files relinked to Links/), Links/, the font files that draw its text in Document Fonts/ (fallback fonts included; unless their licence restricts it), an IDML copy, an optional PDF and a report → {dir, files, report}
- `file.place` [file] Place…: {path?|base64?, name?, frame?: id (place into), spread?, x?, y?, width?, pdfPage?: n (1-based, Image Import Options), pdfCrop?: crop|trim|bleed|art|media, layoutPage?: n (IDML / .designcraft: that page's objects as a group)} — places an image (into the selected empty frame if any); text files (.txt, .docx, .rtf, .md) and Excel workbooks (.xlsx, as a table) go into the insertion point, the selected frame or a new frame on `page`/`rect` — {autoflow?: adds pages with threaded frames until the text fits, removeStyles?, styleMap?: {imported name: document style}, styleConflicts?: useExisting|redefine|autoRename}
- `file.presets` Document Presets: {}
- `file.print` [device] Print…: {printer?, copies?: 1, pages?: "1-3,5", spreads?, marks?, bleed?, dryRun?: bool (only return the spool command)} → {printer, copies, pages, command}
- `file.printBooklet` [file] Print Booklet…: {path?, type?: saddleStitch|twoUpConsecutive, spaceBetween? (pt)} — printer spreads as PDF (pages imposed in booklet order) → {path, bytes, sheets} (no path: {base64, …})
- `file.printers` [device] Printers: {} → {printers: [names], default}
- `file.recovery.list` [file] Recovery Data: {dir?} → [{uid, path, title, saved}] unsaved documents left by a crash (default: the session's recovery folder)
- `file.recovery.open` [file] Recover Documents: {dir?} — reopen documents left in the recovery folder (unsaved)
- `file.recovery.save` [file] Save Recovery Data: {dir?} — write every unsaved document to the recovery folder (the app does this on a timer)
- `file.revert` [file] Revert: {} — back to the last saved version (not undoable)
- `file.save` [file] Save: {path?}
- `file.saveACopy` [file] Save a Copy…: {path} — writes the document without changing which file it is or its unsaved state
- `file.saveAs` [file] Save As…: {path}
- `file.serialize` Serialize: {} → {base64, bytes} the .designcraft file

## find

- `find.change` Change All: {find, change, grep?, caseSensitive?, wholeWord?, scope?, story?, first?: bool (change only the first match), attrs?: {character attributes applied to changed text}} → {count}
- `find.changeObjects` Change All (Object): {…find.objects criteria, change: {fill?, stroke?, strokeWeight?, opacity?}} → {changed}
- `find.find` Find: {find, grep?: bool, caseSensitive?: bool, wholeWord?: bool, scope?: document|story|selection, story?} → matches [{story, start, end, text}]
- `find.next` Find Next: {find, grep?, …} — selects the next match after the caret
- `find.objects` Find Object: {fill?, stroke?: swatch, strokeWeight?, opacity? (0–1), kind?: text|graphic|shape|line|group, layer?, label?, select?: bool (default true)} → {ids} — Find/Change › Object, through the document

## font

- `font.list` Fonts in Document: {} → [{family, style, characters, missing, styleMissing, source: bundled|installed|document|added (null when missing)}] (missing first)
- `font.replace` Find/Replace Font…: {family, style?, toFamily, toStyle?, redefineStyles?: true} — replaces the font in text and (by default) in paragraph and character styles

## footnote

- `footnote.delete` Delete Footnote: {story, id} — deletes the reference (and so the footnote)
- `footnote.goToReference` Go to Footnote Reference: {} — from footnote text, puts the caret after the footnote's reference
- `footnote.insert` Insert Footnote: {text?} — a footnote reference at the insertion point; the caret moves into the new footnote's text
- `footnote.list` Footnotes: {story?} → [{story, id, index, anchor, text}] in story order
- `footnote.options` Document Footnote Options…: {style?: arabic|upperRoman|lowerRoman|upperLetters|lowerLetters|symbols, startAt?, restart?: never|page|spread|section, prefix?, suffix?, affixIn?: none|reference|text|both, refPosition?: superscript|subscript|normal|otSuperscript, refCharStyle?, paraStyle?, separator?, spaceBefore?, spaceBetween?, firstBaseline?, firstBaselineMin?, spanColumns?, rule?: {on, weight, color, tint, width, offset, leftIndent}} → the options (no params: just read them)
- `footnote.setText` Set Footnote Text: {story, id, text} — replaces the footnote's text

## form

- `form.clear` Convert to Object: {ids?} — no longer a form field
- `form.list` Form Fields: {} → [{id, kind, name, value, options}]
- `form.set` Convert to Form Field: {ids?, kind: textField|checkBox|comboBox|listBox|signature, name?, value?, options?: [string], multiline?, required?, fontSize?} — interactive PDF form fields

## frame

- `frame.create` Create Frame: {spread?, rect: [x0,y0,x1,y1] (spread coords), shape?: rectangle|ellipse|polygon, content?: graphic|text|unassigned, sides?: 6, text?: string, caret?: bool, vertical?: bool (text: a new vertical story)}
- `frame.grid` Gridify: {rect, cols, rows, gutter? (pt, 12), shape?, content?, spread?} — a grid of frames filling `rect` (frame tools: arrow keys while dragging)

## gap

- `gap.move` Move Gap: {at: [x, y] (spread), delta | dx/dy, spread?} — Gap tool: the gap at `at` between objects (or an object and the page edge) moves by `delta`; the objects on both sides resize → {axis, changed}

## guide

- `guide.add` New Guide: {orientation: horizontal|vertical, position (spread coordinate), spread?, page? (index in the spread; default: the page under `at` or the position), at? (the other coordinate), spreadGuide?: bool} → {page, index}
- `guide.delete` Delete Guide: {spread?, page, index}
- `guide.deleteAll` Delete All Guides on Spread: {spread?}
- `guide.liquid` Convert to Liquid Guide: {spread?, page, index, on?: bool (default true)}
- `guide.list` Guides: {spread?} → [{page, index, orientation, position, spread}]
- `guide.move` Move Guide: {spread?, page, index, position}

## hyperlink

- `hyperlink.create` New Hyperlink…: {url? | email? | page? (1-based), name?, ids?} — from the selected text, or the selected frames
- `hyperlink.delete` Delete Hyperlink: {id}
- `hyperlink.edit` Hyperlink Options…: {id, name?, url? | email? | page? (1-based)}
- `hyperlink.goToSource` Go To Source: {id} — selects the hyperlink's text or frame
- `hyperlink.list` Hyperlinks: {}

## hyphenation

- `hyphenation.addException` Add Hyphenation Exception: {word: "ex~am~ple" (breaks only at ~) or a word with no ~ (never hyphenated)}
- `hyphenation.list` Hyphenation Exceptions: {} → [word]
- `hyphenation.removeException` Remove Hyphenation Exception: {word} (with or without ~)

## index

- `index.addReference` New Page Reference…: {topics?: [level 1..4] | topic?: "Level 1 > Level 2" (default: the selected text), sort?: [keys], range?: currentPage|toEndOfStory|suppressPageRange|nextParagraphs|see|seeAlso, paragraphs?: n, target?: topic for see/seeAlso} — a marker at the insertion point
- `index.generate` Generate Index…: {title?: "Index", sectionHeadings?: true, runIn?: false, page?: 1-based page for a new frame, rect?: [x0,y0,x1,y1]} — replaces the existing index → {story, entries}
- `index.list` Index Page References: {} → [{story, pos, topics, range, page}]
- `index.update` Update Index: {} — regenerate the existing index

## ink

- `ink.list` Ink Manager: {} → {allToProcess, inks: [{name, process: bool (printed as process), alias?}]} — the spot inks
- `ink.options` Ink Manager: {allToProcess?: bool, ink?: spot swatch, toProcess?: bool, alias?: spot swatch | null} — output: spots to process, ink aliases

## layer

- `layer.activate` Set Active Layer: {id}
- `layer.delete` Delete Layer: {id}
- `layer.deleteUnused` Delete Unused Layers: {} — layers without objects (one layer always stays)
- `layer.merge` Merge Layers: {ids: [layer ids], into?: layer id (default: the first)} — their objects move to `into`, the other layers are deleted
- `layer.move` Move Layer: {id, to: index (0 = top/frontmost)}
- `layer.new` New Layer…: {name?}
- `layer.others` Hide/Lock Others: {id, hide?: bool, lock?: bool, show?: true (Show All Layers), unlock?: true (Unlock All Layers)} — Hide Others / Lock Others act on every layer but `id`
- `layer.selectItems` Select All on Layer: {id}
- `layer.set` Layer Options…: {id, name?, visible?, locked?, printable?, color?: [r,g,b]}

## layout

- `layout.createAlternate` Create Alternate Layout…: {name, width, height, rule?: page rule for the copies (default: keep each page's)} — every page copied after the last at the new size (a new section marked `name`), objects following the liquid rules
- `layout.createGuides` Create Guides…: {rows?: 0, columns?: 0, rowGutter?: 12, columnGutter?: 12, fitTo?: margins|page, removeExisting?: false, spread?, page? (index in the spread; default all pages)}
- `layout.detachAll` Detach All Objects from Parent: {page (0-based)} — the page's overridden copies become ordinary items
- `layout.documentSetup` Document Setup…: {width?, height?, pages?: count, startPage?: n, facingPages?, binding?: leftToRight|rightToLeft, intent?: print|web|mobile, bleed?, slug?: n | [top, bottom, inside, outside], adjustLayout?: bool (objects follow the new page size)} → the document setup
- `layout.marginsAndColumns` Margins and Columns…: {pages?: [index] (default all), margins?: number|{top,bottom,inside,outside}, columns?, gutter?, adjustLayout?: bool (objects follow the margins)}
- `layout.overrideParentItems` Override All Parent Page Items: {page (0-based), ids?: parent items (default: all on the page's parent page)} → {ids} the page's editable copies
- `layout.pageSize` Page Size: {pages: [1-based page numbers], width?, height?, preset?: "A4"|"Letter"|…} — pages of their own size (Page tool); objects on the pages keep their place on them
- `layout.pages.applyParent` Apply Parent to Pages…: {pages: [index], parent: "A"|null}
- `layout.pages.delete` Delete Pages: {pages: [index]}
- `layout.pages.duplicateSpread` Duplicate Spread: {spread}
- `layout.pages.insert` Insert Pages…: {count?: 1, after?: page index (default: last), parent?: "A"|null}
- `layout.pages.move` Move Pages…: {from, to}
- `layout.pages.toSpread` Add Page to Spread: {page (1-based), spread (0-based)} — the page joins that spread (up to 10 pages; the spread stops shuffling)
- `layout.parents.edit` Edit Parents: {on: bool} — show parent spreads on the canvas
- `layout.parents.new` New Parent…: {prefix?, name?: "Parent"}
- `layout.removeOverrides` Remove All Local Overrides: {page (0-based)} — deletes the page's overridden copies; the parent items show again
- `layout.rotateSpreadView` Rotate Spread: {spread?: index (0), angle: 90 (clockwise) | -90 | 180 | 0 (clear)} — turn one spread on screen (output is unaffected)
- `layout.section` Numbering & Section Options…: {page (1-based; starts a section there), startNumber?: n|null (continue), style?: arabic|upperRoman|lowerRoman|upperLetters|lowerLetters, prefix?, includePrefix?, marker?, remove?: bool}
- `layout.spreadShuffle` Allow Selected Spread to Shuffle: {spread (0-based), allow: bool} — off keeps the spread's pages together when pages are added or removed

## library

- `library.add` [file] Add Item: {name?, description?} — the selection → {index}
- `library.close` Close Library: {}
- `library.json` Library Contents: {} → the library as JSON text (to save where there's no file system)
- `library.list` Library Items: {} → [{index, name, description}]
- `library.new` [file] New Library: {path?} — an empty Object Library (saved to `path` when given)
- `library.open` [file] Open Library: {path | json} → {items}
- `library.place` Place Item: {index, spread?, x?, y?} — at its original position unless x/y (top-left) are given
- `library.remove` [file] Delete Item: {index}

## line

- `line.create` Create Line: {spread?, a: [x,y], b: [x,y]}

## links

- `links.copyTo` [file] Copy Link(s) To…: {dir, assets?: [asset ids] (default: all)} — write the placed files to `dir` and relink to the copies → {copied}
- `links.embed` Embed Link: {asset} — keep only the copy in the document
- `links.goTo` Go To Link: {asset} — selects the frames showing the graphic
- `links.list` [file] Links: {} → [{asset, name, path, status: ok|modified|missing|embedded, pixels, uses: [{id, page, ppi}]}]
- `links.relink` [file] Relink…: {asset, path} — use another file for every placement of the graphic
- `links.relinkFolder` [file] Relink to Folder…: {dir?: folder to look in (default: each link's own folder), extension?: e.g. "tif" (Relink File Extension), assets?: [ids], missingOnly?: bool (default true with `dir`)} → {relinked, notFound: [names]}
- `links.update` [file] Update Link: {asset?} — reload modified links from their files (all when no asset)

## liquid

- `liquid.object` Object Liquid Settings: {ids?, resizeWidth?, resizeHeight?, pinTop?, pinBottom?, pinLeft?, pinRight?} — for object-based pages
- `liquid.pageRule` Liquid Page Rule: {rule: off|scale|reCenter|guideBased|objectBased, pages?: [1-based] (default: all)}

## list

- `list.define` Define Lists…: {name, continueAcrossStories?: true, rename?} — paragraphs join it with type.para {listType: numbers, listName}
- `list.delete` Delete List: {name}

## math

- `math.insert` Insert Math Expression…: {latex, size?: points (12), x?, y?, spread?} — at the text insertion point (inline), else as a frame; or {id, latex} to edit one
- `math.svg` Math Expression SVG: {latex, size?} → {svg, width, height} — typeset without placing

## media

- `media.get` Media: {id?} → {kind, name, playOnPageLoad, loop, controls, poster}
- `media.options` [file] Media Options: {ids?, playOnPageLoad?, loop?, controls?, poster?: {path | base64+name} | null (placeholder)} — for placed video and sound

## note

- `note.convertToText` Convert Note to Text: {story, id} — the note's text replaces its anchor
- `note.delete` Delete Note: {story, id}
- `note.edit` Edit Note: {story, id, text?, author?}
- `note.list` Notes: {} → [{story, id, at, author, text}] in story order
- `note.new` New Note: {text, author?} — at the insertion point → {story, id}

## object

- `object.align` Align: {edge: left|hcenter|right|top|vcenter|bottom, to?: selection|keyObject|margins|page|spread, ids?}
- `object.altText` Object Export Options: {text, ids?} — alternative text for tagged PDF and EPUB
- `object.arrange` Arrange: {to: front|forward|backward|back, ids?}
- `object.attributes` Attributes: {overprintFill?, overprintStroke?, overprintGap?, nonprinting?: bool, ids?} — Attributes panel
- `object.bevel` Bevel and Emboss: {on?: bool, size?, depth? (%), angle?, globalLight?: bool, highlight?, highlightOpacity?, shadow?, shadowOpacity?, ids?}
- `object.bringForward` Bring Forward: {}
- `object.bringToFront` Bring to Front: {}
- `object.caption` Generate Static Caption: {text?: template (default "{name}"; variables {name} {path} {altText} {label} {ppi} {dimensions} {format}), position?: below|above|left|right, offset? (pt, 0), height? (pt, 24), style?: paragraph style, live?: bool (kept current as the source changes), ids?} → {ids}
- `object.clippingPath` Clipping Path…: {type?: alpha|edges (default alpha), threshold?: 0–255 (alpha: opaque above; edges: darker than 255 − threshold counts; default 25), tolerance?: px (2), ids?} — traces the placed graphic and makes the outline the frame (Convert Clipping Path to Frame)
- `object.color` Apply Color: {color: "#rrggbb"|{c,m,y,k}(0..100)|[r,g,b](0..255), target?: fill|stroke, ids?} — an unnamed colour (not in the Swatches panel until Add to Swatches) on the selection
- `object.content` Content: {type: graphic|text|unassigned, ids?}
- `object.convertShape` Convert Shape: {to: rectangle|roundedRectangle|beveledRectangle|inverseRoundedRectangle|ellipse|triangle|polygon|line|orthogonalLine|openPath|closedPath, ids?}
- `object.cornerOptions` Corner Options…: {shape: none|rounded|inverseRounded|inset|bevel|fancy, size, ids?}
- `object.directionalFeather` Directional Feather: {on?: bool, widths?: [top, left, bottom, right] | number, ids?}
- `object.distribute` Distribute: {axis: horizontal|vertical, by?: centers|spacing, spacing?: points, ids?}
- `object.dropShadow` Drop Shadow: {on?: bool, distance?, angle?, globalLight?: bool, opacity?, size?, spread? (%), color? (swatch), ids?}
- `object.exportOptions` Object Export Options…: {ids?, artifact?: bool (tagged PDF decoration), rasterize?: bool (EPUB/HTML image), align?: left|center|right|"", pageBreakBefore?: bool, altText?}
- `object.feather` Basic Feather: {width (0 = off), ids?}
- `object.fill` Fill: {swatch, tint?: 0..1, ids?}
- `object.findByLabel` Find by Script Label: {label} → ids of objects whose label is `label`
- `object.fit` Fitting: {mode: fillProportionally|fitProportionally|fitContentToFrame|centerContent|fitFrameToContent, ids?}
- `object.fittingOptions` Frame Fitting Options…: {autoFit?: bool, fitting?: none|fillProportionally|fitProportionally|fitContentToFrame|centerContent, align?: 0..8 (reference point, 4 = centre), crop?: n | [t, l, b, r] (negative adds space), ids?}
- `object.globalLight` Global Light…: {angle} — the light angle shared by shadows that use Global Light
- `object.gradient` Gradient: {kind?: linear|radial, stops?: [{location: 0–100, color: "#rrggbb"|{c,m,y,k}|[r,g,b], opacity?: 0–100, midpoint?: 13–87}], reverse?: bool, angle?: degrees, from?: [x,y], to?: [x,y] (spread coords: the Gradient Swatch tool's drag), ids?} → {swatch, kind, stops} — edits the fill's gradient (an unnamed gradient unless it equals a gradient swatch)
- `object.gradientFeather` Gradient Feather: {on?: true, radial?, angle?, start?: opacity 0–100 (100), end?: (0), from?: [x,y], to?: [x,y] (spread coords: the Gradient Feather tool's drag), ids?}
- `object.group` Group: {ids?}
- `object.hide` Hide: {ids?}
- `object.innerGlow` Inner Glow: {on?: bool, opacity?, size?, choke? (%), center?: bool (glow from the centre), color?, ids?}
- `object.innerShadow` Inner Shadow: {on?: bool, distance?, angle?, globalLight?: bool, opacity?, size?, choke? (%), color?, ids?}
- `object.label` Script Label: {label, ids?} — free text scripts and agents can find objects by
- `object.layerOptions` Object Layer Options…: {ids?, hidden: [layer name]} — layers of placed PDFs to hide ([] = the file's own settings)
- `object.lock` Lock: {ids?}
- `object.makeCompoundPath` Make Compound Path: {ids?} — one path from the selected paths (the backmost object's look); paths inside others become holes → {id}
- `object.matchAttributes` Apply Attributes (Eyedropper): {from: id, ids?} — copy fill, stroke, corners, opacity, effects, wrap
- `object.opacity` Opacity: {opacity: 0..1, blend?: normal|multiply|…, ids?}
- `object.outerGlow` Outer Glow: {on?: bool, opacity?, size?, spread? (%), color?, ids?}
- `object.pathfinder` Pathfinder: {op: add|subtract|intersect|exclude|minusBack, ids?} — combine the selected shapes into one (the backmost keeps its look; the frontmost for minusBack) → {id}
- `object.pdfLayers` Object Layers: {id?} → [{name, visible, default}] — the layers of the selected placed PDF
- `object.primaryTextFrame` Primary Text Frame: {on?: bool (default: toggle), ids?} — the selected text frame's story becomes the primary story (Smart Text Reflow adds and removes pages for it)
- `object.qrCode` Generate QR Code…: {type?: url|text|sms|email|vcard (default url), content (url/text), number?, message?, to?, subject?, body?, name?, phone?, email?, org?, url?, color? (swatch, [Black]), rect? (new object; else into the selected frame, or re-encode a selected code), spread?} → {id, content}
- `object.releaseCompoundPath` Release Compound Path: {ids?} — each subpath becomes its own object → {ids}
- `object.rename` Rename: {id, name}
- `object.satin` Satin: {on?: bool, opacity?, angle?, distance?, size?, invert?: bool, color?, ids?}
- `object.sendBackward` Send Backward: {}
- `object.sendToBack` Send to Back: {}
- `object.setFlags` Set Visibility / Lock: {ids: [id], hidden?: bool, locked?: bool} — Layers panel eye/lock per object
- `object.setLayer` Move to Layer: {layer, ids?}
- `object.showAll` Show All on Spread: {}
- `object.stroke` Stroke: {swatch?, tint?, weight?, align?: center|inside|outside, type?: {kind:…}, cap?: butt|round|projecting, join?: miter|round|bevel, miterLimit?, start?, end?: none|simple|simpleWide|triangle|triangleWide|barbed|curved|circle|circleSolid|square|squareSolid|bar, gapSwatch?, gapTint?, gapOverprint?: bool, ids?}
- `object.textFrameOptions` Text Frame Options…: {columns?, gutter?, inset?: number|[t,l,b,r], verticalJustification?: top|center|bottom|justify, firstBaseline?, autoSize?, ignoreWrap?, balanceColumns?, vertical?: bool (sets the story direction of the frames' stories, as Type ▸ Story Direction), ids?}
- `object.textWrap` Text Wrap: {mode: none|boundingBox|contour|jumpObject|jumpToNextColumn, offset?: number|[t,l,b,r], invert?: bool, ids?}
- `object.transparencyGroup` Group Transparency: {isolate?: bool, knockout?: bool, ids?} — Effects panel: Isolate Blending, Knockout Group
- `object.ungroup` Ungroup: {ids?}
- `object.unlockAll` Unlock All on Spread: {}

## page

- `page.transition` Page Transition: {pages?: [1-based] | spread? | all?: bool, kind: blinds|box|comb|cover|dissolve|fade|push|split|uncover|wipe|zoomIn|zoomOut|none, duration?: seconds (1), horizontal?: bool} — for interactive PDF
- `page.transitions` Page Transitions: {} → [{spread, kind, duration, horizontal}]

## path

- `path.addAnchor` Add Anchor Point: {id, at: [x,y] (spread coords; the nearest point on the path), tolerance?: pt (default 4)} → {subpath, anchor}
- `path.appendAnchor` Add Anchor: {id, anchor: {p, in?, out?}} (spread coords)
- `path.close` Close Path: {id}
- `path.convertAnchor` Convert Direction Point: {id, subpath, anchor, to?: [x,y] (spread coords: drag out symmetric handles to there), corner?: bool} — without `to`, a smooth point becomes a corner and a corner gets smooth handles
- `path.create` Create Path: {spread?, anchors: [{p:[x,y], in?:[x,y], out?:[x,y]}], closed?: bool}
- `path.deleteAnchor` Delete Anchor Point: {id, subpath, anchor} — removes the point (and the object when no path is left)
- `path.erase` Erase: {id, points: [[x,y], …] (spread coords: the Erase tool's drag), tolerance?: pt (default 4)} — removes the stretch of path the drag ran along
- `path.moveAnchors` Move Anchors: {id, anchors: [[subpath, index]], dx, dy, handle?: in|out}
- `path.smooth` Smooth: {id, points: [[x,y], …] (spread coords), tolerance?: pt (default 4)} — re-fits the stretch the drag ran along with fewer, smoother anchors
- `path.split` Split Path: {id, at: [x,y] (spread coords; the nearest point on the path)} — Scissors: an open path becomes two, a closed one opens there → {ids}

## place

- `place.drop` Place Loaded Graphic: {spread?, x, y, rect?: [x0,y0,x1,y1], frame?: id}
- `place.load` [file] Load Place Cursor: {path | base64, name?} — load a graphic into the place cursor (then click/drag with the placeGun tool)
- `place.styles` [file] Import Options: Styles: {path | base64, name} → {paragraph: [name], character: [name], conflicts: [name]} — the styles a Word/RTF file brings (for styleMap / styleConflicts)

## preflight

- `preflight.run` Preflight Document: {minPpi?: 150} → issues [{severity, kind, message, item?, page?}]

## prefs

- `prefs.set` [host] Preferences: {showHiddenCharacters?, typographersQuotes?, polygonSides?, starInset?, scaleStrokes?, dimensionsIncludeStroke?, transformationsAreTotals?, absolutePageNumbers?, highlightHj?, highlightKeeps?, highlightCustomTracking?, highlightSubstitutedFonts?, richBlackOutput?, favoriteFonts?: [family], showFontNamesInEnglish?} → all application preferences

## script

- `script.run` [code] Run Script: {text} — command script lines (`command.id {json}`, JSON lines or a JSON array; `$N.path` refers to earlier results) → {results, steps}

## select

- `select.container` Container: {} — the group or frame holding the selection
- `select.content` Content: {} — the graphic in a frame, or the first object in a group
- `select.firstAbove` First Object Above: {} — the frontmost object on the spread (layers included)
- `select.lastBelow` Last Object Below: {} — the backmost object on the spread
- `select.nextAbove` Next Object Above: {}
- `select.nextBelow` Next Object Below: {}
- `select.nextInGroup` Next Object in Group: {}
- `select.previousInGroup` Previous Object in Group: {}

## selection

- `selection.set` Select: {ids: [id], add?: bool, content?: bool}
- `selection.toggle` Toggle Selection: {id}

## snippet

- `snippet.export` [file] Export Selection as Snippet: {path?} — the selected items (with their stories, styles and images) as a .designcraft snippet; no path: {base64}
- `snippet.place` [file] Place Snippet: {path | base64, spread?, x?, y?} — items keep their positions unless x/y given (top-left)

## spelling

- `spelling.addWord` Add to Dictionary: {word}
- `spelling.change` Change: {story, start, end, to}
- `spelling.check` Check Spelling…: {story?, suggestions?: true} → [{story, start, end, word, suggestions}]
- `spelling.setWords` User Dictionary: {words: [word]} — replace the document's user dictionary
- `spelling.words` User Dictionary Words: {} → [word]

## states

- `states.create` Convert Selection to Multi-State Object: {ids? (default: the selection; one group's objects or several objects become states)} → {id, states}
- `states.list` Object States: {id?} → {states, active}
- `states.release` Release State to Objects: {id?} — back to a plain group (every state shows)
- `states.rename` State Options: {id?, index, name}
- `states.show` Show State: {id?, index? | name?}

## story

- `story.get` Get Story: {story? | frame?} → text, frames, paragraphs, vertical, overset
- `story.links` Linked Stories: {} → [{story, parent, outOfDate}]
- `story.placeAndLink` Place and Link: {story (parent), rect, spread?} — a new frame whose story copies the parent and stays linked → {id, story}
- `story.replaceRange` Edit Story: {story, start, end, text} — replace a byte range keeping surrounding formatting
- `story.setDirection` Story Column Direction: {story?, direction: leftToRight|rightToLeft} — column progression, independent of paragraph direction
- `story.setText` Set Story Text: {story, text} — replace a story's whole text
- `story.unlink` Unlink: {story}
- `story.updateLink` Update Link: {story? (default: every out-of-date linked story)} — the child takes the parent's current content (its own edits are replaced)

## strokeStyle

- `strokeStyle.delete` Delete Stroke Style: {name} — strokes using it become solid
- `strokeStyle.list` Stroke Styles: {} → [{name, kind}]
- `strokeStyle.new` New Stroke Style…: {name, type: stripes|dash|dot, bands?: [[start, width] (fractions of the weight)], pattern?: [dash, gap, …] (pt)} — apply with object.stroke {type: {kind: "style", name}}

## style

- `style.breakLink` Break Link to Style: {kind: paragraph|character} — the selected text keeps its look as local formatting, with [No Paragraph Style] / [None]
- `style.cell.apply` Apply Cell Style: {name, story?, table?} — the target cells (the whole table when the cursor is in it)
- `style.cell.create` New Cell Style…: {name, fromSelection?: bool (the target cell's look), fill?, tint?, insets?: n|[t,l,b,r], vj?: top|center|bottom|justify, stroke?: {weight, color, tint?}, paragraphStyle?} → {name}
- `style.cell.edit` Cell Style Options…: {name, …same as style.cell.create} — cells using the style follow
- `style.character.apply` Apply Character Style: {name}
- `style.character.create` New Character Style…: {name, basedOn?, chars?: {…}}
- `style.character.edit` Character Style Options…: {name, rename?, basedOn?, chars?}
- `style.compositeFont.list` List Composite Fonts: {} → composite font definitions
- `style.compositeFont.set` Set Composite Font: {name, entries: [{name?, characters?, family, style?, relativeSize?, horizontalScale?, verticalScale?, baselineShift?, scaleOption?}]} — replaces a composite font definition; empty characters selects the base entry
- `style.exportTag` Export Tagging: {style, character?: bool, tag?: p|h1…h6|blockquote|pre|li|… (span|em|strong|code|sup|sub… for characters; "" = automatic), class?} → the tagging
- `style.group` Move to Group: {kind: paragraph|character, names: [style names], group: "Heads" | "Heads/Display" | "" (out of any group)} — styles are named Group/Name; every use is renamed → {renamed: {old: new}}
- `style.list` List Styles: {} → paragraph and character style names
- `style.object.apply` Apply Object Style: {name, ids?}
- `style.object.create` New Object Style…: {name, fromSelection?: true, fill?, paragraphStyle?}
- `style.paragraph.apply` Apply Paragraph Style: {name, clearOverrides?: bool}
- `style.paragraph.create` New Paragraph Style…: {name, basedOn?, nextStyle?, para?: {…}, chars?: {…}, fromSelection?: bool}
- `style.paragraph.delete` Delete Paragraph Style: {name, replaceWith?}
- `style.paragraph.edit` Paragraph Style Options…: {name, rename?, basedOn?, nextStyle?, para?: {…}, chars?: {…}}
- `style.table.apply` Apply Table Style: {name, story?, table?}
- `style.table.create` New Table Style…: {name, header?, body?, footer?, leftColumn?, rightColumn?: cell style names, border?: {weight, color}, altRows?: {first, firstColor, next, nextColor}, spaceBefore?, spaceAfter?} → {name}
- `style.table.edit` Table Style Options…: {name, …same as style.table.create} — tables using the style follow

## swatch

- `swatch.addToSwatches` Add to Swatches: {swatch?: name (default: the selection's fill), target?: fill|stroke, name?: new name} — makes an unnamed colour a swatch
- `swatch.addUnnamed` Add Unnamed Colors: {} — every unnamed colour used becomes a swatch
- `swatch.create` New Color Swatch…: {name?, color: "#rrggbb"|{c,m,y,k}(0..100)|[r,g,b], spot?: bool}
- `swatch.delete` Delete Swatch: {name}
- `swatch.load` [file] Load Swatches…: {path?|base64?, replace?: bool} — colour swatches from a swatch exchange (.ase) file; names already in the document are kept unless `replace` → {added, replaced}
- `swatch.moveToGroup` Move to Color Group: {swatches: [names], group: name | null (top level)}
- `swatch.newColorGroup` New Color Group: {name?, swatches?: [names]} — a Swatches panel folder (the swatches move into it) → {name}
- `swatch.renameColorGroup` Color Group Options: {name, to}
- `swatch.save` [file] Save Swatches for Exchange…: {path?, names?: [swatch names] (default: all colour swatches)} → {base64} or writes `path` (.ase)
- `swatch.ungroupColorGroup` Ungroup Color Group: {name} — its swatches go back to the top level

## table

- `table.convertFromText` Convert Text to Table: {columnSeparator?: tab|comma} — selected paragraphs become rows
- `table.convertToText` Convert Table to Text: {} — cells separated by tabs, rows by paragraphs
- `table.delete` Delete Table: {}
- `table.deleteColumn` Delete Column: {} — the columns of the target cells
- `table.deleteRow` Delete Row: {} — the rows of the target cells
- `table.distributeColumns` Distribute Columns Evenly: {}
- `table.dropCells` Drag Rows or Columns: {frame, from: [x, y], to: [x, y]} — selected whole rows (or columns) dragged from `from` move to the row (column) at `to`; otherwise a text drag
- `table.get` Get Table: {story?, table?} → rows, columns, cells (text, spans, fill), options
- `table.insert` Create Table: {rows?: body rows (4), cols? (4), headerRows? (0), footerRows? (0), width?} — at the text insertion point
- `table.insertColumn` Insert Column: {where?: left|right, count? (1), width?}
- `table.insertColumnLeft` Insert Column Left: {count?}
- `table.insertColumnRight` Insert Column Right: {count?}
- `table.insertRow` Insert Row: {where?: above|below, count? (1)}
- `table.insertRowAbove` Insert Row Above: {count?}
- `table.insertRowBelow` Insert Row Below: {count?}
- `table.merge` Merge Cells: {} — merge the target cell range
- `table.moveColumn` Move Column: {from?, to} (0-based; from defaults to the target column)
- `table.moveRow` Move Row: {from?, to} (0-based; from defaults to the target row) — like dragging a row
- `table.nextCell` Next Cell: {}
- `table.options` Table Options: {direction?: leftToRight|rightToLeft, border?: {weight?, color?, tint?, type?}, spaceBefore?, spaceAfter?, headerRows?, footerRows?, repeatHeader?, repeatFooter?, altRows?: {first, firstColor, firstTint, next, nextColor, nextTint, skipFirst, skipLast} | null, altCols?: … | null}
- `table.placeGraphic` [file] Convert Cell to Graphic Cell: {path | base64+name, fit?: proportional|fill (default proportional)} — an image in the target cell (its text is kept but hidden)
- `table.prevCell` Previous Cell: {}
- `table.select` Select Cells: {story?, table?, rows?: [a,b], cols?: [a,b], what?: cell|row|column|table}
- `table.selectColumn` Select Column: {}
- `table.selectRow` Select Row: {}
- `table.selectTable` Select Table: {}
- `table.setCell` Cell Options: {fill?: swatch, tint?, insets?: n | [t,l,b,r], vj?: top|center|bottom|justify, text?, stroke?: {weight?, color?, tint?, type?: solid|dashed|dotted, edges?: all|outer|inner|top|left|bottom|right}}
- `table.setColumnWidth` Column Width: {width}
- `table.setRowHeight` Row Height: {height, mode?: atLeast|exactly}
- `table.sortRows` Sort: {column?: 0-based (default: the target cell's), descending?: bool} — body rows by that column's text (numbers numerically)
- `table.splitHorizontally` Split Cell Horizontally: {} — the target cell becomes two, one above the other
- `table.splitVertically` Split Cell Vertically: {} — the target cell becomes two, side by side
- `table.textCell` Convert Cell to Text Cell: {} — drop the target cells' graphics
- `table.unmerge` Unmerge Cells: {}

## text

- `text.delete` Delete Text: {forward?: bool, word?: bool}
- `text.exitToFrame` Select Frame: {}
- `text.extendTo` Extend Text Selection: {frame, point}
- `text.insert` Type: {text, raw?: bool (no typographer's quotes)} — replaces the selected text
- `text.move` Move Caret: {dir: left|right|up|down|lineStart|lineEnd|storyStart|storyEnd, extend?, word?}
- `text.placeCaret` Place Caret: {frame, point: [x,y] (spread coords)} — converts empty frames to text frames
- `text.release` Release: {frame, point, moved: bool, copy?: bool} — ends a press in text: a drag that started in the selected text moves (copy: duplicates) it to the point; a click places the caret
- `text.select` Select Text: {story, anchor, focus} (UTF-8 byte offsets into the story text, as find.find reports them: á or — counts 2 or 3)
- `text.selectWord` Select Word: {frame, point}

## toc

- `toc.entries` Table of Contents Entries: {entries | styles} → [{level, text, page}] without changing the document
- `toc.generate` Table of Contents…: {entries: [{style, level?: 1}] | styles: [names], title?: "Contents", pageNumbers?: true, page?: 1-based page for a new frame (its margins), rect?: [x0,y0,x1,y1] (spread coords)} → {story, entries}
- `toc.update` Update Table of Contents: {} — regenerate the existing table of contents

## tool

- `tool.list` List Tools: {}
- `tool.polygonSettings` [host] Polygon Settings…: {sides?: 3–100, starInset?: 0–100 (%)} → the settings new polygons use
- `tool.select` Select Tool: {tool: selection|directSelection|type|rectangleFrame|…}

## transform

- `transform.again` Transform Again: {individually?: bool, sequence?: bool, ids?} — repeat the last transform (or the whole sequence applied to this selection), on the selection as a whole or on each object about its own centre
- `transform.clear` Clear Transformations: {ids?} — remove rotation, shear and scaling (the object keeps its centre)
- `transform.flip` Flip: {axis: horizontal|vertical, ids?}
- `transform.info` Transform Values: {ids?} → {scaleX, scaleY (%), rotation, shear (°), content?: the same for a placed graphic} of the first target
- `transform.move` Move: {dx, dy, copy?: bool, ids?, toSpread?}
- `transform.resize` Resize: {from: rect, to: rect, content?: bool (scale content), distribute?: bool (Live Distribute: objects keep their size, their centres spread with the bounds), ids?}
- `transform.rotate` Rotate: {angle (degrees, CCW), ids?}
- `transform.scale` Scale: {sx, sy, ids?} (about the selection centre)
- `transform.set` Transform Panel: {x?, y?, width?, height?, scaleX? (%), scaleY? (%), rotation? (°), shear? (°), ref?: 0..8, ids?} — reference-point based geometry; rotation and shear are absolute (Transformations are Totals decides whether nested objects measure them on the pasteboard)
- `transform.shear` Shear: {angle (degrees), ids?}

## type

- `type.align` Align: {align: left|center|right|leftJustified|…}
- `type.alignCenter` Align Center: {}
- `type.alignLeft` Align Left: {}
- `type.alignRight` Align Right: {}
- `type.allCaps` All Caps: {}
- `type.bold` Bold: {}
- `type.changeCase` Change Case: {case: upper|lower|title|sentence} — the selected text, or the stories of selected frames (formatting kept)
- `type.char` Character Formatting: {attrs: {fontFamily?, fontStyle?, size?, leading?: {kind:auto}|{kind:points,value}, tracking?, kerning?, hScale?, vScale?, baselineShift?, fill?, capitalization?, underline?, …}}
- `type.createOutlines` Create Outlines: {ids?} — text frames become compound paths of their glyphs (one per text colour, grouped when several) → {ids}
- `type.fillWithPlaceholder` Fill with Placeholder Text: {frame?}
- `type.italic` Italic: {}
- `type.justify` Justify Left: {}
- `type.kenten` Kenten: {on?: bool} — emphasis dots over the selected characters (toggles by default)
- `type.onPath` Type on a Path: {id: path, text?, start?: distance along the path, flip?, align?: baseline|ascender|descender|center} → {story} — the text cursor goes into it
- `type.openType` OpenType: {feature?: dlig|frac|ordn|swsh|titl|calt|zero|ssNN|any tag, on?: bool (default: toggle), figures?: tabularLining|proportionalOldstyle|proportionalLining|tabularOldstyle|default, stylisticSets?: [1–20]} → {features, figures}
- `type.para` Paragraph Formatting: {attrs: {align?, leftIndent?, firstLineIndent?, spaceBefore?, spaceAfter?, dropCapLines?, hyphenate?, composer?, tabs?, …}}
- `type.pathOptions` Type on a Path Options…: {id?, start?, flip?, align?: baseline|ascender|descender|center, delete?: true (Delete Type from Path)}
- `type.ruby` Ruby: {text} — the reading set over the selected text (one group); empty removes it
- `type.selectionAttrs` Selection Attributes: {} → resolved character/paragraph attributes at the text selection
- `type.sizeDown` Decrease Point Size: {}
- `type.sizeUp` Increase Point Size: {}
- `type.storyDirection` Story Direction: {vertical: bool} — the stories of the text selection or the selected frames: vertical lines run top to bottom and follow each other right to left, in every frame of the thread
- `type.tateChuYoko` Tate-Chu-Yoko: {on?: bool} — set the selected text horizontally within one em of a vertical line (toggles by default)
- `type.underline` Underline: {}

## variables

- `variables.define` Define Text Variable…: {name, type: custom|lastPageNumber|chapterNumber|fileName|creationDate|modificationDate|outputDate|runningHeader, text?, format?, style?, use?: firstOnPage|lastOnPage, character?, section?, extension?, before?, after?} — creates or replaces by name
- `variables.delete` Delete Text Variable: {name} — instances become plain text of their current value
- `variables.insert` Insert Variable: {name} — at the text insertion point
- `variables.list` Text Variables: {} → [{index, name, type, …, value (page 1)}]

## xml

- `xml.deleteDtd` Delete DTD: {}
- `xml.deleteTag` Delete Tag: {name} — tagged objects are untagged
- `xml.loadDtd` [file] Load DTD…: {path | text} — keep the DTD for validation and add a tag for each declared element; the root takes the first element's name unless already named → {elements}
- `xml.mapStyle` Map Styles to Tags: {style: paragraph style, tag | null} — that style's paragraphs export as `<tag>` elements and import back with it
- `xml.newTag` New Tag: {name, color?: [r, g, b]}
- `xml.structure` Structure: {} → {root, elements: [{tag, id, kind, text?}]} in reading order
- `xml.tag` Tag Frame: {tag (made if new), ids? (default: the selection)} — `tag: null` untags
- `xml.tagText` Tag Text: {tag | null (untag)} — the selected text becomes an inline element
- `xml.tags` Tags: {} → {root, tags: [{name, color}], styleMap: [[style, tag]], dtd: bool}
- `xml.validate` Validate from Root Element: {} — check the structure (as exported) against the loaded DTD → {valid, problems: [{path, message}]}

## xref

- `xref.defineFormat` Define Cross-Reference Format…: {name, definition} — building blocks: <fullPara />, <paraText />, <paraNum />, <pageNum />, <txtAnchrName />, <chapNum />, <fileName />, <partialPara delim=":" includeDelim="false" />; creates or replaces
- `xref.formats` Cross-Reference Formats: {} → [{name, definition}]
- `xref.insert` Insert Cross-Reference…: {anchor? | paragraph?: text to find (first paragraph containing it) | story? + para? (0-based), format?: name (default Full Paragraph & Page Number)} — at the insertion point; the text updates itself as the destination moves
- `xref.list` Cross-References: {} → [{story, pos, target, format, text}]
- `xref.setFormat` Set Cross-Reference Format: {story, index, format}
