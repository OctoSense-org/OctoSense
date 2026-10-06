//! News owns the story, the research grant, and its Glance publication.
//! Research uses the same scoped octos toolbox executor as the News peer.
//! The UI requests a job and remains responsive while it runs.
use octosense_news_service::Item;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const APP: &str = "os.news";
const CARD: &str = include_str!("../../../apps/news/bundle/story.card");
#[derive(Clone)]
struct Running {
    call_id:String,
    account:String,
    consent_generation:Option<u64>,
    started:std::time::Instant,
    item: Item,
    publication: Option<crate::glance::GlanceCard>,
}
static RUNNING: OnceLock<Mutex<HashMap<String,Running>>> = OnceLock::new();
static FOCUS: Mutex<Option<String>> = Mutex::new(None);
static PUBLICATIONS: Mutex<()> = Mutex::new(());

pub(crate) fn valid_story_id(id: &str) -> bool { id.len()==16 && id.bytes().all(|b|b.is_ascii_hexdigit()) }
fn id(item: &Item) -> String { format!("story-{}", item.id.chars().take(32).collect::<String>()) }
fn card_source(card_id: &str) -> String { CARD.replace("\"{thread}\"", &json!(card_id).to_string()) }
fn running() -> &'static Mutex<HashMap<String,Running>> { RUNNING.get_or_init(Default::default) }

/// Cheap timer hook: revoke the synthetic host workflow just as an app peer's
/// lifecycle revokes its calls. No filesystem, provider work or relay lock here.
pub fn tick() {
    // Snapshot only lifecycle metadata; large research/card data is moved out
    // only for cancellation, never cloned on every UI timer tick.
    let snapshot:Vec<_>=running().lock().unwrap_or_else(|e|e.into_inner()).iter()
        .map(|(id,j)|(id.clone(),j.call_id.clone(),j.account.clone(),j.consent_generation,j.started)).collect();
    if snapshot.is_empty() { return; }
    let consent=crate::approvals::with(|a|a.consent.generation());
    let allowed=crate::agents::access(APP)==crate::agents::Access::Allowed;
    let account=crate::ai_host::contained::account_of(APP);
    let mut cancelled=Vec::new();
    for (id,call_id,job_account,job_consent,started) in snapshot {
        if allowed && consent==job_consent && account.as_deref()==Some(&job_account)
            && started.elapsed()<std::time::Duration::from_secs(125) {continue;}
        let removed={
            let mut active=running().lock().unwrap_or_else(|e|e.into_inner());
            if active.get(&id).is_some_and(|current|current.call_id==call_id) {active.remove(&id)} else {None}
        };
        if let Some(job)=removed {
            crate::host_tools::submit(crate::host_tools::Event::Cancel {call_id:job.call_id.clone(),reason:"News research consent/account changed or deadline elapsed".into()});
            cancelled.push(job);
        }
    }
    if !cancelled.is_empty() {
        // Status persistence is off the UI thread. Exact-publication/account
        // guards prevent a cancellation from reviving or replacing a card.
        let _=std::thread::Builder::new().name("news-cancel-status".into()).spawn(move || {
            for job in cancelled { if let Some(card)=job.publication {
                let _=publish_current(&job.item,&json!({"summary":"","points":[],"sources":[]}),"failed",Some(&card));
            }}
        });
    }
}

pub fn focus_story(id: &str) -> Result<(), String> {
    if !valid_story_id(id) { return Err("Invalid News story".into()); }
    *FOCUS.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.into());
    Ok(())
}

pub fn view() -> Result<Value, String> {
    Ok(json!({"id": FOCUS.lock().unwrap_or_else(|e| e.into_inner()).take()}))
}

fn result(item: &Item) -> Value {
    crate::glance_digest::resolve(crate::glance_digest::digest_root().as_deref(), APP, &id(item), crate::glance::now_ms()).value
}

pub fn publish_args(item: &Item, brief: &Value, status: &str) -> Value {
    let card_id = id(item);
    let source = card_source(&card_id);
    json!({"card_id":card_id, "title":item.title.chars().take(crate::glance::TITLE_MAX).collect::<String>(),
        "summary":item.summary.chars().take(crate::glance::SUMMARY_MAX).collect::<String>(), "source":source,
        "data":{"story":{"id":item.id,"title":item.title,"summary":item.summary,"subtitle":item.source,
            "status":status,"coverage":"","url1":format!("app://news/story/{}",item.id),"url2":format!("app://news/research/{}",item.id)}, "brief":brief},
        "open":{"app":"news","route":format!("story/{}",item.id)},"notify":false,"priority":35})
}

fn publish(item: &Item, brief: &Value, status: &str) -> Result<Value, String> {
    publish_current(item, brief, status, None)
}

fn publish_current(item: &Item, brief: &Value, status: &str, expected: Option<&crate::glance::GlanceCard>) -> Result<Value, String> {
    let mut args = publish_args(item, brief, status);
    // Distinguish jobs even if two publications share a millisecond clock tick.
    if status=="running" { args["data"]["story"]["job_nonce"]=json!(uuid::Uuid::new_v4().to_string()); }
    let receipt = match expected {
        Some(card) => crate::glance::publish_for_if_current(card, &args)?,
        None => crate::glance::publish_for(APP, &args)?,
    };
    if let Some(storage) = crate::app_storage::host() {
        let host = storage.layout().apps_root().join(".host");
        // Same lock order as dismissal. If the person dismissed/replaced it
        // between admission and persistence, do not resurrect it on restart.
        let _publication = crate::mail_card::publication_guard();
        let Some(current) = crate::glance::card(&format!("{APP}/{}", id(item))) else { return Ok(receipt); };
        if current.expires_ms != receipt["expires_at"].as_u64().unwrap_or(0)
            || current.l0.as_ref().is_none_or(|l| l.source != args["source"].as_str().unwrap_or("") || l.data["story"] != args["data"]["story"]) { return Ok(receipt); }
        let _guard = PUBLICATIONS.lock().unwrap_or_else(|e| e.into_inner());
        let mut cards = read_cards(&host)?;
        let now = crate::glance::now_ms();
        cards.retain(|p| p["id"] != id(item) && p["expires_at"].as_u64().unwrap_or(0) > now);
        cards.push(json!({"id":id(item),"item":item,"args":args,"account":current.account,"expires_at":receipt["expires_at"],"dismissed":false}));
        if cards.len()>32 { cards.drain(..cards.len()-32); }
        write_cards(&host,&cards)?;
    }
    Ok(receipt)
}

fn read_cards(host: &std::path::Path) -> Result<Vec<Value>, String> {
    let path = host.join("news/cards.json");
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind()==std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    if file.metadata().map_err(|e|e.to_string())?.len()>2*1024*1024 { return Err("News card archive is too large".into()); }
    serde_json::from_reader(file).map_err(|_|"News card archive is damaged".into())
}
fn write_cards(host: &std::path::Path,cards:&[Value]) -> Result<(),String> {
    let path=host.join("news/cards.json");
    crate::approvals::write_private(&path,&serde_json::to_vec(cards).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
}
pub fn set_dismissed(host:&std::path::Path,id:&str,dismissed:bool)->Result<(),String>{
    let _guard=PUBLICATIONS.lock().unwrap_or_else(|e|e.into_inner());
    let mut cards=read_cards(host)?;
    if let Some(card)=cards.iter_mut().find(|c|c["id"]==id){card["dismissed"]=json!(dismissed);write_cards(host,&cards)?;}
    Ok(())
}
pub fn restore(host:&std::path::Path)->Result<usize,String>{
    let cards={let _guard=PUBLICATIONS.lock().unwrap_or_else(|e|e.into_inner());read_cards(host)?};
    let now=crate::glance::now_ms(); let mut restored=0;
    for card in cards {
        let remaining=card["expires_at"].as_u64().unwrap_or(0).saturating_sub(now)/1000;
        if card["dismissed"]==true || remaining<60 {continue;}
        if let Some(account)=card["account"].as_str() {
            if crate::ai_host::contained::account_of(APP).as_deref()!=Some(account) {continue;}
        }
        let item:Item=serde_json::from_value(card["item"].clone()).map_err(|_|"Invalid stored News story")?;
        let brief=result(&item);
        // A killed process cannot still be researching. Persisted successful
        // results are re-read from the host toolbox, never from a model path.
        let status=brief["status"].as_str().unwrap_or("failed");
        let mut args=publish_args(&item,&brief,status);
        args["expires"]=json!(remaining.min(crate::glance::EXPIRES_MAX_S));
        crate::glance::publish_for(APP,&args)?;restored+=1;
    }
    Ok(restored)
}

/// A capability bound to one canonical host-created story publication. A URL
/// in arbitrary model-authored data cannot create this capability.
#[derive(Clone)]
pub(crate) struct ResearchAction { card: crate::glance::GlanceCard, item_id: String }
impl ResearchAction {
    pub(crate) fn from_card(card: &crate::glance::GlanceCard) -> Option<Self> {
        let l0=card.l0.as_ref()?;
        let item_id=l0.data["story"]["id"].as_str()?;
        if card.app!=APP || !card.contained || !valid_story_id(item_id)
            || card.card_id!=format!("story-{item_id}") || card.open_app!="news"
            || card.route.as_deref()!=Some(format!("story/{item_id}").as_str()) || l0.source!=card_source(&card.card_id) { return None; }
        Some(Self {card:card.clone(),item_id:item_id.into()})
    }
    pub(crate) fn accepts(&self, url:&str) -> bool { url==format!("app://news/research/{}",self.item_id) }
    fn current(&self) -> bool { self.card.account_valid() && crate::glance::card(&self.card.key()).as_ref()==Some(&self.card) }
}

/// Called only by the native card tap dispatcher with a host-created binding.
/// All archive/tool/catalog work happens off the UI thread.
pub(crate) fn research_from_card(action: ResearchAction) -> Result<(),String> {
    if crate::agents::access(APP)!=crate::agents::Access::Allowed { return Err("Allow News's assistant in Settings before researching".into()); }
    if !action.current() {return Err("This story card changed; reopen it before researching".into());}
    let storage=crate::app_storage::host().ok_or("News storage unavailable")?;
    let host=storage.layout().apps_root().join(".host");
    std::thread::Builder::new().name("news-card-action".into()).spawn(move || {
        let selected=(|| -> Result<Item,String> {
            let cards={let _guard=PUBLICATIONS.lock().unwrap_or_else(|e|e.into_inner());read_cards(&host)?};
            let card=cards.iter().find(|c|c["id"]==action.card.card_id && c["dismissed"]!=true).ok_or("Story card was dismissed")?;
            let item:Item=serde_json::from_value(card["item"].clone()).map_err(|_|"Stored story unavailable")?;
            if item.id!=action.item_id || !action.current() {return Err("Story card changed".into());}
            Ok(item)
        })();
        if let Ok(item)=selected {
            if research_current(&item,&json!({"language":"en"}),Some(&action.card)).is_err() {
                let _=publish_current(&item,&result(&item),"failed",Some(&action.card));
            }
        }
    }).map_err(|_|"News research worker unavailable")?;
    Ok(())
}

/// Host-service callback. The News service already resolved this exact item
/// from its own store; no model-authored title, URL or run path is accepted.
pub fn call(method: &str, item: &Item, args: &Value) -> Result<Value, String> {
    let brief = result(item);
    match method {
        "publish_card" => publish(item, &brief, brief["status"].as_str().unwrap_or("missing")),
        "research_result" => Ok(json!({"id":item.id,"run_id":id(item),"running":running().lock().unwrap_or_else(|e| e.into_inner()).contains_key(&id(item)),"research":brief})),
        "research" => research(item, args),
        _ => Err("Unknown News card method".into()),
    }
}

#[cfg(feature = "toolbox-peers")]
fn research(item: &Item, args: &Value) -> Result<Value, String> {
    research_current(item, args, None)
}

#[cfg(feature = "toolbox-peers")]
fn research_current(item: &Item, args: &Value, expected: Option<&crate::glance::GlanceCard>) -> Result<Value, String> {
    use crate::ai_host::app_peers::host_tools::{HostToolCall, ToolReply};
    use crate::ai_host::app_peers::TurnTrigger;
    if crate::agents::access(APP) != crate::agents::Access::Allowed { return Err("Allow News's assistant in Settings before researching".into()); }
    let account = crate::ai_host::contained::account_of(APP).ok_or("News account is unavailable")?;
    let run_id = id(item);
    let call_id = format!("news-research-{}", uuid::Uuid::new_v4());
    let consent_generation=crate::approvals::with(|a|a.consent.generation());
    let language = args["language"].as_str().unwrap_or("en");
    if !matches!(language, "en" | "zh") { return Err("Research language must be en or zh".into()); }
    {
        let mut active = running().lock().unwrap_or_else(|e| e.into_inner());
        if active.contains_key(&run_id) { return Ok(json!({"status":"running","run_id":run_id})); }
        active.insert(run_id.clone(),Running {call_id:call_id.clone(),account:account.clone(),consent_generation,started:std::time::Instant::now(),item:item.clone(),publication:None});
    }
    if let Err(error) = publish_current(item, &json!({"summary":"","points":[],"sources":[]}), "running", expected) {
        let mut active=running().lock().unwrap_or_else(|e|e.into_inner());
        if active.get(&run_id).is_some_and(|job|job.call_id==call_id) {active.remove(&run_id);}
        return Err(error);
    }
    let publication=crate::glance::card(&format!("{APP}/{run_id}")).filter(|p|p.l0.as_ref().is_some_and(|l|l.data["story"]["status"]=="running") && p.account.as_deref()==Some(&account));
    {
        let mut active=running().lock().unwrap_or_else(|e|e.into_inner());
        if let Some(job)=active.get_mut(&run_id).filter(|j|j.call_id==call_id) {
            job.publication=publication.clone();
        } else {
            drop(active);
            if let Some(card)=&publication {let _=publish_current(item,&json!({"summary":"","points":[],"sources":[]}),"failed",Some(card));}
            return Err("News research was cancelled before it started".into());
        }
    }
    let params = json!({"call_id":call_id,"name":"workflow.run","app":"toolbox","peer":"card.os.news",
        "session_id":format!("news-card-{run_id}"),"turn_id":call_id,"timeout_ms":120000,
        "args":{"id":"topic-brief","run_id":run_id,"params":{
            "topic":item.title.chars().take(160).collect::<String>(),"language":language,
            "languages":[{"language":language,"translate":false}],"read_top":4,"per_language":4,"max_age_hours":168}}});
    let mut call = HostToolCall::parse(&params)?;
    call.calling_app = "card.os.news".into();
    call.account = Some(account.clone());
    // This bounded host workflow is read-only. Approval provenance is never
    // promoted to a trusted physical send/act event.
    call.trigger = TurnTrigger::Unknown;
    let item = item.clone();
    let completed_id = run_id.clone();
    let completed_call_id=call_id.clone();
    let reply = ToolReply::new(call_id.clone(), move |outcome| {
        {
            let mut active=running().lock().unwrap_or_else(|e|e.into_inner());
            if !active.get(&completed_id).is_some_and(|job|job.call_id==completed_call_id) {return;}
            active.remove(&completed_id);
        }
        if crate::approvals::with(|a|a.consent.generation())!=consent_generation || crate::agents::access(APP) != crate::agents::Access::Allowed || crate::ai_host::contained::account_of(APP).as_deref() != Some(&account) { return; }
        let brief = result(&item);
        let status = if outcome["ok"] == true { brief["status"].as_str().unwrap_or("failed") } else { "failed" };
        // Keep an explicit failure/partial state; never fabricate a report.
        if let Some(publication)=&publication { let _ = publish_current(&item, &brief, status, Some(publication)); }
    });
    // A service executor may be called while the relay lock is held. Loading
    // the peer tool catalog here would re-enter that lock; do it off-thread.
    let failed_reply = reply.clone();
    let queued_id=run_id.clone();
    if std::thread::Builder::new().name("news-research".into()).spawn(move || {
        if let Err(error) = crate::host_tools::script_apps::load(APP) {
            reply.finish(crate::ai_host::app_peers::host_tools::ToolOutcome::error("unavailable",error));
            return;
        }
        // Serialize enqueue with tick's removal: cancellation cannot race
        // ahead of a not-yet-submitted call and leave an orphan workflow.
        let active=running().lock().unwrap_or_else(|e|e.into_inner());
        if !active.get(&queued_id).is_some_and(|job|job.call_id==call_id) {return;}
        crate::host_tools::submit(crate::host_tools::Event::Call { call, reply });
    }).is_err() {
        failed_reply.finish(crate::ai_host::app_peers::host_tools::ToolOutcome::error("unavailable","Research worker unavailable"));
        return Err("Research worker unavailable".into());
    }
    Ok(json!({"status":"running","run_id":run_id,"card_id":run_id}))
}

#[cfg(not(feature = "toolbox-peers"))]
fn research(_: &Item, _: &Value) -> Result<Value, String> { Err("This build has no research toolbox".into()) }
#[cfg(not(feature = "toolbox-peers"))]
fn research_current(_: &Item, _: &Value, _: Option<&crate::glance::GlanceCard>) -> Result<Value, String> { Err("This build has no research toolbox".into()) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stored_story_has_real_route_research_and_shared_chat() {
        let item = Item { id:octosense_news_service::item::item_id("https://example.org/science"),title:"A quoted \"topic\"".repeat(20),summary:"Stored feed summary".repeat(20),source:"Fixture publisher".into(),..Default::default() };
        let args = publish_args(&item,&json!({"summary":"Cited result","points":[],"sources":[]}),"partial");
        crate::glance::check_level(args["source"].as_str().unwrap()).unwrap();
        let mut store = crate::glance::GlanceStore::default();
        store.publish(&crate::glance::Caller::granted(APP),&args,0).unwrap();
        assert_eq!(args["data"]["story"]["url1"],format!("app://news/story/{}",item.id));
        assert_eq!(args["data"]["story"]["status"],"partial");
        assert_eq!(args["notify"],false);
        assert!(!args["source"].as_str().unwrap().contains("sys.chat"),"native Card/Chat supplies publication-bound context");
        assert!(valid_story_id(&item.id));
        assert!(focus_story(&item.id).is_ok());
        assert!(focus_story("a".repeat(64).as_str()).is_err());
        assert!(focus_story("../forged").is_err());
    }
}
