//! Ways a broker reaches a kernel.

#[cfg(any(feature = "octos-core", feature = "ws"))]
use crate::broker::{BoxFuture, Connector, Link};

/// The shell's kernel (`octosense-octos-core`), or a kernel a standalone app
/// owns through its own [`octosense_octos_core::Core`].
#[cfg(feature = "octos-core")]
pub struct CoreConnector {
    core: Option<octosense_octos_core::Core>,
    owned: bool,
}

#[cfg(feature = "octos-core")]
impl CoreConnector {
    /// The shell's process kernel: shared, never owned or stopped by an app.
    pub fn shell() -> Self {
        Self {
            core: None,
            owned: false,
        }
    }

    /// A kernel this app owns: started on the first request, stopped by
    /// [`Connector::shutdown`] (and when its last connection leaves).
    pub fn owned(core: octosense_octos_core::Core) -> Self {
        Self {
            core: Some(core),
            owned: true,
        }
    }

    /// A kernel another party owns and shares (tests; a shell's own
    /// [`octosense_octos_core::Core`] instance): never stopped from here.
    pub fn shared(core: octosense_octos_core::Core) -> Self {
        Self {
            core: Some(core),
            owned: false,
        }
    }
}

#[cfg(feature = "octos-core")]
impl Connector for CoreConnector {
    fn available(&self) -> Result<(), String> {
        match &self.core {
            Some(core) => core.launch().map(|_| ()),
            None => octosense_octos_core::launch().map(|_| ()),
        }
        .map_err(|e| e.to_string())
    }

    fn connect(&self) -> BoxFuture<'static, Result<Box<dyn Link>, String>> {
        let connection = match &self.core {
            Some(core) => core.connect(),
            None => octosense_octos_core::connect(),
        };
        Box::pin(async move {
            connection
                .map(|c| Box::new(CoreLink(c)) as Box<dyn Link>)
                .map_err(|e| e.to_string())
        })
    }

    fn owns_runtime(&self) -> bool {
        self.owned
    }

    fn shutdown(&self) {
        if let (true, Some(core)) = (self.owned, &self.core) {
            core.shutdown_within(std::time::Duration::from_secs(5));
        }
    }
}

#[cfg(feature = "octos-core")]
struct CoreLink(octosense_octos_core::Connection);

#[cfg(feature = "octos-core")]
impl Link for CoreLink {
    fn send(&mut self, frame: String) -> Result<(), String> {
        self.0.send(frame).map_err(|e| e.to_string())
    }

    fn recv(&mut self) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(async move { self.0.recv().await.map_err(|e| e.to_string()) })
    }
}

/// An explicitly configured remote octos server (`<base>/api/ui-protocol/ws`,
/// authenticated by its bearer token only, one profile named in requests). The server owns its runtime and model
/// credentials; disconnecting never stops it.
#[cfg(feature = "ws")]
pub struct WsConnector {
    base: url::Url,
    bearer: String,
    profile: String,
}

#[cfg(feature = "ws")]
impl WsConnector {
    /// `base` must be http(s) without embedded credentials.
    pub fn new(
        base: url::Url,
        bearer: impl Into<String>,
        profile: impl Into<String>,
    ) -> Result<Self, String> {
        if !matches!(base.scheme(), "http" | "https")
            || base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
        {
            return Err("Use an http(s) server URL without embedded credentials".into());
        }
        Ok(Self {
            base,
            bearer: bearer.into(),
            profile: profile.into(),
        })
    }

    fn ws_url(&self) -> Result<url::Url, String> {
        let mut url = self.base.clone();
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        url.set_scheme(scheme).map_err(|_| "bad server URL")?;
        url.path_segments_mut()
            .map_err(|_| "bad server URL")?
            .pop_if_empty()
            .extend(["api", "ui-protocol", "ws"]);
        Ok(url)
    }
}

#[cfg(feature = "ws")]
impl Connector for WsConnector {
    fn available(&self) -> Result<(), String> {
        Ok(())
    }

    fn connect(&self) -> BoxFuture<'static, Result<Box<dyn Link>, String>> {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        use tokio_tungstenite::tungstenite::Message;
        let url = self.ws_url();
        let bearer = self.bearer.clone();
        let profile = self.profile.clone();
        Box::pin(async move {
            let mut request = url?
                .as_str()
                .into_client_request()
                .map_err(|e| format!("server URL: {e}"))?;
            let headers = request.headers_mut();
            headers.insert(
                "authorization",
                format!("Bearer {bearer}")
                    .parse()
                    .map_err(|_| "bad token")?,
            );
            // Authenticate by the token alone. `x-profile-id` is the server's
            // trusted-proxy header (honored from loopback without a token), so
            // sending it would let a wrong token through on a local server.
            // The profile travels in every request's params instead.
            let _ = &profile;
            // Bound sessions reopen in the workspace the server gave them.
            headers.insert(
                "x-octos-ui-features",
                "session.workspace_cwd.v1"
                    .parse()
                    .map_err(|_| "bad features")?,
            );
            let (socket, _) = tokio_tungstenite::connect_async(request)
                .await
                .map_err(|e| format!("could not reach the server: {e}"))?;
            let (mut write, read) = socket.split();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            tokio::spawn(async move {
                while let Some(frame) = rx.recv().await {
                    if write.send(Message::Text(frame)).await.is_err() {
                        break;
                    }
                }
                let _ = write.close().await;
            });
            Ok(Box::new(WsLink { tx, read }) as Box<dyn Link>)
        })
    }

    fn owns_runtime(&self) -> bool {
        false
    }

    fn shutdown(&self) {}
}

#[cfg(feature = "ws")]
type WsRead = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

#[cfg(feature = "ws")]
struct WsLink {
    tx: tokio::sync::mpsc::UnboundedSender<String>,
    read: WsRead,
}

#[cfg(feature = "ws")]
impl Link for WsLink {
    fn send(&mut self, frame: String) -> Result<(), String> {
        self.tx
            .send(frame)
            .map_err(|_| "the server connection closed".to_owned())
    }

    fn recv(&mut self) -> BoxFuture<'_, Result<String, String>> {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;
        Box::pin(async move {
            loop {
                match self.read.next().await {
                    Some(Ok(Message::Text(text))) => return Ok(text),
                    Some(Ok(Message::Close(_))) | None => {
                        return Err("the server closed the connection".into())
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => return Err(format!("server connection: {e}")),
                }
            }
        })
    }
}
