//! How the service asks a source for its feed: one GET, conditional on what
//! the source said last time, with a timeout and a size cap, and never a
//! redirect the service did not check ([`crate`] follows a redirect only to a
//! declared host).
use std::io::Read;
use std::time::Duration;

pub const USER_AGENT: &str = "OctoSense-News/1.0";
/// The most a feed may be; a larger answer is refused, not truncated.
pub const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// What to ask for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    pub url: String,
    /// The source's last `ETag`, sent as `If-None-Match`.
    pub etag: Option<String>,
    /// The source's last `Last-Modified`, sent as `If-Modified-Since`.
    pub last_modified: Option<String>,
}

/// What came back. A 3xx carries its `location`; a 304 has no body.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub location: Option<String>,
    /// `Retry-After`, in seconds (a 429's or a 503's).
    pub retry_after: Option<i64>,
}

/// A `Retry-After` value: seconds, or an HTTP date (`now` in Unix seconds).
pub fn retry_after_secs(value: &str, now: i64) -> Option<i64> {
    let value = value.trim();
    if let Ok(secs) = value.parse::<i64>() {
        return Some(secs.max(0));
    }
    chrono::DateTime::parse_from_rfc2822(value).ok().map(|at| (at.timestamp() - now).max(0))
}

/// The network, or a fixture in tests. Called on the service's fetch thread.
pub trait Fetcher: Send + Sync {
    fn get(&self, request: &Request) -> Result<Response, String>;
}

/// HTTPS through ureq: 10 s to connect, 30 s in all, redirects returned
/// rather than followed, bodies capped at [`MAX_BYTES`].
pub struct HttpFetcher {
    agent: ureq::Agent,
}

impl Default for HttpFetcher {
    fn default() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirects(0)
            .user_agent(USER_AGENT)
            .build();
        HttpFetcher { agent }
    }
}

impl Fetcher for HttpFetcher {
    fn get(&self, request: &Request) -> Result<Response, String> {
        let mut call = self.agent.get(&request.url).set("Accept", "application/rss+xml, application/atom+xml, application/json, text/xml;q=0.9, */*;q=0.5");
        if let Some(etag) = &request.etag {
            call = call.set("If-None-Match", etag);
        }
        if let Some(modified) = &request.last_modified {
            call = call.set("If-Modified-Since", modified);
        }
        let response = match call.call() {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(e)) => return Err(e.to_string()),
        };
        let status = response.status();
        let header = |name: &str| response.header(name).map(str::to_string);
        let (etag, last_modified, location) = (header("ETag"), header("Last-Modified"), header("Location"));
        let retry_after = header("Retry-After").and_then(|v| retry_after_secs(&v, chrono::Utc::now().timestamp()));
        if response.header("Content-Length").and_then(|l| l.parse::<u64>().ok()).is_some_and(|l| l > MAX_BYTES) {
            return Err("the feed is larger than the service reads".into());
        }
        let mut bytes = Vec::new();
        if status == 200 {
            response.into_reader().take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err("the feed is larger than the service reads".into());
            }
        }
        Ok(Response { status, body: String::from_utf8_lossy(&bytes).into_owned(), etag, last_modified, location, retry_after })
    }
}
