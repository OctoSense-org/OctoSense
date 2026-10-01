//! The in-process client hub: the slice of studio's gateway
//! (studio/hub/src/gateway.rs) a compositor needs. A localhost websocket
//! accepts `--stdin-loop` Makepad children at `/app?build=<id>` and
//! forwards their binary `AppToStudio` traffic to the UI thread; the WM
//! answers over the per-socket sender with `StudioToAppVec` frames.
//!
//! A connection is a launched client only when it proves it (ADR 0004 §5):
//! every process launch gets a fresh secret ([`issue_launch_token`]) that
//! the child reads from its stdin and presents in the handshake's
//! [`TOKEN_HEADER`]. The hub admits a socket for slot `<id>` only with that
//! slot's secret (compared in constant time), spends the secret on the
//! first admission so no second socket can bind the slot, refuses any
//! handshake that carries an `Origin` (a browser page), and forwards
//! frames only from sockets it admitted. The id in the path is a claim;
//! the secret is the identity.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

use makepad_network::http_server::{start_http_server, HttpServer, HttpServerRequest};
use makepad_studio_protocol::{AppToStudio, AppToStudioVec, StudioToApp, StudioToAppVec};
use makepad_widgets::makepad_micro_serde::{DeBin, SerBin};
use makepad_widgets::makepad_platform::thread::{SignalToUI, ThreadOptions, ThreadSpawner};

pub type ClientId = u64;

/// The handshake header a child presents its launch secret in (makepad's
/// `STUDIO_TOKEN_HEADER`).
pub const TOKEN_HEADER: &str = "X-Studio-Token";

/// Set on a launch whose first stdin line is its secret (makepad's
/// `STUDIO_HANDSHAKE_STDIN_ENV`): the child reads the line and presents it.
pub const HANDSHAKE_STDIN_ENV: &str = "STUDIO_HANDSHAKE_STDIN";

/// The secrets of launched process clients that have not connected yet, by
/// slot. An in-process module slot never has one, so no socket can ever
/// bind it.
#[derive(Default)]
pub struct Launches {
    pending: HashMap<ClientId, String>,
}

impl Launches {
    /// A fresh secret for `client`'s launch, replacing any earlier one: 244
    /// random bits from the OS CSPRNG (two UUID v4), hex.
    pub fn issue(&mut self, client: ClientId) -> String {
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        self.pending.insert(client, token.clone());
        token
    }

    /// `client`'s launch is gone (it exited, or never connected): its
    /// secret opens nothing any more.
    pub fn revoke(&mut self, client: ClientId) {
        self.pending.remove(&client);
    }

    /// Whether `presented` is `client`'s secret. On a match the secret is
    /// spent: the slot is bound, and a second socket presenting the same
    /// secret is refused. A wrong secret spends nothing, so a guess cannot
    /// lock the real child out.
    pub fn redeem(&mut self, client: ClientId, presented: Option<&str>) -> bool {
        let Some(presented) = presented else { return false };
        let Some(expected) = self.pending.get(&client) else { return false };
        if !constant_time_eq(expected.as_bytes(), presented.as_bytes()) {
            return false;
        }
        self.pending.remove(&client);
        true
    }
}

/// Byte equality whose time does not depend on where the inputs differ
/// (only on their lengths, which are public: every secret is 64 hex).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}

static LAUNCHES: Mutex<Option<Launches>> = Mutex::new(None);

fn with_launches<R>(f: impl FnOnce(&mut Launches) -> R) -> R {
    let mut guard = LAUNCHES.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Launches::default))
}

/// A fresh secret for the process client being launched as `client`.
pub fn issue_launch_token(client: ClientId) -> String {
    with_launches(|l| l.issue(client))
}

/// `client`'s process is gone before it connected.
pub fn revoke_launch_token(client: ClientId) {
    with_launches(|l| l.revoke(client))
}

/// Why the hub closed a handshake (logged; the socket just closes).
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// Not `/app/<id>` or `/app?build=<id>`.
    NotAClient,
    /// The handshake came from a web page.
    BrowserOrigin,
    /// No secret, a wrong one, one already spent, or a slot that was never
    /// launched as a process.
    BadToken(ClientId),
}

/// Admit a websocket handshake as launched client `<id>`, or refuse it.
pub fn admit(launches: &mut Launches, path: &str, origin: Option<&str>, token: Option<&str>) -> Result<ClientId, Refused> {
    // Browsers send Origin on every websocket handshake; a launched child
    // never does. Any page on any site could otherwise dial 127.0.0.1.
    if origin.is_some() {
        return Err(Refused::BrowserOrigin);
    }
    let client = parse_app_path(path).ok_or(Refused::NotAClient)?;
    if launches.redeem(client, token) {
        Ok(client)
    } else {
        Err(Refused::BadToken(client))
    }
}

pub enum HubEvent {
    /// A launched child proved its slot's secret; `sender` transmits raw ws
    /// frames.
    Connected {
        client: ClientId,
        socket: u64,
        sender: Sender<Vec<u8>>,
    },
    Disconnected {
        socket: u64,
    },
    /// Frames from an admitted socket, with the socket so the WM can check
    /// it is still the one bound to `client`'s slot.
    FromApp {
        client: ClientId,
        socket: u64,
        msgs: Vec<AppToStudio>,
    },
}

pub struct WmHub {
    pub port: u16,
    pub rx: Receiver<HubEvent>,
}

impl WmHub {
    /// Bind the first free port in a small range and start serving.
    pub fn start(spawner: ThreadSpawner) -> Option<WmHub> {
        for port in 8765..8785u16 {
            if let Some(hub) = Self::start_on(port, &spawner) {
                return Some(hub);
            }
        }
        None
    }

    fn start_on(port: u16, spawner: &ThreadSpawner) -> Option<WmHub> {
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().ok()?;
        // The http server's bind failure only prints; probe the port first
        // so a WM instance still holding it (or any other service) makes
        // us move on to the next one instead of hosting nothing.
        match std::net::TcpListener::bind(addr) {
            Ok(probe) => drop(probe),
            Err(_) => return None,
        }
        let (request_tx, request_rx) = mpsc::channel::<HttpServerRequest>();
        start_http_server(HttpServer {
            listen_address: addr,
            request: request_tx,
            post_max_size: 1024 * 1024,
            post_max_size_overrides: Vec::new(),
            pre_admit_posts: false,
            client_ip_resolver: None,
            trusted_proxy: None,
            allowed_methods: None,
        })?;

        let (event_tx, event_rx) = mpsc::channel::<HubEvent>();
        let worker = spawner
            .spawn_worker(
                ThreadOptions {
                    name: Some("wm-hub".into()),
                    ..Default::default()
                },
                move || {
                // socket id -> client id, so binary frames route by socket.
                let mut socket_client = HashMap::<u64, ClientId>::new();
                while let Ok(request) = request_rx.recv() {
                    match request {
                        HttpServerRequest::ConnectWebSocket {
                            web_socket_id,
                            headers,
                            response_sender,
                        } => {
                            let path = if let Some(search) =
                                headers.search.as_ref().filter(|s| !s.is_empty())
                            {
                                format!("{}?{}", headers.path, search)
                            } else {
                                headers.path.clone()
                            };
                            let origin = headers.header("Origin");
                            let token = headers.header(TOKEN_HEADER);
                            let client = match with_launches(|l| admit(l, &path, origin, token)) {
                                Ok(client) => client,
                                Err(why) => {
                                    // Not a launched client: close it, and
                                    // none of its frames are ever read.
                                    makepad_widgets::log!("wm-hub: refused a socket ({why:?})");
                                    let _ = response_sender.send(Vec::new());
                                    continue;
                                }
                            };
                            socket_client.insert(web_socket_id, client);
                            let _ = event_tx.send(HubEvent::Connected {
                                client,
                                socket: web_socket_id,
                                sender: response_sender,
                            });
                            SignalToUI::set_ui_signal();
                        }
                        HttpServerRequest::DisconnectWebSocket { web_socket_id } => {
                            if socket_client.remove(&web_socket_id).is_some() {
                                let _ = event_tx.send(HubEvent::Disconnected {
                                    socket: web_socket_id,
                                });
                                SignalToUI::set_ui_signal();
                            }
                        }
                        HttpServerRequest::BinaryMessage {
                            web_socket_id,
                            data,
                            ..
                        } => {
                            // Only admitted sockets are in the map: a
                            // refused or unknown socket's frames are dropped.
                            let Some(&client) = socket_client.get(&web_socket_id) else {
                                continue;
                            };
                            // Children send both single messages and vecs.
                            let msgs = match AppToStudioVec::deserialize_bin(&data) {
                                Ok(vec) => vec.0,
                                Err(_) => match AppToStudio::deserialize_bin(&data) {
                                    Ok(msg) => vec![msg],
                                    Err(_) => continue,
                                },
                            };
                            let _ = event_tx.send(HubEvent::FromApp {
                                client,
                                socket: web_socket_id,
                                msgs,
                            });
                            SignalToUI::set_ui_signal();
                        }
                        _ => {}
                    }
                }
                },
            )
            .ok()?;
        worker.detach();

        Some(WmHub {
            port,
            rx: event_rx,
        })
    }
}

/// Send a batch of StudioToApp messages over a client socket sender.
pub fn send_to_app(sender: &Sender<Vec<u8>>, msgs: Vec<StudioToApp>) {
    if msgs.is_empty() {
        return;
    }
    let _ = sender.send(StudioToAppVec(msgs).serialize_bin());
}

/// `/app/<id>` and `/app?build=<id>[&crate=..]`, like studio's gateway.
fn parse_app_path(path: &str) -> Option<ClientId> {
    if let Some(rest) = path.strip_prefix("/app/") {
        if rest.is_empty() || rest.contains('/') {
            return None;
        }
        return rest.parse::<u64>().ok();
    }
    let (route, query) = path.split_once('?').unwrap_or((path, ""));
    if route != "/app" {
        return None;
    }
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == "build" {
            return value.trim().parse::<u64>().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_refuse_a_socket_when_it_has_no_secret_or_a_wrong_one() {
        let mut launches = Launches::default();
        let token = launches.issue(7);
        assert_eq!(token.len(), 64, "244 random bits, hex");
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(admit(&mut launches, "/app?build=7&crate=terminal", None, None), Err(Refused::BadToken(7)));
        let wrong = format!("{}{}", &token[..63], if token.ends_with('0') { '1' } else { '0' });
        assert_ne!(wrong, token);
        assert_eq!(admit(&mut launches, "/app?build=7&crate=terminal", None, Some(&wrong)), Err(Refused::BadToken(7)));
        assert_eq!(admit(&mut launches, "/app?build=7&crate=terminal", None, Some("")), Err(Refused::BadToken(7)));
        // Another slot's secret opens only that slot.
        let other = launches.issue(8);
        assert_eq!(admit(&mut launches, "/app?build=7", None, Some(&other)), Err(Refused::BadToken(7)));
        // Guesses spent nothing: the real child still gets in.
        assert_eq!(admit(&mut launches, "/app?build=7&crate=terminal", None, Some(&token)), Ok(7));
    }

    #[test]
    fn should_refuse_a_second_socket_when_the_slot_is_already_bound() {
        let mut launches = Launches::default();
        let token = launches.issue(3);
        assert_eq!(admit(&mut launches, "/app/3", None, Some(&token)), Ok(3));
        assert_eq!(admit(&mut launches, "/app/3", None, Some(&token)), Err(Refused::BadToken(3)), "the secret is spent on the first bind");
        assert_eq!(admit(&mut launches, "/app?build=3", None, Some(&token)), Err(Refused::BadToken(3)));
    }

    #[test]
    fn should_refuse_a_socket_when_its_slot_was_never_launched_as_a_process() {
        // An in-process module slot, or an id nobody launched: no secret.
        let mut launches = Launches::default();
        assert_eq!(admit(&mut launches, "/app/5", None, Some("00")), Err(Refused::BadToken(5)));
        // A launch that ended before it connected: its secret is revoked.
        let token = launches.issue(6);
        launches.revoke(6);
        assert_eq!(admit(&mut launches, "/app/6", None, Some(&token)), Err(Refused::BadToken(6)));
    }

    #[test]
    fn should_refuse_a_socket_when_a_browser_origin_opened_it() {
        let mut launches = Launches::default();
        let token = launches.issue(2);
        for origin in ["https://example.com", "null", "http://127.0.0.1:8765"] {
            assert_eq!(admit(&mut launches, "/app/2", Some(origin), Some(&token)), Err(Refused::BrowserOrigin));
        }
        // The refusals spent nothing.
        assert_eq!(admit(&mut launches, "/app/2", None, Some(&token)), Ok(2));
        assert_eq!(admit(&mut launches, "/ui", None, None), Err(Refused::NotAClient));
    }

    #[test]
    fn should_issue_a_fresh_secret_when_a_slot_is_launched() {
        let mut launches = Launches::default();
        let a = launches.issue(1);
        let b = launches.issue(2);
        assert_ne!(a, b);
        assert!(constant_time_eq(a.as_bytes(), a.as_bytes()));
        assert!(!constant_time_eq(a.as_bytes(), b.as_bytes()));
        assert!(!constant_time_eq(a.as_bytes(), &a.as_bytes()[..63]));
    }

    #[test]
    fn should_keep_the_handshake_flag_when_the_sandbox_scrubs_a_launchs_environment() {
        // The flag tells the child to read its secret from stdin; a name the
        // sandbox reads as a secret would be scrubbed and every launch
        // refused.
        assert!(!crate::sandbox::is_secret_var(HANDSHAKE_STDIN_ENV));
        let mut cmd = std::process::Command::new("true");
        cmd.env(HANDSHAKE_STDIN_ENV, "1");
        crate::sandbox::scrub_env_from(&mut cmd, std::iter::empty());
        assert!(cmd.get_envs().any(|(k, v)| k == HANDSHAKE_STDIN_ENV && v.is_some()));
    }

    #[test]
    fn app_paths() {
        assert_eq!(parse_app_path("/app/42"), Some(42));
        assert_eq!(parse_app_path("/app?build=7&crate=terminal"), Some(7));
        assert_eq!(parse_app_path("/app?crate=terminal"), None);
        assert_eq!(parse_app_path("/ui"), None);
        assert_eq!(parse_app_path("/app/x"), None);
    }
}
