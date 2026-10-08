//! Native engine apps' agent tools (ADR 0013): the Sheets app's `sheets.*`
//! run on the `sheet` host service in this process, exactly as a script
//! app's host-service tools run on theirs
//! ([`super::script_apps::HostServiceExecutor`]), with the engine's own
//! containment (its area under the apps root's `.host`). The relay has
//! already authorized the call — the app's own agent by `agent.own_tools`,
//! the system agent by `agent.system_tools` (`native-apps.json`); this
//! executor only renames `sheets.<tool>` to `sheet.<tool>` and routes it.
//! Without an executor the relay would route a native app's tools to its
//! AI bus service, which the Sheets module does not serve.

use std::collections::{BTreeSet, HashMap};

use crate::ai_host::app_peers::host_tools::{HostToolCall, ToolExecutor, ToolOutcome, ToolReply};

use super::script_apps::HostServiceExecutor;

/// One native app whose declared tools run on an engine's host service.
pub struct EngineExecutor {
    app: &'static str,
    /// The service family the tools rename to (`sheets.*` → `sheet.*`).
    family: &'static str,
}

impl EngineExecutor {
    /// The Sheets app's: its `sheets.*` tools on the `sheet` service
    /// (gridcraft, `apps/sheets/host-service`).
    pub fn sheets() -> Self {
        Self { app: "sheets", family: "sheet" }
    }

    /// The route for one call, bound to the apps root of this moment: the
    /// app's declared tool names, each renamed into the engine's family.
    fn service(&self, host_dir: std::path::PathBuf) -> HostServiceExecutor {
        let tools: BTreeSet<String> = crate::native_apps::find(self.app)
            .and_then(|app| serde_json::from_str::<Vec<serde_json::Value>>(app.tools_json).ok())
            .unwrap_or_default()
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect();
        let methods: HashMap<String, String> = tools
            .iter()
            .filter_map(|name| Some((name.clone(), format!("{}.{}", self.family, name.split_once('.')?.1))))
            .collect();
        HostServiceExecutor {
            app: self.app.to_string(),
            tools,
            methods,
            families: [self.family.to_string()].into(),
            host_dir,
        }
    }
}

impl ToolExecutor for EngineExecutor {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        let Some(root) = octosense_appstore::data_root_if_set() else {
            reply.finish(ToolOutcome::error("engine_unready", "App Hub has no apps root yet"));
            return;
        };
        self.service(root.join(".host")).execute(call, reply);
    }

    fn cancel(&self, call_id: &str) {
        // Cancellation only clears the waiting reply; no path is touched.
        self.service(std::path::PathBuf::new()).cancel(call_id);
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
        let service = executor.service(std::path::PathBuf::new());
        assert_eq!(service.app, "sheets");
        assert!(service.tools.contains("sheets.eval"), "{:?}", service.tools);
        assert_eq!(service.tools.len(), service.methods.len());
        for (name, method) in &service.methods {
            let local = name.strip_prefix("sheets.").expect(name);
            assert_eq!(method, &format!("sheet.{local}"));
        }
        assert!(service.families.contains("sheet"));
    }
}
