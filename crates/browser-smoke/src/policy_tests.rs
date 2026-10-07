use makepad_widgets::makepad_platform::system_browser::{
    BrowserPolicy,
    NavigationDecision::{Allow, Block, Callback},
};

#[test]
fn restricted_document_does_not_grant_its_sibling_pages() {
    let policy = BrowserPolicy::reader("http://127.0.0.1:8765/page?q=one", false).unwrap();
    assert_eq!(
        policy.decide("http://127.0.0.1:8765/page?q=one#section", true),
        Allow
    );
    for url in [
        "http://127.0.0.1:8765/other",
        "http://127.0.0.1:8765/page?q=two",
        "https://example.com/",
        "file:///etc/passwd",
    ] {
        assert_eq!(policy.decide(url, true), Block, "{url}");
    }
}

#[test]
fn navigable_readers_allow_public_https_and_reject_private_destinations() {
    let policy = BrowserPolicy::reader("https://news.example/start", true).unwrap();
    assert_eq!(policy.decide("https://article.example/story", true), Allow);
    for url in [
        "http://article.example/",
        "https://127.0.0.1/",
        "https://2130706433/",
        "https://0x7f000001/",
        "https://10.0.0.1/",
        "https://192.168.1.1/",
        "https://169.254.169.254/",
        "https://100.64.0.1/",
        "https://[::1]/",
        "https://[::ffff:127.0.0.1]/",
        "https://[fd00::1]/",
        "https://[2001:db8::1]/",
        "https://printer.local/",
        "https://localhost./",
        "file:///tmp/file",
        "javascript:alert(1)",
        "data:text/html,hello",
    ] {
        assert_eq!(policy.decide(url, true), Block, "{url}");
    }
}

#[test]
fn malformed_or_credentialed_urls_are_not_normalized_into_authority() {
    let policy = BrowserPolicy::reader("https://example.com/", true).unwrap();
    for url in [
        "https://name:password@example.com/",
        " https://example.com/",
        "https://example.com/\n",
        "https://example.com\\@127.0.0.1/",
    ] {
        assert_eq!(policy.decide(url, true), Block, "{url:?}");
    }
    assert!(BrowserPolicy::reader("javascript:alert(1)", false).is_err());
    assert!(BrowserPolicy::auth("about:blank", "https://octosense.invalid/auth/callback").is_err());
}

#[test]
fn authentication_callback_is_exact_and_never_a_subframe_navigation() {
    let policy = BrowserPolicy::auth(
        "https://backend.example/login",
        "https://octosense.invalid/auth/callback",
    )
    .unwrap();
    assert_eq!(
        policy.decide("https://backend.example/register", true),
        Allow
    );
    assert_eq!(
        policy.decide(
            "https://octosense.invalid/auth/callback?code=synthetic&state=fixture",
            true
        ),
        Callback
    );
    for (url, main) in [
        (
            "https://octosense.invalid/auth/callback?code=synthetic",
            false,
        ),
        (
            "https://octosense.invalid/auth/callback?code=synthetic#fragment",
            true,
        ),
        (
            "https://octosense.invalid/auth%2fcallback?code=synthetic",
            true,
        ),
        (
            "https://octosense.invalid/auth/callback/extra?code=synthetic",
            true,
        ),
        (
            "https://octosense.invalid:8443/auth/callback?code=synthetic",
            true,
        ),
        ("https://backend.example.evil.invalid/login", true),
    ] {
        assert_eq!(policy.decide(url, main), Block, "{url}");
    }
}
