# Mail host service

English | [简体中文](README.zh-CN.md)

Ordinary App Hub apps granted `mail` can connect their own mailbox through the
host sign-in sheet, read mail, compose a message and request native send review.
Passwords and SMTP connections remain in this service. A model or app cannot
approve sending.

## Public composers

| Method | Input | Result |
| --- | --- | --- |
| `mail.compose` | `account`, `to`, `subject`, `body`; optional `compose_id`, `expected_revision`, `folder`, `message` | Saved draft, `compose_id`, revision and status. No prompt or send. |
| `mail.compose_status` | `account`, `compose_id` | This app/account's draft and last attempt/receipt. |
| `mail.review_send` | Same fields as `mail.compose` | Foreground native review; callback settles after cancellation or submission. |
| `mail.send` | Same fields as `mail.compose` | Compatibility entry point for third-party apps; opens the same review. Never direct SMTP. |

New drafts receive a host-generated `compose_id`. Keep it and its revision.
Editing an existing draft requires `expected_revision`; a stale editor receives
`revision_conflict`. Pass the saved full fields and revision when requesting
review. Do not create a new composer automatically after a timeout or delivery
error: inspect `mail.compose_status` first.

Draft preparation and submission share a limit of 16 workers. The native sheet
opens immediately with approval disabled while preparation runs off the UI
thread. A bounded channel delivers the exact snapshot and result; the UI polls
without waiting on draft I/O.

The review shows app identity, account, From, To, subject and the complete body.
Only a physical pointer/touch press **and** release on the native **Approve &
Send** control authorizes that immutable revision. Instrumentation, synthetic
clicks, script copies of the review widget and agent tools cannot approve it.
Send review is currently available on macOS and Android; Linux/Windows can
compose/read status but have no authenticated physical-send adapter.

The send callback returns `accepted: true`, `status: "accepted"` and `id` only
after the transport reports SMTP acceptance (`id` is the outbound Message-ID). This is not proof of delivery to
the recipient. Cancellation and uncertain/failed submission return errors;
the durable status remains available. An uncertain attempt is never retried
automatically. This first public API does not offer a retry-after-failure method.
Repeated review/approval of the same attempt cannot call SMTP twice.

Storage is limited to 128 composer records and 64 MiB per app/account (512 KiB
per record, including up to 32 attempts). `resource_limit` refuses a new record
at capacity; editing a known draft still works, and space is reserved for its
bounded final receipt. Accepted/uncertain records are retained, never pruned to
make automatic retries possible. Removing this app's mailbox grant deletes its
composers without deleting another app's records. There is no discard API yet.

Limits: one plain-text To address, subject up to 512 UTF-8 bytes, body up to
8192 UTF-8 bytes. Cc/Bcc, attachments, arbitrary headers and sender overrides
are refused. For replies, optional `folder`/`message` identify a cached source;
the host derives reply headers and preserves that source across edits.

## Migration example

This illustrative Splash integration has not been run as a contestant bundle:

```splash
let compose_id = ""
let revision = 0
fn save_reply(account, to, subject, body){
    let args = {account: account, to: to, subject: subject, body: body}
    if compose_id != "" {
        args.compose_id = compose_id
        args.expected_revision = revision
    }
    host.request("mail.compose", args, fn(r){
        if r.is_ok { compose_id = r.data.compose_id; revision = r.data.revision }
        // Show errors without discarding the person's unsaved text.
    })
}
fn review_reply(account, to, subject, body){
    host.request("mail.review_send", {
        account: account, to: to, subject: subject, body: body,
        compose_id: compose_id, expected_revision: revision
    }, fn(r){
        // Report sent only when r.is_ok AND r.data.accepted == true.
        // On error, read mail.compose_status before offering another action.
    })
}
```

Declare capability `mail`. A new app can require the exact API majors through
`host_api.required: {"mail.compose": 1, "mail.compose_status": 1,
"mail.review_send": 1}`. Agent tools may map to `mail.compose` and
`mail.compose_status` when supported by the consumer's contract. Account-aware
agents declare `storage.accounts: true`; the shell binds tool aliases to the
owner's current account and rejects stale conversations/account arguments.
`mail.send` and `mail.review_send` remain foreground-only. Agent results clip
long draft text and mark truncation; native review always shows the full text.

## Ownership and validation

Public composer IDs include app and account in their host-side derivation;
records live outside app storage. Sharing an account grant does not share
composers. Historical `mail.propose_reply`, `mail.draft`, `mail.suggest_reply`,
`mail.propose_send` and Mail's existing card/editor remain `os.mail`-only.
At the send claim, the shell rechecks installed-app admission, the `mail`
grant, account selection and suspension; the service rechecks account access,
revision, payload, sender and one-use attempt state. Closing the originating
isolate invalidates an unsubmitted review. Review capabilities expire after
ten minutes and are not serialized or exposed to scripts.

The synthetic service tests use a fake transport and isolated mailbox data;
they do not send email. They cover cross-app/account refusal, revision changes,
expired and revoked reviews, uncertain outcomes, untrusted gestures and
single-attempt submission. Native UI and real SMTP acceptance on hardware
remain unverified for this change.
