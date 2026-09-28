//! Talk to Octos: the opt-in loopback server external clients attach to.
//!
//! Off by default: the kernel is a private `octos serve --stdio` child. When
//! the person turns Talk to Octos on, the kernel runs as
//! `octos serve --host-managed` (octos `docs/HOST_MANAGED_SERVE.md`) and
//! native consumers reach it over its WebSocket with the HOST token, which
//! never leaves this process. External clients get the EXTERNAL token, which
//! opens the UI Protocol socket and nothing else: through a one-time pairing
//! code (web) or the private descriptor file (a terminal UI of this user).
//!
//! - The host token lives for the service's lifetime. The external token is
//!   minted when Talk to Octos turns on and whenever the person rotates it.
//! - On Unix this process binds the listener once and passes it to every
//!   kernel generation (`--listen-fd`), so no other local app can take the
//!   port between restarts. Elsewhere a restart reuses the port when it is
//!   free and otherwise falls back to a fresh port and a fresh external token.
//! - The child's stdin is its lifeline: when this process exits or crashes,
//!   the kernel sees EOF and stops.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

pub const SYSTEM_SESSION: &str = "_main:api:octosense#system";
pub const CONNECTION_FILE: &str = "client-connection.json";
pub(crate) const SYSTEM_WORKSPACE_FILE: &str = "system-workspace.txt";
/// `{"enabled": true}` while Talk to Octos is on (the person's choice).
pub(crate) const EXTERNAL_ACCESS_FILE: &str = "external-access.json";
/// The web client's exact origin, the only browser origin the server trusts.
pub(crate) const WEB_ORIGIN_FILE: &str = "web-client-origin.txt";

/// Write `bytes` to `dir/name` atomically (temp file, then rename), readable
/// by this user only on Unix.
pub(crate) fn write_private(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{name}-{}", uuid::Uuid::new_v4().simple()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, dir.join(name))
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

pub(crate) fn save_system_workspace(dir: &Path, workspace: &Path) -> Result<(), String> {
    write_private(dir, SYSTEM_WORKSPACE_FILE, workspace.to_string_lossy().as_bytes())
        .map_err(|e| format!("Could not save the system workspace: {e}"))
}

/// Whether the person turned Talk to Octos on. A missing or unreadable file
/// is off.
pub(crate) fn external_access_enabled(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join(EXTERNAL_ACCESS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|value| value["enabled"] == true)
}

pub(crate) fn save_external_access(dir: &Path, enabled: bool) -> std::io::Result<()> {
    if enabled {
        write_private(dir, EXTERNAL_ACCESS_FILE, json!({"enabled": true}).to_string().as_bytes())
    } else {
        match std::fs::remove_file(dir.join(EXTERNAL_ACCESS_FILE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// The feature set native consumers had on the stdio pipe, from octos.
fn native_features() -> String {
    octos_core::ui_protocol::UI_PROTOCOL_STDIO_DEFAULT_FEATURES.join(",")
}

fn is_loopback_host(host: &url::Host<&str>) -> bool {
    match host {
        url::Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(ip) => *ip == std::net::Ipv4Addr::LOCALHOST,
        url::Host::Ipv6(ip) => *ip == std::net::Ipv6Addr::LOCALHOST,
    }
}

/// An exact web-client origin: `https://…`, or `http://` only for
/// localhost, 127.0.0.1 or [::1]. No wildcard, credentials, path, query or
/// fragment. `""` means none.
pub(crate) fn validate_origin(origin: &str) -> Result<String, String> {
    if origin.is_empty() {
        return Ok(String::new());
    }
    let usage = "Enter a web client origin such as https://web.example or http://localhost:4173";
    let url = url::Url::parse(origin).map_err(|_| usage.to_string())?;
    let host = url.host().ok_or_else(|| usage.to_string())?;
    // On Android any app can serve http://localhost, so a phone trusts https
    // origins only; a desktop also takes a local http development server.
    let loopback_http = !cfg!(target_os = "android");
    let scheme_ok = match url.scheme() {
        "https" => true,
        "http" => loopback_http && is_loopback_host(&host),
        _ => false,
    };
    if !scheme_ok {
        return Err(if loopback_http {
            "Use https, or http only for localhost, 127.0.0.1 or [::1].".into()
        } else {
            "Use an https origin.".into()
        });
    }
    if url.host_str().is_none_or(|h| h.contains('*'))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Use an origin without a path, credentials, query or fragment.".into());
    }
    Ok(url.origin().ascii_serialization())
}

/// The saved web-client origin. A malformed file means no origin (logged):
/// the kernel stays available and simply trusts no browser origin.
pub(crate) fn read_web_origin(dir: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(dir.join(WEB_ORIGIN_FILE)) else {
        return String::new();
    };
    match validate_origin(text.trim()) {
        Ok(origin) => origin,
        Err(e) => {
            log::warn!("octos-core: ignoring the saved web client origin: {e}");
            String::new()
        }
    }
}

pub(crate) fn save_web_origin(dir: &Path, origin: &str) -> Result<(), String> {
    write_private(dir, WEB_ORIGIN_FILE, origin.as_bytes()).map_err(|e| format!("Could not save the origin: {e}"))
}

/// Where the private descriptor lives. On Windows it is kept in the per-user
/// local AppData (`%LOCALAPPDATA%\OctoSense`), whose default ACL admits only
/// this user, SYSTEM and administrators; elsewhere beside the kernel data,
/// mode 0600.
pub fn connection_file(core_dir: &Path) -> PathBuf {
    connection_file_for(core_dir, cfg!(windows).then(|| std::env::var_os("LOCALAPPDATA")).flatten())
}

pub(crate) fn connection_file_for(core_dir: &Path, local_app_data: Option<std::ffi::OsString>) -> PathBuf {
    match local_app_data.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir).join("OctoSense").join(CONNECTION_FILE),
        None => core_dir.join(CONNECTION_FILE),
    }
}

/// 244 random bits from the OS CSPRNG (two UUID v4, via `getrandom`), hex.
fn mint_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// An external client's connection: the server origin and the EXTERNAL
/// token. For the trusted host sheet and this user's terminal UI only; never
/// pass it to an app script, log it, or put it in a command line.
#[derive(Clone)]
pub struct ClientAccess {
    pub origin: String,
    pub token: String,
}

impl std::fmt::Debug for ClientAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientAccess").field("origin", &self.origin).finish_non_exhaustive()
    }
}

impl ClientAccess {
    pub fn endpoint(&self) -> String {
        format!("ws://{}/api/ui-protocol/ws", self.origin.trim_start_matches("http://"))
    }

    fn authority(&self) -> &str {
        self.origin.trim_start_matches("http://")
    }

    /// The descriptor a terminal UI of this user reads.
    pub(crate) fn save(&self, core_dir: &Path) -> std::io::Result<()> {
        let path = connection_file(core_dir);
        let dir = path.parent().unwrap_or(core_dir);
        let data = json!({"origin": self.origin, "endpoint": self.endpoint(), "token": self.token,
            "profile_id": "_main", "session_id": SYSTEM_SESSION, "pid": std::process::id()});
        write_private(dir, CONNECTION_FILE, data.to_string().as_bytes())
    }
}

pub(crate) fn remove_descriptor(core_dir: &Path) {
    let _ = std::fs::remove_file(connection_file(core_dir));
}

/// The shared server's credentials and address for this service lifetime.
pub(crate) struct Network {
    /// The native consumers' (admin) token. Never leaves this process.
    host_token: String,
    /// The external clients' token while Talk to Octos is on.
    external: Mutex<Option<String>>,
    port: AtomicU16,
    /// Unix: the listener this process keeps and every generation inherits.
    #[cfg(unix)]
    listener: Mutex<Option<std::net::TcpListener>>,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            host_token: mint_token(),
            external: Mutex::new(None),
            port: AtomicU16::new(0),
            #[cfg(unix)]
            listener: Mutex::new(None),
        }
    }
}

impl Network {
    pub fn host_token(&self) -> &str {
        &self.host_token
    }

    /// The external token, minted on first use.
    pub fn external_token(&self) -> String {
        self.external.lock().unwrap().get_or_insert_with(mint_token).clone()
    }

    /// A fresh external token: every paired client must pair again.
    pub fn rotate(&self) {
        *self.external.lock().unwrap() = Some(mint_token());
    }

    /// Talk to Octos turned off: forget the external token and the port.
    pub fn clear(&self) {
        *self.external.lock().unwrap() = None;
        #[cfg(unix)]
        {
            *self.listener.lock().unwrap() = None;
        }
        self.port.store(0, Ordering::Relaxed);
    }

    pub fn port(&self) -> u16 {
        self.port.load(Ordering::Relaxed)
    }

    /// Unix: the kept listener's descriptor, binding 127.0.0.1:0 the first
    /// time. The descriptor stays open (and close-on-exec) in this process.
    #[cfg(unix)]
    pub fn listener_fd(&self) -> std::io::Result<std::os::fd::RawFd> {
        use std::os::fd::AsRawFd;
        let mut listener = self.listener.lock().unwrap();
        if listener.is_none() {
            let bound = std::net::TcpListener::bind(("127.0.0.1", 0))?;
            self.port.store(bound.local_addr()?.port(), Ordering::Relaxed);
            *listener = Some(bound);
        }
        Ok(listener.as_ref().expect("bound above").as_raw_fd())
    }

    /// Without an inherited listener: the port for the next start. The
    /// previous port when it is still free; otherwise a fresh port chosen by
    /// the kernel and a fresh external token, since whoever holds the old
    /// port could otherwise receive clients' tokens.
    #[cfg_attr(unix, allow(dead_code))]
    pub fn port_for_restart(&self) -> u16 {
        let port = self.port();
        if port != 0 && std::net::TcpListener::bind(("127.0.0.1", port)).is_err() {
            log::warn!("octos-core: port {port} is taken; moving Talk to Octos to a new port and rotating its token");
            self.fall_back();
        }
        self.port()
    }

    /// A start on the previous port failed: new port, new external token.
    pub fn fall_back(&self) {
        self.port.store(0, Ordering::Relaxed);
        self.rotate();
    }

    /// Parse only the listener announcement (`NO_COLOR`). Never infer
    /// readiness from anything else the kernel prints.
    pub fn announced(&self, line: &str) -> Option<u16> {
        let origin = line.strip_prefix("Listening: ")?;
        let url = url::Url::parse(origin.trim()).ok()?;
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return None;
        }
        let port = url.port().filter(|p| *p != 0)?;
        let expected = self.port();
        if expected != 0 && expected != port {
            return None;
        }
        self.port.store(port, Ordering::Relaxed);
        Some(port)
    }

    pub fn access(&self) -> ClientAccess {
        ClientAccess { origin: format!("http://127.0.0.1:{}", self.port()), token: self.external_token() }
    }
}

/// Native consumers keep their frame API. This private adapter carries those
/// frames over the server's WebSocket with the host token.
pub(crate) async fn connect(access: &ClientAccess, host_token: &str) -> Result<tokio::io::DuplexStream, String> {
    let mut request = access.endpoint().into_client_request().map_err(|e| e.to_string())?;
    let headers = request.headers_mut();
    headers.insert("Authorization", format!("Bearer {host_token}").parse().map_err(|_| "bad token")?);
    headers.insert("X-Octos-Ui-Features", native_features().parse().map_err(|_| "bad features")?);
    // The announcement precedes axum's accept loop by a few statements.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let socket = loop {
        match tokio_tungstenite::connect_async(request.clone()).await {
            Ok((socket, _)) => break socket,
            Err(tokio_tungstenite::tungstenite::Error::Io(_)) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(_) => return Err("the kernel's authenticated WebSocket connection failed".into()),
        }
    };
    let (client, bridge) = tokio::io::duplex(1024 * 1024);
    tokio::spawn(async move {
        let (reader, mut writer) = tokio::io::split(bridge);
        let mut lines = BufReader::new(reader).lines();
        let (mut sink, mut stream) = socket.split();
        loop {
            tokio::select! {
                line = lines.next_line() => match line {
                    Ok(Some(text)) => if sink.send(Message::Text(text)).await.is_err() { break; },
                    _ => break,
                },
                frame = stream.next() => match frame {
                    Some(Ok(Message::Text(text))) => {
                        if writer.write_all(text.as_bytes()).await.is_err() || writer.write_all(b"\n").await.is_err() { break; }
                    }
                    Some(Ok(Message::Ping(bytes))) => if sink.send(Message::Pong(bytes)).await.is_err() { break; },
                    Some(Ok(Message::Pong(_))) => {},
                    _ => break,
                },
            }
        }
        let _ = sink.close().await;
    });
    Ok(client)
}

/// A one-time pairing code for the external token (octos's pairing): valid
/// for five minutes and one claim.
#[derive(Clone)]
pub struct Pairing {
    pub code: String,
    pub server_origin: String,
    pub expires_in_secs: u64,
}

impl std::fmt::Debug for Pairing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pairing").field("server_origin", &self.server_origin).finish_non_exhaustive()
    }
}

/// The web client's pairing link (octos's `?octos=<server>&pair=<code>`),
/// with the system conversation reference when known.
pub fn pairing_link(web_origin: &str, pairing: &Pairing, system: Option<&str>) -> Result<String, String> {
    let mut url = url::Url::parse(web_origin).map_err(|e| e.to_string())?;
    url.query_pairs_mut().append_pair("octos", &pairing.server_origin).append_pair("pair", &pairing.code);
    if let Some(reference) = system {
        url.query_pairs_mut().append_pair("s", reference);
    }
    Ok(url.to_string())
}

/// One host-only request to the server (`/api/admin/host/pairing`).
async fn host_request(access: &ClientAccess, host_token: &str, method: &str) -> Result<(u16, String), String> {
    let authority = access.authority().to_owned();
    let mut tcp = tokio::net::TcpStream::connect(&authority).await.map_err(|e| e.to_string())?;
    let request = format!(
        "{method} /api/admin/host/pairing HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {host_token}\r\n\
         Content-Length: 0\r\nConnection: close\r\n\r\n"
    );
    tcp.write_all(request.as_bytes()).await.map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), tcp.read_to_end(&mut response))
        .await
        .map_err(|_| "the kernel did not answer".to_string())?
        .map_err(|e| e.to_string())?;
    let response = String::from_utf8_lossy(&response);
    let status = response
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or("the kernel sent no status")?;
    let body = response.split_once("\r\n\r\n").map(|(_, b)| b.to_owned()).unwrap_or_default();
    Ok((status, body))
}

pub(crate) async fn start_pairing(access: &ClientAccess, host_token: &str) -> Result<Pairing, String> {
    let (status, body) = host_request(access, host_token, "POST").await?;
    if status != 200 {
        return Err(format!("The kernel refused to pair ({status}). Turn Talk to Octos on first."));
    }
    // Content-Length framing only (no chunking for this small JSON body).
    let value: serde_json::Value = serde_json::from_str(body.trim()).map_err(|_| "unexpected pairing reply")?;
    Ok(Pairing {
        code: value["code"].as_str().ok_or("no pairing code")?.to_owned(),
        server_origin: value["server_origin"].as_str().ok_or("no server origin")?.to_owned(),
        expires_in_secs: value["expires_in_secs"].as_u64().unwrap_or(300),
    })
}

pub(crate) async fn end_pairing(access: &ClientAccess, host_token: &str) {
    let _ = host_request(access, host_token, "DELETE").await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_origins_are_exact_and_https_unless_loopback() {
        for invalid in ["*", "http://*.example.com", "file:///tmp/client", "https://u:p@example.com",
            "https://example.com/app", "https://example.com?token=secret", "https://example.com/#x",
            "http://example.com", "http://192.168.1.2:4173", "http://10.0.0.1", "ws://localhost:4173"] {
            assert!(validate_origin(invalid).is_err(), "{invalid}");
        }
        assert_eq!(validate_origin("http://localhost:4173/").unwrap(), "http://localhost:4173");
        assert_eq!(validate_origin("http://127.0.0.1:4173").unwrap(), "http://127.0.0.1:4173");
        assert_eq!(validate_origin("http://[::1]:4173").unwrap(), "http://[::1]:4173");
        assert_eq!(validate_origin("https://web.example").unwrap(), "https://web.example");
        assert_eq!(validate_origin("").unwrap(), "");
    }

    #[test]
    fn a_malformed_saved_origin_means_none() {
        let dir = std::env::temp_dir().join(format!("octos-origin-{}", uuid::Uuid::new_v4().simple()));
        assert_eq!(read_web_origin(&dir), "");
        save_web_origin(&dir, "http://evil.example").unwrap();
        assert_eq!(read_web_origin(&dir), "", "http for a non-loopback host is refused");
        save_web_origin(&dir, "https://web.example").unwrap();
        assert_eq!(read_web_origin(&dir), "https://web.example");
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with('.')).collect();
        assert!(leftovers.is_empty(), "atomic writes leave no temp files");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn listener_discovery_rejects_untrusted_and_changed_addresses() {
        let network = Network::default();
        for invalid in ["Pair: http://127.0.0.1:1234", "Listening: http://0.0.0.0:1234",
            "Listening: http://127.0.0.1:1234/?token=x", "Listening: http://127.0.0.1:0"] {
            assert!(network.announced(invalid).is_none());
        }
        assert_eq!(network.announced("Listening: http://127.0.0.1:1234"), Some(1234));
        assert!(network.announced("Listening: http://127.0.0.1:1235").is_none());
        let first = network.access();
        assert_eq!(first.origin, "http://127.0.0.1:1234");
        assert_eq!(first.token, network.access().token, "stable until rotated");
        assert_ne!(first.token, network.host_token(), "external and host tokens differ");
        network.rotate();
        assert_ne!(first.token, network.access().token);
        assert!(!format!("{first:?}").contains(&first.token));
    }

    #[test]
    fn a_taken_port_moves_the_server_and_rotates_its_token() {
        let network = Network::default();
        let holder = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = holder.local_addr().unwrap().port();
        network.announced(&format!("Listening: http://127.0.0.1:{port}")).unwrap();
        let before = network.external_token();
        assert_eq!(network.port_for_restart(), 0, "a squatted port is abandoned");
        assert_ne!(network.external_token(), before, "and the token it was given is retired");
        drop(holder);
    }

    #[test]
    fn the_descriptor_lives_in_local_app_data_when_given() {
        let core = Path::new("/core");
        assert_eq!(connection_file_for(core, None), core.join(CONNECTION_FILE));
        assert_eq!(connection_file_for(core, Some("".into())), core.join(CONNECTION_FILE));
        assert_eq!(connection_file_for(core, Some("/appdata".into())),
            Path::new("/appdata").join("OctoSense").join(CONNECTION_FILE));
    }

    #[test]
    fn the_pairing_link_carries_the_code_and_server() {
        let pairing = Pairing { code: "ABCD2345".into(), server_origin: "http://127.0.0.1:4000".into(), expires_in_secs: 300 };
        let link = pairing_link("https://web.example", &pairing, Some("[\"/w\",\"_main\",\"s\"]")).unwrap();
        let url = url::Url::parse(&link).unwrap();
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs["octos"], "http://127.0.0.1:4000");
        assert_eq!(pairs["pair"], "ABCD2345");
        assert!(!format!("{pairing:?}").contains("ABCD2345"));
    }

    #[test]
    fn native_consumers_request_octos_stdio_feature_set() {
        let features = native_features();
        assert!(features.contains("approval.typed.v1") && features.contains("state.session_hydrate.v1"));
    }
}
