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
//! | `llm.import_qr` | – | `{applied, added, updated, message}` once a scanned, picked, dropped or pasted code is imported on the sheet: `applied` labels every provider the code names, `added` those appended (or saved, into an empty list), `updated` the saved ones whose key it changed; `message` says so in a sentence |
//!
//! The app never sees a key, a PIN or a QR. `add_provider`, `edit_provider`,
//! `export_qr` and `import_qr` raise the host's sheet, a separate isolate
//! over the app; only calls from that sheet (`llm.sheet.submit`,
//! `llm.sheet.cancel`, `llm.sheet.qr`, `llm.sheet.show`, `llm.sheet.scan`,
//! `llm.sheet.pick`, `llm.sheet.image`, `llm.sheet.import`) can carry a key, a PIN or a code, and `dispatch` refuses them from
//! anyone else. The export sheet draws an `OCTOS1E:` code (octos's PIN-sealed
//! profile QR) and its PIN, and closes itself after five minutes (the
//! service closes it too, should the sheet stop counting); nothing keeps the
//! code or the PIN once it is closed. An import only adds: into an empty
//! list the code's providers are saved as they are (the first is the
//! primary); otherwise the saved providers keep their order and primary, one
//! the code names again takes the code's key, and the others are appended as
//! fallbacks, each in a key slot of its own when its key would overwrite a
//! saved provider's ([`model::merge_import`]). The import sheet asks the shell's [`QrScanner`] for the camera where there is one,
//! reads the code out of an image from the shell's [`QrImagePicker`] or
//! dropped on the app ([`offer_image`]) where the shell offers those, and
//! takes a pasted code everywhere; the image is searched ([`image_qr`]) and
//! the code opened (Argon2id, 64 MiB) on a worker.
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

pub mod image_qr;
pub mod model;
pub mod probe;
pub mod sheets;
pub mod vault;

use model::ProfileStore;
use vault::Vault;

/// How long the phone QR stays up.
pub const QR_LIFETIME_SECS: u64 = 300;

/// A shorter phone-QR lifetime for end-to-end tests, in seconds
/// (`OCTOSENSE_LLM_QR_SECONDS`, used when [`Options::qr_lifetime_secs`] is
/// not set). It can only shorten the lifetime: anything over
/// [`QR_LIFETIME_SECS`] is cut to it.
pub const QR_LIFETIME_ENV: &str = "OCTOSENSE_LLM_QR_SECONDS";

/// The phone QR's lifetime: `wanted` (or the test variable), 3 s to
/// [`QR_LIFETIME_SECS`].
fn qr_lifetime(wanted: Option<u64>) -> u64 {
    wanted
        .or_else(|| std::env::var(QR_LIFETIME_ENV).ok().and_then(|v| v.trim().parse().ok()))
        .unwrap_or(QR_LIFETIME_SECS)
        .clamp(3, QR_LIFETIME_SECS)
}

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
    /// The shell passes image files dropped on the app to [`offer_image`]
    /// (a desktop): the import sheet then says a screenshot can be dropped
    /// on it.
    pub image_drops: bool,
    pub on_changed: Option<OnChanged>,
    /// How long the phone QR stays up, in seconds (at most
    /// [`QR_LIFETIME_SECS`], which is the default). `None`: the
    /// [`QR_LIFETIME_ENV`] variable, else the default.
    pub qr_lifetime_secs: Option<u64>,
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
    /// The shell delivers dropped images through [`offer_image`].
    pub fn image_drops(mut self, yes: bool) -> Self {
        self.image_drops = yes;
        self
    }
    pub fn on_changed(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_changed = Some(Arc::new(f));
        self
    }
    /// Shorten how long the phone QR stays up (tests).
    pub fn qr_lifetime_secs(mut self, secs: u64) -> Self {
        self.qr_lifetime_secs = Some(secs);
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
    *IMAGE_WAITER.lock().unwrap() = None;
    octosense_appstore::services::register_host_service(Box::new(LlmService {
        shared: Arc::new(Shared { path: profile::profile_path(&core_dir), vault, on_changed: options.on_changed, lock: Mutex::new(()) }),
        scanner: options.scanner,
        image_picker: options.image_picker,
        image_drops: options.image_drops,
        pending: Arc::default(),
        export: Arc::default(),
        generation: 0,
        qr_lifetime: qr_lifetime(options.qr_lifetime_secs),
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
        // Nothing changed (the same code imported again): no write, and no
        // kernel restart.
        if list == store.list && keys.is_empty() {
            return Ok(answer);
        }
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

/// The import sheet waiting for a dropped image (`llm.sheet.image`): its
/// answer, and the service's pending sheet to put the code on. Global so a
/// shell's drop handler reaches it through [`offer_image`] without a handle
/// on the registered service.
type ImageWaiter = (Replier, Arc<Mutex<Option<Pending>>>);
static IMAGE_WAITER: Mutex<Option<ImageWaiter>> = Mutex::new(None);

/// Whether an import sheet is up and would take a dropped image now: a
/// shell answers a drag over the app with "copy" only then.
pub fn wants_image() -> bool {
    IMAGE_WAITER
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|(_, pending)| matches!(pending.lock().unwrap().as_ref(), Some(Pending { kind: Kind::Import { .. }, .. })))
}

/// A dropped image's encoded bytes (PNG or JPEG) for the import sheet that is
/// up, from any thread: the code is read out of it on a worker and the sheet
/// asks for the PIN, as after a pick. `false` when no import sheet waits for
/// one (the drop is not the service's).
pub fn offer_image(bytes: Vec<u8>) -> bool {
    if !wants_image() {
        return false;
    }
    let Some((reply, pending)) = IMAGE_WAITER.lock().unwrap().take() else {
        return false;
    };
    work(move || reply.send(Ok(read_image(&bytes, &pending))));
    true
}

/// What the import sheet reads after an image: every field present (a Splash
/// script may not read a missing one), `error` null unless it failed.
fn image_answer(needs_pin: bool, cancelled: bool, error: Option<String>) -> Value {
    json!({"needs_pin": needs_pin, "cancelled": cancelled, "error": error})
}

/// Find the code in `bytes` and keep it on the import sheet: the answer the
/// sheet reads.
fn read_image(bytes: &[u8], pending: &Mutex<Option<Pending>>) -> Value {
    match image_qr::find_code(bytes) {
        Err(e) => image_answer(false, false, Some(e)),
        Ok(code) => {
            let needs_pin = qr::format_of(&code) == Some(qr::Format::Encrypted);
            match pending.lock().unwrap().as_mut() {
                Some(Pending { kind: Kind::Import { scanned }, .. }) => *scanned = Some(code),
                _ => return image_answer(false, false, Some("No app is waiting for this sheet.".into())),
            }
            image_answer(needs_pin, false, None)
        }
    }
}

/// The phone QR being sealed, by the export it belongs to: `None` while the
/// worker runs, then the finished sheet (or why not).
type ExportJob = (u64, Option<Result<String, String>>);

pub struct LlmService {
    shared: Arc<Shared>,
    scanner: Option<Arc<dyn QrScanner>>,
    image_picker: Option<Arc<dyn QrImagePicker>>,
    image_drops: bool,
    /// Shared with the workers: they answer the app on success.
    pending: Arc<Mutex<Option<Pending>>>,
    export: Arc<Mutex<Option<ExportJob>>>,
    generation: u64,
    qr_lifetime: u64,
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
        IMAGE_WAITER.lock().unwrap().take();
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
        let (shared, export, pending, lifetime) = (self.shared.clone(), self.export.clone(), self.pending.clone(), self.qr_lifetime);
        work(move || {
            let sheet = (|| {
                let store = shared.open()?;
                let provisioning = store.provisioning(&chosen);
                let pin = qr::generate_pin();
                let code = qr::encode_encrypted(&provisioning, &pin).map_err(|e| e.to_string())?;
                let (size, modules) = qr::render_matrix(&code).map_err(|e| e.to_string())?;
                let labels: Vec<String> = chosen.iter().map(label_of).collect();
                Ok(sheets::export(size, &modules, &pin, &labels, lifetime))
            })();
            if let Some(job) = export.lock().unwrap().as_mut().filter(|job| job.0 == generation) {
                job.1 = Some(sheet);
            }
            // The sheet closes itself at 0; this closes it, a little later,
            // for a host whose sheet stopped counting.
            std::thread::sleep(std::time::Duration::from_secs(lifetime + 5));
            let mut pending = pending.lock().unwrap();
            if pending.as_ref().is_some_and(|p| p.kind == Kind::Export(generation)) {
                let waiting = pending.take().unwrap();
                // A sheet never shown must not be shown later.
                if export.lock().unwrap().as_ref().is_some_and(|job| job.0 == generation) {
                    *export.lock().unwrap() = None;
                }
                close_sheet_later(&waiting.app_id);
                waiting.reply.send(Ok(json!({"expired": true})));
            }
        });
    }

    /// The waiting sheet asks whether the finished one is ready
    /// (`{ready}`); it then asks for it with `llm.sheet.show`.
    fn export_ready(&mut self, reply: Replier) {
        let Some(Kind::Export(generation)) = self.pending_kind() else {
            return reply.send(Err("No code is being prepared.".into()));
        };
        let export = self.export.lock().unwrap();
        match export.as_ref().filter(|job| job.0 == generation) {
            None => reply.send(Err("No code is being prepared.".into())),
            Some((_, None)) => reply.send(Ok(json!({"ready": false}))),
            Some((_, Some(Ok(_)))) => reply.send(Ok(json!({"ready": true}))),
            Some((_, Some(Err(e)))) => reply.send(Err(e.clone())),
        }
    }

    /// Swap the finished sheet in for the waiting one. Shown once: the code
    /// and the PIN are not kept here after.
    fn export_show(&mut self, reply: Replier, host: &mut dyn ServiceHost) {
        let Some(Kind::Export(generation)) = self.pending_kind() else {
            return reply.send(Err("No code is being prepared.".into()));
        };
        let mut export = self.export.lock().unwrap();
        if !matches!(export.as_ref(), Some((g, Some(Ok(_)))) if *g == generation) {
            return reply.send(Err("The code is not ready.".into()));
        }
        let Some((_, Some(Ok(sheet)))) = export.take() else { unreachable!() };
        host.open_sheet(sheet);
        reply.send(Ok(json!({})));
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

    /// "Choose image": the shell's picker, then the code read out of the
    /// picture on a worker. Answers `{needs_pin, cancelled, error}` (after an
    /// error the sheet stays up for another try).
    fn pick(&mut self, reply: Replier) {
        if !matches!(self.pending_kind(), Some(Kind::Import { .. })) {
            return reply.send(Err("No app is waiting for this sheet.".into()));
        }
        let Some(picker) = self.image_picker.clone() else {
            return reply.send(Err("This device cannot choose an image. Paste the code instead.".into()));
        };
        let pending = self.pending.clone();
        picker.pick(Box::new(move |result| match result {
            Err(PickError::Cancelled) => reply.send(Ok(image_answer(false, true, None))),
            Err(PickError::Failed(why)) => reply.send(Ok(image_answer(false, false, Some(format!("Could not open the image ({why})."))))),
            Ok(bytes) => work(move || reply.send(Ok(read_image(&bytes, &pending)))),
        }));
    }

    /// The sheet waits for a dropped image; one waiter at a time.
    fn await_image(&mut self, reply: Replier) {
        if !self.image_drops || !matches!(self.pending_kind(), Some(Kind::Import { .. })) {
            return reply.send(Err("No image can be dropped here.".into()));
        }
        if let Some((earlier, _)) = IMAGE_WAITER.lock().unwrap().replace((reply, self.pending.clone())) {
            earlier.send(Err("replaced".into()));
        }
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
            let provisioning = match qr::decode(&code, Some(pin.as_str()).filter(|p| !p.is_empty())) {
                Ok(p) => p,
                Err(e) => return reply.send(Err(e.to_string())),
            };
            apply_import(&shared, &pending, &provisioning, reply);
        });
    }
}

/// "Added A, B as fallbacks. Updated the key for C." — what an import did.
fn import_message(m: &model::Merge) -> String {
    let names = |list: &[Provider]| list.iter().map(label_of).collect::<Vec<_>>().join(", ");
    let mut parts = Vec::new();
    if let Some(primary) = &m.primary {
        parts.push(format!("Saved {} as primary", label_of(primary)));
    }
    match m.added.len() {
        0 => {}
        1 => parts.push(format!("Added {} as a fallback", names(&m.added))),
        _ => parts.push(format!("Added {} as fallbacks", names(&m.added))),
    }
    if !m.updated.is_empty() {
        parts.push(format!("Updated the key for {}", names(&m.updated)));
    }
    if parts.is_empty() {
        parts.push(format!("Already saved: {}", names(&m.unchanged)));
    }
    parts.join(". ") + "."
}

/// Merge an opened code into the saved providers ([`model::merge_import`]),
/// close the sheet and answer the app and the sheet.
fn apply_import(shared: &Shared, pending: &Mutex<Option<Pending>>, provisioning: &qr::Provisioning, reply: Replier) {
    let applied = shared.change(|store, list| {
        let merged = model::merge_import(store, provisioning)?;
        let labels = |l: &[Provider]| l.iter().map(label_of).collect::<Vec<_>>();
        let answer = json!({
            "applied": labels(&model::list_of(&provisioning.set)),
            "added": labels(&merged.primary.iter().chain(&merged.added).cloned().collect::<Vec<_>>()),
            "updated": labels(&merged.updated),
            "message": import_message(&merged),
        });
        *list = merged.list;
        Ok((merged.keys, answer))
    });
    match applied {
        Err(e) => reply.send(Err(e)),
        Ok(answer) => {
            if let Some(waiting) = pending.lock().unwrap().take() {
                close_sheet_later(&waiting.app_id);
                waiting.reply.send(Ok(answer.clone()));
            }
            IMAGE_WAITER.lock().unwrap().take();
            reply.send(Ok(json!({"message": answer["message"], "applied": answer["applied"]})));
        }
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
            "import_qr" => {
                let sheet = sheets::import(self.scanner.is_some(), self.image_picker.is_some(), self.image_drops);
                self.raise(&call.app_id, reply, Kind::Import { scanned: None }, sheet, host)
            }
            "sheet.cancel" => {
                host.close_sheet();
                *self.export.lock().unwrap() = None;
                IMAGE_WAITER.lock().unwrap().take();
                if let Some(waiting) = self.pending.lock().unwrap().take() {
                    // Closing the QR is how an export ends.
                    let answer = if matches!(waiting.kind, Kind::Export(_)) { Ok(json!({})) } else { Err("Cancelled.".into()) };
                    waiting.reply.send(answer);
                }
                reply.send(Ok(json!({})));
            }
            "sheet.submit" => self.submit(&call.args, reply),
            "sheet.qr" => self.export_ready(reply),
            "sheet.show" => self.export_show(reply, host),
            "sheet.scan" => self.scan(reply),
            "sheet.import" => self.import(&call.args, reply),
            "sheet.pick" => self.pick(reply),
            "sheet.image" => self.await_image(reply),
            other => reply.send(Err(format!("llm has no method {other:?}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_qr_lifetime_can_only_be_shortened() {
        assert_eq!(qr_lifetime(Some(10)), 10);
        assert_eq!(qr_lifetime(Some(0)), 3);
        assert_eq!(qr_lifetime(Some(86_400)), QR_LIFETIME_SECS);
    }
}
