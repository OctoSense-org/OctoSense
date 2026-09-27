use std::{cell::RefCell, ops::Range, rc::Rc};
#[path="/* ENGINE_PATH */"] mod android_ime;
#[derive(Clone, Copy, Debug, Default)] struct CharOffset(usize);
impl CharOffset {
    fn to_utf16_index(self, text: &str) -> usize {text.chars().take(self.0).map(char::len_utf16).sum()}
    fn from_utf16_index(text: &str, index: usize) -> Self {
        let mut units=0; let mut count=0;
        for c in text.chars() {if units+c.len_utf16()>index {break;} units+=c.len_utf16(); count+=1;}
        Self(count)
    }
}
#[derive(Clone, Debug, Default)] struct FullTextState {text:String, selection:Range<CharOffset>, composition:Option<Range<CharOffset>>}
#[derive(Default)] struct TextInputEvent {full_state_sync:Option<FullTextState>}
struct ImeAction(i32); impl ImeAction {fn from_android_action_code(code:i32)->Self{Self(code)}}
struct ImeActionEvent {action:ImeAction}
enum Event {ImeAction(ImeActionEvent),TextInputStateQuery(Rc<RefCell<Option<(u64,FullTextState)>>>), TextInput(TextInputEvent)}
#[derive(Default)] struct Os {ime_resumed:bool,ime_last_operation:u64,ime_input_sequence:u64,ime_editor_session:u64,ime_editor_identity:u64,ime_focus_sequence:u64}
mod android_jni {
    use super::*;
    thread_local! {pub static ACKS:RefCell<Vec<(u64,u64,bool,String)>>=RefCell::new(Vec::new());}
    pub unsafe fn to_java_update_ime_text_state(sequence:u64,session:u64,active:bool,text:&str,_a:i32,_b:i32,_c:i32,_d:i32) {
        ACKS.with(|v|v.borrow_mut().push((sequence,session,active,text.into())));
    }
}
mod host {
    use super::*;
    pub struct Cx {pub os:Os,pub widget:Option<(u64,FullTextState)>,pub submitted:Vec<(u64,i32)>}
    impl Cx {
        fn call_event_handler(&mut self,event:&Event) {
            match event {
                Event::ImeAction(action)=>if let Some((uid,_))=self.widget.as_ref(){self.submitted.push((*uid,action.action.0));},
                Event::TextInputStateQuery(response)=>*response.borrow_mut()=self.widget.clone(),
                Event::TextInput(event)=>if let (Some((_,state)),Some(next))=(&mut self.widget,&event.full_state_sync) {*state=next.clone();},
            }
        }
        /* REAL_EDITOR_METHODS */
        pub fn action(&mut self,sequence:u64,session:u64,hardware:bool,code:i32) {
            self.native_editor_operation(sequence,session,hardware,android_ime::Edit{kind:13,text:String::new(),start:code,end:0,cursor:0});
        }
        pub fn publish(&mut self) {self.publish_native_editor();}
        pub fn edit(&mut self,sequence:u64,session:u64,hardware:bool,kind:i32,text:&str) {
            self.native_editor_operation(sequence,session,hardware,android_ime::Edit{kind,text:text.into(),start:0,end:0,cursor:1});
        }
    }
}
fn state(text:&str)->FullTextState {
    let end=CharOffset(text.chars().count()); FullTextState{text:text.into(),selection:end..end,composition:None}
}
fn main() {
    let mut cx=host::Cx{os:Os{ime_resumed:true,..Os::default()},widget:Some((42,state("bluetooth"))),submitted:Vec::new()};
    cx.publish();let session=cx.os.ime_editor_session;
    // The clear is canonical, while Java still optimistically contains the old
    // word. The following commands cannot reintroduce that untransmitted word.
    cx.widget.as_mut().unwrap().1=state("");
    for (index,c) in "wifi".chars().enumerate() {cx.edit(index as u64+1,session,true,1,&c.to_string());}
    assert_eq!(cx.widget.as_ref().unwrap().1.text,"wifi");
    // A marker can reach Rust before its operation. It must not acknowledge
    // that operation and let an earlier programmatic change erase it in Java.
    cx.os.ime_input_sequence=5;cx.publish();
    android_jni::ACKS.with(|v|assert_eq!(v.borrow().last().unwrap().0,4));
    let end=CharOffset(4);cx.widget.as_mut().unwrap().1.selection=CharOffset(0)..end;
    cx.edit(5,session,true,1,"b");
    for (index,c) in "ookkeeper".chars().enumerate(){cx.edit(index as u64+6,session,true,1,&c.to_string());}
    assert_eq!(cx.widget.as_ref().unwrap().1.text,"bookkeeper");
    // Same widget identity survives redraw/theme/layout; composing text keeps
    // replacing the composing range rather than inserting entire snapshots.
    cx.widget.as_mut().unwrap().1=state("");cx.publish();assert_eq!(cx.os.ime_editor_session,session);
    cx.edit(20,session,false,2,"n");cx.edit(21,session,false,2,"ni");cx.edit(22,session,false,1,"你");
    assert_eq!(cx.widget.as_ref().unwrap().1.text,"你");
    // A new field is a different editor even though Android reuses the Surface.
    cx.widget=Some((43,state("new-field")));cx.publish();let next=cx.os.ime_editor_session;
    assert_ne!(next,session);cx.edit(23,session,false,2,"stale");
    assert_eq!(cx.widget.as_ref().unwrap().1.text,"new-field");
    cx.edit(24,next,false,1,"!");assert_eq!(cx.widget.as_ref().unwrap().1.text,"new-field!");
    // Replayed operations and background input cannot mutate the current field.
    cx.edit(24,next,false,1,"duplicate");cx.os.ime_resumed=false;cx.edit(25,next,false,1,"background");
    assert_eq!(cx.widget.as_ref().unwrap().1.text,"new-field!");
    cx.os.ime_resumed=true;cx.widget=None;
    cx.edit(26,next,false,1,"editor gone");
    android_jni::ACKS.with(|v|{let a=v.borrow();let ack=a.last().unwrap();assert_eq!(ack.0,26);assert!(!ack.2);});
    let mut reset=android_ime::EditorState{text:String::new(),selection:(0,0),composition:None};
    assert!(reset.apply(&android_ime::Edit{kind:2,text:"blueh".into(),start:0,end:0,cursor:1}));
    assert_eq!(reset.text,"blueh"); // Native EditText payload semantics; no prefix guessing.
    cx.widget=Some((44,state("action target")));cx.publish();let action_session=cx.os.ime_editor_session;
    cx.action(30,action_session,false,6);assert_eq!(cx.submitted,vec![(44,6)]);
    // Pointer B has already reached Rust, while Java still holds A's session.
    // Neither an ordinary nor hardware-marked editor action may submit B.
    cx.widget=Some((45,state("different target")));cx.publish();
    cx.action(31,action_session,false,6);cx.action(32,action_session,true,6);
    assert_eq!(cx.submitted,vec![(44,6)]);
    cx.action(33,cx.os.ime_editor_session,false,3);assert_eq!(cx.submitted,vec![(44,6),(45,3)]);
    cx.action(34,cx.os.ime_editor_session,false,1000);assert_eq!(cx.submitted.len(),2);
    // Unicode scalar deletion, including a selected range, cannot split UTF16.
    let mut text=android_ime::EditorState{text:"a😀中文".into(),selection:(3,3),composition:None};
    assert!(text.apply(&android_ime::Edit{kind:11,text:String::new(),start:0,end:0,cursor:0}));assert_eq!(text.text,"a中文");
    text.selection=(0,3);assert!(text.apply(&android_ime::Edit{kind:11,text:String::new(),start:0,end:0,cursor:0}));assert_eq!(text.text,"");
    println!("Canonical editor ordering, composition, sessions, retirement and Unicode passed");
}
