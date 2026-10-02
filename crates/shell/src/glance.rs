//! The `glance` host service (ADR 0002 §7–8, issue #63): apps publish
//! cards to the glance screen; the shell stores them; the glance page (phone)
//! and the glance panel (desktop) show them.
//!
//! | method | args | answer |
//! |---|---|---|
//! | `glance.publish` | `{card_id, source \| script, data?, title, priority?, expires?, open?: {app, route?}, notify?}` | `{card_id, replaced, expires_at}` |
//! | `glance.withdraw` | `{card_id}` | `{withdrawn}` |
//! | `glance.list` | – | `[{card_id, title, priority, published_at, expires_at}]`, the caller's own cards |
//!
//! **Identity.** The publishing app is the CALLER: the Card runner's app id
//! for a contained app (`ServiceCall::app_id`), the module id the shell
//! hosts for a native one ([`request`]). It is never read from the
//! arguments: an `app` argument that names anyone else is refused, and
//! `open.app` must be the caller's own app (a card opens the app that
//! published it). Cards are keyed by `(app, card_id)`: publishing the same
//! id again replaces the card, and one app can neither see, replace nor
//! withdraw another's.
//!
//! **Admission.** A card is one of two kinds, the two kinds of bundle the
//! Card runner runs:
//!
//! - `source`: an L0 card (`octoscript_ui_l0::check_ui_l0`: valid at L0, or
//!   at L1 when the header declares it), realized against `data` (a map from
//!   the card's source names to their values; L0's no-facts rule: the card
//!   states nothing it did not get from `data`) and lowered through the Card
//!   runner's pipeline (glance_card.rs) before it is stored: presentation,
//!   no logic, as a card bundle is. A `sys.chat` source (an in-card chat
//!   with the app's agent) must name the publishing app, and the host
//!   answers it from its own transcript, whatever `data` says
//!   (glance_chat.rs). An L2 `source` is refused: it cannot be
//!   lowered, and a card that needs handlers and host requests is a
//!   `script`.
//! - `script`: a Splash program, the same thing a script app's `main.splash`
//!   is: its own state, handlers, `host.request` calls and storage. It runs
//!   as it is, with no `data` (it carries its own values). This is the
//!   interactive card: an editable draft that sends, a reply box, a form.
//!
//! Every tile is interactive and runs under the publishing app's own policy
//! (glance_card.rs), so a card does on the glance screen exactly what the
//! app's UI does. Caps: `card_id` 1–64 of `[A-Za-z0-9._-]`, `title` ≤ 80
//! characters, `source`/`script` ≤ 16 KiB, `data` ≤ 32 KiB as JSON, `route`
//! ≤ 256. `priority` 0–100 (default 50). `expires` is seconds from now, 60 s
//! to 7 days (default 24 h); an expired card is dropped. Each app may publish
//! [`RATE_LIMIT`] times per [`RATE_WINDOW_MS`] (a replace counts, and so
//! does a card the check refuses) and keep [`PER_APP_CARDS`] cards; the
//! store keeps at most [`STORE_CARDS`], dropping the least important.
//!
//! **Notifications.** `notify: true` also posts a notification for the card
//! (the phone's shade, the desktop's toast). Tapping it opens the glance
//! page on the phone; on a desktop, clicking the toast opens THAT card in the
//! card window (glance_sheet.rs), App Clip style, by the key the
//! notification carries ([`GlanceNote::key`], [`NoteTargets`], [`card`]).
//! The shell drains them with [`take_notifications`].
//!
//! **Who may call.** A contained app publishes only when it holds the
//! `glance` capability (App Hub's `KNOWN_CAPABILITIES`; the store tells the
//! person "Show cards on your glance screen"). The Card runner's gate
//! (Makepad's `splash_policy::service_allowed`) lets a `glance.*` request
//! out of an app's isolate only when the app's resolved policy grants
//! `glance`, so a call the runner hands [`GlanceService`] from the app
//! itself holds the grant. A host sheet runs under no app's policy, so a
//! call from a sheet holds none, and [`Caller::Contained`] records which it
//! is. Every method refuses a contained caller without the grant. System
//! apps are no exception: they run under their own manifest's policy like
//! any installed app, so a system app that publishes requests `glance` in
//! its manifest, as Mail requests `mail`. Native modules are the shell's own
//! code and publish by the id the shell hosts them as. The capability only
//! decides who may publish; the limits above hold for every caller.
//!
//! **The feed.** The system agent will rank and trim; until then the shell
//! shows cards by priority, then recency, at most [`SHOWN_CARDS`]
//! ([`shown`]).
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const CARD_ID_MAX: usize = 64;
pub const TITLE_MAX: usize = 80;
pub const SOURCE_MAX: usize = 16 * 1024;
pub const DATA_MAX: usize = 32 * 1024;
pub const ROUTE_MAX: usize = 256;
pub const PRIORITY_DEFAULT: i64 = 50;
pub const EXPIRES_DEFAULT_S: u64 = 24 * 3600;
pub const EXPIRES_MIN_S: u64 = 60;
pub const EXPIRES_MAX_S: u64 = 7 * 24 * 3600;
/// Publishes per app per window.
pub const RATE_LIMIT: usize = 6;
pub const RATE_WINDOW_MS: u64 = 60_000;
pub const PER_APP_CARDS: usize = 4;
pub const STORE_CARDS: usize = 32;
/// Cards the glance screen shows at once.
pub const SHOWN_CARDS: usize = 6;

/// Who is calling: the host decides, never the arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    /// A contained app, by the manifest id the Card runner runs it under,
    /// and whether its policy grants `glance` (see the module docs).
    Contained { app: String, granted: bool },
    /// A native module or the shell itself, by the id the shell hosts it as.
    Native(String),
}

impl Caller {
    pub fn app(&self) -> &str {
        match self {
            Caller::Contained { app: id, .. } | Caller::Native(id) => id,
        }
    }
    /// A contained app whose policy grants `glance`.
    pub fn granted(app: impl Into<String>) -> Caller {
        Caller::Contained { app: app.into(), granted: true }
    }
    /// Whether this caller may use the glance service at all.
    pub fn may_use(&self) -> Result<(), String> {
        match self {
            Caller::Contained { app, granted: false } => Err(format!("{app} was not granted the glance capability")),
            _ => Ok(()),
        }
    }
    /// The launcher id that opens this app: a system app's short id
    /// (`news` for `os.news`), anyone else's own.
    pub fn launch_id(&self) -> &str {
        let app = self.app();
        app.strip_prefix("os.").unwrap_or(app)
    }
}

/// One published card.
#[derive(Clone, Debug, PartialEq)]
pub struct GlanceCard {
    pub app: String,
    pub card_id: String,
    pub title: String,
    pub priority: i64,
    pub published_ms: u64,
    pub expires_ms: u64,
    /// The launcher id the tile opens, and the route inside it.
    pub open_app: String,
    pub route: Option<String>,
    /// The Splash body the tile runs (glance_card.rs): a lowered `source`
    /// card, or a `script` as it was published.
    pub body: Arc<str>,
    /// The publisher is a contained app: its tile runs under that app's
    /// resolved policy. A native module's tile runs with no grants.
    pub contained: bool,
    /// For a `source` card, the L0 card and its data as published: the card
    /// window (glance_sheet.rs) carries out its taps against them and
    /// re-lowers. `None` for a `script` card.
    pub l0: Option<Arc<L0Source>>,
}

/// A published L0 card as it was admitted: its source and its data.
#[derive(Clone, Debug, PartialEq)]
pub struct L0Source {
    pub source: String,
    pub data: Value,
}

impl GlanceCard {
    pub fn key(&self) -> String {
        format!("{}/{}", self.app, self.card_id)
    }
}

/// The published cards and the per-app publish history.
#[derive(Default)]
pub struct GlanceStore {
    cards: Vec<GlanceCard>,
    publishes: Vec<(String, VecDeque<u64>)>,
}

fn text<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn valid_card_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= CARD_ID_MAX && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The L0 admission: the checker's verdict at L0 (or a declared L1), never L2.
pub fn check_level(source: &str) -> Result<(), String> {
    let report = octoscript_ui_l0::check_ui_l0(source);
    if report.level == octoscript_ui_l0::Level::L2 || !report.valid {
        let why: Vec<String> = report.diagnostics.iter().take(3).map(|d| format!("{}:{}: {}", d.line, d.column, d.message)).collect();
        let level = format!("{:?}", report.level);
        return Err(format!("the card is not admissible at L0 (derived level {level}): {}", if why.is_empty() { "refused".into() } else { why.join("; ") }));
    }
    Ok(())
}

impl GlanceStore {
    /// `glance.publish`, for `caller`, at `now_ms`.
    pub fn publish(&mut self, caller: &Caller, args: &Value, now_ms: u64) -> Result<Value, String> {
        self.expire(now_ms);
        caller.may_use()?;
        let app = caller.app().to_string();
        if let Some(claimed) = args.get("app") {
            if claimed.as_str() != Some(app.as_str()) && claimed.as_str() != Some(caller.launch_id()) {
                return Err("the publishing app is the caller; `app` cannot name another".into());
            }
        }
        let card_id = text(args, "card_id").ok_or("card_id is required")?;
        if !valid_card_id(card_id) {
            return Err(format!("card_id must be 1-{CARD_ID_MAX} of [A-Za-z0-9._-]"));
        }
        let title = text(args, "title").ok_or("title is required")?.trim();
        if title.is_empty() || title.chars().count() > TITLE_MAX {
            return Err(format!("title must be 1-{TITLE_MAX} characters"));
        }
        let (kind, source) = match (text(args, "source"), text(args, "script")) {
            (Some(source), None) => ("source", source),
            (None, Some(script)) => ("script", script),
            _ => return Err("give either source (an L0 card) or script (a Splash program)".into()),
        };
        if source.len() > SOURCE_MAX {
            return Err(format!("{kind} is {} bytes, over the {SOURCE_MAX}-byte cap", source.len()));
        }
        if kind == "script" && args.get("data").is_some_and(|d| !d.is_null()) {
            return Err("data is for a source card; a script carries its own values".into());
        }
        let data = args.get("data").cloned().unwrap_or_else(|| json!({}));
        if !data.is_object() {
            return Err("data must be an object of source values".into());
        }
        let data_len = data.to_string().len();
        if data_len > DATA_MAX {
            return Err(format!("data is {data_len} bytes, over the {DATA_MAX}-byte cap"));
        }
        let priority = match args.get("priority") {
            None | Some(Value::Null) => PRIORITY_DEFAULT,
            Some(p) => p.as_i64().filter(|p| (0..=100).contains(p)).ok_or("priority must be an integer 0-100")?,
        };
        let expires_s = match args.get("expires") {
            None | Some(Value::Null) => EXPIRES_DEFAULT_S,
            Some(e) => e.as_u64().filter(|e| (EXPIRES_MIN_S..=EXPIRES_MAX_S).contains(e)).ok_or_else(|| format!("expires must be {EXPIRES_MIN_S}-{EXPIRES_MAX_S} seconds from now"))?,
        };
        if args.get("notify").is_some_and(|n| !n.is_null() && !n.is_boolean()) {
            return Err("notify must be true or false".into());
        }
        let open = args.get("open").cloned().unwrap_or(Value::Null);
        if let Some(target) = open.get("app") {
            if target.as_str() != Some(app.as_str()) && target.as_str() != Some(caller.launch_id()) {
                return Err("a card opens the app that published it".into());
            }
        }
        let route = match open.get("route") {
            None | Some(Value::Null) => None,
            Some(r) => Some(r.as_str().filter(|r| r.len() <= ROUTE_MAX).ok_or_else(|| format!("open.route must be a string of at most {ROUTE_MAX} bytes"))?.to_string()),
        };
        let replacing = self.cards.iter().position(|c| c.app == app && c.card_id == card_id);
        if replacing.is_none() && self.cards.iter().filter(|c| c.app == app).count() >= PER_APP_CARDS {
            return Err(format!("{app} already has {PER_APP_CARDS} cards on the glance screen; withdraw or replace one"));
        }
        // Charged before the costly part (check, realize, lower), so a
        // stream of refused cards is bounded too.
        self.charge(&app, now_ms)?;
        let body: Arc<str> = if kind == "script" {
            source.into()
        } else {
            check_level(source)?;
            // A card's `sys.chat` is its own app's (glance_chat.rs), and its
            // transcript is the host's, never the data's.
            crate::glance_chat::check_publisher(source, &app)?;
            let seeded = crate::glance_chat::seed(&app, source, &data, &Default::default());
            crate::glance_card::lower(source, &seeded)?.into()
        };
        let card = GlanceCard {
            app: app.clone(),
            card_id: card_id.to_string(),
            title: title.to_string(),
            priority,
            published_ms: now_ms,
            expires_ms: now_ms + expires_s * 1000,
            open_app: caller.launch_id().to_string(),
            route,
            body,
            contained: matches!(caller, Caller::Contained { .. }),
            l0: (kind == "source").then(|| Arc::new(L0Source { source: source.to_string(), data })),
        };
        let expires_at = card.expires_ms;
        if let Some(i) = replacing {
            self.cards.remove(i);
        }
        self.cards.push(card);
        if self.cards.len() > STORE_CARDS {
            // The store is full: the least important, oldest card goes.
            if let Some(i) = (0..self.cards.len()).min_by_key(|&i| (self.cards[i].priority, self.cards[i].published_ms)) {
                self.cards.remove(i);
            }
        }
        Ok(json!({"card_id": card_id, "replaced": replacing.is_some(), "expires_at": expires_at}))
    }

    /// Count one publish against `app`'s window, or refuse it.
    fn charge(&mut self, app: &str, now_ms: u64) -> Result<(), String> {
        let at = match self.publishes.iter().position(|(a, _)| a == app) {
            Some(i) => i,
            None => {
                self.publishes.push((app.to_string(), VecDeque::new()));
                self.publishes.len() - 1
            }
        };
        let window = &mut self.publishes[at].1;
        while window.front().is_some_and(|&t| now_ms.saturating_sub(t) >= RATE_WINDOW_MS) {
            window.pop_front();
        }
        if window.len() >= RATE_LIMIT {
            return Err(format!("rate limited: at most {RATE_LIMIT} publishes per {} s", RATE_WINDOW_MS / 1000));
        }
        window.push_back(now_ms);
        Ok(())
    }

    /// `glance.withdraw`: the caller's own card only.
    pub fn withdraw(&mut self, caller: &Caller, args: &Value, now_ms: u64) -> Result<Value, String> {
        caller.may_use()?;
        self.expire(now_ms);
        let card_id = text(args, "card_id").ok_or("card_id is required")?;
        let before = self.cards.len();
        self.cards.retain(|c| !(c.app == caller.app() && c.card_id == card_id));
        Ok(json!({"withdrawn": self.cards.len() != before}))
    }

    /// The person dismissed a card (its close button on the glance
    /// screen): it goes, as if its app had withdrawn it. True when it was
    /// there.
    pub fn dismiss(&mut self, key: &str) -> bool {
        let before = self.cards.len();
        self.cards.retain(|c| c.key() != key);
        self.cards.len() != before
    }

    /// `glance.list`: the caller's own cards.
    pub fn list(&mut self, caller: &Caller, now_ms: u64) -> Result<Value, String> {
        caller.may_use()?;
        self.expire(now_ms);
        Ok(Value::Array(
            self.cards
                .iter()
                .filter(|c| c.app == caller.app())
                .map(|c| json!({"card_id": c.card_id, "title": c.title, "priority": c.priority, "published_at": c.published_ms, "expires_at": c.expires_ms}))
                .collect(),
        ))
    }

    /// Drop expired cards; true when any went.
    pub fn expire(&mut self, now_ms: u64) -> bool {
        let before = self.cards.len();
        self.cards.retain(|c| c.expires_ms > now_ms);
        self.cards.len() != before
    }

    /// What the glance screen shows: by priority, then recency, capped.
    pub fn shown(&self, now_ms: u64, max: usize) -> Vec<GlanceCard> {
        let mut cards: Vec<GlanceCard> = self.cards.iter().filter(|c| c.expires_ms > now_ms).cloned().collect();
        order(&mut cards);
        cards.truncate(max);
        cards
    }

    /// The live card with this key (`app/card_id`), if it is still published.
    pub fn card(&self, key: &str, now_ms: u64) -> Option<GlanceCard> {
        self.cards.iter().find(|c| c.expires_ms > now_ms && c.key() == key).cloned()
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

/// The glance order until the system agent ranks the feed: higher priority
/// first, then the most recently published.
pub fn order(cards: &mut [GlanceCard]) {
    cards.sort_by(|a, b| b.priority.cmp(&a.priority).then(b.published_ms.cmp(&a.published_ms)));
}

// ------------------------------------------------------------- the service

static STORE: Mutex<Option<GlanceStore>> = Mutex::new(None);
/// Cards published with `notify: true`, waiting for the shell to post them.
static NOTES: Mutex<Vec<GlanceNote>> = Mutex::new(Vec::new());

/// A notification a published card asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct GlanceNote {
    /// The card's key (`app/card_id`).
    pub key: String,
    pub app: String,
    pub title: String,
}

/// The notifications cards asked for since the last call.
pub fn take_notifications() -> Vec<GlanceNote> {
    std::mem::take(&mut *NOTES.lock().unwrap())
}

/// Which card each posted notification opens: the shell records the toast
/// (or shade note) id it posted for a [`GlanceNote`], and a click on it
/// takes the card key back out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteTargets(Vec<(u64, String)>);

impl NoteTargets {
    pub fn record(&mut self, id: u64, key: &str) {
        self.0.push((id, key.to_string()));
    }
    pub fn contains(&self, id: u64) -> bool {
        self.0.iter().any(|(i, _)| *i == id)
    }
    /// The notification was clicked: the key of the card it opens, once.
    pub fn activated(&mut self, id: u64) -> Option<String> {
        let at = self.0.iter().position(|(i, _)| *i == id)?;
        Some(self.0.remove(at).1)
    }
    /// The notification went away unclicked.
    pub fn dismissed(&mut self, id: u64) {
        self.0.retain(|(i, _)| *i != id);
    }
}
/// Bumped whenever the published set changes, so a surface re-reads it only then.
static GENERATION: AtomicU64 = AtomicU64::new(1);

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn with_store<R>(f: impl FnOnce(&mut GlanceStore) -> R) -> R {
    let mut guard = STORE.lock().unwrap();
    f(guard.get_or_insert_with(GlanceStore::default))
}

fn changed() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
    makepad_widgets::makepad_platform::SignalToUI::set_ui_signal();
}

/// The published set's generation: changes whenever a card is published,
/// replaced, withdrawn or expires (expiry is noticed by [`shown`]).
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// Serve one `glance.*` call for `caller`: the same API for a contained app
/// (through the Card runner's host service) and a native one.
pub fn request(caller: &Caller, service: &str, args: &Value) -> Result<Value, String> {
    let now = now_ms();
    let method = service.strip_prefix("glance.").unwrap_or(service);
    let result = with_store(|store| match method {
        "publish" => store.publish(caller, args, now),
        "withdraw" => store.withdraw(caller, args, now),
        "list" => store.list(caller, now),
        other => Err(format!("glance has no method {other:?}")),
    });
    if result.is_ok() && method == "publish" && args.get("notify").and_then(Value::as_bool) == Some(true) {
        let card_id = args.get("card_id").and_then(Value::as_str).unwrap_or_default();
        let title = args.get("title").and_then(Value::as_str).unwrap_or_default().trim();
        NOTES.lock().unwrap().push(GlanceNote { key: format!("{}/{card_id}", caller.app()), app: caller.app().to_string(), title: title.to_string() });
    }
    if result.is_ok() && method != "list" {
        changed();
    }
    match &result {
        Ok(_) if method == "publish" => makepad_widgets::log!("glance: {} published {}", caller.app(), args.get("card_id").and_then(Value::as_str).unwrap_or("?")),
        Err(e) => makepad_widgets::log!("glance: {} {} refused: {e}", caller.app(), service),
        _ => {}
    }
    result
}

/// Drop expired cards, bumping the generation when any went. Cheap: a
/// surface calls it every frame it draws the glance screen.
pub fn expire_now() {
    if with_store(|store| store.expire(now_ms())) {
        changed();
    }
}

/// The published card with this key (`app/card_id`), while it is live.
pub fn card(key: &str) -> Option<GlanceCard> {
    with_store(|store| store.card(key, now_ms()))
}

/// The person dismissed the card with this key (`app/card_id`).
pub fn dismiss(key: &str) -> bool {
    let gone = with_store(|store| store.dismiss(key));
    if gone {
        changed();
    }
    gone
}

/// What the glance screen shows now (priority, then recency, capped).
pub fn shown() -> Vec<GlanceCard> {
    expire_now();
    with_store(|store| store.shown(now_ms(), SHOWN_CARDS))
}

/// The `glance` family for the Card runner (App Hub's host services).
#[cfg(any(feature = "app-hub", native_mobile))]
pub struct GlanceService;

#[cfg(any(feature = "app-hub", native_mobile))]
impl octosense_appstore::services::HostService for GlanceService {
    fn family(&self) -> &'static str {
        "glance"
    }
    fn call(&mut self, call: octosense_appstore::services::ServiceCall, reply: octosense_appstore::services::Replier, _host: &mut dyn octosense_appstore::services::ServiceHost) {
        // The identity is the runner's, never the app's arguments. A call from
        // the app's own isolate passed the runner's gate, which requires the
        // `glance` capability; a host sheet's isolate has no app policy and
        // holds no grant.
        let caller = Caller::Contained { app: call.app_id.clone(), granted: !call.from_sheet };
        reply.send(request(&caller, &call.service, &call.args));
    }
}

#[cfg(any(feature = "app-hub", native_mobile))]
pub fn register() {
    octosense_appstore::services::register_host_service(Box::new(GlanceService));
}

/// Publish a card for contained app `app` from one of its host services:
/// a tool call runs outside the app's isolate, where the Card runner's gate
/// does not, so the grant is the app's admitted manifest's `glance`
/// (`script_apps::grants`). Calendar's cards and every app's notice
/// (glance_notice.rs) go out this way.
#[cfg(any(feature = "app-hub", native_mobile))]
pub fn publish_for(app: &str, args: &Value) -> Result<Value, String> {
    let caller = Caller::Contained { app: app.to_string(), granted: crate::host_tools::script_apps::grants(app, "glance") };
    request(&caller, "glance.publish", args)
}

// ---------------------------------------------------------------- the demo

/// The sample News digest card (L0) and fake data, for trying the glance
/// screen without the News agent (M3).
pub fn demo_digest() -> (String, Value) {
    const CARD: &str = include_str!("../resources/glance/news-digest.card");
    let data = json!({
        "status": {"message": "3 stories since this morning", "count": 3},
        "stories": [
            {"id": "s1", "title": "Open-source phone OS ships event-driven app agents", "publisher": "Techmeme"},
            {"id": "s2", "title": "Rust 1.95 stabilises async closures in traits", "publisher": "HN"},
            {"id": "s3", "title": "Makepad adds contained script isolates", "publisher": "Google News"}
        ]
    });
    (CARD.to_string(), data)
}

/// The fake Mail cards of the email action card plan (MVP: no mail is read,
/// no model is called): `(card_id, title, L0 source, data)` for a personal
/// request with a reply draft and an Ask chat, and a shipping update. The
/// request card's summary, suggestion and draft are its own `model-copy`
/// (what Mail's agent will write); its chat is the host's `sys.chat`
/// thread [`DEMO_MAIL_THREAD`], started by [`seed_demo_mail_chat`].
pub fn demo_mail() -> Vec<(&'static str, &'static str, String, Value)> {
    const REQUEST: &str = include_str!("../resources/glance/mail-request.card");
    const SHIPPING: &str = include_str!("../resources/glance/mail-shipping.card");
    let request = json!({
        "msg": {
            "title": "Ana Lee · Contract question",
            "subtitle": "To: Ana Lee · Re: Contract question",
            "as_of": "10:42"
        }
    });
    let shipping = json!({
        "pkg": {
            "title": "UPS · Your desk lamp has shipped",
            "subtitle": "Tracking 1Z 999 AA1 01 2345 6784",
            "as_of": "09:15",
            "summary": "The desk lamp from Lumen & Co. is out for delivery today; no signature needed.",
            "metric1_label": "Status", "metric1_value": "Out for delivery",
            "metric2_label": "ETA", "metric2_value": "Today by 8 pm",
            "url1": "ups.com/track · 1Z999AA10123456784"
        }
    });
    vec![
        ("ana-contract", "Mail · Ana Lee: Contract question", REQUEST.to_string(), request),
        ("ups-lamp", "Mail · UPS: Your desk lamp has shipped", SHIPPING.to_string(), shipping),
    ]
}

/// The Ask chat's thread on the fake request card.
pub const DEMO_MAIL_THREAD: &str = "ana-contract";

/// Start the fake request card's chat with one earlier question and answer
/// (the host's own entries), unless it already has a conversation.
pub fn seed_demo_mail_chat(app: &str) {
    use crate::glance_chat::Role;
    crate::glance_chat::store().seed_if_empty(
        app,
        DEMO_MAIL_THREAD,
        &[(Role::User, "What did they say about payment?"), (Role::Model, "Net 30 instead of net 45, starting with the next invoice.")],
        now_ms(),
    );
}

/// The `glance.publish` arguments for the fake Mail cards, one toast each.
pub fn demo_mail_publishes() -> Vec<Value> {
    demo_mail()
        .into_iter()
        .enumerate()
        .map(|(i, (card_id, title, source, data))| {
            json!({
                "card_id": card_id, "title": title, "source": source, "data": data,
                "priority": 80 - i as i64, "open": {"app": "mail"}, "notify": true
            })
        })
        .collect()
}

/// `OCTOSENSE_GLANCE_DEMO`, once, at startup (a test path; nothing publishes
/// these otherwise): `mail` publishes the fake Mail cards as `os.mail`, each
/// with a notification; any other value but `0` publishes the sample News
/// digest as `os.news`.
pub fn publish_demo_if_asked() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let demo = std::env::var("OCTOSENSE_GLANCE_DEMO").unwrap_or_default();
        if demo.is_empty() || demo == "0" {
            return;
        }
        if demo == "mail" {
            crate::glance_chat::set_demo_mail(true);
            seed_demo_mail_chat("os.mail");
            for args in demo_mail_publishes() {
                if let Err(e) = request(&Caller::granted("os.mail"), "glance.publish", &args) {
                    makepad_widgets::log!("glance: demo mail card refused: {e}");
                }
            }
            return;
        }
        let (source, data) = demo_digest();
        let args = json!({
            "card_id": "digest", "title": "News digest", "source": source, "data": data,
            "priority": 70, "open": {"app": "news"}
        });
        if let Err(e) = request(&Caller::granted("os.news"), "glance.publish", &args) {
            makepad_widgets::log!("glance: demo digest refused: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn news() -> Caller {
        Caller::granted("os.news")
    }
    fn args(card_id: &str) -> Value {
        let (source, data) = demo_digest();
        json!({"card_id": card_id, "title": "News digest", "source": source, "data": data, "open": {"app": "news"}})
    }

    #[test]
    fn the_caller_is_the_publisher_never_the_arguments() {
        let mut store = GlanceStore::default();
        let mut a = args("digest");
        a["app"] = json!("os.mail");
        assert!(store.publish(&news(), &a, 1_000).unwrap_err().contains("caller"));
        let mut a = args("digest");
        a["open"] = json!({"app": "mail"});
        assert!(store.publish(&news(), &a, 1_000).unwrap_err().contains("opens the app that published it"));
        let ok = store.publish(&news(), &args("digest"), 1_000).unwrap();
        assert_eq!(ok["replaced"], false);
        let shown = store.shown(1_000, SHOWN_CARDS);
        assert_eq!((shown[0].app.as_str(), shown[0].open_app.as_str()), ("os.news", "news"));
        // Another app sees, replaces and withdraws only its own cards.
        let maps = Caller::granted("os.maps");
        assert_eq!(store.list(&maps, 1_000).unwrap(), json!([]));
        assert_eq!(store.withdraw(&maps, &json!({"card_id": "digest"}), 1_000).unwrap()["withdrawn"], false);
        assert_eq!(store.len(), 1);
    }

    /// Calendar's event and agenda cards are valid L0 cards the store
    /// admits as Calendar's.
    #[cfg(any(feature = "app-hub", native_mobile))]
    #[test]
    fn calendars_cards_are_admitted_as_its_own() {
        use octosense_calendar_service as cal;
        let mut store = GlanceStore::default();
        let event = cal::event_card_args("Dentist", "Fri 2 Oct", "15:00\u{2013}16:00", "Main St", "Bring the form", "ev-1", 70);
        assert!(store.publish(&Caller::granted("os.calendar"), &event, 1_000).is_ok());
        let now = cal::parse_time("2026-10-01T09:00").unwrap();
        let events = vec![cal::Event { id: "a".into(), title: "Standup".into(), start: "2026-10-02T09:30".into(), end: None, location: String::new(), notes: String::new() }];
        let agenda = cal::agenda_card_args(&events, 7, now);
        assert!(store.publish(&Caller::granted("os.calendar"), &agenda, 1_000).is_ok());
        let shown = store.shown(1_000, SHOWN_CARDS);
        assert!(shown.iter().all(|c| c.app == "os.calendar" && c.open_app == "calendar"));
    }

    /// Mail's `mail.notify` card is a valid L0 card the store admits as
    /// Mail's, and a Mail without the `glance` grant cannot publish it.
    #[cfg(any(feature = "app-hub", native_mobile))]
    #[test]
    fn mails_notice_card_is_admitted_as_mails_own() {
        let mut store = GlanceStore::default();
        let args = crate::glance_notice::publish_args("os.mail", &json!({"title": "Hello", "body": "From the system agent", "card_id": "hello"}), 1).unwrap();
        let ok = store.publish(&Caller::granted("os.mail"), &args, 1_000).unwrap();
        assert_eq!(ok["card_id"], "hello");
        let shown = store.shown(1_000, SHOWN_CARDS);
        assert_eq!((shown[0].app.as_str(), shown[0].open_app.as_str()), ("os.mail", "mail"));
        let ungranted = Caller::Contained { app: "os.mail".into(), granted: false };
        assert!(store.publish(&ungranted, &args, 1_000).is_err());
    }

    /// Every system app's notice (`<namespace>.notify`) is a valid L0 card,
    /// with that app's icon, that the store admits as that app's own and
    /// that opens it; another app cannot publish it.
    #[cfg(any(feature = "app-hub", native_mobile))]
    #[test]
    fn every_apps_notice_card_is_admitted_as_its_own() {
        for app in ["os.news", "os.photos", "os.maps", "os.youtube", "os.camera", "os.calendar", "os.ai-providers"] {
            let mut store = GlanceStore::default();
            let args = crate::glance_notice::publish_args(app, &json!({"title": "Hello", "body": "From the system agent", "card_id": "hello"}), 1).unwrap();
            let ok = store.publish(&Caller::granted(app), &args, 1_000);
            assert!(ok.is_ok(), "{app}: {ok:?}");
            let shown = store.shown(1_000, SHOWN_CARDS);
            assert_eq!((shown[0].app.as_str(), shown[0].open_app.as_str()), (app, app.strip_prefix("os.").unwrap()));
            assert!(store.publish(&Caller::granted("os.mail"), &args, 1_000).unwrap_err().contains("opens the app that published it"), "{app}");
        }
    }

    #[test]
    fn a_contained_app_needs_the_glance_capability_whoever_it_is() {
        let mut store = GlanceStore::default();
        // A store app with the grant publishes like a system app, under the
        // same limits, and its card opens only itself.
        let mut a = args("d");
        a["open"] = json!({"app": "com.example.news"});
        assert!(store.publish(&Caller::granted("com.example.news"), &a, 0).is_ok());
        // Without the grant, no app may publish, list or withdraw; being a
        // system app is not a grant.
        for app in ["com.example.other", "os.maps"] {
            let ungranted = Caller::Contained { app: app.into(), granted: false };
            let err = store.publish(&ungranted, &args("d"), 0).unwrap_err();
            assert!(err.contains("not granted the glance capability"), "{err}");
            assert!(store.list(&ungranted, 0).is_err());
            assert!(store.withdraw(&ungranted, &json!({"card_id": "d"}), 0).is_err());
        }
        // A native module publishes through the same API.
        assert!(store.publish(&Caller::Native("news".into()), &args("d"), 0).is_ok());
        assert_eq!(store.len(), 2);
    }

    /// The grant is the Card runner's: its isolate gate lets `glance.*` out
    /// only for an app whose resolved policy lists `glance` (a prefix or a
    /// neighbouring family is not enough).
    #[test]
    fn the_runner_gate_admits_glance_only_with_the_capability() {
        use makepad_widgets::splash_policy::{service_allowed, set_policy_for_heap};
        set_policy_for_heap(9201, vec!["storage".into(), "news".into()], Vec::new(), None);
        assert!(service_allowed(9201, "glance.publish").is_err());
        set_policy_for_heap(9202, vec!["glance".into()], Vec::new(), None);
        for method in ["glance.publish", "glance.withdraw", "glance.list"] {
            assert!(service_allowed(9202, method).is_ok(), "{method}");
        }
        assert!(service_allowed(9202, "news.list").is_err(), "glance grants nothing else");
    }

    #[test]
    fn l2_and_invalid_cards_are_refused() {
        let mut store = GlanceStore::default();
        let mut a = args("digest");
        a["source"] = json!("# level: L2\nview root Col { TextBody(text: \"hi\") }\nui.label(\"x\").set_text(\"y\")\n");
        let err = store.publish(&news(), &a, 0).unwrap_err();
        assert!(err.contains("not admissible at L0"), "{err}");
        a["source"] = json!("let x = 1 + 2\n");
        assert!(store.publish(&news(), &a, 0).is_err());
        assert!(check_level(&demo_digest().0).is_ok());
        assert!(store.is_empty());
    }

    /// An interactive card is a Splash program, run as it was published
    /// (no L0 check, no lowering), under the same caps and limits.
    #[test]
    fn a_script_card_is_admitted_as_it_is() {
        let mut store = GlanceStore::default();
        let script = "draft := TextInput{text: \"Hi\" on_return: |t| host.request(\"mail.send\", {body: t}, nil)}";
        let a = json!({"card_id": "draft", "title": "Reply", "script": script});
        store.publish(&news(), &a, 0).unwrap();
        let card = &store.shown(0, 9)[0];
        assert_eq!((card.body.as_ref(), card.contained), (script, true));
        let mut both = a.clone();
        both["source"] = json!(demo_digest().0);
        assert!(store.publish(&news(), &both, 0).unwrap_err().contains("either source"));
        assert!(store.publish(&news(), &json!({"card_id": "x", "title": "t"}), 0).unwrap_err().contains("either source"));
        let mut with_data = a.clone();
        with_data["data"] = json!({"x": 1});
        assert!(store.publish(&news(), &with_data, 0).unwrap_err().contains("data is for a source card"));
        let mut big = a.clone();
        big["script"] = json!("x".repeat(SOURCE_MAX + 1));
        assert!(store.publish(&news(), &big, 0).unwrap_err().contains("script is"));
        // A native module's card runs with no grants.
        store.publish(&Caller::Native("news".into()), &json!({"card_id": "n", "title": "t", "script": "View{}"}), 0).unwrap();
        assert!(!store.shown(0, 9).iter().find(|c| c.card_id == "n").unwrap().contained);
    }

    /// `notify: true` queues a notification for the shell to post.
    #[test]
    fn notify_queues_a_notification() {
        let mut a = args("notify-test");
        a["notify"] = json!("yes");
        assert!(GlanceStore::default().publish(&news(), &a, 0).unwrap_err().contains("notify"));
        a["notify"] = json!(true);
        request(&Caller::granted("os.notifytest"), "glance.publish", &{
            a["open"] = Value::Null;
            a
        })
        .unwrap();
        let notes = take_notifications();
        let note = notes.iter().find(|n| n.app == "os.notifytest").expect("queued");
        assert_eq!((note.key.as_str(), note.title.as_str()), ("os.notifytest/notify-test", "News digest"));
        request(&Caller::granted("os.notifytest"), "glance.withdraw", &json!({"card_id": "notify-test"})).unwrap();
    }

    /// The fake Mail cards pass the L0 admission and publish as `os.mail`,
    /// each queuing a notification that carries its card's key; the
    /// notification's id maps back to exactly that card, whose L0 source and
    /// data travel with it for the card window.
    #[test]
    fn a_mail_toast_opens_its_own_card() {
        // The request card chats with `os.mail`'s agent: no other app may
        // publish it.
        let mut ana = demo_mail_publishes().remove(0);
        ana["open"] = Value::Null;
        let err = GlanceStore::default().publish(&Caller::granted("os.mailtest"), &ana, 0).unwrap_err();
        assert!(err.contains("own app's agent only (os.mailtest), not os.mail"), "{err}");
        let mail = Caller::granted("os.mail");
        let mut targets = NoteTargets::default();
        let mut keys = Vec::new();
        for (i, mut args) in demo_mail_publishes().into_iter().enumerate() {
            args["open"] = Value::Null;
            request(&mail, "glance.publish", &args).unwrap();
            let notes: Vec<GlanceNote> = take_notifications().into_iter().filter(|n| n.app == "os.mail").collect();
            assert_eq!(notes.len(), 1, "one toast per card");
            // The shell posts the toast and records its id against the key.
            let toast_id = 100 + i as u64;
            targets.record(toast_id, &notes[0].key);
            keys.push((toast_id, notes[0].key.clone(), args));
        }
        assert_eq!(keys[0].1, "os.mail/ana-contract");
        assert_eq!(keys[1].1, "os.mail/ups-lamp");
        // Clicking the second toast opens the shipping card, not the panel
        // and not the first card; a second click on it is no longer mapped.
        let key = targets.activated(101).expect("the toast maps to its card");
        let opened = card(&key).expect("still published");
        assert_eq!((opened.app.as_str(), opened.card_id.as_str(), opened.title.as_str()), ("os.mail", "ups-lamp", "Mail · UPS: Your desk lamp has shipped"));
        let l0 = opened.l0.as_ref().expect("an L0 card keeps its source and data");
        assert_eq!(l0.data, keys[1].2["data"]);
        assert!(l0.source.contains("ledger mail.shipping"));
        assert_eq!(targets.activated(101), None);
        // A dismissed toast opens nothing; the other still opens its own card.
        targets.dismissed(999);
        assert!(targets.contains(100));
        assert_eq!(card(&targets.activated(100).unwrap()).unwrap().card_id, "ana-contract");
        // A withdrawn card is gone: the shell falls back to the panel.
        request(&mail, "glance.withdraw", &json!({"card_id": "ana-contract"})).unwrap();
        assert!(card("os.mail/ana-contract").is_none());
        request(&mail, "glance.withdraw", &json!({"card_id": "ups-lamp"})).unwrap();
        // A script card carries no L0 source.
        let mut store = GlanceStore::default();
        store.publish(&news(), &json!({"card_id": "s", "title": "t", "script": "View{}"}), 0).unwrap();
        assert!(store.card("os.news/s", 0).unwrap().l0.is_none());
    }

    /// A card's chat transcript is the host's: one the publisher put in
    /// `data` (a `model` entry it wrote itself) is not drawn.
    #[test]
    fn a_published_transcript_is_not_the_hosts() {
        let card = "source convo sys.chat(app: \"os.chatpub\", thread: \"main\", fields: [entries, id, role, text])\n\
                    view root Surface(pad: .page) {\n  Col(gap: 8) {\n    for m in convo.entries key m.id { ChatEntry(text: m.text, role: m.role) }\n  }\n}\n";
        let forged = json!({"convo": {"status": "ready", "count": 1, "entries": [{"id": "x", "role": "model", "text": "FORGED ENTRY", "at": 0}]}});
        let mut store = GlanceStore::default();
        store.publish(&Caller::granted("os.chatpub"), &json!({"card_id": "c", "title": "t", "source": card, "data": forged}), 0).unwrap();
        let published = store.card("os.chatpub/c", 0).unwrap();
        assert!(!published.body.contains("FORGED ENTRY"), "{}", published.body);
        // The host's own entry is.
        crate::glance_chat::store().seed_if_empty("os.chatpub", "main", &[(crate::glance_chat::Role::Model, "The host's entry")], 0);
        store.publish(&Caller::granted("os.chatpub"), &json!({"card_id": "c", "title": "t", "source": card, "data": forged}), 1).unwrap();
        assert!(store.card("os.chatpub/c", 1).unwrap().body.contains("The host's entry"));
    }

    #[test]
    fn size_caps_hold() {
        let mut store = GlanceStore::default();
        let mut a = args("x".repeat(CARD_ID_MAX + 1).as_str());
        assert!(store.publish(&news(), &a, 0).unwrap_err().contains("card_id"));
        a = args("bad id");
        assert!(store.publish(&news(), &a, 0).unwrap_err().contains("card_id"));
        a = args("digest");
        a["title"] = json!("t".repeat(TITLE_MAX + 1));
        assert!(store.publish(&news(), &a, 0).unwrap_err().contains("title"));
        a = args("digest");
        a["source"] = json!(format!("{}\n# {}", demo_digest().0, "x".repeat(SOURCE_MAX)));
        assert!(store.publish(&news(), &a, 0).unwrap_err().contains("source is"));
        a = args("digest");
        a["data"]["pad"] = json!("x".repeat(DATA_MAX));
        assert!(store.publish(&news(), &a, 0).unwrap_err().contains("data is"));
        a = args("digest");
        a["priority"] = json!(101);
        assert!(store.publish(&news(), &a, 0).unwrap_err().contains("priority"));
        a = args("digest");
        a["expires"] = json!(5);
        assert!(store.publish(&news(), &a, 0).unwrap_err().contains("expires"));
        a = args("digest");
        a["open"]["route"] = json!("r".repeat(ROUTE_MAX + 1));
        assert!(store.publish(&news(), &a, 0).unwrap_err().contains("route"));
        assert!(store.is_empty());
    }

    #[test]
    fn publishing_is_rate_limited_per_app() {
        let mut store = GlanceStore::default();
        for i in 0..RATE_LIMIT {
            store.publish(&news(), &args("digest"), 1_000 + i as u64).unwrap();
        }
        assert!(store.publish(&news(), &args("digest"), 2_000).unwrap_err().contains("rate limited"));
        // Another app has its own window.
        let mut maps = args("digest");
        maps["open"] = json!({"app": "maps"});
        assert!(store.publish(&Caller::granted("os.maps"), &maps, 2_000).is_ok());
        // The window slides.
        assert!(store.publish(&news(), &args("digest"), 1_000 + RATE_WINDOW_MS).is_ok());
    }

    #[test]
    fn the_same_card_id_replaces_and_each_app_is_capped() {
        let mut store = GlanceStore::default();
        store.publish(&news(), &args("digest"), 0).unwrap();
        let mut a = args("digest");
        a["title"] = json!("Evening digest");
        assert_eq!(store.publish(&news(), &a, 10).unwrap()["replaced"], true);
        assert_eq!(store.len(), 1);
        assert_eq!(store.shown(10, 9)[0].title, "Evening digest");
        for i in 1..PER_APP_CARDS {
            store.publish(&news(), &args(&format!("c{i}")), 100_000 * i as u64).unwrap();
        }
        assert!(store.publish(&news(), &args("one-more"), 900_000).unwrap_err().contains("already has"));
        // Replacing is still allowed at the cap.
        assert!(store.publish(&news(), &args("digest"), 900_000).is_ok());
    }

    #[test]
    fn cards_expire_and_withdraw() {
        let mut store = GlanceStore::default();
        let mut a = args("digest");
        a["expires"] = json!(60);
        let ok = store.publish(&news(), &a, 1_000).unwrap();
        assert_eq!(ok["expires_at"], 61_000);
        assert_eq!(store.list(&news(), 60_999).unwrap().as_array().unwrap().len(), 1);
        assert!(store.shown(61_000, 9).is_empty());
        assert_eq!(store.list(&news(), 61_000).unwrap(), json!([]));
        store.publish(&news(), &args("digest"), 70_000).unwrap();
        assert_eq!(store.withdraw(&news(), &json!({"card_id": "digest"}), 70_001).unwrap()["withdrawn"], true);
        assert!(store.is_empty());
    }

    /// The person's close button takes the one card it is on; the app can
    /// publish it again.
    #[test]
    fn the_person_dismisses_one_card() {
        let mut store = GlanceStore::default();
        store.publish(&news(), &args("digest"), 1_000).unwrap();
        store.publish(&news(), &args("other"), 1_000).unwrap();
        let key = store.shown(1_000, 9).iter().find(|c| c.card_id == "digest").unwrap().key();
        assert!(store.dismiss(&key));
        assert!(!store.dismiss(&key), "already gone");
        assert_eq!(store.shown(1_000, 9).iter().map(|c| c.card_id.as_str()).collect::<Vec<_>>(), ["other"]);
        store.publish(&news(), &args("digest"), 2_000).unwrap();
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn the_feed_orders_by_priority_then_recency_and_caps() {
        let mut store = GlanceStore::default();
        let apps = ["os.a", "os.b", "os.c", "os.d"];
        let mut t = 0;
        for app in apps {
            for (i, p) in [10, 90].iter().enumerate() {
                t += 1;
                let mut a = args(&format!("c{i}"));
                a["priority"] = json!(p);
                a["open"] = Value::Null;
                store.publish(&Caller::granted(app), &a, t).unwrap();
            }
        }
        let shown = store.shown(t, SHOWN_CARDS);
        assert_eq!(shown.len(), SHOWN_CARDS);
        let keys: Vec<String> = shown.iter().map(GlanceCard::key).collect();
        assert_eq!(&keys[..4], ["os.d/c1", "os.c/c1", "os.b/c1", "os.a/c1"]);
        assert_eq!(&keys[4..], ["os.d/c0", "os.c/c0"]);
    }

    /// Through App Hub's host-service dispatch, as the Card runner calls it:
    /// the identity is the runner's `app_id`, whatever the arguments say.
    #[cfg(feature = "app-hub")]
    #[test]
    fn the_card_runner_dispatch_carries_the_callers_identity() {
        use octosense_appstore::services::{dispatch, take_replies_for, ServiceCall, ServiceHost};
        struct NoSheets;
        impl ServiceHost for NoSheets {
            fn open_sheet(&mut self, _: String) {}
            fn close_sheet(&mut self) {}
        }
        register();
        let call = |app: &str, service: &str, args: Value| ServiceCall { app_id: app.into(), service: service.into(), args, from_sheet: false, may_prompt: true, host_dir: std::env::temp_dir() };
        let from_sheet = |app: &str, service: &str, args: Value| ServiceCall { from_sheet: true, ..call(app, service, args) };
        let mut spoof = args("dispatch-test");
        spoof["app"] = json!("os.mail");
        dispatch(call("os.news", "glance.publish", spoof), 9101, 1, &mut NoSheets);
        let refused = take_replies_for(&[9101]);
        assert!(refused[0].2.as_ref().unwrap_err().contains("caller"), "{refused:?}");
        // A host sheet over an app runs under no app policy: no grant.
        dispatch(from_sheet("os.news", "glance.publish", args("dispatch-test")), 9102, 1, &mut NoSheets);
        assert!(take_replies_for(&[9102])[0].2.as_ref().unwrap_err().contains("not granted the glance capability"));
        dispatch(call("os.news", "glance.publish", args("dispatch-test")), 9103, 1, &mut NoSheets);
        assert!(take_replies_for(&[9103])[0].2.is_ok());
        assert!(shown().iter().any(|c| c.key() == "os.news/dispatch-test" && c.open_app == "news"));
        dispatch(call("os.news", "glance.withdraw", json!({"card_id": "dispatch-test"})), 9104, 1, &mut NoSheets);
        assert!(take_replies_for(&[9104])[0].2.as_ref().unwrap().contains("true"));
        assert!(!shown().iter().any(|c| c.key() == "os.news/dispatch-test"));
    }
}
