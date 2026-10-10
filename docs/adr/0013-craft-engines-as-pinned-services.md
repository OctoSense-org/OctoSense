# ADR 0013: Craft engines as pinned services

English | [简体中文](0013-craft-engines-as-pinned-services.zh-CN.md)

Status: Proposed. The measurements below were taken on 8 Oct 2026 against
the real engines (photocraft's suite and CLI, gridcraft's parser and
evaluator) and the Splash compute kernels of makepad `32d6415f`; no service
is implemented yet.

## Context

The storytold "craft" projects are clean-room, Apache-2.0 reimplementations
of professional tools in pure Rust: photocraft (Photoshop-class raster
engine: own JPEG/PNG/WebP/TIFF/EXR codecs, 817 registered commands, a
headless CLI), gridcraft (spreadsheet: formula parser, evaluator,
dependency-driven recalc, xlsx), pdfcraft, soundcraft, vectorcraft and
others. Their engines are cleanly separated from their UIs (egui, which we
would not use), and the two we probed are real: photocraft passes 1,203
engine tests and converts/filters real files headlessly; gridcraft's parsed
formula AST lowers mechanically onto our Splash compute kernels.

OctoSense wants these capabilities — spreadsheets agents can recalc,
image/PDF processing agents can run — without writing engines ourselves and
without taking unpinned third-party code into the trust base. We already
have the shapes such code fits: host services with typed tools (mail,
calendar, news), native widgets registered for Octoscript (makepad-d3's
charts), script apps distributed through App Hub, and the compute kernel
JIT with admission for untrusted work.

artcraft, the same org's flagship, carries no license and is excluded
entirely.

## Decision

1. **Three layers, three homes.** A craft capability enters OctoSense as:
   the **engine** — their crates, unchanged — behind a **host service**
   with typed tools (`sheet.*`, `photo.*`), which is what agents call; any
   **heavy surface** (sheet grid, raster canvas) as a **native Makepad
   widget** in Octoscript-Makepad or the makepad fork, script-visible like
   `d3.*`; and the **app** as a pure Octoscript bundle through App Hub,
   carrying its agent in `tools.json`. The egui UIs are not ported;
   panels and dialogs are rewritten declaratively, and only the few
   surfaces that genuinely need Rust become widgets.
2. **Pinned forks under `ymote`.** Engines are consumed as git
   dependencies of forks under the `ymote` account (`ymote/gridcraft`,
   `ymote/photocraft`, more as adopted), pinned by revision in the root
   `Cargo.toml` like every external dependency (ADR 0001 discipline). The
   forks are read-only mirrors: fixes we need go upstream to storytold
   first and are adopted by moving the pin. Initial pins: gridcraft
   `c6b6f4177cbf`, photocraft `eec4af65513b`.
3. **Headless before UI.** The first deliverable per engine is the host
   service and its agent tools, desktop-first — spreadsheet recalc and
   image operations for every agent with no UI built. Services stay out of
   the phone shells until their binary cost is weighed per engine.
4. **The kernel JIT is the differentiator, with authorship-mapped
   admission.** gridcraft recalc becomes hybrid: numeric fill-downs lower
   from their formula AST to f64 compute kernels (cached per formula),
   everything else stays on their evaluator. photocraft adjustment and
   curve formulas run as kernels for previews and batch. A formula or
   filter written by a person runs under the user origin; one written by a
   model runs under `Ai` admission with its element bound and budget.
5. **Approvals and files as everywhere else.** The services own no new
   access: file reads and writes go through the existing files host tools
   and approval flow, and every agent-visible tool is granted per app the
   way mail's and calendar's are.

## Measurements

- gridcraft, `=@A:A*1.05+SIN(@B:B)*0.5+EXP(-@A:A*0.01)` filled down 1M
  rows, values cross-checked: their evaluator 595.7 ms (596 ns/cell); the
  same AST lowered to a Splash kernel 4.9 ms on one thread (122×), 2.5 ms
  on eight (243×); idiomatic Rust with libm 10.0 ms — the kernel's inlined
  polynomial `sin`/`exp` beat per-cell libm calls by 2×. Kernel compile
  2.9 ms, once per formula.
- photocraft: 1,203 engine tests across 41 suites pass; PSD→PNG, JPEG→TIFF,
  PNG→WebP and a Gaussian blur verified on real files through its CLI.
  HEIF is feature-gated off in the default build; EXIF and DPI are dropped
  on convert.

## Consequences

- Agents gain spreadsheet and image/PDF operations quickly, and the
  engines stay upgradable by moving a pin.
- Two engine lineages exist for overlapping ground (makepad's own apps
  vs craft engines). The sheet pilot resolves this deliberately: the
  existing native Sheets app adopts gridcraft's formula/calc stack rather
  than a second spreadsheet appearing.
- The fork set under `ymote` is part of the supply chain and joins the
  pin-sweep discipline; divergence from storytold is treated like our
  makepad fork's divergence from upstream — deliberate, listed, and
  upstreamed where possible.
- Kernel lowering covers the numeric subset only; text, references and
  dynamic arrays remain on the engine's evaluator, so recalc results stay
  exactly the engine's except where a kernel provably computes the same
  values (f64 kernels; the f32 probe is not the production shape).

## Agent tools (8 Oct 2026)

The ten engine services (word, deck, cad, light, sound, design, film,
effect, vector, pdf) now have agent tools, and the system agent can call
them:

- **Declared with each service.** `apps/<family>/host-service/tools.json`,
  in App Hub's `tools.json` shape (the crate's `TOOLS_JSON`), with object
  schemas both ways. Each crate's tests load it with App Hub's own loader.
- **A virtual owner per engine.** No app ships these tools yet, so the
  shell declares them under `os.<family>`
  (`crates/shell/src/host_tools/engines.rs`) and runs each on the engine's
  service, as that system identity, by the method of its own name, in the
  engine's area `<apps root>/.host/<family>` (since 9 Oct 2026, in the
  caller's own folder: see below); answers and errors name files relative
  to it, never by the host's own paths. A virtual owner is not an app:
  it has no bundle, no app agent and no Settings row, and it is admitted as
  the shell's own compiled-in service. When an engine ships as an app
  (decision 1), its bundle's `tools.json` takes over the namespace and the
  virtual owner goes.
- **Granted to the system agent only.** `ENGINE_TOOLS`
  (`crates/shell/src/system_chat/grants.rs`) is a reviewed narrow grant
  like Calendar's: 43 tools, every read tool and each act tool that writes
  only inside its engine's own area. None is destructive, outward or
  shareable, so no app's agent can be granted one. The generic command
  doors `vector.run` and `effect.run` are declared but held back for a
  separate review. (Since 9 Oct 2026 the grant is 27 tools: `info` and a
  reviewed `run` door for seven engines; see "Command doors" below.) The `commands` catalogs of design, effect and vector are
  not declared, because they answer JSON arrays and octos takes object
  results only.
- **Files are the open gap** (closed on 9 Oct 2026: see below). Decision 5
  expected file access through the existing files host tools. They do not
  reach the engines' areas, and neither does the system agent's workspace.
  Until a reviewed staging path exists, an engine sees only what its own
  tools wrote (`word.new`, `deck.new` and the conversions), and every tool
  description says its paths are relative to its engine's workspace.
  designcraft resolves data-merge sources and vectorcraft linked images by
  the paths inside the documents they open, so a staging path must not
  admit outside documents to those two until that is contained.
- **The kernel's cap.** octos takes at most 64 host tools in one
  registration and refuses a larger set whole. With the engines, the system
  session's largest set is 63 (47 since the command doors). The shell offers the engine tools last and
  cuts them first past the cap, and a test keeps the whole grant within it.
- **Long calls.** An engine call runs on the thread that dispatches it
  (the shell's UI thread for a tool call) and holds App Hub's service
  registry while it runs, as app calls to the engines always have. A long
  export stalls the shell for its duration. The kernel waits 30 s for a
  host tool's answer (an act call then ends with an unknown outcome), and
  App Hub times the request out after 60 s. Moving engine work to a worker
  with per-method timeouts is follow-up work.

## Engine skills (9 Oct 2026)

The system agent now learns each engine from an octos skill it reads on
demand. The per-method tools send their full schemas on every turn (about
29 KB for the 43 engine tools) and still leave most of each engine out of
reach: photocraft alone has 817 commands.

- **One skill per linked engine.** `apps/<family>/host-service/skill/`
  holds a hand-written `SKILL.md`: frontmatter `name: <family>-engine` and a
  one-line `description` under 200 bytes, then what the engine does, the
  tools the system agent has for it, the file rules and worked examples.
  Beside it are references generated from the engine at its pin: the
  command catalog `commands.md`, one line per id, for the ten engines that
  have a catalog (photo, word, deck, cad, light, design, film, effect,
  vector, pdf; 4,557 ids in all), light's `controls.md` and sheet's
  `functions.md`. Each service embeds its skill (`src/skill.rs`), so a build
  ships the skill that matches its engine. The shell registers the linked
  engines' skills with the services' own gates
  (`crates/shell/src/system_chat/skills.rs`): sheet with `app-hub`,
  photo and the ten with `craft-engines`.
- **Installed before every kernel start.** The kernel service
  (`crates/kernel/src/skills.rs`) writes them into the skills dir octos
  reads for the system agent's profile, `<core dir>/profiles/_main/data/skills`,
  each marked `.octosense-managed`. It refreshes a changed skill, removes a
  managed one that is no longer registered, and never touches the person's.
  When the profile runtime starts, octos lists each skill's name,
  description and location in the profile's system prompt
  (`build_skills_summary`). The agent reads a `SKILL.md` with `read_file`,
  since the dir is a read zone of every session's file tools. The twelve
  summary entries cost about 4.6 KB a turn.
- **Every `_main` session sees them.** octos scopes skills to a profile,
  and app agents' peers run on `_main` too. They hold no engine tool, and
  each description says to read the skill before using `<family>.*` tools.
- **Generated and checked.** `crates/skill-gen` makes `commands.md` and
  `safety.json` from the engine's live catalog and the hand-written
  `safety-rules.json`. Each service's `tests/skill.rs` fails on an
  unclassified id, a stale rule or a drifted file;
  `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-<family>-service --test skill`
  regenerates them. The shell's tests check that each skill's `## Tools` is
  exactly the system agent's grant for its engine and that its examples
  match the tools' schemas.
- **Safety classes.** Every catalog id is classed from its implementation
  as `safe`, `file`, `code` (plug-ins, scripts, commands that run other
  commands), `network`, `device` or `host` (windows, views, preferences,
  the clipboard). Each `safety.json` has its engine's counts.
- **Next** (done on 9 Oct 2026: see "Command doors" below). Once
  per-caller areas land, `<family>.info` plus one reviewed `<family>.run`
  door per engine with a catalog replace the 43 per-method tools. Each door
  denies `file` outside the caller's area and every `code`, `network`,
  `device` and `host` id. An engine that cannot be fenced keeps its curated
  tools.

## Engines work in the caller's own folder (9 Oct 2026)

The engines are OctoSense-wide tools that run in an app's file folders, not
in folders of their own. This closes "Files are the open gap" above. The
private per-engine folder `<apps root>/.host/<family>` is used only by a
service run without the shell (its own tests, App Hub's card-host).

- **One resolver per service.** Each of the twelve engine services (sheet,
  photo and the ten above) asks a resolver where a call works
  (`set_area_resolver`; the shared rules are in `crates/engine-area`). The
  shell installs its resolver when the services register
  (`crates/shell/src/host_tools/areas.rs`). An area is a root folder,
  whether a write there may replace a file, and how many bytes the call
  may still add. The resolver decides from trusted host data only, never
  from a call's arguments:
  - **The system agent's call to a craft engine's tool:** its workspace,
    the folder its conversation runs in, which its own file tools
    (`read_file`, `list_dir`) see. The kernel names it when the system
    conversation opens (`session/open`'s `workspace_root`); before that,
    the workspace saved in the core directory counts. When neither is
    known, the call is refused (`no_workspace`).
  - **An app agent's call to a craft engine's tool:** that account's folder
    `accounts/<hash>/` in the app's jail, the agent's own workspace. None of
    the tools is shareable, so no app's agent reaches one today (developer
    mode grants only shareable tools across apps); the rule is ready for
    when one may.
  - **Any agent's call to an app's own engine tool** (Sheets' `sheets.*`,
    Photos' `photos.info`): that app's agent folder, for its own agent's
    account or else the account the app acts for now. Every app tool works
    on its app's data, so the system agent's `sheets.get` reads the
    workbook that Sheets' agent opened, in Sheets' folder.
  - **An app's own `host.request` to an engine** (no bundle makes one
    yet): the app's storage, the jail its `fs.*` sees. The app must hold
    the `storage` capability.

  A signed-out, suspended or refused account is refused (`signed_out`,
  `workspace_refused`), and so is a request that names any other folder.
- **Plumbing.** The executor that runs an agent's tool knows who called
  (the app, account and caller kind that the broker stamps) and which owner
  it runs the tool as. It resolves the area from both before dispatch and
  registers it as a grant, held until the call is
  answered or cancelled. It passes the area's root in the `ServiceCall`'s
  `host_dir`, the one field that only host code sets. The resolver answers
  a granted root with its area, and App Hub's shared `<apps root>/.host`
  with the calling app's storage.
- **Agents never overwrite.** An agent's call (`may_prompt` false) never
  replaces an existing file. A write refuses any entry already at its path
  (a file, a folder or a link), and a file created at the same moment is
  not replaced either (`create_new`, or a hard link from a staging
  folder). The error asks for a new name. An app's own foreground request
  may still replace a file, atomically. An engine that writes its own
  files (a PDF split, a batch develop with its sidecars, a multi-artboard
  SVG, a video export) writes them into a hidden staging folder inside the
  area. They are moved into place all at once, or not at all.
- **Quota.** Output into an app's jail must fit what is left of the app's
  storage quota: the jail's ceiling (App Hub's admitted `storage` limit
  for a script app, `storage.max_bytes` for a native app) less what the
  jail holds when the call starts. All writes of one call share that
  budget, and each is checked before it is written. The system agent's
  workspace has no quota beyond the services' own per-call caps.
- **Containment.** Every path is relative to the area. `..`, absolute paths
  and symbolic links that lead out of it are refused, and no write goes
  through a link. Answers and errors name files relative to the area.
- **Paths inside documents.** Each engine was checked for files it opens
  on its own, and the per-command safety review of the engine skills
  (above) for what else a document can make it do. Each is fenced to the
  area, and each fence has a hostile fixture in its service's tests:
  - **design:** an IDML whose graphics are links (any link, `file://`
    included) is refused before the engine opens it: the engine's own
    importer runs first, with a reader that records each link instead of
    reading it. The `Document Fonts` folder beside a document must stay
    inside the area. A `.designcraft` document's data-merge source and
    asset links are cleared before any output, so no path elsewhere
    reaches an output. A placed SVG that links a file (`<image href>`)
    refuses the document for every call, because usvg reads the link as
    soon as it parses the SVG.
  - **vector:** the engine's own SVG importer and native loader run first
    and list every file a document links (absolute, `file://`, `../`, and
    the editing payload inside an exported SVG or PDF). A document that
    links a file outside the area is refused.
  - **effect:** footage and every frame of an image sequence must be inside
    the area, and 3D models are refused. An effect parameter that the
    engine reads as a file by its own path, with `std::fs` past every gate
    (Apply Color LUT, Lumetri's input LUT and look, an OCIO file transform
    or config, mocha shape data), refuses the project unless it holds the
    file's text inline; so does an expression on such a parameter, which
    could produce a path when the frame renders.
  - **photo:** the engine's own authorized workspace is the area. A
    document with a smart object linked to a file outside it is refused
    when it opens, nested smart objects included: the engine reads such a
    link by its own path, and a PSD export embeds the file's raw bytes.
  - **pdf:** reads and writes are rooted at the area, and a document's own
    scripts never run: every service session turns the engine's
    JavaScript off (it runs an XFA form's scripts on opening by default,
    in its sandbox).
  - **film:** media a project points at outside the area stay offline.
    **light:** an original must be a regular file, and its XMP sidecars
    must be inside the area.
  - **word, deck, cad, sound, sheet:** bytes in, bytes out; the engines
    open no other file. word, deck and cad use system fonts by family
    name, gridcraft refuses links to other workbooks, and sound never
    opens an audio or MIDI device. Since the command doors, `word.run`
    runs wordcraft's commands, and `review.readAloud` (which starts a
    speech program) is classed `device`, which no door runs.

  All twelve engines are fenced; none was left on its private folder.
- **The command doors are held** (until 9 Oct 2026: see "Command doors"
  below). `vector.run` and `effect.run` were never granted, and their lists of refused ids are no fence: a wrapper
  (`command.batch`, `engine.batch`, `file.runScript`), a preference
  (`prefs.set` of a plug-ins folder) or a plug-in effect runs past them.
  With the shell's resolver installed, both services refuse `run`
  outright, so the doors reach no caller's folder until their review.
  `photo.run`, also ungranted, keeps photocraft's own allow-list for file
  commands and smart-object paths.
- **Workbooks.** A Sheets workbook belongs to the area it was created or
  opened in. A call from another area cannot see, change, export or close
  it. Each area holds at most 16 open workbooks, and all areas together
  at most 64.

## Command doors (9 Oct 2026)

The system agent now drives seven engines through one reviewed command door
each, instead of a curated tool per method (#418).

- **The surface.** word, deck, cad, light, film, effect and vector each
  give the system agent `<family>.info` (a read) and `<family>.run`: up to
  64 commands of the engine's catalog, run in order on the document at
  `path` (or on a new one), then the result written to `out`, a new file in
  the caller's folder. `out`'s extension picks what is written, so one door
  covers what the per-method tools did (conversions, renders, frames,
  exports, the Lottie import and export, a develop). sound, design and pdf
  keep fixed tools. The engine grant goes from 43 tools to 27, and the
  system session's largest set from 63 to 47 of octos's 64 (96 after octos
  #2737 and a repin). The per-method service methods stay for apps' own
  requests (`host.request`), which the doors do not change.
- **An allowlist, never a deny-list.** Each service builds its gate
  (`crates/engine-area/src/door.rs`, `Door`) from the engine's generated
  classification `skill/safety.json` and its own `REVIEWED` settlements.
  Every command of a call is admitted before any runs. Only ids classed
  `safe` run, and the `file` commands a reviewer found only read the file
  their parameters name: the gate resolves that path inside the caller's
  folder (relative, no `..`, through links, an existing file) and hands
  the engine the absolute path. Writes go through the door's own `out`
  only (`Area::write`: never over an existing file for an agent, within
  the quota). Every other id is refused: `code`, `network`, `device` and
  `host`, an unreviewed `file` command, and an id the classification does
  not have (the skill drift test fails while any engine id lacks a class).
  A reviewer can also hold back a command that its class would let run,
  when the engine cannot do it safely yet (`Held`): the door refuses it,
  saying why, until the engine is fixed.
- **Composite and indirect commands.** Batches, macros and scripts
  (`command.batch`, `engine.batch`, `tools.macros`, `file.runScript`) are
  classed `code` and refused whole. A setter that changes app-wide state by
  key runs only with a key its service reviewed; none is reviewed today, so
  cad's `setvar` is refused. A command that names another command
  (vector's `perspective.draw {command}`) has that id admitted in turn,
  with its own parameters, at most four deep. A command that names an
  effect (`effect.apply`, vector's `appearance.addEffect`) runs only an
  effect the engine builds in, so an effect plug-in (`plugin.<id>`) never
  runs through a door.
- **Caps (decided 9 Oct 2026).** Engine work runs on the shell's UI
  thread until #399, so no single call may multiply work or memory
  without bound. Each service reviews the parameters that do (array and
  copy counts, rows and columns, canvas, page and render sizes, frame
  ranges and rates, iteration counts, and the small scales and spacings
  that multiply drawing work) and gives each a `Limit`: a ceiling on one
  parameter or on a product, with its reason beside it, checked by the
  gate before anything runs, inner commands included. Copies multiply
  across a call (an array of an array), within a per-call budget. Each
  service also keeps the document within a size ceiling after every
  command (which stops a copy-and-paste loop that has no count) and bounds
  its own `out` (a render's pixels, an export's frames).
- **Fences after every command.** What a command can write into the
  document, and a later command, render or `out` would then read, is
  checked after each command, and the call fails before anything is
  written: vector's linked images; effect's footage, LUT, OCIO and mocha
  parameters (Essential Graphics values included) and effect plug-ins;
  film's effect parameters, scratch disks, ingest folder and queued
  exports (its media stay offline outside the area, as before); photo's
  linked smart objects and Color Lookup files.
- **#418's routes.** A batch wrapping `plugin.install`, `prefs.set` of a
  plug-ins folder (vector: `code`; effect and film: `host`) and
  `effect.apply {effect: "plugin.<id>"}` are refused, each with a hostile
  fixture in its service's tests and again through the shell's relay
  (`every_command_door_refuses_what_its_review_does_not_admit`). The fences
  of #419 stay: design's IDML links and pdf's scripts (neither has a door),
  effect's LUT and colour files and photo's `.psd` links (checked after
  every command), and word's `review.readAloud` (now `device`).

Each engine's surface:

| Engine | The system agent's tools | What its door runs beyond `safe` ids | Caps (one call) |
| --- | --- | --- | --- |
| word | `word.info`, `word.run` | 328 of 389 ids. Reads: `insert.picture`, `picture.change` (`path`). | Tables ≤ 10,000 cells; pages 72–1584 pt a side; a replacement grows the text ≤ 1,000× (chained, ≤ 10,000×); the document ≤ 500,000 characters, 50,000 paragraphs and 128 MiB of pictures; a PDF ≤ 10,000 pages. |
| deck | `deck.info`, `deck.run` | 198 of 222. Reads: `insert.picture`, `picture.change` (`path`). Held back until deckcraft bounds its media and zip parsing (hostile data can abort the shell, #448): `insert.audio`, `insert.video`, `media.info`, `media.posterFrame`, `file.openBytes`. | Tables ≤ 5,625 cells; charts ≤ 10,000 points; a slide ≤ 1920 × 1080 pt of area; the presentations ≤ 500 slides, 20,000 shapes and 1,000,000 characters; rasters ≤ 160 MP a call, 4096² each. |
| cad | `cad.info`, `cad.run` | 288 of 295. `setvar` is refused: it sets variables by name, and no name is reviewed. | Arrays and copies ≤ 10,000 copies, multiplied across the call ≤ 10,000; polygons ≤ 1,024 sides; spline fit points ≤ 2,000; hatch and linetype scale ≥ 0.0001; drawings ≤ 200,000 objects; a render ≤ about a second of drawing work, estimated first. |
| light | `light.info`, `light.run` | 189 of 239. Nothing more. | Originals ≤ 64 MP; exports ≤ 16 MP (AVIF ≤ 4); ≤ 16 photos (virtual copies included); ≤ 16 masks, 256 strokes and 64 spots; a crop ≥ 1% a side. |
| film | `film.info`, `film.run` | 525 of 675. Read: `captions.import` (`path`). Built-ins only: `effects.apply`, both transition commands, `effects.setDefaultTransition`, `mixer.addInsert`, `presets.apply`, `lumetri.applyPreset`, `essentialSound.applyPreset`. | Sequences ≤ 4096 a side and 9.4 MP, ≤ 120 fps, ≤ 96 kHz; speed 1–10,000%; durations ≤ 24 h; a call adds ≤ 5,000 elements; analyses ≤ 18,000 frames; exports ≤ 18,000 frames. |
| effect | `effect.info`, `effect.run` | 460 of 665. Built-ins only: `effect.apply`. | Comps ≤ 8.85 MP (4096 × 2160), ≤ 36,000 frames, 1–240 fps; repeater copies ≤ 1,000 (≤ 10,000 a call); about 120 effect parameters capped; the project ≤ 5,000 items, layers and effects. Expressions are `code`: a project holding one is saved, never rendered. |
| vector | `vector.info`, `vector.run` | 574 of 679. Built-ins only: `effect.apply` and `appearance.addEffect` (by `effect` or `id`); `perspective.draw` runs only `shape.*` commands, each admitted in turn. | Shapes ≤ 1,000 points; blends ≤ 1,000 steps; repeats, mosaics and grids ≤ 10,000 copies (≤ 10,000 a call); Transform effects ≤ 1,000 copies; the document ≤ 20,000 nodes and 100,000 objects as drawn; a raster `out` ≤ 8192 px a side and 16 MP. |
| sound | `sound.info`, `peaks`, `convert`, `trim`, `mix` | No door: soundcraft has no command catalog. | — |
| design | `design.info`, `render`, `export` | No door, by decision (#418). | — |
| pdf | `pdf.info`, `text`, `render`, `merge`, `split` | No door: a few fixed operations. Apps' own requests also reach PDF Tools v2's open documents (`apps/pdftools/design/SERVICE.md`): each method runs the commands reviewed for it, with arguments the service builds; `comment_add` without its attachment type or a path; `page_insert_file` reads a PDF in the app's storage; `doc_save`, `page_extract` and the exports write to a staging folder there, moved into place without replacing a file (except a save over the document's own file); `doc_open` and `form_fill` run with JavaScript off. | Apps' own requests: ≤ 8 open documents a caller; renders 24–300 dpi and ≤ 16 MP; find ≤ 500 matches; ≤ 512 pages a call; a render cache of ≤ 16 MiB and 64 files, cleared before a write would fail for room. |
| photo | none (Photos' own `photos.info`) | `photo.run`, for apps' own requests only: 692 of 817 ids, each also passing photocraft's own workspace check. | Not yet capped (apps' own requests only). |
| sheet | none (the Sheets app's own `sheets.*`) | No door: formula evaluation. | — |

The review reclassified these ids: word's `review.readAloud` (`code` to
`device`), vector's `effect.apply` and `appearance.addEffect` (`code` to
`safe`, with their effect checked), effect's two Media Browser favourites
(`safe` to `host`), effect's 14 commands that set or link expressions
(`safe` to `code`: effectcraft runs an expression with no time, step or
memory budget) and photo's `layer.smartFilter.setParams` (`safe` to
`file`: it could plant a Color Lookup file path). It also closed two routes
that no id check sees: Essential Graphics values that set an effect's LUT
file inside a precomp (effect), and a Color Lookup smart filter naming a
file (photo); both services' fences now catch them, at open and after
every command. Within its caps a call can still hold the UI thread for
seconds (a 4K export, a heavy stack of effects at their caps); moving
engine work to a worker with timeouts is #399.

`photo.run`, reached only by an app's own `host.request`, now goes through
the same gate. Each door engine's `SKILL.md` lists `info` and `run`,
explains the door and teaches commands by example; its service's tests run
every example (`the_skill_examples_run`), and the shell checks them against
the tools' schemas.

## Phone packaging, weighed per engine (9 Oct 2026)

Decision 3 kept the services out of the phone shells until each engine's
binary cost was weighed. Since #415 the ten engines behind the system
agent are desktop only. The sheet and photo engines were weighed on
Home's Android library: release builds for aarch64 with the packager step
of `rom/scripts/build-home.py` (default features, no kernel, no signing)
on main `c6fbae6f`, as is and with each engine moved behind
`craft-engines`:

| Engine | `libmakepad.so` as packaged | Stripped | APK | Crates only it brings |
| --- | --- | --- | --- | --- |
| photo | +36.1 MB | +26.8 MB | +13.2 MB | 78: photocraft, its text and font stack (parley, skrifa, harfrust), codecs (exr, tiff, WebP, a JPEG encoder), wasmi, rayon |
| sheet | +4.7 MB | +3.4 MB | +1.8 MB | 13: gridcraft, zip, quick-xml, `makepad-script-compute` |

Before, Home's library was 410.6 MB as packaged (314.7 MB stripped) and
its APK, without the kernel, 239.5 MB. The rule is per engine: a few MB
stays, more is desktop only.

- **The photo engine is desktop only.** `craft-engines` owns it with the
  ten, and `tools/check-shell-graph.sh` fails if it reaches Home's graph.
  On Home the shell's notice service answers Photos' namespace, so
  `photos.notify` works, and Photos' declared `photos.info` is refused
  before it runs, as `unavailable`: "photos.info isn't available on this
  device: the photo engine is only in the desktop build"
  (`host_tools::script_apps::unlinked_engine`). Its skill ships only where
  the engine does.
- **The sheet engine stays in Home.** The native Sheets app's agent tools
  run on it there, and the graph guard requires it wherever App Hub is
  linked.
- Both engines compile and link for `aarch64-linux-android`, with no
  warnings. Neither has run on a phone (**unverified**).

## Open questions

- Where the service crates live (`crates/craft-*` vs per-app
  `apps/<name>/host-service`) once the first one is written.
- Whether soundcraft's engine (and `audio_aot`, not yet vendored in our
  makepad fork) justifies a scriptable-effects lane this quarter.
