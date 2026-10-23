//! Bridge an update request to the ordinary module/process close protocol.
use crate::{makepad_widgets::*, App};

impl App {
    pub(crate) fn cancel_update_restart(&mut self) {
        if !self.update_restart_waiting {
            return;
        }
        #[cfg(all(target_os = "macos", any(feature = "app-updater", native_mobile)))]
        octosense_updater_ui::restart::cancel();
        self.update_restart_waiting = false;
        self.quit_waiting_since = None;
        self.module_host.abandon_quit();
        self.process_close.cancel_update_quit();
    }

    pub(crate) fn quit_after_review(&mut self, cx: &mut Cx) {
        #[cfg(all(target_os = "macos", any(feature = "app-updater", native_mobile)))]
        if self.update_restart_waiting {
            if let Err(error) = octosense_updater_ui::restart::commit() {
                self.cancel_update_restart();
                self.notify(cx, "Update restart cancelled", &error);
                return;
            }
            self.update_restart_waiting = false;
        }
        cx.quit();
    }

    pub(crate) fn poll_update_restart(&mut self, cx: &mut Cx) {
        #[cfg(all(target_os = "macos", any(feature = "app-updater", native_mobile)))]
        {
            use octosense_updater_ui::restart;
            // Once per start: a rollback the helper signalled when it reopened
            // this app, or a failed or cancelled attempt from an earlier run.
            if self.state.is_some() {
                if let Some((title, body)) = restart::take_install_notice() {
                    self.notify(cx, &title, &body);
                }
            }
            if self.update_restart_waiting
                && (!restart::pending()
                    || (!self.process_close.quit_waiting()
                        && !self.module_host.close_gate_mut().quit_waiting()))
            {
                self.cancel_update_restart();
            }
            if restart::take_request() {
                // Finish an outstanding process close or shell quit first.
                // Hosted modules may re-answer a previous veto without force.
                if self.quit_waiting_since.is_some() || !self.process_close.idle() {
                    restart::cancel();
                    self.notify(
                        cx,
                        "Update restart cancelled",
                        "Finish the current close request, then try Install & Restart again.",
                    );
                    return;
                }
                self.update_restart_waiting = true;
                if self.ask_before_quit(cx) {
                    self.quit_after_review(cx);
                }
            }
        }
        #[cfg(not(all(target_os = "macos", any(feature = "app-updater", native_mobile))))]
        let _ = cx;
    }
}
