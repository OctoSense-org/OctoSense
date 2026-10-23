//! Exercise actual wasm32-wasip2 std::net, using only isolated loopback peers.
use octosense_wasm_host::{
    component::{Grants, SocketBudget},
    CallError, Limits, Runtime,
};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

const GUEST: &[u8] = include_bytes!("fixtures/netprobe.component.wasm");
fn runtime() -> Runtime {
    Runtime::new(
        Limits {
            network_deadline: Duration::from_secs(2),
            ..Limits::default()
        },
        None,
    )
    .unwrap()
}
fn listener_port(value: Value) -> u16 {
    value.as_u64().unwrap() as u16
}
fn assert_closed(port: u16) {
    assert!(TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(100)
    )
    .is_err());
    TcpListener::bind(("127.0.0.1", port)).expect("listener's port should be released");
}

#[test]
fn tcp_udp_and_dns_work_without_a_network_grant() {
    let rt = runtime();
    let program = rt.load_component(GUEST).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = tcp.local_addr().unwrap().to_string();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = tcp.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut buf = [0; 5];
        stream.read_exact(&mut buf).unwrap();
        stream.write_all(&buf).unwrap();
    });
    assert_eq!(
        instance
            .call_json("tcp_echo", &json!({"addr":addr,"text":"hello"}))
            .unwrap(),
        json!("hello")
    );
    server.join().unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = udp.local_addr().unwrap().to_string();
    udp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let server = std::thread::spawn(move || {
        let mut bytes = [0; 128];
        let (n, peer) = udp.recv_from(&mut bytes).unwrap();
        udp.send_to(&bytes[..n], peer).unwrap();
    });
    assert_eq!(
        instance
            .call_json("udp_echo", &json!({"addr":addr,"text":"datagram"}))
            .unwrap(),
        json!("datagram")
    );
    server.join().unwrap();
    let addresses = instance.call_json("resolve", &json!("localhost")).unwrap();
    assert!(
        addresses
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "127.0.0.1" || v == "::1"),
        "{addresses}"
    );
}

#[test]
fn tcp_listener_accepts_and_drop_releases_the_port() {
    let rt = runtime();
    let program = rt.load_component(GUEST).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    let port = listener_port(instance.call_json("hold_listener", &json!(null)).unwrap());
    let client = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut buf = [0; 5];
        stream.read_exact(&mut buf).unwrap();
        stream.write_all(&buf).unwrap();
    });
    assert_eq!(
        instance.call_json("accept_echo", &json!("hello")).unwrap(),
        json!("hello")
    );
    client.join().unwrap();
    drop(instance);
    // An accepted TCP connection can leave TIME_WAIT, so test release with a
    // fresh listener that never accepted a connection.
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    let port = listener_port(instance.call_json("hold_listener", &json!(null)).unwrap());
    drop(instance);
    assert_closed(port);
}

#[test]
fn socket_limit_is_shared_across_components_and_close_refunds_it() {
    let rt = runtime();
    let program = rt.load_component(GUEST).unwrap();
    let budget = SocketBudget::default();
    let mut first = rt
        .instantiate_component_with_sockets(&program, &Grants::default(), None, budget.clone())
        .unwrap();
    let mut second = rt
        .instantiate_component_with_sockets(&program, &Grants::default(), None, budget)
        .unwrap();
    for _ in 0..16 {
        first.call_json("hold_listener", &json!(null)).unwrap();
        second.call_json("hold_listener", &json!(null)).unwrap();
    }
    assert!(matches!(
        second.call_json("hold_listener", &json!(null)),
        Err(CallError::Guest(_))
    ));
    // Another app is independent.
    let mut other = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    other.call_json("hold_listener", &json!(null)).unwrap();
    first.call_json("close_listeners", &json!(null)).unwrap();
    // More than a budget's worth of open/close cycles also checks resource
    // destructors, not just dropping the entire Wasmtime store.
    for _ in 0..64 {
        let port = listener_port(second.call_json("hold_listener", &json!(null)).unwrap());
        second.call_json("close_listeners", &json!(null)).unwrap();
        assert_closed(port);
    }
}

#[test]
fn cancellation_interrupts_a_blocked_tcp_read_and_closes_retained_sockets() {
    let rt = runtime();
    let program = rt.load_component(GUEST).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    let retained = listener_port(instance.call_json("hold_listener", &json!(null)).unwrap());
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = tcp.local_addr().unwrap().to_string();
    let pending = Arc::new(AtomicBool::new(true));
    let cancel = pending.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = tcp.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        cancel.store(false, Ordering::Release);
        assert_eq!(
            stream.read(&mut [0]).unwrap(),
            0,
            "cancelled guest should close its connection"
        );
    });
    let start = Instant::now();
    let result = instance.call_json_guarded(
        "read_forever",
        &json!(addr),
        start + Duration::from_secs(5),
        move || pending.load(Ordering::Acquire),
    );
    assert!(matches!(result, Err(CallError::Deadline)), "{result:?}");
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_closed(retained); // store is deliberately still alive
    server.join().unwrap();
}

#[test]
fn deadline_interrupts_an_empty_udp_receive() {
    let rt = runtime();
    let program = rt.load_component(GUEST).unwrap();
    let mut instance = rt
        .instantiate_component(&program, &Grants::default(), None)
        .unwrap();
    let retained = listener_port(instance.call_json("hold_listener", &json!(null)).unwrap());
    let start = Instant::now();
    let result = instance.call_json_guarded(
        "udp_receive",
        &json!(null),
        start + Duration::from_millis(100),
        || true,
    );
    assert!(matches!(result, Err(CallError::Deadline)), "{result:?}");
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_closed(retained);
}
