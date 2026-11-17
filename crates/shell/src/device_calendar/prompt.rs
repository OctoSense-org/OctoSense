//! Host-owned immutable calendar review. No script method approves it.
use super::*;
use crate::octosense::style::{AppIconDraw, DesktopStyle};
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
    // A phone keeps this sheet's fixed look; a desktop's is a Mac dialog that
    // follows the appearance (`border_radius` is half the corner it draws).
    let desk = try { theme.ui_pointer == true } catch { false }
    // The Mac style: attached under the window's top band over a light dim, with
    // labelled rows and the event in plain words. Other desktop styles keep theirs.
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
    mod.widgets.DeviceCalendarIcon=set_type_default() do #(DeviceCalendarIcon::register_widget(vm)){width:48 height:48}
    mod.widgets.DeviceCalendarReviewBase=#(DeviceCalendarReview::register_widget(vm))
    // A Mac's push buttons: the plain one, and the blue default.
    let MacPlain=ButtonFlat{height:28 margin:0 padding:Inset{left:12 right:12 bottom:2}
        draw_bg +: {border_radius:3.0 border_size:1.0 color:sh_control color_hover:tone(#xf2f2f4, #x545458) color_down:tone(#xe5e5ea, #x5e5e62) color_focus:sh_control color_disabled:sh_off
            border_color:sh_control_line border_color_hover:sh_control_line border_color_down:sh_control_line border_color_focus:sh_control_line border_color_disabled:sh_control_line}
        draw_text +: {color:sh_ink color_hover:sh_ink color_down:sh_ink color_focus:sh_ink color_disabled:sh_off_ink text_style:theme.font_regular{font_size:9.75}}
    }
    let MacBlue=ButtonFlat{height:28 margin:0 padding:Inset{left:16 right:16 bottom:2}
        draw_bg +: {border_radius:3.0 border_size:0.0 color:#x0071e3 color_hover:#x006edb color_down:#x0060c0 color_focus:#x0071e3 color_disabled:sh_off}
        draw_text +: {color:#xffffff color_hover:#xffffff color_down:#xffffff color_focus:#xffffff color_disabled:sh_off_ink text_style:theme.font_regular{font_size:9.75}}
    }
    let MacFact=View{width:Fill height:Fit flow:Right spacing:8 visible:false
        k:=Label{width:96 height:Fit padding:0 align:Align{x:1.0} draw_text.color:tone(#x636368, #xa8a8ad) draw_text.text_style.font_size:9.75}
        v:=Label{width:Fill height:Fit padding:0 draw_text.color:sh_ink draw_text.wrap:Words draw_text.text_style.font_size:9.75}
    }
    mod.widgets.DeviceCalendarReview=if desk_mac {
        set_type_default() do mod.widgets.DeviceCalendarReviewBase{
            desk:true
            mac:true
            width:Fill height:Fill flow:Down new_batch:true show_bg:true
            draw_bg +: {color:instance(tone(#x00000024, #x00000052)) pixel:fn(){return Pal.premul(self.color)}}
            ScrollYView{width:Fill height:Fill flow:Down align:Align{x:0.5} padding:Inset{left:12 right:12 top:52 bottom:24} container_id:@review_area
                review_card:=RoundedShadowView{width:Fill max_width:520 height:Fit flow:Down spacing:8 padding:20 new_batch:true show_bg:true
                    draw_bg +: {color:tone(#xf6f6f8, #x2c2c2e) border_radius:5.0 border_size:1.0 border_color:sh_line shadow_color:tone(#x00000040, #x00000080) shadow_radius:18.0 shadow_offset:vec2(0.0, 6.0)}
                    head:=View{width:Fill height:Fit flow:Right spacing:14
                        icon_box:=View{width:Fit height:Fit visible:false
                            app_icon:=mod.widgets.DeviceCalendarIcon{}
                        }
                        View{width:Fill height:Fit flow:Down spacing:8
                            title:=Label{width:Fill height:Fit padding:0 text:"Review Calendar Access" draw_text.color:sh_ink draw_text.wrap:Words draw_text.text_style:theme.font_bold{font_size:11.25}}
                            sentence:=Label{width:Fill height:Fit padding:0 draw_text.color:sh_ink draw_text.wrap:Words draw_text.text_style.font_size:9.75}
                        }
                    }
                    // Whether the app may see your calendars at all is a Mac's privacy alert, as the
                    // camera's is (`style_alert`): the app's icon over the question, the words centred.
                    // A Label's margin counts twice: 10 points over the question, 4 over the words.
                    alert_head:=View{width:Fill height:Fit flow:Down align:Align{x:0.5} visible:false
                        alert_icon:=mod.widgets.DeviceCalendarIcon{width:64 height:64}
                        alert_title:=Label{width:Fill height:Fit padding:0 margin:Inset{top:5} clip_y:false align:Align{x:0.5} draw_text +: {color:sh_ink wrap:Words text_style:theme.font_bold{font_size:9.75}}}
                        alert_sentence:=Label{width:Fill height:Fit padding:0 margin:Inset{top:2} clip_y:false align:Align{x:0.5} draw_text +: {color:sh_muted wrap:Words text_style:theme.font_regular{font_size:8.25}}}
                    }
                    // What the other trees show in one block is in the rows here.
                    details:=Label{width:Fill height:0 padding:0 visible:false}
                    facts_box:=RoundedView{width:Fill height:Fit flow:Down show_bg:true margin:Inset{top:4 bottom:4}
                        draw_bg +: {color:tone(#xffffff, #x3a3a3c) border_radius:3.0 border_size:1.0 border_color:sh_line}
                        // Long details scroll here: what is left of the window keeps the actions in view.
                        ScrollYView{width:Fill height:Fit max_height:"max(120px, 100cqh - 250px)" flow:Down spacing:4 padding:10
                            fact0:=MacFact{}
                            fact1:=MacFact{}
                            fact2:=MacFact{}
                            fact3:=MacFact{}
                            fact4:=MacFact{}
                            fact5:=MacFact{}
                            fact6:=MacFact{}
                            fact7:=MacFact{}
                            fact8:=MacFact{}
                            fact9:=MacFact{}
                            fact10:=MacFact{}
                            fact11:=MacFact{}
                            fact12:=MacFact{}
                            fact13:=MacFact{}
                        }
                    }
                    // The note sits with the words while they stand alone, and with the rows once there are rows.
                    status_row:=View{width:Fill height:Fit flow:Right
                        status_pad:=View{width:62 height:0 visible:false}
                        status:=Label{width:Fill height:Fit padding:0 text:"No changes are made until you approve." draw_text.color:sh_muted draw_text.wrap:Words draw_text.text_style.font_size:8.25}
                    }
                    // Two buttons at a time: the default rightmost. Where what this asks
                    // undoes something, Cancel is the default (`careful`) and what it
                    // asks keeps a red title, as a Mac's destructive button does.
                    View{width:Fill height:Fit flow:Right spacing:8 align:Align{x:1.0 y:0.5} margin:Inset{top:6}
                        approve_plain:=MacPlain{visible:false text:"Approve" enabled:false draw_text +: {color:sh_red color_hover:sh_red color_down:sh_red color_focus:sh_red}}
                        cancel:=MacPlain{text:"Cancel"}
                        cancel_blue:=MacBlue{visible:false text:"Cancel"}
                        approve:=MacBlue{text:"Approve" enabled:false}
                    }
                }
            }
        }
    } else if desk {
        set_type_default() do mod.widgets.DeviceCalendarReviewBase{
            desk:true
            width:Fill height:Fill flow:Down new_batch:true show_bg:true
            draw_bg +: {color:instance(tone(#x00000040, #x00000066)) pixel:fn(){return Pal.premul(self.color)}}
            ScrollYView{width:Fill height:Fill flow:Down align:Align{x:0.5} padding:Inset{left:12 right:12 top:40 bottom:24} container_id:@review_area
                RoundedView{width:Fill max_width:480 height:Fit flow:Down spacing:8 padding:18 new_batch:true show_bg:true
                    draw_bg +: {color:tone(#xf6f6f8, #x2c2c2e) border_radius:5.0 border_size:1.0 border_color:sh_line}
                    Label{width:Fill height:Fit padding:0 text:"Review device calendar" draw_text.color:sh_ink draw_text.wrap:Words draw_text.text_style:theme.font_bold{font_size:12.75}}
                    RoundedView{width:Fill height:Fit flow:Down show_bg:true
                        draw_bg +: {color:tone(#xffffff, #x3a3a3c) border_radius:3.0 border_size:1.0 border_color:sh_line}
                        // Long details scroll here: what is left of the window keeps the actions in view.
                        ScrollYView{width:Fill height:Fit max_height:"max(120px, 100cqh - 190px)" flow:Down padding:10
                            details:=Label{width:Fill height:Fit padding:0 text:"Loading calendar details…" draw_text.color:sh_ink draw_text.wrap:Words draw_text.text_style.font_size:9.75}
                        }
                    }
                    status:=Label{width:Fill height:Fit padding:0 text:"No changes are made until you approve." draw_text.color:sh_muted draw_text.wrap:Words draw_text.text_style.font_size:9}
                    View{width:Fill height:Fit flow:Right spacing:8 align:Align{x:1.0 y:0.5} margin:Inset{top:6}
                        cancel:=ButtonFlat{height:28 margin:0 padding:Inset{left:12 right:12} text:"Close"
                            draw_bg +: {border_radius:3.0 border_size:1.0 color:sh_control color_hover:tone(#xf2f2f4, #x545458) color_down:tone(#xe5e5ea, #x5e5e62) color_focus:sh_control
                                border_color:sh_control_line border_color_hover:sh_control_line border_color_down:sh_control_line border_color_focus:sh_control_line}
                            draw_text +: {color:sh_ink color_hover:sh_ink color_down:sh_ink color_focus:sh_ink text_style:theme.font_regular{font_size:9.75}}
                        }
                        approve:=ButtonFlat{height:28 margin:0 padding:Inset{left:16 right:16} text:"Approve" enabled:false
                            draw_bg +: {border_radius:3.0 border_size:0.0 color:#x0071e3 color_hover:#x006edb color_down:#x0060c0 color_focus:#x0071e3 color_disabled:sh_off}
                            draw_text +: {color:#xffffff color_hover:#xffffff color_down:#xffffff color_focus:#xffffff color_disabled:sh_off_ink text_style:theme.font_regular{font_size:9.75}}
                        }
                    }
                }
            }
        }
    } else {
        set_type_default() do mod.widgets.DeviceCalendarReviewBase{
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
}
#[derive(Script, ScriptHook, Widget)]
pub struct DeviceCalendarReview {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    ticket: String,
    /// The desktop tree: Approve looks disabled while it cannot be pressed.
    #[live]
    desk: bool,
    #[rust]
    trusted_down: bool,
    #[rust]
    last_text: String,
    /// The label `last_text` is in.
    #[rust]
    filled: Option<WidgetUid>,
    /// The Mac's tree: a title, a sentence and labelled rows.
    #[live]
    mac: bool,
    #[rust]
    last_mac: Option<MacCalendar>,
    /// A Mac's review of what undoes something: the other pair of buttons is up.
    #[rust]
    careful: bool,
    /// The look the Mac's tree wears: a privacy alert (true) or a sheet; none yet.
    #[rust]
    alert_look: Option<bool>,
}
/// The asking app's icon on a Mac's sheet. It is drawn in its place in the tree:
/// the card is a draw list of its own, lifted over anything drawn around it.
#[derive(Script, ScriptHook, Widget)]
pub struct DeviceCalendarIcon {
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
impl Widget for DeviceCalendarIcon {
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
/// A review in a Mac's words: the same facts as [`details`], each under its own
/// label, the app by its name.
#[derive(Default, PartialEq)]
struct MacCalendar {
    /// Who asks: its icon stands beside the words.
    app: String,
    title: String,
    sentence: String,
    rows: Vec<(String, String)>,
    /// What Approve does, when it can be pressed.
    approve: &'static str,
    /// The way out where it is a refusal; otherwise Cancel backs out.
    refuse: &'static str,
    /// Nothing is left to decide: the way out is Close, not Cancel.
    over: bool,
    /// It undoes something: Cancel is the default, not what it asks.
    careful: bool,
    /// A privacy alert (whether the app may see the calendars at all) or a sheet;
    /// `None` keeps the look it has, as what follows an answer does.
    alert: Option<bool>,
}
const FACT_ROWS: usize = 14;
fn fact_row(i: usize) -> (&'static [LiveId], &'static [LiveId], &'static [LiveId]) {
    match i {
        0 => (ids!(fact0), ids!(fact0.k), ids!(fact0.v)),
        1 => (ids!(fact1), ids!(fact1.k), ids!(fact1.v)),
        2 => (ids!(fact2), ids!(fact2.k), ids!(fact2.v)),
        3 => (ids!(fact3), ids!(fact3.k), ids!(fact3.v)),
        4 => (ids!(fact4), ids!(fact4.k), ids!(fact4.v)),
        5 => (ids!(fact5), ids!(fact5.k), ids!(fact5.v)),
        6 => (ids!(fact6), ids!(fact6.k), ids!(fact6.v)),
        7 => (ids!(fact7), ids!(fact7.k), ids!(fact7.v)),
        8 => (ids!(fact8), ids!(fact8.k), ids!(fact8.v)),
        9 => (ids!(fact9), ids!(fact9.k), ids!(fact9.v)),
        10 => (ids!(fact10), ids!(fact10.k), ids!(fact10.v)),
        11 => (ids!(fact11), ids!(fact11.k), ids!(fact11.v)),
        12 => (ids!(fact12), ids!(fact12.k), ids!(fact12.v)),
        _ => (ids!(fact13), ids!(fact13.k), ids!(fact13.v)),
    }
}
/// "Nov 2, 2026 at 3:45 PM PST". Never silently in an unrelated time zone.
fn mac_time(ms: i64, zone: &str) -> String {
    let Some(utc) = chrono::DateTime::from_timestamp_millis(ms) else {
        return "Invalid date. Reload the event.".into();
    };
    let shown = |seconds: bool| if seconds { "%b %-d, %Y at %-I:%M:%S %p" } else { "%b %-d, %Y at %-I:%M %p" };
    use chrono::Timelike;
    let Ok(tz) = zone.parse::<chrono_tz::Tz>() else {
        return format!(
            "{} UTC (unknown time zone; check it in Calendar)",
            utc.format(shown(utc.second() != 0))
        );
    };
    let local = utc.with_timezone(&tz);
    format!("{} {}", local.format(shown(local.second() != 0)), local.format("%Z"))
}
/// An all-day event's days: its end date is exclusive, so the last day is the one before.
fn mac_days(event: &EventData) -> Option<String> {
    let tz = event.timezone.parse::<chrono_tz::Tz>().ok()?;
    let day = |ms: i64| chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&tz).date_naive());
    let (first, after) = (day(event.start_ms)?, day(event.end_ms)?);
    let shown = |d: chrono::NaiveDate| d.format("%b %-d, %Y").to_string();
    Some(match after.pred_opt().filter(|last| *last > first) {
        Some(last) => format!("All day, {} through {}", shown(first), shown(last)),
        None => format!("All day, {}", shown(first)),
    })
}
fn mac_event(rows: &mut Vec<(String, String)>, head: &str, event: &EventData) {
    let mut row = |key: &str, value: String| rows.push((key.to_string(), value));
    row(head, event.title.clone());
    if event.all_day {
        let days = mac_days(event).unwrap_or_else(|| {
            format!(
                "All day: {} through {} (end date exclusive)",
                civil_time(event.start_ms, &event.timezone, true),
                civil_time(event.end_ms, &event.timezone, true)
            )
        });
        row("When", days);
    } else {
        row("Starts", mac_time(event.start_ms, &event.timezone));
        row("Ends", mac_time(event.end_ms, &event.timezone));
        // The place the zone is named for: its id says it with a region and underscores.
        row("Time Zone", event.timezone.rsplit('/').next().unwrap_or(&event.timezone).replace('_', " "));
    }
    if !event.location.is_empty() {
        row("Location", event.location.clone());
    }
    if !event.notes.is_empty() {
        row("Notes", event.notes.clone());
    }
}
fn mac_details(review: &Review) -> MacCalendar {
    mac_words(&review.context.call.app_id, &review.phase)
}

fn mac_words(app: &str, phase: &Phase) -> MacCalendar {
    let (name, id) = crate::connected_review::shown_app(app);
    let who = match id {
        Some(id) => format!("{name} ({id})"),
        None => name.clone(),
    };
    let wants = |what: &str| format!("{who} wants to {what}.");
    let place = |calendar: &Calendar| {
        vec![
            ("Calendar".to_string(), calendar.name.clone()),
            ("Account".to_string(), calendar.account_name.clone()),
        ]
    };
    match phase {
        Phase::Loading => MacCalendar {
            title: format!("{name} Wants to Use Your Calendar"),
            sentence: "Loading your calendar and account details…".into(),
            ..Default::default()
        },
        Phase::Ready(Prepared::Permission) => MacCalendar {
            title: format!("Allow {name} to see your calendars?"),
            sentence: format!("{who}, on its current account, can see your list of calendars and read the ones you choose. It must ask again each time it adds, changes or deletes an event. Your computer may ask you too."),
            approve: "Allow",
            refuse: "Don't Allow",
            alert: Some(true),
            ..Default::default()
        },
        Phase::Ready(Prepared::Select(calendar)) => {
            let mut rows = place(calendar);
            rows.push(("Can Change It".to_string(), if calendar.writable { "Yes" } else { "No" }.to_string()));
            MacCalendar {
                title: format!("Let {name} use this calendar?"),
                sentence: format!("Only {who}'s current account gets access to it."),
                rows,
                approve: "Allow",
                refuse: "Don't Allow",
                alert: Some(false),
                ..Default::default()
            }
        }
        Phase::Ready(Prepared::Save { calendar, event, previous }) => {
            let mut rows = place(calendar);
            match previous {
                Some(old) => {
                    mac_event(&mut rows, "Current Event", old);
                    mac_event(&mut rows, "New Event", event);
                }
                None => mac_event(&mut rows, "Event", event),
            }
            let changing = previous.is_some();
            MacCalendar {
                title: if changing { "Change this event?" } else { "Add this event to your calendar?" }.into(),
                sentence: wants(if changing { "change this event" } else { "add this event" }),
                rows,
                approve: if changing { "Change Event" } else { "Add Event" },
                alert: Some(false),
                ..Default::default()
            }
        }
        Phase::Ready(Prepared::Delete { calendar, previous }) => {
            let mut rows = place(calendar);
            mac_event(&mut rows, "Event", previous);
            MacCalendar {
                title: "Delete this event?".into(),
                sentence: wants("delete this event"),
                rows,
                approve: "Delete Event",
                careful: true,
                alert: Some(false),
                ..Default::default()
            }
        }
        Phase::Submitted => MacCalendar {
            title: "Finishing the Change".into(),
            sentence: "Finishing what you approved. There's no need to do it again.".into(),
            ..Default::default()
        },
        Phase::Finished(Ok(_)) => MacCalendar {
            title: "Done".into(),
            sentence: "The change is made. You can close this.".into(),
            over: true,
            ..Default::default()
        },
        Phase::Finished(Err(error)) => MacCalendar {
            title: "Not Done".into(),
            sentence: error.clone(),
            over: true,
            ..Default::default()
        },
    }
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
        // A phone's style giving way to a desktop's builds the other tree, whose
        // label starts over: a review must never show the wrong words with Approve live.
        let label = self.view.widget(cx, ids!(details)).widget_uid();
        if self.filled != Some(label) {
            self.filled = Some(label);
            self.last_text.clear();
            self.last_mac = None;
            self.alert_look = None;
        }
        if self.mac {
            self.refresh_mac(cx, enabled);
            // An alert's note is only ever what went wrong.
            if self.alert_look == Some(true) {
                let said = !self.view.label(cx, ids!(status)).text().is_empty();
                self.view.view(cx, ids!(status_row)).set_visible(cx, said);
            }
        }
        if text != self.last_text {
            self.view.label(cx, ids!(details)).set_text(cx, &text);
            self.last_text = text;
        }
        self.view.button(cx, ids!(approve)).set_enabled(cx, enabled);
        self.view.button(cx, ids!(approve_plain)).set_enabled(cx, enabled);
        // `set_enabled` alone leaves a button's look; a phone keeps that.
        if self.desk {
            for id in [ids!(approve), ids!(approve_plain)] {
                if let Some(mut approve) = self.view.button(cx, id).borrow_mut() {
                    if approve.disabled(cx) == enabled {
                        approve.set_disabled(cx, !enabled);
                    }
                }
            }
        }
    }
    /// The Mac's rows, title and buttons, when they change.
    fn refresh_mac(&mut self, cx: &mut Cx, enabled: bool) {
        let words = if splash_policy::is_enforced(self.source.heap_key()) {
            MacCalendar {
                title: "Review Calendar Access".into(),
                sentence: "Only OctoSense's own calendar sheet can review this request.".into(),
                over: true,
                ..Default::default()
            }
        } else {
            state()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .reviews
                .get(&self.ticket)
                .map(|review| MacCalendar { app: review.context.call.app_id.clone(), ..mac_details(review) })
                .unwrap_or_else(|| MacCalendar {
                    title: "Review Calendar Access".into(),
                    sentence: "This request has expired. Return to the app.".into(),
                    over: true,
                    ..Default::default()
                })
        };
        if self.last_mac.as_ref() == Some(&words) {
            return;
        }
        if let Some(alert) = words.alert.filter(|&alert| self.alert_look != Some(alert)) {
            self.alert_look = Some(alert);
            self.style_alert(cx, alert);
        }
        let alert = self.alert_look == Some(true);
        for path in [ids!(app_icon), ids!(alert_icon)] {
            if let Some(mut icon) = self.view.widget(cx, path).borrow_mut::<DeviceCalendarIcon>() {
                icon.app = words.app.clone();
            }
        }
        self.view.view(cx, ids!(icon_box)).set_visible(cx, !words.app.is_empty());
        self.view.label(cx, ids!(title)).set_text(cx, &words.title);
        self.view.label(cx, ids!(sentence)).set_text(cx, &words.sentence);
        self.view.label(cx, ids!(alert_title)).set_text(cx, &words.title);
        self.view.label(cx, ids!(alert_sentence)).set_text(cx, &words.sentence);
        // Past the rows there are, the rest joins the last one: nothing is left out.
        let mut rows = words.rows.clone();
        if rows.len() > FACT_ROWS {
            let rest: Vec<String> = rows.split_off(FACT_ROWS - 1).into_iter().map(|(k, v)| format!("{k}: {v}")).collect();
            rows.push(("More".to_string(), rest.join("\n")));
        }
        for i in 0..FACT_ROWS {
            let (row, key, value) = fact_row(i);
            let fact = rows.get(i);
            self.view.view(cx, row).set_visible(cx, fact.is_some());
            self.view.label(cx, key).set_text(cx, fact.map_or("", |(k, _)| k.as_str()));
            self.view.label(cx, value).set_text(cx, fact.map_or("", |(_, v)| v.as_str()));
        }
        self.view.view(cx, ids!(facts_box)).set_visible(cx, !rows.is_empty());
        self.view.view(cx, ids!(status_pad)).set_visible(cx, rows.is_empty() && !words.app.is_empty() && !alert);
        self.careful = words.careful;
        let out = if words.over { "Close" } else if words.refuse.is_empty() { "Cancel" } else { words.refuse };
        self.view.button(cx, ids!(cancel)).set_text(cx, out);
        self.view.widget(cx, ids!(cancel)).set_visible(cx, !words.careful);
        self.view.widget(cx, ids!(cancel_blue)).set_visible(cx, words.careful);
        let approve = if words.approve.is_empty() { "Approve" } else { words.approve };
        self.view.button(cx, ids!(approve)).set_text(cx, approve);
        self.view.button(cx, ids!(approve_plain)).set_text(cx, approve);
        self.view.widget(cx, ids!(approve)).set_visible(cx, !words.over && !words.careful);
        self.view.widget(cx, ids!(approve_plain)).set_visible(cx, !words.over && words.careful);
        let status = if enabled && !alert { "Nothing changes until you choose." } else { "" };
        self.view.label(cx, ids!(status)).set_text(cx, status);
        self.last_mac = Some(words);
    }
    /// A Mac's privacy alert is 300 points wide: the app's icon over the question, the
    /// words centred, Don't Allow and Allow side by side under them, each as wide as the
    /// other, as the camera's and an assistant's consent are. The reviews with rows are
    /// sheets, their buttons at the right. The buttons are the same ones either way.
    fn style_alert(&mut self, cx: &mut Cx, alert: bool) {
        self.view.view(cx, ids!(head)).set_visible(cx, !alert);
        self.view.view(cx, ids!(alert_head)).set_visible(cx, alert);
        self.view.view(cx, ids!(status_row)).set_visible(cx, true);
        let mut card = self.view.widget(cx, ids!(review_card));
        let mut status = self.view.widget(cx, ids!(status));
        if alert {
            script_apply_eval!(cx, card, {use mod.prelude.widgets.* width: 300 padding: Inset{left: 16 right: 16 top: 16 bottom: 16}});
            script_apply_eval!(cx, status, {use mod.prelude.widgets.* align: Align{x: 0.5 y: 0.0}});
        } else {
            script_apply_eval!(cx, card, {use mod.prelude.widgets.* width: Fill padding: Inset{left: 20 right: 20 top: 20 bottom: 20}});
            script_apply_eval!(cx, status, {use mod.prelude.widgets.* align: Align{x: 0.0 y: 0.0}});
        }
        for path in [ids!(cancel), ids!(cancel_blue), ids!(approve), ids!(approve_plain)] {
            let mut button = self.view.widget(cx, path);
            if alert {
                script_apply_eval!(cx, button, {use mod.prelude.widgets.* width: Fill});
            } else {
                script_apply_eval!(cx, button, {use mod.prelude.widgets.* width: Fit});
            }
        }
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
        let button = self.view.button(cx, if self.careful { ids!(approve_plain) } else { ids!(approve) });
        if button.pressed(&actions) {
            self.trusted_down = makepad_platform::trusted_user_input();
        }
        // Escape is Cancel's on a Mac, and so is Return where Cancel is the default.
        let by_key = self.mac
            && matches!(event, Event::KeyDown(e) if e.key_code == KeyCode::Escape
                || (self.careful && matches!(e.key_code, KeyCode::ReturnKey | KeyCode::NumpadEnter)));
        let cancel = self.view.button(cx, if self.careful { ids!(cancel_blue) } else { ids!(cancel) });
        if cancel.clicked(&actions) || by_key {
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
    fn a_mac_reads_times_as_dates_and_keeps_the_zone() {
        use chrono::DateTime;
        let epoch = |text: &str| DateTime::parse_from_rfc3339(text).unwrap().timestamp_millis();
        assert_eq!(mac_time(epoch("2026-11-02T23:45:00Z"), "America/Los_Angeles"), "Nov 2, 2026 at 3:45 PM PST");
        // The instant under review: a stray second is not rounded away.
        assert_eq!(mac_time(epoch("2026-11-02T23:45:07Z"), "America/Los_Angeles"), "Nov 2, 2026 at 3:45:07 PM PST");
        assert_eq!(
            mac_time(epoch("2026-11-02T23:45:00Z"), "Nowhere/Unknown"),
            "Nov 2, 2026 at 11:45 PM UTC (unknown time zone; check it in Calendar)"
        );
        // An all-day event's end date is exclusive.
        let days = |end: u64| {
            let event = model::event(&json!({"title":"Synthetic day","start_ms":86400000,"end_ms":end,"timezone":"UTC","all_day":true})).unwrap();
            mac_days(&event).unwrap()
        };
        assert_eq!(days(172800000), "All day, Jan 2, 1970");
        assert_eq!(days(259200000), "All day, Jan 2, 1970 through Jan 3, 1970");
    }
    #[test]
    fn a_mac_asks_before_deleting_with_cancel_the_default() {
        let calendar = Calendar { id: "c".into(), account: "a".into(), name: "Family".into(), account_name: "Home".into(), writable: true };
        let event = model::event(&json!({"title":"Planning","start_ms":86400000,"end_ms":90000000,"timezone":"UTC"})).unwrap();
        let words = |prepared| mac_words("os.calendar", &Phase::Ready(prepared));
        let delete = words(Prepared::Delete { calendar: calendar.clone(), previous: event.clone() });
        assert!(delete.careful, "what undoes something does not default to doing it");
        assert_eq!(delete.approve, "Delete Event");
        assert!(!words(Prepared::Save { calendar: calendar.clone(), event, previous: None }).careful);
        for prepared in [Prepared::Permission, Prepared::Select(calendar)] {
            let asks = words(prepared);
            assert!(!asks.careful);
            assert_eq!((asks.approve, asks.refuse), ("Allow", "Don't Allow"));
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
                r#"review := DeviceCalendarReview {ticket: "already-expired"}"#,
            );
            let widget = sheet
                .view
                .children
                .iter()
                .find(|(id, _)| *id == id!(review))
                .unwrap()
                .1
                .clone();
            let mut review = widget.borrow_mut::<DeviceCalendarReview>().unwrap();
            assert_eq!(review.desk, desk);
            review.refresh(&mut cx);
            for id in [ids!(details), ids!(status), ids!(cancel), ids!(approve)] {
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
    /// Which question is which; the looks themselves are the approval rig's (a test's
    /// Splash is a heap of its own, which `style_alert`'s script can't reach).
    #[test]
    fn the_access_question_is_a_privacy_alert_and_a_review_a_sheet() {
        let calendar = Calendar { id: "c".into(), account: "a".into(), name: "Family".into(), account_name: "Home".into(), writable: true };
        let event = model::event(&json!({"title":"Planning","start_ms":86400000,"end_ms":90000000,"timezone":"UTC"})).unwrap();
        let words = |phase: &Phase| mac_words("os.calendar", phase);
        assert_eq!(words(&Phase::Ready(Prepared::Permission)).alert, Some(true));
        assert_eq!(words(&Phase::Ready(Prepared::Select(calendar.clone()))).alert, Some(false));
        assert_eq!(words(&Phase::Ready(Prepared::Save { calendar: calendar.clone(), event: event.clone(), previous: None })).alert, Some(false));
        assert_eq!(words(&Phase::Ready(Prepared::Delete { calendar, previous: event })).alert, Some(false));
        // What comes before the question and after an answer keeps the look it has.
        assert_eq!(words(&Phase::Loading).alert, None);
        assert_eq!(words(&Phase::Submitted).alert, None);
        assert_eq!(words(&Phase::Finished(Err("no".into()))).alert, None);
    }
    #[test]
    fn a_style_family_change_keeps_the_review_s_words() {
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
            r#"review := DeviceCalendarReview {width: Fill height: Fill ticket: "already-expired"}"#,
        );
        for style in [DesktopStyle::Ios, DesktopStyle::Macos] {
            sheet.set_stylesheet(&mut cx, load_sheet(style, false));
            draw_frame(&mut cx, &mut sheet);
            let widget = sheet
                .view
                .children
                .iter()
                .find(|(id, _)| *id == id!(review))
                .unwrap()
                .1
                .clone();
            let review = widget.borrow::<DeviceCalendarReview>().unwrap();
            assert_eq!(review.desk, style == DesktopStyle::Macos);
            assert!(review
                .view
                .widget(&mut cx, ids!(details))
                .text()
                .contains("expired"));
            let approve = review.view.button(&mut cx, ids!(approve));
            assert!(!approve.borrow().unwrap().enabled());
        }
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
