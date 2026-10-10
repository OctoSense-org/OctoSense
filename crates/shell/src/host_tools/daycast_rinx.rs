//! UI-only Daycast → live Rinx bridge. Fixed methods, authenticated app id,
//! no agent impersonation, credentials, HTTP Matrix requests or room creation.
//! Rinx's `send_message` presents its own exact-text confirmation and uses
//! the signed-in Matrix SDK session (including encrypted rooms).
use std::collections::HashMap;
use std::sync::{atomic::{AtomicU64, Ordering}, Mutex};
use std::time::{Duration, Instant};
use octosense_appstore::services::{Replier, ServiceCall};
use serde_json::{json, Value};
use super::{BusRequest, ToolOutcome};

pub const PREFIX: &str = "daycast-rinx-";
static NEXT: AtomicU64 = AtomicU64::new(1);
static WAITING: Mutex<Option<HashMap<String, (String, Replier, Instant)>>> = Mutex::new(None);

fn request(call: &ServiceCall) -> Result<(&'static str, Value), String> {
    if !["os.weather-assistant", "weather-assistant"].contains(&call.app_id.as_str()) || call.from_sheet || !call.may_prompt {
        return Err("Rinx 家庭提醒仅供 Daycast 使用。".into());
    }
    let args = call.args.as_object().ok_or("提醒参数必须是对象。")?;
    match call.method() {
        "daycast.rinx.rooms" if args.is_empty() => Ok(("list_rooms", json!({}))),
        "daycast.rinx.send" => {
            if args.len() != 2 || !args.contains_key("room") || !args.contains_key("text") {
                return Err("仅支持聊天室 ID 和消息正文。".into());
            }
            let room = args["room"].as_str().unwrap_or("").trim();
            let text = args["text"].as_str().unwrap_or("").trim();
            if !room.starts_with('!') || !room.contains(':') || room.len() > 1024 || room.chars().any(char::is_whitespace) {
                return Err("请先从 Rinx 已加入的聊天室中选择家人聊天室。".into());
            }
            if text.is_empty() || text.len() > 8192 {
                return Err("天气提醒正文不能为空或超过 8192 字节。".into());
            }
            Ok(("send_message", json!({"room": room, "text": text})))
        }
        _ => Err("不支持的 Rinx 家庭提醒方法。".into()),
    }
}

pub fn call(call: ServiceCall, reply: Replier) {
    let (tool, args) = match request(&call) {
        Ok(request) => request,
        Err(error) => { reply.send(Err(error)); return; }
    };
    let mut guard = WAITING.lock().unwrap_or_else(|e| e.into_inner());
    let waiting = guard.get_or_insert_with(HashMap::new);
    if waiting.values().any(|(app, _, _)| app == &call.app_id) {
        reply.send(Err("上一次 Rinx 请求尚未完成，请先处理 Rinx 的确认窗口。".into()));
        return;
    }
    let id = format!("{PREFIX}{}", NEXT.fetch_add(1, Ordering::Relaxed));
    waiting.insert(id.clone(), (call.app_id, reply, Instant::now()));
    drop(guard);
    super::BUS.lock().unwrap_or_else(|e| e.into_inner()).push(BusRequest::Call {
        call_id: id, app: "rinx".into(), tool: tool.into(), args: args.to_string(),
    });
    makepad_widgets::makepad_platform::SignalToUI::set_ui_signal();
}

pub fn result(id: &str, outcome: ToolOutcome) {
    let reply = WAITING.lock().unwrap_or_else(|e| e.into_inner()).as_mut().and_then(|w| w.remove(id));
    if let Some((_, reply, _)) = reply {
        reply.send(match outcome {
            ToolOutcome::Ok(value) => Ok(value["data"].clone()),
            ToolOutcome::Error { message, .. } => Err(message),
        });
    }
}

pub fn expire() {
    let mut guard = WAITING.lock().unwrap_or_else(|e| e.into_inner());
    let Some(waiting) = guard.as_mut() else { return };
    let expired: Vec<_> = waiting.iter().filter(|(_, (_, _, start))| start.elapsed() >= Duration::from_secs(120)).map(|(id, _)| id.clone()).collect();
    let replies: Vec<_> = expired.iter().filter_map(|id| waiting.remove(id).map(|(_, reply, _)| (id.clone(), reply))).collect();
    drop(guard);
    for (id, reply) in replies {
        super::BUS.lock().unwrap_or_else(|e| e.into_inner()).push(BusRequest::Cancel { call_id: id });
        reply.send(Err("Rinx 请求超时，发送结果未知；请先检查聊天室，再决定重试。".into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(app: &str, method: &str, args: Value) -> ServiceCall {
        ServiceCall { app_id: app.into(), service: format!("storage.daycast.rinx.{method}"), args, from_sheet: false, may_prompt: true, host_dir: "/unused".into() }
    }
    #[test]
    fn restricts_caller_methods_and_parameters() {
        assert!(request(&sample("os.mail", "rooms", json!({}))).is_err());
        assert!(request(&sample("os.weather-assistant", "login", json!({}))).is_err());
        assert!(request(&sample("weather-assistant", "send", json!({"room":"@dad:matrix.rinx.chat", "text":"下雨"}))).is_err());
        assert!(request(&sample("weather-assistant", "send", json!({"room":"!home:matrix.rinx.chat", "text":"下雨", "silent":true}))).is_err());
        let mut sheet = sample("weather-assistant", "rooms", json!({}));
        sheet.from_sheet = true;
        assert!(request(&sheet).is_err());
        sheet.from_sheet = false;
        sheet.may_prompt = false;
        assert!(request(&sheet).is_err());
    }
    #[test]
    fn sends_only_via_rinx_confirmed_tool_and_keeps_exact_text() {
        let (tool, args) = request(&sample("os.weather-assistant", "send", json!({"room":"!home:matrix.rinx.chat", "text":"北京开始降雨，请带伞。"}))).unwrap();
        assert_eq!(tool, "send_message");
        assert_eq!(args["text"], "北京开始降雨，请带伞。");
        assert_eq!(request(&sample("weather-assistant", "rooms", json!({}))).unwrap().0, "list_rooms");
    }
    #[test]
    fn host_dispatch_routes_native_results_once_without_a_matrix_session() {
        use octosense_appstore::services::{self, ServiceHost};
        struct NoSheet;
        impl ServiceHost for NoSheet {
            fn open_sheet(&mut self, _: String) { panic!("credentials never belong to Daycast") }
            fn close_sheet(&mut self) {}
        }
        octosense_daycast_service::register();
        octosense_daycast_service::register_rinx_bridge(call);
        let heap = 812345;
        services::dispatch(sample("weather-assistant", "rooms", json!({})), heap, 1, &mut NoSheet);
        let requests = super::super::take_bus_requests();
        let BusRequest::Call { call_id, app, tool, .. } = &requests[0] else { panic!("native call") };
        assert_eq!((app.as_str(), tool.as_str()), ("rinx", "list_rooms"));
        assert!(services::take_replies_for(&[heap]).is_empty());
        super::super::bus_result(call_id, ToolOutcome::Ok(json!({"data":{"rooms":[{"id":"!home:matrix.rinx.chat", "name":"家人"}]}})));
        super::super::bus_result(call_id, ToolOutcome::error("late", "late response"));
        let replies = services::take_replies_for(&[heap]);
        assert_eq!(replies.len(), 1);
        assert!(replies[0].2.as_ref().unwrap().contains("家人"));
        services::dispatch(sample("weather-assistant", "send", json!({"room":"!home:matrix.rinx.chat", "text":"下雨"})), heap, 2, &mut NoSheet);
        let requests = super::super::take_bus_requests();
        let BusRequest::Call { call_id, tool, .. } = &requests[0] else { panic!("native call") };
        assert_eq!(tool, "send_message");
        super::super::bus_result(call_id, ToolOutcome::error("denied", "person declined"));
        assert!(services::take_replies_for(&[heap])[0].2.is_err());
        services::dispatch(sample("weather-assistant", "rooms", json!({})), heap, 3, &mut NoSheet);
        let requests = super::super::take_bus_requests();
        let BusRequest::Call { call_id, .. } = &requests[0] else { panic!("native call") };
        WAITING.lock().unwrap().as_mut().unwrap().get_mut(call_id).unwrap().2 = Instant::now() - Duration::from_secs(121);
        let requests = super::super::take_bus_requests();
        assert!(matches!(&requests[0], BusRequest::Cancel { call_id: cancelled } if cancelled == call_id));
        assert!(services::take_replies_for(&[heap])[0].2.as_ref().unwrap_err().contains("结果未知"));
        super::super::bus_result(call_id, ToolOutcome::Ok(json!({"data":{}})));
        assert!(services::take_replies_for(&[heap]).is_empty());
    }
}
