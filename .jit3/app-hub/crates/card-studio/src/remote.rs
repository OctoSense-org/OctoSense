//! A GET against the makepad remote instrument on 127.0.0.1.
//!
//! The instrument is loopback-only HTTP/1.1 answering one small reply per
//! request, so a plain `TcpStream` does; no HTTP client is pulled in.
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// GET `path` (with its query) on `127.0.0.1:port`; the body as bytes.
pub fn get(port: u16, path: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let mut stream =
        TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("connect :{port}: {e}"))?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|e| format!("GET {path}: {e}"))?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| format!("GET {path}: {e}"))?;
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| format!("GET {path}: no HTTP header"))?;
    let head = String::from_utf8_lossy(&raw[..split]).to_lowercase();
    let body = raw[split + 4..].to_vec();
    if head.contains("transfer-encoding: chunked") {
        return dechunk(&body).ok_or_else(|| format!("GET {path}: bad chunked body"));
    }
    Ok(body)
}

/// GET and parse JSON; an `{"err": …}` reply is an error.
pub fn get_json(port: u16, path: &str, timeout: Duration) -> Result<serde_json::Value, String> {
    let body = get(port, path, timeout)?;
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
        format!(
            "GET {path}: not JSON ({e}): {}",
            String::from_utf8_lossy(&body)
        )
    })?;
    if let Some(err) = value.get("err") {
        return Err(format!("GET {path}: {err}"));
    }
    Ok(value)
}

fn dechunk(mut body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = body.windows(2).position(|w| w == b"\r\n")?;
        let size = usize::from_str_radix(
            std::str::from_utf8(&body[..line_end])
                .ok()?
                .split(';')
                .next()?
                .trim(),
            16,
        )
        .ok()?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Some(out);
        }
        out.extend_from_slice(body.get(..size)?);
        body = body.get(size + 2..)?;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dechunks() {
        assert_eq!(
            super::dechunk(b"3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n").unwrap(),
            b"abcde"
        );
    }
}
