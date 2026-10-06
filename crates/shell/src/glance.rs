//! The `glance` host service (ADR 0002 §7–8, issue #63): apps publish
//! cards to the glance screen; the shell stores them; the glance page (phone)
//! and the glance panel (desktop) show them.
//!
//! | method | args | answer |
//! |---|---|---|
//! | `glance.publish` | `{card_id, source \| script, data?, title, summary?, priority?, expires?, open?: {app, route?}, notify?}` | `{card_id, replaced, expires_at}` |
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
//!   (glance_chat.rs). A `sys.digest(app:, id:)` source is not the
//!   publisher's to supply either: the shell resolves it from the digests
//!   it holds for the calling app, and a card naming another app is refused
//!   (glance_digest.rs). A card bound to a current digest expires no later
//!   than the digest does. An L2 `source` is refused: it cannot be
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
//! characters, `summary` ≤ 200 (the notification's second line; without
//! one it is the card's own `summary` or `note.summary` in its data, or
//! nothing), `source`/`script` ≤ 16 KiB, `data` ≤ 32 KiB as JSON, `route`
//! ≤ 256. `priority` 0–100 (default 50). `expires` is seconds from now, 60 s
//! to 7 days (default 24 h); an expired card is dropped. Each app may publish
//! [`RATE_LIMIT`] times per [`RATE_WINDOW_MS`] (a replace counts, and so
//! does a card the check refuses). Retained payloads use byte budgets, not
//! card-count limits: [`PER_APP_BYTES`] per publisher and [`STORE_BYTES`] total.
//! Under memory pressure, the least important older cards retire; a new valid
//! publication is retained. Drafts and source mail are owned by their services.
//!
//! **Notifications.** `notify: true` also posts a notification for the card
//! (the phone's shade, the desktop's toast). Tapping it opens the glance
//! workspace directly on the phone; on a desktop, clicking the toast opens THAT card in the
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
//! scrolls all retained cards by priority, then recency ([`shown`]); only
//! summaries in the viewport are painted.
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const CARD_ID_MAX: usize = 64;
pub const TITLE_MAX: usize = 80;
/// A notification's second line, in characters.
pub const SUMMARY_MAX: usize = 200;
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
/// Retained source/data/lowered-body budgets, independent of visible rows.
/// These bound payload storage, not the allocator overhead of every clone.
pub const PER_APP_BYTES: usize = 8 * 1024 * 1024;
pub const STORE_BYTES: usize = 32 * 1024 * 1024;

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
    /// Account at publication, supplied by the host, never by card arguments.
    pub account: Option<String>,
    pub card_id: String,
    pub title: String,
    /// Bounded publication summary. Phone previews never execute the card.
    pub summary: String,
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
    /// The digest run ids the card binds (`sys.digest`), kept from pruning
    /// while the card is live.
    pub digests: Vec<String>,
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
    /// Out-of-band Mail service metadata; never deserialized from card data.
    pub mail: Option<crate::mail_card::Binding>,
}

impl GlanceCard {
    pub(crate) fn account_valid(&self) -> bool {
        self.account.as_ref().is_none_or(|account|
            crate::ai_host::contained::account_of(&self.app).as_ref() == Some(account)
            && crate::app_storage::host().is_some_and(|s| !s.is_signed_out(&self.app,
                (account != crate::ai_host::contained::ACCOUNT).then_some(account.as_str()))))
    }
    pub fn key(&self) -> String {
        format!("{}/{}", self.app, self.card_id)
    }
    fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.app.len() + self.card_id.len()
            + self.account.as_ref().map_or(0, String::len)
            + self.title.len() + self.summary.len() + self.open_app.len()
            + self.route.as_ref().map_or(0, String::len) + self.body.len()
            + self.digests.iter().map(String::len).sum::<usize>()
            + self.l0.as_ref().map_or(0, |l0|
                l0.source.len() + l0.data.to_string().len()
                + l0.mail.as_ref().map_or(0, |binding| serde_json::to_vec(binding).map_or(0, |v| v.len())))
    }
}

/// The published cards and the per-app publish history.
#[derive(Default)]
pub struct GlanceStore {
    cards: Vec<GlanceCard>,
    publishes: Vec<(String, VecDeque<u64>)>,
    /// Where the host keeps each app's toolbox folder (`sys.digest`); no
    /// digest resolves without one.
    digest_root: Option<PathBuf>,
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

/// Model-authored Mail cards deliberately use the declaration-only L0 subset.
pub fn check_generated_l0(source: &str) -> Result<(), String> {
    check_level(source)?;
    if octoscript_ui_l0::check_ui_l0(source).level != octoscript_ui_l0::Level::L0 {
        return Err("Generated Mail cards must use L0 declarations; L1 expressions are not allowed".into());
    }
    Ok(())
}

/// A syntactically valid generated card can still render only placeholders if
/// its dataset values are flat instead of keyed by the declared source name.
/// Reject that publication so the model can repair its own tool arguments.
fn check_generated_data(source: &str, data: &Value) -> Result<(), String> {
    use octoscript_ui_l0::SourceArg;
    let plan = octoscript_ui_l0::source_plan(source);
    if !plan.diagnostics.is_empty() {
        return Err("Generated card source dependencies must resolve without cycles or errors".into());
    }
    for request in plan.requests {
        if request.helper != "sys.dataset" {
            continue;
        }
        let record = request.name.split('.').try_fold(data, |v, key| v.get(key))
            .and_then(Value::as_object)
            .ok_or_else(|| format!("Missing dataset object data.{}; nest extracted facts under the declared source name", request.name))?;
        let fields = match request.args.iter().find(|(name, _)| name == "fields") {
            Some((_, SourceArg::List(fields))) if !fields.is_empty() => fields,
            _ => return Err(format!("Dataset {} must declare a nonempty fields list for its extracted facts", request.name)),
        };
        for field in fields {
            if record.get(field).is_none_or(Value::is_null) {
                return Err(format!("Missing dataset field data.{}.{field}; supply the email fact or explicit unknown text", request.name));
            }
        }
    }
    Ok(())
}

/// The card's `sys.digest` sources, resolved for `caller` into `data`: the
/// value under the source's name and its lifecycle under `$status`, both
/// replacing anything the publisher sent. Returns the bound run ids and the
/// earliest expiry of a current digest.
fn resolve_digests(caller: &Caller, source: &str, data: &mut Value, root: Option<&std::path::Path>, now_ms: u64) -> Result<(Vec<String>, Option<u64>), String> {
    use octoscript_ui_l0::SourceArg;
    let plan = octoscript_ui_l0::source_plan(source);
    let mut ids = Vec::new();
    let mut expires: Option<u64> = None;
    for request in plan.requests.iter().filter(|r| r.helper == "sys.digest") {
        let arg = |name: &str| request.args.iter().find(|(n, _)| n == name).map(|(_, a)| a);
        match arg("app") {
            Some(SourceArg::Text(app)) if app == caller.app() || app == caller.launch_id() => {}
            _ => return Err(format!("a card binds only its own app's digests: `sys.digest(app:)` must be {:?}", caller.app())),
        }
        let id = match arg("id") {
            Some(SourceArg::Text(id)) => id.clone(),
            // An id kept in card state: its declared initial.
            Some(SourceArg::Path(path)) => match path.strip_prefix("state.") {
                Some(state) => octoscript_ui_l0::state_initials(source).get(state).and_then(Value::as_str).unwrap_or("").to_string(),
                None => path.split('.').try_fold(&*data, |v, k| v.get(k)).and_then(Value::as_str).unwrap_or("").to_string(),
            },
            _ => String::new(),
        };
        let resolved = crate::glance_digest::resolve(root, caller.app(), &id, now_ms);
        if let Some(at) = resolved.expires_ms {
            expires = Some(expires.map_or(at, |e| e.min(at)));
        }
        if crate::glance_digest::valid_id(&id) && !ids.contains(&id) {
            ids.push(id);
        }
        let Some(fields) = data.as_object_mut() else { return Err("data must be an object of source values".into()) };
        // A dotted source name navigates the data, as realization reads it.
        let mut segments: Vec<&str> = request.name.split('.').collect();
        let last = segments.pop().unwrap_or_default();
        let mut at = fields;
        for segment in segments {
            let slot = at.entry(segment.to_string()).or_insert_with(|| json!({}));
            if !slot.is_object() {
                *slot = json!({});
            }
            at = slot.as_object_mut().expect("an object");
        }
        at.insert(last.to_string(), resolved.value);
        let status = data.as_object_mut().expect("an object").entry("$status").or_insert_with(|| json!({}));
        if !status.is_object() {
            *status = json!({});
        }
        status[request.name.as_str()] = json!(resolved.state);
    }
    Ok((ids, expires))
}

impl GlanceStore {
    /// A store whose `sys.digest` sources resolve under `root`
    /// (`<root>/<app>/toolbox/runs/…`, glance_digest.rs).
    pub fn with_digest_root(mut self, root: Option<PathBuf>) -> Self {
        self.digest_root = root;
        self
    }

    /// `glance.publish`, for `caller`, at `now_ms`.
    pub fn publish(&mut self, caller: &Caller, args: &Value, now_ms: u64) -> Result<Value, String> {
        if args.get("mail_binding").is_some() {
            return Err("Mail bindings can only be attached by the Mail host service".into());
        }
        self.publish_bound(caller, args, now_ms, None)
    }

    fn publish_bound(&mut self, caller: &Caller, args: &Value, now_ms: u64, mail: Option<crate::mail_card::Binding>) -> Result<Value, String> {
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
        match args.get("summary") {
            None | Some(Value::Null) => {}
            Some(Value::String(s)) if s.chars().count() <= SUMMARY_MAX => {}
            Some(_) => return Err(format!("summary must be a string of at most {SUMMARY_MAX} characters")),
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
        let mut data = args.get("data").cloned().unwrap_or_else(|| json!({}));
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
        if let Some(previous) = replacing.and_then(|i| self.cards[i].l0.as_ref()).and_then(|l| l.mail.as_ref()) {
            let next = mail.as_ref().ok_or("A bound Mail card cannot lose its draft on republish")?;
            if previous.account != next.account || previous.draft_id != next.draft_id || previous.source_message != next.source_message {
                return Err("A Mail card cannot change its account, email or draft on republish".into());
            }
            crate::mail_card::cancel_for(&previous.key());
        }
        // Charged before the costly part (check, realize, lower), so a
        // stream of refused cards is bounded too.
        self.charge(&app, now_ms)?;
        let (body, digests, digest_expires): (Arc<str>, Vec<String>, Option<u64>) = if kind == "script" {
            (source.into(), Vec::new(), None)
        } else {
            check_level(source)?;
            crate::mail_card::check_sources(source, mail.as_ref())?;
            // A card's `sys.digest` values are the host's (glance_digest.rs),
            // resolved into the data the card keeps.
            let (digests, digest_expires) = resolve_digests(caller, source, &mut data, self.digest_root.as_deref(), now_ms)?;
            // A card's `sys.chat` is its own app's (glance_chat.rs), and its
            // transcript is the host's, never the data's.
            crate::glance_chat::check_publisher(source, &app)?;
            let seeded = match &mail {
                Some(binding) => crate::mail_card::seed_publication(binding, source, &data)?,
                None => crate::glance_chat::seed(&app, source, &data, &Default::default()),
            };
            (crate::glance_card::lower(source, &seeded)?.into(), digests, digest_expires)
        };
        let binds_digests = !digests.is_empty();
        let summary = note_summary(&json!({"summary": args.get("summary"), "data": &data}));
        let card = GlanceCard {
            app: app.clone(),
            account: mail.as_ref().map(|b| b.account.clone()).or_else(||
                crate::app_storage::host().and_then(|_| crate::ai_host::contained::account_of(&app))),
            card_id: card_id.to_string(),
            title: title.to_string(),
            summary,
            priority,
            published_ms: now_ms,
            // Never past the digest it shows.
            expires_ms: digest_expires.map_or(now_ms + expires_s * 1000, |d| d.min(now_ms + expires_s * 1000)),
            open_app: caller.launch_id().to_string(),
            route,
            body,
            contained: matches!(caller, Caller::Contained { .. }),
            digests,
            l0: (kind == "source").then(|| Arc::new(L0Source { source: source.to_string(), data, mail })),
        };
        if card.retained_bytes() > PER_APP_BYTES {
            return Err("The rendered card exceeds the retained payload budget".into());
        }
        let expires_at = card.expires_ms;
        if let Some(i) = replacing {
            self.cards.remove(i);
        }
        let key = card.key();
        self.cards.push(card);
        // A scrolling feed admits new cards independently of its current row
        // count. Retire older payloads only under actual storage pressure.
        self.trim_payloads(Some(&key), PER_APP_BYTES, STORE_BYTES);
        if binds_digests {
            self.retain_digests(&app);
        }
        Ok(json!({"card_id": card_id, "replaced": replacing.is_some(), "expires_at": expires_at}))
    }

    /// Keep a bounded payload working set without turning the viewport into
    /// a publication quota. Cold restore has no protected key, so reordering
    /// persisted records cannot displace newer, higher-priority cards.
    fn trim_payloads(&mut self, protected: Option<&str>, per_app: usize, total_budget: usize) {
        use std::collections::{HashMap, HashSet};
        let mut totals = HashMap::<String, usize>::new();
        let mut total = 0;
        let mut candidates = Vec::new();
        for card in &self.cards {
            let bytes = card.retained_bytes();
            total += bytes;
            *totals.entry(card.app.clone()).or_default() += bytes;
            if protected != Some(card.key().as_str()) {
                candidates.push((card.priority, card.published_ms, card.key(), card.app.clone(), bytes));
            }
        }
        if total <= total_budget && totals.values().all(|bytes| *bytes <= per_app) {
            return;
        }
        candidates.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
        let mut retired = HashSet::new();
        for (_, _, key, app, bytes) in candidates {
            if total > total_budget || totals[&app] > per_app {
                total -= bytes;
                *totals.get_mut(&app).unwrap() -= bytes;
                retired.insert(key);
            }
        }
        self.cards.retain(|card| !retired.contains(&card.key()));
    }

    /// Merge one revalidated durable publication. A skipped/oversized older
    /// record cannot erase the current version or cause generation churn.
    fn restore_publication(&mut self, card: GlanceCard, now_ms: u64) -> bool {
        let expired = self.expire(now_ms);
        if card.expires_ms <= now_ms || card.retained_bytes() > PER_APP_BYTES {
            return expired;
        }
        let key = card.key();
        if self.cards.iter().any(|old| old.key() == key && old.published_ms >= card.published_ms) {
            return expired;
        }
        let before: Vec<_> = self.cards.iter().map(|card| (card.key(), card.published_ms)).collect();
        self.cards.retain(|old| old.key() != key);
        self.cards.push(card);
        self.trim_payloads(None, PER_APP_BYTES, STORE_BYTES);
        expired || before != self.cards.iter().map(|card| (card.key(), card.published_ms)).collect::<Vec<_>>()
    }

    /// Digest retention for `app` (glance_digest.rs): each template folder
    /// keeps its newest results and every run a live card of the app binds.
    /// Called after a publish that binds digests.
    fn retain_digests(&self, app: &str) {
        let Some(root) = &self.digest_root else { return };
        let pinned: Vec<String> = self.cards.iter().filter(|c| c.app == app).flat_map(|c| c.digests.iter().cloned()).collect();
        crate::glance_digest::retain(root, app, &pinned);
    }

    /// [`note_summary`] for the card just published under `key`, read from
    /// the card's data as the store keeps it: a `sys.digest` value there is
    /// the host's (glance_digest.rs), never what the publisher sent.
    fn note_summary_for(&self, key: &str, args: &Value) -> String {
        match self.cards.iter().find(|c| c.key() == key).and_then(|c| c.l0.as_ref()) {
            Some(l0) => {
                let mut kept = args.clone();
                kept["data"] = l0.data.clone();
                note_summary(&kept)
            }
            None => note_summary(args),
        }
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
        self.take(key).is_some()
    }

    /// Take the card with this key out (a dismiss that can be undone).
    pub fn take(&mut self, key: &str) -> Option<GlanceCard> {
        let at = self.cards.iter().position(|c| c.key() == key)?;
        Some(self.cards.remove(at))
    }

    /// Put cards back that were taken out ([`Self::take`]): each keeps its
    /// publish and expiry, unless it expired meanwhile or its app published
    /// the same card again since. How many came back.
    pub fn restore(&mut self, cards: Vec<GlanceCard>, now_ms: u64) -> usize {
        let mut restored = Vec::new();
        for card in cards {
            if card.expires_ms <= now_ms || self.cards.iter().any(|c| c.key() == card.key()) {
                continue;
            }
            restored.push(card.key());
            self.cards.push(card);
        }
        self.trim_payloads(None, PER_APP_BYTES, STORE_BYTES);
        restored.iter().filter(|key| self.cards.iter().any(|card| card.key() == **key)).count()
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

    /// Cards in priority/recency order; callers can request a bounded subset.
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
    /// The launcher id the card opens (its app's icon on the toast).
    pub open_app: String,
    pub title: String,
    /// The notification's second line ([`note_summary`]); may be empty.
    pub summary: String,
}

/// A card notification's second line: the publisher's `summary`, else the
/// card's own summary in its data (`summary`, or a source record's, as a
/// notice's `note.summary`), else nothing.
pub fn note_summary(args: &Value) -> String {
    let pick = |v: Option<&Value>| v.and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let record = || args.get("data").and_then(Value::as_object).and_then(|data| data.values().find_map(|v| pick(v.get("summary"))));
    let summary = pick(args.get("summary")).or_else(|| pick(args.pointer("/data/summary"))).or_else(record).unwrap_or_default();
    match summary.char_indices().nth(SUMMARY_MAX) {
        Some((cut, _)) => format!("{}\u{2026}", &summary[..cut]),
        None => summary,
    }
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
    f(guard.get_or_insert_with(|| GlanceStore::default().with_digest_root(crate::glance_digest::digest_root())))
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
    request_bound(caller, service, args, None)
}

fn request_bound(caller: &Caller, service: &str, args: &Value, mail: Option<crate::mail_card::Binding>) -> Result<Value, String> {
    let gate = crate::mail_card::publication_guard();
    let now = now_ms();
    let method = service.strip_prefix("glance.").unwrap_or(service);
    caller.may_use()?;
    if method == "withdraw" {
        let id = args["card_id"].as_str().filter(|id| valid_card_id(id)).ok_or("Invalid card_id")?;
        crate::mail_card::remove_publication(&gate, &format!("{}/{id}", caller.app()))?;
        #[cfg(any(feature = "app-hub", native_mobile))]
        crate::mail_background::dismiss(&format!("{}/{id}", caller.app()), true)?;
    }
    let before = with_store(|store| store.cards.clone());
    let binding = mail.clone();
    let result = with_store(|store| match method {
        "publish" => match mail {
            Some(binding) => store.publish_bound(caller, args, now, Some(binding)),
            None => store.publish(caller, args, now),
        },
        "withdraw" => store.withdraw(caller, args, now),
        "list" => store.list(caller, now),
        other => Err(format!("glance has no method {other:?}")),
    });
    if result.is_ok() && method != "list" {
        let after = with_store(|store| store.cards.clone());
        for old in &before {
            if !after.iter().any(|card| card.key() == old.key()) {
                crate::mail_card::set_publication_dismissed(&gate, old, true)?;
                // A payload-budget retirement must also retire its native
                // notification record, or the next outbox scan resurrects it
                // and can displace the publication we just admitted.
                #[cfg(any(feature = "app-hub", native_mobile))]
                crate::mail_background::dismiss(&old.key(), true)?;
                #[cfg(any(feature = "app-hub", native_mobile))]
                calendar_dismissed(old, true)?;
            }
        }
        if let Some(binding) = &binding {
            let card = after.iter().find(|card| card.key() == binding.key()).ok_or("Published card missing")?;
            crate::mail_card::save_publication(&gate, args, binding, card.published_ms, card.expires_ms)?;
        }
    }
    #[cfg(any(feature = "app-hub", native_mobile))]
    if result.is_ok() && method == "publish" {
        let key = format!("{}/{}", caller.app(), args["card_id"].as_str().unwrap_or_default());
        if let Some(card) = with_store(|store| store.card(&key, now)) {
            crate::mail_background::record(args, &card)?;
        }
    }
    if result.is_ok() && method == "publish" && args.get("notify").and_then(Value::as_bool) == Some(true) {
        let card_id = args.get("card_id").and_then(Value::as_str).unwrap_or_default();
        let title = args.get("title").and_then(Value::as_str).unwrap_or_default().trim();
        let key = format!("{}/{card_id}", caller.app());
        let summary = with_store(|store| store.note_summary_for(&key, args));
        NOTES.lock().unwrap().push(GlanceNote {
            key,
            app: caller.app().to_string(),
            open_app: caller.launch_id().to_string(),
            title: title.to_string(),
            summary,
        });
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
    crate::mail_card::restore_publications();
    #[cfg(any(feature = "app-hub", native_mobile))]
    restore_calendar_publications();
    if with_store(|store| {
        let before = store.len();
        store.expire(now_ms());
        store.cards.retain(GlanceCard::account_valid);
        before != store.len()
    }) {
        changed();
    }
}

/// The published card with this key (`app/card_id`), while it is live.
pub fn card(key: &str) -> Option<GlanceCard> {
    with_store(|store| store.card(key, now_ms())).filter(GlanceCard::account_valid)
}

/// The person dismissed the card with this key (`app/card_id`).
pub fn dismiss(key: &str) -> bool {
    dismiss_all(&[key.to_string()]) == 1
}

/// Retire a completed host-bound reply without deleting its draft or receipts,
/// changing the person's Undo action, or revoking the visible send receipt.
#[cfg(any(feature = "app-hub", native_mobile))]
pub(crate) fn retire_completed_mail(_gate: &std::sync::MutexGuard<'static, ()>, binding: &crate::mail_card::Binding) {
    let key = binding.key();
    let removed = with_store(|store| {
        let matches = store.cards.iter().find(|card| card.key() == key)
            .and_then(|card| card.l0.as_ref()).and_then(|l| l.mail.as_ref())
            .is_some_and(|current| current.account == binding.account && current.draft_id == binding.draft_id
                && current.source_message == binding.source_message);
        matches && store.take(&key).is_some()
    });
    if removed {
        NOTES.lock().unwrap().retain(|note| note.key != key);
        changed();
    }
}

/// The cards the person dismissed last (one card's close, or Clear all),
/// kept so [`undo_dismiss`] can bring them back.
static UNDO: Mutex<Vec<GlanceCard>> = Mutex::new(Vec::new());

/// The person dismissed these cards: they go, as if their apps had
/// withdrawn them, and the next [`undo_dismiss`] brings them back. How many
/// went.
pub fn dismiss_all(keys: &[String]) -> usize {
    let gate = crate::mail_card::publication_guard();
    let gone: Vec<GlanceCard> = with_store(|store| keys.iter().filter_map(|key| {
        let card = store.card(key, now_ms())?;
        match crate::mail_card::set_publication_dismissed(&gate, &card, true) {
            Ok(true) => {
                #[cfg(any(feature = "app-hub", native_mobile))]
                if crate::mail_background::dismiss(key, true).is_err() { return None; }
                #[cfg(any(feature = "app-hub", native_mobile))]
                if calendar_dismissed(&card, true).is_err() { return None; }
                store.take(key)
            },
            _ => None,
        }
    }).collect());
    let count = gone.len();
    if count > 0 {
        *UNDO.lock().unwrap() = gone;
        changed();
    }
    count
}

/// Put back the cards the last dismiss took: the keys of those that came
/// back (one its app published anew meanwhile stays as it is now).
pub fn undo_dismiss() -> Vec<String> {
    let gate = crate::mail_card::publication_guard();
    let cards = std::mem::take(&mut *UNDO.lock().unwrap());
    let cards: Vec<_> = cards.into_iter().filter(|card| {
        card.expires_ms > now_ms() && crate::glance::card(&card.key()).is_none()
            && crate::mail_card::publication_can_undo(card, now_ms())
            && crate::mail_card::set_publication_dismissed(&gate, card, false) == Ok(true)
    }).collect();
    #[cfg(any(feature = "app-hub", native_mobile))]
    let cards: Vec<_> = cards.into_iter().filter(|c| crate::mail_background::dismiss(&c.key(), false).is_ok()).collect();
    #[cfg(any(feature = "app-hub", native_mobile))]
    let cards: Vec<_> = cards.into_iter().filter(|c| calendar_dismissed(c, false).is_ok()).collect();
    let keys: Vec<String> = cards.iter().map(|c| c.key()).collect();
    let now = now_ms();
    let back = with_store(|store| {
        store.restore(cards, now);
        keys.into_iter().filter(|key| store.card(key, now).is_some()).collect::<Vec<_>>()
    });
    if !back.is_empty() {
        changed();
    }
    back
}

/// The complete retained feed, ordered by priority then recency.
pub fn shown() -> Vec<GlanceCard> {
    expire_now();
    with_store(|store| store.shown(now_ms(), usize::MAX))
}

/// Both the desktop panel and the phone feed scroll the complete retained set.
/// Clear all takes that same set.
pub fn listed() -> Vec<GlanceCard> {
    expire_now();
    with_store(|store| store.shown(now_ms(), usize::MAX))
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

#[cfg(any(feature = "app-hub", native_mobile))]
pub fn withdraw_for(app: &str, id: &str) -> Result<(), String> {
    let caller = Caller::Contained {app:app.into(),granted:crate::host_tools::script_apps::grants(app,"glance")};
    request(&caller,"glance.withdraw",&json!({"card_id":id})).map(|_| ())
}

#[cfg(any(feature = "app-hub", native_mobile))]
fn calendar_dismissed(card: &GlanceCard, dismissed: bool) -> Result<(), String> {
    if card.app != "os.calendar" { return Ok(()); }
    if let Some(host) = crate::app_storage::host() {
        octosense_calendar_service::set_dismissed(&host.layout().apps_root().join(".host"),&card.card_id,dismissed)?;
    }
    Ok(())
}

#[cfg(any(feature = "app-hub", native_mobile))]
fn restore_calendar_publications() {
    static RESTORED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if RESTORED.load(Ordering::Acquire) { return; }
    let Some(host) = crate::app_storage::host() else { return; };
    if !crate::host_tools::script_apps::grants("os.calendar","glance") { return; }
    let root = host.layout().apps_root().join(".host");
    let now = now_ms();
    let Ok(publications) = octosense_calendar_service::publications(&root,now) else { return; };
    let events = octosense_calendar_service::load(&root);
    for p in publications {
        let Some(event) = events.iter().find(|e| e.id == p.event) else { continue; };
        let mut args = octosense_calendar_service::saved_event_card_args(event,p.args["card_id"].as_str().unwrap_or(&event.id),p.args["priority"].as_i64().unwrap_or(70));
        args["notify"] = json!(false);
        args["expires"] = json!(EXPIRES_MAX_S);
        let mut temporary = GlanceStore::default();
        if temporary.publish(&Caller::granted("os.calendar"),&args,now).is_err() { continue; }
        let Some(mut card) = temporary.cards.pop() else { continue; };
        card.published_ms = p.published; card.expires_ms = p.expires;
        if with_store(|store| store.restore_publication(card,now)) { changed(); }
    }
    RESTORED.store(true,Ordering::Release);
}

#[cfg(any(feature = "app-hub", native_mobile))]
pub fn publish_l0_for(app: &str, args: &Value) -> Result<Value, String> {
    if args.get("script").is_some() {
        return Err("Generated Mail cards accept L0 source only".into());
    }
    let source = args["source"].as_str().ok_or("Missing L0 source")?;
    check_generated_l0(source)?;
    check_generated_data(source, &args["data"])?;
    publish_for(app, args)
}

/// Opt-in developer harness; normal app attribution/admission still applies.
#[cfg(all(feature = "dev-mode", any(feature = "app-hub", native_mobile)))]
pub(crate) fn publish_test_fixtures(path: &str) -> Result<usize, String> {
    // Real app publishers pass their admitted glance grant. test.*
    // prototypes carry no app capabilities and have no agent.
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path).map_err(|e| e.to_string())?
        .take(256 * 1024 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > 256 * 1024 { return Err("Fixture file too large".into()); }
    let fixtures: Vec<serde_json::Value> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if fixtures.len() > 12 { return Err("Too many fixtures".into()); }
    for fixture in &fixtures {
        let app = fixture["app"].as_str().ok_or("Missing fixture publisher")?;
        if app.starts_with("test.") {
            request(&Caller::Native(app.into()), "glance.publish", &fixture["args"])?;
        } else { publish_for(app, &fixture["args"])?; }
    }
    Ok(fixtures.len())
}

/// Only installed as the Rust Mail service callback. Ordinary glance.publish
/// rejects the reserved field, even when the caller is os.mail.
#[cfg(any(feature = "app-hub", native_mobile))]
pub fn publish_mail_l0_for(app: &str, args: &Value) -> Result<Value, String> {
    let Some(metadata) = args.get("mail_binding") else { return publish_l0_for(app, args); };
    if app != "os.mail" || args.get("script").is_some() { return Err("Bound Mail cards require os.mail L0 source".into()); }
    let source = args["source"].as_str().ok_or("Missing L0 source")?;
    check_generated_l0(source)?;
    check_generated_data(source, &args["data"])?;
    let mut binding: crate::mail_card::Binding = serde_json::from_value(metadata.clone()).map_err(|_| "Invalid Mail host binding")?;
    binding.card_id = args["card_id"].as_str().ok_or("Missing card_id")?.to_string();
    binding.validate()?;
    let caller = Caller::Contained { app: app.to_string(), granted: crate::host_tools::script_apps::grants(app, "glance") };
    caller.may_use()?;
    if crate::mail_card::completed(&binding) {
        // A repair/retry of a completed message must not resurrect its card or
        // notification. Return a receipt so the incoming event can finish.
        return Ok(json!({"card_id":binding.card_id, "replaced":false, "expires_at":now_ms(), "processed":true}));
    }
    request_bound(&caller, "glance.publish", args, Some(binding))
}

/// Hide an inactive account's cards without dismissing its durable publications.
#[cfg(any(feature = "app-hub", native_mobile))]
pub(crate) fn hide_other_mail_accounts(active: Option<&str>) {
    let removed = with_store(|store| {
        let before = store.cards.len();
        store.cards.retain(|card| card.l0.as_ref().and_then(|l| l.mail.as_ref())
            .is_none_or(|binding| active == Some(binding.account.as_str())));
        before != store.cards.len()
    });
    if removed { changed(); }
}

/// Revalidate cached L0, retaining the original absolute lifetime. The caller
/// holds the publication gate; restore neither notifies nor rewrites the cache.
#[cfg(any(feature = "app-hub", native_mobile))]
pub(crate) fn restore_mail_publication(args: &Value, binding: crate::mail_card::Binding, published: u64, expires: u64) -> Result<(), String> {
    let now = now_ms();
    if expires <= now || published >= expires { return Err("Expired Mail publication".into()); }
    if crate::mail_card::completed(&binding) { return Err("This reply is completed; its card is no longer active".into()); }
    let source = args["source"].as_str().ok_or("Missing L0 source")?;
    check_generated_l0(source)?;
    check_generated_data(source, &args["data"])?;
    let caller = Caller::Contained { app: binding.publisher.clone(), granted: crate::host_tools::script_apps::grants(&binding.publisher, "glance") };
    let mut temporary = GlanceStore::default().with_digest_root(crate::glance_digest::digest_root());
    let mut args = args.clone();
    args["notify"] = json!(false);
    args["expires"] = json!(EXPIRES_MAX_S);
    temporary.publish_bound(&caller, &args, now, Some(binding))?;
    let mut card = temporary.cards.pop().ok_or("Missing restored Mail card")?;
    card.published_ms = published;
    card.expires_ms = card.expires_ms.min(expires);
    let inserted = with_store(|store| store.restore_publication(card, now));
    if inserted { changed(); }
    Ok(())
}

/// Native notification restore: the account comes from private host storage.
#[cfg(any(feature = "app-hub", native_mobile))]
pub(crate) fn restore_mail_notification(args: &Value, account: &str, published: u64, expires: u64) -> Result<(), String> {
    let _gate = crate::mail_card::publication_guard();
    if crate::ai_host::contained::account_of("os.mail").as_deref() != Some(account) { return Err("Inactive account".into()); }
    if let Some(metadata) = args.get("mail_binding") {
        let mut binding: crate::mail_card::Binding = serde_json::from_value(metadata.clone()).map_err(|_| "Invalid binding")?;
        binding.card_id = args["card_id"].as_str().ok_or("Missing card id")?.into();
        if binding.account != account { return Err("Wrong account".into()); }
        crate::mail_card::read(&binding)?;
        return restore_mail_publication(args, binding, published, expires);
    }
    let now = now_ms();
    if expires <= now || published >= expires { return Err("Expired publication".into()); }
    let source = args["source"].as_str().ok_or("Missing source")?;
    check_generated_l0(source)?;
    check_generated_data(source, &args["data"])?;
    let caller = Caller::Contained { app:"os.mail".into(), granted:crate::host_tools::script_apps::grants("os.mail", "glance") };
    let mut temporary = GlanceStore::default().with_digest_root(crate::glance_digest::digest_root());
    let mut args = args.clone();
    args["notify"] = json!(false);
    args["expires"] = json!(EXPIRES_MAX_S);
    temporary.publish(&caller, &args, now)?;
    let mut card = temporary.cards.pop().ok_or("Missing publication")?;
    card.account = Some(account.to_owned());
    card.published_ms = published;
    card.expires_ms = card.expires_ms.min(expires);
    let inserted = with_store(|store| store.restore_publication(card, now));
    if inserted { changed(); }
    Ok(())
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
            let mut args = json!({
                "card_id": card_id, "title": title, "source": source, "data": data,
                "priority": 80 - i as i64, "open": {"app": "mail"}, "notify": true
            });
            // The request card's gist is its chat's, not its data's.
            if card_id == "ana-contract" {
                args["summary"] = json!("Ana asks whether you can sign by Friday, with the revised payment terms.");
            }
            args
        })
        .collect()
}

/// `OCTOSENSE_GLANCE_DEMO=many`'s notices: (app, card id, title, text).
#[cfg(any(feature = "app-hub", native_mobile))]
const DEMO_NOTICES: &[(&str, &str, &str, &str)] = &[
    ("os.calendar", "dentist", "Dentist at 3 pm", "Main St 12. Leave by 2:40 to be on time."),
    ("os.photos", "hike", "12 new photos", "From Saturday's hike; three are already favourites."),
    ("os.maps", "commute", "Traffic on your way home", "I-280 is slow: 18 minutes longer than usual."),
    ("os.youtube", "makepad", "New from a channel you follow", "Makepad: building a GPU shell in Rust (24 min)."),
    ("os.calendar", "standup", "Standup moved", "Tomorrow's standup is at 9:30 instead of 9:00."),
];

/// The News digest card (M4), bound to the digest the shell holds for
/// `os.news` under the run id `glance` (glance_digest.rs).
pub const NEWS_BRIEF_CARD: &str = include_str!("../resources/glance/news-brief.card");

/// `OCTOSENSE_GLANCE_DEMO`, once, at startup (a test path; nothing publishes
/// these otherwise): `mail` publishes the fake Mail cards as `os.mail`, each
/// with a notification; `many` adds notices from five more apps and the News
/// digest (eight cards, seven notifications: the panel's overflow and the
/// toasts' cap); `digest` publishes [`NEWS_BRIEF_CARD`] as `os.news`, bound
/// to `<apps root>/.host/toolbox/os.news/toolbox/runs/*/glance.json`; any
/// other value but `0` publishes the sample News digest as `os.news`.
pub fn publish_demo_if_asked() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let demo = std::env::var("OCTOSENSE_GLANCE_DEMO").unwrap_or_default();
        if demo.is_empty() || demo == "0" {
            return;
        }
        if demo == "mail" || demo == "many" {
            crate::glance_chat::set_demo_mail(true);
            seed_demo_mail_chat("os.mail");
            for args in demo_mail_publishes() {
                if let Err(e) = request(&Caller::granted("os.mail"), "glance.publish", &args) {
                    makepad_widgets::log!("glance: demo mail card refused: {e}");
                }
            }
            if demo == "mail" {
                return;
            }
            #[cfg(any(feature = "app-hub", native_mobile))]
            for &(app, card_id, title, body) in DEMO_NOTICES {
                let args = crate::glance_notice::publish_args(app, &json!({"title": title, "body": body, "card_id": card_id}), now_ms());
                if let Err(e) = args.and_then(|args| request(&Caller::granted(app), "glance.publish", &args)) {
                    makepad_widgets::log!("glance: demo notice {card_id} refused: {e}");
                }
            }
        }
        let (source, data) = if demo == "digest" { (NEWS_BRIEF_CARD.to_string(), json!({})) } else { demo_digest() };
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

    #[cfg(any(feature = "app-hub", native_mobile))]
    #[test]
    fn completed_mail_retirement_keeps_other_accounts_new_messages_and_undo() {
        let gate = crate::mail_card::publication_guard();
        let now = now_ms();
        let mut local = GlanceStore::default();
        local.publish(&news(), &args("completion-template"), now).unwrap();
        let template = local.cards.pop().unwrap();
        let binding = crate::mail_card::Binding {
            publisher:"os.mail".into(), account:"completion-account".into(),
            source_message:json!({"folder":"INBOX","message":"m1"}),
            draft_id:"completion_draft".into(), draft_revision:1, chat_thread:"completion-chat".into(),
            card_id:"completion-old".into(),
        };
        let make_card = |b: &crate::mail_card::Binding| {
            let mut card = template.clone();
            card.app = b.publisher.clone(); card.card_id = b.card_id.clone(); card.account = Some(b.account.clone());
            let mut source = (**card.l0.as_ref().unwrap()).clone(); source.mail = Some(b.clone());
            card.l0 = Some(Arc::new(source)); card
        };
        let mut next = binding.clone(); next.card_id = "completion-new-message".into();
        next.draft_id = "completion_next_draft".into(); next.source_message["message"] = json!("m2");
        with_store(|store| store.cards.extend([make_card(&binding), make_card(&next)]));
        for mismatch in ["account", "draft", "source"] {
            let mut wrong = binding.clone();
            match mismatch {
                "account" => wrong.account = "another-account".into(),
                "draft" => wrong.draft_id = "another_draft".into(),
                _ => wrong.source_message["message"] = json!("another-message"),
            }
            retire_completed_mail(&gate, &wrong);
            assert!(with_store(|store| store.card(&binding.key(), now).is_some()), "stale {mismatch} completion must not remove a current publication");
        }
        let undo_before: Vec<_> = UNDO.lock().unwrap().iter().map(GlanceCard::key).collect();
        retire_completed_mail(&gate, &binding);
        assert!(with_store(|store| store.card(&binding.key(), now).is_none()));
        assert!(with_store(|store| store.card(&next.key(), now).is_some()), "a newly arrived message stays actionable");
        assert_eq!(UNDO.lock().unwrap().iter().map(GlanceCard::key).collect::<Vec<_>>(), undo_before, "automatic completion must not replace a person's Undo");
        retire_completed_mail(&gate, &binding); // idempotent repeat
        with_store(|store| { store.take(&next.key()); });
    }

    fn news() -> Caller {
        Caller::granted("os.news")
    }
    fn args(card_id: &str) -> Value {
        let (source, data) = demo_digest();
        json!({"card_id": card_id, "title": "News digest", "source": source, "data": data, "open": {"app": "news"}})
    }

    #[test]
    fn generated_mail_cards_cannot_opt_into_l1() {
        assert!(check_generated_l0(&demo_digest().0).is_ok());
        let l1 = "# level: L1\nsource quote sys.quote(ticker: state.sym, fields: [last])\nstate sym { shape: text, initial: \"NVDA\" }\nstate shares { shape: number, initial: 10 }\nview root Surface { TextHero(value: shares * quote.last) }\n";
        assert!(check_level(l1).is_ok(), "general glance admission still supports declared L1");
        assert!(check_generated_l0(l1).unwrap_err().contains("L1 expressions"));
        assert!(check_generated_l0("ui.label(\"x\").set_text(\"y\")").is_err());
    }

    #[test]
    fn generated_mail_data_must_bind_every_declared_dataset_field() {
        let source = "source note sys.dataset(fields: [title, summary])\nview root Surface { TextTitle(text: note.title) }";
        assert!(check_generated_l0(source).is_ok());
        let flat = json!({"title": "Delivery", "summary": "Tuesday"});
        assert!(check_generated_data(source, &flat).unwrap_err().contains("data.note"));
        assert!(check_generated_data(source, &json!({"note": flat})).is_ok());
        assert!(check_generated_data(source, &json!({"note": {"title": "Delivery"}})).unwrap_err().contains("data.note.summary"));
        assert!(check_generated_data(source, &json!({"note": {"title": "Delivery", "summary": null}})).is_err());
        let no_fields = source.replace("fields: [title, summary]", "");
        assert!(check_generated_data(&no_fields, &json!({"note": {}})).unwrap_err().contains("nonempty fields list"));
        let empty_fields = source.replace("[title, summary]", "[]");
        assert!(check_generated_data(&empty_fields, &json!({"note": {}})).is_err());
        let nested = source.replace("note", "mail.note");
        assert!(check_generated_data(&nested, &json!({"mail": {"note": {"title": "Delivery", "summary": "Date unknown"}}})).is_ok());
        let cycle = "source a sys.dataset(id: b.title, fields: [title])\nsource b sys.dataset(id: a.title, fields: [title])\nview root Surface { TextTitle(text: a.title) }";
        assert!(check_generated_l0(cycle).is_ok());
        assert!(check_generated_data(cycle, &json!({})).unwrap_err().contains("dependencies"));
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
        let shown = store.shown(1_000, usize::MAX);
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
        store.publish(&Caller::granted("os.calendar"), &event, 1_000).expect("Calendar event template must be admitted");
        let now = cal::parse_time("2026-10-01T09:00").unwrap();
        let events = vec![cal::Event { id: "a".into(), title: "Standup".into(), start: "2026-10-02T09:30".into(), end: None, location: String::new(), notes: String::new(), request_id: String::new(), timezone: String::new() }];
        let agenda = cal::agenda_card_args(&events, 7, now);
        assert!(store.publish(&Caller::granted("os.calendar"), &agenda, 1_000).is_ok());
        let shown = store.shown(1_000, usize::MAX);
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
        let shown = store.shown(1_000, usize::MAX);
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
            let shown = store.shown(1_000, usize::MAX);
            assert_eq!((shown[0].app.as_str(), shown[0].open_app.as_str()), (app, app.strip_prefix("os.").unwrap()));
            assert!(store.publish(&Caller::granted("os.mail"), &args, 1_000).unwrap_err().contains("opens the app that published it"), "{app}");
        }
    }

    mod digest {
        use super::*;
        use crate::glance_digest::tests::{news_digest_result, now, root_with};

        fn brief(card: &str) -> Value {
            json!({"card_id": "brief", "title": "News digest", "source": card, "open": {"app": "news"}})
        }

        #[test]
        fn a_digest_card_shows_the_digest_the_host_holds() {
            let root = root_with("glance-bound", &[("os.news", "news-digest", news_digest_result())]);
            let mut store = GlanceStore::default().with_digest_root(Some(root));
            let mut args = brief(NEWS_BRIEF_CARD);
            // The publisher cannot supply the digest: its value is replaced.
            args["data"] = json!({"brief": {"summary": "Forged", "points": [], "sources": []}, "$status": {"brief": "ready"}});
            let ok = store.publish(&news(), &args, now()).unwrap();
            let card = &store.shown(now(), usize::MAX)[0];
            for want in ["Harbor City's council approved an order for 120 electric buses on Tuesday.", "Clearwater Courier", "city infrastructure", "SOURCES"] {
                assert!(card.body.contains(want), "{want:?} not in {}", card.body);
            }
            assert!(!card.body.contains("Forged") && !card.body.contains("No digest yet"), "{}", card.body);
            assert_eq!(card.digests, ["glance"]);
            // The card expires with the digest (48 h from the run), not 24 h
            // from now, whichever is sooner.
            let digest_expires = crate::glance_digest::parse_rfc3339_ms("2026-09-20T08:00:00Z").unwrap() + crate::glance_digest::MAX_AGE_MS;
            assert_eq!(ok["expires_at"], json!((now() + EXPIRES_DEFAULT_S * 1000).min(digest_expires)));
            let mut long = brief(NEWS_BRIEF_CARD);
            long["expires"] = json!(EXPIRES_MAX_S);
            assert_eq!(store.publish(&news(), &long, now()).unwrap()["expires_at"], json!(digest_expires));
        }

        /// A digest card's notification line is the digest's summary, read
        /// from the card's data as kept, never a summary the publisher put
        /// in `data`; its own `summary` argument still comes first.
        #[test]
        fn a_digest_cards_notification_summarizes_the_hosts_digest() {
            let root = root_with("glance-note", &[("os.news", "news-digest", news_digest_result())]);
            let mut store = GlanceStore::default().with_digest_root(Some(root));
            let mut args = brief(NEWS_BRIEF_CARD);
            args["data"] = json!({"brief": {"summary": "Forged"}});
            args["notify"] = json!(true);
            store.publish(&news(), &args, now()).unwrap();
            assert_eq!(note_summary(&args), "Forged", "the raw arguments carry the forged summary");
            let summary = store.note_summary_for("os.news/brief", &args);
            assert!(summary.starts_with("Sources: Harbor City approves electric bus order"), "{summary}");
            args["summary"] = json!("Your morning brief");
            assert_eq!(store.note_summary_for("os.news/brief", &args), "Your morning brief");
            // A card the store does not hold reads the arguments, as before.
            assert_eq!(store.note_summary_for("os.news/other", &json!({"data": {"pkg": {"summary": "Out"}}})), "Out");
        }

        #[test]
        fn a_card_binds_only_its_own_apps_digests() {
            let root = root_with("glance-own", &[("os.news", "news-digest", news_digest_result())]);
            let mut store = GlanceStore::default().with_digest_root(Some(root));
            let other = NEWS_BRIEF_CARD.replace("app: \"os.news\"", "app: \"os.mail\"");
            let err = store.publish(&news(), &brief(&other), now()).unwrap_err();
            assert!(err.contains("own app's digests"), "{err}");
            // Maps naming itself gets its own (empty) folder, never News's.
            let maps = Caller::granted("os.maps");
            let mut args = brief(&NEWS_BRIEF_CARD.replace("app: \"os.news\"", "app: \"os.maps\""));
            args["open"] = Value::Null;
            store.publish(&maps, &args, now()).unwrap();
            let card = &store.shown(now(), usize::MAX)[0];
            assert!(card.body.contains("No digest yet") && !card.body.contains("Harbor"), "{}", card.body);
            // The launcher id names the publisher too.
            let short = NEWS_BRIEF_CARD.replace("app: \"os.news\"", "app: \"news\"");
            store.publish(&news(), &brief(&short), now()).unwrap();
            assert!(store.shown(now(), usize::MAX).iter().any(|c| c.app == "os.news" && c.body.contains("Harbor")));
        }

        #[test]
        fn a_missing_or_expired_digest_renders_the_cards_own_absence() {
            let mut store = GlanceStore::default();
            store.publish(&news(), &brief(NEWS_BRIEF_CARD), now()).unwrap();
            let card = &store.shown(now(), usize::MAX)[0];
            assert!(card.body.contains("No digest yet"), "{}", card.body);
            assert_eq!(card.expires_ms, now() + EXPIRES_DEFAULT_S * 1000);
            let root = root_with("glance-expired", &[("os.news", "news-digest", news_digest_result())]);
            let mut store = GlanceStore::default().with_digest_root(Some(root));
            let late = now() + crate::glance_digest::MAX_AGE_MS;
            store.publish(&news(), &brief(NEWS_BRIEF_CARD), late).unwrap();
            assert!(store.shown(late, usize::MAX)[0].body.contains("No digest yet"));
        }

        #[test]
        fn publishing_a_digest_card_prunes_old_runs_but_not_bound_ones() {
            let mut runs = vec![("os.news", "news-digest", news_digest_result())];
            for i in 0..crate::glance_digest::KEEP_PER_TEMPLATE + 2 {
                let mut r = news_digest_result();
                r["run_id"] = json!(format!("r{i:02}"));
                runs.push(("os.news", "news-digest", r));
            }
            let root = root_with("glance-retain", &runs);
            let dir = root.join("os.news/toolbox/runs/news-digest");
            // `glance` is the oldest file, yet a live card binds it.
            let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1);
            std::fs::File::options().write(true).open(dir.join("glance.json")).unwrap().set_modified(old).unwrap();
            let mut store = GlanceStore::default().with_digest_root(Some(root));
            store.publish(&news(), &brief(NEWS_BRIEF_CARD), now()).unwrap();
            let left = std::fs::read_dir(&dir).unwrap().count();
            assert_eq!(left, crate::glance_digest::KEEP_PER_TEMPLATE + 1);
            assert!(dir.join("glance.json").exists());
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
    fn the_same_card_id_replaces_without_a_four_card_quota() {
        let mut store = GlanceStore::default();
        store.publish(&news(), &args("digest"), 0).unwrap();
        let mut a = args("digest");
        a["title"] = json!("Evening digest");
        assert_eq!(store.publish(&news(), &a, 10).unwrap()["replaced"], true);
        assert_eq!(store.len(), 1);
        assert_eq!(store.shown(10, 9)[0].title, "Evening digest");
        for i in 1..40 {
            store.publish(&news(), &args(&format!("c{i}")), 100_000 * i as u64).unwrap();
        }
        assert_eq!(store.len(), 40, "card count is not a publication quota");
        assert!(store.publish(&news(), &args("one-more"), 4_000_000).is_ok());
        assert!(store.publish(&news(), &args("digest"), 4_100_000).is_ok());
        assert_eq!(store.len(), 41);
        assert!(store.card("os.news/c1", 4_100_000).is_some(), "earlier cards remain scrollable");
    }

    #[test]
    fn payload_pressure_retires_older_cards_without_rejecting_the_new_one() {
        let mut store = GlanceStore::default();
        store.publish(&news(), &args("older"), 1).unwrap();
        let mut incoming = args("incoming");
        incoming["priority"] = json!(0);
        store.publish(&news(), &incoming, 2).unwrap();
        let mut maps = args("commute");
        maps["open"] = Value::Null;
        store.publish(&Caller::granted("os.maps"), &maps, 3).unwrap();
        let one_card = store.cards.iter().map(GlanceCard::retained_bytes).max().unwrap();
        store.trim_payloads(Some("os.news/incoming"), one_card, usize::MAX);
        assert!(store.card("os.news/older", 3).is_none());
        assert!(store.card("os.news/incoming", 3).is_some(), "new lower-priority publication still arrives");
        assert!(store.card("os.maps/commute", 3).is_some(), "publisher pressure does not retire another app");
        store.trim_payloads(Some("os.news/incoming"), usize::MAX, one_card);
        assert_eq!(store.len(), 1);
        assert!(store.card("os.news/incoming", 3).is_some());
    }

    #[test]
    fn durable_restore_does_not_reinstate_the_old_card_count_limits() {
        let mut original = GlanceStore::default();
        for i in 0..40 {
            original.publish(&news(), &args(&format!("c{i}")), 100_000 * i).unwrap();
        }
        let mut restored = GlanceStore::default();
        for card in original.cards.iter().rev() {
            assert!(restored.restore_publication(card.clone(), 4_000_000));
        }
        assert_eq!(restored.len(), 40);
        for card in &original.cards {
            assert!(!restored.restore_publication(card.clone(), 4_000_000));
        }
        let mut oversized = original.cards[0].clone();
        oversized.published_ms += 1;
        oversized.body = "x".repeat(PER_APP_BYTES + 1).into();
        assert!(!restored.restore_publication(oversized, 4_000_000));
        assert_eq!(restored.card("os.news/c0", 4_000_000).unwrap().published_ms, 0);
    }

    #[test]
    fn restore_keeps_more_than_six_cards_and_has_stable_budget_ordering() {
        let mut original = GlanceStore::default();
        for i in 0..12 {
            original.publish(&news(), &args(&format!("c{i}")), 100_000 * i).unwrap();
        }
        let cards = original.cards.clone();
        let mut restored = GlanceStore::default();
        assert_eq!(restored.restore(cards.clone(), 1_200_000), 12);
        assert_eq!(restored.shown(1_200_000, usize::MAX).len(), 12);
        assert_eq!(restored.restore(cards, 1_200_000), 0, "no duplicates on a second restore");
        let budget = restored.cards.iter().map(GlanceCard::retained_bytes).sum::<usize>() / 2;
        let mut reordered = GlanceStore { cards: restored.cards.iter().rev().cloned().collect(), ..Default::default() };
        restored.trim_payloads(None, usize::MAX, budget);
        reordered.trim_payloads(None, usize::MAX, budget);
        let keys = |store: &GlanceStore| store.shown(1_200_000, usize::MAX).iter().map(GlanceCard::key).collect::<Vec<_>>();
        assert_eq!(keys(&restored), keys(&reordered), "restore order cannot churn the retained set");
        assert!(restored.card("os.news/c11", 1_200_000).is_some());
        assert!(restored.cards.iter().map(GlanceCard::retained_bytes).sum::<usize>() <= budget);
    }

    /// A card's notification gets its publisher's summary, else the card's
    /// own (a source record's, a notice's), clipped; a long one is refused
    /// at publish.
    #[test]
    fn a_cards_notification_says_its_gist() {
        assert_eq!(note_summary(&json!({"summary": " Out today ", "data": {"pkg": {"summary": "x"}}})), "Out today");
        assert_eq!(note_summary(&json!({"data": {"pkg": {"summary": "Out for delivery"}}})), "Out for delivery");
        assert_eq!(note_summary(&json!({"data": {"note": {"title": "Hi", "summary": "From the agent"}}})), "From the agent");
        assert_eq!(note_summary(&json!({"data": {"msg": {"title": "Hi"}}})), "");
        let long = "x".repeat(SUMMARY_MAX + 5);
        assert_eq!(note_summary(&json!({"data": {"summary": long}})).chars().count(), SUMMARY_MAX + 1, "clipped with an ellipsis");
        let mut store = GlanceStore::default();
        let mut a = args("digest");
        a["summary"] = json!("y".repeat(SUMMARY_MAX + 1));
        assert!(store.publish(&news(), &a, 1_000).unwrap_err().contains("summary"));
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

    /// A dismissed card comes back on undo, with its publish and expiry,
    /// unless it expired or its app published it again meanwhile.
    #[test]
    fn a_dismissed_card_comes_back_on_undo() {
        let mut store = GlanceStore::default();
        store.publish(&news(), &args("digest"), 1_000).unwrap();
        let key = store.shown(1_000, 9)[0].key();
        let card = store.take(&key).unwrap();
        assert!(store.is_empty());
        assert_eq!(store.restore(vec![card.clone()], 2_000), 1);
        assert_eq!(store.shown(2_000, 9)[0].published_ms, 1_000, "as published");
        let again = store.take(&key).unwrap();
        store.publish(&news(), &args("digest"), 3_000).unwrap();
        assert_eq!(store.restore(vec![again], 3_000), 0, "published anew meanwhile");
        assert_eq!(store.restore(vec![card], card_expiry(1_000) + 1), 0, "expired meanwhile");
    }

    fn card_expiry(published_ms: u64) -> u64 {
        published_ms + EXPIRES_DEFAULT_S * 1000
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
    fn the_complete_feed_orders_by_priority_then_recency() {
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
        let shown = store.shown(t, usize::MAX);
        assert_eq!(shown.len(), 8);
        let keys: Vec<String> = shown.iter().map(GlanceCard::key).collect();
        assert_eq!(&keys[..4], ["os.d/c1", "os.c/c1", "os.b/c1", "os.a/c1"]);
        assert_eq!(&keys[4..], ["os.d/c0", "os.c/c0", "os.b/c0", "os.a/c0"]);
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
    #[test]
    fn publication_account_is_host_metadata_and_stale_accounts_cannot_open() {
        let mut store = GlanceStore::default();
        let mut input = args("account-bound");
        input.as_object_mut().unwrap().remove("open");
        input["account"] = json!("forged-account");
        store.publish(&Caller::granted("os.account-probe"), &input, 0).unwrap();
        let mut card = store.card("os.account-probe/account-bound", 0).unwrap();
        assert_ne!(card.account.as_deref(), Some("forged-account"));
        card.account = Some("signed-out-test-account".into());
        assert!(!card.account_valid(), "opening cannot rebind an old publication to the new account");
    }

}
