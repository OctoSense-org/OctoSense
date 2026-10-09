//! A component's outgoing `wasi:http`: only to its app's own network hosts.
//!
//! The rule is a script's (makepad's `splash_policy::url_allowed`): a
//! request's host must be listed exactly, without regard to ASCII case, as
//! the app contract compares hosts; a listed `host` matches any port, and a
//! listed `host:port` only that port. Nothing else is reachable, and with no
//! hosts nothing at all, so a component may import `wasi:http` and still
//! reach nothing until its app is granted hosts. The request is HTTPS, as
//! the contract has it for an app's hosts, and it never goes to this device
//! or its local network — loopback, private, link-local and similar
//! addresses, `localhost` and `.local`-style or single-label names — even
//! when the app lists one: the services there (the shell's own, a
//! developer's) are no app's to reach. A test or a developer's run may allow
//! them ([`super::Grants::http_local`]); the shell never does. A public name
//! that resolves to a local address is not caught here (as for scripts).
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
    /// This device and its local network too, over plain HTTP as well:
    /// tests and developers' runs only.
    local: bool,
    /// The running call's deadline, set for every call.
    pub(crate) deadline: Option<Instant>,
    /// The requests refused since the caller last asked: the host, and why.
    refused: Vec<(String, &'static str)>,
}

impl Hosts {
    pub(crate) fn new(hosts: Vec<String>, local: bool) -> Hosts {
        Hosts {
            hosts,
            local,
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
        let local = is_local(&host);
        if local && !self.local {
            return Err("it is this device or its local network, which a component never reaches");
        }
        if uri.scheme() != Some(&http::uri::Scheme::HTTPS) && !(local && self.local) {
            return Err("it is plain HTTP; a component's requests use HTTPS");
        }
        Ok(())
    }
}

/// Whether `host` (lowercased, as a URI writes it) names this device or its
/// local network: makepad's `splash_policy::public_https_host`, inverted.
fn is_local(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<std::net::IpAddr>() {
        return !is_public_ip(ip);
    }
    // An IP in a form only some resolvers read as one (`0x7f.1`, `127.1`),
    // or a single-label name, is not a public host either.
    let last = host.rsplit('.').next().unwrap_or("");
    let numeric = last.bytes().all(|b| b.is_ascii_digit()) || last.starts_with("0x");
    numeric
        || !host.contains('.')
        || host.starts_with('[')
        || host == "localhost"
        || [".localhost", ".internal", ".local", ".lan", ".home.arpa"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

fn is_public_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || a == 0
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80)
        }
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

    fn allows_with(local: bool, hosts: &[&str], uri: &str) -> bool {
        Hosts::new(hosts.iter().map(|h| h.to_string()).collect(), local)
            .allows(&uri.parse().unwrap())
            .is_ok()
    }

    fn allows(hosts: &[&str], uri: &str) -> bool {
        allows_with(false, hosts, uri)
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
        assert!(allows(
            &["api.example.com:8443"],
            "https://api.example.com:8443/"
        ));
        assert!(!allows(
            &["api.example.com:8443"],
            "https://api.example.com:9443/"
        ));
        assert!(!allows(&["api.example.com"], "https://evil.example.com/"));
        assert!(!allows(&["example.com"], "https://api.example.com/"));
        assert!(!allows(
            &["api.example.com"],
            "https://api.example.com.evil.net/"
        ));
        assert!(!allows(&[], "https://api.example.com/"));
        assert!(!allows(&["api.example.com"], "/relative"));
    }

    /// HTTPS only, and never this device or its local network, even listed.
    #[test]
    fn a_request_never_reaches_this_device_or_its_network() {
        assert!(!allows(&["api.example.com"], "http://api.example.com/"));
        for (listed, url) in [
            ("localhost", "https://localhost:8080/"),
            ("127.0.0.1", "https://127.0.0.1:47631/"),
            ("[::1]", "https://[::1]/"),
            ("10.0.0.2", "https://10.0.0.2/"),
            ("192.168.1.2", "https://192.168.1.2/"),
            ("169.254.169.254", "https://169.254.169.254/"),
            ("[fd00::1]", "https://[fd00::1]/"),
            ("printer.local", "https://printer.local/"),
            ("router.lan", "https://router.lan/"),
            ("db.internal", "https://db.internal/"),
            ("intranet", "https://intranet/"),
            ("0x7f.1", "https://0x7f.1/"),
            ("127.1", "https://127.1/"),
        ] {
            assert!(!allows(&[listed], url), "{url}");
        }
        assert!(allows(&["93.184.215.14"], "https://93.184.215.14/"));
    }

    /// A test or a developer's run may reach this device, over plain HTTP
    /// too; still only the listed hosts.
    #[test]
    fn local_runs_may_reach_this_device() {
        assert!(allows_with(true, &["127.0.0.1"], "http://127.0.0.1:8080/"));
        assert!(allows_with(true, &["localhost"], "http://localhost:8080/"));
        assert!(!allows_with(true, &["127.0.0.1"], "http://localhost:8080/"));
        assert!(!allows_with(
            true,
            &["api.example.com"],
            "http://api.example.com/"
        ));
    }
}
