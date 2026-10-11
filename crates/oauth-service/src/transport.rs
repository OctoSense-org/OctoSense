//! Bounded provider transport. URLs and authorization headers are host-owned.
use serde_json::Value;
use std::{io::Read, time::Duration};
use url::Url;

pub enum Body {
    Empty,
    Form(Vec<(String, String)>),
    Json(Value),
}

// No Debug: requests may carry an authorization code, token or verifier.
pub struct Request {
    pub method: &'static str,
    pub url: Url,
    pub bearer: Option<String>,
    pub if_match: Option<String>,
    pub body: Body,
}
pub struct Response {
    pub status: u16,
    pub body: Value,
    pub etag: Option<String>,
}

pub trait Transport: Send + Sync {
    fn send(&self, request: Request) -> Result<Response, String>;
}

pub struct HttpsTransport {
    client: reqwest::blocking::Client,
}
impl HttpsTransport {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("OctoSense-ConnectedApps/0.1")
            .build()
            .map_err(|_| "Cannot initialize HTTPS")?;
        Ok(Self { client })
    }
}
impl Transport for HttpsTransport {
    fn send(&self, request: Request) -> Result<Response, String> {
        let allowed = [
            "github.com",
            "api.github.com",
            "oauth2.googleapis.com",
            "www.googleapis.com",
            "gmail.googleapis.com",
            "openidconnect.googleapis.com",
        ];
        if request.url.scheme() != "https"
            || !request
                .url
                .host_str()
                .is_some_and(|host| allowed.contains(&host))
            || request.url.port().is_some_and(|port| port != 443)
            || !request.url.username().is_empty()
            || request.url.password().is_some()
            || request.url.fragment().is_some()
        {
            return Err("Unsupported provider endpoint".into());
        }
        let method = reqwest::Method::from_bytes(request.method.as_bytes())
            .map_err(|_| "Invalid provider method")?;
        let mut req = self
            .client
            .request(method, request.url)
            .header("Accept", "application/json");
        if let Some(token) = request.bearer {
            req = req.bearer_auth(token);
        }
        if let Some(etag) = request.if_match {
            req = req.header("If-Match", etag);
        }
        req = match request.body {
            Body::Empty => req,
            Body::Form(fields) => req.form(&fields),
            Body::Json(body) => req.json(&body),
        };
        // Never surface reqwest's request/debug text or provider error bodies:
        // either can include private callback parameters or credentials.
        let response = req
            .send()
            .map_err(|_| "Provider connection failed; check the network")?;
        let status = response.status().as_u16();
        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let mut bytes = Vec::new();
        response
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read provider response")?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Provider response exceeds 2 MiB".into());
        }
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).map_err(|_| "Provider returned invalid JSON")?
        };
        Ok(Response { status, body, etag })
    }
}

pub(crate) fn json_ok(response: Response) -> Result<Value, String> {
    match response.status {
        200..=299 => Ok(response.body),
        401 => Err("Sign in again: provider authorization expired".into()),
        403 => Err("Provider denied this operation; check granted permissions".into()),
        404 => Err("The selected resource was not found or is not accessible".into()),
        409 | 412 => Err("The remote content changed; reload before saving".into()),
        429 => Err("Provider rate limit reached; try again later".into()),
        _ => Err(format!(
            "Provider request failed (HTTP {})",
            response.status
        )),
    }
}

pub(crate) fn form(url: &str, fields: Vec<(&str, String)>) -> Request {
    Request {
        method: "POST",
        url: Url::parse(url).expect("fixed provider endpoint"),
        bearer: None,
        if_match: None,
        body: Body::Form(fields.into_iter().map(|(k, v)| (k.into(), v)).collect()),
    }
}
