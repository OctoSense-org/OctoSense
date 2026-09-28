//! The `llm` host service: the assistant's LLM providers, keys kept by the
//! host.
//!
//! A system app granted `llm` (the AI providers app, `os.ai-providers`)
//! calls, through `host.request`:
//!
//! | method | args | answer |
//! |---|---|---|
//! | `llm.providers` | – | `{primary, fallbacks, scanner, image_picker, store}`: each provider `{id, family, label, model, model_label, custom_model, route, route_label, context, price, tier, base_url, api_type, key}` (`label` is the family's name; `route` the catalog route id, `official` or `custom`; `context` e.g. `"1M context"` and `price` e.g. `"$0.14 / $0.28 per 1M"`, empty for a model the catalog does not list), `key` being `"set ••••1234"`, `"missing"`, `"not needed"` or `"keychain locked"`; `scanner` says the host can scan a QR; `image_picker` that it can read one from a chosen image; `store` where keys go (`keychain`, `secrets folder`, `profile`) |
//! | `llm.families` | `{query?}` | `[{id, label, models, models_text, default_model, default_label, key_required, key_env, default_base_url, configured, key_text}]`: octos's families with their catalog models, sorted by name; with `query`, those it names (name or id first, then alias, then one of its models); `configured` says the profile has a key for the family (the vault is not read) |
//! | `llm.models` | `{family, query?}` | `{family, label, key_required, default_model, models, routes}`: the family's catalog models for a pull-down, each `{id, label, context, price, detail, tier, default}` with `routes` `[{id, label, detail}]` (the official route first), and the family's routes |
//! | `llm.add_provider` | – | `{id, label}` once the person saves it on the host's sheet |
//! | `llm.edit_provider` | `{id}` | `{id, label}` (the id changes with the route) once saved on the sheet |
//! | `llm.set_model` | `{id, model}` | `{id}`; an empty model means the family default |
//! | `llm.move` | `{id, to}` | `{}`; `to` is the new position, 0 = primary |
//! | `llm.set_primary` | `{id}` | `{}` |
//! | `llm.remove` | `{id}` | `{}`; its key leaves the profile when no provider reads it |
//! | `llm.test` | `{id}` | `{ok, ms, error?}` after one tiny request to the provider (`ok` is a Splash keyword: a script tests `error == nil`) |
//! | `llm.export_qr` | `{ids?}` | `{}` when the person closes the sheet that shows the phone QR (all providers, or `ids`) |
//! | `llm.import_qr` | – | `{applied, added, updated, message}` once a scanned, picked, dropped or pasted code is imported on the sheet: `applied` labels every provider the code names, `added` those appended (or saved, into an empty list), `updated` the saved ones whose key it changed; `message` says so in a sentence |
//! | `llm.connect_client` | – | `{}` when the person closes the Talk to Octos sheet (`os.ai-providers` only; the app receives no connection detail) |
//!
//! The app never sees a key, a PIN or a QR. `add_provider`, `edit_provider`,
//! `export_qr` and `import_qr` raise the host's sheet, a separate isolate
//! over the app; only calls from that sheet (`llm.sheet.submit`,
//! `llm.sheet.test`, `llm.sheet.fetch_models`, `llm.sheet.cancel`,
//! `llm.sheet.qr`, `llm.sheet.show`, `llm.sheet.scan`, `llm.sheet.pick`,
//! `llm.sheet.image`, `llm.sheet.import`) can carry a key, a PIN or a code,
//! and `dispatch` refuses them from
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
//! The add and edit sheet is a five-step wizard in Octoscode's order: a
//! family, then one of its catalog models (or a custom id, or one the
//! endpoint lists: `llm.sheet.fetch_models`), then a route (the catalog's
//! endpoints, or a custom base URL and protocol), the key, and a test that
//! must pass before the save ([`sheets::edit`]). `llm.sheet.submit` takes `{family, model, route, base_url,
//! api_type, key, check}`: `route` is a catalog route id (`official`, `autodl`,
//! …) or `custom`; a catalog route other than the official one is saved with
//! its `route_id`, `label`, `base_url` and `api_key_env` (octos
//! `LlmRouteConfig`). With `check`, the route is tested first (the typed key,
//! else the saved one) and a failed test answers `{saved: false, error}`
//! without saving (`{saved: false, error, network}`). `llm.sheet.test` runs
//! the same test alone (`{ok, ms, error, network, reason}`; `network`: the
//! provider was not reached, and the wizard then offers "Save without
//! testing"; `reason`: `HTTP 401 · invalid key`, `unreachable`, …);
//! `llm.sheet.fetch_models` answers `{models: [{id, label}]}` from the
//! endpoint's model list (`GET {base}/models`, Anthropic `GET /v1/models`).
//!
//! Talk to Octos (`llm.connect_client`, feature `octos-core`) is off by
//! default. Its sheet, and only its sheet, calls `llm.sheet.client_info`,
//! `client_enable {on}`, `client_rotate`, `client_origin {origin}`,
//! `client_open`, `client_pair` (then `client_pair_ready` /
//! `client_pair_show`, the export sheet's pattern) and `client_back`. They
//! answer `{enabled, origin, endpoint, descriptor, profile_id, session_id,
//! web_origin}`; no token ever reaches a script, the clipboard or a link. A
//! web client pairs with a one-time code drawn on a host sheet with a QR of
//! its link ([`sheets::pair_client`]); leaving that sheet or closing the Talk
//! to Octos sheet turns pairing off. See `crates/kernel/README.md`.
//!
//! Only `os.` apps are served: the provider set is the device's.
//!
//! State is the kernel's profile, `<core_dir>/profiles/_main.json`: its
//! `config.llm` (primary and fallbacks) and the key env vars in
//! `config.env_vars`. Keys go to the [`vault`]; every change restarts the
//! shell's octos kernel (with the `octos-core` feature:
//! `octosense_kernel::restart()`, a no-op when none runs) and then calls
//! the shell's `on_changed` hook.
use octosense_appstore::services::{close_sheet_later, HostService, Replier, ServiceCall, ServiceHost};
use octosense_llm_config::{catalog, profile, qr, registry, Provider};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub mod complete;
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
/// import), after the service restarted the shell's octos kernel (feature
/// `octos-core`). Runs on whichever thread made the change.
pub type OnChanged = Arc<dyn Fn() + Send + Sync>;

/// Trusted host UI actions for Talk to Octos. No token ever reaches a
/// script, the clipboard or a link: a web client pairs with a one-time code.
pub enum ClientUiAction {
    /// Open the web client (a credential-free link) in the browser.
    OpenWeb(String),
}
pub type ClientUi = Arc<dyn Fn(ClientUiAction) + Send + Sync>;

/// What a shell hands the service. `Options::default()` is what
/// [`register`] uses.
#[derive(Clone, Default)]
pub struct Options {
    /// The kernel's octos home, holding `profiles/_main.json`. `None`: the
    /// shell's octos kernel's (`octosense_kernel::core_dir()`, feature
    /// `octos-core`), else `octosense_llm_config::profile::default_core_dir()`.
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
    pub client_ui: Option<ClientUi>,
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
    let core_dir = resolved_core_dir(&options);
    let vault = options.vault.clone().unwrap_or_else(|| vault::platform(&core_dir));
    *IMAGE_WAITER.lock().unwrap() = None;
    octosense_appstore::services::register_host_service(Box::new(LlmService {
        shared: Arc::new(Shared { path: profile::profile_path(&core_dir), vault, on_changed: options.on_changed, lock: Mutex::new(()) }),
        scanner: options.scanner,
        image_picker: options.image_picker,
        image_drops: options.image_drops,
        pending: Arc::default(),
        export: Arc::default(),
        pair: Arc::default(),
        generation: 0,
        qr_lifetime: qr_lifetime(options.qr_lifetime_secs),
        client_ui: options.client_ui,
    }));
}

/// Offer the `model` service ([`complete`]) over the same profile and vault
/// as the `llm` service `options` describe: the person's providers, in
/// their order, with their keys, which no app ever sees. `model` supplies
/// the rest (grants, limits, a transport for tests); providers it already
/// names are kept.
pub fn register_model(options: &Options, model: complete::Options) -> Arc<complete::ModelHost> {
    let model = if model.providers.is_some() {
        model
    } else {
        let core_dir = resolved_core_dir(options);
        let vault = options.vault.clone().unwrap_or_else(|| vault::platform(&core_dir));
        model.providers(Arc::new(complete::ProfileProviders { path: profile::profile_path(&core_dir), vault }))
    };
    complete::register_with(model)
}

/// Where the profile lives: the shell's choice, else the octos kernel's core
/// dir, else the default.
fn resolved_core_dir(options: &Options) -> PathBuf {
    let kernel: Option<PathBuf> = {
        #[cfg(feature = "octos-core")]
        {
            octosense_kernel::core_dir()
        }
        #[cfg(not(feature = "octos-core"))]
        {
            None
        }
    };
    options
        .core_dir
        .clone()
        .or(kernel)
        .or_else(profile::default_core_dir)
        .unwrap_or_else(|| std::env::temp_dir().join("octos-home/.octos"))
}

/// The provider set changed on disk: the kernel reads it only when it
/// starts, so restart it (if it runs), then tell the shell.
fn changed(on_changed: &Option<OnChanged>) {
    #[cfg(feature = "octos-core")]
    octosense_kernel::restart();
    if let Some(changed) = on_changed {
        changed();
    }
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
        changed(&self.on_changed);
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
    Connect,
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
    /// The pairing sheet being prepared, like `export`.
    pair: Arc<Mutex<Option<ExportJob>>>,
    generation: u64,
    qr_lifetime: u64,
    client_ui: Option<ClientUi>,
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
    let model = model::effective_model(p).unwrap_or_default();
    let known = catalog::model(&p.family, &model);
    json!({
        "id": model::id_of(p),
        "family": registry::lookup(&p.family).map(|f| f.id).unwrap_or(p.family.as_str()),
        "label": model::family_label(&p.family),
        "model": model,
        "model_label": model::model_label(p),
        "custom_model": p.model.is_some(),
        "route": model::route_choice(p),
        "route_label": model::route_label(p),
        "context": known.map(catalog::Model::context_text).unwrap_or_default(),
        "price": known.map(catalog::Model::price_text).unwrap_or_default(),
        "tier": known.map(|m| m.tier.as_str()).unwrap_or(""),
        "base_url": p.base_url,
        "api_type": p.api_type.map(|t| t.as_str()),
        "key": store.status(p).text(),
    })
}

/// How well `query` (lowercased, trimmed) names `family`: 0 its name or id
/// starts with it, 1 they contain it, 2 an alias does, 3 one of its models
/// does; `None`: no match.
fn family_rank(f: &catalog::CatalogFamily, query: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(0);
    }
    let (id, label) = (f.id().to_lowercase(), f.label().to_lowercase());
    if id.starts_with(query) || label.starts_with(query) {
        Some(0)
    } else if id.contains(query) || label.contains(query) {
        Some(1)
    } else if f.family.aliases.iter().any(|a| a.to_lowercase().contains(query)) {
        Some(2)
    } else if f.models.iter().any(|m| m.id.to_lowercase().contains(query) || m.label.to_lowercase().contains(query)) {
        Some(3)
    } else {
        None
    }
}

/// The families for the add sheet's first step, sorted by name. `env_vars`
/// is the profile's (a family with a value for its key env var is
/// `configured`; the vault is not read).
fn families(query: &str, env_vars: &BTreeMap<String, String>) -> Value {
    let query = query.trim().to_lowercase();
    let mut list: Vec<(u8, &catalog::CatalogFamily)> =
        catalog::families().iter().filter_map(|f| Some((family_rank(f, &query)?, f))).collect();
    list.sort_by_key(|(rank, f)| (*rank, f.label().to_lowercase()));
    let list: Vec<&catalog::CatalogFamily> = list.into_iter().map(|(_, f)| f).collect();
    json!(list
        .iter()
        .map(|f| {
            let fam = f.family;
            let key_env = registry::key_env_for(fam.id);
            let configured = env_vars.get(&key_env).is_some_and(|v| !v.trim().is_empty());
            let n = f.models.len();
            let default = f.default_model();
            json!({
                "id": fam.id, "label": fam.label, "models": n,
                "models_text": if n == 1 { "1 model".to_string() } else { format!("{n} models") },
                "default_model": default,
                "default_label": default.map(catalog::model_label),
                "key_required": fam.key_required, "key_env": key_env, "has_key": fam.key_env.is_some(),
                "default_base_url": fam.default_base_url,
                "configured": configured,
                "key_text": if !fam.key_required { "No key needed" } else if configured { "Key saved" } else { "Needs a key" },
            })
        })
        .collect::<Vec<_>>())
}

/// A route's second line: where it goes.
fn route_detail(family: &registry::Family, r: &catalog::Route) -> String {
    let url = r.base_url.as_deref().or(family.default_base_url).unwrap_or("your endpoint");
    url.trim_start_matches("https://").trim_start_matches("http://").split('/').next().unwrap_or(url).to_string()
}

fn route_json(family: &registry::Family, r: &catalog::Route) -> Value {
    json!({"id": r.id, "label": r.label, "detail": route_detail(family, r)})
}

/// A family's catalog models for a pull-down (the default first marked),
/// those matching `query`, with each model's routes.
fn models(family: &str, query: &str) -> Result<Value, String> {
    let f = catalog::family(family).ok_or("There is no such provider.")?;
    let query = query.trim().to_lowercase();
    let rows: Vec<Value> = f
        .models
        .iter()
        .filter(|m| query.is_empty() || m.id.to_lowercase().contains(&query) || m.label.to_lowercase().contains(&query))
        .map(|m| {
            let (context, price) = (m.context_text(), m.price_text());
            let detail = [context.as_str(), price.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" · ");
            json!({
                "id": m.id, "label": m.label, "context": context, "price": price, "detail": detail,
                "tier": m.tier.as_str(), "default": m.default,
                "routes": m.routes().iter().map(|r| route_json(f.family, r)).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({
        "family": f.id(), "label": f.label(), "key_required": f.family.key_required,
        "default_model": f.default_model(),
        "models": rows,
        "routes": f.routes().iter().map(|r| route_json(f.family, r)).collect::<Vec<_>>(),
    }))
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

    /// The route a sheet form describes: `route` names a catalog route (or
    /// `custom`); a form without one gives the base URL and protocol itself.
    fn form_route(args: &Value) -> Result<Provider, String> {
        match args["route"].as_str() {
            Some(route) => model::provider_from_route(text(args, "family"), text(args, "model"), route, text(args, "base_url"), text(args, "api_type")),
            None => model::provider_from(text(args, "family"), text(args, "model"), text(args, "base_url"), text(args, "api_type")),
        }
    }

    /// The key a sheet typed: taken as typed but for surrounding space.
    fn form_key(args: &Value) -> Result<String, String> {
        let key = text(args, "key").trim().to_string();
        if key.chars().any(char::is_control) || key.len() > 16 * 1024 {
            return Err("That key is not acceptable text.".into());
        }
        Ok(key)
    }

    /// Edit keeps the key slot the provider reads while it stays on the same
    /// family and route (a numbered slot an import made, say).
    fn keep_slot(list: &[Provider], edit: Option<&str>, route: &mut Provider) {
        let Some(at) = edit.and_then(|id| model::index_of(list, id).ok()) else { return };
        let old = &list[at];
        if registry::lookup(&old.family).map(|f| f.id) == Some(route.family.as_str()) && old.route_id == route.route_id {
            route.key_env = old.key_env.clone();
        }
    }

    fn submit(&mut self, args: &Value, reply: Replier) {
        let edit = match self.pending_kind() {
            Some(Kind::Add) => None,
            Some(Kind::Edit(id)) => Some(id),
            _ => return reply.send(Err("No app is waiting for this sheet.".into())),
        };
        let mut route = match Self::form_route(args) {
            Ok(p) => p,
            Err(e) => return reply.send(Err(e)),
        };
        let key = match Self::form_key(args) {
            Ok(k) => k,
            Err(e) => return reply.send(Err(e)),
        };
        let check = args["check"].as_bool() == Some(true);
        let (shared, pending) = (self.shared.clone(), self.pending.clone());
        work(move || {
            if check {
                // Test before saving; a failure keeps the sheet up and says why.
                let tested = shared.open().map(|store| {
                    Self::keep_slot(&store.list, edit.as_deref(), &mut route);
                    let saved = store.key(&route);
                    let key = Some(key.as_str()).filter(|k| !k.is_empty()).or(saved.as_deref());
                    if key.is_none() && model::key_required(&route.family) {
                        return Err("Type the provider's API key.".to_string());
                    }
                    Ok(probe::run_within(&route, key, std::time::Duration::from_secs(10)))
                });
                match tested.and_then(|t| t) {
                    Err(e) => return reply.send(Err(e)),
                    Ok(t) if t["ok"] != true => {
                        let error = t["error"].as_str().unwrap_or("The test failed.");
                        return reply.send(Ok(json!({"saved": false, "error": error, "network": t["network"] == true})));
                    }
                    Ok(_) => {}
                }
            }
            let saved = shared.change(|store, list| {
                Self::keep_slot(list, edit.as_deref(), &mut route);
                match &edit {
                    Some(id) => {
                        let at = model::index_of(list, id)?;
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
                    reply.send(Ok(json!({"saved": true})));
                }
            }
        });
    }

    /// "Test connection" on the sheet: the form's route with the typed key,
    /// else the saved one.
    fn sheet_test(&mut self, args: &Value, reply: Replier) {
        let edit = match self.pending_kind() {
            Some(Kind::Add) => None,
            Some(Kind::Edit(id)) => Some(id),
            _ => return reply.send(Err("No app is waiting for this sheet.".into())),
        };
        let (route, key) = match Self::form_route(args).and_then(|r| Ok((r, Self::form_key(args)?))) {
            Ok(v) => v,
            Err(e) => return reply.send(Err(e)),
        };
        let shared = self.shared.clone();
        work(move || {
            reply.send(shared.open().map(|store| {
                let mut route = route;
                Self::keep_slot(&store.list, edit.as_deref(), &mut route);
                let saved = store.key(&route);
                let key = Some(key.as_str()).filter(|k| !k.is_empty()).or(saved.as_deref());
                if key.is_none() && model::key_required(&route.family) {
                    return json!({"ok": false, "ms": 0, "error": "Type the provider's API key first.", "network": false, "reason": "no key"});
                }
                probe::run(&route, key)
            }))
        });
    }

    /// "Fetch models from provider": the endpoint's own model list, with the
    /// typed key, else the saved one.
    fn sheet_fetch_models(&mut self, args: &Value, reply: Replier) {
        if !matches!(self.pending_kind(), Some(Kind::Add | Kind::Edit(_))) {
            return reply.send(Err("No app is waiting for this sheet.".into()));
        }
        // The list does not depend on the model: any id makes the route.
        let mut form = args.clone();
        if text(args, "model").trim().is_empty() {
            form["model"] = json!("-");
        }
        let (route, key) = match Self::form_route(&form).and_then(|r| Ok((r, Self::form_key(args)?))) {
            Ok(v) => v,
            Err(e) => return reply.send(Err(e)),
        };
        let shared = self.shared.clone();
        work(move || {
            let answer = shared.open().and_then(|store| {
                let saved = store.key(&route);
                let key = Some(key.as_str()).filter(|k| !k.is_empty()).or(saved.as_deref());
                if key.is_none() && model::key_required(&route.family) {
                    return Err("Type the provider's API key first.".to_string());
                }
                let ids = probe::fetch_models(&route, key)?;
                let n = ids.len();
                Ok(json!({
                    "models": ids.iter().map(|id| json!({"id": id, "label": catalog::model_label(id)})).collect::<Vec<_>>(),
                    "message": if n == 1 { "The provider lists 1 model.".to_string() } else { format!("The provider lists {n} models.") },
                }))
            });
            reply.send(answer);
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


/// The Talk to Octos sheet's controls (a worker thread: they wait on the
/// kernel). What they answer never carries a token.
#[cfg(feature = "octos-core")]
fn client_action(method: &str, args: &Value, ui: Option<ClientUi>) -> Result<Value, String> {
    use octosense_kernel as kernel;
    match method {
        "sheet.client_enable" => kernel::set_external_access(args["on"] == true)?,
        "sheet.client_rotate" => kernel::rotate_external_access()?,
        "sheet.client_origin" => kernel::set_web_client_origin(text(args, "origin"))?,
        "sheet.client_open" => {
            let ui = ui.ok_or("The host cannot open a browser.")?;
            ui(ClientUiAction::OpenWeb(kernel::web_client_url()?));
            return Ok(json!({}));
        }
        _ => {}
    }
    client_info()
}

#[cfg(feature = "octos-core")]
fn client_info() -> Result<Value, String> {
    use octosense_kernel as kernel;
    let enabled = kernel::external_access();
    let web_origin = kernel::web_client_origin();
    let (origin, endpoint, descriptor) = if enabled {
        let access = kernel::client_access()?;
        let descriptor = kernel::core_dir().map(|d| kernel::connection_file(&d).display().to_string()).unwrap_or_default();
        (access.origin.clone(), access.endpoint(), descriptor)
    } else {
        Default::default()
    };
    Ok(json!({"enabled": enabled, "origin": origin, "endpoint": endpoint, "descriptor": descriptor,
        "profile_id": "_main", "session_id": kernel::SYSTEM_SESSION, "web_origin": web_origin}))
}

/// Mint a pairing code and build its sheet: the code, and a QR of the web
/// client's pairing link.
#[cfg(feature = "octos-core")]
fn pairing_sheet(lifetime: u64) -> Result<String, String> {
    use octosense_kernel as kernel;
    let web_origin = kernel::web_client_origin();
    let pairing = kernel::pairing()?;
    let link = if web_origin.is_empty() {
        None
    } else {
        let system = kernel::system_reference().ok().map(|(_, reference)| reference);
        Some(kernel::pairing_link(&web_origin, &pairing, system.as_deref())?)
    };
    let qr = match &link {
        Some(link) => Some(qr::render_matrix(link).map_err(|e| e.to_string())?),
        None => None,
    };
    Ok(sheets::pair_client(&pairing.code, &pairing.server_origin, qr.as_ref().map(|(n, m)| (*n, m.as_slice())), pairing.expires_in_secs.min(lifetime)))
}

#[cfg(not(feature = "octos-core"))]
fn client_action(_method: &str, _args: &Value, _ui: Option<ClientUi>) -> Result<Value, String> {
    Err("This build does not include the Octos kernel service.".into())
}

#[cfg(not(feature = "octos-core"))]
fn pairing_sheet(_lifetime: u64) -> Result<String, String> {
    Err("This build does not include the Octos kernel service.".into())
}

/// The pairing code's sheet is up or being prepared: turn pairing off. The
/// epoch is taken now, so a newer code minted before the worker runs stays.
fn end_pairing() {
    #[cfg(feature = "octos-core")]
    {
        let epoch = octosense_kernel::pairing_epoch();
        work(move || octosense_kernel::end_pairing_if(epoch));
    }
}

impl LlmService {
    /// `llm.sheet.client_pair`: prepare the pairing sheet on a worker; the
    /// connect sheet polls `sheet.client_pair_ready`, then asks for
    /// `sheet.client_pair_show` (the export sheet's pattern).
    fn client_pair(&mut self, reply: Replier) {
        self.generation += 1;
        let generation = self.generation;
        *self.pair.lock().unwrap() = Some((generation, None));
        let pair = self.pair.clone();
        let lifetime = self.qr_lifetime.max(60);
        work(move || {
            let sheet = pairing_sheet(lifetime);
            if let Some(job) = pair.lock().unwrap().as_mut().filter(|job| job.0 == generation) {
                job.1 = Some(sheet);
            }
        });
        reply.send(Ok(json!({})));
    }

    fn client_pair_ready(&self, reply: Replier) {
        match self.pair.lock().unwrap().as_ref() {
            None => reply.send(Err("No pairing code is being prepared.".into())),
            Some((_, None)) => reply.send(Ok(json!({"ready": false}))),
            Some((_, Some(Ok(_)))) => reply.send(Ok(json!({"ready": true}))),
            Some((_, Some(Err(e)))) => reply.send(Err(e.clone())),
        }
    }

    fn client_pair_show(&mut self, reply: Replier, host: &mut dyn ServiceHost) {
        match self.pair.lock().unwrap().take() {
            Some((_, Some(Ok(sheet)))) => {
                host.open_sheet(sheet);
                reply.send(Ok(json!({})));
            }
            _ => reply.send(Err("The pairing code is not ready.".into())),
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
            "connect_client" => {
                if call.app_id != "os.ai-providers" { return reply.send(Err("Open AI providers to connect a client.".into())); }
                self.raise(&call.app_id, reply, Kind::Connect, sheets::connect_client(), host);
            }
            method if method.starts_with("sheet.client_") => {
                // Only the Talk to Octos sheet this app raised, never the app.
                let permitted = call.from_sheet && self.pending.lock().unwrap().as_ref()
                    .is_some_and(|p| p.kind == Kind::Connect && p.app_id == call.app_id);
                if !permitted { return reply.send(Err("Open the Talk to Octos sheet first.".into())); }
                match method {
                    "sheet.client_pair" => self.client_pair(reply),
                    "sheet.client_pair_ready" => self.client_pair_ready(reply),
                    "sheet.client_pair_show" => self.client_pair_show(reply, host),
                    "sheet.client_back" => {
                        // Leaving the code's sheet ends pairing.
                        end_pairing();
                        host.open_sheet(sheets::connect_client());
                        reply.send(Ok(json!({})));
                    }
                    "sheet.client_info" | "sheet.client_enable" | "sheet.client_rotate" | "sheet.client_origin" | "sheet.client_open" => {
                        let method = method.to_owned();
                        let ui = self.client_ui.clone();
                        work(move || reply.send(client_action(&method, &call.args, ui)));
                    }
                    other => reply.send(Err(format!("llm has no method {other:?}"))),
                }
            }
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
            "families" => {
                let (shared, query) = (self.shared.clone(), text(&call.args, "query").to_string());
                work(move || {
                    let env = shared.open().map(|s| s.env_vars).unwrap_or_default();
                    reply.send(Ok(families(&query, &env)));
                });
            }
            "models" => reply.send(models(text(&call.args, "family"), text(&call.args, "query"))),
            "add_provider" => match self.shared.open() {
                Ok(store) => {
                    let sheet = sheets::edit(None, !store.list.is_empty());
                    self.raise(&call.app_id, reply, Kind::Add, sheet, host)
                }
                Err(e) => reply.send(Err(e)),
            },
            "edit_provider" => match self.provider(&id) {
                Ok(p) => self.raise(&call.app_id, reply, Kind::Edit(id), sheets::edit(Some(&p), true), host),
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
                if self.pair.lock().unwrap().take().is_some() || self.pending_kind() == Some(Kind::Connect) {
                    // The Talk to Octos sheet closed: no pairing code outlives it.
                    end_pairing();
                }
                IMAGE_WAITER.lock().unwrap().take();
                if let Some(waiting) = self.pending.lock().unwrap().take() {
                    // Closing the QR is how an export ends.
                    let answer = if matches!(waiting.kind, Kind::Export(_) | Kind::Connect) { Ok(json!({})) } else { Err("Cancelled.".into()) };
                    waiting.reply.send(answer);
                }
                reply.send(Ok(json!({})));
            }
            "sheet.submit" => self.submit(&call.args, reply),
            "sheet.test" => self.sheet_test(&call.args, reply),
            "sheet.fetch_models" => self.sheet_fetch_models(&call.args, reply),
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

#[cfg(all(test, feature = "octos-core"))]
mod octos_core_tests {
    use super::*;

    /// The service and the kernel agree on the profile: without a shell
    /// choice the service writes under the kernel's core dir, and a shell's
    /// explicit dir still wins.
    #[test]
    fn the_profile_is_the_kernels_unless_the_shell_says_otherwise() {
        if let Some(kernel) = octosense_kernel::core_dir() {
            assert_eq!(resolved_core_dir(&Options::default()), kernel);
        }
        let explicit = std::env::temp_dir().join("llm-explicit-core");
        assert_eq!(resolved_core_dir(&Options::default().core_dir(&explicit)), explicit);
        // A change with no kernel running restarts nothing and still calls
        // the shell's hook.
        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = called.clone();
        changed(&Some(Arc::new(move || flag.store(true, std::sync::atomic::Ordering::SeqCst))));
        assert!(called.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!octosense_kernel::status().running);
    }
}
