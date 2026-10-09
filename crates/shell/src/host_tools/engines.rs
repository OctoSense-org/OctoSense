//! The craft engines' agent tools (ADR 0013): each runs on its engine's
//! host service in this process, exactly as a script app's host-service
//! tools run on theirs ([`super::script_apps::HostServiceExecutor`]), in
//! the area [`super::areas`] picks: a craft engine's tool in the calling
//! agent's own folder (the system agent's workspace, or an app agent's
//! account folder), the Sheets app's own tools in its agent's folder,
//! whoever calls them. The relay has already
//! authorized every call; an [`EngineExecutor`] only routes it. Without one
//! the relay would send a native app's tools to its AI bus service, and
//! would have nowhere to send an engine's.
//!
//! **Sheets** (#382): the native Sheets app's `sheets.*` run on the `sheet`
//! service, renamed into its family (`sheets.eval` → `sheet.eval`). Its own
//! agent may call them by `agent.own_tools`, the system agent by
//! `agent.system_tools` (`native-apps.json`). They work on Sheets' data,
//! in its agent's folder, whoever calls: the system agent's `sheets.get`
//! reads the workbook Sheets' agent opened.
//!
//! **The ten engines** ([`ENGINES`], ADR 0013's agent tools): word, deck,
//! cad, light, sound, design, film, effect, vector and pdf. Each ships its
//! tool manifest with its service (`apps/<family>/host-service/tools.json`,
//! the crate's `TOOLS_JSON`), read here with App Hub's own loader, the one
//! a bundle's `tools.json` goes through. No app ships them yet, so the
//! shell declares them under a **virtual owner**, `os.<family>`:
//!
//! - It is not an app. It has no bundle, no app agent, no peer and no
//!   Settings row (`apps::agent_apps` lists apps, not owners), and nothing
//!   to consent to: the system agent's calls are authorized by its reviewed
//!   grant alone (`system_chat::grants::ENGINE_TOOLS`), as its Calendar
//!   tools are. None of the tools is shareable, so no app's agent can be
//!   granted one. The generic engine command doors are declared but held
//!   back from that grant ([`HELD_FOR_REVIEW`]).
//! - It is admitted as the shell's own compiled-in service
//!   (`admission::check`): no catalog entry or bundle exists to withdraw.
//!   No path loads a bundle for it: `ensure_loaded` skips an owner the
//!   catalog knows, and a load that fails anyway (`script_apps::load`)
//!   changes nothing before it has an admitted bundle.
//! - Its calls run as `os.<family>` on the `<family>` service, each tool
//!   on the method of its own name, in the caller's own folder (ADR 0013,
//!   9 Oct 2026): every path is relative to it, in the tools' answers and
//!   in their errors ([`super::areas::relative_errors`]). The system agent
//!   works in its workspace, so an engine reads what the agent holds and
//!   writes there, never replacing a file.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
#[cfg(feature = "craft-engines")]
use std::sync::Arc;

#[cfg(feature = "craft-engines")]
use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest, ToolSpec};

use crate::ai_host::app_peers::host_tools::{HostToolCall, ToolExecutor, ToolOutcome, ToolReply};

use super::script_apps::{AreaSource, HostServiceExecutor};

/// One engine whose host service the system agent reaches through a
/// virtual owner.
#[cfg(feature = "craft-engines")]
pub struct Engine {
    /// Its service family (`word`): its tools' namespace.
    pub family: &'static str,
    /// Its `tools.json`, as its service crate ships it.
    pub tools_json: &'static str,
}

/// The ten engines, in the order their services register
/// (`apps::register_host_services`).
#[cfg(feature = "craft-engines")]
pub const ENGINES: &[Engine] = &[
    Engine { family: "word", tools_json: octosense_word_service::TOOLS_JSON },
    Engine { family: "deck", tools_json: octosense_deck_service::TOOLS_JSON },
    Engine { family: "cad", tools_json: octosense_cad_service::TOOLS_JSON },
    Engine { family: "light", tools_json: octosense_light_service::TOOLS_JSON },
    Engine { family: "sound", tools_json: octosense_sound_service::TOOLS_JSON },
    Engine { family: "design", tools_json: octosense_design_service::TOOLS_JSON },
    Engine { family: "film", tools_json: octosense_film_service::TOOLS_JSON },
    Engine { family: "effect", tools_json: octosense_effect_service::TOOLS_JSON },
    Engine { family: "vector", tools_json: octosense_vector_service::TOOLS_JSON },
    Engine { family: "pdf", tools_json: octosense_pdf_service::TOOLS_JSON },
];

/// Declared, never in the system agent's grant: the generic engine command
/// doors, which run any command of an engine's catalog. Their services
/// fence file access, but what a command can do is the whole engine's
/// surface, so they are reviewed separately before anyone is granted one.
#[cfg(feature = "craft-engines")]
pub const HELD_FOR_REVIEW: &[&str] = &["effect.run", "vector.run"];

#[cfg(feature = "craft-engines")]
impl Engine {
    /// Its virtual owner, `os.<family>`.
    pub fn owner(&self) -> String {
        format!("{}{}", octosense_appstore::system::SYSTEM_ID_PREFIX, self.family)
    }

    /// Its tools as App Hub's loader admits them: named in its family,
    /// each run on its service by the method of its own name.
    pub fn tools(&self) -> Result<Vec<ToolSpec>, String> {
        let (manifest, _) = ToolManifest::load(self.tools_json, self.family, ToolHost::Contained, false)?;
        if let Some(tool) = manifest.tools.iter().find(|t| t.implemented_by != ImplementedBy::HostService || t.host_method.is_some()) {
            return Err(format!("{} must run on the {} service by its own name", tool.name, self.family));
        }
        Ok(manifest.tools)
    }
}

/// Whether `app` is an engine's virtual owner (`os.word`).
#[cfg(feature = "craft-engines")]
pub fn is_virtual_owner(app: &str) -> bool {
    app.strip_prefix(octosense_appstore::system::SYSTEM_ID_PREFIX)
        .is_some_and(|family| ENGINES.iter().any(|engine| engine.family == family))
}

/// At startup, once the engines' services are registered: every engine's
/// tools declared under its virtual owner, with its executor. An engine
/// whose manifest App Hub's loader refuses is left out, and says why.
#[cfg(feature = "craft-engines")]
pub fn register() {
    super::with_relay(|relay| install(relay, None));
}

/// [`register`] into `relay`: `areas` gives the agents' areas (a test's);
/// `None`, the shell's.
#[cfg(feature = "craft-engines")]
pub(crate) fn install(relay: &mut super::Relay, areas: AreaSource) {
    for engine in ENGINES {
        let owner = engine.owner();
        match engine.tools() {
            Ok(tools) => {
                relay.catalog.declare(&owner, tools.iter().map(super::script_apps::declaration).collect());
                relay.set_executor(&owner, Some(Arc::new(EngineExecutor::engine(engine, &tools).with_areas(areas.clone()))));
            }
            Err(e) => makepad_widgets::log!("host tools: {owner}'s tools were refused: {e}"),
        }
    }
}

/// The declared tools of one owner, run on an engine's host service, in
/// the calling agent's area.
pub struct EngineExecutor {
    /// The owning app the service sees: a native app (`sheets`) or an
    /// engine's virtual owner (`os.word`).
    app: String,
    /// The service family the tools run on (`sheet`, `word`).
    family: String,
    /// Each declared tool's service method.
    methods: HashMap<String, String>,
    /// Where the agents' areas come from: the shell (`None`), or a test.
    areas: AreaSource,
}

impl EngineExecutor {
    /// The Sheets app's: its `sheets.*` tools on the `sheet` service
    /// (gridcraft, `apps/sheets/host-service`), each renamed into it.
    pub fn sheets() -> Self {
        let tools: BTreeSet<String> = crate::native_apps::find("sheets")
            .and_then(|app| serde_json::from_str::<Vec<serde_json::Value>>(app.tools_json).ok())
            .unwrap_or_default()
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect();
        let methods = tools.iter().filter_map(|name| Some((name.clone(), format!("sheet.{}", name.split_once('.')?.1)))).collect();
        EngineExecutor { app: "sheets".into(), family: "sheet".into(), methods, areas: Default::default() }
    }

    /// An engine's, as its virtual owner: each tool on its own method.
    #[cfg(feature = "craft-engines")]
    fn engine(engine: &Engine, tools: &[ToolSpec]) -> Self {
        let methods = tools.iter().map(|tool| (tool.name.clone(), tool.service_method().to_string())).collect();
        EngineExecutor { app: engine.owner(), family: engine.family.into(), methods, areas: Default::default() }
    }

    /// The same, with the agents' areas from `areas` (a test's).
    #[cfg(any(test, feature = "craft-engines"))]
    pub(crate) fn with_areas(mut self, areas: AreaSource) -> Self {
        self.areas = areas;
        self
    }

    /// The route for one call. An engine's method never reads its host
    /// directory: the service works in the calling agent's area, which the
    /// route resolves ([`HostServiceExecutor::run`]).
    fn service(&self) -> HostServiceExecutor {
        HostServiceExecutor {
            app: self.app.clone(),
            tools: self.methods.keys().cloned().collect(),
            methods: self.methods.clone(),
            families: [self.family.clone()].into(),
            host_dir: PathBuf::new(),
        }
    }
}

impl ToolExecutor for EngineExecutor {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        // Every method here works in an area; one that would not is refused
        // rather than run with no folder of its own.
        if let Some(method) = self.methods.get(&call.name).filter(|m| !super::areas::needs_area(m)) {
            reply.finish(ToolOutcome::error("engine_unready", format!("{method} is no engine method")));
            return;
        }
        self.service().run(call, reply, self.areas.clone());
    }

    fn cancel(&self, call_id: &str) {
        // Cancellation only clears the waiting reply; no path is touched.
        self.service().cancel(call_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every declared Sheets tool renames into the `sheet` family, so the
    /// engine's service answers exactly the declared set.
    #[test]
    fn sheets_tools_rename_into_the_sheet_family() {
        let executor = EngineExecutor::sheets();
        let service = executor.service();
        assert_eq!(service.app, "sheets");
        assert!(service.tools.contains("sheets.eval"), "{:?}", service.tools);
        assert_eq!(service.tools.len(), service.methods.len());
        for (name, method) in &service.methods {
            let local = name.strip_prefix("sheets.").expect(name);
            assert_eq!(method, &format!("sheet.{local}"));
        }
        assert!(service.families.contains("sheet"));
    }

    /// Each engine's tools load with App Hub's loader and route, as
    /// `os.<family>`, to the method of their own name on their own family:
    /// the service sees a system app's identity, never the caller's; the
    /// caller decides only where it works (its area, `areas.rs`), every
    /// method of the family working in one.
    #[cfg(feature = "craft-engines")]
    #[test]
    fn each_engine_routes_its_tools_as_its_virtual_owner_on_its_own_family() {
        assert_eq!(ENGINES.len(), 10);
        for engine in ENGINES {
            let tools = engine.tools().unwrap_or_else(|e| panic!("{}: {e}", engine.family));
            let service = EngineExecutor::engine(engine, &tools).service();
            assert_eq!(service.app, format!("os.{}", engine.family));
            assert!(is_virtual_owner(&service.app));
            assert_eq!(service.families, [engine.family.to_string()].into());
            assert_eq!(service.tools.len(), tools.len());
            for tool in &tools {
                assert!(tool.name.starts_with(&format!("{}.", engine.family)), "{}", tool.name);
                assert_eq!(service.methods[&tool.name], tool.name, "a tool runs on the method of its own name");
                assert!(super::super::areas::needs_area(&tool.name), "{} works in its caller's area", tool.name);
                assert!(!tool.shareable, "{}: only the system agent's grant reaches an engine", tool.name);
                assert!(!tool.outward, "{}: an engine never reaches past the device", tool.name);
            }
        }
        for not_owner in ["os.mail", "os.calendar", "os.photos", "word", "org.example.word", "os.", "os.sheet"] {
            assert!(!is_virtual_owner(not_owner), "{not_owner}");
        }
    }

    /// The system agent's engine grant is exactly every declared engine
    /// tool but the held command doors, each a read or an act (an act only
    /// creates files in the agent's own workspace, never replacing one:
    /// each service's tests); none is destructive or outward, and no door
    /// reaches the grant.
    #[cfg(feature = "craft-engines")]
    #[test]
    fn the_system_grant_is_every_engine_tool_but_the_held_doors() {
        use octosense_app_policy::Risk;
        use crate::system_chat::grants::{self, ENGINE_TOOLS};
        let granted: BTreeSet<String> = ENGINE_TOOLS.iter().map(|tool| tool.to_string()).collect();
        assert_eq!(granted.len(), ENGINE_TOOLS.len(), "no tool granted twice");
        let mut declared = BTreeSet::new();
        for engine in ENGINES {
            for tool in engine.tools().unwrap() {
                assert!(matches!(tool.risk, Risk::Read | Risk::Act), "{} is {:?}", tool.name, tool.risk);
                declared.insert(tool.name);
            }
        }
        let held: BTreeSet<String> = HELD_FOR_REVIEW.iter().map(|tool| tool.to_string()).collect();
        assert!(held.is_subset(&declared));
        assert_eq!(granted, &declared - &held);
        let now = grants::host_tools();
        for tool in &granted {
            assert!(!tool.ends_with(".run"), "{tool} is a command door");
            assert!(grants::is_engine_tool(tool) && now.contains(tool), "{tool}");
        }
        for door in HELD_FOR_REVIEW {
            assert!(!now.contains(*door) && !grants::is_engine_tool(door), "{door}");
        }
    }

    /// The command doors are declared by their engines, so the relay can
    /// refuse them by name (`not_granted`) rather than as unknown tools.
    #[cfg(feature = "craft-engines")]
    #[test]
    fn the_held_doors_are_declared_tools() {
        for door in HELD_FOR_REVIEW {
            let family = door.split('.').next().unwrap();
            let engine = ENGINES.iter().find(|e| e.family == family).unwrap();
            assert!(engine.tools().unwrap().iter().any(|t| t.name == *door), "{door}");
        }
    }

    /// No engine ships as an app yet. When one does, its bundle's
    /// `tools.json` owns the namespace: retire its virtual owner then,
    /// rather than let both declare `os.<family>`.
    #[cfg(feature = "craft-engines")]
    #[test]
    fn no_shipped_app_takes_an_engines_virtual_owner() {
        let apps = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
        for packaging in ["../../desktop/system-apps.json", "../../phone/system-apps.json"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(packaging);
            let list: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for engine in ENGINES {
                assert!(!list["apps"].as_array().unwrap().iter().any(|app| app == engine.family), "{packaging} ships {}", engine.family);
            }
        }
        for engine in ENGINES {
            assert!(!apps.join(engine.family).join("bundle").exists(), "apps/{}/bundle exists: retire os.{}", engine.family, engine.family);
            assert!(crate::native_apps::find(engine.family).is_none(), "a native app named {}", engine.family);
        }
    }
}
