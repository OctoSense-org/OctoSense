//! The app: one card bundle in a policy-configured splash isolate.
//!
//! The order is the one ADR 0002 fixes: admit, resolve, apply, then evaluate.
//! Nothing here may widen what the manifest asked for, and the two settings
//! the runtime cannot enforce yet are printed rather than assumed.
use makepad_widgets::*;
use crate::args::Args;
use octosense_app_policy::{admit_and_resolve_dir, AppPolicy, HostLimits, RefuseAllSignatures};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

app_main!(App, font_set: International);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.title: "Card host"
                window.inner_size: vec2(412, 892)
                pass +: { clear_color: #fff }
                body +: {
                    padding: 0 margin: 0 spacing: 0 flow: Overlay
                    card := Splash { width: Fill height: Fill }
                    sheet := Splash { visible: false width: Fill height: Fill }
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    card_surface: SplashRef,
    #[rust]
    host_sheet: SplashRef,
    #[rust]
    mounted: bool,
    /// Set when the bundle was refused: nothing of the bundle runs, and the
    /// window carries the reason instead of an empty frame.
    #[rust]
    refused: bool,
    #[rust]
    assets: Option<octosense_app_policy::AssetServer>,
    #[rust]
    app_id: String,
    #[rust]
    host_dir: PathBuf,
}

/// The command line, parsed by `main` before the app starts.
static ARGS: OnceLock<Args> = OnceLock::new();

/// Start the app with `args`. Returns when the app quits.
pub fn run(args: Args) {
    let _ = ARGS.set(args);
    app_main();
}

/// Read `--static` directories into memory for the life of the process.
fn load_statics(mounts: &[(String, PathBuf)]) -> octosense_app_policy::StaticAssets {
    let mut out: Vec<(&'static str, &'static [u8])> = Vec::new();
    for (prefix, dir) in mounts {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
            out.push((Box::leak(name.into_boxed_str()), Box::leak(bytes.into_boxed_slice())));
        }
    }
    Box::leak(out.into_boxed_slice())
}

/// The size the bundle's manifest asks its window to open at (`window`, in
/// points), which a run without `--size` uses. card-host is a render target,
/// so it does not clamp the hint to the screen, just as it does not clamp
/// `--size`. A manifest that does not parse sizes nothing: it is refused,
/// with its reason, in the default window.
fn manifest_window(bundle: &Path) -> Option<(f64, f64)> {
    let text = std::fs::read_to_string(bundle.join(octosense_app_policy::MANIFEST_FILE)).ok()?;
    let window = octosense_app_policy::AppManifest::parse(&text).ok()?.window?;
    Some((f64::from(window.width), f64::from(window.height)))
}

/// Admit the bundle and resolve what it gets. Refusals are fatal: a card that
/// cannot be admitted must not be drawn, not even partially.
fn policy_for(args: &Args) -> Result<AppPolicy, String> {
    let manifest_path = args.bundle.join(octosense_app_policy::MANIFEST_FILE);
    let mut manifest_json = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let digest = octosense_app_policy::digest_dir(&args.bundle)?;

    if args.stamp {
        let mut value: serde_json::Value = serde_json::from_str(&manifest_json).map_err(|e| e.to_string())?;
        if !value["integrity"]["signature"].is_null() || !value["integrity"]["github"].is_null()
            || value["requires"].as_array().is_some_and(|features| features.iter().any(|f| f == "publisher-github-v1")) {
            return Err("card-host --stamp refuses publisher signing metadata; use an unsigned development copy".into());
        }
        value["integrity"]["bundle_blake3"] = serde_json::Value::String(digest.clone());
        manifest_json = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        std::fs::write(&manifest_path, format!("{manifest_json}\n")).map_err(|e| e.to_string())?;
        log!("card-host: stamped {} with digest {}", manifest_path.display(), digest);
    }

    // A system app's source manifest leaves its digest empty: the build
    // stamps it into the packed copy. Developing one, stamp it in memory.
    if args.system && !args.stamp {
        let mut value: serde_json::Value = serde_json::from_str(&manifest_json).map_err(|e| e.to_string())?;
        if value["integrity"]["bundle_blake3"].as_str().unwrap_or("").is_empty() {
            value["integrity"]["bundle_blake3"] = serde_json::Value::String(digest.clone());
            manifest_json = serde_json::to_string(&value).map_err(|e| e.to_string())?;
        }
    }

    let limits = if args.system {
        HostLimits::system()
    } else {
        HostLimits::default().with_require_signature(!args.allow_unsigned)
    };
    // The digest is computed from the directory, so the manifest's claim is
    // checked against what is actually there.
    admit_and_resolve_dir(&manifest_json, &digest, &limits, &RefuseAllSignatures)
}

/// The card a refusal draws.
///
/// A refused bundle gives the host nothing to draw, which used to leave the
/// window empty with the reason on stdout only. But the author is looking at
/// the window, and a capture of a refusal should be a failure state rather
/// than a blank frame — so the host says why in the window it already opened.
///
/// This is not the bundle's card, and grants it nothing: no manifest is
/// admitted, no isolate is configured, no asset server starts. It is one
/// plain `View` with the reason in it, wrapped by hand because a label does
/// not wrap and a digest, a path or a `refused:` line is one long token.
fn refusal_card(reason: &str) -> String {
    let mut out = String::from(
        "View{width: Fill height: Fill flow: Down padding: Inset{left: 24 right: 24 top: 28 bottom: 24} \
         spacing: 10 show_bg: true draw_bg.color: #xffffff\n\
         \x20   Label{width: Fill max_lines: 1 text: \"card-host refused this bundle\" \
         draw_text.color: #x1c1c1e draw_text.text_style: theme.font_bold{font_size: 16}}\n",
    );
    for line in wrap_script_text(reason, 52, 6) {
        out.push_str(&format!(
            "    Label{{width: Fill max_lines: 1 text: \"{}\" \
             draw_text.color: #x3a3a3c draw_text.text_style.font_size: 13}}\n",
            escape_script_text(&line)
        ));
    }
    out.push_str("}\n");
    out
}

/// `reason` broken into lines of at most `width` characters, at most `lines`
/// of them. A word that fits on a line moves to the next one whole; only a
/// token longer than `width` is broken rather than allowed to run off the
/// card, which is what a digest or a path is.
fn wrap_script_text(text: &str, width: usize, lines: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word = word;
        loop {
            let room = width.saturating_sub(if line.is_empty() { 0 } else { line.chars().count() + 1 });
            if word.chars().count() <= room {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
                break;
            }
            if room > 1 && word.chars().count() > width {
                let take: String = word.chars().take(room).collect();
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(&take);
                word = &word[take.len()..];
            }
            out.push(std::mem::take(&mut line));
            if out.len() == lines {
                // On a full line the ellipsis takes the last character's place.
                let last = out.last_mut().unwrap();
                if last.chars().count() >= width {
                    last.pop();
                }
                last.push('…');
                return out;
            }
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    if out.is_empty() {
        out.push("(no reason given)".to_string());
    }
    out.truncate(lines);
    out
}

/// A string as a Makepad script string literal: the two characters that would
/// end or corrupt it, escaped.
fn escape_script_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ").replace('\t', " ")
}

/// Lower the card to isolate source: realize it, then lower it with the kit
/// that ships in the bundle. Nothing is read from outside the bundle.
///
/// An L0 card also logs one `card-host: realize {json}` line (see
/// [`realize_report`]) before it is lowered, so a harness reading the log —
/// card-studio does — gets the lint result, the realize report and each
/// source's `$state`, whether or not the card then lowers.
fn card_source(bundle: &Path, asset_origin: &str) -> Result<String, String> {
    // A script app runs its own program.
    if let Some(script) = octosense_app_policy::script_source(bundle, asset_origin) {
        return script;
    }
    let card = std::fs::read_to_string(bundle.join("page.card")).map_err(|e| format!("page.card: {e}"))?;
    let data_text = std::fs::read_to_string(bundle.join("page.data.json")).unwrap_or_else(|_| "{}".into());
    let mut data: serde_json::Value = serde_json::from_str(&data_text).map_err(|e| format!("page.data.json: {e}"))?;
    octosense_app_policy::rewrite_assets(&mut data, asset_origin);
    let mut report = realize_report(&card, &data);
    let lowered = lower(&card, &data, bundle, asset_origin);
    match &lowered {
        Ok((_, lowering)) => report["lowering"] = serde_json::Value::String((*lowering).into()),
        Err(e) => report["lower_error"] = serde_json::Value::String(e.clone()),
    }
    log!("card-host: realize {report}");
    // The isolate's prelude opens a `View{`; the body continues it, exactly as
    // a splash app's body does.
    lowered.map(|(ui, _)| format!("width:Fill height:Fill flow:Overlay {ui}"))
}

/// Lower a realized card to Makepad UI, naming the lowering used.
///
/// A card built from a native kit pack (the image-to-card flow's measured
/// placements) goes through the measured-design lowering, as before. A plain
/// L0 card composed from the role kit (`Surface`, `Col`, `TextTitle`, …) has
/// no placements, so the measured lowering refuses it ("unsupported measured
/// design node"); it falls back to the kit lowering the Card runner uses,
/// with inspectable ids (`beauty_0_1_…`) so `/snap` can name every node.
fn lower(card: &str, data: &serde_json::Value, bundle: &Path, asset_origin: &str) -> Result<(String, &'static str), String> {
    let mut prepared = octosense_appstore::card_assets::prepare(card, data, bundle, asset_origin)?;
    match octoscript_makepad::design::to_makepad_ui(&prepared.tree) {
        Ok(ui) => Ok((ui, "design")),
        Err(e) if prepared.native_components => Err(e),
        Err(_) => {
            octoscript_makepad::l0::inspectable(&mut prepared.tree);
            Ok((octoscript_makepad::to_makepad_l0_ui(&prepared.tree), "l0-kit"))
        }
    }
}

/// The card's lint result, realize report and source lifecycle, as JSON.
///
/// This is what ADR 0002 §7 asks a render tool to return next to the frame:
/// `lint` is `check_ui_l0` (valid, level, diagnostics), `realize` is the
/// realization the card is drawn from (`nodes`, `truncated` — a bound was
/// reached and the tree is partial — and diagnostics), and `sources` gives
/// each declared source's `$state` as the card sees it: the host's
/// `$status` entry if it reported one, else `ready` with a value, `pending`
/// without.
fn realize_report(card: &str, data: &serde_json::Value) -> serde_json::Value {
    use octoscript_ui_l0 as l0;
    let diagnostics = |ds: &[l0::SyntaxDiagnostic]| -> Vec<serde_json::Value> {
        ds.iter()
            .map(|d| serde_json::json!({"line": d.line, "column": d.column, "message": d.message}))
            .collect()
    };
    let lint = l0::check_ui_l0(card);
    let realized = l0::realize(card, data, Default::default());
    let sources: Vec<serde_json::Value> = l0::source_plan(card)
        .requests
        .iter()
        .map(|request| {
            let reported = data.get("$status").and_then(|s| s.get(&request.name)).and_then(|v| v.as_str());
            let present = request
                .name
                .split('.')
                .try_fold(data, |value, key| value.get(key))
                .is_some_and(|value| !value.is_null());
            let state = reported.unwrap_or(if present { "ready" } else { "pending" });
            serde_json::json!({"name": request.name, "helper": request.helper, "state": state})
        })
        .collect();
    serde_json::json!({
        "lint": {
            "valid": lint.valid,
            "level": format!("{:?}", lint.level),
            "diagnostics": diagnostics(&lint.diagnostics),
            "diagnostics_truncated": lint.diagnostics_truncated,
        },
        "realize": {
            "nodes": realized.nodes,
            "truncated": realized.truncated,
            "diagnostics": diagnostics(&realized.diagnostics),
        },
        "sources": sources,
    })
}

/// Give every card isolate the kit vocabulary it needs to draw.
///
/// An isolate starts with the standard widgets only, so a lowered L0 card,
/// which names `DesignSurface`, `KitButton` and friends, does not evaluate
/// without this. Note the shape of the hook: it is PROCESS-WIDE, so every
/// isolate gets the same vocabulary. ADR 0002 phase 1 wants this per app,
/// from the manifest; until the runtime can do that, a host must choose one
/// vocabulary for all its cards and keep it to drawing.
fn register_card_vocabulary() {
    use makepad_widgets::widget_async::register_splash_isolate_mod;
    fn design(vm: &mut ScriptVm) {
        octoscript_widgets::design::script_mod(vm);
    }
    fn kit(vm: &mut ScriptVm) {
        octoscript_widgets::kit::script_mod(vm);
    }
    register_splash_isolate_mod(design);
    register_splash_isolate_mod(kit);
    register_splash_isolate_mod(makepad_widgets::splash::register_agent_module);
}

impl App {
    fn mount(&mut self, cx: &mut Cx) {
        let Some(args) = ARGS.get() else {
            error!("card-host: started without its arguments");
            return;
        };
        let manifest_size = if args.size.is_none() { manifest_window(&args.bundle) } else { None };
        if let Some((w, h)) = args.size.or(manifest_size) {
            // A sized window is a render target (card-studio, App Flow's
            // captures at a design's size): the card gets the whole inner
            // size, so the caption bar goes.
            let mut window = self.ui.widget(cx, ids!(main_window));
            script_apply_eval!(cx, window, { show_caption_bar: false });
            let size = dvec2(w, h);
            self.ui.window(cx, ids!(main_window)).configure_window(cx, size, dvec2(40.0, 40.0), false, "Card host".into());
            self.ui.window(cx, ids!(main_window)).resize(cx, size);
            log!("card-host: window sized {w}x{h}");
            if manifest_size.is_some() {
                log!("card-host: the size is the manifest's window; --size overrides it");
            }
        }
        let policy = match policy_for(args) {
            Ok(policy) => policy,
            Err(e) => {
                error!("card-host: refused: {e}");
                // The refusal is right; an empty window is not. Say why in the
                // window the author is already looking at, and stop there:
                // nothing of the bundle is admitted, configured or served.
                self.refused = true;
                self.card_surface.set_text(cx, &refusal_card(&e));
                return;
            }
        };
        log!(
            "card-host: {} {} admitted — capabilities {:?}, hosts {:?}, storage {} bytes, agent {}",
            policy.app_id,
            policy.version,
            policy.capabilities,
            policy.hosts,
            policy.storage_bytes,
            policy.agent.as_ref().map(|a| a.profile.as_kernel_mode()).unwrap_or("none"),
        );

        self.app_id = policy.app_id.clone();
        self.host_dir = args.app_data.join(".host");
        let mut settings = policy.isolate_settings(&args.app_data);
        if let Err(e) = std::fs::create_dir_all(&settings.jail_root) {
            error!("card-host: cannot make the app's jail at {}: {e}", settings.jail_root.display());
            return;
        }
        // The app's artwork is served from a loopback origin of our own,
        // serving only its bundle, and that origin — this port, no other — is
        // the one loopback entry the isolate may reach.
        let server = match octosense_app_policy::AssetServer::start_with_static(&args.bundle, load_statics(&args.statics)) {
            Ok(server) => server,
            Err(e) => {
                error!("card-host: cannot serve the app's artwork: {e}");
                return;
            }
        };
        settings.hosts.push(server.allowlist_entry());
        let origin = server.origin().to_string();
        self.assets = Some(server);
        let splash = self.card_surface.clone();
        let applied = octosense_app_policy::splash_adapter::apply(&splash, cx, &settings);
        log!(
            "card-host: isolate jailed at {} with {} bytes, {} capability(ies), {} host(s), {} instructions, {} bytes of heap, prompts {} — all enforced",
            settings.jail_root.display(),
            applied.storage_quota,
            applied.capabilities,
            applied.hosts,
            applied.instruction_budget,
            applied.memory_bytes,
            settings.host_prompts,
        );

        // The session profile this app's agent would run under. Printed here
        // so the two containers can be compared at a glance; asking the kernel
        // for it is the next phase.
        if let Some(session) = policy.session_profile(&args.app_data) {
            match serde_json::to_string(&session) {
                Ok(json) => log!("card-host: agent session profile {json}"),
                Err(e) => error!("card-host: cannot render the session profile: {e}"),
            }
        }

        let compatible = std::fs::read_to_string(args.bundle.join("manifest.json"))
            .map_err(|error|error.to_string())
            .and_then(|text|octosense_app_policy::AppManifest::parse(&text))
            .and_then(|manifest|octosense_appstore::host_api::check_manifest(&manifest))
            .and_then(|_|octosense_appstore::apply_device_consent(cx, &args.bundle, &splash));
        if let Err(error) = compatible {
            self.refused = true;
            splash.set_text(cx, &refusal_card(&error));
            return;
        }
        match card_source(&args.bundle, &origin) {
            Ok(source) => splash.set_text(cx, &source),
            Err(e) => error!("card-host: the card did not lower: {e}"),
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        octoscript_widgets::design::script_mod(vm);
        octoscript_widgets::kit::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if !self.mounted {
            // Capture trusted surfaces before evaluating any bundle widgets.
            self.card_surface = self.ui.splash(cx, ids!(card));
            self.host_sheet = self.ui.splash(cx, ids!(sheet));
            self.mounted = true;
            register_card_vocabulary();
            self.mount(cx);
        }
        let sheet_up = self.host_sheet.borrow().is_some_and(|sheet| sheet.view.visible);
        if sheet_up && octosense_appstore::services::is_sheet_input_event(event) {
            self.host_sheet.handle_event(cx, event, &mut Scope::empty());
        } else {
            self.ui.handle_event(cx, event, &mut Scope::empty());
        }
        // Host services (a sheet the service raises, answers from its
        // workers), exactly as the Card runner does. A refused bundle has no
        // app to pump for.
        if !self.refused {
            octosense_appstore::services::pump(cx, &self.app_id, &self.host_dir, &self.card_surface, &self.host_sheet);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    thread_local! { static INPUT: RefCell<Vec<String>> = const {RefCell::new(Vec::new())}; }
    script_mod! {
        use mod.prelude.widgets.*
        mod.widgets.HostInputProbe = set_type_default() do #(HostInputProbe::register_widget(vm)) {}
        mod.prelude.widgets.HostInputProbe = mod.widgets.HostInputProbe
    }
    #[derive(Script, ScriptHook, Widget)]
    struct HostInputProbe {
        #[deref] view: View,
        #[live] label: String,
    }
    impl Widget for HostInputProbe {
        fn handle_event(&mut self, _: &mut Cx, event: &Event, _: &mut Scope) {
            if matches!(event, Event::TextInput(_)) {
                INPUT.with(|input| input.borrow_mut().push(self.label.clone()));
            }
        }
        fn draw_walk(&mut self, _: &mut Cx2d, _: &mut Scope, _: Walk) -> DrawStep { DrawStep::done() }
    }

    #[test]
    fn stamp_never_rewrites_publisher_signing_metadata() {
        for (index, metadata) in [
            serde_json::json!({"signature":{}}),
            serde_json::json!({"signature":{"key":"fixture","hex":"00"}}),
            serde_json::json!({"github":{"repository":"example/app","attestation":null}}),
            serde_json::json!({"github":{"repository":"example/app","attestation":{"proof":"fixture"}}}),
        ].into_iter().enumerate() {
            let dir = std::env::temp_dir().join(format!("card-host-sealed-{}-{index}", std::process::id()));
            std::fs::create_dir(&dir).unwrap();
            let path = dir.join("manifest.json");
            let mut manifest = serde_json::json!({"schema":1,"id":"example.fixture","version":"1.0.0","name":"Fixture","integrity":{"bundle_blake3":""}});
            manifest["integrity"].as_object_mut().unwrap().extend(metadata.as_object().unwrap().clone());
            let before = serde_json::to_vec(&manifest).unwrap();
            std::fs::write(&path, &before).unwrap();
            std::fs::write(dir.join("main.splash"), r#"Label{text:"fixture"}"#).unwrap();
            let args = crate::args::parse(["--bundle".into(), dir.to_str().unwrap().into(), "--stamp".into(), "--allow-unsigned".into()]).unwrap();
            let error = policy_for(&args).err().expect("signing metadata must be refused before rewriting");
            assert!(error.contains("signing metadata"), "{error}");
            assert_eq!(std::fs::read(&path).unwrap(), before);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn without_size_the_window_opens_at_the_manifests_hint() {
        let dir = std::env::temp_dir().join(format!("card-host-window-hint-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("manifest.json");
        let manifest = |extra: &str| {
            format!(r#"{{"schema":1,"id":"example.window","version":"1.0.0","name":"Fixture","integrity":{{"bundle_blake3":""}}{extra}}}"#)
        };
        std::fs::write(&path, manifest(r#","schema_minor":1,"window":{"width":1536,"height":1024}"#)).unwrap();
        assert_eq!(manifest_window(&dir), Some((1536.0, 1024.0)));
        // No hint, or a manifest the contract refuses: the default window.
        std::fs::write(&path, manifest("")).unwrap();
        assert_eq!(manifest_window(&dir), None);
        std::fs::write(&path, manifest(r#","window":{"width":1536,"height":1024}"#)).unwrap();
        assert_eq!(manifest_window(&dir), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stamp_preserves_unsigned_null_signature_support() {
        let dir = std::env::temp_dir().join(format!("card-host-unsigned-null-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("manifest.json");
        std::fs::write(&path, r#"{"schema":1,"id":"example.unsigned","version":"1.0.0","name":"Fixture","integrity":{"bundle_blake3":"","signature":null}}"#).unwrap();
        std::fs::write(dir.join("main.splash"), r#"Label{text:"fixture"}"#).unwrap();
        let args = crate::args::parse(["--bundle".into(), dir.to_str().unwrap().into(), "--stamp".into(), "--allow-unsigned".into()]).unwrap();
        policy_for(&args).expect("typed unsigned null-signature manifest must remain runnable");
        let manifest = octosense_app_policy::AppManifest::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(manifest.integrity.signature.is_none());
        assert_eq!(manifest.integrity.bundle_blake3, octosense_app_policy::digest_dir(&dir).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn standalone_sheet_keeps_its_identity_and_exclusive_input_after_guest_mount() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        widget_async::register_splash_isolate_mod(|vm| {script_mod(vm);});
        let mut app = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            let value = script_eval!(vm, {
                use mod.widgets.*
                {ui: View{card := Splash{} sheet := Splash{visible:false}}}
            });
            App::script_from_value(vm, value)
        });
        // Exercise normal first-event reference capture. With no CLI in the
        // test process, mount returns before reading any real bundle/profile.
        app.handle_event(&mut cx, &Event::Custom("mount fixture".into()));
        app.card_surface.set_text(&mut cx, "sheet := View{}\nHostInputProbe{label:\"app\"}");
        app.host_sheet.set_text(&mut cx, "HostInputProbe{label:\"host\"}");
        app.host_sheet.borrow_mut().unwrap().view.visible = true;
        assert_ne!(app.ui.widget(&mut cx, ids!(sheet)).widget_uid(), app.host_sheet.widget_uid());
        INPUT.with(|input| input.borrow_mut().clear());
        app.handle_event(&mut cx, &Event::TextInput(TextInputEvent{input:"sheet input".into(), ..Default::default()}));
        assert_eq!(INPUT.with(|input| input.borrow().clone()), ["host"]);
    }

    #[test]
    fn a_refusal_card_carries_the_reason() {
        let card = refusal_card("app shiyi is unsigned and this host requires a signature");
        assert!(card.starts_with("View{"), "{card}");
        assert!(card.ends_with("}\n"), "{card}");
        assert!(card.contains("card-host refused this bundle"), "{card}");
        assert!(card.contains("unsigned"), "{card}");
    }

    #[test]
    fn a_reason_longer_than_a_line_is_wrapped_not_run_off_the_card() {
        // A digest is one token and no amount of it fits on one line.
        let digest = "a".repeat(64);
        let lines = wrap_script_text(&format!("bundle digest {digest} does not match"), 52, 6);
        assert!(lines.len() > 1, "{lines:?}");
        assert!(lines.iter().all(|l| l.chars().count() <= 52), "{lines:?}");
        assert!(lines.iter().all(|l| !l.trim().is_empty()), "{lines:?}");
        let card = refusal_card(&format!("bundle digest {digest} does not match"));
        assert_eq!(card.matches("text: \"").count(), lines.len() + 1, "{card}");
    }

    #[test]
    fn a_reason_keeps_its_quotes_and_backslashes_inside_the_literal() {
        // Windows paths and JSON-quoted field names both turn up in reasons.
        assert_eq!(escape_script_text(r#"a "b" c\d"#), r#"a \"b\" c\\d"#);
        let card = refusal_card(r#"unknown field "background" at C:\bundle"#);
        assert!(card.contains(r#"\"background\""#), "{card}");
        assert!(card.contains(r"C:\\bundle"), "{card}");
    }

    #[test]
    fn a_reason_never_makes_more_lines_than_the_card_shows() {
        let lines = wrap_script_text(&"word ".repeat(200), 52, 6);
        assert_eq!(lines.len(), 6);
        assert!(lines.last().unwrap().ends_with('…'), "{lines:?}");
    }

    #[test]
    fn an_empty_reason_still_draws_something() {
        let lines = wrap_script_text("   ", 52, 6);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].is_empty());
    }

    #[test]
    fn a_word_that_fits_on_a_line_is_not_split() {
        let lines = wrap_script_text("app shiyi is unsigned and this host requires a signature", 52, 6);
        assert_eq!(lines, ["app shiyi is unsigned and this host requires a", "signature"]);
    }

    #[test]
    fn the_ellipsis_stays_within_the_line() {
        let lines = wrap_script_text(&"a".repeat(400), 52, 6);
        assert_eq!(lines.len(), 6);
        assert!(lines.iter().all(|l| l.chars().count() <= 52), "{lines:?}");
        assert!(lines.last().unwrap().ends_with('…'), "{lines:?}");
    }
}
