//! The measured checks over real card-host captures (`fixtures/captures`,
//! taken from `card-studio render` of `fixtures/cards/*.card`) and over
//! small synthetic snapshots. No card-host, no GPU.
use octosense_card_studio::capture::Capture;
use octosense_card_studio::checks::check;
use octosense_card_studio::report::{Finding, Severity};
use octosense_card_studio::sizes::TargetSize;
use serde_json::json;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn load(name: &str) -> Capture {
    let read =
        |ext: &str| std::fs::read_to_string(fixture(&format!("captures/{name}.{ext}"))).unwrap();
    let snap: serde_json::Value = serde_json::from_str(&read("snap.json")).unwrap();
    let log: serde_json::Value = serde_json::from_str(&read("log.json")).unwrap();
    Capture::from_parts(snap, read("tree.txt"), &json!({ "l": log }))
}

fn of<'a>(findings: &'a [Finding], check: &str) -> Vec<&'a Finding> {
    findings.iter().filter(|f| f.check == check).collect()
}

#[test]
fn the_deliberately_clipped_card_is_caught_on_the_glance_tile() {
    let checked = check(
        &load("clipped-glance"),
        &TargetSize::parse("glance").unwrap(),
        None,
    );
    let f = &checked.findings;

    // The second topic chip is squeezed to a sliver of its text.
    let truncated = of(f, "text_truncated");
    assert_eq!(truncated.len(), 1, "{f:#?}");
    assert_eq!(truncated[0].widget.as_deref(), Some("beauty_0_1_1_0"));
    assert_eq!(
        truncated[0].text.as_deref(),
        Some("Artificial intelligence regulation")
    );
    assert_eq!(truncated[0].severity, Severity::Error);

    // Stories c–e (title and publisher each) were never laid out.
    let hidden = of(f, "text_hidden");
    assert_eq!(hidden.len(), 5, "{f:#?}");
    assert!(hidden
        .iter()
        .any(|h| h.text.as_deref() == Some("Satellite internet reaches the poles")));

    // It does not fit the tile; with no probe the height is a lower bound.
    let fit = of(f, "does_not_fit");
    assert_eq!(fit.len(), 1);
    assert_eq!(fit[0].severity, Severity::Error);
    assert!(!checked.metrics.fits);
    assert!(checked.metrics.content_height_is_lower_bound);
    assert_eq!(checked.metrics.hidden_text, 5);
    assert_eq!(checked.metrics.realize_nodes, Some(16));
    assert!(!checked.pass());
    assert_eq!(checked.viewport.map(|v| (v.w, v.h)), Some((350.0, 160.0)));
}

#[test]
fn on_a_phone_everything_is_laid_out_but_the_chip_is_still_cut() {
    let checked = check(
        &load("clipped-phone"),
        &TargetSize::parse("phone").unwrap(),
        None,
    );
    let f = &checked.findings;
    assert_eq!(of(f, "text_hidden").len(), 0);
    assert_eq!(of(f, "does_not_fit").len(), 0);
    let truncated = of(f, "text_truncated");
    assert_eq!(truncated.len(), 1, "{f:#?}");
    assert_eq!(truncated[0].widget.as_deref(), Some("beauty_0_1_1_0"));
    assert!(checked.metrics.fits);
    assert_eq!(checked.metrics.content_height, Some(228.0));
    assert!(!checked.pass());
}

#[test]
fn a_card_that_fits_passes_clean() {
    let checked = check(
        &load("digest-glance"),
        &TargetSize::parse("glance").unwrap(),
        None,
    );
    assert!(checked.findings.is_empty(), "{:#?}", checked.findings);
    assert!(checked.pass());
    let m = &checked.metrics;
    assert!(m.fits && !m.content_height_is_lower_bound && !m.realize_truncated);
    assert_eq!(
        (m.widgets, m.drawn, m.text_nodes, m.hidden_text),
        (7, 7, 5, 0)
    );
    assert_eq!(m.content_height, Some(118.0));
}

#[test]
fn a_probe_render_measures_the_height_the_content_wants() {
    // The phone capture is the same card laid out with room to spare: use it
    // as the probe. Its viewport is taller, so the content is all drawn.
    let checked = check(
        &load("clipped-glance"),
        &TargetSize::parse("glance").unwrap(),
        Some(&load("clipped-phone")),
    );
    assert_eq!(checked.metrics.content_height, Some(228.0));
    assert!(!checked.metrics.content_height_is_lower_bound);
    let fit = of(&checked.findings, "does_not_fit");
    assert!(fit[0].message.contains("needs 228pt"), "{}", fit[0].message);
}

/// `(id, parent, type, [x, y, w, h], text)`.
type Spec<'a> = (&'a str, Option<&'a str>, &'a str, [f64; 4], Option<&'a str>);

/// A capture from hand-written widgets: `(id, parent, type, [x,y,w,h], text)`,
/// drawn unless the rect is empty.
fn synthetic(widgets: &[Spec], viewport: [f64; 2], log: &[&str]) -> Capture {
    let mut snap =
        vec![json!({"i": "card", "ty": "Splash", "r": [0, 0, viewport[0], viewport[1]]})];
    let mut tree = format!(
        "W3 x\n0 -1 main_window Window 0 0 {} {}\n1 0 card Splash 0 0 {} {}\n",
        viewport[0], viewport[1], viewport[0], viewport[1]
    );
    let mut index = std::collections::HashMap::new();
    index.insert("card", 1);
    for (i, (id, parent, ty, r, text)) in widgets.iter().enumerate() {
        let mut w = json!({"i": id, "ty": ty, "r": r});
        if let Some(t) = text {
            w["t"] = json!(t);
        }
        snap.push(w);
        if r[2] > 0.0 && r[3] > 0.0 {
            let n = i + 2;
            index.insert(*id, n);
            let p = index[parent.unwrap_or("card")];
            tree.push_str(&format!(
                "{n} {p} {id} {ty} {} {} {} {}\n",
                r[0], r[1], r[2], r[3]
            ));
        }
    }
    snap.push(json!({"i": "sheet", "ty": "Splash", "r": [0, 0, 0, 0]}));
    tree.push_str(&format!("{} 0 sheet Splash 0 0 0 0\n", widgets.len() + 2));
    Capture::from_parts(json!({ "s": snap }), tree, &json!({ "l": log }))
}

fn realize_line(report: serde_json::Value) -> String {
    format!("[I] card-host - card-host: realize {report}")
}

#[test]
fn overflow_overlap_viewport_and_states() {
    let ok_realize = realize_line(
        json!({"lint": {"valid": true, "level": "L0", "diagnostics": []},
        "realize": {"nodes": 6, "truncated": false, "diagnostics": []}, "sources": []}),
    );
    let capture = synthetic(
        &[
            ("root", None, "View", [0.0, 0.0, 300.0, 200.0], None),
            // Text on text: two labels stacked on the same spot.
            (
                "a",
                Some("root"),
                "Label",
                [10.0, 10.0, 120.0, 20.0],
                Some("Headline one"),
            ),
            (
                "b",
                Some("root"),
                "Label",
                [12.0, 12.0, 120.0, 20.0],
                Some("Headline two"),
            ),
            // A box with a label that sticks out of it and off the viewport.
            ("box", Some("root"), "View", [10.0, 50.0, 100.0, 30.0], None),
            (
                "wide",
                Some("box"),
                "Label",
                [10.0, 55.0, 330.0, 20.0],
                Some("A line that runs far past its box and the edge"),
            ),
            // An image hanging off the bottom.
            (
                "pic",
                Some("root"),
                "Image",
                [10.0, 180.0, 40.0, 40.0],
                None,
            ),
            // A binding that resolved to nothing, and a loading state.
            (
                "dash",
                Some("root"),
                "Label",
                [10.0, 100.0, 10.0, 12.0],
                Some("—"),
            ),
            (
                "wait",
                Some("root"),
                "Label",
                [10.0, 120.0, 150.0, 16.0],
                Some("Loading the digest…"),
            ),
        ],
        [300.0, 200.0],
        &[&ok_realize],
    );
    let checked = check(&capture, &TargetSize::parse("phone=300x200").unwrap(), None);
    let f = &checked.findings;
    let overlap = of(f, "overlap");
    assert_eq!(overlap.len(), 1, "{f:#?}");
    assert_eq!(overlap[0].severity, Severity::Warn);
    let overflow = of(f, "overflow");
    assert!(
        overflow
            .iter()
            .any(|o| o.widget.as_deref() == Some("wide") && o.severity == Severity::Error),
        "{f:#?}"
    );
    assert!(of(f, "text_clipped")
        .iter()
        .any(|o| o.widget.as_deref() == Some("wide")));
    assert!(of(f, "outside_viewport")
        .iter()
        .any(|o| o.widget.as_deref() == Some("pic") && o.severity == Severity::Warn));
    assert_eq!(of(f, "missing_value")[0].widget.as_deref(), Some("dash"));
    assert_eq!(of(f, "state_visible")[0].widget.as_deref(), Some("wait"));
    assert!(!checked.metrics.fits);
    assert!(!checked.pass());
}

#[test]
fn a_label_over_an_image_is_only_information() {
    let capture = synthetic(
        &[
            ("root", None, "View", [0.0, 0.0, 300.0, 200.0], None),
            ("art", Some("root"), "Image", [0.0, 0.0, 300.0, 120.0], None),
            (
                "caption",
                Some("root"),
                "Label",
                [10.0, 90.0, 120.0, 20.0],
                Some("On the image"),
            ),
        ],
        [300.0, 200.0],
        &[],
    );
    let checked = check(&capture, &TargetSize::parse("phone=300x200").unwrap(), None);
    let overlap = of(&checked.findings, "overlap");
    assert_eq!(overlap.len(), 1);
    assert_eq!(overlap[0].severity, Severity::Info);
    // No realize report is information too; nothing here fails.
    assert_eq!(of(&checked.findings, "no_realize_report").len(), 1);
    assert!(checked.pass());
}

#[test]
fn realize_report_log_errors_and_sources() {
    let report = realize_line(json!({
        "lint": {"valid": false, "level": "L2", "diagnostics": [{"line": 3, "column": 1, "message": "imperative command"}]},
        "realize": {"nodes": 8192, "truncated": true, "diagnostics": []},
        "sources": [{"name": "digest", "helper": "sys.digest", "state": "failed"},
                    {"name": "weather", "helper": "sys.weather", "state": "pending"}],
        "lower_error": "unsupported measured design node: Column"
    }));
    let capture = synthetic(
        &[("t", None, "Label", [0.0, 0.0, 100.0, 20.0], Some("Title"))],
        [300.0, 200.0],
        &[
            &report,
            "[E] splash.octoscript:12:3 - method digest not found on object",
        ],
    );
    let checked = check(&capture, &TargetSize::parse("glance").unwrap(), None);
    let f = &checked.findings;
    for (check, severity) in [
        ("lint", Severity::Error),
        ("realize_truncated", Severity::Error),
        ("lower_failed", Severity::Error),
        ("source_failed", Severity::Error),
        ("source_pending", Severity::Warn),
        ("log_error", Severity::Error),
    ] {
        let found = of(f, check);
        assert_eq!(found.len(), 1, "{check}: {f:#?}");
        assert_eq!(found[0].severity, severity, "{check}");
    }
    assert!(checked.metrics.realize_truncated);
    assert_eq!(checked.metrics.realize_nodes, Some(8192));
    assert_eq!(checked.metrics.log_errors, 1);
}

#[test]
fn nothing_drawn_is_an_empty_card() {
    let capture = synthetic(
        &[("t", None, "Label", [0.0, 0.0, 0.0, 0.0], Some("Title"))],
        [350.0, 160.0],
        &[],
    );
    let checked = check(&capture, &TargetSize::parse("glance").unwrap(), None);
    assert_eq!(of(&checked.findings, "empty_card").len(), 1);
    assert_eq!(of(&checked.findings, "text_hidden").len(), 1);
}

#[test]
fn the_check_command_reads_saved_captures() {
    let run = |name: &str, size: &str| {
        std::process::Command::new(env!("CARGO_BIN_EXE_card-studio"))
            .args(["check", "--size", size])
            .arg("--snap")
            .arg(fixture(&format!("captures/{name}.snap.json")))
            .arg("--tree")
            .arg(fixture(&format!("captures/{name}.tree.txt")))
            .arg("--log")
            .arg(fixture(&format!("captures/{name}.log.json")))
            .output()
            .unwrap()
    };
    let bad = run("clipped-glance", "glance");
    assert_eq!(bad.status.code(), Some(1));
    let out: serde_json::Value = serde_json::from_slice(&bad.stdout).unwrap();
    assert_eq!(out["summary"]["pass"], false);
    assert!(out["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["check"] == "text_truncated"));
    let good = run("digest-glance", "glance");
    assert_eq!(
        good.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&good.stdout)
    );
}
