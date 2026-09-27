//! REST snapshot client. Surfaces per `03-PROTOCOL-CONTRACT.md` § "What stays
//! REST" plus `upload` (octos-cli handlers.rs:944) and `my_content`
//! (octos-cli auth_handlers.rs:1121).

use octos_core::{SessionKey, Task};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::multipart::Form;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{FileHandle, ProfileId, SecretString};

const PROFILE_HEADER: &str = "X-Profile-Id";

pub type RestResult<T> = Result<T, RestError>;

/// REST-layer errors. Distinct from `RpcError`: snapshot endpoints are plain
/// HTTP, not JSON-RPC.
#[derive(Debug)]
pub enum RestError {
    Network(reqwest::Error),
    Http(reqwest::Error),
    Status { status: u16, body: serde_json::Value },
    DecodeBody(serde_json::Error),
    Decode(serde_json::Error),
    Url(url::ParseError),
    Other(String),
}

impl std::fmt::Display for RestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "rest network: {e}"),
            Self::Http(e) => write!(f, "rest http: {e}"),
            Self::Status { status, .. } => write!(f, "rest status: {status}"),
            Self::DecodeBody(e) => write!(f, "rest decode body: {e}"),
            Self::Decode(e) => write!(f, "rest decode: {e}"),
            Self::Url(e) => write!(f, "rest url: {e}"),
            Self::Other(msg) => write!(f, "rest other: {msg}"),
        }
    }
}

impl std::error::Error for RestError {}

impl From<reqwest::Error> for RestError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_connect() || e.is_timeout() {
            Self::Network(e)
        } else {
            Self::Http(e)
        }
    }
}

impl From<url::ParseError> for RestError {
    fn from(e: url::ParseError) -> Self {
        Self::Url(e)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListItem {
    /// The WS `session/list` rows (server `SessionInfo`,
    /// `octos-cli/src/api/handlers.rs:555`) name this `id`; the retired
    /// REST shape used `session_id`. Accept both.
    #[serde(alias = "id")]
    pub session_id: SessionKey,
    #[serde(default)]
    pub title: Option<String>,
    /// `SessionInfo` calls this `updated_at` (RFC3339 string).
    #[serde(default, alias = "updated_at")]
    pub last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentCursor(pub String);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MyContentQuery {
    pub kind: Option<String>,
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<ContentCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyContentRow {
    pub id: String,
    pub kind: String,
    pub title: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// `/api/my/content` envelope. Server returns `{ entries: [...], total }`
/// (octos-cli `auth_handlers.rs:1171`, `ContentQueryResult`); the transport
/// surfaces both fields so the browser can drive cursoring off `total`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyContentResponse {
    #[serde(default)]
    pub entries: Vec<MyContentRow>,
    #[serde(default)]
    pub total: u64,
}

/// `/api/version` response. Tolerant shape: today's server returns
/// `{build_date, service, tunnel_domain, version}` (a flat string version);
/// the future protocol-aware shape will add typed `version` /
/// `capabilities`. Accept either by deserializing to a `serde_json::Value`
/// and projecting accessors on top. See 02-API-DRIFT.md "Watch list".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionProbe {
    #[serde(flatten)]
    pub raw: serde_json::Value,
}

impl VersionProbe {
    /// Best-effort version string. Tries the typed `version.version` shape
    /// first, then the flat `version` shape used by today's server.
    pub fn version_string(&self) -> Option<String> {
        self.raw
            .pointer("/version/version")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                self.raw
                    .get("version")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
    }

    /// Service name if the server emits it. Today's server returns "octos".
    pub fn service(&self) -> Option<&str> {
        self.raw.get("service").and_then(|v| v.as_str())
    }
}

/// Body for `POST /api/auth/send-code` (octos-cli auth_handlers.rs:389).
#[derive(Debug, Clone, Serialize)]
pub struct SendCodeRequest<'a> {
    pub email: &'a str,
}

/// Response for `POST /api/auth/send-code`. Server always returns
/// `ok: true` to prevent enumeration; `message` may carry advisory text.
#[derive(Debug, Clone, Deserialize)]
pub struct SendCodeResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub message: Option<String>,
}

/// Body for `POST /api/auth/verify` (octos-cli auth_handlers.rs:543).
#[derive(Debug, Clone, Serialize)]
pub struct VerifyRequest<'a> {
    pub email: &'a str,
    pub code: &'a str,
}

/// Response for `POST /api/auth/verify`. On success carries a bearer
/// `token` and optionally the resolved profile id.
#[derive(Debug, Clone, Deserialize)]
pub struct VerifyResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RestClient {
    pub http: reqwest::Client,
    pub base: Url,
    pub token: SecretString,
    pub profile_id: ProfileId,
}

/// Resolved file URL plus its companion query-string variant. W04 picks one
/// (header-bearer for native viewers, query-token for ones that can't set
/// custom headers, e.g. `<img src>`).
#[derive(Debug, Clone)]
pub struct ResolvedFileUrl {
    pub bare: Url,
    pub with_token: Url,
}

impl RestClient {
    pub fn new(http: reqwest::Client, base: Url, token: SecretString, profile_id: ProfileId) -> Self {
        Self { http, base, token, profile_id }
    }

    fn headers(&self) -> Result<HeaderMap, RestError> {
        let mut h = HeaderMap::new();
        let mut auth = HeaderValue::from_str(&format!("Bearer {}", self.token.expose()))
            .map_err(|e| RestError::Other(format!("invalid bearer: {e}")))?;
        auth.set_sensitive(true);
        h.insert(AUTHORIZATION, auth);
        h.insert(
            PROFILE_HEADER,
            HeaderValue::from_str(&self.profile_id.0)
                .map_err(|e| RestError::Other(format!("invalid profile id: {e}")))?,
        );
        Ok(h)
    }

    fn url(&self, path: &str) -> Result<Url, RestError> {
        let mut u = self.base.clone();
        let trimmed = path.trim_start_matches('/');
        u.path_segments_mut()
            .map_err(|_| RestError::Other("base url is cannot-be-a-base".into()))?
            .pop_if_empty()
            .extend(trimmed.split('/'));
        Ok(u)
    }

    async fn decode<T: DeserializeOwned>(resp: reqwest::Response) -> RestResult<T> {
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(RestError::from)?;
        if !status.is_success() {
            let body = serde_json::from_slice::<serde_json::Value>(&bytes)
                .unwrap_or(serde_json::Value::Null);
            return Err(RestError::Status { status: status.as_u16(), body });
        }
        serde_json::from_slice::<T>(&bytes).map_err(RestError::DecodeBody)
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> RestResult<T> {
        let url = self.url(path)?;
        let resp = self.http.get(url).headers(self.headers()?).send().await?;
        Self::decode(resp).await
    }

    /// `GET /api/sessions`.
    pub async fn list_sessions(&self) -> RestResult<Vec<SessionListItem>> {
        self.get_json("api/sessions").await
    }

    /// `GET /api/sessions/{id}/messages` — server projects to octos-core `Task`.
    pub async fn messages_for(&self, session_id: &SessionKey) -> RestResult<Task> {
        self.get_json(&format!("api/sessions/{}/messages", session_id.0))
            .await
    }

    /// `DELETE /api/sessions/{id}`.
    pub async fn delete_session(&self, session_id: &SessionKey) -> RestResult<()> {
        let url = self.url(&format!("api/sessions/{}", session_id.0))?;
        let resp = self.http.delete(url).headers(self.headers()?).send().await?;
        let status = resp.status();
        if status == StatusCode::NO_CONTENT || status.is_success() {
            return Ok(());
        }
        let body = resp
            .json::<serde_json::Value>()
            .await
            .unwrap_or(serde_json::Value::Null);
        Err(RestError::Status { status: status.as_u16(), body })
    }

    /// `POST /api/upload` — multipart; one handle per part.
    pub async fn upload(&self, parts: Form) -> RestResult<Vec<FileHandle>> {
        let url = self.url("api/upload")?;
        let resp = self
            .http
            .post(url)
            .headers(self.headers()?)
            .multipart(parts)
            .send()
            .await?;
        Self::decode(resp).await
    }

    /// Resolve `FileHandle` to `/api/files/{handle}`. Returns both the bare
    /// URL (use a bearer header) and a `?token=...` variant for clients that
    /// can't set custom headers.
    pub fn file_url(&self, handle: &FileHandle) -> RestResult<ResolvedFileUrl> {
        let bare = self.url(&format!("api/files/{}", handle.0))?;
        let mut with_token = bare.clone();
        with_token
            .query_pairs_mut()
            .append_pair("token", self.token.expose());
        Ok(ResolvedFileUrl { bare, with_token })
    }

    /// `GET /api/version` — pre-WS capability probe (W01 § Capability handshake).
    pub async fn version_probe(&self) -> RestResult<VersionProbe> {
        self.get_json("api/version").await
    }

    /// `POST /api/auth/send-code` (octos-cli auth_handlers.rs:389). Anonymous
    /// — no `Authorization` / `X-Profile-Id` headers required. Server returns
    /// `ok: true` even on rate-limit / unknown-email so the UI can always
    /// advance to `AwaitingCode` regardless of body content.
    pub async fn send_code(&self, email: &str) -> RestResult<SendCodeResponse> {
        let url = self.url("api/auth/send-code")?;
        let resp = self
            .http
            .post(url)
            .json(&SendCodeRequest { email })
            .send()
            .await?;
        Self::decode(resp).await
    }

    /// `POST /api/auth/verify` (octos-cli auth_handlers.rs:543). Anonymous;
    /// success carries `token` (bearer) and optionally `profile_id`.
    pub async fn verify(&self, email: &str, code: &str) -> RestResult<VerifyResponse> {
        let url = self.url("api/auth/verify")?;
        let resp = self
            .http
            .post(url)
            .json(&VerifyRequest { email, code })
            .send()
            .await?;
        Self::decode(resp).await
    }

    /// `GET /api/my/content`. Returns the full `{ entries, total }` envelope
    /// — the server (octos-cli `auth_handlers.rs:1171`) wraps the rows; the
    /// total field supports pagination.
    pub async fn my_content(&self, query: MyContentQuery) -> RestResult<MyContentResponse> {
        let url = self.url("api/my/content")?;
        let mut req = self.http.get(url).headers(self.headers()?);
        let mut pairs: Vec<(&str, String)> = Vec::new();
        if let Some(kind) = &query.kind {
            pairs.push(("kind", kind.clone()));
        }
        if let Some(q) = &query.q {
            pairs.push(("q", q.clone()));
        }
        if let Some(limit) = query.limit {
            pairs.push(("limit", limit.to_string()));
        }
        if let Some(cursor) = &query.cursor {
            pairs.push(("cursor", cursor.0.clone()));
        }
        if !pairs.is_empty() {
            req = req.query(&pairs);
        }
        let resp = req.send().await?;
        Self::decode(resp).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> RestClient {
        RestClient::new(
            reqwest::Client::new(),
            Url::parse("https://example.test").unwrap(),
            SecretString::new("tk"),
            ProfileId::new("p1"),
        )
    }

    #[test]
    fn url_builder_handles_no_trailing_slash() {
        let c = client();
        assert_eq!(c.url("api/sessions").unwrap().as_str(), "https://example.test/api/sessions");
    }

    #[test]
    fn file_url_emits_both_variants() {
        let c = client();
        let r = c.file_url(&FileHandle("abc".into())).unwrap();
        assert_eq!(r.bare.as_str(), "https://example.test/api/files/abc");
        assert!(r.with_token.as_str().contains("token=tk"));
    }
}
