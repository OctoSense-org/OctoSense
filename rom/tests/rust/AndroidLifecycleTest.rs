mod jni_sys {pub enum JNIEnv {} pub type jobject=usize;}
#[derive(Clone, Copy, Debug, PartialEq)] enum FromJavaMessage {Resume,Pause}
thread_local! {static QUEUE:std::cell::RefCell<Option<Vec<FromJavaMessage>>>=std::cell::RefCell::new(None);}
fn send_from_java_message(message:FromJavaMessage) {QUEUE.with(|queue|if let Some(queue)=queue.borrow_mut().as_mut(){queue.push(message);});}
/* ACTIVITY_STATE */
/* JNI_CALLBACKS */
mod android_jni {pub use super::activity_is_resumed;}
struct Cx {events:Vec<FromJavaMessage>}
impl Cx {
    fn handle_message(&mut self,event:FromJavaMessage) {self.events.push(event);}
    fn after_startup(&mut self) {/* RESTORE_AFTER_STARTUP */}
}
fn main() {
    let mut cx=Cx{events:Vec::new()};
    unsafe {Java_dev_makepad_android_MakepadNative_activityOnResume(std::ptr::null_mut(),0);}
    // JNI ran before a queue, or bootstrap consumed the queued message before
    // Startup. The normal app handler must still receive real foreground state.
    QUEUE.with(|v|assert!(v.borrow().is_none()));cx.after_startup();
    assert_eq!(cx.events,vec![FromJavaMessage::Resume]);
    unsafe {Java_dev_makepad_android_MakepadNative_activityOnPause(std::ptr::null_mut(),0);}
    cx.events.clear();cx.after_startup();assert!(cx.events.is_empty());
    QUEUE.with(|v|*v.borrow_mut()=Some(Vec::new()));
    unsafe {
        Java_dev_makepad_android_MakepadNative_activityOnResume(std::ptr::null_mut(),0);
        Java_dev_makepad_android_MakepadNative_activityOnPause(std::ptr::null_mut(),0);
    }
    QUEUE.with(|v|assert_eq!(v.borrow().as_ref().unwrap(),&[FromJavaMessage::Resume,FromJavaMessage::Pause]));
    assert!(!activity_is_resumed());cx.after_startup();assert!(cx.events.is_empty());
    println!("Lifecycle before queue/bootstrap and subsequent transitions passed");
}
