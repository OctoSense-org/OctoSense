//! The shell's end of an app's kernel port (Makepad's `OctosUiPort`; ADR
//! 0003, "An app that is an octos client").
//!
//! An app that is itself an octos client (OctosCode) speaks the kernel's UI
//! protocol. Its module instance opens a port; the module host claims the
//! port for that instance and, when the app's `native-apps.json` entry names
//! a `kernel` port, connects it here: one kernel connection held to the
//! app's scope, which the kernel router enforces on every frame, served on
//! the kernel's own runtime (`octosense_kernel::serve_scoped`). A port the
//! entry does not grant is closed at once ([`refuse`]).
//!
//! The port outlives a kernel restart: the kernel connects again behind it
//! and the app is sent `Reset`, after which it opens its sessions again. Any
//! other end of the kernel closes the port, as does closing the instance
//! (dropping the bridge).

use makepad_ai_services::ui_port::{UiPortEvent, UiPortLink};

/// Close a port its app may not have, saying why.
pub fn refuse(link: UiPortLink, reason: &str) {
    link.down.send(UiPortEvent::Closed { reason: reason.to_string() });
}

/// A port connected to the kernel. Dropping it ends the connection.
pub struct KernelPortBridge {
    #[cfg(kernel)]
    _served: octosense_ai_host::kernel::PortHandle,
}

#[cfg(kernel)]
impl KernelPortBridge {
    /// Connect `link`, opened by an instance of `app`, held to `scope`.
    pub fn open(app: &str, link: UiPortLink, scope: std::sync::Arc<dyn octosense_ai_host::kernel::Scope>) -> KernelPortBridge {
        use octosense_ai_host::kernel::{self, PortEvent};
        let UiPortLink { up, down } = link;
        let deliver = Box::new(move |event: PortEvent| {
            down.send(match event {
                PortEvent::Frame(frame) => UiPortEvent::Frame(frame),
                PortEvent::Reset(reason) => UiPortEvent::Reset { reason },
                PortEvent::Closed(reason) => UiPortEvent::Closed { reason },
            })
        });
        KernelPortBridge { _served: kernel::serve_scoped(app, scope, up, deliver) }
    }
}
