//! End to end: render the fixture cards in a real, hidden `card-host
//! --remote` and read the report. Needs a GPU and a graphical session, a
//! release card-host, and the L0 kit, so it is ignored by default:
//!
//! ```sh
//! cargo build --release -p octosense-card-host
//! cargo test --release -p octosense-card-studio --test render_gpu -- --ignored --nocapture
//! ```
//!
//! card-host is `$CARD_HOST` or the workspace's `target/release/card-host`;
//! the kit is `$CARD_STUDIO_KIT` or the sibling Octoscript-Makepad checkout's
//! `components/l0`. The report is written to `$CARD_STUDIO_OUT` (default: a
//! temporary directory) and printed.
use octosense_card_studio::render::{render, RenderOptions};
use octosense_card_studio::sizes::TargetSize;
use std::path::PathBuf;

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn options(card: &str, out: &str) -> RenderOptions {
    let card_host = std::env::var_os("CARD_HOST")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace().join("target/release/card-host"));
    assert!(
        card_host.is_file(),
        "no card-host at {}: build it with `cargo build --release -p octosense-card-host`",
        card_host.display()
    );
    let kit = std::env::var_os("CARD_STUDIO_KIT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace().join("../octoscript-makepad/components/l0"));
    let root = std::env::var_os("CARD_STUDIO_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("card-studio-gpu-test"));
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/cards");
    let mut opts = RenderOptions::new(
        fixtures.join(format!("{card}.card")),
        card_host,
        root.join(out),
    );
    opts.data = Some(fixtures.join(format!("{card}.data.json")));
    opts.kit = Some(kit);
    opts.sizes = vec![
        TargetSize::parse("glance").unwrap(),
        TargetSize::parse("phone").unwrap(),
    ];
    opts
}

#[test]
#[ignore = "needs a GPU, a graphical session and a release card-host"]
fn renders_two_sizes_and_catches_the_clipped_card() {
    let opts = options("clipped", "clipped");
    let report = render(&opts).expect("render");
    println!("{}", serde_json::to_string_pretty(&report).unwrap());

    // The shape: one entry per size, each with its files and a verdict.
    assert_eq!(report.schema, "card-studio/report@1");
    assert_eq!(report.sizes.keys().collect::<Vec<_>>(), ["glance", "phone"]);
    for (name, size) in &report.sizes {
        for file in [&size.png, &size.snapshot, &size.tree, &size.log] {
            let path = opts.out_dir.join(
                file.as_ref()
                    .unwrap_or_else(|| panic!("{name}: a file is missing")),
            );
            assert!(
                std::fs::metadata(&path)
                    .map(|m| m.len() > 0)
                    .unwrap_or(false),
                "{}",
                path.display()
            );
        }
        assert_eq!(
            size.viewport,
            Some(size.target),
            "{name}: the card gets the whole window"
        );
        let realize = size
            .realize
            .as_ref()
            .expect("card-host logged its realize report");
        assert_eq!(realize["lint"]["valid"], true);
        assert_eq!(realize["realize"]["truncated"], false);
        assert_eq!(realize["lowering"], "l0-kit");
    }
    assert!(opts.out_dir.join("report.json").is_file());

    // The deliberately clipped card is caught.
    let glance = &report.sizes["glance"];
    let checks: Vec<&str> = glance.findings.iter().map(|f| f.check.as_str()).collect();
    for expected in ["text_truncated", "text_hidden", "does_not_fit"] {
        assert!(
            checks.contains(&expected),
            "glance lacks {expected}: {checks:?}"
        );
    }
    assert!(!glance.pass && !report.summary.pass);
    assert!(glance.metrics.content_height.unwrap() > 160.0);
    assert!(report.sizes["phone"]
        .findings
        .iter()
        .any(|f| f.check == "text_truncated"));
}

#[test]
#[ignore = "needs a GPU, a graphical session and a release card-host"]
fn a_card_that_fits_passes() {
    let opts = options("digest", "digest");
    let report = render(&opts).expect("render");
    println!("{}", serde_json::to_string_pretty(&report.summary).unwrap());
    assert!(
        report.summary.pass,
        "{:#?}",
        report
            .sizes
            .values()
            .flat_map(|s| &s.findings)
            .collect::<Vec<_>>()
    );
    assert!(report.sizes["glance"].metrics.fits);
}
