# Device calendar host API

English | [简体中文](README.zh-CN.md)

`device_calendar.*` gives installed App Hub apps bounded access to calendars already configured in the host OS. It uses EventKit on macOS and CalendarProvider in Android Home. It does not log into Google or create calendars. The bundled `calendar.*` service and the OAuth-backed `gcalendar.*` service keep their separate identities and storage.

This is source implementation, not a claim that an existing release contains it. A compatible App Hub contract must admit `device_calendar`, and the host must register this service. Check `runtime.list` and `runtime.describe`, then `device_calendar.permission.status`. Windows, Linux, iOS and Android packagings without the Home adapter report unsupported; discovery describes supported platforms. OS-account acceptance remains unverified until explicitly tested with a synthetic test calendar.

## Declaration and migration

Declare the `device_calendar` capability, `requires: ["host-api-v1"]`, and every used method under `host_api.required` at version `1`. For example, the relevant **manifest fragment** is:

```json
{
  "capabilities": ["device_calendar"],
  "requires": ["host-api-v1"],
  "host_api": {
    "required": {
      "device_calendar.permission.status": 1,
      "device_calendar.permission.request": 1,
      "device_calendar.calendars.list": 1,
      "device_calendar.calendars.select": 1,
      "device_calendar.events.list": 1,
      "device_calendar.events.create": 1
    }
  }
}
```

Muse's custom `calendar.*` calls are not stock OS-calendar APIs. Migrate its adapter to this family; do not rename the built-in `calendar` service or assume a contestant's custom Rust host is installed. A device calendar is also not a Google OAuth connection: use `gcalendar` when the app needs the connected Google account rather than OS calendars.

## User flow and methods

1. Request app consent in the foreground. The host explains access in its own review; physical approval may open the OS permission dialog.
2. List calendar choices and call `calendars.select` with a native `calendar_id`. A second native review shows its calendar and account. Approval returns an opaque `handle`.
3. Save the handle in this app account's storage. List/read using it. Never infer an account from user-supplied JSON.
4. Submit an exact draft with `events.create` or `events.update`. The app waits while the host shows the immutable draft, selected calendar and account. Update also shows the current event. Only a physical native approval sends the write to the OS.
5. Refresh after completion. Keep the returned event's `revision` for a later edit or deletion.

All names in this table have the `device_calendar.` prefix. Arguments are JSON objects; unknown fields are rejected.

| Method | Arguments | Result / authority |
| --- | --- | --- |
| `permission.status` | `{}` | `supported`, `app_consent`, `os_permission`; never prompts |
| `permission.request` | `{}` | Native app-consent review followed by OS permission; foreground only |
| `permission.revoke` | `{}` | Revokes this app account's consent and handles after durable completion; does not revoke the whole host's OS permission; foreground only |
| `calendars.list` | `{}` | At most 64 calendar choices, including source/account labels; requires app and OS consent |
| `calendars.select` | `calendar_id` | Native calendar/account review; returns `handle` and calendar metadata |
| `events.list` | `handle`, `start_ms`, `end_ms`, `limit` | `events` and `truncated`; at most 200 occurrences within 93 days |
| `events.get` | `handle`, `event_id` | One event plus `revision` |
| `events.create` | `handle`, `event` | Native review, then `{saved, event}` |
| `events.update` | `handle`, `event_id`, `revision`, `event` | Native review of full replacement; conflicts reject stale content |
| `events.delete` | `handle`, `event_id`, `revision` | Native deletion review, then `{deleted, event_id}` |

An event draft requires `title`, `start_ms`, `end_ms` and an actual IANA `timezone`; optional fields are `all_day` (false), `location` and `notes` (empty). Epoch values are milliseconds, from 1970 through 2100; the end is exclusive and the duration cannot exceed 93 days. Text limits are 512/2048/8192 UTF-8 bytes for title/location/notes. All-day drafts use `timezone: "UTC"` and UTC-midnight date boundaries. Native review shows civil time with the correct zone abbreviation and UTC offset, including DST, or an explicit all-day date range with exclusive end.

**Unverified interaction recipe using synthetic content:** select a dedicated test calendar; create a “Synthetic visit” at 09:00 in `America/Los_Angeles`; physically approve; list that day's events; change only the title using its returned revision; physically approve; refresh; delete using the new revision and approve again. Do not use a personal calendar for automated validation.

## Agent and safety boundary

Read-only status/list/get methods can be offered as agent tools after normal app grants. Consent, selection, revocation and mutations are foreground-only; this version has no unattended event-writing or background mutation tool. An app agent can propose a draft in its app/card and ask the human to open review. It cannot manufacture the native approval through a tool, Splash copy, synthetic click, sheet method or guessed ticket.

Admission is revalidated against the installed bundle and `host-api-v1`. A host callback supplies the active authenticated app-account scope, including its explicit accountless scope. Signed-out apps that require an account are rejected. Consent, selected handles, revisions and pending reviews bind to that scope and the app's host storage. OS calendar source identity is checked independently. Account switches and removed apps invalidate requests; selecting or revoking increments the consent revision. Revocation is durable when its callback completes. An already-started OS commit cannot be recalled.

Events with recurrence or attendees are read-only in version 1; no invitation sending, recurrence editing, reminders, calendar creation, free/busy sharing, or background scheduler is included. Android uses an atomic provider batch with reviewed-event and source assertions. EventKit checks the reviewed snapshot again immediately before committing, but its API cannot atomically compare against a concurrent external calendar editor.

## Execution and limits

Native work and consent writes run off the UI thread. The host uses its task pool; Android uses one bounded provider executor. Replies arrive through a bounded nonblocking channel. There are at most 16 outstanding jobs/reviews, two host workers, 45-second job deadlines, 300-second reviews, and replies of at most 1 MiB (256 KiB on Android’s extension bridge). Reduce list size when a reply exceeds the limit. Expired queued requests never dispatch; timers stop when no work or review remains. Close, account changes and expiry invalidate replies; Android receives cancellation IDs. Work already executing inside the OS may finish even when its reply is discarded. After an uncertain write timeout, refresh before retrying to avoid a duplicate.

The host-only consent file is atomically replaced (0600 on Unix). Normal reads use a nonblocking cache lookup and never wait for a worker's lock. First call `permission.status`: it loads at most 1 MiB of consent data on the task pool before querying OS status. Earlier data or prompt calls return an initialization error; a busy cache reports an explicit retry error. The cache admits at most 64 app roots per process. A persisted consent-write failure is returned to the caller, not reported as a successful grant/revocation.

Packaged macOS builds include Calendar usage descriptions and the Calendar entitlement. A bare binary without usage descriptions refuses a permission request instead of invoking Apple's privacy termination. Android Home declares READ_CALENDAR and WRITE_CALENDAR, prompts through an OS permission fragment, and advertises adapter availability to Rust. These declarations do not grant an individual App Hub app access.

## Source map and validation

- `mod.rs`: admission/account checks, public discovery, bounded scheduling and one-use review state.
- `model.rs`: argument limits, timezone validation, immutable event revisions.
- `store.rs`: app/account consent, opaque handles and durable revision checks.
- `prompt.rs`: native review, local-time display, trusted input and focus-loss reset.
- `macos.rs`: EventKit; `phone/resources/android/java/dev/makepad/octosense/DeviceCalendarClient.java`: CalendarProvider.

Synthetic Rust tests cover bounds, timezone/DST display, consent isolation/revocation, stale revisions, single-use review and refusal of synthetic approval. Compilation of the adapters does not establish real OS permission or provider correctness. No test here reads personal calendars or grants OS access.
