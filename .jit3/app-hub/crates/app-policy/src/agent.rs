//! An app's own agent, as its bundle ships it (ADR 0002 §3, §4, §12).
//!
//! Next to `manifest.json` a contained app may ship:
//!
//! - [`TOOLS_FILE`] (`tools.json`): its tool manifest. Each tool is named in
//!   the app's namespace (`news.list`), typed with a small JSON Schema subset
//!   for its input and output, and carries a risk level (read, act,
//!   destructive) and whether it may run in the background or be shared with
//!   other callers. The shell registers exactly these with the app's peer.
//! - [`AGENT_FILE`] (`AGENT.md`): the agent's instructions, named by
//!   `agent.instructions`.
//! - `skills/<name>/{SKILL.md,manifest.json}`: octos skills, named by
//!   `agent.skills`, installed into this app's peer workspace only. For a
//!   contained app they are data-only: a skill manifest that declares
//!   executables, MCP servers or hooks is refused, and a skill may use only
//!   tools the app already has.
//!
//! `tools.json` is the one tool manifest for every app, contained or native.
//! A contained app ships it in its bundle, and the gate checks it here. A
//! native module ships the same file as a module resource, pinned by the
//! shell build; the app-peers broker (OctoSense `crates/app-peers`) loads it
//! with [`ToolManifest::load`] under [`ToolHost::Native`] and builds its
//! `ToolDef`s from it, so Rust only implements executors keyed by tool name.
//! The parser and validator make no assumption about bundles or the Card
//! runner. The risk vocabulary is the broker's ([`Risk::broker_name`]).
//!
//! Every file is under the bundle digest like the rest of the app, so what
//! was reviewed is what runs. [`review`] is what the gate runs over a bundle
//! directory; [`AgentBundle::load`] is what a shell calls on an installed
//! (unpacked) bundle, and refuses anything [`review`] refuses, plus a digest
//! that does not match.
use crate::manifest::{short_id, AppManifest, ModelSpec, Triggers};
use crate::policy::{check_bundle_path, check_skill_name};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The conventional name of the agent's instructions.
pub const AGENT_FILE: &str = "AGENT.md";
/// The app's tool manifest, at the bundle root.
pub const TOOLS_FILE: &str = "tools.json";
/// Where skills live: `skills/<name>/`.
pub const SKILLS_DIR: &str = "skills";
/// A skill's instructions, octos layout.
pub const SKILL_FILE: &str = "SKILL.md";
/// A skill's manifest, octos layout.
pub const SKILL_MANIFEST: &str = "manifest.json";
/// The tool manifest schema this build understands.
pub const TOOLS_SCHEMA: u32 = 1;

/// Ceilings. Instructions and schemas go into a model's context on every
/// run, so they are small by rule.
pub const MAX_INSTRUCTIONS_BYTES: usize = 32 * 1024;
pub const MAX_TOOLS_FILE_BYTES: usize = 64 * 1024;
pub const MAX_TOOLS: usize = 64;
pub const MAX_SCHEMA_BYTES: usize = 8 * 1024;
pub const MAX_SCHEMA_DEPTH: usize = 8;
pub const MAX_TOOL_DESCRIPTION: usize = 1024;
pub const MAX_SKILL_FILE_BYTES: usize = 64 * 1024;
/// The broker's limits: a service id is `[a-z0-9_]{1,24}` and a tool name
/// `[a-z0-9_]{1,32}`.
pub const MAX_NAMESPACE: usize = 24;
pub const MAX_BROKER_TOOL_NAME: usize = 32;

/// How much a tool can break. The same three levels as the app-peers
/// broker's `Risk`; `tools.json` spells them in lowercase and also accepts
/// the broker's capitalised spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// Looks at something. Runs unattended.
    #[serde(alias = "Read")]
    Read,
    /// Changes the app's own state. Runs unattended.
    #[serde(alias = "Act")]
    Act,
    /// Deletes, sends, spends, shares or otherwise reaches past the app.
    /// Always waits for the person's approval.
    #[serde(alias = "Destructive")]
    Destructive,
}

impl Risk {
    /// The broker's spelling (`makepad_ai_services::wire::Risk`).
    pub fn broker_name(self) -> &'static str {
        match self {
            Risk::Read => "Read",
            Risk::Act => "Act",
            Risk::Destructive => "Destructive",
        }
    }
}

/// Where a tool's implementation lives (ADR 0002 §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImplementedBy {
    /// The app's host service: native code that holds the data, devices,
    /// network or secrets. The shell routes the call there with the caller's
    /// identity.
    HostService,
    /// The app's own script, for tools that only reshape the app's data.
    App,
}

/// Who confirms a destructive call with the person. Declared per tool and
/// independent of the risk: the risk says whether a confirmation is needed,
/// this says whose surface asks. Never derived from the risk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confirm {
    /// The host's (kernel's) approval path asks. The default.
    #[default]
    Host,
    /// The app's own confirmation sheet is the confirmation when the person
    /// is present (Rinx's `send_message`), so the host does not ask again.
    /// With the person absent, the call becomes an approval request in the
    /// app's own conversation. Allowed only for tools the app implements
    /// itself (`implemented_by: "app"`) or a native module's tools.
    App,
}

/// How a call is supervised: whether it needs the person comes from the
/// risk alone, and whose surface asks from [`Confirm`]. An app cannot talk
/// its way out of a confirmation, only choose to ask on its own sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Supervision {
    /// Runs without asking, with the person present or not.
    Unattended,
    /// Runs only after the person approves it through the host's approval
    /// path. With the person absent the call becomes an approval request in
    /// the app's conversation.
    HostApproval,
    /// Runs only after the person confirms it on the app's own sheet; the
    /// host does not prompt as well. With the person absent the call becomes
    /// an approval request in the app's conversation.
    AppConfirmation,
}

impl Supervision {
    /// Whether the person must approve before the call runs.
    pub fn needs_person(self) -> bool {
        self != Supervision::Unattended
    }
}

/// Who ships a `tools.json`, which decides the namespace and what `confirm`
/// may say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolHost {
    /// A contained app (a bundle the Card runner runs): the namespace is the
    /// app id's last segment.
    Contained,
    /// A native module: the namespace is the module id. Its tools run in the
    /// module, which may confirm them on its own sheet.
    Native,
}

/// One tool in `tools.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSpec {
    /// `<namespace>.<tool>`, the namespace being the app's short id
    /// ([`short_id`]): `news.list`, `news.topics.get`.
    pub name: String,
    /// What it does and when to use it, for a model.
    pub description: String,
    /// JSON Schema (the subset in [`check_schema`]) for the arguments; an
    /// object.
    pub input_schema: Value,
    /// JSON Schema for the result.
    pub output_schema: Value,
    pub risk: Risk,
    /// May run in a run the person did not start. A destructive tool may say
    /// so, but still only ever runs after approval.
    #[serde(default)]
    pub background: bool,
    /// Callers other than the app's own agent (the system agent, other apps'
    /// agents, the person's assistant) may be granted it.
    #[serde(default)]
    pub shareable: bool,
    /// The result carries the person's private data. Required as `false` on
    /// a shareable tool of an app whose model is `local_only`: such an app
    /// may not hand private data to callers whose models it cannot choose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_data: Option<bool>,
    pub implemented_by: ImplementedBy,
    /// An explicitly reviewed shared host-service method. The public tool name
    /// stays in this app's namespace; the host still dispatches as this app.
    /// Omitted preserves the historical same-name route. Only methods in
    /// [`SHARED_HOST_METHODS`] are eligible; this field never grants a service
    /// or turns a background tool into a foreground approval surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_method: Option<String>,
    /// Whose surface confirms a destructive or outward call. Omitted means
    /// the host.
    #[serde(default, skip_serializing_if = "is_host_confirm")]
    pub confirm: Confirm,
    /// The call reaches outside the device (sends, posts, shares): like a
    /// destructive one, it runs only after the person approves it, live or
    /// by a standing rule (OctoSense ADR 0004 §7, §8). Omitted means `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outward: bool,
    /// A standing rule ("allow for an hour") may approve it. `false` for
    /// permanent deletion, payments, sharing outside the device, account and
    /// security changes: each call then needs the person live (ADR 0004 §8).
    /// Omitted means `true`.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub auto_approvable: bool,
}

fn is_host_confirm(confirm: &Confirm) -> bool {
    *confirm == Confirm::Host
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl ToolSpec {
    /// Dispatch target after bundle validation and the executor's grant check.
    /// Never infer it from arguments or the tool's untrusted result.
    pub fn service_method(&self) -> &str {
        self.host_method.as_deref().unwrap_or(&self.name)
    }

    /// The name without its namespace: `topics.get`.
    pub fn local_name(&self) -> &str {
        self.name.split_once('.').map(|(_, rest)| rest).unwrap_or("")
    }

    /// The name as the broker spells a tool within the app's service:
    /// `[a-z0-9_]`, dots become underscores (`topics_get`).
    pub fn broker_name(&self) -> String {
        self.local_name().replace('.', "_")
    }

    pub fn supervision(&self) -> Supervision {
        let needs_person = self.risk == Risk::Destructive || (self.outward && self.risk != Risk::Read);
        match (needs_person, self.confirm) {
            (false, _) => Supervision::Unattended,
            (true, Confirm::Host) => Supervision::HostApproval,
            (true, Confirm::App) => Supervision::AppConfirmation,
        }
    }

    /// The broker's `confirmed_by_app`: exactly the declared field, never
    /// inferred from the risk.
    pub fn confirmed_by_app(&self) -> bool {
        self.confirm == Confirm::App
    }

    /// Whether a background run may call it without anyone approving.
    pub fn runs_unattended_in_background(&self) -> bool {
        self.background && self.supervision() == Supervision::Unattended
    }
}

/// `tools.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolManifest {
    pub schema: u32,
    pub tools: Vec<ToolSpec>,
}

impl ToolManifest {
    /// Parse, refusing unknown fields and a foreign schema. The rules that
    /// need the owner's identity are in [`ToolManifest::validate`].
    pub fn parse(json: &str) -> Result<Self, String> {
        if json.len() > MAX_TOOLS_FILE_BYTES {
            return Err(format!("{TOOLS_FILE} is {} bytes, over the {MAX_TOOLS_FILE_BYTES} ceiling", json.len()));
        }
        let manifest: ToolManifest = serde_json::from_str(json).map_err(|e| format!("{TOOLS_FILE} is not valid: {e}"))?;
        if manifest.schema != TOOLS_SCHEMA {
            return Err(format!("{TOOLS_FILE} schema {} is not {TOOLS_SCHEMA}", manifest.schema));
        }
        Ok(manifest)
    }

    /// Parse and validate in one call, for any owner: the broker loading a
    /// native module's `tools.json` passes the module id and
    /// [`ToolHost::Native`]. Refusals fail the load; warnings are returned.
    pub fn load(json: &str, namespace: &str, host: ToolHost, local_only: bool) -> Result<(Self, Vec<Issue>), String> {
        let manifest = Self::parse(json)?;
        let issues = manifest.validate(namespace, host, local_only);
        let refusals: Vec<&str> = issues.iter().filter(|i| i.refusal).map(|i| i.detail.as_str()).collect();
        if !refusals.is_empty() {
            return Err(refusals.join("; "));
        }
        Ok((manifest, issues))
    }

    /// Every rule for a contained app's tools, from its manifest.
    pub fn check(&self, manifest: &AppManifest) -> Vec<Issue> {
        let local_only = manifest
            .agent
            .as_ref()
            .and_then(|a| a.model.as_ref())
            .is_some_and(|m| m.local_only);
        self.validate(short_id(&manifest.id), ToolHost::Contained, local_only)
    }

    /// Every rule for one owner's tools: `namespace` is the app's short id
    /// or the native module's id; `local_only` is whether the owner's model
    /// must stay on the person's devices.
    pub fn validate(&self, namespace: &str, host: ToolHost, local_only: bool) -> Vec<Issue> {
        let mut issues = Vec::new();
        if namespace.is_empty()
            || namespace.len() > MAX_NAMESPACE
            || !namespace.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            issues.push(Issue::refuse(
                "tools",
                format!(
                    "the namespace {namespace:?} (a module id, or the last segment of an app id) must be [a-z0-9_]{{1,{MAX_NAMESPACE}}} to declare tools"
                ),
            ));
        }
        if self.tools.is_empty() {
            issues.push(Issue::refuse("tools", format!("{TOOLS_FILE} declares no tools; omit the file instead")));
        }
        if self.tools.len() > MAX_TOOLS {
            issues.push(Issue::refuse("tools", format!("{TOOLS_FILE} declares more than {MAX_TOOLS} tools")));
        }
        let mut names = BTreeSet::new();
        let mut broker_names = BTreeSet::new();
        for tool in &self.tools {
            let name = &tool.name;
            if let Err(error) = check_host_method(tool) {
                issues.push(Issue::refuse("tools", format!("{name}: {error}")));
            }
            if let Err(e) = check_tool_name(name, namespace) {
                issues.push(Issue::refuse("tools", e));
            } else {
                if !names.insert(name.clone()) {
                    issues.push(Issue::refuse("tools", format!("{name} is declared twice")));
                }
                let broker = tool.broker_name();
                if broker.len() > MAX_BROKER_TOOL_NAME {
                    issues.push(Issue::refuse(
                        "tools",
                        format!("{name}: {broker:?} is over the broker's {MAX_BROKER_TOOL_NAME}-character tool name"),
                    ));
                }
                if !broker_names.insert(broker.clone()) {
                    issues.push(Issue::refuse("tools", format!("{name} collides with another tool as {broker:?}")));
                }
            }
            let description = tool.description.trim();
            if description.is_empty() || tool.description.chars().count() > MAX_TOOL_DESCRIPTION {
                issues.push(Issue::refuse("tools", format!("{name}: description must be 1 to {MAX_TOOL_DESCRIPTION} characters")));
            }
            for (which, schema) in [("input_schema", &tool.input_schema), ("output_schema", &tool.output_schema)] {
                if let Err(e) = check_schema(schema) {
                    issues.push(Issue::refuse("tools", format!("{name}: {which}: {e}")));
                }
            }
            if tool.input_schema.get("type").and_then(Value::as_str) != Some("object") {
                issues.push(Issue::refuse("tools", format!("{name}: input_schema must describe an object (\"type\": \"object\")")));
            }
            if tool.risk == Risk::Destructive && tool.background {
                issues.push(Issue::warn(
                    "tools",
                    format!("{name} is destructive and marked background: in a background run it only becomes an approval request, and runs after the person approves"),
                ));
            }
            if tool.confirm == Confirm::App {
                if host == ToolHost::Contained && tool.implemented_by != ImplementedBy::App {
                    issues.push(Issue::refuse(
                        "tools",
                        format!(
                            "{name} says confirm \"app\" but is implemented by the host service: only a tool the app implements itself, or a native module's tool, may confirm on the app's own sheet"
                        ),
                    ));
                } else if tool.supervision().needs_person() {
                    issues.push(Issue::warn(
                        "tools",
                        format!("{name} confirms on the app's own sheet: with the person present that sheet is the only confirmation; with the person absent it becomes an approval request in the app's conversation"),
                    ));
                } else {
                    issues.push(Issue::warn("tools", format!("{name} says confirm \"app\" but is neither destructive nor outward, so nothing is confirmed")));
                }
            }
            if tool.outward && tool.risk == Risk::Read {
                issues.push(Issue::refuse("tools", format!("{name} is outward but its risk is read: a call that reaches outside the device is at least act")));
            }
            if local_only && tool.shareable && tool.private_data != Some(false) {
                issues.push(Issue::refuse(
                    "tools",
                    format!(
                        "{name} is shareable but the app's model is local_only: a shareable tool of such an app must declare \"private_data\": false, and one that returns private data may not be shared"
                    ),
                ));
            }
        }
        issues
    }

    /// The tools of this manifest by risk, for the store's lines.
    pub fn by_risk(&self, risk: Risk) -> Vec<&ToolSpec> {
        self.tools.iter().filter(|t| t.risk == risk).collect()
    }
}

/// Initial shared-service tool contract, deliberately narrower than UI calls.
/// New methods need a reviewed host implementation, account/grant checks and a
/// risk floor here. Provider writes, approval/review controls and account
/// management are not agent aliases. The executor must still check its actual
/// resolved capability and registered handler; admission does not provide one.
pub const SHARED_HOST_METHODS: &[(&str, Risk)] = &[
    ("runtime.list", Risk::Read),
    ("runtime.describe", Risk::Read),
    ("model.capabilities", Risk::Read),
    ("model.video.status", Risk::Read),
    // Generation submits potentially billable work using host-owned provider
    // credentials. Cancellation changes a remote job; none is a read alias.
    ("model.image", Risk::Act),
    ("model.audio", Risk::Act),
    ("model.embeddings", Risk::Act),
    ("model.video", Risk::Act),
    ("model.video.cancel", Risk::Act),
    ("auth.backend.me", Risk::Read),
    // Agent tools always carry may_prompt=false. The backend service permits
    // only declared GET operations in that context; mutations require the
    // foreground native review and are refused before HTTP execution.
    ("auth.backend.request", Risk::Read),
    ("camera.permission.status", Risk::Read),
    ("microphone.permission.status", Risk::Read),
    ("location.permission.status", Risk::Read),
    ("location.get", Risk::Read),
    ("device_calendar.permission.status", Risk::Read),
    ("device_calendar.calendars.list", Risk::Read),
    ("device_calendar.events.list", Risk::Read),
    ("device_calendar.events.get", Risk::Read),
    // Local, app/account-bound draft preparation only. The foreground host
    // review and physical approval remain separate from agent execution.
    ("mail.compose", Risk::Act),
    ("mail.compose_status", Risk::Read),
    ("github.repositories", Risk::Read),
    ("github.files", Risk::Read),
    ("github.read", Risk::Read),
    ("gcalendar.calendars", Risk::Read),
    ("gcalendar.sync", Risk::Read),
    ("gcalendar.refresh", Risk::Read),
    ("gcalendar.cached", Risk::Read),
    ("gcalendar.get", Risk::Read),
    ("gcalendar.prepare", Risk::Read),
    ("gmail.labels", Risk::Read),
    ("gmail.messages", Risk::Read),
    ("gmail.message", Risk::Read),
    ("gmail.draft.open", Risk::Act),
    ("gmail.draft.get", Risk::Read),
    ("gmail.draft.edit", Risk::Act),
    ("gmail.event.decide", Risk::Act),
    ("gmail.event.status", Risk::Read),
    ("glance.publish", Risk::Act),
    ("glance.withdraw", Risk::Act),
    ("glance.list", Risk::Read),
];

fn check_host_method(tool: &ToolSpec) -> Result<(), String> {
    let Some(method) = &tool.host_method else {
        return Ok(());
    };
    if tool.implemented_by != ImplementedBy::HostService {
        return Err("host_method is only valid for implemented_by \"host-service\"".into());
    }
    if method.len() > 96
        || !method.contains('.')
        || method.split('.').any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        })
    {
        return Err(
            "host_method must be family.method with nonempty [a-z0-9_] segments, at most 96 bytes"
                .into(),
        );
    }
    if method.split('.').any(|part| part == "sheet") {
        return Err("host_method cannot target a host sheet or approve an action".into());
    }
    // The app's own WebAssembly function (`wasm.<function>`, contract 1.7):
    // the host runs the bundle's code in a sandbox. A module reaches nothing
    // but its input; a component (`wasm-components-v1`) also reaches the
    // clock, random numbers and, with `storage`, the app's own folder, as
    // the app's script does. Neither reaches shared data, so nothing sets a
    // risk floor or calls for `private_data`. `check` still requires the
    // app to use an available Wasm runtime; capabilities describe usage.
    if method.split('.').count() == 2 && method.starts_with("wasm.") {
        return Ok(());
    }
    let Some((_, minimum)) = SHARED_HOST_METHODS.iter().find(|(name, _)| *name == method) else {
        return Err(format!(
            "host_method {method:?} is not in the reviewed shared-service tool contract"
        ));
    };
    if tool.risk < *minimum {
        return Err(format!(
            "host_method {method:?} requires at least {} risk",
            minimum.broker_name().to_lowercase()
        ));
    }
    if tool.private_data != Some(true) {
        return Err("shared-service tools must declare private_data: true".into());
    }
    Ok(())
}

/// `<namespace>.<segment>[.<segment>…]`, segments `[a-z0-9_]+`.
fn check_tool_name(name: &str, namespace: &str) -> Result<(), String> {
    let Some((head, rest)) = name.split_once('.') else {
        return Err(format!("tool {name:?} must be named {namespace}.<tool>"));
    };
    if head != namespace {
        return Err(format!("tool {name:?} is outside the app's namespace {namespace:?}"));
    }
    if name.len() > 64
        || rest.split('.').any(|seg| seg.is_empty() || !seg.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
    {
        return Err(format!("tool {name:?} must be {namespace}.<segments of [a-z0-9_]>, at most 64 characters"));
    }
    Ok(())
}

/// The JSON Schema keywords a tool schema may use. A subset every model
/// provider accepts and a person can read: no references, no composition,
/// no conditionals. Anything else is refused, not ignored.
pub const SCHEMA_KEYWORDS: &[&str] = &[
    "type", "title", "description", "properties", "required", "items", "enum", "const", "default",
    "minimum", "maximum", "minLength", "maxLength", "minItems", "maxItems", "additionalProperties",
    "format", "pattern",
];

const SCHEMA_TYPES: &[&str] = &["object", "array", "string", "number", "integer", "boolean", "null"];

/// Check one schema against the subset and its size and depth ceilings.
pub fn check_schema(schema: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(schema).map_err(|e| e.to_string())?.len();
    if bytes > MAX_SCHEMA_BYTES {
        return Err(format!("{bytes} bytes, over the {MAX_SCHEMA_BYTES} ceiling"));
    }
    check_schema_node(schema, 0, "#")
}

fn check_schema_node(node: &Value, depth: usize, at: &str) -> Result<(), String> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(format!("{at}: nested deeper than {MAX_SCHEMA_DEPTH}"));
    }
    let Value::Object(map) = node else {
        return Err(format!("{at}: a schema must be an object"));
    };
    for key in map.keys() {
        if !SCHEMA_KEYWORDS.contains(&key.as_str()) {
            return Err(format!("{at}: keyword {key:?} is not in the supported subset {SCHEMA_KEYWORDS:?}"));
        }
    }
    let types: Vec<&str> = match map.get("type") {
        None => return Err(format!("{at}: \"type\" is required")),
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(items)) if !items.is_empty() => {
            let mut out = Vec::new();
            for item in items {
                out.push(item.as_str().ok_or_else(|| format!("{at}: \"type\" entries must be strings"))?);
            }
            out
        }
        Some(_) => return Err(format!("{at}: \"type\" must be a string or a non-empty list of strings")),
    };
    for t in &types {
        if !SCHEMA_TYPES.contains(t) {
            return Err(format!("{at}: type {t:?} is not one of {SCHEMA_TYPES:?}"));
        }
    }
    for key in ["title", "description", "format", "pattern"] {
        if let Some(value) = map.get(key) {
            let text = value.as_str().ok_or_else(|| format!("{at}: {key:?} must be a string"))?;
            if text.len() > MAX_TOOL_DESCRIPTION {
                return Err(format!("{at}: {key:?} is over {MAX_TOOL_DESCRIPTION} characters"));
            }
        }
    }
    for key in ["minimum", "maximum"] {
        if map.get(key).is_some_and(|v| !v.is_number()) {
            return Err(format!("{at}: {key:?} must be a number"));
        }
    }
    for key in ["minLength", "maxLength", "minItems", "maxItems"] {
        if map.get(key).is_some_and(|v| !v.is_u64()) {
            return Err(format!("{at}: {key:?} must be a non-negative integer"));
        }
    }
    if let Some(values) = map.get("enum") {
        match values {
            Value::Array(items) if !items.is_empty() && items.len() <= 64 => {}
            _ => return Err(format!("{at}: \"enum\" must be a list of 1 to 64 values")),
        }
    }
    let mut property_names = BTreeSet::new();
    if let Some(properties) = map.get("properties") {
        let Value::Object(properties) = properties else {
            return Err(format!("{at}: \"properties\" must be an object"));
        };
        for (name, child) in properties {
            property_names.insert(name.as_str());
            check_schema_node(child, depth + 1, &format!("{at}/properties/{name}"))?;
        }
    }
    if let Some(required) = map.get("required") {
        let Value::Array(required) = required else {
            return Err(format!("{at}: \"required\" must be a list"));
        };
        for name in required {
            let name = name.as_str().ok_or_else(|| format!("{at}: \"required\" entries must be strings"))?;
            if !property_names.contains(name) {
                return Err(format!("{at}: \"required\" names {name:?}, which \"properties\" does not declare"));
            }
        }
    }
    if let Some(items) = map.get("items") {
        check_schema_node(items, depth + 1, &format!("{at}/items"))?;
    }
    match map.get("additionalProperties") {
        None | Some(Value::Bool(_)) => {}
        Some(child) => check_schema_node(child, depth + 1, &format!("{at}/additionalProperties"))?,
    }
    Ok(())
}

/// A skill's `manifest.json`, the data-only subset of the octos skill
/// manifest. octos ignores `uses`; the host reads it to hold the skill to
/// the app's tools.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillManifest {
    /// Must equal the skill's directory name.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The tools the skill calls: the app's own (`news.list`) or the generic
    /// ones in `agent.tools`. A skill never widens what the app has.
    #[serde(default)]
    pub uses: Vec<String>,
    /// Markdown in the skill's directory octos adds to the prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts: Option<SkillPrompts>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillPrompts {
    #[serde(default)]
    pub include: Vec<String>,
}

/// octos skill-manifest fields that run something. None is allowed in a
/// contained app's skill: no host policy grants a host-run skill today.
pub const EXECUTABLE_SKILL_FIELDS: &[&str] =
    &["tools", "binaries", "sha256", "mcp_servers", "hooks", "hardware_lifecycle", "tool_discovery", "actions", "make_type"];

/// One skill as installed into the app's peer workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub manifest: SkillManifest,
    /// `SKILL.md`, as text.
    pub skill_md: String,
    /// Every file of the skill directory, relative to it, with its text:
    /// what the shell writes into `skills/<name>/` of the peer workspace.
    pub files: BTreeMap<String, String>,
}

/// An app's agent, as a shell installs it into the app's peer. Loaded only
/// from a bundle whose digest matches its manifest, so every byte here was
/// reviewed and pinned.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentBundle {
    pub app_id: String,
    /// The tools' namespace, the app's short id.
    pub namespace: String,
    /// `AGENT.md`, when the manifest names instructions.
    pub agent_md: Option<String>,
    pub skills: Vec<Skill>,
    /// The app's own tools from `tools.json`; empty when it ships none.
    pub tools: Vec<ToolSpec>,
    /// The generic host tools the manifest names (`agent.tools`).
    pub generic_tools: Vec<String>,
    pub model: ModelSpec,
    /// Requested, not granted: the person decides.
    pub background: bool,
    pub triggers: Triggers,
}

impl AgentBundle {
    /// Load an unpacked bundle's agent: its digest must match the manifest,
    /// and everything [`review`] refuses is refused here too. `Ok(None)` for
    /// an app that asks for no agent and ships no tools.
    pub fn load(bundle: &Path, manifest: &AppManifest) -> Result<Option<AgentBundle>, String> {
        let digest = crate::bundle::digest_dir(bundle)?;
        if !digest.eq_ignore_ascii_case(&manifest.integrity.bundle_blake3) {
            return Err(format!(
                "bundle digest {digest} does not match the manifest's {}: app {}",
                manifest.integrity.bundle_blake3, manifest.id
            ));
        }
        let review = review(bundle, manifest);
        let refusals: Vec<String> = review.refusals().map(|i| i.detail.clone()).collect();
        if !refusals.is_empty() {
            return Err(refusals.join("; "));
        }
        Ok(review.bundle)
    }

    /// The octos kernel tools its agent keeps: the plain names of
    /// `agent.tools`, only ever [`crate::policy::KERNEL_TOOLS`] (the review
    /// refuses any other). A shell hands exactly these to the app's peer.
    pub fn kernel_tools(&self) -> Vec<String> {
        self.generic_tools
            .iter()
            .filter(|t| crate::policy::is_kernel_tool_name(t) && crate::policy::KERNEL_TOOLS.contains(&t.as_str()))
            .cloned()
            .collect()
    }

    /// Every tool the agent may call: its app's own, then the generic ones.
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name.clone()).chain(self.generic_tools.iter().cloned()).collect()
    }

    pub fn tool(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.iter().find(|t| t.name == name)
    }
}

/// A finding about the agent files, in the gate's terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub refusal: bool,
    pub check: &'static str,
    pub detail: String,
}

impl Issue {
    fn refuse(check: &'static str, detail: impl Into<String>) -> Self {
        Issue { refusal: true, check, detail: detail.into() }
    }
    fn warn(check: &'static str, detail: impl Into<String>) -> Self {
        Issue { refusal: false, check, detail: detail.into() }
    }
}

/// What [`review`] found, and the parsed agent when nothing was refused.
#[derive(Debug, Default)]
pub struct Review {
    pub issues: Vec<Issue>,
    pub bundle: Option<AgentBundle>,
}

impl Review {
    pub fn refusals(&self) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(|i| i.refusal)
    }
    pub fn warnings(&self) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(|i| !i.refusal)
    }
}

/// Read and parse `tools.json` if the bundle has one.
pub fn read_tools(bundle: &Path) -> Result<Option<ToolManifest>, String> {
    let path = bundle.join(TOOLS_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("{TOOLS_FILE}: {e}"))?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{TOOLS_FILE} is not UTF-8"))?;
    ToolManifest::parse(&text).map(Some)
}

/// Every rule about a bundle's agent files. Does not check the digest; the
/// gate does that for the whole bundle, and [`AgentBundle::load`] does it
/// before calling this.
pub fn review(bundle: &Path, manifest: &AppManifest) -> Review {
    let mut issues = Vec::new();
    let spec = manifest.agent.as_ref();

    // ---- identity ------------------------------------------------------------
    // A host keys the agent's tools, executor and consent by the app's id and
    // namespace: a reserved one would stand in for a native app or the host.
    if let Err(e) = crate::manifest::check_reserved_id(&manifest.id) {
        issues.push(Issue::refuse("identity", e));
    }

    // ---- tools.json --------------------------------------------------------
    let tools = match read_tools(bundle) {
        Ok(Some(tools)) => {
            issues.extend(tools.check(manifest));
            tools.tools
        }
        Ok(None) => Vec::new(),
        Err(e) => {
            issues.push(Issue::refuse("tools", e));
            Vec::new()
        }
    };
    let tool_names: BTreeSet<&str> = tools.iter().map(|t| t.name.as_str()).collect();

    // ---- AGENT.md ------------------------------------------------------------
    // Agent files are declared, not merely present: an AGENT.md or skills/
    // the manifest does not name would be pinned but never shown as part of
    // what the app asked for.
    let declared_instructions = spec.and_then(|s| s.instructions.as_deref());
    if declared_instructions != Some(AGENT_FILE) && bundle.join(AGENT_FILE).exists() {
        issues.push(Issue::refuse("agent", format!("{AGENT_FILE} is in the bundle but agent.instructions does not name it")));
    }
    let mut agent_md = None;
    if let Some(path) = declared_instructions {
        match check_bundle_path(path) {
            Err(e) => issues.push(Issue::refuse("agent", format!("agent.instructions: {e}"))),
            Ok(()) => match read_instruction_text(&bundle.join(path), path, MAX_INSTRUCTIONS_BYTES) {
                Ok(text) => agent_md = Some(text),
                Err(e) => issues.push(Issue::refuse("agent", e)),
            },
        }
    }

    // ---- skills --------------------------------------------------------------
    let declared_skills: BTreeSet<&str> = spec.map(|s| s.skills.iter().map(String::as_str).collect()).unwrap_or_default();
    let skills_root = bundle.join(SKILLS_DIR);
    if skills_root.exists() {
        match std::fs::read_dir(&skills_root) {
            Err(e) => issues.push(Issue::refuse("skills", format!("{SKILLS_DIR}: {e}"))),
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    if !is_dir {
                        issues.push(Issue::refuse("skills", format!("{SKILLS_DIR}/{name}: only skill directories belong in {SKILLS_DIR}/")));
                    } else if !declared_skills.contains(name.as_str()) {
                        issues.push(Issue::refuse("skills", format!("{SKILLS_DIR}/{name} is in the bundle but agent.skills does not name it")));
                    }
                }
            }
        }
    }
    let generic: BTreeSet<&str> = spec.map(|s| s.tools.iter().map(String::as_str).collect()).unwrap_or_default();
    let mut skills = Vec::new();
    for name in &declared_skills {
        match read_skill(bundle, name) {
            Err(e) => issues.push(Issue::refuse("skills", e)),
            Ok(skill) => {
                for used in &skill.manifest.uses {
                    if !tool_names.contains(used.as_str()) && !generic.contains(used.as_str()) {
                        issues.push(Issue::refuse(
                            "skills",
                            format!("skill {name} uses {used}, which is neither one of the app's tools nor in agent.tools"),
                        ));
                    }
                }
                skills.push(skill);
            }
        }
    }

    // ---- the agent as a whole --------------------------------------------------
    if let Some(spec) = spec {
        // Of the kernel's own tools a contained agent keeps only
        // `ask_user_question` (a question to the person, no side effect).
        for tool in spec.tools.iter().filter(|t| crate::policy::is_kernel_tool_name(t)) {
            if !crate::policy::KERNEL_TOOLS.contains(&tool.as_str()) {
                issues.push(Issue::refuse(
                    "agent",
                    format!(
                        "agent.tools names the octos kernel tool {tool:?}; a contained app's agent may keep only {}",
                        crate::policy::KERNEL_TOOLS.join(", ")
                    ),
                ));
            }
        }
        if spec.background && tools.iter().any(|t| t.risk == Risk::Destructive) {
            issues.push(Issue::warn(
                "agent",
                "a background agent with destructive tools: each destructive call waits as an approval request until the person answers",
            ));
        }
        if !tools.is_empty() && !spec.model.as_ref().is_some_and(|m| m.needs.contains(&crate::manifest::ModelNeed::ToolCalling)) {
            issues.push(Issue::warn("agent", "the app declares tools but its model needs do not include tool_calling"));
        }
    } else if !declared_skills.is_empty() || agent_md.is_some() {
        issues.push(Issue::refuse("agent", "agent files without an agent: declare `agent` in the manifest"));
    }

    let refused = issues.iter().any(|i| i.refusal);
    let bundle = match (refused, spec) {
        (true, _) => None,
        (false, None) if tools.is_empty() => None,
        (false, spec) => Some(AgentBundle {
            app_id: manifest.id.clone(),
            namespace: short_id(&manifest.id).to_string(),
            agent_md,
            skills,
            tools,
            generic_tools: spec.map(|s| s.tools.clone()).unwrap_or_default(),
            model: spec.and_then(|s| s.model.clone()).unwrap_or_default(),
            background: spec.is_some_and(|s| s.background),
            triggers: spec.map(|s| s.triggers.clone()).unwrap_or_default(),
        }),
    };
    Review { issues, bundle }
}

fn read_skill(bundle: &Path, name: &str) -> Result<Skill, String> {
    check_skill_name(name)?;
    let dir = bundle.join(SKILLS_DIR).join(name);
    if !dir.is_dir() {
        return Err(format!("agent.skills names {name}, but {SKILLS_DIR}/{name}/ is not in the bundle"));
    }
    let manifest_path = dir.join(SKILL_MANIFEST);
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .map_err(|_| format!("{SKILLS_DIR}/{name}/{SKILL_MANIFEST} is missing or not UTF-8"))?;
    let raw: Value =
        serde_json::from_str(&manifest_text).map_err(|e| format!("{SKILLS_DIR}/{name}/{SKILL_MANIFEST} is not valid: {e}"))?;
    if let Value::Object(map) = &raw {
        for field in EXECUTABLE_SKILL_FIELDS {
            if map.contains_key(*field) {
                return Err(format!(
                    "skill {name} declares {field:?}: a contained app's skills are data-only (SKILL.md, prompts, uses)"
                ));
            }
        }
    }
    let manifest: SkillManifest =
        serde_json::from_value(raw).map_err(|e| format!("{SKILLS_DIR}/{name}/{SKILL_MANIFEST} is not valid: {e}"))?;
    if manifest.name != name {
        return Err(format!("skill {name}'s manifest is named {:?}; it must match its directory", manifest.name));
    }
    if manifest.version.trim().is_empty() {
        return Err(format!("skill {name} has an empty version"));
    }
    let skill_md = read_instruction_text(&dir.join(SKILL_FILE), &format!("{SKILLS_DIR}/{name}/{SKILL_FILE}"), MAX_INSTRUCTIONS_BYTES)?;
    for include in manifest.prompts.iter().flat_map(|p| p.include.iter()) {
        check_bundle_path(include.trim_start_matches("./")).map_err(|e| format!("skill {name} prompts.include: {e}"))?;
    }
    let mut files = BTreeMap::new();
    collect_skill_files(&dir, &dir, name, &mut files)?;
    Ok(Skill { name: name.to_string(), manifest, skill_md, files })
}

/// A skill directory holds text only: Markdown, JSON and plain text.
fn collect_skill_files(root: &Path, dir: &Path, skill: &str, out: &mut BTreeMap<String, String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{SKILLS_DIR}/{skill}: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
        if kind.is_symlink() {
            return Err(format!("{SKILLS_DIR}/{skill}/{relative}: a skill may not hold a symlink"));
        }
        if kind.is_dir() {
            collect_skill_files(root, &path, skill, out)?;
            continue;
        }
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if !matches!(extension.as_str(), "md" | "json" | "txt") {
            return Err(format!("{SKILLS_DIR}/{skill}/{relative}: a contained app's skill holds only .md, .json and .txt files"));
        }
        let label = format!("{SKILLS_DIR}/{skill}/{relative}");
        out.insert(relative, read_instruction_text(&path, &label, MAX_SKILL_FILE_BYTES)?);
    }
    Ok(())
}

/// Instructions are text a model reads, nothing else: UTF-8, capped, no
/// control characters, nothing a renderer or shell would execute.
fn read_instruction_text(path: &Path, label: &str, cap: usize) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|_| format!("{label} is named but not in the bundle"))?;
    if bytes.len() > cap {
        return Err(format!("{label} is {} bytes, over the {cap} ceiling", bytes.len()));
    }
    let text = String::from_utf8(bytes).map_err(|_| format!("{label} is not UTF-8"))?;
    check_instruction_text(&text).map_err(|e| format!("{label}: {e}"))?;
    Ok(text)
}

/// The executable-content rule for instruction text, exposed for tests and
/// tools that lint a draft before packing.
pub fn check_instruction_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("is empty".into());
    }
    if let Some(c) = text.chars().find(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) {
        return Err(format!("holds control character U+{:04X}", c as u32));
    }
    if text.starts_with("#!") {
        return Err("starts with #!, which makes it a script".into());
    }
    let lower = text.to_ascii_lowercase();
    for needle in ["<script", "<iframe", "<object", "<embed", "javascript:", "vbscript:", "data:text/html"] {
        if lower.contains(needle) {
            return Err(format!("holds {needle:?}; instructions are text, never executable content"));
        }
    }
    Ok(())
}

/// Is this bundle-relative path one of the agent's files (instructions,
/// tool manifest or a skill)? The gate holds their URLs to the app's hosts.
pub fn is_agent_file(relative: &Path, manifest: &AppManifest) -> bool {
    let text = relative.to_string_lossy().replace('\\', "/");
    text == TOOLS_FILE
        || text.starts_with(&format!("{SKILLS_DIR}/"))
        || manifest.agent.as_ref().and_then(|a| a.instructions.as_deref()) == Some(text.as_str())
}
