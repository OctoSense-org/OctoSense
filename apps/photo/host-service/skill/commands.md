# photo engine commands

The photocraft engine's command catalog at revision eec4af65513b: 817 commands, one per line as `id` label: params.
Generated from the engine by `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-photo-service --test skill`; do not edit.

A tag after the id marks a command that reaches past the open document (safety.json has every id's class): [file] reads or writes a path, [code] installs or runs code or other commands, [network] uses the network, [device] uses hardware, [host] changes windows, views, preferences, the clipboard or other app state. An untagged command works on the open document only.

## actions

- `actions.delete` Delete Action: {"action":name|index} → {deleted:name}. Refused while recording.
- `actions.get` Get Action: {"action":name|index} → {name, steps:[[id, params]…]} (the shape file.automate.batch and droplets take)
- `actions.list` List Actions: {} → {actions:[{name, steps:count}], recording:name|null}
- `actions.play` [code] Play Action: {"action":name|index, "from":step?} → {action, ran, failed?:{step, id, error}}. step and from are 0-based. Stops at the first error (the command still returns ok, with failed set) and leaves one history step per step that ran. Refuses to play while a play is already running. Each step is checked with Session::authorize when one is installed.
- `actions.record` Record Action: {"name":str? (new action, default "Action N")} or {"action":name|index} (append) → {action, index}
- `actions.stop` Stop Recording: {} → {action, steps:count} (steps recorded since actions.record, queries and actions.* omitted)

## brush

- `brush.defineFromSelection` Define Brush Preset…: {"name":string}
- `brush.get` Get Brush: {}
- `brush.presets.delete` Delete Brush Preset: {"name":string}
- `brush.presets.deleteGroup` Delete Brush Group: {"group":string} → {deleted, count}
- `brush.presets.importAbr` [file] Import Brushes…: {"path":".abr file"?,"data":base64 bytes?,"group":string?=file name,"replace":bool=true (replace a group of the same name),"select":bool=false (make the first imported preset current)}
- `brush.presets.list` List Brush Presets: {"full":bool=false}
- `brush.presets.move` Move Brush: {"name":string,"group":string?=its group ("" = ungrouped),"before":name? (preset to land before) | "index":n? (position in the group)=end} → {name, group, index}
- `brush.presets.moveGroup` Move Brush Group: {"group":string,"before":group? | "index":n? (group position)=end} → {group, index}
- `brush.presets.rename` Rename Brush: {"name":string,"newName":string} → {name}
- `brush.presets.renameGroup` Rename Brush Group: {"group":string,"newName":string} → {group}
- `brush.presets.save` Save Brush Preset: {"name":string,"brush":{…BrushSettings}?=current brush}
- `brush.texturePattern` Brush Texture Pattern: {"pattern":id|name,"scale":1..1000?,"enabled":bool=true} (sets the session brush's Texture to that pattern)

## channel

- `channel.delete` Delete Channel: {"channel":index|name?=targeted}
- `channel.duplicate` Duplicate Channel…: {"channel":index|name|"red|…"|"composite"|"quickMask","name":str?,"document":index|"new"?=active,"invert":bool=false}
- `channel.list` Channels: {}
- `channel.merge` Merge Channels…: {"mode":"rgb|cmyk|lab"="rgb","documents":[index,…]?=open grayscale docs of the active size,"name":str?}
- `channel.mergeSpot` Merge Spot Channel: {"channel":index|name?=targeted or first spot}
- `channel.move` Reorder Channel: {"channel":index|name,"to":index}
- `channel.new` New Channel…: {"name":str?,"fill":"black|white|selection"="black","color":"#rrggbb"="#ff0000","opacity":0..100=50,"indicates":"masked|selected"="masked"}
- `channel.newSpot` New Spot Channel…: {"name":str?,"color":"#rrggbb"="#ff0000","solidity":0..100=0,"fromSelection":bool=true}
- `channel.options` Channel Options…: {"channel":index|name|"quickMask","name":str?,"indicates":"masked|selected|spot"?,"color":"#rrggbb"?,"opacity":0..100?,"solidity":0..100?}
- `channel.rename` Rename Channel: {"channel":index|name,"name":str}
- `channel.setVisible` [host] Channel Visibility: {"channel":"composite"|"red|…"|index|name|"quickMask","visible":bool?=toggle}
- `channel.split` Split Channels: {"closeOriginal":bool=true}
- `channel.target` Target Channel: {"channel":"composite"|"red|green|blue|…"|index|name="composite"}
- `channel.target.composite` Target Composite Channel: {}
- `channel.target.slot3` Target Channel 3: {}
- `channel.target.slot4` Target Channel 4: {}
- `channel.target.slot5` Target Channel 5: {}
- `channel.target.slot6` Target Channel 6: {}
- `channel.target.slot7` Target Channel 7: {}
- `channel.target.slot8` Target Channel 8: {}
- `channel.target.slot9` Target Channel 9: {}

## cloneSource

- `cloneSource.list` Clone Sources: {} → {active,sources:[{index,source,anchor,offset,layer,width,height,rotation,flipH,flipV}],overlay}
- `cloneSource.overlay` [host] Clone Source Overlay: {"show":bool?,"opacity":0..100?,"clipped":bool?,"autoHide":bool?,"invert":bool?,"blend":"normal|darken|lighten|difference"?}
- `cloneSource.resetTransform` Reset Transform: {"index":0..4?=active}
- `cloneSource.select` Select Clone Source: {"index":0..4}
- `cloneSource.set` Set Clone Source: {"index":0..4?=active,"source":[x,y]? (⌥-click; re-pairs with the next stroke),"offset":[dx,dy]?,"width":%?,"height":%?|"scale":[w%,h%]?,"rotation":deg?,"flipH":bool?,"flipV":bool?,"layer":id?,"clear":bool?,"select":bool=true}

## color

- `color.profileMismatch` [file] Embedded Profile Mismatch: {"action":"preserve|convert|discard|assignWorking"}

## command

- `command.list` List Commands: {}

## count

- `count.add` Add Count: {"x":px,"y":px,"group":index?}
- `count.clear` Clear Count: {"group":index|"all"=active}
- `count.deleteGroup` Delete Count Group: {"group":index=active}
- `count.move` Move Count: {"index":n,"group":index?,"to":[x,y]} | {"x":px,"y":px,"to":[x,y]}
- `count.newGroup` New Count Group: {"name":str?,"color":"#rrggbb"|[r,g,b]?}
- `count.remove` Remove Count: {"index":n,"group":index?} | {"x":px,"y":px,"radius":px=6}
- `count.setGroup` Count Group Options: {"group":index=active,"name":str?,"color":"#rrggbb"|[r,g,b]?,"markerSize":1..10?,"labelSize":8..72?,"visible":bool?,"active":bool?}

## document

- `document.activate` Activate Document: {"document":index}
- `document.inspect` Inspect Document: {"document":index?}
- `document.move` Move Document: {"document":index?,"to":index}
- `document.pixel` Read Composite Pixel: {"x":i32,"y":i32}

## edit

- `edit.assignProfile` [file] Assign Profile…: {"profile":"working|srgb|display-p3|adobe-rgb-compat|prophoto-compat|linear-srgb|rec2020|gray-gamma-2.2|sgray|lab-d50|coated-cmyk|none" (or a path to an .icc file)}
- `edit.autoAlignLayers` Auto-Align Layers…: {"projection":"auto|perspective|cylindrical|spherical|collage|reposition","reference":layer id?=bottom selected layer,"geometricCorrection":bool=false,"interpolation":"bicubic|bilinear|nearest"}
- `edit.autoBlendLayers` Auto-Blend Layers…: {"method":"panorama|stack","seamlessTones":bool=true}
- `edit.checkSpelling` [host] Check Spelling…: {"action":"list|change|changeAll|addToDictionary|removeFromDictionary|suggest"="list","allLayers":bool=true,"layer":id? (check one layer),"ignore":[word]? (Ignore All for this check),"suggestions":n=5; change: "layer","start","end" (chars),"word"? (verified),"replace"; changeAll: "word","replace"; addToDictionary/removeFromDictionary/suggest: "word"} → list: {"misspellings":[{"layer","start","end","word","suggestions"}],"count"}
- `edit.clear` Clear: {}
- `edit.colorSettings` [host] Color Settings…: {"workingRgb":"srgb|display-p3|adobe-rgb-compat|prophoto-compat|linear-srgb|rec2020","workingCmyk":"coated-cmyk","workingGray":"sgray|gray-gamma-2.2","policyRgb":"preserve|convert|off","policyCmyk":"preserve|convert|off","policyGray":"preserve|convert|off","askOnMismatch":bool=true,"askOnPaste":bool=true,"askOnMissing":bool=false,"intent":"relative|perceptual|saturation|absolute","blendTextGamma":1.0..2.2|bool=1.45,"bpc":bool=true,"dither":bool=true,"monitorProfile":"auto|srgb|display-p3|adobe-rgb-compat|prophoto-compat|rec2020","reset":bool=false} (working spaces and the monitor profile also accept .icc paths; monitor `auto` = the main display's profile when the platform provides it, else sRGB; the reply's `monitorStatus` says which profile is in use and why: source auto|manual|fallback, reason)
- `edit.contentAwareFill` Content-Aware Fill…: {"sampling":"auto|rectangular|custom","margin":px?,"area":[x,y,w,h]?,"channel":index|name?,"colorAdaptation":"default|none|high|veryHigh","rotationAdaptation":"none|low|medium|high|full","scale":bool=false,"mirror":bool=false,"output":"current|new|duplicate","seed":u64=1}
- `edit.contentAwareScale` Content-Aware Scale: {"width":px?,"height":px?,"scaleX":1..400=100,"scaleY":1..400=100,"amount":0..100=100,"protect":"none"|channel index|name,"protectSkinTones":bool=false}
- `edit.convertToProfile` [file] Convert to Profile…: {"profile":"srgb|display-p3|adobe-rgb-compat|prophoto-compat|linear-srgb|rec2020|gray-gamma-2.2|sgray|lab-d50|coated-cmyk|working" (or a path to an .icc file),"intent":"perceptual|relative|saturation|absolute"="relative","bpc":bool=true}
- `edit.copy` Copy: {}
- `edit.copyMerged` Copy Merged: {}
- `edit.cut` Cut: {}
- `edit.defineBrushPreset` Define Brush Preset…: {"name":text}
- `edit.defineCustomShape` Define Custom Shape…: {"name":text,"path":"work|<saved path name>"?}
- `edit.definePattern` Define Pattern…: {"name":str?,"rect":[x0,y0,x1,y1]? (default: selection bounds, else the canvas)} → {"pattern":id} (added to the library; samples the visible composite)
- `edit.fade` Fade…: {"opacity":0..100=100,"mode":"normal|multiply|screen|overlay|softLight|hardLight|darken|lighten|difference|color|luminosity"}
- `edit.fill` Fill…: {"contents":"foreground|background|color|contentAware|pattern|history|black|gray|white"="color","color":"#rrggbb|[r,g,b,a]"=foreground (contents=color),"pattern":id|name (contents=pattern),"scale":%=100,"angle":deg,"state":index? (contents=history; default the oldest state),"colorAdaptation":bool=true (contents=contentAware),"mode":"normal|multiply|…"="normal","opacity":0..100=100,"preserveTransparency":bool=false,"target":"pixels"|{"channel":i}|"quickMask"?}
- `edit.fillBackground` Fill with Background Color: {"layer":id?,"target":"pixels"|{"channel":i}|"quickMask"?}
- `edit.fillBackgroundPreserve` Fill with Background Color, Preserve Transparency: {"layer":id?,"target":"pixels"|{"channel":i}|"quickMask"?}
- `edit.fillForeground` Fill with Foreground Color: {"layer":id?,"target":"pixels"|{"channel":i}|"quickMask"?}
- `edit.fillForegroundPreserve` Fill with Foreground Color, Preserve Transparency: {"layer":id?,"target":"pixels"|{"channel":i}|"quickMask"?}
- `edit.findAndReplaceText` Find and Replace Text…: {"find":text,"replace":text,"action":"changeAll|find|change|changeFind","caseSensitive":bool=false,"wholeWord":bool=false,"forward":bool=true,"allLayers":bool=true}
- `edit.keyboardShortcuts` [host] Keyboard Shortcuts…: {"set":{"<command id>|tools.temporary.hand|zoomIn|zoomOut":"Cmd+Shift+X"|""(remove)|null(default)}?,"reset":true|["<id>",…]?,"removeConflicts":bool=true,"filter":str?,"list":bool=false}
- `edit.menus` [host] Menus…: {"hide":["<id>",…]?,"show":["<id>",…]?,"color":{"<id>":"red|orange|yellow|green|blue|violet|gray|none"}?,"reset":bool=false}
- `edit.paste` Paste: {"center":[x,y]? (view centre; default keeps the position when it overlaps the canvas)} (with no document open: a new document from the clipboard)
- `edit.pasteSpecial.pasteInPlace` Paste in Place: {}
- `edit.pasteSpecial.pasteInto` Paste Into: {"center":[x,y]?}
- `edit.pasteSpecial.pasteOutside` Paste Outside: {"center":[x,y]?}
- `edit.perspectiveWarp` Perspective Warp: {"planes":[{"src":[[x,y]×4],"dst":[[x,y]×4]}…] (corners clockwise from top-left; corners that coincide in src are linked),"straighten":"horizontal|vertical|auto"?,"interpolation":"bicubic|bilinear|nearest","layer":id?}
- `edit.preferences.cursors` [host] Cursors…: {}
- `edit.preferences.enhancedControls` [host] Enhanced Controls…: {}
- `edit.preferences.export` [host] Export…: {}
- `edit.preferences.fileHandling` [host] File Handling…: {}
- `edit.preferences.general` [host] General…: {}
- `edit.preferences.guidesGridAndSlices` [host] Guides, Grid & Slices…: {}
- `edit.preferences.historyLog` [host] History Log…: {}
- `edit.preferences.integrations` [host] Integrations…: {}
- `edit.preferences.interface` [host] Interface…: {}
- `edit.preferences.performance` [host] Performance…: {}
- `edit.preferences.plugIns` [host] Plug-ins…: {}
- `edit.preferences.rawDefaults` [host] Camera Raw…: {}
- `edit.preferences.scratchDisks` [host] Scratch Disks…: {}
- `edit.preferences.tools` [host] Tools…: {}
- `edit.preferences.transparencyAndGamut` [host] Transparency & Gamut…: {}
- `edit.preferences.type` [host] Type…: {}
- `edit.preferences.unitsAndRulers` [host] Units & Rulers…: {}
- `edit.preferences.workspace` [host] Workspace…: {}
- `edit.presets.exportImportPresets` Export/Import Presets…: {"action":"export|import","kinds":["brushes","customShapes"]?,"data":json (import),"includeBuiltins":bool=false}
- `edit.presets.migratePresets` [file] Migrate Presets: {path} → {migrated, added:{gradients,styles,shapes,patternGroups,toolPresets}}: merge a presets/preferences file into the library (appends groups by name)
- `edit.presets.presetManager` Preset Manager…: {"action":"list|rename|delete|move","kind":"brushes|customShapes|patterns","index":n?,"name":str?,"newName":str?,"to":n?}
- `edit.profileInfo` [file] Profile Info: {"profile":"<builtin id>|document|/path/to/profile.icc"=document}
- `edit.puppetWarp` Puppet Warp: {"pins":[{"src":[x,y],"dst":[x,y],"rotate":deg?,"depth":n?}…],"mode":"rigid|normal|distort"="normal","density":"fewer|normal|more"="normal","expansion":px=2,"interpolation":"bicubic|bilinear|nearest","layer":id?} — mesh over the opaque region, as-rigid-as-possible; on a smart object it becomes a smart filter
- `edit.purge.all` All: {}
- `edit.purge.clipboard` Clipboard: {}
- `edit.purge.histories` Histories: {}
- `edit.purge.undo` Undo: {}
- `edit.purge.videoCache` Video Cache: {}
- `edit.redo` Redo: {}
- `edit.stroke` Stroke…: {"width":1..250=1,"color":"#rrggbb|[r,g,b,a]"=foreground,"location":"inside|center|outside"="center","opacity":0..100=100}
- `edit.toggleLastState` Toggle Last State: {}
- `edit.toolbar` [host] Toolbar…: {"hidden":["<tool>",…]?,"order":["<tool>",…]?,"reset":bool=false}
- `edit.transform` Free Transform: {"layer":id?,"rect":[x0,y0,x1,y1]? (source frame; default = layer content ∩ selection),"quad":[[x,y]×4]? (where the frame's corners go, clockwise from top-left),"matrix":[a,b,c,d,e,f]? (affine alternative),"interpolation":"bicubic|bilinear|nearest"="bicubic","target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target (an unlinked mask, an alpha channel or the Quick Mask transforms alone; a linked mask moves with its layer)}
- `edit.transform.again` Again: {}
- `edit.transform.againCopy` Transform Again on a Copy: {} (duplicates the active layer and repeats the last transform on the copy: step and repeat)
- `edit.transform.flipHorizontal` Flip Horizontal: {}
- `edit.transform.flipVertical` Flip Vertical: {}
- `edit.transform.removeWarpSplit` Remove Warp Split: {"warp":{…}? (the warp being edited; returned split),"rect":[x0,y0,x1,y1]?,"at":[x,y]? (document point; default = middle of the patch)} — without `warp`, edits the active smart object's warp. A 1×1 custom mesh is divided at the thirds first (the guides drawn on the default grid), so splitWarpCrosswise on an identity custom warp returns a 4×4 mesh (knots at 0, 1/3, 1/2, 2/3 and 1), not a 2×2. A preset is fitted as one patch and split without that step.
- `edit.transform.rotate180` Rotate 180°: {}
- `edit.transform.rotate90Ccw` Rotate 90° Counter Clockwise: {}
- `edit.transform.rotate90Cw` Rotate 90° Clockwise: {}
- `edit.transform.splitWarpCrosswise` Split Warp Crosswise: {"warp":{…}? (the warp being edited; returned split),"rect":[x0,y0,x1,y1]?,"at":[x,y]? (document point; default = middle of the patch)} — without `warp`, edits the active smart object's warp. A 1×1 custom mesh is divided at the thirds first (the guides drawn on the default grid), so splitWarpCrosswise on an identity custom warp returns a 4×4 mesh (knots at 0, 1/3, 1/2, 2/3 and 1), not a 2×2. A preset is fitted as one patch and split without that step.
- `edit.transform.splitWarpHorizontally` Split Warp Horizontally: {"warp":{…}? (the warp being edited; returned split),"rect":[x0,y0,x1,y1]?,"at":[x,y]? (document point; default = middle of the patch)} — without `warp`, edits the active smart object's warp. A 1×1 custom mesh is divided at the thirds first (the guides drawn on the default grid), so splitWarpCrosswise on an identity custom warp returns a 4×4 mesh (knots at 0, 1/3, 1/2, 2/3 and 1), not a 2×2. A preset is fitted as one patch and split without that step.
- `edit.transform.splitWarpVertically` Split Warp Vertically: {"warp":{…}? (the warp being edited; returned split),"rect":[x0,y0,x1,y1]?,"at":[x,y]? (document point; default = middle of the patch)} — without `warp`, edits the active smart object's warp. A 1×1 custom mesh is divided at the thirds first (the guides drawn on the default grid), so splitWarpCrosswise on an identity custom warp returns a 4×4 mesh (knots at 0, 1/3, 1/2, 2/3 and 1), not a 2×2. A preset is fitted as one patch and split without that step.
- `edit.transform.warp` Warp: {"layer":id?,"rect":[x0,y0,x1,y1]? (warp box; default = layer content ∩ selection),"style":"custom|none|arc|arcLower|arcUpper|arch|bulge|shellLower|shellUpper|flag|wave|fish|rise|fisheye|inflate|squeeze|twist","bend":%=50,"hDistort":%,"vDistort":%,"vertical":bool,"mesh":{"us":[0,…,1],"vs":[0,…,1],"points":[[x,y]…]} ((3c+1)×(3r+1) control points, row-major, document px),"grid":[cols,rows]?,"warp":{full warp object}?,"interpolation":"bicubic|bilinear|nearest"}
- `edit.transform.warpGrid` Warp Grid: {"warp":{…}? (the warp being edited; returned with the new mesh),"rect":[x0,y0,x1,y1]?,"size":n|"default"|"3"|"4"|"5" (square patches, 1..=64; 1 and "default" are the single-patch grid)} — without `warp`/`style`/`mesh`, edits the active smart object's warp. The surface is kept and the new lines are real splits at i/n.
- `edit.undo` Undo: {}

## file

- `file.automate.batch` [code] Batch…: {"steps":[[commandId,params]|{"command":id,"params":{}}…] (a recorded action),"input":folder|[paths],"output":folder,"format":"same|png|jpg|psd|tiff|…"="same","quality":0..12?,"tiffLayers":bool=false} → {files, errors} (an input whose output name was already written in the run goes to errors)
- `file.automate.conditionalModeChange` [file] Conditional Mode Change…: {"from":["rgb","grayscale","cmyk","lab","indexed","bitmap",…]|"any"="any","to":"rgb|grayscale|cmyk|lab"}
- `file.automate.contactSheetII` [file] Contact Sheet II…: {"input":folder|[paths],"units":"inches|cm|mm|pixels"="inches","width":8,"height":10,"resolution":ppi=300,"mode":"rgb|gray|cmyk|lab"="rgb","depth":8|16=8,"columns":5,"rows":6,"placeAcrossFirst":bool=true,"autoSpacing":bool=true,"horizontal":units?,"vertical":units?,"rotateForBestFit":bool=false,"caption":bool=true (file name as caption),"font":family?,"fontSize":pt=12,"flatten":bool=false} → {documents, pages, images}
- `file.automate.createDroplet` [code] Create Droplet…: {"path":str (.pcdroplet),"steps":[[id,params]…] (the action),"name":str?,"output":folder?,"format":"same|png|jpg|…"?,"quality":0..12?,"shim":bool=true on Unix (writes <name>.command calling `photocraft-cli droplet`)} → {path, shim}
- `file.automate.cropAndStraightenPhotos` Crop and Straighten Photos: {} → one new document per photo found on the scan
- `file.automate.fitImage` Fit Image…: {"width":px,"height":px,"dontEnlarge":bool=false,"resample":"bicubic|bilinear|nearest|lanczos|preserveDetails"="bicubic"}
- `file.automate.lensCorrection` [file] Lens Correction…: {"input":folder|[files],"output":folder,"format":"same|jpg|png|tif|psd","quality":0..12?, …Filter › Lens Correction params ("profile" defaults to "auto")}
- `file.automate.mergeToHdrPro` [file] Merge to HDR Pro…: {"paths":[str]|folder,"useOpenDocuments":bool=false,"exposures":[ev]? (default EXIF, else estimated),"align":bool=true,"removeGhosts":bool=false,"ghostBase":int?,"mode":"32|16|8","method":"localAdaptation|exposureGamma|highlightCompression|equalizeHistogram","radius":1..500=7,"strength":0.1..4=0.52,"gamma":0.1..2=1,"exposure":-5..5=0,"detail":-100..300=30,"shadow":-100..100=0,"highlight":-100..100=0,"vibrance":-100..100=0,"saturation":-100..100=20} → new document (32-bit linear, or tone-mapped 16/8-bit)
- `file.automate.photomerge` [file] Photomerge…: {"paths":[str]|folder,"useOpenDocuments":bool=false,"layout":"auto|perspective|cylindrical|spherical|collage|reposition","blend":bool=true,"vignetteRemoval":bool=false,"geometricCorrection":bool=false,"contentAwareFill":bool=false,"focalLength":mm=0 (35 mm equivalent; 0 = EXIF or estimated)} → new document, one masked layer per image
- `file.automate.runDroplet` [code] Run Droplet: {"droplet":path,"input":[files or folders],"output":folder? (default: droplet's, else <input folder>/droplet-output)} → {files, errors} (an input whose output name was already written in the run goes to errors)
- `file.close` Close: {"document":index?}
- `file.closeAll` Close All: {}
- `file.closeOthers` Close Others: {"document":index? (the one to keep; default active)}
- `file.export.artboardsToFiles` [file] Artboards to Files…: {"dir":folder,"format":"png|jpg|psd|tiff|…"="png","prefix":str=document name ("" = none),"artboards":[id]? (default all),"quality":0..12?} → {files}
- `file.export.artboardsToPdf` [file] Artboards to PDF…: {"path":str (.pdf),"artboards":[id]?,"quality":0..12=10} → {path, pages} (one raster page per board)
- `file.export.colorLookupTables` [file] Color Lookup Tables…: {"path":str? (.cube; omit to return the text),"size":2..256=33,"title":str?}
- `file.export.dataSetsAsFiles` [file] Data Sets as Files…: {dir, format?:png, dataSets?[names], naming?:"{name}|{index}|{document}"} → {files,count}: apply each data set and export the flattened document
- `file.export.exportPreferences` [host] Export Preferences…: {"quickExportFormat":"png|jpg|gif|webp"?,"quickExportLocation":"ask|sameFolder"?,"jpegQuality":1..100?,"metadata":"none|copyright|all"?,"convertToSrgb":bool?} → {values}
- `file.export.layerCompsToFiles` [file] Layer Comps to Files…: {"dir":folder,"format":"png|jpg|psd|tiff|…"="png","prefix":str=document name,"selectedOnly":bool=false (only the last applied comp),"comps":[id]?,"quality":0..12?} → {files}
- `file.export.layersToFiles` [file] Layers to Files…: {"dir":folder,"format":"png|jpg|psd|tiff|…"="png","prefix":str=document name,"visibleOnly":bool=true,"quality":0..12?,"tiffLayers":bool=false} → {files}
- `file.export.pathsToIllustrator` [file] Paths to Illustrator…: {"path":str? (.ai; omit to return the text),"paths":"all|work|<path name>"="all"} → {path, paths}
- `file.export.quickExport` [file] Quick Export: {"path":str? (required unless Export Preferences › Location is "sameFolder" and the document is saved)} → {path, format, bytes}
- `file.export.renderVideo` [file] Render Video…: {} → {}
- `file.export.saveForWebLegacy` [file] Save for Web (Legacy)…: {"preset":"GIF 128 Dithered|JPEG High|PNG-24|…"?,"format":"gif|png8|png24|jpeg|wbmp"="png24","palette":"perceptual|selective|adaptive|restrictive|exact|systemMac|systemWindows|uniform"="selective","colors":2..256=256,"dither":"none|diffusion|pattern|noise"="diffusion","ditherAmount":0..100=88,"transparency":bool=true,"matte":"#rrggbb|none"="#ffffff","interlaced":bool=false,"webSnap":0..100=0,"quality":0..100=60,"progressive":bool=false,"optimized":bool=true,"embedIcc":bool=false,"metadata":"none|copyright|copyrightAndContact|all"="copyright","convertToSrgb":bool=true,"width"|"height"|"percent"? (image size),"resample":"bicubic|bilinear|nearest"?,"path":file? (whole image),"dir":folder? (one file per slice in images/),"html":bool=false,"slices":"all|user"="all","numbers":[n]?} → no path/dir: {bytes,width,height,colors} estimate; else {files, html, bytes}
- `file.fileInfo` File Info…: {"title":str?,"author":str?,"authorTitle":str?,"description":str?,"keywords":[str]|"a; b"?,"copyright":str?,"copyrightStatus":"unknown|copyrighted|publicDomain"?,"copyrightUrl":str?} (no keys: read)
- `file.generate.imageAssets` [file] Image Assets: {"on":bool? (default: toggle),"dir":folder? (default <document>-assets next to the file)} → {enabled, files, errors}; layers named like "foo.png", "200% foo@2x.png", "48x48 icons/a.png8", "photo.jpg80%" are exported, now and after each save
- `file.import.notes` [file] Notes…: {"path":".psd|.psb|.pcraft with notes"}
- `file.import.variableDataSets` [file] Variable Data Sets…: {path, delimiter?} → {imported}: CSV header = variable names, each row a data set (first column may be the data-set name)
- `file.import.videoFramesToLayers` [file] Video Frames to Layers…: {} → {}
- `file.import.wiaSupport` [device] WIA Support…: {} → {available, devices, note}: acquire from a scanner/camera (Windows only)
- `file.new` New…: {"width":u32=1920,"height":u32=1080,"mode":"rgb|gray|cmyk|lab"="rgb","depth":8|16|32=8,"background":"white|black|backgroundColor|transparent|#rrggbb"="white","resolution":ppi=72,"name":str}
- `file.newFromClipboard` New from Clipboard: {} (a new document the size of the clipboard image, holding it as one layer)
- `file.openAs` [file] Open As…: {"path":str,"as":"psd|png|jpg|tiff|…"? (decode as this format)}
- `file.package` [file] Package…: {"dir":folder,"format":"pcraft|psd|psb"="pcraft"} → {folder, document, links, missing} (copies the document and its linked files into <dir>/<name>/, relinked to Links/)
- `file.placeEmbedded` [file] Place Embedded…: {"path":str,"scale":%? (default: fit when larger than the canvas),"fit":bool=true,"center":[x,y]?}
- `file.placeLinked` [file] Place Linked…: {"path":str,"scale":%?,"fit":bool=true,"center":[x,y]?}
- `file.print` [device] Print…: {"printer":name? (default printer),"copies":1..999=1,"paper":"letter|legal|tabloid|a3|a4|a5|4x6|5x7"|[w,h] pt="letter","orientation":"portrait|landscape"="portrait","center":bool=true,"top":in?,"left":in?,"scale":%=100,"scaleToFit":bool=false,"colorHandling":"printerManages|photocraftManages|noColorManagement"="printerManages","printerProfile":profile? (photocraftManages),"intent":"perceptual|relative|saturation|absolute"="relative","bpc":bool=true,"cornerCropMarks":bool,"centerCropMarks":bool,"registrationMarks":bool,"description":bool,"labels":bool,"output":pdf path? (print to PDF; then "send" defaults to false),"send":bool?,"dryRun":bool=false (render the PDF, report the lp command, don't spool)} → {pdf, imageRect, command, sent}
- `file.printOneCopy` [device] Print One Copy: {} (the last Print settings, one copy; any Print key overrides)
- `file.revert` [file] Revert: {} (reloads the saved file as one undoable step)
- `file.saveACopy` [file] Save a Copy…: {"path":str (format from the extension),"quality":0..12? (JPEG),"layers":bool=true,"tiffLayers":bool=false (TIFF: keep the layers; flat by default)}
- `file.scripts.browse` [code] Browse…: {"path":script file | "script":text | "steps":[[id,params]…]} (JSON action, or one `command.id {json}` per line) → {steps, ok, results}
- `file.scripts.deleteAllEmptyLayers` Delete All Empty Layers: {}
- `file.scripts.flattenAllLayerEffects` Flatten All Layer Effects: {}
- `file.scripts.flattenAllMasks` Flatten All Masks: {} (pixel layers; masks on other layer kinds are left)
- `file.scripts.imageProcessor` [file] Image Processor…: {"input":folder|[paths],"output":folder,"format":"jpg|png|psd|tiff|…"="jpg","quality":0..12=8,"tiffLayers":bool=false,"width":px?,"height":px? (fit, never enlarge),"convertToSrgb":bool=false} → {files, errors} (an input whose output name was already written in the run goes to errors)
- `file.scripts.loadFilesIntoStack` [file] Load Files into Stack…: {"paths":[str]|folder,"createSmartObject":bool=false} → new document with one layer per file (inside one smart object with createSmartObject)
- `file.scripts.scriptEventsManager` [code] Script Events Manager…: {"enabled":bool?,"add":{"event":"startApplication|newDocument|openDocument|saveDocument|closeDocument|print|export|everything","script":path?|"steps":[…]?,"name":str?}?,"remove":index?,"removeAll":bool?} → {enabled, bindings, events}
- `file.scripts.statistics` [file] Statistics…: {"mode":"mean|median|maximum|minimum|range|summation|variance|standardDeviation|skewness|kurtosis|entropy"="median","input":folder|[paths],"align":bool=false (Auto-Align first)} → new document with one stack-mode smart object

## filter

- `filter.adaptiveWideAngle` Adaptive Wide Angle…: {"model":"auto|fisheye|perspective|fullSpherical","focalLength":0..200=0,"cropFactor":0.1..10=1,"scale":50..150=100,"constraints":[{"a":[x,y],"b":[x,y],"orientation":"free|horizontal|vertical"}]}
- `filter.blur.average` Average: {}
- `filter.blur.blur` Blur: {}
- `filter.blur.blurMore` Blur More: {}
- `filter.blur.boxBlur` Box Blur…: {"radius":1..2000=1}
- `filter.blur.gaussianBlur` Gaussian Blur…: {"radius":0.1..1000=1}
- `filter.blur.lensBlur` Lens Blur…: {"radius":0..100=15,"shape":"hexagon|triangle|square|pentagon|heptagon|octagon","bladeCurvature":0..100=0,"rotation":0..360=0,"depthMap":"none|transparency|layerMask","focalDistance":0..255=0,"invert":bool,"brightness":0..100=0,"threshold":0..255=255,"noise":0..100=0,"distribution":"uniform|gaussian","monochromatic":bool,"seed":u32=0}
- `filter.blur.motionBlur` Motion Blur…: {"angle":-360..360=0,"distance":1..2000=10}
- `filter.blur.radialBlur` Radial Blur…: {"amount":1..100=10,"method":"spin|zoom","centerX":0..1=0.5,"centerY":0..1=0.5}
- `filter.blur.shapeBlur` Shape Blur…: {"radius":5..1000=10,"shape":"circle|ring|square|diamond|triangle|hexagon|star|heart|cross"}
- `filter.blur.smartBlur` Smart Blur…: {"radius":0.1..100=3,"threshold":0.1..100=25,"quality":"high|medium|low","mode":"normal|edgeOnly|overlayEdge"}
- `filter.blur.surfaceBlur` Surface Blur…: {"radius":1..100=5,"threshold":2..255=15}
- `filter.blurGallery.fieldBlur` Field Blur…: {"blur":0..500=15,"centerX":0..1=0.5,"centerY":0..1=0.5,"pins":json}
- `filter.blurGallery.irisBlur` Iris Blur…: {"blur":0..500=15,"centerX":0..1=0.5,"centerY":0..1=0.5,"radiusX":0.01..1=0.35,"radiusY":0.01..1=0.25,"angle":-180..180=0,"roundness":0..100=0,"feather":0..0.99=0.5,"pins":json}
- `filter.blurGallery.pathBlur` Path Blur…: {"speed":0..500=50,"taper":0..100=0,"startX":0..1=0.2,"startY":0..1=0.5,"endX":0..1=0.8,"endY":0..1=0.5,"paths":json}
- `filter.blurGallery.spinBlur` Spin Blur…: {"blurAngle":0..360=15,"centerX":0..1=0.5,"centerY":0..1=0.5,"radiusX":0.01..1=0.3,"radiusY":0.01..1=0.3,"angle":-180..180=0,"pins":json}
- `filter.blurGallery.tiltShift` Tilt-Shift…: {"blur":0..500=15,"centerX":0..1=0.5,"centerY":0..1=0.5,"angle":-90..90=0,"focus":0..1=0.1,"transition":0.01..1=0.15}
- `filter.cameraRaw` Camera Raw Filter…: {"temperature":-100..100=0,"tint":-100..100=0,"exposure":-5..5=0,"contrast":-100..100=0,"highlights":-100..100=0,"shadows":-100..100=0,"whites":-100..100=0,"blacks":-100..100=0,"texture":-100..100=0,"clarity":-100..100=0,"dehaze":-100..100=0,"vibrance":-100..100=0,"saturation":-100..100=0,"curveHighlights":-100..100=0,"curveLights":-100..100=0,"curveDarks":-100..100=0,"curveShadows":-100..100=0,"curveSplits":[25,50,75],"pointCurve":[[in,out]],"pointCurveRed":[[in,out]],"pointCurveGreen":[[in,out]],"pointCurveBlue":[[in,out]],"hslHue":[8],"hslSat":[8],"hslLum":[8],"gradeShadows":{"hue":deg,"sat":0..100,"lum":-100..100},"gradeMidtones":{},"gradeHighlights":{},"gradeGlobal":{},"gradeBlending":0..100=50,"gradeBalance":-100..100=0,"sharpenAmount":0..150=0,"sharpenRadius":0.5..3=1,"sharpenDetail":0..100=25,"sharpenMasking":0..100=0,"noiseLuminance":0..100=0,"noiseLuminanceDetail":0..100=50,"noiseColor":0..100=0,"noiseColorDetail":0..100=50,"grainAmount":0..100=0,"grainSize":0..100=25,"grainRoughness":0..100=50,"vignetteAmount":-100..100=0,"vignetteMidpoint":0..100=50,"vignetteRoundness":-100..100=0,"vignetteFeather":0..100=50,"vignetteHighlights":0..100=0,"vignetteStyle":"highlightPriority|colorPriority|paintOverlay","seed":u32=0}
- `filter.convertForSmartFilters` Convert for Smart Filters: {"layer":id?}
- `filter.distort.displace` [file] Displace…: {"horizontal":-999..999=10,"vertical":-999..999=10,"fit":"stretch|tile","undefinedAreas":"repeat|wrap","mapDocument":doc,"mapPath":text,"mapLayer":json}
- `filter.distort.pinch` Pinch…: {"amount":-100..100=50}
- `filter.distort.polarCoordinates` Polar Coordinates…: {"mode":"rectangularToPolar|polarToRectangular"}
- `filter.distort.ripple` Ripple…: {"amount":-999..999=100,"size":"small|medium|large"}
- `filter.distort.shear` Shear…: {"amount":-100..100=0,"undefinedAreas":"wrap|repeat","points":json}
- `filter.distort.spherize` Spherize…: {"amount":-100..100=100,"mode":"normal|horizontalOnly|verticalOnly"}
- `filter.distort.twirl` Twirl…: {"angle":-999..999=50}
- `filter.distort.wave` Wave…: {"generators":1..999=5,"wavelengthMin":1..998=10,"wavelengthMax":2..999=120,"amplitudeMin":1..998=5,"amplitudeMax":1..999=35,"type":"sine|triangle|square","undefinedAreas":"wrap|repeat","seed":u32=0}
- `filter.distort.zigZag` ZigZag…: {"amount":-100..100=10,"ridges":0..20=5,"style":"pondRipples|outFromCenter|aroundCenter"}
- `filter.filterGallery` Filter Gallery…: {"effects":json,"foreground":json,"background":json,"list":bool}
- `filter.gallery.accentedEdges` Accented Edges: {"edgeWidth":1..14=2,"edgeBrightness":0..50=38,"smoothness":1..15=5}
- `filter.gallery.angledStrokes` Angled Strokes: {"directionBalance":0..100=50,"strokeLength":3..50=15,"sharpness":0..10=3}
- `filter.gallery.basRelief` Bas Relief: {"detail":1..15=13,"smoothness":1..15=3,"light":"bottom|bottomLeft|left|topLeft|top|topRight|right|bottomRight","foreground":json,"background":json}
- `filter.gallery.chalkCharcoal` Chalk & Charcoal: {"charcoalArea":0..20=6,"chalkArea":0..20=6,"strokePressure":0..5=1,"foreground":json,"background":json}
- `filter.gallery.charcoal` Charcoal: {"charcoalThickness":1..7=1,"detail":0..5=5,"lightDarkBalance":0..100=50,"foreground":json,"background":json}
- `filter.gallery.chrome` Chrome: {"detail":0..10=4,"smoothness":0..10=7}
- `filter.gallery.coloredPencil` Colored Pencil: {"pencilWidth":1..24=4,"strokePressure":0..15=8,"paperBrightness":0..50=25}
- `filter.gallery.conteCrayon` Conté Crayon: {"foregroundLevel":1..15=11,"backgroundLevel":1..15=7,"texture":"canvas|brick|burlap|sandstone","scaling":50..200=100,"relief":0..50=4,"light":"top|topRight|right|bottomRight|bottom|bottomLeft|left|topLeft","invert":bool,"foreground":json,"background":json}
- `filter.gallery.craquelure` Craquelure: {"crackSpacing":2..100=15,"crackDepth":0..10=6,"crackBrightness":0..10=9}
- `filter.gallery.crosshatch` Crosshatch: {"strokeLength":3..50=9,"sharpness":0..20=6,"strength":1..3=1}
- `filter.gallery.cutout` Cutout: {"numberOfLevels":2..8=4,"edgeSimplicity":0..10=4,"edgeFidelity":1..3=2}
- `filter.gallery.darkStrokes` Dark Strokes: {"balance":0..10=5,"blackIntensity":0..10=6,"whiteIntensity":0..10=2}
- `filter.gallery.diffuseGlow` Diffuse Glow: {"graininess":0..10=6,"glowAmount":0..20=10,"clearAmount":0..20=15,"background":json}
- `filter.gallery.dryBrush` Dry Brush: {"brushSize":0..10=2,"brushDetail":0..10=8,"texture":1..3=1}
- `filter.gallery.filmGrain` Film Grain: {"grain":0..20=4,"highlightArea":0..20=0,"intensity":0..10=10}
- `filter.gallery.fresco` Fresco: {"brushSize":0..10=2,"brushDetail":0..10=8,"texture":1..3=1}
- `filter.gallery.glass` Glass: {"distortion":0..20=5,"smoothness":1..15=3,"texture":"frosted|blocks|canvas|tinyLens","scaling":50..200=100,"invert":bool}
- `filter.gallery.glowingEdges` Glowing Edges: {"edgeWidth":1..14=2,"edgeBrightness":0..20=6,"smoothness":1..15=5}
- `filter.gallery.grain` Grain: {"intensity":0..100=40,"contrast":0..100=50,"grainType":"regular|soft|sprinkles|clumped|contrasty|enlarged|stippled|horizontal|vertical|speckle","foreground":json,"background":json}
- `filter.gallery.graphicPen` Graphic Pen: {"strokeLength":1..15=15,"lightDarkBalance":0..100=50,"strokeDirection":"rightDiagonal|horizontal|leftDiagonal|vertical","foreground":json,"background":json}
- `filter.gallery.halftonePattern` Halftone Pattern: {"size":1..12=1,"contrast":0..50=5,"patternType":"dot|circle|line","foreground":json,"background":json}
- `filter.gallery.inkOutlines` Ink Outlines: {"strokeLength":1..50=4,"darkIntensity":0..50=20,"lightIntensity":0..50=10}
- `filter.gallery.mosaicTiles` Mosaic Tiles: {"tileSize":2..100=12,"groutWidth":1..15=3,"lightenGrout":0..10=9}
- `filter.gallery.neonGlow` Neon Glow: {"glowSize":-24..24=5,"glowBrightness":0..50=15,"glowColor":json}
- `filter.gallery.notePaper` Note Paper: {"imageBalance":0..50=25,"graininess":0..20=10,"relief":0..25=11,"foreground":json,"background":json}
- `filter.gallery.oceanRipple` Ocean Ripple: {"rippleSize":1..15=9,"rippleMagnitude":0..20=9}
- `filter.gallery.paintDaubs` Paint Daubs: {"brushSize":1..50=8,"sharpness":0..40=7,"brushType":"simple|lightRough|darkRough|wideSharp|wideBlurry|sparkle"}
- `filter.gallery.paletteKnife` Palette Knife: {"strokeSize":1..50=25,"strokeDetail":1..3=3,"softness":0..10=0}
- `filter.gallery.patchwork` Patchwork: {"squareSize":0..10=4,"relief":0..25=8}
- `filter.gallery.photocopy` Photocopy: {"detail":1..24=7,"darkness":1..50=8,"foreground":json,"background":json}
- `filter.gallery.plaster` Plaster: {"imageBalance":0..50=20,"smoothness":1..15=2,"light":"top|topRight|right|bottomRight|bottom|bottomLeft|left|topLeft","foreground":json,"background":json}
- `filter.gallery.plasticWrap` Plastic Wrap: {"highlightStrength":0..20=15,"detail":1..15=9,"smoothness":1..15=7}
- `filter.gallery.posterEdges` Poster Edges: {"edgeThickness":0..10=2,"edgeIntensity":0..10=1,"posterization":0..6=2}
- `filter.gallery.reticulation` Reticulation: {"density":0..50=12,"foregroundLevel":0..50=40,"backgroundLevel":0..50=5,"foreground":json,"background":json}
- `filter.gallery.roughPastels` Rough Pastels: {"strokeLength":0..40=6,"strokeDetail":1..20=4,"texture":"canvas|brick|burlap|sandstone","scaling":50..200=100,"relief":0..50=20,"light":"bottom|bottomLeft|left|topLeft|top|topRight|right|bottomRight","invert":bool}
- `filter.gallery.smudgeStick` Smudge Stick: {"strokeLength":0..10=2,"highlightArea":0..20=0,"intensity":0..10=10}
- `filter.gallery.spatter` Spatter: {"sprayRadius":0..25=10,"smoothness":1..15=5}
- `filter.gallery.sponge` Sponge: {"brushSize":0..10=2,"definition":0..25=12,"smoothness":1..15=5}
- `filter.gallery.sprayedStrokes` Sprayed Strokes: {"strokeLength":0..20=12,"sprayRadius":0..25=7,"strokeDirection":"rightDiagonal|horizontal|leftDiagonal|vertical"}
- `filter.gallery.stainedGlass` Stained Glass: {"cellSize":2..50=10,"borderThickness":1..20=4,"lightIntensity":0..10=3,"foreground":json}
- `filter.gallery.stamp` Stamp: {"lightDarkBalance":0..50=25,"smoothness":1..50=5,"foreground":json,"background":json}
- `filter.gallery.sumiE` Sumi-e: {"strokeWidth":3..15=10,"strokePressure":0..15=2,"contrast":0..40=16}
- `filter.gallery.texturizer` Texturizer: {"texture":"canvas|brick|burlap|sandstone","scaling":50..200=100,"relief":0..50=4,"light":"top|topRight|right|bottomRight|bottom|bottomLeft|left|topLeft","invert":bool}
- `filter.gallery.tornEdges` Torn Edges: {"imageBalance":0..50=25,"smoothness":1..15=11,"contrast":1..25=17,"foreground":json,"background":json}
- `filter.gallery.underpainting` Underpainting: {"brushSize":0..40=6,"textureCoverage":0..40=16,"texture":"canvas|brick|burlap|sandstone","scaling":50..200=100,"relief":0..50=4,"light":"top|topRight|right|bottomRight|bottom|bottomLeft|left|topLeft","invert":bool}
- `filter.gallery.waterPaper` Water Paper: {"fiberLength":3..50=15,"brightness":0..100=60,"contrast":0..100=80}
- `filter.gallery.watercolor` Watercolor: {"brushDetail":1..14=9,"shadowIntensity":0..10=1,"texture":1..3=1}
- `filter.lastFilter` [code] Last Filter: {}
- `filter.lensCorrection` Lens Correction…: {"profile":"none|auto|generic","focalLength":mm=0,"correctDistortion":bool=true,"correctVignette":bool=true,"correctCA":bool=true,"autoScale":bool=false,"distortion":-100..100=0,"redCyan":-100..100=0,"blueYellow":-100..100=0,"vignetteAmount":-100..100=0,"vignetteMidpoint":0..100=50,"vertical":-100..100=0,"horizontal":-100..100=0,"angle":-180..180=0,"scale":50..150=100,"edge":"transparency|edgeExtension|black|white","straighten":[[x,y],[x,y]]?}
- `filter.liquify` Liquify…: {"strokes":[{"tool":"forwardWarp|reconstruct|smooth|twirlCw|twirlCcw|pucker|bloat|pushLeft|freeze|thaw|lassoMask|reconstructAll","size":px=100,"density":0-100=50,"pressure":0-100=100,"rate":0-100=80,"points":[[x,y,pressure?]…],"amount":%? (reconstructAll; lassoMask: 1 freezes the polygon in points, 0 thaws it)}…],"meshSize":px? (field resolution, px per node; default 2, or 4 above 4 MP),"layer":id?} — strokes replay in order on a fresh field; a selection limits the effect; on a smart object it becomes a smart filter
- `filter.noise.addNoise` Add Noise…: {"amount":0.1..400=12.5,"distribution":"uniform|gaussian","monochromatic":bool,"seed":u32=0}
- `filter.noise.despeckle` Despeckle: {}
- `filter.noise.dustAndScratches` Dust & Scratches…: {"radius":1..500=1,"threshold":0..255=0}
- `filter.noise.median` Median…: {"radius":1..500=1}
- `filter.noise.reduceNoise` Reduce Noise…: {"strength":0..10=6,"preserveDetails":0..100=60,"reduceColorNoise":0..100=45,"sharpenDetails":0..100=25,"removeJpegArtifact":bool}
- `filter.other.custom` Custom…: {"kernel":int[25],"scale":1..9999=1,"offset":-9999..9999=0}
- `filter.other.highPass` High Pass…: {"radius":0.1..1000=10}
- `filter.other.hsbHsl` HSB/HSL: {"inputMode":"rgb|hsb|hsl","rowOrder":"hsb|hsl|rgb"}
- `filter.other.maximum` Maximum…: {"radius":0.2..500=1,"preserve":"squareness|roundness"}
- `filter.other.minimum` Minimum…: {"radius":0.2..500=1,"preserve":"squareness|roundness"}
- `filter.other.offset` Offset…: {"horizontal":px=0,"vertical":px=0,"undefinedAreas":"wrap|repeat|transparent"}
- `filter.pixelate.colorHalftone` Color Halftone…: {"maxRadius":4..127=8,"channel1":-360..360=108,"channel2":-360..360=162,"channel3":-360..360=90,"channel4":-360..360=45}
- `filter.pixelate.crystallize` Crystallize…: {"cellSize":3..300=10,"seed":u32=0}
- `filter.pixelate.facet` Facet: {}
- `filter.pixelate.fragment` Fragment: {}
- `filter.pixelate.mezzotint` Mezzotint…: {"type":"fineDots|mediumDots|grainyDots|coarseDots|shortLines|mediumLines|longLines|shortStrokes|mediumStrokes|longStrokes","seed":u32=0}
- `filter.pixelate.mosaic` Mosaic…: {"cellSize":2..200=10}
- `filter.pixelate.pointillize` Pointillize…: {"cellSize":3..300=5,"seed":u32=0,"background":json}
- `filter.render.clouds` Clouds: {"seed":u32=0}
- `filter.render.differenceClouds` Difference Clouds: {"seed":u32=0}
- `filter.render.fibers` Fibers…: {"variance":1..64=16,"strength":1..64=4,"seed":u32=0,"foreground":json,"background":json}
- `filter.render.flame` Flame…: {"flameType":"oneFlameAlongPath|multipleFlamesAlongPath|multipleFlamesPathDirections|multipleFlamesVariousLength|candleLight|multipleFlamesOneDirection","length":1..1000=150,"randomizeLength":bool,"width":1..1000=40,"angle":-180..180=0,"interval":1..1000=60,"adjustIntervalForLoops":bool=true,"useCustomColor":bool,"color":color,"turbulent":0..100=25,"jag":0..100=25,"opacity":0..100=75,"flameLines":1..100=20,"flameBottomAlignment":0..100=20,"flameStyle":"normal|violent|flat","flameShape":"parallel|toCenter|spread|oval|pointed","randomizeShapes":bool,"seed":u32=0,"quality":"draft|low|medium|high|fine","path":text,"newLayer":bool} → {layer,newLayer,bounds,primitives,usedPath}
- `filter.render.lensFlare` Lens Flare…: {"brightness":10..300=100,"centerX":0..1=0.5,"centerY":0..1=0.5,"lens":"zoom|prime35|prime105|moviePrime"}
- `filter.render.lightingEffects` Lighting Effects…: {"lightType":"spot|point|infinite","intensity":-100..100=75,"lightX":0..1=0.25,"lightY":0..1=0.2,"lightZ":0..2=0.6,"targetX":0..1=0.5,"targetY":0..1=0.55,"cone":1..89=45,"hotspot":0..100=50,"angle":-180..180=135,"elevation":0..90=45,"gloss":-100..100=0,"metallic":-100..100=0,"exposure":-100..100=0,"ambience":-100..100=8,"texture":"none|red|green|blue|alpha|luminance","height":0..100=50,"whiteIsHigh":bool=true,"lights":json}
- `filter.render.pictureFrame` Picture Frame…: {"frame":"vineWithFlowers|vineWithLeaves|ivy|roses|daisies|berries|bamboo|waves|zigzag|dots|rope|scallops|doubleLine|snowflakes|stars|hearts","margin":0..30=4,"size":1..100=50,"arrangement":1..100=50,"lines":1..5=1,"thickness":1..100=30,"fade":0..100=0,"vineColor":color,"flowerColor":color,"leafColor":color,"seed":u32=0,"newLayer":bool} → {layer,newLayer,bounds,primitives,frame}
- `filter.render.relight` Relight…: {"angle":-180..180=45,"elevation":0..90=40,"intensity":0..100=40,"ambient":0..100=55,"warmth":-100..100=0,"softness":1..100=25}
- `filter.render.tree` Tree…: {"baseTreeType":1..34=1,"lightDirection":1..5=3,"leavesAmount":0..100=50,"leavesSize":0..200=100,"branchesHeight":50..300=100,"branchesThickness":50..200=100,"defaultLeaves":bool=true,"leavesColor":color,"customBranchColor":bool,"branchesColor":color,"flatShading":bool,"seed":u32=0,"x":0..1=0.5,"y":0..1=0.95,"size":0.05..2=0.8,"newLayer":bool} → {layer,newLayer,bounds,primitives,treeType}
- `filter.sharpen.sharpen` Sharpen: {}
- `filter.sharpen.sharpenEdges` Sharpen Edges: {}
- `filter.sharpen.sharpenMore` Sharpen More: {}
- `filter.sharpen.smartSharpen` Smart Sharpen…: {"amount":1..500=100,"radius":0.1..64=1,"reduceNoise":0..100=10}
- `filter.sharpen.unsharpMask` Unsharp Mask…: {"amount":1..500=50,"radius":0.1..1000=1,"threshold":0..255=0}
- `filter.stylize.diffuse` Diffuse…: {"mode":"normal|darkenOnly|lightenOnly|anisotropic","seed":u32=0}
- `filter.stylize.emboss` Emboss…: {"angle":-180..180=135,"height":1..100=3,"amount":1..500=100}
- `filter.stylize.extrude` Extrude…: {"type":"blocks|pyramids","size":2..255=30,"depth":1..255=30,"depthMode":"random|levelBased","solidFrontFaces":bool,"maskIncompleteBlocks":bool,"seed":u32=0}
- `filter.stylize.findEdges` Find Edges: {}
- `filter.stylize.oilPaint` Oil Paint…: {"stylization":0.1..10=4,"cleanliness":0..10=5,"scale":0.1..10=1,"bristleDetail":0..10=5,"lighting":bool=true,"angle":-180..180=-60,"shine":0..10=1}
- `filter.stylize.solarize` Solarize: {}
- `filter.stylize.tiles` Tiles…: {"count":1..99=10,"maxOffset":1..99=10,"fill":"background|foreground|inverse|unaltered","seed":u32=0,"foreground":json,"background":json}
- `filter.stylize.traceContour` Trace Contour…: {"level":0..255=128,"edge":"lower|upper"}
- `filter.stylize.wind` Wind…: {"method":"wind|blast|stagger","direction":"fromRight|fromLeft","seed":u32=0}
- `filter.vanishingPoint` Vanishing Point…: {"planes":[{"corners":[[x,y]×4]} | {"from":plane,"edge":"top|right|bottom|left","angle":deg=90,"depth":1}],"focalLength":px=0,"paste":[{"plane":0,"layer":id? (else the clipboard),"at":[u,v]=[0.25,0.25],"width":0..1=0.5}],"clone":[{"source":[x,y],"points":[[x,y]],"size":px=40,"hardness":0..100=50,"opacity":0..100=100}],"newLayer":bool=false}
- `filter.video.deInterlace` De-Interlace…: {"eliminate":"oddFields|evenFields","createBy":"interpolation|duplication"}
- `filter.video.ntscColors` NTSC Colors: {}

## gradient

- `gradient.fill.create` Gradient (Live): {"from":[x,y],"to":[x,y],"style":"linear|radial|angle|reflected|diamond"="linear","gradient":preset name?,"stops":[[t,"#rrggbb"|"foreground"|"background"],…]?,"transparency":[[t,0..100],…]? (default: the current gradient),"reverse":bool=false,"dither":bool=true,"opacity":0..100=100,"mode":"normal|multiply|…"="normal"} → {"layer":id}; a Gradient Fill layer above the active layer, masked by the selection
- `gradient.fill.get` Gradient Fill Settings: {"layer":id?} → {"style","angle","scale":%,"offset":[%,%],"reverse","dither","align","from":[x,y],"to":[x,y],"stops":[[t,"#hex"]],"transparency":[[t,%]],"midpoints":[…]}
- `gradient.fill.set` Edit Gradient Fill: {"layer":id?,"from":[x,y]?,"to":[x,y]?,"style":"linear|radial|angle|reflected|diamond"?,"angle":deg?,"scale":%?,"offset":[x%,y%]?,"reverse":bool?,"dither":bool?,"align":bool?,"stops":[[t,"#rrggbb"|"#rrggbbaa"|[r,g,b,a]|"foreground"|"background"],…]?,"transparency":[[t,0..100],…]?,"midpoints":[0.05..0.95 per segment]?} → the gradient (see gradient.fill.get)
- `gradient.fill.stop` Edit Gradient Stop: {"layer":id?,"action":"add|move|delete|color|opacity|midpoint","kind":"color|opacity"="color","index":stop (midpoint: segment) number,"location":0..1 (midpoint: of the segment),"color":"#rrggbb"|[r,g,b,a]|"foreground"|"background" (add: default the colour there),"opacity":0..100} → the gradient
- `gradient.presets.apply` New Gradient Fill Layer from Preset: {"preset":name?=current|"stops":[[t 0..1,"#rrggbb"|"foreground"|"background"],…],"transparency":[[t,opacity 0..100],…]?,"angle":deg=90,"style":"linear|radial|angle|reflected|diamond"="linear","scale":10..150=100,"reverse":bool} → {layer}
- `gradient.presets.edit` Edit Gradient Presets: {"action":"rename|delete|move|newGroup|renameGroup|deleteGroup","preset":name|[names] (rename/delete/move),"group":name? (narrows the lookup; the group for renameGroup/deleteGroup),"name":str (rename: new name; newGroup/renameGroup: group name),"to":group (move),"index":n? (move)}
- `gradient.presets.importGrd` [file] Import Gradients…: {"path":".grd file"?,"data":base64 bytes?,"group":string?=file name,"replace":bool=true (replace a group of the same name)}
- `gradient.presets.list` Gradient Presets: {} → {groups:[{name,presets:[{name,stops,transparency}]}],current}
- `gradient.presets.new` New Gradient Preset: {"name":str="Custom","group":name?=first,"stops":[[t 0..1,"#rrggbb"|"foreground"|"background"],…],"transparency":[[t,opacity 0..100],…]? (default: the current gradient)}
- `gradient.presets.reset` Restore Default Gradients: {"append":bool=false}
- `gradient.presets.select` Select Gradient: {"preset":name|"stops":[[t 0..1,"#rrggbb"|"foreground"|"background"],…],"transparency":[[t,opacity 0..100],…]?,"group":name?,"applyToLayer":bool=true (also recolours a selected Gradient Fill layer)} → {current,layer?}. The Gradient tool paints with it.

## image

- `image.adjustments.blackWhite` Black & White…: {"reds":-200..300=40,"yellows":-200..300=60,"greens":-200..300=40,"cyans":-200..300=60,"blues":-200..300=20,"magentas":-200..300=80,"tint":bool=false,"tintColor":"#rrggbb"}
- `image.adjustments.brightnessContrast` Brightness/Contrast…: {"brightness":-150..150=0,"contrast":-50..100=0,"legacy":bool=false}
- `image.adjustments.channelMixer` Channel Mixer…: {"red":json,"green":json,"blue":json,"gray":json,"monochrome":bool=false} (per output channel [red %, green %, blue %, constant %] in -200..200; gray = the monochrome mix)
- `image.adjustments.colorBalance` Color Balance…: {"shadows":json,"midtones":json,"highlights":json,"preserveLuminosity":bool=true} (each tone [cyan-red, magenta-green, yellow-blue] in -100..100)
- `image.adjustments.colorLookup` [file] Color Lookup…: {"lut":"none|warm|cool|tealOrange|bleachBypass|fadedFilm|dayForNight|monoContrast|crossProcess"="none","file":text,"interpolation":"trilinear|tetrahedral"="trilinear","dither":bool=false,"data":json} (file: .cube/.3dl/.look path; data: file text + "fileName")
- `image.adjustments.colorLookup.list` List Color Lookup Looks: {}
- `image.adjustments.curves` Curves…: {"points":json,"red":json,"green":json,"blue":json} (curves as [[in,out],…] in 0..255, 2..19 points: points = composite; red/green/blue, gray, cyan/magenta/yellow/black or lightness/a/b per channel)
- `image.adjustments.desaturate` Desaturate: {}
- `image.adjustments.equalize` Equalize: {}
- `image.adjustments.exposure` Exposure…: {"exposure":-20..20=0,"offset":-0.5..0.5=0,"gamma":0.01..9.99=1}
- `image.adjustments.gradientMap` Gradient Map…: {"stops":json,"reverse":bool=false,"dither":bool=false} (stops [[location 0..1, "#rrggbb"], …], 2..64)
- `image.adjustments.hdrToning` HDR Toning…: {"radius":1..500=30,"strength":0.1..4=0.5,"gamma":0.1..2=1,"exposure":-5..5=0,"detail":-100..300=30,"shadow":-100..100=0,"highlight":-100..100=0,"vibrance":-100..100=0,"saturation":-100..100=20,"curve":[[in,out],…] 0..255} (flattens the image)
- `image.adjustments.hueSaturation` Hue/Saturation…: {"hue":-180..180=0,"saturation":-100..100=0,"lightness":-100..100=0,"colorize":bool=false,"reds":json,"yellows":json,"greens":json,"cyans":json,"blues":json,"magentas":json} (per range {"hue","saturation","lightness","range":[4 hue degrees]}; colorize: hue 0..360, saturation 0..100)
- `image.adjustments.invert` Invert: {}
- `image.adjustments.levels` Levels…: {"inBlack":0..253=0,"gamma":0.01..9.99=1,"inWhite":2..255=255,"outBlack":0..255=0,"outWhite":0..255=255,"red":json,"green":json,"blue":json} (top level = composite; per channel {"inBlack","gamma","inWhite","outBlack","outWhite"} under red/green/blue, gray, cyan/magenta/yellow/black or lightness/a/b)
- `image.adjustments.matchColor` Match Color…: {"source":doc,"sourceLayer":json,"luminance":1..200=100,"intensity":1..200=100,"fade":0..100=0,"neutralize":bool=false,"useSelectionInSource":bool=false,"useSelectionInTarget":bool=false} (source: document index; sourceLayer: layer id, default the merged image)
- `image.adjustments.photoFilter` Photo Filter…: {"filter":"warming85|warmingLBA|warming81|cooling80|coolingLBB|cooling82|red|orange|yellow|green|cyan|blue|violet|magenta|sepia|deepRed|deepBlue|deepEmerald|deepYellow|underwater","color":"#rrggbb","density":0..100=25,"preserveLuminosity":bool=true}
- `image.adjustments.posterize` Posterize…: {"levels":2..255=4}
- `image.adjustments.replaceColor` Replace Color…: {"color":json,"fuzziness":0..200=40,"hue":-180..180=0,"saturation":-100..100=0,"lightness":-100..100=0} (color: "#rrggbb", default the foreground colour)
- `image.adjustments.selectiveColor` Selective Color…: {"method":"relative|absolute"="relative","colors":"reds|yellows|greens|cyans|blues|magentas|whites|neutrals|blacks"="reds","cyan":-100..100=0,"magenta":-100..100=0,"yellow":-100..100=0,"black":-100..100=0,"reds":json} (per-range [c,m,y,k] arrays: reds yellows greens cyans blues magentas whites neutrals blacks)
- `image.adjustments.shadowsHighlights` Shadows/Highlights…: {"shadowAmount":0..100=35,"shadowTone":0..100=50,"shadowRadius":0..2500=30,"highlightAmount":0..100=0,"highlightTone":0..100=50,"highlightRadius":0..2500=30,"color":-100..100=20,"midtone":-100..100=0,"blackClip":0..50=0.01,"whiteClip":0..50=0.01}
- `image.adjustments.threshold` Threshold…: {"level":1..255=128}
- `image.adjustments.vibrance` Vibrance…: {"vibrance":-100..100=0,"saturation":-100..100=0}
- `image.analysis.countTool` Count Tool: {} → count groups and markers (edit with count.*)
- `image.analysis.info` Analysis Info: {} → scale, ruler readout, count groups, note and log counts
- `image.analysis.placeScaleMarker` Place Scale Marker…: {"length":logical units=nice ≈ width/5,"font":str?,"fontSize":pt=12,"displayText":bool=true,"textPosition":"top|bottom"="bottom","color":"black|white"="black"}
- `image.analysis.recordMeasurements` Record Measurements: {"source":"auto|selection|ruler|count"="auto"} → appended Measurement Log rows (selection: summary + one row per feature)
- `image.analysis.rulerTool` Ruler Tool: {"start":[x,y],"end":[x,y],"protractor":[x,y]|null?,"clear":bool=false} (no params: read) → X/Y/W/H/angle/L1/L2
- `image.analysis.selectDataPoints` Select Data Points…: {"selection":[keys]|{key:bool}?,"ruler":[keys]|{key:bool}?,"count":[keys]|{key:bool}?,"reset":bool=false} (keys: label,dateTime,document,source,scale,scaleUnits,scaleFactor,count,area,perimeter,circularity,height,width,grayMin,grayMax,grayMean,grayMedian,integratedDensity,histogram,length,angle)
- `image.analysis.setMeasurementScale` Set Measurement Scale…: {"preset":"default|custom"="custom","pixelLength":px,"logicalLength":number,"units":str (e.g. "mm")} (no params: read the scale)
- `image.analysis.straightenLayer` Straighten Layer: {"crop":bool=(active layer is the Background)} (rotates so the ruler line is level; crop = rotate the canvas and crop to the image)
- `image.applyDataSet` [file] Apply Data Set…: {name|index} → {applied, index}: sets layer visibility/text/pixels from the data set (one history step)
- `image.applyImage` Apply Image…: {"source":{"document":index?,"layer":id|"merged"="merged","channel":"composite"|"red|…"|index|"selection"|"transparency"|"mask"="composite","invert":bool=false},"blending":"normal|multiply|screen|overlay|softLight|hardLight|colorDodge|colorBurn|darken|lighten|difference|exclusion|linearBurn|linearDodge|add|subtract|…"="multiply","opacity":0..100=100,"scale":1..2=1,"offset":-255..255=0,"preserveTransparency":bool=false,"mask":{"document","layer","channel","invert"}?,"sourceChannel|sourceDocument|sourceLayer|sourceInvert|maskChannel|…":flat form of source/mask?}
- `image.autoColor` Auto Color: {}
- `image.autoContrast` Auto Contrast: {}
- `image.autoTone` Auto Tone: {}
- `image.calculations` Calculations…: {"source1":{"document","layer","channel","invert"},"source2":{…},"blending":"multiply|…"="multiply","opacity":0..100=100,"scale":1..2=1,"offset":-255..255=0,"mask":{…}?,"result":"newChannel|newDocument|selection"="newChannel","name":str?}
- `image.canvasSize` Canvas Size…: {"width":px,"height":px,"relative":bool=false,"anchor":"topLeft|top|topRight|left|center|right|bottomLeft|bottom|bottomRight"="center","extensionColor":"background|foreground|white|black|transparent|#rrggbb"="background"}
- `image.crop` Crop: {"x":px,"y":px,"width":px,"height":px,"deleteCroppedPixels":bool=true}
- `image.duplicate` Duplicate…: {"name":str,"mergedOnly":bool=false}
- `image.imageRotation.180` 180°: {}
- `image.imageRotation.90ccw` 90° Counter Clockwise: {}
- `image.imageRotation.90cw` 90° Clockwise: {}
- `image.imageRotation.flipCanvasHorizontal` Flip Canvas Horizontal: {}
- `image.imageRotation.flipCanvasVertical` Flip Canvas Vertical: {}
- `image.imageSize` Image Size…: {"width":px,"height":px,"resolution":ppi,"resample":"bicubic|bilinear|nearest|lanczos|preserveDetails|none"="bicubic"}
- `image.mode.bitmap` Bitmap…: {"method":"diffusion|threshold|pattern|halftone"="diffusion","frequency":1..999=53,"angle":-180..180=45,"shape":"round|ellipse|line|square|diamond|cross"="round"} (halftone frequency in lines/inch; flattens)
- `image.mode.bits16` 16 Bits/Channel: {}
- `image.mode.bits32` 32 Bits/Channel: {}
- `image.mode.bits8` 8 Bits/Channel: {}
- `image.mode.cmyk` [file] CMYK Color: {"profile":"<builtin id>|working|/path/to/profile.icc"=working,"intent":"perceptual|relative|saturation|absolute"="relative","bpc":bool=true}
- `image.mode.colorTable` Color Table…: {"table":"custom|blackBody|grayscale|spectrum|systemMac|systemWindows|web"="custom","colors":json,"entries":json,"transparent":json} (colors: ["#rrggbb", …]; entries: {"index": "#rrggbb"}; transparent: index|null)
- `image.mode.duotone` Duotone…: {"type":"duotone|monotone|tritone|quadtone"="duotone","inks":json} (inks: [{"name","color":"#rrggbb","curve":[[in,out],…] in 0..100}, …])
- `image.mode.grayscale` [file] Grayscale: {"profile":"<builtin id>|working|/path/to/profile.icc"=working,"intent":"perceptual|relative|saturation|absolute"="relative","bpc":bool=true}
- `image.mode.indexedColor` Indexed Color…: {"palette":"selective|perceptual|adaptive|exact|systemMac|systemWindows|web|uniform"="selective","colors":2..256=256,"forced":"blackWhite|none|primaries|web"="blackWhite","transparency":bool=true,"dither":"diffusion|none|pattern|noise"="diffusion","amount":0..100=75} (flattens)
- `image.mode.lab` [file] Lab Color: {"profile":"<builtin id>|working|/path/to/profile.icc"=working,"intent":"perceptual|relative|saturation|absolute"="relative","bpc":bool=true}
- `image.mode.multichannel` Multichannel: {} (flattens; RGB → Cyan/Magenta/Yellow, CMYK → Cyan/Magenta/Yellow/Black, Lab → Alpha 1–3, Grayscale → Black, Duotone → its inks)
- `image.mode.rgb` [file] RGB Color: {"profile":"<builtin id>|working|/path/to/profile.icc"=working,"intent":"perceptual|relative|saturation|absolute"="relative","bpc":bool=true}
- `image.revealAll` Reveal All: {}
- `image.rotation.arbitrary` Arbitrary…: {"angle":-359.99..359.99=0,"direction":"cw|ccw"="cw","interpolation":"bicubic|bilinear|nearest"="bicubic"}
- `image.trap` Trap…: {width:px>=1=1} → {trapped,width}: spread inks at colour edges (CMYK only)
- `image.trim` Trim…: {"basedOn":"transparent|topLeft|bottomRight"="transparent","top":bool=true,"bottom":bool=true,"left":bool=true,"right":bool=true}
- `image.variables.dataSets` Data Sets…: {dataSets:[{name, values:[{variable, kind:visibility|text|pixels, value}]}], append?} → {defs,dataSets,active}
- `image.variables.define` Define…: {defs:[{name, layer:id, type:visibility|textReplacement|pixelReplacement, method?:fit|fill|asIs|conform, align?, clip?}]} → {defs,dataSets,active}

## jobs

- `jobs.cancel` Cancel Background Job: {"job":u64? (default: every running job)}
- `jobs.list` List Background Jobs: {}

## layer

- `layer.addAboveToSelection` Add Layer Above to Selection: {} (follows the Layers panel's rows; collapsed groups are one row)
- `layer.addBelowToSelection` Add Layer Below to Selection: {} (follows the Layers panel's rows; collapsed groups are one row)
- `layer.align.bottomEdges` Bottom Edges: {"to":"auto|layers|selection|canvas"="auto"} (auto: the selection bounds with one layer and an active selection, else the selected layers' bounds)
- `layer.align.horizontalCenters` Horizontal Centers: {"to":"auto|layers|selection|canvas"="auto"} (auto: the selection bounds with one layer and an active selection, else the selected layers' bounds)
- `layer.align.leftEdges` Left Edges: {"to":"auto|layers|selection|canvas"="auto"} (auto: the selection bounds with one layer and an active selection, else the selected layers' bounds)
- `layer.align.rightEdges` Right Edges: {"to":"auto|layers|selection|canvas"="auto"} (auto: the selection bounds with one layer and an active selection, else the selected layers' bounds)
- `layer.align.topEdges` Top Edges: {"to":"auto|layers|selection|canvas"="auto"} (auto: the selection bounds with one layer and an active selection, else the selected layers' bounds)
- `layer.align.verticalCenters` Vertical Centers: {"to":"auto|layers|selection|canvas"="auto"} (auto: the selection bounds with one layer and an active selection, else the selected layers' bounds)
- `layer.arrange.bringForward` Bring Forward: {"layer":id?}
- `layer.arrange.bringToFront` Bring to Front: {"layer":id?}
- `layer.arrange.reverse` Reverse: {}
- `layer.arrange.sendBackward` Send Backward: {"layer":id?}
- `layer.arrange.sendToBack` Send to Back: {"layer":id?}
- `layer.artboard.set` Edit Artboard: {"layer":id? (default: the active artboard),"x","y","width","height"?|"rect":[x,y,w,h]?,"preset":str?,"background":"white|black|transparent|custom"?,"color":…?,"name":str?,"moveContents":bool=true} → {layer, rect}
- `layer.combineShapes.excludeOverlappingShapes` Exclude Overlapping Shapes: {"layer":id?,"subpath":index?}
- `layer.combineShapes.intersectShapeAreas` Intersect Shape Areas: {"layer":id?,"subpath":index?}
- `layer.combineShapes.mergeShapeComponents` Merge Shape Components: {"layer":id?,"tolerance":px=0.1} (bakes the path operations into plain combined outlines, traced from the exact coverage)
- `layer.combineShapes.subtractFrontShape` Subtract Front Shape: {"layer":id?,"subpath":index?}
- `layer.combineShapes.unite` Unite Shapes: {"layer":id?,"subpath":index? (default all but the first)}
- `layer.copyToDocument` Copy Layers to Document: {"document":index (destination),"layers":[id,…]?=the source's selected layers,"source":index?=active,"center":bool=false (centre on the destination's canvas),"at":[x,y]? (centre the copies on this point),"offset":[dx,dy]?=[0,0] (from their place in the source)}
- `layer.createClippingMask` Create Clipping Mask: {"layer":id?}
- `layer.delete` Delete Layer: {"layer":id?} (no layer: every selected layer)
- `layer.delete.hiddenLayers` Hidden Layers: {}
- `layer.distribute.bottomEdges` Bottom Edges: {}
- `layer.distribute.horizontalCenters` Horizontal Centers: {}
- `layer.distribute.horizontally` Horizontally: {} (equal horizontal gaps)
- `layer.distribute.leftEdges` Left Edges: {}
- `layer.distribute.rightEdges` Right Edges: {}
- `layer.distribute.topEdges` Top Edges: {}
- `layer.distribute.verticalCenters` Vertical Centers: {}
- `layer.distribute.vertically` Vertically: {} (equal vertical gaps)
- `layer.duplicate` Duplicate Layer…: {"layer":id?} (no layer: every selected layer)
- `layer.exportAs` [file] Export As…: {"layer":id?,"path":text,"scale":1..1000=100} (format from the path's extension)
- `layer.flattenImage` Flatten Image: {}
- `layer.groupLayers` Group Layers: {"layer":id?,"name":str?} (no layer: every selected layer)
- `layer.hideLayers` Hide Layers: {"layer":id?}
- `layer.layerContentOptions` Layer Content Options…: {}
- `layer.layerMask.apply` Apply: {"layer":id?}
- `layer.layerMask.delete` Delete: {"layer":id?}
- `layer.layerMask.enabled` Disable Layer Mask: {"layer":id?,"enabled":bool? (default: toggle)}
- `layer.layerMask.fromTransparency` From Transparency: {"layer":id?}
- `layer.layerMask.hideAll` Hide All: {"layer":id?}
- `layer.layerMask.hideSelection` Hide Selection: {"layer":id?}
- `layer.layerMask.linked` Unlink Layer Mask: {"layer":id?,"linked":bool? (default: toggle)}
- `layer.layerMask.revealAll` Reveal All: {"layer":id?}
- `layer.layerMask.revealSelection` Reveal Selection: {"layer":id?}
- `layer.layerStyle.bevelEmboss` Bevel & Emboss…: {"style":"inner|outer|emboss|pillow|stroke","technique":"smooth|chiselHard|chiselSoft","contour":bool,"contourRange":1..100=50,"texture":pattern id|name?,"textureScale":1..1000=100,"textureDepth":-1000..1000=100,"textureInvert":bool,"textureLink":bool=true,"depth":1..1000=100,"direction":"up|down","size":px=5,"soften":px,"angle":deg,"altitude":deg,"add":bool}
- `layer.layerStyle.blendingOptions` Blending Options…: {"layer":id?,"blend":"normal|multiply|…"?,"opacity":0..100?,"fillOpacity":0..100?,"blendIf":{"channel":"gray|red|green|blue|cyan|…"|index="gray","thisLayer":[black,white]|[blackLo,blackHi,whiteLo,whiteHi]?,"underlying":[…]?}|[{…},…]|null?} (Blend If values 0..255; split points fade; null resets)
- `layer.layerStyle.clear` Clear Layer Style: {"layer":id}
- `layer.layerStyle.colorOverlay` Color Overlay…: {"color":"#rrggbb","opacity":0..100=100,"blend":str,"add":bool}
- `layer.layerStyle.copyLayerStyle` Copy Layer Style: {"layer":id?}
- `layer.layerStyle.createLayer` Create Layer: {"layer":id?}
- `layer.layerStyle.defaultFor` [host] Layer Style Default: {"kind":str} → {"params":{…},"user":bool} (the saved user default, else the factory defaults)
- `layer.layerStyle.dropShadow` Drop Shadow…: {"color":"#rrggbb","opacity":0..100=75,"blend":str="multiply","angle":deg=120,"useGlobalLight":bool,"distance":px=5,"spread":0..100,"size":px=5,"contour":"Linear|Cone|Cone (Inverted)|Domed|Domed (Inverted)|Diagonal (Descending)"="Linear","noise":0..100,"knocksOut":bool,"add":bool,"layer":id}
- `layer.layerStyle.globalLight` Global Light…: {"angle":-180..180=120,"altitude":0..90=30}
- `layer.layerStyle.gradientOverlay` Gradient Overlay…: {"from":"#rrggbb","to":"#rrggbb","style":"linear|radial|angle|reflected|diamond","angle":deg=90,"scale":10..150=100,"reverse":bool,"opacity":0..100,"blend":str,"add":bool}
- `layer.layerStyle.hideAllEffects` Hide All Effects: {}
- `layer.layerStyle.innerGlow` Inner Glow…: {"color":"#rrggbb","opacity":0..100=75,"blend":str="screen","technique":"softer|precise","source":"edge|center","choke":0..100,"size":px,"contour":name,"noise":0..100,"add":bool}
- `layer.layerStyle.innerShadow` Inner Shadow…: {"color":"#rrggbb","opacity":0..100=75,"blend":str,"angle":deg,"distance":px,"choke":0..100,"size":px,"contour":name,"noise":0..100,"add":bool}
- `layer.layerStyle.makeDefault` [host] Make Layer Style Default: {"kind":"stroke|dropShadow|innerShadow|outerGlow|innerGlow|colorOverlay|gradientOverlay|patternOverlay|satin|bevelEmboss","params":{…param set…}} (stores the user default the Layer Style dialog's Reset to Default restores)
- `layer.layerStyle.outerGlow` Outer Glow…: {"color":"#rrggbb","opacity":0..100=75,"blend":str="screen","technique":"softer|precise","spread":0..100,"size":px,"range":0..100,"contour":name,"noise":0..100,"add":bool}
- `layer.layerStyle.pasteLayerStyle` Paste Layer Style: {"layer":id?}
- `layer.layerStyle.patternOverlay` Pattern Overlay…: {"pattern":id|name?=first library pattern,"opacity":0..100=100,"blend":str,"scale":1..1000=100,"angle":deg=0,"link":bool=true,"phaseX":px,"phaseY":px,"add":bool}
- `layer.layerStyle.replace` Edit Layer Style: {"layer":id,"effects":[{"kind":"dropShadow|innerShadow|outerGlow|innerGlow|stroke|colorOverlay|gradientOverlay|patternOverlay|satin|bevelEmboss","params":{…layer.layerStyle.<kind> params…},"fx":<effect snapshot carried through the Layer Style dialog, optional>}]} (replaces the layer's whole effect list; an entry with `fx` edits that effect with the params present, one without builds from params)
- `layer.layerStyle.satin` Satin…: {"color":"#rrggbb","opacity":0..100=50,"blend":str,"angle":deg,"distance":px,"size":px,"invert":bool,"add":bool}
- `layer.layerStyle.scaleEffects` Scale Effects…: {"scale":1..1000=100}
- `layer.layerStyle.showAllEffects` Show All Effects: {}
- `layer.layerStyle.stroke` Stroke…: {"size":px=3,"position":"outside|inside|center","color":"#rrggbb","from":"#rrggbb","to":"#rrggbb","style":str,"angle":deg,"opacity":0..100,"blend":str,"add":bool}
- `layer.linkLayers` Link Layers: {} (toggles: unlinks when the selection is already one link group)
- `layer.lockLayers` Lock Layers…: {"transparency":bool?,"pixels":bool?,"position":bool?,"artboard":bool?,"all":bool?} (none given: toggle lock all)
- `layer.maskAllObjects` Mask All Objects: {"layer":id?}
- `layer.matting.colorDecontaminate` Color Decontaminate…: {"amount":0..100=100,"radius":1..100=4}
- `layer.matting.defringe` Defringe…: {"width":1..200=1}
- `layer.matting.removeBlackMatte` Remove Black Matte: {}
- `layer.matting.removeWhiteMatte` Remove White Matte: {}
- `layer.mergeDown` Merge Down: {"layer":id?}
- `layer.mergeLayers` Merge Layers: {} (one layer selected: Merge Down)
- `layer.mergeVisible` Merge Visible: {}
- `layer.moveTo` Reorder Layer: {"layer":id?,"target":id,"position":"above|below|into"="above"}
- `layer.new.artboard` Artboard…: {"rect":[x,y,w,h]? | "x","y","width","height"? (default: canvas size, right of the last board),"preset":"iPhone 14|Web 1920|A4|…"?,"name":str?,"background":"white|black|transparent|custom"="white","color":[r,g,b]|"#rrggbb"? (custom)} → {layer, rect}
- `layer.new.artboardFromGroup` Artboard from Group…: {"layer":id? (a top-level group; default active),"background":…?} → {layer, rect}
- `layer.new.artboardFromLayers` Artboard from Layers…: {"name":str?,"background":…?} (the selected layers) → {layer, rect}
- `layer.new.frameFromLayers` Frame from Layers: {"name":str?} → {layer, frame:[x,y,w,h]}: groups the selected layers and clips them to a frame rect
- `layer.new.group` Group…: {"name":str?}
- `layer.new.groupFromLayers` Group from Layers…: {"name":str?}
- `layer.new.layer` Layer…: {"name":str?}
- `layer.new.layerFromBackground` Layer From Background…: {}
- `layer.new.layerViaCopy` Layer via Copy: {}
- `layer.new.layerViaCut` Layer via Cut: {}
- `layer.newAdjustmentLayer.blackWhite` Black & White…: {"reds":-200..300=40,"yellows":-200..300=60,"greens":-200..300=40,"cyans":-200..300=60,"blues":-200..300=20,"magentas":-200..300=80,"tint":bool=false,"tintColor":"#rrggbb"}
- `layer.newAdjustmentLayer.brightnessContrast` Brightness/Contrast…: {"brightness":-150..150=0,"contrast":-50..100=0,"legacy":bool=false}
- `layer.newAdjustmentLayer.channelMixer` Channel Mixer…: {"red":json,"green":json,"blue":json,"gray":json,"monochrome":bool=false} (per output channel [red %, green %, blue %, constant %] in -200..200; gray = the monochrome mix)
- `layer.newAdjustmentLayer.colorBalance` Color Balance…: {"shadows":json,"midtones":json,"highlights":json,"preserveLuminosity":bool=true} (each tone [cyan-red, magenta-green, yellow-blue] in -100..100)
- `layer.newAdjustmentLayer.colorLookup` [file] Color Lookup…: {"lut":"none|warm|cool|tealOrange|bleachBypass|fadedFilm|dayForNight|monoContrast|crossProcess"="none","file":text,"interpolation":"trilinear|tetrahedral"="trilinear","dither":bool=false,"data":json} (file: .cube/.3dl/.look path; data: file text + "fileName")
- `layer.newAdjustmentLayer.curves` Curves…: {"points":json,"red":json,"green":json,"blue":json} (curves as [[in,out],…] in 0..255, 2..19 points: points = composite; red/green/blue, gray, cyan/magenta/yellow/black or lightness/a/b per channel)
- `layer.newAdjustmentLayer.exposure` Exposure…: {"exposure":-20..20=0,"offset":-0.5..0.5=0,"gamma":0.01..9.99=1}
- `layer.newAdjustmentLayer.gradientMap` Gradient Map…: {"stops":json,"reverse":bool=false,"dither":bool=false} (stops [[location 0..1, "#rrggbb"], …], 2..64)
- `layer.newAdjustmentLayer.hueSaturation` Hue/Saturation…: {"hue":-180..180=0,"saturation":-100..100=0,"lightness":-100..100=0,"colorize":bool=false,"reds":json,"yellows":json,"greens":json,"cyans":json,"blues":json,"magentas":json} (per range {"hue","saturation","lightness","range":[4 hue degrees]}; colorize: hue 0..360, saturation 0..100)
- `layer.newAdjustmentLayer.invert` Invert: {}
- `layer.newAdjustmentLayer.levels` Levels…: {"inBlack":0..253=0,"gamma":0.01..9.99=1,"inWhite":2..255=255,"outBlack":0..255=0,"outWhite":0..255=255,"red":json,"green":json,"blue":json} (top level = composite; per channel {"inBlack","gamma","inWhite","outBlack","outWhite"} under red/green/blue, gray, cyan/magenta/yellow/black or lightness/a/b)
- `layer.newAdjustmentLayer.photoFilter` Photo Filter…: {"filter":"warming85|warmingLBA|warming81|cooling80|coolingLBB|cooling82|red|orange|yellow|green|cyan|blue|violet|magenta|sepia|deepRed|deepBlue|deepEmerald|deepYellow|underwater","color":"#rrggbb","density":0..100=25,"preserveLuminosity":bool=true}
- `layer.newAdjustmentLayer.posterize` Posterize…: {"levels":2..255=4}
- `layer.newAdjustmentLayer.selectiveColor` Selective Color…: {"method":"relative|absolute"="relative","colors":"reds|yellows|greens|cyans|blues|magentas|whites|neutrals|blacks"="reds","cyan":-100..100=0,"magenta":-100..100=0,"yellow":-100..100=0,"black":-100..100=0,"reds":json} (per-range [c,m,y,k] arrays: reds yellows greens cyans blues magentas whites neutrals blacks)
- `layer.newAdjustmentLayer.threshold` Threshold…: {"level":1..255=128}
- `layer.newAdjustmentLayer.vibrance` Vibrance…: {"vibrance":-100..100=0,"saturation":-100..100=0}
- `layer.newFillLayer.gradient` Gradient…: {"from":"#rrggbb","to":"#rrggbb","angle":deg=90,"style":"linear|radial|angle|reflected|diamond","reverse":bool}
- `layer.newFillLayer.pattern` Pattern…: {"pattern":id|name?=first library pattern,"scale":1..1000=100,"angle":deg=0,"link":bool=true,"phase":[x,y]?}
- `layer.newFillLayer.solidColor` Solid Color…: {"color":"#rrggbb"=foreground}
- `layer.newLayerBasedSlice` New Layer Based Slice: {"layer":id?,"name":str?,"url":str?,"alt":str?} (follows the layer's bounds with effects) → {slice, rect}
- `layer.pickAt` Auto-Select Layer: {"x":px,"y":px,"target":"layer|group"="layer","select":bool=true,"mode":"replace|toggle|add"="replace","list":bool=false (return every layer with pixels there, topmost first)} → {layer} | {layers}
- `layer.quickExportAsPng` [file] Quick Export as PNG: {"layer":id?,"path":text}
- `layer.rasterize.allLayers` All Layers: {}
- `layer.rasterize.fillContent` Fill Content: {"layer":id?}
- `layer.rasterize.layer` Layer: {"layer":id?}
- `layer.rasterize.shape` Rasterize Shape: {"layer":id?}
- `layer.rasterize.smartObject` Smart Object: {"layer":id?}
- `layer.rasterize.type` Type: {"layer":id?}
- `layer.rasterize.vectorMask` Rasterize Vector Mask: {"layer":id?} (multiplied into the layer mask)
- `layer.rasterize.video` Video: {} → {}
- `layer.releaseClippingMask` Release Clipping Mask: {"layer":id?}
- `layer.removeBackground` Remove Background: {"layer":id?,"sampleAllLayers":bool=false,"refine":bool=true} → {layer,bounds} (adds a layer mask from Select Subject; the Background becomes a normal layer)
- `layer.renameLayer` Rename Layer: {"layer":id?,"name":str}
- `layer.select` Select Layer: {"layer":id,"mode":"replace|toggle|range|add"="replace"} (toggle = ⌘-click, range = ⇧-click)
- `layer.selectAbove` Select Layer Above: {} (follows the Layers panel's rows; collapsed groups are one row)
- `layer.selectBelow` Select Layer Below: {} (follows the Layers panel's rows; collapsed groups are one row)
- `layer.selectBottom` Select Bottom Layer: {} (follows the Layers panel's rows; collapsed groups are one row)
- `layer.selectLinkedLayers` Select Linked Layers: {}
- `layer.selectTop` Select Top Layer: {} (follows the Layers panel's rows; collapsed groups are one row)
- `layer.setAdjustment` [file] Adjustment Properties: {"layer":id?, …params of that adjustment kind}
- `layer.setEffectsExpanded` [host] Expand/Collapse Effects: {"layer":id?,"expanded":bool?,"all":bool?} (no expanded: toggle; all: every layer with effects; view state, not an undo step)
- `layer.setExpanded` Expand/Collapse Group: {"layer":id?,"expanded":bool?,"all":bool?} (no expanded: toggle; all: every group; not an undo step)
- `layer.setProps` Layer Properties: {"layer":id?,"name":str?,"visible":bool?,"opacity":0..1?,"fill":0..1?,"blend":"Multiply|…"?,"clipped":bool?,"locked":bool?,"locks":{"transparency","pixels","position","artboard","all":bool}?,"channels":[bool,…]? (Advanced Blending: which colour channels blend, R G B / C M Y K / L a b)}
- `layer.showLayers` Show Layers: {"layer":id?}
- `layer.showOnly` Show Only This Layer: {"layer":id?} → {shownAlone} (⌥-click a layer's eye; again restores the other layers' visibility)
- `layer.smartFilter.blendingOptions` Blending Options…: {"layer":id?,"index":u32? (0 = bottom; default top),"blend":str?,"opacity":0..1?}
- `layer.smartFilter.clearSmartFilters` Clear Smart Filters: {"layer":id?}
- `layer.smartFilter.delete` Delete Smart Filter: {"layer":id?,"index":u32?}
- `layer.smartFilter.deleteFilterMask` Delete Filter Mask: {"layer":id?}
- `layer.smartFilter.disableFilterMask` Disable Filter Mask: {"layer":id?,"enabled":bool? (default: toggle)}
- `layer.smartFilter.disableSmartFilters` Disable Smart Filters: {"layer":id?,"enabled":bool? (default: toggle)}
- `layer.smartFilter.move` Move Smart Filter: {"layer":id?,"index":u32?,"to":u32}
- `layer.smartFilter.setParams` [file] Edit Smart Filter: {"layer":id?,"index":u32?,"params":{…} (merged)}
- `layer.smartFilter.setVisible` Show/Hide Smart Filter: {"layer":id?,"index":u32?,"visible":bool? (default: toggle)}
- `layer.smartObjects.convertToEmbedded` [file] Convert to Embedded: {"layer":id?}
- `layer.smartObjects.convertToLayers` [file] Convert to Layers: {"layer":id?} (contents unpacked at the placement: one layer, or a group named after the smart object; smart filters are discarded)
- `layer.smartObjects.convertToLinked` [file] Convert to Linked…: {"layer":id?,"path":str} (writes the contents there)
- `layer.smartObjects.convertToSmartObject` Convert to Smart Object: {"layer":id?}
- `layer.smartObjects.editContents` [file] Edit Contents: {"layer":id?} → opens the contents as a new document; saving (layer.smartObjects.saveContents) or closing it updates the smart object
- `layer.smartObjects.exportContents` [file] Export Contents…: {"layer":id?,"path":str}
- `layer.smartObjects.newSmartObjectViaCopy` [file] New Smart Object via Copy: {"layer":id?}
- `layer.smartObjects.perspectiveWarp` Perspective Warp: {"planes":[{"src":[[x,y]×4],"dst":[[x,y]×4]}…] (corners clockwise from top-left; corners that coincide in src are linked),"straighten":"horizontal|vertical|auto"?,"interpolation":"bicubic|bilinear|nearest","layer":id?}
- `layer.smartObjects.puppetWarp` Puppet Warp: {"pins":[{"src":[x,y],"dst":[x,y],"rotate":deg?,"depth":n?}…],"mode":"rigid|normal|distort"="normal","density":"fewer|normal|more"="normal","expansion":px=2,"interpolation":"bicubic|bilinear|nearest","layer":id?} — mesh over the opaque region, as-rigid-as-possible; on a smart object it becomes a smart filter
- `layer.smartObjects.rasterize` Rasterize: {"layer":id?}
- `layer.smartObjects.relinkToFile` [file] Relink to File…: {"layer":id?,"path":str}
- `layer.smartObjects.replaceContents` [file] Replace Contents…: {"layer":id?,"path":str}
- `layer.smartObjects.revealInFinder` [host] Reveal in Finder: {"dryRun":bool=false}
- `layer.smartObjects.saveContents` Save Contents: {} (in an Edit Contents document)
- `layer.smartObjects.stackMode.entropy` Entropy: {"layer":id?}
- `layer.smartObjects.stackMode.kurtosis` Kurtosis: {"layer":id?}
- `layer.smartObjects.stackMode.maximum` Maximum: {"layer":id?}
- `layer.smartObjects.stackMode.mean` Mean: {"layer":id?}
- `layer.smartObjects.stackMode.median` Median: {"layer":id?}
- `layer.smartObjects.stackMode.minimum` Minimum: {"layer":id?}
- `layer.smartObjects.stackMode.none` None: {"layer":id?}
- `layer.smartObjects.stackMode.range` Range: {"layer":id?}
- `layer.smartObjects.stackMode.skewness` Skewness: {"layer":id?}
- `layer.smartObjects.stackMode.standardDeviation` Standard Deviation: {"layer":id?}
- `layer.smartObjects.stackMode.summation` Summation: {"layer":id?}
- `layer.smartObjects.stackMode.variance` Variance: {"layer":id?}
- `layer.smartObjects.updateAllModifiedContent` [file] Update All Modified Content: {}
- `layer.smartObjects.updateModifiedContent` [file] Update Modified Content: {"layer":id?}
- `layer.smartObjects.warp` Warp: {"layer":id?,"rect":[x0,y0,x1,y1]? (warp box; default = layer content ∩ selection),"style":"custom|none|arc|arcLower|arcUpper|arch|bulge|shellLower|shellUpper|flag|wave|fish|rise|fisheye|inflate|squeeze|twist","bend":%=50,"hDistort":%,"vDistort":%,"vertical":bool,"mesh":{"us":[0,…,1],"vs":[0,…,1],"points":[[x,y]…]} ((3c+1)×(3r+1) control points, row-major, document px),"grid":[cols,rows]?,"warp":{full warp object}?,"interpolation":"bicubic|bilinear|nearest"}
- `layer.stampDown` Stamp Down: {} → {layer} (a copy of the active layer merged into the pixel layer below; several selected: into a new layer above them)
- `layer.stampVisible` Stamp Visible: {} → {layer} (every visible layer merged into a new layer above the active one; the originals stay)
- `layer.translate` Move Layer: {"layer":id?,"dx":i32,"dy":i32} (no layer: every selected layer; linked layers follow)
- `layer.ungroupLayers` Ungroup Layers: {"layer":id?}
- `layer.vectorMask.add` Add Vector Mask: {"layer":id?,"path":{…}? | "name":str|"work"? (copy a document path),"hide":bool=false (Hide All = inverted)} → vector mask info. Without a path the mask reveals all.
- `layer.vectorMask.currentPath` Vector Mask: Current Path: {"layer":id?,"name":str|"work"="work"}
- `layer.vectorMask.delete` Delete Vector Mask: {"layer":id?}
- `layer.vectorMask.edit` Edit Vector Mask: {"layer":id?,"path":{…}?,"enabled":bool?,"linked":bool?,"density":0..100?,"feather":px?,"invert":true?}. path: {"subpaths":[{"closed":bool=true,"op":"combine|subtract|intersect|exclude","knots":[[x,y] | {"anchor":[x,y],"in":[x,y],"out":[x,y],"smooth":bool}]}],"fillRule":"nonzero|evenodd","inverted":bool}
- `layer.vectorMask.enabled` Enable Vector Mask: {"layer":id?,"enabled":bool? (default: toggle)}
- `layer.vectorMask.fromPath` Vector Mask from Current Path: {"layer":id?,"name":str|"work"="work"}
- `layer.vectorMask.hideAll` Vector Mask: Hide All: {"layer":id?}
- `layer.vectorMask.info` Vector Mask Info: {"layer":id?} → {layer,path,enabled,linked,density,feather}
- `layer.vectorMask.linked` Link Vector Mask: {"layer":id?,"linked":bool? (default: toggle)}
- `layer.vectorMask.revealAll` Vector Mask: Reveal All: {"layer":id?}
- `layer.videoLayers.deleteFrame` Delete Frame: {} → {}
- `layer.videoLayers.duplicateFrame` Duplicate Frame: {} → {}
- `layer.videoLayers.insertBlankFrame` Insert Blank Frame: {} → {}
- `layer.videoLayers.interpretFootage` Interpret Footage…: {} → {}
- `layer.videoLayers.newBlankVideoLayer` New Blank Video Layer: {} → {}
- `layer.videoLayers.newVideoLayerFromFile` [file] New Video Layer from File…: {} → {}
- `layer.videoLayers.rasterize` Rasterize: {} → {}
- `layer.videoLayers.reloadFrame` [file] Reload Frame: {} → {}
- `layer.videoLayers.replaceFootage` [file] Replace Footage…: {} → {}
- `layer.videoLayers.restoreAllFrames` Restore All Frames: {} → {}
- `layer.videoLayers.restoreFrame` Restore Frame: {} → {}
- `layer.videoLayers.showAlteredVideo` Show Altered Video: {} → {}

## layerComp

- `layerComp.apply` Apply Layer Comp: {"comp":id|name? (default: last applied)} → {comp, missingLayers}
- `layerComp.delete` Delete Layer Comp: {"comp":id|name?}
- `layerComp.duplicate` Duplicate Layer Comp: {"comp":id|name?} → {comp}
- `layerComp.list` List Layer Comps: {} → {comps:[{id,name,comment,visibility,position,appearance,layers,missingLayers}],lastApplied,hasLastDocumentState}
- `layerComp.new` New Layer Comp…: {"name":str="Layer Comp N","comment":str="","visibility":bool=true,"position":bool=true,"appearance":bool=true} → {comp}
- `layerComp.next` Apply Next Layer Comp: {}
- `layerComp.previous` Apply Previous Layer Comp: {}
- `layerComp.rename` Rename Layer Comp: {"comp":id|name?,"name":str}
- `layerComp.restoreLastDocumentState` Restore Last Document State: {}
- `layerComp.setComment` Layer Comp Comment: {"comp":id|name?,"comment":str}
- `layerComp.setOptions` Layer Comp Options…: {"comp":id|name?,"visibility":bool?,"position":bool?,"appearance":bool?,"name":str?,"comment":str?}
- `layerComp.update` Update Layer Comp: {"comp":id|name|"*"? (default: last applied; "*" = all),"what":"all|visibility|position|appearance"="all"}
- `layerComp.updateWarnings` Layer Comp Warnings: {"clear":bool=false (drop states of deleted layers)} → {warnings:[{comp,name,missingLayers}],count}

## measurementLog

- `measurementLog.delete` Delete Measurements: {"rows":[ids]}|{"all":true}
- `measurementLog.export` [file] Export Measurements…: {"path":str? (CSV file; omitted → returns the CSV text),"rows":[ids]?}
- `measurementLog.list` Measurement Log: {} → rows and columns

## notes

- `notes.add` New Note: {"x":px,"y":px,"text":str="","author":str="","color":"#rrggbb"|[r,g,b]=pale yellow,"open":bool=true}
- `notes.delete` Delete Note: {"index":n}|{"all":true}
- `notes.list` Notes: {} → notes (index, author, text, colour, position, open, modified)
- `notes.set` Edit Note: {"index":n,"text":str?,"author":str?,"color":"#rrggbb"|[r,g,b]?,"x":px?,"y":px?,"open":bool?}

## paint

- `paint.backgroundEraser` Background Eraser: {"points":[[x,y,pressure?],…],"size":px?,"hardness":0..1?,"brush":{…}?,"preset":name?,"sampling":"continuous|once|backgroundSwatch"="continuous","limits":"discontiguous|contiguous|findEdges"="contiguous","tolerance":0..100=50,"protectForegroundColor":bool=false,"seed":u64?}
- `paint.blur` Blur: {"points":[[x,y,pressure?],…],"size":1..5000 px=tool size,"hardness":0..100=tool hardness,"opacity":1..100=100,"flow":1..100=100,"spacing":1..1000 (% of size)=25,"layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target,"strength":1..100=50,"sampleAllLayers":bool=false}
- `paint.bucket` Paint Bucket: {"x":px,"y":px,"tolerance":0..255=32,"contiguous":bool=true,"antiAlias":bool=true,"contents":"foreground|pattern"="foreground","color":"#rrggbb"=foreground,"pattern":id|name (contents=pattern),"scale":%=100,"angle":deg,"opacity":1..100=100,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target}
- `paint.burn` Burn: {"points":[[x,y,pressure?],…],"size":1..5000 px=tool size,"hardness":0..100=tool hardness,"opacity":1..100=100,"flow":1..100=100,"spacing":1..1000 (% of size)=25,"layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target,"range":"shadows|midtones|highlights"="midtones","exposure":1..100=50,"protectTones":bool=true}
- `paint.cloneStamp` Clone Stamp: {"points":[[x,y,pressure?],…],"size":1..5000 px=tool size,"hardness":0..100=tool hardness,"opacity":1..100=100,"flow":1..100=100,"spacing":1..1000 (% of size)=25,"layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target,"source":[sx,sy] (sampled under the first point) | "offset":[dx,dy],"aligned":bool=true,"sampleLayer":"current|currentAndBelow|all"="current","mode":"normal|multiply|…"="normal" → {"damage","offset","aligned","nextSource"}}
- `paint.colorReplacement` Color Replacement: {"points":[…],"brush":{…}?,"size":px?,"mode":"hue|saturation|color|luminosity"="color","sampling":"continuous|once|backgroundSwatch"="continuous","limits":"contiguous|discontiguous|findEdges"="contiguous","tolerance":0..100=30,"antiAlias":bool=true,"color":"#rrggbb"?=foreground,"seed":u64?}
- `paint.contentAwareMove` Content-Aware Move: {"offset":[dx,dy] (how far the selection was dragged),"mode":"move|extend"="move","structure":1..7=4 (7 keeps the content up to its edge, lower blends a wider edge band),"color":0..10=0 (how far the content's colour adapts to its new place),"sampleAllLayers":bool=false,"layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target} → {"damage","offset","mode"} (a background job; the selection moves with the content)
- `paint.dodge` Dodge: {"points":[[x,y,pressure?],…],"size":1..5000 px=tool size,"hardness":0..100=tool hardness,"opacity":1..100=100,"flow":1..100=100,"spacing":1..1000 (% of size)=25,"layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target,"range":"shadows|midtones|highlights"="midtones","exposure":1..100=50,"protectTones":bool=true}
- `paint.gradient` Gradient: {"from":[x,y],"to":[x,y],"style":"linear|radial|angle|reflected|diamond"="linear","colors":["#rrggbb",…]? (evenly spaced),"gradient":preset name?,"stops":[[t,"#rrggbb"|"foreground"|"background"],…]?,"transparency":[[t,0..100],…]? (default: the current gradient, see gradient.presets.select),"reverse":bool=false,"dither":bool=true,"opacity":1..100=100,"mode":"normal|multiply|…"="normal","target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target}
- `paint.healingBrush` Healing Brush: {"points":[[x,y,pressure?],…],"size":1..5000 px=tool size,"hardness":0..100=tool hardness,"opacity":1..100=100,"flow":1..100=100,"spacing":1..1000 (% of size)=25,"layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target,"source":[sx,sy] | "offset":[dx,dy],"aligned":bool=true,"sampleLayer":"current|currentAndBelow|all"="current","mode":"normal|…"="normal" → {"damage","offset","aligned","nextSource"}}
- `paint.historyBrush` History Brush: {"points":[[x,y,pressure?],…],"size":1..5000 px=tool size,"hardness":0..100=tool hardness,"opacity":1..100=100,"flow":1..100=100,"spacing":1..1000 (% of size)=25,"layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target,"state":index (into history.entries; default 0 = the oldest held state, normally the Open snapshot)}
- `paint.magicEraser` Magic Eraser: {"x":px,"y":px,"tolerance":0..255=32,"antiAlias":bool=true,"contiguous":bool=true,"sampleAllLayers":bool=false,"opacity":1..100=100}
- `paint.mixerBrush` Mixer Brush: {"points":[…],"brush":{…}?,"preset":name?,"size":0.5..5000 px?,"wet":0..100=brush.mixer.wet,"load":0..100=brush.mixer.load,"mix":0..100=brush.mixer.mix,"flow":0..100=brush.mixer.flow,"color":"#rrggbb"?=foreground,"sampleAllLayers":bool=brush.mixer.sampleAllLayers,"cleanAfterStroke":bool=true,"loadAfterStroke":bool=true,"seed":u64?}
- `paint.patch` Patch: {"offset":[dx,dy] (how far the selection was dragged),"mode":"source|destination"="source","layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target} → {"damage","offset"}
- `paint.pencil` Pencil: {"points":[[x,y,pressure?,tiltX?,tiltY?,rotation?,timeMs?,wheel?],…],"brush":{…}?,"preset":name?,"size":0.5..5000 px?,"opacity":0..1?,"color":"#rrggbb"?=foreground,"mode":"normal|multiply|screen|…"="normal","erase":bool?,"autoErase":bool=false,"seed":u64?,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target}
- `paint.sharpen` Sharpen: {"points":[[x,y,pressure?],…],"size":1..5000 px=tool size,"hardness":0..100=tool hardness,"opacity":1..100=100,"flow":1..100=100,"spacing":1..1000 (% of size)=25,"layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target,"strength":1..100=50,"protectDetail":bool=true,"sampleAllLayers":bool=false}
- `paint.smudge` Smudge: {"points":[[x,y,pressure?],…],"size":1..5000 px=tool size,"hardness":0..100=tool hardness,"opacity":1..100=100,"flow":1..100=100,"spacing":1..1000 (% of size)=25,"layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target,"strength":1..100=50,"fingerPainting":bool=false (starts with the foreground colour),"sampleAllLayers":bool=false}
- `paint.sponge` Sponge: {"points":[[x,y,pressure?],…],"size":1..5000 px=tool size,"hardness":0..100=tool hardness,"opacity":1..100=100,"flow":1..100=100,"spacing":1..1000 (% of size)=25,"layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target,"mode":"desaturate|saturate"="desaturate","vibrance":bool=true (flow = sponge strength)}
- `paint.spotHealing` Spot Healing Brush: {"points":[[x,y,pressure?],…],"size":1..5000 px=tool size,"hardness":0..100=tool hardness,"opacity":1..100=100,"flow":1..100=100,"spacing":1..1000 (% of size)=25,"layer":id?=active,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target,"type":"contentAware|createTexture|proximityMatch"="contentAware","sampleAllLayers":bool=false}
- `paint.stroke` Brush Stroke: {"points":[[x,y,pressure?,tiltX?,tiltY?,rotation?,timeMs?,wheel?],…],"brush":{…BrushSettings}?,"preset":name?,"size":0.5..5000 px?,"hardness":0..1?,"opacity":0..1?,"flow":0..1?,"spacing":0..10?,"color":"#rrggbb"?=foreground,"mode":"normal|multiply|screen|…"="normal","erase":bool?,"smoothing":0..1?,"zoom":number=1,"seed":u64?,"target":"pixels"|"mask"|"quickMask"|{"channel":i}=Channels panel target}
- `paint.symmetryDisable` Disable Symmetry Path: {}
- `paint.symmetryFromPath` Make Symmetry Path: {"name":"work"|"layer"|savedPathName="work"} — mirror Brush, Pencil and Eraser strokes across the path

## path

- `path.bendSegment` Drag Segment: {"name":str|"work"|"layer"="work","layer":id? (with "layer": a shape layer's path or a vector mask),"subpath":i,"knot":j (the segment leaving it),"t":0..1=0.5 (where it was grabbed),"move":[dx,dy] (a straight segment moves with its anchors; a curve reshapes through the dragged point)} → {name,path}
- `path.clippingPath.clear` Clear Clipping Path: {}
- `path.clippingPath.set` Clipping Path: {"name":savedPathName,"flatness":0..100=0} (PSD export clipping path)
- `path.convertPoint` Convert Point: {"name":str|"work"|"layer"="work","layer":id? (with "layer": a shape layer's path or a vector mask),"subpath":i,"knot":j,"out":[x,y]? (none: corner with retracted handles; given: smooth with this out handle and its mirror)} → {name,path}
- `path.delete` Delete Path: {"name":str|"work"="work"}
- `path.fill` Fill Path: {"name":str|"work"|"layer"="work","layer":id? (pixel layer, default active),"color":"#rrggbb"=foreground,"opacity":0..100=100,"mode":"normal|multiply|…"="normal","feather":px=0,"antiAlias":bool=true}
- `path.info` Path Info: {"name":str|"work"|"layer"="work"} → {name,path}
- `path.list` List Paths: {} → {paths:[{name,knots,subpaths}],workPath,clippingPath,layerPath (active layer's shape path / vector mask)}
- `path.moveAnchors` Drag Anchor Points: {"name":str|"work"|"layer"="work","layer":id? (with "layer": a shape layer's path or a vector mask),"anchors":[[subpath,knot],…],"move":[dx,dy] (anchors move with their handles; Direct Selection drag)} → {name,path}
- `path.moveHandle` Drag Direction Point: {"name":str|"work"|"layer"="work","layer":id? (with "layer": a shape layer's path or a vector mask),"subpath":i,"knot":j,"handle":"in|out","to":[x,y],"independent":bool=false (a smooth point turns its other handle to stay collinear; independent = ⌥ / Convert Point, makes a corner)} → {name,path}
- `path.rename` Rename Path: {"name":str|"work"="work","to":str} (renaming the work path saves it)
- `path.set` Set Path: {"name":str|"work"="work","path":{…},"op":"combine|subtract|intersect|exclude"? (append to the existing path with this op instead of replacing)}. path: {"subpaths":[{"closed":bool=true,"op":"combine|subtract|intersect|exclude","knots":[[x,y] | {"anchor":[x,y],"in":[x,y],"out":[x,y],"smooth":bool}]}],"fillRule":"nonzero|evenodd","inverted":bool}
- `path.stroke` Stroke Path: {"name":str|"work"|"layer"="work","layer":id? (pixel layer),"tool":"brush|pencil|eraser"="brush","size":0.5..5000 px?,"hardness":0..1?,"opacity":0..100?,"color":"#rrggbb"=foreground} (current brush settings otherwise)
- `path.style.copyFill` Copy Fill: {} (copies active shape layer fill)
- `path.style.copyStroke` Copy Complete Stroke: {} (copies active shape layer stroke)
- `path.style.pasteFill` Paste Fill: {"layer":id?} (pastes copied fill onto active shape layer)
- `path.style.pasteStroke` Paste Complete Stroke: {"layer":id?} (pastes copied stroke onto active shape layer)
- `path.toSelection` Make Selection from Path: {"name":str|"work"|"layer"="work","feather":px=0,"antiAlias":bool=true,"mode":"replace|add|subtract|intersect"="replace"}
- `path.transform` Free Transform Path: {"name":str|"work"|"layer"="work","layer":id? (with layer target),"matrix":[a,b,c,d,e,f] | "translateX":px?,"translateY":px?,"scaleX":factor?,"scaleY":factor?,"angle":degrees?} (numeric transforms pivot on path bounds center)

## pattern

- `pattern.delete` Delete Pattern: {"pattern":id|name} (library only; documents keep their copy)
- `pattern.export` [file] Export Patterns…: {"path":".pat file","patterns":[id|name]? (default: the whole library)}
- `pattern.import` [file] Import Patterns…: {"path":".pat file"}
- `pattern.list` Patterns: {} → [{id,name,width,height,mode,depth,inDocument,inLibrary}]
- `pattern.presets.apply` New Pattern Fill Layer from Preset: {"pattern":id|name?=selected,"scale":1..1000=100,"angle":deg=0} → {layer}
- `pattern.presets.edit` Edit Pattern Presets: {"action":"rename|delete|move|newGroup|renameGroup|deleteGroup","preset":name|[names] (rename/delete/move),"group":name? (narrows the lookup; the group for renameGroup/deleteGroup),"name":str (rename: new name; newGroup/renameGroup: group name),"to":group (move),"index":n? (move)}
- `pattern.presets.list` Pattern Presets: {} → {groups:[{name,patterns:[{id,name,width,height}]}],current}
- `pattern.presets.new` New Pattern Preset: {"name":str?,"group":name?,"rect":[x0,y0,x1,y1]?} (Define Pattern from the selection/canvas into a group)
- `pattern.presets.select` Select Pattern: {"pattern":id|name,"applyToLayer":bool=true (also changes a selected Pattern Fill layer)} (Fill and new Pattern Fill layers default to it)
- `pattern.rename` Rename Pattern: {"pattern":id|name,"name":str}

## plugin

- `plugin.install` [code] Install Plug-in…: {"path":text,"data":json,"replace":bool=true} (path: a .wasm file, native only; data: the module as base64)
- `plugin.list` [host] List Plug-ins: {}
- `plugin.reload` [code] Reload Plug-ins: {"path":text} (a folder of .wasm plug-ins; default: the Plug-ins preference folder)
- `plugin.remove` [code] Remove Plug-in: {"id":text}
- `plugin.run` [code] Run Plug-in: {"id":text,"params":json} (plug-in parameters under "params" or at the top level; see plugin.list)

## prefs

- `prefs.get` [host] Get Preferences: {"path":"section.key"?=everything (e.g. "performance.historyStates", "colorSettings.workingRgb")}
- `prefs.reset` [host] Reset Preferences: {"path":"section|section.key"?=everything}
- `prefs.set` [code] Set Preferences: {"path":"section.key","value":json} or {"values":{"section.key":json,…}} (validated; all or nothing)

## select

- `select.all` All: {}
- `select.allLayers` All Layers: {}
- `select.colorRange` Color Range…: {"select":"sampledColors|reds|yellows|greens|cyans|blues|magentas|highlights|midtones|shadows|outOfGamut"="sampledColors","color":"#rrggbb"=foreground,"colors":["#rrggbb",…]?,"points":[[x,y],…]? (eyedropper samples),"subtractPoints":[[x,y],…]? (minus eyedropper),"fuzziness":0..200=40 (tones: 0..100 %=20),"localized":bool=false,"range":0..100=100 (% of the longer side),"tonalRange":level|[lo,hi] (shadows 65, highlights 190, midtones [105,150]),"invert":bool=false,"sampleAllLayers":bool=true,"mode":"replace|add|subtract|intersect"="replace"}
- `select.convertToShape` Convert Selection to Shape: {"tolerance":px=2,"fill":"#rrggbb"=foreground,"name":str?} → shape.info
- `select.deselect` Deselect: {}
- `select.deselectLayers` Deselect Layers: {}
- `select.drop` Drop Floating Selection: {} → {dropped, offset} (drops the floating piece into its layer and moves the selection with it: one history step)
- `select.editInQuickMaskMode` Edit in Quick Mask Mode: {"on":bool?=toggle}
- `select.findLayers` Find Layers: {"name":str (case-insensitive substring)}
- `select.float` Float Selection: {"dx":px=0,"dy":px=0} → {layer, offset} (cuts the selected pixels of the active layer into a floating piece the first time, then moves it by whole pixels; dropped by select.drop or any other command, put back by edit.undo)
- `select.focusArea` Focus Area…: {"range":0..1=0.5,"noise":0..1=0,"sampleAllLayers":bool=true,"mode":"replace|add|subtract|intersect"="replace"}
- `select.grow` Grow: {"tolerance":0..255=32,"sampleAllLayers":bool=false}
- `select.inverse` Inverse: {}
- `select.isolateLayers` [host] Isolate Layers: {"on":bool? (default: toggle)} — Layers panel lists only the selected layers → {isolated, layers}
- `select.lasso` Lasso: {"points":[[x,y],…],"mode":"replace|add|subtract|intersect"="replace","antiAlias":bool=true,"feather":px=0}
- `select.loadSelection` Load Selection…: {"channel":index|name|"composite"|"red|green|blue|…"|"transparency"|"mask"|"vectorMask"|"quickMask"|"selection","layer":id?,"document":index?,"invert":bool=false,"operation":"new|add|subtract|intersect"="new"}
- `select.magicWand` Magic Wand: {"x":px,"y":px,"tolerance":0..255=32,"contiguous":bool=true,"antiAlias":bool=true,"sampleAllLayers":bool=false,"mode":"replace|add|subtract|intersect"="replace"}
- `select.magneticLasso` Magnetic Lasso: {"points":[[x,y],…] (≥3 fastening points in order; with trace=false, the finished outline),"width":1..256=10 (px: follows edges this close to the points),"contrast":1..100=10 (%: weaker edges are ignored),"close":"magnetic|straight"="magnetic" (how the last point joins the first),"trace":bool=true,"mode":"replace|add|subtract|intersect"="replace","antiAlias":bool=true,"feather":0..1000=0 (px)}
- `select.modify.border` Border…: {"radius":1..200=1}
- `select.modify.contract` Contract…: {"radius":1..500=1}
- `select.modify.expand` Expand…: {"radius":1..500=1}
- `select.modify.feather` Feather…: {"radius":0.1..1000=1}
- `select.modify.smooth` Smooth…: {"radius":1..500=1}
- `select.object` Object Selection: {"rect":[x,y,w,h],"mode":"replace|add|subtract|intersect"="replace","sampleAllLayers":bool=false}
- `select.quick` Quick Selection: {"points":[[x,y],…],"size":px=30,"mode":"add|subtract|replace"="add","sampleAllLayers":bool=false,"enhanceEdge":bool=false}
- `select.rect` Rectangular Selection: {"x":i32,"y":i32,"width":u32,"height":u32,"mode":"replace|add|subtract|intersect"="replace","ellipse":bool=false,"antiAlias":bool=true,"feather":px=0}
- `select.refineEdge` Refine Edge: {"radius":px=0,"smartRadius":bool=false,"smooth":0..100=0,"feather":px=0,"contrast":0..100=0,"shiftEdge":-100..100=0,"decontaminate":bool=false,"amount":0..100=100,"output":"selection|layerMask|newLayer|newLayerWithMask"="selection","sampleAllLayers":bool=false}
- `select.reselect` Reselect: {}
- `select.saveSelection` Save Selection…: {"name":str?,"channel":"new"|index|name="new","document":index?,"operation":"new|replace|add|subtract|intersect"="new"}
- `select.similar` Similar: {"tolerance":0..255=32,"sampleAllLayers":bool=false}
- `select.sky` Sky: {"mode":"replace|add|subtract|intersect"="replace","sampleAllLayers":bool=true,"threshold":0..100=50 (higher = stricter),"softness":0..100=50 (edge softness)} → {selected, changed, coverage (0–1 of the canvas), bounds [x,y,w,h]}
- `select.subject` Subject: {"sampleAllLayers":bool=true,"mode":"replace|add|subtract|intersect"="replace"}
- `select.toWorkPath` Make Work Path: {"tolerance":0.5..10 px=2} → {subpaths,knots}
- `select.transformSelection` Transform Selection: {"rect":[x0,y0,x1,y1]? (frame; default = selection bounds),"quad"|"corners":[[x,y]×4]? (where the frame's corners go: distort/perspective),"matrix":[a,b,c,d,e,f]?,"scaleX":%=100,"scaleY":%=100,"rotate":deg=0,"skewX":deg=0,"skewY":deg=0,"dx":px=0,"dy":px=0,"reference":"center|topLeft|top|topRight|left|right|bottomLeft|bottom|bottomRight"|[x,y]="center","style"|"mesh"|"grid"|"warp":… (warp, as edit.transform.warp),"interpolation":"bilinear|bicubic|nearest"="bilinear"}

## session

- `session.inspect` Inspect Session: {}

## shape

- `shape.create` New Shape Layer: {"kind":"rect|roundedRect|ellipse|polygon|star|line|path"="rect","rect":[x,y,w,h] (rect/roundedRect/ellipse/polygon/star),"radii":[tl,tr,br,bl]|r (roundedRect=10),"sides":3..100=5,"starRatio":0..1 (star=0.5),"from":[x,y],"to":[x,y],"weight":px=1 (line),"path":{…} (kind path),"fill":"#rrggbb"|[r,g,b,a]|{"gradient":{"stops":[[t,"#hex"]],"angle":deg,"scale":%,"style":"linear|radial|angle|reflected|diamond","reverse":bool}}|{"pattern":name}|null (no fill)=foreground,"stroke":{"width":px,"color":"#rrggbb"|fill,"opacity":0..100,"align":"inside|center|outside","cap":"butt|round|square","join":"miter|round|bevel","miterLimit":n,"dashes":[multiples of width],"dashOffset":n}|null=none,"name":str?,"addTo":layerId? + "op":"combine|subtract|intersect|exclude" (append to an existing shape layer)} → shape.info. path: {"subpaths":[{"closed":bool=true,"op":"combine|subtract|intersect|exclude","knots":[[x,y] | {"anchor":[x,y],"in":[x,y],"out":[x,y],"smooth":bool}]}],"fillRule":"nonzero|evenodd","inverted":bool}
- `shape.edit` Edit Shape: {"layer":id?,"path":{…}? (replaces; drops live shape),"kind":str?,"rect":[x,y,w,h]?,"radii":[tl,tr,br,bl]|r?,"sides":n?,"starRatio":r?,"from":[x,y]?,"to":[x,y]?,"weight":px? (live-shape params regenerate the path),"move":[dx,dy]?,"transform":[a,b,c,d,e,f]?,"op":"combine|…"? + "subpath":index? (default all but the first),"fillRule":"nonzero|evenodd"?,"fill":"#rrggbb"|[r,g,b,a]|{"gradient":{"stops":[[t,"#hex"]],"angle":deg,"scale":%,"style":"linear|radial|angle|reflected|diamond","reverse":bool}}|{"pattern":name}|null (no fill)?,"stroke":{"width":px,"color":"#rrggbb"|fill,"opacity":0..100,"align":"inside|center|outside","cap":"butt|round|square","join":"miter|round|bevel","miterLimit":n,"dashes":[multiples of width],"dashOffset":n}|null? (merged into the current stroke),"name":str?} → shape.info
- `shape.info` Shape Layer Info: {"layer":id?} → {layer,name,kind,path,fill,stroke,live,bounds:[x,y,w,h]}
- `shape.presets.edit` Edit Shape Presets: {"action":"rename|delete|move|newGroup|renameGroup|deleteGroup","preset":name|[names] (rename/delete/move),"group":name? (narrows the lookup; the group for renameGroup/deleteGroup),"name":str (rename: new name; newGroup/renameGroup: group name),"to":group (move),"index":n? (move)}
- `shape.presets.list` Shape Presets: {} → {groups:[{name,shapes:[{name,subpaths}]}]}
- `shape.presets.new` New Shape Preset: {"name":str="Shape","group":name?,"path":{…path}|"M x y L …" (preset path language)|"work"|saved path name? (default: work path, active shape layer or vector mask)}
- `shape.presets.place` Place Custom Shape: {"preset":name,"group":name?,"rect":[x,y,w,h]? (default: centred, half the canvas),"keepAspect":bool=true,"fill":…?=foreground,"stroke":…?,"name":str?,"addTo":layerId?,"op":"combine|subtract|intersect|exclude"?} → shape.info (Custom Shape tool / Shapes panel; fill and stroke as in shape.create)
- `shape.presets.reset` Restore Default Shapes: {"append":bool=false}
- `shape.rasterize` Rasterize Shape: {"layer":id?}

## slice

- `slice.delete` Delete Slice: {"slice":id | "number":n | "slices":[id…]} → {deleted}
- `slice.divide` Divide Slice…: {"slice":id | "number":n,"horizontal":n=1 (slices down),"vertical":n=1 (slices across)} → {slices}
- `slice.fromGuides` Slices From Guides: {} (replaces every slice by the grid of the canvas guides) → {slices}
- `slice.list` List Slices: {} → {slices:[{number,id,origin:auto|layer|user,name,rect:[x,y,w,h],kind,layer,url,alt,…}], locked}
- `slice.new` Slice Tool: {"rect":[x,y,w,h] | "x","y","width","height", plus Slice Options ("name","kind":"image|noImage|table","url","target","message","alt","cellText","cellTextIsHtml","background":"none|#rrggbb")?} → {slice, number}
- `slice.promote` Promote: {"slice":id | "number":n} (auto or layer-based → user slice) → {slice}
- `slice.set` Slice Options…: {"slice":id | "number":n (an auto slice is promoted), "name"?,"kind":"image|noImage|table"?,"url"?,"target"?,"message"?,"alt"?,"cellText"?,"cellTextIsHtml"?,"horizontalAlign":0..4?,"verticalAlign":0..4?,"background":"none|#rrggbb"?,"outsets":[t,l,b,r]? (layer slices),"rect":[x,y,w,h]? (move/resize; a layer slice becomes a user slice)} → the slice

## style

- `style.presets.apply` Apply Style: {"preset":name,"group":name?,"layers":[ids]?|"layer":id? (default: the selected layers),"add":bool=false (⇧-click: add to the existing effects)} → {layers,style}
- `style.presets.edit` Edit Style Presets: {"action":"rename|delete|move|newGroup|renameGroup|deleteGroup","preset":name|[names] (rename/delete/move),"group":name? (narrows the lookup; the group for renameGroup/deleteGroup),"name":str (rename: new name; newGroup/renameGroup: group name),"to":group (move),"index":n? (move)}
- `style.presets.list` Style Presets: {} → {groups:[{name,presets:[{name,effects,blend,fillOpacity}]}]}
- `style.presets.new` New Style…: {"name":str="Style","group":name?,"layer":id?,"includeEffects":bool=true,"includeBlending":bool=true,"effects":[[kind, params]]? (explicit list, e.g. the Layer Style dialog's pending state; overrides includeEffects),"blend":str?,"fillOpacity":0..100? (override the layer's blending options)}
- `style.presets.reset` Restore Default Styles: {"append":bool=false}

## timeline

- `timeline.create` Create Video Timeline: {duration?:30, fps?:30} → {timeline}
- `timeline.delete` Delete Timeline: {} → {timeline:null}
- `timeline.info` Timeline Info: {} → {timeline}
- `timeline.nextFrame` Next Frame: {} → {timeline}
- `timeline.previousFrame` Previous Frame: {} → {timeline}
- `timeline.setFrame` Go to Frame: {frame} → {timeline}
- `timeline.setProps` Timeline Settings: {fps?, duration?, workStart?, workEnd?} → {timeline}

## tool

- `tool.presets.edit` Edit Tool Presets: {"action":"rename|delete","preset":name|[names],"name":str (rename)}
- `tool.presets.list` Tool Presets: {"tool":name? (Current Tool Only)} → {presets:[{name,tool}]}
- `tool.presets.new` New Tool Preset…: {"name":str?=tool,"tool":name,"options":{…}? (opaque; "brush" defaults to the current brush for painting tools),"includeColor":bool=false}
- `tool.presets.reset` Reset Tool Presets: {}
- `tool.presets.select` Select Tool Preset: {"preset":name} → {name,tool,options} (applies "brush" and "foreground"; the shell switches tool and options)

## tools

- `tools.decreaseBrushHardness` Decrease Brush Hardness: {}
- `tools.decreaseBrushSize` Decrease Brush Size: {}
- `tools.defaultColors` Default Foreground and Background Colors: {}
- `tools.increaseBrushHardness` Increase Brush Hardness: {}
- `tools.increaseBrushSize` Increase Brush Size: {}
- `tools.setBrush` Set Brush: {"preset":name?,"reset":bool?,…BrushSettings fields (camelCase, deep-merged)}
- `tools.setColors` Set Colors: {"foreground":"#rrggbb"?,"background":"#rrggbb"?}
- `tools.swapColors` Switch Foreground and Background Colors: {}

## type

- `type.antiAlias.crisp` Crisp: {"layer":id?}
- `type.antiAlias.none` None: {"layer":id?}
- `type.antiAlias.sharp` Sharp: {"layer":id?}
- `type.antiAlias.smooth` Smooth: {"layer":id?}
- `type.antiAlias.strong` Strong: {"layer":id?}
- `type.antiAlias.windows` Windows: {"layer":id?}
- `type.antiAlias.windowsLcd` Windows LCD: {"layer":id?}
- `type.caret` Type Caret: {"layer":id?, "index":char (0..=length; empty text is 0)} → {"index","line","segment":[[x,y],[x,y]]} caret segment in document pixels, for rotated and vertical type too
- `type.characterStyle.apply` Apply Character Style: {"id":u32|0|null (0/null: None / Basic Paragraph),"clearOverrides":bool=false (Alt-click),"layer":id? | "layers":[id]? (default: the selected type layers),"range":[startChar,endChar]? (one layer; default all text)}
- `type.characterStyle.clearOverride` Clear Override: {"layer":id? | "layers":[id]? (default: the selected type layers),"range":[startChar,endChar]? (one layer; default all text)}
- `type.characterStyle.delete` Delete Style: {"id":u32} (text keeps its formatting as overrides)
- `type.characterStyle.duplicate` Duplicate Style: {"id":u32}
- `type.characterStyle.list` List Character Styles: {"layer":id? | "layers":[id]? (default: the selected type layers),"range":[startChar,endChar]? (one layer; default all text)} → {"styles":[{"id","name",attributes,"resolved"}],"current":{"character":id|null (mixed),"characterOverride":bool,"paragraph":id|null,"paragraphOverride":bool}}
- `type.characterStyle.new` New Character Style: {"name":str?,"attrs":{model fields as in type.info ("size_pt":36,"font_family":"Inter","align":"Center",…) or Character/Paragraph panel keys as in type.setStyle ("size":36,"font":"Inter","color":"#rrggbb","align":"center",…)}?,"fromSelection":bool=true (start from the targeted text's formatting),"apply":bool=false,"layer":id? | "layers":[id]? (default: the selected type layers),"range":[startChar,endChar]? (one layer; default all text)} → {"id","name"}
- `type.characterStyle.redefine` Redefine Character Style: {"id":u32? (default: the targeted text's style),"layer":id? | "layers":[id]? (default: the selected type layers),"range":[startChar,endChar]? (one layer; default all text)} (the style takes the formatting of the start of the targeted text)
- `type.characterStyle.rename` Rename Style: {"id":u32,"name":str}
- `type.characterStyle.set` Character Style Options: {"id":u32 (paragraph: 0 = Basic Paragraph),"name":str?,"attrs":{model fields as in type.info ("size_pt":36,"font_family":"Inter","align":"Center",…) or Character/Paragraph panel keys as in type.setStyle ("size":36,"font":"Inter","color":"#rrggbb","align":"center",…)},"replace":bool=false (drop attributes not given),"clear":[field]?} (text using the style updates, overrides kept)
- `type.convertToParagraphText` Convert to Paragraph Text: {"layer":id?}
- `type.convertToPointText` Convert to Point Text: {"layer":id?}
- `type.convertToShape` Convert to Shape: {"layer":id?}
- `type.create` New Type Layer: {"x":px,"y":px (baseline anchor of point text),"text":str,"box":[x,y,w,h]? (paragraph text),"name":str?,"align":"left|center|right|justify…"?,"orientation":"horizontal|vertical"="horizontal", …character keys: "font","size":pt=12,"color",…}
- `type.createWorkPath` Create Work Path: {"layer":id?}
- `type.edit` Edit Type: {"layer":id?,"text":str? (replace all, styles kept),"replace":{"start":char,"end":char,"text":str}?,"runs":[{"start":char,"end":char,…character keys}]?,"box":[x,y,w,h]? (to paragraph text),"point":[x,y]? (to point text),"move":[dx,dy]?,"transform":[a,b,c,d,e,f]?,"antialias":"none|sharp|crisp|strong|smooth"?,"name":str?,"kerning":1/1000em|"metrics"|"optical"|"off"? (with "range":[startChar,endChar]?, default all),"kernPair":{"at":caretChar,"by":1/1000em}? (Photoshop Alt+←/→: the pair before the caret becomes manual, its current kerning + by)}
- `type.fonts` List Fonts: {"family":str? (faces of one family)}
- `type.hitTest` Hit-Test Type: {"layer":id?, "x":px, "y":px} → {"layer","index":char,"line","inside":bool}. No layer: the topmost visible type layer whose laid-out text or rendered pixels contain the point (same order as the Type tool); a miss is an error. With a layer, index is the nearest caret even when inside is false
- `type.info` Type Layer Info: {"layer":id?} → text, runs/paragraphs (char offsets + styles), shape, transform, laid-out lines (text space px), bounds
- `type.insertText` Insert Glyph: {"layer":id?,"text":str,"at":char? (default: end of text),"range":[startChar,endChar]? (replaced),"label":str?} → {"layer","caret"}
- `type.loadDefaultTypeStyles` Load Default Type Styles: {"layer":id?}
- `type.navigate` Navigate Type: {"layer":id?, "index":char, "move":"wordPrev|wordNext|linePrev|lineNext|lineStart|lineEnd|start|end", "x":px?} → {"index"}. Words are alphanumeric runs. x is the document x to keep across linePrev/lineNext (a caret segment's x); omitted uses the caret's own column. Empty text returns index 0
- `type.openType.contextualAlternates` Contextual Alternates: {"layer":id?,"on":bool? (default: toggle),"range":[startChar,endChar]? (default all)}
- `type.openType.discretionaryLigatures` Discretionary Ligatures: {"layer":id?,"on":bool? (default: toggle),"range":[startChar,endChar]? (default all)}
- `type.openType.fractions` Fractions: {"layer":id?,"on":bool? (default: toggle),"range":[startChar,endChar]? (default all)}
- `type.openType.oldstyle` Oldstyle: {"layer":id?,"on":bool? (default: toggle),"range":[startChar,endChar]? (default all)}
- `type.openType.ordinals` Ordinals: {"layer":id?,"on":bool? (default: toggle),"range":[startChar,endChar]? (default all)}
- `type.openType.ornaments` Ornaments: {"layer":id?,"on":bool? (default: toggle),"range":[startChar,endChar]? (default all)}
- `type.openType.standardLigatures` Standard Ligatures: {"layer":id?,"on":bool? (default: toggle),"range":[startChar,endChar]? (default all)}
- `type.openType.stylisticAlternates` Stylistic Alternates: {"layer":id?,"on":bool? (default: toggle),"range":[startChar,endChar]? (default all)}
- `type.openType.swash` Swash: {"layer":id?,"on":bool? (default: toggle),"range":[startChar,endChar]? (default all)}
- `type.openType.titlingAlternates` Titling Alternates: {"layer":id?,"on":bool? (default: toggle),"range":[startChar,endChar]? (default all)}
- `type.orientation.horizontal` Horizontal: {"layer":id?}
- `type.orientation.vertical` Vertical: {"layer":id?}
- `type.paragraphStyle.apply` Apply Paragraph Style: {"id":u32|0|null (0/null: None / Basic Paragraph),"clearOverrides":bool=false (Alt-click),"layer":id? | "layers":[id]? (default: the selected type layers),"range":[startChar,endChar]? (one layer; default all text)}
- `type.paragraphStyle.clearOverride` Clear Override: {"layer":id? | "layers":[id]? (default: the selected type layers),"range":[startChar,endChar]? (one layer; default all text)}
- `type.paragraphStyle.delete` Delete Style: {"id":u32} (text keeps its formatting as overrides)
- `type.paragraphStyle.duplicate` Duplicate Style: {"id":u32}
- `type.paragraphStyle.list` List Paragraph Styles: {"layer":id? | "layers":[id]? (default: the selected type layers),"range":[startChar,endChar]? (one layer; default all text)} → {"styles":[{"id","name",attributes,"resolved"}],"current":{"character":id|null (mixed),"characterOverride":bool,"paragraph":id|null,"paragraphOverride":bool}}
- `type.paragraphStyle.new` New Paragraph Style: {"name":str?,"attrs":{model fields as in type.info ("size_pt":36,"font_family":"Inter","align":"Center",…) or Character/Paragraph panel keys as in type.setStyle ("size":36,"font":"Inter","color":"#rrggbb","align":"center",…)}?,"fromSelection":bool=true (start from the targeted text's formatting),"apply":bool=false,"layer":id? | "layers":[id]? (default: the selected type layers),"range":[startChar,endChar]? (one layer; default all text)} → {"id","name"}
- `type.paragraphStyle.redefine` Redefine Paragraph Style: {"id":u32? (default: the targeted text's style),"layer":id? | "layers":[id]? (default: the selected type layers),"range":[startChar,endChar]? (one layer; default all text)} (the style takes the formatting of the start of the targeted text)
- `type.paragraphStyle.rename` Rename Style: {"id":u32,"name":str}
- `type.paragraphStyle.set` Paragraph Style Options: {"id":u32 (paragraph: 0 = Basic Paragraph),"name":str?,"attrs":{model fields as in type.info ("size_pt":36,"font_family":"Inter","align":"Center",…) or Character/Paragraph panel keys as in type.setStyle ("size":36,"font":"Inter","color":"#rrggbb","align":"center",…)},"replace":bool=false (drop attributes not given),"clear":[field]?} (text using the style updates, overrides kept)
- `type.pasteLoremIpsum` Paste Lorem Ipsum: {"layer":id?,"at":char? (default end),"new":bool=false (new paragraph text layer)}
- `type.rasterize` Rasterize Type: {"layer":id?}
- `type.rasterizeTypeLayer` Rasterize Type Layer: {"layer":id?}
- `type.replaceAllMissingFonts` Replace All Missing Fonts: {} (with the default family)
- `type.resolveMissingFonts` Resolve Missing Fonts…: {"map":{"Missing Family":"Installed Family"}?} (no map: list the missing families)
- `type.saveDefaultTypeStyles` Save Default Type Styles: {} (from the active type layer; new type layers start from them)
- `type.setStyle` Set Type Style: {"layer":id?,"range":[startChar,endChar]? (default all), "font":str,"fontStyle":str,"weight":100..900,"italic":bool,"size":0.1..=1296 pt,"color":"#rrggbb"|[r,g,b,a],"tracking":-1000..=10000 (1/1000 em),"leading":pt|"auto","baselineShift":pt,"horizontalScale":%,"verticalScale":%,"underline":bool,"strikethrough":bool,"fauxBold":bool,"fauxItalic":bool,"kerning":1/1000em (manual, after each character)|"metrics"|"optical"|"off","caps":"normal|small|all","ligatures":bool,"discretionaryLigatures":bool,"features":{"ss01":1},"variations":{"wght":650},"language":str, paragraph keys: "align":"left|center|right|justify|justifyCenter|justifyRight|justifyAll","firstLineIndent":pt,"startIndent":pt,"endIndent":pt,"spaceBefore":pt,"spaceAfter":pt,"autoLeading":%,"direction":"auto|ltr|rtl","hyphenate":bool}
- `type.updateAllTextLayers` Update All Text Layers: {}
- `type.warpText` Warp Text…: {"layer":id?,"style":"none|arc|arcLower|arcUpper|arch|bulge|shellLower|shellUpper|flag|wave|fish|rise|fisheye|inflate|squeeze|twist"="arc","bend":-100..100=50,"horizontalDistortion":-100..100=0,"verticalDistortion":-100..100=0,"orientation":"horizontal|vertical"="horizontal"}

## variables

- `variables.list` List Variables: {} → {defs,dataSets,active}

## view

- `view.clearCanvasGuides` Clear Canvas Guides: {}
- `view.clearGuides` Clear Guides: {}
- `view.clearSelectedArtboardGuides` Clear Selected Artboard Guides: {} (guides inside the active artboard)
- `view.clearSlices` Clear Slices: {} (deletes every user and layer-based slice) → {cleared}
- `view.deleteGuide` Delete Guide: {"orientation":"horizontal|vertical","index":n}
- `view.gamutWarning` [host] Gamut Warning: {"on":bool=toggle,"threshold":deltaE=4,"profile":"<proof profile override>"}
- `view.layerMask` [host] View Layer Mask: {"layer":id?,"mode":"off"|"gray"|"overlay"|"toggleGray"|"toggleOverlay"="toggleGray"} (gray: the mask alone, ⌥-click its thumbnail; overlay: red rubylith over the composite, ⇧⌥-click; view state, not an undo step; painting while shown paints the mask)
- `view.lockSlices` Lock Slices: {"on":bool? (default: toggle)} → {locked}
- `view.moveGuide` Move Guide: {"orientation":"horizontal|vertical","index":n,"position":px}
- `view.newGuide` New Guide…: {"orientation":"horizontal|vertical","position":px}
- `view.newGuideLayout` New Guide Layout…: {"columns":n=0,"width":px?,"gutter":px=0,"rows":n=0,"height":px?,"rowGutter":px=gutter,"margin":px|[top,left,bottom,right]=0,"centerColumns":bool=false,"clearExisting":bool=false}
- `view.newGuidesFromShape` New Guides From Shape: {"layer":id?}
- `view.proofColors` [host] Proof Colors: {"on":bool=toggle}
- `view.proofSetup` [host] Proof Setup…: {"profile":"working-cmyk|srgb|display-p3|adobe-rgb-compat|prophoto-compat|linear-srgb|rec2020|gray-gamma-2.2|sgray|lab-d50|coated-cmyk" (or a path to an .icc file)="working-cmyk","intent":"perceptual|relative|saturation|absolute"="relative","bpc":bool=true,"simulatePaper":bool=false}
- `view.proofSetup.colorBlindnessDeuteranopia` [host] Color Blindness — Deuteranopia-type: {} (sets the proof and turns Proof Colors on)
- `view.proofSetup.colorBlindnessProtanopia` [host] Color Blindness — Protanopia-type: {} (sets the proof and turns Proof Colors on)
- `view.proofSetup.legacyMacintoshRgb` [host] Legacy Macintosh RGB: {} (sets the proof and turns Proof Colors on)
- `view.proofSetup.workingBlackPlate` [host] Working Black Plate: {} (sets the proof and turns Proof Colors on)
- `view.proofSetup.workingCmyPlate` [host] Working CMY Plate: {} (sets the proof and turns Proof Colors on)
- `view.proofSetup.workingCyanPlate` [host] Working Cyan Plate: {} (sets the proof and turns Proof Colors on)
- `view.proofSetup.workingMagentaPlate` [host] Working Magenta Plate: {} (sets the proof and turns Proof Colors on)
- `view.proofSetup.workingYellowPlate` [host] Working Yellow Plate: {} (sets the proof and turns Proof Colors on)
- `view.thirtyTwoBitPreviewOptions` [host] 32-bit Preview Options…: {"method":"exposureGamma|highlightCompression"="exposureGamma","exposure":-20..20=0,"gamma":0.1..9.99=1}
