use super::*;
use crate::{
    geo::LonLat,
    routing::{self, Mode},
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

const CERT: &[u8] = include_bytes!("../tests/fixtures/tls/localhost.der");
const KEY: &[u8] = include_bytes!("../tests/fixtures/tls/localhost-key.der");
const CAR: &str = include_str!("../tests/fixtures/osrm-car.json");

fn trusted_client() -> reqwest::Client {
    client_builder()
        .add_root_certificate(reqwest::Certificate::from_der(CERT).unwrap())
        .build()
        .unwrap()
}

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

// Only TLS 1.3 is accepted, matching the routing service. The stalled mode
// lets cancellation tests observe the actual network connection closing.
fn serve(response: Option<Vec<u8>>) -> (String, mpsc::Receiver<String>, mpsc::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "https://localhost:{}/route",
        listener.local_addr().unwrap().port()
    );
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![CertificateDer::from(CERT.to_vec())],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec())),
    )
    .unwrap();
    let (request_tx, request_rx) = mpsc::channel();
    let (closed_tx, closed_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let connection = rustls::ServerConnection::new(Arc::new(config)).unwrap();
        let mut tls = rustls::StreamOwned::new(connection, socket);
        let mut head = Vec::new();
        let mut byte = [0];
        while !head.ends_with(b"\r\n\r\n") && head.len() < 16384 {
            if tls.read_exact(&mut byte).is_err() {
                return;
            }
            head.push(byte[0]);
        }
        assert_eq!(
            tls.conn.protocol_version(),
            Some(rustls::ProtocolVersion::TLSv1_3)
        );
        let _ = request_tx.send(String::from_utf8(head).unwrap());
        if let Some(response) = response {
            let _ = tls.write_all(&response);
            let _ = tls.flush();
        } else {
            let _ = tls.read(&mut byte);
            let _ = closed_tx.send(());
        }
    });
    (url, request_rx, closed_rx)
}

fn ok(body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

#[test]
fn tls13_route_is_downloaded_and_parsed_with_service_headers() {
    let (url, request, _) = serve(Some(ok(CAR.as_bytes())));
    let body = run(fetch(&trusted_client(), &url, MAX_BODY_BYTES)).unwrap();
    assert!(!routing::parse(Mode::Car, &body)
        .unwrap()
        .route
        .points
        .is_empty());
    let head = request
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .to_lowercase();
    assert!(head.contains("user-agent: octosmap/0.1"));
    assert!(head.contains("accept: application/json"));
    assert!(
        routing::route_url(Mode::Car, LonLat::new(0.0, 0.0), LonLat::new(1.0, 1.0))
            .starts_with("https://")
    );
}

#[test]
fn untrusted_certificate_is_rejected() {
    let (url, _, _) = serve(Some(ok(b"{}")));
    let client = client_builder().build().unwrap();
    assert_eq!(
        run(fetch(&client, &url, MAX_BODY_BYTES)).unwrap_err(),
        "secure connection failed"
    );
}

#[test]
fn trusted_certificate_for_a_different_hostname_is_rejected() {
    let (url, _, _) = serve(Some(ok(b"{}")));
    let url = url.replace("localhost", "127.0.0.1");
    assert_eq!(
        run(fetch(&trusted_client(), &url, MAX_BODY_BYTES)).unwrap_err(),
        "secure connection failed"
    );
}

#[test]
fn cleartext_and_redirects_are_not_followed() {
    let client = trusted_client();
    assert!(run(fetch(&client, "http://127.0.0.1:9/route", MAX_BODY_BYTES)).is_err());
    let (url, _, _) = serve(Some(
        b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/route\r\nContent-Length: 0\r\n\r\n"
            .to_vec(),
    ));
    assert_eq!(
        run(fetch(&client, &url, MAX_BODY_BYTES)).unwrap_err(),
        "HTTP 302"
    );
}

#[test]
fn declared_and_chunked_bodies_cannot_exceed_the_limit() {
    let client = trusted_client();
    let (url, _, _) = serve(Some(ok(b"123456789")));
    assert!(run(fetch(&client, &url, 8))
        .unwrap_err()
        .contains("response too large"));
    let (url, _, _) = serve(Some(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n12345\r\n5\r\n67890\r\n0\r\n\r\n".to_vec()));
    assert!(run(fetch(&client, &url, 8))
        .unwrap_err()
        .contains("response too large"));
}

#[test]
fn invalid_utf8_and_unsuccessful_statuses_are_reported() {
    let client = trusted_client();
    let (url, _, _) = serve(Some(ok(&[255])));
    assert_eq!(
        run(fetch(&client, &url, MAX_BODY_BYTES)).unwrap_err(),
        "response is not UTF-8"
    );
    let (url, _, _) = serve(Some(
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n".to_vec(),
    ));
    assert_eq!(
        run(fetch(&client, &url, MAX_BODY_BYTES)).unwrap_err(),
        "HTTP 503"
    );
}

#[test]
fn a_stalled_response_times_out() {
    let (url, _, _) = serve(None);
    let client = client_builder()
        .add_root_certificate(reqwest::Certificate::from_der(CERT).unwrap())
        .timeout(Duration::from_millis(150))
        .build()
        .unwrap();
    let start = Instant::now();
    assert_eq!(
        run(fetch(&client, &url, MAX_BODY_BYTES)).unwrap_err(),
        "timed out"
    );
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn cancel_and_drop_close_in_flight_requests_without_a_reply() {
    for shutdown in [false, true] {
        let (url, request, closed) = serve(None);
        let mut transport = MapHttp::default();
        let id = LiveId(100);
        transport.send_with_client(id, url, trusted_client());
        request.recv_timeout(Duration::from_secs(2)).unwrap();
        if shutdown {
            drop(transport);
        } else {
            transport.cancel(id);
            closed.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(transport.drain().is_empty());
            continue;
        }
        closed.recv_timeout(Duration::from_secs(2)).unwrap();
    }
}

#[test]
fn a_completed_reply_keeps_its_request_id_and_cancelled_replies_are_discarded() {
    let (url, request, _) = serve(Some(ok(b"{}")));
    let mut transport = MapHttp::default();
    let id = LiveId(101);
    transport.send_with_client(id, url, trusted_client());
    request.recv_timeout(Duration::from_secs(2)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let mut replies = transport.drain();
        if let Some(reply) = replies.pop() {
            assert_eq!(reply.id, id);
            assert_eq!(reply.body.unwrap(), "{}");
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let (cancel, _) = tokio::sync::oneshot::channel();
    transport.pending.insert(id, cancel);
    transport
        .replies
        .sender()
        .send(Reply {
            id,
            body: Ok("stale".into()),
        })
        .unwrap();
    transport.cancel(id);
    assert!(transport.drain().is_empty());
}
