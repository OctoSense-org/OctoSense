# word engine commands

The wordcraft engine's command catalog at revision 7584b9b2930f: 389 commands, one per line as `id` label: params.
Generated from the engine by `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-word-service --test skill`; do not edit.

A tag after the id marks a command that reaches past the open document (safety.json has every id's class): [file] reads or writes a path, [code] installs or runs code or other commands, [network] uses the network, [device] uses hardware, [host] changes windows, views, preferences, the clipboard or other app state. An untagged command works on the open document only.

## arrange

- `arrange.align` Align: {"value": "left|center|right"}
- `arrange.bringForward` Bring Forward: {}
- `arrange.position` Position: {"preset"?: "topLeft|topCenter|topRight|middleLeft|middleCenter|middleRight|bottomLeft|bottomCenter|bottomRight", "x"?: pt, "y"?: pt}
- `arrange.rotate` Rotate: {"direction": "right|left|flipH|flipV"}
- `arrange.selectionPane` Selection Pane: {}
- `arrange.sendBackward` Send Backward: {}
- `arrange.wrap` Wrap Text: {"wrap": "inline|square|tight|through|topAndBottom|behindText|inFrontOfText"}

## caret

- `caret.docEnd` Document End: {"extend"?: bool}
- `caret.docStart` Document Start: {"extend"?: bool}
- `caret.down` Down: {"extend"?: bool}
- `caret.end` Line End: {"extend"?: bool}
- `caret.home` Line Start: {"extend"?: bool}
- `caret.left` Left: {"extend"?: bool}
- `caret.pageDown` Page Down: {"extend"?: bool}
- `caret.pageUp` Page Up: {"extend"?: bool}
- `caret.paraDown` Paragraph Down: {"extend"?: bool}
- `caret.paraUp` Paragraph Up: {"extend"?: bool}
- `caret.right` Right: {"extend"?: bool}
- `caret.set` Set Caret: {"pos": Pos, "extend"?: bool} | {"page": n, "x": pt, "y": pt}
- `caret.up` Up: {"extend"?: bool}
- `caret.wordLeft` Word Left: {"extend"?: bool}
- `caret.wordRight` Word Right: {"extend"?: bool}

## design

- `design.effects` Effects: {}
- `design.pageBorders` Page Borders: {"kind": "box|none", "width"?: pt, "color"?: "RRGGBB"}
- `design.pageColor` Page Color: {"color": "RRGGBB" | null}
- `design.paragraphSpacing` Paragraph Spacing: {"value": "default|none|compact|tight|open|relaxed|double"}
- `design.setDefault` [host] Set as Default: {}
- `design.styleSet` Style Set: {"name": "default|basic|lines|shaded|casual|centered|minimalist|title"}
- `design.theme` Themes: {"name": string}
- `design.themeColors` Colors: {}
- `design.themeFonts` Fonts: {"heading": string, "body"?: string}
- `design.themes` Theme List: {}
- `design.watermark` Watermark: {"text"?: string, "remove"?: bool, "diagonal"?: bool, "color"?: "RRGGBB"}

## document

- `document.inspect` Inspect Document: {"text"?: bool}
- `document.layout` Layout Summary: {}
- `document.paragraph` Paragraph Details: {"path": [n], "story"?: Story}
- `document.selection` Selection: {}
- `document.setText` Replace Document Text: {"text": string}
- `document.text` Document Text: {}

## edit

- `edit.copy` Copy: {}
- `edit.copyFormat` Copy Formatting: {}
- `edit.cut` Cut: {}
- `edit.find` Find: {"text": string, "matchCase"?: bool, "wholeWord"?: bool, "regex"?: bool}
- `edit.findNext` Find Next: {}
- `edit.findPrevious` Find Previous: {}
- `edit.formatPainter` Format Painter: {"sticky"?: bool}
- `edit.goto` Go To: {"page"?: n, "bookmark"?: string, "paragraph"?: n}
- `edit.paste` Paste: {"text"?: string}
- `edit.pasteFormat` Paste Formatting: {}
- `edit.pasteMerge` Paste: Merge Formatting: {"text"?: string}
- `edit.pasteText` Paste: Keep Text Only: {"text"?: string}
- `edit.redo` Redo: {}
- `edit.repeat` [code] Repeat: {}
- `edit.replace` Replace: {"text": string, "with": string}
- `edit.replaceAll` Replace All: {"text": string, "with": string, "matchCase"?: bool, "wholeWord"?: bool, "regex"?: bool}
- `edit.undo` Undo: {}

## file

- `file.accessibility` Check Accessibility: {}
- `file.autosave` [file] AutoSave: {}
- `file.close` [host] Close: {}
- `file.compatibility` Check Compatibility: {}
- `file.encrypt` [host] Encrypt with Password: {}
- `file.exportPdf` [file] Export PDF: {"path": string}
- `file.exportPng` [file] Export Page as PNG: {"path": string, "page"?: n (1-based), "scale"?: px per pt}
- `file.info` Info: {}
- `file.inspect` Inspect Document: {"remove"?: ["comments", "revisions", "properties", "hidden", "headers"]}
- `file.new` New: {"template"?: "blank|sample|letter|resume|report"}
- `file.newFromTemplate` [file] New from Template: {"path": string}
- `file.open` [file] Open: {"path": string}
- `file.options` [host] Options: {}
- `file.print` [host] Print: {}
- `file.properties` Properties: {"title"?, "subject"?, "author"?, "keywords"?, "comments"?, "category"?}
- `file.protect` Protect Document: {"mode": "none|readOnly|comments|trackedChanges"}
- `file.recover` Recover Unsaved Documents: {}
- `file.save` [file] Save: {"path"?: string}
- `file.saveAs` [file] Save As: {"path": string}
- `file.saveTemplate` [file] Save as Template: {"path": string}
- `file.setAuthor` [host] User Name: {}
- `file.share` [host] Share: {}
- `file.versions` Version History: {"save"?: label, "restore"?: index}

## format

- `format.allCaps` All Caps: {}
- `format.bold` Bold: {"value"?: bool}
- `format.changeCase` Change Case: {"mode"?: "sentence|lower|upper|title|toggle"}
- `format.charStyle` Apply Character Style: {"style": string}
- `format.clear` Clear All Formatting: {}
- `format.color` Font Color: {"color": "RRGGBB" | "auto"}
- `format.doubleStrikethrough` Double Strikethrough: {}
- `format.doubleUnderline` Double Underline: {}
- `format.emboss` Emboss: {}
- `format.engrave` Engrave: {}
- `format.font` Font: {"name": string}
- `format.fontDialog` [host] Font Dialog: {}
- `format.growFont` Increase Font Size: {}
- `format.growFont1` Grow Font 1 Point: {}
- `format.hidden` Hidden: {}
- `format.highlight` Text Highlight Color: {"color": "yellow|brightGreen|turquoise|pink|blue|red|darkBlue|teal|green|violet|darkRed|darkYellow|gray50|gray25|black|none"}
- `format.italic` Italic: {"value"?: bool}
- `format.outline` Outline: {}
- `format.position` Character Position: {"points": number}
- `format.scale` Character Scale: {"percent": number}
- `format.set` Set Character Formatting: {"props": CharProps}
- `format.shading` Character Shading: {"color": "RRGGBB" | null}
- `format.shadow` Shadow: {}
- `format.shrinkFont` Decrease Font Size: {}
- `format.shrinkFont1` Shrink Font 1 Point: {}
- `format.size` Font Size: {"size": number}
- `format.smallCaps` Small Caps: {}
- `format.spacing` Character Spacing: {"points": number}
- `format.state` Formatting at Selection: {}
- `format.strikethrough` Strikethrough: {"value"?: bool}
- `format.subscript` Subscript: {}
- `format.superscript` Superscript: {}
- `format.underline` Underline: {"value"?: bool, "style"?: "single|double|thick|dotted|dash|dotDash|dotDotDash|wave|words"}
- `format.wordUnderline` Underline Words Only: {}

## hf

- `hf.next` Next Section: {}
- `hf.position` Header/Footer Position: {"header"?: pt, "footer"?: pt}
- `hf.previous` Previous Section: {}

## insert

- `insert.autoText` AutoText: {"save"?: name, "insert"?: name}
- `insert.blankPage` Blank Page: {}
- `insert.bookmark` Bookmark: {"name": string}
- `insert.closeHeader` Close Header and Footer: {}
- `insert.coverPage` Cover Page: {"title"?: string, "subtitle"?: string, "author"?: string}
- `insert.crossReference` Cross-reference: {"to": "heading|bookmark|figure|table", "target": string (text / name / number), "show"?: "text|page|number|aboveBelow"} or {} to list targets
- `insert.dateTime` Date & Time: {"format"?: "M/d/yyyy", "update"?: bool}
- `insert.docProperty` Document Property: {"name": "Title|Author|Subject|Keywords|Comments|Category"}
- `insert.dropCap` Drop Cap: {"lines"?: n (0 = none)}
- `insert.editFooter` Edit Footer: {}
- `insert.editHeader` Edit Header: {}
- `insert.equation` Equation: {"linear"?: string}
- `insert.field` Field: {"instr": string, "result"?: string}
- `insert.footer` Footer: {"text"?: string, "preset"?: "blank|blankThree|pageNumber"}
- `insert.header` Header: {"text"?: string, "preset"?: "blank|blankThree|title"}
- `insert.horizontalLine` Horizontal Line: {}
- `insert.link` Link: {"url": string, "text"?: string}
- `insert.object` [file] Object: {"path": string}
- `insert.pageBreak` Page Break: {}
- `insert.pageNumber` Page Number: {"position"?: "top|bottom|current", "align"?: "left|center|right", "format"?: "x of y"}
- `insert.picture` [file] Pictures: {"path"?: string, "data"?: base64, "width"?: pt, "alt"?: string}
- `insert.quickParts` Quick Parts: {"save"?: name (from the selection), "insert"?: name, "delete"?: name} → list
- `insert.removeFooter` Remove Footer: {}
- `insert.removeHeader` Remove Header: {}
- `insert.removeLink` Remove Hyperlink: {}
- `insert.shape` Shapes: {"kind": "rectangle|roundedRectangle|ellipse|triangle|diamond|line|arrow|star|heart", "width"?: pt, "height"?: pt, "fill"?: "RRGGBB", "stroke"?: "RRGGBB"}
- `insert.signatureLine` Signature Line: {"signer"?: string, "title"?: string}
- `insert.spreadsheet` Spreadsheet Table: {"csv"?: string}
- `insert.symbol` Symbol: {"char": string}
- `insert.table` Table: {"rows": n, "cols": n, "style"?: string}
- `insert.textBox` Text Box: {"text"?: string, "width"?: pt, "height"?: pt}
- `insert.textFromFile` [file] Text from File: {"path": string}
- `insert.wordArt` WordArt: {}

## layout

- `layout.break` Breaks: {"kind": "page|column|textWrapping|nextPage|continuous|evenPage|oddPage"}
- `layout.columns` Columns: {"count": 1-12, "space"?: pt, "separator"?: bool, "preset"?: "left|right"}
- `layout.differentFirstPage` Different First Page: {}
- `layout.differentOddEven` Different Odd & Even Pages: {}
- `layout.hyphenation` Hyphenation: {}
- `layout.lineNumbers` Line Numbers: {"value": "none|continuous|restartPage|restartSection"}
- `layout.linkToPrevious` Link to Previous: {"footer"?: bool}
- `layout.margins` Margins: {"preset"?: "normal|narrow|moderate|wide|mirrored|office2003", "top"?: pt, "bottom"?: pt, "left"?: pt, "right"?: pt, "gutter"?: pt}
- `layout.orientation` Orientation: {"value": "portrait|landscape"}
- `layout.pageNumberFormat` Format Page Numbers: {"format"?: "decimal|lowerRoman|upperRoman|lowerLetter|upperLetter", "start"?: n}
- `layout.pageSetup` Page Setup: {"section"?: SectionProps}
- `layout.section` Section Properties: {}
- `layout.size` Size: {"name"?: "Letter|Legal|A4|…", "width"?: pt, "height"?: pt}
- `layout.verticalAlign` Vertical Alignment: {}

## mailings

- `mailings.addressBlock` Address Block: {}
- `mailings.checkErrors` Check for Errors: {}
- `mailings.editRecipients` Edit Recipient List: {"rows"?: [{field: value}]}
- `mailings.envelopes` Envelopes: {"delivery": string, "return"?: string, "size"?: "Envelope #10|Envelope DL"}
- `mailings.findRecipient` Find Recipient: {}
- `mailings.finish` [file] Finish & Merge: {"path"?: string (save the merged document), "from"?: n, "to"?: n}
- `mailings.greetingLine` Greeting Line: {}
- `mailings.highlightFields` Highlight Merge Fields: {}
- `mailings.insertField` Insert Merge Field: {"field": string}
- `mailings.labels` Labels: {"text"?: string, "rows"?: n, "cols"?: n, "fromRecipients"?: bool}
- `mailings.matchFields` Match Fields: {}
- `mailings.next` Next Record: {}
- `mailings.preview` Preview Results: {}
- `mailings.previous` Previous Record: {}
- `mailings.recipients` [file] Select Recipients: {"csv"?: string, "path"?: string, "rows"?: [{field: value}]}
- `mailings.rules` Rules: {"rule": "IF|SKIPIF|NEXT|MERGEREC", "field"?, "value"?, "then"?, "else"?}
- `mailings.start` Start Mail Merge: {"kind": "letters|emails|envelopes|labels|directory"}

## para

- `para.addSpaceBefore` Add Space Before Paragraph: {}
- `para.align` Alignment: {"value": "left|center|right|justify|distribute"}
- `para.alignCenter` Center: {}
- `para.alignLeft` Align Left: {}
- `para.alignRight` Align Right: {}
- `para.borders` Borders: {"kind": "bottom|top|left|right|none|all|outside|inside|horizontalLine", "width"?: pt, "color"?: "RRGGBB", "style"?: "single|double|dotted|dashed|thick"}
- `para.bullets` Bullets: {"kind"?: "bullet" | single char, "off"?: bool}
- `para.defineBullet` Define New Bullet: {"char": string}
- `para.defineNumber` Define New Number Format: {"format": "decimal|upperRoman|lowerLetter…", "text"?: "%1.", "start"?: n}
- `para.dialog` [host] Paragraph Settings: {}
- `para.distribute` Distributed: {}
- `para.double` Double Spacing: {}
- `para.hangingIndent` Hanging Indent: {}
- `para.heading1` Heading 1: {}
- `para.heading2` Heading 2: {}
- `para.heading3` Heading 3: {}
- `para.indent` Increase Indent: {}
- `para.indents` Indents: {"left"?: pt, "right"?: pt, "firstLine"?: pt (negative = hanging)}
- `para.justify` Justify: {}
- `para.keepLines` Keep Lines Together: {}
- `para.keepNext` Keep with Next: {}
- `para.lineSpacing` Line and Paragraph Spacing: {"value": number (multiple) } | {"atLeast": pt} | {"exactly": pt}
- `para.listLevel` Change List Level: {"level": 0-8}
- `para.multilevel` Multilevel List: {"kind"?: "legal|outline"}
- `para.normal` Normal Style: {}
- `para.numbering` Numbering: {"kind"?: "numbered|numberedParen|upperLetter|lowerLetter|lowerRoman|outline", "off"?: bool}
- `para.oneAndHalf` 1.5 Line Spacing: {}
- `para.outdent` Decrease Indent: {}
- `para.outlineLevel` Outline Level: {}
- `para.pageBreakBefore` Page Break Before: {}
- `para.removeHanging` Reduce Hanging Indent: {}
- `para.removeSpaceAfter` Remove Space After Paragraph: {}
- `para.restartNumbering` Restart at 1: {}
- `para.rtl` Right-to-Left Text Direction: {}
- `para.set` Set Paragraph Formatting: {"props": ParaProps}
- `para.setNumberingValue` Set Numbering Value: {"value": n}
- `para.shading` Shading: {"color": "RRGGBB" | null}
- `para.single` Single Spacing: {}
- `para.sort` Sort: {"descending"?: bool}
- `para.spacing` Paragraph Spacing: {"before"?: pt, "after"?: pt}
- `para.style` Apply Style: {"style": string (name or id)}
- `para.tabs` Tabs: {"tabs": [{"pos": pt, "align": "left|center|right|decimal|bar", "leader": "none|dot|hyphen|underscore"}]}
- `para.widowControl` Widow/Orphan Control: {}

## picture

- `picture.altText` Alt Text: {"text": string}
- `picture.border` Picture Border: {"color"?: "RRGGBB", "width"?: px}
- `picture.change` [file] Change Picture: {"path"?: string, "data"?: base64}
- `picture.color` Color: {"mode": "grayscale|sepia|washout|blackAndWhite|saturation|tint", "saturation"?: 0..400}
- `picture.compress` Compress Pictures: {"maxPixels"?: n}
- `picture.corrections` Corrections: {"brightness"?: -100..100, "contrast"?: -100..100, "sharpen"?: -100..100}
- `picture.crop` Crop: {"left"?, "top"?, "right"?, "bottom"? (fractions 0–0.45)}
- `picture.effects` Artistic Effects: {"effect": "blur|sharpen|invert|posterize|pixelate"}
- `picture.removeBackground` Remove Background: {"tolerance"?: 0..255}
- `picture.reset` Reset Picture: {}
- `picture.size` Size: {"width"?: pt, "height"?: pt, "lockAspect"?: bool, "scale"?: percent}
- `picture.style` Picture Styles: {"style": "simpleFrame|thickFrame|rounded|softEdge|shadow"}
- `picture.transparency` Transparency: {"percent": 0..100}

## references

- `references.addText` Add Text: {"level": 0 (do not show) | 1-9}
- `references.bibliography` Bibliography: {"title"?: string}
- `references.caption` Insert Caption: {"label"?: "Figure|Table|Equation", "text"?: string}
- `references.citation` Insert Citation: {"tag"?: string, "source"?: Source (added if new), "pages"?: string}
- `references.citationStyle` Style: {"style": "APA|MLA|Chicago|IEEE"}
- `references.endnote` Insert Endnote: {"text"?: string}
- `references.footnote` Insert Footnote: {"text"?: string}
- `references.index` Insert Index: {}
- `references.markCitation` Mark Citation: {"entry"?: string}
- `references.markEntry` Mark Entry: {"entry"?: string}
- `references.nextFootnote` Next Footnote: {}
- `references.noteOptions` Footnote and Endnote: {"footnoteFormat"?: "decimal|lowerRoman|upperRoman|lowerLetter|upperLetter", "endnoteFormat"?: …}
- `references.notes` Show Notes: {}
- `references.removeToc` Remove Table of Contents: {}
- `references.researcher` [network] Researcher: {}
- `references.sources` Manage Sources: {"add"?: Source, "remove"?: tag} → list
- `references.tableOfAuthorities` Insert Table of Authorities: {}
- `references.tableOfFigures` Insert Table of Figures: {"label"?: "Figure|Table|Equation"}
- `references.toc` Table of Contents: {"levels"?: 1-9, "title"?: string}
- `references.updateFields` Update Field: {}
- `references.updateFigures` Update Table: {}
- `references.updateIndex` Update Index: {}
- `references.updateToc` Update Table: {}

## review

- `review.accept` Accept: {}
- `review.acceptAll` Accept All Changes: {}
- `review.addToDictionary` [host] Add to Dictionary: {"word"?: string}
- `review.applySuggestion` Change: {"text": string}
- `review.changes` Reviewing Pane: {}
- `review.combine` [file] Combine: {"path"?: string}
- `review.comments` Show Comments: {}
- `review.compare` [file] Compare: {"path"?: string, "text"?: string (revised version)}
- `review.deleteComment` Delete: {"id"?: n, "all"?: bool}
- `review.editor` Editor: {}
- `review.ignoreAll` [host] Ignore All: {}
- `review.issues` Proofing Issues: {}
- `review.language` Language: {"lang": "en-US|en-GB|fr-FR|…", "noProof"?: bool}
- `review.markup` [host] Display for Review: {}
- `review.newComment` New Comment: {"text": string}
- `review.nextChange` Next Change: {}
- `review.nextComment` Next: {}
- `review.previousChange` Previous Change: {}
- `review.previousComment` Previous: {}
- `review.proofing` [host] Check Spelling as You Type: {}
- `review.readAloud` [device] Read Aloud: {}
- `review.reject` Reject: {}
- `review.rejectAll` Reject All Changes: {}
- `review.reply` Reply: {"id": n, "text": string}
- `review.resolveComment` Resolve: {}
- `review.restrict` Restrict Editing: {"mode": "none|readOnly|comments|trackedChanges|forms"}
- `review.showMarkup` [host] Show Markup: {}
- `review.spelling` Spelling & Grammar: {}
- `review.suggestions` Spelling Suggestions: {"pos"?: Pos}
- `review.thesaurus` Thesaurus: {}
- `review.trackChanges` Track Changes: {}
- `review.wordCount` Word Count: {}

## select

- `select.all` Select All: {}
- `select.collapse` Collapse Selection: {}
- `select.extend` Extend Selection: {}
- `select.line` Select Line: {}
- `select.objects` Select Objects: {"index"?: n}
- `select.paragraph` Select Paragraph: {}
- `select.range` Select Range: {"anchor": Pos, "focus": Pos}
- `select.sentence` Select Sentence: {}
- `select.similar` Select Text with Similar Formatting: {}
- `select.text` Select Text: {"text": string, "occurrence"?: n}
- `select.word` Select Word: {}

## shape

- `shape.change` Change Shape: {"kind": string}
- `shape.fill` Shape Fill: {"color": "RRGGBB" | null}
- `shape.outline` Shape Outline: {"color": "RRGGBB" | null, "width"?: pt}

## styles

- `styles.addToGallery` Add to Style Gallery: {}
- `styles.create` Create a Style: {"name": string, "basedOn"?: string, "fromSelection"?: bool}
- `styles.delete` Delete Style: {}
- `styles.list` Styles: {}
- `styles.modify` Modify Style: {"style": string, "chr"?: CharProps, "para"?: ParaProps, "name"?: string, "next"?: string}
- `styles.pane` [host] Styles Pane: {}
- `styles.updateToMatch` Update Style to Match Selection: {"style"?: string}

## table

- `table.autofit` AutoFit: {"mode": "contents|window|fixed"}
- `table.borderPainter` Border Painter: {}
- `table.borders` Borders: {"kind": "all|outside|inside|none|top|bottom|left|right", "width"?: pt, "color"?: "RRGGBB"}
- `table.cellAlign` Alignment: {"value": "topLeft|topCenter|…|bottomRight"}
- `table.cellMargins` Cell Margins: {"top"?, "left"?, "bottom"?, "right"? (pt)}
- `table.columnWidth` Table Column Width: {"width": pt}
- `table.deleteCells` Delete Cells: {}
- `table.deleteColumn` Delete Columns: {}
- `table.deleteRow` Delete Rows: {}
- `table.deleteTable` Delete Table: {}
- `table.distributeColumns` Distribute Columns: {}
- `table.distributeRows` Distribute Rows: {}
- `table.formula` Formula: {"formula"?: "=SUM(ABOVE)"}
- `table.fromText` Convert Text to Table: {"separator"?: "tab|comma"}
- `table.insertColumnLeft` Insert Left: {}
- `table.insertColumnRight` Insert Right: {}
- `table.insertRowAbove` Insert Above: {}
- `table.insertRowBelow` Insert Below: {}
- `table.look` Table Style Options: {"headerRow"?: bool, "totalRow"?: bool, "bandedRows"?: bool, "firstColumn"?: bool, "lastColumn"?: bool, "bandedColumns"?: bool}
- `table.merge` Merge Cells: {}
- `table.properties` Properties: {"align"?: "left|center|right"}
- `table.quick` Quick Tables: {"kind"?: "calendar|tabular|matrix"}
- `table.repeatHeader` Repeat Header Rows: {}
- `table.rowHeight` Table Row Height: {"height": pt}
- `table.selectCell` Select Cell: {}
- `table.selectRow` Select Row: {}
- `table.selectTable` Select Table: {}
- `table.shading` Shading: {"color": "RRGGBB" | null}
- `table.sort` Sort: {"column"?: n, "descending"?: bool, "header"?: bool}
- `table.split` Split Cells: {"columns"?: n}
- `table.splitTable` Split Table: {}
- `table.style` Table Styles: {"style": string}
- `table.textDirection` Text Direction: {}
- `table.toText` Convert to Text: {"separator"?: "tab|comma|paragraph"}

## text

- `text.backTab` Shift+Tab: {}
- `text.backspace` Backspace: {}
- `text.columnBreak` Column Break: {}
- `text.delete` Delete: {}
- `text.deleteWordBack` Delete Previous Word: {}
- `text.deleteWordForward` Delete Next Word: {}
- `text.insert` Type Text: {"text": string}
- `text.lineBreak` Line Break: {}
- `text.nbHyphen` Nonbreaking Hyphen: {}
- `text.nbsp` Nonbreaking Space: {}
- `text.newParagraph` New Paragraph: {}
- `text.optionalHyphen` Optional Hyphen: {}
- `text.pageBreak` Page Break: {}
- `text.tab` Tab: {}

## tools

- `tools.autocorrect` [host] AutoCorrect Options: {"enabled"?: bool, "add"?: {"from": string, "to": string}}
- `tools.macros` [code] Macros: {"run"?: name, "define"?: {"name": string, "steps": [{"command", "params"}]}, "delete"?: name}
- `tools.recordMacro` [code] Record Macro: {"name"?: string} (call again to stop)

## view

- `view.commentsPane` [host] Comments Pane: {}
- `view.darkMode` [host] Switch Modes: {}
- `view.draft` [host] Draft: {}
- `view.focus` [host] Focus: {}
- `view.gridlines` [host] Gridlines: {}
- `view.immersive` [host] Immersive Reader: {}
- `view.marks` [host] Show/Hide ¶: {}
- `view.multiplePages` [host] Multiple Pages: {}
- `view.navigationPane` [host] Navigation Pane: {}
- `view.newWindow` [host] New Window: {}
- `view.onePage` [host] One Page: {}
- `view.outline` [host] Outline: {}
- `view.pageWidth` [host] Page Width: {}
- `view.printLayout` [host] Print Layout: {}
- `view.readMode` [host] Read Mode: {}
- `view.ruler` [host] Ruler: {}
- `view.sideToSide` [host] Side to Side: {}
- `view.split` [host] Split: {}
- `view.state` [host] View State: {}
- `view.stylesPane` [host] Styles Pane: {}
- `view.vertical` [host] Vertical: {}
- `view.webLayout` [host] Web Layout: {}
- `view.zoom` [host] Zoom: {"value": percent (10-500) | "pageWidth" | "onePage" | "multiplePages"}
- `view.zoom100` [host] 100%: {}
- `view.zoomIn` [host] Zoom In: {}
- `view.zoomOut` [host] Zoom Out: {}
