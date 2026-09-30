//! The system toolbox as an owner of host-routed tools (ADR 0002 §6, ADR
//! 0004 §12; feature `toolbox-peers`).
//!
//! The toolbox is one more owning app in the relay, [`super::TOOLBOX`]:
//!
//! - its tools are declared once in the catalog
//!   (`octosense_ai_host::toolbox_peers::catalog`: `workflow.run`,
//!   `workflow.fork`, `toolbox.search`, `toolbox.web_read`,
//!   `toolbox.deep_crawl`, each `app: "toolbox"` and `shareable`);
//! - each app is granted exactly the ones it declares AND the person
//!   granted ([`grant_module`], [`grant_manifest`]), so the broker's
//!   registration (after every prepare and reconnect) carries them, marked
//!   with their owner;
//! - they are offered only once the person allowed the app's agent (the
//!   #120 first-use consent, `Catalog::offered`), and the relay refuses a
//!   call before that (`consent_pending`), as for every host tool;
//! - calls reach the toolbox's executor (`ToolboxExecutor`) through the
//!   relay like any in-process app's, which runs them with the calling
//!   app's grant and scope and answers once.

use std::sync::OnceLock;

use serde_json::Value;

use crate::ai_host::toolbox_peers::{self, ToolboxExecutor, ToolboxGrant};

static EXECUTOR: OnceLock<ToolboxExecutor> = OnceLock::new();

/// Where toolbox results live: the host's apps root (`<apps root>/.host/
/// toolbox/<app>`), outside every app's jail.
fn apps_root() -> std::path::PathBuf {
    crate::app_storage::host()
        .map(|s| s.layout().apps_root().to_path_buf())
        .unwrap_or_else(|| crate::octosense::paths::home().join("apps"))
}

/// With the relay's `init`: declare the toolbox's tools and install its
/// executor as the `toolbox` owner's.
pub fn init() {
    let executor = EXECUTOR.get_or_init(|| ToolboxExecutor::shell(&apps_root())).clone();
    super::with_relay(|r| {
        r.catalog.declare(super::TOOLBOX, toolbox_peers::catalog());
        r.set_executor(super::TOOLBOX, Some(std::sync::Arc::new(executor)));
    });
}

/// `app`'s toolbox grant becomes `grant`: the catalog grants it exactly
/// those tools, and the executor runs them with its scope.
pub fn set_grant(app: &str, grant: ToolboxGrant) {
    for note in &grant.notes {
        makepad_widgets::log!("toolbox: {note}");
    }
    let tools: Vec<&str> = match EXECUTOR.get() {
        Some(executor) => executor.set_grant(app, grant).into_iter().collect(),
        None => Vec::new(),
    };
    super::with_relay(|r| r.catalog.set_grants(app, super::TOOLBOX, &tools));
}

/// A native module's grant, from the capabilities it declares (reviewed
/// with the shell), before its instance is offered the assistant.
pub fn grant_module(app: &str, capabilities: &[&str]) {
    set_grant(app, ToolboxGrant::for_module(app, capabilities));
}

/// A script app's grant from its manifest (App Hub #26's shape; system
/// apps only until the shells' App Hub pin verifies it).
pub fn grant_manifest(app: &str, manifest: &Value) {
    set_grant(app, ToolboxGrant::for_manifest(app, manifest));
}

/// `--test-action toolbox-research:<topic>`: a full research run through
/// the toolbox, on this host's apps root (on-device check).
pub fn research_test(topic: &str) {
    toolbox_peers::research_test(&apps_root(), topic.to_string());
}
