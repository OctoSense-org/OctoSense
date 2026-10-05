//! The Java JobService and the window load this same library/process. JNI
//! carries only host paths, lifecycle leases, and bounded notification state.
use makepad_widgets::*;
use std::cell::RefCell;
use std::time::{Duration, Instant};
thread_local! { static ENTRY: RefCell<Option<(String, Instant)>> = const { RefCell::new(None) }; }

pub(crate) fn event(app: &mut crate::App, cx: &mut Cx, event: &Event) {
    if let Event::AndroidIntegration { channel, payload } = event {
        if channel == "mail.notification.open" && payload.len() <= 256 {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
                if let Some(token) = value["token"]
                    .as_str()
                    .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                {
                    ENTRY.with(|entry| {
                        *entry.borrow_mut() =
                            Some((token.into(), Instant::now() + Duration::from_secs(30)))
                    });
                }
            }
        }
    }
    if matches!(event, Event::HomeIntent) {
        ENTRY.with(|entry| *entry.borrow_mut() = None);
    }
    if app.state.is_none() {
        return;
    }
    #[cfg(any(feature = "app-hub", target_os = "android"))]
    ENTRY.with(|entry| {
        let mut entry = entry.borrow_mut();
        let Some((token, until)) = entry.as_ref() else {
            return;
        };
        if Instant::now() >= *until {
            *entry = None;
            return;
        }
        if let Some(key) = octosense_shell::mail_background::open(token) {
            *entry = None;
            app.shell.open_glance_card(cx, &key);
        }
    });
}

#[cfg(target_os = "android")]
mod jni {
    use makepad_jni_sys::*;
    use octosense_shell::{agent_events, agents, apps, host_tools, mail_background, runtime_host};
    use std::path::PathBuf;
    use std::sync::Once;
    unsafe fn string(env: *mut JNIEnv, text: jstring) -> String {
        if text.is_null() {
            return String::new();
        }
        let len = (**env).GetStringLength.unwrap()(env, text);
        let ptr = (**env).GetStringChars.unwrap()(env, text, std::ptr::null_mut());
        if ptr.is_null() {
            return String::new();
        }
        let result = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len as usize));
        (**env).ReleaseStringChars.unwrap()(env, text, ptr);
        result
    }
    unsafe fn java_string(env: *mut JNIEnv, text: &str) -> jstring {
        let units: Vec<_> = text.encode_utf16().collect();
        (**env).NewString.unwrap()(env, units.as_ptr(), units.len() as jsize)
    }
    #[no_mangle]
    pub unsafe extern "system" fn Java_dev_makepad_octosense_MailBackground_nativeInit(
        env: *mut JNIEnv,
        _: jclass,
        files: jstring,
        kernel: jstring,
    ) -> jboolean {
        let files = string(env, files);
        let kernel = string(env, kernel);
        std::panic::catch_unwind(|| {
            static INIT: Once = Once::new();
            INIT.call_once(|| {
                makepad_widgets::makepad_platform::home::set_platform_data_dir(
                    std::path::Path::new(&files),
                );
                runtime_host::init(Some(files), Some(PathBuf::from(kernel)));
                apps::register_mail_services();
                agent_events::start();
            });
            if let Some(app) = agents::find("os.mail") {
                agents::prepare(&app);
            }
        })
        .is_ok() as jboolean
    }
    #[no_mangle]
    pub extern "system" fn Java_dev_makepad_octosense_MailBackground_nativeForeground(
        _: *mut JNIEnv,
        _: jclass,
        active: jboolean,
    ) {
        mail_background::foreground(active != 0);
    }
    #[no_mangle]
    pub extern "system" fn Java_dev_makepad_octosense_MailBackground_nativeBegin(
        _: *mut JNIEnv,
        _: jclass,
    ) -> jlong {
        mail_background::begin() as jlong
    }
    #[no_mangle]
    pub extern "system" fn Java_dev_makepad_octosense_MailBackground_nativeEnd(
        _: *mut JNIEnv,
        _: jclass,
        id: jlong,
    ) {
        mail_background::end(id as u64);
    }
    #[no_mangle]
    pub unsafe extern "system" fn Java_dev_makepad_octosense_MailBackground_nativeTick(
        env: *mut JNIEnv,
        _: jclass,
        snapshot: jboolean,
    ) -> jstring {
        let state = std::panic::catch_unwind(|| {
            host_tools::pump();
            if snapshot != 0 {
                mail_background::state().to_string()
            } else {
                String::from("{}")
            }
        })
        .unwrap_or_else(|_| String::from("{\"error\":\"host_unavailable\"}"));
        java_string(env, &state)
    }
    #[no_mangle]
    pub unsafe extern "system" fn Java_dev_makepad_octosense_MailBackground_nativeDelivered(
        env: *mut JNIEnv,
        _: jclass,
        token: jstring,
        published: jlong,
    ) {
        let token = string(env, token);
        let _ = std::panic::catch_unwind(|| mail_background::delivered(&token, published as u64));
    }
}
