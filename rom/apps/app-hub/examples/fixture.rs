//! Local-only signed catalog for testing App Hub install/open. Never publish it.
//!
//! cargo run -p octosense-app-hub-app --example fixture -- target/app-hub/fixture
//! Read the generated environment.json and set OCTOSENSE_HUB,
//! OCTOSENSE_HUB_ANCHOR and OCTOSENSE_APP_DATA before launching OctoSense.
//! The actual card host consumes page.card + page.data.json + kit/native/light,
//! not main.splash. Each fixture uses those files to draw native widgets.

use octosense_app_hub::{Catalog, Entry, HubKey, PublisherKeys, Source, Status};
use octosense_app_policy::{AppManifest, HostLimits, Listing};
use serde_json::{json, Value};
use std::path::Path;

fn main() {
    let result = std::env::args_os()
        .nth(1)
        .ok_or_else(|| "Usage: fixture OUTPUT_DIRECTORY (must be empty)".to_string())
        .and_then(|path| generate(Path::new(&path)));
    match result {
        Ok(environment) => println!("{}", serde_json::to_string_pretty(&environment).unwrap()),
        Err(error) => {
            eprintln!("fixture: {error}");
            std::process::exit(1);
        }
    }
}

fn generate(output: &Path) -> Result<Value, String> {
    if output.exists()
        && std::fs::read_dir(output)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err(
            "The output directory must be empty; use a new directory for each fixture catalog"
                .into(),
        );
    }
    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let output = std::fs::canonicalize(output).map_err(|e| e.to_string())?;
    let hub = output.join("hub");
    let apps = output.join("apps");
    std::fs::create_dir_all(hub.join("artifacts")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&apps).map_err(|e| e.to_string())?;
    // Keys are fresh for this run and remain in memory. Only public keys and
    // signatures are written; no production publisher or anchor is involved.
    let anchor = HubKey::generate();
    let working = HubKey::generate();
    let publisher = HubKey::generate();
    let mut entries = Vec::new();
    for (id, name, subtitle, value, detail, category, storage, accent) in [
        (
            "trail-notes",
            "Trail Notes",
            "Small notes for your next adventure",
            "3.2 km",
            "Ridge trail · an easy afternoon",
            "travel",
            true,
            0xff246753_u64,
        ),
        (
            "focus-timer",
            "Focus Timer",
            "A quiet place to focus",
            "25:00",
            "Your next focus session",
            "productivity",
            false,
            0xff345f95_u64,
        ),
    ] {
        let artifact = format!("artifacts/{id}-1.0.0.bundle");
        let bundle = hub.join(&artifact);
        std::fs::create_dir_all(bundle.join("kit/native/light")).map_err(|e| e.to_string())?;
        let texts = [
            "LOCAL APP",
            name,
            subtitle,
            value,
            detail,
            "Installed from your local App Hub.\nThis is a static validation fixture.",
            if storage {
                "Permission: its own local storage"
            } else {
                "No device permissions requested"
            },
        ];
        let roles = [
            "tag",
            "title",
            "subtitle",
            "value",
            "detail",
            "footer",
            "permission",
        ];
        let mut card = String::from("# Local App Hub validation fixture\ntheme light\ncomponent FixtureSurface(instance: text) { view Kit(component: \"surface\", instance: instance) { slot } }\ncomponent FixturePanel(instance: text) { view Kit(component: \"panel\", instance: instance) }\n");
        for role in roles {
            card.push_str(&format!("component Fixture{role}(text: text) {{ view Kit(component: \"{role}\", instance: \"{role}\", text: text) }}\n"));
        }
        card.push_str(
            "view root FixtureSurface(instance: \"page\") {\n FixturePanel(instance: \"panel\")\n",
        );
        for (role, text) in roles.into_iter().zip(texts) {
            card.push_str(&format!(
                " Fixture{role}(text: {})\n",
                serde_json::to_string(text).unwrap()
            ));
        }
        card.push_str("}\n");
        std::fs::write(bundle.join("page.card"), card).map_err(|e| e.to_string())?;
        let mut components = json!({
            "surface": {"style": {"t":"stack", "variant":"surface", "bg":0xfff4f7f5_u64}, "props":{}, "slot":true},
            "panel": {"style": {"t":"stack", "variant":"surface", "bg":accent, "radius":24}, "props":{}, "slot":false}
        });
        for (role, size, weight, color) in [
            ("tag", 12, 600, 0xff516c65_u64),
            ("title", 30, 700, 0xff182e35),
            ("subtitle", 16, 400, 0xff536762),
            ("value", 46, 700, 0xffffffff),
            ("detail", 15, 400, 0xffffffff),
            ("footer", 14, 400, 0xff536762),
            ("permission", 13, 500, 0xff536762),
        ] {
            components[role] = json!({"style":{"t":"text","size":size,"weight":weight,"color":color,"font_src":"makepad_widgets:resources/Inter.ttf","line_height":size as f64 * 1.5,"alignx":0},"props":{"text":"text"},"slot":false});
        }
        write_json(
            &bundle.join("kit/native/light/kit.json"),
            &json!({"schema_version":1,"theme":"light","tokens":{},"components":components}),
        )?;
        let mut placements = json!({});
        for (instance, component, x, y, w, h) in [
            ("page", "surface", 0, 0, 360, 640),
            ("panel", "panel", 24, 200, 312, 210),
            ("tag", "tag", 24, 34, 312, 24),
            ("title", "title", 24, 75, 312, 52),
            ("subtitle", "subtitle", 24, 128, 312, 54),
            ("value", "value", 44, 236, 272, 76),
            ("detail", "detail", 44, 324, 272, 48),
            ("footer", "footer", 24, 453, 312, 68),
            ("permission", "permission", 24, 550, 312, 44),
        ] {
            placements[instance] =
                json!({"component":component,"layout":{"x":x,"y":y,"w":w,"h":h}});
        }
        write_json(
            &bundle.join("page.data.json"),
            &json!({"$kit":{"theme":"light","placements":placements}}),
        )?;
        let rgb = accent & 0xffffff;
        std::fs::write(bundle.join("icon.svg"), format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96" viewBox="0 0 96 96"><rect width="96" height="96" rx="24" fill="#{rgb:06x}"/><path d="M24 64 42 32 53 51 62 39 76 64Z" fill="white"/></svg>"##)).map_err(|e| e.to_string())?;
        std::fs::write(bundle.join("preview.png"), preview_png(accent)?)
            .map_err(|e| e.to_string())?;
        let listing_json = json!({
            "schema":1, "subtitle":subtitle,
            "description":format!("{name} is a local development fixture for App Hub. Install it to verify permissions, signed bundles, launcher registration and native card opening. Its screen is a static demo, not a production app. The listing image is a diagram of the fixture layout."),
            "category":category, "keywords":["fixture","local","validation"], "icon":"icon.svg", "screenshots":["preview.png"],
            "platforms":["android","macos"],
            "publisher":{"name":"OctoSense local validation","support":"https://example.invalid/local-fixtures","privacy_policy_url":"https://example.invalid/local-fixtures/privacy"},
            "release_notes":"Initial local validation fixture.", "age_rating":"all", "license":"Apache-2.0"
        });
        let listing = Listing::parse(&listing_json.to_string())?;
        write_json(&bundle.join("listing.json"), &listing_json)?;
        let mut manifest = AppManifest::parse(
            &json!({
                "schema":1,"id":id,"version":"1.0.0","name":name,
                "integrity":{"bundle_blake3":octosense_app_policy::digest_dir(&bundle)?},
                "capabilities":if storage {vec!["storage"]} else {vec![]},
            })
            .to_string(),
        )?;
        octosense_app_hub::sign_manifest(&publisher, &mut manifest, "local-fixture-publisher")?;
        write_json(
            &bundle.join("manifest.json"),
            &serde_json::to_value(&manifest).map_err(|e| e.to_string())?,
        )?;
        let keys = PublisherKeys::new().with("local-fixture-publisher", &publisher.public_hex());
        let report = octosense_app_hub::check_bundle(&bundle, &HostLimits::default(), &keys, None)?;
        if !report.passed() {
            return Err(report.render());
        }
        write_json(
            &hub.join(format!("{artifact}.pack.json")),
            &serde_json::to_value(octosense_app_hub::pack_dir(&bundle)?)
                .map_err(|e| e.to_string())?,
        )?;
        entries.push(Entry {
            manifest,
            listing: Some(listing),
            artifact,
            publisher: "local-fixture-publisher".into(),
            publisher_key: publisher.public_hex(),
            source: Source {
                repository: "https://example.invalid/local-fixtures".into(),
                commit: "local-development-fixture".into(),
            },
            status: Status::Offered,
            admitted: octosense_app_hub::today(),
        });
    }
    let mut catalog = Catalog::new(1, &octosense_app_hub::today(), entries);
    working.sign_catalog(&mut catalog, &anchor.certify(&working.public_hex())?)?;
    write_json(
        &hub.join("catalog.json"),
        &serde_json::to_value(&catalog).map_err(|e| e.to_string())?,
    )?;
    let environment = json!({"OCTOSENSE_HUB":hub,"OCTOSENSE_HUB_ANCHOR":anchor.public_hex(),"OCTOSENSE_APP_DATA":apps});
    write_json(&output.join("environment.json"), &environment)?;
    Ok(environment)
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    std::fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("{}: {e}", path.display()))
}

// Diagrammatic fixture image for testing the details image rail. It is explicitly
// described as a layout diagram in the listing, not a device capture.
fn preview_png(accent: u64) -> Result<Vec<u8>, String> {
    use makepad_widgets::makepad_zune_png::{
        makepad_zune_core::{bit_depth::BitDepth, colorspace::ColorSpace, options::EncoderOptions},
        PngEncoder,
    };
    let mut pixels = vec![0u8; 360 * 640 * 4];
    for (x, y, width, height, color) in [
        (0, 0, 360, 640, 0xfff4f7f5_u64),
        (24, 34, 112, 12, 0xff516c65),
        (24, 75, 232, 32, 0xff182e35),
        (24, 132, 276, 12, 0xffa5b6b0),
        (24, 200, 312, 210, accent),
        (44, 246, 170, 40, 0xffffffff),
        (44, 324, 232, 12, 0xffdfece5),
        (24, 462, 268, 10, 0xffa5b6b0),
        (24, 486, 236, 10, 0xffa5b6b0),
        (24, 555, 250, 10, 0xffa5b6b0),
    ] {
        for row in y..y + height {
            for col in x..x + width {
                let offset = (row * 360 + col) * 4;
                pixels[offset..offset + 4].copy_from_slice(&[
                    (color >> 16) as u8,
                    (color >> 8) as u8,
                    color as u8,
                    255,
                ]);
            }
        }
    }
    let options = EncoderOptions::new(360, 640, ColorSpace::RGBA, BitDepth::Eight);
    let mut png = Vec::new();
    PngEncoder::new(&pixels, options)
        .encode(&mut png)
        .map_err(|e| format!("Could not encode fixture image: {e:?}"))?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use octosense_app_hub_app::catalog::{Backend, EntryStatus};
    use octosense_appstore::source::Origin;
    use std::path::PathBuf;

    #[test]
    fn generated_catalog_verifies_and_both_apps_install_and_open() {
        let scratch = tempfile::tempdir().unwrap();
        let environment = generate(&scratch.path().join("fixture")).unwrap();
        let root = PathBuf::from(environment["OCTOSENSE_APP_DATA"].as_str().unwrap());
        let hub = PathBuf::from(environment["OCTOSENSE_HUB"].as_str().unwrap());
        let mut backend = Backend::new(
            root,
            Origin::Directory(hub.clone()),
            environment["OCTOSENSE_HUB_ANCHOR"].as_str().unwrap().into(),
        );
        let snapshot = backend.refresh();
        assert!(snapshot.verified);
        assert_eq!(snapshot.entries.len(), 2);
        for entry in snapshot.entries {
            assert_eq!(entry.status, EntryStatus::Available);
            let installed = backend.install(&entry.consent.unwrap()).unwrap();
            assert!(backend.may_open(&entry.id).is_ok());
            assert!(installed.library.iter().any(|item| item.id == entry.id));
            let bundle = hub.join(format!("artifacts/{}-1.0.0.bundle", entry.id));
            assert!(bundle.join("page.card").is_file());
            assert!(bundle.join("page.data.json").is_file());
            assert!(bundle.join("kit/native/light/kit.json").is_file());
            assert!(hub
                .join(format!("artifacts/{}-1.0.0.bundle.pack.json", entry.id))
                .is_file());
        }
    }

    #[test]
    fn generator_refuses_to_replace_an_existing_catalog() {
        let scratch = tempfile::tempdir().unwrap();
        let output = scratch.path().join("fixture");
        let first = generate(&output).unwrap();
        let original = std::fs::read(output.join("hub/catalog.json")).unwrap();
        assert!(generate(&output).unwrap_err().contains("empty"));
        assert_eq!(
            std::fs::read(output.join("hub/catalog.json")).unwrap(),
            original
        );
        assert!(first["OCTOSENSE_HUB_ANCHOR"].as_str().unwrap().len() == 64);
    }
}
