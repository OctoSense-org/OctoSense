//! A system app's notice (`<namespace>.notify`): how an app's agent tells
//! the person something now, as that app, with a card on the glance screen
//! and a notification.
//!
//! | method | args | answer |
//! |---|---|---|
//! | `<namespace>.notify` | `{title, body, card_id?, priority?}` | `{card_id, replaced, expires_at}` once the card is on the glance screen ([`notify`]) |
//!
//! **One card for every app.** [`NOTICE_CARD`] is a fixed L0 card: the
//! app's icon (a meaning from L0's closed set, [`icon`]) and name
//! ([`app_name`]), put in by the shell ([`card`]), and from the call the
//! title (1 to [`TITLE_MAX`] characters) and text (1 to [`BODY_MAX`]), with
//! the time ([`publish_args`]). A model writes text, never card code. The
//! card opens the app, posts a notification, and is published through the
//! glance service as the calling app, only when that app's admitted
//! manifest was granted `glance` ([`crate::glance::publish_for`]). The same
//! `card_id` replaces the app's earlier notice.
//!
//! **Who answers.** A tool runs on the host service of its namespace
//! (`host_tools::script_apps`). An app whose namespace has a service of its
//! own answers `notify` there and hands it here: Mail (`octosense_mail_
//! service::on_notify`), News (`octosense_news_service::Options::
//! on_notify`) and, where the photo engine is linked (the desktop's
//! `craft-engines`), Photos (`octosense_photo_service::on_notify`, whose
//! `photos` service also answers `photos.info` on the photo engine, ADR
//! 0013). Every other system app gets [`NoticeService`]: registered
//! once, after the shell's own services ([`serve_system_apps`]), for each
//! system app no service answers, it serves that app alone, and only
//! `notify`. Its agent reaches it when the app's `tools.json` declares
//! `<namespace>.notify` (Maps, YouTube, Camera, and Photos on Home, which
//! leaves the photo engine out). There Photos' `photos.info` answers that
//! it is not available on this device
//! (`host_tools::script_apps::unlinked_engine`), not "no method".
use serde_json::{json, Value};

/// The notice card (L0), with slots for the app's icon and name ([`card`]).
pub const NOTICE_CARD: &str = include_str!("../resources/glance/notice.card");
const ICON_SLOT: &str = "{icon}";
const NAME_SLOT: &str = "\"{app}\"";
/// The caller's title and text, in characters.
pub const TITLE_MAX: usize = 80;
pub const BODY_MAX: usize = 600;
/// The priority of a notice that names none.
pub const PRIORITY_DEFAULT: i64 = 60;

/// The app's icon on its notice: a meaning from L0's closed icon set.
pub fn icon(app: &str) -> &'static str {
    match app.strip_prefix("os.").unwrap_or(app) {
        "mail" => "mail",
        "photos" => "image",
        "maps" => "map",
        "camera" => "camera",
        "youtube" => "play",
        "calendar" => "calendar",
        _ => "bell",
    }
}

/// The app's name on its card and notification: the system app's manifest
/// name (`YouTube`), else a label made from its id.
pub fn app_name(app: &str) -> String {
    octosense_app_hub_app::system_apps()
        .into_iter()
        .find(|a| a.id == app)
        .map(|a| a.name.to_string())
        .unwrap_or_else(|| crate::approvals::sheet::app_label(app))
}

/// [`NOTICE_CARD`] for `app`: its icon, then its name as the card's copy,
/// a JSON string (what L0 reads a string as), so any name stays text.
pub fn card(app: &str, name: &str) -> String {
    NOTICE_CARD.replace(ICON_SLOT, icon(app)).replace(NAME_SLOT, &Value::from(name).to_string())
}

/// `text` cut to `max` characters, an ellipsis last when it was longer.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

/// The `glance.publish` arguments for `app`'s notice: its [`card`],
/// filled from the call (`{title, body, card_id?, priority?}`), opening
/// `app`, with a notification titled `<App> · <title>`. A blank or long
/// title or body is refused before anything is published; `now_ms` names a
/// notice that gives no `card_id`.
pub fn publish_args(app: &str, args: &Value, now_ms: u64) -> Result<Value, String> {
    let field = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let (title, body) = (field("title"), field("body"));
    if title.is_empty() || title.chars().count() > TITLE_MAX {
        return Err(format!("Provide a title of 1 to {TITLE_MAX} characters."));
    }
    if body.is_empty() || body.chars().count() > BODY_MAX {
        return Err(format!("Provide a body of 1 to {BODY_MAX} characters."));
    }
    let card_id = match field("card_id") {
        id if id.is_empty() => format!("notice-{now_ms:x}"),
        id => id,
    };
    let priority = args.get("priority").and_then(Value::as_i64).unwrap_or(PRIORITY_DEFAULT).clamp(0, 100);
    let name = app_name(app);
    let as_of = crate::shell::bar::local_time(c"%H:%M");
    Ok(json!({
        // The notification's title: the glance service takes at most
        // `glance::TITLE_MAX` characters, the app's name included.
        "card_id": card_id, "title": clip(&format!("{name} \u{00b7} {title}"), crate::glance::TITLE_MAX),
        "source": card(app, &name),
        "data": {"note": {"title": title, "summary": body, "as_of": as_of}},
        "priority": priority, "open": {"app": app.strip_prefix("os.").unwrap_or(app)}, "notify": true,
    }))
}

/// `<namespace>.notify` for `app`: its notice, published as `app`.
pub fn notify(app: &str, args: &Value) -> Result<Value, String> {
    let publish = publish_args(app, args, crate::glance::now_ms())?;
    crate::glance::publish_for(app, &publish)
}

/// `<namespace>.notify` for one system app whose namespace no service of
/// its own answers. Only that app may call it.
pub struct NoticeService {
    /// The system app (`os.photos`) and its namespace (`photos`).
    app: &'static str,
    family: &'static str,
}

impl octosense_appstore::services::HostService for NoticeService {
    fn family(&self) -> &'static str {
        self.family
    }
    fn call(&mut self, call: octosense_appstore::services::ServiceCall, reply: octosense_appstore::services::Replier, _host: &mut dyn octosense_appstore::services::ServiceHost) {
        if call.app_id != self.app {
            return reply.send(Err(format!("{}.notify serves {} only", self.family, self.app)));
        }
        // A method of the app's own whose engine this build leaves out
        // (Photos' `photos.info` on Home): plainly not here.
        if let Some(engine) = crate::host_tools::script_apps::unlinked_engine(&call.service) {
            return reply.send(Err(crate::host_tools::script_apps::not_on_this_device(&call.service, engine)));
        }
        match call.method() {
            "notify" => reply.send(notify(&call.app_id, &call.args)),
            other => reply.send(Err(format!("{} has no method {other:?}", self.family))),
        }
    }
}

/// Register [`NoticeService`] for every system app whose namespace no host
/// service answers: once, after the shell registered its own (Mail's,
/// Calendar's, News's), so it never replaces one. A service registered
/// later for that namespace replaces it, and answers `notify` itself.
/// The system apps that got one.
pub fn serve_system_apps() -> Vec<&'static str> {
    let mut served = Vec::new();
    for app in octosense_app_hub_app::system_apps() {
        let Some(family) = app.id.strip_prefix(octosense_appstore::system::SYSTEM_ID_PREFIX) else { continue };
        if octosense_appstore::services::has_service(family) {
            continue;
        }
        octosense_appstore::services::register_host_service(Box::new(NoticeService { app: app.id, family }));
        served.push(app.id);
    }
    served
}

#[cfg(test)]
mod tests {
    use super::*;
    use octosense_appstore::services::{dispatch, take_replies_for, ServiceCall, ServiceHost};

    /// A notice is the fixed card filled with the caller's text: it opens
    /// the app and notifies, a missing `card_id` is made from the time and
    /// the priority is clamped; a blank or long title or body is refused
    /// before anything is published. Mail's is what `mail.notify` always
    /// published.
    #[test]
    fn a_notice_is_the_fixed_card_filled_with_the_callers_text() {
        let args = publish_args("os.mail", &json!({"title": "Hello", "body": "From the system agent", "card_id": "hello"}), 1).unwrap();
        assert_eq!(args["card_id"], "hello");
        assert_eq!(args["title"], "Mail \u{00b7} Hello");
        let source = args["source"].as_str().unwrap();
        assert_eq!(source, card("os.mail", "Mail"));
        assert!(source.contains("copy app { class: vocabulary, en: \"Mail\" }") && source.contains("Icon(name: .mail, size: .row)"), "{source}");
        assert_eq!(args["data"]["note"]["title"], "Hello");
        assert_eq!(args["data"]["note"]["summary"], "From the system agent");
        assert_eq!((args["open"]["app"].as_str(), args["notify"].as_bool()), (Some("mail"), Some(true)));
        assert_eq!(args["priority"], 60);
        let generated = publish_args("os.mail", &json!({"title": "Hi", "body": "x", "priority": 500}), 0xabc).unwrap();
        assert_eq!((generated["card_id"].as_str(), generated["priority"].as_i64()), (Some("notice-abc"), Some(100)));
        assert!(publish_args("os.mail", &json!({"title": " ", "body": "x"}), 0).is_err());
        assert!(publish_args("os.mail", &json!({"title": "x".repeat(81), "body": "x"}), 0).is_err());
        assert!(publish_args("os.mail", &json!({"title": "x", "body": "y".repeat(601)}), 0).is_err());
        // A full-length title still makes a notification the glance
        // service takes: the app's name comes first, the end is cut.
        let long = publish_args("os.youtube", &json!({"title": "t".repeat(80), "body": "x"}), 0).unwrap();
        let toast = long["title"].as_str().unwrap();
        assert!(toast.starts_with("YouTube \u{00b7} ttt") && toast.ends_with('\u{2026}') && toast.chars().count() == crate::glance::TITLE_MAX, "{toast}");
        assert_eq!(long["data"]["note"]["title"], "t".repeat(80), "the card keeps the whole title");
    }

    /// Each app's notice carries its own name and icon, and opens it.
    #[test]
    fn each_apps_notice_is_its_own() {
        for (app, name, icon) in [("os.photos", "Photos", "image"), ("os.maps", "Maps", "map"), ("os.youtube", "YouTube", "play"), ("os.news", "News", "bell")] {
            let args = publish_args(app, &json!({"title": "Hello", "body": "Hi"}), 1).unwrap();
            assert_eq!(args["title"], format!("{name} \u{00b7} Hello"));
            let source = args["source"].as_str().unwrap();
            assert!(source.contains(&format!("en: \"{name}\" }}")) && source.contains(&format!("Icon(name: .{icon}, size: .row)")), "{app}: {source}");
            assert_eq!(args["open"]["app"], app.strip_prefix("os.").unwrap());
        }
        assert_eq!(icon("os.camera"), "camera", "Camera ships on the phone only; its icon is in the set");
    }

    /// The name is the card's text, never its code: quotes, backslashes or
    /// a line break in it leave a valid L0 card that shows them.
    #[test]
    fn an_apps_name_stays_text_in_its_card() {
        let source = card("os.photos", "Odd \"name\" \\ with\na break");
        assert!(crate::glance::check_level(&source).is_ok(), "{source}");
        assert!(source.contains(r#"en: "Odd \"name\" \\ with\na break" }"#), "{source}");
    }

    struct NoSheets;
    impl ServiceHost for NoSheets {
        fn open_sheet(&mut self, _body: String) {}
        fn close_sheet(&mut self) {}
    }

    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX / 4);

    fn ask(app: &str, service: &str, args: Value) -> Result<Value, String> {
        let heap = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let call = ServiceCall { app_id: app.into(), service: service.into(), args, from_sheet: false, may_prompt: false, host_dir: std::env::temp_dir() };
        dispatch(call, heap, 1, &mut NoSheets);
        let (_, _, answer) = take_replies_for(&[heap]).pop().expect("answered at once");
        answer.map(|json| serde_json::from_str(&json).unwrap())
    }

    /// Once the shell's own services are registered, every system app no
    /// service answers gets the notice service, which serves that app
    /// alone and only `notify`; an app's own service (Mail's) is not
    /// replaced. A hyphenated namespace (`ai-providers`) routes like any
    /// other: the shell splits the family at the first dot.
    #[test]
    fn system_apps_without_a_service_of_their_own_get_the_notice_service() {
        // Registers the shell's services, then the notice services.
        let _ = crate::apps::system_card_apps();
        for family in ["photos", "maps", "youtube", "ai-providers"] {
            assert!(octosense_appstore::services::has_service(family), "{family}");
        }
        assert!(ask("os.maps", "photos.notify", json!({"title": "x", "body": "y"})).unwrap_err().contains("serves os.photos only"));
        assert!(ask("os.photos", "photos.list", json!({})).unwrap_err().contains("no method"));
        // Where the photo engine is linked (the desktop's `craft-engines`,
        // ADR 0013), Photos' namespace is its own service: `photos.info`
        // reaches the photos service, not a notice service's "no method".
        // That service answers first for the folder: this call names none the
        // host handed out, so it is refused before the engine opens anything.
        #[cfg(feature = "craft-engines")]
        {
            let refused = ask("os.photos", "photos.info", json!({"path": "nothing.png"})).unwrap_err();
            assert!(!refused.contains("no method"), "{refused}");
            assert!(refused.starts_with("photos") || refused.contains("photo.info"), "{refused}");
        }
        // Home leaves the photo engine out: the notice service answers
        // Photos' namespace, and `photos.info` says plainly it is not here.
        #[cfg(not(feature = "craft-engines"))]
        {
            let refused = ask("os.photos", "photos.info", json!({"path": "nothing.png"})).unwrap_err();
            assert_eq!(refused, "photos.info isn't available on this device: the photo engine is only in the desktop build");
        }
        // Its own app reaches the notice (a blank title is refused there,
        // before anything is published).
        for (app, service) in [("os.photos", "photos.notify"), ("os.ai-providers", "ai-providers.notify")] {
            let refused = ask(app, service, json!({"title": " ", "body": "Hi"})).unwrap_err();
            assert!(refused.contains("Provide a title"), "{service}: {refused}");
        }
        // Mail answers its own namespace: its service, not a notice service.
        let dir = std::env::temp_dir().join(format!("notice-mail-{}", std::process::id()));
        let call = ServiceCall { app_id: "os.mail".into(), service: "mail.accounts".into(), args: json!({}), from_sheet: false, may_prompt: false, host_dir: dir.clone() };
        let heap = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        dispatch(call, heap, 1, &mut NoSheets);
        assert_eq!(take_replies_for(&[heap]).pop().unwrap().2.unwrap(), "[]");
        let _ = std::fs::remove_dir_all(dir);
    }
}
