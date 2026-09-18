//! The Hacker News front page from Algolia's search API: one JSON page of
//! hits with points, comment counts and times, so a widget needs one
//! request instead of the official API's one-per-story.

use crate::model::{is_http_url, Headline, HN_LABEL};
use makepad_widgets::makepad_micro_serde::*;

#[derive(Clone, Debug, Default, DeJson)]
struct Page {
    hits: Option<Vec<Hit>>,
}

#[derive(Clone, Debug, Default, DeJson)]
struct Hit {
    title: Option<String>,
    url: Option<String>,
    author: Option<String>,
    points: Option<f64>,
    num_comments: Option<f64>,
    created_at_i: Option<f64>,
    #[rename(objectID)]
    object_id: Option<String>,
}

pub fn parse(json: &str) -> Result<Vec<Headline>, String> {
    // Lenient: Algolia sends fields we do not model (tags, highlights).
    let page: Page = DeJson::deserialize_json_lenient(json).map_err(|e| format!("{e:?}"))?;
    Ok(page.hits.unwrap_or_default().into_iter().filter_map(row).collect())
}

/// One hit as a row; None when it has no title, or nowhere to link to.
fn row(hit: Hit) -> Option<Headline> {
    let title = hit.title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())?;
    // Only Algolia's numeric ids make an item URL: anything else would be
    // spliced into a link the row hands to a browser.
    let discussion = hit
        .object_id
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .map(|id| format!("https://news.ycombinator.com/item?id={id}"));
    // A text post links to its discussion.
    let link = hit.url.filter(|u| is_http_url(u)).or_else(|| discussion.clone())?;
    let points = hit.points.map(|p| p.max(0.0) as u32);
    let comments = hit.num_comments.map(|c| c.max(0.0) as u32);
    // Points and comments are the meta line's; the summary adds only what
    // it does not show. No author, no summary: the view says so.
    let summary = hit.author.map(|a| format!("Posted by {a}")).unwrap_or_default();
    Some(Headline {
        title,
        link,
        source: HN_LABEL.into(),
        // The model stamps the id when the rows land.
        source_id: String::new(),
        // Only a plausible unix time: nothing before 1970, nothing past year 5138.
        published: hit.created_at_i.filter(|t| *t > 0.0 && *t < 1e11).map(|t| t as i64),
        points,
        comments,
        summary,
        discussion,
        image: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = include_str!("../tests/fixtures/hn.json");

    #[test]
    fn hits_become_rows_with_points_comments_and_a_discussion_fallback_link() {
        let rows = parse(PAGE).unwrap();
        assert_eq!(rows.len(), 2, "an untitled hit is skipped");
        assert_eq!(rows[0].title, "Show HN: A tiny news reader");
        assert_eq!(rows[0].link, "https://example.com/reader");
        assert_eq!(rows[0].source, "Hacker News");
        assert_eq!(rows[0].points, Some(312));
        assert_eq!(rows[0].comments, Some(145));
        assert_eq!(rows[0].published, Some(1_789_466_400));
        assert_eq!(rows[0].summary, "Posted by pg", "points and comments are the meta line's; the discussion is a field");
        assert_eq!(rows[0].discussion.as_deref(), Some("https://news.ycombinator.com/item?id=44000001"));
        assert_eq!(rows[1].link, "https://news.ycombinator.com/item?id=44000002", "a text post links to its discussion");
        assert_eq!(rows[1].discussion.as_deref(), Some("https://news.ycombinator.com/item?id=44000002"));
    }

    #[test]
    fn not_json_is_an_error_and_no_hits_is_an_empty_page() {
        assert!(parse("<html>").is_err());
        assert!(parse(r#"{"hits": []}"#).unwrap().is_empty());
        assert!(parse(r#"{"nbHits": 0}"#).unwrap().is_empty(), "a missing hits array is an empty page");
    }

    #[test]
    fn an_implausible_time_is_no_time() {
        let rows = parse(
            r#"{"hits":[{"title":"Bad time","url":"https://u.example/1","created_at_i":-1e300},{"title":"Far future","url":"https://u.example/2","created_at_i":1e300},{"title":"Fine","url":"https://u.example/3","created_at_i":1789466400}]}"#,
        )
        .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].published, None);
        assert_eq!(rows[1].published, None);
        assert_eq!(rows[2].published, Some(1_789_466_400));
    }

    #[test]
    fn a_hit_whose_url_is_not_http_links_to_its_discussion() {
        let rows = parse(r#"{"hits":[{"title":"Odd","url":"httpx://x.example/1","objectID":"7"},{"title":"Ftp","url":"ftp://x.example/2","objectID":"8"},{"title":"Upper","url":"HTTP://x.example/3","objectID":"9"}]}"#).unwrap();
        assert_eq!(rows[0].link, "https://news.ycombinator.com/item?id=7", "httpx is not http");
        assert_eq!(rows[1].link, "https://news.ycombinator.com/item?id=8");
        assert_eq!(rows[2].link, "HTTP://x.example/3", "the scheme is case-insensitive");
    }

    #[test]
    fn a_discussion_needs_a_numeric_object_id() {
        let rows = parse(r#"{"hits":[{"title":"Odd id","url":"https://u.example/1","objectID":"abc\"><script>"},{"title":"Fine","url":"https://u.example/2","objectID":"42"}]}"#).unwrap();
        assert_eq!(rows[0].link, "https://u.example/1", "the row still links to its url");
        assert_eq!(rows[0].discussion, None, "a non-numeric id makes no discussion url");
        assert_eq!(rows[1].discussion.as_deref(), Some("https://news.ycombinator.com/item?id=42"));
        assert!(parse(r#"{"hits":[{"title":"Nowhere","url":null,"objectID":"x1"}]}"#).unwrap().is_empty(), "nor a fallback link");
    }

    #[test]
    fn a_hit_with_nowhere_to_link_is_skipped() {
        assert!(parse(r#"{"hits":[{"title":"Nowhere","url":null}]}"#).unwrap().is_empty());
        assert!(parse(r#"{"hits":[{"title":"Blank id","url":null,"objectID":""}]}"#).unwrap().is_empty());
        let rows = parse(r#"{"hits":[{"title":"Url only","url":"https://u.example/"}]}"#).unwrap();
        assert_eq!(rows[0].link, "https://u.example/");
        assert_eq!(rows[0].discussion, None, "no object id, no discussion");
        assert_eq!(rows[0].summary, "", "an unknown author leaves no summary");
    }
}
