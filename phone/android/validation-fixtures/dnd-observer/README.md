# Synthetic DND observer

Disposable API35 instrumentation observes native default/consolidated notification
policy and all automatic rules. Complete Parcelable encodings allow the runner to
verify unedited fields without depending on a lossy UI projection. Observation
briefly adopts shell `MANAGE_NOTIFICATIONS`; it never calls a policy/rule setter.

An optional notification listener reports ranking only for this fixture's six
synthetic notices (alarm, event, reminder, message, call category, ordinary status).
It reads no unrelated notification contents. These are local notices, not real
calls, texts, alarms or calendar events. Ranking acceptance is distinct from
audible delivery, caller affinity, repeat-call history and conversations.

The runner must guard the disposable emulator, capture existing policy/rules and
notification-listener grants, grant only this synthetic publisher's notification
permission and temporary listener access, and remove both grant and package in
`finally`. No network, contacts, calendar, sensors or system-policy writes occur
inside this fixture. Built-in UI mutations and exact native restoration belong
to the separate Settings acceptance scenario.
