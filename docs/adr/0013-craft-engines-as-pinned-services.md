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

## Open questions

- Where the service crates live (`crates/craft-*` vs per-app
  `apps/<name>/host-service`) once the first one is written.
- Whether soundcraft's engine (and `audio_aot`, not yet vendored in our
  makepad fork) justifies a scriptable-effects lane this quarter.
- Phone packaging: which engines, if any, ship in Home rather than
  desktop-only.
