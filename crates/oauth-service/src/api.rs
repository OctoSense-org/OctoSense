//! Provider connectors. Only host code constructs this client after consent.
use crate::{
    authorize::refresh_google,
    providers::ClientRegistration,
    store::Connections,
    transport::{json_ok, Body, Request, Transport},
    Provider,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use url::Url;

pub const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar.events";
pub const CALENDAR_LIST_SCOPE: &str =
    "https://www.googleapis.com/auth/calendar.calendarlist.readonly";
pub const GMAIL_READ_SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
pub const GMAIL_SEND_SCOPE: &str = "https://www.googleapis.com/auth/gmail.send";

pub struct Api<'a> {
    pub connections: &'a mut Connections,
    pub transport: &'a dyn Transport,
    pub google_client: Option<&'a ClientRegistration>,
    pub google_client_secret: Option<&'a str>,
    pub now: u64,
}

impl Api<'_> {
    pub(crate) fn request(
        &mut self,
        caller: &str,
        connection: &str,
        provider: Provider,
        scope: &str,
        method: &'static str,
        url: Url,
        body: Body,
        etag: Option<&str>,
    ) -> Result<crate::transport::Response, String> {
        let mut token = self
            .connections
            .tokens(caller, connection, provider, scope)?;
        if token
            .expires_at
            .is_some_and(|at| at <= self.now.saturating_add(60))
        {
            if provider != Provider::Google {
                return Err("Sign in again: GitHub authorization expired".into());
            }
            let client = self
                .google_client
                .ok_or("Google OAuth is not configured on this host")?;
            token = refresh_google(
                client,
                self.google_client_secret,
                token,
                self.transport,
                self.now,
            )?;
            self.connections.replace_tokens(caller, connection, token)?;
            token = self
                .connections
                .tokens(caller, connection, provider, scope)?;
        }
        self.transport.send(Request {
            method,
            url,
            bearer: Some(token.access),
            if_match: etag.map(str::to_string),
            body,
        })
    }
    fn github_scope(&self, caller: &str, connection: &str) -> Result<&'static str, String> {
        if self
            .connections
            .authorized(caller, connection, Provider::Github, "repo")
            .is_ok()
        {
            Ok("repo")
        } else {
            self.connections
                .authorized(caller, connection, Provider::Github, "public_repo")?;
            Ok("public_repo")
        }
    }
    pub fn github_repositories(
        &mut self,
        caller: &str,
        connection: &str,
        page: u32,
    ) -> Result<Value, String> {
        let scope = self.github_scope(caller, connection)?;
        let mut url = endpoint("https://api.github.com", &["user", "repos"])?;
        url.query_pairs_mut().extend_pairs([
            ("per_page", "50"),
            ("sort", "updated"),
            ("affiliation", "owner,collaborator,organization_member"),
            ("page", &page.max(1).to_string()),
        ]);
        json_ok(self.request(
            caller,
            connection,
            Provider::Github,
            scope,
            "GET",
            url,
            Body::Empty,
            None,
        )?)
    }
    pub fn github_read(
        &mut self,
        caller: &str,
        connection: &str,
        owner: &str,
        repo: &str,
        branch: &str,
        path: &str,
    ) -> Result<Value, String> {
        let scope = self.github_scope(caller, connection)?;
        let mut url = github_file(owner, repo, path)?;
        nonempty(branch, 255, "branch")?;
        url.query_pairs_mut().append_pair("ref", branch);
        let body = json_ok(self.request(
            caller,
            connection,
            Provider::Github,
            scope,
            "GET",
            url,
            Body::Empty,
            None,
        )?)?;
        if body["type"] != "file" || body["encoding"] != "base64" {
            return Err("Select a Markdown file smaller than 1 MiB".into());
        }
        let encoded = body["content"]
            .as_str()
            .ok_or("GitHub returned no file content")?
            .replace(['\n', '\r'], "");
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| "Invalid GitHub file encoding")?;
        if bytes.len() > 1024 * 1024 {
            return Err("Markdown file exceeds 1 MiB".into());
        }
        let text =
            String::from_utf8(bytes).map_err(|_| "The selected file is not UTF-8 Markdown")?;
        Ok(json!({"path":path,"branch":branch,"sha":body["sha"],"content":text}))
    }
    pub fn github_files(
        &mut self,
        caller: &str,
        connection: &str,
        owner: &str,
        repo: &str,
        branch: &str,
        directory: &str,
    ) -> Result<Value, String> {
        let scope = self.github_scope(caller, connection)?;
        let mut url = github_path(owner, repo, directory, true)?;
        nonempty(branch, 255, "branch")?;
        url.query_pairs_mut().append_pair("ref", branch);
        let response = json_ok(self.request(caller, connection, Provider::Github, scope,
            "GET", url, Body::Empty, None)?)?;
        let entries = response.as_array().ok_or("Choose a repository folder")?;
        // Do not expose download URLs (which can embed temporary credentials),
        // symlink targets, or unrelated provider account metadata to the app.
        let files: Vec<_> = entries.iter().filter(|entry| {
            entry["type"] == "dir" || (entry["type"] == "file" &&
                entry["path"].as_str().is_some_and(|path| path.to_ascii_lowercase().ends_with(".md")))
        }).map(|entry| json!({"name":entry["name"], "path":entry["path"],
            "type":entry["type"], "sha":entry["sha"]})).collect();
        Ok(json!({"files":files,"path":directory,"branch":branch}))
    }
    /// The caller must pass the exact host-reviewed snapshot; never retry an
    /// uncertain write without fetching the resulting remote state first.
    pub fn github_save(
        &mut self,
        caller: &str,
        connection: &str,
        file: &GithubFile,
    ) -> Result<Value, String> {
        file.validate()?;
        let scope = self.github_scope(caller, connection)?;
        let url = github_file(&file.owner, &file.repo, &file.path)?;
        let mut body = json!({"message":file.message,"content":STANDARD.encode(file.content.as_bytes()),"branch":file.branch});
        if let Some(sha) = &file.sha {
            body["sha"] = json!(sha);
        }
        let result = json_ok(self.request(
            caller,
            connection,
            Provider::Github,
            scope,
            "PUT",
            url,
            Body::Json(body),
            None,
        )?)?;
        let commit = result["commit"]["sha"]
            .as_str()
            .ok_or("GitHub did not confirm a commit; reload before retrying")?;
        Ok(
            json!({"commit_sha":commit,"content_sha":result["content"]["sha"],"url":result["commit"]["html_url"],"path":file.path,"branch":file.branch}),
        )
    }
    pub fn calendars(
        &mut self,
        caller: &str,
        connection: &str,
        page: Option<&str>,
    ) -> Result<Value, String> {
        let mut url = endpoint(
            "https://www.googleapis.com",
            &["calendar", "v3", "users", "me", "calendarList"],
        )?;
        url.query_pairs_mut().append_pair("maxResults", "100");
        if let Some(page) = page {
            nonempty(page, 4096, "page token")?;
            url.query_pairs_mut().append_pair("pageToken", page);
        }
        json_ok(self.request(
            caller,
            connection,
            Provider::Google,
            CALENDAR_LIST_SCOPE,
            "GET",
            url,
            Body::Empty,
            None,
        )?)
    }
    /// Follow pages with the same sync token. Commit nextSyncToken only after
    /// the final page has been applied atomically to the local cache.
    pub fn calendar_sync(
        &mut self,
        caller: &str,
        connection: &str,
        calendar: &str,
        sync: Option<&str>,
        page: Option<&str>,
    ) -> Result<Value, String> {
        nonempty(calendar, 1024, "calendar")?;
        let mut url = endpoint(
            "https://www.googleapis.com",
            &["calendar", "v3", "calendars", calendar, "events"],
        )?;
        url.query_pairs_mut()
            .extend_pairs([("maxResults", "250"), ("showDeleted", "true")]);
        if let Some(sync) = sync {
            nonempty(sync, 4096, "sync token")?;
            url.query_pairs_mut().append_pair("syncToken", sync);
        }
        if let Some(page) = page {
            nonempty(page, 4096, "page token")?;
            url.query_pairs_mut().append_pair("pageToken", page);
        }
        let response = self.request(
            caller,
            connection,
            Provider::Google,
            CALENDAR_SCOPE,
            "GET",
            url,
            Body::Empty,
            None,
        )?;
        if response.status == 410 {
            return Ok(json!({"reset_required":true}));
        }
        json_ok(response)
    }
    pub fn calendar_get(
        &mut self,
        caller: &str,
        connection: &str,
        calendar: &str,
        event: &str,
    ) -> Result<Value, String> {
        let url = event_url(calendar, event)?;
        json_ok(self.request(
            caller,
            connection,
            Provider::Google,
            CALENDAR_SCOPE,
            "GET",
            url,
            Body::Empty,
            None,
        )?)
    }
    pub fn calendar_save(
        &mut self,
        caller: &str,
        connection: &str,
        calendar: &str,
        draft: &CalendarEvent,
        existing: Option<(&str, &str)>,
        create_id: &str,
    ) -> Result<Value, String> {
        draft.validate()?;
        let mut body = serde_json::to_value(draft).map_err(|_| "Invalid event draft")?;
        let (method, mut url, etag) = if let Some((id, etag)) = existing {
            nonempty(etag, 1024, "event revision")?;
            ("PATCH", event_url(calendar, id)?, Some(etag))
        } else {
            if !(5..=1024).contains(&create_id.len())
                || !create_id
                    .bytes()
                    .all(|c| matches!(c,b'0'..=b'9'|b'a'..=b'v'))
            {
                return Err("Invalid stable event ID".into());
            }
            body["id"] = json!(create_id);
            nonempty(calendar, 1024, "calendar")?;
            (
                "POST",
                endpoint(
                    "https://www.googleapis.com",
                    &["calendar", "v3", "calendars", calendar, "events"],
                )?,
                None,
            )
        };
        // Invitation emails require their own explicit workflow; this sample
        // edits the selected calendar without silently mailing attendees.
        url.query_pairs_mut().append_pair("sendUpdates", "none");
        let result = json_ok(self.request(
            caller,
            connection,
            Provider::Google,
            CALENDAR_SCOPE,
            method,
            url,
            Body::Json(body),
            etag,
        )?)?;
        if result["id"].as_str().is_none() || result["etag"].as_str().is_none() {
            return Err("Google did not confirm the event; refresh before retrying".into());
        }
        Ok(result)
    }
    pub fn gmail_messages(
        &mut self,
        caller: &str,
        connection: &str,
        page: Option<&str>,
    ) -> Result<Value, String> {
        let mut url = endpoint(
            "https://gmail.googleapis.com",
            &["gmail", "v1", "users", "me", "messages"],
        )?;
        url.query_pairs_mut()
            .extend_pairs([("maxResults", "25"), ("labelIds", "INBOX")]);
        if let Some(page) = page {
            nonempty(page, 4096, "page token")?;
            url.query_pairs_mut().append_pair("pageToken", page);
        }
        json_ok(self.request(
            caller,
            connection,
            Provider::Google,
            GMAIL_READ_SCOPE,
            "GET",
            url,
            Body::Empty,
            None,
        )?)
    }
    pub fn gmail_message(
        &mut self,
        caller: &str,
        connection: &str,
        id: &str,
    ) -> Result<Value, String> {
        nonempty(id, 256, "message ID")?;
        let mut url = endpoint(
            "https://gmail.googleapis.com",
            &["gmail", "v1", "users", "me", "messages", id],
        )?;
        url.query_pairs_mut().append_pair("format", "full");
        json_ok(self.request(
            caller,
            connection,
            Provider::Google,
            GMAIL_READ_SCOPE,
            "GET",
            url,
            Body::Empty,
            None,
        )?)
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubFile {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub path: String,
    pub content: String,
    pub message: String,
    pub sha: Option<String>,
}
impl GithubFile {
    pub fn validate(&self) -> Result<(), String> {
        github_file(&self.owner, &self.repo, &self.path)?;
        nonempty(&self.branch, 255, "branch")?;
        nonempty(&self.message, 1024, "commit message")?;
        if self.content.len() > 1024 * 1024 {
            return Err("Markdown exceeds 1 MiB".into());
        }
        if self
            .sha
            .as_ref()
            .is_some_and(|sha| sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Invalid GitHub file revision".into());
        }
        Ok(())
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventTime {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(rename = "dateTime", skip_serializing_if = "Option::is_none")]
    pub date_time: Option<String>,
    #[serde(rename = "timeZone", skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarEvent {
    pub summary: String,
    pub description: String,
    pub location: String,
    pub start: EventTime,
    pub end: EventTime,
}
impl CalendarEvent {
    pub fn validate(&self) -> Result<(), String> {
        nonempty(&self.summary, 1024, "event title")?;
        if self.description.len() > 16384 || self.location.len() > 2048 {
            return Err("Event details exceed their limit".into());
        }
        for t in [&self.start, &self.end] {
            if t.date.is_some() == t.date_time.is_some() {
                return Err("Use either an all-day date or a timestamp".into());
            }
            if t.time_zone
                .as_ref()
                .is_some_and(|z| z.parse::<chrono_tz::Tz>().is_err())
            {
                return Err("Select a valid IANA timezone".into());
            }
        }
        match (
            &self.start.date,
            &self.end.date,
            &self.start.date_time,
            &self.end.date_time,
        ) {
            (Some(start), Some(end), None, None) => {
                let start = chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d")
                    .map_err(|_| "Invalid start date")?;
                let end = chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d")
                    .map_err(|_| "Invalid end date")?;
                if end <= start {
                    return Err("All-day end date must be after the start date".into());
                }
            }
            (None, None, Some(start), Some(end)) => {
                let start = chrono::DateTime::parse_from_rfc3339(start)
                    .map_err(|_| "Start time needs an explicit UTC offset")?;
                let end = chrono::DateTime::parse_from_rfc3339(end)
                    .map_err(|_| "End time needs an explicit UTC offset")?;
                if end <= start {
                    return Err("Event end must be after its start".into());
                }
            }
            _ => return Err("Start and end must both be dates or both be timestamps".into()),
        }
        Ok(())
    }
}
fn nonempty(value: &str, limit: usize, name: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
        Err(format!("Invalid {name}"))
    } else {
        Ok(())
    }
}
fn endpoint(origin: &str, segments: &[&str]) -> Result<Url, String> {
    let mut url = Url::parse(origin).map_err(|_| "Invalid service endpoint")?;
    url.path_segments_mut()
        .map_err(|_| "Invalid service path")?
        .extend(segments.iter().copied());
    Ok(url)
}
fn github_file(owner: &str, repo: &str, path: &str) -> Result<Url, String> {
    if !path.to_ascii_lowercase().ends_with(".md") {
        return Err("Choose a relative .md file path inside the repository".into());
    }
    github_path(owner, repo, path, false)
}
fn github_path(owner: &str, repo: &str, path: &str, root_allowed: bool) -> Result<Url, String> {
    for name in [owner, repo] {
        nonempty(name, 100, "repository")?;
        if !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            || matches!(name, "." | "..")
        {
            return Err("Invalid repository name".into());
        }
    }
    if root_allowed && path.is_empty() {
        return endpoint("https://api.github.com", &["repos", owner, repo, "contents"]);
    }
    nonempty(path, 1024, "repository path")?;
    if path.contains(['\\', '\0'])
        || path
            .split('/')
            .any(|p| p.is_empty() || matches!(p, "." | ".."))
    {
        return Err("Choose a relative .md file path inside the repository".into());
    }
    let mut segments = vec!["repos", owner, repo, "contents"];
    segments.extend(path.split('/'));
    endpoint("https://api.github.com", &segments)
}
fn event_url(calendar: &str, event: &str) -> Result<Url, String> {
    nonempty(calendar, 1024, "calendar")?;
    nonempty(event, 1024, "event ID")?;
    if matches!(calendar, "." | "..") || matches!(event, "." | "..") {
        return Err("Invalid calendar resource".into());
    }
    endpoint(
        "https://www.googleapis.com",
        &["calendar", "v3", "calendars", calendar, "events", event],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repository_paths_cannot_escape_or_change_host() {
        for path in [
            "../x.md",
            "/x.md",
            "a/../../x.md",
            "x.txt",
            "a\\x.md",
            "a//x.md",
        ] {
            assert!(github_file("owner", "repo", path).is_err(), "{path}");
        }
        let url = github_file("owner", "repo", "notes/你好 #?.md").unwrap();
        assert_eq!(url.host_str(), Some("api.github.com"));
        assert!(url.query().is_none());
        assert!(url.fragment().is_none());
    }
    #[test]
    fn calendar_validation_requires_real_time_and_preserves_all_day_semantics() {
        let mut event:CalendarEvent=serde_json::from_value(json!({"summary":"Fixture appointment","description":"","location":"", "start":{"date":"2026-10-07"},"end":{"date":"2026-10-08"}})).unwrap();
        assert!(event.validate().is_ok());
        event.end.date = Some("2026-10-07".into());
        assert!(event.validate().is_err());
        event.start = EventTime {
            date: None,
            date_time: Some("2026-10-07T09:00:00-07:00".into()),
            time_zone: Some("America/Los_Angeles".into()),
        };
        event.end = EventTime {
            date: None,
            date_time: Some("2026-10-07T09:30:00-07:00".into()),
            time_zone: Some("America/Los_Angeles".into()),
        };
        assert!(event.validate().is_ok());
        event.end.date_time = Some("2026-10-07T09:30:00".into());
        assert!(event.validate().is_err());
    }
}
