//! Real GitHub catalog -> Store install -> Splash app tool -> shared component.
//! Uses only the two synthetic component-demo identities and a new private home.
//! No legacy channel, signing key, model, provider account, or grant substitution.
use makepad_widgets::*;
use octosense_app_hub::{PreparedLaunch, Store};
use octosense_app_policy::{AssetServer, HostLimits};
use octosense_appstore::{
    components, host_api, script_tools, services,
    source::{CatalogChannel, Origin},
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

const IDS: [&str; 2] = [
    "org.ymote.componentdemo.first",
    "org.ymote.componentdemo.second",
];

pub struct Config {
    mirror: PathBuf,
    home: PathBuf,
    receipt: PathBuf,
}

pub fn config(cx: &Cx) -> Option<Config> {
    #[cfg(target_os = "android")]
    {
        let directory = PathBuf::from(cx.get_data_dir()?).join("shared-components-lab");
        let mirror = directory.join("mirror");
        mirror.join("catalog-v2.json").exists().then(|| Config {
            mirror,
            home: directory.join("home"),
            receipt: directory.join("receipt.json"),
        })
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = cx;
        let argument = |prefix: &str| {
            std::env::args().find_map(|arg| arg.strip_prefix(prefix).map(PathBuf::from))
        };
        argument("--shared-mirror=").map(|mirror| Config {
            mirror,
            home: argument("--shared-home=").expect("Use a new --shared-home directory"),
            receipt: argument("--receipt=").expect("Use a new --receipt path"),
        })
    }
}

pub struct Acceptance {
    root: PathBuf,
    receipt: PathBuf,
    launches: Vec<PreparedLaunch>,
    card: SplashRef,
    sheet: SplashRef,
    assets: Option<AssetServer>,
    registration: Option<script_tools::Registration>,
    replies: Arc<Mutex<Vec<Result<Value, String>>>>,
    step: usize,
    results: Vec<Value>,
    receipt_data: Value,
    completed: bool,
    _timer: Timer,
}

impl Acceptance {
    pub fn new(cx: &mut Cx, ui: &WidgetRef, config: Config) -> Result<Self, String> {
        if config.receipt.exists() || config.home.exists() {
            return Err("The shared-component fixture requires a new home and receipt".into());
        }
        if matches!(std::env::var("OCTOSENSE_HUB_CATALOG").ok().as_deref(), Some(value) if value != "github")
        {
            return Err("This acceptance refuses a legacy catalog override".into());
        }
        let root = config.home.join("apps");
        std::fs::create_dir(&config.home).map_err(|e| e.to_string())?;
        // All process-wide paths are redirected before any storage service is
        // initialized. Existing user profiles and native app homes are unused.
        std::env::set_var("OCTOSENSE_HOME", &config.home);
        std::env::set_var("OCTOSENSE_APP_DATA", &root);
        std::env::set_var("OCTOSENSE_SECRETS", "file");
        let storage = octosense_shell::app_storage::init(Some(config.home.clone()))
            .ok_or("Private app storage unavailable")?;
        if storage.layout().apps_root() != root {
            return Err("Fixture storage was already initialized elsewhere".into());
        }
        octosense_appstore::set_data_root(root.clone());
        let channel = CatalogChannel::from_environment(&root)?;
        if channel != CatalogChannel::GitHub {
            return Err("Default GitHub trust channel is required".into());
        }
        let origin = Origin::Directory(config.mirror);
        let catalog = origin.catalog_for(channel)?;
        let verified = octosense_app_hub::github_catalog::verify_document(catalog.as_bytes())?;
        let mut store = Store::new(
            octosense_appstore::DEFAULT_ANCHOR,
            &root,
            HostLimits::default(),
        )
        .with_github_catalog()
        .with_host_api_versions(host_api::available_versions());
        store.accept_catalog_and_cache(&catalog, &root.join("catalog-v2.json"))?;
        let mut launches = Vec::new();
        for id in IDS {
            let entry = store
                .catalog()
                .unwrap()
                .entries
                .iter()
                .rev()
                .find(|entry| entry.app_id() == id)
                .ok_or_else(|| format!("Verified catalog has no {id}"))?
                .clone();
            let staging = config.home.join(format!("staging-{id}"));
            std::fs::create_dir(&staging).map_err(|e| e.to_string())?;
            let bundle = origin.stage(&entry.artifact, &staging)?;
            components::install(&store, &origin, &entry.manifest)?;
            store.install_staged(
                id,
                &bundle,
                &store.publisher_keys(),
                &octosense_app_hub::today(),
            )?;
            let launch = store.prepare_launch(id)?;
            store.validate_prepared_launch(&launch)?;
            storage.installed(id);
            launches.push(launch);
            std::fs::remove_dir_all(staging).map_err(|e| e.to_string())?;
        }
        let first = serde_json::to_value(&launches[0].manifest).map_err(|e| e.to_string())?;
        let second = serde_json::to_value(&launches[1].manifest).map_err(|e| e.to_string())?;
        if first["capabilities"]
            .as_array()
            .is_some_and(|caps| !caps.is_empty())
        {
            return Err("First consumer must exercise an empty capability declaration".into());
        }
        if second["capabilities"] != json!(["storage", "wasm"])
            && second["capabilities"] != json!(["wasm", "storage"])
        {
            return Err("Second consumer must declare wasm and storage".into());
        }
        let first_components = components::resolved_in(&root, IDS[0])?;
        let second_components = components::resolved_in(&root, IDS[1])?;
        if first_components.len() != 2 || second_components.len() != 2 {
            return Err("Expected two pinned shared components per app".into());
        }
        for (a, b) in first_components.iter().zip(&second_components) {
            if a.path != b.path
                || a.blake3 != b.blake3
                || !std::fs::metadata(&a.path)
                    .map_err(|e| e.to_string())?
                    .permissions()
                    .readonly()
            {
                return Err("Shared bytes are not deduplicated and read-only".into());
            }
        }
        let receipt_data = json!({"schema":1,"platform":host_api::platform(),
            "proof":"real GitHub catalog and publisher proofs -> Store install -> app tool -> Splash host.request -> shared component",
            "source_revision":env!("WASM_FIXTURE_REVISION"),"source_dirty":env!("WASM_FIXTURE_DIRTY")!="false",
            "runtime_tree":env!("WASM_FIXTURE_RUNTIME_TREE"),"catalog_sha256":verified.sha256(),
            "catalog_sequence":verified.catalog().sequence,"catalog_channel":"github","legacy_fallback":false,
            "apps":[{"id":IDS[0],"bundle_digest":launches[0].manifest.integrity.bundle_blake3,"declarations":first["capabilities"]},
                    {"id":IDS[1],"bundle_digest":launches[1].manifest.integrity.bundle_blake3,"declarations":second["capabilities"]}],
            "components":first_components.iter().map(|entry|json!({"alias":entry.alias,"id":entry.id,"version":entry.version,"blake3":entry.blake3})).collect::<Vec<_>>(),
            "deduplicated_readonly_components":true,"checks":{},"personal_accounts_used":false,"model_started":false,
            "not_verified":["performance benchmark","live model relay","production Home upgrade"]});
        let mut acceptance = Self {
            root,
            receipt: config.receipt,
            launches,
            card: ui.splash(cx, ids!(app)),
            sheet: ui.splash(cx, ids!(sheet)),
            assets: None,
            registration: None,
            replies: Arc::default(),
            step: 0,
            results: vec![],
            receipt_data,
            completed: false,
            _timer: cx.start_interval(0.03),
        };
        acceptance.open_current(cx)?;
        Ok(acceptance)
    }

    fn open_current(&mut self, cx: &mut Cx) -> Result<(), String> {
        self.registration.take();
        self.assets.take();
        let index = self.step % 2;
        let launch = &self.launches[index];
        let id = IDS[index];
        let bundle = launch.bundle();
        let mut settings = launch.policy.isolate_settings(&self.root);
        std::fs::create_dir_all(&settings.jail_root).map_err(|e| e.to_string())?;
        let assets = AssetServer::start_with_static(bundle, &[])?;
        settings.hosts.push(assets.allowlist_entry());
        let source = octosense_app_policy::script_source(bundle, assets.origin())?
            .ok_or("Consumer has no script")?;
        octosense_app_policy::splash_adapter::apply(&self.card, cx, &settings);
        octosense_appstore::apply_device_consent(cx, bundle, &self.card)?;
        self.card.set_text(cx, &source);
        self.registration = script_tools::bind(cx, id, bundle, &self.card)?;
        self.assets = Some(assets);
        let replies = self.replies.clone();
        script_tools::submit(
            id,
            if index == 0 {
                "first.inspect"
            } else {
                "second.inspect"
            },
            json!({}),
            "device",
            "native-acceptance",
            Duration::from_secs(45),
            Box::new(move |reply| replies.lock().unwrap().push(reply)),
        )?;
        Ok(())
    }

    pub fn handle_event(&mut self, cx: &mut Cx, event: &Event, ui: &WidgetRef) {
        ui.handle_event(cx, event, &mut Scope::empty());
        if self.completed {
            return;
        }
        let id = IDS[self.step % 2];
        // No test grant and no approval sheet: component host calls use the
        // production UI dispatcher with may_prompt=false.
        self.card.set_host_prompts(cx, false);
        script_tools::pump(cx, id, &self.card);
        services::pump(cx, id, &self.root.join(".host"), &self.card, &self.sheet);
        octosense_shell::wasm_service::pump_host_calls();
        octosense_shell::platform_services::handle_event(cx, event);
        let reply = self.replies.lock().unwrap().pop();
        if let Some(reply) = reply {
            match reply {
                Ok(value) => self.results.push(value),
                Err(error) => {
                    self.finish(Err(error));
                    return;
                }
            }
            if self.sheet.borrow().is_some_and(|sheet| sheet.view.visible) {
                self.finish(Err("Unexpected host approval sheet".into()));
                return;
            }
            self.step += 1;
            if self.step == 4 {
                let result = self.validate();
                self.finish(result);
            } else if let Err(error) = self.open_current(cx) {
                self.finish(Err(error));
            }
        }
    }

    fn validate(&mut self) -> Result<(), String> {
        let mut checks = serde_json::Map::new();
        for (step, value) in self.results.iter().enumerate() {
            let id = IDS[step % 2];
            let name = if step % 2 == 0 { "First" } else { "Second" };
            let expected = format!("<h1>Component Demo · {name}</h1>\n");
            let results = &value["results"];
            let expected_count = if step < 2 { 1 } else { 3 };
            let mut check = |name: &str, ok: bool| -> Result<(), String> {
                checks.insert(format!("turn_{}_{}", step + 1, name), json!(ok));
                if ok {
                    Ok(())
                } else {
                    Err(format!("turn {}: {name}", step + 1))
                }
            };
            check("app_identity", value["app"] == id)?;
            check(
                "retained_isolated_counter",
                results["count_1"]["data"] == expected_count
                    && results["count_2"]["data"] == expected_count + 1,
            )?;
            check(
                "markdown",
                results["markdown"]["is_ok"] == true && results["markdown"]["data"] == expected,
            )?;
            check(
                "private_storage_before_write",
                if step < 2 {
                    results["before_write"]["is_ok"] == false
                } else {
                    results["before_write"]["data"] == expected
                },
            )?;
            check(
                "private_storage_after_write",
                results["saved"]["is_ok"] == true && results["read_back"]["data"] == expected,
            )?;
            let descriptor = results["host_call"]["data"]
                .as_str()
                .and_then(|text| serde_json::from_str::<Value>(text).ok())
                .unwrap_or(Value::Null);
            check(
                "real_host_dispatch",
                results["host_call"]["is_ok"] == true
                    && descriptor["descriptor"]["name"] == "runtime.describe"
                    && descriptor["implemented"] == true,
            )?;
            let names = results["functions"]["data"]["functions"].as_array();
            check(
                "alias_discovery",
                names.is_some_and(|names| {
                    names.contains(&json!("md.to_html")) && names.contains(&json!("bridge.call"))
                }),
            )?;
        }
        self.receipt_data["checks"] = Value::Object(checks);
        Ok(())
    }

    fn finish(&mut self, result: Result<(), String>) {
        use std::io::Write;
        self.completed = true;
        self.registration.take();
        self.receipt_data["passed"] = json!(result.is_ok());
        self.receipt_data["turns"] = json!(self.results);
        if let Err(error) = result {
            self.receipt_data["error"] = json!(error);
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.receipt)
            .and_then(|mut file| {
                file.write_all(&serde_json::to_vec_pretty(&self.receipt_data).unwrap())
            })
            .expect("Write private shared-component receipt");
        println!("SHARED_COMPONENT_ACCEPTANCE_COMPLETE");
    }
}
