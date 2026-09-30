//! The system chat against a scripted kernel connection: streaming
//! assembly, tool-call status, questions, interrupt, history, new
//! conversation, reconnect-and-resume after a kernel restart, approvals
//! through the router, and the command-execution switch.

use super::grants::{self, CommandGesture, GrantStore};
use super::model::{ApprovalState, ChatModel, Effect, Item, Phase, Role, ToolStatus};
use super::session::{Closed, Command, Connector, Driver, Link, Recv, SystemHost, Unavailable, SYSTEM_SESSION};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the fake kernel saw and will say.
#[derive(Default)]
struct Script {
    /// Every request the driver sent (method, params).
    sent: Vec<(String, Value)>,
    /// Frames waiting for the driver.
    inbox: VecDeque<String>,
    /// History `session/hydrate` answers with.
    history: Value,
    /// Close the link at the next receive (a kernel restart).
    close: Option<Closed>,
    connects: usize,
    /// `connect` answers this instead of a link.
    unavailable: Option<Unavailable>,
    /// `session/open` fails with this message.
    open_error: Option<String>,
}

#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Script>>);

impl Fake {
    fn s(&self) -> std::sync::MutexGuard<'_, Script> {
        self.0.lock().unwrap()
    }
    fn notify(&self, method: &str, mut params: Value) {
        params["session_id"] = json!(SYSTEM_SESSION);
        self.s().inbox.push_back(json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string());
    }
    fn sent(&self, method: &str) -> Vec<Value> {
        self.s().sent.iter().filter(|(m, _)| m == method).map(|(_, p)| p.clone()).collect()
    }
}

struct FakeLink(Fake);

impl Link for FakeLink {
    fn send(&mut self, frame: String) -> Result<(), Closed> {
        let v: Value = serde_json::from_str(&frame).unwrap();
        let method = v["method"].as_str().unwrap().to_string();
        let mut s = self.0.s();
        s.sent.push((method.clone(), v["params"].clone()));
        let id = v["id"].clone();
        let reply = match method.as_str() {
            "session/open" => match &s.open_error {
                Some(e) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": e}}),
                None => json!({"jsonrpc": "2.0", "id": id, "result": {"opened": {"session_id": SYSTEM_SESSION}}}),
            },
            "session/hydrate" => json!({"jsonrpc": "2.0", "id": id, "result": {"messages": s.history.clone()}}),
            _ => json!({"jsonrpc": "2.0", "id": id, "result": {}}),
        };
        s.inbox.push_back(reply.to_string());
        Ok(())
    }
    fn recv(&mut self, _wait: Duration) -> Recv {
        let mut s = self.0.s();
        if let Some(closed) = s.close.take() {
            return Recv::Closed(closed);
        }
        match s.inbox.pop_front() {
            Some(f) => Recv::Frame(f),
            None => Recv::Idle,
        }
    }
}

struct FakeConnector(Fake);

impl Connector for FakeConnector {
    fn connect(&mut self) -> Result<Box<dyn Link>, Unavailable> {
        let mut s = self.0.s();
        s.connects += 1;
        if let Some(u) = s.unavailable.clone() {
            return Err(u);
        }
        Ok(Box::new(FakeLink(self.0.clone())))
    }
}

/// The system session's grants and credential, as a test sets them.
#[derive(Clone, Default)]
struct Grants(Arc<Mutex<(Vec<Value>, Option<String>)>>);

impl SystemHost for Grants {
    fn declarations(&self) -> Vec<Value> {
        self.0.lock().unwrap().0.clone()
    }
    fn host_token(&self) -> Option<String> {
        self.0.lock().unwrap().1.clone()
    }
}

fn driver() -> (Driver, Fake) {
    let (d, fake, _) = driver_with(Grants::default());
    (d, fake)
}

fn driver_with(grants: Grants) -> (Driver, Fake, Grants) {
    let fake = Fake::default();
    (Driver::with_system_host(Box::new(FakeConnector(fake.clone())), Box::new(grants.clone())), fake, grants)
}

/// Step until nothing is left to read.
fn settle(d: &mut Driver) {
    for _ in 0..20 {
        d.step(Duration::ZERO);
    }
}

fn opened() -> (Driver, Fake) {
    let (mut d, fake) = driver();
    d.command(Command::Open);
    settle(&mut d);
    assert_eq!(d.model.phase(), &Phase::Ready);
    (d, fake)
}

fn text_of(model: &ChatModel, role: Role) -> Vec<String> {
    model.items.iter().filter_map(|i| match i {
        Item::Message { role: r, text, .. } if *r == role => Some(text.clone()),
        _ => None,
    }).collect()
}

// ---------------------------------------------------------------- the model

#[test]
fn deltas_assemble_the_answer_of_their_turn() {
    let mut m = ChatModel::new();
    m.start_turn("t1", "What's on today?");
    for piece in ["You have ", "two ", "meetings."] {
        m.apply("message/delta", &json!({"turn_id": "t1", "text": piece}));
    }
    assert_eq!(text_of(&m, Role::Assistant), ["You have two meetings."]);
    assert_eq!(m.phase().running_turn(), Some("t1"));
    m.apply("turn/completed", &json!({"turn_id": "t1"}));
    assert_eq!(m.phase(), &Phase::Ready);
    // The next turn's text starts a new message.
    m.start_turn("t2", "And tomorrow?");
    m.apply("message/delta", &json!({"turn_id": "t2", "text": "Nothing."}));
    assert_eq!(text_of(&m, Role::Assistant), ["You have two meetings.", "Nothing."]);
}

#[test]
fn v2_envelopes_stream_and_the_saved_text_wins() {
    let mut m = ChatModel::new();
    m.start_turn("t1", "hi");
    let env = |kind: &str, text: &str| json!({"turn_id": "t1", "payload": {"type": kind, "data": {"assistant_segment_id": "s1", "text": text}}});
    m.apply("projection/envelope", &env("assistant_delta", "Hel"));
    m.apply("projection/envelope", &env("assistant_delta", "lo"));
    m.apply("projection/envelope", &env("assistant_persisted", "Hello!"));
    m.apply("projection/envelope", &env("assistant_delta", "late"));
    assert_eq!(text_of(&m, Role::Assistant), ["Hello!"]);
    m.apply("projection/envelope", &json!({"turn_id": "t1", "payload": {"type": "turn_terminal", "data": {"outcome": "completed"}}}));
    assert_eq!(m.phase(), &Phase::Ready);
}

/// The frames a real kernel (octos e045c727, stdio) sent for one turn that
/// streamed, called `peer_list`, then answered, in the order it sent them:
/// `turn_terminal` first, a segment's saved text before its last deltas,
/// the second segment saved before the tool ran. The live run showed the
/// same text twice, a partial row ("There's no peer available for") left
/// behind, and an empty "Assistant" row between them.
#[test]
fn out_of_order_envelopes_make_one_message_per_segment_in_ledger_order() {
    let mut m = ChatModel::new();
    m.start_turn("t1", "list peers");
    let a = "t1:iteration:1";
    let b = "t1:iteration:2";
    let env = |cursor: u64, kind: &str, data: Value| json!({"turn_id": "t1", "cursor": {"seq": cursor}, "payload": {"type": kind, "data": data}});
    let text = |segment: &str, text: &str| json!({"assistant_segment_id": segment, "text": text});
    let frames = [
        env(30, "turn_terminal", json!({"outcome": "completed"})),
        env(7, "user_message", json!({"text": "list peers"})),
        env(8, "assistant_delta", text(a, "There's no peer ")),
        env(9, "assistant_persisted", text(a, "There's no peer available for that yet. Let me check.")),
        env(10, "assistant_delta", text(a, "available for ")),
        env(11, "assistant_delta", text(a, "that yet. Let me check.")),
        env(12, "assistant_persisted", text(b, "Here are both results: done.")),
        env(16, "tool_start", json!({"tool_call_id": "call_1", "name": "peer_list"})),
        env(18, "tool_end", json!({"tool_call_id": "call_1", "status": "complete", "output_preview": "(no peers staged)"})),
        env(24, "assistant_delta", text(b, "Here are ")),
        env(25, "assistant_delta", text(b, "both results: ")),
        env(26, "assistant_delta", text(b, "done.")),
    ];
    for frame in &frames {
        m.apply("projection/envelope", frame);
    }
    assert_eq!(text_of(&m, Role::Assistant), ["There's no peer available for that yet. Let me check.", "Here are both results: done."]);
    assert_eq!(m.phase(), &Phase::Ready);
    // You, the first answer, the tool, the second answer: nothing else.
    let shape: Vec<String> = m.items.iter().map(|i| match i {
        Item::Message { role: Role::User, .. } => "you".to_string(),
        Item::Message { text, .. } => format!("assistant:{}", &text[..9]),
        Item::Tool { name, status, .. } => format!("tool:{name}:{status:?}"),
        other => format!("{other:?}"),
    }).collect();
    assert_eq!(shape, ["you", "assistant:There's n", "tool:peer_list:Done", "assistant:Here are "]);
}

/// An iteration that only called tools saves an empty segment: no empty
/// "Assistant" row; and text that only streamed is shown until it is saved.
#[test]
fn an_empty_segment_shows_nothing_and_streamed_text_shows_before_it_is_saved() {
    let mut m = ChatModel::new();
    m.start_turn("t1", "hi");
    let env = |cursor: u64, kind: &str, segment: &str, text: &str| json!({"turn_id": "t1", "cursor": {"seq": cursor}, "payload": {"type": kind, "data": {"assistant_segment_id": segment, "text": text}}});
    m.apply("projection/envelope", &env(3, "assistant_persisted", "s1", ""));
    assert!(text_of(&m, Role::Assistant).is_empty());
    m.apply("projection/envelope", &env(5, "assistant_delta", "s2", "Work"));
    m.apply("projection/envelope", &env(6, "assistant_delta", "s2", "ing"));
    assert_eq!(text_of(&m, Role::Assistant), ["Working"]);
    // Saved empty after all (the text was withdrawn): the row goes.
    m.apply("projection/envelope", &env(7, "assistant_persisted", "s2", " "));
    assert!(text_of(&m, Role::Assistant).is_empty());
}

/// A provider's 401 as the kernel passes it on (the live run's, with its
/// masked key tail): the person never sees a piece of the key.
#[test]
fn provider_errors_are_shown_without_key_fragments() {
    use super::model::redact_secrets;
    let raw = r#"Authentication failed for deepseek@api/deepseek-v4-flash - check your API key (HTTP 401 - {"error":{"message":"Authentication Fails, Your api key: ****fcb0 is invalid (request_id: 0a94e4b1-824d-456b-942a-d4a7f2f84553)","type":"authentication_error"}})"#;
    let shown = redact_secrets(raw);
    assert!(!shown.contains("fcb0"), "{shown}");
    assert!(shown.contains("Your api key: [key] is invalid"), "{shown}");
    assert!(shown.contains("deepseek@api/deepseek-v4-flash") && shown.contains("0a94e4b1-824d-456b-942a-d4a7f2f84553"), "the rest stays: {shown}");
    for (input, gone) in [
        ("Incorrect API key provided: sk-proj-****abcd.", "abcd"),
        ("Authorization: Bearer abcDEF123ghiJKL456", "abcDEF"),
        ("key sk-ant-api03-Zx9Qw8Er7Ty6 rejected", "Zx9Qw8"),
        ("token 9f8e7d6c5b4a39281706f5e4d3c2b1a0ffeeddcc", "9f8e7d6c5b4a"),
    ] {
        let shown = redact_secrets(input);
        assert!(!shown.contains(gone) && shown.contains("[key]"), "{input} -> {shown}");
    }
    assert_eq!(redact_secrets("The model gpt-4o-mini timed out after 30 s."), "The model gpt-4o-mini timed out after 30 s.");
    // The model's own notices go through it.
    let mut m = ChatModel::new();
    m.start_turn("t1", "hi");
    m.apply("turn/error", &json!({"turn_id": "t1", "message": raw}));
    assert!(m.items.iter().any(|i| matches!(i, Item::Notice(n) if n.contains("[key]") && !n.contains("fcb0"))));
}

#[test]
fn tool_calls_carry_their_status() {
    let mut m = ChatModel::new();
    m.start_turn("t1", "read my notes");
    m.apply("tool/started", &json!({"turn_id": "t1", "tool_call_id": "c1", "tool_name": "read_file", "arguments": {"path": "notes.md"}}));
    m.apply("tool/started", &json!({"turn_id": "t1", "tool_call_id": "c2", "tool_name": "web_fetch", "arguments": {"url": "https://example.org"}}));
    let status = |m: &ChatModel, id: &str| m.items.iter().find_map(|i| match i {
        Item::Tool { call_id, status, detail, .. } if call_id == id => Some((status.clone(), detail.clone())),
        _ => None,
    }).unwrap();
    assert_eq!(status(&m, "c1"), (ToolStatus::Running, "path: notes.md".into()));
    m.apply("tool/progress", &json!({"turn_id": "t1", "tool_call_id": "c2", "message": "fetching"}));
    assert_eq!(status(&m, "c2").1, "fetching");
    m.apply("tool/completed", &json!({"turn_id": "t1", "tool_call_id": "c1", "tool_name": "read_file", "success": true}));
    m.apply("tool/completed", &json!({"turn_id": "t1", "tool_call_id": "c2", "tool_name": "web_fetch", "success": false, "output_preview": "404"}));
    assert_eq!(status(&m, "c1").0, ToolStatus::Done);
    assert_eq!(status(&m, "c2"), (ToolStatus::Failed, "404".into()));
    // A tool still running when its turn ends did not finish.
    m.apply("tool/started", &json!({"turn_id": "t1", "tool_call_id": "c3", "tool_name": "grep"}));
    m.apply("turn/error", &json!({"turn_id": "t1", "code": "x", "message": "The provider refused."}));
    assert_eq!(status(&m, "c3").0, ToolStatus::Failed);
    assert!(matches!(m.items.last(), Some(Item::Notice(n)) if n == "The provider refused."));
}

#[test]
fn history_replaces_the_transcript_and_keeps_what_is_still_open() {
    let mut m = ChatModel::new();
    m.notice("old");
    m.apply("user_question/requested", &json!({"turn_id": "t9", "question_id": "q1", "title": "Which?", "body": "b",
        "questions": [{"header": "h", "question": "Which Edward?", "options": [{"label": "Edward A", "description": ""}, {"label": "Edward B", "description": ""}]}]}));
    m.load_history(&json!([
        {"seq": 1, "role": "user", "content": "hello"},
        {"seq": 2, "role": "assistant", "content": "Hi there."},
        {"seq": 3, "role": "tool", "content": "{}", "name": "read_file"},
        {"seq": 4, "role": "system", "content": "ignored"},
    ]));
    assert_eq!(text_of(&m, Role::User), ["hello"]);
    assert_eq!(text_of(&m, Role::Assistant), ["Hi there."]);
    assert!(m.items.iter().any(|i| matches!(i, Item::Tool { name, status: ToolStatus::Done, .. } if name == "read_file")));
    assert!(!m.items.iter().any(|i| matches!(i, Item::Notice(_))), "notices are not history");
    assert_eq!(m.open_question(), Some(("q1", 1)), "an open question survives a reload");
}

#[test]
fn an_approval_is_handed_on_never_answered_by_the_model() {
    let mut m = ChatModel::new();
    m.start_turn("t1", "Email Ana the notes");
    let effects = m.apply("approval/requested", &json!({"turn_id": "t1", "approval_id": "a1", "tool_name": "write_file",
        "title": "Write notes.md", "body": "outside the workspace"}));
    let Some(Effect::Approval(ask)) = effects.first() else { panic!("{effects:?}") };
    assert_eq!(ask.plan, "Email Ana the notes", "the turn's prompt is the batch's plan");
    assert_eq!(ask.args, json!({"title": "Write notes.md", "body": "outside the workspace"}));
    assert!(matches!(&m.items.last(), Some(Item::Approval { state: ApprovalState::Waiting, .. })));
    m.apply("approval/decided", &json!({"turn_id": "t1", "approval_id": "a1", "decision": "deny"}));
    assert!(matches!(&m.items.last(), Some(Item::Approval { state: ApprovalState::Denied, .. })));
}

// ---------------------------------------------------------------- the driver

#[test]
fn opening_resumes_the_system_session_and_loads_its_history() {
    let (mut d, fake) = driver();
    fake.s().history = json!([{"seq": 1, "role": "user", "content": "earlier"}, {"seq": 2, "role": "assistant", "content": "Earlier answer."}]);
    d.command(Command::Open);
    settle(&mut d);
    let open = fake.sent("session/open");
    assert_eq!(open, [json!({"session_id": SYSTEM_SESSION, "profile_id": "_main"})], "the one system conversation, no cwd, no token");
    assert_eq!(fake.sent("session/hydrate")[0]["include"], json!(["messages"]));
    assert_eq!(text_of(&d.model, Role::Assistant), ["Earlier answer."]);
    assert_eq!(d.model.phase(), &Phase::Ready);
}

#[test]
fn a_turn_streams_and_interrupt_stops_it() {
    let (mut d, fake) = opened();
    d.command(Command::Send("  Plan my Tuesday  ".into()));
    let start = fake.sent("turn/start");
    assert_eq!(start[0]["input"], json!([{"kind": "text", "text": "Plan my Tuesday"}]));
    let turn = start[0]["turn_id"].as_str().unwrap().to_string();
    assert_eq!(turn.len(), 36, "a UUID turn id");
    fake.notify("message/delta", json!({"turn_id": turn, "text": "Looking"}));
    fake.notify("tool/started", json!({"turn_id": turn, "tool_call_id": "c1", "tool_name": "peer_list"}));
    settle(&mut d);
    assert_eq!(text_of(&d.model, Role::Assistant), ["Looking"]);
    assert_eq!(d.model.phase().running_turn(), Some(turn.as_str()));
    // A second prompt waits for the first.
    d.command(Command::Send("again".into()));
    assert_eq!(fake.sent("turn/start").len(), 1);
    d.command(Command::Interrupt);
    assert_eq!(fake.sent("turn/interrupt"), [json!({"session_id": SYSTEM_SESSION, "turn_id": turn})]);
    assert_eq!(d.model.phase(), &Phase::Ready);
    assert!(d.model.items.iter().any(|i| matches!(i, Item::Tool { status: ToolStatus::Failed, .. })), "the running tool is marked stopped");
}

#[test]
fn a_question_is_answered_from_the_pane() {
    let (mut d, fake) = opened();
    d.command(Command::Send("invite Edward".into()));
    let turn = fake.sent("turn/start")[0]["turn_id"].clone();
    fake.notify("user_question/requested", json!({"turn_id": turn, "question_id": "q1", "title": "Which Edward?", "body": "b",
        "questions": [{"header": "h", "question": "Which one?", "options": [{"label": "Edward A", "description": ""}]}]}));
    settle(&mut d);
    let (q, n) = d.model.open_question().map(|(q, n)| (q.to_string(), n)).unwrap();
    d.command(Command::Answer { question: q, count: n, text: "Edward A".into(), option: true });
    let respond = fake.sent("user_question/respond");
    assert_eq!(respond[0]["answers"], json!([{"selected_labels": ["Edward A"]}]));
    assert_eq!(d.model.open_question(), None);
}

#[test]
fn approvals_are_answered_only_with_the_routers_decision() {
    let (mut d, fake) = opened();
    d.command(Command::Send("tidy my notes".into()));
    let turn = fake.sent("turn/start")[0]["turn_id"].clone();
    fake.notify("approval/requested", json!({"turn_id": turn, "approval_id": "a1", "tool_name": "write_file", "title": "t", "body": "b"}));
    settle(&mut d);
    assert!(matches!(d.effects.as_slice(), [Effect::Approval(a)] if a.approval_id == "a1"));
    assert!(fake.sent("approval/respond").is_empty(), "the chat never answers on its own");
    d.command(Command::Approval { approval_id: "a1".into(), approve: true });
    assert_eq!(fake.sent("approval/respond")[0]["decision"], "approve");
}

#[test]
fn new_conversation_clears_and_reloads() {
    let (mut d, fake) = opened();
    d.model.notice("something");
    d.command(Command::NewConversation);
    assert!(d.model.items.is_empty());
    assert_eq!(fake.sent("turn/start")[0]["input"][0]["text"], "/new", "the kernel's own new-conversation command, no model call");
    settle(&mut d);
    assert_eq!(fake.sent("session/hydrate").len(), 2, "history reloaded after");
}

#[test]
fn a_kernel_restart_resumes_the_same_session() {
    let (mut d, fake) = opened();
    d.command(Command::Send("long job".into()));
    fake.s().close = Some(Closed { restarted: true, why: "restarted".into() });
    settle(&mut d);
    assert!(matches!(d.model.phase(), Phase::Reconnecting(_)), "{:?}", d.model.phase());
    assert!(d.model.items.iter().any(|i| matches!(i, Item::Notice(n) if n.contains("restarted"))));
    std::thread::sleep(Duration::from_millis(250));
    settle(&mut d);
    assert_eq!(fake.s().connects, 2, "reconnected");
    assert_eq!(fake.sent("session/open").len(), 2, "reopened the same session");
    assert!(fake.sent("session/open").iter().all(|p| p["session_id"] == SYSTEM_SESSION));
    assert_eq!(d.model.phase(), &Phase::Ready);
}

#[test]
fn no_provider_and_no_kernel_are_said_plainly() {
    let (mut d, fake) = driver();
    fake.s().unavailable = Some(Unavailable::NoProvider);
    d.command(Command::Open);
    assert_eq!(d.model.phase(), &Phase::NoProvider);
    // Sending waits for a provider rather than failing.
    d.command(Command::Send("hello".into()));
    assert!(fake.sent("turn/start").is_empty());
    let (mut d, fake) = driver();
    fake.s().unavailable = Some(Unavailable::NoKernel("no kernel binary".into()));
    d.command(Command::Open);
    assert_eq!(d.model.phase(), &Phase::NoKernel("no kernel binary".into()));
    assert_eq!(super::view::phase_text(d.model.phase()), "The assistant isn't available on this device: no kernel binary.");
}

#[test]
fn closing_the_pane_lets_the_kernel_go_unless_a_turn_runs() {
    let (mut d, _fake) = opened();
    d.command(Command::Close);
    assert!(!d.is_connected(), "idle: the kernel may stop");
    let (mut d, _fake) = opened();
    d.command(Command::Send("keep going".into()));
    d.command(Command::Close);
    assert!(d.is_connected(), "a running turn keeps its connection");
}

// ---------------------------------------------------------------- approvals

#[test]
fn the_chats_approvals_reach_the_router_batched_per_request() {
    use crate::approvals::{sheet::Place, Approvals, Route};
    let mut a = Approvals::memory();
    let ask = |id: &str| super::model::ApprovalAsk {
        approval_id: id.into(), turn: "t1".into(), tool: "write_file".into(), title: "t".into(), body: "b".into(),
        args: json!({"path": id}), plan: "Tidy my notes".into(), app: None, outcome_unknown: false, external: false,
    };
    // What `route_approval` sends, on a router of our own.
    for id in ["a1", "a2"] {
        let ask = ask(id);
        let context = crate::approvals::RequestContext {
            call_id: format!("{}{}", super::HELD_PREFIX, ask.approval_id),
            trigger: crate::approvals::Trigger::Person,
            batch: Some(crate::approvals::Batch { id: format!("{}{}", super::HELD_PREFIX, ask.turn), plan: ask.plan.clone() }),
            ..Default::default()
        };
        let route = a.router.request(crate::approvals::router::make_request(super::APP, crate::approvals::ToolSpec::host(&ask.tool), ask.args.clone(), crate::approvals::Caller::SystemAgent, context, 1, 0), 1);
        assert!(matches!(route, Route::Sheet(_)), "{route:?}");
    }
    assert_eq!(a.router.sheets().len(), 1, "one sheet for the request");
    let sheet = &a.router.sheets()[0];
    assert!(matches!(&sheet.place, Place::SystemChat { plan, .. } if plan == "Tidy my notes"));
    assert_eq!(sheet.lines.len(), 2);
}

// ---------------------------------------------------------------- command execution

#[test]
fn command_execution_needs_the_persons_gesture_and_changes_the_tool_set() {
    let mut store = GrantStore::memory();
    assert!(!store.grants().command_execution, "off by default");
    assert!(store.tools().host_tools().is_empty());
    // No gesture, no grant.
    assert!(store.set_command_execution(true, None).is_err());
    assert!(!store.grants().command_execution);
    assert!(CommandGesture::settings_phrase("run commands").is_none());
    assert!(CommandGesture::settings_phrase("yes").is_none());
    let gesture = CommandGesture::settings_phrase("  Let the Assistant  run commands ").expect("the typed phrase");
    store.set_command_execution(true, Some(gesture)).unwrap();
    assert!(store.tools().command_execution());
    assert!(store.tools().host_tools().contains(grants::COMMAND_TOOL), "terminal.run joins the system agent's set");
    #[cfg(kernel)]
    assert!(!store.tools().names().contains("shell"), "never octos's shell");
    // Off needs nothing.
    store.set_command_execution(false, None).unwrap();
    assert!(store.tools().host_tools().is_empty());
}

#[test]
fn the_switch_is_persisted_owner_only() {
    let home = std::env::temp_dir().join(format!("syschat-grants-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let mut store = GrantStore::in_home(&home);
    store.set_command_execution(true, CommandGesture::settings_phrase(grants::CONFIRM_PHRASE)).unwrap();
    assert!(GrantStore::in_home(&home).grants().command_execution);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(home.join(grants::GRANTS_FILE)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn each_command_is_a_live_approval_with_the_exact_command() {
    use crate::approvals::dev_hooks::FixedDevMode;
    use crate::approvals::router::{make_request, AutoBy, Route};
    use crate::approvals::rules::{ApprovalGesture, RuleDraft};
    use crate::approvals::{Approvals, Caller, RequestContext, ToolSpec};
    // Not granted: refused before any sheet.
    assert!(matches!(grants::request_command(false, "c0", "ls", None), Route::Refused(_)));
    // Granted: a command no rule answers, even "everything for 60 min".
    let mut a = Approvals::memory();
    a.router.create_rule(&ApprovalGesture::settings_tap(), RuleDraft::everything(grants::COMMAND_APP, 60), 1).unwrap();
    let req = |id: &str| make_request(grants::COMMAND_APP, ToolSpec::host(grants::COMMAND_TOOL).command(), json!({"command": "rm -rf build"}),
        Caller::SystemAgent, RequestContext { call_id: id.into(), ..Default::default() }, 1, 0);
    assert!(matches!(a.router.request(req("c1"), 1), Route::Sheet(_)));
    let line = &a.router.front_sheet().unwrap().lines[0];
    assert!(line.args.iter().any(|l| l.contains("rm -rf build")), "the sheet shows the exact command: {:?}", line.args);
    assert!(line.always.is_empty(), "no \u{201c}always\u{201d} for a command");
    // Developer mode still answers it (ADR 0004 §13).
    a.router.set_hooks(Box::new(FixedDevMode::all()));
    assert_eq!(a.router.request(req("c2"), 1), Route::Approved(AutoBy::DeveloperMode));
}

/// Only Settings makes a `CommandGesture`: no agent, app, bus call or
/// service can turn command execution on.
#[test]
fn only_the_settings_row_makes_a_command_gesture() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut makers = Vec::new();
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                let rel = path.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
                if rel.starts_with("system_chat/") {
                    continue;
                }
                if text.contains("CommandGesture::") {
                    makers.push(rel);
                }
            }
        }
    }
    assert_eq!(makers, ["lib.rs"], "{makers:?}");
    let lib = std::fs::read_to_string(src.join("lib.rs")).unwrap();
    let at = lib.find("CommandGesture::").unwrap();
    let handler = lib[..at].rfind("fn ").map(|i| &lib[i..at]).unwrap();
    assert!(handler.starts_with("fn assistant_commands_activate"), "only Settings makes one: {}", &handler[..60.min(handler.len())]);
    assert!(!std::fs::read_to_string(src.join("ai_bus.rs")).unwrap().contains("set_command_execution"));
}

// ---------------------------------------------------------------- a real kernel

/// A system-agent turn from the pane's driver against a real `octos`
/// kernel and a scripted model (`crates/app-peers/tests/fixtures/mock_llm.py`,
/// no network, no keys): the answer streams in, the history reloads it, a
/// kernel restart resumes the same conversation. Runs when
/// `OCTOS_CORE_TEST_KERNEL` names an `octos` binary (see
/// crates/kernel/tests/real_kernel.rs); otherwise it says so and passes.
#[cfg(kernel)]
#[test]
fn a_system_agent_turn_from_the_pane_on_a_real_kernel() {
    use octosense_ai_host::kernel::{Core, Options};
    use std::io::BufRead;
    let Some(program) = std::env::var_os("OCTOS_CORE_TEST_KERNEL") else {
        eprintln!("OCTOS_CORE_TEST_KERNEL is not set: skipping the real-kernel chat test");
        return;
    };
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app-peers/tests/fixtures/mock_llm.py");
    let mut model = std::process::Command::new("python3").arg(script).stdout(std::process::Stdio::piped()).spawn().unwrap();
    let mut line = String::new();
    std::io::BufReader::new(model.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let port: u16 = line.trim().parse().unwrap();
    let dir = std::env::temp_dir().join(format!("syschat-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    std::fs::write(dir.join("profiles/_main.json"), json!({
        "id": "_main", "name": "Main", "enabled": true,
        "created_at": "2026-09-28T00:00:00Z", "updated_at": "2026-09-28T00:00:00Z",
        "config": {"llm": {"primary": {"family_id": "local", "model_id": "mock-model",
            "route": {"base_url": format!("http://127.0.0.1:{port}/v1"), "api_type": "openai"}}}}
    }).to_string()).unwrap();
    assert!(super::provider_configured(Some(&dir.join("profiles/_main.json"))));
    let core = Core::new(Options::default().program(std::path::PathBuf::from(program)).core_dir(&dir));
    struct CoreConnector(Core);
    impl Connector for CoreConnector {
        fn connect(&mut self) -> Result<Box<dyn Link>, Unavailable> {
            self.0.connect().map(super::link::link).map_err(|e| Unavailable::Failed(e.to_string()))
        }
    }
    let mut d = Driver::new(Box::new(CoreConnector(core.clone())));
    let until = |d: &mut Driver, what: &str, done: &dyn Fn(&Driver) -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !done(d) {
            assert!(std::time::Instant::now() < deadline, "timed out waiting for {what}: {:?} {:?}", d.model.phase(), d.model.items);
            d.step(Duration::from_millis(100));
        }
    };
    d.command(Command::Open);
    until(&mut d, "the session", &|d| d.model.phase() == &Phase::Ready);
    d.command(Command::Send("hello from the pane".into()));
    until(&mut d, "the answer", &|d| d.model.phase() == &Phase::Ready && text_of(&d.model, Role::Assistant).iter().any(|t| t.contains("ECHO: hello from the pane")));
    // The kernel restarts (a provider change, Settings' restart): the chat
    // reconnects and the same conversation's history comes back.
    assert!(core.restart());
    until(&mut d, "the reconnect", &|d| matches!(d.model.phase(), Phase::Reconnecting(_) | Phase::Connecting));
    until(&mut d, "the resumed session", &|d| d.model.phase() == &Phase::Ready && text_of(&d.model, Role::User).iter().any(|t| t == "hello from the pane")
        && text_of(&d.model, Role::Assistant).iter().any(|t| t.contains("ECHO: hello from the pane")));
    d.command(Command::Send("second".into()));
    until(&mut d, "the second answer", &|d| d.model.phase() == &Phase::Ready && text_of(&d.model, Role::Assistant).iter().any(|t| t.contains("ECHO: second")));
    drop(d);
    core.shutdown_within(Duration::from_secs(10));
    let _ = model.kill();
    let _ = model.wait();
    let _ = std::fs::remove_dir_all(dir);
}


// ---------------------------------------------------------------- host tools (UPCR-2026-035)

fn command_grant() -> Grants {
    let g = Grants::default();
    *g.0.lock().unwrap() = (vec![crate::host_tools::relay::terminal_run_declaration()], Some("peer-host-token".into()));
    g
}

#[test]
fn the_granted_command_tool_is_registered_on_the_system_session_and_withdrawn_when_the_switch_goes_off() {
    let (mut d, fake, grants) = driver_with(command_grant());
    d.command(Command::Open);
    settle(&mut d);
    let register = fake.sent("peer/tools/register");
    assert_eq!(register.len(), 1, "registered once the session is open");
    assert!(register[0].get("peer").is_none(), "the host session itself: no peer");
    assert_eq!(register[0]["session_id"], SYSTEM_SESSION);
    assert_eq!(register[0]["host_token"], "peer-host-token", "an app peer's credential");
    assert!(register[0].get("generic_tools").is_none(), "the system agent keeps its kernel tools");
    assert_eq!(register[0]["tools"][0]["name"], "terminal.run");
    assert_eq!(d.registered_tools(), Some(&["terminal.run".to_string()][..]));
    // Nothing changes, nothing is sent again.
    d.command(Command::SyncTools);
    settle(&mut d);
    assert_eq!(fake.sent("peer/tools/register").len(), 1);
    // The person turns it off: the set is withdrawn at once.
    grants.0.lock().unwrap().0.clear();
    d.command(Command::SyncTools);
    settle(&mut d);
    let register = fake.sent("peer/tools/register");
    assert_eq!(register.len(), 2);
    assert_eq!(register[1]["tools"], json!([]));
    assert_eq!(d.registered_tools(), Some(&[][..]));
}

#[test]
fn nothing_is_registered_without_a_grant_and_a_reconnect_registers_again() {
    let (mut d, fake) = opened();
    assert!(fake.sent("peer/tools/register").is_empty(), "no grant, no registration");
    let (mut d2, fake2, _) = driver_with(command_grant());
    d2.command(Command::Open);
    settle(&mut d2);
    fake2.s().close = Some(Closed { restarted: true, why: "restarted".into() });
    settle(&mut d2);
    std::thread::sleep(Duration::from_millis(250));
    settle(&mut d2);
    assert_eq!(fake2.sent("peer/tools/register").len(), 2, "the set lives with the connection: registered again");
    d.command(Command::Close);
}

#[test]
fn without_a_credential_the_grant_waits_and_says_why() {
    let grants = command_grant();
    grants.0.lock().unwrap().1 = None;
    let (mut d, fake, grants) = driver_with(grants);
    d.command(Command::Open);
    settle(&mut d);
    assert!(fake.sent("peer/tools/register").is_empty());
    assert!(d.model.items.iter().any(|i| matches!(i, Item::Notice(n) if n.contains("app's agent"))));
    // Once an app's agent exists, the next request registers first.
    grants.0.lock().unwrap().1 = Some("t".into());
    d.command(Command::Send("list my files".into()));
    settle(&mut d);
    let s = fake.s();
    let register = s.sent.iter().position(|(m, _)| m == "peer/tools/register").expect("registered");
    let turn = s.sent.iter().position(|(m, _)| m == "turn/start").unwrap();
    assert!(register < turn);
}

#[test]
fn the_system_agents_calls_go_to_the_relay_and_are_answered_once_on_this_link() {
    let (mut d, fake, _) = driver_with(command_grant());
    d.command(Command::Open);
    settle(&mut d);
    d.command(Command::Send("list my files".into()));
    let turn = fake.sent("turn/start")[0]["turn_id"].as_str().unwrap().to_string();
    let call = |id: &str, turn: &str| json!({"peer": null, "context_id": null, "turn_id": turn, "call_id": id, "tool_call_id": format!("tc-{id}"), "args_digest": "d",
        "name": "terminal.run", "app": "terminal", "caller": {"kind": "system", "peer": null, "session_id": SYSTEM_SESSION, "turn_id": turn},
        "args": {"command": "ls"}, "risk": "destructive", "confirm_required": false, "timeout_ms": 30000, "tools_version": 1});
    fake.notify("peer/tool/call", call("c1", &turn));
    settle(&mut d);
    let (tool_call, reply) = match d.effects.as_slice() {
        [Effect::ToolCall { call, reply }] => (call.clone(), reply.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(tool_call.caller_kind, crate::ai_host::app_peers::host_tools::CallerKind::System);
    assert_eq!(tool_call.args["command"], "ls");
    assert!(reply.finish(crate::ai_host::app_peers::host_tools::ToolOutcome::Ok(json!({"text": "typed"}))));
    settle(&mut d);
    let results = fake.sent("peer/tool/result");
    assert_eq!(results.len(), 1);
    assert!(results[0].get("peer").is_none(), "a host session set's result names no peer");
    assert_eq!(results[0]["host_token"], "peer-host-token");
    assert_eq!((results[0]["call_id"].as_str(), results[0]["ok"].as_bool()), (Some("c1"), Some(true)));

    // A cancel closes the call: its late answer is never sent.
    d.effects.clear();
    fake.notify("peer/tool/call", call("c2", &turn));
    settle(&mut d);
    let Some(Effect::ToolCall { reply, .. }) = d.effects.first().cloned() else { panic!() };
    fake.s().inbox.push_back(json!({"jsonrpc": "2.0", "method": "peer/tool/cancel", "params": {"call_id": "c2", "reason": "timeout"}}).to_string());
    settle(&mut d);
    assert!(d.effects.contains(&Effect::ToolCancel("c2".into())));
    assert!(!reply.finish(crate::ai_host::app_peers::host_tools::ToolOutcome::Ok(json!({}))));
    settle(&mut d);
    assert_eq!(fake.sent("peer/tool/result").len(), 1);

    // The person stops the turn: its in-flight call ends, and a late call of
    // it is refused (octos follow-up N1).
    d.effects.clear();
    fake.notify("peer/tool/call", call("c3", &turn));
    settle(&mut d);
    d.command(Command::Interrupt);
    assert!(d.effects.contains(&Effect::ToolCancel("c3".into())));
    fake.notify("peer/tool/call", call("c4", &turn));
    settle(&mut d);
    let results = fake.sent("peer/tool/result");
    assert_eq!(results.last().unwrap()["call_id"], "c4");
    assert_eq!(results.last().unwrap()["error"]["kind"], "turn_interrupted");
    assert!(!d.effects.iter().any(|e| matches!(e, Effect::ToolCall { call, .. } if call.call_id == "c4")));
}

#[test]
fn a_host_tool_approval_names_the_owning_app_the_tool_and_the_exact_arguments() {
    let (mut d, fake) = opened();
    d.command(Command::Send("list my files".into()));
    let turn = fake.sent("turn/start")[0]["turn_id"].clone();
    fake.notify("approval/requested", json!({"turn_id": turn, "approval_id": "a9", "tool_name": "terminal_run", "title": "Run", "body": "",
        "approval_kind": "host_tool", "typed_details": {"kind": "host_tool", "host_tool": {"app": "terminal", "tool": "terminal.run", "args": {"command": "ls -la"},
        "risk": "destructive", "outward": false, "calling_kind": "system", "calling_session_id": SYSTEM_SESSION, "outcome_unknown_before": false}}}));
    settle(&mut d);
    let Some(Effect::Approval(ask)) = d.effects.first().cloned() else { panic!("{:?}", d.effects) };
    assert_eq!((ask.tool.as_str(), ask.app.as_deref()), ("terminal.run", Some("terminal")));
    assert_eq!(ask.args, json!({"command": "ls -la"}));
    // The router takes it as a command: no rule answers it, the person sees it.
    use crate::approvals::{dev_hooks::FixedDevMode, Approvals, Route};
    let mut a = Approvals::memory();
    let context = crate::approvals::RequestContext { call_id: format!("{}{}", super::HELD_PREFIX, ask.approval_id), trigger: crate::approvals::Trigger::Person, ..Default::default() };
    let spec = crate::approvals::ToolSpec::host(&ask.tool).command();
    let route = a.router.request(crate::approvals::router::make_request(super::grants::COMMAND_APP, spec.clone(), ask.args.clone(), crate::approvals::Caller::SystemAgent, context.clone(), 1, 0), 1);
    assert!(matches!(route, Route::Sheet(_)), "{route:?}");
    // Developer mode answers it (ADR 0004 §13).
    a.router.set_hooks(Box::new(FixedDevMode::all()));
    let context = crate::approvals::RequestContext { call_id: "syschat:a10".into(), ..context };
    let route = a.router.request(crate::approvals::router::make_request(super::grants::COMMAND_APP, spec, ask.args.clone(), crate::approvals::Caller::SystemAgent, context, 1, 0), 1);
    assert!(matches!(route, Route::Approved(_)), "{route:?}");
}

// ---------------------------------------------------------------- external clients (G1)

/// Another client (Talk to Octos) runs a turn on the same session, and its
/// approval reaches this connection too.
fn external_turn_approval(fake: &Fake, approval: &str) {
    fake.notify("turn/started", json!({"turn_id": "talk-to-octos-1"}));
    fake.notify("approval/requested", json!({"turn_id": "talk-to-octos-1", "approval_id": approval, "tool_name": "write_file", "title": "Write", "body": "b"}));
}

#[test]
fn only_the_chats_own_turns_are_routed_as_the_system_agents() {
    let (mut d, fake) = opened();
    d.command(Command::Send("tidy my notes".into()));
    let own = fake.sent("turn/start")[0]["turn_id"].as_str().unwrap().to_string();
    fake.notify("approval/requested", json!({"turn_id": own, "approval_id": "mine", "tool_name": "write_file", "title": "t", "body": "b"}));
    external_turn_approval(&fake, "theirs");
    settle(&mut d);
    let asks: Vec<_> = d.effects.iter().filter_map(|e| match e { Effect::Approval(a) => Some(a.clone()), _ => None }).collect();
    assert_eq!(asks.len(), 2);
    let mine = asks.iter().find(|a| a.approval_id == "mine").unwrap();
    let theirs = asks.iter().find(|a| a.approval_id == "theirs").unwrap();
    assert!(!mine.external && theirs.external, "only the chat's own turn is its own");
    assert!(matches!(d.model.items.iter().find(|i| matches!(i, Item::Approval { id, .. } if id == "theirs")), Some(Item::Approval { state: ApprovalState::External, .. })));

    // What the router gets: the chat's own turn is the system agent's; the
    // other is an external caller on an external connection.
    use crate::approvals::{Caller, Connection, Trigger};
    let (_, _, _, caller, context) = super::approval_request(mine);
    assert_eq!((caller, context.connection, context.trigger), (Caller::SystemAgent, Connection::Host, Trigger::Person));
    let (_, _, _, caller, context) = super::approval_request(theirs);
    assert_eq!(caller, Caller::External { client: None });
    assert_eq!(context.connection, Connection::External);
    assert_ne!(context.trigger, Trigger::Person, "never 'triggered by the person'");
    assert!(context.batch.is_none(), "never batched with the system agent's request");
}

#[test]
fn an_external_turns_approval_is_never_auto_approved_or_answered() {
    use crate::approvals::{dev_hooks::FixedDevMode, rules::RuleDraft, rules::ApprovalGesture, Approvals, Route};
    let (mut d, fake) = opened();
    external_turn_approval(&fake, "theirs");
    settle(&mut d);
    let Some(Effect::Approval(ask)) = d.effects.first().cloned() else { panic!("{:?}", d.effects) };
    // Developer mode on for everything, and a time-boxed "everything" rule
    // on the system agent's app that also covers incoming content.
    let mut a = Approvals::memory();
    a.router.set_hooks(Box::new(FixedDevMode::all()));
    let mut everything = RuleDraft::everything(super::APP, 30);
    everything.include_incoming = true;
    a.router.create_rule(&ApprovalGesture::settings_tap(), everything, 1).unwrap();
    let (app, tool, args, caller, context) = super::approval_request(&ask);
    let route = a.router.request(crate::approvals::router::make_request(&app, tool, args, caller, context, 1, 0), 1);
    assert!(matches!(route, Route::LeftToClient(_)), "{route:?}");
    assert_eq!(a.router.pending(), 0);
    assert!(a.router.sheets().is_empty());
    assert!(crate::approvals::take_system_chat_decisions().is_empty());
    // Even a stray decision for it is not sent to the kernel.
    d.command(Command::Approval { approval_id: "theirs".into(), approve: true });
    assert!(fake.sent("approval/respond").is_empty(), "the shell never answers another client's approval");
    // The other client answers; the pane shows the outcome.
    fake.notify("approval/decided", json!({"turn_id": "talk-to-octos-1", "approval_id": "theirs", "decision": "approve"}));
    settle(&mut d);
    assert!(matches!(d.model.items.iter().find(|i| matches!(i, Item::Approval { id, .. } if id == "theirs")), Some(Item::Approval { state: ApprovalState::Approved, .. })));
}

#[test]
fn a_new_conversation_turn_is_the_chats_own() {
    let (mut d, fake) = opened();
    d.command(Command::NewConversation);
    let turn = fake.sent("turn/start")[0]["turn_id"].as_str().unwrap().to_string();
    assert!(d.model.is_own_turn(&turn));
    assert!(!d.model.is_own_turn("talk-to-octos-1"));
}

#[test]
fn only_the_chats_own_turns_calls_are_triggered_by_the_person() {
    use crate::ai_host::app_peers::TurnTrigger;
    let (mut d, fake) = opened();
    d.command(Command::Send("list my files".into()));
    let own = fake.sent("turn/start")[0]["turn_id"].as_str().unwrap().to_string();
    let call = |id: &str, turn: &str| json!({"peer": null, "context_id": null, "turn_id": turn, "call_id": id, "tool_call_id": format!("tc-{id}"), "args_digest": "d",
        "name": "terminal.run", "app": "terminal", "caller": {"kind": "system", "peer": null, "session_id": SYSTEM_SESSION, "turn_id": turn},
        "args": {"command": "ls"}, "risk": "destructive", "confirm_required": false, "timeout_ms": 30000, "tools_version": 1});
    fake.notify("peer/tool/call", call("c1", &own));
    fake.notify("peer/tool/call", call("c2", "talk-to-octos-1"));
    settle(&mut d);
    let triggers: Vec<(String, TurnTrigger)> = d.effects.iter().filter_map(|e| match e { Effect::ToolCall { call, .. } => Some((call.call_id.clone(), call.trigger.clone())), _ => None }).collect();
    assert_eq!(triggers, vec![("c1".to_string(), TurnTrigger::Person), ("c2".to_string(), TurnTrigger::Unknown)]);
}


/// G12: `terminal.run` is registered on the system session only when the
/// person granted command execution AND the Terminal runs as a process on
/// this device; granted without one, nothing is registered.
#[test]
fn terminal_run_needs_a_process_terminal_even_when_granted() {
    use super::grants::{host_tools_given, COMMAND_TOOL};
    assert!(host_tools_given(true, true).contains(COMMAND_TOOL));
    assert!(host_tools_given(true, false).is_empty(), "granted, but the Terminal runs in-process here");
    assert!(host_tools_given(false, true).is_empty());
    assert!(host_tools_given(false, false).is_empty());
}
