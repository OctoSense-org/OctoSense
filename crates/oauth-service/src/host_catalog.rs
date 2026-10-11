//! Descriptions of executable app-facing methods. Host sheet control methods
//! intentionally never enter discovery or app compatibility requirements.
use octosense_appstore::services::{AgentAccess, HostApiMethod};
use serde_json::{json, Value};

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn string() -> Value {
    json!({"type":"string","minLength":1})
}
fn input(fields: &[&str], optional: &[(&str, Value)]) -> Value {
    let mut properties = serde_json::Map::new();
    for field in fields {
        properties.insert((*field).into(), string());
    }
    for (field, schema) in optional {
        properties.insert((*field).into(), schema.clone());
    }
    object(Value::Object(properties), fields)
}
fn requiring(mut schema: Value, fields: &[&str]) -> Value {
    let required = schema["required"].as_array_mut().unwrap();
    required.extend(fields.iter().map(|field| json!(field)));
    schema
}
fn github_file() -> Value {
    requiring(
        input(
            &["owner", "repo", "branch", "path", "message"],
            &[
                ("content", json!({"type":"string","maxLength":1048576})),
                (
                    "sha",
                    json!({"type":["string","null"],"pattern":"^[a-fA-F0-9]{40}$"}),
                ),
            ],
        ),
        &["content"],
    )
}
fn event_time() -> Value {
    json!({"type":"object","properties":{"date":{"type":"string","format":"date"},
        "dateTime":{"type":"string","format":"date-time"},"timeZone":string()},
        "additionalProperties":false,"oneOf":[{"required":["date"]},{"required":["dateTime"]}]})
}
fn event() -> Value {
    requiring(
        input(
            &["summary"],
            &[
                ("description", json!({"type":"string","maxLength":16384})),
                ("location", json!({"type":"string","maxLength":2048})),
                ("start", event_time()),
                ("end", event_time()),
            ],
        ),
        &["description", "location", "start", "end"],
    )
}
fn editor() -> Value {
    let strings = [
        "summary",
        "description",
        "location",
        "start_date",
        "start_time",
        "end_date",
        "end_time",
        "timezone",
    ];
    let mut properties = serde_json::Map::new();
    for field in strings {
        properties.insert(field.into(), json!({"type":"string"}));
    }
    properties.insert("all_day".into(), json!({"type":"boolean"}));
    let mut fields = strings.to_vec();
    fields.push("all_day");
    object(Value::Object(properties), &fields)
}
fn method(
    name: &str,
    summary: &str,
    args: Value,
    result: Value,
    agent: AgentAccess,
) -> HostApiMethod {
    HostApiMethod::new(
        name,
        1,
        name.split('.').next().unwrap(),
        summary,
        args,
        result,
    )
    // Native account/vault and external-browser routes exist on both desktop
    // targets too. Do not hide read/local-draft methods from their agents.
    // Write-review methods retain the physical-approval platform boundary;
    // a separate OS-authenticated approval adapter must establish the others.
    .with_platforms(
        if matches!(
            name,
            "github.review_save" | "gcalendar.review_save" | "gmail.draft.review"
        ) {
            &["macos", "android"]
        } else {
            &["macos", "android", "linux", "windows"]
        },
    )
    .with_agent_access(agent)
}
fn result() -> Value {
    json!({"type":"object"})
}
fn list() -> Value {
    json!({"type":"array","items":{"type":"object"}})
}
fn connection() -> Value {
    object(
        json!({"handle":string(),"app_id":string(),"provider":{"enum":["github","google","backend"]},
        "subject":string(),"label":string(),"scopes":{"type":"array","items":string()},
        "expires_at":{"type":["integer","null"]},"backend_id":string()}),
        &["handle", "app_id", "provider", "subject", "label", "scopes"],
    )
}
pub(crate) fn auth() -> Vec<HostApiMethod> {
    use AgentAccess::{Allowed, ForegroundOnly};
    vec![
        method("auth.connect", "Open host-owned sign-in; provider registration and platform support are checked before prompting", object(json!({
            "provider":{"enum":["github","google","backend"]},"scopes":{"type":"array","items":string()},
            "presentation":{"enum":["browser","webview"]}}), &["provider","scopes"]), connection(), ForegroundOnly),
        method("auth.accounts", "List this app's connected accounts without credentials", input(&[], &[]), json!({"type":"array","items":connection()}), Allowed),
        method("auth.active", "Return this app's selected connection or null", input(&[], &[]), json!({"anyOf":[connection(),{"type":"null"}]}), Allowed),
        method("auth.select", "Select an app-owned account and invalidate pending authorizations", input(&["connection"], &[]), connection(), ForegroundOnly),
        method("auth.disconnect", "Revoke the local connection, then attempt backend remote logout", input(&["connection"], &[]), result(), ForegroundOnly),
        method("auth.backend.me", "Read this app's active backend identity", input(&["connection"], &[]), object(json!({"connection":string(),"backend_id":string(),"identity":object(json!({"sub":string(),"label":string()}), &["sub","label"])}), &["connection","backend_id","identity"]), Allowed),
        method("auth.backend.request", "Execute a declared backend operation; GET can run in background, mutations require native physical review", input(&["connection","operation"], &[
            ("query",json!({"type":"object","maxProperties":32,"additionalProperties":{"type":"string","maxLength":2048}})),
            ("body",json!({"description":"Operation JSON body, at most 64 KiB; omitted for GET"}))]), json!({"description":"Declared endpoint JSON result, at most 64 KiB"}), Allowed),
    ]
}
pub(crate) fn connector(family: &str, native_review: bool) -> Vec<HostApiMethod> {
    use AgentAccess::{Allowed, ForegroundOnly};
    let mut methods = match family {
        "github" => vec![
            method(
                "github.repositories",
                "List repositories available to the selected GitHub account",
                input(
                    &["connection"],
                    &[("page", json!({"type":"integer","minimum":1,"maximum":1000}))],
                ),
                list(),
                Allowed,
            ),
            method(
                "github.files",
                "List one repository directory without download credentials",
                input(
                    &["connection", "owner", "repo", "branch"],
                    &[("path", json!({"type":"string"}))],
                ),
                result(),
                Allowed,
            ),
            method(
                "github.read",
                "Read a Markdown file smaller than 1 MiB",
                input(&["connection", "owner", "repo", "branch", "path"], &[]),
                result(),
                Allowed,
            ),
        ],
        "gcalendar" => vec![
            method(
                "gcalendar.calendars",
                "List calendars accessible to the selected Google account",
                input(&["connection"], &[("page_token", string())]),
                result(),
                Allowed,
            ),
            method(
                "gcalendar.cached",
                "Read the bounded local agenda for this account and calendar",
                input(&["connection", "calendar"], &[]),
                result(),
                Allowed,
            ),
            method(
                "gcalendar.refresh",
                "Refresh the bounded agenda; failed pages preserve its previous snapshot",
                input(&["connection", "calendar"], &[]),
                result(),
                Allowed,
            ),
            method(
                "gcalendar.get",
                "Read one saved Google Calendar event",
                input(&["connection", "calendar", "event_id"], &[]),
                result(),
                Allowed,
            ),
            method(
                "gcalendar.prepare",
                "Convert local editor dates and timezone into a validated event draft",
                editor(),
                result(),
                Allowed,
            ),
        ],
        _ => vec![],
    };
    if native_review {
        match family {
            "github" => methods.push(method(
                "github.review_save",
                "Review an immutable Markdown commit; only physical native approval can save",
                requiring(
                    input(&["connection"], &[("file", github_file())]),
                    &["file"],
                ),
                result(),
                ForegroundOnly,
            )),
            "gcalendar" => methods.push(method(
                "gcalendar.review_save",
                "Review an immutable event create/update; only physical native approval can save",
                requiring(
                    input(
                        &["connection", "calendar"],
                        &[
                            ("event", event()),
                            ("event_id", string()),
                            ("etag", string()),
                        ],
                    ),
                    &["event"],
                ),
                result(),
                ForegroundOnly,
            )),
            _ => {}
        }
    }
    methods
}
pub(crate) fn gmail(native_review: bool) -> Vec<HostApiMethod> {
    use AgentAccess::{Allowed, ForegroundOnly};
    let mut methods = vec![
        method(
            "gmail.labels",
            "List this account's Gmail labels",
            input(&["connection"], &[]),
            result(),
            Allowed,
        ),
        method(
            "gmail.messages",
            "List messages in a label, default INBOX",
            input(
                &["connection"],
                &[("label", string()), ("page_token", string())],
            ),
            result(),
            Allowed,
        ),
        method(
            "gmail.message",
            "Read one message with attachment notices",
            input(&["connection", "message_id"], &[]),
            result(),
            Allowed,
        ),
        method(
            "gmail.draft.open",
            "Create or reopen a local reply draft; never send it",
            input(&["connection", "message_id"], &[]),
            result(),
            Allowed,
        ),
        method(
            "gmail.draft.get",
            "Read a saved local draft",
            input(&["connection", "draft"], &[]),
            result(),
            Allowed,
        ),
        method(
            "gmail.draft.edit",
            "Update a local draft at its current revision; invalidates earlier review",
            object(
                json!({"connection":string(),"draft":string(),"revision":{"type":"integer","minimum":1},"to":string(),"subject":{"type":"string"},"body":{"type":"string"},"provenance":string()}),
                &["connection", "draft", "revision", "to", "subject", "body"],
            ),
            result(),
            Allowed,
        ),
        method(
            "gmail.event.status",
            "Read the app's recorded decision for one incoming event",
            input(&["connection", "event_id"], &[]),
            result(),
            Allowed,
        ),
        method(
            "gmail.events.status",
            "Read the app's durable incoming-event queue status",
            input(&["connection"], &[]),
            result(),
            Allowed,
        ),
        method(
            "gmail.event.decide",
            "Record quiet or verified Glance publication for an incoming event",
            input(&["connection", "event_id", "decision", "reason"], &[]),
            result(),
            Allowed,
        ),
    ];
    if native_review {
        methods.push(method(
            "gmail.draft.review",
            "Review the exact saved draft; physical native approval is required to send",
            requiring(
                input(
                    &["connection", "draft"],
                    &[("revision", json!({"type":"integer","minimum":1}))],
                ),
                &["revision"],
            ),
            result(),
            ForegroundOnly,
        ));
    }
    methods
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_accounts_reads_and_local_drafts_are_discoverable_without_widening_write_approval() {
        let methods = auth()
            .into_iter()
            .chain(connector("github", true))
            .chain(connector("gcalendar", true))
            .chain(gmail(true));
        let mut count = 0;
        for method in methods {
            let review = matches!(
                method.name.as_str(),
                "github.review_save" | "gcalendar.review_save" | "gmail.draft.review"
            );
            for platform in ["linux", "windows"] {
                assert_eq!(
                    method.platforms.iter().any(|p| p == platform),
                    !review,
                    "{} on {platform}",
                    method.name
                );
            }
            if method.name == "auth.backend.request" {
                assert!(method.platforms.iter().any(|p| p == "linux"));
            }
            count += 1;
        }
        assert!(count > 20);
    }
    #[test]
    fn descriptors_are_unique_valid_and_exclude_private_sheet_controls() {
        let methods = auth()
            .into_iter()
            .chain(connector("github", true))
            .chain(connector("gcalendar", true))
            .chain(gmail(true));
        let mut names = std::collections::BTreeSet::new();
        for method in methods {
            method.validate().unwrap();
            assert!(names.insert(method.name.clone()));
            assert!(!method.name.contains(".sheet."));
        }
        assert!(!connector("github", false)
            .iter()
            .any(|m| m.name.ends_with("review_save")));
        assert!(!gmail(false)
            .iter()
            .any(|m| m.name.ends_with("draft.review")));
    }
}
