//! A test component that reaches the network over `wasi:http`, the way a
//! crate built for wasm32-wasip2 does, with WASI 0.2's own bindings.
use wasi::http::outgoing_handler;
use wasi::http::types::{Fields, Method, OutgoingRequest, Scheme};
use wasi::io::streams::StreamError;

wit_bindgen::generate!({ world: "fetch", path: "wit" });

struct Fetch;

impl Guest for Fetch {
    fn get(url: String) -> Result<String, String> {
        let (scheme, rest) = url.split_once("://").ok_or("the URL has no scheme")?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let request = OutgoingRequest::new(Fields::new());
        request.set_method(&Method::Get).map_err(|()| "method")?;
        let scheme = if scheme == "https" { Scheme::Https } else { Scheme::Http };
        request.set_scheme(Some(&scheme)).map_err(|()| "scheme")?;
        request.set_authority(Some(authority)).map_err(|()| "authority")?;
        request.set_path_with_query(Some(path)).map_err(|()| "path")?;
        let pending = outgoing_handler::handle(request, None).map_err(|e| format!("{e:?}"))?;
        pending.subscribe().block();
        let response = pending
            .get()
            .ok_or("no response")?
            .map_err(|()| "the response was taken")?
            .map_err(|e| format!("{e:?}"))?;
        let status = response.status();
        let body = response.consume().map_err(|()| "no body")?;
        let stream = body.stream().map_err(|()| "no body stream")?;
        let mut bytes = Vec::new();
        loop {
            match stream.blocking_read(64 << 10) {
                Ok(chunk) => bytes.extend_from_slice(&chunk),
                Err(StreamError::Closed) => break,
                Err(e) => return Err(format!("{e:?}")),
            }
        }
        Ok(format!("{status} {}", String::from_utf8_lossy(&bytes)))
    }
}

export!(Fetch);
