//! EventKit adapter. Constructed only for a host-authorized command, on a
//! bounded worker; tests never invoke EventKit or inspect personal calendars.
use super::model::{Calendar, Command, EventData};
use makepad_widgets::makepad_platform::{
    makepad_objc_sys::{class, msg_send, objc_block, sel, sel_impl},
    os::apple::apple_sys::*,
};
use serde_json::{json, Value};
use std::{
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};
#[link(name = "EventKit", kind = "framework")]
unsafe extern "C" {}
struct Owned(ObjcId);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.0, release];
        }
    }
}
unsafe fn string(object: ObjcId) -> String {
    if object == nil {
        String::new()
    } else {
        nsstring_to_string(object)
    }
}
unsafe fn property(object: ObjcId, selector: Sel) -> String {
    string(
        makepad_widgets::makepad_platform::makepad_objc_sys::__send_message(object, selector, ())
            .unwrap(),
    )
}
unsafe fn auth() -> i64 {
    msg_send![class!(EKEventStore),authorizationStatusForEntityType:0usize]
}
fn auth_name(status: i64) -> &'static str {
    match status {
        3 => "granted",
        0 => "not_determined",
        _ => "denied",
    }
}
unsafe fn calendar(store: ObjcId, id: &str) -> Result<(ObjcId, Calendar), String> {
    let object: ObjcId = msg_send![store,calendarWithIdentifier:str_to_nsstring(id)];
    if object == nil {
        return Err("calendar_missing: Calendar is no longer available".into());
    }
    let source: ObjcId = msg_send![object, source];
    let data = Calendar {
        id: property(object, sel!(calendarIdentifier)),
        account: property(source, sel!(sourceIdentifier)),
        name: property(object, sel!(title)),
        account_name: property(source, sel!(title)),
        writable: {
            let v: BOOL = msg_send![object, allowsContentModifications];
            v == YES
        },
    };
    if data.id.len() > 1024
        || data.account.is_empty()
        || data.account.len() > 1024
        || data.name.len() > 2048
        || data.account_name.len() > 2048
    {
        return Err("limit: Calendar metadata exceeds bounds".into());
    }
    Ok((object, data))
}
unsafe fn selected(store: ObjcId, expected: &Calendar, write: bool) -> Result<ObjcId, String> {
    let (object, current) = calendar(store, &expected.id)?;
    if current.account != expected.account {
        return Err("account_changed: Calendar now belongs to a different source".into());
    }
    if write && !current.writable {
        return Err("read_only: Calendar cannot be edited".into());
    }
    Ok(object)
}
unsafe fn event_data(object: ObjcId) -> Result<EventData, String> {
    if object == nil {
        return Err("event_missing: Event no longer exists".into());
    }
    let start: ObjcId = msg_send![object, startDate];
    let end: ObjcId = msg_send![object, endDate];
    let start: f64 = msg_send![start, timeIntervalSince1970];
    let end: f64 = msg_send![end, timeIntervalSince1970];
    let mut timezone: ObjcId = msg_send![object, timeZone];
    if timezone == nil {
        timezone = msg_send![class!(NSTimeZone), localTimeZone];
    }
    let all_day: BOOL = msg_send![object, isAllDay];
    let recurring: BOOL = msg_send![object, hasRecurrenceRules];
    let attendees: BOOL = msg_send![object, hasAttendees];
    let data = EventData {
        id: property(object, sel!(eventIdentifier)),
        title: property(object, sel!(title)),
        start_ms: (start * 1000.).round() as i64,
        end_ms: (end * 1000.).round() as i64,
        timezone: if timezone == nil {
            "UTC".into()
        } else {
            property(timezone, sel!(name))
        },
        all_day: all_day == YES,
        location: property(object, sel!(location)),
        notes: property(object, sel!(notes)),
        recurring: recurring == YES,
        has_attendees: attendees == YES,
    };
    if data.id.len() > 1024
        || data.title.len() > 512
        || data.notes.len() > 8192
        || data.location.len() > 2048
    {
        return Err("limit: Event content exceeds public API bounds".into());
    }
    Ok(data)
}
unsafe fn get(store: ObjcId, expected: &Calendar, id: &str) -> Result<(ObjcId, EventData), String> {
    selected(store, expected, false)?;
    let object: ObjcId = msg_send![store,eventWithIdentifier:str_to_nsstring(id)];
    if object == nil {
        return Err("event_missing: Event no longer exists".into());
    }
    let parent: ObjcId = msg_send![object, calendar];
    if property(parent, sel!(calendarIdentifier)) != expected.id {
        return Err("invalid_handle: Event is outside the selected calendar".into());
    }
    Ok((object, event_data(object)?))
}
pub(super) fn execute(
    command: &Command,
    guard: impl Fn() -> Result<(), String>,
) -> Result<Value, String> {
    unsafe {
        let _pool = Owned(msg_send![class!(NSAutoreleasePool), new]);
        if matches!(command, Command::Status) {
            return Ok(json!({"os_permission":auth_name(auth())}));
        }
        let store = Owned(msg_send![class!(EKEventStore), new]);
        if store.0 == nil {
            return Err("platform_error: Cannot open EventKit".into());
        }
        if matches!(command, Command::Permission) {
            if auth() == 3 {
                return Ok(json!({"os_permission":"granted"}));
            }
            // Never trigger Apple's privacy termination for a developer executable
            // lacking the packaged usage descriptions. Rebuild the .app instead.
            let bundle: ObjcId = msg_send![class!(NSBundle), mainBundle];
            let modern: BOOL = msg_send![store.0,respondsToSelector:sel!(requestFullAccessToEventsWithCompletion:)];
            let key = if modern == YES {
                "NSCalendarsFullAccessUsageDescription"
            } else {
                "NSCalendarsUsageDescription"
            };
            let description: ObjcId =
                msg_send![bundle,objectForInfoDictionaryKey:str_to_nsstring(key)];
            if description == nil {
                return Err("host_not_configured: Install the packaged app with Calendar usage descriptions".into());
            }
            let (tx, rx) = mpsc::sync_channel(1);
            let completion = objc_block!(move |granted: BOOL, _error: ObjcId| {
                let _ = tx.try_send(granted == YES);
            });
            guard()?;
            if modern == YES {
                let _: () = msg_send![store.0,requestFullAccessToEventsWithCompletion:&completion];
            } else {
                let _: () =
                    msg_send![store.0,requestAccessToEntityType:0usize completion:&completion];
            }
            let granted = rx
                .recv_timeout(Duration::from_secs(40))
                .map_err(|_| "timeout: Calendar OS permission still unanswered")?;
            return Ok(json!({"os_permission":if granted&&auth()==3{"granted"}else{"denied"}}));
        }
        if auth() != 3 {
            return Err("authorization_required: OS calendar permission is not granted".into());
        }
        match command {
            Command::Calendars => {
                let all: ObjcId = msg_send![store.0,calendarsForEntityType:0usize];
                let count: usize = msg_send![all, count];
                if count > 64 {
                    return Err(
                        "limit: More than 64 calendars; narrow OS account configuration".into(),
                    );
                }
                let mut values = Vec::new();
                for i in 0..count {
                    let c: ObjcId = msg_send![all,objectAtIndex:i];
                    let id = property(c, sel!(calendarIdentifier));
                    values.push(calendar(store.0, &id)?.1);
                }
                Ok(json!({"calendars":values}))
            }
            Command::Calendar { id } => Ok(serde_json::to_value(calendar(store.0, id)?.1).unwrap()),
            Command::Get {
                calendar: expected,
                id,
            } => {
                let mut value = serde_json::to_value(get(store.0, expected, id)?.1).unwrap();
                value["_calendar"] =
                    serde_json::to_value(calendar(store.0, &expected.id)?.1).unwrap();
                Ok(value)
            }
            Command::List {
                calendar: expected,
                start,
                end,
                limit,
            } => {
                let calendar = selected(store.0, expected, false)?;
                let calendars: ObjcId = msg_send![class!(NSArray),arrayWithObject:calendar];
                let start: ObjcId =
                    msg_send![class!(NSDate),dateWithTimeIntervalSince1970:(*start as f64/1000.)];
                let end: ObjcId =
                    msg_send![class!(NSDate),dateWithTimeIntervalSince1970:(*end as f64/1000.)];
                let predicate: ObjcId = msg_send![store.0,predicateForEventsWithStartDate:start endDate:end calendars:calendars];
                let events = Arc::new(Mutex::new(Vec::<EventData>::new()));
                let output = events.clone();
                let maximum = *limit;
                let failure = Arc::new(Mutex::new(None));
                let error = failure.clone();
                let block = objc_block!(move |event: ObjcId, stop: *mut BOOL| {
                    match event_data(event) {
                        Ok(data) => output.lock().unwrap().push(data),
                        Err(e) => {
                            *error.lock().unwrap() = Some(e);
                            *stop = YES;
                        }
                    }
                    if output.lock().unwrap().len() > maximum {
                        *stop = YES;
                    }
                });
                let _: () =
                    msg_send![store.0,enumerateEventsMatchingPredicate:predicate usingBlock:&block];
                if let Some(error) = failure.lock().unwrap().take() {
                    return Err(error);
                }
                let mut rows = events.lock().unwrap().clone();
                let truncated = rows.len() > *limit;
                rows.truncate(*limit);
                Ok(json!({"events":rows,"truncated":truncated}))
            }
            Command::Save {
                calendar,
                event,
                previous,
            } => {
                let native_calendar = selected(store.0, calendar, true)?;
                let object = if let Some(previous) = previous {
                    let (object, current) = get(store.0, calendar, &previous.id)?;
                    current.editable()?;
                    if current != *previous {
                        return Err(
                            "conflict: Event changed after review; reload and review again".into(),
                        );
                    }
                    object
                } else {
                    msg_send![class!(EKEvent),eventWithEventStore:store.0]
                };
                let zone: ObjcId =
                    msg_send![class!(NSTimeZone),timeZoneWithName:str_to_nsstring(&event.timezone)];
                if zone == nil {
                    return Err("invalid_arguments: Unknown timezone".into());
                }
                let start: ObjcId = msg_send![class!(NSDate),dateWithTimeIntervalSince1970:(event.start_ms as f64/1000.)];
                let end: ObjcId = msg_send![class!(NSDate),dateWithTimeIntervalSince1970:(event.end_ms as f64/1000.)];
                let _: () = msg_send![object,setCalendar:native_calendar];
                let _: () = msg_send![object,setTitle:str_to_nsstring(&event.title)];
                let _: () = msg_send![object,setStartDate:start];
                let _: () = msg_send![object,setEndDate:end];
                let _: () = msg_send![object,setTimeZone:zone];
                let _: () = msg_send![object,setAllDay:if event.all_day{YES}else{NO}];
                let _: () = msg_send![object,setLocation:str_to_nsstring(&event.location)];
                let _: () = msg_send![object,setNotes:str_to_nsstring(&event.notes)];
                let mut error: ObjcId = nil;
                guard()?;
                let ok: BOOL =
                    msg_send![store.0,saveEvent:object span:0usize commit:YES error:&mut error];
                if ok != YES {
                    return Err(
                        "platform_error: Calendar save failed; refresh before retrying".into(),
                    );
                }
                Ok(json!({"saved":true,"event":event_data(object)?.public()}))
            }
            Command::Delete { calendar, previous } => {
                selected(store.0, calendar, true)?;
                let (object, current) = get(store.0, calendar, &previous.id)?;
                current.editable()?;
                if current != *previous {
                    return Err(
                        "conflict: Event changed after review; reload and review again".into(),
                    );
                }
                let mut error: ObjcId = nil;
                guard()?;
                let ok: BOOL =
                    msg_send![store.0,removeEvent:object span:0usize commit:YES error:&mut error];
                if ok != YES {
                    return Err(
                        "platform_error: Calendar deletion failed; refresh before retrying".into(),
                    );
                }
                Ok(json!({"deleted":true,"event_id":previous.id}))
            }
            Command::LoadConsent | Command::Status | Command::Permission | Command::Persist(_) => {
                unreachable!()
            }
        }
    }
}
