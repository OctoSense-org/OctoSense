// relax check: "macro `json!` is not supported" at the json! line; rustc compiles it.
// globb = { path = "globb" } (lib.rs: `mod serde_json { pub struct SerJsonMarker; } pub use crate::serde_json::*;`)
mod with_private_serde_json_module {
    use globb::*;
    pub fn v() -> serde_json::Value { serde_json::json!({ "b": 2 }) }
}
fn main() { println!("{}", with_private_serde_json_module::v()); }
