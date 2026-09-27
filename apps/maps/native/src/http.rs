//! Android service requests use rustls: Android 9's Java HTTPS provider
//! cannot negotiate TLS 1.3 with the routing service. Other platforms keep
//! using the framework transport, as do the map renderer's tile requests.

#[cfg(not(any(target_os = "android", test)))]
#[derive(Default)]
pub(crate) struct MapHttp;

#[cfg(any(target_os = "android", test))]
pub(crate) use native::MapHttp;

#[cfg(any(target_os = "android", test))]
mod native {
    use crate::model::{body_text, plain_error, MAX_BODY_BYTES, USER_AGENT};
    use makepad_widgets::{makepad_platform::thread::ToUIReceiver, LiveId};
    use reqwest::{Client, ClientBuilder};
    use std::{collections::HashMap, error::Error, time::Duration};
    use tokio::sync::oneshot;

    pub(crate) struct Reply {
        pub id: LiveId,
        pub body: Result<String, String>,
    }

    #[derive(Default)]
    pub(crate) struct MapHttp {
        // Dropping a sender cancels its worker, including on widget drop.
        pending: HashMap<LiveId, oneshot::Sender<()>>,
        replies: ToUIReceiver<Reply>,
    }

    fn client_builder() -> ClientBuilder {
        Client::builder()
            // Select explicitly even if another dependency enables native TLS.
            .use_rustls_tls()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
    }

    fn transport_error(error: reqwest::Error) -> String {
        if error.is_timeout() {
            return "timed out".into();
        }
        // The useful TLS/DNS cause is nested. Do not expose trip coordinates
        // from the request URL in a displayed error.
        let error = error.without_url();
        let mut message = error.to_string();
        let mut source = error.source();
        while let Some(cause) = source {
            message.push_str(": ");
            message.push_str(&cause.to_string());
            source = cause.source();
        }
        plain_error(&message)
    }

    async fn fetch(client: &Client, url: &str, limit: usize) -> Result<String, String> {
        let mut response = client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(transport_error)?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }
        if response
            .content_length()
            .is_some_and(|len| len > limit as u64)
        {
            return Err("response too large".into());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            if chunk.len() > limit.saturating_sub(body.len()) {
                return Err("response too large".into());
            }
            body.extend_from_slice(&chunk);
        }
        body_text(status, Some(&body)).map(str::to_owned)
    }

    impl MapHttp {
        #[cfg(target_os = "android")]
        pub fn send(&mut self, id: LiveId, url: String) {
            match client_builder().build() {
                Ok(client) => self.send_with_client(id, url, client),
                Err(error) => {
                    self.cancel(id);
                    let (cancel, _) = oneshot::channel();
                    self.pending.insert(id, cancel);
                    let _ = self.replies.sender().send(Reply {
                        id,
                        body: Err(transport_error(error)),
                    });
                }
            }
        }

        fn send_with_client(&mut self, id: LiveId, url: String, client: Client) {
            self.cancel(id);
            let (cancel, cancelled) = oneshot::channel();
            self.pending.insert(id, cancel);
            let replies = self.replies.sender();
            let worker = std::thread::Builder::new()
                .name("octosmap-https".into())
                .spawn(move || {
                    let runtime = match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(runtime) => runtime,
                        Err(_) => {
                            let _ = replies.send(Reply {
                                id,
                                body: Err("could not start network request".into()),
                            });
                            return;
                        }
                    };
                    let body = runtime.block_on(async {
                        tokio::select! {
                            biased;
                            _ = cancelled => None,
                            body = fetch(&client, &url, MAX_BODY_BYTES) => Some(body),
                        }
                    });
                    // A platform DNS lookup may still be finishing in the
                    // blocking pool; it must not hold up worker shutdown.
                    runtime.shutdown_background();
                    if let Some(body) = body {
                        let _ = replies.send(Reply { id, body });
                    }
                });
            if worker.is_err() {
                let _ = self.replies.sender().send(Reply {
                    id,
                    body: Err("could not start network request".into()),
                });
            }
        }

        pub fn cancel(&mut self, id: LiveId) {
            self.pending.remove(&id);
        }

        pub fn drain(&mut self) -> Vec<Reply> {
            let mut replies = Vec::new();
            while let Ok(reply) = self.replies.try_recv() {
                // Cancellation also rejects a reply already queued for the UI.
                if self.pending.remove(&reply.id).is_some() {
                    replies.push(reply);
                }
            }
            replies
        }
    }

    #[cfg(test)]
    mod tests {
        include!("http_tests.rs");
    }
}
