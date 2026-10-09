//! A test component that imports wasi:sockets through std::net.
wit_bindgen::generate!({ world: "netprobe", path: "wit" });

struct Probe;

impl Guest for Probe {
    fn connect(addr: String) -> Result<(), String> {
        std::net::TcpStream::connect(addr).map(drop).map_err(|e| e.to_string())
    }
}

export!(Probe);
