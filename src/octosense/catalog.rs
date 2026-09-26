use crate::clients::{AppDef, LaunchPolicy};
use makepad_strict_json::{self as json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub fn parse_catalog(
    bytes: &[u8],
    base: &Path,
    makepad_root: Option<&Path>,
) -> Result<Vec<AppDef>, String> {
    let value = json::parse(bytes).map_err(|e| format!("invalid JSON: {e}"))?;
    let rows = value.as_arr().ok_or("catalog must be a JSON array")?;
    let mut ids = HashSet::new();
    let mut out = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        // `None` is a row this host cannot launch, not a broken catalog.
        let parse = || -> Result<Option<AppDef>, String> {
            let Value::Obj(fields) = row else {
                return Err("entry must be an object".into());
            };
            for (key, _) in fields {
                if ![
                    "id",
                    "label",
                    "manifest",
                    "source",
                    "package",
                    "bin",
                    "executable",
                    "args",
                    "policy",
                ]
                .contains(&key.as_str())
                {
                    return Err(format!("unknown field '{key}'"));
                }
            }
            let required = |key| {
                row.get(key)
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_string)
                    .ok_or_else(|| format!("'{key}' must be a nonempty string"))
            };
            let id = required("id")?;
            if !id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
            {
                return Err("id must contain only lowercase letters, digits, '-' or '_'".into());
            }
            let label = required("label")?;
            let manifest = row.get("manifest").is_some();
            let executable = row.get("executable").is_some();
            let source = row.get("source").is_some();
            if [manifest, executable, source].iter().filter(|set| **set).count() != 1 {
                return Err(
                    "specify exactly one of 'manifest', 'executable' and 'source'".into(),
                );
            }
            let path = |value: String| base.join(value).to_string_lossy().into_owned();
            let mut target_dir = None;
            let (manifest, package, bin) = if source {
                // A named upstream checkout, not a path: the row travels with
                // the repository and resolves wherever Cargo placed the
                // revision this build pins.
                let name = required("source")?;
                if name != "makepad" {
                    return Err(format!("unknown source '{name}'"));
                }
                let Some(root) = makepad_root else {
                    return Ok(None);
                };
                target_dir = Some(
                    super::paths::home()
                        .join("build")
                        .join(name)
                        .to_string_lossy()
                        .into_owned(),
                );
                (
                    Some(root.join("Cargo.toml").to_string_lossy().into_owned()),
                    required("package")?,
                    required("bin")?,
                )
            } else if manifest {
                (
                    Some(path(required("manifest")?)),
                    required("package")?,
                    required("bin")?,
                )
            } else {
                if row.get("package").is_some() || row.get("bin").is_some() {
                    return Err("executable entries cannot specify package or bin".into());
                }
                (None, String::new(), path(required("executable")?))
            };
            let policy = match row.get("policy") {
                None => LaunchPolicy::OrFocus,
                Some(Value::Str(s)) if s == "focus" => LaunchPolicy::OrFocus,
                Some(Value::Str(s)) if s == "new" => LaunchPolicy::AlwaysNew,
                _ => return Err("policy must be 'focus' or 'new'".into()),
            };
            let args = match row.get("args") {
                None => Vec::new(),
                Some(Value::Arr(args)) => args
                    .iter()
                    .map(|a| {
                        a.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| "args must contain strings".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => return Err("args must be an array of strings".into()),
            };
            Ok(Some(AppDef {
                id,
                label,
                bin,
                package,
                dir: base.to_string_lossy().into_owned(),
                manifest,
                args,
                policy,
                target_dir,
            }))
        };
        let Some(app) = parse().map_err(|e| format!("entry {}: {e}", i + 1))? else {
            continue;
        };
        if !ids.insert(app.id.clone()) {
            return Err(format!("entry {}: duplicate id '{}'", i + 1, app.id));
        }
        out.push(app);
    }
    Ok(out)
}

fn catalog_path() -> Result<Option<PathBuf>, String> {
    let args: Vec<_> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--apps") {
        return args
            .get(i + 1)
            .map(PathBuf::from)
            .map(Some)
            .ok_or_else(|| "--apps requires a JSON catalog path".into());
    }
    let user = super::paths::home().join("apps.json");
    if user.exists() {
        return Ok(Some(user));
    }
    Ok(super::paths::project_root().map(|p| p.join("config/apps.json")))
}

pub fn loaded() -> &'static Result<Vec<AppDef>, String> {
    static CATALOG: OnceLock<Result<Vec<AppDef>, String>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let Some(path) = catalog_path()? else {
            // `clients::registry` adds the linked modules and Card apps.
            return Ok(Vec::new());
        };
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        parse_catalog(
            &bytes,
            path.parent().unwrap_or(Path::new(".")),
            super::makepad_source::makepad_root(),
        )
            .map_err(|e| format!("{}: {e}", path.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_and_executable_paths_are_relative_to_the_catalog() {
        let apps = parse_catalog(br#"[
            {"id":"reference","label":"Reference","manifest":"../apps/reference/Cargo.toml","package":"octosense-reference","bin":"octosense-reference","policy":"new","args":["two words","$(literal)"]},
            {"id":"installed","label":"Installed","executable":"bin/my app"}
        ]"#, Path::new("/project/config"), None).unwrap();
        assert_eq!(
            apps[0].manifest.as_deref(),
            Some("/project/config/../apps/reference/Cargo.toml")
        );
        assert_eq!(apps[0].policy, LaunchPolicy::AlwaysNew);
        assert_eq!(apps[0].args, ["two words", "$(literal)"]);
        assert_eq!(apps[1].bin, "/project/config/bin/my app");
        assert!(apps[1].manifest.is_none());
        assert!(apps[1].package.is_empty());
        assert_eq!(apps[1].policy, LaunchPolicy::OrFocus);
    }

    #[test]
    fn absolute_paths_are_preserved() {
        let apps = parse_catalog(
            br#"[{"id":"app","label":"App","executable":"/opt/apps/app"}]"#,
            Path::new("/elsewhere"),
            None,
        )
        .unwrap();
        assert_eq!(apps[0].bin, "/opt/apps/app");
    }

    #[test]
    fn invalid_registrations_are_reported_with_the_entry() {
        for (json, reason) in [
            (
                r#"[{"id":"a","label":"A","executable":"a"},{"id":"a","label":"A","executable":"b"}]"#,
                "duplicate",
            ),
            (
                r#"[{"id":"a","label":"A","manifest":"Cargo.toml","executable":"a"}]"#,
                "exactly one",
            ),
            (
                r#"[{"id":"a","label":"A","manifest":"Cargo.toml","bin":"a"}]"#,
                "package",
            ),
            (
                r#"[{"id":"a","label":"A","executable":"a","args":[12]}]"#,
                "args",
            ),
            (
                r#"[{"id":"a","label":"A","executable":"a","policy":"random"}]"#,
                "policy",
            ),
            (
                r#"[{"id":"a","label":"A","executable":"a","argz":[]}]"#,
                "argz",
            ),
        ] {
            let error = parse_catalog(json.as_bytes(), Path::new("/catalog"), None).unwrap_err();
            assert!(error.contains(reason), "expected {reason}, got {error}");
        }
    }

    /// Makepad's apps are not in this repository and must not be reached
    /// through a path beside it: they live in the checkout Cargo already
    /// fetched from GitHub for the pinned revision.
    #[test]
    fn makepad_rows_resolve_against_the_pinned_checkout() {
        let apps = parse_catalog(
            br#"[{"id":"browser","label":"Browser","source":"makepad","package":"makepad-browser","bin":"browser","policy":"focus"}]"#,
            Path::new("/project/config"),
            Some(Path::new("/cargo/checkouts/makepad-d00a/ad8f372")),
        )
        .unwrap();
        assert_eq!(
            apps[0].manifest.as_deref(),
            Some("/cargo/checkouts/makepad-d00a/ad8f372/Cargo.toml")
        );
        assert_eq!(apps[0].package, "makepad-browser");
        assert_eq!(apps[0].bin, "browser");
    }

    /// An installed host has no Cargo checkout. Its Makepad rows cannot
    /// launch, but the rows that can must still reach the launcher.
    #[test]
    fn an_absent_checkout_drops_its_rows_instead_of_failing_the_catalog() {
        let apps = parse_catalog(
            br#"[
                {"id":"browser","label":"Browser","source":"makepad","package":"makepad-browser","bin":"browser"},
                {"id":"reference","label":"Reference","manifest":"../apps/reference/Cargo.toml","package":"octosense-reference","bin":"octosense-reference"}
            ]"#,
            Path::new("/project/config"),
            None,
        )
        .unwrap();
        assert_eq!(
            apps.iter().map(|app| app.id.as_str()).collect::<Vec<_>>(),
            ["reference"]
        );
    }
}
