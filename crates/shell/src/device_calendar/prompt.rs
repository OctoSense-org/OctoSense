//! Host-owned immutable calendar review. No script method approves it.
use super::*;
use makepad_widgets::*;
pub(super) fn register() {
    widget_async::register_splash_isolate_mod(|vm| {
        script_mod(vm);
        script_eval!(vm,{mod.prelude.widgets.DeviceCalendarReview=mod.widgets.DeviceCalendarReview});
    });
}
script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    mod.widgets.DeviceCalendarReviewBase=#(DeviceCalendarReview::register_widget(vm))
    mod.widgets.DeviceCalendarReview=set_type_default() do mod.widgets.DeviceCalendarReviewBase{
        width:Fill height:Fill flow:Down padding:20 spacing:14 show_bg:true
        draw_bg +: {color:instance(#fff) pixel:fn(){return Pal.premul(self.color)}}
        Label{width:Fill height:Fit text:"Review device calendar" draw_text.color:#x172336 draw_text.text_style.font_size:22}
        ScrollYView{width:Fill height:Fill
            details:=Label{width:Fill height:Fit text:"Loading calendar details…" draw_text.color:#x172336 draw_text.wrap:Words draw_text.text_style.font_size:16}
        }
        status:=Label{width:Fill height:Fit text:"No changes are made until you approve." draw_text.color:#x526071 draw_text.wrap:Words}
        View{width:Fill height:48 spacing:12
            cancel:=ButtonFlat{width:Fill height:Fill text:"Close"}
            approve:=Button{width:Fill height:Fill text:"Approve" enabled:false}
        }
    }
}
#[derive(Script, ScriptHook, Widget)]
pub struct DeviceCalendarReview {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    ticket: String,
    #[rust]
    trusted_down: bool,
    #[rust]
    last_text: String,
}
fn event_text(event: &EventData) -> String {
    let times = if event.all_day {
        format!(
            "All day: {} through {} (end date exclusive)",
            civil_time(event.start_ms, &event.timezone, true),
            civil_time(event.end_ms, &event.timezone, true)
        )
    } else {
        format!(
            "Start: {}\nEnd: {}\nTimezone: {}",
            civil_time(event.start_ms, &event.timezone, false),
            civil_time(event.end_ms, &event.timezone, false),
            event.timezone
        )
    };
    format!(
        "{}\n{}\nLocation: {}\n\n{}",
        event.title, times, event.location, event.notes
    )
}
fn civil_time(ms: i64, zone: &str, date_only: bool) -> String {
    let Some(utc) = chrono::DateTime::from_timestamp_millis(ms) else {
        return "Invalid date — reload the event".into();
    };
    let Ok(zone) = zone.parse::<chrono_tz::Tz>() else {
        // Existing OS events may contain a provider-specific name. Never
        // silently display that event in an unrelated timezone.
        return format!(
            "{} UTC (unknown timezone; review in Calendar)",
            utc.format("%Y-%m-%d %H:%M:%S")
        );
    };
    let local = utc.with_timezone(&zone);
    local
        .format(if date_only {
            "%Y-%m-%d"
        } else {
            "%Y-%m-%d %H:%M:%S %Z (UTC%:z)"
        })
        .to_string()
}
fn details(review: &Review) -> (String, bool) {
    let owner = format!("App: {}\n", review.context.call.app_id);
    match &review.phase {
        Phase::Loading => (format!("{owner}Loading calendar/account details…"), false),
        Phase::Ready(p) => {
            let body=match p{
                Prepared::Permission=>"Allow this app account to see calendar choices and read calendars you select? Each create, edit or delete still requires a separate native review. The operating system may ask separately.".into(),
                Prepared::Select(c)=>format!("Select calendar: {}\nAccount: {}\nWritable: {}\n\nOnly this app's current account receives the returned handle.",c.name,c.account_name,c.writable),
                Prepared::Save{calendar,event,previous}=>format!("{}\nCalendar: {}\nAccount: {}\n\n{}{}",if previous.is_some(){"Update event"}else{"Create event"},calendar.name,calendar.account_name,
                    previous.as_ref().map(|old|format!("CURRENT\n{}\n\nREPLACEMENT\n",event_text(old))).unwrap_or_default(),event_text(event)),
                Prepared::Delete{calendar,previous}=>format!("DELETE EVENT\nCalendar: {}\nAccount: {}\n\n{}",calendar.name,calendar.account_name,event_text(previous)),
            };
            (format!("{owner}{body}"), true)
        }
        Phase::Submitted => (
            format!("{owner}Approved operation is completing. Do not submit it again."),
            false,
        ),
        Phase::Finished(Ok(_)) => (
            format!("{owner}Completed. You can close this review."),
            false,
        ),
        Phase::Finished(Err(error)) => (format!("{owner}{error}"), false),
    }
}
impl DeviceCalendarReview {
    fn refresh(&mut self, cx: &mut Cx) {
        let (text, enabled) = if splash_policy::is_enforced(self.source.heap_key()) {
            (
                "Only the host's native calendar sheet can review this request.".into(),
                false,
            )
        } else {
            state()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .reviews
                .get(&self.ticket)
                .map(details)
                .unwrap_or((
                    "This calendar review expired. Return to the app.".into(),
                    false,
                ))
        };
        if text != self.last_text {
            self.view.label(cx, ids!(details)).set_text(cx, &text);
            self.last_text = text;
        }
        self.view.button(cx, ids!(approve)).set_enabled(cx, enabled);
    }
    fn close(&self, cx: &mut Cx) {
        if splash_policy::is_enforced(self.source.heap_key()) {
            return;
        }
        if let Some(owner) = cx.script_ref_vm_id(&self.source) {
            cx.with_script_vm_id(owner, |vm| {
                vm.eval(ScriptMod {
                    cargo_manifest_path: env!("CARGO_MANIFEST_DIR").into(),
                    module_path: "device_calendar_close".into(),
                    file: "device_calendar_close.splash".into(),
                    line: 0,
                    column: 0,
                    code: format!(
                        "mod.host.request(\"device_calendar.sheet.close\", {{ticket:{}}}, nil)",
                        json!(self.ticket)
                    ),
                    values: vec![],
                });
            });
        }
    }
}
impl Widget for DeviceCalendarReview {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.refresh(cx);
        self.view.draw_walk(cx, scope, walk)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if matches!(
            event,
            Event::Pause | Event::Background | Event::WindowLostFocus(_)
        ) {
            self.trusted_down = false;
        }
        self.refresh(cx);
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        let button = self.view.button(cx, ids!(approve));
        if button.pressed(&actions) {
            self.trusted_down = makepad_platform::trusted_user_input();
        }
        if self.view.button(cx, ids!(cancel)).clicked(&actions) {
            self.close(cx);
        }
        if button.clicked(&actions) {
            let down = std::mem::take(&mut self.trusted_down);
            if let Err(error) = approve(
                &self.ticket,
                splash_policy::is_enforced(self.source.heap_key()),
                down,
                makepad_platform::trusted_user_input(),
            ) {
                self.view.label(cx, ids!(status)).set_text(cx, &error);
            }
            self.refresh(cx);
        } else if button.released(&actions) {
            self.trusted_down = false;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_review_time_tracks_dst_and_keeps_zone() {
        use chrono::DateTime;
        let epoch = |text: &str| {
            DateTime::parse_from_rfc3339(text)
                .unwrap()
                .timestamp_millis()
        };
        assert_eq!(
            civil_time(epoch("2026-10-06T16:00:00Z"), "America/Los_Angeles", false),
            "2026-10-06 09:00:00 PDT (UTC-07:00)"
        );
        assert_eq!(
            civil_time(epoch("2026-11-01T08:30:00Z"), "America/Los_Angeles", false),
            "2026-11-01 01:30:00 PDT (UTC-07:00)"
        );
        assert_eq!(
            civil_time(epoch("2026-11-01T09:30:00Z"), "America/Los_Angeles", false),
            "2026-11-01 01:30:00 PST (UTC-08:00)"
        );
        let event = model::event(&json!({"title":"Synthetic day","start_ms":86400000,"end_ms":172800000,"timezone":"UTC","all_day":true})).unwrap();
        assert!(event_text(&event).contains("1970-01-02 through 1970-01-03 (end date exclusive)"));
    }
    #[test]
    fn automation_cannot_approve_even_a_guessed_ticket() {
        assert!(approve("missing", false, false, true)
            .unwrap_err()
            .starts_with("trusted_input_required"));
        assert!(approve("missing", true, true, true)
            .unwrap_err()
            .starts_with("trusted_input_required"));
    }
}
