//! Call one function of an app's WebAssembly file from the command line, as
//! the shell's `wasm` service would, with no shell and no device:
//!
//! ```sh
//! cargo run -q -p octosense-wasm-host --example wasm_call -- \
//!     bundle/fns/text-tools.wasm count '{"text": "two words"}'
//! ```
//!
//! For a component (ADR 0014) the arguments are the JSON an app's script
//! passes to `wasm.<function>`: a bare value for one parameter, an object by
//! name or an array in order; the answer prints as JSON. For a core module
//! (ADR 0011) the arguments are the function's input as text, and its output
//! prints as text when it is UTF-8.
//!
//! `--storage DIR` gives a component `DIR` as its app's storage folder
//! (`wasi:filesystem`); without it there is none. Its `wasi:http` requests
//! go out as in the shell. Its `octosense:host` calls are answered with an
//! error: there are no host services here. Each call runs in a fresh
//! instance; the shell keeps a component's instance between calls.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use octosense_wasm_host::component::{self, Grants, HostCalls};
use octosense_wasm_host::{Limits, Runtime};
use serde_json::Value;

/// `octosense:host` without a shell.
struct NoHost;

impl HostCalls for NoHost {
    fn request(&self, service: &str, _args: &str, _deadline: Instant) -> Result<String, String> {
        Err(format!(
            "no host services in wasm_call: {service} needs a shell"
        ))
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: wasm_call FILE.wasm FUNCTION [ARGS] [--storage DIR]");
    eprintln!(
        "  ARGS: JSON for a component (default null), text for a core module (default empty)"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut positional = Vec::new();
    let mut storage = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--storage" => match args.next() {
                Some(dir) => storage = Some(PathBuf::from(dir)),
                None => return usage(),
            },
            "-h" | "--help" => return usage(),
            _ => positional.push(arg),
        }
    }
    let (file, function, input) = match positional.as_slice() {
        [file, function] => (file, function, None),
        [file, function, input] => (file, function, Some(input.as_str())),
        _ => return usage(),
    };
    match run(file, function, input, storage) {
        Ok(answer) => {
            println!("{answer}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("wasm_call: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(
    file: &str,
    function: &str,
    input: Option<&str>,
    storage: Option<PathBuf>,
) -> Result<String, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
    let runtime = Runtime::new(Limits::default(), None)?;
    if component::is_component(&bytes) {
        let program = runtime
            .load_component(&bytes)
            .map_err(|e| format!("{file}: {e}"))?;
        let args: Value = match input {
            None => Value::Null,
            Some(text) => serde_json::from_str(text)
                .map_err(|e| format!("the arguments are not JSON: {e}"))?,
        };
        let grants = Grants {
            storage_dir: storage,
            read_only: false,
        };
        let mut instance = runtime
            .instantiate_component(&program, &grants, None)
            .map_err(|e| e.to_string())?;
        instance.set_host_calls(Some(Arc::new(NoHost)));
        let result = instance.call_json(&component::snake(function), &args);
        for line in instance.take_logs() {
            eprintln!("log: {line}");
        }
        let answer = result.map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&answer).map_err(|e| e.to_string())
    } else {
        if storage.is_some() {
            return Err("a core module has no storage; --storage is for components".into());
        }
        let program = runtime.load(&bytes).map_err(|e| format!("{file}: {e}"))?;
        let mut instance = runtime.instantiate(&program).map_err(|e| e.to_string())?;
        let output = instance
            .call(function, input.unwrap_or("").as_bytes())
            .map_err(|e| e.to_string())?;
        Ok(match String::from_utf8(output) {
            Ok(text) => text,
            Err(bytes) => format!("{:?}", bytes.into_bytes()),
        })
    }
}
