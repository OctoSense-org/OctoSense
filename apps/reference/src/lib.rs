//! The Reference app as a library: the sample widget and the module a host
//! links to seat it in-process. The binary (`main.rs`, feature
//! `standalone`) is the same widget under a `Window`.

pub use makepad_widgets;
use makepad_widgets::*;

pub mod module;
pub mod view;

pub use module::{ReferenceModule, REFERENCE_MODULE};
pub use view::ReferenceView;

/// This crate's widget family, into a VM whose widgets module exists.
pub fn register(vm: &mut ScriptVm) {
    view::script_mod(vm);
}
