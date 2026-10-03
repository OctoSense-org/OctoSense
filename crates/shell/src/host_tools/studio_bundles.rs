//! Host-owned developer bundle snapshots. These are local installs, never
//! signed catalog releases. Source trees remain in the author's workspace;
//! admitted bytes and DevTag receipts stay outside every app's storage jail.
use super::studio::scoped::Workspace;
use crate::ai_host::app_peers::host_tools::HostToolCall;
use crate::dev_mode::DevTag;
use octosense_app_policy::{AppManifest, AppPolicy, HostLimits, RefuseAllSignatures};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub const MAX_FILES: usize = 128;
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 512 * 1024;
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
static INSTALL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Identity comes from broker/session stamping, never tool arguments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub app: String,
    pub account: Option<String>,
    pub session: String,
    pub context: Option<String>,
}
impl Owner {
    pub fn of(call: &HostToolCall) -> Self {
        Self {
            app: call.calling_app.clone(),
            account: call.account.clone(),
            session: call.session_id.clone(),
            context: call.context_id.clone(),
        }
    }
}

pub struct Bundle {
    pub id: String,
    pub name: String,
    pub version: String,
    pub digest: String,
    pub root: PathBuf,
    pub source: String,
    pub policy: AppPolicy,
    pub owner: Owner,
    pub dev_tag: DevTag,
    /// Installed snapshots are retained; temporary previews remove theirs.
    retained: std::sync::atomic::AtomicBool,
}
impl Bundle {
    pub fn valid(&self) -> bool {
        crate::dev_mode::tag_valid(&self.dev_tag)
            && crate::dev_mode::grants_all(super::app_of_peer(&self.owner.app))
    }
    pub fn summary(&self) -> Value {
        json!({"app_id":self.id,"name":self.name,"version":self.version,
            "digest":self.digest,"source_modified":false,"admission":"local_developer",
            "capabilities":self.policy.capabilities,"publisher_signed":false})
    }
}
impl Drop for Bundle {
    fn drop(&mut self) {
        if !self.retained.load(std::sync::atomic::Ordering::Acquire) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}
fn base() -> Result<PathBuf, String> {
    crate::octosense::paths::private_dir("studio").map_err(|e| e.to_string())
}
fn private_child(parent: &Path, child: &str) -> Result<PathBuf, String> {
    let path = parent.join(child);
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("studio directory must be private and real".into());
    }
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    Ok(path)
}
fn valid_id(id: &str) -> Result<(), String> {
    if !id.starts_with("dev.studio.")
        || id.len() > 100
        || id.len() <= 11
        || !id.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
        })
    {
        return Err("developer app ids must be under dev.studio.*".into());
    }
    crate::apps::check_script_app_id(id)
}

pub fn stage(
    scope: &Workspace,
    relative: &str,
    owner: Owner,
    tag: DevTag,
) -> Result<Arc<Bundle>, String> {
    let snapshots = private_child(&base()?, "bundles")?;
    let root = snapshots.join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    let result = (|| {
        scope.copy_bundle(relative, &root, MAX_FILES, MAX_BYTES, MAX_FILE_BYTES)?;
        admit_snapshot(root.clone(), owner, tag, None)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&root);
    }
    result.map(Arc::new)
}
fn admit_snapshot(
    root: PathBuf,
    owner: Owner,
    tag: DevTag,
    expected: Option<(&str, &str)>,
) -> Result<Bundle, String> {
    let manifest_path = root.join("manifest.json");
    let text =
        std::fs::read_to_string(&manifest_path).map_err(|e| format!("manifest.json: {e}"))?;
    if text.len() > 64 * 1024 {
        return Err("manifest exceeds 64 KiB".into());
    }
    let mut raw: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if raw
        .pointer("/integrity/signature")
        .is_some_and(|s| !s.is_null())
    {
        return Err(
            "developer bundles must be unsigned; Studio never impersonates a publisher".into(),
        );
    }
    let digest = octosense_app_policy::digest_dir(&root)?;
    if let Some((wanted_digest, wanted_manifest)) = expected {
        if digest != wanted_digest || text != wanted_manifest {
            return Err("installed developer bundle changed after admission".into());
        }
    } else {
        if !raw.is_object() {
            return Err("manifest must be an object".into());
        }
        if raw.get("integrity").is_none() {
            raw["integrity"] = json!({});
        }
        if !raw["integrity"].is_object() {
            return Err("integrity must be an object".into());
        }
        raw["integrity"]["bundle_blake3"] = json!(digest);
    }
    let text = if expected.is_some() {
        text
    } else {
        serde_json::to_string_pretty(&raw).map_err(|e| e.to_string())?
    };
    let manifest = AppManifest::parse(&text)?;
    valid_id(&manifest.id)?;
    if manifest.capabilities.iter().any(|c| c != "storage")
        || !manifest.network.hosts.is_empty()
        || manifest.agent.is_some()
        || manifest.storage.accounts
    {
        return Err("this Studio release accepts offline, storage-only script apps without accounts or agents".into());
    }
    let policy = octosense_app_policy::admit_and_resolve_dir(
        &text,
        &digest,
        &HostLimits::default()
            .with_require_signature(false)
            .with_max_storage_bytes(1024 * 1024)
            .with_max_instruction_budget(5_000_000)
            .with_max_memory_bytes(16 * 1024 * 1024),
        &RefuseAllSignatures,
    )?;
    let source =
        octosense_app_policy::script_source(&root, "").ok_or("bundle needs main.splash")??;
    if source.is_empty() || source.len() > MAX_SOURCE_BYTES {
        return Err("main.splash must be nonempty and at most 64 KiB".into());
    }
    if source.contains("{{assets}}") || source.contains("http_resource(") {
        return Err("this Studio release accepts resource-free script screens; local launcher artwork may still ship in the bundle".into());
    }
    if expected.is_none() {
        std::fs::write(&manifest_path, &text).map_err(|e| e.to_string())?;
    }
    Ok(Bundle {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        digest,
        root,
        source,
        policy,
        owner,
        dev_tag: tag,
        retained: std::sync::atomic::AtomicBool::new(expected.is_some()),
    })
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    owner: Owner,
    profile_id: String,
    since: u64,
    snapshot: String,
    digest: String,
    manifest: String,
}
/// Commit only host-owned, already-admitted bytes. Existing data remains for
/// an update by the same owner; a different conversation cannot take its id.
pub fn install(bundle: &Arc<Bundle>) -> Result<Value, String> {
    let _guard = INSTALL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !bundle.valid() {
        return Err("developer grant expired".into());
    }
    let receipts = private_child(&base()?, "installed")?;
    let path = receipts.join(format!("{}.json", bundle.id));
    let previous = if path.exists() {
        let old: Receipt =
            serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if old.owner != bundle.owner {
            return Err("this app id belongs to another Studio conversation".into());
        }
        Some(old.snapshot)
    } else {
        if std::fs::read_dir(&receipts)
            .map_err(|e| e.to_string())?
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|s| s == "json"))
            .count()
            >= 16
        {
            return Err("Studio supports at most 16 local developer installs".into());
        }
        None
    };
    let receipt = Receipt {
        owner: bundle.owner.clone(),
        profile_id: bundle.dev_tag.profile_id.clone(),
        since: bundle.dev_tag.since,
        snapshot: bundle
            .root
            .file_name()
            .ok_or("invalid snapshot")?
            .to_string_lossy()
            .into_owned(),
        digest: bundle.digest.clone(),
        manifest: std::fs::read_to_string(bundle.root.join("manifest.json"))
            .map_err(|e| e.to_string())?,
    };
    let temp = receipts.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec(&receipt).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    if !bundle.valid() {
        let _ = std::fs::remove_file(&temp);
        return Err("developer grant expired".into());
    }
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())?;
    bundle
        .retained
        .store(true, std::sync::atomic::Ordering::Release);
    if let Some(old) = previous.filter(|old| uuid::Uuid::parse_str(old).is_ok()) {
        let old = bundle.root.parent().unwrap().join(old);
        if old != bundle.root {
            let _ = std::fs::remove_dir_all(old);
        }
    }
    let mut result = bundle.summary();
    result["installed"] = json!(true);
    Ok(result)
}
/// Launcher calls with None; host tools pass the full calling identity.
pub fn installed(id: &str, owner: Option<&Owner>) -> Result<Arc<Bundle>, String> {
    valid_id(id)?;
    let root = base()?;
    let bytes = std::fs::read(root.join("installed").join(format!("{id}.json")))
        .map_err(|e| e.to_string())?;
    if bytes.len() > 128 * 1024 {
        return Err("invalid developer receipt".into());
    }
    let receipt: Receipt = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if owner.is_some_and(|o| o != &receipt.owner) {
        return Err("this app belongs to another Studio conversation".into());
    }
    let tag = DevTag {
        profile_id: receipt.profile_id,
        since: receipt.since,
    };
    if !crate::dev_mode::tag_valid(&tag)
        || !crate::dev_mode::grants_all(super::app_of_peer(&receipt.owner.app))
    {
        return Err("developer grant expired".into());
    }
    uuid::Uuid::parse_str(&receipt.snapshot).map_err(|_| "invalid developer snapshot")?;
    let bundle = admit_snapshot(
        root.join("bundles").join(receipt.snapshot),
        receipt.owner,
        tag,
        Some((&receipt.digest, &receipt.manifest)),
    )?;
    if bundle.id != id {
        return Err("developer receipt identity mismatch".into());
    }
    Ok(Arc::new(bundle))
}
pub fn installed_apps() -> Vec<Arc<Bundle>> {
    let Ok(root) = base() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(root.join("installed")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| e.path().file_stem()?.to_str().map(str::to_owned))
        .filter_map(|id| installed(&id, None).ok())
        .collect()
}
pub fn data_dir(bundle: &Bundle) -> Result<PathBuf, String> {
    if !bundle.valid() {
        return Err("developer grant expired".into());
    }
    private_child(&private_child(&base()?, "data")?, &bundle.id)
}

static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::Acquire)
}
fn owner_key(owner: &Owner) -> String {
    serde_json::to_string(owner).expect("owner serializes")
}
fn open_bundle(bundle: Arc<Bundle>, persistent: bool) -> Result<String, String> {
    if !bundle.valid() {
        return Err("developer grant expired".into());
    }
    let instance_id = uuid::Uuid::new_v4().to_string();
    let jail = if persistent {
        data_dir(&bundle)?
    } else {
        private_child(&private_child(&base()?, "previews")?, &instance_id)?
    };
    let admission = crate::studio::apps::stage_open(crate::studio::apps::OpenSpec {
        instance_id: instance_id.clone(),
        app_id: if persistent {
            Some(bundle.id.clone())
        } else {
            None
        },
        owner: owner_key(&bundle.owner),
        dev_tag: bundle.dev_tag.clone(),
        source: bundle.source.clone(),
        storage_quota: if bundle.policy.allows("storage") {
            bundle.policy.storage_bytes
        } else {
            0
        },
        instruction_budget: bundle.policy.instruction_budget,
        memory_bytes: bundle.policy.memory_bytes,
        jail: jail.clone(),
        persistent,
        title: bundle.name.clone(),
    });
    if let Err(error) = admission {
        if !persistent {
            let _ = std::fs::remove_dir_all(&jail);
        }
        return Err(error);
    }
    Ok(instance_id)
}
/// A person launches a validated installed receipt through the same UI queue.
pub fn request_launch_installed(id: &str) -> Result<String, String> {
    open_bundle(installed(id, None)?, true)
}

pub fn execute(
    call: &HostToolCall,
    root: &Path,
    tag: &DevTag,
    cancel: &std::sync::atomic::AtomicBool,
    reply: &crate::ai_host::app_peers::host_tools::ToolReply,
) -> crate::ai_host::app_peers::host_tools::ToolOutcome {
    use crate::ai_host::app_peers::host_tools::ToolOutcome;
    let run = || -> Result<Value, String> {
        let scope = Workspace::open(root, call.context_id.as_deref())?;
        let owner = Owner::of(call);
        let mut opened: Option<String> = None;
        let mut guard = OpenGuard {
            instance: None,
            owner: owner_key(&owner),
            tag: tag.clone(),
        };
        let mut opened_summary = None;
        let mut output = None;
        let (instance_id, action) = match call.name.as_str() {
            "studio.bundle_check" | "studio.install" => {
                let bundle = stage(
                    &scope,
                    call.args["bundle_path"].as_str().unwrap_or(""),
                    owner,
                    tag.clone(),
                )?;
                if cancel.load(std::sync::atomic::Ordering::Acquire) || !reply.is_open() {
                    return Err("studio_cancelled".into());
                }
                scope.verify(root, call.context_id.as_deref())?;
                if call.name == "studio.install" {
                    let result = install(&bundle)?;
                    GENERATION.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                    makepad_widgets::SignalToUI::set_ui_signal();
                    return Ok(result);
                }
                return Ok(bundle.summary());
            }
            "studio.open" => {
                let path = call.args["bundle_path"].as_str();
                let id = call.args["app_id"].as_str();
                if path.is_some() == id.is_some() {
                    return Err("supply exactly one of bundle_path or app_id".into());
                }
                let persistent = id.is_some();
                let bundle = if let Some(id) = id {
                    installed(id, Some(&owner))?
                } else {
                    stage(&scope, path.unwrap(), owner.clone(), tag.clone())?
                };
                opened_summary = Some(bundle.summary());
                let instance = open_bundle(bundle, persistent)?;
                guard.instance = Some(instance.clone());
                opened = Some(instance.clone());
                (instance, crate::studio::apps::Action::Ready)
            }
            "studio.input" => {
                let kind = call.args["action"].as_str().unwrap_or("");
                let text = call.args["text"].as_str().map(str::to_owned);
                let delta_y = call.args["delta_y"].as_f64();
                if kind == "scroll" && (delta_y.is_none() || text.is_some()) {
                    return Err("scroll requires delta_y and no text".into());
                }
                if kind != "scroll" && delta_y.is_some() {
                    return Err("delta_y is only for scroll".into());
                }
                if kind == "text" && text.is_none() {
                    return Err("text action requires text".into());
                }
                if kind == "tap" && text.is_some() {
                    return Err("tap action takes no text".into());
                }
                (
                    call.args["instance_id"].as_str().unwrap_or("").into(),
                    crate::studio::apps::Action::Input {
                        widget_id: call.args["widget_id"].as_str().unwrap_or("").into(),
                        kind: kind.into(),
                        text,
                        delta_y,
                    },
                )
            }
            "studio.inspect" => {
                let file = scope.output()?;
                let action = crate::studio::apps::Action::Inspect {
                    output: file.file.try_clone().map_err(|e| e.to_string())?,
                };
                output = Some(file);
                (
                    call.args["instance_id"].as_str().unwrap_or("").into(),
                    action,
                )
            }
            "studio.close" => (
                call.args["instance_id"].as_str().unwrap_or("").into(),
                crate::studio::apps::Action::Close,
            ),
            _ => return Err("unsupported Studio operation".into()),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        crate::studio::apps::submit(crate::studio::apps::Request {
            id: call.call_id.clone(),
            owner: owner_key(&owner),
            dev_tag: tag.clone(),
            instance_id: instance_id.clone(),
            action,
            reply: tx,
        })?;
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(
                call.timeout_ms.saturating_sub(500).clamp(1, 25_000),
            );
        let mut result = loop {
            if cancel.load(std::sync::atomic::Ordering::Acquire)
                || !reply.is_open()
                || !crate::dev_mode::tag_valid(tag)
                || !crate::dev_mode::grants_all(super::app_of_peer(&owner.app))
                || std::time::Instant::now() >= deadline
            {
                crate::studio::apps::cancel(&call.call_id);
                return Err("studio_cancelled_or_timed_out".into());
            }
            match rx.recv_timeout(std::time::Duration::from_millis(25)) {
                Ok(result) => break result?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err("studio app runner stopped".into()),
            }
        };
        scope.verify(root, call.context_id.as_deref())?;
        if !crate::dev_mode::tag_valid(tag)
            || !reply.is_open()
            || cancel.load(std::sync::atomic::Ordering::Acquire)
        {
            return Err("studio_cancelled".into());
        }
        if let Some(id) = opened {
            result["instance_id"] = json!(id);
            if let Some(summary) = opened_summary {
                result["bundle"] = summary;
            }
        }
        if let Some(output) = output {
            use std::io::Write;
            result["path"] = json!(output.path());
            // Line-oriented diagnostics let read_file offset/limit recover a
            // bounded section through the kernel's own model-output budget.
            let bytes = serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?;
            if bytes.len() > INSPECT_ARTIFACT_BYTES {
                return Err("studio_snapshot_too_large".into());
            }
            let mut artifact = scope.output_json()?;
            artifact
                .file
                .write_all(&bytes)
                .and_then(|_| artifact.file.sync_all())
                .map_err(|e| e.to_string())?;
            let offset = call
                .args
                .get("offset")
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .ok_or("studio_inspect_offset_invalid")
                })
                .transpose()?
                .unwrap_or(0);
            result = compact_inspect(&result, artifact.path(), offset)?;
            // Both guards unlink on any error, including revocation or a
            // replaced context directory during artifact serialization.
            scope.verify(root, call.context_id.as_deref())?;
            if !crate::dev_mode::tag_valid(tag)
                || !crate::dev_mode::grants_all(super::app_of_peer(&owner.app))
                || !reply.is_open()
                || cancel.load(std::sync::atomic::Ordering::Acquire)
            {
                return Err("studio_cancelled".into());
            }
            output.keep();
            artifact.keep();
        }
        guard.instance = None;
        Ok(result)
    };
    match run() {
        Ok(value) => ToolOutcome::Ok(value),
        Err(why) => ToolOutcome::error("studio_app_failed", why),
    }
}

// The pinned kernel context manager exposes only a 4096-byte prefix of tool
// output to the model. Keep actionable selectors intact inside that budget;
// verbose geometry/tree data lives in the separate caller-scoped artifact.
const INSPECT_REPLY_BYTES: usize = 3800;
const INSPECT_ARTIFACT_BYTES: usize = 1024 * 1024;
fn compact_inspect(full: &Value, snapshot_path: &str, offset: usize) -> Result<Value, String> {
    let widgets: Vec<&Value> = full["snapshot"]["widgets"]
        .as_array()
        .ok_or("studio_snapshot_widgets_missing")?
        .iter()
        .filter(|w| w["visible"] == true && w["type"] != "Splash")
        .collect();
    if offset > widgets.len() {
        return Err("studio_inspect_offset_out_of_range".into());
    }
    let findings = full["snapshot"]["checks"]["findings"].as_array();
    let mut result = json!({
        "instance_id": full["instance_id"], "path": full["path"],
        "snapshot_path": snapshot_path, "width": full["width"], "height": full["height"],
        "settled": full["settled"], "offset": offset, "total_widgets": widgets.len(),
        "next_offset": null,
        "snapshot": {"widgets": [], "checks": {
            "pass": full["snapshot"]["checks"]["pass"],
            "finding_count": findings.map_or(0, |f| f.len()),
            "error_count": findings.map_or(0, |f| f.iter().filter(|v| v["severity"] == "error").count())
        }}
    });
    for (i, widget) in widgets.iter().enumerate().skip(offset) {
        let mut row = json!({"selector":widget["selector"], "type":widget["type"],
            "visible":true, "enabled":widget["enabled"]});
        for field in ["text", "value"] {
            if let Some(text) = widget[field].as_str() {
                let mut end = text.len().min(160);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                row[field] = json!(&text[..end]);
                if end < text.len() {
                    row[format!("{field}_truncated")] = json!(true);
                }
            }
        }
        for field in ["checked", "selected"] {
            if !widget[field].is_null() {
                row[field] = widget[field].clone();
            }
        }
        result["snapshot"]["widgets"]
            .as_array_mut()
            .unwrap()
            .push(row);
        result["next_offset"] = if i + 1 < widgets.len() {
            json!(i + 1)
        } else {
            Value::Null
        };
        if serde_json::to_vec(&result)
            .map_err(|e| e.to_string())?
            .len()
            > INSPECT_REPLY_BYTES
        {
            result["snapshot"]["widgets"].as_array_mut().unwrap().pop();
            result["next_offset"] = json!(i);
            if i == offset {
                return Err("studio_widget_selector_exceeds_reply_budget".into());
            }
            break;
        }
    }
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > INSPECT_REPLY_BYTES
    {
        return Err("studio_inspect_metadata_exceeds_reply_budget".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspect_pages_fit_model_budget_without_losing_unicode_selectors() {
        let widgets: Vec<Value> = (0..100).map(|i| json!({
            "selector":format!("完成任务{i}@0"),"type":"Button", "visible":true,"enabled":true,
            "text":"中文😀\"\n".repeat(1000), "value":"记录".repeat(100), "checked":false
        })).collect();
        let full = json!({"instance_id":"test", "path":".studio-test.png", "width":1080,
            "height":2000,"settled":false,"snapshot":{"widgets":widgets,
            "checks":{"pass":true,"findings":[]},"geometry":{"large":"x".repeat(50000)},"tree":"verbose"}});
        let mut offset = 0;
        let mut seen = Vec::new();
        loop {
            let page = compact_inspect(&full, ".studio-test.json", offset).unwrap();
            let serialized = serde_json::to_vec(&page).unwrap();
            assert!(serialized.len() <= INSPECT_REPLY_BYTES);
            let rows = page["snapshot"]["widgets"].as_array().unwrap();
            assert!(!rows.is_empty());
            for row in rows {
                assert_eq!(row["text_truncated"], true);
                assert_eq!(row["value_truncated"], true);
                seen.push(row["selector"].as_str().unwrap().to_owned());
            }
            match page["next_offset"].as_u64() {
                Some(next) => {
                    assert!(next as usize > offset);
                    offset = next as usize;
                }
                None => break,
            }
        }
        assert_eq!(
            seen,
            (0..100)
                .map(|i| format!("完成任务{i}@0"))
                .collect::<Vec<_>>()
        );
        assert!(compact_inspect(&full, "artifact.json", 101).is_err());
    }
    #[test]
    fn inspect_paging_excludes_hidden_and_source_widgets_and_refuses_unpageable_selector() {
        let mut full = json!({"snapshot":{"widgets":[
            {"type":"Splash","visible":true,"selector":"source@0"},
            {"type":"Button","visible":false,"selector":"hidden@0"},
            {"type":"Button","visible":true,"enabled":false,"selector":"disabled@0","text":"No"}
        ],"checks":{"pass":false,"findings":[{"severity":"error"},{"severity":"warning"}]}}});
        let page = compact_inspect(&full, "artifact.json", 0).unwrap();
        assert_eq!(page["total_widgets"], 1);
        assert_eq!(page["snapshot"]["widgets"][0]["selector"], "disabled@0");
        assert_eq!(page["snapshot"]["checks"]["error_count"], 1);
        assert!(
            compact_inspect(&full, "artifact.json", 1).unwrap()["snapshot"]["widgets"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        full["snapshot"]["widgets"][2]["selector"] = json!("x".repeat(4000));
        assert!(compact_inspect(&full, "artifact.json", 0).is_err());
    }

    fn owner() -> Owner {
        Owner {
            app: "system".into(),
            account: None,
            session: "system-session".into(),
            context: None,
        }
    }
    fn tag() -> DevTag {
        DevTag {
            profile_id: "test-profile".into(),
            since: 1,
        }
    }
    fn fixture(root: &Path, edit: impl FnOnce(&mut Value)) {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(root.join("main.splash"), "View{Label{text: \"authored\"}}").unwrap();
        let mut manifest = json!({"schema":1,"id":"dev.studio.planner","version":"0.1.0","name":"Planner","capabilities":["storage"],"integrity":{"bundle_blake3":"pending"}});
        edit(&mut manifest);
        std::fs::write(root.join("manifest.json"), manifest.to_string()).unwrap();
    }
    #[test]
    fn staging_is_bounded_confined_and_never_stamps_author_source() {
        let home = crate::app_storage::tests::Scratch::new("studio-bundle");
        let author = home.0.join("author");
        fixture(&author, |_| {});
        let original = std::fs::read(author.join("manifest.json")).unwrap();
        let snapshot = home.0.join("copy");
        std::fs::create_dir(&snapshot).unwrap();
        let scope = Workspace::open(&home.0, None).unwrap();
        scope
            .copy_bundle("author", &snapshot, MAX_FILES, MAX_BYTES, MAX_FILE_BYTES)
            .unwrap();
        let bundle = admit_snapshot(snapshot, owner(), tag(), None).unwrap();
        assert_eq!(
            std::fs::read(author.join("manifest.json")).unwrap(),
            original
        );
        assert_ne!(bundle.digest, "pending");
        assert!(bundle.policy.hosts.is_empty());
        assert_eq!(bundle.policy.storage_bytes, 1024 * 1024);
        let manifest = std::fs::read_to_string(bundle.root.join("manifest.json")).unwrap();
        std::fs::write(bundle.root.join("main.splash"), "different").unwrap();
        assert!(admit_snapshot(
            bundle.root.clone(),
            owner(),
            tag(),
            Some((&bundle.digest, &manifest))
        )
        .is_err());
        assert!(scope
            .copy_bundle("../escape", &home.0.join("never"), 10, 1024, 1024)
            .is_err());
        let tiny = home.0.join("tiny");
        std::fs::create_dir(&tiny).unwrap();
        assert!(scope.copy_bundle("author", &tiny, 10, 1, 1).is_err());
    }
    #[test]
    fn local_admission_cannot_claim_system_ids_publishers_network_or_agents() {
        for change in 0..4 {
            let home = crate::app_storage::tests::Scratch::new("studio-refusal");
            fixture(&home.0, |m| match change {
                0 => m["id"] = json!("os.mail"),
                1 => m["integrity"]["signature"] = json!({"key_id":"fake","value":"fake"}),
                2 => m["capabilities"] = json!(["storage", "net"]),
                _ => m["storage"] = json!({"accounts":true}),
            });
            assert!(admit_snapshot(home.0.clone(), owner(), tag(), None).is_err());
        }
    }
    #[test]
    fn snapshot_refuses_symlinks_and_owner_includes_account_and_context() {
        let home = crate::app_storage::tests::Scratch::new("studio-links");
        fixture(&home.0.join("author"), |_| {});
        std::os::unix::fs::symlink("/etc/hosts", home.0.join("author/link")).unwrap();
        let target = home.0.join("copy");
        std::fs::create_dir(&target).unwrap();
        let scope = Workspace::open(&home.0, None).unwrap();
        assert!(scope
            .copy_bundle("author", &target, MAX_FILES, MAX_BYTES, MAX_FILE_BYTES)
            .is_err());
        let a = owner();
        let mut b = a.clone();
        b.context = Some("other".into());
        assert_ne!(owner_key(&a), owner_key(&b));
        b = a.clone();
        b.account = Some("other-account".into());
        assert_ne!(owner_key(&a), owner_key(&b));
        b = a.clone();
        b.session = "different-peer".into();
        assert_ne!(owner_key(&a), owner_key(&b));
    }
}

struct OpenGuard {
    instance: Option<String>,
    owner: String,
    tag: DevTag,
}
impl Drop for OpenGuard {
    fn drop(&mut self) {
        if let Some(id) = &self.instance {
            let _ = crate::studio::apps::abandon(id, &self.owner, &self.tag);
        }
    }
}
