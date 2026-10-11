//! The provider sign-in sheet's words, and its quoting of app-controlled text.
use super::*;

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

#[test]
fn the_sheet_names_the_app_and_quotes_every_app_controlled_word() {
    // A manifest name is the app's own text: it must stay a string literal.
    set_app_names(Arc::new(|app: &str| {
        (app == "sample.quoting").then(|| r#"Notes" } host.request("auth.sheet.cancel") {"#.into())
    }));
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
