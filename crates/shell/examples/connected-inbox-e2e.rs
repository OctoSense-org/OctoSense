//! Real shell, installed Inbox bundle, broker and configured model. Gmail alone
//! is synthetic via a compile-only provider dependency. No approval is forged.
//! Requires a signed connected-install --keep-profile root, explicit
//! OCTOSENSE_HOME/OCTOSENSE_APP_DATA and a configured pinned kernel.
use octosense_shell::{makepad_widgets::*, App as ShellApp};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};

const APP: &str = "org.octosense.samples.inbox";
static FIXTURE: OnceLock<Arc<octosense_oauth_service::acceptance_inbox::Fixture>> = OnceLock::new();
static ROOT: OnceLock<PathBuf> = OnceLock::new();

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)){ui: mod.widgets.OctoSenseRoot{}}
}
#[derive(Script, ScriptHook)]
struct App {
    #[deref]
    shell: ShellApp,
}
fn setup() {
    let provider = std::env::args()
        .find_map(|a| a.strip_prefix("--provider-fixture=").map(str::to_owned))
        .unwrap_or_else(|| "inbox".into());
    let app = match provider.as_str() {
        "inbox" => APP,
        "calendar" => "org.octosense.samples.googlecalendar",
        "github" => "org.octosense.samples.githubnotes",
        _ => panic!("Unknown acceptance provider"),
    };
    let apps = PathBuf::from(
        std::env::var_os("OCTOSENSE_APP_DATA").expect("Choose isolated OCTOSENSE_APP_DATA"),
    );
    let meta: serde_json::Value = serde_json::from_slice(
        &std::fs::read(apps.join(".connected-e2e.json"))
            .expect("Install signed connected fixture first"),
    )
    .expect("Read fixture metadata");
    assert_eq!(meta["fixture"], "connected-e2e");
    assert!(meta["apps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["id"] == app));
    assert_eq!(
        std::env::var("OCTOSENSE_HUB_ANCHOR").unwrap(),
        meta["anchor"].as_str().unwrap()
    );
    let root = apps.join(".host");
    std::fs::create_dir_all(&root).expect("Create isolated fixture host directory");
    match provider.as_str() {
        "inbox" => {
            let fixture = octosense_oauth_service::acceptance_inbox::install(&root, app)
                .expect("Install isolated synthetic Gmail dependency");
            assert!(FIXTURE.set(Arc::new(fixture)).is_ok());
        }
        "calendar" => {
            octosense_oauth_service::acceptance_calendar::install(&root, app)
                .expect("Install isolated synthetic Calendar dependency");
        }
        "github" => {
            octosense_oauth_service::acceptance_github::install(&root, app)
                .expect("Install isolated synthetic GitHub dependency");
        }
        _ => unreachable!(),
    }
    ROOT.set(root).unwrap();
    octosense_shell::octosense::paths::set_package_dir(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../desktop"
    ));
}
fn receipt(value: serde_json::Value) {
    let root = ROOT.get().unwrap();
    let value = json!({"fixture":"real-shell-and-model-synthetic-gmail","live_google_oauth":false,"real_mail_delivery":false,"result":value,"provider":FIXTURE.get().unwrap().receipt()});
    std::fs::write(
        root.join("inbox-e2e-receipt.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        setup();
        ShellApp::shell_script_mod(vm);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.shell.shell_handle_event(cx, event);
        if matches!(event, Event::Startup) && FIXTURE.get().is_some() {
            if let Some(app) = octosense_shell::agents::find(APP) {
                octosense_shell::agents::ask(&app);
            }
            std::thread::spawn(|| {
                let fixture = FIXTURE.get().unwrap();
                receipt(json!({"phase":"waiting_for_normal_agent_consent"}));
                // Consent is requested and answered through the normal UI.
                // There is no developer bypass or prewritten consent record.
                for _ in 0..600 {
                    if octosense_shell::agents::access(APP)
                        == octosense_shell::agents::Access::Allowed
                    {
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
                let result = (|| -> Result<serde_json::Value, String> {
                    let baseline = octosense_shell::connected_events::poll_once(
                        APP,
                        &fixture.connection.handle,
                    )?;
                    receipt(json!({"phase":"baseline_ready","baseline":baseline}));
                    fixture.release_new_mail()?;
                    receipt(json!({"phase":"new_synthetic_mail_released"}));
                    let processed = octosense_shell::connected_events::poll_once(
                        APP,
                        &fixture.connection.handle,
                    )?;
                    Ok(json!({"phase":"events_processed","report":processed}))
                })();
                receipt(match result {
                    Ok(value) => value,
                    Err(error) => json!({"phase":"failed","error":error}),
                });
            });
        }
    }
}
octosense_shell::octosense_main!();
