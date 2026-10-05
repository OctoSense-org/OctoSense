//! Host bindings for composed Mail cards (ADR 0007). Generated data never
//! supplies account identity, durable drafts, or approval authority.
use octoscript_ui_l0::{CollectionWrite, SourceArg, ValueOrigin};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static UI_GENERATION: AtomicU64 = AtomicU64::new(0);
fn changed() {
    UI_GENERATION.fetch_add(1, Ordering::Relaxed);
    makepad_widgets::makepad_platform::SignalToUI::set_ui_signal();
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub publisher: String,
    pub account: String,
    pub source_message: Value,
    pub draft_id: String,
    pub draft_revision: u64,
    pub chat_thread: String,
    #[serde(default)]
    pub card_id: String,
}

impl Binding {
    pub fn key(&self) -> String {
        format!("{}/{}", self.publisher, self.card_id)
    }
    fn draft_key(&self) -> String {
        format!("{}/{}/{}", self.publisher, self.account, self.draft_id)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.publisher != "os.mail"
            || self.account.is_empty()
            || self.account.len() > 512
            || self.draft_id.is_empty()
            || self.draft_id.len() > 64
            || !self
                .draft_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            || !self.source_message.is_object()
            || self.chat_thread.is_empty()
        {
            return Err("Invalid host Mail card binding".into());
        }
        Ok(())
    }
}

pub fn account_valid(account: &str) -> bool {
    crate::ai_host::contained::account_of("os.mail").as_deref() == Some(account)
        && crate::app_storage::host()
            .is_some_and(|storage| !storage.is_signed_out("os.mail", Some(account)))
}

pub fn host_dir() -> Result<std::path::PathBuf, String> {
    crate::app_storage::host()
        .map(|s| s.layout().apps_root().join(".host"))
        .ok_or_else(|| "Mail storage is unavailable".into())
}

/// The platform must supply positive evidence; an app NAV or a remote
/// instrument event can never manufacture it. See the platform input guard.
pub fn trusted_user_gesture() -> bool {
    makepad_widgets::makepad_platform::trusted_user_input()
}

pub fn generation() -> u64 {
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        octosense_mail_service::drafts::generation() + UI_GENERATION.load(Ordering::Relaxed)
    }
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    {
        UI_GENERATION.load(Ordering::Relaxed)
    }
}

pub(crate) fn read(binding: &Binding) -> Result<Value, String> {
    binding.validate()?;
    if !account_valid(&binding.account) {
        return Err(
            "The Mail account changed. Reopen this card under its original account.".into(),
        );
    }
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        let value = octosense_mail_service::drafts::read(
            &host_dir()?,
            &binding.publisher,
            &binding.account,
            &binding.draft_id,
        )?;
        if value["chat_thread"].as_str() != Some(binding.chat_thread.as_str())
            || value["source_message"] != binding.source_message
        {
            return Err("The Mail card no longer matches its original email".into());
        }
        Ok(value)
    }
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    {
        Err("Mail is unavailable in this packaging".into())
    }
}

/// Check every Mail source against the out-of-band host binding. Mail source
/// IDs are literal host-issued IDs, never mutable state or model data.
pub fn check_sources(source: &str, binding: Option<&Binding>) -> Result<(), String> {
    for request in octoscript_ui_l0::source_plan(source).requests {
        if !matches!(
            request.helper.as_str(),
            "sys.mail_draft" | "sys.mail_review"
        ) {
            continue;
        }
        let binding = binding.ok_or("Mail sources require a host-bound Mail publication")?;
        binding.validate()?;
        let arg = |name: &str| request.args.iter().find(|(n, _)| n == name).map(|(_, v)| v);
        if !matches!(arg("app"), Some(SourceArg::Text(app)) if app == &binding.publisher)
            || !matches!(arg("id"), Some(SourceArg::Text(id)) if id == &binding.draft_id)
        {
            return Err("Mail sources must name this card's publisher and literal draft ID".into());
        }
    }
    Ok(())
}

fn set_path(data: &mut Value, path: &str, value: Value) {
    let mut parts = path.split('.').peekable();
    let mut at = data;
    while let Some(part) = parts.next() {
        if !at.is_object() {
            *at = json!({});
        }
        if parts.peek().is_none() {
            at[part] = value;
            return;
        }
        at = at
            .as_object_mut()
            .unwrap()
            .entry(part)
            .or_insert_with(|| json!({}));
    }
}

fn display_value(snapshot: &Value, error: Option<&str>) -> Value {
    let suggestion = snapshot["suggestions"]
        .as_array()
        .and_then(|v| v.last())
        .cloned()
        .unwrap_or(Value::Null);
    let attempt = snapshot["attempts"]
        .as_array()
        .and_then(|v| v.last())
        .cloned()
        .unwrap_or(Value::Null);
    json!({
        "draft_id":snapshot["draft_id"], "revision":snapshot["revision"],
        "operation_id":attempt["operation_id"].as_str().unwrap_or(""),
        "to":snapshot["to"], "subject":snapshot["subject"], "body":snapshot["body"],
        "status":error.map(str::to_owned).unwrap_or_else(|| snapshot["status"].as_str().unwrap_or("draft").to_string()),
        "chat_thread":snapshot["chat_thread"], "ai_written":matches!(snapshot["body_origin"].as_str(), Some("model" | "model_accepted")),
        "suggestion_id":suggestion["suggestion_id"].as_str().unwrap_or(""),
        "suggestion_body":suggestion["body"].as_str().unwrap_or("")
    })
}

/// Clear all transcript answers when the bound account/draft cannot be read.
/// Generated data must never masquerade as a host-owned chat transcript.
pub fn unavailable_chat(source: &str, data: &Value) -> Value {
    let mut out = data.clone();
    for request in octosense_l0_chat::sources(source) {
        set_path(&mut out, &request.name, octosense_l0_chat::unavailable());
    }
    out
}

fn seed_values(source: &str, data: &Value, snapshot: &Value, error: Option<&str>) -> Value {
    let mut out = data.clone();
    let display = display_value(snapshot, error);
    for request in octoscript_ui_l0::source_plan(source).requests {
        if matches!(
            request.helper.as_str(),
            "sys.mail_draft" | "sys.mail_review"
        ) {
            // Host data overwrites every generated answer for these sources.
            set_path(&mut out, &request.name, display.clone());
        }
    }
    out
}

#[derive(Clone, Default)]
struct Dirty {
    fields: serde_json::Map<String, Value>,
    error: String,
    revision: u64,
}
fn unsaved() -> &'static Mutex<HashMap<String, Dirty>> {
    static UNSAVED: OnceLock<Mutex<HashMap<String, Dirty>>> = OnceLock::new();
    UNSAVED.get_or_init(Default::default)
}

fn draft_identity(publisher: &str, account: &str, draft_id: &str) -> String {
    format!("{publisher}/{account}/{draft_id}")
}
fn draft_review_generations() -> &'static Mutex<HashMap<String, u64>> {
    static VALUES: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    VALUES.get_or_init(Default::default)
}
pub fn draft_review_generation(publisher: &str, account: &str, draft_id: &str) -> u64 {
    draft_review_generations()
        .lock()
        .unwrap()
        .get(&draft_identity(publisher, account, draft_id))
        .copied()
        .unwrap_or(0)
}
pub fn draft_has_unsaved(publisher: &str, account: &str, draft_id: &str) -> bool {
    unsaved()
        .lock()
        .unwrap()
        .contains_key(&draft_identity(publisher, account, draft_id))
}
fn invalidate_draft_review(binding: &Binding) {
    *draft_review_generations()
        .lock()
        .unwrap()
        .entry(binding.draft_key())
        .or_default() += 1;
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        // A queued view has not captured its generation yet. Remove its token
        // now, including reviews queued by a different card of the same draft.
        let stale = {
            let mut queue = reviews().lock().unwrap();
            let mut stale = Vec::new();
            let mut i = 0;
            while i < queue.len() {
                let snapshot = queue[i].1.snapshot();
                if snapshot["publisher"] == binding.publisher
                    && snapshot["account"] == binding.account
                    && snapshot["draft_id"] == binding.draft_id
                {
                    stale.push(queue.remove(i).1);
                } else {
                    i += 1;
                }
            }
            stale
        };
        for review in stale {
            octosense_mail_service::drafts::revoke_review(review);
        }
    }
    makepad_widgets::makepad_platform::SignalToUI::set_ui_signal();
}

fn action_errors() -> &'static Mutex<HashMap<String, String>> {
    static VALUES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    VALUES.get_or_init(Default::default)
}
fn action_result(binding: &Binding, result: &Result<(), String>) {
    let moved = {
        let mut errors = action_errors().lock().unwrap();
        match result {
            Ok(()) => errors.remove(&binding.draft_key()).is_some(),
            Err(error) => errors.insert(binding.draft_key(), error.clone()).as_ref() != Some(error),
        }
    };
    if moved {
        changed();
    }
}

/// Per-view displayed revision. Unsaved edits are also retained outside the
/// L0 InstanceStore so replacing a source or closing a view cannot erase them.
pub struct Session {
    pub binding: Binding,
    snapshot: Value,
    pub error: Option<String>,
    generation: u64,
    authorized: bool,
}

impl Session {
    pub fn new(binding: Binding) -> Self {
        let mut session = Self {
            binding,
            snapshot: json!({}),
            error: None,
            generation: 0,
            authorized: false,
        };
        session.refresh();
        session
    }
    pub fn moved(&self) -> bool {
        self.generation != generation() || self.authorized != account_valid(&self.binding.account)
    }
    pub fn refresh(&mut self) {
        self.generation = generation();
        self.authorized = account_valid(&self.binding.account);
        if !self.authorized {
            self.snapshot = json!({"body":"","to":"","subject":"","status":"Account unavailable"});
            self.error =
                Some("The Mail account changed. Reopen under the original account.".into());
            cancel_for(&self.binding.key());
            return;
        }
        match read(&self.binding) {
            Ok(value) => {
                self.snapshot = value;
                self.error = None;
            }
            Err(error) => {
                self.error = Some(error);
            }
        }
        if let Some(dirty) = unsaved().lock().unwrap().get(&self.binding.draft_key()) {
            for (name, value) in &dirty.fields {
                self.snapshot[name] = value.clone();
            }
            self.snapshot["revision"] = json!(dirty.revision);
            self.error = Some(dirty.error.clone());
        }
    }
    pub fn seed(&self, source: &str, data: &Value) -> Value {
        seed_values(source, data, &self.snapshot, self.error.as_deref())
    }
    pub fn snapshot(&self) -> &Value { &self.snapshot }

    /// Native keystrokes stage in memory, outside generated-card state. Disk
    /// writes are coalesced by the editor; pending text blocks chat and review.
    pub fn stage(&mut self, field: &str, text: &str) -> Result<(), String> {
        if !matches!(field, "body" | "to" | "subject") || !account_valid(&self.binding.account) {
            return Err("This draft cannot be edited".into());
        }
        let revision = self.snapshot["revision"].as_u64().ok_or("Missing draft revision")?;
        invalidate_draft_review(&self.binding);
        let mut pending = unsaved().lock().unwrap();
        let dirty = pending.entry(self.binding.draft_key()).or_insert_with(|| Dirty {revision, ..Default::default()});
        dirty.fields.insert(field.into(), json!(text));
        dirty.error = "Unsaved changes".into();
        self.snapshot[field] = json!(text);
        self.error = Some(dirty.error.clone());
        Ok(())
    }
    pub fn flush(&mut self) -> Result<(), String> {
        let dirty = unsaved().lock().unwrap().get(&self.binding.draft_key()).cloned();
        let Some(dirty) = dirty else { return Ok(()); };
        if !account_valid(&self.binding.account) { return Err("The Mail account changed; your edits are retained".into()); }
        #[cfg(any(feature = "app-hub", native_mobile))]
        {
            let b = &self.binding;
            match octosense_mail_service::drafts::update(&host_dir()?, &b.publisher, &b.account, &b.draft_id, dirty.revision, &Value::Object(dirty.fields)) {
                Ok(value) => {
                    unsaved().lock().unwrap().remove(&b.draft_key());
                    self.snapshot = value;
                    self.error = None;
                    self.generation = generation();
                    action_result(b, &Ok(()));
                    Ok(())
                }
                Err(error) => {
                    if let Some(dirty) = unsaved().lock().unwrap().get_mut(&b.draft_key()) { dirty.error = error.clone(); }
                    self.error = Some(error.clone());
                    Err(error)
                }
            }
        }
        #[cfg(not(any(feature = "app-hub", native_mobile)))]
        { let _ = dirty; Err("Mail is unavailable".into()) }
    }
    /// Explicit conflict resolution from a native control, never an agent.
    pub fn resolve_edit(&mut self, keep_mine: bool) -> Result<(), String> {
        let durable = read(&self.binding)?;
        if keep_mine {
            if let Some(dirty) = unsaved().lock().unwrap().get_mut(&self.binding.draft_key()) {
                dirty.revision = durable["revision"].as_u64().ok_or("Missing revision")?;
            }
            self.flush()
        } else {
            discard_unsaved(&self.binding)?;
            self.refresh();
            Ok(())
        }
    }
    pub fn chat_binding(&self) -> Result<crate::glance_chat::ContextBinding, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        // Chat always uses acknowledged durable text, never a dirty overlay.
        let durable = read(&self.binding)?;
        Ok(crate::glance_chat::ContextBinding {
            kind: octosense_l0_chat::ContextKind::Mail,
            account: self.binding.account.clone(),
            thread: self.binding.chat_thread.clone(),
            source_message: json!({"identity":self.binding.source_message,"email":durable["email"]}),
            draft: json!({"draft_id":durable["draft_id"],"revision":durable["revision"],
                "to":durable["to"],"subject":durable["subject"],"body":durable["body"],"status":durable["status"]}),
        })
    }
    /// Only the native user composer requests this capability. Reading a card,
    /// seeding chat, generated NAV, and background turns never mint one.
    pub fn chat_edit_binding(&self) -> Result<crate::glance_chat::ContextBinding, String> {
        let mut binding = self.chat_binding()?;
        #[cfg(any(feature = "app-hub", native_mobile))]
        if matches!(binding.draft["status"].as_str(), Some("draft" | "awaiting_approval")) {
            binding.draft["edit_token"] = json!(octosense_mail_service::drafts::issue_chat_edit(
                &host_dir()?, &self.binding.publisher, &self.binding.account, &self.binding.draft_id,
                binding.draft["revision"].as_u64().ok_or("Missing draft revision")?,
            )?);
        }
        Ok(binding)
    }
    pub fn perform(
        &mut self,
        write: &CollectionWrite,
        origin: Option<ValueOrigin>,
        from_field: bool,
    ) -> Result<(), String> {
        let result = self.perform_inner(write, origin, from_field);
        action_result(&self.binding, &result);
        result
    }
    fn perform_inner(
        &mut self,
        write: &CollectionWrite,
        origin: Option<ValueOrigin>,
        from_field: bool,
    ) -> Result<(), String> {
        if !account_valid(&self.binding.account) {
            return Err("The Mail account changed".into());
        }
        #[cfg(any(feature = "app-hub", native_mobile))]
        {
            let dir = host_dir()?;
            let b = &self.binding;
            let revision = self.snapshot["revision"]
                .as_u64()
                .ok_or("No durable draft revision is displayed")?;
            match write.helper.as_str() {
                "sys.mail_draft" => {
                    if write.op != "set"
                        || !matches!(write.field.as_str(), "body" | "subject" | "to")
                        || !from_field
                        || origin != Some(ValueOrigin::UserInput)
                    {
                        return Err("Draft edits require a bound text field's user input".into());
                    }
                    // An attempted edit invalidates every review of this draft,
                    // even when validation/CAS fails and only dirty text remains.
                    invalidate_draft_review(b);
                    let mut changes = serde_json::Map::new();
                    changes.insert(write.field.clone(), json!(write.value));
                    match octosense_mail_service::drafts::update(
                        &dir,
                        &b.publisher,
                        &b.account,
                        &b.draft_id,
                        revision,
                        &Value::Object(changes.clone()),
                    ) {
                        Ok(value) => {
                            self.snapshot = value;
                            self.error = None;
                            let mut pending = unsaved().lock().unwrap();
                            if let Some(dirty) = pending.get_mut(&b.draft_key()) {
                                dirty.fields.remove(&write.field);
                                dirty.revision =
                                    self.snapshot["revision"].as_u64().unwrap_or(revision);
                                if dirty.fields.is_empty() {
                                    pending.remove(&b.draft_key());
                                } else {
                                    for (name, value) in &dirty.fields {
                                        self.snapshot[name] = value.clone();
                                    }
                                    self.error = Some(dirty.error.clone());
                                }
                            }
                            self.generation = generation();
                        }
                        Err(error) => {
                            let mut pending = unsaved().lock().unwrap();
                            let dirty = pending.entry(b.draft_key()).or_default();
                            dirty.fields.extend(changes);
                            dirty.revision = revision;
                            dirty.error = format!("Not saved: {error}");
                            self.snapshot[&write.field] = json!(write.value);
                            self.error = Some(dirty.error.clone());
                            changed();
                            return Err(dirty.error.clone());
                        }
                    }
                }
                "sys.mail_review" => {
                    if write.op == "clear" {
                        cancel_for(&b.key());
                        return Ok(());
                    }
                    if write.op != "set" || write.value != b.draft_id {
                        return Err("Review must name the host-bound draft".into());
                    }
                    if let Some(error) = &self.error {
                        return Err(format!("Save or resolve the draft before review: {error}"));
                    }
                    let review = octosense_mail_service::drafts::prepare_review(
                        &dir,
                        &b.publisher,
                        &b.account,
                        &b.draft_id,
                        revision,
                    )?;
                    queue_review(b.key(), review);
                }
                _ => return Err("Unsupported Mail card write".into()),
            }
            makepad_widgets::makepad_platform::SignalToUI::set_ui_signal();
            Ok(())
        }
        #[cfg(not(any(feature = "app-hub", native_mobile)))]
        {
            let _ = (write, origin, from_field);
            Err("Mail is unavailable in this packaging".into())
        }
    }
}

/// Revokes unused authority when the answer finishes or its callback is dropped.
pub struct ChatEditLease(Option<String>);
impl ChatEditLease {
    pub fn new(binding: Option<&crate::glance_chat::ContextBinding>) -> Self {
        Self(binding.filter(|b| matches!(b.kind, octosense_l0_chat::ContextKind::Mail)).and_then(|b| b.draft["edit_token"].as_str()).map(str::to_owned))
    }
}
impl Drop for ChatEditLease {
    fn drop(&mut self) {
        #[cfg(any(feature = "app-hub", native_mobile))]
        if let Some(token) = &self.0 { octosense_mail_service::drafts::revoke_chat_edit(token); }
    }
}

pub fn seed_publication(binding: &Binding, source: &str, data: &Value) -> Result<Value, String> {
    check_sources(source, Some(binding))?;
    let session = Session::new(binding.clone());
    if let Some(error) = &session.error {
        return Err(error.clone());
    }
    crate::glance_chat::seed_bound(
        &binding.publisher,
        source,
        &session.seed(source, data),
        &Default::default(),
        &session.chat_binding()?,
    )
}

#[cfg(any(feature = "app-hub", native_mobile))]
type Review = octosense_mail_service::drafts::Review;

#[cfg(any(feature = "app-hub", native_mobile))]
fn reviews() -> &'static Mutex<Vec<(String, Review)>> {
    static REVIEWS: OnceLock<Mutex<Vec<(String, Review)>>> = OnceLock::new();
    REVIEWS.get_or_init(Default::default)
}
#[cfg(any(feature = "app-hub", native_mobile))]
pub(crate) fn queue_review(key: String, review: Review) {
    // A replacement may refer to the SAME deduplicated operation. Revoke its
    // old UI capability without cancelling the newly prepared operation.
    if let Some(previous) = take_review(&key) {
        octosense_mail_service::drafts::revoke_review(previous);
    }
    *review_generations()
        .lock()
        .unwrap()
        .entry(key.clone())
        .or_default() += 1;
    reviews().lock().unwrap().push((key, review));
    makepad_widgets::makepad_platform::SignalToUI::set_ui_signal();
}
#[cfg(any(feature = "app-hub", native_mobile))]
pub fn take_review(key: &str) -> Option<Review> {
    let mut queue = reviews().lock().unwrap();
    let at = queue.iter().position(|(k, _)| k == key)?;
    Some(queue.remove(at).1)
}
pub fn requested_card() -> Option<String> {
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        reviews().lock().unwrap().last().map(|(key, _)| key.clone())
    }
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    {
        None
    }
}
pub fn cancel_for(key: &str) {
    let mut generations = review_generations().lock().unwrap();
    *generations.entry(key.to_string()).or_default() += 1;
    drop(generations);
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        if let Some(review) = take_review(key) {
            let _ = octosense_mail_service::drafts::cancel_review(review);
        }
    }
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    {
        let _ = key;
    }
}

fn review_generations() -> &'static Mutex<HashMap<String, u64>> {
    static GENERATIONS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    GENERATIONS.get_or_init(Default::default)
}
pub fn review_generation(key: &str) -> u64 {
    review_generations()
        .lock()
        .unwrap()
        .get(key)
        .copied()
        .unwrap_or(0)
}

pub struct CardStatus {
    pub text: String,
    pub suggestion: bool,
    pub unsaved: bool,
}
pub fn card_status(binding: &Binding) -> CardStatus {
    match read(binding) {
        Err(error) => CardStatus {
            text: error,
            suggestion: false,
            unsaved: false,
        },
        Ok(value) => {
            let dirty = unsaved().lock().unwrap().get(&binding.draft_key()).cloned();
            let action_error = action_errors()
                .lock()
                .unwrap()
                .get(&binding.draft_key())
                .cloned();
            let suggestion = value["suggestions"]
                .as_array()
                .and_then(|v| v.last())
                .is_some_and(|s| s["revision"] == value["revision"]);
            CardStatus {
                text: dirty
                    .as_ref()
                    .map(|d| d.error.clone())
                    .or(action_error)
                    .unwrap_or_else(|| match value["status"].as_str().unwrap_or("draft") {
                        "draft" | "awaiting_approval" => {
                            format!("Draft saved · revision {}", value["revision"])
                        }
                        "accepted" => "SMTP accepted · delivery unconfirmed".into(),
                        "outcome_unknown" => "Outcome unknown · check Sent before retrying".into(),
                        other => other.replace('_', " "),
                    }),
                suggestion: suggestion && dirty.is_none(),
                unsaved: dirty.is_some(),
            }
        }
    }
}

pub fn discard_unsaved(binding: &Binding) -> Result<(), String> {
    read(binding)?;
    unsaved().lock().unwrap().remove(&binding.draft_key());
    action_errors().lock().unwrap().remove(&binding.draft_key());
    changed();
    Ok(())
}

pub fn suggestion_preview(binding: &Binding) -> Result<Value, String> {
    let value = read(binding)?;
    let suggestion = value["suggestions"]
        .as_array()
        .and_then(|v| v.last())
        .ok_or("No pending suggestion")?;
    if suggestion["revision"] != value["revision"] {
        return Err("The suggested draft is stale; ask Mail to revise the current draft".into());
    }
    Ok(suggestion.clone())
}

pub fn accept_suggestion(binding: &Binding, id: &str, revision: u64) -> Result<(), String> {
    if unsaved().lock().unwrap().contains_key(&binding.draft_key()) {
        return Err("Resolve unsaved edits before accepting an AI suggestion".into());
    }
    read(binding)?;
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        invalidate_draft_review(binding);
        octosense_mail_service::drafts::accept_suggestion(
            &host_dir()?,
            &binding.publisher,
            &binding.account,
            &binding.draft_id,
            id,
            revision,
        )?;
        cancel_for(&binding.key());
        action_result(binding, &Ok(()));
        Ok(())
    }
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    {
        let _ = (id, revision);
        Err("Mail is unavailable".into())
    }
}

pub fn request_review(binding: &Binding) -> Result<(), String> {
    if unsaved().lock().unwrap().contains_key(&binding.draft_key()) {
        return Err("Resolve unsaved edits before review".into());
    }
    let value = read(binding)?;
    let revision = value["revision"].as_u64().ok_or("Invalid draft revision")?;
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        let review = octosense_mail_service::drafts::prepare_review(
            &host_dir()?,
            &binding.publisher,
            &binding.account,
            &binding.draft_id,
            revision,
        )?;
        queue_review(binding.key(), review);
        action_result(binding, &Ok(()));
        Ok(())
    }
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    {
        let _ = revision;
        Err("Mail is unavailable".into())
    }
}

mod persistence;
pub(crate) use persistence::{
    publication_can_undo, publication_guard, publication_host_ready, remove_publication,
    restore_publications, save_publication, set_publication_dismissed,
};

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Binding {
        Binding {
            publisher: "os.mail".into(),
            account: "account-a".into(),
            source_message: json!({"folder":"INBOX","message":"m1"}),
            draft_id: "draft_1".into(),
            draft_revision: 1,
            chat_thread: "thread_1".into(),
            card_id: "card1".into(),
        }
    }
    #[test]
    fn editing_one_card_invalidates_all_reviews_of_its_draft_only() {
        let mut one = binding();
        one.draft_id = "invalidation_test".into();
        let mut alias = one.clone();
        alias.card_id = "other_card".into();
        let before = draft_review_generation(&one.publisher, &one.account, &one.draft_id);
        invalidate_draft_review(&one);
        assert_eq!(
            draft_review_generation(&alias.publisher, &alias.account, &alias.draft_id),
            before + 1
        );
        assert_eq!(
            draft_review_generation(&one.publisher, "different-account", &one.draft_id),
            0
        );
    }
    #[test]
    fn action_failure_survives_reread_and_alias_until_explicit_success() {
        let mut one = binding();
        one.draft_id = "error_test".into();
        let mut alias = one.clone();
        alias.card_id = "another_card".into();
        action_result(&one, &Err("Review is stale".into()));
        assert_eq!(
            action_errors()
                .lock()
                .unwrap()
                .get(&alias.draft_key())
                .map(String::as_str),
            Some("Review is stale")
        );
        // Merely projecting a successful durable read must not clear an action error.
        let _ = display_value(&json!({"status":"draft"}), None);
        assert!(action_errors()
            .lock()
            .unwrap()
            .contains_key(&one.draft_key()));
        action_result(&alias, &Ok(()));
        assert!(!action_errors()
            .lock()
            .unwrap()
            .contains_key(&one.draft_key()));
    }
    #[test]
    fn generated_data_cannot_supply_durable_values() {
        let source = "source reply sys.mail_draft(app: \"os.mail\", id: \"draft_1\", fields: [body])\nview root TextBody(text: reply.body)";
        let seeded = seed_values(
            source,
            &json!({"reply":{"body":"forged","status":"accepted"}}),
            &json!({"body":"saved draft","status":"draft","revision":7}),
            None,
        );
        assert_eq!(seeded["reply"]["body"], "saved draft");
        assert_eq!(seeded["reply"]["status"], "draft");
    }
    #[test]
    fn mail_source_requires_host_binding_and_exact_identity() {
        let source = "source reply sys.mail_draft(app: \"os.mail\", id: \"draft_1\", fields: [body])\nview root TextBody(text: reply.body)";
        assert!(check_sources(source, None).is_err());
        assert!(check_sources(source, Some(&binding())).is_ok());
        assert!(
            check_sources(&source.replace("draft_1", "draft_other"), Some(&binding())).is_err()
        );
        assert!(check_sources(&source.replace("os.mail", "other.app"), Some(&binding())).is_err());
    }
    #[test]
    fn inaccessible_bound_chat_redacts_generated_transcript() {
        let source = "source chat sys.chat(app: \"os.mail\", thread: \"mail-thread\", fields: [status, entries])\nview root TextBody(text: chat.status)";
        let seeded = unavailable_chat(
            source,
            &json!({"chat":{"status":"ready","entries":[{"role":"model","text":"Sent!"}]},"other":"kept"}),
        );
        assert_eq!(seeded["chat"], octosense_l0_chat::unavailable());
        assert_eq!(seeded["other"], "kept");
    }
    #[test]
    fn programmatic_call_is_not_human_authorization() {
        assert!(!trusted_user_gesture());
    }
}
