//! A component's outgoing `wasi:http`: to any host.
//!
//! An app's network declarations (`net`, `network.hosts`) are shown when it
//! is installed, not enforced while it runs: the architecture ruling of
//! 8 October 2026 removes the per-app runtime gates, and the OS and the
//! host's API surface are the boundary. (The app's Splash script still
//! passes the Splash runtime's URL gate until the Makepad fork drops it; a
//! component never gets one.)
//!
//! What the runtime still owes the call: a request waits for the network
//! outside the guest, where the deadline's epoch check cannot end it, so
//! its timeouts (connect, first byte, between bytes) are clamped to what is
//! left of the call.

use std::future::Future;
use std::time::{Duration, Instant};

use http_body_util::BodyExt;
use wasmtime_wasi_http::{Error, RequestOptions, WasiBody, WasiHttpHooks};

/// The running call's deadline, for the requests it sends.
#[derive(Debug, Default)]
pub(crate) struct Network {
    /// The running call's deadline, set for every call.
    pub(crate) deadline: Option<Instant>,
}

type Response = (
    http::Response<WasiBody>,
    Box<dyn Future<Output = Result<(), Error>> + Send>,
);

impl WasiHttpHooks for Network {
    fn send_request(
        &mut self,
        request: http::Request<WasiBody>,
        options: Option<RequestOptions>,
        fut: Box<dyn Future<Output = Result<(), Error>> + Send>,
    ) -> Box<dyn Future<Output = Result<Response, Error>> + Send> {
        _ = fut;
        let left = self
            .deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()));
        Box::new(async move {
            let options = clamped(options.unwrap_or_default(), left);
            let (response, io) =
                wasmtime_wasi_http::default_send_request(request, Some(options)).await?;
            Ok((
                response.map(BodyExt::boxed_unsync),
                Box::new(io) as Box<dyn Future<Output = Result<(), Error>> + Send>,
            ))
        })
    }
}

/// `options` with no timeout longer than `left` (and `left` where it sets
/// none).
fn clamped(options: RequestOptions, left: Option<Duration>) -> RequestOptions {
    let clamp = |asked: Option<Duration>| match (asked, left) {
        (Some(asked), Some(left)) => Some(asked.min(left)),
        (asked, left) => asked.or(left),
    };
    RequestOptions {
        connect_timeout: clamp(options.connect_timeout),
        first_byte_timeout: clamp(options.first_byte_timeout),
        between_bytes_timeout: clamp(options.between_bytes_timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No timeout outlives the call.
    #[test]
    fn a_requests_timeouts_end_with_the_call() {
        let left = Some(Duration::from_secs(3));
        let asked = RequestOptions {
            connect_timeout: Some(Duration::from_secs(60)),
            first_byte_timeout: Some(Duration::from_secs(1)),
            between_bytes_timeout: None,
        };
        let got = clamped(asked, left);
        assert_eq!(got.connect_timeout, Some(Duration::from_secs(3)));
        assert_eq!(got.first_byte_timeout, Some(Duration::from_secs(1)));
        assert_eq!(got.between_bytes_timeout, Some(Duration::from_secs(3)));
        assert_eq!(
            clamped(RequestOptions::default(), None).connect_timeout,
            None
        );
    }
}
