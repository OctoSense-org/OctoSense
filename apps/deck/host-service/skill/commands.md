# deck engine commands

The deckcraft engine's command catalog at revision d0e57d7e25f9: 222 commands, one per line as `id` label: params.
Generated from the engine by `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-deck-service --test skill`; do not edit.

A tag after the id marks a command that reaches past the open document (safety.json has every id's class): [file] reads or writes a path, [code] installs or runs code or other commands, [network] uses the network, [device] uses hardware, [host] changes windows, views, preferences, the clipboard or other app state. An untagged command works on the open document only.

## animation

- `animation.add` Add Animation: {effect, class?, option?, start?: onClick|withPrevious|afterPrevious, duration?: ms, delay?: ms, ids?}
- `animation.clear` Remove All Animations: {index?: slide}
- `animation.get` Animations on Slide: {slide?} → [animation]
- `animation.list` Animation Gallery: {} → [{id, label, class, options}]
- `animation.move` Reorder Animation: {index, to}
- `animation.options` Effect Options: {option?, textBuild?: asOne|byParagraph|byWord|byLetter, index?}
- `animation.remove` Remove Animation: {index} | {ids?} (all of the shapes' animations)
- `animation.set` Animation Styles: {effect: fade|fly|zoom|wipe|appear|spin|pulse|fadeOut|lines|… (animation.list), class?: entrance|emphasis|exit|path, option?, ids?} — replaces the shapes' animations
- `animation.timing` Timing: {index? (else selected shapes), start?: onClick|withPrevious|afterPrevious, duration?: ms, delay?: ms, repeat?: n, rewind?: bool, trigger?: shape id | null}

## arrange

- `arrange.align` Align: {edge: left|center|right|top|middle|bottom, to?: slide|selection (default: slide for one object), ids?}
- `arrange.bringForward` Bring Forward: {ids?}
- `arrange.bringToFront` Bring to Front: {ids?}
- `arrange.distribute` Distribute: {dir: horizontal|vertical, to?: slide|selection, ids?}
- `arrange.flipHorizontal` Flip Horizontal: {ids?}
- `arrange.flipVertical` Flip Vertical: {ids?}
- `arrange.group` Group: {ids?}
- `arrange.nudge` Nudge: {dx, dy, ids?} (arrow keys: 6 pt, with Alt 1 pt)
- `arrange.regroup` Regroup: {}
- `arrange.reorder` Reorder Objects: {id, index: position in the stack (0 = back)}
- `arrange.rotateLeft` Rotate Left 90°: {ids?}
- `arrange.rotateRight` Rotate Right 90°: {ids?}
- `arrange.sendBackward` Send Backward: {ids?}
- `arrange.sendToBack` Send to Back: {ids?}
- `arrange.ungroup` Ungroup: {ids?}

## chart

- `chart.data` Edit Data: {categories?: [..], series?: [{name, values}], id?}
- `chart.options` Chart Elements: {title?: string|null, legend?: b|t|l|r|null, dataLabels?: bool, gridlines?: bool, palette?, id?}
- `chart.type` Change Chart Type: {type, id?}

## commands

- `commands.list` List Commands: {filter?: substring} → [{id, label, params, enabled}]

## comment

- `comment.add` New Comment: {text, x?, y?, id?: shape}
- `comment.delete` Delete Comment: {index? | all?: bool}
- `comment.list` Comments: {slide?} → comments
- `comment.reply` Reply: {index, text}
- `comment.resolve` Resolve Thread: {index, resolved?: bool}

## design

- `design.background` [file] Format Background: {color? | gradient? | picture?: base64 | style?: 1..12 | reset?, all?: bool (apply to all), index?}
- `design.colors` Colors: {name} | {colors: {accent1: #RRGGBB, …}}
- `design.fonts` Fonts: {name} | {major, minor}
- `design.headerFooter` Header and Footer…: {date?: bool, dateText?: fixed text, slideNumber?: bool, footer?: bool, footerText?, hideOnTitle?: bool, all?: bool (default true)}
- `design.hideBackgroundGraphics` Hide Background Graphics: {hide?: bool}
- `design.slideSize` Slide Size: {preset?: widescreen|standard|… | w, h: pt, scale?: maximize|ensureFit|none}
- `design.theme` Themes: {name} (see design.themes)
- `design.themes` Theme Gallery: {} → themes, colour schemes and font schemes

## document

- `document.inspect` Inspect Presentation: {} → slides with titles, layouts, shape counts; selection; size; theme

## edit

- `edit.copy` Copy: {scope?: slides} → {text}
- `edit.cut` Cut: {scope?: slides}
- `edit.delete` Delete: {ids?, scope?: slides}
- `edit.deselect` Deselect: {}
- `edit.duplicate` Duplicate: {ids?}
- `edit.paste` Paste: {text?: plain text to paste, scope?: slides}
- `edit.pasteText` Paste and Match Formatting: {text?}
- `edit.redo` Redo: {}
- `edit.select` Select Objects: {ids: [id], add?: bool, toggle?: bool}
- `edit.selectAll` Select All: {}
- `edit.undo` Undo: {}

## file

- `file.close` [file] Close: {}
- `file.export` [file] Export…: {path, format: png|jpeg|pptx|deckcraft|outline|pdf, slide?: index, all?: bool, scale?: px per pt; pdf: layout?: slides|notes|handouts, perPage?: 1|2|3|4|6|9, dpi?, slides?: [index], includeHidden?, textLayer?, frame?}
- `file.new` New Presentation: {theme?: name, size?: [w,h] pt, blank?: bool}
- `file.open` [file] Open…: {path}
- `file.openBytes` Open Data: {name, data: base64}
- `file.properties` Properties: {title?, subject?, author?, keywords?, comments?, category?, company?} → properties
- `file.recovery.discard` [file] Discard Recovered Presentations: {uid?: one entry, else all}
- `file.recovery.list` [file] Recovered Presentations: {} → [{uid, path, title, saved}]
- `file.recovery.open` [file] Open Recovered Presentations: {} → {opened: [document index]}
- `file.recovery.save` [file] Save AutoRecover Information: {} → {written}
- `file.render` Render Slide: {slide?: index, scale?, edit?: bool} → {png: base64, width, height}
- `file.revert` Revert: {}
- `file.save` [host] Save: {path?, format?: deckcraft|pptx}
- `file.saveAs` [host] Save As…: {path, format?}
- `file.saveBytes` Save to Data: {format?: deckcraft|pptx} → {data: base64}
- `file.saveTemplate` [host] Save as Template…: {path}

## format

- `format.align` Align: {align: left|center|right|justify|distributed}
- `format.alignCenter` Center: {}
- `format.alignLeft` Align Left: {}
- `format.alignRight` Align Right: {}
- `format.anchor` Align Text: {anchor: top|middle|bottom}
- `format.autofit` Autofit: {mode: none|shrink|resize}
- `format.bold` Bold: {on?: bool}
- `format.bullets` Bullets: {on?: bool, char?: •|○|■|□|◆|➢|✓|–, color?, size?: % of text}
- `format.caps` Caps: {caps: none|small|all}
- `format.case` Change Case: {mode: sentence|lower|upper|title|toggle}
- `format.clear` Clear All Formatting: {}
- `format.color` Font Color: {color: #RRGGBB | accent1..6 | tx1… | {scheme, lumMod…}}
- `format.columns` Columns: {count, spacing?: pt}
- `format.direction` Text Direction: {dir: horizontal|rotate90|rotate270|stacked}
- `format.font` Font: {family}
- `format.grow` Increase Font Size: {}
- `format.highlight` Text Highlight Color: {color? (absent = none)}
- `format.indent` Increase List Level: {}
- `format.italic` Italic: {on?: bool}
- `format.justify` Justify: {}
- `format.lineSpacing` Line Spacing: {lines?: 1.0|1.5|2.0…, pt?: exactly}
- `format.margins` Text Box Margins: {left?, top?, right?, bottom?: pt}
- `format.numbering` Numbering: {on?: bool, scheme?: arabicPeriod|arabicParenR|romanUcPeriod|romanLcPeriod|alphaUcPeriod|alphaLcParenR|alphaLcPeriod, start?: n}
- `format.outdent` Decrease List Level: {}
- `format.painter` Format Painter: {sticky?: bool}
- `format.painterApply` Paste Formatting: {ids?}
- `format.paragraph` Paragraph…: {align?, indentLeft?: pt, indentFirst?: pt (negative = hanging), spaceBefore?: pt, spaceAfter?: pt, lineSpacing?: lines}
- `format.shrink` Decrease Font Size: {}
- `format.size` Font Size: {size: pt}
- `format.spacing` Character Spacing: {pt} (veryTight -3, tight -1.5, normal 0, loose 3, veryLoose 6)
- `format.state` Formatting State: {} → the effective formatting at the selection (font, size, bold, …)
- `format.strikethrough` Strikethrough: {on?: bool, double?: bool}
- `format.subscript` Subscript: {on?: bool}
- `format.superscript` Superscript: {on?: bool}
- `format.textOutline` Text Outline: {color?, width?: pt} (no color = none)
- `format.textShadow` Text Shadow: {on?: bool}
- `format.underline` Underline: {on?: bool, style?: sng|dbl|heavy|dotted|dash|wavy}
- `format.wrap` Wrap Text in Shape: {on: bool}

## insert

- `insert.actionButton` Action Buttons: {kind: back|forward|beginning|end|home|information|return|movie|document|sound|help|custom, rect?}
- `insert.audio` [file] Audio from File…: {path? | data: base64, name?, rect?}
- `insert.chart` Chart…: {type?: column|bar|line|pie|doughnut|area|scatter|stackedColumn|…, rect?, categories?: [..], series?: [{name, values}]}
- `insert.dateTime` Date & Time: {format?: datetime1..}
- `insert.hyperlink` Link: {url? | slide?: index, tooltip?, ids?}
- `insert.picture` [file] Picture from File…: {path? | data: base64, name?, rect?: [x,y,w,h]}
- `insert.slideNumber` Slide Number: {}
- `insert.smartArt` SmartArt: {kind: list|process|cycle|hierarchy|pyramid|matrix, items: [text]}
- `insert.symbol` Symbol: {text: character(s)}
- `insert.table` Table…: {rows, cols, rect?}
- `insert.textBox` Text Box: {rect?: [x,y,w,h], text?: string}
- `insert.video` [file] Video from File…: {path? | data: base64, name?, rect?}
- `insert.wordArt` WordArt: {text?, style?: 0..}

## master

- `master.deleteLayout` Delete Layout: {master?, layout?}
- `master.insertLayout` Insert Layout: {name?}
- `master.insertPlaceholder` Insert Placeholder: {kind: content|text|picture|chart|table|media, rect?: [x,y,w,h]}
- `master.renameLayout` Rename Layout: {name, master?, layout?}

## media

- `media.info` Media Info: {id?} → {id, name, contentType, bytes, durationMs, video, width, height, probe: {container, audio?, video?}, options, status?}
- `media.options` Playback: {autoplay?, loop?, rewind?, acrossSlides?, hide?, fullScreen?, volume?: 0..1, trimStart?: ms, trimEnd?: ms, fadeIn?: ms, fadeOut?: ms, ids?}
- `media.pause` [device] Pause: {id?}
- `media.play` [device] Play: {id?, from?: ms}
- `media.posterFrame` Poster Frame: {id?, ms?: frame time (default: current position)}
- `media.seek` [device] Seek: {id?, ms}
- `media.stop` [device] Stop: {id?}
- `media.toggle` [device] Play/Pause: {id?}

## picture

- `picture.adjust` Corrections: {brightness?: -1..1, contrast?: -1..1, saturation?: 0..4, grayscale?: bool, transparency?: 0..1, reset?: bool, ids?}
- `picture.change` [file] Change Picture: {path? | data: base64, name?, ids?}
- `picture.crop` Crop: {left?, top?, right?, bottom?: fraction 0..1, ids?}
- `picture.reset` Reset Picture: {ids?}

## review

- `review.accessibility` Check Accessibility: {} → [{slide, shape, issue}]
- `review.spelling` Spelling: {} → [{slide, shape, word}] (words not in the built-in list)

## section

- `section.add` Add Section: {name?, at?: slide index}
- `section.move` Move Section: {index, to}
- `section.remove` Remove Section: {index, slides?: bool (also delete its slides)}
- `section.removeAll` Remove All Sections: {}
- `section.rename` Rename Section: {index, name}

## shape

- `shape.adjust` Adjust Shape: {index, value (file units), ids?}
- `shape.altText` Alt Text: {text, decorative?: bool, ids?}
- `shape.change` Change Shape: {preset, ids?}
- `shape.connect` Connect Shapes: {from: shape id, to: shape id, fromSite?, toSite?: site index (default: the closest pair), id?: existing line to glue (else a new connector), preset?: straightConnector1|bentConnector3|curvedConnector3} → {id}
- `shape.effects` Shape Effects: {shadow?: none|outer|inner|perspective|{color, blur, dist, dir}, glow?: none|{color, radius}, softEdges?: pt, reflection?: none|tight|half|full, bevel?: none|circle|…, reset?, ids?}
- `shape.fill` [file] Shape Fill: {color? | none?: true | gradient?: {stops: [[pos, color]], angle?, kind?: linear|radial} | picture?: base64 | pattern?: {preset, fg, bg} | transparency?: 0..1 | reset?: true, ids?}
- `shape.flip` Flip: {axis: horizontal|vertical, ids?}
- `shape.freeform` Freeform: {points: [[x, y], …] slide pt (≥ 2), closed?: bool (filled shape), smooth?: bool (curve through the points)} → {id}
- `shape.insert` Shapes: {preset: rect|roundRect|ellipse|triangle|rightArrow|star5|… (see shape.presets), rect: [x,y,w,h] pt, text?: string}
- `shape.inspect` Inspect Shape: {id} → full shape JSON plus its effective box
- `shape.line` Shape Outline: {color?, none?, width?: pt, dash?: solid|dot|dash|lgDash|dashDot|sysDot|sysDash, cap?, join?, head?: none|triangle|stealth|diamond|oval|arrow, tail?, reset?, ids?}
- `shape.lock` Lock: {locked?: bool, ids?}
- `shape.merge` Merge Shapes: {op: union|combine|fragment|intersect|subtract, ids?: shapes in selection order (the first one's formatting is kept)} → {ids}
- `shape.move` Move: {dx?, dy?: pt (relative) | x?, y?: pt (absolute), ids?}
- `shape.presets` Shape Gallery: {} → [{name, label, category}]
- `shape.quickStyle` Quick Styles: {index: 0..41 (theme style row×accent), ids?}
- `shape.rename` Rename: {name, id?}
- `shape.resize` Size: {w?, h?: pt, lockAspect?: bool, ids?}
- `shape.rotate` Rotation: {deg?: absolute, by?: relative, ids?}
- `shape.setBounds` Position and Size: {x, y, w, h, ids?}
- `shape.setDefault` Set as Default Shape: {}
- `shape.sites` Connection Sites: {id} → [[x, y]] connection sites in slide points
- `shape.visible` Show/Hide: {visible?: bool, ids?}

## show

- `show.customShow` Custom Slide Show: {name, slides: [index]} | {name, delete: true}
- `show.fromCurrent` [device] Play from Current Slide: {}
- `show.fromStart` [device] Play from Start: {}
- `show.setup` Set Up Slide Show…: {type?: speaker|browsed|kiosk, loop?: bool, noNarration?, noAnimation?, useTimings?, from?, to?}

## slide

- `slide.delete` Delete Slide: {index?, ids?: [slide id]}
- `slide.duplicate` Duplicate Slide: {index?}
- `slide.first` First Slide: {}
- `slide.fromOutline` Slides from Outline…: {text}
- `slide.go` Go to Slide: {index} | {id}
- `slide.hide` Hide Slide: {index?, hidden?: bool}
- `slide.inspect` Inspect Slide: {index?} → the slide's shapes with ids, kinds, boxes, text, fills, animations, transition, notes
- `slide.last` Last Slide: {}
- `slide.layout` Layout: {layout: name or kind, index?}
- `slide.move` Move Slide: {from?: index, to: index}
- `slide.new` New Slide: {layout?: layout name or kind (title, titleAndContent, sectionHeader, twoContent, comparison, titleOnly, blank, contentWithCaption, pictureWithCaption), at?: index, title?: text, body?: text}
- `slide.next` Next Slide: {}
- `slide.notes` Notes: {text, index?}
- `slide.previous` Previous Slide: {}
- `slide.rename` Rename Slide: {name, index?}
- `slide.reset` Reset: {index?}
- `slide.selectSlides` Select Slides: {ids?: [slide id], indices?: [index], add?: bool}

## table

- `table.cellFill` Shading: {color? | none?, cells?: [[r,c],…] (default: selected cells or all), id?}
- `table.columnWidth` Width: {col, width: pt, id?}
- `table.deleteColumn` Delete Columns: {col?, id?}
- `table.deleteRow` Delete Rows: {row?, id?}
- `table.distributeColumns` Distribute Columns: {id?}
- `table.distributeRows` Distribute Rows: {id?}
- `table.insertColumnLeft` Insert Left: {col?, id?}
- `table.insertColumnRight` Insert Right: {col?, id?}
- `table.insertRowAbove` Insert Above: {row?, id?}
- `table.insertRowBelow` Insert Below: {row?, id?}
- `table.merge` Merge Cells: {from: [r, c], to: [r, c], id?}
- `table.options` Table Style Options: {headerRow?, totalRow?, bandedRows?, firstColumn?, lastColumn?, bandedColumns?: bool, id?}
- `table.rowHeight` Height: {row, height: pt, id?}
- `table.selectCells` Select Cells: {from: [r,c], to: [r,c], id?}
- `table.split` Split Cells: {cell: [r, c], id?}
- `table.style` Table Styles: {style: medium2-accent1|light1-accent2|dark1-tx1|grid|none…, id?}

## text

- `text.delete` Delete Text: {dir?: backward|forward|wordBackward|wordForward}
- `text.edit` Edit Text: {id?, at?: [paragraph, char], end?: bool, cell?: [row, col], notes?: bool}
- `text.exit` Stop Editing Text: {}
- `text.get` Get Text: {id?} → {text, paragraphs}
- `text.insert` Type Text: {text} (\n = new paragraph, \u000b = line break)
- `text.move` Move Insertion Point: {to: left|right|up|down|wordLeft|wordRight|lineStart|lineEnd|paraStart|paraEnd|start|end, extend?: bool}
- `text.select` Select Text: {anchor: [p, c], caret: [p, c]}
- `text.selectAll` Select All Text: {}
- `text.selectParagraph` Select Paragraph: {at?: [p, c]}
- `text.selectWord` Select Word: {at?: [p, c]}
- `text.set` Set Text: {id?, text, cell?: [r, c]} — replaces the whole text, keeping the first run's formatting

## transition

- `transition.applyAll` Apply To All: {}
- `transition.list` Transition Gallery: {} → [{id, label, category, duration, options}]
- `transition.options` Effect Options: {option, index?}
- `transition.set` Transition: {kind: none|morph|fade|push|wipe|split|reveal|cut|randomBar|shape|uncover|cover|flash|… (transition.list), option?, duration?: ms, index?}
- `transition.timing` Timing: {duration?: ms, onClick?: bool, after?: ms | null, index?}

## view

- `view.closeMaster` Close Master View: {}
- `view.slideMaster` Slide Master: {master?: index, layout?: index}

## window

- `window.next` Next Window: {index?}
