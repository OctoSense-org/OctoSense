//! A test component that calls its app's host services through
//! `octosense:host` (crates/wasm-host/wit/octosense-host.wit).
wit_bindgen::generate!({ world: "hostcall", path: "wit", generate_all });

struct HostCall;

impl Guest for HostCall {
    fn call(service: String, args: String) -> Result<String, String> {
        octosense::host::services::request(&service, &args)
    }
}

export!(HostCall);
