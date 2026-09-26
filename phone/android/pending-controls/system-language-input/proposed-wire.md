# Proposed finite system language boundary

Proposal only; no Binder slots reserved and no production integration in2534.
Names can be aligned with the parent-owned transport before implementation.

Snapshot input: positive `request_id`, optional observed `key`, optional observed
`parent`, bounded local `query` <=80 codepoints, aligned `offset`20. Opaque values
are64 lowercase hex. A root request may establish a new key. A retained branch
requires the exact catalog/key; it must fail stale rather than silently retarget.

Snapshot payload `launcher.system_languages_state`:

```
{
  schema: 1,
  request_id: positive_i64,
  availability: available | restricted | unavailable | unsupported,
  key: opaque_key | null,
  can_apply: boolean,
  current: [{ target, label, native_label, tag, translated, primary }],
  parent: opaque_branch | null,
  query: bounded_string,
  offset: nonnegative_multiple_of_20,
  total: bounded_count,
  rows: [{ target, label, native_label, tag?, kind: language | region | numbering | leaf }]
}
```

`current` carries the complete authoritative ordered list, including custom
Unicode extensions; `rows` is a20-row view into a bounded native hierarchy.
Neither raw language tags nor row labels are accepted back as targets. Malformed
native state is unavailable; it is not an empty current list. The last language
cannot be removed. Avoid publishing duplicate selected leaves.

The mutation is `system_languages_apply(key, ordered_targets[])`. It contains
only unique observed current-row/leaf targets from the bound catalog, never a tag
or arbitrary configuration. Omitted current targets mean reviewed removal;
appended leaves mean reviewed additions. At least one and at most the documented
native-safe bound are required. Re-resolve every target against a fresh native
current list/catalog and exact restrictions before claiming the key. Consume
once before invoking native LocalePicker.updateLocales. No automatic replay.

Return `languages_applied` only after exact Configuration LocaleList equality,
including order and Unicode extensions; `languages_requested` when the native
operation was accepted but readback is pending; `languages_target_changed`,
`languages_restricted`, `languages_unavailable` or `languages_unconfirmed`
otherwise. The host preserves the reviewed draft on rejection/timeouts and
allows an explicit fresh read. A changed post-write configuration cannot itself
turn a successful pending request into a false stale-key failure.

The key must bind current user, policy/authority, the full current list,
LOCALE_PREFERENCES, framework catalog semantics and a bounded monotonic lifetime.
No lease renewal from polls. A current primary replacement triggers built-in
review before enqueue; the warning names both old/new display languages and
explains immediate UI/configuration changes. RTL is a real layout transition,
not a test-only locale string.

Backend separation:

- Pure `SystemLanguageSettings` owns bounded catalog/lease/draft validation and
  one-use state, with injectable native provider and monotonic clock.
- Broker native adapter owns SystemLocaleCollector/LocaleStore hierarchy,
  LocalePicker update and exact Configuration readback.
- Agent and Home clients retain the observed route and capabilities, never retry
  a failed mutation through an alternate backend.
- Dedicated Rust model/host/UI owns current ordering, native hierarchy paging,
  stable row identities, review/Cancel and nested Back. No provider-generated UI.

Unit checks before staging: language→region→numbering traversal, deterministic
native sort/hash ties, complete custom current extensions, add/remove/reorder,
last-language denial, duplicate/forged targets, flags/restriction/user change,
empty/malformed config, expired/double claim, partial catalog overflow, exact
applied versus pending readback and no replay after process/config changes.
