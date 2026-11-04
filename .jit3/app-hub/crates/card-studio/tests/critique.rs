//! The critique payload's shape, from a report built over the fixtures.
use octosense_card_studio::capture::Capture;
use octosense_card_studio::checks::check;
use octosense_card_studio::critique::{payload, CritiqueOptions, REQUEST_SCHEMA};
use octosense_card_studio::report::{CardInput, Report, SizeReport};
use octosense_card_studio::sizes::TargetSize;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn captures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/captures")
}

/// A report in a fresh directory, over the clipped glance capture, with a
/// stand-in PNG (the payload only carries it).
fn report_dir() -> (PathBuf, Report) {
    let dir =
        std::env::temp_dir().join(format!("card-studio-critique-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for ext in ["snap.json", "tree.txt", "log.json"] {
        std::fs::copy(
            captures().join(format!("clipped-glance.{ext}")),
            dir.join(format!("glance.{ext}")),
        )
        .unwrap();
    }
    std::fs::write(dir.join("glance.png"), b"\x89PNG\r\n\x1a\nstand-in").unwrap();
    let snap =
        serde_json::from_str(&std::fs::read_to_string(dir.join("glance.snap.json")).unwrap())
            .unwrap();
    let log: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("glance.log.json")).unwrap())
            .unwrap();
    let capture = Capture::from_parts(
        snap,
        std::fs::read_to_string(dir.join("glance.tree.txt")).unwrap(),
        &serde_json::json!({"l": log}),
    );
    let size = TargetSize::parse("glance").unwrap();
    let checked = check(&capture, &size, None);
    let mut sizes = BTreeMap::new();
    sizes.insert(
        "glance".to_string(),
        SizeReport {
            kind: size.kind,
            target: [size.width, size.height],
            viewport: checked.viewport.map(|v| [v.w, v.h]),
            png: Some("glance.png".into()),
            snapshot: Some("glance.snap.json".into()),
            tree: Some("glance.tree.txt".into()),
            log: Some("glance.log.json".into()),
            realize: capture.realize_report(),
            pass: checked.pass(),
            findings: checked.findings,
            metrics: checked.metrics,
        },
    );
    let report = Report::new(
        CardInput {
            input: "clipped.card".into(),
            kind: "l0".into(),
            data: None,
        },
        sizes,
    );
    std::fs::write(
        dir.join("report.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    (dir, report)
}

#[test]
fn the_payload_carries_image_snapshot_findings_and_rubric() {
    let (dir, report) = report_dir();
    let rubric = "# Rubric\n\n- Headline first.\n- Nothing cut.\n";
    let p = payload(
        &report,
        &dir,
        rubric,
        &CritiqueOptions {
            sizes: vec![],
            inline_images: true,
        },
    )
    .unwrap();
    assert_eq!(p["schema"], REQUEST_SCHEMA);
    assert_eq!(p["rubric"], rubric);
    assert!(p["prompt"].as_str().unwrap().contains("Headline first."));
    assert_eq!(p["measured"]["pass"], false);
    assert_eq!(
        p["response_schema"]["properties"]["verdict"]["enum"][1],
        "revise"
    );
    let glance = &p["sizes"][0];
    assert_eq!(glance["size"], "glance");
    assert_eq!(glance["kind"], "glance");
    assert_eq!(glance["target"][0], 350.0);
    assert_eq!(glance["image"]["media_type"], "image/png");
    assert!(glance["image"]["path"]
        .as_str()
        .unwrap()
        .ends_with("glance.png"));
    assert!(glance["image"]["data_base64"]
        .as_str()
        .unwrap()
        .starts_with("iVBORw0KGg"));
    let widgets = glance["widgets"].as_array().unwrap();
    assert!(widgets
        .iter()
        .any(|w| w["id"] == "beauty_0_1_1_0" && w["text"] == "Artificial intelligence regulation"));
    assert!(widgets.iter().any(|w| w["drawn"] == false));
    assert!(glance["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["check"] == "text_truncated"));

    // Without --inline the image is only a path; an unknown size is refused.
    let p = payload(&report, &dir, rubric, &CritiqueOptions::default()).unwrap();
    assert!(p["sizes"][0]["image"].get("data_base64").is_none());
    assert!(payload(
        &report,
        &dir,
        rubric,
        &CritiqueOptions {
            sizes: vec!["desktop".into()],
            inline_images: false
        }
    )
    .is_err());

    // The same through the binary's skill protocol.
    let rubric_path = dir.join("rubric.md");
    std::fs::write(&rubric_path, rubric).unwrap();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_card-studio"))
        .arg("card_critique_payload")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    let input = serde_json::json!({"report": dir.join("report.json"), "rubric": rubric_path});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["success"], true, "{envelope}");
    let inner: serde_json::Value =
        serde_json::from_str(envelope["output"].as_str().unwrap()).unwrap();
    assert_eq!(inner["schema"], REQUEST_SCHEMA);
    let _ = std::fs::remove_dir_all(&dir);
}
