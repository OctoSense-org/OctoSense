//! "Ask <app>" and an agent for every app (ADR 0004 §4, §6): which apps
//! have an agent, the conversation's two lanes with their speakers, the
//! panel on a fake peer (it opens a sharing context and sends the person's
//! turns there; Stop; revocation), and what the system agent is told.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::model::{Conversation, LANE_PERSON, LANE_SYSTEM_AGENT};
use crate::ai_host::app_peers::{Availability, ContextEvent, ContextOp, ContextSpec, Deployment, EventSink, ModelInfo, OctosAppService, OctosContext, SettingsEntry, TurnTrigger, OCTOS_SERVICES};
use crate::apps::AgentApp;
use crate::system_chat::model::{Item, Role};

fn labels(c: &Conversation) -> Vec<(String, String)> {
    c.chat
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Message { speaker, text, .. } => Some((speaker.clone().unwrap_or_default(), text.clone())),
            _ => None,
        })
        .collect()
}

fn env(lane: &str, turn: &str, cursor: u64, kind: &str, data: Value) -> Value {
    json!({"method": "projection/envelope", "lane": lane,
        "params": {"turn_id": turn, "cursor": {"seq": cursor}, "payload": {"type": kind, "data": data}}})
}

// ---------------------------------------------------------------- which apps

/// News ships `tools.json` (G3) and declares no `octos.*`: it has an agent.
/// An app with neither tools nor an agent block has none; a manifest's
/// `agent` block is enough.
#[test]
fn news_is_listed_as_having_an_agent() {
    let dir = std::env::temp_dir().join(format!("octosense-ask-apps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    #[cfg(any(feature = "app-hub", native_mobile))]
    let news = crate::host_tools::script_apps::tests::stamped_bundle("news", "ask", |_, _| {});
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    let news = {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/news/bundle");
        let d = dir.join("news");
        std::fs::create_dir_all(&d).unwrap();
        for f in ["manifest.json", "tools.json"] {
            std::fs::copy(src.join(f), d.join(f)).unwrap();
        }
        d
    };
    let app = crate::apps::script_agent_app(&news.join("manifest.json"), "os.news", "News").expect("News has an agent");
    assert!(app.octos.is_empty(), "it declares no octos.* service");
    assert!(!app.native);
    assert_eq!(crate::agents::peer_of(&app), "card.os.news");
    // No tools, no agent block, no octos.*: no agent.
    let clock = dir.join("clock");
    std::fs::create_dir_all(&clock).unwrap();
    std::fs::write(clock.join("manifest.json"), r#"{"id":"org.example.clock","capabilities":["storage"]}"#).unwrap();
    assert!(crate::apps::script_agent_app(&clock.join("manifest.json"), "org.example.clock", "Clock").is_none());
    std::fs::write(clock.join("manifest.json"), r#"{"id":"org.example.clock","capabilities":["storage"],"agent":{"profile":"read-only"}}"#).unwrap();
    assert!(crate::apps::script_agent_app(&clock.join("manifest.json"), "org.example.clock", "Clock").is_some(), "an agent block declares one");
    // Found by what the shell knows of a window: its launcher id or name.
    let apps = vec![app.clone()];
    for name in ["os.news", "news", "News", "card.os.news", "hub:os.news"] {
        assert_eq!(crate::agents::find_in(&apps, name).map(|a| a.id), Some("os.news".to_string()), "{name}");
    }
    // Native apps come from native-apps.json (Rinx's agent block).
    assert!(crate::apps::agent_apps().iter().any(|a| a.id == "rinx" && a.native));
    let _ = std::fs::remove_dir_all(&news);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The consent store tells the shell which agents were just allowed (it
/// prepares their peers); turning one off takes it back.
#[test]
fn allowing_an_agent_queues_its_preparation() {
    let mut store = crate::approvals::consent::ConsentStore::memory();
    let tap = crate::approvals::rules::ApprovalGesture::sheet_tap();
    store.set(&tap, "os.news", true, 1);
    store.set(&tap, "org.example.trip", true, 1);
    store.turn_off("org.example.trip", 2);
    assert_eq!(store.take_allowed(), ["os.news".to_string()]);
    assert!(store.take_allowed().is_empty(), "once");
    assert_eq!(store.take_revoked(), ["org.example.trip".to_string()]);
}

// ---------------------------------------------------------------- the lanes

/// Both lanes, merged, each message with who spoke: the person's turn in
/// the panel, the system agent's `peer/input` turn, and the agent's
/// answers in each lane. The panel is busy while either lane runs.
#[test]
fn both_lanes_show_with_their_speakers_and_run_in_parallel() {
    let mut c = Conversation::new("News");
    // The system agent asks for a digest (its lane).
    c.apply(&json!({"method": "turn/started", "lane": LANE_SYSTEM_AGENT, "params": {"turn_id": "s1"}}));
    c.apply(&json!({"method": "projection/envelope", "lane": LANE_SYSTEM_AGENT, "speaker": {"kind": "system_agent"}, "display_text": "Give me today's digest",
        "params": {"turn_id": "s1", "cursor": {"seq": 10}, "payload": {"type": "user_message", "data": {"text": "[from the system agent] Give me today's digest"}}}}));
    c.apply(&env(LANE_SYSTEM_AGENT, "s1", 11, "assistant_delta", json!({"assistant_segment_id": "s1:1", "text": "Working on the digest"})));
    assert!(c.busy());
    // Meanwhile the person asks in the panel (their lane).
    c.apply(&json!({"method": "projection/envelope", "lane": LANE_PERSON, "speaker": {"kind": "person", "label": "News"},
        "params": {"turn_id": "p1", "cursor": {"seq": 12}, "payload": {"type": "user_message", "data": {"text": "[from the person: News] what are you working on?"}}}}));
    c.apply(&env(LANE_PERSON, "p1", 13, "assistant_persisted", json!({"assistant_segment_id": "p1:1", "text": "A digest for the system agent."})));
    c.apply(&env(LANE_PERSON, "p1", 14, "turn_terminal", json!({"outcome": "completed"})));
    assert!(c.busy(), "the system agent's lane still runs");
    c.apply(&env(LANE_SYSTEM_AGENT, "s1", 15, "assistant_persisted", json!({"assistant_segment_id": "s1:1", "text": "Working on the digest: 3 stories."})));
    c.apply(&env(LANE_SYSTEM_AGENT, "s1", 16, "turn_terminal", json!({"outcome": "completed"})));
    assert!(!c.busy());
    assert_eq!(
        labels(&c),
        [
            ("System agent".to_string(), "Give me today's digest".to_string()),
            ("News's agent, to the system agent".to_string(), "Working on the digest: 3 stories.".to_string()),
            ("You".to_string(), "what are you working on?".to_string()),
            ("News's agent".to_string(), "A digest for the system agent.".to_string()),
        ]
    );
    // A late envelope of an ended turn does not make it run again.
    c.apply(&json!({"method": "projection/envelope", "lane": LANE_PERSON, "speaker": {"kind": "person"},
        "params": {"turn_id": "p1", "cursor": {"seq": 12}, "payload": {"type": "user_message", "data": {"text": "what are you working on?"}}}}));
    assert!(!c.busy());
}

/// The merged history (both transcripts, by time) shows the same speakers.
#[test]
fn the_merged_history_names_its_speakers() {
    let mut c = Conversation::new("News");
    c.load_history(&json!([
        {"role": "user", "content": "[from the system agent] digest please", "lane": "system_agent", "speaker": {"kind": "system_agent"}, "display_text": "digest please", "thread_id": "s1"},
        {"role": "user", "content": "[from the person: News] focus on tech", "lane": "person", "speaker": {"kind": "person", "label": "News"}, "display_text": "focus on tech", "thread_id": "p1"},
        {"role": "assistant", "content": "Tech it is.", "lane": "person", "thread_id": "p1"},
        {"role": "assistant", "content": "Here is the tech digest.", "lane": "system_agent", "thread_id": "s1"},
        {"role": "user", "content": "[from the app] refresh", "lane": "person", "thread_id": "a1"},
    ]));
    assert_eq!(
        labels(&c),
        [
            ("System agent".to_string(), "digest please".to_string()),
            ("You".to_string(), "focus on tech".to_string()),
            ("News's agent".to_string(), "Tech it is.".to_string()),
            ("News's agent, to the system agent".to_string(), "Here is the tech digest.".to_string()),
            ("You".to_string(), "refresh".to_string()),
        ]
    );
}

/// The transcript as the panel draws it, row by row: speakers, notices,
/// tool rows.
fn transcript(c: &Conversation) -> Vec<String> {
    c.chat
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Message { speaker, text, .. } => Some(format!("{}: {text}", speaker.clone().unwrap_or_default())),
            Item::Notice(n) => Some(format!("({n})")),
            Item::Tool { name, .. } => Some(format!("[{name}]")),
            _ => None,
        })
        .collect()
}

fn started(lane: &str, turn: &str, text: &str, kind: &str) -> Value {
    json!({"method": "turn/started", "lane": lane, "speaker": {"kind": kind}, "request": {"text": text, "speaker": {"kind": kind}}, "params": {"turn_id": turn}})
}

/// A reload (the panel reopened) puts the notices back where they stood,
/// after the row they followed, not below every newer row. The instrument
/// run's order: the person's turn; the system agent's turn and the person's
/// second one; the person's Stop ("Stopped." and its end), Ctrl+. ("Nothing
/// of yours was running."), the row's Stop ("Stopped the system agent's
/// task." and its end); then, with the panel closed, the system agent's
/// next turn. After the reopen the notices stood below that last turn.
#[test]
fn a_reload_keeps_the_notices_where_they_stood() {
    let mut c = Conversation::new("News");
    c.apply(&started(LANE_PERSON, "p1", "Hello News", "person"));
    c.apply(&env(LANE_PERSON, "p1", 2, "assistant_persisted", json!({"assistant_segment_id": "p1:1", "text": "SLOW DONE after 15 s"})));
    c.apply(&env(LANE_PERSON, "p1", 3, "turn_terminal", json!({"outcome": "completed"})));
    c.apply(&started(LANE_SYSTEM_AGENT, "s1", "SLOW:45", "system_agent"));
    c.apply(&started(LANE_PERSON, "p2", "SLOW:40", "person"));
    c.notice("Stopped.");
    c.apply(&env(LANE_PERSON, "p2", 5, "turn_terminal", json!({"outcome": "interrupted", "error": {"message": "turn interrupted by client"}})));
    c.notice("Nothing of yours was running.");
    c.notice("Stopped the system agent's task.");
    c.apply(&env(LANE_SYSTEM_AGENT, "s1", 900, "turn_terminal", json!({"outcome": "interrupted", "error": {"message": "turn interrupted by client"}})));
    // The panel is closed; its follower still hears the system agent's turn.
    c.apply(&started(LANE_SYSTEM_AGENT, "s2", "SLOW:3", "system_agent"));
    c.apply(&env(LANE_SYSTEM_AGENT, "s2", 902, "assistant_persisted", json!({"assistant_segment_id": "s2:1", "text": "SLOW DONE after 3 s"})));
    c.apply(&env(LANE_SYSTEM_AGENT, "s2", 903, "turn_terminal", json!({"outcome": "completed"})));
    let live = transcript(&c);
    assert_eq!(
        live,
        [
            "You: Hello News",
            "News's agent: SLOW DONE after 15 s",
            "System agent: SLOW:45",
            "You: SLOW:40",
            "(Stopped.)",
            "(turn interrupted by client)",
            "(Nothing of yours was running.)",
            "(Stopped the system agent's task.)",
            "(turn interrupted by client)",
            "System agent: SLOW:3",
            "News's agent, to the system agent: SLOW DONE after 3 s",
        ]
    );
    // Reopened: the merged history as the broker sends it. Every row a turn
    // persisted carries its turn as `thread_id`; the two stopped turns the
    // kernel never recorded are the broker's rows, with `turn_id`.
    let history = json!([
        {"role": "user", "content": "[from the person: News] Hello News", "lane": "person", "speaker": {"kind": "person"}, "display_text": "Hello News", "thread_id": "p1"},
        {"role": "assistant", "content": "SLOW DONE after 15 s", "lane": "person", "thread_id": "p1"},
        {"role": "user", "content": "SLOW:45", "display_text": "SLOW:45", "speaker": {"kind": "system_agent"}, "lane": "system_agent", "turn_id": "s1", "unrecorded": true},
        {"role": "user", "content": "SLOW:40", "display_text": "SLOW:40", "speaker": {"kind": "person"}, "lane": "person", "turn_id": "p2", "unrecorded": true},
        {"role": "user", "content": "[from the system agent] SLOW:3", "lane": "system_agent", "speaker": {"kind": "system_agent"}, "display_text": "SLOW:3", "thread_id": "s2"},
        {"role": "assistant", "content": "SLOW DONE after 3 s", "lane": "system_agent", "thread_id": "s2"},
    ]);
    c.load_history(&history);
    assert_eq!(transcript(&c), live, "the reload keeps every notice in its place");
    c.load_history(&history);
    assert_eq!(transcript(&c), live, "and so does the next one");
    // A notice from before any row stays first; one after rows the history
    // no longer has goes last.
    let mut c = Conversation::new("News");
    c.notice("Could not load the conversation: closed");
    c.apply(&started(LANE_PERSON, "gone", "a turn the history lost", "person"));
    c.apply(&env(LANE_PERSON, "gone", 2, "turn_terminal", json!({"outcome": "completed"})));
    c.notice("Stopped.");
    c.load_history(&json!([{"role": "user", "content": "earlier", "lane": "person", "thread_id": "p0"}]));
    assert_eq!(transcript(&c), ["(Could not load the conversation: closed)", "You: earlier", "(Stopped.)"]);
}

/// The history's tool rows carry no tool name (octos's hydrate rows have
/// none), so a reload showed "⚙ tool · done". A row keeps the name the
/// panel showed for the same turn's call; a turn the panel never followed
/// stays "tool" (never another call's name).
#[test]
fn a_reload_keeps_the_tool_names_the_panel_showed() {
    let mut c = Conversation::new("News");
    c.apply(&started(LANE_PERSON, "p1", "SCN_ASK", "person"));
    c.apply(&env(LANE_PERSON, "p1", 2, "tool_start", json!({"tool_call_id": "c1", "name": "ask_user_question"})));
    c.apply(&env(LANE_PERSON, "p1", 3, "tool_end", json!({"tool_call_id": "c1", "status": "complete"})));
    c.apply(&env(LANE_PERSON, "p1", 4, "assistant_persisted", json!({"assistant_segment_id": "p1:1", "text": "SCN AUDIENCE Team"})));
    c.apply(&env(LANE_PERSON, "p1", 5, "turn_terminal", json!({"outcome": "completed"})));
    c.load_history(&json!([
        {"role": "user", "content": "[from the person: News] older", "lane": "person", "speaker": {"kind": "person"}, "display_text": "older", "thread_id": "p0"},
        {"role": "assistant", "content": "", "lane": "person", "thread_id": "p0"},
        {"role": "tool", "content": "{\"ok\":true}", "lane": "person", "thread_id": "p0"},
        {"role": "user", "content": "[from the person: News] SCN_ASK", "lane": "person", "speaker": {"kind": "person"}, "display_text": "SCN_ASK", "thread_id": "p1"},
        {"role": "assistant", "content": "", "lane": "person", "thread_id": "p1"},
        {"role": "tool", "content": "{\"ok\":true,\"kind\":\"user_question_answer\"}", "lane": "person", "thread_id": "p1"},
        {"role": "assistant", "content": "SCN AUDIENCE Team", "lane": "person", "thread_id": "p1"},
    ]));
    assert_eq!(transcript(&c), ["You: older", "[tool]", "You: SCN_ASK", "[ask_user_question]", "News's agent: SCN AUDIENCE Team"]);
}

/// The live run's order (#184 follow-up): each lane counts its own
/// `cursor.seq` (the peer's session is long, the person's context new), and
/// the kernel sends a turn's user message when the turn ENDS. Replayed as
/// it arrived: the system agent asks, the person asks while its answer
/// streams, the person asks again and stops. Each request stands where its
/// turn started, within a lane in ledger order (an envelope that arrives
/// after a later one of its lane goes before it), and the stopped turn's
/// request, which the kernel never records, stays.
#[test]
fn two_lanes_interleave_by_arrival_and_a_stopped_request_stays() {
    let mut c = Conversation::new("News");
    let sys = LANE_SYSTEM_AGENT;
    let started = |lane: &str, turn: &str, text: &str, kind: &str| {
        json!({"method": "turn/started", "lane": lane, "speaker": {"kind": kind},
            "request": {"text": text, "speaker": {"kind": kind}}, "params": {"turn_id": turn}})
    };
    let user = |lane: &str, turn: &str, cursor: u64, text: &str, kind: &str| {
        json!({"method": "projection/envelope", "lane": lane, "speaker": {"kind": kind}, "display_text": text,
            "params": {"turn_id": turn, "cursor": {"seq": cursor}, "payload": {"type": "user_message", "data": {"text": text}}}})
    };
    // The system agent's digest (the peer's ledger is far along).
    c.apply(&started(sys, "s1", "Please provide a digest", "system_agent"));
    c.apply(&env(sys, "s1", 1001, "assistant_delta", json!({"assistant_segment_id": "s1:1", "text": "Gathering"})));
    c.apply(&env(sys, "s1", 1003, "tool_start", json!({"tool_call_id": "t1", "name": "news_top"})));
    // Arrives after its lane's later tool_start: goes before it.
    c.apply(&env(sys, "s1", 1002, "assistant_delta", json!({"assistant_segment_id": "s1:2", "text": "Top stories"})));
    // The person asks meanwhile (a new context: small seqs).
    c.apply(&started(LANE_PERSON, "p1", "focus the digest on technology", "person"));
    c.apply(&env(LANE_PERSON, "p1", 3, "assistant_persisted", json!({"assistant_segment_id": "p1:1", "text": "Tech only."})));
    c.apply(&env(LANE_PERSON, "p1", 5, "turn_terminal", json!({"outcome": "completed"})));
    // The kernel records the person's words at the end.
    c.apply(&user(LANE_PERSON, "p1", 4, "focus the digest on technology", "person"));
    c.apply(&env(sys, "s1", 1700, "assistant_persisted", json!({"assistant_segment_id": "s1:2", "text": "Top stories: three."})));
    c.apply(&user(sys, "s1", 1706, "Please provide a digest", "system_agent"));
    c.apply(&env(sys, "s1", 1707, "turn_terminal", json!({"outcome": "completed"})));
    // A long request, stopped: no user message ever comes.
    c.apply(&started(LANE_PERSON, "p2", "a long digest please", "person"));
    c.apply(&env(LANE_PERSON, "p2", 7, "assistant_delta", json!({"assistant_segment_id": "p2:1", "text": "Working"})));
    c.apply(&env(LANE_PERSON, "p2", 8, "turn_terminal", json!({"outcome": "interrupted", "error": {"message": "turn interrupted by client"}})));
    assert!(!c.busy());
    assert_eq!(
        labels(&c),
        [
            ("System agent".to_string(), "Please provide a digest".to_string()),
            ("News's agent, to the system agent".to_string(), "Gathering".to_string()),
            ("News's agent, to the system agent".to_string(), "Top stories: three.".to_string()),
            ("You".to_string(), "focus the digest on technology".to_string()),
            ("News's agent".to_string(), "Tech only.".to_string()),
            ("You".to_string(), "a long digest please".to_string()),
            ("News's agent".to_string(), "Working".to_string()),
        ]
    );
    let tool = c.chat.items.iter().position(|i| matches!(i, Item::Tool { call_id, .. } if call_id == "t1")).unwrap();
    let second = c.chat.items.iter().position(|i| matches!(i, Item::Message { segment: Some(s), .. } if s == "s1:2")).unwrap();
    assert!(second < tool, "ledger order within the lane: {:?}", c.chat.items);
    assert!(matches!(c.chat.items.last(), Some(Item::Notice(n)) if n == "turn interrupted by client"));
}

// ---------------------------------------------------------------- the panel

struct FakeContext {
    ops: Mutex<Vec<ContextOp>>,
    open: AtomicBool,
    follower: Mutex<Option<EventSink>>,
    /// The next turn is stopped: it starts, ends interrupted, and the
    /// send fails with the same words (as the broker does).
    stop_next: AtomicBool,
}

impl OctosContext for FakeContext {
    fn call(&self, op: ContextOp, sink: EventSink) -> Result<(), String> {
        if !self.open.load(Ordering::SeqCst) {
            return Err("closed".into());
        }
        self.ops.lock().unwrap().push(op.clone());
        match op {
            ContextOp::History => {
                // Both lanes, as the broker merges them: the system agent's
                // row, then what the person said in THIS context.
                let mut rows = vec![json!({"role": "user", "content": "[from the system agent] earlier", "lane": "system_agent", "speaker": {"kind": "system_agent"}, "display_text": "earlier"})];
                for op in self.ops.lock().unwrap().iter() {
                    if let ContextOp::TurnFrom { text, .. } = op {
                        rows.push(json!({"role": "user", "content": text, "lane": "person", "speaker": {"kind": "person"}}));
                        rows.push(json!({"role": "assistant", "content": "Noted.", "lane": "person"}));
                    }
                }
                sink(ContextEvent::Complete(Ok(json!({"messages": rows}))))
            }
            ContextOp::TurnFrom { text, .. } if self.stop_next.swap(false, Ordering::SeqCst) => {
                let follower = self.follower.lock().unwrap().clone();
                let events = [
                    json!({"method": "turn/started", "lane": "person", "speaker": {"kind": "person"}, "request": {"text": text, "speaker": {"kind": "person"}},
                        "params": {"turn_id": "p10"}}),
                    env("person", "p10", 101, "turn_terminal", json!({"outcome": "interrupted", "error": {"message": "turn interrupted by client"}})),
                ];
                for event in events {
                    // The caller's own turn reaches its sink too.
                    sink(ContextEvent::Data(event.clone()));
                    if let Some(follower) = &follower {
                        follower(ContextEvent::Data(event));
                    }
                }
                sink(ContextEvent::Complete(Err("turn interrupted by client".into())));
            }
            ContextOp::TurnFrom { text, .. } => {
                // The kernel's events reach the follower, then the answer.
                if let Some(follower) = self.follower.lock().unwrap().clone() {
                    follower(ContextEvent::Data(json!({"method": "projection/envelope", "lane": "person", "speaker": {"kind": "person"}, "display_text": text,
                        "params": {"turn_id": "p9", "cursor": {"seq": 90}, "payload": {"type": "user_message", "data": {"text": text}}}})));
                    follower(ContextEvent::Data(env("person", "p9", 91, "assistant_persisted", json!({"assistant_segment_id": "p9:1", "text": "Noted."}))));
                    follower(ContextEvent::Data(env("person", "p9", 92, "turn_terminal", json!({"outcome": "completed"}))));
                }
                sink(ContextEvent::Complete(Ok(json!({"text": "Noted."}))));
            }
            _ => sink(ContextEvent::Complete(Ok(json!({})))),
        }
        Ok(())
    }
    fn close(&self) {
        self.open.store(false, Ordering::SeqCst);
    }
    fn is_open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }
    fn subscribe(&self, sink: Option<EventSink>) {
        *self.follower.lock().unwrap() = sink;
    }
}

#[derive(Default)]
struct FakePeer {
    conversations: Mutex<Vec<(ContextSpec, Arc<FakeContext>)>>,
    prepared: Mutex<usize>,
    released: AtomicBool,
}

impl OctosAppService for FakePeer {
    fn deployment(&self) -> Deployment {
        Deployment::Hosted
    }
    fn availability(&self) -> Availability {
        Availability::Ready
    }
    fn services(&self) -> BTreeSet<String> {
        OCTOS_SERVICES.iter().map(|s| s.to_string()).collect()
    }
    fn model(&self) -> Option<ModelInfo> {
        None
    }
    fn settings_entry(&self) -> SettingsEntry {
        SettingsEntry::Host
    }
    fn set_account(&self, _account: Option<&str>) {}
    fn open_context(&self, _spec: ContextSpec) -> Result<Arc<dyn OctosContext>, String> {
        Err("the panel opens the app's conversation, never a plain context".into())
    }
    fn open_conversation(&self, spec: ContextSpec) -> Result<Arc<dyn OctosContext>, String> {
        let context = Arc::new(FakeContext { ops: Mutex::default(), open: AtomicBool::new(true), follower: Mutex::default(), stop_next: AtomicBool::new(false) });
        self.conversations.lock().unwrap().push((spec, context.clone()));
        Ok(context)
    }
    fn prepare(&self) -> Result<(), String> {
        *self.prepared.lock().unwrap() += 1;
        Ok(())
    }
    fn peer_slug(&self) -> Option<String> {
        // The kernel's slug: the label with an account tag.
        Some("org-example-asktest-22a12f90".into())
    }
    fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
        for (_, c) in self.conversations.lock().unwrap().iter() {
            c.close();
        }
    }
    fn shutdown(&self) {}
}

#[derive(Default)]
struct FakePeers(Mutex<Vec<(String, Arc<FakePeer>)>>);

impl crate::ai_host::contained::PeerFactory for FakePeers {
    fn launch(&self, peer_id: &str, _app_id: &str, _services: &BTreeSet<String>) -> Option<Arc<dyn OctosAppService>> {
        let peer = Arc::new(FakePeer::default());
        self.0.lock().unwrap().push((peer_id.to_string(), peer.clone()));
        Some(peer)
    }
}

fn wait(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// These tests replace the peer transport, not bundle admission. Give each
/// fake app real, digest-checked instruction text before opening its agent.
#[cfg(any(feature = "app-hub", native_mobile))]
struct AgentFixture(std::path::PathBuf);
#[cfg(any(feature = "app-hub", native_mobile))]
impl AgentFixture {
    fn new(app: &str) -> Self {
        let root = octosense_appstore::data_root_if_set().unwrap_or_else(|| {
            let root = std::env::temp_dir().join(format!("octosense-chat-bundles-{}", std::process::id()));
            octosense_appstore::set_data_root(root.clone());
            root
        });
        let bundle = root.join(".bundles").join(app).join("bundle");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("AGENT.md"), "Answer questions about this fixture app.").unwrap();
        let mut manifest = json!({
            "schema":1, "id":app, "name":"Conversation fixture", "version":"1.0.0",
            "capabilities":["storage"], "agent":{"profile":"read-only", "instructions":"AGENT.md"},
            "integrity":{"bundle_blake3":""}
        });
        std::fs::write(bundle.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
        manifest["integrity"]["bundle_blake3"] = json!(octosense_app_contract::digest_dir(&bundle).unwrap());
        std::fs::write(bundle.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
        let loaded = crate::host_tools::script_apps::guidance(app).expect("fixture must pass real bundle admission");
        assert_eq!(loaded.agent_md.as_deref(), Some("Answer questions about this fixture app."));
        Self(bundle)
    }
}
#[cfg(any(feature = "app-hub", native_mobile))]
impl Drop for AgentFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The panel, end to end on a fake peer: it asks consent first; once the
/// person allowed the agent it opens a SHARING context on the app's peer
/// (`open_conversation`, never a plain context), follows both lanes, loads
/// the merged history, and sends the person's words as a person turn in
/// that context. The shell prepares an allowed agent's peer. Turning the
/// agent off releases the peer and closes the panel's context.
#[test]
fn the_panel_opens_a_sharing_context_and_sends_person_turns_there() {
    const APP: &str = "org.example.asktest";
    let _factory = crate::agents::FACTORY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    #[cfg(any(feature = "app-hub", native_mobile))]
    let _fixture = AgentFixture::new(APP);
    if crate::approvals::with(|_| ()).is_none() {
        crate::approvals::init_memory();
    }
    crate::approvals::with(|a| a.consent.turn_off(APP, 1));
    let peers = Arc::new(FakePeers::default());
    crate::ai_host::contained::set_factory(peers.clone());
    let app = AgentApp { id: APP.into(), name: "Ask Test".into(), octos: Vec::new(), manifest: json!({"capabilities": ["storage"]}), native: false };

    // Off: the panel says so and opens nothing.
    super::open_app(app.clone());
    assert_eq!(super::status(), super::Status::Off);
    assert!(peers.0.lock().unwrap().is_empty(), "no peer for an agent that is off");

    // The person allows it (Settings, or the first-use sheet).
    crate::approvals::with(|a| a.consent.set(&crate::approvals::rules::ApprovalGesture::sheet_tap(), APP, true, 2));
    assert_eq!(crate::agents::access(APP), crate::agents::Access::Allowed);
    super::pump();
    wait("the conversation", || super::status() == super::Status::Ready);
    let (spec, context) = {
        let convs = &peers.0.lock().unwrap()[0].1;
        let c = convs.conversations.lock().unwrap();
        (c[0].0.clone(), c[0].1.clone())
    };
    assert_eq!(peers.0.lock().unwrap()[0].0, format!("card.{APP}"));
    assert_eq!(spec.services.len(), OCTOS_SERVICES.len(), "the panel reads history, starts and stops turns");
    assert_eq!(context.ops.lock().unwrap().as_slice(), [ContextOp::History]);
    assert!(super::snapshot().items.iter().any(|i| matches!(i, Item::Message { speaker: Some(s), text, .. } if s == "System agent" && text == "earlier")), "the merged history");

    // The person's words: a person turn in that context.
    super::send("focus the digest on technology");
    assert_eq!(context.ops.lock().unwrap().last(), Some(&ContextOp::TurnFrom { text: "focus the digest on technology".into(), trigger: TurnTrigger::Person }));
    let model = super::snapshot();
    let said: Vec<(Role, String, String)> = model.items.iter().filter_map(|i| match i {
        Item::Message { role, text, speaker, .. } => Some((*role, speaker.clone().unwrap_or_default(), text.clone())),
        _ => None,
    }).collect();
    assert!(said.contains(&(Role::User, "You".into(), "focus the digest on technology".into())), "{said:?}");
    assert!(said.contains(&(Role::Assistant, "Ask Test's agent".into(), "Noted.".into())), "{said:?}");

    // A stopped turn: its request line stays, and "interrupted" is said
    // once (the turn's end), not again for the send's error.
    context.stop_next.store(true, Ordering::SeqCst);
    super::send("a long digest please");
    let model = super::snapshot();
    let interrupted = model.items.iter().filter(|i| matches!(i, Item::Notice(n) if n == "turn interrupted by client")).count();
    assert_eq!(interrupted, 1, "{:?}", model.items);
    assert!(model.items.iter().any(|i| matches!(i, Item::Message { role: Role::User, text, .. } if text == "a long digest please")), "{:?}", model.items);

    // The shell prepares an allowed agent's peer: the panel's is the same
    // one (nothing more to do); another allowed app's is bound now.
    crate::agents::prepare(&app);
    assert_eq!(crate::agents::prepared(APP), Some(crate::agents::Prepared::Ready));
    // The system agent is given the peer's SLUG for peer_send_input, never
    // just the app id (it tried `os.news` first in the live run).
    const SLUG: &str = "org-example-asktest-22a12f90";
    assert_eq!(crate::agents::peer_slug(&app).as_deref(), Some(SLUG));
    let listed = crate::agents::line(&app);
    assert_eq!(listed["peer_slug"], SLUG);
    assert!(listed["what_to_do"].as_str().unwrap().contains(&format!("peer_send_input and the peer slug \"{SLUG}\"")), "{listed}");
    let note = crate::agents::note_part(&app);
    assert!(note.contains(SLUG) && note.contains("peer_send_input") && note.contains("not the app id"), "{note}");
    // A held agents.ask answers once the agent is ready: its slug, to send
    // the request to now, in the same turn (it ended the turn to wait for
    // the sheet in the live run, and the request was lost).
    match crate::agents::ask_settled(&app) {
        Some(crate::ai_host::app_peers::host_tools::ToolOutcome::Ok(v)) => {
            assert_eq!(v["peer_slug"], SLUG);
            assert!(v["text"].as_str().unwrap().contains("in this turn"), "{v}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(peers.0.lock().unwrap().len(), 1, "one peer per app");
    const OTHER: &str = "org.example.asktest2";
    #[cfg(any(feature = "app-hub", native_mobile))]
    let _other_fixture = AgentFixture::new(OTHER);
    let other = AgentApp { id: OTHER.into(), name: "Other".into(), ..app.clone() };
    // The person has not answered the sheet: the call is held, and a call
    // that waited too long says they have not.
    assert!(crate::agents::ask_settled(&other).is_none(), "held while the sheet waits");
    match crate::agents::ask_pending(&other) {
        crate::ai_host::app_peers::host_tools::ToolOutcome::Ok(v) => assert!(v["text"].as_str().unwrap().contains("has not answered"), "{v}"),
        other => panic!("{other:?}"),
    }
    crate::approvals::with(|a| a.consent.set(&crate::approvals::rules::ApprovalGesture::sheet_tap(), OTHER, true, 2));
    crate::agents::prepare(&other);
    wait("the preparation", || crate::agents::prepared(OTHER) == Some(crate::agents::Prepared::Ready));
    let prepared = peers.0.lock().unwrap().iter().find(|(id, _)| *id == format!("card.{OTHER}")).map(|(_, p)| *p.prepared.lock().unwrap());
    assert_eq!(prepared, Some(1), "bound before any turn: the system agent's peer_list shows it");
    crate::approvals::with(|a| a.consent.turn_off(OTHER, 3));
    crate::ai_host::contained::revoke(OTHER);

    // Turned off: the peer goes, the panel's context closes, and it says so.
    crate::approvals::with(|a| a.consent.turn_off(APP, 3));
    match crate::agents::ask_settled(&app) {
        Some(crate::ai_host::app_peers::host_tools::ToolOutcome::Ok(v)) => assert_eq!(v["access"], "off", "{v}"),
        other => panic!("{other:?}"),
    }
    assert!(crate::ai_host::contained::revoke(APP));
    assert!(peers.0.lock().unwrap().iter().find(|(id, _)| *id == format!("card.{APP}")).unwrap().1.released.load(Ordering::SeqCst));
    super::pump();
    assert_eq!(super::status(), super::Status::Off);
    assert!(!context.is_open());
    super::close();
}

/// Close and reopen "Ask <app>": the person's own rows are still there
/// (the device lost them: every open made a new context, whose person's
/// lane starts empty), both lanes' history is loaded again, and the live
/// follower still delivers.
#[test]
fn reopening_the_panel_keeps_the_persons_rows_and_the_live_follower() {
    const APP: &str = "org.example.askreopen";
    let _factory = crate::agents::FACTORY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    #[cfg(any(feature = "app-hub", native_mobile))]
    let _fixture = AgentFixture::new(APP);
    if crate::approvals::with(|_| ()).is_none() {
        crate::approvals::init_memory();
    }
    let peers = Arc::new(FakePeers::default());
    crate::ai_host::contained::set_factory(peers.clone());
    crate::approvals::with(|a| a.consent.set(&crate::approvals::rules::ApprovalGesture::sheet_tap(), APP, true, 2));
    let app = AgentApp { id: APP.into(), name: "Reopen".into(), octos: Vec::new(), manifest: json!({"capabilities": ["storage"]}), native: false };
    let persons_rows = || -> Vec<String> {
        super::snapshot().items.iter().filter_map(|i| match i {
            Item::Message { role: Role::User, speaker: Some(s), text, .. } if s == "You" => Some(text.clone()),
            _ => None,
        }).collect()
    };

    super::open_app(app.clone());
    wait("the conversation", || super::status() == super::Status::Ready);
    super::send("what is new?");
    assert_eq!(persons_rows(), ["what is new?"]);
    let context = peers.0.lock().unwrap()[0].1.conversations.lock().unwrap()[0].1.clone();
    assert_eq!(super::shown_app().as_deref(), Some(APP), "open: its questions are the panel's, not the overlay card's");

    super::close();
    assert!(!super::is_open());
    assert_eq!(super::shown_app(), None, "hidden: the overlay's card asks its questions again");
    assert!(context.is_open(), "closing hides the panel; its context stays");
    // While hidden, the follower keeps the conversation up to date.
    let follower = context.follower.lock().unwrap().clone().expect("still followed");
    follower(ContextEvent::Data(env("system_agent", "s7", 5, "assistant_persisted", json!({"assistant_segment_id": "s7:1", "text": "Digest ready."}))));

    super::open_app(app.clone());
    wait("the history again", || context.ops.lock().unwrap().iter().filter(|op| **op == ContextOp::History).count() == 2);
    wait("ready", || super::status() == super::Status::Ready);
    assert_eq!(peers.0.lock().unwrap()[0].1.conversations.lock().unwrap().len(), 1, "the same context, not a new empty lane");
    assert_eq!(persons_rows(), ["what is new?"], "{:?}", super::snapshot().items);
    assert!(super::snapshot().items.iter().any(|i| matches!(i, Item::Message { speaker: Some(s), text, .. } if s == "System agent" && text == "earlier")), "the system agent's lane too");
    // The live follower still delivers after the reopen.
    follower(ContextEvent::Data(env("system_agent", "s8", 6, "assistant_persisted", json!({"assistant_segment_id": "s8:1", "text": "Live again."}))));
    assert!(super::snapshot().items.iter().any(|i| matches!(i, Item::Message { text, .. } if text == "Live again.")));
    super::send("and now?");
    assert_eq!(persons_rows(), ["what is new?", "and now?"]);

    // The peer went while hidden (the agent turned off, the app released):
    // a reopen opens a new context, for the same instance, so the broker's
    // history still has the person's earlier rows.
    super::close();
    crate::approvals::with(|a| a.consent.turn_off(APP, 3));
    crate::ai_host::contained::revoke(APP);
    assert!(!context.is_open());
    super::close();
    assert_eq!(super::status(), super::Status::Idle);
    crate::approvals::with(|a| a.consent.set(&crate::approvals::rules::ApprovalGesture::sheet_tap(), APP, true, 4));
    super::open_app(app.clone());
    wait("a new conversation", || super::status() == super::Status::Ready);
    let instances: Vec<String> = peers.0.lock().unwrap().iter().flat_map(|(_, p)| p.conversations.lock().unwrap().iter().map(|(spec, _)| spec.instance.clone()).collect::<Vec<_>>()).collect();
    assert_eq!(instances, [super::INSTANCE, super::INSTANCE], "one instance on every open");
    super::close();
    crate::approvals::with(|a| a.consent.turn_off(APP, 5));
    crate::ai_host::contained::revoke(APP);
}

/// The two lanes are independent: the system agent's running turn does not
/// make the person's lane busy (Send stays), and each lane's turn is known.
#[test]
fn each_lane_has_its_own_running_turn() {
    let mut c = Conversation::new("News");
    c.apply(&json!({"method": "turn/started", "lane": LANE_SYSTEM_AGENT, "params": {"turn_id": "s1"}, "request": {"text": "digest", "speaker": {"kind": "system_agent"}}}));
    assert_eq!(c.running_in(LANE_SYSTEM_AGENT), Some("s1"));
    assert_eq!(c.running_in(LANE_PERSON), None, "the person may send");
    c.apply(&json!({"method": "turn/started", "lane": LANE_PERSON, "params": {"turn_id": "p1"}, "request": {"text": "why?", "speaker": {"kind": "person"}}}));
    assert_eq!(c.running_in(LANE_PERSON), Some("p1"));
    c.apply(&env(LANE_PERSON, "p1", 3, "turn_terminal", json!({"outcome": "interrupted"})));
    assert_eq!(c.running_in(LANE_PERSON), None, "the person's Stop ended only theirs");
    assert_eq!(c.running_in(LANE_SYSTEM_AGENT), Some("s1"));
}

// ---------------------------------------------------------------- the system agent

/// The system agent hears which apps have an agent and where each stands,
/// including the ones it cannot see in peer_list; the pane hides the note.
#[test]
fn the_system_agent_is_told_about_agents_it_cannot_list() {
    let note = crate::agents::system_note().expect("Rinx has an agent in every build");
    assert!(note.starts_with("[OctoSense: apps with an agent:"), "{note}");
    assert!(note.contains("Rinx [rinx]"), "{note}");
    assert!(note.contains("peer_send_input takes a peer slug from peer_list, never an app id"), "{note}");
    let sent = format!("{note}\nask News for a digest");
    assert_eq!(crate::agents::strip_note(&sent), "ask News for a digest");
    assert_eq!(crate::agents::strip_note("ask News"), "ask News");
    // The tools it can call, answered by the shell.
    let declarations = crate::agents::declarations();
    let names: Vec<String> = declarations.iter().map(|d| d["name"].as_str().unwrap().to_string()).collect();
    let mut expected = vec![crate::agents::LIST_TOOL, crate::agents::ASK_TOOL];
    if cfg!(any(feature = "app-hub", native_mobile)) {
        expected.extend([crate::agents::PROVISION_TOOL, crate::agents::STATUS_TOOL]);
    }
    assert_eq!(names, expected);
    // agents.ask's confirmation is the shell's own sheet, so the kernel
    // holds the call as long as an approval, not a read tool's 30 s.
    let ask = &declarations[1];
    assert_eq!((ask["risk"].as_str(), ask["outward"].as_bool(), ask["confirm"].as_str()), (Some("act"), Some(true), Some("app")));
    match crate::agents::call(crate::agents::LIST_TOOL, &json!({})) {
        crate::ai_host::app_peers::host_tools::ToolOutcome::Ok(v) => {
            assert!(v["apps"].as_array().unwrap().iter().any(|a| a["app"] == "rinx" && a["what_to_do"].as_str().is_some()), "{v}");
        }
        other => panic!("{other:?}"),
    }
    match crate::agents::call(crate::agents::ASK_TOOL, &json!({"app": "no such app"})) {
        crate::ai_host::app_peers::host_tools::ToolOutcome::Error { kind, .. } => assert_eq!(kind, "no_such_agent"),
        other => panic!("{other:?}"),
    }
}
