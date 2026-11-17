//! Native Gmail, GitHub and Calendar approval. The script can request a review, never approve one.
use crate::octosense::style::{AppIconDraw, DesktopStyle};
use makepad_widgets::*;
use octosense_oauth_service::{host_api, host_inbox};

enum ReviewRequest {
    Mail(host_inbox::ReviewRequest),
    Smtp(octosense_mail_service::public_review::ReviewRequest),
    Save(host_api::ReviewRequest),
    /// A test shell's sample (`register_samples`): a review's words, with nothing
    /// behind them, so there is nothing to approve.
    Sample { close: String, snapshot: serde_json::Value },
}
impl ReviewRequest {
    fn approve(&mut self, down: bool, up: bool) -> Result<(), String> {
        match self {
            Self::Mail(r) => r.approve(down, up),
            Self::Smtp(r) => r.approve(down, up),
            Self::Save(r) => r.approve(down, up),
            Self::Sample { .. } => Err("This is a sample. Nothing was saved.".into()),
        }
    }
    fn result(&mut self) -> Option<Result<serde_json::Value, String>> {
        match self {
            Self::Mail(r) => r.result(),
            Self::Smtp(r) => r.result(),
            Self::Save(r) => r.result(),
            Self::Sample { .. } => None,
        }
    }
    fn cancel(&mut self) -> Result<(), String> {
        match self {
            Self::Mail(r) => r.cancel(),
            Self::Smtp(r) => r.cancel(),
            Self::Save(r) => r.cancel(),
            Self::Sample { .. } => Ok(()),
        }
    }
    fn is_preparing(&self) -> bool {
        matches!(self, Self::Smtp(request) if request.is_preparing())
    }
    fn close_request(&self) -> (String, String) {
        match self {
            Self::Mail(_) => ("gmail.sheet.close".into(), String::new()),
            Self::Smtp(_) => ("mail.sheet.review_close".into(), String::new()),
            Self::Save(r) => r.close_request(),
            Self::Sample { close, .. } => (close.clone(), String::new()),
        }
    }
    /// What a save's review shows.
    fn save_snapshot(&self) -> serde_json::Value {
        match self {
            Self::Save(r) => r.snapshot(),
            Self::Sample { snapshot, .. } => snapshot.clone(),
            _ => serde_json::Value::Null,
        }
    }
}
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    time::Instant,
};

/// What the mail service answers for an account that isn't there. On a Mac it says
/// what to do, and offers the way to Mail.
const NO_ACCOUNT: &str = "There is no such account.";
static OPEN_MAIL: AtomicBool = AtomicBool::new(false);

/// Whether "Open Mail" was pressed since the shell last looked.
pub fn take_open_mail() -> bool {
    OPEN_MAIL.swap(false, Ordering::Relaxed)
}

type Pending = HashMap<String, (Instant, ReviewRequest)>;
fn pending() -> &'static Mutex<Pending> {
    static PENDING: OnceLock<Mutex<Pending>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register() {
    widget_async::register_splash_isolate_mod(|vm| {
        script_mod(vm);
        script_eval!(vm, {mod.prelude.widgets.ConnectedReplyReview = mod.widgets.ConnectedReplyReview});
    });
}

/// The owning service mounts this source in its host sheet, outside the app.
pub fn sheet(request: host_inbox::ReviewRequest) -> Result<String, String> {
    mount(ReviewRequest::Mail(request))
}
pub fn smtp_sheet(
    request: octosense_mail_service::public_review::ReviewRequest,
) -> Result<String, String> {
    mount(ReviewRequest::Smtp(request))
}
pub fn connector_sheet(request: host_api::ReviewRequest) -> Result<String, String> {
    mount(ReviewRequest::Save(request))
}

/// `--test-action sample-reviews`: in that shell, and only there, GitHub's and
/// Google Calendar's reviews are samples. A hidden test shell's vault is a file, so
/// the real services cannot open their store; this lets an app's request raise the
/// real sheet through the real path with a fictional account, and no provider or
/// store is involved. Approving one does nothing.
pub fn register_samples() {
    // First, so that the shell's own registration does not replace these later.
    crate::apps::register_host_services();
    for family in ["github", "gcalendar"] {
        octosense_appstore::services::register_host_service(Box::new(SampleReviews { family, waiting: None }));
    }
}
struct SampleReviews {
    family: &'static str,
    waiting: Option<octosense_appstore::services::Replier>,
}
impl octosense_appstore::services::HostService for SampleReviews {
    fn family(&self) -> &'static str {
        self.family
    }
    fn call(&mut self, call: octosense_appstore::services::ServiceCall, reply: octosense_appstore::services::Replier, host: &mut dyn octosense_appstore::services::ServiceHost) {
        match call.method() {
            "review_save" => {
                let close = format!("{}.sheet.cancel", self.family);
                let source = host_api::sample_snapshot(self.family, &call.app_id, &call.args)
                    .and_then(|snapshot| mount(ReviewRequest::Sample { close, snapshot }));
                match source {
                    Ok(source) => {
                        host.open_sheet(source);
                        self.waiting = Some(reply);
                    }
                    Err(error) => reply.send(Err(error)),
                }
            }
            "sheet.cancel" => {
                if let Some(waiting) = self.waiting.take() {
                    waiting.send(Err("Save cancelled; your local draft is unchanged".into()));
                }
                host.close_sheet();
                reply.send(Ok(serde_json::json!({"cancelled": true})));
            }
            _ => reply.send(Err("This shell only has sample reviews".into())),
        }
    }
}

fn mount(request: ReviewRequest) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut requests = pending().lock().unwrap_or_else(|e| e.into_inner());
    requests.retain(|_, (when, _)| when.elapsed().as_secs() < 600);
    if requests.len() >= 32 {
        return Err("Too many pending reviews; close an earlier review".into());
    }
    let (close_service, close_ticket) = request.close_request();
    requests.insert(id.clone(), (Instant::now(), request));
    Ok(format!(
        "ConnectedReplyReview {{width: Fill height: Fill ticket: {} close_service: {} close_ticket: {}}}",
        serde_json::json!(id),
        serde_json::json!(close_service), serde_json::json!(close_ticket)
    ))
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    // A phone keeps this sheet's fixed look; a desktop's is a Mac dialog that
    // follows the appearance (`border_radius` is half the corner it draws).
    let desk = try { theme.ui_pointer == true } catch { false }
    // The Mac style: attached under the window's top band over a light dim,
    // with labelled rows. Every other desktop style keeps what it had.
    let desk_mac = desk && try { theme.ui_unified == true } catch { false }
    let desk_dark = desk && dot(theme.color_text, #x36b61200) >= 0.5
    fn tone(light, dark){ if desk_dark { dark } else { light } }
    let sh_ink = tone(#x1d1d1f, #xf5f5f7)
    let sh_muted = tone(#x636368, #xa1a1a6)
    let sh_line = tone(#xe3e3e8, #x48484a)
    let sh_control = tone(#xffffff, #x48484a)
    let sh_control_line = tone(#xc9c9ce, #x545458)
    let sh_off = tone(#xe3e3e8, #x3a3a3c)
    let sh_off_ink = tone(#x636368, #xaeaeb2)
    // A Mac alert's destructive button draws its title in the system red.
    let sh_red = tone(#xff3b30, #xff453a)
    mod.widgets.ReviewAppIcon = set_type_default() do #(ReviewAppIcon::register_widget(vm)) {width: 48 height: 48}
    mod.widgets.ConnectedReplyReviewBase = #(ConnectedReplyReview::register_widget(vm))
    // One labelled row: 13 points, its label right-aligned to the left of the value.
    let MacFact = View {width: Fill height: Fit flow: Right spacing: 8 visible: false
        k := Label {width: 96 height: Fit padding: 0 align: Align{x: 1.0} draw_text.color: tone(#x636368, #xa8a8ad) draw_text.text_style.font_size: 9.75}
        v := Label {width: Fill height: Fit padding: 0 draw_text.color: sh_ink draw_text.wrap: Words draw_text.text_style.font_size: 9.75}
    }
    // A Mac's push buttons: the plain one, and the blue default.
    let MacPlain = ButtonFlat {height: 28 margin: 0 padding: Inset{left: 12 right: 12 bottom: 2}
        draw_bg +: {border_radius: 3.0 border_size: 1.0 color: sh_control color_hover: tone(#xf2f2f4, #x545458) color_down: tone(#xe5e5ea, #x5e5e62) color_focus: sh_control color_disabled: sh_off
            border_color: sh_control_line border_color_hover: sh_control_line border_color_down: sh_control_line border_color_focus: sh_control_line border_color_disabled: sh_control_line}
        draw_text +: {color: sh_ink color_hover: sh_ink color_down: sh_ink color_focus: sh_ink color_disabled: sh_off_ink text_style: theme.font_regular{font_size: 9.75}}
    }
    let MacBlue = ButtonFlat {height: 28 margin: 0 padding: Inset{left: 16 right: 16 bottom: 2}
        draw_bg +: {border_radius: 3.0 border_size: 0.0 color: #x0071e3 color_hover: #x006edb color_down: #x0060c0 color_focus: #x0071e3 color_disabled: sh_off}
        draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff color_focus: #xffffff color_disabled: sh_off_ink text_style: theme.font_regular{font_size: 9.75}}
    }
    mod.widgets.ConnectedReplyReview = if desk_mac {
        set_type_default() do mod.widgets.ConnectedReplyReviewBase {
            desk: true
            mac: true
            width: Fill height: Fill flow: Down new_batch: true
            show_bg: true
            draw_bg +: {
                color: instance(tone(#x00000024, #x00000052))
                pixel: fn() { return Pal.premul(self.color) }
            }
            ScrollYView {width: Fill height: Fill flow: Down align: Align{x: 0.5} padding: Inset{left: 12 right: 12 top: 52 bottom: 24} container_id: @review_area
                RoundedShadowView {width: Fill max_width: 520 height: Fit flow: Down spacing: 8 padding: 20 new_batch: true show_bg: true
                    draw_bg +: {color: tone(#xf6f6f8, #x2c2c2e) border_radius: 5.0 border_size: 1.0 border_color: sh_line shadow_color: tone(#x00000040, #x00000080) shadow_radius: 18.0 shadow_offset: vec2(0.0, 6.0)}
                    View {width: Fill height: Fit flow: Right spacing: 14
                        icon_box := View {width: Fit height: Fit visible: false
                            app_icon := mod.widgets.ReviewAppIcon {}
                        }
                        View {width: Fill height: Fit flow: Down spacing: 8
                            heading := Label {width: Fill height: Fit padding: 0 text: "Review Reply" draw_text.color: sh_ink draw_text.wrap: Words draw_text.text_style: theme.font_bold{font_size: 11.25}}
                            account := Label {width: Fill height: Fit padding: 0 draw_text.color: sh_ink draw_text.wrap: Words draw_text.text_style.font_size: 9.75}
                        }
                    }
                    facts := RoundedView {width: Fill height: Fit flow: Down spacing: 4 padding: 10 margin: Inset{top: 4 bottom: 4} show_bg: true
                        draw_bg +: {color: tone(#xffffff, #x3a3a3c) border_radius: 3.0 border_size: 1.0 border_color: sh_line}
                        fact0 := MacFact {}
                        fact1 := MacFact {}
                        fact2 := MacFact {}
                        fact3 := MacFact {}
                        fact4 := MacFact {}
                        fact5 := MacFact {}
                        fact6 := MacFact {}
                        fact7 := MacFact {}
                    }
                    // What the other trees show here is in the rows above.
                    recipient := Label {width: Fill height: 0 padding: 0 visible: false}
                    subject := Label {width: Fill height: 0 padding: 0 visible: false}
                    message_box := RoundedView {width: Fill height: Fit flow: Down show_bg: true
                        draw_bg +: {color: tone(#xffffff, #x3a3a3c) border_radius: 3.0 border_size: 1.0 border_color: sh_line}
                        // A long message scrolls here: what is left of the window keeps the actions in view.
                        ScrollYView {width: Fill height: Fit max_height: "max(120px, 100cqh - 380px)" flow: Down padding: 10
                            message := Label {width: Fill height: Fit padding: 0 draw_text.color: sh_ink draw_text.wrap: Words draw_text.text_style.font_size: 9.75}
                        }
                    }
                    outcome := Label {width: Fill height: Fit padding: 0 visible: false draw_text.color: sh_ink draw_text.wrap: Words draw_text.text_style.font_size: 9.75}
                    status := Label {width: Fill height: Fit padding: 0 text: "Check the recipient and the complete reply before sending." draw_text.color: sh_muted draw_text.wrap: Words draw_text.text_style.font_size: 8.25}
                    // Two buttons at a time: the default rightmost. Where what this asks
                    // undoes something, Cancel is the default (`careful`) and what it
                    // asks keeps a red title, as a Mac's destructive button does.
                    View {width: Fill height: Fit flow: Right spacing: 8 align: Align{x: 1.0 y: 0.5} margin: Inset{top: 6}
                        approve_plain := MacPlain {visible: false text: "Approve & Send" draw_text +: {color: sh_red color_hover: sh_red color_down: sh_red color_focus: sh_red}}
                        cancel := MacPlain {text: "Cancel"}
                        cancel_blue := MacBlue {visible: false text: "Cancel"}
                        open_mail := MacBlue {visible: false text: "Open Mail"}
                        approve := MacBlue {text: "Approve & Send"}
                    }
                }
            }
        }
    } else if desk {
        set_type_default() do mod.widgets.ConnectedReplyReviewBase {
            desk: true
            width: Fill height: Fill flow: Down new_batch: true
            // The dialog's dim: the card over it is the host's own, opaque, and
            // receives all the input while it is up.
            show_bg: true
            draw_bg +: {
                color: instance(tone(#x00000026, #x00000026))
                pixel: fn() { return Pal.premul(self.color) }
            }
            ScrollYView {width: Fill height: Fill flow: Down align: Align{x: 0.5} padding: Inset{left: 12 right: 12 top: 40 bottom: 24} container_id: @review_area
                RoundedView {width: Fill max_width: 480 height: Fit flow: Down spacing: 8 padding: 18 new_batch: true show_bg: true
                    draw_bg +: {color: tone(#xf6f6f8, #x2c2c2e) border_radius: 5.0 border_size: 1.0 border_color: sh_line}
                    heading := Label {width: Fill height: Fit padding: 0 text: "Review reply" draw_text.color: sh_ink draw_text.wrap: Words draw_text.text_style: theme.font_bold{font_size: 12.75}}
                    account := Label {width: Fill height: Fit padding: 0 draw_text.color: sh_muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                    RoundedView {width: Fill height: Fit flow: Down show_bg: true
                        draw_bg +: {color: tone(#xffffff, #x3a3a3c) border_radius: 3.0 border_size: 1.0 border_color: sh_line}
                        // A long message scrolls here: what is left of the window keeps the actions in view.
                        ScrollYView {width: Fill height: Fit max_height: "max(120px, 100cqh - 230px)" flow: Down spacing: 8 padding: 10
                            recipient := Label {width: Fill height: Fit padding: 0 draw_text.color: sh_ink draw_text.wrap: Words draw_text.text_style.font_size: 9.75}
                            subject := Label {width: Fill height: Fit padding: 0 draw_text.color: sh_ink draw_text.wrap: Words draw_text.text_style: theme.font_bold{font_size: 9.75}}
                            message := Label {width: Fill height: Fit padding: 0 draw_text.color: sh_ink draw_text.wrap: Words draw_text.text_style.font_size: 9.75}
                        }
                    }
                    status := Label {width: Fill height: Fit padding: 0 text: "Check the recipient and the complete reply before sending." draw_text.color: sh_muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                    View {width: Fill height: Fit flow: Right spacing: 8 align: Align{x: 1.0 y: 0.5} margin: Inset{top: 6}
                        cancel := ButtonFlat {height: 28 margin: 0 padding: Inset{left: 12 right: 12} text: "Back to editing"
                            draw_bg +: {border_radius: 3.0 border_size: 1.0 color: sh_control color_hover: tone(#xf2f2f4, #x545458) color_down: tone(#xe5e5ea, #x5e5e62) color_focus: sh_control color_disabled: sh_off
                                border_color: sh_control_line border_color_hover: sh_control_line border_color_down: sh_control_line border_color_focus: sh_control_line border_color_disabled: sh_control_line}
                            draw_text +: {color: sh_ink color_hover: sh_ink color_down: sh_ink color_focus: sh_ink color_disabled: sh_off_ink text_style: theme.font_regular{font_size: 9.75}}
                        }
                        approve := ButtonFlat {height: 28 margin: 0 padding: Inset{left: 16 right: 16} text: "Approve & Send"
                            draw_bg +: {border_radius: 3.0 border_size: 0.0 color: #x0071e3 color_hover: #x006edb color_down: #x0060c0 color_focus: #x0071e3 color_disabled: sh_off}
                            draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff color_focus: #xffffff color_disabled: sh_off_ink text_style: theme.font_regular{font_size: 9.75}}
                        }
                    }
                }
            }
        }
    } else {
        set_type_default() do mod.widgets.ConnectedReplyReviewBase {
            width: Fill height: Fill flow: Down padding: 16 spacing: 12
            // This native composite dereferences View, not SolidView. View's
            // default DrawQuad shader is transparent even when color is set.
            // The host review must cover the untrusted app beneath it.
            show_bg: true
            draw_bg +: {
                color: instance(#fff)
                pixel: fn() { return Pal.premul(self.color) }
            }
            heading := Label {width: Fill height: Fit text: "Review reply" draw_text.color: #x172336 draw_text.text_style.font_size: 22}
            ScrollYView {width: Fill height: Fill flow: Down spacing: 12
                account := Label {width: Fill height: Fit draw_text.color: #x526071 draw_text.wrap: Words}
                recipient := Label {width: Fill height: Fit draw_text.color: #x172336 draw_text.wrap: Words}
                subject := Label {width: Fill height: Fit draw_text.color: #x172336 draw_text.wrap: Words draw_text.text_style.font_size: 16}
                message := Label {width: Fill height: Fit draw_text.color: #x172336 draw_text.wrap: Words draw_text.text_style.font_size: 15}
            }
            status := Label {width: Fill height: Fit text: "Check the recipient and the complete reply before sending." draw_text.color: #x526071 draw_text.wrap: Words}
            View {width: Fill height: 48 spacing: 8
                cancel := ButtonFlat {width: Fill height: Fill text: "Back to editing"}
                approve := Button {width: Fill height: Fill text: "Approve & Send"}
            }
        }
    }
}

/// Return on a Mac's host service's sheet: it stays on the sheet, whose widgets have
/// it. A native review takes it where its default is Cancel, and otherwise leaves it:
/// it does not approve (`handle_event`). Whether a sheet was up. Runs in the module's isolate.
pub(crate) fn return_host_sheet(cx: &mut Cx, root: &WidgetRef, event: &Event) -> bool {
    let sheet = root.splash(cx, ids!(sheet));
    if !sheet.borrow().is_some_and(|s| s.view.visible) {
        return false;
    }
    sheet.handle_event(cx, event, &mut Scope::empty());
    cx.redraw_all();
    true
}

/// Escape on a host service's sheet: what its Cancel does. A sheet written in
/// Splash names that function; a native review takes the key itself, where it
/// knows whether Cancel can be pressed. Never an approval. Whether it took the key:
/// while a sheet is up the app under it gets none. Runs in the module's isolate.
pub(crate) fn escape_host_sheet(cx: &mut Cx, root: &WidgetRef, event: &Event) -> bool {
    let sheet = root.splash(cx, ids!(sheet));
    if !sheet.borrow().is_some_and(|s| s.view.visible) {
        return false;
    }
    // A pop-up open on the sheet: Escape closes that, and the sheet stays.
    if makepad_widgets::gesture_view::view_menu_up(cx) {
        sheet.handle_event(cx, event, &mut Scope::empty());
        makepad_widgets::gesture_view::forget_view_menu(cx);
        cx.redraw_all();
        return true;
    }
    if !sheet.call_script_fn(cx, id!(host_dismiss), &[]) && !sheet.call_script_fn(cx, id!(cancel), &[]) {
        sheet.handle_event(cx, event, &mut Scope::empty());
    }
    cx.redraw_all();
    true
}

/// How a host sheet names an app, from trusted host data: the admitted
/// manifest's name for it. A system app's `os.` id says nothing more (that
/// namespace is refused to every other app at admission); another app's name is
/// its own text, so its id comes with it: the id is what tells a look-alike from
/// the real one.
pub(crate) fn shown_app(app: &str) -> (String, Option<String>) {
    let system = app.starts_with("os.");
    let name = octosense_oauth_service::host::display_name(app).unwrap_or_else(|| {
        if system {
            crate::approvals::sheet::app_label(app)
        } else {
            app.to_string()
        }
    });
    let id = (!system && name != app).then(|| app.to_string());
    (name, id)
}

/// "Writer wants to send this message." with the id of an app that is not the system's.
fn who_wants(app: &str, wants: &str) -> String {
    match shown_app(app) {
        (name, Some(id)) => format!("{name} ({id}) wants to {wants}."),
        (name, None) => format!("{name} wants to {wants}."),
    }
}

/// A review in a Mac's words. What will happen is exact, from the host's own
/// snapshot; only how it reads differs from the other trees.
struct MacWords {
    /// Who asks: its icon stands beside the words.
    app: String,
    heading: String,
    sentence: String,
    facts: Vec<(String, String)>,
    body: String,
    /// What the default button and the note say when the change is not a save.
    approve: Option<String>,
    note: Option<String>,
    /// It undoes something: Cancel is the default, not what it asks.
    careful: bool,
}

const FACT_ROWS: usize = 8;
fn fact_row(i: usize) -> (&'static [LiveId], &'static [LiveId], &'static [LiveId]) {
    match i {
        0 => (ids!(fact0), ids!(fact0.k), ids!(fact0.v)),
        1 => (ids!(fact1), ids!(fact1.k), ids!(fact1.v)),
        2 => (ids!(fact2), ids!(fact2.k), ids!(fact2.v)),
        3 => (ids!(fact3), ids!(fact3.k), ids!(fact3.v)),
        4 => (ids!(fact4), ids!(fact4.k), ids!(fact4.v)),
        5 => (ids!(fact5), ids!(fact5.k), ids!(fact5.v)),
        6 => (ids!(fact6), ids!(fact6.k), ids!(fact6.v)),
        _ => (ids!(fact7), ids!(fact7.k), ids!(fact7.v)),
    }
}

/// Labelled rows, dropping what is not known yet. Past the rows there are, the
/// rest joins the last one: nothing the person should see is left out.
fn facts(rows: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = rows.into_iter().filter(|(_, value)| !value.is_empty()).collect();
    if rows.len() > FACT_ROWS {
        let rest: Vec<String> = rows.split_off(FACT_ROWS - 1).into_iter().map(|(k, v)| format!("{k}: {v}")).collect();
        rows.push(("More".to_string(), rest.join("\n")));
    }
    rows
}

/// A website change's address row as a Mac names it: the site it goes to
/// ("Website"), not the address of the API it calls. Any other row is as it
/// was.
fn site_fact(key: &str, value: String) -> (String, String) {
    const ADDRESSED: [&str; 5] = ["Sends to", "Replaces", "Changes", "Deletes", "Requests"];
    let rest = value.strip_prefix("https://").or_else(|| value.strip_prefix("http://"));
    let host = rest.and_then(|rest| rest.split(['/', '?', '#']).next()).and_then(|authority| authority.rsplit('@').next());
    match host {
        Some(host) if ADDRESSED.contains(&key) && !host.is_empty() => ("Website".to_string(), host.to_string()),
        _ => (key.to_string(), value),
    }
}

fn mac_words(request: &ReviewRequest) -> MacWords {
    let text = |value: &serde_json::Value| value.as_str().unwrap_or("").to_owned();
    let row = |key: &str, value: String| (key.to_string(), value);
    match request {
        ReviewRequest::Mail(request) => {
            let snapshot = request.snapshot();
            let reply = &snapshot["reply"];
            MacWords {
                app: text(&snapshot["app"]),
                heading: "Review Reply".into(),
                sentence: who_wants(&text(&snapshot["app"]), "send this reply"),
                facts: facts(vec![
                    row("From", text(&reply["from"])),
                    row("To", text(&reply["to"])),
                    row("Subject", text(&reply["subject"])),
                ]),
                body: text(&reply["body"]),
                approve: None,
                note: None,
                careful: false,
            }
        }
        ReviewRequest::Smtp(request) => {
            let snapshot = request.snapshot();
            let payload = &snapshot["payload"];
            // A new message and an answer read differently; the draft knows which it is.
            let reply = !text(&payload["in_reply_to"]).is_empty();
            MacWords {
                app: text(&snapshot["publisher"]),
                heading: if reply { "Review Reply" } else { "Review Message" }.into(),
                // The sending address is the account: its internal handle says nothing.
                sentence: who_wants(&text(&snapshot["publisher"]), if reply { "send this reply" } else { "send this message" }),
                facts: facts(vec![
                    row("From", text(&payload["from"])),
                    row("To", text(&payload["to"])),
                    row("Subject", text(&payload["subject"])),
                ]),
                body: text(&payload["body"]),
                approve: None,
                note: None,
                careful: false,
            }
        }
        ReviewRequest::Save(_) | ReviewRequest::Sample { .. } => {
            let snapshot = request.save_snapshot();
            let plain = &snapshot["plain"];
            let mut rows = vec![row("Account", text(&snapshot["account"]))];
            // A website's change names the site, as a Mac's alert does, and what
            // it changes is the Action row under it: never the API's path (the
            // other styles keep the whole address).
            let site_rows: Vec<(String, String)> = plain["facts"].as_array().into_iter().flatten().map(|fact| site_fact(&text(&fact[0]), text(&fact[1]))).collect();
            let site = site_rows.iter().any(|(key, _)| key == "Website");
            rows.extend(site_rows);
            // Data sent to a website is labelled rows under the others, its secrets hidden.
            // A file's or a message's content is the thing being reviewed: exact.
            let body = match (plain.get("data"), plain["body"].as_str()) {
                (Some(data), _) => {
                    rows.extend(crate::approvals::words::change_rows(data));
                    String::new()
                }
                (None, Some(body)) => body.to_string(),
                (None, None) => text(&snapshot["body"]),
            };
            MacWords {
                app: text(&snapshot["app"]),
                heading: plain["title"].as_str().map_or_else(|| text(&snapshot["title"]), str::to_string),
                sentence: who_wants(&text(&snapshot["app"]), &text(&plain["wants"])),
                facts: facts(rows),
                body,
                approve: plain["approve"].as_str().map(str::to_string),
                note: plain["note"].as_str().map(|note| if site { note.replace("the address", "the website").replace("destination", "website") } else { note.to_string() }),
                careful: plain["careful"].as_bool().unwrap_or(false),
            }
        }
    }
}

/// The asking app's icon on a Mac's review. It is drawn in its place in the tree:
/// the card is a draw list of its own, lifted over anything drawn around it.
#[derive(Script, ScriptHook, Widget)]
pub struct ReviewAppIcon {
    #[uid]
    uid: WidgetUid,
    #[walk]
    walk: Walk,
    /// A sheet in script names its app here (`oauth-service`'s sign-in sheets).
    #[live]
    app: String,
    #[rust]
    icons: AppIconDraw,
    #[rust]
    drawn: bool,
    #[redraw]
    #[rust]
    area: Area,
}
impl Widget for ReviewAppIcon {
    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let rect = cx.walk_turtle_with_area(&mut self.area, walk);
        if !self.app.is_empty() {
            let name = crate::approvals::mac_kit::icon_name(&self.app);
            self.icons.draw(cx, &name, DesktopStyle::Macos, rect, 1.0, vec4(0.5, 0.5, 0.5, 1.0));
            // A new icon's shader is ready only for the next frame, and nothing
            // else on the sheet asks for one.
            if !std::mem::replace(&mut self.drawn, true) {
                self.area.redraw(cx);
            }
        }
        DrawStep::done()
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct ConnectedReplyReview {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    ticket: String,
    #[rust]
    request: Option<ReviewRequest>,
    #[rust]
    initialized: bool,
    #[rust]
    trusted_down: bool,
    #[rust]
    finished: bool,
    #[rust]
    is_save: bool,
    /// A Mac's review of a change that goes to a website: it is sent, not saved.
    #[rust]
    sends: bool,
    /// A Mac's review of what undoes something: the other pair of buttons is up.
    #[rust]
    careful: bool,
    /// A Mac's message from an account that isn't here: Open Mail is the default.
    #[rust]
    no_account: bool,
    #[live]
    close_service: String,
    #[live]
    close_ticket: String,
    /// The desktop tree: its controls look disabled while they cannot be pressed.
    #[live]
    desk: bool,
    /// The Mac's tree: a sentence and labelled rows, the app by its name.
    #[live]
    mac: bool,
    /// The label the content was last filled into.
    #[rust]
    filled: Option<WidgetUid>,
}

impl ConnectedReplyReview {
    /// A phone's style giving way to a desktop's builds the other tree, whose
    /// labels start empty: a review must never stand blank with Approve live.
    fn refill_if_rebuilt(&mut self, cx: &mut Cx) {
        let label = self.view.widget(cx, ids!(message)).widget_uid();
        if self.filled.is_some_and(|filled| filled != label) {
            self.trusted_down = false;
            // A final status, if there was one, comes back with the next event.
            self.finished = false;
            self.refresh_snapshot(cx);
        }
        self.filled = Some(label);
    }
    /// `set_enabled` alone leaves a button's look; a phone keeps that.
    fn dim_disabled(&mut self, cx: &mut Cx) {
        if self.desk {
            for button in [
                self.view.button(cx, ids!(approve)),
                self.view.button(cx, ids!(cancel)),
                self.view.button(cx, ids!(approve_plain)),
                self.view.button(cx, ids!(cancel_blue)),
            ] {
                if let Some(mut button) = button.borrow_mut() {
                    let off = !button.enabled();
                    if button.disabled(cx) != off {
                        button.set_disabled(cx, off);
                    }
                }
            }
        }
    }
    fn close_sheet(&self, cx: &mut Cx) {
        if makepad_widgets::splash_policy::is_enforced(self.source.heap_key()) {
            return;
        }
        // Close only this originating host sheet. The same app can also
        // be resident in its ordinary window or another Glance workspace.
        if !matches!(
            self.close_service.as_str(),
            "gmail.sheet.close"
                | "mail.sheet.review_close"
                | "github.sheet.cancel"
                | "gcalendar.sheet.cancel"
        ) {
            return;
        }
        if let Some(owner) = cx.script_ref_vm_id(&self.source) {
            cx.with_script_vm_id(owner, |vm| {
                vm.eval(ScriptMod {
                    cargo_manifest_path: env!("CARGO_MANIFEST_DIR").into(),
                    module_path: "native_review_close".into(),
                    file: "native_review_close.splash".into(),
                    line: 0,
                    column: 0,
                    code: format!(
                        "mod.host.request({}, {{ticket: {}}}, nil)",
                        serde_json::json!(self.close_service),
                        serde_json::json!(self.close_ticket)
                    ),
                    values: vec![],
                });
            });
        }
    }
    fn initialize(&mut self, cx: &mut Cx) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        if makepad_widgets::splash_policy::is_enforced(self.source.heap_key()) {
            self.view.button(cx, ids!(approve)).set_enabled(cx, false);
            return;
        }
        self.request = pending()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.ticket)
            .filter(|(when, _)| when.elapsed().as_secs() < 600)
            .map(|(_, r)| r);
        self.refresh_snapshot(cx);
    }
    fn refresh_snapshot(&mut self, cx: &mut Cx) {
        if let Some(request) = &mut self.request {
            let _ = request.result();
            (self.close_service, self.close_ticket) = request.close_request();
            let text = |value: &serde_json::Value| value.as_str().unwrap_or("").to_owned();
            let (heading, account, details, subject, body) = match request {
                ReviewRequest::Mail(request) => {
                    let snapshot = request.snapshot();
                    let reply = &snapshot["reply"];
                    (
                        "Review reply".to_owned(),
                        format!("{}\nFrom: {}", text(&snapshot["app"]), text(&reply["from"])),
                        format!("To: {}", text(&reply["to"])),
                        text(&reply["subject"]),
                        text(&reply["body"]),
                    )
                }
                ReviewRequest::Smtp(request) => {
                    self.view
                        .button(cx, ids!(approve))
                        .set_enabled(cx, request.can_approve());
                    self.view.label(cx, ids!(status)).set_text(
                        cx,
                        if request.is_preparing() {
                            "Preparing the complete message for review…"
                        } else {
                            "Check the recipient and the complete message before sending."
                        },
                    );
                    let snapshot = request.snapshot();
                    let payload = &snapshot["payload"];
                    (
                        "Review message".to_owned(),
                        format!(
                            "{}\nAccount: {}\nFrom: {}",
                            text(&snapshot["publisher"]),
                            text(&snapshot["account"]),
                            text(&payload["from"])
                        ),
                        format!("To: {}", text(&payload["to"])),
                        text(&payload["subject"]),
                        text(&payload["body"]),
                    )
                }
                ReviewRequest::Save(_) | ReviewRequest::Sample { .. } => {
                    self.is_save = true;
                    let snapshot = request.save_snapshot();
                    self.view
                        .button(cx, ids!(approve))
                        .set_text(cx, "Approve & Save");
                    self.view.label(cx, ids!(status)).set_text(
                        cx,
                        "Review the account, destination and exact content before saving.",
                    );
                    (
                        text(&snapshot["title"]),
                        format!(
                            "{}\nAccount: {}",
                            text(&snapshot["app"]),
                            text(&snapshot["account"])
                        ),
                        text(&snapshot["details"]),
                        String::new(),
                        text(&snapshot["body"]),
                    )
                }
            };
            if self.mac {
                let words = mac_words(request);
                self.sends = words.approve.is_some();
                self.careful = words.careful;
                if let Some(word) = &words.approve {
                    self.view.button(cx, ids!(approve)).set_text(cx, word);
                    self.view.button(cx, ids!(approve_plain)).set_text(cx, word);
                }
                self.view.widget(cx, ids!(approve)).set_visible(cx, !words.careful);
                self.view.widget(cx, ids!(cancel)).set_visible(cx, !words.careful);
                self.view.widget(cx, ids!(approve_plain)).set_visible(cx, words.careful);
                self.view.widget(cx, ids!(cancel_blue)).set_visible(cx, words.careful);
                if let Some(mut icon) = self.view.widget(cx, ids!(app_icon)).borrow_mut::<ReviewAppIcon>() {
                    icon.app = words.app.clone();
                }
                self.view.view(cx, ids!(icon_box)).set_visible(cx, !words.app.is_empty());
                if let Some(note) = &words.note {
                    self.view.label(cx, ids!(status)).set_text(cx, note);
                }
                self.view.label(cx, ids!(heading)).set_text(cx, &words.heading);
                self.view.label(cx, ids!(account)).set_text(cx, &words.sentence);
                self.view.label(cx, ids!(recipient)).set_text(cx, &details);
                self.view.label(cx, ids!(subject)).set_text(cx, &subject);
                self.view.view(cx, ids!(facts)).set_visible(cx, !words.facts.is_empty());
                for i in 0..FACT_ROWS {
                    let (row, key, value) = fact_row(i);
                    let fact = words.facts.get(i);
                    self.view.view(cx, row).set_visible(cx, fact.is_some());
                    self.view.label(cx, key).set_text(cx, fact.map_or("", |(k, _)| k.as_str()));
                    self.view.label(cx, value).set_text(cx, fact.map_or("", |(_, v)| v.as_str()));
                }
                self.view
                    .view(cx, ids!(message_box))
                    .set_visible(cx, !words.body.is_empty());
                self.view.label(cx, ids!(message)).set_text(cx, &words.body);
                return;
            }
            self.view.label(cx, ids!(heading)).set_text(cx, &heading);
            self.view.label(cx, ids!(account)).set_text(cx, &account);
            self.view.label(cx, ids!(recipient)).set_text(cx, &details);
            self.view.label(cx, ids!(subject)).set_text(cx, &subject);
            self.view
                .label(cx, ids!(subject))
                .set_visible(cx, !subject.is_empty());
            self.view.label(cx, ids!(message)).set_text(cx, &body);
        } else {
            self.view.label(cx, ids!(status)).set_text(
                cx,
                "This review expired. Return to the app and review your saved draft again.",
            );
            self.view.button(cx, ids!(approve)).set_enabled(cx, false);
            self.careful = false;
        }
    }
}
impl Widget for ConnectedReplyReview {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.initialize(cx);
        self.refill_if_rebuilt(cx);
        self.dim_disabled(cx);
        self.view.draw_walk(cx, scope, walk)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.initialize(cx);
        let preparing = self
            .request
            .as_ref()
            .is_some_and(ReviewRequest::is_preparing);
        if preparing {
            self.refresh_snapshot(cx);
            // The worker's answer has to be read here, whether or not anything else happens.
            cx.new_next_frame();
        }
        if matches!(event, Event::Pause | Event::Background) {
            self.trusted_down = false;
            if let Some(mut request) = self.request.take() {
                let _ = request.cancel();
            }
            return;
        }
        if !self.finished {
            if let Some(result) = self.request.as_mut().and_then(ReviewRequest::result) {
                self.finished = true;
                let no_account = self.mac && matches!(self.request, Some(ReviewRequest::Smtp(_))) && matches!(&result, Err(error) if error == NO_ACCOUNT);
                let message = match result {
                    Err(_) if no_account => {
                        "The mail account it uses isn't set up here. Add the account in Mail, then send the message again.".to_owned()
                    }
                    Ok(_) if self.mac && self.sends => {
                        "The reviewed change was sent. Return to the app.".to_owned()
                    }
                    Ok(_) if self.is_save => {
                        "The reviewed change was saved. Return to the app.".to_owned()
                    }
                    Ok(value) if value["status"] == "accepted" => {
                        "The mail provider accepted the message. Return to the app.".to_owned()
                    }
                    Ok(value) => format!(
                        "Submission status: {}. Check the saved receipt before trying again.",
                        value["status"].as_str().unwrap_or("unknown")
                    ),
                    Err(error) => error,
                };
                self.view.label(cx, ids!(status)).set_text(cx, &message);
                self.view.button(cx, ids!(approve)).set_enabled(cx, false);
                if self.mac {
                    // How it ended is the sheet's news, and nothing is left to approve.
                    let outcome = self.view.label(cx, ids!(outcome));
                    outcome.set_text(cx, &message);
                    outcome.set_visible(cx, true);
                    self.view.label(cx, ids!(status)).set_visible(cx, false);
                    self.view.button(cx, ids!(approve)).set_visible(cx, false);
                    self.view.button(cx, ids!(approve_plain)).set_visible(cx, false);
                    if no_account {
                        // An alert's shape: what happened beside the icon, then the way on.
                        self.view.label(cx, ids!(heading)).set_text(cx, "Can't Send This Message");
                        self.view.label(cx, ids!(account)).set_text(cx, &message);
                        self.view.label(cx, ids!(outcome)).set_visible(cx, false);
                        self.view.button(cx, ids!(open_mail)).set_visible(cx, true);
                    }
                    self.no_account = no_account;
                }
                let cancel = self.view.button(cx, if self.careful { ids!(cancel_blue) } else { ids!(cancel) });
                cancel.set_enabled(cx, true);
                cancel.set_text(cx, if self.mac { "Return to App" } else { "Return to app" });
            }
        }
        // The worker's answer can arrive between the two reads above: what the
        // review shows and whether Approve can be pressed must follow it.
        // On a Mac how it ended has replaced the review's words by then, and stays.
        if preparing && !self.request.as_ref().is_some_and(ReviewRequest::is_preparing) && !(self.mac && self.finished) {
            self.refresh_snapshot(cx);
        }
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        let approve = self.view.button(cx, if self.careful { ids!(approve_plain) } else { ids!(approve) });
        if approve.pressed(&actions) {
            self.trusted_down = makepad_platform::trusted_user_input();
        }
        let cancel = self.view.button(cx, if self.careful { ids!(cancel_blue) } else { ids!(cancel) });
        // Escape is Cancel's on a Mac, and so is Return where Cancel is the default.
        let by_key = self.mac
            && matches!(event, Event::KeyDown(e) if e.key_code == KeyCode::Escape
                || (self.careful && matches!(e.key_code, KeyCode::ReturnKey | KeyCode::NumpadEnter)))
            && cancel.borrow().is_some_and(|button| button.enabled());
        if cancel.clicked(&actions) || by_key {
            if let Some(mut request) = self.request.take() {
                let _ = request.cancel();
            }
            self.close_sheet(cx);
        }
        // The way to Mail, from a host sheet and nowhere else.
        let mail_by_key = self.no_account && matches!(event, Event::KeyDown(e) if matches!(e.key_code, KeyCode::ReturnKey | KeyCode::NumpadEnter));
        if (self.view.button(cx, ids!(open_mail)).clicked(&actions) || mail_by_key) && !makepad_widgets::splash_policy::is_enforced(self.source.heap_key()) {
            if let Some(mut request) = self.request.take() {
                let _ = request.cancel();
            }
            self.close_sheet(cx);
            OPEN_MAIL.store(true, Ordering::Relaxed);
            SignalToUI::set_ui_signal();
        }
        if approve.clicked(&actions) {
            let down = std::mem::take(&mut self.trusted_down);
            if let Some(request) = &mut self.request {
                match request.approve(down, makepad_platform::trusted_user_input()) {
                    Ok(()) => {
                        self.view.label(cx, ids!(status)).set_text(
                            cx,
                            if self.mac && self.sends {
                                "Sending this exact reviewed change…"
                            } else if self.is_save {
                                "Saving this exact reviewed version…"
                            } else if self.mac {
                                "Sending this exact reviewed message…"
                            } else {
                                "Sending this exact reviewed reply…"
                            },
                        );
                        approve.set_enabled(cx, false);
                        cancel.set_enabled(cx, false);
                    }
                    Err(error) => self.view.label(cx, ids!(status)).set_text(cx, &error),
                }
            }
        } else if approve.released(&actions) {
            self.trusted_down = false;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A Mac's review names the website a change goes to, never its API's
    /// path; other rows are as they were.
    #[test]
    fn a_macs_review_names_the_site_not_the_path() {
        let site = |key: &str, value: &str| site_fact(key, value.to_string());
        assert_eq!(site("Sends to", "https://backend.example.test/api/notes"), ("Website".into(), "backend.example.test".into()));
        assert_eq!(site("Deletes", "https://backend.example.test/api/notes/trail?id=4"), ("Website".into(), "backend.example.test".into()));
        assert_eq!(site("Changes", "http://me@notes.example.test:8080/v1"), ("Website".into(), "notes.example.test:8080".into()));
        assert_eq!(site("Action", "Remove notes"), ("Action".into(), "Remove notes".into()));
        assert_eq!(site("File", "https://example.test/readme"), ("File".into(), "https://example.test/readme".into()));
    }

    #[test]
    fn native_review_close_is_scoped_and_contained_copies_cannot_use_it() {
        // The real shell owns one Cx. Keep its isolate registry alive across
        // both routes instead of reusing allocator heap addresses while the
        // runtime's thread-local policy records await isolate collection.
        let mut cx = Cx::new(Box::new(|_, _| {}));
        register();
        for service in ["github.sheet.cancel", "mail.sheet.review_close"] {
            let mut make = |contained| {
                let mut splash = cx.with_vm(|vm| {
                    makepad_widgets::script_mod(vm);
                    script_mod(vm);
                    let value = vm.eval(script! { use mod.widgets.* Splash {} });
                    Splash::script_from_value(vm, value)
                });
                if contained {
                    splash.set_policy(&mut cx, Some(vec![]), None);
                }
                splash.set_text(
                    &mut cx,
                    &r#"review := ConnectedReplyReview {
                ticket: "missing-native-capability"
                close_service: "github.sheet.cancel"
                close_ticket: "synthetic-ticket"
            }"#
                    .replace("github.sheet.cancel", service),
                );
                splash
            };
            let host = make(false);
            let app = make(true);
            for (splash, contained) in [(&host, false), (&app, true)] {
                let widget = splash
                    .view
                    .children
                    .iter()
                    .find(|(id, _)| *id == id!(review))
                    .unwrap()
                    .1
                    .clone();
                let mut review = widget.borrow_mut::<ConnectedReplyReview>().unwrap();
                review.initialize(&mut cx);
                assert!(
                    review.request.is_none(),
                    "live fields cannot forge a native capability"
                );
                review.close_sheet(&mut cx);
                let requests =
                    makepad_widgets::splash_host::take_splash_host_requests_for(&[review
                        .source
                        .heap_key()]);
                if contained {
                    assert!(requests.is_empty());
                } else {
                    assert_eq!(requests.len(), 1, "native close route {service}");
                    assert_eq!(requests[0].service, service);
                }
            }
        }
    }

    #[test]
    fn a_desktop_draws_a_mac_dialog_and_a_phone_keeps_its_page() {
        use crate::octosense::style::{load_sheet, DesktopStyle};
        let mut cx = Cx::new(Box::new(|_, _| {}));
        register();
        for (style, dark, desk) in [
            (DesktopStyle::Macos, false, true),
            (DesktopStyle::Macos, true, true),
            (DesktopStyle::Ios, false, false),
        ] {
            let mut sheet = cx.with_vm(|vm| {
                makepad_widgets::script_mod(vm);
                script_mod(vm);
                let value = vm.eval(script! {use mod.widgets.* Splash {}});
                Splash::script_from_value(vm, value)
            });
            sheet.set_stylesheet(&mut cx, load_sheet(style, dark));
            sheet.set_text(
                &mut cx,
                r#"review := ConnectedReplyReview {ticket: "expired"}"#,
            );
            let widget = sheet
                .view
                .children
                .iter()
                .find(|(id, _)| *id == id!(review))
                .unwrap()
                .1
                .clone();
            let mut review = widget.borrow_mut::<ConnectedReplyReview>().unwrap();
            assert_eq!(review.desk, desk);
            review.initialize(&mut cx);
            review.dim_disabled(&mut cx);
            for id in [
                ids!(heading),
                ids!(account),
                ids!(recipient),
                ids!(subject),
                ids!(message),
                ids!(status),
                ids!(cancel),
                ids!(approve),
            ] {
                assert!(!review.view.widget(&mut cx, id).is_empty(), "lost {id:?}");
            }
            let approve = review.view.widget(&mut cx, ids!(approve));
            let walk = approve.walk(&mut cx);
            if desk {
                assert!(matches!(walk.height, Size::Fixed(h) if h == 28.0));
                assert!(!matches!(walk.width, Size::Fill { .. }), "never full width");
            } else {
                assert!(matches!(walk.width, Size::Fill { .. }));
            }
            assert_eq!(
                approve.as_button().borrow().unwrap().disabled(&cx),
                desk,
                "an expired review's Approve looks disabled on a desktop; a phone keeps its look"
            );
        }
    }

    /// One frame, off screen: the frame is where a Splash takes up a new style.
    pub(crate) fn draw_frame(cx: &mut Cx, sheet: &mut Splash) {
        let size = dvec2(900.0, 700.0);
        let pass = DrawPass::new(cx);
        pass.set_size(cx, size);
        let mut list = DrawList2d::new(cx);
        let event = DrawEvent::default();
        let mut draw = CxDraw::new(cx, &event);
        let mut draw = Cx2d::new(&mut draw);
        draw.begin_pass(&pass, Some(1.0));
        list.begin_always(&mut draw);
        draw.begin_root_turtle(size, Layout::default());
        sheet.draw_walk_all(&mut draw, &mut Scope::empty(), Walk::fixed(900.0, 700.0));
        draw.end_turtle();
        list.end(&mut draw);
        draw.end_pass(&pass);
    }

    /// A phone's style giving way to a desktop's brings the other tree, whose
    /// labels start empty: a review must never stand blank with Approve live.
    #[test]
    fn a_review_keeps_its_exact_content_when_the_style_family_changes() {
        use crate::octosense::style::{load_sheet, DesktopStyle};
        use octosense_appstore::services::{self, ServiceCall, ServiceHost};
        let root_dir =
            std::env::temp_dir().join(format!("native-review-flip-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root_dir.join("oauth")).unwrap();
        let app = "org.example.native_review_flip";
        let handle = "6f2c6f6e-1d2b-4c55-8a55-0b3f8c6f2d11";
        // Metadata only, as in the test below: no token, no provider, no approval.
        std::fs::write(root_dir.join("oauth/connections.json"),serde_json::json!({
            "entries":{handle:{"handle":handle,"app_id":app,"provider":"github","subject":"synthetic-review-user",
                "label":"Fictional review account","scopes":["public_repo"],"expires_at":null}},
            "active":{app:handle}
        }).to_string()).unwrap();
        struct Host(String);
        impl ServiceHost for Host {
            fn open_sheet(&mut self, source: String) {
                self.0 = source;
            }
            fn close_sheet(&mut self) {}
        }
        host_api::register_with_review_hook(connector_sheet);
        let mut host = Host(String::new());
        const HEAP: usize = 993721;
        services::dispatch(
            ServiceCall {
                app_id: app.into(),
                service: "github.review_save".into(),
                args: serde_json::json!({"connection":handle,"file":{"owner":"fictional","repo":"notes","branch":"main",
                "path":"note.md","content":"Exact fictional draft","message":"Save note","sha":null}}),
                from_sheet: false,
                may_prompt: true,
                host_dir: root_dir.clone(),
            },
            HEAP,
            1,
            &mut host,
        );
        assert!(!host.0.is_empty());
        let mut cx = Cx::new(Box::new(|_, _| {}));
        register();
        let mut sheet = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            script_mod(vm);
            let value = vm.eval(script! {use mod.widgets.* Splash {width: Fill height: Fill}});
            Splash::script_from_value(vm, value)
        });
        sheet.set_stylesheet(&mut cx, load_sheet(DesktopStyle::Ios, false));
        sheet.set_text(&mut cx, &format!("review := {}", host.0));
        for (style, dark) in [
            (DesktopStyle::Ios, false),
            (DesktopStyle::Macos, false),
            (DesktopStyle::Macos, true),
        ] {
            sheet.set_stylesheet(&mut cx, load_sheet(style, dark));
            draw_frame(&mut cx, &mut sheet);
            let review = sheet
                .view
                .children
                .iter()
                .find(|(id, _)| *id == id!(review))
                .unwrap()
                .1
                .clone();
            let review = review.borrow::<ConnectedReplyReview>().unwrap();
            assert_eq!(review.desk, style == DesktopStyle::Macos);
            let text = |cx: &mut Cx, id: &[LiveId]| review.view.widget(cx, id).text();
            assert_eq!(text(&mut cx, ids!(message)), "Exact fictional draft");
            assert_eq!(text(&mut cx, ids!(heading)), "Save Markdown to GitHub");
            let account = if style == DesktopStyle::Macos { text(&mut cx, ids!(fact0.v)) } else { text(&mut cx, ids!(account)) };
            assert!(account.contains("Fictional review account"));
            assert!(text(&mut cx, ids!(recipient)).contains("note.md"));
            assert_eq!(text(&mut cx, ids!(approve)), "Approve & Save");
        }
        drop(sheet);
        services::cancel_heap(HEAP);
        std::fs::remove_dir_all(root_dir).unwrap();
    }

    #[test]
    fn a_contained_copy_cannot_claim_a_real_pending_host_review_ticket() {
        use octosense_appstore::services::{self, ServiceCall, ServiceHost};
        let root =
            std::env::temp_dir().join(format!("native-review-ticket-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("oauth")).unwrap();
        let app = "org.example.native_review_test";
        let handle = "45c5676e-821e-4a6b-9719-301f8790fc52";
        // Metadata only: this test never creates, reads or writes a provider
        // token, and never approves a write or calls a provider endpoint.
        std::fs::write(root.join("oauth/connections.json"),serde_json::json!({
            "entries":{handle:{"handle":handle,"app_id":app,"provider":"github","subject":"synthetic-review-user",
                "label":"Fictional review account","scopes":["public_repo"],"expires_at":null}},
            "active":{app:handle}
        }).to_string()).unwrap();
        struct Host(String);
        impl ServiceHost for Host {
            fn open_sheet(&mut self, source: String) {
                self.0 = source;
            }
            fn close_sheet(&mut self) {}
        }
        host_api::register_with_review_hook(connector_sheet);
        let mut host = Host(String::new());
        const HEAP: usize = 993720;
        services::dispatch(
            ServiceCall {
                app_id: app.into(),
                service: "github.review_save".into(),
                args: serde_json::json!({"connection":handle,"file":{"owner":"fictional","repo":"notes","branch":"main",
                "path":"note.md","content":"Exact fictional draft","message":"Save note","sha":null}}),
                from_sheet: false,
                may_prompt: true,
                host_dir: root.clone(),
            },
            HEAP,
            1,
            &mut host,
        );
        assert!(
            !host.0.is_empty(),
            "the real connector must create a pending native capability"
        );
        let mut cx = Cx::new(Box::new(|_, _| {}));
        register();
        let mut make = |contained| {
            let mut splash = cx.with_vm(|vm| {
                makepad_widgets::script_mod(vm);
                script_mod(vm);
                let value = vm.eval(script! {use mod.widgets.* Splash {}});
                Splash::script_from_value(vm, value)
            });
            if contained {
                splash.set_policy(&mut cx, Some(vec![]), None);
            }
            splash.set_text(&mut cx, &format!("review := {}", host.0));
            splash
        };
        let untrusted = make(true);
        let trusted = make(false);
        let find = |splash: &Splash| {
            splash
                .view
                .children
                .iter()
                .find(|(id, _)| *id == id!(review))
                .unwrap()
                .1
                .clone()
        };
        let copy = find(&untrusted);
        let owner = find(&trusted);
        let ticket = copy
            .borrow::<ConnectedReplyReview>()
            .unwrap()
            .ticket
            .clone();
        assert!(pending().lock().unwrap().contains_key(&ticket));
        copy.borrow_mut::<ConnectedReplyReview>()
            .unwrap()
            .initialize(&mut cx);
        assert!(copy
            .borrow::<ConnectedReplyReview>()
            .unwrap()
            .request
            .is_none());
        assert!(
            pending().lock().unwrap().contains_key(&ticket),
            "the hostile copy cannot consume the ticket"
        );
        owner
            .borrow_mut::<ConnectedReplyReview>()
            .unwrap()
            .initialize(&mut cx);
        assert!(owner
            .borrow::<ConnectedReplyReview>()
            .unwrap()
            .request
            .is_some());
        assert!(!pending().lock().unwrap().contains_key(&ticket));
        drop(owner);
        drop(copy);
        drop(trusted);
        drop(untrusted);
        services::cancel_heap(HEAP);
        std::fs::remove_dir_all(root).unwrap();
    }
}
