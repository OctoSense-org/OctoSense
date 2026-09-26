use std::{cell::RefCell,collections::VecDeque};
enum CxOsOp {ShowTextIME((),(),i32)}
struct Keyboard {text_ime_dismissed:bool}
struct Os {last_ime_config:Option<i32>}
struct Cx {keyboard:Keyboard,os:Os,ops:VecDeque<CxOsOp>}
mod android_jni {
    use super::*;
    thread_local! {pub static SHOWS:RefCell<Vec<bool>>=RefCell::new(Vec::new());}
    pub unsafe fn to_java_configure_keyboard(_config:&i32){}
    pub unsafe fn to_java_show_keyboard(show:bool){SHOWS.with(|v|v.borrow_mut().push(show));}
}
impl Cx {
    fn native_editor_state(&mut self){}
    fn dispatch(&mut self) {
        while let Some(op)=self.ops.pop_front() {
            match op {/* REAL_SHOW_ARM */}
        }
    }
}
fn main() {
    let mut cx=Cx{keyboard:Keyboard{text_ime_dismissed:false},os:Os{last_ime_config:Some(1)},ops:VecDeque::new()};
    // A draw requested Show before Gboard delivered its final hidden inset.
    // DidHide retires the cached configuration and records user dismissal;
    // the already queued request must be revalidated when dispatched.
    cx.ops.push_back(CxOsOp::ShowTextIME((),(),1));
    cx.keyboard.text_ime_dismissed=true;cx.os.last_ime_config=None;
    cx.dispatch();android_jni::SHOWS.with(|v|assert!(v.borrow().is_empty()));
    // A deliberate new tap resets dismissal and may reopen the same editor.
    cx.keyboard.text_ime_dismissed=false;
    cx.ops.push_back(CxOsOp::ShowTextIME((),(),1));cx.dispatch();
    android_jni::SHOWS.with(|v|assert_eq!(*v.borrow(),vec![true]));
    cx.ops.push_back(CxOsOp::ShowTextIME((),(),1));cx.dispatch();
    android_jni::SHOWS.with(|v|assert_eq!(*v.borrow(),vec![true]));
    println!("Stale queued IME show cannot reverse dismissal; deliberate retap still opens");
}
