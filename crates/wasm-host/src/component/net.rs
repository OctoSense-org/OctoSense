//! A component's outgoing `wasi:http`: only to its app's own network hosts.
//!
//! The rule is a script's (makepad's `splash_policy::url_allowed`): a
//! request's host must be listed exactly, without regard to ASCII case, as
//! the app contract compares hosts; a listed `host` matches any port, and a
//! listed `host:port` only that port. Nothing else is reachable, and with no
//! hosts nothing at all, so a component may import `wasi:http` and still
//! reach nothing until its app is granted hosts. The request is HTTPS, as
//! the contract has it for an app's hosts, except to this device itself
//! (`localhost`, `127.0.0.1`, `[::1]`), where plain HTTP is allowed too.
//!
//! A request waits for the network outside the guest, where the deadline's
//! epoch check cannot end it, so its timeouts (connect, first byte, between
//! bytes) are clamped to what is left of the call.

use std::future::Future;
use std::time::{Duration, Instant};

use http_body_util::BodyExt;
use wasmtime_wasi_http::{Error, RequestOptions, WasiBody, WasiHttpHooks};

/// The hosts an instance may reach, and the running call's deadline.
#[derive(Debug, Default)]
pub(crate) struct Hosts {
    hosts: Vec<String>,
    /// The running call's deadline, set for every call.
    pub(crate) deadline: Option<Instant>,
    /// The requests refused since the caller last asked: the host, and why.
    refused: Vec<(String, &'static str)>,
}

impl Hosts {
    pub(crate) fn new(hosts: Vec<String>) -> Hosts {
        Hosts {
            hosts,
            ..Hosts::default()
        }
    }

    /// The requests refused since the last time this was asked: the host,
    /// and why.
    pub(crate) fn take_refused(&mut self) -> Vec<(String, &'static str)> {
        std::mem::take(&mut self.refused)
    }

    /// Whether a request to `uri` may go out, or why not.
    fn allows(&self, uri: &http::Uri) -> Result<(), &'static str> {
        const NOT_LISTED: &str = "it is not one of the app's network hosts";
        let host = uri.host().ok_or(NOT_LISTED)?.to_ascii_lowercase();
        let host_port = uri.port_u16().map(|port| format!("{host}:{port}"));
        let listed = self.hosts.iter().any(|listed| {
            listed.eq_ignore_ascii_case(&host)
                || host_port
                    .as_ref()
                    .is_some_and(|hp| listed.eq_ignore_ascii_case(hp))
        });
        if !listed {
            return Err(NOT_LISTED);
        }
        let this_device = matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]");
        if uri.scheme() != Some(&http::uri::Scheme::HTTPS) && !this_device {
            return Err("it is plain HTTP; a component's requests use HTTPS");
        }
        Ok(())
    }
}

type Response = (
    http::Response<WasiBody>,
    Box<dyn Future<Output = Result<(), Error>> + Send>,
);

impl WasiHttpHooks for Hosts {
    fn send_request(
        &mut self,
        request: http::Request<WasiBody>,
        options: Option<RequestOptions>,
        fut: Box<dyn Future<Output = Result<(), Error>> + Send>,
    ) -> Box<dyn Future<Output = Result<Response, Error>> + Send> {
        _ = fut;
        let allowed = self.allows(request.uri());
        if let Err(why) = allowed {
            let uri = request.uri();
            let host = uri
                .authority()
                .map(|a| a.to_string())
                .unwrap_or_else(|| uri.to_string());
            self.refused.push((host, why));
        }
        let allowed = allowed.is_ok();
        let left = self
            .deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()));
        Box::new(async move {
            if !allowed {
                return Err(Error::HttpRequestDenied);
            }
            let clamp = |asked: Option<Duration>| match (asked, left) {
                (Some(asked), Some(left)) => Some(asked.min(left)),
                (asked, left) => asked.or(left),
            };
            let options = options.unwrap_or_default();
            let options = RequestOptions {
                connect_timeout: clamp(options.connect_timeout),
                first_byte_timeout: clamp(options.first_byte_timeout),
                between_bytes_timeout: clamp(options.between_bytes_timeout),
            };
            let (response, io) =
                wasmtime_wasi_http::default_send_request(request, Some(options)).await?;
            Ok((
                response.map(BodyExt::boxed_unsync),
                Box::new(io) as Box<dyn Future<Output = Result<(), Error>> + Send>,
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allows(hosts: &[&str], uri: &str) -> bool {
        Hosts::new(hosts.iter().map(|h| h.to_string()).collect())
            .allows(&uri.parse().unwrap())
            .is_ok()
    }

    /// A script's rule: a listed host matches any port and any case, a
    /// listed host:port only that port, and nothing else matches.
    #[test]
    fn a_request_reaches_only_a_listed_host() {
        assert!(allows(&["api.example.com"], "https://api.example.com/v1"));
        assert!(allows(
            &["api.example.com"],
            "https://API.Example.com:8443/"
        ));
        assert!(allows(&["127.0.0.1:8141"], "http://127.0.0.1:8141/x"));
        assert!(!allows(&["127.0.0.1:8141"], "http://127.0.0.1:8142/x"));
        assert!(!allows(&["127.0.0.1:8141"], "http://127.0.0.1/x"));
        assert!(!allows(&["api.example.com"], "https://evil.example.com/"));
        assert!(!allows(&["example.com"], "https://api.example.com/"));
        assert!(!allows(
            &["api.example.com"],
            "https://api.example.com.evil.net/"
        ));
        assert!(!allows(&[], "https://api.example.com/"));
        assert!(!allows(&["api.example.com"], "/relative"));
    }

    /// HTTPS, except to this device.
    #[test]
    fn plain_http_reaches_only_this_device() {
        assert!(!allows(&["api.example.com"], "http://api.example.com/"));
        assert!(allows(&["localhost"], "http://localhost:8080/"));
        assert!(allows(&["127.0.0.1"], "http://127.0.0.1:8080/"));
        assert!(allows(&["[::1]"], "http://[::1]:8080/"));
        assert!(!allows(&["192.168.1.2"], "http://192.168.1.2/"));
    }
}
