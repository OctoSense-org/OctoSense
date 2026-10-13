//! Test real std::net calls, including retained sockets across invocations.
wit_bindgen::generate!({ world: "netprobe", path: "wit" });
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::Mutex;
static LISTENERS: Mutex<Vec<TcpListener>> = Mutex::new(Vec::new());
struct Probe;
impl Guest for Probe {
    // Avoid the bare export name `connect`: it collides with WASI libc.
    fn tcp_connect(addr: String) -> Result<(), String> {
        TcpStream::connect(addr)
            .map(drop)
            .map_err(|e| e.to_string())
    }
    fn tcp_echo(addr: String, text: String) -> Result<String, String> {
        (|| {
            let mut stream = TcpStream::connect(addr)?;
            stream.write_all(text.as_bytes())?;
            let mut result = vec![0; text.len()];
            stream.read_exact(&mut result)?;
            Ok(String::from_utf8_lossy(&result).into_owned())
        })()
        .map_err(|e: std::io::Error| e.to_string())
    }
    fn udp_echo(addr: String, text: String) -> Result<String, String> {
        (|| {
            let socket = UdpSocket::bind("127.0.0.1:0")?;
            socket.send_to(text.as_bytes(), addr)?;
            let mut result = [0; 1024];
            let (n, _) = socket.recv_from(&mut result)?;
            Ok(String::from_utf8_lossy(&result[..n]).into_owned())
        })()
        .map_err(|e: std::io::Error| e.to_string())
    }
    fn resolve(name: String) -> Result<Vec<String>, String> {
        (name.as_str(), 80)
            .to_socket_addrs()
            .map(|a| a.map(|a| a.ip().to_string()).collect())
            .map_err(|e| e.to_string())
    }
    fn hold_listener() -> Result<u16, String> {
        let socket = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = socket.local_addr().map_err(|e| e.to_string())?.port();
        LISTENERS.lock().unwrap().push(socket);
        Ok(port)
    }
    fn close_listeners() {
        LISTENERS.lock().unwrap().clear();
    }
    fn accept_echo(text: String) -> Result<String, String> {
        (|| {
            let listeners = LISTENERS.lock().unwrap();
            let (mut stream, _) = listeners[0].accept()?;
            stream.write_all(text.as_bytes())?;
            let mut result = vec![0; text.len()];
            stream.read_exact(&mut result)?;
            Ok(String::from_utf8_lossy(&result).into_owned())
        })()
        .map_err(|e: std::io::Error| e.to_string())
    }
    fn read_forever(addr: String) -> Result<(), String> {
        (|| TcpStream::connect(addr)?.read_exact(&mut [0]))().map_err(|e| e.to_string())
    }
    fn udp_receive() -> Result<(), String> {
        (|| {
            UdpSocket::bind("127.0.0.1:0")?
                .recv_from(&mut [0])
                .map(|_| ())
        })()
        .map_err(|e| e.to_string())
    }
}
export!(Probe);
