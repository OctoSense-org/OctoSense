//! Deterministic, read-only app-to-app transport over the existing tool relay.
//! Identities come from ServiceCall and admitted bundles, never from payload data.
use std::{collections::HashMap, path::PathBuf, sync::{Mutex, OnceLock, atomic::{AtomicU64, Ordering}}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use serde_json::{Value, json};
use octosense_appstore::services::{HostService, ServiceCall, ServiceHost, Replier, HostApiMethod, AgentAccess};
use crate::ai_host::app_peers::{TurnTrigger, host_tools::{HostToolCall, ToolReply, CallerKind, CallOrigin}};

#[derive(Clone, PartialEq)]
struct Authority { caller: String, owner: String, root: PathBuf, caller_account: String, owner_account: String, epoch: u64, caller_manifest: Value, owner_manifest: Value }
impl Authority {
    fn capture(caller: &str, owner: &str, root: &std::path::Path) -> Result<Self,String> {
        let a = super::script_apps::admitted_host(caller, root)?;
        let b = super::script_apps::admitted_host(owner, root)?;
        if !crate::approvals::consent_granted(caller) || !crate::approvals::consent_granted(owner) { return Err("NO_CONSENT".into()); }
        let caller_account = super::script_apps::current_script_account(root, caller);
        let owner_account = super::script_apps::current_script_account(root, owner);
        if super::suspended(&format!("card.{caller}"),Some(&caller_account)) || super::suspended(&format!("card.{owner}"),Some(&owner_account)) { return Err("REVOKED".into()); }
        Ok(Self { caller: caller.into(), owner: owner.into(), root: root.into(), caller_account, owner_account, epoch: crate::approvals::consent_epoch(), caller_manifest: a.manifest, owner_manifest: b.manifest })
    }
    fn live(&self) -> bool { Self::capture(&self.caller,&self.owner,&self.root).is_ok_and(|now| now == *self) }
}
struct Pending { authority: Authority, request: String, generation: u64, tool: String, reply: Replier, tool_reply: ToolReply }
static PENDING: OnceLock<Mutex<HashMap<String,Pending>>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);
static FOCUS: AtomicU64 = AtomicU64::new(0);
static BOOT: OnceLock<String> = OnceLock::new();
struct Watch { authority: Authority, reply: Replier, at: Instant, checked: Instant }
static WATCHES: OnceLock<Mutex<Vec<Watch>>> = OnceLock::new();
fn watches() -> std::sync::MutexGuard<'static,Vec<Watch>> { WATCHES.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner()) }
fn pending() -> std::sync::MutexGuard<'static,HashMap<String,Pending>> { PENDING.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner()) }
fn epoch(generation: u64) -> String {
    let boot = BOOT.get_or_init(|| SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos().to_string());
    format!("{boot}:{generation}")
}
fn finish(id: &str, fields: Value) {
    if fields.get("status").is_some() { return; }
    let p = pending().remove(id);
    let Some(p) = p else { return; };
    if !p.authority.live() { p.reply.send(Err("REVOKED".into())); return; }
    if fields["ok"] != true {
        p.reply.send(Err(fields["error"]["kind"].as_str().unwrap_or("TRANSPORT_UNAVAILABLE").into())); return;
    }
    p.reply.send(Ok(json!({"binding":{"owner_app":p.authority.owner,"caller_app":p.authority.caller,"tool_name":p.tool,"auth_epoch":epoch(p.authority.epoch),"account":p.authority.owner_account},"request_id":p.request,"generation":p.generation,"reply":fields["data"]})));
}
fn cancel(id: &str, reason: &str) {
    let p = pending().remove(id);
    if let Some(p) = p {
        p.tool_reply.cancel();
        p.reply.send(Err(reason.into()));
        super::submit(super::Event::Cancel { call_id:id.into(),reason:reason.into() });
    }
}
pub fn poll() {
    // No lock is held while inspecting admission/consent or settling replies.
    let checks: Vec<_> = pending().iter().map(|(id,p)|(id.clone(),p.authority.clone(),p.reply.clone())).collect();
    for (id,a,reply) in checks {
        if !reply.is_pending() { cancel(&id,"CANCELLED"); }
        else if !a.live() { cancel(&id,"REVOKED"); }
    }
    let checks = std::mem::take(&mut *watches());
    for mut watch in checks {
        if !watch.reply.is_pending() { continue; }
        let changed = crate::approvals::consent_epoch()!=watch.authority.epoch;
        let refresh = watch.checked.elapsed()>=Duration::from_millis(250);
        if changed || (refresh && !watch.authority.live()) { watch.reply.send(Err("REVOKED".into())); }
        else if watch.at.elapsed()>=Duration::from_secs(30) { watch.reply.send(Ok(json!({"changed":false,"auth_epoch":epoch(watch.authority.epoch)}))); }
        else { if refresh { watch.checked=Instant::now(); } watches().push(watch); }
    }
}
struct ReadBridge;
fn input_schema(name: &str) -> Value {
    let text = json!({"type":"string","minLength":1,"maxLength":128});
    let generation = json!({"type":"integer","minimum":1});
    match name {
        "read" => json!({"type":"object","properties":{"source_app":text,"tool":{"type":"string","minLength":1,"maxLength":256},"args":{"type":"object"},"request_id":text,"generation":generation},"required":["source_app","tool","args","request_id","generation"],"additionalProperties":false}),
        "cancel" => json!({"type":"object","properties":{"request_id":text,"generation":generation},"required":["request_id","generation"],"additionalProperties":false}),
        "watch_authorization" => json!({"type":"object","properties":{"source_app":text,"auth_epoch":{"type":"string","minLength":1}},"required":["source_app","auth_epoch"],"additionalProperties":false}),
        _ => json!({"type":"object","properties":{},"additionalProperties":false}),
    }
}
impl HostService for ReadBridge {
    fn family(&self) -> &'static str { "app_tools" }
    fn api_methods(&self) -> Vec<HostApiMethod> {
        ["read", "cancel", "focus", "watch_authorization"].into_iter().map(|name|
            HostApiMethod::new(format!("app_tools.{name}"),1,"app_tools",
                match name { "read"=>"Invoke an admitted shared read tool with native caller and authorization binding", "cancel"=>"Cancel this app's pending read request", "focus"=>"Activate this admitted app's own window", _=>"Wait for authorization or admitted identity changes without reading source data" },
                input_schema(name), json!({"type":"object"}))
                // Agent hooks run without foreground prompt authority. These
                // methods still enforce admission, consent and identity below;
                // Allowed does not grant data access or permission to prompt.
                .with_platforms(&["windows"]).with_agent_access(AgentAccess::Allowed)).collect()
    }
    fn timeout(&self,_: &ServiceCall) -> Duration { Duration::from_secs(45) }
    fn call(&mut self,call: ServiceCall,reply: Replier,_: &mut dyn ServiceHost) {
        if call.from_sheet || !reply.is_pending() { reply.send(Err("INVALID_ARGUMENT".into())); return; }
        if let Err(e) = super::script_apps::admitted_host(&call.app_id,&call.host_dir) { reply.send(Err(e));return; }
        if call.method() == "watch_authorization" {
            let Some(owner) = call.args["source_app"].as_str() else { reply.send(Err("INVALID_ARGUMENT".into()));return; };
            let granted = super::script_apps::guidance(&call.app_id).ok().zip(super::script_apps::guidance(owner).ok())
                .is_some_and(|(caller,target)| caller.asks.iter().any(|tool| target.script_tools.contains(tool) && target.tools.iter().any(|t|t["name"]==*tool && t["risk"]=="read" && t["shareable"]==true)));
            if !granted { reply.send(Err("NOT_GRANTED".into()));return; }
            let authority = match Authority::capture(&call.app_id,owner,&call.host_dir) { Ok(v)=>v,Err(e)=>{reply.send(Err(e));return;} };
            if call.args["auth_epoch"] != epoch(authority.epoch) { reply.send(Err("REVOKED".into()));return; }
            let mut q=watches();
            q.retain(|w| {
                if w.authority.caller==call.app_id&&w.authority.owner==owner { w.reply.clone().send(Err("REPLACED".into()));false }
                else { w.reply.is_pending() }
            });
            if q.len()>=16 { reply.send(Err("APP_BUSY".into()));return; }
            q.push(Watch{authority,reply,at:Instant::now(),checked:Instant::now()});return;
        }
        if call.method() == "cancel" {
            let Some(request) = call.args["request_id"].as_str() else { reply.send(Err("INVALID_ARGUMENT".into()));return; };
            let Some(generation) = call.args["generation"].as_u64() else { reply.send(Err("INVALID_ARGUMENT".into()));return; };
            let ids:Vec<_> = pending().iter().filter(|(_,p)|p.authority.caller == call.app_id && p.request == request && p.generation == generation).map(|(id,_)|id.clone()).collect();
            for id in ids { cancel(&id,"CANCELLED"); }
            reply.send(Ok(json!({"cancelled":true})));return;
        }
        if call.method() == "focus" {
            let authority = match Authority::capture(&call.app_id,&call.app_id,&call.host_dir) { Ok(v)=>v,Err(e)=>{reply.send(Err(e));return;} };
            if FOCUS.fetch_add(1,Ordering::SeqCst)>=16 { FOCUS.fetch_sub(1,Ordering::SeqCst);reply.send(Err("APP_BUSY".into()));return; }
            std::thread::spawn(move || {
                if reply.is_pending() && authority.live() {
                    let result = super::activation::request(&call.app_id);
                    if authority.live() { reply.send(result.map(|()|json!({"app":call.app_id,"mounted":true}))); }
                    else { reply.send(Err("REVOKED".into())); }
                } else if reply.is_pending() {
                    reply.send(Err("REVOKED".into()));
                }
                FOCUS.fetch_sub(1,Ordering::SeqCst);
            });return;
        }
        if call.method() != "read" { reply.send(Err("UNSUPPORTED".into()));return; }
        let parsed = (|| {
            let owner = call.args["source_app"].as_str().ok_or("INVALID_ARGUMENT")?;
            let tool = call.args["tool"].as_str().ok_or("INVALID_ARGUMENT")?;
            let request = call.args["request_id"].as_str().filter(|v|!v.is_empty()&&v.len()<=128).ok_or("INVALID_ARGUMENT")?;
            let generation = call.args["generation"].as_u64().filter(|v|*v>0).ok_or("INVALID_ARGUMENT")?;
            let caller = super::script_apps::guidance(&call.app_id)?;
            let target = super::script_apps::guidance(owner)?;
            if owner == call.app_id || !caller.asks.iter().any(|t|t==tool) { return Err("NOT_GRANTED".into()); }
            if !target.script_tools.contains(tool) || !target.tools.iter().any(|t|t["name"]==tool && t["risk"]=="read" && t["shareable"]==true && t.get("confirm").is_none()) { return Err("READ_ONLY_REQUIRED".into()); }
            if call.may_prompt {
                if !crate::approvals::consent_for_contained(&call.app_id) || !crate::approvals::consent_for_contained(owner) { return Err("NO_CONSENT".into()); }
            }
            let authority = Authority::capture(&call.app_id,owner,&call.host_dir)?;
            // Load the exact owner before resolving the caller's grants.
            super::script_apps::load(owner)?;
            super::script_apps::load(&call.app_id)?;
            if super::script_apps::owner_for(tool).as_deref()!=Some(owner) { return Err("IDENTITY_MISMATCH".into()); }
            Ok((authority,tool.to_owned(),request.to_owned(),generation))
        })();
        let (authority,tool,request,generation) = match parsed { Ok(v)=>v,Err(e)=>{reply.send(Err(e));return;} };
        let id = format!("app-read-{}",NEXT.fetch_add(1,Ordering::Relaxed));
        let done_id = id.clone();
        let tool_reply = ToolReply::new(&id,move |fields|finish(&done_id,fields));
        let invocation = HostToolCall {
            call_id:id.clone(),tool_call_id:String::new(),args_digest:String::new(),name:tool.clone(),app:authority.owner.clone(),args:call.args["args"].clone(),risk:"read".into(),confirm_required:false,timeout_ms:20000,tools_version:0,
            peer:None,session_id:format!("host-app-tools:{}",call.app_id),context_id:None,turn_id:id.clone(),caller_kind:CallerKind::AppPeer,calling_app:format!("card.{}",call.app_id),peer_workspace:None,account:Some(authority.caller_account.clone()),client:None,origin:CallOrigin::Context,trigger:TurnTrigger::App,
        };
        {
            let mut map = pending();
            if map.len()>=16 || map.values().any(|p|p.authority.caller==call.app_id&&p.request==request) { reply.send(Err("APP_BUSY".into()));return; }
            map.insert(id.clone(),Pending{authority:authority.clone(),request,generation,tool,reply:reply.clone(),tool_reply:tool_reply.clone()});
        }
        std::thread::spawn(move || {
            if !reply.is_pending() || !authority.live() { cancel(&id,"REVOKED");return; }
            if super::activation::request(&authority.owner).is_err() { cancel(&id,"APP_NOT_RUNNING");return; }
            if !reply.is_pending() || !authority.live() { cancel(&id,"REVOKED");return; }
            super::submit(super::Event::Call{call:invocation,reply:tool_reply});
        });
    }
}
pub fn register() { octosense_appstore::services::register_host_service(Box::new(ReadBridge)); }

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    struct NoSheet;
    impl ServiceHost for NoSheet {
        fn open_sheet(&mut self, _: String) { panic!("background reads must not request consent implicitly"); }
        fn close_sheet(&mut self) {}
    }

    #[test]
    fn background_bridge_reaches_admission_but_never_grants_an_unknown_caller() {
        use octosense_appstore::services::{dispatch, take_replies_for};
        register();
        let app = "invalid/background-bridge-test";
        let root = PathBuf::new();
        let expected = super::super::script_apps::admitted_host(app, &root)
            .err().expect("invalid caller must be rejected");
        let mut host = NoSheet;
        let heap = usize::MAX - 9321;
        for (index, method, args) in [
            (1, "read", json!({"source_app":"fixture","tool":"fixture.read","args":{},"request_id":"test","generation":1})),
            (2, "cancel", json!({"request_id":"test","generation":1})),
            (3, "focus", json!({})),
            (4, "watch_authorization", json!({"source_app":"fixture","auth_epoch":"test"})),
        ] {
            dispatch(ServiceCall {app_id:app.into(),service:format!("app_tools.{method}"),args,
                from_sheet:false,may_prompt:false,host_dir:root.clone()},heap,index,&mut host);
            let replies = take_replies_for(&[heap]);
            assert_eq!(replies.len(), 1, "{method} must settle exactly once");
            assert_eq!(replies[0].2.as_ref().unwrap_err(), &expected,
                "{method} must reach admission, not fail at the generic background gate");
        }
    }
}
