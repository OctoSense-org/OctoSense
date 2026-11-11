//! The provider sign-in sheet's words, and its quoting of app-controlled text;
//! every sign-in sheet's two trees: a phone's page and a desktop's Mac dialog.
use super::*;
use makepad_widgets::{
    desktop_style::{DesktopStyle, StyleSheet},
    splash::validate_splash_body,
    *,
};

#[test]
fn provider_failures_read_as_plain_words_and_backend_errors_pass_through() {
    let github = |error| sheet_failure(Provider::Github, error);
    assert!(github("GitHub authorization was declined").starts_with("You declined on GitHub"));
    assert!(github("GitHub authorization expired; start again").starts_with("The code expired"));
    assert!(github("Sign-in cancelled or expired").contains("timed out"));
    assert!(github("Provider connection failed; check the network")
        .starts_with("Couldn't reach GitHub"));
    assert_eq!(
        github("Required permissions were not granted"),
        "Required permissions were not granted"
    );
    assert_eq!(
        sheet_failure(Provider::Backend, "GitHub authorization was declined"),
        "GitHub authorization was declined"
    );
}

/// The one list every test's app names come from: tests share the setter.
fn sample_app_names() {
    set_app_names(Arc::new(|app: &str| match app {
        "sample.quoting" => Some(r#"Notes" } host.request("auth.sheet.cancel") {"#.into()),
        "os.writer" => Some("Writer".into()),
        "sample.lookalike" => Some("Mail".into()),
        _ => None,
    }));
}

#[test]
fn the_sheet_names_the_app_and_quotes_every_app_controlled_word() {
    // A manifest name is the app's own text: it must stay a string literal.
    sample_app_names();
    let scopes = BTreeSet::from(["read:user".to_string(), "public_repo".to_string()]);
    let sheet = provider_sheet("ticket-1", Provider::Github, "sample.quoting", &scopes);
    assert!(sheet.contains(
        r#""Notes\" } host.request(\"auth.sheet.cancel\") { wants to use your GitHub account.""#
    ));
    assert!(!sheet.contains(r#"Notes" }"#));
    assert!(sheet.contains(r#"text: "sample.quoting""#));
    // Without a usable name the sheet shows the app's id.
    let unnamed = provider_sheet("ticket-2", Provider::Github, "sample.unnamed", &scopes);
    assert!(unnamed.contains(r#""sample.unnamed wants to use your GitHub account.""#));
}

fn backend() -> crate::backend::BackendClient {
    crate::backend::BackendClient::new(crate::backend::BackendRegistration {
        id: "fixture".into(),
        app_id: "sample.backend".into(),
        client_id: "public-native".into(),
        authorization_url: "https://example.test/authorize".into(),
        token_url: "https://example.test/token".into(),
        me_url: "https://example.test/me".into(),
        logout_url: "https://example.test/logout".into(),
        scopes: BTreeSet::from(["app.session".to_string()]),
        operations: Default::default(),
    })
    .unwrap()
}

/// Each sheet, with the ids its script reaches on both trees.
fn sheets() -> Vec<(&'static str, String, &'static [&'static str])> {
    const PROVIDER: &[&str] = &[
        "oauth_title",
        "oauth_subtitle",
        "intro",
        "preparing",
        "step",
        "device_code",
        "code_box",
        "code",
        "copy",
        "open",
        "waiting",
        "expiry",
        "failed",
        "failure",
        "oauth_lifetime",
        "oauth_cancel",
        "oauth_continue",
    ];
    let scopes = BTreeSet::from(["read:user".to_string()]);
    let session = BTreeSet::from(["app.session".to_string()]);
    let client = backend();
    vec![
        (
            "GitHub",
            provider_sheet("t", Provider::Github, "sample.app", &scopes),
            PROVIDER,
        ),
        (
            "Google",
            provider_sheet("t", Provider::Google, "sample.app", &scopes),
            PROVIDER,
        ),
        (
            "backend, browser",
            browser_sheet(
                "t",
                Provider::Backend,
                "sample.backend",
                &session,
                Some(&client),
            ),
            &[
                "oauth_lifetime",
                "oauth_code",
                "oauth_browser",
                "oauth_status",
            ],
        ),
        (
            "backend, web view",
            embedded_sheet("t", "sample.backend", &client),
            &[
                "oauth_intro",
                "oauth_webview",
                "oauth_status",
                "oauth_toolbar",
                "oauth_continue",
            ],
        ),
    ]
}

/// A script error on one of these leaves the person with no way to sign in.
#[test]
fn every_sign_in_sheet_evaluates_on_a_phone_and_on_a_desktop() {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    crate::sign_in_code::register();
    let desktop = "mod.theme.ui_pointer = true\n";
    let dark = "mod.theme.ui_pointer = true\nmod.theme.color_text = #xf2f2f4\n";
    let mac = "mod.theme.ui_pointer = true\nmod.theme.ui_unified = true\n";
    let mac_dark = "mod.theme.ui_pointer = true\nmod.theme.ui_unified = true\nmod.theme.color_text = #xf2f2f4\n";
    for (name, source, _) in sheets() {
        for (place, prelude) in [("phone", ""), ("desktop", desktop), ("dark desktop", dark), ("Mac", mac), ("dark Mac", mac_dark)] {
            let errors = validate_splash_body(&mut cx, &format!("{prelude}{source}"), false);
            assert!(errors.is_empty(), "{name} on a {place}: {errors:?}");
        }
    }
}

fn built(cx: &mut Cx, source: &str, pointer: bool, dark: bool) -> Splash {
    let mut splash = cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        let value = vm.eval(script! { use mod.widgets.* Splash {} });
        Splash::script_from_value(vm, value)
    });
    let mut sheet = StyleSheet::load_with_appearance(DesktopStyle::Macos, dark);
    if pointer {
        sheet
            .theme
            .push_str("\nmod.theme.ui_pointer = true\ntrue\n");
    }
    splash.set_stylesheet(cx, sheet);
    splash.set_text(cx, source);
    splash
}

#[test]
fn a_desktop_draws_a_mac_dialog_and_a_phone_keeps_its_page() {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    crate::sign_in_code::register();
    for (name, source, ids) in sheets() {
        for (pointer, dark) in [(false, false), (true, false), (true, true)] {
            let splash = built(&mut cx, &source, pointer, dark);
            for id in ids {
                let id = LiveId::from_str(id);
                assert!(
                    !splash.widget(&mut cx, &[id]).is_empty(),
                    "{name} (pointer {pointer}) lost {id}"
                );
            }
        }
    }
    let scopes = BTreeSet::from(["read:user".to_string()]);
    let source = provider_sheet("t", Provider::Github, "sample.app", &scopes);
    let walk_of = |cx: &mut Cx, pointer, id: &[LiveId]| {
        built(cx, &source, pointer, false).widget(cx, id).walk(cx)
    };
    let desk = walk_of(&mut cx, true, ids!(oauth_continue));
    assert!(matches!(desk.height, Size::Fixed(h) if h == 28.0));
    assert!(!matches!(desk.width, Size::Fill { .. }), "never full width");
    let phone = walk_of(&mut cx, false, ids!(oauth_continue));
    assert!(matches!(phone.height, Size::Fixed(h) if h == 48.0));
    assert!(matches!(phone.width, Size::Fill { .. }));
}

/// A Mac's words: a system app by its name alone, because no other app can have
/// its id; anyone else's name with the id beside it, which is what tells a
/// look-alike from the real one. Other styles keep what they said.
#[test]
fn a_mac_names_a_system_app_and_keeps_another_apps_id_in_sight() {
    sample_app_names();
    let session = BTreeSet::from(["app.session".to_string()]);
    let client = backend();
    let system = browser_sheet("t", Provider::Backend, "os.writer", &session, Some(&client));
    assert!(system.contains(r#""Sign In to Writer""#));
    assert!(system.contains("visible: desk_mac && false"));
    assert!(system.contains(r#""https://example.test""#));
    assert!(system.contains("Continue to Writer's website in your browser"));
    assert!(!system.contains("fixture wants"), "no backend's internal name in a sentence");

    let other = browser_sheet("t", Provider::Backend, "sample.lookalike", &session, Some(&client));
    assert!(other.contains(r#""Sign In to Mail""#));
    assert!(other.contains(r#"visible: desk_mac && true text: "sample.lookalike""#));

    let unnamed = browser_sheet("t", Provider::Backend, "sample.unnamed", &session, Some(&client));
    assert!(unnamed.contains("visible: desk_mac && false"), "an app without a name is not said twice");

    let scopes = BTreeSet::from(["read:user".to_string()]);
    let shown = |app: &str| provider_sheet("t", Provider::Github, app, &scopes);
    assert!(shown("os.writer").contains("visible: !desk_mac || false"));
    assert!(shown("sample.lookalike").contains("visible: !desk_mac || true"));
    assert!(shown("sample.unnamed").contains("visible: !desk_mac || false"));

    let web = embedded_sheet("t", "os.writer", &client);
    assert!(web.contains(r#""Sign In to Writer""#));
    assert!(web.contains("Continue to Writer's website"));
    assert!(web.contains("Sign in or create an account on Writer's website."));
    assert!(web.contains("This page tried to leave Writer's website"));
    assert!(web.contains("visible: !desk_mac || false"));
    assert!(embedded_sheet("t", "sample.lookalike", &client).contains("visible: !desk_mac || true"));
}
