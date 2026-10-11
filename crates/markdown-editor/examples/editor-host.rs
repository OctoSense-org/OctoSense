//! Owned native instrument host for the editor and connected-sample UI.
//! --source=<main.splash> --app-data=<temporary directory> --remote
//! This fixture has NO OAuth account and cannot perform provider operations.
//! --fixture-provider exposes synthetic read-only repositories for UI tests.
use makepad_widgets::*;
use std::path::PathBuf;
app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    let app = startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.inner_size: #(if std::env::args().any(|arg| arg == "--wide") {vec2(1200.0, 820.0)} else {vec2(430.0, 850.0)})
                body +: {flow: Down
                    app := Splash {width: Fill height: Fill}
                }
            }
        }
    }
    app
}

#[derive(Script, ScriptHook)]
struct App {
    #[live]
    ui: WidgetRef,
}
impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let arg =
            |name: &str| std::env::args().find_map(|a| a.strip_prefix(name).map(str::to_owned));
        let path = arg("--source=").expect("Pass --source=<main.splash>");
        let root =
            PathBuf::from(arg("--app-data=").expect("Pass --app-data=<temporary directory>"));
        std::fs::create_dir_all(&root).expect("Create fixture state directory");
        let source = std::fs::read_to_string(path).expect("Read the test source");
        let app = self.ui.splash(cx, ids!(app));
        app.set_sandbox_dir(cx, Some(root));
        app.set_host_tag(cx, Some("org.octosense.samples.githubnotes".into()));
        app.set_host_caps(cx, vec!["storage".into(), "auth".into(), "github".into()]);
        app.set_policy(cx, Some(vec![]), Some(50_000_000));
        app.set_text(cx, &source);
    }
}
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        script_eval!(vm, {mod.theme = mod.themes.light});
        makepad_widgets::widgets_mod(vm);
        widget_async::set_splash_theme(widget_async::SplashTheme::Light);
        octosense_markdown_editor::register();
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        for request in splash_host::take_splash_host_requests() {
            let result = fixture_response(&request.service, &request.args_json);
            splash_host::splash_host_respond(
                cx,
                request.heap_key,
                request.req_id,
                result.as_deref().map_err(String::as_str),
            );
            cx.redraw_all();
            SignalToUI::set_ui_signal();
        }
    }
}

fn fixture_response(service: &str, args: &str) -> Result<String, String> {
    use serde_json::{json, Value};
    let fixture = std::env::args().any(|arg| arg == "--fixture-provider");
    if !fixture {
        return if service == "auth.accounts" {
            Ok("[]".into())
        } else {
            Err("Provider services are unavailable in this offline editor test host. Open the installed app in OctoSense to connect.".into())
        };
    }
    let args: Value = serde_json::from_str(args).map_err(|_| "Invalid fixture request")?;
    let result = match service {
        "auth.accounts" => json!([{"provider":"github","handle":"fixture-connection","label":"Example writer · offline fixture"}]),
        "github.repositories" if args["page"] == 1 => json!([
            {"owner":{"login":"sample-writer"},"name":"notes","full_name":"sample-writer/notes","default_branch":"main"},
            {"owner":{"login":"sample-team"},"name":"handbook","full_name":"sample-team/handbook","default_branch":"main"}
        ]),
        "github.repositories" => json!([]),
        "github.files" => json!({"path":args["path"],"branch":args["branch"],"files":[
            {"type":"file","name":"overview.md","path":"overview.md","sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
            {"type":"file","name":"second-note.md","path":"second-note.md","sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
        ]}),
        "github.read" => json!({"path":args["path"],"branch":args["branch"],"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","content": if args["path"] == "overview.md" {"# Team notes\n\nFixture overview from the selected repository.\n"} else {"# Second note\n\nA different file for identity and recovery checks.\n"}}),
        "github.review_save" => return Err("Fixture cannot approve or commit. Simulated conflict: fetch the latest file; your draft is retained.".into()),
        _ => return Err("Offline fixture: no sign-in, approval or provider mutation is performed.".into()),
    };
    Ok(result.to_string())
}
