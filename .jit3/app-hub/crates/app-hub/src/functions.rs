//! The app's own functions, `fns/*.wasm`: WebAssembly core modules
//! (OctoSense ADR 0011) and components (OctoSense ADR 0014).
//!
//! A core module is checked by its header only; the host checks its imports
//! and exports when it loads it. A component is validated here, and its
//! imports must all come from the WASI packages a host scopes to the app
//! ([`ALLOWED_COMPONENT_IMPORTS`]). Whether the manifest may carry one
//! (`requires: ["wasm-components-v1"]`, plus `wasm.sockets@1` for sockets)
//! is the gate's check ([`crate::gate`]), which
//! also tells reviewers what each component reaches
//! ([`ComponentInfo::reach`]).
//!
//! [`describe`] is `hub component-info`: a file's kind, imports and exports,
//! with each function's parameters and result written as WIT writes them
//! (the same text OctoSense's `wasm.functions` shows).

use serde::{Deserialize, Serialize};
use wasmparser::component_types::{ComponentDefinedType, ComponentEntityType, ComponentFuncTypeId, ComponentValType};
use wasmparser::types::TypesRef;
use wasmparser::{ComponentTypeRef, Parser, Payload, PrimitiveValType, TypeBounds, Validator, WasmFeatures};

/// The packages a component may import. Files stay in the app's jail;
/// host-service calls retain consent and account/sharing checks. HTTP and
/// ordinary TCP/UDP/DNS use the host's network under its resource limits.
/// Network declarations describe usage; they are not destination allowlists.
pub const ALLOWED_COMPONENT_IMPORTS: &[&str] =
    &["wasi:cli/", "wasi:clocks/", "wasi:filesystem/", "wasi:http/", "wasi:io/", "wasi:random/", "wasi:sockets/", "octosense:host/"];

/// An extensible host API requirement, not a new manifest `requires` feature:
/// older catalogs must remain readable even when an app needs sockets.
pub const SOCKETS_HOST_API: &str = "wasm.sockets";
pub const SOCKETS_HOST_API_VERSION: u32 = 1;

/// The allowed packages in words: "wasi:cli, wasi:clocks, … and
/// octosense:host".
pub fn allowed_import_packages() -> String {
    let names: Vec<&str> = ALLOWED_COMPONENT_IMPORTS.iter().map(|p| p.trim_end_matches('/')).collect();
    match names.split_last() {
        Some((last, [])) => last.to_string(),
        Some((last, init)) => format!("{} and {last}", init.join(", ")),
        None => String::new(),
    }
}

/// The custom section in which a component lists the crates it is built
/// from (App Flow's `tools/octo wasm build` and its release workflow write
/// it): JSON `{"schema": 1, "crates": [{"name", "version", "source",
/// "checksum"?}]}`. Reviewers see it, and `hub check --advisory-db` checks it
/// against RustSec ([`crate::advisories`]). It says what the publisher's
/// build linked; the gate cannot check it against the code.
pub const CRATES_SECTION: &str = "octosense-crates";

/// One crate a component is built from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Crate {
    pub name: String,
    pub version: String,
    /// `crates.io`, `git+<url>#<commit>`, `path`, or another registry's
    /// source as Cargo writes it.
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

/// The crates a reviewer reads, in one line: "3 crates: getrandom 0.4.0,
/// pulldown-cmark 0.13.4 and wit-bindgen 0.62.0", the first 12 by name.
pub fn crate_line(crates: &[Crate]) -> String {
    const SHOWN: usize = 12;
    let names: Vec<String> = crates.iter().take(SHOWN).map(|c| format!("{} {}", c.name, c.version)).collect();
    let more = crates.len().saturating_sub(SHOWN);
    let count = match crates.len() {
        0 => return "no crates".into(),
        1 => "1 crate".to_string(),
        n => format!("{n} crates"),
    };
    match (names.split_last(), more) {
        (Some((last, [])), 0) => format!("{count}: {last}"),
        (Some((last, init)), 0) => format!("{count}: {} and {last}", init.join(", ")),
        (_, more) => format!("{count}: {} and {more} more", names.join(", ")),
    }
}

#[derive(serde::Deserialize)]
struct CrateList {
    schema: u32,
    crates: Vec<Crate>,
}

/// The feature a manifest requires when its `fns/` holds a component.
pub const COMPONENTS_FEATURE: &str = "wasm-components-v1";

/// The feature a manifest requires when it names shared components from
/// the catalog (`components`, App Hub ADR 0003).
pub const SHARED_COMPONENTS_FEATURE: &str = "wasm-shared-components-v1";

/// Validate `bytes` as a component whose imports a host scopes to its app:
/// what admission requires of an app's own component and of a shared one.
pub fn admissible_component(bytes: &[u8]) -> Result<ComponentInfo, String> {
    let info = inspect_component(bytes)?;
    if let Some(import) = info.refused_imports().first() {
        return Err(format!("the component imports {import}; a component may import only {}", allowed_import_packages()));
    }
    Ok(info)
}

/// Whether `bytes` starts like a WebAssembly core module, version 1.
pub fn is_module(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\0asm\x01\0\0\0")
}

/// Whether `bytes` starts like a WebAssembly component (the preamble's
/// version and layer fields are `0d 00 01 00`).
pub fn is_component(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\0asm\x0d\0\x01\0")
}

/// One function a component exports: `name`, or `interface.name` for a
/// function in an exported interface. Each parameter is `(name, type)`; a
/// core module's parameters are named `p0`, `p1`, ….
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Export {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub result: Option<String>,
}

/// A validated component's imports and exports.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentInfo {
    /// Every top-level import of an interface or a function, as
    /// `package:namespace/interface@version`. A type the world names is
    /// not one: it reaches nothing.
    pub imports: Vec<String>,
    /// Its functions, sorted by name.
    pub exports: Vec<Export>,
    /// The crates it says it is built from ([`CRATES_SECTION`]); `None`
    /// when it does not say.
    pub crates: Option<Vec<Crate>>,
}

impl ComponentInfo {
    /// The imports outside [`ALLOWED_COMPONENT_IMPORTS`].
    pub fn refused_imports(&self) -> Vec<&str> {
        self.imports
            .iter()
            .map(String::as_str)
            .filter(|name| !ALLOWED_COMPONENT_IMPORTS.iter().any(|p| name.starts_with(p)))
            .collect()
    }

    /// Whether it imports `wasi:filesystem`: the app's own jailed files;
    /// `storage` describes this usage.
    pub fn uses_files(&self) -> bool {
        self.imports.iter().any(|name| name.starts_with("wasi:filesystem/"))
    }

    /// Whether it imports `wasi:http`: the network, which the app declares
    /// with the `net` capability.
    pub fn uses_http(&self) -> bool {
        self.imports.iter().any(|name| name.starts_with("wasi:http/"))
    }

    /// Whether it imports ordinary WASI TCP/UDP/DNS interfaces. Even a
    /// network/types-only import requires the sockets host ABI.
    pub fn uses_sockets(&self) -> bool {
        self.imports.iter().any(|name| name.starts_with("wasi:sockets/"))
    }

    /// Socket imports require an explicit host ABI so older hosts can read
    /// the catalog, but refuse this app before installing or running it.
    pub fn missing_host_api(&self, manifest: &octosense_app_policy::AppManifest) -> Option<&'static str> {
        if self.uses_sockets()
            && manifest.host_api.as_ref().and_then(|api| api.required.get(SOCKETS_HOST_API)) != Some(&SOCKETS_HOST_API_VERSION)
        {
            Some("requires host_api.required wasm.sockets@1 (and requires host-api-v1)")
        } else {
            None
        }
    }

    /// Whether it imports `octosense:host`: public host services called with
    /// the same app identity and actual authorization as its script.
    pub fn uses_host_services(&self) -> bool {
        self.imports.iter().any(|name| name.starts_with("octosense:host/"))
    }

    /// What it reaches, for a reviewer: "the clock, files in its app folder
    /// and the network, but no other app".
    pub fn reach(&self) -> String {
        let imports = |prefix: &str| self.imports.iter().any(|n| n.starts_with(prefix));
        let mut reaches = Vec::new();
        if imports("wasi:clocks/") {
            reaches.push("the clock".to_string());
        }
        if imports("wasi:random/") {
            reaches.push("random numbers".to_string());
        }
        if self.uses_files() {
            reaches.push("files in its app folder".to_string());
        }
        let network = self.uses_http() || self.uses_sockets();
        if network {
            reaches.push("the network".to_string());
        }
        if self.uses_host_services() {
            reaches.push("its app's host services".to_string());
        }
        let not = match (self.uses_files(), network) {
            (true, true) => "no other app",
            (true, false) => "no network or other app",
            (false, true) => "no files or other app",
            (false, false) => "no files, network or other app",
        };
        match reaches.as_slice() {
            [] => "nothing but its input".into(),
            [one] => format!("{one}, but {not}"),
            [init @ .., last] => format!("{} and {last}, but {not}", init.join(", ")),
        }
    }
}

/// Validates a component and reads its imports and exports.
pub fn inspect_component(bytes: &[u8]) -> Result<ComponentInfo, String> {
    if !is_component(bytes) {
        return Err("not a WebAssembly component".into());
    }
    let mut validator = Validator::new_with_features(WasmFeatures::default() | WasmFeatures::COMPONENT_MODEL);
    let types = validator
        .validate_all(bytes)
        .map_err(|e| format!("not a valid WebAssembly component: {e}"))?;
    let types = types.as_ref();
    let (imports, export_names, crates) = top_level_names(bytes)?;
    let mut exports = Vec::new();
    for name in export_names {
        match types.component_item_for_export(&name).map(|item| item.ty) {
            Some(ComponentEntityType::Func(id)) => exports.push(function(&types, name.clone(), id)),
            Some(ComponentEntityType::Instance(id)) => {
                let short = short_name(&name).to_string();
                for (fname, item) in types[id].exports.iter() {
                    if let ComponentEntityType::Func(fid) = item.ty {
                        exports.push(function(&types, format!("{short}.{fname}"), fid));
                    }
                }
            }
            _ => {}
        }
    }
    exports.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ComponentInfo { imports, exports, crates })
}

/// `hub component-info`'s answer, in this order: `kind` (`component` or
/// `module`), `imports` and `exports`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Description {
    pub kind: &'static str,
    pub imports: Vec<String>,
    pub exports: Vec<Export>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crates: Option<Vec<Crate>>,
}

/// `hub component-info`: what a function file is, its imports and exports.
pub fn describe(bytes: &[u8]) -> Result<Description, String> {
    if is_component(bytes) {
        let ComponentInfo { imports, exports, crates } = inspect_component(bytes)?;
        return Ok(Description { kind: "component", imports, exports, crates });
    }
    if is_module(bytes) {
        return describe_module(bytes);
    }
    Err("not a WebAssembly core module or component".into())
}

/// A core module's imports (`module.name`) and exported functions with their
/// core types.
fn describe_module(bytes: &[u8]) -> Result<Description, String> {
    let mut validator = Validator::new_with_features(WasmFeatures::default());
    let types = validator
        .validate_all(bytes)
        .map_err(|e| format!("not a valid WebAssembly module: {e}"))?;
    let types = types.as_ref();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| e.to_string())? {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.map_err(|e| e.to_string())?;
                    imports.push(format!("{}.{}", import.module, import.name));
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(|e| e.to_string())?;
                    if export.kind != wasmparser::ExternalKind::Func {
                        continue;
                    }
                    let ty = types.core_function_at(export.index);
                    let func = types[ty].unwrap_func();
                    let params = func.params().iter().enumerate().map(|(i, t)| (format!("p{i}"), t.to_string())).collect();
                    let result = match func.results() {
                        [] => None,
                        [one] => Some(one.to_string()),
                        many => Some(format!("({})", many.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", "))),
                    };
                    exports.push(Export { name: export.name.to_string(), params, result });
                }
            }
            _ => {}
        }
    }
    Ok(Description { kind: "module", imports, exports, crates: None })
}

/// The top-level component's imports, not counting the types it names, and
/// its function and instance exports, in order. Nested modules and
/// components (`parse_all` flattens them) are skipped by depth.
type TopLevel = (Vec<String>, Vec<String>, Option<Vec<Crate>>);

fn top_level_names(bytes: &[u8]) -> Result<TopLevel, String> {
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut crates = None;
    let mut depth = 0usize;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| e.to_string())? {
            Payload::ModuleSection { .. } | Payload::ComponentSection { .. } => depth += 1,
            Payload::End(_) => depth = depth.saturating_sub(1),
            Payload::ComponentImportSection(reader) if depth == 0 => {
                for import in reader {
                    let import = import.map_err(|e| e.to_string())?;
                    // A type the world defines (a record a function returns,
                    // say) is imported as `(type (eq …))`: a name for a type
                    // the component already has, which reaches nothing.
                    if matches!(import.ty, ComponentTypeRef::Type(TypeBounds::Eq(_))) {
                        continue;
                    }
                    imports.push(import.name.name.to_string());
                }
            }
            Payload::CustomSection(reader) if depth == 0 && reader.name() == CRATES_SECTION => {
                if crates.is_some() {
                    return Err(format!("more than one {CRATES_SECTION} section"));
                }
                let list: CrateList = serde_json::from_slice(reader.data())
                    .map_err(|e| format!("its {CRATES_SECTION} section is not a crate list: {e}"))?;
                if list.schema != 1 {
                    return Err(format!("its {CRATES_SECTION} section has schema {}, not 1", list.schema));
                }
                crates = Some(list.crates);
            }
            Payload::ComponentExportSection(reader) if depth == 0 => {
                for export in reader {
                    let export = export.map_err(|e| e.to_string())?;
                    if matches!(
                        export.kind,
                        wasmparser::ComponentExternalKind::Func | wasmparser::ComponentExternalKind::Instance
                    ) {
                        exports.push(export.name.name.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    Ok((imports, exports, crates))
}

fn function(types: &TypesRef<'_>, name: String, id: ComponentFuncTypeId) -> Export {
    let ty = &types[id];
    Export {
        name,
        params: ty
            .params
            .iter()
            .map(|(n, t)| (n.to_string(), wit(types, t)))
            .collect(),
        result: ty.result.as_ref().map(|t| wit(types, t)),
    }
}

/// `my:pkg/markdown@0.1.0` → `markdown`.
fn short_name(full: &str) -> &str {
    let tail = full.rsplit('/').next().unwrap_or(full);
    tail.split('@').next().unwrap_or(tail)
}

fn primitive(p: &PrimitiveValType) -> &'static str {
    match p {
        PrimitiveValType::Bool => "bool",
        PrimitiveValType::S8 => "s8",
        PrimitiveValType::U8 => "u8",
        PrimitiveValType::S16 => "s16",
        PrimitiveValType::U16 => "u16",
        PrimitiveValType::S32 => "s32",
        PrimitiveValType::U32 => "u32",
        PrimitiveValType::S64 => "s64",
        PrimitiveValType::U64 => "u64",
        PrimitiveValType::F32 => "f32",
        PrimitiveValType::F64 => "f64",
        PrimitiveValType::Char => "char",
        PrimitiveValType::String => "string",
        PrimitiveValType::ErrorContext => "error-context",
    }
}

/// A value type as WIT writes it: records, variants and enums by their shape,
/// the same text OctoSense's `wasm.functions` shows. Anything else (a
/// resource handle, a future, a stream, a map) is `resource`, as there.
fn wit(types: &TypesRef<'_>, ty: &ComponentValType) -> String {
    let id = match ty {
        ComponentValType::Primitive(p) => return primitive(p).to_string(),
        ComponentValType::Type(id) => *id,
    };
    let join = |items: Vec<String>| items.join(", ");
    match &types[id] {
        ComponentDefinedType::Primitive(p) => primitive(p).to_string(),
        ComponentDefinedType::Record(record) => format!(
            "record {{ {} }}",
            join(record.fields.iter().map(|(n, t)| format!("{n}: {}", wit(types, t))).collect())
        ),
        ComponentDefinedType::Variant(variant) => format!(
            "variant {{ {} }}",
            join(
                variant
                    .cases
                    .iter()
                    .map(|(n, case)| match &case.ty {
                        Some(t) => format!("{n}({})", wit(types, t)),
                        None => n.to_string(),
                    })
                    .collect()
            )
        ),
        ComponentDefinedType::List { element, .. } => format!("list<{}>", wit(types, element)),
        ComponentDefinedType::Tuple(tuple) => {
            format!("tuple<{}>", join(tuple.types.iter().map(|t| wit(types, t)).collect()))
        }
        ComponentDefinedType::Flags(names) => {
            format!("flags {{ {} }}", join(names.iter().map(|n| n.to_string()).collect()))
        }
        ComponentDefinedType::Enum(names) => {
            format!("enum {{ {} }}", join(names.iter().map(|n| n.to_string()).collect()))
        }
        ComponentDefinedType::Option { ty, .. } => format!("option<{}>", wit(types, ty)),
        ComponentDefinedType::Result { ok, err, .. } => match (ok, err) {
            (Some(ok), Some(err)) => format!("result<{}, {}>", wit(types, ok), wit(types, err)),
            (Some(ok), None) => format!("result<{}>", wit(types, ok)),
            (None, Some(err)) => format!("result<_, {}>", wit(types, err)),
            (None, None) => "result".into(),
        },
        _ => "resource".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(imports: &[&str]) -> ComponentInfo {
        ComponentInfo { imports: imports.iter().map(|i| i.to_string()).collect(), exports: Vec::new(), crates: None }
    }

    #[test]
    fn tcp_udp_and_dns_imports_are_allowed_and_disclosed_as_network() {
        for interface in ["tcp", "tcp-create-socket", "udp", "udp-create-socket", "ip-name-lookup", "network", "instance-network"] {
            let name = format!("wasi:sockets/{interface}@0.2.9");
            let component = info(&[&name]);
            assert!(component.refused_imports().is_empty(), "{interface}");
            assert!(component.uses_sockets(), "{interface}");
            assert_eq!(component.reach(), "the network, but no files or other app");
        }
        assert_eq!(info(&["wasi:sockets-extra/network@0.2.9"]).refused_imports(), ["wasi:sockets-extra/network@0.2.9"]);
    }

    #[test]
    fn the_reach_line_names_what_a_component_imports() {
        assert_eq!(info(&["wasi:cli/stdout@0.2.9"]).reach(), "nothing but its input");
        assert_eq!(
            info(&["wasi:clocks/wall-clock@0.2.9", "wasi:random/random@0.2.9"]).reach(),
            "the clock and random numbers, but no files, network or other app"
        );
        assert_eq!(
            info(&["wasi:filesystem/types@0.2.9"]).reach(),
            "files in its app folder, but no network or other app"
        );
        assert_eq!(
            info(&["wasi:http/outgoing-handler@0.2.4", "wasi:clocks/wall-clock@0.2.9"]).reach(),
            "the clock and the network, but no files or other app"
        );
        assert_eq!(
            info(&["wasi:http/outgoing-handler@0.2.4", "wasi:filesystem/types@0.2.9"]).reach(),
            "files in its app folder and the network, but no other app"
        );
        assert_eq!(
            info(&["octosense:host/services@0.1.0"]).reach(),
            "its app's host services, but no files, network or other app"
        );
        // Any wasi:http interface is the network, even types alone.
        assert_eq!(info(&["wasi:http/types@0.2.4"]).reach(), "the network, but no files or other app");
    }

    #[test]
    fn only_the_scoped_wasi_packages_are_allowed() {
        let component = info(&["wasi:io/poll@0.2.9", "wasi:sockets/network@0.2.9", "wasi:http/types@0.2.9", "my:pkg/host", "octosense:host/services@0.1.0", "octosense:hostile/x"]);
        assert_eq!(component.refused_imports(), ["my:pkg/host", "octosense:hostile/x"]);
        assert!(component.uses_http());
        assert_eq!(
            allowed_import_packages(),
            "wasi:cli, wasi:clocks, wasi:filesystem, wasi:http, wasi:io, wasi:random, wasi:sockets and octosense:host"
        );
        // A package name is matched whole: wasi:clocksmith is not wasi:clocks.
        assert_eq!(info(&["wasi:clocksmith/x"]).refused_imports(), ["wasi:clocksmith/x"]);
    }
}
