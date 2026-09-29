spec: task
name: "The octos host service for the Card runner"
tags: [ai-host, card-runner, octos, contained-apps]
estimate: 1d
---

## Intent

Let OctoScript apps that App Hub installs and the Card runner hosts use the shell's assistant: a new `octos` host service hands an app's `host.request("octos.*")` to a peer the system octos allocates for that app alone. The system agent (`_main:api:octosense#system`) already owns such a peer for Rinx; this task wires the same mechanism to the Card runner, with no change to App Hub or to app bundles. Afterwards a store app that declares `octos.*` gets a reply on a shell that hosts a kernel, instead of `no service answers "octos"`.

## Decisions

- The service implements App Hub's `HostService` with family `octos`, and `start()` in `octosense-ai-host` registers it when the shell hosts a kernel, next to `register_llm`, with `Policy.contained_apps` as its on/off state; `crates/shell` is unchanged.
- The peer id is `card.` followed by the app id (for example `card.com.example.trip`), checked with the `is_namespace_segment` character rule and at most 64 characters, so a store app can never share a native module's (for example `rinx`) peer or memory.
- One `OctosAppService` per app (created on its first call, then reused) and one handle per app on its conversation (`open_conversation`: the person's lane, a request context of the peer that shares history with the peer's own session, the system agent's lane, the two running in parallel; ADR 0004 §6), so a turn is the person's (`origin: person`) unless the app says it started it (`trigger: app`), and `octos.session.history` is both lanes' transcripts merged by time, each message with its `lane` and speaker; the account is always `device`, the instance key is `card.<app id>-g<generation>`, and a closed handle is reopened with the next generation on the next call.
- The argument rules are Rinx's: `octos.turn.start` takes only `text`, non-blank after trimming and at most 32 KiB; `octos.session.open`, `octos.session.history` and `octos.turn.interrupt` take only the empty object `{}`.
- Tool approvals: an `approval/requested` event is answered with `ContextOp::Approval { approve: false }`; a successful reply object gains `denied_approvals`, the titles of the declined tools.
- Replies are capped at 2 MiB (`MAX_REPLY_BYTES`); a larger one is an error, never truncated.
- `Policy` gains `contained_apps`: false in `Policy::shipped()` until first-use consent exists (ADR 0004 section 4; `OCTOSENSE_CONTAINED_APPS=1` turns it on for development), false in `Policy::none()`; it is the shell's switch.
- The logic does not depend on `cfg(kernel)`: peers come from a `PeerFactory`. Only the shell's factory, under `cfg(kernel)`, creates brokers through `hosted::launch`. Tests use a fake factory over App Hub's real `dispatch` and `take_replies_for`.
- `octosense-appstore` becomes a regular dependency of `octosense-ai-host` (it is already in the workspace graph).
<!-- lint-ack: decision-coverage — a dependency is a build decision, proven by `cargo check -p octosense-ai-host` passing; it has no runtime behaviour a scenario could observe -->

## Boundaries

### Allowed Changes
- crates/ai-host/**
- Cargo.lock
- specs/**
- docs/ai-services.md
- docs/ai-services.zh-CN.md
- README.md
- README.zh-CN.md

### Forbidden
- Do not change App Hub, anything under `.sources/`, or the dependency pins OctoSense locks
- Do not change the behaviour of Rinx's existing grant `allow("rinx", …)`
- Do not let an app pass or read session, profile, workspace or approval decisions
- Do not add a crate that is not already in the workspace graph

## Out of Scope

- An approval sheet for tool approvals (needs an App Hub API to open a sheet from a worker thread; a second phase)
- Limiting the tools a peer may use (needs octos `peer/prepare` support; a second phase)
- Closing a context when its app instance closes (`ServiceCall` carries no instance identity; a second phase)
- A Settings switch for `contained_apps`
- `model.complete`

## Completion Criteria

### Rule: routes-app-requests — hand each app's requests to its own peer
Scenario: A turn reaches the app's own peer and returns its reply
  Test:
    Package: octosense-ai-host
    Filter: contained_turn_reaches_app_peer_and_replies
  Given the switch is on and the fake factory gives app "com.example.trip" a context that replies "你好"
  When the app calls "octos.turn.start" through App Hub's `dispatch` with {"text": "hi"}
  Then the app receives a successful reply whose "text" is "你好"
  And the fake factory received the peer id "card.com.example.trip"
  And the context received a Turn operation with the text "hi"

Scenario: One app reuses its peer and each app has its own
  Test:
    Package: octosense-ai-host
    Filter: contained_each_app_gets_its_own_peer
  Given the switch is on
  When app "com.example.a" calls "octos.session.open" twice and app "com.example.b" calls it once
  Then the fake factory created exactly two peers, "card.com.example.a" and "card.com.example.b"

Scenario: Session calls map to their context operations
  Test:
    Package: octosense-ai-host
    Filter: contained_session_calls_map_to_context_ops
  Given the switch is on
  When the app calls "octos.session.open", "octos.session.history" and "octos.turn.interrupt", each with {}
  Then the context receives Open, History and Interrupt in that order

### Rule: refuses-bad-requests — refuse requests outside the rules
Scenario: Extra arguments are refused
  Test:
    Package: octosense-ai-host
    Filter: contained_rejects_unsupported_arguments
  When the app calls "octos.turn.start" with {"text": "hi", "session_id": "x"}
  Then the app receives the error "Unsupported Octos arguments"
  And the fake factory created no peer

Scenario: Blank text and text over 32 KiB are refused
  Test:
    Package: octosense-ai-host
    Filter: contained_rejects_empty_or_oversized_text
  When the app calls "octos.turn.start" with the text "   " and with 32769 bytes
  Then both calls receive the error "Provide text (at most 32 KiB)"

Scenario: An unknown octos method is refused
  Test:
    Package: octosense-ai-host
    Filter: contained_rejects_unknown_method
  When the app calls "octos.admin"
  Then the app receives the error "Unknown Octos service octos.admin"

Scenario: A call marked as coming from a sheet is refused
  Test:
    Package: octosense-ai-host
    Filter: contained_refuses_sheet_calls
  When an "octos.turn.start" call marked from_sheet arrives
  Then the app receives an error saying octos has no sheet

### Rule: stays-contained — containment and degraded states
Scenario: With the switch off the call is refused and no peer is made
  Test:
    Package: octosense-ai-host
    Filter: contained_switch_off_refuses_without_peer
  Given the switch is off
  When the app calls "octos.turn.start" with {"text": "hi"}
  Then the app receives the error "The assistant is turned off for apps on this device"
  And the fake factory created no peer

Scenario: Peer ids are namespaced and bounded
  Test:
    Package: octosense-ai-host
    Filter: contained_peer_ids_are_namespaced_and_bounded
  When the peer id is computed for app "rinx" and for a 60-character app id
  Then the first is "card.rinx", which differs from the native module "rinx"
  And the second is an error saying the app id is too long

Scenario: A device that cannot give a peer answers unavailable
  Test:
    Package: octosense-ai-host
    Filter: contained_unavailable_when_no_peer
  Given the fake factory gives no peer to any app
  When the app calls "octos.session.open"
  Then the app receives the error "The assistant is not available on this device"

Scenario: A closed context is reopened with the next generation
  Test:
    Package: octosense-ai-host
    Filter: contained_reopens_closed_context
  Given the app's first context closes after one call
  When the app calls "octos.session.open" again
  Then the fake service received a second open_context with the instance key "card.com.example.trip-g2"

Scenario: Tool approvals are declined and reported to the app
  Test:
    Package: octosense-ai-host
    Filter: contained_denies_tool_approvals
  Given the context raises approval/requested (id "a1", title "Run shell") during the turn and then completes with {"text": "done"}
  When the app calls "octos.turn.start"
  Then the context receives Approval with id "a1" and approve false
  And the app's reply has "denied_approvals" equal to ["Run shell"]

Scenario: A reply over 2 MiB is refused
  Test:
    Package: octosense-ai-host
    Filter: contained_rejects_oversized_reply
  Given the context completes the turn with more than 2 MiB of text
  When the app calls "octos.turn.start"
  Then the app receives an error saying the reply is over `MAX_REPLY_BYTES` (2 MiB)

Scenario: The switch's defaults
  Test:
    Package: octosense-ai-host
    Filter: policy_contained_apps_defaults
  When contained_apps is read from `Policy::shipped()` and `Policy::none()`
  Then both are false unless `OCTOSENSE_CONTAINED_APPS=1` is set
