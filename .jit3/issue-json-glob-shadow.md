# relax: a glob re-export of a private module's items also shadows the module's NAME, so `serde_json::json!` stops resolving

## What happens
A crate that keeps its own module named `serde_json` private and re-exports only its items:

```rust
// globb/src/lib.rs  (this is the shape of makepad_micro_serde: `mod serde_json; pub use crate::serde_json::*;`)
mod serde_json { pub struct SerJsonMarker; }
pub use crate::serde_json::*;
```

used through a glob in a crate that also depends on the real `serde_json`:

```rust
use globb::*;
pub fn v() -> serde_json::Value { serde_json::json!({ "b": 2 }) }
```

`cargo check` accepts it (the private module is not exported; `serde_json` is the extern crate). `relax check` reports

```
internal: macro `json!` is not supported   (at the json! call)
```

`serde_json::Value` on the same line resolves, only the macro path fails. Writing `::serde_json::json!` works, so relax resolves the `serde_json` segment of the macro path through the glob to the private module instead of the extern crate.

## Where it bites
Every `use makepad_widgets::*;` file that calls `serde_json::json!` (makepad_widgets → … → makepad_micro_serde has exactly this private `mod serde_json` + `pub use crate::serde_json::*`). In our shell that is 15 call sites in two files; the other files either do not glob makepad_widgets or only use `json!` in tests.

## Repro
Two crates, 12 lines, attached (`globb/` + `main.rs`): `relax check` on the bin → 1 error; `cargo check` → clean. relax from makepad `work` 60abec8ba.
