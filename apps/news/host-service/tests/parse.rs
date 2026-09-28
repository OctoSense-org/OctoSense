//! Every source format, from fixtures: no network.
use octosense_news_service::parse::{self, Style};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn hacker_news_through_algolia() {
    let rows = parse::hn(&fixture("hn.json")).unwrap();
    assert_eq!(rows.len(), 2, "the untitled hit is skipped");
    assert_eq!(rows[0].title, "Show HN: A tiny news reader");
    assert_eq!(rows[0].url, "https://example.com/reader");
    assert_eq!(rows[0].discussion.as_deref(), Some("https://news.ycombinator.com/item?id=44000001"));
    assert_eq!((rows[0].points, rows[0].comments, rows[0].published), (Some(312), Some(145), Some(1789466400)));
    assert_eq!(rows[0].source, "Hacker News");
    // A text post links to its thread.
    assert_eq!(rows[1].url, "https://news.ycombinator.com/item?id=44000002");
    assert!(parse::hn("<html>").is_err());
}

#[test]
fn techmeme_names_the_outlet() {
    let rows = parse::feed(&fixture("techmeme.xml"), Style::Digest).unwrap();
    assert_eq!(rows.len(), 2, "an item without a title or link is skipped");
    assert_eq!(rows[0].title, "Acme & Co launches \"Widget 2\" for $99");
    assert_eq!(rows[0].summary, "Example: Acme launches Widget 2 — a $99 device.");
    assert_eq!(rows[0].source, "", "no outlet in the title: the service names the feed");
    assert_eq!(rows[0].url, "https://www.techmeme.com/260916/p1#a260916p1");
    assert_eq!(rows[0].published, Some(1789560901));
    assert_eq!(rows[1].title, "Second story with <brackets>");
    assert_eq!(rows[1].summary, "Plain text description.");
}

#[test]
fn google_news_splits_the_outlet_off_the_title() {
    let rows = parse::feed(&fixture("google.xml"), Style::Google).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0].title.as_str(), rows[0].source.as_str()), ("Markets rally as rates hold", "Reuters"));
    assert_eq!(rows[0].summary, "", "Google's description only repeats the headline and outlet");
    assert_eq!((rows[1].title.as_str(), rows[1].source.as_str()), ("A title with - a dash", "The Verge"));
    assert_eq!(rows[0].published, Some(1789561800));
}

#[test]
fn atom_entries() {
    let rows = parse::feed(&fixture("atom.xml"), Style::Plain).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].title, "First & foremost");
    assert_eq!(rows[0].url, "https://blog.example/posts/first", "the alternate link, not self");
    assert_eq!(rows[0].summary, "A short summary.");
    assert_eq!(rows[0].published, Some(1789540200));
    assert_eq!(rows[1].url, "https://blog.example/posts/second");
    assert_eq!(rows[1].summary, "Full content here.");
}

#[test]
fn rss_images_and_dates() {
    let rows = parse::feed(&fixture("bbc.xml"), Style::Plain).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].image.as_deref(), Some("https://ichef.bbci.co.uk/images/c1.jpg"), "thumbnails load over https");
    assert_eq!(rows[1].image.as_deref(), Some("https://ichef.bbci.co.uk/images/c2.jpg"));
    assert_eq!(rows[2].image.as_deref(), Some("https://img.example/big.jpg"), "not the pixel, not the audio enclosure");
    assert_eq!(rows[2].summary, "Text after.");
    assert_eq!(rows[2].published, Some(1789549200), "`UTC` as a zone");
}

#[test]
fn rss_1_0_rdf() {
    let rows = parse::feed(&fixture("rdf.xml"), Style::Plain).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Ein Artikel über Energie");
    assert_eq!(rows[0].summary, "Kurz & bündig.");
    assert_eq!(rows[0].published, Some(1789536600));
}

#[test]
fn not_a_feed() {
    assert!(parse::feed("<html><body>hi</body></html>", Style::Plain).is_err());
    assert!(parse::feed("not xml", Style::Plain).is_err());
    // An external entity is not fetched, and a billion laughs does not explode.
    let xxe = r#"<?xml version="1.0"?><!DOCTYPE r [<!ENTITY x SYSTEM "file:///etc/passwd">]><rss><channel><item><title>&x;</title><link>https://a.example/</link></item></channel></rss>"#;
    assert!(parse::feed(xxe, Style::Plain).map(|r| r.iter().all(|i| !i.title.contains("root"))).unwrap_or(true));
    let laughs = format!(
        r#"<?xml version="1.0"?><!DOCTYPE r [<!ENTITY a "{}">{}]><rss><channel><item><title>&j;</title><link>https://a.example/</link></item></channel></rss>"#,
        "lol".repeat(10),
        (b'b'..=b'j').map(|c| format!("<!ENTITY {} \"{}\">", c as char, format!("&{};", (c - 1) as char).repeat(10))).collect::<String>()
    );
    let _ = parse::feed(&laughs, Style::Plain);
}

#[test]
fn gdelt_article_list() {
    let rows = parse::gdelt(&fixture("gdelt.json")).unwrap();
    assert_eq!(rows.len(), 2, "a row without an http URL is skipped");
    assert_eq!(rows[0].title, "Elektroautos: Absatz steigt deutlich");
    assert_eq!(rows[0].source, "example.de");
    assert_eq!(rows[0].lang.as_deref(), Some("de"));
    assert_eq!(rows[0].published, Some(1789546500));
    assert_eq!(rows[0].image.as_deref(), Some("https://www.example.de/img/1.jpg"));
    assert_eq!(rows[1].lang.as_deref(), Some("fr"));
    assert_eq!(rows[1].image, None);
    assert!(parse::gdelt("{}").unwrap().is_empty(), "no matches is no articles");
    assert!(parse::gdelt("").unwrap().is_empty());
    assert!(parse::gdelt("Your search contained too short a keyword.").unwrap_err().starts_with("GDELT: Your search"));
}

#[test]
fn opml_subscriptions() {
    let feeds = parse::opml(&fixture("feeds.opml")).unwrap();
    assert_eq!(feeds.len(), 3);
    assert_eq!(feeds[0].url, "https://feeds.npr.org/1007/rss.xml");
    assert_eq!(feeds[0].title, "NPR Science");
    assert_eq!(feeds[0].topics, vec!["science"]);
    assert_eq!(feeds[2].topics, Vec::<String>::new());
    assert!(parse::opml(&fixture("atom.xml")).is_err());
}
