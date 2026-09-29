//! The question model: routing by who started the turn, consumers, the one
//! answer path, and that nothing an app reaches can answer.

use super::*;
use crate::ai_host::app_peers::host_tools::{AgentQuestion, CallOrigin, QuestionAnswer, QuestionReply, TurnOrigin};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

type Sent = Arc<Mutex<Vec<Value>>>;

fn question(id: &str, turn: &str, origin: CallOrigin, context: Option<&str>) -> AgentQuestion {
    let mut q = AgentQuestion::parse(
        &json!({"question_id": id, "turn_id": turn, "title": "Which room?", "body": "Pick one",
            "questions": [{"header": "Room", "question": "Post where?", "options": [{"label": "#a", "description": ""}, {"label": "#b", "description": ""}]}]}),
        "_main:api:octosense#peer-rinx-1",
    )
    .unwrap();
    q.origin = origin;
    // As the broker derives it until octos reports it.
    q.turn_origin = match origin {
        CallOrigin::PeerInput => TurnOrigin::SystemAgent,
        CallOrigin::Context => TurnOrigin::Person,
        _ => TurnOrigin::App,
    };
    q.context_id = context.map(str::to_string);
    q.client = context.map(|_| "mini.news#1".to_string());
    q
}

fn answer_handle() -> (QuestionAnswer, Sent) {
    let sent: Sent = Arc::default();
    let s = sent.clone();
    (QuestionAnswer::new(move |v| s.lock().unwrap().push(v)), sent)
}

struct Recorder(Arc<Mutex<Vec<(u64, Conversation, State)>>>);
impl Consumer for Recorder {
    fn changed(&mut self, r: &Request) {
        self.0.lock().unwrap().push((r.id, r.conversation.clone(), r.state.clone()));
    }
}

/// ADR 0004 §6: a context's or the peer's own turn asks in the app's
/// conversation; a `peer/input` turn (the system agent's) in the system
/// chat. Every consumer hears each change.
#[test]
fn a_question_is_routed_by_who_started_the_turn_and_every_consumer_hears_it() {
    let mut model = Questions::default();
    let heard = Arc::new(Mutex::new(Vec::new()));
    model.subscribe(Box::new(Recorder(heard.clone())));
    let (a1, _) = answer_handle();
    let (a2, _) = answer_handle();
    let (a3, _) = answer_handle();
    let by_person = model.requested("rinx", Some("@a:x"), question("q1", "t1", CallOrigin::Context, Some("ctx1")), a1);
    let by_app = model.requested("rinx", Some("@a:x"), question("q2", "t2", CallOrigin::PeerOwn, None), a2);
    let by_system = model.requested("card.os.news", None, question("q3", "t3", CallOrigin::PeerInput, None), a3);
    let r = model.get(by_person).unwrap();
    assert_eq!((r.origin, &r.conversation), (Origin::Person, &Conversation::App("rinx".into())));
    assert_eq!(r.client.as_deref(), Some("mini.news#1"));
    assert_eq!(model.get(by_app).unwrap().conversation, Conversation::App("rinx".into()));
    let r = model.get(by_system).unwrap();
    assert_eq!((r.origin, &r.conversation), (Origin::SystemAgent, &Conversation::SystemChat));
    assert!(!r.origin_reported);
    assert_eq!(r.app, "os.news", "a script app's peer names its app");
    assert_eq!(model.open(&Conversation::SystemChat).len(), 1);
    assert_eq!(model.open_in_apps().len(), 2);
    assert_eq!(heard.lock().unwrap().len(), 3);
    assert_eq!(r.options(), vec!["#a".to_string(), "#b".to_string()]);
}

/// The TURN's origin routes a question, not its session: on the peer's one
/// shared conversation a person's turn asks in the app, a system agent's
/// turn in the system chat; an origin octos reports wins over the
/// derivation.
#[test]
fn a_question_follows_its_turns_origin_not_its_session() {
    let mut model = Questions::default();
    let mut person_on_peer = question("q1", "t1", CallOrigin::PeerOwn, None);
    person_on_peer.turn_origin = TurnOrigin::Person;
    person_on_peer.origin_reported = true;
    let mut system_on_peer = question("q2", "t2", CallOrigin::PeerOwn, None);
    system_on_peer.turn_origin = TurnOrigin::SystemAgent;
    system_on_peer.origin_reported = true;
    let (a1, _) = answer_handle();
    let (a2, _) = answer_handle();
    let person = model.requested("rinx", None, person_on_peer, a1);
    let system = model.requested("rinx", None, system_on_peer, a2);
    assert_eq!(model.get(person).unwrap().conversation, Conversation::App("rinx".into()));
    assert_eq!(model.get(system).unwrap().conversation, Conversation::SystemChat);
    assert!(model.get(system).unwrap().origin_reported);
}

#[test]
fn the_persons_answer_is_sent_once_and_a_closed_question_takes_none() {
    let mut model = Questions::default();
    let heard = Arc::new(Mutex::new(Vec::new()));
    model.subscribe(Box::new(Recorder(heard.clone())));
    let (answer, sent) = answer_handle();
    let id = model.requested("rinx", Some("@a:x"), question("q1", "t1", CallOrigin::PeerOwn, None), answer);
    let person = PersonAnswer::from_shell_surface();
    assert!(model.answer(id, &[], &person).is_err(), "an answer is needed");
    assert!(model.answer(id, &[QuestionReply::option("#a"), QuestionReply::option("#b")], &person).is_err(), "one question, one answer");
    model.answer(id, &[QuestionReply::option("#b")], &person).unwrap();
    assert_eq!(*sent.lock().unwrap(), vec![json!([{"selected_labels": ["#b"]}])]);
    assert!(model.answer(id, &[QuestionReply::option("#a")], &person).is_err(), "answered once");
    assert_eq!(model.get(id).unwrap().state, State::Answered("#b".into()));
    // The turn ended before the person answered: closed, nothing sent.
    let (answer, sent) = answer_handle();
    let id = model.requested("rinx", Some("@a:x"), question("q2", "t2", CallOrigin::PeerOwn, None), answer);
    model.closed("rinx", "q2");
    assert_eq!(model.get(id).unwrap().state, State::Closed);
    assert!(model.answer(id, &[QuestionReply::text("late")], &person).is_err());
    assert!(sent.lock().unwrap().is_empty());
    assert!(model.open_in_apps().is_empty());
    let states: Vec<State> = heard.lock().unwrap().iter().map(|(_, _, s)| s.clone()).collect();
    assert_eq!(states, [State::Open, State::Answered("#b".into()), State::Open, State::Closed]);
}

/// The system chat is a consumer: a `peer/input` turn's question shows in
/// its conversation, and the person's answer from the pane goes back to
/// the app peer's turn through the model (never the chat's own session).
#[test]
fn the_system_chat_shows_the_system_agents_questions_and_answers_them_through_the_model() {
    crate::system_chat::subscribe_questions();
    let (answer, sent) = answer_handle();
    let id = super::requested("rinx", Some("@a:x"), question("q-chat", "t-chat", CallOrigin::PeerInput, None), answer);
    let routed = format!("{}{id}", crate::system_chat::ROUTED_PREFIX);
    let model = crate::system_chat::snapshot();
    let shown = model.items.iter().any(|i| matches!(i, crate::system_chat::model::Item::Question { id: q, answered: None, .. } if *q == routed));
    assert!(shown, "{:?}", model.items);
    crate::system_chat::answer_option(&routed, 1, "#a");
    assert_eq!(*sent.lock().unwrap(), vec![json!([{"selected_labels": ["#a"]}])]);
    assert_eq!(super::get(id).unwrap().state, State::Answered("#a".into()));
    let model = crate::system_chat::snapshot();
    assert!(model.items.iter().any(|i| matches!(i, crate::system_chat::model::Item::Question { id: q, answered: Some(_), .. } if *q == routed)));
}

/// Only the shell's own surfaces make a [`PersonAnswer`] and answer: no
/// host service, executor, peer link or app-facing code does.
#[test]
fn only_the_shells_surfaces_answer_questions() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let allowed = ["questions/mod.rs", "questions/tests.rs", "system_chat/mod.rs", "approvals/view.rs"];
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            if text.contains("PersonAnswer::from_shell_surface(") || text.contains("questions::answer(") {
                found.push(path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    for file in &found {
        assert!(allowed.contains(&file.as_str()), "{file} answers agents' questions; only the shell's surfaces may");
    }
}

/// What an app reaches cannot answer a question (or an approval): the peer
/// link has no such method, and the `octos` host service refuses it.
#[test]
fn no_app_facing_path_can_answer() {
    for method in ["octos.user_question.respond", "octos.question.answer", "octos.approval.respond", "user_question/respond", "approval/respond"] {
        let frame = json!({"octos_peer": {"up": "request", "req_id": 1, "method": method, "args": {"question_id": "q1", "answers": []}}}).to_string();
        assert_eq!(crate::peer_link::wire::Up::parse(&frame), None, "the peer link carries no {method}");
        assert!(crate::ai_host::contained::parse(method, &json!({})).is_err(), "the octos host service has no {method}");
    }
}


/// ADR 0004 §8: a question nobody answers within the deadline is declined
/// (free text saying it expired, never an option chosen for the person),
/// shown expired rather than removed, and takes no late answer.
#[test]
fn a_question_expires_declined_and_shows_as_expired() {
    let mut model = Questions { deadline_s: Some(600), ..Questions::default() };
    let heard = Arc::new(Mutex::new(Vec::new()));
    model.subscribe(Box::new(Recorder(heard.clone())));
    let (answer, sent) = answer_handle();
    let id = model.requested_at("rinx", Some("@a:x"), question("q1", "t1", CallOrigin::Context, Some("ctx")), answer, 1_000);
    assert!(!model.tick(1_599));
    assert!(model.tick(1_600));
    let r = model.get(id).unwrap().clone();
    assert_eq!(r.state, State::Expired("no answer in 10 min".into()));
    let sent = sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert!(sent[0][0].get("selected_labels").is_none());
    assert!(sent[0][0]["free_text"].as_str().unwrap().contains("expired, no answer in 10 min"));
    assert!(model.answer(id, &[QuestionReply::option("#a")], &PersonAnswer::from_shell_surface()).is_err(), "no late answer");
    assert!(model.open_in_apps().is_empty(), "no longer asked");
    assert_eq!(model.expired_in_apps().len(), 1, "still shown, as expired");
    assert_eq!(heard.lock().unwrap().last().unwrap().2, State::Expired("no answer in 10 min".into()));
    model.dismiss(id);
    assert!(model.expired_in_apps().is_empty());
}

#[test]
fn stop_declines_the_stopped_apps_open_questions() {
    let mut model = Questions::default();
    let (a1, sent1) = answer_handle();
    let (a2, sent2) = answer_handle();
    let rinx = model.requested("rinx", None, question("q1", "t1", CallOrigin::PeerOwn, None), a1);
    let news = model.requested("card.os.news", None, question("q2", "t2", CallOrigin::PeerOwn, None), a2);
    assert_eq!(model.stop_agent("rinx"), 1);
    assert_eq!(model.get(rinx).unwrap().state, State::Answered("(stopped)".into()));
    assert!(sent1.lock().unwrap()[0][0]["free_text"].as_str().unwrap().contains("stopped"));
    assert_eq!(model.get(news).unwrap().state, State::Open);
    assert!(sent2.lock().unwrap().is_empty());
}
