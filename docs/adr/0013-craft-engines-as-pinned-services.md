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
  engine's area `<apps root>/.host/<family>`; answers and errors name files
  relative to it, never by the host's own paths. A virtual owner is not an app:
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
  separate review. The `commands` catalogs of design, effect and vector are
  not declared, because they answer JSON arrays and octos takes object
  results only.
- **Files are the open gap.** Decision 5 expected file access through the
  existing files host tools. They do not reach the engines' areas, and
  neither does the system agent's workspace. Until a reviewed staging path
  exists, an engine sees only what its own tools wrote (`word.new`,
  `deck.new` and the conversions), and every tool description says its
  paths are relative to its engine's workspace. designcraft resolves
  data-merge sources and vectorcraft linked images by the paths inside the
  documents they open, so a staging path must not admit outside documents
  to those two until that is contained.
- **The kernel's cap.** octos takes at most 64 host tools in one
  registration and refuses a larger set whole. With the engines, the system
  session's largest set is 63. The shell offers the engine tools last and
  cuts them first past the cap, and a test keeps the whole grant within it.
- **Long calls.** An engine call runs on the thread that dispatches it
  (the shell's UI thread for a tool call) and holds App Hub's service
  registry while it runs, as app calls to the engines always have. A long
  export stalls the shell for its duration. The kernel waits 30 s for a
  host tool's answer (an act call then ends with an unknown outcome), and
  App Hub times the request out after 60 s. Moving engine work to a worker
  with per-method timeouts is follow-up work.

## Open questions

- Where the service crates live (`crates/craft-*` vs per-app
  `apps/<name>/host-service`) once the first one is written.
- Whether soundcraft's engine (and `audio_aot`, not yet vendored in our
  makepad fork) justifies a scriptable-effects lane this quarter.
- Phone packaging. Since 9 Oct 2026 the ten engines behind the system
  agent (word, deck, cad, light, sound, design, film, effect, vector, pdf)
  are desktop only: the desktop-default feature `craft-engines` owns them,
  and `tools/check-shell-graph.sh` fails if one reaches Home's graph. The
  sheet and photo engines still ship in Home, because the native Sheets
  app's and Photos' agent tools run on them there. Whether to keep them
  (their binary cost, not yet weighed) or move them desktop-only too (and
  withdraw those tools on the phone) is still open.
