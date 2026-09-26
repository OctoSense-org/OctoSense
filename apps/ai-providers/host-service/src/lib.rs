//! The `llm` host service: the assistant's LLM providers, keys kept by the
//! host.
//!
//! A system app granted `llm` (the AI providers app, `os.ai-providers`)
//! calls, through `host.request`:
//!
//! | method | args | answer |
//! |---|---|---|
//! | `llm.providers` | – | `{primary, fallbacks, scanner, image_picker, store}`: each provider `{id, family, label, model, custom_model, base_url, api_type, key}`, `key` being `"set ••••1234"`, `"missing"`, `"not needed"` or `"keychain locked"`; `scanner` says the host can scan a QR; `image_picker` that it can read one from a chosen image; `store` where keys go (`keychain`, `secrets folder`, `profile`) |
//! | `llm.families` | – | `[{id, label, default_model, key_required, default_base_url}]`, octos's registry |
//! | `llm.add_provider` | – | `{id, label}` once the person saves it on the host's sheet |
//! | `llm.edit_provider` | `{id}` | `{id, label}` (the id changes with the route) once saved on the sheet |
//! | `llm.set_model` | `{id, model}` | `{id}`; an empty model means the family default |
//! | `llm.move` | `{id, to}` | `{}`; `to` is the new position, 0 = primary |
//! | `llm.set_primary` | `{id}` | `{}` |
//! | `llm.remove` | `{id}` | `{}`; its key leaves the profile when no provider reads it |
//! | `llm.test` | `{id}` | `{ok, ms, error?}` after one tiny request to the provider (`ok` is a Splash keyword: a script tests `error == nil`) |
//! | `llm.export_qr` | `{ids?}` | `{}` when the person closes the sheet that shows the phone QR (all providers, or `ids`) |
//! | `llm.import_qr` | – | `{applied: [label]}` once a scanned or pasted code is imported on the sheet |
//!
//! The app never sees a key, a PIN or a QR. `add_provider`, `edit_provider`,
//! `export_qr` and `import_qr` raise the host's sheet, a separate isolate
//! over the app; only calls from that sheet (`llm.sheet.submit`,
//! `llm.sheet.cancel`, `llm.sheet.qr`, `llm.sheet.scan`, `llm.sheet.import`)
//! can carry a key, a PIN or a code, and `dispatch` refuses them from
//! anyone else. The export sheet draws an `OCTOS1E:` code (octos's PIN-sealed
//! profile QR) and its PIN, and closes itself after five minutes. The import
//! sheet asks the shell's [`QrScanner`] for the camera where there is one,
//! and takes a pasted code everywhere; the code is opened (Argon2id, 64 MiB)
//! on a worker.
//!
//! Only `os.` apps are served: the provider set is the device's.
//!
//! State is the kernel's profile, `<core_dir>/profiles/_main.json`: its
//! `config.llm` (primary and fallbacks) and the key env vars in
//! `config.env_vars`. Keys go to the [`vault`]; every change calls the
//! shell's `on_changed` hook so it can restart the AppCard kernel.
use octosense_appstore::services::{close_sheet_later, HostService, Replier, ServiceCall, ServiceHost};
use octosense_llm_config::{profile, qr, registry, Provider};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub mod model;
pub mod probe;
pub mod sheets;
pub mod vault;

use model::ProfileStore;
use vault::Vault;

/// How long the phone QR stays up.
pub const QR_LIFETIME_SECS: u64 = 300;

/// Called with a scan's result, once, from any thread: the decoded text, or
/// why there is none (`"cancelled"`, `"permission_denied"`, `"unsupported"`…).
pub type ScanDone = Box<dyn FnOnce(Result<String, String>) + Send>;

/// The device's QR scanner, provided by the shell (on Android, Makepad's
/// `cx.show_qr_scanner()` and its `NativeQrScanned` / `NativeQrCancelled`
/// actions). The service never links Makepad itself.
pub trait QrScanner: Send + Sync {
    /// Open the scanner over everything and call `done` exactly once.
    fn scan(&self, done: ScanDone);
}

/// Why a pick produced no image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickError {
    /// The person closed the picker without choosing.
    Cancelled,
    /// The picker or the file failed; the text is shown on the sheet.
    Failed(String),
}

/// Called with a pick's result, once, from any thread: the chosen file's
/// encoded bytes as stored (PNG or JPEG; the service decodes them), or why
/// there are none.
pub type ImageDone = Box<dyn FnOnce(Result<Vec<u8>, PickError>) + Send>;

/// The shell's image picker (a file dialog on the desktop, the photo picker
/// on a phone): the import sheet reads a provider QR out of the image, e.g.
/// a screenshot. The service never links Makepad itself.
pub trait QrImagePicker: Send + Sync {
    /// Let the person choose one image and call `done` exactly once.
    fn pick(&self, done: ImageDone);
}

/// Called after the provider set changed on disk (a save, a removal, an
/// import): the shell restarts the AppCard kernel so it reads the new
/// profile. Runs on whichever thread made the change.
pub type OnChanged = Arc<dyn Fn() + Send + Sync>;

/// What a shell hands the service. `Options::default()` is what
/// [`register`] uses.
#[derive(Clone, Default)]
pub struct Options {
    /// The kernel's octos home, holding `profiles/_main.json`. `None`:
    /// `octosense_llm_config::profile::default_core_dir()`.
    pub core_dir: Option<PathBuf>,
    /// Where keys go. `None`: [`vault::platform`] for the core dir.
    pub vault: Option<Arc<dyn Vault>>,
    /// The camera scanner, where the device has one: the import sheet then
    /// scans, and the app offers "Scan QR from desktop".
    pub scanner: Option<Arc<dyn QrScanner>>,
    /// The image picker, where the shell has one: the import sheet then
    /// offers "Choose image" and reads the QR out of the chosen picture.
    pub image_picker: Option<Arc<dyn QrImagePicker>>,
    pub on_changed: Option<OnChanged>,
}

impl Options {
    pub fn core_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.core_dir = Some(dir.into());
        self
    }
    pub fn vault(mut self, vault: Arc<dyn Vault>) -> Self {
        self.vault = Some(vault);
        self
    }
    pub fn scanner(mut self, scanner: Arc<dyn QrScanner>) -> Self {
        self.scanner = Some(scanner);
        self
    }
    pub fn image_picker(mut self, picker: Arc<dyn QrImagePicker>) -> Self {
        self.image_picker = Some(picker);
        self
    }
    pub fn on_changed(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_changed = Some(Arc::new(f));
        self
    }
}

/// Offer the service to the Card runner with the default core dir and the
/// platform's vault, no scanner and no change hook.
pub fn register() {
    register_with(Options::default());
}

/// Offer the service as `options` say. A second call replaces the first.
pub fn register_with(options: Options) {
    let core_dir = options
        .core_dir
        .clone()
        .or_else(profile::default_core_dir)
        .unwrap_or_else(|| std::env::temp_dir().join("octos-home/.octos"));
    let vault = options.vault.clone().unwrap_or_else(|| vault::platform(&core_dir));
    octosense_appstore::services::register_host_service(Box::new(LlmService {
        shared: Arc::new(Shared { path: profile::profile_path(&core_dir), vault, on_changed: options.on_changed, lock: Mutex::new(()) }),
        scanner: options.scanner,
        image_picker: options.image_picker,
        pending: Arc::default(),
        export: Arc::default(),
        generation: 0,
    }));
}

/// What workers need: the profile, its vault, and one lock so two changes
/// never interleave their read-modify-write.
struct Shared {
    path: PathBuf,
    vault: Arc<dyn Vault>,
    on_changed: Option<OnChanged>,
    lock: Mutex<()>,
}

impl Shared {
    fn open(&self) -> Result<ProfileStore, String> {
        ProfileStore::open(self.path.clone(), self.vault.clone())
    }

    /// Change the list under the lock and save it; `f` returns the keys to
    /// store with it and the answer.
    fn change(
        &self,
        f: impl FnOnce(&ProfileStore, &mut Vec<Provider>) -> Result<(BTreeMap<String, String>, Value), String>,
    ) -> Result<Value, String> {
        let _guard = self.lock.lock().unwrap();
        let mut store = self.open()?;
        let mut list = store.list.clone();
        let (keys, answer) = f(&store, &mut list)?;
        store.save(list, &keys)?;
        if let Some(changed) = &self.on_changed {
            changed();
        }
        Ok(answer)
    }
}

/// The app waiting on a sheet, and what the sheet is for.
struct Pending {
    app_id: String,
    reply: Replier,
    kind: Kind,
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Add,
    Edit(String),
    Export(u64),
    Import { scanned: Option<String> },
}

/// The phone QR being sealed, by the export it belongs to: `None` while the
/// worker runs, then the finished sheet (or why not).
type ExportJob = (u64, Option<Result<String, String>>);

pub struct LlmService {
    shared: Arc<Shared>,
    scanner: Option<Arc<dyn QrScanner>>,
    image_picker: Option<Arc<dyn QrImagePicker>>,
    /// Shared with the workers: they answer the app on success.
    pending: Arc<Mutex<Option<Pending>>>,
    export: Arc<Mutex<Option<ExportJob>>>,
    generation: u64,
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

fn label_of(p: &Provider) -> String {
    match model::effective_model(p) {
        Some(m) => format!("{} · {m}", model::family_label(&p.family)),
        None => model::family_label(&p.family),
    }
}

/// What an app is shown of a provider: never its key.
fn entry(store: &ProfileStore, p: &Provider) -> Value {
    json!({
        "id": model::id_of(p),
        "family": registry::lookup(&p.family).map(|f| f.id).unwrap_or(p.family.as_str()),
        "label": model::family_label(&p.family),
        "model": model::effective_model(p).unwrap_or_default(),
        "custom_model": p.model.is_some(),
        "base_url": p.base_url,
        "api_type": p.api_type.map(|t| t.as_str()),
        "key": store.status(p).text(),
    })
}

fn families() -> Value {
    json!(registry::all()
        .iter()
        .map(|f| json!({
            "id": f.id, "label": f.label, "default_model": f.default_model,
            "key_required": f.key_required, "default_base_url": f.default_base_url,
        }))
        .collect::<Vec<_>>())
}

/// A worker thread for slow work (the keychain, the network, Argon2id), so
/// the UI thread never waits.
fn work(f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(f);
}

fn scan_error(reason: &str) -> String {
    match reason {
        "cancelled" | "interrupted" => "Scan cancelled.".into(),
        "permission_denied" => "OctoSense may not use the camera. Allow it in Settings, or paste the code.".into(),
        "camera_error" => "The camera is unavailable. Paste the code instead.".into(),
        "unsupported" => "This device cannot scan. Paste the code instead.".into(),
        other => format!("Scan failed ({other})."),
    }
}

impl LlmService {
    /// Raise `sheet` for `app_id`, replacing any sheet already waiting.
    fn raise(&mut self, app_id: &str, reply: Replier, kind: Kind, sheet: String, host: &mut dyn ServiceHost) {
        if let Some(earlier) = self.pending.lock().unwrap().take() {
            earlier.reply.send(Err("Another sheet replaced this one.".into()));
        }
        *self.export.lock().unwrap() = None;
        *self.pending.lock().unwrap() = Some(Pending { app_id: app_id.to_string(), reply, kind });
        host.open_sheet(sheet);
    }

    fn pending_kind(&self) -> Option<Kind> {
        self.pending.lock().unwrap().as_ref().map(|p| p.kind.clone())
    }

    /// The saved route an app named by `id`.
    fn provider(&self, id: &str) -> Result<Provider, String> {
        let store = self.shared.open()?;
        Ok(store.list[model::index_of(&store.list, id)?].clone())
    }

    fn submit(&mut self, args: &Value, reply: Replier) {
        let edit = match self.pending_kind() {
            Some(Kind::Add) => None,
            Some(Kind::Edit(id)) => Some(id),
            _ => return reply.send(Err("No app is waiting for this sheet.".into())),
        };
        let mut route = match model::provider_from(text(args, "family"), text(args, "model"), text(args, "base_url"), text(args, "api_type")) {
            Ok(p) => p,
            Err(e) => return reply.send(Err(e)),
        };
        // A key is taken as typed but for surrounding space.
        let key = text(args, "key").trim().to_string();
        if key.chars().any(char::is_control) || key.len() > 16 * 1024 {
            return reply.send(Err("That key is not acceptable text.".into()));
        }
        let (shared, pending) = (self.shared.clone(), self.pending.clone());
        work(move || {
            let saved = shared.change(|store, list| {
                match &edit {
                    Some(id) => {
                        let at = model::index_of(list, id)?;
                        // The same family keeps the env var its route reads.
                        if registry::lookup(&list[at].family).map(|f| f.id) == Some(route.family.as_str()) {
                            route.key_env = list[at].key_env.clone();
                        }
                        if list.iter().enumerate().any(|(i, p)| i != at && model::id_of(p) == model::id_of(&route)) {
                            return Err("That provider is already in the list.".into());
                        }
                        list[at] = route.clone();
                    }
                    None => {
                        if list.iter().any(|p| model::id_of(p) == model::id_of(&route)) {
                            return Err("That provider is already in the list.".into());
                        }
                        list.push(route.clone());
                    }
                }
                if key.is_empty() && model::key_required(&route.family) && store.key(&route).is_none() {
                    return Err("Type the provider's API key.".into());
                }
                let keys = if key.is_empty() { BTreeMap::new() } else { [(route.key_env.clone(), key)].into() };
                Ok((keys, json!({"id": model::id_of(&route), "label": label_of(&route)})))
            });
            match saved {
                Err(e) => reply.send(Err(e)),
                Ok(answer) => {
                    if let Some(waiting) = pending.lock().unwrap().take() {
                        close_sheet_later(&waiting.app_id);
                        waiting.reply.send(Ok(answer));
                    }
                    reply.send(Ok(json!({})));
                }
            }
        });
    }

    fn export(&mut self, app_id: &str, args: &Value, reply: Replier, host: &mut dyn ServiceHost) {
        let store = match self.shared.open() {
            Ok(s) => s,
            Err(e) => return reply.send(Err(e)),
        };
        let chosen: Vec<Provider> = match args["ids"].as_array() {
            Some(ids) => {
                let ids: Vec<&str> = ids.iter().filter_map(Value::as_str).collect();
                store.list.iter().filter(|p| ids.contains(&model::id_of(p).as_str())).cloned().collect()
            }
            None => store.list.clone(),
        };
        if chosen.is_empty() {
            return reply.send(Err("There is no provider to show.".into()));
        }
        self.generation += 1;
        let generation = self.generation;
        self.raise(app_id, reply, Kind::Export(generation), sheets::export_waiting(), host);
        *self.export.lock().unwrap() = Some((generation, None));
        let (shared, export, pending) = (self.shared.clone(), self.export.clone(), self.pending.clone());
        work(move || {
            let sheet = (|| {
                let store = shared.open()?;
                let provisioning = store.provisioning(&chosen);
                let pin = qr::generate_pin();
                let code = qr::encode_encrypted(&provisioning, &pin).map_err(|e| e.to_string())?;
                let (size, modules) = qr::render_matrix(&code).map_err(|e| e.to_string())?;
                let labels: Vec<String> = chosen.iter().map(label_of).collect();
                Ok(sheets::export(size, &modules, &pin, &labels, QR_LIFETIME_SECS))
            })();
            if let Some(job) = export.lock().unwrap().as_mut().filter(|job| job.0 == generation) {
                job.1 = Some(sheet);
            }
            // The sheet closes itself; this closes it for a host whose
            // sheet stopped ticking.
            std::thread::sleep(std::time::Duration::from_secs(QR_LIFETIME_SECS + 2));
            let mut pending = pending.lock().unwrap();
            if pending.as_ref().is_some_and(|p| p.kind == Kind::Export(generation)) {
                let waiting = pending.take().unwrap();
                close_sheet_later(&waiting.app_id);
                waiting.reply.send(Ok(json!({"expired": true})));
            }
        });
    }

    /// The waiting sheet asks for the finished one.
    fn export_ready(&mut self, reply: Replier, host: &mut dyn ServiceHost) {
        let Some(Kind::Export(generation)) = self.pending_kind() else {
            return reply.send(Err("No code is being prepared.".into()));
        };
        let mut export = self.export.lock().unwrap();
        match export.as_mut().filter(|job| job.0 == generation) {
            None => reply.send(Err("No code is being prepared.".into())),
            Some((_, None)) => reply.send(Ok(json!({"ready": false}))),
            Some((_, Some(result))) => match result.clone() {
                Ok(sheet) => {
                    // Shown once; the PIN is not kept.
                    *export = None;
                    host.open_sheet(sheet);
                    reply.send(Ok(json!({"ready": true})));
                }
                Err(e) => reply.send(Err(e)),
            },
        }
    }

    fn scan(&mut self, reply: Replier) {
        if !matches!(self.pending_kind(), Some(Kind::Import { .. })) {
            return reply.send(Err("No app is waiting for this sheet.".into()));
        }
        let Some(scanner) = self.scanner.clone() else {
            return reply.send(Err("This device has no camera scanner. Paste the code instead.".into()));
        };
        let pending = self.pending.clone();
        scanner.scan(Box::new(move |result| match result {
            Err(reason) => reply.send(Err(scan_error(&reason))),
            Ok(code) => {
                let Some(format) = qr::format_of(&code) else {
                    return reply.send(Err("That is not an OctoSense provider code.".into()));
                };
                if let Some(Pending { kind: Kind::Import { scanned }, .. }) = pending.lock().unwrap().as_mut() {
                    *scanned = Some(code.trim().to_string());
                }
                reply.send(Ok(json!({"needs_pin": format == qr::Format::Encrypted})));
            }
        }));
    }

    fn import(&mut self, args: &Value, reply: Replier) {
        let Some(Kind::Import { scanned }) = self.pending_kind() else {
            return reply.send(Err("No app is waiting for this sheet.".into()));
        };
        let pasted = text(args, "text").trim().to_string();
        let Some(code) = Some(pasted).filter(|c| !c.is_empty()).or(scanned) else {
            return reply.send(Err("Scan or paste a code first.".into()));
        };
        let pin = text(args, "pin").trim().to_string();
        let (shared, pending) = (self.shared.clone(), self.pending.clone());
        work(move || {
            let legacy = qr::format_of(&code) == Some(qr::Format::LegacyJson);
            let provisioning = match qr::decode(&code, Some(pin.as_str()).filter(|p| !p.is_empty())) {
                Ok(p) => p,
                Err(e) => return reply.send(Err(e.to_string())),
            };
            let applied = shared.change(|_, list| {
                let incoming = model::list_of(&provisioning.set);
                if legacy {
                    // The old single-provider code replaces the primary only.
                    let primary = incoming.into_iter().next().ok_or("The code names no provider.")?;
                    let rest: Vec<Provider> = list.iter().skip(1).filter(|p| model::id_of(p) != model::id_of(&primary)).cloned().collect();
                    *list = std::iter::once(primary).chain(rest).collect();
                } else {
                    *list = incoming;
                }
                let labels: Vec<String> = model::list_of(&provisioning.set).iter().map(label_of).collect();
                Ok((provisioning.secrets.clone(), json!({"applied": labels})))
            });
            match applied {
                Err(e) => reply.send(Err(e)),
                Ok(answer) => {
                    if let Some(waiting) = pending.lock().unwrap().take() {
                        close_sheet_later(&waiting.app_id);
                        waiting.reply.send(Ok(answer.clone()));
                    }
                    reply.send(Ok(answer));
                }
            }
        });
    }
}

impl HostService for LlmService {
    fn family(&self) -> &'static str {
        "llm"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
        if !call.app_id.starts_with("os.") {
            return reply.send(Err("llm is for OctoSense's own apps.".into()));
        }
        let id = text(&call.args, "id").to_string();
        match call.method() {
            "providers" => {
                let (shared, scanner, image_picker) = (self.shared.clone(), self.scanner.is_some(), self.image_picker.is_some());
                work(move || {
                    let answer = shared.open().map(|store| {
                        let entries: Vec<Value> = store.list.iter().map(|p| entry(&store, p)).collect();
                        json!({
                            "primary": entries.first(), "fallbacks": entries.iter().skip(1).collect::<Vec<_>>(),
                            "scanner": scanner, "image_picker": image_picker, "store": shared.vault.kind(),
                        })
                    });
                    reply.send(answer);
                });
            }
            "families" => reply.send(Ok(families())),
            "add_provider" => self.raise(&call.app_id, reply, Kind::Add, sheets::edit(None), host),
            "edit_provider" => match self.provider(&id) {
                Ok(p) => self.raise(&call.app_id, reply, Kind::Edit(id), sheets::edit(Some(&p)), host),
                Err(e) => reply.send(Err(e)),
            },
            "set_model" | "move" | "set_primary" | "remove" => {
                let (shared, method, args) = (self.shared.clone(), call.method().to_string(), call.args.clone());
                work(move || {
                    reply.send(shared.change(|_, list| {
                        let at = model::index_of(list, &id)?;
                        let mut answer = json!({});
                        match method.as_str() {
                            "set_model" => {
                                let wanted = text(&args, "model").trim();
                                if wanted.len() > 512 || wanted.chars().any(char::is_control) {
                                    return Err("That model name is not acceptable text.".into());
                                }
                                list[at].model = (!wanted.is_empty()).then(|| wanted.to_string());
                                if model::effective_model(&list[at]).is_none() {
                                    return Err("This provider needs a model.".into());
                                }
                                answer = json!({"id": model::id_of(&list[at])});
                            }
                            "remove" => {
                                list.remove(at);
                            }
                            _ => {
                                let to = if method == "set_primary" { 0 } else { args["to"].as_f64().unwrap_or(0.0).max(0.0) as usize };
                                let p = list.remove(at);
                                list.insert(to.min(list.len()), p);
                            }
                        }
                        Ok((BTreeMap::new(), answer))
                    }));
                });
            }
            "test" => {
                let shared = self.shared.clone();
                work(move || {
                    let answer = shared.open().and_then(|store| {
                        let p = store.list[model::index_of(&store.list, &id)?].clone();
                        let key = store.key(&p);
                        Ok(probe::run(&p, key.as_deref()))
                    });
                    reply.send(answer);
                });
            }
            "export_qr" => self.export(&call.app_id, &call.args, reply, host),
            "import_qr" => self.raise(&call.app_id, reply, Kind::Import { scanned: None }, sheets::import(self.scanner.is_some()), host),
            "sheet.cancel" => {
                host.close_sheet();
                *self.export.lock().unwrap() = None;
                if let Some(waiting) = self.pending.lock().unwrap().take() {
                    // Closing the QR is how an export ends.
                    let answer = if matches!(waiting.kind, Kind::Export(_)) { Ok(json!({})) } else { Err("Cancelled.".into()) };
                    waiting.reply.send(answer);
                }
                reply.send(Ok(json!({})));
            }
            "sheet.submit" => self.submit(&call.args, reply),
            "sheet.qr" => self.export_ready(reply, host),
            "sheet.scan" => self.scan(reply),
            "sheet.import" => self.import(&call.args, reply),
            other => reply.send(Err(format!("llm has no method {other:?}"))),
        }
    }
}
