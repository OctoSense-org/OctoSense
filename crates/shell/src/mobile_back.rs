//! Back on the phone: who gets it, and where leaving an app goes.
//!
//! Android's Back key, the floating navigation's Back and Escape all become
//! `PhoneHit::Back` (`mobile_app.rs`). While an in-process app is in front,
//! Back is offered to it first ([`offer_back_to_module`]); only if the app
//! does not take it does the phone leave the app, to the app it was opened
//! from when that is still running ([`leave_target`]), else to Home.
//!
//! [`offer_back_to_module`] is the one seam every module kind goes through,
//! so a new kind of foreground module gets Back by adding a hook there (a
//! host-side check, like the Card runner's sheet) or simply by answering
//! `Event::BackPressed` and setting its `handled` flag, which is how native
//! modules (Settings, and any other linked module) already take it.
use crate::hub::ClientId;
use makepad_widgets::*;

/// An app opened from inside another app (Settings → Accounts → AI
/// providers): leaving it with Back returns to `origin` rather than Home.
/// Held by `PhoneState::return_to` and forgotten on any other navigation,
/// so it only ever describes the app that is in front right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReturnTo {
    pub app: ClientId,
    pub origin: ClientId,
}

/// Where Back leaves `leaving` to: its origin while that is still a live
/// window, otherwise `None`, meaning Home.
pub fn leave_target(return_to: Option<ReturnTo>, leaving: ClientId, alive: impl Fn(ClientId) -> bool) -> Option<ClientId> {
    return_to.filter(|r| r.app == leaving && r.origin != leaving && alive(r.origin)).map(|r| r.origin)
}

/// The foreground module gets Back first. `module` is the module's id
/// (`card` for every App Hub app, system apps included), `root` its
/// instance's root. Returns whether the module took Back; if not, the caller
/// leaves the app.
///
/// In order: the host-side hooks for module kinds that cannot answer Back
/// themselves, then `Event::BackPressed` delivered to the root inside the
/// module's isolate, taken if anything in it set `handled`.
///
/// The caller runs this inside the module's isolate, with its panics
/// contained (`ModuleHost::dispatch`).
pub fn offer_back_to_module(cx: &mut Cx, module: &str, root: &WidgetRef) -> bool {
    let hook = match module {
        "card" => card_sheet_back(cx, root),
        _ => false,
    };
    hook || {
        let event = Event::BackPressed { handled: std::cell::Cell::new(false) };
        root.handle_event(cx, &event, &mut Scope::empty());
        matches!(event, Event::BackPressed { handled } if handled.get())
    }
}

/// The Card runner draws a host service's sheet (a sign-in, the AI
/// providers' add wizard, a provider QR) over the app, as the Splash
/// `sheet` (App Hub `cardapp.rs`). While one is up, Back is the sheet's
/// Cancel: it calls the sheet's `cancel()`, the function every service
/// sheet's Cancel button calls, so the service closes the sheet, forgets
/// what it was doing and answers the app that is waiting. A multi-step
/// sheet (the add-model wizard) is closed too, not stepped back: the
/// runner offers no step-back API yet. A sheet with no `cancel()` is
/// closed as a service closes it.
fn card_sheet_back(cx: &mut Cx, root: &WidgetRef) -> bool {
    let sheet = root.splash(cx, ids!(sheet));
    if !sheet.borrow().is_some_and(|s| s.view.visible) {
        return false;
    }
    if !sheet.call_script_fn(cx, id!(cancel), &[]) {
        sheet.set_text(cx, "");
        if let Some(mut s) = sheet.borrow_mut() {
            s.view.visible = false;
        }
    }
    cx.redraw_all();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaving_returns_to_a_live_origin_and_otherwise_home() {
        let from_settings = Some(ReturnTo { app: 7, origin: 3 });
        assert_eq!(leave_target(from_settings, 7, |_| true), Some(3), "opened from Settings: back to Settings");
        assert_eq!(leave_target(from_settings, 7, |c| c != 3), None, "Settings closed meanwhile: Home");
        assert_eq!(leave_target(from_settings, 9, |_| true), None, "another app leaving: Home");
        assert_eq!(leave_target(None, 7, |_| true), None, "opened from Home: Home");
        assert_eq!(leave_target(Some(ReturnTo { app: 7, origin: 7 }), 7, |_| true), None);
    }

    #[test]
    fn a_return_is_forgotten_on_any_other_navigation() {
        use crate::mobile::{PhoneScreen, PhoneState};
        let opened = |phone: &mut PhoneState| {
            phone.activate(3);
            phone.activate(7);
            phone.return_to = Some(ReturnTo { app: 7, origin: 3 });
        };
        for (what, step) in [
            ("Home", Box::new(|p: &mut PhoneState| p.navigate(PhoneScreen::Home)) as Box<dyn Fn(&mut PhoneState)>),
            ("Recents", Box::new(|p: &mut PhoneState| p.navigate(PhoneScreen::Recents))),
            ("another app", Box::new(|p: &mut PhoneState| p.activate(9))),
        ] {
            let mut phone = PhoneState::default();
            opened(&mut phone);
            step(&mut phone);
            assert_eq!(phone.return_to, None, "{what}");
        }
        let mut phone = PhoneState::default();
        opened(&mut phone);
        phone.activate(7);
        assert!(phone.return_to.is_some(), "the same app again keeps it");
    }

    /// The real Card runner: with no sheet up Back is not the runner's
    /// (the app is left); with a sheet up Back runs the sheet's own cancel,
    /// whose request goes to the service exactly as its Cancel button's does,
    /// and a sheet without one is closed.
    #[cfg(feature = "app-hub")]
    #[test]
    fn a_card_sheet_takes_back_with_its_own_cancel() {
        use makepad_app_module::AppModule;
        use makepad_widgets::splash_host::take_splash_host_requests_for;
        use makepad_widgets::widget_async::{enter_isolate, leave_isolate};
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host = crate::module_host::ModuleHost::default();
        host.apply_style(&mut cx, &desktop_style::StyleSheet::load(desktop_style::DesktopStyle::Android));
        let module = &octosense_app_hub_app::CARD_MODULE;
        let open = module.open_schema().validate(r#"{"app":"os.back-probe"}"#, &[]).unwrap();
        host.create(&mut cx, 11, module, open, dvec2(400.0, 700.0)).unwrap();
        let (root, vm_id) = host.get(11).map(|i| (i.root.clone(), i.vm_id)).unwrap();
        let sheet = |cx: &mut Cx| {
            let entry = enter_isolate(cx, vm_id);
            let sheet = root.splash(cx, ids!(sheet));
            leave_isolate(cx, entry);
            sheet
        };
        let raise = |cx: &mut Cx, body: &str| {
            let s = sheet(cx);
            let entry = enter_isolate(cx, vm_id);
            s.set_text(cx, body);
            s.borrow_mut().unwrap().view.visible = true;
            let heap = s.isolate_heap_key(cx).unwrap();
            leave_isolate(cx, entry);
            heap
        };
        let up = |cx: &mut Cx| sheet(cx).borrow().unwrap().view.visible;
        let back = |cx: &mut Cx| {
            let entry = enter_isolate(cx, vm_id);
            let taken = card_sheet_back(cx, &root);
            leave_isolate(cx, entry);
            taken
        };

        assert!(!host.dispatch(&mut cx, 11, "Back", |cx, root| offer_back_to_module(cx, "card", root)).unwrap(), "no sheet: Back leaves the app");

        // First, while the sheet isolate has never defined one: a program's
        // functions outlive a replacement in the same isolate.
        raise(&mut cx, "View{}");
        assert!(back(&mut cx));
        assert!(!up(&mut cx), "a sheet with no cancel of its own is closed");

        let heap = raise(&mut cx, "fn cancel(){ host.request(\"probe.sheet.cancel\", {}, nil) }\nView{}");
        assert!(host.dispatch(&mut cx, 11, "Back", |cx, root| offer_back_to_module(cx, "card", root)).unwrap());
        let sent: Vec<String> = take_splash_host_requests_for(&[heap]).into_iter().map(|r| r.service).collect();
        assert_eq!(sent, ["probe.sheet.cancel"], "Back is the sheet's Cancel");

        drop(root);
        assert!(host.teardown(&mut cx, 11));
    }
}
