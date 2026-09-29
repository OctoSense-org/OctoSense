//! The router's precedence, every rule condition, the cap, the time box,
//! the incoming-content exclusion, the sheet model, the `confirm: app`
//! hand-off, consent and the audit.

use super::audit::AuditLog;
use super::consent::{AgentSummary, ConsentStore, State};
use super::dev_hooks::FixedDevMode;
use super::relay::RecordingRelay;
use super::router::{make_request, AppConfirm, AppConfirmRequest, AutoBy, Route, Router};
use super::contacts::{ContactList, ContactsGate, ContactsSource, MailContacts, NoContacts};
use super::rules::{Conditions, ApprovalGesture, RuleDraft, RuleStore, DEFAULT_DAILY_CAP};
use super::sheet::{Answer, Place, Surfaced};
use super::types::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const T0: u64 = 1_790_000_000; // a fixed "now", mid-day UTC
const MAIL: &str = "os.mail";

fn router_with(dev: FixedDevMode) -> (Router, RecordingRelay) {
    let relay = RecordingRelay::default();
    let r = router_with_contacts(dev, relay.clone(), true);
    (r, relay)
}
/// Ana and Bo are contacts; `consent`: "Use my contacts in approval rules" is on.
fn router_with_contacts(dev: FixedDevMode, relay: RecordingRelay, consent: bool) -> Router {
    let mut contacts = ContactsGate::memory(Box::new(ContactList(vec!["ana@example.org".into(), "bo@example.org".into(), "@ana:example.org".into()])));
    if consent {
        contacts.allow(&ApprovalGesture::settings_tap(), T0);
    }
    Router::new(RuleStore::memory(), AuditLog::memory(), Box::new(dev), contacts, Box::new(relay))
}
fn router() -> (Router, RecordingRelay) {
    router_with(FixedDevMode::off())
}

fn ctx(id: &str, trigger: Trigger) -> RequestContext {
    RequestContext { call_id: id.into(), trigger, ..RequestContext::default() }
}
fn req(id: &str, tool: ToolSpec, args: Value, trigger: Trigger) -> Request {
    make_request(MAIL, tool, args, Caller::AppAgent { app: "calendar".into() }, ctx(id, trigger), T0, 0)
}
fn send(id: &str, args: Value) -> Request {
    req(id, ToolSpec::host("mail.send"), args, Trigger::Person)
}
fn rule(r: &mut Router, draft: RuleDraft) -> RuleId {
    r.create_rule(&ApprovalGesture::settings_tap(), draft, T0).expect("rule")
}
fn contacts_rule() -> RuleDraft {
    RuleDraft::tool(MAIL, "mail.send", Conditions { recipients_in_contacts: true, ..Conditions::default() })
}

// ---------------------------------------------------------------- precedence

#[test]
fn no_rule_means_a_sheet() {
    let (mut r, relay) = router();
    assert!(matches!(r.request(send("a", json!({"to": "ana@example.org"})), T0), Route::Sheet(_)));
    assert!(relay.take().is_empty(), "nothing is decided before the person answers");
    assert_eq!(r.front_sheet().unwrap().lines[0].surfaced, Surfaced::NoRule);
}

#[test]
fn developer_mode_first_even_for_not_auto_approvable_and_app_confirm() {
    let (mut r, relay) = router_with(FixedDevMode::all());
    let del = req("del", ToolSpec::host("mail.delete_forever").not_auto_approvable(), json!({"id": 1}), Trigger::Unknown);
    assert_eq!(r.request(del, T0), Route::Approved(AutoBy::DeveloperMode));
    let app = make_request("rinx", ToolSpec::app("rinx.message.send"), json!({}), Caller::SystemAgent, ctx("rx", Trigger::SystemAgent), T0, 0);
    assert_eq!(r.request(app, T0), Route::Approved(AutoBy::DeveloperMode));
    let cmd = make_request("terminal", ToolSpec::host("terminal.run").command(), json!({"command": "ls"}), Caller::SystemAgent, ctx("cmd", Trigger::Person), T0, 0);
    assert_eq!(r.request(cmd, T0), Route::Approved(AutoBy::DeveloperMode));
    let d = relay.take();
    assert_eq!(d.len(), 3);
    assert!(d.iter().all(|(_, dec, reason)| *dec == Decision::ApproveOnce && reason == "developer mode"));
    // Audited and notified.
    assert!(r.audit.all().iter().all(|e| e.by == "developer_mode" && e.result == "approved"));
    assert!(r.take_notices().iter().filter(|n| n.title.starts_with("Approved automatically")).count() == 3);
}

#[test]
fn developer_mode_never_answers_an_external_client() {
    let (mut r, relay) = router_with(FixedDevMode::all());
    let mut q = send("x", json!({"to": "ana@example.org"}));
    q.context.connection = Connection::External;
    assert!(matches!(r.request(q, T0), Route::Sheet(_)));
    assert!(relay.take().is_empty());
    assert_eq!(r.front_sheet().unwrap().lines[0].surfaced, Surfaced::External);
    assert!(r.front_sheet().unwrap().lines[0].always.is_empty(), "no rule for an external client");
}

#[test]
fn developer_mode_for_other_apps_changes_nothing() {
    let (mut r, _) = router_with(FixedDevMode { all: false, apps: vec!["rinx".into()] });
    assert!(matches!(r.request(send("a", json!({"to": "ana@example.org"})), T0), Route::Sheet(_)));
}

#[test]
fn not_auto_approvable_ignores_every_rule() {
    let (mut r, relay) = router();
    rule(&mut r, RuleDraft::everything(MAIL, 30));
    rule(&mut r, RuleDraft::tool(MAIL, "mail.delete_forever", Conditions::default()));
    let del = req("del", ToolSpec::host("mail.delete_forever").not_auto_approvable(), json!({"id": 1}), Trigger::Person);
    assert!(matches!(r.request(del, T0), Route::Sheet(_)));
    assert!(relay.take().is_empty());
    let line = &r.front_sheet().unwrap().lines[0];
    assert_eq!(line.surfaced, Surfaced::NotAutoApprovable);
    assert!(line.always.is_empty(), "no \u{201c}always\u{201d} for a tool no rule may answer");
}

#[test]
fn unknown_outcome_always_goes_to_the_person() {
    let (mut r, relay) = router();
    rule(&mut r, contacts_rule());
    let mut q = send("u", json!({"to": "ana@example.org"}));
    q.context.outcome_unknown = true;
    assert!(matches!(r.request(q, T0), Route::Sheet(_)));
    assert!(relay.take().is_empty());
    assert_eq!(r.front_sheet().unwrap().lines[0].surfaced, Surfaced::OutcomeUnknown);
}

#[test]
fn incoming_content_is_excluded_by_default() {
    let (mut r, relay) = router();
    rule(&mut r, contacts_rule());
    let from = Trigger::IncomingContent { from: Some("eve@example.org".into()) };
    let q = req("in", ToolSpec::host("mail.send"), json!({"to": "ana@example.org"}), from.clone());
    assert!(matches!(r.request(q, T0), Route::Sheet(_)));
    assert!(relay.take().is_empty());
    assert_eq!(r.front_sheet().unwrap().lines[0].surfaced, Surfaced::IncomingContent);
    // An unstated trigger is treated the same.
    let q = req("unk", ToolSpec::host("mail.send"), json!({"to": "ana@example.org"}), Trigger::Unknown);
    assert!(matches!(r.request(q, T0), Route::Sheet(_)));
    // A rule that opts in answers it.
    let mut d = contacts_rule();
    d.include_incoming = true;
    let id = rule(&mut r, d);
    let q = req("in2", ToolSpec::host("mail.send"), json!({"to": "ana@example.org"}), from);
    assert_eq!(r.request(q, T0), Route::Approved(AutoBy::Rule(id)));
}

#[test]
fn rules_are_keyed_on_owning_app_and_tool_whoever_calls() {
    let (mut r, relay) = router();
    let id = rule(&mut r, contacts_rule());
    for (i, caller) in [Caller::OwnAgent { client: None }, Caller::AppAgent { app: "calendar".into() }, Caller::SystemAgent].into_iter().enumerate() {
        let q = make_request(MAIL, ToolSpec::host("mail.send"), json!({"to": "ana@example.org"}), caller.clone(), ctx(&format!("c{i}"), Trigger::SystemAgent), T0, 0);
        assert_eq!(r.request(q, T0), Route::Approved(AutoBy::Rule(id.clone())), "{caller:?}");
    }
    // Another tool, or the same tool name of another app, is not covered.
    assert!(matches!(r.request(req("t", ToolSpec::host("mail.forward"), json!({"to": "ana@example.org"}), Trigger::Person), T0), Route::Sheet(_)));
    let other = make_request("os.notes", ToolSpec::host("mail.send"), json!({"to": "ana@example.org"}), Caller::SystemAgent, ctx("o", Trigger::Person), T0, 0);
    assert!(matches!(r.request(other, T0), Route::Sheet(_)));
    let d = relay.take();
    assert_eq!(d.len(), 3);
    assert!(d.iter().all(|(_, dec, _)| *dec == Decision::ApproveByRule(id.clone())));
}

// ---------------------------------------------------------------- conditions

fn matches(r: &mut Router, id: &str, args: Value, trigger: Trigger, thread: &[&str]) -> bool {
    let mut q = req(id, ToolSpec::host("mail.send"), args, trigger);
    q.context.thread = thread.iter().map(|s| s.to_string()).collect();
    matches!(r.request(q, T0), Route::Approved(AutoBy::Rule(_)))
}

#[test]
fn condition_recipients_in_contacts() {
    let (mut r, _) = router();
    rule(&mut r, contacts_rule());
    assert!(matches(&mut r, "1", json!({"to": ["Ana <ana@example.org>", "bo@example.org"]}), Trigger::Person, &[]));
    assert!(!matches(&mut r, "2", json!({"to": ["ana@example.org", "eve@example.org"]}), Trigger::Person, &[]));
    assert!(!matches(&mut r, "3", json!({"to": "ana@example.org", "bcc": "eve@example.org"}), Trigger::Person, &[]), "bcc counts");
    assert!(!matches(&mut r, "4", json!({"subject": "no recipients"}), Trigger::Person, &[]), "a missing fact fails");
    assert!(matches(&mut r, "5", json!({"to": "@Ana:example.org"}), Trigger::Person, &[]), "a Matrix ID");
    assert!(!matches(&mut r, "6", json!({"to": "@eve:example.org"}), Trigger::Person, &[]));
}

#[test]
fn contacts_never_match_without_consent() {
    let relay = RecordingRelay::default();
    let mut r = router_with_contacts(FixedDevMode::off(), relay.clone(), false);
    rule(&mut r, contacts_rule());
    assert!(!r.contacts().allowed(), "off by default");
    assert!(!matches(&mut r, "1", json!({"to": "ana@example.org"}), Trigger::Person, &[]), "a known recipient, no consent");
    assert!(
        r.front_sheet().unwrap().lines[0].always.iter().all(|c| !c.label.contains("contacts")),
        "no \u{201c}always for people in my contacts\u{201d} either"
    );
    r.contacts_mut().allow(&ApprovalGesture::settings_tap(), T0);
    assert!(matches(&mut r, "2", json!({"to": "ana@example.org"}), Trigger::Person, &[]));
    assert!(!matches(&mut r, "3", json!({"to": "eve@example.org"}), Trigger::Person, &[]), "an unknown recipient");
    r.contacts_mut().turn_off(T0);
    assert!(!matches(&mut r, "4", json!({"to": "ana@example.org"}), Trigger::Person, &[]), "turned off again");
    // A new source keeps the person's choice.
    r.set_contacts(Box::new(ContactList(vec!["ana@example.org".into()])));
    assert!(!r.contacts().is_known("ana@example.org"));
}

#[test]
fn mail_contacts_read_the_mail_services_data_where_it_lives() {
    fn read(dir: &std::path::Path) -> Vec<String> {
        // Stands in for the mail service's known_addresses.
        if dir.ends_with(".host") { vec!["me@example.com".into(), "ana@example.org".into()] } else { Vec::new() }
    }
    let mail = MailContacts::new(|| Some(std::path::PathBuf::from("apps-root").join(".host")), read);
    assert!(mail.is_known("Ana@Example.org") && mail.is_known("me@example.com"));
    assert!(!mail.is_known("eve@example.org") && !mail.is_known(""));
    let unset = MailContacts::new(|| None, read);
    assert!(!unset.is_known("ana@example.org"), "no App Hub data root yet: nobody");
    assert!(!NoContacts.is_known("ana@example.org"));
    // Behind the gate: nothing until the person allows it.
    let mut gate = ContactsGate::memory(Box::new(mail));
    assert!(!gate.is_known("ana@example.org"));
    gate.allow(&ApprovalGesture::settings_tap(), T0);
    assert!(gate.is_known("ana@example.org"));
}

#[test]
fn condition_recipients_in_thread_and_either() {
    let (mut r, _) = router();
    rule(&mut r, RuleDraft::tool(MAIL, "mail.send", Conditions { recipients_in_thread: true, ..Conditions::default() }));
    assert!(matches(&mut r, "1", json!({"to": "eve@example.org"}), Trigger::Person, &["eve@example.org"]));
    assert!(!matches(&mut r, "2", json!({"to": "ana@example.org"}), Trigger::Person, &["eve@example.org"]), "in contacts is not in the thread");
    let (mut r, _) = router();
    rule(&mut r, RuleDraft::tool(MAIL, "mail.send", Conditions { recipients_in_thread: true, recipients_in_contacts: true, ..Conditions::default() }));
    assert!(matches(&mut r, "3", json!({"to": ["ana@example.org", "eve@example.org"]}), Trigger::Person, &["eve@example.org"]));
    assert!(!matches(&mut r, "4", json!({"to": ["zed@example.org"]}), Trigger::Person, &["eve@example.org"]));
}

#[test]
fn condition_no_attachments() {
    let (mut r, _) = router();
    rule(&mut r, RuleDraft::tool(MAIL, "mail.send", Conditions { no_attachments: true, ..Conditions::default() }));
    assert!(matches(&mut r, "1", json!({"to": "x@y", "attachments": []}), Trigger::Person, &[]));
    assert!(!matches(&mut r, "2", json!({"to": "x@y", "attachments": ["contract.pdf"]}), Trigger::Person, &[]));
}

#[test]
fn condition_triggered_by_the_person() {
    let (mut r, _) = router();
    rule(&mut r, RuleDraft::tool(MAIL, "mail.send", Conditions { triggered_by_person: true, ..Conditions::default() }));
    assert!(matches(&mut r, "1", json!({"to": "x@y"}), Trigger::Person, &[]));
    assert!(!matches(&mut r, "2", json!({"to": "x@y"}), Trigger::App, &[]));
    assert!(!matches(&mut r, "3", json!({"to": "x@y"}), Trigger::SystemAgent, &[]));
}

#[test]
fn condition_amount_and_count_limits() {
    let (mut r, _) = router();
    rule(&mut r, RuleDraft::tool(MAIL, "mail.send", Conditions { max_amount: Some(50.0), ..Conditions::default() }));
    assert!(matches(&mut r, "1", json!({"amount": 49.5}), Trigger::Person, &[]));
    assert!(matches(&mut r, "2", json!({"amount": "$50"}), Trigger::Person, &[]));
    assert!(!matches(&mut r, "3", json!({"amount": 50.01}), Trigger::Person, &[]));
    assert!(!matches(&mut r, "4", json!({"note": "no amount"}), Trigger::Person, &[]), "a missing amount fails");
    let (mut r, _) = router();
    rule(&mut r, RuleDraft::tool(MAIL, "mail.send", Conditions { max_count: Some(2), ..Conditions::default() }));
    assert!(matches(&mut r, "5", json!({"to": ["a@x", "b@x"]}), Trigger::Person, &[]));
    assert!(!matches(&mut r, "6", json!({"to": ["a@x", "b@x", "c@x"]}), Trigger::Person, &[]));
    assert!(!matches(&mut r, "7", json!({"count": 3}), Trigger::Person, &[]));
}

#[test]
fn daily_cap_then_the_next_day() {
    let (mut r, _) = router();
    let mut d = contacts_rule();
    d.daily_cap = Some(2);
    let id = rule(&mut r, d);
    let args = json!({"to": "ana@example.org"});
    assert!(matches!(r.request(send("1", args.clone()), T0), Route::Approved(_)));
    assert!(matches!(r.request(send("2", args.clone()), T0 + 5), Route::Approved(_)));
    assert!(matches!(r.request(send("3", args.clone()), T0 + 10), Route::Sheet(_)), "the cap is reached");
    assert_eq!(r.rules.get(&id).unwrap().used_today(T0), 2);
    let tomorrow = T0 + 86_400;
    assert!(matches!(r.request(send("4", args), tomorrow), Route::Approved(_)));
    assert_eq!(r.rules.get(&id).unwrap().used_today(tomorrow), 1);
    // A sheet's rule gets the default cap.
    assert_eq!(contacts_rule().daily_cap, Some(DEFAULT_DAILY_CAP));
}

#[test]
fn time_boxed_everything_expires_and_is_never_forever() {
    let (mut r, _) = router();
    let g = ApprovalGesture::settings_tap();
    assert!(r.create_rule(&g, RuleDraft { minutes: None, ..RuleDraft::everything(MAIL, 1) }, T0).is_err(), "no forever");
    assert!(r.create_rule(&g, RuleDraft::everything(MAIL, 61), T0).is_err(), "60 minutes at most");
    let id = rule(&mut r, RuleDraft::everything(MAIL, 60));
    assert_eq!(r.rules.active_everything(T0).len(), 1, "the indicator shows it");
    assert_eq!(r.rules.get(&id).unwrap().minutes_left(T0 + 90), Some(59));
    let other_tool = req("a", ToolSpec::host("mail.archive"), json!({"id": 3}), Trigger::App);
    assert_eq!(r.request(other_tool, T0 + 60), Route::Approved(AutoBy::Rule(id.clone())));
    // Not after the hour.
    let late = T0 + 3600;
    assert!(r.tick(late));
    assert!(r.rules.get(&id).is_none(), "expired rules are removed");
    assert!(r.take_notices().iter().any(|n| n.title == "Approval rule ended"));
    assert!(matches!(r.request(req("b", ToolSpec::host("mail.archive"), json!({"id": 4}), Trigger::App), late), Route::Sheet(_)));
    // A tool rule with a time box expires too, even before tick.
    let id = rule(&mut r, RuleDraft::tool(MAIL, "mail.send", Conditions::default()).for_minutes(10));
    assert!(r.rules.get(&id).unwrap().expired(T0 + 600));
}

#[test]
fn narrow_rules_answer_before_the_time_box() {
    let (mut r, _) = router();
    let broad = rule(&mut r, RuleDraft::everything(MAIL, 30));
    let narrow = rule(&mut r, contacts_rule());
    assert_eq!(r.request(send("1", json!({"to": "ana@example.org"})), T0), Route::Approved(AutoBy::Rule(narrow)));
    assert_eq!(r.request(send("2", json!({"to": "eve@example.org"})), T0), Route::Approved(AutoBy::Rule(broad)));
}

#[test]
fn one_tap_turns_every_rule_off() {
    let (mut r, _) = router();
    let narrow = rule(&mut r, contacts_rule());
    rule(&mut r, RuleDraft::everything(MAIL, 30));
    assert_eq!(r.all_off(), 2);
    assert!(r.rules.active_everything(T0).is_empty(), "the time box ends");
    assert!(!r.rules.get(&narrow).unwrap().enabled);
    assert!(matches!(r.request(send("1", json!({"to": "ana@example.org"})), T0), Route::Sheet(_)));
    // Only the person turns one back on.
    assert!(r.rules.enable(&ApprovalGesture::settings_tap(), &narrow));
    assert!(matches!(r.request(send("2", json!({"to": "ana@example.org"})), T0), Route::Approved(_)));
}

// ---------------------------------------------------------------- sheets

#[test]
fn sheet_model_shows_app_tool_caller_and_redacted_args() {
    let (mut r, _) = router();
    let tool = ToolSpec::host("mail.send").secret("signature_key");
    let args = json!({"to": ["ana@example.org"], "subject": "Tuesday", "signature_key": "k-123", "smtp_password": "hunter2"});
    let Route::Sheet(id) = r.request(req("s", tool, args, Trigger::Person), T0) else { panic!() };
    let sheet = r.front_sheet().unwrap();
    assert_eq!(sheet.id, id);
    assert_eq!(sheet.place, Place::AppConversation { app: MAIL.into() });
    assert_eq!(sheet.title(), "Mail wants to use mail.send");
    let line = &sheet.lines[0];
    assert_eq!(line.heading(), "Mail \u{00b7} mail.send");
    assert_eq!(line.caller, "Calendar's agent");
    let text = line.args.join("\n");
    assert!(text.contains("\"subject\": \"Tuesday\"") && text.contains("ana@example.org"), "{text}");
    assert!(!text.contains("k-123") && !text.contains("hunter2"), "secrets redacted: {text}");
    // Offers: contacts (Ana is one), when I start it, 1 hour, the time box.
    let labels: Vec<&str> = line.always.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels[0], "Always for people in my contacts, no attachments");
    assert!(labels.contains(&"Always when I start it"));
    assert!(labels.iter().any(|l| l.starts_with("Everything Mail asks")));
}

#[test]
fn a_batch_is_one_sheet_in_the_system_chat() {
    let (mut r, relay) = router();
    let batch = Batch { id: "plan-7".into(), plan: "Book Tue 3 pm and invite 2".into() };
    let mut ids = Vec::new();
    for (i, to) in ["ana@example.org", "eve@example.org"].iter().enumerate() {
        let mut q = send(&format!("b{i}"), json!({"to": to}));
        q.context.batch = Some(batch.clone());
        q.caller = Caller::SystemAgent;
        let Route::Sheet(id) = r.request(q, T0) else { panic!() };
        ids.push(id);
    }
    assert_eq!(ids[0], ids[1], "one sheet");
    assert_eq!(r.sheets().len(), 1);
    let sheet = r.front_sheet().unwrap().clone();
    assert!(matches!(sheet.place, Place::SystemChat { .. }));
    assert_eq!(sheet.title(), "Book Tue 3 pm and invite 2");
    assert_eq!(sheet.subtitle(), "In the system chat \u{00b7} 2 actions to approve");
    assert_eq!(sheet.lines[1].caller, "The system agent");
    // Answered line by line.
    let g = ApprovalGesture::sheet_tap();
    r.answer(sheet.id, &RequestId("b0".into()), Answer::Once, &g, T0).unwrap();
    assert_eq!(r.sheets().len(), 1, "one line still open");
    r.answer(sheet.id, &RequestId("b1".into()), Answer::Deny, &g, T0).unwrap();
    assert!(r.sheets().is_empty());
    assert_eq!(relay.take(), vec![(RequestId("b0".into()), Decision::ApproveOnce, "approved on the sheet".into()), (RequestId("b1".into()), Decision::Deny, "denied on the sheet".into())]);
    assert!(r.answer(sheet.id, &RequestId("b1".into()), Answer::Once, &g, T0).is_err(), "answered once");
}

#[test]
fn always_for_makes_a_rule_and_answers_the_matching_open_lines() {
    let (mut r, relay) = router();
    let Route::Sheet(s1) = r.request(send("1", json!({"to": "ana@example.org"})), T0) else { panic!() };
    let Route::Sheet(_) = r.request(send("2", json!({"to": "bo@example.org"})), T0) else { panic!() };
    let Route::Sheet(_) = r.request(send("3", json!({"to": "eve@example.org"})), T0) else { panic!() };
    let made = r.answer(s1, &RequestId("1".into()), Answer::Always(0), &ApprovalGesture::sheet_tap(), T0).unwrap().expect("a rule");
    let rule = r.rules.get(&made).unwrap();
    assert!(rule.conditions.recipients_in_contacts && rule.conditions.no_attachments);
    assert_eq!(rule.daily_cap, Some(DEFAULT_DAILY_CAP));
    let d = relay.take();
    assert_eq!(d[0], (RequestId("1".into()), Decision::ApproveByRule(made.clone()), "approved and made a rule: Always for people in my contacts, no attachments".into()));
    assert_eq!(d[1].0, RequestId("2".into()), "Bo is in contacts too");
    assert_eq!(d.len(), 2, "Eve still asks");
    assert_eq!(r.sheets().len(), 1);
    assert_eq!(r.rules.get(&made).unwrap().used_today(T0), 2);
}

#[test]
fn an_unanswered_sheet_expires_declined() {
    let (mut r, relay) = router();
    r.request(send("1", json!({"to": "eve@example.org"})), T0);
    assert!(!r.tick(T0 + 10));
    assert!(r.tick(T0 + r.sheet_expiry_s));
    assert_eq!(relay.last().unwrap().1, Decision::Deny);
    assert!(r.sheets().is_empty());
    assert_eq!(r.audit.all().last().unwrap().by, "timeout");
}

// ---------------------------------------------------------------- confirm: app

#[derive(Clone, Default)]
struct AppSheet(Arc<Mutex<Vec<AppConfirmRequest>>>);
impl AppConfirm for AppSheet {
    fn confirm(&mut self, request: &AppConfirmRequest) {
        self.0.lock().unwrap().push(request.clone());
    }
}

fn rinx_send(id: &str) -> Request {
    let mut q = make_request("rinx", ToolSpec::app("rinx.message.send"), json!({"room": "!r:x", "body": "hi"}), Caller::AppAgent { app: "calendar".into() }, ctx(id, Trigger::SystemAgent), T0, 0);
    q.context.context_id = Some("ctx-1".into());
    q
}

#[test]
fn confirm_app_goes_to_the_owning_apps_sheet_with_the_caller() {
    let (mut r, relay) = router();
    // Even a rule for it is not asked.
    rule(&mut r, RuleDraft::everything("rinx", 30));
    let sheet = AppSheet::default();
    r.register_app_confirm("rinx", Box::new(sheet.clone()));
    assert_eq!(r.request(rinx_send("m1"), T0), Route::HandedToApp);
    let seen = sheet.0.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].caller_label, "Calendar's agent");
    assert_eq!(seen[0].context_id.as_deref(), Some("ctx-1"));
    assert!(relay.take().is_empty());
    r.app_confirm_answered(&RequestId("m1".into()), true, "sent from Rinx's sheet", T0).unwrap();
    assert_eq!(relay.last().unwrap().1, Decision::ApproveOnce);
    assert_eq!(r.audit.all().last().unwrap().by, "app_sheet");
    assert!(r.app_confirm_answered(&RequestId("m1".into()), true, "", T0).is_err(), "once");
}

#[test]
fn confirm_app_waits_for_the_app_then_is_refused_visibly() {
    let (mut r, relay) = router();
    let Route::WaitingForApp { until } = r.request(rinx_send("m1"), T0) else { panic!() };
    assert_eq!(until, T0 + r.app_wait_s);
    // The app comes: it gets the request.
    let sheet = AppSheet::default();
    r.register_app_confirm("rinx", Box::new(sheet.clone()));
    assert_eq!(sheet.0.lock().unwrap().len(), 1);
    r.unregister_app_confirm("rinx", T0 + 1);
    assert_eq!(relay.last().unwrap().1, Decision::Deny, "closed before confirming");
    // It never comes: refused at the deadline, with a notice.
    let Route::WaitingForApp { until } = r.request(rinx_send("m2"), T0) else { panic!() };
    r.take_notices();
    r.tick(until - 1);
    assert!(r.is_pending(&RequestId("m2".into())));
    r.tick(until);
    assert!(!r.is_pending(&RequestId("m2".into())));
    assert_eq!(relay.last().unwrap(), (RequestId("m2".into()), Decision::Deny, "Rinx wasn't opened in time to confirm it".into()));
    assert!(r.take_notices().iter().any(|n| n.title.starts_with("Refused: Rinx")));
    // No waiting at all.
    r.app_wait_s = 0;
    assert!(matches!(r.request(rinx_send("m3"), T0), Route::Refused(_)));
}

// ---------------------------------------------------------------- audit, files

#[test]
fn every_automatic_approval_is_audited_and_notified() {
    let (mut r, _) = router();
    let id = rule(&mut r, contacts_rule());
    let args = json!({"to": "ana@example.org", "body": "private text"});
    r.request(send("1", args.clone()), T0);
    let e = r.audit.recent_automatic(1).pop().unwrap();
    assert_eq!((e.app.as_str(), e.tool.as_str(), e.caller.as_str(), e.by.as_str(), e.result.as_str()), (MAIL, "mail.send", "app/calendar", "rule", "approved"));
    assert_eq!(e.rule.as_deref(), Some(id.0.as_str()));
    assert_eq!(e.args_digest, super::facts::digest(&args));
    assert_eq!(e.ts, T0);
    assert!(!serde_json::to_string(&e).unwrap().contains("private text"), "the log holds a digest, not the arguments");
    let n = r.take_notices();
    assert!(n.iter().any(|n| n.title == "Approved automatically: Mail \u{00b7} mail.send" && n.body.contains("people in my contacts") && n.body.contains("Calendar's agent")));
}

fn temp_home(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("octosense-approvals-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn rules_consent_and_audit_persist_per_home_owner_only() {
    let home = temp_home("persist");
    {
        let mut a = super::Approvals::in_home(&home);
        a.router.set_contacts(Box::new(ContactList(vec!["ana@example.org".into()])));
        assert!(!a.router.contacts().allowed(), "contacts are off in a new home");
        a.router.contacts_mut().allow(&ApprovalGesture::settings_tap(), T0);
        let id = a.router.create_rule(&ApprovalGesture::settings_tap(), contacts_rule(), T0).unwrap();
        a.router.request(send("1", json!({"to": "ana@example.org"})), T0);
        assert_eq!(a.router.rules.get(&id).unwrap().used_today(T0), 1);
        a.consent.set(&ApprovalGesture::sheet_tap(), "os.news", true, T0);
    }
    let a = super::Approvals::in_home(&home);
    assert_eq!(a.router.rules.rules().len(), 1);
    assert_eq!(a.router.rules.rules()[0].used_today(T0), 1, "the cap's count survives a restart");
    assert_eq!(a.consent.state("os.news"), State::Allowed);
    assert!(a.router.contacts().allowed(), "the contacts choice survives a restart");
    assert_eq!(a.router.audit.recent_automatic(5).len(), 1, "the log is read back");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for f in [super::rules::RULES_FILE, super::consent::CONSENT_FILE, super::contacts::CONTACTS_FILE, super::audit::AUDIT_FILE] {
            let mode = std::fs::metadata(home.join(f)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{f}");
        }
    }
    // Append-only: a second session adds, never rewrites.
    let before = std::fs::read_to_string(home.join(super::audit::AUDIT_FILE)).unwrap();
    let mut a = a;
    a.router.set_contacts(Box::new(ContactList(vec!["ana@example.org".into()])));
    a.router.request(send("2", json!({"to": "ana@example.org"})), T0);
    let after = std::fs::read_to_string(home.join(super::audit::AUDIT_FILE)).unwrap();
    assert!(after.starts_with(&before) && after.lines().count() == 2);
    let _ = std::fs::remove_dir_all(&home);
}

// ---------------------------------------------------------------- consent

fn news() -> AgentSummary {
    AgentSummary::from_manifest(
        "os.news",
        "News",
        &json!({"capabilities": ["octos.session", "research", "crawl"], "storage": {"accounts": false, "agent_workspace": "account"}}),
        &["octos.session".into(), "research".into()],
        "The model set in AI providers",
    )
}

/// The consent sheet and app storage read one `storage` block the same
/// way: `accounts` defaults to false (one `device` folder) in both.
#[test]
fn consent_and_app_storage_agree_on_accounts() {
    use crate::app_storage::{AppKind, StorageSpec};
    for manifest in [
        json!({}),
        json!({"storage": {}}),
        json!({"storage": {"agent_workspace": "account"}}),
        json!({"storage": {"accounts": false}}),
        json!({"storage": {"accounts": true}}),
        json!({"storage": {"accounts": true, "agent_workspace": "account"}}),
    ] {
        let per_account = StorageSpec::from_manifest(&manifest, AppKind::Script).unwrap().accounts;
        let summary = AgentSummary::from_manifest("os.x", "X", &manifest, &[], "m");
        let says_account = summary.reads.iter().any(|r| r.contains("signed-in account"));
        let says_device = summary.reads.iter().any(|r| r.contains("on this device"));
        assert_eq!((says_account, says_device), (per_account, !per_account), "{manifest}");
    }
    assert!(!StorageSpec::from_manifest(&json!({}), AppKind::Script).unwrap().accounts, "an app declares accounts");
}

#[test]
fn consent_at_first_use_is_asked_once_and_remembered() {
    let mut c = ConsentStore::memory();
    let s = news();
    assert!(s.reads.iter().any(|r| r.contains("on this device")));
    assert!(s.uses.iter().any(|u| u.contains("toolbox")), "{:?}", s.uses);
    assert!(!s.uses.iter().any(|u| u.contains("Crawling")), "only what was granted");
    assert!(!c.granted("os.news", false));
    assert_eq!(c.ask(s.clone(), false), State::Undecided);
    assert_eq!(c.ask(s.clone(), false), State::Undecided);
    assert_eq!(c.prompt().map(|p| p.app.as_str()), Some("os.news"));
    c.set(&ApprovalGesture::sheet_tap(), "os.news", true, T0);
    assert!(c.prompt().is_none());
    assert!(c.granted("os.news", false));
    assert_eq!(c.ask(s, false), State::Allowed, "no second prompt");
    // Settings' off switch.
    c.turn_off("os.news", T0);
    assert!(!c.granted("os.news", false));
    assert_eq!(c.agents(), vec![("os.news".to_string(), "News".to_string(), State::Denied)]);
}

#[test]
fn developer_mode_skips_the_consent_prompt() {
    let mut c = ConsentStore::memory();
    assert_eq!(c.ask(news(), true), State::Allowed);
    assert!(c.prompt().is_none());
    assert!(c.granted("os.news", true));
    assert!(!c.granted("os.news", false), "nothing is remembered from developer mode");
}

// ---------------------------------------------------------------- who may create

/// Only the shell's Settings page and its sheet make a ApprovalGesture: no
/// agent-, app- or relay-facing code can create a rule or give consent.
#[test]
fn only_settings_and_the_sheet_make_a_person_gesture() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);
    let allowed = ["approvals/view.rs", "approvals/settings_page.rs", "approvals/rules.rs", "approvals/tests.rs", "system_chat/tests.rs"];
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let rel = f.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
        if text.contains("ApprovalGesture::sheet_tap") || text.contains("ApprovalGesture::settings_tap") || text.contains("ApprovalGesture {") {
            assert!(allowed.contains(&rel.as_str()), "{rel} makes a ApprovalGesture");
        }
    }
}

// ---------------------------------------------------------------- the relay seam

#[test]
fn the_relay_gets_decisions_made_before_it_was_installed() {
    super::init_memory();
    let route = super::approval_requested("os.mail", ToolSpec::host("mail.archive").not_auto_approvable(), json!({"id": 1}), Caller::SystemAgent, ctx("g1", Trigger::Person));
    assert!(matches!(route, Route::Sheet(_)));
    let (sheet, id) = super::with(|a| {
        let s = a.router.front_sheet().unwrap();
        (s.id, s.lines[0].request.clone())
    })
    .unwrap();
    super::with(|a| a.router.answer(sheet, &id, Answer::Deny, &ApprovalGesture::sheet_tap(), T0)).unwrap().unwrap();
    let relay = RecordingRelay::default();
    super::set_relay(Box::new(relay.clone()));
    assert_eq!(relay.take(), vec![(RequestId("g1".into()), Decision::Deny, "denied on the sheet".into())]);
    // From now on, straight to it.
    super::approval_requested("os.mail", ToolSpec::host("mail.archive").not_auto_approvable(), json!({"id": 2}), Caller::SystemAgent, ctx("g2", Trigger::Person));
    let (sheet, id) = super::with(|a| {
        let s = a.router.front_sheet().unwrap();
        (s.id, s.lines[0].request.clone())
    })
    .unwrap();
    super::with(|a| a.router.answer(sheet, &id, Answer::Once, &ApprovalGesture::sheet_tap(), T0)).unwrap().unwrap();
    assert_eq!(relay.take().len(), 1);
    // The AI bus's held `confirm: host` calls come back to the shell, not
    // to the relay: the Terminal's `run` asks the person, never a rule.
    super::with(|a| a.router.create_rule(&ApprovalGesture::settings_tap(), RuleDraft::everything("terminal", 30), T0)).unwrap().unwrap();
    let held = crate::ai_bus::HeldCall {
        key: "bus:w4:c1".into(),
        app: "terminal".into(),
        tool: "run".into(),
        args: r#"{"command":"ls"}"#.into(),
        auto_approvable: false,
        command: true,
    };
    assert!(matches!(super::bus_requested(&held), Route::Sheet(_)));
    let (sheet, id, caller, args) = super::with(|a| {
        let s = a.router.front_sheet().unwrap();
        (s.id, s.lines[0].request.clone(), s.lines[0].caller.clone(), s.lines[0].args.join(" "))
    })
    .unwrap();
    assert_eq!(id, RequestId("bus:w4:c1".into()));
    assert_eq!(caller, "The system agent");
    assert!(args.contains("\"command\": \"ls\""), "{args}");
    super::with(|a| a.router.answer(sheet, &id, Answer::Once, &ApprovalGesture::sheet_tap(), T0)).unwrap().unwrap();
    assert!(relay.take().is_empty());
    assert_eq!(super::take_bus_decisions(), vec![(id, Decision::ApproveOnce, "approved on the sheet".into())]);
    assert!(!super::consent_granted("os.news"), "developer mode is off in tests: nothing is granted");
}

/// The module host's gate: the first ask shows the first-use sheet and
/// offers nothing; once allowed, the next instance gets its agent.
#[test]
fn the_module_gate_asks_once_then_follows_consent() {
    let mut a = super::Approvals::memory();
    assert!(!super::module_gate(&mut a, "rinx", "Rinx", &["octos.session.open", "octos.turn.start"]));
    assert_eq!(a.consent.prompt().map(|p| p.name.as_str()), Some("Rinx"));
    assert!(!super::module_gate(&mut a, "rinx", "Rinx", &["octos.session.open"]), "no second prompt, still no agent");
    a.consent.set(&ApprovalGesture::sheet_tap(), "rinx", true, T0);
    assert!(super::module_gate(&mut a, "rinx", "Rinx", &["octos.session.open"]));
    a.consent.turn_off("rinx", T0);
    assert!(!super::module_gate(&mut a, "rinx", "Rinx", &["octos.session.open"]));
    assert!(a.consent.prompt().is_none(), "a person's no is not asked again");
}
