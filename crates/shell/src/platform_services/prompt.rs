//! Consent is collected in a native widget owned by the host sheet. A script
//! cannot approve by calling a sheet method, mounting a copy, or remote clicks.
use super::*;
use makepad_widgets::*;

pub(super) fn register() {
    widget_async::register_splash_isolate_mod(|vm| {
        script_mod(vm);
        script_eval!(vm, {mod.prelude.widgets.DevicePermissionPrompt = mod.widgets.DevicePermissionPrompt});
    });
}
script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    mod.widgets.DevicePermissionPromptBase = #(DevicePermissionPrompt::register_widget(vm))
    mod.widgets.DevicePermissionPrompt = set_type_default() do mod.widgets.DevicePermissionPromptBase {
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
#[derive(Script, ScriptHook, Widget)]
pub struct DevicePermissionPrompt {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    ticket: String,
    #[rust]
    family: String,
    #[rust]
    initialized: bool,
    #[rust]
    trusted_down: bool,
}
fn can_approve(contained: bool, down: bool, up: bool) -> bool {
    !contained && down && up
}
impl DevicePermissionPrompt {
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
            self.view.label(cx, ids!(details)).set_text(
                cx,
                &format!(
                    "Allow {} to use {}?\n\nDevice access applies to this app across its accounts.",
                    review.work.call.app_id, self.family
                ),
            );
        } else {
            self.view.label(cx, ids!(status)).set_text(
                cx,
                "This permission request is no longer available. Return to the app.",
            );
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
        if self.view.button(cx, ids!(cancel)).clicked(&actions) {
            self.close(cx);
        }
        if approve.clicked(&actions) {
            let down = std::mem::take(&mut self.trusted_down);
            match self.approve(down, makepad_platform::trusted_user_input()) {
                Ok(()) => {
                    approve.set_enabled(cx, false);
                    self.close(cx);
                }
                Err(error) => self.view.label(cx, ids!(status)).set_text(cx, &error),
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
