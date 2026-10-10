# effect engine commands

The effectcraft engine's command catalog at revision d60cb71e297a: 665 commands, one per line as `id` label: params.
Generated from the engine by `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-effect-service --test skill`; do not edit.

A tag after the id marks a command that reaches past the open document (safety.json has every id's class): [file] reads or writes a path, [code] installs or runs code or other commands, [network] uses the network, [device] uses hardware, [host] changes windows, views, preferences, the clipboard or other app state. An untagged command works on the open document only.

## anim

- `anim.addKeyframe` Add Keyframe: {} — keys the selected properties at the CTI
- `anim.applyPreset` [file] Apply Animation Preset...: {path | preset, layers?}
- `anim.applyRecentPreset` [file] Recent Animation Presets: {index? | path?, layers?}
- `anim.browsePresets` [host] Browse Presets...: {}
- `anim.clearRecentPresets` [host] Clear Recent Presets: {}
- `anim.reveal` Reveal Properties: {kind: keyframes|animation|modified}
- `anim.savePreset` [file] Save Animation Preset...: {path (.ecpreset), name?} — saves the selected properties/effects

## app

- `app.about` [host] About EffectCraft...: {}
- `app.capabilities` App Capabilities: {} → version, command/effect counts, export formats, parity summary
- `app.commandPalette` [host] Quick Apply...: {query?}
- `app.find` [host] Find: {query?}
- `app.gpuInfo` [host] GPU Information...: {}
- `app.hide` [host] Hide EffectCraft: {}
- `app.hideOthers` [host] Hide Others: {}
- `app.keyboardShortcuts` [host] Keyboard Shortcuts: {}
- `app.quit` [host] Quit EffectCraft: {}
- `app.settings` [host] Settings...: {page?: general|startup|project|composition|previews|appearance|grids|labels|type|import|export|audio|disk|memory|video|3d|scripting}
- `app.showAll` [host] Show All: {}
- `app.templates` [host] Templates: {kind: renderSettings|outputModule}

## cache

- `cache.diskStats` [host] Disk Cache Statistics: {}

## camera

- `camera.analyze` Analyze: {layer?, effect?: uid|name|index, wait?: block until done}
- `camera.cancel` Cancel: {}
- `camera.create` Create Camera: {layer?, effect?}
- `camera.createFromSolve` Create from Camera Solve: {kind: text|solid|null|shadowCatcher|camera, points?: [id] (default: selected), target?: {center, normal, size?} (comp world), multiple?, layer?, effect?}
- `camera.deletePoints` Delete Selected Points: {points?: [id] (default: selected), layer?, effect?, wait?}
- `camera.dolly` Dolly Camera: {amount (px, + = forward), layer?, merge?}
- `camera.fromModel` Create Cameras from 3D Model: {comp?, layers?: 3D model layers (default selected)}
- `camera.fromView` Create Camera from 3D View: {comp?}
- `camera.linkFocusToLayer` [code] Link Focus Distance to Layer: {comp?, camera?, layer?}
- `camera.linkFocusToPoi` [code] Link Focus Distance to Point of Interest: {comp?, camera?}
- `camera.orbit` Orbit Camera: {yaw|dx deg, pitch|dy deg, layer?, merge?} — current 3D view (camera layer in Active Camera)
- `camera.orbitNull` Create Orbit Null: {comp?, layer?: camera (default selected/active)}
- `camera.pan` Pan Camera: {dx, dy (comp px), layer?, merge?}
- `camera.points` 3D Camera Tracker Points: {layer?, effect?, time?: comp seconds} → visible solved points
- `camera.selectPoints` Select Track Points: {points: [id], add?, toggle?}
- `camera.setFocusToLayer` Set Focus Distance to Layer: {comp?, camera?, layer?}
- `camera.setGroundPlane` Set Ground Plane and Origin: {points?: [id] (default: selected), layer?, effect?}
- `camera.solveStatus` 3D Camera Tracker Status: {layer?, effect?}
- `camera.stereoRig` [code] Create Stereo 3D Rig: {comp?, configuration?: stereoPair|centerRight|centerLeft, sceneDepth? (% of comp width, default 3), convergence?: bool, convergenceOf?: poi|zoom, zOffset?, view3d?: 3D Glasses view index (default 5 Balanced Colored Red Blue)}

## command

- `command.describe` Describe Command: {command} → id, label, menu, shortcut, params doc, JSON `schema`, enabled
- `command.list` List Commands: {filter?, enabledOnly?, schemas? (add each command's params JSON Schema)}

## comp

- `comp.addMarker` Add Marker: {time?, comment?}
- `comp.close` Close Composition: {comp?}
- `comp.cropToLayerBounds` Crop Comp to Selected Layer(s) Bounds: {layers?}
- `comp.cropToRegionOfInterest` Crop Comp to Region of Interest: {comp?}
- `comp.flowchart` [host] Composition Flowchart: {}
- `comp.info` Composition Info: {comp?}
- `comp.miniFlowchart` [host] Composition Mini-Flowchart: {}
- `comp.new` New Composition...: {name?, width?, height?, frameRate?, duration? (s), startTime? (s) | startTimecode?, background? [r,g,b]|#hex, pixelAspect?, shutterAngle?, shutterPhase?, motionBlurSamples?, adaptiveSampleLimit? (16–256), preserveFrameRate?: bool, preserveResolution?: bool, renderer?: classic3D|advanced3D, anchor?, open?}
- `comp.open` Open Composition: {comp: id|name}
- `comp.openInEssentialGraphics` Open in Essential Graphics: {comp?} → makes it the Essential Graphics panel's Primary composition and shows the panel
- `comp.renderer` Renderer: {renderer?: advanced3d|classic3d (omit to read), value?}
- `comp.responsiveTime` Responsive Design — Time: {op: intro|outro|workArea}
- `comp.revealInProject` Reveal Composition in Project: {comp?}
- `comp.saveFrameAs` [file] File...: {path (.png; with queue: an output path or template), time?, scale?, queue?: bool (add a Render Queue item from the Frame Default templates instead of writing now)}
- `comp.saveFrameAsExr` [file] ProEXR...: {path (.exr), comp?, time?, scale?} → a multi-layer OpenEXR: composite R,G,B,A + `<layer>.R/G/B/A` per layer (linear, premultiplied)
- `comp.saveFrameAsPsd` [file] Photoshop Layers...: {path (.psd), comp?, time?, scale?} → a layered PSD: one layer per visible comp layer (blend mode, opacity) + the merged frame
- `comp.setPosterTime` Set Poster Time: {}
- `comp.setSwitch` Composition Switch: {switch: hideShy|motionBlur|frameBlending|draft3d, value?}
- `comp.settings` Composition Settings...: {comp?, name?, width?, height?, anchor? 0-8 (resize anchor, 4 = center), frameRate?, duration?, startTime? (s) | startTimecode?, background?, shutterAngle?, shutterPhase?, motionBlurSamples?, adaptiveSampleLimit? (16–256), preserveFrameRate?: bool (nested or in the render queue it shows only its own frames), preserveResolution?: bool (nested, it renders at full size), pixelAspect?, renderer?: classic3D|advanced3D}
- `comp.trimToWorkArea` Trim Comp to Work Area: {comp?}
- `comp.vr.createEnvironment` Create VR Environment...: {comp?, size?: face pixels (1024), position?: [x,y,z]}
- `comp.vr.environments` VR Environments: {} → [{output, name, cubeMap, faces: [{face, comp, camera}], view: [x, y, z]}]
- `comp.vr.extractCubemap` Extract Cubemap...: {comp?, faceSize?}
- `comp.vr.setView` VR View Orientation: {comp? (an environment's output / cube map / face comp), orientation?: [x, y, z] degrees, pan?, tilt?, roll?} → turns the six face cameras together
- `comp.workArea` Set Work Area: {start?, end?, set?: begin|end (at CTI)}

## contentFill

- `contentFill.generate` [file] Generate Fill Layer: {layer?, method?: object|surface|edgeBlend, range?: workArea|entire, alphaExpansion? (px), lightingCorrection?: none|subtle|moderate|strong, referenceLayer?, referenceTime? (s), outputDir?, wait?: bool}
- `contentFill.set` Content-Aware Fill Settings: {method?: object|surface|edgeBlend, range?: workArea|entire, alphaExpansion? (px), lightingCorrection?: none|subtle|moderate|strong, referenceLayer?: layer|null, referenceTime? (s)}

## edit

- `edit.clear` Clear: {layers?}
- `edit.copy` Copy: {layers?} (keyframes, effects or shape items when they are selected)
- `edit.copyExpressionOnly` [code] Copy Expression Only: {}
- `edit.copyWithPropertyLinks` [code] Copy with Property Links: {layers?} (selected properties, or layers, as expressions linking to the originals)
- `edit.copyWithRelativePropertyLinks` [code] Copy with Relative Property Links: {layers?} (like Copy with Property Links, using thisComp)
- `edit.cut` Cut: {layers?} (keyframes, effects or shape items when they are selected)
- `edit.deselectAll` Deselect All: {}
- `edit.duplicate` Duplicate: {layers?} (effects or shape items when they are selected)
- `edit.editOriginal` [host] Edit Original...: {}
- `edit.extractWorkArea` Extract Work Area: {layers?}
- `edit.history` History: {steps?: n (undo n steps), redo?: n}
- `edit.history.goto` Go to History State: {index? (from edit.history.list) | id? | steps? (negative = back along the line)}
- `edit.history.list` History: {} → {states: [{index, id, label, parent, depth, current, line, future}], current, branches}
- `edit.label` Label: {label: Red|Yellow|Aqua|…, layers?, keys?, target?: layers}
- `edit.liftWorkArea` Lift Work Area: {layers?}
- `edit.paste` Paste: {} (layers, keyframes at the CTI, effects, shape items into the selected shape layers, or property links / expressions)
- `edit.pasteReversedKeyframes` Paste Reversed Keyframes: {layers?, prop?|path?, time?}
- `edit.pasteTextFormattingOnly` Paste Text Formatting Only: {layer?} (the copied text's formatting on the selected text or text layers)
- `edit.pasteTextMatchFormatting` Paste Text and Match Formatting: {text?} (the clipboard's text in the style at the caret)
- `edit.purge` [host] Purge: {what?: all|memoryAndDisk|memory|disk|3d|image|snapshot}
- `edit.purgeUndo` Undo: {}
- `edit.redo` Redo: {}
- `edit.selectAll` Select All: {}
- `edit.selectLabelGroup` Select Label Group: {}
- `edit.splitLayer` Split Layer: {layers?}
- `edit.undo` Undo: {}

## editor

- `editor.state` Editor State: {}

## effect

- `effect.apply` Apply Effect: {effect: id|name (e.g. Gaussian Blur), layers?}
- `effect.applyLast` Last Effect: {layers?}
- `effect.copy` Copy Effects: {layer?, effect? | effects?: [uid|name|index]} (default: the selected effects)
- `effect.duplicate` Duplicate Effect: {layer?, effect}
- `effect.editDropdown` Edit Dropdown Menu: {layer, path?|prop?, items: [string]}
- `effect.list` List Effects: {filter?}
- `effect.manage` [host] Manage Effects...: {}
- `effect.paste` Paste Effects: {layers?} — adds the copied effects to the layers
- `effect.pickColor` Pick Effect Colour: {layer?, effect?: index|uid|name, param: id (e.g. screenColour) | prop: uid, x, y (layer px), average?: bool (5×5), time?} — the colour of the effect's input there
- `effect.plugins.list` List Effect Plug-ins: {} → {api, wasm, plugins: [{id, name, category, version, author, source, params}]}
- `effect.plugins.load` [code] Load Effect Plug-in...: {path (.wasm / .wat plug-in, API v1) | folder (loads every .wasm in it)}
- `effect.remove` Remove Effect: {layer?, effect: index|uid|name}
- `effect.removeAll` Remove All: {layers?}
- `effect.reorder` Reorder Effect: {layer?, effect, index}
- `effect.reset` Reset Effect: {layer?, effect}
- `effect.toggle` Toggle Effect: {layer?, effect, value?}

## engine

- `engine.batch` [code] Run Commands (Batch): {steps: [{command, params?}], label? (undo step name, default Batch), atomic?: bool (default true: a failing step rolls the whole batch back)} → {steps, results: [each step's result]}; a string param "$N" or "$N.key.0" is step N's result (1-based), e.g. {"layer": "$1.layer"}

## essential

- `essential.addComment` Add Comment: {comp?, text?, group?}
- `essential.addGroup` Add Group: {comp?, name?, index?}
- `essential.addMedia` Add Media Replacement: {layer, name?, group?}
- `essential.addMirror` Add Mirror: {comp?, control, name?, group?, index?}
- `essential.addProperty` Add Property to Essential Graphics: {comp?, layer?, path?|prop?: uid (default: the selected properties), name?, group?: control id, index?, as?: font (Source Text font family/style/size) | scale (uniform Scale), mirror?: bool (a property already present is added as a mirror; default true)}
- `essential.canAdd` Can Add Property to Essential Graphics: {layer, path|prop, as?} → {ok, type?, reason?}
- `essential.exportTemplate` [file] Essential Graphics Template...: {comp?, path (.ectemplate), name?}
- `essential.importTemplate` [file] Essential Graphics Template...: {path (.ectemplate), comp?, addToComp?: bool}
- `essential.instance` Essential Properties of a precomp layer: {layer}
- `essential.linkProperty` Link Property to Control: {comp?, control, layer?, path?|prop? (default: the selected property)}
- `essential.list` Essential Graphics controls: {comp?, supported?: bool}
- `essential.move` Move Control: {comp?, control, group?: id (null = top level), index?}
- `essential.pushToComp` Push Override Values to Source: {layer, control? (default: all overridden)}
- `essential.remove` Remove Control: {comp?, control}
- `essential.rename` Rename Control: {comp?, control, name}
- `essential.revert` Revert: {layer, control? (default: all overridden)}
- `essential.set` Set Essential Property: {layer, control: id|name, value | item (media)}
- `essential.setName` Essential Graphics Name: {comp?, name}
- `essential.setPrimary` Primary Composition: {comp}
- `essential.soloSupported` Solo Supported Properties: {on?}
- `essential.templateInfo` [file] Read a template's manifest: {path}
- `essential.unlinkProperty` Unlink Property: {comp?, control, layer?, path?|prop? (default: the selected property)}

## expr

- `expr.errors` Expression errors (the error bar): {comp?, time? (s)}
- `expr.languageMenu` Expression Language menu: {}

## face

- `face.model.download` [network] Download Face Tracking Model: {id, wait?} — the official model, verified (desktop; uses the system curl)
- `face.model.install` [file] Install Face Tracking Model: {path: model file, id?, wait?} — verified against the registry's SHA-256
- `face.model.remove` [host] Remove Face Tracking Model: {id}
- `face.model.select` [host] Use Face Tracking Model: {id: classical|mediapipe-face, wait?} (face tracking uses it; wait: loaded before returning)
- `face.models` Face Tracking Models: {}

## file

- `file.applyInterpretation` Apply Interpretation: {items?}
- `file.autoSave` [file] Auto-Save Now: {}
- `file.clearRecent` [host] Clear Recent Projects: {}
- `file.clearRecentFootage` [host] Clear Recent Footage: {}
- `file.close` Close: {comp?}
- `file.closeProject` Close Project: {}
- `file.collectFiles` [file] Collect Files...: {folder}
- `file.consolidateFootage` Consolidate All Footage: {}
- `file.createProxy` [file] Create Proxy: {kind: still|movie, comp?|item?, path?|output? (template), resolution? (default 0.5)}
- `file.cycleBitDepth` Cycle Project Bit Depth: {}
- `file.executeFile` [code] Execute File: {path, confirmed?}
- `file.exportLottie` [file] Lottie JSON...: {comp?, path (.json or .lottie), includeExpressions?: bool, textAsShapes?: bool}
- `file.exportTimeline` [file] Adobe Premiere Pro Project...: {comp?, path (Final Cut Pro XML .xml for Premiere Pro or .fcpxml .otio .edl .aaf .omf), format?: xml|fcpxml|otio|edl|aaf|omf, prerender?: none|unsupported|all, precomps?: nest|prerender}
- `file.findMissing` [file] Find Missing: {what: footage|effects|fonts}
- `file.import` [file] File...: {paths: [string], importAs?: footage|composition|compositionLayerSizes (Photoshop, PDF, Illustrator and EPS files), layer?: name|index (footage of one Photoshop layer), page?: number from 1 (PDF / Illustrator page), drag?: bool (dropped files: Settings ▸ Import ▸ Default Drag Import As), addToComp?: bool (also add them to the active comp, at time?, index?, position? as in layer.addItem), background?: bool (probe the files in a background job, jobs.list / jobs.wait; returns {job})}
- `file.importLottie` [file] Lottie...: {path (.json or .lottie)}
- `file.importMultiple` [file] Multiple Files...: {paths: [string]}
- `file.importPlaceholder` Placeholder...: {name?, width?, height?, frameRate?, duration? (s)}
- `file.importRecent` [file] Import Recent Footage: {index? | path?}
- `file.importSolid` Solid...: {name?, color?, width?, height?}
- `file.importTimeline` [file] Adobe Premiere Pro Project...: {path (.xml Final Cut Pro XML from Premiere Pro or .fcpxml .otio .edl .aaf .omf), format?: auto|xml|fcpxml|otio|edl|aaf|omf, edlFrameRate?: number}
- `file.importVanishingPoint` [file] Vanishing Point (.vpe)...: {} (always disabled: undocumented format)
- `file.incrementAndSave` [file] Increment and Save: {}
- `file.installScript` [code] Install Script File...: {path (.jsx / .js)} → copies it to the Scripts folder; it appears in File ▸ Scripts
- `file.installScriptUIPanel` [code] Install ScriptUI Panel...: {path (.jsx / .js)} → copies it to the ScriptUI Panels folder; it appears in the Window menu
- `file.interpretFootage` Main...: {items?, frameRate?: fps|"file", alpha?: straight|premultiplied|ignore|guess, guessAlpha?, matteColor?, invertAlpha?, loop?, pixelAspect?, fields?: off|upper|lower, colorProfile?: srgb|rec709|rec2020|p3|auto, linearLight?}
- `file.interpretProxy` Proxy...: {items?|item?, alpha?, matteColor?, invertAlpha?, frameRate?, loop?, pixelAspect?, fields?, colorProfile?, linearLight?}
- `file.newCompFromSelection` New Comp from Selection...: {duration? (s, for stills), single?: bool (one comp for all), dimensionsFrom?: index, sequence?: bool, overlap?: bool, overlapDuration? (s), transition?: off|dissolveFront|crossDissolve, addToRenderQueue?: bool} → {comps}
- `file.newFromTemplate` [host] New Project from Template...: {} → the Home screen's Templates tab
- `file.newProject` [file] New Project: {}
- `file.open` [file] Open Project...: {path}
- `file.openDemoProject` Open Demo Project: {}
- `file.openRecent` [file] Open Recent: {index? | path?}
- `file.projectSettings` Project Settings...: {bitDepth?: 8|16|32, colorEngine?: adobe|ocio, workingSpace?: none|srgb|rec709|rec2020|p3|acescg|aces2065, linearize?, blendLinear?, hdr?: clip|compand|toneMap, outputSpace?: srgb|rec709|rec2020|p3|rec2100pq|rec2100hlg, renderer?: gpu|software, timeDisplay?: timecode|frames|feet35|feet16}
- `file.recoveryInfo` [host] Auto-Save Status: {}
- `file.reduceProject` Reduce Project: {} (keeps the selected comps and what they use)
- `file.reloadFootage` [file] Reload Footage: {items?}
- `file.rememberInterpretation` Remember Interpretation: {item?}
- `file.removeUnusedFootage` Remove Unused Footage: {}
- `file.replaceFootage` [file] File...: {path, item?}
- `file.replaceWithLayeredComp` [file] With Layered Comp: {item?}
- `file.replaceWithPlaceholder` Placeholder...: {item?, width?, height?, frameRate?, duration?}
- `file.replaceWithSolid` Solid...: {item?, color?, width?, height?}
- `file.revealInFinder` [host] Reveal in Finder: {item?}
- `file.revert` [file] Revert: {}
- `file.runScript` [code] Run Script File...: {path (.jsx/.js JavaScript, or a .jsonl/.json command script) | name (an installed or sample script, see file.scripts.list) | steps: [{command, params}]}
- `file.save` [file] Save: {path?}
- `file.saveAs` [file] Save As...: {path}
- `file.saveCopy` [file] Save a Copy...: {path}
- `file.saveCopyAsXml` [file] Save a Copy As XML...: {path: .ecprojx}
- `file.scripts.list` List Scripts: {} → [{name, panel, source: installed|sample}]
- `file.setProxy` [file] File...: {path, items?|item?}
- `file.setProxyNone` None: {items?|item?}
- `file.timelineFormats` Timeline Interchange Formats: {}
- `file.uninstallScript` [host] Uninstall Script: {name}
- `file.useProxy` Use Proxy: {items?|item?, on?}
- `file.watchFolder` [file] Watch Folder...: {folder, stop?: bool} → watches the folder for .ecproj files with queued renders, renders them and writes `<project>.status.json`
- `file.watchFolder.poll` [file] Poll Watch Folder: {folder? (default: the watched one)} → renders new projects now: {watching, rendered: [{project, state: done|failed, items: [{comp, status, output}]}]}

## footage

- `footage.check` [file] Check Footage: {items?: [id], wait?} — look for every footage file (in the background unless `wait`) and flag missing items
- `footage.clearInOut` Clear In and Out: {}
- `footage.info` Footage Panel State: {}
- `footage.open` Open in Footage Panel: {item: id|name}
- `footage.overlayEdit` Overlay Edit: {item?, comp?, in? (s), out? (s), at? (comp s; default the current time)}
- `footage.rippleInsertEdit` Ripple Insert Edit: {item?, comp?, in? (s), out? (s), at? (comp s; default the current time)}
- `footage.setIn` Set In Point: {time? | frame? (default: the panel's time)}
- `footage.setOut` Set Out Point: {time? | frame? (default: the panel's time)}
- `footage.setTime` Footage Panel Time: {time? (s, source) | frame?}

## help

- `help.appPage` [network] EffectCraft Home Page: {}
- `help.discord` [network] Join the ArtCraft Discord...: {}
- `help.docs` [network] EffectCraft Help...: {page?: help|scripting|expressions|effects|agents}
- `help.enableLogging` [host] Enable Logging: {on?}
- `help.github` [network] EffectCraft on GitHub: {}
- `help.inAppTutorials` [host] In-App Tutorials...: {} → the Home screen's Learn tab
- `help.onlineTutorials` [network] Online Tutorials...: {}
- `help.reportIssue` [network] Provide Feedback...: {}
- `help.revealLogFile` [host] Reveal Logging File: {}
- `help.sibling` [network] Other ArtCraft Apps: {app: photocraft|vectorcraft|filmcraft|lightcraft|pdfcraft|designcraft, kind?: page|github}
- `help.systemInfo` [host] System Information: {}
- `help.systemReport` [host] System Compatibility Report...: {quiet?}
- `help.website` [network] ArtCraft Website: {}

## item

- `item.metadata` [file] Metadata: {item?: id|name}

## jobs

- `jobs.cancel` Cancel Job: {job?: render|track|maskTrack|warp|camera|roto|task:<n>|all}
- `jobs.list` Background Jobs: {}
- `jobs.wait` Wait for Background Jobs: {}

## keys

- `keys.audioToKeyframes` Convert Audio to Keyframes: {comp?}
- `keys.copy` Copy Keyframes: {}
- `keys.delete` Delete Keyframes: {}
- `keys.easePreset.apply` Apply Ease Preset: {preset (name) | curve: {outInfluence (%), outSpeed, inInfluence (%), inSpeed} | [x1, y1, x2, y2]} — eases every pair of neighbouring selected keyframes; speeds are relative to the segment's average speed
- `keys.easePreset.capture` Ease Curve of Selected Keyframes: {} — the curve between the first selected pair of keyframes
- `keys.easePreset.delete` [host] Delete Ease Preset: {name}
- `keys.easePreset.list` Ease Presets: {} — [{name, builtIn, curve}]
- `keys.easePreset.rename` [host] Rename Ease Preset: {name, newName}
- `keys.easePreset.save` [host] Save Ease Preset: {name, curve? (as for apply; default: the curve between the first selected pair of keyframes)} — replaces a user preset of that name
- `keys.easyEase` Easy Ease: {which?: both|in|out}
- `keys.easyEaseIn` Easy Ease In: {}
- `keys.easyEaseOut` Easy Ease Out: {}
- `keys.exponentialScale` Exponential Scale: {} — two selected Scale keys
- `keys.info` Keyframe Info: {keys?: [{layer, prop, time}]}
- `keys.interpolation` Keyframe Interpolation...: {interpolation?: linear|bezier|continuousBezier|autoBezier|hold, in?|out?: linear|bezier|hold, autoBezier?, continuous?, spatial?: linear|bezier|continuousBezier|autoBezier, roving?}
- `keys.move` Move Keyframes: {delta (s), merge?}
- `keys.nudge` Nudge Keyframes: {frames, merge?}
- `keys.nudgeBackward` Move Keyframes 1 Frame Earlier: {}
- `keys.nudgeBackward10` Move Keyframes 10 Frames Earlier: {}
- `keys.nudgeForward` Move Keyframes 1 Frame Later: {}
- `keys.nudgeForward10` Move Keyframes 10 Frames Later: {}
- `keys.paste` Paste Keyframes: {layers?, prop?|path?, time?}
- `keys.rpfCameraImport` [file] RPF Camera Import: {path: .json|.csv camera data, comp?}
- `keys.select` Select Keyframes: {keys: [{layer, prop: uid | path (or `path`), time (layer s)}], add?, toggle?: bool (Shift+click: in or out of the selection), selectProperties?: bool (their properties and layers too, as a Timeline click does)}
- `keys.selectAll` Select All Keyframes: {layers?, visible?: [{layer, prop}] (every key of these properties)}
- `keys.selectEqual` Select Equal Keyframes: {}
- `keys.selectFollowing` Select Following Keyframes: {}
- `keys.selectLabelGroup` Select Keyframe Label Group: {scope: selected|all|visibleSelected|visibleAll, visible?: [prop uid]}
- `keys.selectPrevious` Select Previous Keyframes: {}
- `keys.set` Edit Keyframe: {layer?, path|prop, time (layer s), newTime?, value?, merge?}
- `keys.setEase` Set Keyframe Ease: {layer?, path|prop, time (layer s), side: in|out, dim?, speed?, influence? %, merge?}
- `keys.setLabel` Keyframe Label: {label: name|none|0–16}
- `keys.setSpatialTangents` Edit Spatial Tangents: {layer?, path|prop, time (layer s), in?: [dx,dy,dz?], out?, break?, merge?}
- `keys.smooth` Smoother: {tolerance? (property units, 1)}
- `keys.timeReverse` Time-Reverse Keyframes: {}
- `keys.toggleHold` Toggle Hold Keyframe: {}
- `keys.toggleTransform` Add or Remove Transform Keyframe: {layers?, prop: anchor|position|scale|rotation|opacity} (Alt+Shift+A/P/S/R/T)
- `keys.transform` Transform Keyframes: {timeScale?, timeAnchor? (comp s), timeOffset? (s), valueScale?, valueAnchor?, valueOffset?, dim? | dims?: [d…], merge?, fromStart?: bool (with merge: values are the whole transform since the drag started)}
- `keys.velocity` Keyframe Velocity...: {inSpeed?, inInfluence? %, outSpeed?, outInfluence? % (numbers or per-dimension arrays), continuous?}
- `keys.wiggle` Wiggler: {apply?: spatial|temporal, noise?: smooth|jagged, dimensions?: one|same|independent, dimension? (index for `one`), frequency? (keys/s, 5), magnitude?, seed?}

## layer

- `layer.addItem` Add Footage to Comp: {item: id|name, time? (s, the In point), index? (1-based stack position; default above the selected layer), position? ([x, y] comp px; default the centre), duration? (s, for a still)}
- `layer.addMarker` Add Marker: {layers?, time?, comment?}
- `layer.addMask` New Mask: {layer?, shape?: rect|ellipse|rounded|polygon|star, rect? [x,y,w,h] (layer space; a polygon or star fits its width), mode?}
- `layer.addShapeItem` Add (Shape): {layer?, kind: group|rect|ellipse|star|polygon|path|fill|stroke|gfill|gstroke|trim|repeater|round|offset|pucker|twist|zigzag|wiggle|merge, group?: uid|path} → {uid, path}
- `layer.addTextAnimator` Animate Text: {layer?, properties: [anchor|position|scale|skew|rotation|opacity|transformAll|lineAnchor|lineSpacing|characterOffset|characterValue|blur|fillColor|fillHue|fillSaturation|fillBrightness|fillOpacity|strokeColor|strokeHue|strokeSaturation|strokeBrightness|strokeOpacity|strokeWidth|tracking|perChar3d], name?}
- `layer.addTextAnimatorProperty` Add Property: {layer?, animator?: uid|index, property: (see layer.addTextAnimator)}
- `layer.addTextSelector` Add Text Selector: {layer?, animator?: uid|index, kind: range|wiggly|expression}
- `layer.align` Align Layers: {edge: left|hcenter|right|top|vcenter|bottom, to?: composition|selection (default composition), layers?}
- `layer.alignVideoToData` [file] Align Video to Data: {layer?, data?: data footage id|name, key?: time field, videoStart?: ISO date-time | hh:mm:ss | timecode | seconds (default: file creation time), dataStart? (comp s of the first sample, 0)}
- `layer.applyTextPreset` Apply Text Animation Preset: {layer?, preset: typewriter|fadeUpCharacters|bounceInWords|trackingIn|scramble|blurIn|jitter|dropInLines}
- `layer.arrange` Arrange: {layers?, to: front|forward|backward|back, index?, above?: layer}
- `layer.autoOrient` Auto-Orient...: {mode: off|alongPath|towardsCamera|towardsPointOfInterest, layers?}
- `layer.autoTrace` Auto-trace...: {layer?, timeSpan?: currentFrame|workArea, channel?: alpha|red|green|blue|luminance, invert?, blur? (px, 1), tolerance? (px, 1), threshold? (%, 50), minimumArea? (px, 10), cornerRoundness? (%, 50), applyToNewLayer?}
- `layer.cameraSettings` Camera Settings...: {layer?, name?, type?, preset?, zoom?, focalLength?, angleOfView?, dof?, focusDistance?, aperture?, fStop?, blurLevel?, position?, poi?}
- `layer.centerAnchor` Center Anchor Point in Layer Content: {layers?}
- `layer.create` [file] Create: {op: editableText|shapesFromText|masksFromText|shapesFromVector, layers?}
- `layer.deleteAllMarkers` Delete All Markers: {layers?}
- `layer.distribute` Distribute Layers: {mode: left|hcenter|right|top|vcenter|bottom (edges or centres), layers?}
- `layer.enablePerChar3D` Enable Per-character 3D: {layer?, enabled?: bool, toggle?: bool}
- `layer.enableTimeRemap` Enable Time Remapping: {layers?, value?}
- `layer.environment` Environment Layer: {layers?, on?}
- `layer.expressions` [code] Enable/Disable Expressions: {layers?, enabled: bool}
- `layer.frameBlending` Frame Blending: {layers?, mode: off|frameMix|pixelMotion}
- `layer.freezeFrame` Freeze Frame: {layers?}
- `layer.freezeOnLastFrame` Freeze On Last Frame: {layers?}
- `layer.hideOtherVideo` Hide Other Video: {layers?}
- `layer.lightSettings` Light Settings...: {layer?, name?, kind?, color?, intensity?, coneAngle?, coneFeather?, falloff?, radius?, falloffDistance?, castsShadows?, shadowDarkness?, shadowDiffusion?, position?, poi?}
- `layer.markersLock` Lock Markers: {layers?, value?}
- `layer.mask.featherFalloff` Feather Falloff: {layer?, mask?, mode: smooth|linear}
- `layer.mask.hideLocked` Hide Locked Masks: {value?}
- `layer.mask.invert` Inverted: {layer?, mask?, value?}
- `layer.mask.lock` Locked: {layer?, mask?, value?}
- `layer.mask.lockOthers` Lock Other Masks: {layer?, mask?}
- `layer.mask.mode` Mask Mode: {layer?, mask?, mode: None|Add|Subtract|Intersect|Lighten|Darken|Difference}
- `layer.mask.motionBlur` Motion Blur: {layer?, mask?, mode: sameAsLayer|on|off}
- `layer.mask.remove` Remove Mask: {layer?, mask?}
- `layer.mask.removeAll` Remove All Masks: {layers?}
- `layer.mask.reset` Reset Mask: {layer?, mask?}
- `layer.mask.set` Mask Settings: {layer?, mask?, field: feather|opacity|expansion, value}
- `layer.mask.shape` Mask Shape...: {layer?, mask?, rect: [x, y, w, h], shape?: rect|ellipse}
- `layer.mask.unlockAll` Unlock All Masks: {layers?}
- `layer.new3dPrimitive` 3D Primitive: {kind: cube|sphere|plane|torus|cone|cylinder, name?, width?, height?, depth?, radius?, tubeRadius?, segments?, rings?, position? [x,y,z], baseColor?, metallic?, roughness?, emissive?, castsShadows?, acceptsShadows?, acceptsLights?}; switches a Classic 3D comp to Advanced 3D
- `layer.newAdjustment` Adjustment Layer: {name?}
- `layer.newCamera` Camera...: {name?, type?: oneNode|twoNode (default twoNode), preset?: 15mm|20mm|24mm|28mm|35mm|50mm|80mm|135mm|200mm, zoom?, focalLength? mm, angleOfView? deg, dof?, focusDistance?, aperture?, fStop?, blurLevel?, lockToZoom?, position? [x,y,z], poi? [x,y,z]}
- `layer.newContentAwareFill` [file] Content-Aware Fill Layer...: {layer?, method?: object|surface|edgeBlend, range?: workArea|entire, alphaExpansion? (px), lightingCorrection?: none|subtle|moderate|strong, referenceLayer?, referenceTime? (s), outputDir?, wait?: bool}
- `layer.newLight` Light...: {kind?: Parallel|Spot|Point|Ambient (default Spot), name?, color?, intensity?, coneAngle?, coneFeather?, falloff?: None|Smooth|Inverse Square Clamped, radius?, falloffDistance?, castsShadows?, shadowDarkness?, shadowDiffusion?, position? [x,y,z], poi? [x,y,z]}
- `layer.newModel` [file] 3D Model Layer: {item?: id or name of a 3D model footage item, path?: a .gltf, .glb or .obj file to import, name?, time? (s), index? (1-based stack position), position? ([x, y] comp px)}; switches a Classic 3D comp to Advanced 3D
- `layer.newNull` Null Object: {name?}
- `layer.newPrimitive` New 3D Primitive: {kind: cube|sphere|plane|torus|cone|cylinder, name?, width?, height?, depth?, radius?, tubeRadius?, segments?, rings?, position? [x,y,z], baseColor?, metallic?, roughness?, emissive?, castsShadows?, acceptsShadows?, acceptsLights?}; switches a Classic 3D comp to Advanced 3D
- `layer.newShape` Shape Layer: {kind?: rect|rounded|ellipse|star|polygon|none, name?, size?, fill?, fillType?, fillBlend?, fillOpacity?, stroke?, strokeType?, strokeBlend?, strokeOpacity?, strokeWidth?, position?}
- `layer.newSolid` Solid...: {name?, color? #hex|[r,g,b], width?, height?, pixelAspect?}
- `layer.newText` Text: {text?, name? (layer name; default the text), position? [x,y], box? [x,y,w,h] (paragraph text, comp space), vertical?, edit? (start editing), font?, style?, size?, fill?, stroke?, applyFill?, applyStroke?, strokeWidth?, tracking?, leading?, baselineShift?, hScale?, vScale?, tsume?, fauxBold?, fauxItalic?, allCaps?, smallCaps?, baseline?, superscript?, subscript?, kerning?, ligatures?, justify?, indentLeft?, indentRight?, indentFirst?, spaceBefore?, spaceAfter?, direction?, composer?, hangingPunctuation?, strokeOverFill?} (attributes as layer.setText)
- `layer.openLayer` [host] Open Layer: {layer?}
- `layer.openSource` Open Layer Source: {layer?}
- `layer.precompose` Pre-compose...: {layers?, name?, mode?: move|leave, adjustDuration?, open?}
- `layer.quality` Quality: {layers?, quality: best|draft|wireframe}
- `layer.rename` Rename: {layer?, name}
- `layer.revealExpressionErrors` Reveal Expression Errors: {comp?}
- `layer.revealInFinder` [host] Reveal in Finder: {layer?}
- `layer.revealSource` Reveal Layer Source in Project: {layer?}
- `layer.sampling` Sampling: {layers?, sampling: bilinear|bicubic}
- `layer.sceneEditDetection` Scene Edit Detection...: {layer?, mode?: markers|split|splitPrecompose, threshold? (0…1, 0.25), wait?: bool}
- `layer.select` Select Layers: {layers: [id|name|#n], add?, toggle?}
- `layer.selectNext` Select Next Layer: {add?}
- `layer.selectPrevious` Select Previous Layer: {add?}
- `layer.sequence` Sequence Layers...: {layers? (in order), overlap?: bool, duration? (s), transition?: off|dissolveFront|crossDissolve}
- `layer.setBlendMode` Blending Mode: {layers?, mode?: Normal|Multiply|Screen|…, step?: ±1}
- `layer.setComment` Layer Comment: {layers?, comment}
- `layer.setMask` Mask Mode: {layer?, mask: index|uid|name, mode?, inverted?}
- `layer.setParent` Parent: {layers?, parent: layer|null}
- `layer.setSwitch` Layer Switch: {layers?, switch: video|audio|solo|lock|shy|collapse|quality|fx|frameBlend|motionBlur|adjustment|threeD|guide|preserveTransparency, value?, merge?}
- `layer.setText` Edit Text: {layer?, range?: [start, end] (characters; default all), text?, font?, style?, size?, fill?, stroke?, applyFill?, applyStroke?, strokeWidth?, tracking?, leading?: px|"auto", baselineShift? px, hScale? %, vScale? %, tsume? %, fauxBold?, fauxItalic?, allCaps?, smallCaps?, baseline?: normal|superscript|subscript, superscript?, subscript?, kerning?: metrics|optical|number, ligatures?, OpenType: discretionaryLigatures?, contextualAlternates?, stylisticAlternates?, stylisticSets?: [1–20], ss01…ss20?, swash?, titling?, ordinals?, fractions?, allSmallCaps?, figureStyle?: default|lining|oldStyle, figureWidth?: default|proportional|tabular, figures?, variations?: {tag: value} (variable font axes; null resets), justify?: left|center|right|justifyLeft|justifyCenter|justifyRight|justifyAll, indentLeft?, indentRight?, indentFirst?, spaceBefore?, spaceAfter?, direction?: ltr|rtl, composer?: everyLine|singleLine, hangingPunctuation?, strokeOverFill?, box?: [x,y,w,h]|null (layer space), vertical?}
- `layer.setTrackMatte` Track Matte: {layer?, matte: layer|null, kind?: alpha|alphaInverted|luma|lumaInverted}
- `layer.setTransform` Transform Value: {layers?, prop: anchor|position|scale|orientation|rotation|opacity, value}
- `layer.settings` Layer Settings...: {layer?, name?, color?, width?, height?, pixelAspect?, affectAll?: bool (default true; false gives this layer its own copy of a solid other layers use)} → {layer, solid?}
- `layer.showAllVideo` Show All Video: {comp?}
- `layer.slip` Slip Edit: {layers?, delta? (seconds) | frames?, merge?}
- `layer.slipBack` Slip Edit 1 Frame Earlier: {layers?}
- `layer.slipForward` Slip Edit 1 Frame Later: {layers?}
- `layer.style.add` Add Layer Style: {style: dropShadow|innerShadow|outerGlow|innerGlow|bevelEmboss|satin|colorOverlay|gradientOverlay|stroke (or display name), layers?}
- `layer.style.bevelEmboss` Bevel and Emboss: {layers?}
- `layer.style.colorOverlay` Color Overlay: {layers?}
- `layer.style.convertToEditable` Convert to Editable Styles: {layers?}
- `layer.style.dropShadow` Drop Shadow: {layers?}
- `layer.style.globalLight` Global Light: {comp?, angle? (deg), altitude? (deg, 0..90), time? (s)} — no values returns the current setting
- `layer.style.gradientOverlay` Gradient Overlay: {layers?}
- `layer.style.innerGlow` Inner Glow: {layers?}
- `layer.style.innerShadow` Inner Shadow: {layers?}
- `layer.style.list` List Layer Styles: {layer?}
- `layer.style.options` [host] Layer Style Options...: {layer?, style?: blendingOptions|dropShadow|…}
- `layer.style.outerGlow` Outer Glow: {layers?}
- `layer.style.remove` Remove Layer Style: {layer?, style: id|name|uid|layerStyles}
- `layer.style.removeAll` Remove All: {layers?}
- `layer.style.satin` Satin: {layers?}
- `layer.style.showAll` Show All: {layers?}
- `layer.style.stroke` Stroke: {layers?}
- `layer.style.toggle` Toggle Layer Style: {layer?, style: id|name|uid|layerStyles, value?}
- `layer.timeReverse` Time-Reverse Layer: {layers?}
- `layer.timeStretch` Time Stretch...: {layers?, percent?|duration? (s), hold?: in|current|out, op?: reverse}
- `layer.timing` Layer Timing: {layers?, op?: moveInToTime|moveOutToTime|trimInToTime|trimOutToTime, delta?, start?, in?, out?, merge?}
- `layer.trackMatte` Track Matte: {layers?, op: none|alpha|alphaInverted|luma|lumaInverted|above|below}
- `layer.transform` Transform: {layers?, op: reset|center|fit|fitWidth|fitHeight|flipH|flipV}
- `layer.tree` Layer Property Tree: {layer?, comp?, depth?, time? (comp s)} → nodes with `path`
- `layer.unlockAll` Unlock All Layers: {comp?}
- `layer.updateMarkersFromSource` Update Markers From Source: {layers?}

## learn

- `learn.list` Learn Tutorials: {} → [{id, title, summary, minutes, steps}]
- `learn.start` Start Tutorial: {id}
- `learn.state` Tutorial State: {} → {tutorial, step, steps, done, current: {title, text, target, accepts}} | null
- `learn.step` [code] Tutorial Step: {action?: next|back|showMe|goto, index?}
- `learn.stop` Close Tutorial: {}

## light

- `light.controlWithCamera` Control Light with Camera: {comp?, layers?: lights (default selected), camera? (default active), on?: bool (default toggle)}
- `light.environmentBackground` Create Environment Light Background Layer: {comp?, layer?: environment light (default selected / first)}
- `light.fromModel` Create Lights from 3D Model: {comp?, layers?: 3D model layers (default selected)}

## liquify

- `liquify.clear` Clear Liquify Mesh: {layer?, effect?: uid}
- `liquify.stroke` Liquify Stroke: {layer?, effect?: uid, tool?: warp|turbulence|twirlClockwise|twirlCounterclockwise|pucker|bloat|shiftPixels|reflection|clone|reconstruction|freeze|thaw, points: [[x,y],…] (layer space), size?, pressure? (1-100), jitter? (1-100), cloneOffset?: [dx,dy], time? (s) | frame?}

## markers

- `markers.convert` Convert Marker: {layer? (layer marker → comp marker), index? | at?, toLayer? (comp marker → layer marker)}
- `markers.delete` Delete Marker: {layer?, index? | at? (comp s)}
- `markers.list` List Markers: {layer? (omit for composition markers)}
- `markers.nested` Nested Comp Markers: {layer: a precomp layer, comp?} — its comp's markers at their times in this comp (as on the layer bar)
- `markers.set` Marker Settings: {layer? (omit: comp marker), index? (0-based) | at? (comp s) | new?: true, time? (comp s), duration? (s), comment?, chapter?, url?, frameTarget?, cuePoint?: {name, navigation?, params?: [[name, value]…]} | null, protected?, label?}

## mask

- `mask.addVertex` Add Mask Vertex: {layer?, mask, point: [x,y], in?, out?, index?}
- `mask.convertVertex` Convert Vertex: {layer?, mask: uid (mask or shape Path item), index, smooth?}
- `mask.deleteVertices` Delete Mask Vertices: {vertices?}
- `mask.featherPoint.add` Add Mask Feather Point: {layer?, mask, segment?, t?: 0..1, point?: [x,y] (layer space), radius? (px, negative = inner), tension? (%)} → {index}
- `mask.featherPoint.list` Mask Feather Points: {layer?, mask}
- `mask.featherPoint.remove` Delete Mask Feather Point: {layer?, mask, index? | all?}
- `mask.featherPoint.set` Move Mask Feather Point: {layer?, mask, index, segment?, t?, point? (slide along the path), radius?, toward?: [x,y] (radius from the drag point, layer space), tension? (%), merge?}
- `mask.insertVertex` Add Vertex: {layer?, mask: uid (mask or shape Path item), segment, t?: 0..1}
- `mask.interpolate` Apply Mask Interpolation: {layer?, mask?, times?: [from, to] (s; default the selected Mask Path keys), keyframeRate?: n|auto, keyframeFields?, linearVertexPaths?, bendingResistance? (0-100), quality? (0-100), addVertices?: n|false, addVerticesUnit?: pixels|total|percent, matchingMethod?: auto|curve|polyline, oneToOne?, firstVerticesMatch?}
- `mask.interpolationOptions` Mask Interpolation Options: {keyframeRate?: n|auto, keyframeFields?, linearVertexPaths?, bendingResistance?, quality?, addVertices?: n|false, addVerticesUnit?: pixels|total|percent, matchingMethod?: auto|curve|polyline, oneToOne?, firstVerticesMatch?}
- `mask.moveVertices` Move Mask Vertices: {vertices?: [{layer, mask, index}], delta: [dx,dy], merge?}
- `mask.new` New Mask from Points: {layer?, vertices: [[x,y]…], inTangents?, outTangents?, closed?, mode?}
- `mask.remove` Remove Mask: {layer?, mask}
- `mask.removeAll` Remove All Masks: {layer?}
- `mask.selectVertices` Select Mask Vertices: {vertices: [{layer, mask, index}], add?, toggle?}
- `mask.setClosed` Closed: {layer?, mask, closed?}
- `mask.setVertex` Set Mask Vertex: {layer?, mask, index, point?, in?, out?, merge?}

## material

- `material.duplicateAssign` Duplicate and Assign Material: {layers? (first = source)}
- `material.reset` Reset Material: {layers?}
- `material.revealSource` Reveal Material Source in Project: {layer?}
- `material.set` Material Options: {layers?, castsShadows?: off|on|only, acceptsShadows?: off|on|only, acceptsLights?, appearsInReflections?, lightTransmission?, ambient?, diffuse?, specularIntensity?, specularShininess?, metal?, baseColor?, metallic?, roughness?, emissive?, time?}

## mediaBrowser

- `mediaBrowser.action` [file] Media Browser Action: {action}: an action from mediaBrowser.list's `actions` (web: openFolder, addFiles, uploadFolder)
- `mediaBrowser.addFavorite` [host] Add to Favorites: {path?}
- `mediaBrowser.fileInfo` [file] File Info: {path}
- `mediaBrowser.go` [file] Go to Folder: {path?: folder | "..", importableOnly?}
- `mediaBrowser.import` [file] Import: {paths, addToComp?, time?, index?, position? (where the layers go, as in layer.addItem)}
- `mediaBrowser.list` [file] List Folder: {path?, importableOnly?}
- `mediaBrowser.removeFavorite` [host] Remove from Favorites: {path?}

## motion

- `motion.sketch` Motion Sketch: {layer?, points: [[t, x, y]…] (t = capture seconds) | [[x, y]…] (one per frame), start? (s, default current time), captureSpeed? (%, 100), smoothing? (px, 1)}

## paint

- `paint.brushPreset` Brush Preset: {preset?: index | name}
- `paint.options` Paint Options: {color?, background?, opacity?, flow?, mode?, channels?, durationMode?, customFrames?, eraseMode?, diameter?, angle?, roundness?, hardness?, spacing?, preset?, sizePressure?, minSize?, opacityPressure?, flowPressure?, clonePreset?, cloneSource?, clonePoint?, cloneOffset?, aligned?, lockSourceTime?, cloneTimeShift?, cloneTime?, showOverlay?, overlayOpacity?, overlayDifference?, reset?}
- `paint.presets` Brush Presets: {}
- `paint.removeStroke` Delete Paint Stroke: {layer?, stroke: uid | name}
- `paint.setCloneSource` Set Clone Source: {layer?, point: [x,y]}
- `paint.stroke` Paint Stroke: {layer?, kind?: brush|clone|eraser, points: [[x,y,pressure?],…] (layer space), time? (s) | frame?, duration? (s, drawing time for Write On), color?: [r,g,b,a], diameter?, angle?, hardness?, roundness?, spacing?, opacity?, flow?, mode?, channels?: RGBA|RGB|Alpha, durationMode?: constant|writeOn|singleFrame|custom, customFrames?, eraseMode?: layerSourceAndPaint|paintOnly|lastStrokeOnly, cloneSource?: layer id, clonePosition?: [x,y], cloneTimeShift? (s), lockSourceTime?, cloneTime? (s), sizePressure?, minSize?, opacityPressure?, flowPressure?}

## path

- `path.convertToBezier` Convert To Bezier Path: {layer?, mask?}
- `path.freeTransform` Free Transform Points: {layer?, mask?, scale?: % | [x%, y%], rotation?: deg, offset?: [dx, dy], anchor?: [x, y]}
- `path.groupShapes` Group Shapes: {layer?, items?: [uid]}
- `path.rotoBezier` RotoBezier: {layer?, mask?, value?}
- `path.setFirstVertex` Set First Vertex: {layer?, mask?: uid, index?}
- `path.ungroupShapes` Ungroup Shapes: {layer?, items?: [group uid]}

## paths

- `paths.nullsFollowPoints` [code] Nulls Follow Points: {layer?, path?|prop?, pins?: [uid | name…]} → a null per vertex (or puppet pin) following it (Position expressions)
- `paths.pointsFollowNulls` [code] Points Follow Nulls: {layer?, path?|prop?, pins?: [uid | name…]} → a null per vertex (or Position / Advanced puppet pin); the path (pins) follow them (expressions)
- `paths.tracePath` [code] Trace Path: {layer?, path?|prop?, loop?: bool} → a null moving along the path (Progress slider)

## playback

- `playback.audio` [host] Audio: {value?: include audio in previews}
- `playback.cacheWhenIdle` [host] Cache Frames When Idle: {value?}
- `playback.scrubAudio` [device] Scrub Audio: {time? (s, default the current time)} — plays one frame of the comp's audio there (Ctrl/Cmd-drag the current time)
- `playback.settings.get` [host] Preview Settings: {shortcut?: spacebar|shiftSpacebar|numpad0|shiftNumpad0|altNumpad0 (default: the one shown in the Preview panel)} → options + plan {start, end, first, step, fps}
- `playback.settings.set` [host] Change Preview Settings: {shortcut?, current?: shortcut shown in the panel, reset?: bool, values?: {…}, includeVideo?, includeAudio?, includeOverlays?, includeLayerControls?, loop?, cacheBeforePlayback?, range?: workArea|workAreaExtended|entireDuration|aroundCurrentTime, preRoll?, postRoll?: seconds, playFrom?: rangeStart|currentTime, frameRate?: fps|"auto", skip?, resolution?: auto|full|half|third|quarter|custom, customResolution?, fullScreen?, playCachedFrames?, moveTimeToPreviewTime?}
- `playback.toggle` [device] Play Current Preview: {shortcut?: spacebar|shiftSpacebar|numpad0|shiftNumpad0|altNumpad0 (whose Preview panel options to use; default spacebar)}

## prefs

- `prefs.get` [host] Get Setting: {key?: e.g. general.undoLevels (omit for all settings)}
- `prefs.open` [host] Open Settings: {page?: general|startup|project|composition|previews|appearance|grids|labels|type|import|export|audio|disk|memory|video|3d|scripting}
- `prefs.pages` [host] Settings Pages: {}
- `prefs.reset` [host] Reset Settings: {page?: general|labels|project|…}
- `prefs.set` [host] Change Setting: {key, value, values?: {key: value}}

## project

- `project.delete` Delete Project Items: {items?: [id|name] (default: selected)} (folders with their contents; layers using the items go too)
- `project.duplicate` Duplicate Project Items: {items?: [id|name] (default: selected)} → {items}
- `project.move` Move to Folder: {items?: [id|name] (default: selected), folder: id|name|null (root)}
- `project.newFolder` New Folder: {name?, parent?}
- `project.rename` Rename Item: {item?: id|name (default: selected), name}
- `project.select` Select Project Items: {items: [id|name], add?}
- `project.setComment` Item Comment: {items?, comment}
- `project.setLabel` Item Label: {items?, label: name|index}
- `project.setProjectComment` Project Comment: {comment}
- `project.summary` Project Summary: {}
- `project.usage` Project Item Usage: {items?: [id|name] (default: selected)} → {items, layers, comps} that deleting them removes

## prop

- `prop.addKey` Add Keyframe: {layer?, path|prop, time?, value?}
- `prop.convertExpressionToKeyframes` [code] Convert Expression to Keyframes: {layer?, path|prop}
- `prop.duplicateGroup` Duplicate Property Group: {layer?, prop: uid} → {prop: new uid}
- `prop.get` Get Property: {layer?, path|prop, time? (comp s)}
- `prop.moveGroup` Reorder Property Group: {layer?, prop: uid, index (1-based among its siblings)}
- `prop.pickWhip` [code] Pick Whip (Link Property): {layer?, path|prop, target: {layer, path|prop}} → sets an AE reference expression
- `prop.removeGroup` Delete Property Group: {layer?, prop: uid}
- `prop.renameGroup` Rename Property Group: {layer?, prop: uid, name}
- `prop.reset` Reset Property: {layer?, path|prop, default?}
- `prop.select` Select Property: {layer?, path|prop, add?, selectKeys?}
- `prop.separateDimensions` Separate Dimensions: {layer?, value?}
- `prop.set` Set Property Value: {layer?, path|prop, value, time?, merge?}
- `prop.setExpression` [code] Add Expression: {layer?, path|prop, expression?, enabled?}
- `prop.setGroupEnabled` Enable Property Group: {layer?, prop: uid, value?}
- `prop.toggleAnimation` Toggle Stopwatch: {layer?, path|prop, value?}
- `prop.toggleKey` Add or Remove Keyframe at Current Time: {layer?, path|prop}

## puppet

- `puppet.addPin` Add Puppet Pin: {layer?, kind?: position|advanced|bend|starch|overlap, position: [x,y] (layer space), time? (s) | frame?, mesh?: uid, newMesh?, density?, expansion?, triangles?}
- `puppet.follow` [code] Follow-Through...: {layer?, leader?: uid | name (default: the first selected pin), pins?: [uid | name…] (default: the other selected pins), delay? (s, 0.1), amount? (%, 100), cascade? (true: the k-th nearest pin trails k × delay)}
- `puppet.info` Puppet Mesh Info: {layer?, time? | frame?}
- `puppet.mesh` Puppet Mesh Options: {layer?, mesh?: uid, density?, expansion?, triangles?, showMesh?, time?, merge?}
- `puppet.movePin` Move Puppet Pin: {layer?, pin: uid | name, position: [x,y], time? | frame?, merge?}
- `puppet.recordOptions` Record Options...: {speed? (%, 100), smoothing? (0-100), useDraftDeformation?, showMesh?}
- `puppet.recordPin` Record Puppet Pin: {layer?, pin: uid | name, samples: [[t, x, y]…] (t = seconds since the drag began, layer space), pins?: [uid | name…] (more pins that move by the same displacement), start? (s, default current time), speed? (%, Record Options), smoothing? (Record Options)}
- `puppet.removePin` Delete Puppet Pin: {layer?, pin?: uid | name, pins?: [uid | name…]} (default: the selected pins)
- `puppet.selectPins` Select Puppet Pins: {layer?, pins: [uid | name…] ([] deselects), add?, toggle?}
- `puppet.setPin` Edit Puppet Pin: {layer?, pin: uid | name, position?, scale? (%), rotation? (°), amount? (%), extent? (px), inFront?, time? | frame?, merge?}

## render

- `render.addOutputModule` Add Output Module: {item?|index? (default: the last item), format?, output?, channels?, quality?, bitrate?, proresProfile?, audio?}
- `render.backend` Video Rendering and Effects: {backend?: gpu|cpu}
- `render.preRender` Pre-render...: {comp?, output?: path|template, format?}
- `render.saveCurrentPreview` [file] Save Current Preview...: {comp?, path}

## renderQueue

- `renderQueue.add` Add to Render Queue: {comp?: id|name, template?: Render Settings template, format?: h264|hevc|av1|prores|webm|png|jpeg|tiff|exr|gif|wav|aiff, output?: path|template, log?: errorsOnly|plusSettings|plusPerFrameInfo, quality?: best|draft|1-100 (jpeg, webm), resolution?: full|half|third|quarter|scale, proxyUse?, effects?, solo?, guideLayers?, colorDepth?, frameBlending?, fieldRender?, pulldown?, motionBlur?, timeSpan?: workArea|comp|custom, start?: s, end?: s, duration?: s, frameRate?: fps|null, skipExisting?: bool, storageOverflow?: bool, channels?: rgb|rgba|alpha, color?, bitrate?: kbps, webmBitrate?, keyframeInterval?, proresProfile?, profile?, level?, rateControl?, webmCodec?, audioBitrate?, opusApplication?, crop?, resize?, includeProjectLink?, audio?: auto|on|off, sampleRate?, audioChannels?, audioFormat?, loop?: bool (values as in setRenderSettings / setOutputModule)}
- `renderQueue.applyTemplate` Apply Template: {item?|index?, renderSettings?: template name, outputModule?: template name, module?: n}
- `renderQueue.deleteTemplate` Delete Template: {kind: renderSettings|outputModule, name}
- `renderQueue.duplicate` Duplicate Render Item: {item?|index?}
- `renderQueue.formats` Output Formats: {}
- `renderQueue.list` Render Queue Items: {}
- `renderQueue.move` Move in Render Queue: {item?|index?, to: n (1-based)}
- `renderQueue.remove` Remove from Render Queue: {item?: id, index?: n}
- `renderQueue.render` [file] Render: {wait?: bool (default true; the UI renders in the background)}
- `renderQueue.saveTemplate` Save Template...: {kind: renderSettings|outputModule, name, item?|index? (save from this item), from?: template to copy, module?, params?: {setRenderSettings / setOutputModule keys}}
- `renderQueue.setLog` Render Queue: Log: {item?|index?, log: errorsOnly|plusSettings|plusPerFrameInfo}
- `renderQueue.setNotify` Notify When Done: {notify?: bool (toggles)}
- `renderQueue.setOutput` Output To...: {item?|index?, module?: n, path: file path or template like [compName].[fileExtension]}
- `renderQueue.setOutputModule` Output Module Settings...: {item?|index?, module?: n (1 = first), template?: name, format?: h264|hevc|av1|prores|webm|png|jpeg|tiff|exr|gif|wav|aiff, channels?: rgb|rgba|alpha, color?: straight|premultiplied, quality?: 1-100, bitrate?: kbps, webmBitrate?: bool, keyframeInterval?: frames (0 = auto), proresProfile?: proxy|lt|standard|hq|4444|4444xq, profile?: main|main10 (HEVC/AV1), level?: auto|4.1, rateControl?: bitrate|quality, webmCodec?: vp9|av1, audioBitrate?: Opus kbps, opusApplication?: audio|voice, crop?: bool|{useRoi?, roi?, top?, left?, bottom?, right?}, cropTop?, cropLeft?, cropBottom?, cropRight?, resize?: bool|{preset?, width?, height?, lockAspect?, quality?: low|high}, resizeWidth?, resizeHeight?, postRenderAction?: none|import|importAndReplace|setProxy, includeProjectLink?: bool, audio?: auto|on|off, sampleRate?, audioChannels?: mono|stereo, audioFormat?: 16|24|32, loop?: bool, output?}
- `renderQueue.setOverflowFolders` Storage Overflow Folders: {folders: [path, …] (used in order when the output volume is full)}
- `renderQueue.setRender` Render Queue: Render Checkbox: {item?|index?, render?: bool (toggles)}
- `renderQueue.setRenderSettings` Render Settings...: {item?|index?, template?: name, quality?: best|draft, resolution?: full|half|third|quarter|scale, proxyUse?: current|all|comp|none, effects?: current|allOn|allOff, solo?: current|allOff, guideLayers?: current|allOff, colorDepth?: current|8|16|32, frameBlending?: current|onForChecked|offForAll, fieldRender?: off|upper|lower, pulldown?: off|WSSWW|SSWWW|SWWWS|WWWSS|WWSSW, motionBlur?: bool|current|onForChecked|offForAll, timeSpan?: workArea|comp|custom, start?: s, end?: s, duration?: s, frameRate?: fps|null, skipExisting?: bool, storageOverflow?: bool}
- `renderQueue.setTemplateDefault` Set Template Default: {kind: renderSettings|outputModule, slot: movie|still|preRender|movieProxy|stillProxy, name}
- `renderQueue.stop` Stop Rendering: {}
- `renderQueue.templates` Render Templates: {}

## roto

- `roto.cancel` Stop Roto Brush: {}
- `roto.clearStrokes` Remove Roto Brush Strokes: {layer?, effect?, frame? (only this layer frame), kind?}
- `roto.freeze` Freeze: {layer?, effect?, wait?}
- `roto.model.download` [network] Download Roto Brush Model: {id, wait?} — the official weights, verified (desktop; uses the system curl)
- `roto.model.install` [file] Install Roto Brush Model: {path: weights file, id?, wait?} — verified against the registry's SHA-256
- `roto.model.remove` [host] Remove Roto Brush Model: {id}
- `roto.model.select` [host] Use Roto Brush Model: {id: classical|mobilesam, wait?} (Roto Brush 2.0 / 3.0 use it; wait: loaded before returning)
- `roto.models` Roto Brush Models: {}
- `roto.options` Roto Brush Options: {diameter?, refineDiameter?, view?: alphaBoundary|alpha|alphaOverlay|none, overlayColor?: [r,g,b], overlayOpacity?, boundaryColor?: [r,g,b], autoPropagate?}
- `roto.propagate` Propagate Roto Brush: {layer?, effect?, direction?: forward|backward|both, to? (layer frame: extend the span), wait?: block until done}
- `roto.span` Set Segmentation Span: {layer?, effect?, start?, end? (layer frames)}
- `roto.status` Roto Brush Status: {layer?, effect?, frame? (layer frame: also report its matte), matte?: include the matte (RLE, base64), compute?: compute the frame if needed, compareTo?: RLE matte to report the IoU against}
- `roto.stroke` Roto Brush Stroke: {layer?, kind?: fg|bg|refine|refineErase, points: [[x, y], …] (layer pixels), frame? (layer frame; default current), radius? (layer px; default the tool's diameter / 2), effect?}
- `roto.unfreeze` Unfreeze: {layer?, effect?}

## scopes

- `scopes.analyze` Lumetri Scopes: {comp?, time?|frame?, scope?: waveformRgb|waveformLuma|waveformYc|vectorscopeYuv|vectorscopeHls|histogram|paradeRgb|paradeYuv, standard?: rec601|rec709|rec2020, float?, clamp?, size?}

## script

- `script.run` [code] Run Script: {code (JavaScript: app, app.project, CompItem, Layer, Property… like After Effects scripting), name?, console? (Script Console context)} → {ok, result, output, error: {message, line, column} | null}

## scriptui

- `scriptui.click` [code] Click Script Window Control: {window?: id | title, widget: id | "#id" | properties.name | text} (buttons, checkboxes, radio buttons, tabs)
- `scriptui.close` [code] Close Script Window: {window?, result? (a dialog's show() returns it; default 2 = Cancel)}
- `scriptui.get` Script Window Controls: {window?: id | title} → {id, title, kind, root: {id, type, name, text, value, checked, items, selection, bounds, enabled, draw (onDraw paint list), children…}}
- `scriptui.list` List Script Windows: {} → [{window, title, kind: dialog|palette|window|panel, script, modal, size}]
- `scriptui.set` [code] Set Script Window Control: {window?, widget, value: text | number | bool | item index | item text, changing?: bool (a live update: each keystroke / slider step; fires onChanging only)} (edit text, sliders, checkboxes, lists) → fires onChanging / onChange

## shape

- `shape.dashes.add` Add Dash or Gap: {layer?, prop?: stroke uid}
- `shape.dashes.remove` Remove Dash or Gap: {layer?, prop?: stroke uid}
- `shape.fillStroke` Fill and Stroke: {layers?, fill?: color, stroke?: color, strokeWidth?, merge?} → {fill, stroke, strokeWidth} (first layer)
- `shape.newPath` Pen Tool (Shape Path): {layer?, vertices: [[x,y]…], inTangents?, outTangents?, closed?, space?: comp|layer, fill?: [r,g,b]|#hex|false, fillType?, fillBlend?, fillOpacity?, stroke?, strokeType?, strokeBlend?, strokeOpacity?, strokeWidth? (default the Tools bar's, shape.toolOptions), name?}
- `shape.newShape` Shape Tool: {layer?, kind?: rect|rounded|ellipse|polygon|star, size? [w,h], position? [x,y] (the centre), space?: comp|layer, name? (a new layer's), fill?, fillType?, fillBlend?, fillOpacity?, stroke?, strokeType?, strokeBlend?, strokeOpacity?, strokeWidth? (default the Tools bar's, shape.toolOptions)} → {layer, group}
- `shape.stroke.taper` Stroke Taper: {layer?, prop?: stroke uid, units?: pixels|percent, startLength?, endLength?, startWidth? (%), endWidth? (%), startEase? (%), endEase? (%)}
- `shape.stroke.wave` Stroke Wave: {layer?, prop?: stroke uid, amount? (%), units?: pixels|cycles, wavelength?, cycles?, phase? (°)}
- `shape.toolOptions` Shape Tool Options: {createsMask?: bool (Tool Creates Mask with a shape layer selected), fill?: [r,g,b]|#hex|false, fillType?: none|solid|linear|radial, fillBlend?: blend mode, fillOpacity? %, stroke?, strokeType?, strokeBlend?, strokeOpacity?, strokeWidth? px, reset?} → the options

## shortcuts

- `shortcuts.conflicts` [host] Keyboard Shortcut Conflicts: {}
- `shortcuts.export` [file] Export Keyboard Shortcuts: {preset?, path?}
- `shortcuts.import` [file] Import Keyboard Shortcuts: {path? | preset?: exported document, name?}
- `shortcuts.list` [host] List Keyboard Shortcuts: {query?, keys?, assigned?: bool}
- `shortcuts.preset` [host] Keyboard Shortcut Preset: {op: select|new|duplicate|delete|rename, name?, from?, newName?}
- `shortcuts.reset` [host] Reset Keyboard Shortcuts: {command?, params?}
- `shortcuts.set` [host] Set Keyboard Shortcut: {command, params?, keys: "Cmd+Shift+K" | [keys] | null}

## storage

- `storage.clear` [host] Clear Browser Storage: {what: diskCache|media|projects|autoSaves|all} → {cleared, entries, bytes}
- `storage.info` [host] Browser Storage: {} → {available, backend, usage, quota, persisted, files: {count, bytes, media, projects, autoSaves}, diskCache: {enabled, entries, bytes, maxBytes, hits, misses, writes, evictions}}
- `storage.persist` [host] Request Persistent Storage: {} → {requested, persisted}

## templates

- `templates.create` [file] New Project from Template: {id (templates.list) | path (.ectemplate), projectPath? (save the new project there; embedded footage goes to `<stem> Footage/` next to it), footageDir?} → an untitled copy (embedded footage extracted: `footage` lists the files)
- `templates.delete` [file] Delete Template: {id: user/<file>}
- `templates.list` [file] Project Templates: {thumbnails?: bool} → [{id, name, description, category, builtin, width, height, frameRate, duration, controls, thumbnail? (hex RGB 256×144)}]
- `templates.saveAs` [file] Save as Template...: {name, description?, category?, overwrite?: bool (default true), embedFootage?: bool (default true: the footage files go into the template), embedLimitMB? (default 256: footage beyond it stays linked, with a warning)} → {id, path, embedded, embeddedBytes, linked: [{item, name, reason}], warning?}
- `templates.thumbnail` [file] Template Thumbnail: {id} → {id, width, height, rgb (hex)}

## text

- `text.addSelector` Add Text Selector: {layer?, animator?: uid|index, kind: range|wiggly|expression}
- `text.animatorFontAxes` Variable Font Axes: {layer?, axis?: tag|name (omit to list the font's axes), animator?: uid|index}
- `text.animatorProperties` Text Animator Properties: {}
- `text.delete` Delete Text: {layer?, direction?: backward|forward, word?, range?: [start, end], merge?}
- `text.edit` Edit Text: {layer?, select?: [anchor, caret]|"none", caret?, created?} → {layer, anchor, caret}
- `text.endEdit` Exit Text Editing: {}
- `text.fontFeatures` OpenType Features: {layer?, font?, style?} → {family, style, features: [tags], options: {smallCaps, superscript, stylisticSets: [n], fractions, …}} (what the font sets with its own glyphs)
- `text.fonts` [file] List Fonts: {query?, rescan?} → {count, families: [{family, styles, origin: bundled|system|user, nativeName?}], added} (rescan picks up fonts installed since launch)
- `text.insert` Type Text: {layer?, text, range?: [start, end], merge?} (replaces the selection)
- `text.moveCaret` Move Text Caret: {to: left|right|wordLeft|wordRight|lineStart|lineEnd|up|down|paraStart|paraEnd|start|end, extend?}
- `text.presets` Text Animation Presets: {}
- `text.removeAllAnimators` Remove All Text Animators: {layers?}
- `text.setSelection` Set Text Selection: {layer?, start, end?} | {anchor, caret} | {select: "all"} (character indices)

## time

- `time.back10` Back 10 Frames: {}
- `time.end` Go to End: {}
- `time.forward10` Forward 10 Frames: {}
- `time.go` Go To: {to: start|end|workStart|workEnd|layerIn|layerOut|nextKey|prevKey, prop?: uid, visible?: [{layer, prop}]}
- `time.layerIn` Go to Layer In Point: {}
- `time.layerOut` Go to Layer Out Point: {}
- `time.nextFrame` Next Frame: {}
- `time.nextKey` Go to Next Keyframe or Marker: {visible?: [{layer, prop}] (only these properties' keys, as the Timeline shows them)}
- `time.previousFrame` Previous Frame: {}
- `time.previousKey` Go to Previous Keyframe or Marker: {visible?: [{layer, prop}]}
- `time.set` Go to Time...: {time? (s) | frame? | timecode?}
- `time.start` Go to Start: {}
- `time.step` Step Frames: {frames}

## track

- `track.analyze` Analyze: {layer?, tracker?, direction?: forward|backward|frameForward|frameBackward, start? (s), end? (s), wait?: block until done}
- `track.apply` Apply: {layer?, tracker?, dimensions?: xy|x|y}
- `track.camera` Track Camera: {layer?, shotType?: fixed|variable|specify, aov?: degrees, solveMethod?: auto|typical|flat|tripod, detailed?, lensDistortion?: solve k1/k2, undistort?: render undistorted, wait?}
- `track.delete` Delete Tracker: {layer?, tracker?}
- `track.editTargetDialog` [host] Edit Target...: {}
- `track.extractFaceMeasurements` Extract & Copy Face Measurements: {layer?} → keys a Face Measurements effect from the layer's Face Track Points (one key per tracked frame) and copies those keys
- `track.mask` Track Mask: {layer?, mask?: uid|index|name, method?: position|positionScale|positionScaleRotation|positionScaleRotationSkew|perspective|faceOutline|faceDetailed, direction?: forward|backward|frameForward|frameBackward, start? (s), end? (s), wait?: block until done}
- `track.maskMethod` Mask Tracking Method: {method: position|positionScale|positionScaleRotation|positionScaleRotationSkew|perspective|faceOutline|faceDetailed}
- `track.motion` Track Motion: {layer?}
- `track.new` New Tracker: {layer?, kind?: transform|stabilize|affine|perspective|raw, position?, rotation?, scale?, target?: layer}
- `track.options` Motion Tracker Options: {layer?, tracker?, name?, channel?: rgb|luminance|saturation, blur? (px, 0 = off), enhance?, subpixel?, adaptEveryFrame?, threshold? (%), action?: continue|stop|extrapolate|adapt, trackShape?}
- `track.optionsDialog` [host] Options...: {}
- `track.property` Track this Property: {layer?, source?: layer}
- `track.reset` Reset: {layer?, tracker?}
- `track.select` Current Track: {layer?, tracker?: uid|name|#n}
- `track.setPoint` Move Track Point: {layer?, tracker?, point: n (1-based), center?: [x,y], featureSize?: [w,h], searchOffset?: [x,y], searchSize?: [w,h], attachOffset?: [x,y], move?: [dx,dy], time? (s)}
- `track.setTarget` Edit Target: {layer?, tracker?, target: layer|null}
- `track.setType` Track Type: {layer?, tracker?, kind?: transform|stabilize|affine|perspective|raw, position?, rotation?, scale?}
- `track.stabilize` Stabilize Motion: {layer?}
- `track.status` Tracker Status: {layer?, tracker?}
- `track.stop` Stop Analysis: {}
- `track.warpStabilizer` Warp Stabilizer VFX: {layer?, wait?}

## view

- `view.3d.activeCamera` Active Camera: {comp?}
- `view.3d.back` Back: {comp?}
- `view.3d.bottom` Bottom: {comp?}
- `view.3d.custom1` Custom View 1: {comp?}
- `view.3d.custom2` Custom View 2: {comp?}
- `view.3d.custom3` Custom View 3: {comp?}
- `view.3d.default` Default: {comp?}
- `view.3d.front` Front: {comp?}
- `view.3d.last` Switch to Last 3D View: {comp?}
- `view.3d.left` Left: {comp?}
- `view.3d.right` Right: {comp?}
- `view.3d.top` Top: {comp?}
- `view.addGuide` Add Guide...: {orientation?: vertical|horizontal, position? (comp px)}
- `view.assign3dShortcut` [host] Assign Shortcut to 3D View: {slot: F10|F11|F12, comp?}
- `view.channel` [host] Show Channel: {channel: rgb|red|green|blue|alpha|rgbStraight, colorized?, toggle?}
- `view.clearGuides` Clear Guides: {comp?}
- `view.closeLockedViewer` [host] Close Locked Viewer: {}
- `view.customRgb` [file] My Custom RGB...: {name?, red?/green?/blue?/white?: [x, y] (CIE xy), gamma?, srgbCurve?: bool, icc?: path (RGB ICC profile, matrix/TRC or LUT-based A2B0: read when it changes, "" clears; a LUT profile simulates through its tables, `lutProfile` in the reply; typed-in numbers replace it), reload?, from?: a Simulate Output profile to start from, reset?, preserveRgb?, apply?: bool (default true: simulate it)} → the definition (kept in Settings)
- `view.displayColor` [host] Viewer Color State: {} → display colour management, output simulation, display profile, locked viewer
- `view.displayColorManagement` [host] Use Display Color Management: {value?}
- `view.exportGuides` [file] Export Guides...: {path}
- `view.exposure` [host] Adjust Exposure: {stops? | delta?}
- `view.extendedViewer` [host] Extended Viewer: {value?}
- `view.fastPreviewMode` [host] Fast Previews: {mode: off|adaptive|draft|fastDraft|wireframe}
- `view.fullScreen` [host] Enter Full Screen: {}
- `view.get3D` 3D View State: {comp?} → current 3D view, view camera, active camera layer, lights
- `view.grid` [host] Show Grid: {value?}
- `view.guides` [host] Show Guides: {value?}
- `view.importGuides` [file] Import Guides...: {path}
- `view.layerControls` [host] Show Layer Controls: {value?}
- `view.layout` [host] Switch View Layout: {views: 1|2|4}
- `view.lockGuides` [host] Lock Guides: {value?}
- `view.lookAtAll` Look at All Layers: {comp?}
- `view.lookAtSelected` Look at Selected Layers: {comp?, layers?}
- `view.moveGuide` Move Guide: {comp?, index, position (comp px), merge?}
- `view.newViewer` [host] New Viewer: {} — another Composition viewer of the active comp; the one in use is locked
- `view.options` [host] View Options...: {}
- `view.panelBackground` [host] Panel Background Color: {color?: black|darkGray|mediumGray|lightGray|white|custom|#hex, pick?: true}
- `view.removeGuide` Remove Guide: {comp?, index}
- `view.res.custom` [host] Custom...: {factor?: 1..40 (render every n-th pixel)}
- `view.res.full` [host] Full: {}
- `view.res.half` [host] Half: {}
- `view.res.quarter` [host] Quarter: {}
- `view.res.third` [host] Third: {}
- `view.reset3DView` Reset 3D View: {comp?}
- `view.resetExposure` [host] Reset Exposure: {}
- `view.rulers` [host] Show Rulers: {value?}
- `view.set3DView` Switch 3D View: {view: activeCamera|default|front|left|top|back|right|bottom|custom1|custom2|custom3, comp?}
- `view.set3DViewCamera` Set 3D View Camera: {view, eye? [x,y,z], poi? [x,y,z], zoom?}
- `view.setRegionOfInterest` Region of Interest: {rect?: [x, y, w, h] | null}
- `view.shareViewOptions` [host] Share View Options: {value?}
- `view.showSnapshot` [host] Show Snapshot: {value?}
- `view.simulateOutput` [host] Simulate Output: {profile: none|rec709|ntsc|pal|mac18|srgb|rec2020|p3|linear|myCustom|custom, preserveRgb?, space? (custom)}
- `view.snapToGrid` [host] Snap to Grid: {value?}
- `view.snapToGuides` [host] Snap to Guides: {value?}
- `view.snapping` [host] Snapping: {value?}
- `view.snappingOptions` [host] Snapping Options: {edgesExtended?, edges?, corners?, centers?, anchorPoints?, paths?: bool, toggle?: one of those keys} → the options
- `view.splitLockedViewer` [host] Split with New Locked Viewer: {comp?, view?: 3D view id}
- `view.takeSnapshot` [host] Take Snapshot: {comp?, scale?: 0.05..1}
- `view.zoomIn` [host] Zoom In: {}
- `view.zoomOut` [host] Zoom Out: {}

## warp

- `warp.analyze` Analyze: {layer?, effect?: uid|name|index, wait?: block until done}
- `warp.cancel` Cancel: {}
- `warp.status` Warp Stabilizer Status: {layer?, effect?}

## window

- `window.assignWorkspaceShortcut` [host] Assign Shortcut to Workspace: {slot: Shift+F10|Shift+F11|Shift+F12, workspace?}
- `window.editWorkspaces` [host] Edit Workspaces...: {name, rename?: new name, delete?: bool}
- `window.panel` [host] Show Panel: {panel: project|effectControls|composition|layer|timeline|info|audio|preview|effectsPresets|properties|character|paragraph|align|tracker|wiggler|smoother|motionSketch|paint|brushes|renderQueue|flowchart|history|markers|tools|lumetriScopes|footage|mediaBrowser|metadata|progress|contentAwareFill|createNullsFromPaths|vrCompEditor|easePresets}
- `window.resetWorkspace` [host] Reset to Saved Layout: {}
- `window.saveWorkspace` [host] Save Changes to this Workspace: {}
- `window.saveWorkspaceAs` [host] Save as New Workspace...: {name}
- `window.scriptPanel` [code] ScriptUI Panel: {name (a script in the ScriptUI Panels folder, e.g. `Layer Tools.jsx`)} → opens it as a dockable panel
- `window.workspace` [host] Workspace: {name: Default|Standard|Small Screen|Animation|Effects|Motion Tracking|Paint|Text|Minimal|All Panels}
