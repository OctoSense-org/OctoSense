//! The in-process leg of the peer link (ADR 0004 §5, OctoSense #142).
//!
//! An app reaches its agent through Makepad's peer client,
//! `makepad_ai_services::peer::OctosPeer`, whether a shell hosts it as a
//! process or in-process as a module. Hosted as a process, the client's
//! frames ride the hub socket and the shell's peer link reads them as JSON.
//! In-process, `OctosPeer::open` parks the host's end of a channel pair
//! (`PeerLink`) on `Cx` (`PendingPeerLinks`) and the module host takes it
//! right after the module code that opened it ran.
//!
//! [`ModulePeerLink`] turns that channel pair into the same JSON frames, so
//! the shell serves a module's link with exactly the code that serves a
//! process's: requests, contexts, tool calls, host obligations, a closed
//! instance. It adds no identity: the module host attributes a link to the
//! instance whose code opened it, as it attributes a socket to the process
//! it launched.
//!
//! A frame the client cannot parse is dropped here, as the client itself
//! drops it on the hub socket. `conversation` frames (both lanes of the
//! app's conversation) pass, since Makepad's client reads them.

use makepad_ai_services::peer::{PeerDown, PeerLink, PeerUp};
use makepad_widgets::SignalToUI;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Sends one peer frame (JSON) down to the module; `Send + Sync`, like the
/// shell's frame sender for a socket, because turn events stream from
/// provider threads.
pub type FrameSink = Arc<dyn Fn(String) + Send + Sync>;

/// The host's end of one module's peer link, as JSON frames.
pub struct ModulePeerLink {
    up: Receiver<PeerUp>,
    down: Arc<Mutex<Sender<PeerDown>>>,
}

impl ModulePeerLink {
    pub fn new(link: PeerLink) -> ModulePeerLink {
        ModulePeerLink { up: link.up, down: Arc::new(Mutex::new(link.down)) }
    }

    /// What the module sent since the last call, as the frames a process
    /// would have written on its socket.
    pub fn take_up(&self) -> Vec<String> {
        self.up.try_iter().map(|up| up.to_json()).collect()
    }

    /// The sender the shell's peer link writes this module's frames to. A
    /// frame the client cannot read is dropped; each one delivered wakes
    /// the UI thread, where the module drains its link.
    pub fn frames_down(&self) -> FrameSink {
        let down = self.down.clone();
        Arc::new(move |frame: String| {
            let Some(parsed) = PeerDown::parse(&frame) else { return };
            let sent = down.lock().unwrap_or_else(|e| e.into_inner()).send(parsed).is_ok();
            if sent {
                SignalToUI::set_ui_signal();
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_ai_services::peer::{OctosPeer, PeerCaller, PeerToolOutcome};
    use std::sync::mpsc::channel;

    #[test]
    fn should_hand_the_shell_the_frames_a_process_would_write_when_a_module_requests() {
        let (mut peer, link) = OctosPeer::in_process();
        let link = ModulePeerLink::new(link);
        let open = peer.open_session(None);
        let turn = peer.start_turn("pl3-1", "hello").unwrap();
        let frames = link.take_up();
        assert_eq!(frames.len(), 2);
        // The exact wire of a hosted process (makepad's fixture).
        assert_eq!(frames[1], format!(r#"{{"octos_peer":{{"up":"request","req_id":{turn},"method":"octos.turn.start","args":{{"context":"pl3-1","text":"hello"}}}}}}"#));
        assert!(frames[0].contains(&format!(r#""req_id":{open}"#)) && frames[0].contains("octos.session.open"));
        assert!(link.take_up().is_empty(), "each frame once");
    }

    #[test]
    fn should_carry_a_tool_result_up_when_the_module_answers_a_call() {
        let (up_tx, up_rx) = channel();
        let (down_tx, _down_rx) = channel();
        let link = ModulePeerLink::new(PeerLink { up: up_rx, down: down_tx });
        up_tx.send(PeerUp::ToolResult { call_id: "c1".into(), outcome: PeerToolOutcome::AwaitingConfirmation }).unwrap();
        assert_eq!(link.take_up(), vec![r#"{"octos_peer":{"up":"tool_result","call_id":"c1","ok":false,"awaiting_confirmation":true}}"#.to_string()]);
    }

    #[test]
    fn should_deliver_the_shells_frames_to_the_module_when_they_parse() {
        let (_up_tx, up_rx) = channel();
        let (down_tx, down_rx) = channel();
        let link = ModulePeerLink::new(PeerLink { up: up_rx, down: down_tx });
        let down = link.frames_down();
        // What the shell's `peer_link::wire` writes (serde_json key order).
        down(r#"{"octos_peer":{"data":{"context":"pl3-1"},"down":"reply","ok":true,"req_id":1}}"#.to_string());
        down(r#"{"octos_peer":{"account":"device","args":{},"call_id":"c1","caller":"own_agent","client":null,"confirm_required":false,"context_id":null,"down":"tool_call","name":"lookup","risk":"read","timeout_ms":30000}}"#.to_string());
        // The conversation (both lanes) the shell follows for the app.
        down(r#"{"octos_peer":{"context":"pl3-1","down":"conversation","event":{"method":"message/delta","lane":"system_agent","params":{}}}}"#.to_string());
        // Anything that is not a peer frame is dropped, not an error.
        down("not a frame".to_string());
        let got: Vec<PeerDown> = down_rx.try_iter().collect();
        assert_eq!(got.len(), 3, "{got:?}");
        assert!(matches!(&got[2], PeerDown::Conversation { context, .. } if context == "pl3-1"));
        assert!(matches!(&got[0], PeerDown::Reply { req_id: 1, result: Ok(_) }));
        match &got[1] {
            PeerDown::ToolCall(call) => assert_eq!((call.name.as_str(), call.caller.clone(), call.account.as_deref()), ("lookup", PeerCaller::OwnAgent, Some("device"))),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn should_drop_frames_quietly_when_the_module_is_gone() {
        let (_up_tx, up_rx) = channel();
        let (down_tx, down_rx) = channel::<PeerDown>();
        let link = ModulePeerLink::new(PeerLink { up: up_rx, down: down_tx });
        drop(down_rx);
        // A provider thread may still stream a turn's events after the
        // instance closed: nothing panics.
        (link.frames_down())(r#"{"octos_peer":{"down":"reply","ok":true,"req_id":1,"data":null}}"#.to_string());
    }
}
