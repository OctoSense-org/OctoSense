# film engine commands

The filmcraft engine's command catalog at revision 523185244336: 675 commands, one per line as `id` label: params.
Generated from the engine by `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-film-service --test skill`; do not edit.

A tag after the id marks a command that reaches past the open document (safety.json has every id's class): [file] reads or writes a path, [code] installs or runs code or other commands, [network] uses the network, [device] uses hardware, [host] changes windows, views, preferences, the clipboard or other app state. An untagged command works on the open document only.

## audio

- `audio.toggleScrubbing` [host] Toggle Audio During Scrubbing: {}
- `audio.voiceover.settings` [device] Voice-Over Record Settings: {"source":str?,"inputChannel":n?,"name":str?,"countdownSoundCues":bool?,"prerollSeconds":f64?,"postrollSeconds":f64?}
- `audio.voiceover.start` [device] Start Voice-over Recording: {"track":"A1"|id?,"time":ticks?,"preroll":seconds?}
- `audio.voiceover.stop` [device] Stop Voice-over Recording: {"time":ticks?,"dir":str?,"discard":bool?}
- `audio.voiceover.sync` [device] Sync Voice-over Capture: {"time":ticks?}

## captions

- `captions.add` Add Caption at Playhead: {"track":id|"C1"?,"text":str?,"time":ticks?,"seconds":f64?,"durationSeconds":f64=3}
- `captions.delete` Delete Captions: {"captions":[id]?,"ripple":bool=false}
- `captions.deleteTrack` Delete Caption Track: {"track":id|"C1"}
- `captions.export` [file] Captions…: {"path":str,"format":"srt|vtt|scc|mcc|stl|ttml|dfxp"? (default: from the extension),"track":id|"C1"?,"dropFrame":bool=true}
- `captions.goTo` Go to Caption: {"caption":id}
- `captions.hideAll` Hide All Caption Tracks: {}
- `captions.import` [file] Import Captions…: {"path":str,"name":str?}
- `captions.list` List Captions: {"track":id|"C1"?}
- `captions.merge` Merge Captions: {"captions":[id]?}
- `captions.move` Move Captions: {"captions":[id]?,"delta":ticks|"deltaFrames":i64}
- `captions.newTrack` Add New Caption Track…: {"format":"Subtitle|CEA-608|CEA-708|Teletext","name":str?,"language":str?}
- `captions.next` Go to Next Caption Segment: {}
- `captions.previous` Go to Previous Caption Segment: {}
- `captions.select` Select Captions: {"captions":[id],"add":bool?}
- `captions.setStyle` Caption Track Style: {"track":id|"C1","font":str?,"size":f32?,"color":"#rrggbb[aa]"?,"background":bool?,"backgroundColor":"#rrggbbaa"?,"align":"left|center|right"?,"anchor":"top|middle|bottom"?,"margin":0..0.45 (fraction of frame height)?,"lineSpacing":f32?,"outline":f32?,"outlineColor":str?,"reset":bool?}
- `captions.setText` Edit Caption Text: {"caption":id?,"text":str?,"speaker":str|null?}
- `captions.setTimes` Set Caption In/Out: {"caption":id?,"startTime|startFrame|startSeconds|startTimecode":…,"endTime|endFrame|endSeconds|endTimecode":…}
- `captions.setTrack` Caption Track Settings: {"track":id|"C1","name":str?,"format":str?,"language":str?,"enabled":bool?,"locked":bool?,"syncLock":bool?}
- `captions.showActiveOnly` Show Active Caption Tracks Only: {"track":id|"C1"?}
- `captions.showAll` Show All Caption Tracks: {}
- `captions.split` Split Caption: {"caption":id?,"time":ticks?}
- `captions.trim` Trim Caption: {"caption":id,"edge":"in|out","delta":ticks|"deltaFrames":i64}

## clip

- `clip.audioChannels` Audio Channels…: {"items":[id]?,"format":"mono|stereo|5.1|adaptive","clips":[[channel]]?,"channels":[channel]?}
- `clip.audioGain` Audio Gain…: {"clips":[id]?,"mode":"set|adjust|normalizeMax|normalizeAll"?,"db":f64,"relative":bool?}
- `clip.audioPeak` Audio Clip Peak Amplitude: {"clips":[id]?}
- `clip.automateToSequence` Automate to Sequence…: {"items":[id]?,"ordering":"sort|selection","placement":"sequentially|unnumberedMarkers","method":"insert|overwrite","overlapFrames":n=30,"stillFrames":n?,"videoTransition":bool=true,"audioTransition":bool=true,"ignoreAudio":bool=false,"ignoreVideo":bool=false}
- `clip.breakoutToMono` Breakout to Mono: {"items":[id]?}
- `clip.clearPosterFrame` Clear Poster Frame: {"item":id?}
- `clip.createMulticam` Create Multi-Camera Source Sequence…: {"items":[id]?,"name":str?,"method":"in|out|timecode|marker|audio","ignoreHours":bool?,"marker":str?,"offset":frames?,"audio":"camera1|all|switch","cameraNames":"clip|track|metadata","processedBin":bool?,"reference":item?}
- `clip.editOffline` Edit Offline…: {"item":id?,"mediaName":str?,"tapeName":str?,"description":str?,"scene":str?,"shot":str?,"logNote":str?}
- `clip.editSubclip` Edit Subclip…: {"item":id?,"start":ticks?,"end":ticks?,"startFrame":i64?,"endFrame":i64?,"restrictTrims":bool?,"convertToMaster":bool?}
- `clip.enable` Enable: {"clips":[id]?}
- `clip.extractAudio` [file] Extract Audio: {"items":[id]?,"dir":str?}
- `clip.fieldOptions` Field Options…: {"clips":[id]?,"reverseFieldDominance":bool=false,"processing":"none|alwaysDeinterlace|flickerRemoval"}
- `clip.fillFrame` Fill frame: {"clips":[id]?}
- `clip.fitToFrame` Fit to frame: {"clips":[id]?}
- `clip.frameHold` Add Frame Hold: {"clips":[id]?,"time":ticks?}
- `clip.frameHoldOptions` Frame Hold Options…: {"clips":[id]?,"enabled":bool=true,"holdOn":"sourceTimecode|sequenceTime|in|out|playhead","time":ticks?,"timecode":str?,"holdFilters":bool=false}
- `clip.generateAudioWaveform` Generate Audio Waveform: {"items":[id]?}
- `clip.group` Group: {}
- `clip.insertFrameHoldSegment` Insert Frame Hold Segment: {"clip":id?,"time":ticks?,"seconds":f64=2}
- `clip.interpretFootage` Interpret Footage…: {"items":[id]?,"colorSpace":"auto"|"<color space id>"}
- `clip.link` Link: {"clips":[id]?}
- `clip.makeSubclip` Make Subclip…: {"item":id?,"clip":id?,"name":str?,"start":ticks?,"end":ticks?,"startFrame":i64?,"endFrame":i64?,"restrictTrims":bool=true}
- `clip.mergeClips` Merge Clips…: {"items":[id]?,"method":"in|out|timecode|marker|audio","name":str?,"removeVideoAudio":bool?,"ignoreHours":bool?,"marker":str?,"offset":frames?}
- `clip.modifyTimecode` Timecode…: {"item":id?,"timecode":"HH:MM:SS:FF"?,"frame":i64?,"tapeName":str?,"reset":bool?}
- `clip.multicamEnable` Enable: {"clips":[id]?,"enabled":bool?}
- `clip.multicamFlatten` Flatten: {"clips":[id]?}
- `clip.nest` Nest…: {"name":str}
- `clip.nudgeVolumeDown1` Nudge Volume -1dB: {}
- `clip.nudgeVolumeDown3` Nudge Volume -3dB: {}
- `clip.nudgeVolumeUp1` Nudge Volume +1dB: {}
- `clip.nudgeVolumeUp3` Nudge Volume +3dB: {}
- `clip.remix` Remix: {"clip":id?,"duration":ticks?,"seconds":f64?,"frame":n?,"timecode":str?,"segments":0..100?,"variations":0..100?}
- `clip.remix.enable` Enable Remix: {"clip":id?}
- `clip.remix.properties` Remix Properties…: {"clip":id?,"duration":ticks?,"seconds":f64?,"frame":n?,"timecode":str?,"segments":0..100?,"variations":0..100?}
- `clip.remix.revert` Revert Remix: {"clip":id?}
- `clip.rename` Rename…: {"clip":id?,"item":id?,"name":str}
- `clip.replaceFromBin` From Bin: {"clips":[id]?,"item":id?,"keepSourceIn":bool=false}
- `clip.replaceFromSource` From Source Monitor: {"clips":[id]?}
- `clip.replaceFromSourceMatchFrame` From Source Monitor, Match Frame: {"clips":[id]?}
- `clip.restoreCaptionsFromSource` Restore Captions from Source Clip: {}
- `clip.revealInProject` Reveal in Project: {"clip":id?}
- `clip.scaleToFrameSize` Scale to Frame Size: {}
- `clip.sceneEditDetection` Scene Edit Detection…: {"clips":[id]?,"sensitivity":0..100=50,"minShotFrames":n=6,"applyCuts":bool=true,"createSubclips":bool=false,"generateMarkers":bool=false,"wait":bool=false}
- `clip.setPosterFrame` Set Poster Frame: {"item":id?,"time":ticks?}
- `clip.setTimeInterpolation` Set Time Interpolation: {"clips":[id]?,"mode":"frameSampling|frameBlending|opticalFlow"}
- `clip.sourceSettings` Source Settings…: {"item":id?}
- `clip.speedDuration` Speed/Duration…: {"clips":[id]?,"speed":percent=100,"reverse":bool,"ripple":bool,"interpolation":"frameSampling|frameBlending|opticalFlow"?}
- `clip.synchronize` Synchronize…: {"method":"in|out|timecode|marker|audio","clips":[id]?,"reference":clip?,"track":"V1"|"A1"?,"ignoreHours":bool?,"marker":str?,"offset":frames?}
- `clip.timeInterpolation.frameBlending` Frame Blending: {"clips":[id]?}
- `clip.timeInterpolation.frameSampling` Frame Sampling: {"clips":[id]?}
- `clip.timeInterpolation.opticalFlow` Optical Flow: {"clips":[id]?}
- `clip.ungroup` Ungroup: {}
- `clip.updateMetadata` [file] Update Metadata…: {"items":[id]?}
- `clip.volumeDown` Decrease Clip Volume: {}
- `clip.volumeDownMany` Decrease Clip Volume Many: {}
- `clip.volumeUp` Increase Clip Volume: {}
- `clip.volumeUpMany` Increase Clip Volume Many: {}

## clipMixer

- `clipMixer.release` Release Clip Mixer Control: {"track":"A1"|id,"lane":"volume|pan","time":ticks?}
- `clipMixer.set` Audio Clip Mixer Adjust: {"clip":id,"effect":"volume"|"panner","param":"level"|"balance","value":f64,"keyframe":bool?,"time":ticks?,"begin":bool?}
- `clipMixer.setMode` Audio Clip Mixer Automation Mode: {"track":"A1"|id,"mode":"Off|Read|Latch|Touch|Write"}
- `clipMixer.touch` Touch Clip Mixer Control: {"track":"A1"|id,"lane":"volume|pan","value":f64,"time":ticks?}

## color

- `color.spaces` List Colour Spaces: {}

## command

- `command.list` List Commands: {}

## edit

- `edit.clear` Clear: {"clips":[id]?}
- `edit.consolidateDuplicates` Consolidate Duplicates: {}
- `edit.copy` Copy: {}
- `edit.cut` Cut: {}
- `edit.deselectAll` Deselect All: {}
- `edit.duplicate` Duplicate: {}
- `edit.editOriginal` Edit Original: {"items":[id]?}
- `edit.find` Find…: {"scope":"project|timeline"?,"column":str?,"operator":"contains|matches|beginsWith|endsWith|doesNotContain"?,"text":str?,"rows":[{"column":str,"operator":str,"text":str}]?,"matchAll":bool=true,"caseSensitive":bool=false,"in":"all|clips|markers"?}
- `edit.findNext` Find Next: {}
- `edit.label` Label: {"label":"Violet|Iris|…"}
- `edit.label.blue` Blue: {"items":[id]?,"clips":[id]?}
- `edit.label.brown` Brown: {"items":[id]?,"clips":[id]?}
- `edit.label.caribbean` Caribbean: {"items":[id]?,"clips":[id]?}
- `edit.label.cerulean` Cerulean: {"items":[id]?,"clips":[id]?}
- `edit.label.forest` Forest: {"items":[id]?,"clips":[id]?}
- `edit.label.green` Green: {"items":[id]?,"clips":[id]?}
- `edit.label.iris` Iris: {"items":[id]?,"clips":[id]?}
- `edit.label.lavender` Lavender: {"items":[id]?,"clips":[id]?}
- `edit.label.magenta` Magenta: {"items":[id]?,"clips":[id]?}
- `edit.label.mango` Mango: {"items":[id]?,"clips":[id]?}
- `edit.label.purple` Purple: {"items":[id]?,"clips":[id]?}
- `edit.label.rose` Rose: {"items":[id]?,"clips":[id]?}
- `edit.label.tan` Tan: {"items":[id]?,"clips":[id]?}
- `edit.label.teal` Teal: {"items":[id]?,"clips":[id]?}
- `edit.label.violet` Violet: {"items":[id]?,"clips":[id]?}
- `edit.label.yellow` Yellow: {"items":[id]?,"clips":[id]?}
- `edit.paste` Paste: {}
- `edit.pasteAttributes` Paste Attributes…: {"clips":[id]?,"motion":bool=true,"opacity":bool=true,"timeRemapping":bool=true,"volume":bool=true,"channelVolume":bool=true,"panner":bool=true,"effects":bool|[effectId]=true,"scaleTimes":bool=true}
- `edit.pasteInsert` Paste Insert: {}
- `edit.redo` Redo: {}
- `edit.removeAttributes` Remove Attributes…: {"clips":[id]?,"motion":bool=true,"opacity":bool=true,"timeRemapping":bool=true,"volume":bool=true,"channelVolume":bool=true,"panner":bool=true,"effects":bool|[effectId]=true}
- `edit.removeUnused` Remove Unused: {}
- `edit.rippleDelete` Ripple Delete: {"clips":[id]?}
- `edit.selectAll` Select All: {}
- `edit.selectAllMatching` Select All Matching: {"clips":[id]?}
- `edit.selectLabelGroup` Select Label Group: {}
- `edit.undo` Undo: {}

## effects

- `effects.addKeyframe` Add/Remove Keyframe: {"clip":id,"effect":str|index,"param":str,"mask":n?,"time":ticks?}
- `effects.apply` Apply Effect: {"clips":[id]?,"effect":"gaussian_blur|Gaussian Blur|…"}
- `effects.deleteKeyframe` Delete Keyframe: {"clip":id,"effect":str|index,"param":str,"mediaTime":ticks}
- `effects.list` List Effects: {"kind":"Video"|"Audio"|"VideoTransition"|"AudioTransition"?,"folder":"Video Transitions/Wipe"?,"detail":bool?}
- `effects.moveKeyframe` Move Keyframe: {"clip":id,"effect":str|index,"param":str,"mediaTime":ticks,"to":ticks}
- `effects.remove` Remove Effect: {"clip":id,"index":n}
- `effects.reset` Reset Effect: {"clip":id,"index":n}
- `effects.setDefaultTransition` Set Selected as Default Transition: {"effect":"constant_power|constant_gain|exponential_fade|<video transition>"}
- `effects.setInterpolation` Keyframe Interpolation: {"clip":id,"effect":str|index,"param":str,"mediaTime":ticks,"interpolation":"linear|bezier|autoBezier|continuousBezier|hold|easeIn|easeOut"}
- `effects.setKeyframe` Edit Keyframe: {"clip":id,"effect":str|index,"param":str,"mediaTime":ticks,"value":any?,"inInfluence":0..1?,"outInfluence":0..1?}
- `effects.setParam` Set Effect Parameter: {"clip":id,"effect":"motion"|index,"param":str,"mask":n?,"value":num|[x,y]|"#rrggbb"|bool|path,"time":ticks?,"merge":bool?,"begin":bool?}
- `effects.toggleAnimation` Toggle Animation: {"clip":id,"effect":str|index,"param":str,"mask":n?}
- `effects.toggleEnabled` Toggle Effect: {"clip":id,"index":n}

## essentialSound

- `essentialSound.applyPreset` Apply Sound Preset: {"clips":[id]?,"preset":str,"type":"dialogue|music|sfx|ambience"?}
- `essentialSound.autoMatch` Auto-Match Loudness: {"clips":[id]?,"target":lufs?}
- `essentialSound.clearType` Clear Audio Type: {"clips":[id]?}
- `essentialSound.deletePreset` [host] Delete Sound Preset: {"name":str,"type":"dialogue|music|sfx|ambience"?}
- `essentialSound.generateDucking` Generate Ducking Keyframes: {"clips":[id]?}
- `essentialSound.inspect` Inspect Essential Sound: {"clips":[id]?}
- `essentialSound.savePreset` [host] Save Sound Preset: {"clips":[id]?,"name":str}
- `essentialSound.set` Essential Sound Setting: {"clips":[id]?,"key":"repair.noise.on|repair.noise.amount|repair.humHz|clarity.eqPreset|creative.reverbPreset|ducking.reduceDb|volume.levelDb|mute|…","value":any,"values":{key:value}?,"begin":bool?}
- `essentialSound.setType` Set Audio Type: {"clips":[id]?,"type":"dialogue|music|sfx|ambience"}

## events

- `events.clear` Clear All Events: {}
- `events.list` List Events: {"level":"info|warning|error"?,"since":id?}

## export

- `export.formats` List Export Formats: {}
- `export.presets.delete` [file] Delete Export Preset: {"name":str}
- `export.presets.export` [file] Export Export Presets: {"path":str,"names":[str]?}
- `export.presets.favorite` [file] Favorite Export Preset: {"name":str,"favorite":bool?}
- `export.presets.get` Get Export Preset: {"name":str}
- `export.presets.import` [file] Import Export Presets: {"path":str}
- `export.presets.list` List Export Presets: {"query":str?,"category":str?,"format":str?,"favorites":bool?}
- `export.presets.save` [file] Save Export Preset: {"name":str,"from":str?,"settings":ExportSettings?,"category":str?,"description":str?,"overwrite":bool=true, …flat overrides}
- `export.queue.add` [file] Send to Export Queue: {…settings params,"path":str|dir?,"sequences":[id]?,"ranges":[range]?,"start":bool?,"wait":bool?}
- `export.queue.cancel` Cancel Queued Export: {"id":id?,"wait":bool?}
- `export.queue.clear` Clear Finished Exports: {"all":bool=false}
- `export.queue.list` [file] List Export Queue: {}
- `export.queue.move` Reorder Queued Export: {"id":id,"to":index?,"by":int?}
- `export.queue.remove` Remove Queued Export: {"id":id}
- `export.queue.retry` [file] Retry Queued Export: {"id":id,"start":bool?,"wait":bool?}
- `export.queue.start` [file] Start Export Queue: {"wait":bool=false}
- `export.queue.stop` Stop Export Queue: {}
- `export.quick` [file] Quick Export: {"preset":str?,"path":str?,"wait":bool?, …settings params}
- `export.resolve` [file] Resolve Export Settings: {…settings params, "path":str?}

## file

- `file.autoSaveNow` [file] Auto Save Now: {}
- `file.autoSaveStatus` [file] Auto Save Status: {}
- `file.close` Close: {"item":id?}
- `file.closeAllOtherProjects` Close All Other Projects: {}
- `file.closeAllProjects` Close All Projects: {"force":bool?}
- `file.closeProject` Close Project: {"force":bool?}
- `file.discardRecovery` [file] Discard Unsaved Changes: {"id":str?,"all":bool?}
- `file.exportAaf` [file] AAF…: {"path":str,"sequence":id?,"mixdownVideo":bool?,"mixdownFormat":"mov|mxf"?,"breakoutToMono":bool?,"audio":"embedded|separate|linked"?,"audioFormat":"wav|aiff|mxf"?,"sampleRate":int?,"bitDepth":"16|24"?,"trimAudio":bool?,"handles":frames?,"renderAudioEffects":bool?,"smallSectors":bool?}
- `file.exportAle` [file] Avid Log Exchange…: {"path":str,"items":[id]?}
- `file.exportEdl` [file] EDL…: {"path":str}
- `file.exportFcp7Xml` [file] Final Cut Pro XML…: {"path":str}
- `file.exportFcpxml` [file] FCPXML…: {"path":str}
- `file.exportFrame` [file] Export Frame: {"path":str?,"format":"png|tiff|bmp"?,"import":bool?}
- `file.exportGraphicsTemplate` [file] Motion Graphics Template…: as graphics.template.export
- `file.exportInterchange` [file] Export Interchange: {"format":"edl|xml|fcpxml|otio|aaf|omf"=xml,"path":str,"sequence":id?}
- `file.exportMedia` [file] Media…: {"path":str,"preset":str?,"settings":ExportSettings?,"format":"h264|hevc|prores|dnxhr|apv|mjpeg|mxf-op1a|mxf-opatom|png|tiff|bmp|gif|wav|aiff"?,"width":u32?,"height":u32?,"fps":f64?,"bitrateKbps":u32?,"bitrateMode":"cbr|vbr1Pass|vbr2Pass"?,"hardwareEncoding":"off|auto"?,"scale":f32=1,"audio":bool=true,"quality":0..100,"burnCaptions":bool=false,"captionSidecar":"srt|vtt"?,"loudnessLufs":f64?,"proresProfile":"proxy|lt|standard|hq"?,"dnxProfile":"lb|sq|hq|hqx"?,"apvProfile":"422-10|422-12|444-10|444-12"?,"mxfVideoCodec":"dnxhr|proRes|h264"?,"sequence":id?,"range":"entire|inOut|workArea|custom"?,"startSeconds":f64?,"endSeconds":f64?,"wait":bool=false}
- `file.exportOmf` [file] OMF…: {"path":str,"sequence":id?,"title":str?,"audio":"embedded|separate"?,"audioFormat":"wav|aiff"?,"sampleRate":int?,"bitDepth":"16|24"?,"trimAudio":bool?,"handles":frames?,"renderAudioEffects":bool?,"breakoutToMono":bool?}
- `file.exportOtio` [file] OpenTimelineIO…: {"path":str}
- `file.exportSelectionProject` [file] Selection as FilmCraft Project…: {"path":str,"items":[id]?}
- `file.import` [file] Import…: {"paths":[str],"bin":binId?,"imageSequence":bool?}
- `file.importAaf` [file] Import AAF…: {"path":str}
- `file.importDemoFootage` Demo Footage: {"scene":"OceanSunset|Aurora|CityNight|Dunes|Plasma|Forest"}
- `file.importFromMediaBrowser` [file] Import from Media Browser: {"paths":[str]?,"imageSequence":bool?}
- `file.importImageSequence` [file] Import Image Sequence…: {"path":str,"bin":binId?}
- `file.listAutoSaves` [file] Browse Auto-Saves: {}
- `file.mediaProperties` Selection…: {"items":[id]?}
- `file.mediaPropertiesFile` [file] File…: {"path":str}
- `file.newAdjustmentLayer` Adjustment Layer…: {"seconds":f64=5}
- `file.newBarsAndTone` Bars and Tone…: {"seconds":f64=10}
- `file.newBin` Bin: {"name":str,"parent":binId?}
- `file.newBinFromSelection` Bin From Selection: {"items":[id]?,"name":str?}
- `file.newBlackVideo` Black Video…: {"seconds":f64}
- `file.newColorMatte` Color Matte…: {"color":"#rrggbb","seconds":f64}
- `file.newCountingLeader` Universal Counting Leader…: {}
- `file.newOfflineFile` Offline File…: {"name":str?,"fileName":str?,"tapeName":str?,"video":bool=true,"audio":bool=true,"width":u32?,"height":u32?,"fps":f64?,"sampleRate":u32=48000,"channels":u32=2,"timecode":"HH:MM:SS:FF"?,"seconds":f64=10,"description":str?}
- `file.newProject` Project…: {"name":str}
- `file.newProjectFromTemplate` [file] New Project from Template: {"template":name|path,"name":str?}
- `file.newSearchBin` Search Bin: {"name":str?,"column":str?,"operator":str?,"text":str?,"rows":[..]?,"matchAll":bool?,"caseSensitive":bool?}
- `file.newSequence` Sequence…: {"name":str,"width":u32=1920,"height":u32=1080,"fps":f64=23.976,"sampleRate":u32=48000,"video":n=3,"audio":n=3,"mix":"Stereo|Mono|5.1|Adaptive"?,"trackType":"Standard|Mono|5.1|Adaptive"?,"fromItem":itemId?}
- `file.newSequenceFromClip` Sequence From Clip: {"items":[id]?}
- `file.newTransparentVideo` Transparent Video…: {}
- `file.open` [file] Open Project…: {"path":str}
- `file.openDemoProject` Demo Project: {}
- `file.projectManager` [file] Project Manager…: {"destination":str,"mode":"collect|consolidate","sequences":[id]?,"excludeUnused":bool=true,"handles":frames=30,"preset":"prores_lt|prores_hq|h264|…","includeProxies":bool,"includePreviews":bool=false,"projectName":str?,"dryRun":bool=false,"overwrite":bool=false,"wait":bool=false}
- `file.projectSettings.general` General…: {"renderer":"gpu|software"?,"videoDisplay":"timecode|feet35|feet16|frames"?,"audioDisplay":"samples|milliseconds"?,"captureFormat":"DV|HDV"?,"titleSafe":[h,v]?,"actionSafe":[h,v]?}
- `file.projectSettings.scratchDisks` [file] Scratch Disks…: {"captured":path|null?,"videoPreviews":path|null?,"audioPreviews":path|null?,"autoSave":path|null?}
- `file.recover` [file] Recover Unsaved Changes…: {"id":str?}
- `file.recoveryList` [file] List Recoverable Sessions: {}
- `file.replaceFonts` Replace Fonts in Projects…: {"from":family|{"family","style"?},"to":family|{"family","style"?},"toStyle":str?}
- `file.revert` [file] Revert: {}
- `file.save` [file] Save: {"path":str?}
- `file.saveAll` [file] Save All: {}
- `file.saveAs` [file] Save As…: {"path":str}
- `file.saveAsTemplate` [file] Save as Template…: {"name":str?,"path":str?}
- `file.saveCopy` [file] Save a Copy…: {"path":str}
- `file.templates` [file] List Project Templates: {}

## fonts

- `fonts.list` [host] List Fonts: {"system":bool=true (scan the system font folders)}

## graphics

- `graphics.align` Align Layers: {"clip":id?,"layers":[n]?,"align":"left|hcenter|right|top|vcenter|bottom","to":"frame|group|selection"="frame" (frame: each layer; group: the layers' union; one layer always aligns to the frame)}
- `graphics.alignFrame.bottom` Bottom: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignFrame.hcenter` Center Horizontally: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignFrame.left` Left: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignFrame.right` Right: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignFrame.top` Top: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignFrame.vcenter` Center Vertically: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignGroup.bottom` Bottom: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignGroup.hcenter` Center Horizontally: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignGroup.left` Left: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignGroup.right` Right: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignGroup.top` Top: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignGroup.vcenter` Center Vertically: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignSelection.bottom` Bottom: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignSelection.hcenter` Center Horizontally: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignSelection.left` Left: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignSelection.right` Right: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignSelection.top` Top: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignSelection.vcenter` Center Vertically: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.alignTextCenter` Center align text: {}
- `graphics.alignTextLeft` Left align text: {}
- `graphics.alignTextRight` Right align text: {}
- `graphics.arrangeLayer` Arrange Graphic Layer: {"clip":id?,"layer":n?,"to":"front|back|forward|backward"|index}
- `graphics.bringForward` Bring Forward: {"clip":id?,"layer":n?}
- `graphics.bringToFront` Bring to Front: {"clip":id?,"layer":n?}
- `graphics.deleteLayer` Delete Graphic Layer: {"clip":id?,"layer":n?}
- `graphics.distribute` Distribute Layers: {"clip":id?,"layers":[n] (3 or more)?,"axis":"horizontal|vertical","space":bool=false (equal gaps instead of equal centre spacing)}
- `graphics.distributeHorizontally` Distribute Horizontally: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.distributeSpaceHorizontally` Distribute Space Horizontally: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.distributeSpaceVertically` Distribute Space Vertically: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.distributeVertically` Distribute Vertically: {"clip":id?,"layers":[n]? (default: the selected layers)}
- `graphics.fontSizeDown` Decrease Font Size by One Unit: {}
- `graphics.fontSizeDown5` Decrease Font Size by Five Units: {}
- `graphics.fontSizeUp` Increase Font Size by One Unit: {}
- `graphics.fontSizeUp5` Increase Font Size by Five Units: {}
- `graphics.fonts.used` List Fonts Used: {}
- `graphics.leadingDown` Decrease Leading by One Unit: {}
- `graphics.leadingDown5` Decrease Leading by Five Units: {}
- `graphics.leadingUp` Increase Leading by One Unit: {}
- `graphics.leadingUp5` Increase Leading by Five Units: {}
- `graphics.list` List Graphic Layers: {"clip":id?}
- `graphics.newEllipse` Ellipse: {"position":[x,y]?,"size":[w,h]=[400,200],"clip":id?}
- `graphics.newFromFile` [file] From file…: {"path":str,"time":ticks?,"track":index?} (imports the image or video and places it above the clips at the playhead)
- `graphics.newPolygon` Polygon: {"position":[x,y]?,"size":[w,h]=[300,300],"sides":n=6,"clip":id?}
- `graphics.newRectangle` Rectangle: {"position":[x,y]?,"size":[w,h]=[400,200],"clip":id?}
- `graphics.newShape` Shape: {"shape":"rectangle|ellipse|polygon|path","position":[x,y]?,"size":[w,h]=[400,200],"points":[[x,y],…]?,"clip":id?,"seconds":f64=5}
- `graphics.newText` Text: {"text":str="New Text","position":[x,y]? (point text: its alignment point on the first baseline; with `box`: the box's top-left corner),"box":[w,h]? (paragraph text wrapped in a box this size),"clip":id?,"newClip":bool?,"vertical":bool=false,"size":px=100,"font":str?,"fontStyle":str?,"seconds":f64=5,"track":index?,"time":ticks?}
- `graphics.newVerticalText` Vertical Text: {"text":str="New Text","position":[x,y]?,"clip":id?,"size":px=100,"seconds":f64=5,"time":ticks?}
- `graphics.nudgeDown` Nudge Selected Object down by one: {}
- `graphics.nudgeDown5` Nudge Selected Object down by five: {}
- `graphics.nudgeLeft` Nudge Selected Object to left by one: {}
- `graphics.nudgeLeft5` Nudge Selected Object to left by five: {}
- `graphics.nudgeRight` Nudge Selected Object to right by one: {}
- `graphics.nudgeRight5` Nudge Selected Object to right by five: {}
- `graphics.nudgeUp` Nudge Selected Object up by one: {}
- `graphics.nudgeUp5` Nudge Selected Object up by five: {}
- `graphics.pin` Responsive Design - Position: {"clip":id?,"layer":n|name?,"to":"frame"|"none"|layer index|layer name,"edges":["left","top","right","bottom"]|"all"="all"}
- `graphics.resetAllParameters` Reset All Parameters: {"clip":id?,"layers":[n]? (default: the selected layers, else all)}
- `graphics.resetDuration` Reset Duration: {"clip":id?,"seconds":f64=5 (the default graphic duration; limited by the next clip on the track)}
- `graphics.selectLayer` Select Graphic Layer: {"clip":id?,"layers":[n]}
- `graphics.selectNextGraphic` Select Next Graphic: {}
- `graphics.selectNextLayer` Select Next Layer: {}
- `graphics.selectPreviousGraphic` Select Previous Graphic: {}
- `graphics.selectPreviousLayer` Select Previous Layer: {}
- `graphics.sendBackward` Send Backward: {"clip":id?,"layer":n?}
- `graphics.sendToBack` Send to Back: {"clip":id?,"layer":n?}
- `graphics.set` Set Graphic Properties: {"clip":id?,"layer":n|name?,"props":{"font":"Inter","font_style":"Bold","size":120,"align":"center","tracking":50,"leading":0,"fill_color":"#ffcc00","stroke":true,"stroke_width":6,"background":true,"shadow":true,"position":[x,y],"scale":100,"rotation":0,"opacity":100,…},"time":ticks?}
- `graphics.setCharStyle` Character Style: {"clip":id?,"layer":n|name?,"start":char=0,"end":char=len,"style":{"font":str,"fontStyle":str,"size":px,"color":"#rrggbb","bold":bool,"italic":bool,"underline":bool,"tracking":n,"baselineShift":px,"caps":"normal|all caps|small caps"},"clear":bool? (remove styles from the range)}
- `graphics.setResponsiveTime` Responsive Design - Time: {"clip":id?,"introFrames":n?,"outroFrames":n? (or introSeconds / outroSeconds, or ticks as intro / outro)}
- `graphics.setRoll` Roll/Crawl Options: {"clip":id?,"mode":"off|roll|crawlLeft|crawlRight"?,"startOffScreen":bool?,"endOffScreen":bool?,"prerollFrames":n?,"easeInFrames":n?,"easeOutFrames":n?,"postrollFrames":n? (or …Seconds, or ticks as preroll/easeIn/easeOut/postroll)}
- `graphics.setText` Edit Text: {"clip":id?,"layer":n|name?,"text":str,"merge":bool? (coalesce with the previous Edit Text undo step)}
- `graphics.setTextType` Text Layer Type: {"clip":id?,"layer":n|name?,"type":"point|paragraph" (point: no box, handles scale the text; paragraph: the text wraps in a box that handles resize)}
- `graphics.template.apply` [file] Apply Graphics Template: {"template":id|name|path,"values":{control id or name: value}?,"time":ticks?,"track":index?}
- `graphics.template.controls` List Template Properties: {"clip":id?}
- `graphics.template.export` [file] Export As Motion Graphics Template…: {"clip":id?,"name":str,"category":str="My Templates","description":str?,"controls":[{"layer":n|name,"param":"text|fill_color|size|font|position|enabled|…","name":str?,"kind":"text|color|slider|checkbox|font|position"?,"min":f64?,"max":f64?,"id":str?}]? (default: each text layer's text),"path":str? (default: the user templates folder),"embedFonts":bool=false,"fontLicense":str?}
- `graphics.template.install` [file] Install Motion Graphics Template…: {"path":str (a .fcgt file; Adobe .mogrt files are refused)}
- `graphics.template.list` [file] List Graphics Templates: {"query":str? (search names, categories, descriptions, tags),"category":str?,"source":"builtin|user"?}
- `graphics.template.remove` [file] Remove Graphics Template: {"template":id|name (a user template)}
- `graphics.template.set` Set Template Property: {"clip":id?,"control":id|name,"value":any} or {"clip":id?,"values":{control: value}}
- `graphics.template.thumbnail` [file] Graphics Template Thumbnail: {"template":id|name|path,"width":px=320,"path":str? (write a PNG there; else returned as pngBase64)}
- `graphics.upgradeCaption` Upgrade Caption to Graphic: {"captions":[id]? (default: the selected captions, else the one under the playhead)}
- `graphics.upgradeToSourceGraphic` Upgrade to Source Graphic: {"clip":id?}

## help

- `help.systemReport` [host] System Compatibility Report: {}

## history

- `history.list` List History: {}

## jobs

- `jobs.cancel` Cancel Job: {"job":id}
- `jobs.list` List Jobs: {}

## lumetri

- `lumetri.applyMatch` Apply Match: {"clip":id?,"referenceTime":ticks?|"referenceFrame":n?|"referenceTimecode":str?,"faceDetection":bool=true}
- `lumetri.applyPreset` Apply Lumetri Preset: {"name":str,"clips":[id]?}
- `lumetri.presetThumbnails` [file] Lumetri Preset Thumbnails: {"folder":str?,"names":[str]?,"width":n=160,"columns":n=4,"path":str?}
- `lumetri.presets` List Lumetri Presets: {"folder":str?}
- `lumetri.setInputLut` [file] Set Input LUT: {"clip":id?,"lut":"lib:<id>"|"builtin:<id>"|""?,"path":str?}
- `lumetri.setLook` [file] Set Creative Look: {"clip":id?,"lut":"lib:<id>"|"builtin:<id>"|""?,"path":str?}
- `lumetri.setSection` Toggle Lumetri Section: {"clip":id?,"section":"basic"|"creative"|"curves"|"wheels"|"hsl"|"vignette","on":bool?}

## lut

- `lut.export` [file] Export LUT: {"lut":"lib:<id>"|"builtin:<id>","path":str,"format":"cube"|"3dl"?}
- `lut.import` [file] Import LUT…: {"path":str,"name":str?}
- `lut.list` List LUTs: {}
- `lut.remove` Remove LUT: {"id":str}

## markers

- `markers.add` Add Marker: {"time":ticks?,"name":str?,"comment":str?,"color":label?,"durationFrames":i64?}
- `markers.addChapter` Add Chapter Marker…: {"time":ticks?,"name":str?,"comment":str?,"color":label?}
- `markers.addFlashCue` Add Flash Cue Marker…: {"time":ticks?,"name":str?,"comment":str?,"color":label?}
- `markers.addRange` Add Range Marker: {"time":ticks?,"durationFrames":i64=1s,"duration":ticks?,"name":str?,"comment":str?,"color":label?}
- `markers.addRangeInOut` Add Range Marker to In and Out: {"name":str?,"comment":str?,"color":label?}
- `markers.clearAll` Clear Markers: {}
- `markers.clearCurrent` Clear Selected Marker: {}
- `markers.clearIn` Clear In: {}
- `markers.clearInOut` Clear In and Out: {}
- `markers.clearOut` Clear Out: {}
- `markers.copyPasteIncludesSequenceMarkers` Copy Paste Includes Sequence Markers: {"on":bool?}
- `markers.edit` Edit Marker…: {"marker":id,"name":str?,"comment":str?,"color":label?,"durationFrames":i64?}
- `markers.filterColors` [host] Marker Colour Filter: {"hidden":[label]}|{"color":label,"visible":bool?}
- `markers.goNext` Go to Next Marker: {}
- `markers.goPrev` Go to Previous Marker: {}
- `markers.goToIn` Go to In: {}
- `markers.goToOut` Go to Out: {}
- `markers.goToSplitAudioIn` Audio In: {"target":"program|source"}
- `markers.goToSplitAudioOut` Audio Out: {"target":"program|source"}
- `markers.goToSplitVideoIn` Video In: {"target":"program|source"}
- `markers.goToSplitVideoOut` Video Out: {"target":"program|source"}
- `markers.markClip` Mark Clip: {}
- `markers.markIn` Mark In: {"time":ticks?,"target":"program|source"}
- `markers.markOut` Mark Out: {"time":ticks?,"target":"program|source"}
- `markers.markSelection` Mark Selection: {}
- `markers.markSplitAudioIn` Audio In: {"time":ticks?,"target":"program|source"}
- `markers.markSplitAudioOut` Audio Out: {"time":ticks?,"target":"program|source"}
- `markers.markSplitVideoIn` Video In: {"time":ticks?,"target":"program|source"}
- `markers.markSplitVideoOut` Video Out: {"time":ticks?,"target":"program|source"}
- `markers.rippleSequenceMarkers` Ripple Sequence Markers: {"on":bool?}
- `markers.showAllMarkerColors` [host] Show All Marker Colors: {}

## masks

- `masks.add` Create Mask: {"clip":id?,"effect":index|id?="opacity","shape":"ellipse"|"polygon"|"bezier","path":[[x,y]…]|{vertices}?,"center":[x,y]?,"size":[w,h]?}
- `masks.addVertex` Add Mask Vertex: {"clip":id?,"effect":index|id?,"mask":n?,"after":n,"at":[x,y]}
- `masks.list` List Masks: {"clip":id?,"time":ticks?}
- `masks.moveVertex` Move Mask Vertex: {"clip":id?,"effect":index|id?,"mask":n?,"vertex":n,"handle":"point"|"in"|"out"?,"to":[x,y]?|"delta":[dx,dy]?,"breakHandles":bool?,"merge":key?}
- `masks.remove` Delete Mask: {"clip":id?,"effect":index|id?,"mask":n?}
- `masks.removeVertex` Delete Mask Vertex: {"clip":id?,"effect":index|id?,"mask":n?,"vertex":n}
- `masks.select` Select Mask: {"clip":id,"effect":index|id,"mask":n}|{"none":true}
- `masks.set` Change Mask: {"clip":id?,"effect":index|id?,"mask":n?,"name":str?,"inverted":bool?,"mode":"none|add|subtract|intersect|lighten|darken|difference"?,"trackMethod":"position|positionRotation|positionScaleRotation"?,"feather":px?,"opacity":pct?,"expansion":px?,"path":path?,"time":ticks?,"merge":key?}
- `masks.toggleVertexSmooth` Convert Mask Vertex: {"clip":id?,"effect":index|id?,"mask":n?,"vertex":n}
- `masks.track` Track Selected Mask: {"clip":id?,"effect":index|id?,"mask":n?,"direction":"forward"|"backward"?,"frames":n?,"method":"position|positionRotation|positionScaleRotation"?,"wait":bool?}
- `masks.translate` Move Mask: {"clip":id?,"effect":index|id?,"mask":n?,"delta":[dx,dy],"merge":key?}

## media

- `media.attachProxies` [file] Attach Proxies…: {"item":id,"path":str}|{"items":[id],"paths":[str]},"force":bool=false
- `media.autoRelink` [file] Relink Moved Media: {"from":str,"to":str}|{"folder":str} + match options
- `media.colorInfo` Media Colour Info: {"item":id}
- `media.createProxies` [file] Create Proxies…: {"items":[id]?,"preset":"prores_proxy_quarter|prores_proxy_half|prores_lt_half|h264_quarter|h264_half","destination":str?,"attach":bool=true,"wait":bool=false}
- `media.detachProxies` Detach Proxies: {"items":[id]?}
- `media.findMissing` [file] Find Missing Media: {}
- `media.linkMedia` [host] Link Media…: {} (opens the Link Media dialog; agents use media.relink / media.autoRelink)
- `media.makeOffline` [file] Make Offline…: {"items":[id]?,"deleteFiles":bool=false}
- `media.offlineAll` [host] Offline All: {} (leave every missing file offline and close the Link Media dialog)
- `media.proxyPresets` Proxy Presets: {}
- `media.reconnectFullRes` [file] Reconnect Full Resolution Media…: {"item":id,"path":str}
- `media.relink` [file] Relink Media: {"item":id,"path":str,"force":bool=false,"relinkOthers":bool=true,"alignTimecode":bool=false,"match":{"fileName":bool,"extension":bool,"clipId":bool,"duration":bool,"mediaStart":bool,"metadata":bool}?}
- `media.replaceFootage` [file] Replace Footage…: {"item":id,"path":str}
- `media.search` [file] Search for Media: {"folder":str,"item":id?,"exactName":bool=true}
- `media.status` [file] Media Status: {"item":id?}
- `media.toggleProxies` [host] Toggle Proxies: {"enabled":bool?}

## mediaBrowser

- `mediaBrowser.clearRecent` [host] Clear Recent Directories: {}
- `mediaBrowser.favorite` [host] Add to Favorites: {"path":str?,"remove":bool?}
- `mediaBrowser.import` [file] Import: {"paths":[str]?,"bin":binId?,"imageSequence":bool?}
- `mediaBrowser.list` [file] List Directory: {"path":str?,"fileTypes":str?}
- `mediaBrowser.navigate` [host] Go to Directory: {"path":str} | {"back":true} | {"forward":true} | {"up":true}
- `mediaBrowser.openInSource` [file] Open In Source Monitor: {"path":str?}
- `mediaBrowser.probe` [file] Media File Properties: {"path":str}
- `mediaBrowser.roots` [host] Media Browser Locations: {}
- `mediaBrowser.select` [file] Select Files: {"paths":[str]}
- `mediaBrowser.settings` [host] Media Browser Settings: {"fileTypes":"all|video|audio|image|project|caption|<ext>"?,"view":"list|thumbnails"?,"columns":[str]?,"importAsImageSequence":bool?,"hoverScrub":bool?,"thumbnailSize":f32?}

## mediaCache

- `mediaCache.clean` [file] Delete Media Cache Files: {"all":bool?}
- `mediaCache.info` [file] Media Cache Info: {}

## metadata

- `metadata.get` Get Metadata: {"item":id?,"clip":id?}
- `metadata.set` Edit Metadata: {"item":id?,"clip":id?,"field":"Name|Label|Description|Scene|Shot|Log Note|Comment|Tape Name|Client|Camera Angle|…","value":str} | {"item":id?,"fields":{name:str}}

## mixer

- `mixer.addInsert` Add Track Effect: {"strip":"A1"|"S1"|"Mix"|id,"effect":str,"slot":n?,"postFader":bool?}
- `mixer.addSend` Add Send: {"strip":"A1"|"S1"|id,"target":"S1"|id,"levelDb":f64?,"preFader":bool?,"pan":f64?}
- `mixer.addSubmix` Add Audio Submix Track: {"name":str?,"channels":"Mono|Stereo|5.1"?}
- `mixer.clearLane` Clear Track Keyframes: {"strip":"A1"|"S1"|"Mix"|id,"lane":str?}
- `mixer.deleteKeyframe` Delete Track Keyframe: {"strip":"A1"|"S1"|"Mix"|id,"lane":str?,"time":ticks}
- `mixer.deleteSubmix` Delete Submix Track: {"strip":"S1"|id}
- `mixer.inspect` Inspect Audio Track Mixer: {"time":ticks?}
- `mixer.moveKeyframe` Move Track Keyframe: {"strip":"A1"|"S1"|"Mix"|id,"lane":str?,"time":ticks,"newTime":ticks?,"value":f64?}
- `mixer.recordStart` Start Automation Pass: {"time":ticks?}
- `mixer.recordStop` Write Automation: {"time":ticks?}
- `mixer.release` Release Mixer Control: {"strip":"A1"|"S1"|"Mix"|id,"lane":str?,"time":ticks?}
- `mixer.removeInsert` Remove Track Effect: {"strip":"A1"|"S1"|"Mix"|id,"slot":n}
- `mixer.removeSend` Remove Send: {"strip":"A1"|"S1"|id,"send":n}
- `mixer.setInsert` Track Effect Settings: {"strip":"A1"|"S1"|"Mix"|id,"slot":n,"enabled":bool?,"postFader":bool?,"params":{id:value}?}
- `mixer.setKeyframe` Add Track Keyframe: {"strip":"A1"|"S1"|"Mix"|id,"lane":str?,"time":ticks?,"value":f64}
- `mixer.setSend` Send Settings: {"strip":"A1"|"S1"|id,"send":n,"levelDb":f64?,"pan":f64?,"preFader":bool?,"muted":bool?,"target":"S1"?}
- `mixer.setStrip` Track Mixer Settings: {"strip":"A1"|"S1"|"Mix"|id,"name":str?,"volumeDb":f64?,"pan":f64?,"muted":bool?,"solo":bool?,"recordArm":bool?,"soloSafe":bool?,"mode":"Off|Read|Latch|Touch|Write"?,"output":"Mix"|"S1"?,"inputMap":"Stereo|Left|Right|Swap|Mono"?,"channels":"Mono|Stereo|5.1"?}
- `mixer.setValue` Set Mixer Control: {"strip":"A1"|"S1"|"Mix"|id,"lane":"volume|pan|mute|send.<i>.level|fx.<slot>.<param>","value":f64,"time":ticks?}
- `mixer.touch` Touch Mixer Control: {"strip":"A1"|"S1"|"Mix"|id,"lane":"volume|pan|mute|send.<i>.level|fx.<slot>.<param>","value":f64,"time":ticks?}
- `mixer.writeAutomation` Write Automation Points: {"strip":"A1"|"S1"|"Mix"|id,"lane":str?,"points":[[ticks,value]],"tolerance":f64?}

## multicam

- `multicam.audioFollowsVideo` Multi-Camera Audio Follows Video: {"enabled":bool?}
- `multicam.autoAdjustQuality` [host] Auto-Adjust Multi-Camera Playback Quality: {"enabled":bool?}
- `multicam.cut` Cut to Camera: {"camera":1..16|"angle":0-based,"time":ticks?,"videoOnly":bool?}
- `multicam.cutToCamera` Cut to Camera: {"camera":1..16|"angle":0-based,"time":ticks?,"videoOnly":bool?}
- `multicam.cutToCamera1` Cut to Camera 1: {"videoOnly":bool?}
- `multicam.cutToCamera2` Cut to Camera 2: {"videoOnly":bool?}
- `multicam.cutToCamera3` Cut to Camera 3: {"videoOnly":bool?}
- `multicam.cutToCamera4` Cut to Camera 4: {"videoOnly":bool?}
- `multicam.cutToCamera5` Cut to Camera 5: {"videoOnly":bool?}
- `multicam.cutToCamera6` Cut to Camera 6: {"videoOnly":bool?}
- `multicam.cutToCamera7` Cut to Camera 7: {"videoOnly":bool?}
- `multicam.cutToCamera8` Cut to Camera 8: {"videoOnly":bool?}
- `multicam.cutToCamera9` Cut to Camera 9: {"videoOnly":bool?}
- `multicam.editCameras` Edit Cameras…: {"sequence":id?,"cameras":[{"angle":0-based,"name":str?,"enabled":bool?}]?,"audio":"camera1|all|switch"?}
- `multicam.grid` Multi-Camera Grid: {"time":ticks?,"playing":bool?,"cellPixels":f32?,"playbackScale":f32?}
- `multicam.gridLayout` Multi-Camera Layout: {"layout":"auto|2x2|3x3|4x4"}
- `multicam.inspect` Inspect Multi-Camera: {"time":ticks?}
- `multicam.nextPage` Next Multi-Camera Page: {}
- `multicam.page` Multi-Camera Page: {"page":0-based|"next"|"prev"}
- `multicam.prevPage` Previous Multi-Camera Page: {}
- `multicam.recordStart` Start Multi-Camera Recording: {"time":ticks?}
- `multicam.recordStop` Stop Multi-Camera Recording: {"time":ticks?}
- `multicam.selectCamera1` Select Camera 1: {"videoOnly":bool?}
- `multicam.selectCamera2` Select Camera 2: {"videoOnly":bool?}
- `multicam.selectCamera3` Select Camera 3: {"videoOnly":bool?}
- `multicam.selectCamera4` Select Camera 4: {"videoOnly":bool?}
- `multicam.selectCamera5` Select Camera 5: {"videoOnly":bool?}
- `multicam.selectCamera6` Select Camera 6: {"videoOnly":bool?}
- `multicam.selectCamera7` Select Camera 7: {"videoOnly":bool?}
- `multicam.selectCamera8` Select Camera 8: {"videoOnly":bool?}
- `multicam.selectCamera9` Select Camera 9: {"videoOnly":bool?}
- `multicam.selectionTopDown` Multi-Camera Selection Top Down: {"enabled":bool?}
- `multicam.showPreviewMonitor` [host] Show Multi-Camera Preview Monitor: {"enabled":bool?}
- `multicam.switchAngle` Switch Multi-Camera Angle: {"camera":1..16 (shown order)|"angle":0-based,"clips":[id]?,"time":ticks?,"videoOnly":bool?,"audioOnly":bool?}
- `multicam.transmitView` [device] Transmit Multi-Camera View: {"enabled":bool?}

## perf

- `perf.stats` [host] Performance Statistics: {}

## playhead

- `playhead.end` Go to Sequence End: {}
- `playhead.nextEdit` Go to Next Edit Point: {}
- `playhead.nextEditAnyTrack` Go to Next Edit Point on Any Track: {}
- `playhead.prevEdit` Go to Previous Edit Point: {}
- `playhead.prevEditAnyTrack` Go to Previous Edit Point on Any Track: {}
- `playhead.selectedClipEnd` Go to Selected Clip End: {}
- `playhead.selectedClipStart` Go to Selected Clip Start: {}
- `playhead.set` Set Playhead: {"time":ticks|"frame":i64|"seconds":f64|"timecode":str}
- `playhead.start` Go to Sequence Start: {}
- `playhead.step` Step Frames: {"frames":i64}
- `playhead.stepBack` Step Back One Frame: {}
- `playhead.stepBack5` Step Back Five Frames: {}
- `playhead.stepForward` Step Forward One Frame: {}
- `playhead.stepForward5` Step Forward Five Frames: {}

## prefs

- `prefs.get` [host] Get Preferences: {"key":str?}
- `prefs.reset` [host] Reset Preferences: {"category":str?}
- `prefs.schema` [host] Settings Schema: {"category":str?}
- `prefs.set` [host] Set Preferences: {"key":str,"value":any}|{"values":{key:value}}

## presets

- `presets.apply` Apply Preset: {"preset":str,"clips":[id]?}
- `presets.delete` [file] Delete Preset: {"name":str}
- `presets.export` [file] Export Presets: {"path":str,"names":[str]?}
- `presets.import` [file] Import Presets: {"path":str}
- `presets.list` List Effect Presets: {}
- `presets.rename` [file] Rename Preset: {"name":str,"to":str}
- `presets.save` [file] Save Preset: {"clip":id?,"effects":[index]?,"name":str,"description":str?,"keyframes":"scale"|"anchorIn"|"anchorOut"|"none"?}

## project

- `project.columns.list` [host] List Project Columns: {}
- `project.columns.resize` [host] Resize Column: {"column":str,"width":f32}
- `project.columns.set` [host] Metadata Display: {"columns":[name|{"name":str,"width":f32}]}
- `project.delete` Clear: {"items":[id]?}
- `project.deleteSearchBin` Delete Search Bin: {"bin":id}
- `project.deselectAll` Deselect All Project Items: {}
- `project.editSearchBin` Edit Search Bin: {"bin":id,"name":str?,"column":str?,"operator":str?,"text":str?,"rows":[..]?,"matchAll":bool?,"caseSensitive":bool?}
- `project.freeform.alignToGrid` Align to Grid: {"bin":binId?,"grid":f32?}
- `project.freeform.arrangements` List Arrangements: {"bin":binId?}
- `project.freeform.deleteArrangement` Delete Arrangement: {"name":str,"bin":binId?}
- `project.freeform.layout` Freeform Layout: {"bin":binId?,"width":f32?}
- `project.freeform.move` Move Clip Cards: {"items":[id]?,"x":f32,"y":f32,"snap":bool?} | {"positions":{"<id>":[x,y]}}
- `project.freeform.options` [host] Freeform View Options…: {"grid":f32?,"snap":bool?,"showNames":bool?,"showDurations":bool?,"cardSize":f32?}
- `project.freeform.reset` Reset to Grid: {"bin":binId?}
- `project.freeform.resize` Clip Size: {"items":[id]?,"size":f32?,"step":1|-1?}
- `project.freeform.restoreArrangement` Restore Arrangement: {"name":str,"bin":binId?}
- `project.freeform.saveArrangement` Save Arrangement: {"name":str,"bin":binId?}
- `project.freeform.stack` Stack Clips: {"items":[id]?}
- `project.freeform.unstack` Unstack Clips: {"items":[id]?}
- `project.ingestSettings` [file] Ingest Settings…: {"enabled":bool?,"action":"copy|transcode|createProxies|copyAndCreateProxies"?,"destination":str?,"preset":str?}
- `project.inspect` Inspect Project: {}
- `project.items` List Project Panel Rows: {"bin":binId?,"recursive":bool?}
- `project.matteColor` Color Matte Color…: {"item":id?,"color":"#rrggbb"}
- `project.moveToBin` Move to Bin: {"items":[id]?,"bin":binId|null}
- `project.renameBin` Rename Bin: {"bin":binId,"name":str}
- `project.searchBinItems` Search Bin Contents: {"bin":id}
- `project.select` Select Project Items: {"items":[id]}
- `project.selectAll` Select All Project Items: {}
- `project.setMarks` Set Source In/Out: {"item":id,"in":ticks?|null,"out":ticks?|null}
- `project.sort` [host] Sort Project Items: {"column":str,"descending":bool?}
- `project.view.get` [host] Project Panel View: {}
- `project.view.set` [host] Set Project Panel View: {"view":"list|icon|freeform"?,"iconSize":f32?,"fontSize":"small|medium|large|extraLarge"?,"previewArea":bool?,"thumbnails":bool?,"thumbnailsShowEffects":bool?,"hoverScrub":bool?,"thumbnailControlsAllDevices":bool?,"iconSort":column|{"column":str,"descending":bool}?}
- `project.viewPreset.delete` [host] Delete View Preset: {"slot":1..10}
- `project.viewPreset.list` [host] List View Presets: {}
- `project.viewPreset.rename` [host] Rename View Preset: {"slot":1..10,"name":str}
- `project.viewPreset.restore` [host] Restore View Preset: {"slot":1..10}
- `project.viewPreset.save` [host] Save Current View Preset: {"slot":1..10?,"name":str?}
- `project.viewPreset.saveAs` [host] Save As New View Preset: {"name":str?,"slot":1..10?}

## scopes

- `scopes.read` Read Lumetri Scopes: {"scopes":["waveform","parade","histogram","vectorscopeYuv","vectorscopeHls"]?,"waveformType":"rgb|luma|yc|ycNoChroma"?,"paradeType":"rgb|yuv|rgbWhite"?,"colorSpace":"auto|601|709|2100"?,"clamp":bool=true,"columns":n=8,"peaks":n=8,"bins":bool=true,"scale":0.5,"time":ticks?|"frame"|"seconds"|"timecode"}

## sequence

- `sequence.addEdit` Add Edit: {"time":ticks?}
- `sequence.addEditAllTracks` Add Edit to All Tracks: {"time":ticks?}
- `sequence.addTracks` Add Tracks…: {"video":n=1,"audio":n=0,"submix":n=0,"videoAfter":"first"|"V2"|n?,"audioAfter":"first"|"A2"|n?,"submixAfter":"first"|"S1"|n?,"audioType":"standard|5.1|adaptive|mono"?,"submixType":"stereo|5.1|adaptive|mono"?}
- `sequence.applyAudioTransition` Apply Audio Transition: {"clip":id?,"effect":str?,"frames":i64?}
- `sequence.applyVideoTransition` Apply Video Transition: {"clip":id?,"effect":"cross_dissolve|Cross Dissolve|…"?,"frames":i64?,"edge":"in"|"out"?,"params":{param:value}?,"reverse":bool?}
- `sequence.close` Close Sequence: {"item":id?}
- `sequence.closeGap` Close Gap: {"track":"V1"|id,"time":ticks}
- `sequence.closeOthers` Close Other Timeline Panels: {"item":id?}
- `sequence.colorSettings` Color Management…: {"workingSpace":"rec709"|"rec2100-pq"|"rec2100-hlg"?,"wideGamut":bool?,"autoToneMap":bool?}
- `sequence.deleteRenderFiles` [file] Delete Render Files: {}
- `sequence.deleteRenderFilesInToOut` [file] Delete Render Files In to Out: {}
- `sequence.deleteTrack` Delete Track: {"track":"V3"|id}
- `sequence.deleteTracks` Delete Tracks…: {"video":"empty"|"V2"|id?,"audio":"empty"|"A2"|id?}
- `sequence.extract` Extract: {}
- `sequence.goToNextGap` Next in Sequence: {}
- `sequence.goToNextGapInTrack` Next in Track: {"track":"V1"|id?}
- `sequence.goToPrevGap` Previous in Sequence: {}
- `sequence.goToPrevGapInTrack` Previous in Track: {"track":"V1"|id?}
- `sequence.inspect` Inspect Sequence: {"item":id?}
- `sequence.joinThroughEdits` Join Through Edits: {"clips":[id]?,"all":bool?}
- `sequence.lift` Lift: {}
- `sequence.linkedSelection` Linked Selection: {"on":bool?}
- `sequence.makeSubsequence` Make Subsequence: {"name":str?}
- `sequence.matchFrame` Match Frame: {}
- `sequence.moveTab` Move Sequence Tab: {"item":id?,"index":int}
- `sequence.nestSequences` Insert and overwrite sequences as nests or individual clips: {"on":bool?}
- `sequence.normalizeMixTrack` Normalize Mix Track…: {"db":f64=0}
- `sequence.open` Open in Timeline: {"item":id}
- `sequence.renderAudio` [file] Render Audio: {"wait":bool=false}
- `sequence.renderBar` Render Bar: {}
- `sequence.renderEffectsInToOut` [file] Render Effects In to Out: {"wait":bool=false}
- `sequence.renderInToOut` [file] Render In to Out: {"wait":bool=false}
- `sequence.renderSelection` [file] Render Selection: {"wait":bool=false}
- `sequence.revealInProject` Reveal Sequence in Project: {"item":id?}
- `sequence.revealNested` Reveal Nested Sequence: {}
- `sequence.reverseMatchFrame` Reverse Match Frame: {}
- `sequence.selectionFollowsPlayhead` Selection Follows Playhead: {"on":bool?}
- `sequence.setTransition` Edit Transition Settings: {"transition":id,"params":{param:value}?,"reverse":bool?,"reset":bool?}
- `sequence.settings` Sequence Settings…: {"width":u32?,"height":u32?,"fps":f64?,"name":str?,"sampleRate":u32?,"mix":"Stereo|Mono|5.1|Adaptive"?}
- `sequence.showThroughEdits` [host] Show Through Edits: {"on":bool?}
- `sequence.simplify` Simplify Sequence…: {"name":str?,"removeDisabled":bool=true,"removeEmptyTracks":bool=true,"closeGaps":bool=false,"moveClipsDown":bool=false,"removeVideoEffects":bool=false,"removeAudioEffects":bool=false,"removeText":bool=false,"keep":"both|video|audio"}
- `sequence.snap` Snap in Timeline: {"on":bool?}
- `sequence.throughEdits` List Through Edits: {}
- `sequence.transcribe` [file] Transcribe Sequence…: {"track":"mix"|"A1"|id?,"language":"en|auto"?,"diarize":bool?,"maxSpeakers":n?,"model":str?}

## shortcuts

- `shortcuts.audit` [host] Shortcut Audit: {}
- `shortcuts.clear` [host] Clear Shortcut: {"command":id,"keys":str?,"panel":str?}
- `shortcuts.conflicts` [host] Shortcut Conflicts: {"platform":"mac|windows|linux"?}
- `shortcuts.deletePreset` [host] Delete Shortcut Preset: {"name":str}
- `shortcuts.export` [host] Export Keyboard Shortcuts: {"path":str}
- `shortcuts.forKey` [host] Shortcuts on a Key: {"key":"K","platform":str?}
- `shortcuts.get` [host] Get Shortcuts of a Command: {"command":id,"platform":str?}
- `shortcuts.import` [host] Import Keyboard Shortcuts: {"path":str,"activate":bool=true}
- `shortcuts.list` [host] List Keyboard Shortcuts: {"query":str?,"panel":str?,"assigned":bool?,"platform":"mac|windows|linux"?}
- `shortcuts.loadPreset` [host] Load Shortcut Preset: {"name":str}
- `shortcuts.presets` [host] Keyboard Shortcut Presets: {}
- `shortcuts.redo` [host] Redo Shortcut Change: {}
- `shortcuts.resolve` [host] Resolve Shortcut: {"keys":str,"panel":str?,"platform":str?}
- `shortcuts.savePreset` [host] Save Shortcut Preset As: {"name":str}
- `shortcuts.set` [host] Assign Shortcut: {"command":id,"keys":"Cmd+Shift+K","panel":str?,"add":bool?,"keepConflicts":bool?}
- `shortcuts.undo` [host] Undo Shortcut Change: {}

## source

- `source.insert` Insert: {}
- `source.inspect` Inspect Source Monitor: {}
- `source.open` Open in Source Monitor: {"item":id?}
- `source.overwrite` Overwrite: {}
- `source.setPlayhead` Set Source Playhead: {"time":ticks|"frame":i64|"seconds":f64}

## state

- `state.inspect` Inspect Editor State: {}

## timeline

- `timeline.move` Move Clips: {"moves":[{"clip":id,"track":id|"V2","time":ticks}],"insert":bool,"linked":bool?}
- `timeline.moveAudioTargetsDown` Move All Audio Targets Down: {}
- `timeline.moveAudioTargetsUp` Move All Audio Targets Up: {}
- `timeline.moveVideoTargetsDown` Move All Video Targets Down: {}
- `timeline.moveVideoTargetsUp` Move All Video Targets Up: {}
- `timeline.nudgeDown` Nudge Clip Selection Down: {}
- `timeline.nudgeLeft` Nudge Clip Selection Left One Frame: {}
- `timeline.nudgeLeft5` Nudge Clip Selection Left Five Frames: {}
- `timeline.nudgeRight` Nudge Clip Selection Right One Frame: {}
- `timeline.nudgeRight5` Nudge Clip Selection Right Five Frames: {}
- `timeline.nudgeUp` Nudge Clip Selection Up: {}
- `timeline.place` Place Clip: {"item":id,"track":"V1"|id|"A1" (sound only)?,"audioTrack":"A1"|id?,"time":ticks|"frame":i64|"seconds":f64,"insert":bool,"sourceIn":ticks?,"duration":ticks?}
- `timeline.rateStretch` Rate Stretch: {"clip":id,"edge":"in|out","delta":ticks}
- `timeline.razor` Razor: {"time":ticks,"track":"V1"|id?,"clip":id?}
- `timeline.roll` Rolling Edit: {"left":id,"right":id,"delta":ticks|"deltaFrames":i64}
- `timeline.select` Select Clips: {"clips":[id],"add":bool,"toggle":bool}
- `timeline.selectClipAtPlayhead` Select Clip at Playhead: {}
- `timeline.selectNextClip` Select Next Clip: {}
- `timeline.selectPrevClip` Select Previous Clip: {}
- `timeline.setTargeting` Track Targeting: {"track":"V1"|id,"targeted":bool?,"sourcePatch":bool?}
- `timeline.setTrack` Track Settings: {"track":"V1"|id,"locked":bool?,"syncLock":bool?,"enabled":bool?,"muted":bool?,"solo":bool?,"name":str?,"volumeDb":f64?,"pan":f64?}
- `timeline.slide` Slide: {"clip":id,"delta":ticks|"deltaFrames":i64}
- `timeline.slideLeft` Slide Clip Selection Left One Frame: {}
- `timeline.slideLeft5` Slide Clip Selection Left Five Frames: {}
- `timeline.slideRight` Slide Clip Selection Right One Frame: {}
- `timeline.slideRight5` Slide Clip Selection Right Five Frames: {}
- `timeline.slip` Slip: {"clip":id,"delta":ticks|"deltaFrames":i64}
- `timeline.slipLeft` Slip Clip Selection Left One Frame: {}
- `timeline.slipLeft5` Slip Clip Selection Left Five Frames: {}
- `timeline.slipRight` Slip Clip Selection Right One Frame: {}
- `timeline.slipRight5` Slip Clip Selection Right Five Frames: {}
- `timeline.toggleAllAudioTargets` Toggle All Audio Targets: {}
- `timeline.toggleAllSourceAudio` Toggle All Source Audio: {}
- `timeline.toggleAllSourceVideo` Toggle All Source Video: {}
- `timeline.toggleAllVideoTargets` Toggle All Video Targets: {}
- `timeline.toggleMuteTargetedAudio` Toggle Mute for All Targeted Audio Tracks: {}
- `timeline.toggleOutputTargetedVideo` Toggle Track Output for All Targeted Video Tracks: {}
- `timeline.toggleSoloTargetedAudio` Toggle Solo for All Targeted Audio Tracks: {}
- `timeline.toggleTargetA1` Toggle Target Audio 1: {}
- `timeline.toggleTargetA2` Toggle Target Audio 2: {}
- `timeline.toggleTargetA3` Toggle Target Audio 3: {}
- `timeline.toggleTargetA4` Toggle Target Audio 4: {}
- `timeline.toggleTargetA5` Toggle Target Audio 5: {}
- `timeline.toggleTargetA6` Toggle Target Audio 6: {}
- `timeline.toggleTargetA7` Toggle Target Audio 7: {}
- `timeline.toggleTargetA8` Toggle Target Audio 8: {}
- `timeline.toggleTargetV1` Toggle Target Video 1: {}
- `timeline.toggleTargetV2` Toggle Target Video 2: {}
- `timeline.toggleTargetV3` Toggle Target Video 3: {}
- `timeline.toggleTargetV4` Toggle Target Video 4: {}
- `timeline.toggleTargetV5` Toggle Target Video 5: {}
- `timeline.toggleTargetV6` Toggle Target Video 6: {}
- `timeline.toggleTargetV7` Toggle Target Video 7: {}
- `timeline.toggleTargetV8` Toggle Target Video 8: {}
- `timeline.trim` Trim Edit: {"clip":id,"edge":"in|out","mode":"regular|ripple","delta":ticks|"deltaFrames":i64}

## transcript

- `transcript.createCaptions` Create Captions from Transcript…: {"maxChars":n?,"lines":1|2?,"minSeconds":f?,"maxSeconds":f?,"gapFrames":n?,"format":str?,"name":str?}
- `transcript.delete` Delete Transcript: {"items":[id]?}
- `transcript.downloadModel` [network] Download Speech Model: {"model":"whisper-base"?}
- `transcript.extract` Extract Selected Text: {"from":word,"to":word?}
- `transcript.generate` [file] Transcribe…: {"items":[id]?,"model":"whisper-base"?,"language":"en|auto"?,"diarize":bool?,"maxSpeakers":n?}
- `transcript.inspect` Inspect Transcript: {"paragraphGapSeconds":f?}
- `transcript.lift` Lift Selected Text: {"from":word,"to":word?}
- `transcript.models` [file] List Speech Models: {}
- `transcript.removeFillers` Remove Filler Words: {"fillers":[str]?}
- `transcript.removePauses` Remove Pauses: {"minSeconds":f?,"keepSeconds":f?}
- `transcript.renameSpeaker` Rename Speaker…: {"speaker":"Speaker 1"|index,"name":str,"item":id?}
- `transcript.search` Search Transcript: {"query":str}
- `transcript.select` Mark Selected Text: {"from":word,"to":word?}
- `transcript.set` Import Transcript: {"item":id,"transcript":{"language":str,"speakers":[{"name":str}],"words":[{"text":str,"start":tick,"end":tick,"speaker":n?}]}}

## trim

- `trim.applyDefaultTransition` Apply Default Transitions to Selection: {"clips":[id]?}
- `trim.backward` Trim Backward: {}
- `trim.backwardMany` Trim Backward Many: {}
- `trim.cancelDynamic` Cancel Dynamic Trim: {}
- `trim.clear` Clear Edit Point Selection: {}
- `trim.edit` Trim Edit: {}
- `trim.extendNextEdit` Extend Next Edit To Playhead: {}
- `trim.extendPreviousEdit` Extend Previous Edit To Playhead: {}
- `trim.extendToPlayhead` Extend Selected Edit to Playhead: {}
- `trim.forward` Trim Forward: {}
- `trim.forwardMany` Trim Forward Many: {}
- `trim.monitor` Trim Monitor State: {}
- `trim.next` Trim Next Edit to Playhead: {}
- `trim.nudge` Trim by Frames: {"frames":i64}
- `trim.playAround` Play Around Edit: {"clock":seconds,"loop":bool=true,"toggle":bool?}
- `trim.previous` Trim Previous Edit to Playhead: {}
- `trim.rippleNext` Ripple Trim Next Edit to Playhead: {}
- `trim.ripplePrevious` Ripple Trim Previous Edit to Playhead: {}
- `trim.selectEditPoint` Select Edit Point: {"clip":id,"edge":"in|out","kind":"trim|ripple|roll","add":bool?}
- `trim.selectNearest` Select Nearest Edit Point: {"kind":"rippleIn|rippleOut|roll|trimIn|trimOut"}
- `trim.shuttle` Dynamic Trim (Shuttle): {"direction":"forward|reverse","slow":bool?,"clock":seconds}
- `trim.shuttleStop` Dynamic Trim Stop: {"clock":seconds?}
- `trim.tick` Advance Trim Playback: {"clock":seconds}
- `trim.toggleType` Toggle Trim Type: {}
