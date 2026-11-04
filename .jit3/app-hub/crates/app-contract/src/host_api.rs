//! Versioned host API descriptions and bundle compatibility requirements.
//!
//! Availability is not permission. A described method still checks the caller's
//! resolved grants, account and OS authorization whenever it executes.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct HostApiRequirements {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub required: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub optional: BTreeMap<String, u32>,
}

impl HostApiRequirements {
    pub fn validate(&self) -> Result<(), String> {
        if self.required.len() + self.optional.len() > 128 {
            return Err("host_api declares more than 128 methods".into());
        }
        for (name, version) in self.required.iter().chain(&self.optional) {
            if !valid_method(name) || *version == 0 {
                return Err(format!("invalid host_api method/version: {name}"));
            }
            if name.split('.').any(|part| part == "sheet") {
                return Err("host sheet methods cannot be requested by an app".into());
            }
        }
        if self
            .required
            .keys()
            .any(|key| self.optional.contains_key(key))
        {
            return Err("a host_api method cannot be both required and optional".into());
        }
        Ok(())
    }

    /// Versions are exact ABI major versions, not a minimum host release.
    pub fn check_available(&self, available: &BTreeMap<String, u32>) -> Result<(), String> {
        self.validate()?;
        let missing: Vec<_> = self
            .required
            .iter()
            .filter(|(name, version)| available.get(*name) != Some(*version))
            .map(|(name, version)| format!("{name}@{version}"))
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "this host does not implement required APIs: {}",
                missing.join(", ")
            ))
        }
    }
}

pub fn valid_method(name: &str) -> bool {
    name.len() <= 128
        && name.contains('.')
        && name.split('.').all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum AgentAccess {
    Allowed,
    ForegroundOnly,
    #[default]
    Denied,
}

/// Descriptions are supplied by an implemented service, never by an app.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct HostApiMethod {
    pub name: String,
    pub version: u32,
    pub capability: String,
    pub summary: String,
    pub input_schema: Value,
    pub output_schema: Value,
    /// Empty means the service has not claimed any supported platform.
    pub platforms: Vec<String>,
    pub agent_access: AgentAccess,
}

impl HostApiMethod {
    pub fn new(
        name: impl Into<String>,
        version: u32,
        capability: impl Into<String>,
        summary: impl Into<String>,
        input_schema: Value,
        output_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            version,
            capability: capability.into(),
            summary: summary.into(),
            input_schema,
            output_schema,
            platforms: Vec::new(),
            agent_access: AgentAccess::Denied,
        }
    }
    pub fn with_platforms(mut self, platforms: &[&str]) -> Self {
        self.platforms = platforms.iter().map(|s| (*s).to_owned()).collect();
        self
    }
    pub fn with_agent_access(mut self, access: AgentAccess) -> Self {
        self.agent_access = access;
        self
    }
    pub fn supports(&self, platform: &str) -> bool {
        self.platforms.iter().any(|s| s == platform)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !valid_method(&self.name) || self.version == 0 || self.summary.is_empty() {
            return Err("invalid host API description".into());
        }
        if !crate::KNOWN_CAPABILITIES.contains(&self.capability.as_str()) {
            return Err(format!("unknown host API capability: {}", self.capability));
        }
        if !self.input_schema.is_object() || !self.output_schema.is_object() {
            return Err("host API schemas must be objects".into());
        }
        if self.platforms.iter().any(|s| {
            ![
                "android",
                "macos",
                "linux",
                "windows",
                "ios",
                "openharmony",
                "web",
            ]
            .contains(&s.as_str())
        }) {
            return Err("unknown host API platform".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn required_versions_are_enforced_but_optional_methods_do_not_block() {
        let needs: HostApiRequirements = serde_json::from_value(
            serde_json::json!({"required":{"calendar.list":1},"optional":{"camera.capture":1}}),
        )
        .unwrap();
        assert!(needs.check_available(&BTreeMap::new()).is_err());
        assert!(needs
            .check_available(&BTreeMap::from([("calendar.list".into(), 2)]))
            .is_err());
        assert!(needs
            .check_available(&BTreeMap::from([("calendar.list".into(), 1)]))
            .is_ok());
    }
    #[test]
    fn requirements_cannot_name_sheet_methods_or_ambiguous_names() {
        for name in [
            "calendar.sheet.save",
            "camera..status",
            "camera",
            "camera/status",
        ] {
            let needs = HostApiRequirements {
                required: BTreeMap::from([(name.into(), 1)]),
                ..Default::default()
            };
            assert!(needs.validate().is_err(), "{name}");
        }
    }
}
