//! The Python runner inserts the real Android batch loop, not a second implementation.
use std::sync::mpsc::{channel, Receiver};
mod event { pub mod finger {
    #[derive(Clone, Copy, PartialEq)] pub enum TouchState { Start, Move, Stop }
} }
use event::finger::TouchState;
struct Touch { state: TouchState, serial: u8 }
enum FromJavaMessage { RenderLoop, Wake, Touch(Vec<Touch>), Clear, Query }
struct Host { pending_clear: bool, editable: &'static str, events: Vec<String> }
impl Host {
    fn handle_message(&mut self, message: FromJavaMessage) {
        match message {
            FromJavaMessage::Clear => { self.pending_clear=true; self.events.push("clear requested".into()); }
            FromJavaMessage::Query => self.events.push(format!("query {}", self.editable)),
            FromJavaMessage::Touch(touches) => self.events.push(format!("touch {}", touches[0].serial)),
            _ => (),
        }
    }
    fn handle_platform_ops(&mut self) {
        if self.pending_clear { self.pending_clear=false; self.editable="empty"; self.events.push("clear applied".into()); }
    }
    fn drain(&mut self, from_java_rx: &Receiver<FromJavaMessage>) {
        let mut vsync=false;
        /* ANDROID_BATCH */
        assert!(vsync);
    }
}
fn main() {
    let (sender, receiver)=channel();
    for message in [
        FromJavaMessage::Touch(vec![Touch{state:TouchState::Start,serial:1}]),
        FromJavaMessage::Touch(vec![Touch{state:TouchState::Move,serial:2}]),
        FromJavaMessage::Touch(vec![Touch{state:TouchState::Move,serial:3}]),
        FromJavaMessage::RenderLoop, FromJavaMessage::Wake,
        FromJavaMessage::Clear, FromJavaMessage::Query,
        FromJavaMessage::Touch(vec![Touch{state:TouchState::Move,serial:4}]),
        FromJavaMessage::Touch(vec![Touch{state:TouchState::Stop,serial:5}]),
        FromJavaMessage::Query,
    ] { sender.send(message).unwrap(); }
    let mut host=Host{pending_clear:false,editable:"old",events:vec![]};
    host.drain(&receiver);
    assert_eq!(host.events, ["touch 1","touch 3","clear requested","clear applied","query empty","touch 4","touch 5","query empty"]);
}
