//! Consent is collected in a native widget owned by the host sheet. A script
//! cannot approve by calling a sheet method, mounting a copy, or remote clicks.
use super::*;
use crate::octosense::style::{AppIconDraw, DesktopStyle};
use makepad_widgets::*;

pub(super) fn register() {
    widget_async::register_splash_isolate_mod(|vm| {
        // An app's own icon art is drawn with this, and the alert runs in the sheet's isolate.
        crate::octosense::icon_frame::script_mod(vm);
        script_mod(vm);
        script_eval!(vm, {mod.prelude.widgets.DevicePermissionPrompt = mod.widgets.DevicePermissionPrompt});
    });
}
script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    // A phone keeps this sheet's fixed look; a desktop's is a Mac dialog that
    // follows the appearance (`border_radius` is half the corner it draws).
    let desk = try { theme.ui_pointer == true } catch { false }
    // The Mac style: a Mac's privacy alert, as consent asks for an assistant (approvals
    // mac_view): the app's icon on top, the question and what it covers centred under
    // it, Don't Allow and Allow side by side, attached under the window's top band over
    // a light dim. Every other desktop style keeps what it had.
    let desk_mac = desk && try { theme.ui_unified == true } catch { false }
    let desk_dark = desk && dot(theme.color_text, #x36b61200) >= 0.5
    fn tone(light, dark){ if desk_dark { dark } else { light } }
    let sh_ink = tone(#x1d1d1f, #xf5f5f7)
    let sh_muted = tone(#x636368, #xa1a1a6)
    let sh_control = tone(#xffffff, #x48484a)
    let sh_control_line = tone(#xc9c9ce, #x545458)
    let sh_off = tone(#xe3e3e8, #x3a3a3c)
    let sh_off_ink = tone(#x636368, #xaeaeb2)
    mod.widgets.DeviceAlertIcon = set_type_default() do #(DeviceAlertIcon::register_widget(vm)) {width: 48 height: 48}
    mod.widgets.DevicePermissionPromptBase = #(DevicePermissionPrompt::register_widget(vm))
    mod.widgets.DevicePermissionPrompt = if desk_mac {
        set_type_default() do mod.widgets.DevicePermissionPromptBase {
            desk: true
            mac: true
            width: Fill height: Fill flow: Down new_batch: true
            show_bg: true draw_bg +: {color: instance(tone(#x00000024, #x00000052)) pixel: fn(){return Pal.premul(self.color)}}
            ScrollYView {width: Fill height: Fill flow: Down align: Align{x: 0.5} padding: Inset{left: 12 right: 12 top: 52 bottom: 24}
                RoundedShadowView {width: 300 height: Fit flow: Down align: Align{x: 0.5} padding: 16 new_batch: true show_bg: true
                    draw_bg +: {color: tone(#xf6f6f8, #x2c2c2e) border_radius: 6.0 border_size: 1.0 border_color: tone(#xe3e3e8, #x48484a) shadow_color: tone(#x00000040, #x00000080) shadow_radius: 18.0 shadow_offset: vec2(0.0, 6.0)}
                    app_icon := mod.widgets.DeviceAlertIcon {width: 64 height: 64}
                    // A Label's margin counts twice: 10 points over the question, 4 over the rest.
                    title := Label {clip_y: false width: Fill height: Fit padding: 0 margin: Inset{top: 5} align: Align{x: 0.5} flow: Flow.Right{wrap: true} text: "Device Permission" draw_text +: {color: sh_ink text_style: theme.font_bold{font_size: 9.75 line_spacing: 1.3}}}
                    details := Label {clip_y: false width: Fill height: Fit padding: 0 margin: Inset{top: 2} align: Align{x: 0.5} flow: Flow.Right{wrap: true} draw_text +: {color: sh_muted text_style: theme.font_regular{font_size: 8.25 line_spacing: 1.3}}}
                    explanation := Label {clip_y: false width: Fill height: Fit padding: 0 margin: Inset{top: 2} align: Align{x: 0.5} flow: Flow.Right{wrap: true} text: "Your computer may ask you too." draw_text +: {color: sh_muted text_style: theme.font_regular{font_size: 8.25 line_spacing: 1.3}}}
                    status := Label {visible: false clip_y: false width: Fill height: Fit padding: 0 margin: Inset{top: 2} align: Align{x: 0.5} flow: Flow.Right{wrap: true} text: "" draw_text +: {color: sh_muted text_style: theme.font_regular{font_size: 8.25 line_spacing: 1.3}}}
                    View {width: Fill height: Fit flow: Right spacing: 8 margin: Inset{top: 14}
                        cancel := ButtonFlat {width: Fill height: 28 margin: 0 padding: Inset{left: 12 right: 12 bottom: 2} text: "Don't Allow"
                            draw_bg +: {border_radius: 3.0 border_size: 1.0 color: sh_control color_hover: tone(#xf2f2f4, #x545458) color_down: tone(#xe5e5ea, #x5e5e62) color_focus: sh_control
                                border_color: sh_control_line border_color_hover: sh_control_line border_color_down: sh_control_line border_color_focus: sh_control_line}
                            draw_text +: {color: sh_ink color_hover: sh_ink color_down: sh_ink color_focus: sh_ink text_style: theme.font_regular{font_size: 9.75}}
                        }
                        approve := ButtonFlat {width: Fill height: 28 margin: 0 padding: Inset{left: 16 right: 16 bottom: 2} text: "Allow"
                            draw_bg +: {border_radius: 3.0 border_size: 0.0 color: #x0071e3 color_hover: #x006edb color_down: #x0060c0 color_focus: #x0071e3 color_disabled: sh_off}
                            draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff color_focus: #xffffff color_disabled: sh_off_ink text_style: theme.font_regular{font_size: 9.75}}
                        }
                    }
                }
            }
        }
    } else if desk {
        set_type_default() do mod.widgets.DevicePermissionPromptBase {
            desk: true
            width: Fill height: Fill flow: Down new_batch: true
            show_bg: true draw_bg +: {color: instance(tone(#x00000040, #x00000066)) pixel: fn(){return Pal.premul(self.color)}}
            ScrollYView {width: Fill height: Fill flow: Down align: Align{x: 0.5} padding: Inset{left: 12 right: 12 top: 40 bottom: 24}
                RoundedView {width: Fill max_width: 480 height: Fit flow: Down spacing: 8 padding: 18 new_batch: true show_bg: true
                    draw_bg +: {color: tone(#xf6f6f8, #x2c2c2e) border_radius: 5.0 border_size: 1.0 border_color: tone(#xe3e3e8, #x48484a)}
                    title := Label {width: Fill height: Fit padding: 0 text: "Device permission" draw_text.color: sh_ink draw_text.wrap: Words draw_text.text_style: theme.font_bold{font_size: 12.75}}
                    details := Label {width: Fill height: Fit padding: 0 draw_text.color: sh_ink draw_text.wrap: Words draw_text.text_style.font_size: 9.75}
                    explanation := Label {width: Fill height: Fit padding: 0 text: "This grants this app access in OctoSense. Your operating system may ask separately. Other installed apps do not receive this consent." draw_text.color: sh_muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                    status := Label {width: Fill height: Fit padding: 0 text: "Choose Continue to review the operating system permission." draw_text.color: sh_muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                    View {width: Fill height: Fit flow: Right spacing: 8 align: Align{x: 1.0 y: 0.5} margin: Inset{top: 6}
                        cancel := ButtonFlat {height: 28 margin: 0 padding: Inset{left: 12 right: 12} text: "Not now"
                            draw_bg +: {border_radius: 3.0 border_size: 1.0 color: sh_control color_hover: tone(#xf2f2f4, #x545458) color_down: tone(#xe5e5ea, #x5e5e62) color_focus: sh_control
                                border_color: sh_control_line border_color_hover: sh_control_line border_color_down: sh_control_line border_color_focus: sh_control_line}
                            draw_text +: {color: sh_ink color_hover: sh_ink color_down: sh_ink color_focus: sh_ink text_style: theme.font_regular{font_size: 9.75}}
                        }
                        approve := ButtonFlat {height: 28 margin: 0 padding: Inset{left: 16 right: 16} text: "Continue"
                            draw_bg +: {border_radius: 3.0 border_size: 0.0 color: #x0071e3 color_hover: #x006edb color_down: #x0060c0 color_focus: #x0071e3 color_disabled: sh_off}
                            draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff color_focus: #xffffff color_disabled: sh_off_ink text_style: theme.font_regular{font_size: 9.75}}
                        }
                    }
                }
            }
        }
    } else {
        set_type_default() do mod.widgets.DevicePermissionPromptBase {
            width: Fill height: Fill flow: Down padding: 24 spacing: 18
            show_bg: true draw_bg +: {color: instance(#fff) pixel: fn(){return Pal.premul(self.color)}}
            title := Label {width: Fill height: Fit text: "Device permission" draw_text.color: #x172336 draw_text.text_style.font_size: 22}
            ScrollYView {width: Fill height: Fill flow: Down spacing: 16
                details := Label {width: Fill height: Fit draw_text.color: #x172336 draw_text.wrap: Words draw_text.text_style.font_size: 16}
                explanation := Label {width: Fill height: Fit text: "This grants this app access in OctoSense. Your operating system may ask separately. Other installed apps do not receive this consent." draw_text.color: #x526071 draw_text.wrap: Words}
                status := Label {width: Fill height: Fit text: "Choose Continue to review the operating system permission." draw_text.color: #x526071 draw_text.wrap: Words}
            }
            View {width: Fill height: 48 spacing: 12
                cancel := ButtonFlat {width: Fill height: Fill text: "Not now"}
                approve := Button {width: Fill height: Fill text: "Continue"}
            }
        }
    }
}
#[derive(Script, ScriptHook, Widget)]
pub struct DevicePermissionPrompt {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    ticket: String,
    // Supplied by the trusted host declaration so an already-expired request
    // can still close before its first draw. It grants no approval authority.
    #[live]
    family: String,
    /// The desktop tree: Continue looks disabled while it cannot be pressed.
    #[live]
    desk: bool,
    /// The Mac's tree: an alert with the app's icon and its name.
    #[live]
    mac: bool,
    #[rust]
    initialized: bool,
    #[rust]
    trusted_down: bool,
    /// The label the request was last described in.
    #[rust]
    filled: Option<WidgetUid>,
}
/// The asking app's icon in a Mac alert. It is drawn in its place in the tree: the
/// card is a draw list of its own, lifted over anything drawn around it.
#[derive(Script, ScriptHook, Widget)]
pub struct DeviceAlertIcon {
    #[uid]
    uid: WidgetUid,
    #[walk]
    walk: Walk,
    #[rust]
    app: String,
    #[rust]
    icons: AppIconDraw,
    #[rust]
    drawn: bool,
    #[redraw]
    #[rust]
    area: Area,
}
impl Widget for DeviceAlertIcon {
    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let rect = cx.walk_turtle_with_area(&mut self.area, walk);
        if !self.app.is_empty() {
            let name = crate::approvals::mac_kit::icon_name(&self.app);
            self.icons.draw(cx, &name, DesktopStyle::Macos, rect, 1.0, vec4(0.5, 0.5, 0.5, 1.0));
            // A new icon's shader is ready only for the next frame, and nothing
            // else on an alert asks for one.
            if !std::mem::replace(&mut self.drawn, true) {
                self.area.redraw(cx);
            }
        }
        DrawStep::done()
    }
}
/// A Mac alert's title and message for a device. What it grants is the same as
/// on the other trees: this app, all of its accounts, in OctoSense, and only this
/// app. Another app's id comes along: its name is its own text.
fn device_words(name: &str, id: Option<&str>, family: &str) -> (String, String) {
    let device = match family {
        "camera" | "microphone" | "location" => format!("your {family}"),
        other => other.to_string(),
    };
    let who = match id {
        Some(id) => format!("{name} ({id})"),
        None => name.to_string(),
    };
    (
        format!("Allow {name} to use {device}?"),
        format!("{who} can use it with any of its accounts. Other apps can't."),
    )
}
fn can_approve(contained: bool, down: bool, up: bool) -> bool {
    !contained && down && up
}
impl DevicePermissionPrompt {
    /// A phone's style giving way to a desktop's builds the other tree, whose
    /// labels start empty: consent must never stand blank with Continue live.
    fn refill_if_rebuilt(&mut self, cx: &mut Cx) {
        let label = self.view.widget(cx, ids!(details)).widget_uid();
        if self.filled.is_some_and(|filled| filled != label) {
            self.trusted_down = false;
            self.initialized = false;
            self.initialize(cx);
        }
        self.filled = Some(label);
    }
    /// `set_enabled` alone leaves a button's look; a phone keeps that.
    fn dim_disabled(&mut self, cx: &mut Cx) {
        if self.desk {
            if let Some(mut approve) = self.view.button(cx, ids!(approve)).borrow_mut() {
                let off = !approve.enabled();
                if approve.disabled(cx) != off {
                    approve.set_disabled(cx, off);
                }
            }
        }
    }
    fn initialize(&mut self, cx: &mut Cx) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let state = state().lock().unwrap_or_else(|e| e.into_inner());
        let review = state.reviews.get(&self.ticket).filter(|review| {
            !splash_policy::is_enforced(self.source.heap_key())
                && review.created.elapsed() < REVIEW_TTL
                && review.work.reply.is_pending()
        });
        if let Some(review) = review {
            self.family = review.work.family.into();
            if self.mac {
                let app = review.work.call.app_id.clone();
                let (name, id) = crate::connected_review::shown_app(&app);
                if let Some(mut icon) = self.view.widget(cx, ids!(app_icon)).borrow_mut::<DeviceAlertIcon>() {
                    icon.app = app;
                }
                let (title, message) = device_words(&name, id.as_deref(), &self.family);
                self.view.label(cx, ids!(title)).set_text(cx, &title);
                self.view.label(cx, ids!(details)).set_text(cx, &message);
            } else {
                self.view.label(cx, ids!(details)).set_text(
                    cx,
                    &format!(
                        "Allow {} to use {}?\n\nDevice access applies to this app across its accounts.",
                        review.work.call.app_id, self.family
                    ),
                );
            }
        } else {
            self.view.label(cx, ids!(status)).set_text(
                cx,
                "This permission request is no longer available. Return to the app.",
            );
            self.view.widget(cx, ids!(status)).set_visible(cx, true);
            self.view.button(cx, ids!(approve)).set_enabled(cx, false);
        }
    }
    fn close(&self, cx: &mut Cx) {
        if splash_policy::is_enforced(self.source.heap_key()) || permission(&self.family).is_none()
        {
            return;
        }
        if let Some(owner) = cx.script_ref_vm_id(&self.source) {
            cx.with_script_vm_id(owner, |vm| {
                vm.eval(ScriptMod {
                    cargo_manifest_path: env!("CARGO_MANIFEST_DIR").into(),
                    module_path: "device_consent_close".into(),
                    file: "device_consent_close.splash".into(),
                    line: 0,
                    column: 0,
                    code: format!(
                        "mod.host.request({}, {{ticket: {}}}, nil)",
                        json!(format!("{}.sheet.close", self.family)),
                        json!(self.ticket)
                    ),
                    values: vec![],
                });
            });
        }
    }
    fn approve(&self, down: bool, up: bool) -> Result<(), String> {
        if !can_approve(splash_policy::is_enforced(self.source.heap_key()), down, up) {
            return Err("Use the device's physical controls to grant permission.".into());
        }
        let mut state = state().lock().unwrap_or_else(|e| e.into_inner());
        let mut review = state
            .reviews
            .remove(&self.ticket)
            .ok_or("This permission request expired")?;
        if !review.work.reply.is_pending()
            || review.created.elapsed() >= REVIEW_TTL
            || !review.work.policy_allows()
        {
            review.work.fail(
                "cancelled",
                "The app or permission request is no longer active",
            );
            return Err("The app or request is no longer active".into());
        }
        match consent::set(
            &review.work.call.host_dir,
            &review.work.call.app_id,
            review.work.family,
            true,
            Some(review.work.grant.revision),
        ) {
            Ok(grant) => review.work.grant = grant,
            Err(error) => {
                review.work.reply.send(Err(error.clone()));
                return Err(error);
            }
        }
        review.work.deadline = Instant::now() + DEADLINE;
        drop(state);
        queue(review.work);
        Ok(())
    }
}
impl Widget for DevicePermissionPrompt {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.initialize(cx);
        self.refill_if_rebuilt(cx);
        self.dim_disabled(cx);
        self.view.draw_walk(cx, scope, walk)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.initialize(cx);
        if matches!(event, Event::Pause | Event::Background) {
            self.trusted_down = false;
            return;
        }
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        let approve = self.view.button(cx, ids!(approve));
        if approve.pressed(&actions) {
            self.trusted_down = makepad_platform::trusted_user_input();
        }
        let escape = self.mac && matches!(event, Event::KeyDown(e) if e.key_code == KeyCode::Escape);
        if self.view.button(cx, ids!(cancel)).clicked(&actions) || escape {
            self.close(cx);
        }
        if approve.clicked(&actions) {
            let down = std::mem::take(&mut self.trusted_down);
            match self.approve(down, makepad_platform::trusted_user_input()) {
                Ok(()) => {
                    approve.set_enabled(cx, false);
                    self.close(cx);
                }
                Err(error) => {
                    self.view.label(cx, ids!(status)).set_text(cx, &error);
                    self.view.widget(cx, ids!(status)).set_visible(cx, true);
                }
            }
        } else if approve.released(&actions) {
            self.trusted_down = false;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_request_expired_before_first_draw_can_still_dismiss_its_native_sheet() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        register();
        let mut sheet = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            script_mod(vm);
            let value = vm.eval(script! {use mod.widgets.* Splash {}});
            Splash::script_from_value(vm, value)
        });
        sheet.set_text(&mut cx,
            r#"prompt := DevicePermissionPrompt {ticket: "already-expired" family: "camera"}"#);
        let widget = sheet.view.children.iter().find(|(id, _)| *id == id!(prompt)).unwrap().1.clone();
        let mut prompt = widget.borrow_mut::<DevicePermissionPrompt>().unwrap();
        prompt.initialize(&mut cx);
        assert!(prompt.approve(true, true).is_err());
        prompt.close(&mut cx);
        let calls = makepad_widgets::splash_host::take_splash_host_requests_for(&[prompt.source.heap_key()]);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].service, "camera.sheet.close");
    }

    #[test]
    fn a_macs_device_alert_names_the_app_and_keeps_another_apps_id() {
        let (title, message) = device_words("Writer", None, "camera");
        assert_eq!(title, "Allow Writer to use your camera?");
        assert_eq!(message, "Writer can use it with any of its accounts. Other apps can't.");
        let (title, message) = device_words("Mail", Some("org.example.mail"), "location");
        assert_eq!(title, "Allow Mail to use your location?");
        assert!(message.starts_with("Mail (org.example.mail) can use it with any of its accounts."));
    }

    #[test]
    fn a_desktop_draws_a_mac_dialog_and_a_phone_keeps_its_page() {
        use crate::octosense::style::{load_sheet, DesktopStyle};
        let mut cx = Cx::new(Box::new(|_, _| {}));
        register();
        for (style, dark, desk) in [
            (DesktopStyle::Macos, false, true),
            (DesktopStyle::Macos, true, true),
            (DesktopStyle::Windows, false, true),
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
                r#"prompt := DevicePermissionPrompt {ticket: "already-expired" family: "camera"}"#,
            );
            let widget = sheet
                .view
                .children
                .iter()
                .find(|(id, _)| *id == id!(prompt))
                .unwrap()
                .1
                .clone();
            let mut prompt = widget.borrow_mut::<DevicePermissionPrompt>().unwrap();
            assert_eq!(prompt.desk, desk);
            prompt.initialize(&mut cx);
            prompt.dim_disabled(&mut cx);
            for id in [
                ids!(title),
                ids!(details),
                ids!(explanation),
                ids!(status),
                ids!(cancel),
                ids!(approve),
            ] {
                assert!(!prompt.view.widget(&mut cx, id).is_empty(), "lost {id:?}");
            }
            let approve = prompt.view.widget(&mut cx, ids!(approve));
            let walk = approve.walk(&mut cx);
            let cancel = prompt.view.widget(&mut cx, ids!(cancel)).walk(&mut cx);
            if style == DesktopStyle::Macos {
                // A Mac's privacy alert: Don't Allow and Allow side by side, each as wide as the other.
                assert!(matches!(walk.height, Size::Fixed(h) if h == 28.0));
                assert!(matches!(walk.width, Size::Fill { .. }) && matches!(cancel.width, Size::Fill { .. }), "the alert's two buttons share its width");
            } else if desk {
                assert!(matches!(walk.height, Size::Fixed(h) if h == 28.0));
                assert!(!matches!(walk.width, Size::Fill { .. }), "never full width");
            } else {
                assert!(matches!(walk.width, Size::Fill { .. }));
            }
            assert_eq!(
                approve.as_button().borrow().unwrap().disabled(&cx),
                desk,
                "an expired request's Continue looks disabled on a desktop; a phone keeps its look"
            );
        }
    }

    #[test]
    fn a_style_family_change_does_not_bring_back_a_live_continue() {
        use crate::connected_review::tests::draw_frame;
        use crate::octosense::style::{load_sheet, DesktopStyle};
        let mut cx = Cx::new(Box::new(|_, _| {}));
        register();
        let mut sheet = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            script_mod(vm);
            let value = vm.eval(script! {use mod.widgets.* Splash {width: Fill height: Fill}});
            Splash::script_from_value(vm, value)
        });
        sheet.set_stylesheet(&mut cx, load_sheet(DesktopStyle::Ios, false));
        sheet.set_text(
            &mut cx,
            r#"prompt := DevicePermissionPrompt {width: Fill height: Fill ticket: "already-expired" family: "camera"}"#,
        );
        for style in [DesktopStyle::Ios, DesktopStyle::Macos] {
            sheet.set_stylesheet(&mut cx, load_sheet(style, false));
            draw_frame(&mut cx, &mut sheet);
            let widget = sheet
                .view
                .children
                .iter()
                .find(|(id, _)| *id == id!(prompt))
                .unwrap()
                .1
                .clone();
            let prompt = widget.borrow::<DevicePermissionPrompt>().unwrap();
            assert_eq!(prompt.desk, style == DesktopStyle::Macos);
            let approve = prompt.view.button(&mut cx, ids!(approve));
            assert!(!approve.borrow().unwrap().enabled(), "{}", style.id());
            assert!(prompt
                .view
                .widget(&mut cx, ids!(status))
                .text()
                .contains("no longer available"));
        }
    }

    #[test]
    fn a_script_copy_cannot_approve_or_close_a_host_permission_sheet() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        register();
        let mut app = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            script_mod(vm);
            let value = vm.eval(script! {use mod.widgets.* Splash {}});
            Splash::script_from_value(vm, value)
        });
        app.set_policy(&mut cx, Some(vec![]), None);
        app.set_text(
            &mut cx,
            r#"prompt := DevicePermissionPrompt {ticket: "forged-native-ticket"}"#,
        );
        let widget = app
            .view
            .children
            .iter()
            .find(|(id, _)| *id == id!(prompt))
            .unwrap()
            .1
            .clone();
        let mut prompt = widget.borrow_mut::<DevicePermissionPrompt>().unwrap();
        prompt.initialize(&mut cx);
        prompt.family = "camera".into();
        assert!(
            prompt.approve(true, true).unwrap_err().contains("physical"),
            "a contained widget cannot borrow native consent even with fabricated input flags"
        );
        prompt.close(&mut cx);
        assert!(
            makepad_widgets::splash_host::take_splash_host_requests_for(&[prompt
                .source
                .heap_key()])
            .is_empty()
        );
        // Remote events do not receive a native provenance lease.
        assert!(!makepad_platform::trusted_user_input());
        assert!(prompt.approve(false, false).is_err());
    }
}
