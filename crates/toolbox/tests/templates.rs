//! Every shipped template against its recorded fixtures, and the library
//! lock. `TOOLBOX_BLESS=1 cargo test -p octosense-toolbox --test templates`
//! rewrites the lock and each fixture's `expected` from the current output;
//! review the diff before committing it.

use octosense_toolbox::fixture::{self, Expected, FixtureCase};
use octosense_toolbox::library::{Library, Template, TemplateOrigin, BUILTIN};
use octosense_toolbox::{json, run, AppContext, RunOptions, RunStatus};
use std::path::{Path, PathBuf};

fn bless() -> bool {
    std::env::var_os("TOOLBOX_BLESS").is_some()
}

fn templates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("templates")
}

fn builtin() -> Vec<Template> {
    BUILTIN
        .iter()
        .map(|(_, manifest, source)| {
            Template::from_parts(manifest, source, TemplateOrigin::Library).unwrap()
        })
        .collect()
}

#[test]
fn library_lock_pins_every_shipped_template() {
    let lock = Library::lock_for(&builtin());
    let path = templates_dir().join("library.lock.json");
    if bless() {
        std::fs::write(&path, serde_json::to_string_pretty(&lock).unwrap() + "\n").unwrap();
    }
    let library = Library::builtin().expect("library.lock.json is current");
    assert_eq!(library.len(), BUILTIN.len());
    // The directory loader sees the same library.
    let from_dir = Library::load_dir(&templates_dir()).unwrap();
    assert_eq!(from_dir.len(), library.len());
    for template in library.templates() {
        assert_eq!(from_dir.get(template.id()).unwrap().digest, template.digest);
    }
}

fn fixture_cases(id: &str) -> Vec<(PathBuf, FixtureCase)> {
    let dir = templates_dir().join(id).join("fixtures");
    let mut cases: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| {
            let case: FixtureCase = serde_json::from_str(&std::fs::read_to_string(&p).unwrap())
                .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            (p, case)
        })
        .collect();
    cases.sort_by(|a, b| a.0.cmp(&b.0));
    cases
}

#[tokio::test]
async fn every_template_matches_its_fixtures() {
    let app = AppContext::new("os.news", std::env::temp_dir()).grant("research");
    let mut checked = 0;
    for template in builtin() {
        let cases = fixture_cases(template.id());
        assert!(!cases.is_empty(), "{} ships no fixtures", template.id());
        for (path, mut case) in cases {
            let name = format!("{}/{}", template.id(), case.name);
            let host = fixture::host(&case.fixture);
            let result = run(
                &template,
                &app,
                case.params.clone(),
                &host,
                RunOptions::default(),
            )
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"));
            // Every URL in the data is one the host kept.
            for url in json::strings(&result.data)
                .into_iter()
                .filter(|s| s.starts_with("http"))
            {
                assert!(
                    result
                        .provenance
                        .iter()
                        .any(|p| p.url == url || p.resolved_url.as_deref() == Some(url)),
                    "{name}: {url} has no provenance"
                );
            }
            if bless() {
                case.expected = Some(Expected {
                    status: result.status,
                    reasons: result.status_reasons.clone(),
                    data: result.data.clone(),
                });
                std::fs::write(&path, serde_json::to_string_pretty(&case).unwrap() + "\n").unwrap();
            }
            let expected = case
                .expected
                .as_ref()
                .unwrap_or_else(|| panic!("{name} has no expected result"));
            assert_eq!(
                result.status, expected.status,
                "{name}: {:?}",
                result.diagnostics
            );
            assert_eq!(result.status_reasons, expected.reasons, "{name}");
            assert!(
                json::equivalent(&result.data, &expected.data),
                "{name}: data differs at {:?}",
                json::first_difference(&expected.data, &result.data, "$")
            );
            if result.status != RunStatus::Failed {
                assert!(result.stats.calls <= template.manifest.budget.max_calls);
                assert!(result.stats.model_calls <= template.manifest.budget.max_model_calls);
                assert!(result.stats.reads <= template.manifest.budget.max_reads);
            }
            checked += 1;
        }
    }
    assert!(checked >= 12, "{checked} fixture cases");
}

fn template(id: &str) -> octosense_toolbox::library::Template {
    let (_, manifest, source) = BUILTIN
        .iter()
        .find(|(i, _, _)| *i == id)
        .expect("template in BUILTIN");
    Template::from_parts(manifest, source, TemplateOrigin::Library).unwrap()
}

/// The dossier template's own fixtures: its two cases (a clean two-question
/// run; a failed read and an empty question) replay green against the ported
/// template.
#[tokio::test]
async fn the_dossier_fixtures_replay() {
    let app = AppContext::new("os.news", std::env::temp_dir()).grant("research");
    let template = template("dossier");
    for (_, mut case) in fixture_cases("dossier") {
        let name = format!("dossier/{}", case.name);
        let result = run(
            &template,
            &app,
            case.params.clone(),
            &fixture::host(&case.fixture),
            RunOptions::default(),
        )
        .await
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let expected = case
            .expected
            .take()
            .unwrap_or_else(|| panic!("{name} has no expected result"));
        assert_eq!(
            result.status, expected.status,
            "{name}: {:?}",
            expected.reasons
        );
        assert_eq!(result.status_reasons, expected.reasons, "{name}");
        assert_eq!(result.data, expected.data, "{name}");
    }
}
