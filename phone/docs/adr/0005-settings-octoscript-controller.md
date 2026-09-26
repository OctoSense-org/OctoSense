# ADR 0005: Settings application logic in Octoscript

Date: 2026-09-25

Status: Implemented in source; emulator acceptance pending

## Reason

The user requires the Settings app itself to be authored in Octoscript. ADR 0004
put its layout in Octoscript but retained navigation, interaction state, drafts,
review flows, and event handling in Rust. That implementation does not meet the
requested application boundary. This decision replaces that part of ADR 0004.

## Application boundary

Bundled Octoscript modules own navigation, search, labels, presentation choices,
selection, local drafts, review/cancel behavior, event handlers, and the requests
resulting from those handlers. They consume validated observations as data.
Domain controllers must contain the actual application decisions; a script that
forwards widget IDs to the former Rust controller does not count as a port.

Native code retains the renderer and widget/input/accessibility mechanics, data
conversion, finite Android bindings, request lifetime/caller checks, and Android
policy enforcement. Android still owns credential, permission, biometric and
provider consent flows. Moving application logic into script never broadens the
authority available to an ordinary OctoSense app.

## Controller protocol

A persistent renderer-free VM compiles the bundled controller once. It installs
no Android, widget, network, filesystem, shell, or tool functions. Pure Unicode
text helpers are available for application search. The Makepad widget tree stays
mounted, preserving native text-input, caret, focus, scroll, and theme behavior.

`settings_new()` returns a plain initial state record.
`settings_step(state, event, observed)` returns exactly:

```text
{state: <plain record>, patch: <array>, requests: <array>, handled: <boolean>}
```

Events describe input, press/release/click, navigation/Back, external entry,
observations, operation results, lifecycle, and theme changes. Data is allocated
directly in the VM; observation strings are never interpolated into source.
Unbounded native integer identifiers cross as decimal strings. Opaque native
keys retain their exact bytes.

Patches have a finite presentation vocabulary: text, input, enabled, visible,
scroll, focus, blur, semantic identity, accessible label and bounded RGBA colors. Targets must exist
in the bundled layout. An explicit input patch is distinct from a label update;
observations cannot casually replace the active editor's text. Each request is
decoded into a finite native operation and checked against current authority.
No generic Android key/value, component, intent, Binder or shell API is added.

The host imports fresh copies of the last committed state and event data for
each transition. It validates the entire returned state, patch and request batch
before committing anything. Failure leaves committed state unchanged, discards
the VM (including mutated captured closures), and never retries the event.
Execution, stack, heap, string, output depth/size, and batch lengths are bounded.
Cycles and non-data values fail instead of producing truncated output.

## Migration and acceptance

Existing native service implementations and their acceptance evidence remain
valid for their own boundary. They do not establish acceptance of the new script
controller. The live Settings view now calls the script controller. The old Rust page
handlers and native UI-controller modules have been removed; the native search
catalog remains only as a test oracle. Source cutover and device acceptance
are tracked separately. New parity features are not declared finished merely
because the controller language changed.

Completion requires every existing application controller to execute from
Octoscript, removal of the old Rust application decision paths, no lost controls,
and emulator tests of navigation, real operations/readback, held-target changes,
review/cancel, lifecycle, stable editors, shared themes, and accessibility. Pure
runtime tests must prove that invalid output, script errors, exhausted budgets,
and denied requests cannot commit state or effects. No physical phone operation
is authorized by this port; the current validation scope is emulator-only.

The original source port passed 495 host tests with embedded mobile apps enabled and 31
Java/repository Settings checks. Its standalone ARM64 development APK `2026092540`
has been built and signed. Installation and Android acceptance remain pending
because this session cannot create ADB's local listener. The source inventory
and exact validation scope are recorded in
[the migration inventory](../android/settings-octoscript-logic-inventory.md).
PR #21's subsequent main rebase and validation are recorded separately there;
the original APK is not acceptance evidence for the rebased source.
