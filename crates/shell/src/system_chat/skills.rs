//! The system agent's skills (ADR 0013): one per craft engine linked into
//! this build, which the kernel installs into its skills dir before every
//! start (`octosense_kernel::skills`).
//!
//! A skill teaches the system agent an engine on demand. octos lists each
//! skill's one-line description in the agent's system prompt; the agent
//! reads the skill's `SKILL.md` (what the engine can do, the tools it has
//! for it, the file rules, worked examples) when a request needs that
//! engine, and greps its generated references (`commands.md`,
//! `controls.md`, `functions.md`) instead of loading them. Only the summary
//! lines cost prompt space on every turn.
//!
//! Each engine service embeds its own skill (`apps/<family>/host-service/
//! skill/`, its `skill` module), so the set follows the build's features,
//! with the same gates that link the services (`apps::register_host_services`):
//! the sheet engine comes with `app-hub`, the photo engine and the ten
//! engines behind the system agent's engine tools with the desktop-only
//! `craft-engines` ([`engine_skills`]). An engine that is not linked has no
//! skill, and the kernel removes one an earlier build installed.

/// One linked engine's skill: its name and files.
pub type Skill = (&'static str, &'static [(&'static str, &'static str)]);

/// The skills of the engines linked into this build.
pub fn engine_skills() -> Vec<Skill> {
    #[allow(unused_mut)]
    let mut skills: Vec<Skill> = Vec::new();
    // The sheet engine: the native Sheets app's agent tools run on it
    // wherever App Hub is linked, Home included.
    #[cfg(feature = "app-hub")]
    skills.push((octosense_sheets_service::skill::NAME, octosense_sheets_service::skill::FILES));
    // The photo engine (Photos' `photos.info`) and the ten engines behind
    // the system agent's engine tools (`host_tools::engines::ENGINES`):
    // desktop only.
    #[cfg(feature = "craft-engines")]
    skills.extend([
        (octosense_photo_service::skill::NAME, octosense_photo_service::skill::FILES),
        (octosense_word_service::skill::NAME, octosense_word_service::skill::FILES),
        (octosense_deck_service::skill::NAME, octosense_deck_service::skill::FILES),
        (octosense_cad_service::skill::NAME, octosense_cad_service::skill::FILES),
        (octosense_light_service::skill::NAME, octosense_light_service::skill::FILES),
        (octosense_sound_service::skill::NAME, octosense_sound_service::skill::FILES),
        (octosense_design_service::skill::NAME, octosense_design_service::skill::FILES),
        (octosense_film_service::skill::NAME, octosense_film_service::skill::FILES),
        (octosense_effect_service::skill::NAME, octosense_effect_service::skill::FILES),
        (octosense_vector_service::skill::NAME, octosense_vector_service::skill::FILES),
        (octosense_pdf_service::skill::NAME, octosense_pdf_service::skill::FILES),
    ]);
    skills
}

/// At startup (`system_chat::init`, before any kernel start): this build's
/// skills, for every kernel start from now on.
pub fn init() {
    #[cfg(kernel)]
    {
        use octosense_ai_host::kernel::skills::{set_managed, ManagedSkill};
        let skills = engine_skills().into_iter().map(|(name, files)| ManagedSkill { name, files }).collect();
        for error in set_managed(skills) {
            makepad_widgets::log!("skills: not installed: {error}");
        }
    }
}

#[cfg(all(test, kernel))]
mod tests {
    use super::*;
    use octosense_ai_host::kernel::skills::{frontmatter, sync, validate, ManagedSkill, DESCRIPTION_BUDGET, MARKER};
    use std::collections::BTreeSet;

    fn managed() -> Vec<ManagedSkill> {
        engine_skills().into_iter().map(|(name, files)| ManagedSkill { name, files }).collect()
    }

    /// The system agent's own tools for a skill's engine: its engine tools
    /// (`ENGINE_TOOLS`), the Sheets app's `agent.system_tools` for the sheet
    /// engine, none for the photo engine.
    #[cfg(feature = "app-hub")]
    fn granted(skill: &str) -> BTreeSet<String> {
        match skill {
            "sheet-engine" => crate::native_apps::APPS
                .iter()
                .find(|app| app.id == "sheets")
                .expect("the Sheets app")
                .system_tools
                .iter()
                .map(|t| t.to_string())
                .collect(),
            "photo-engine" => BTreeSet::new(),
            #[cfg(feature = "craft-engines")]
            other => {
                let prefix = format!("{}.", other.strip_suffix("-engine").unwrap());
                crate::system_chat::grants::ENGINE_TOOLS.iter().filter(|t| t.starts_with(&prefix)).map(|t| t.to_string()).collect()
            }
            #[cfg(not(feature = "craft-engines"))]
            other => panic!("{other}: no such skill without craft-engines"),
        }
    }

    /// The tools a `SKILL.md`'s `## Tools` section lists: the backticked
    /// name that starts each bullet.
    #[cfg(feature = "app-hub")]
    fn listed_tools(body: &str) -> BTreeSet<String> {
        let mut in_tools = false;
        let mut tools = BTreeSet::new();
        for line in body.lines() {
            if let Some(heading) = line.strip_prefix("## ") {
                in_tools = heading.trim() == "Tools";
            } else if let Some(rest) = line.strip_prefix("- `").filter(|_| in_tools) {
                tools.insert(rest.split([' ', '`']).next().unwrap_or("").to_owned());
            }
        }
        tools
    }

    /// Each skill is one octos will list as written: a valid frontmatter,
    /// a one-line description within the budget, the name `<family>-engine`.
    #[test]
    fn every_linked_engine_skill_is_valid_and_within_its_budget() {
        let skills = managed();
        let names: BTreeSet<&str> = skills.iter().map(|s| s.name).collect();
        assert_eq!(names.len(), skills.len(), "one skill per engine");
        for skill in &skills {
            assert_eq!(validate(skill), Ok(()), "{}", skill.name);
            assert!(skill.name.ends_with("-engine"), "{}", skill.name);
            let body = skill.files.iter().find(|(p, _)| *p == "SKILL.md").unwrap().1;
            let (_, description) = frontmatter(body).unwrap();
            assert!(description.len() <= DESCRIPTION_BUDGET, "{}: {} bytes", skill.name, description.len());
            assert!(!description.contains("placeholder"), "{}: write its description", skill.name);
        }
    }

    /// Exactly the linked engines have a skill: the sheet engine with App
    /// Hub, the photo engine and the ten with `craft-engines` (the
    /// desktop's default), none without either.
    #[test]
    fn only_linked_engines_have_skills() {
        let names: BTreeSet<&str> = engine_skills().iter().map(|(name, _)| *name).collect();
        let mut expected: BTreeSet<String> = BTreeSet::new();
        #[cfg(feature = "app-hub")]
        expected.insert("sheet-engine".to_owned());
        #[cfg(feature = "craft-engines")]
        expected.insert("photo-engine".to_owned());
        #[cfg(feature = "craft-engines")]
        expected.extend(crate::host_tools::engines::ENGINES.iter().map(|engine| format!("{}-engine", engine.family)));
        assert_eq!(names, expected.iter().map(String::as_str).collect::<BTreeSet<_>>());
        #[cfg(feature = "craft-engines")]
        assert_eq!(names.len(), 12);
    }

    /// A skill's `## Tools` are exactly the system agent's own tools for its
    /// engine: no tool it may not call, none it is granted left out. A grant
    /// change fails here until the skill says so too.
    #[cfg(feature = "app-hub")]
    #[test]
    fn each_skill_lists_exactly_the_tools_the_system_agent_has() {
        for skill in managed() {
            let body = skill.files.iter().find(|(p, _)| *p == "SKILL.md").unwrap().1;
            assert_eq!(listed_tools(body), granted(skill.name), "{}: its ## Tools against the grant", skill.name);
        }
    }

    /// The input schemas of the tools a skill's examples may call, by name:
    /// the Sheets app's for the sheet engine, an engine's own `tools.json`
    /// for the ten, none for the photo engine.
    #[cfg(feature = "app-hub")]
    fn schemas(skill: &str) -> std::collections::BTreeMap<String, serde_json::Value> {
        let from = |tools: &serde_json::Value| -> std::collections::BTreeMap<String, serde_json::Value> {
            tools
                .as_array()
                .unwrap()
                .iter()
                .map(|t| (t["name"].as_str().unwrap().to_owned(), t["input_schema"].clone()))
                .collect()
        };
        match skill {
            "sheet-engine" => {
                let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../../native-apps.json")).unwrap();
                let sheets = manifest["apps"].as_array().unwrap().iter().find(|app| app["id"] == "sheets").expect("the Sheets app");
                from(&sheets["agent"]["tools"])
            }
            "photo-engine" => Default::default(),
            #[cfg(feature = "craft-engines")]
            other => {
                let family = other.strip_suffix("-engine").unwrap();
                let engine = crate::host_tools::engines::ENGINES.iter().find(|e| e.family == family).expect("an engine");
                let manifest: serde_json::Value = serde_json::from_str(engine.tools_json).unwrap();
                from(&manifest["tools"])
            }
            #[cfg(not(feature = "craft-engines"))]
            other => panic!("{other}: no such skill without craft-engines"),
        }
    }

    /// `value` against the parts of a JSON Schema the tools use: types,
    /// enums, declared and required properties, items, and
    /// `additionalProperties` schemas.
    #[cfg(feature = "app-hub")]
    fn check_args(value: &serde_json::Value, schema: &serde_json::Value, at: &str) -> Result<(), String> {
        use serde_json::Value;
        let kind = |v: &Value| match v {
            Value::String(_) => "string",
            Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
            Value::Number(_) => "number",
            Value::Bool(_) => "boolean",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Null => "null",
        };
        let types: Vec<&str> = match &schema["type"] {
            Value::String(t) => vec![t.as_str()],
            Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
            _ => vec![],
        };
        let k = kind(value);
        if !types.is_empty() && !types.contains(&k) && !(k == "integer" && types.contains(&"number")) {
            return Err(format!("{at}: a {k}, the schema takes {types:?}"));
        }
        if let Some(allowed) = schema["enum"].as_array() {
            if !allowed.contains(value) {
                return Err(format!("{at}: {value} is not one of {allowed:?}"));
            }
        }
        if let Some(obj) = value.as_object() {
            let props = schema["properties"].as_object();
            for (key, v) in obj {
                match (props.and_then(|p| p.get(key)), schema.get("additionalProperties")) {
                    (Some(s), _) => check_args(v, s, &format!("{at}.{key}"))?,
                    (None, Some(extra)) if extra.is_object() => check_args(v, extra, &format!("{at}.{key}"))?,
                    (None, _) if props.is_some() => return Err(format!("{at}: `{key}` is not a parameter")),
                    (None, _) => {}
                }
            }
            for required in schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                if !obj.contains_key(required) {
                    return Err(format!("{at}: `{required}` is required"));
                }
            }
        }
        if let (Some(items), Some(schema)) = (value.as_array(), schema.get("items")) {
            for (i, v) in items.iter().enumerate() {
                check_args(v, schema, &format!("{at}[{i}]"))?;
            }
        }
        Ok(())
    }

    /// Every call a skill's `## Examples` shows (`` `<tool> {json}` ``) names
    /// a tool of its engine, with arguments that tool's schema takes, so an
    /// example cannot teach a parameter that does not exist or leave out a
    /// required one.
    #[cfg(feature = "app-hub")]
    #[test]
    fn each_skill_example_calls_its_tools_as_their_schemas_say() {
        for skill in managed() {
            let body = skill.files.iter().find(|(p, _)| *p == "SKILL.md").unwrap().1;
            let schemas = schemas(skill.name);
            let examples = body.split("\n## Examples").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap_or("");
            let mut calls = 0;
            for span in examples.split('`').skip(1).step_by(2) {
                let Some((tool, args)) = span.split_once(' ').filter(|(_, args)| args.starts_with('{')) else { continue };
                let schema = schemas.get(tool).unwrap_or_else(|| panic!("{}: an example calls `{tool}`, not a tool of its engine", skill.name));
                let args: serde_json::Value =
                    serde_json::from_str(args).unwrap_or_else(|e| panic!("{}: `{span}` is not a JSON call: {e}", skill.name));
                if let Err(e) = check_args(&args, schema, tool) {
                    panic!("{}: `{span}`: {e}", skill.name);
                }
                calls += 1;
            }
            assert_eq!(calls == 0, schemas.is_empty(), "{}: {calls} example calls", skill.name);
        }
    }

    /// A skill's `SKILL.md` names its references by their file names, and
    /// every reference it ships is one it names.
    #[test]
    fn each_skill_points_at_the_references_it_ships() {
        for skill in managed() {
            let body = skill.files.iter().find(|(p, _)| *p == "SKILL.md").unwrap().1;
            for (path, _) in skill.files.iter().filter(|(p, _)| *p != "SKILL.md") {
                assert!(body.contains(&format!("`{path}`")), "{}: SKILL.md never mentions `{path}`", skill.name);
            }
        }
    }

    /// The kernel installs this build's skills into a core dir's skills dir,
    /// every file as embedded, and marks them as OctoSense's.
    #[test]
    fn the_kernel_installs_this_builds_skills() {
        let core = std::env::temp_dir().join(format!("octosense-skills-{}", uuid::Uuid::new_v4().simple()));
        let skills = managed();
        let synced = sync(&core, &skills).unwrap();
        assert_eq!(synced.dir, core.join("profiles/_main/data/skills"));
        assert_eq!(synced.current.len(), skills.len());
        assert!(synced.refused.is_empty(), "{:?}", synced.refused);
        for skill in &skills {
            let dir = synced.dir.join(skill.name);
            assert!(dir.join(MARKER).is_file(), "{}", skill.name);
            for (path, body) in skill.files {
                assert_eq!(std::fs::read_to_string(dir.join(path)).unwrap(), *body, "{}/{path}", skill.name);
            }
        }
        let again = sync(&core, &skills).unwrap();
        assert!(again.written.is_empty(), "nothing changed: {again:?}");
        let _ = std::fs::remove_dir_all(core);
    }
}
