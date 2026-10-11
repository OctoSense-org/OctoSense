//! Wasm Lab's functions: a stand-in for a developer's own Rust crate. Light
//! computing and algorithms an app calls from its script or its agent, built
//! for wasm32 by `../build.sh` and run by the shell's runtime.

use serde::{Deserialize, Serialize};

octosense_guest::abi!();

// ---- Markdown to HTML --------------------------------------------------------

/// CommonMark to HTML, with tables, task lists, strikethrough and footnotes.
pub fn md_to_html(input: &[u8]) -> Result<Vec<u8>, String> {
    use pulldown_cmark::{html, Options, Parser};
    let text = std::str::from_utf8(input).map_err(|_| "the text is not UTF-8".to_string())?;
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES;
    let mut out = String::with_capacity(text.len() * 3 / 2);
    html::push_html(&mut out, Parser::new_ext(text, options));
    Ok(out.into_bytes())
}
octosense_guest::export!(md_to_html);

// ---- Free meeting slots ------------------------------------------------------

/// A day's free slots of `duration` minutes between `day_start` and
/// `day_end`, every `step` minutes, around the `busy` intervals ("HH:MM").
#[derive(Deserialize)]
pub struct SlotRequest {
    pub day_start: String,
    pub day_end: String,
    pub duration: u32,
    #[serde(default = "fifteen")]
    pub step: u32,
    #[serde(default)]
    pub busy: Vec<[String; 2]>,
    #[serde(default = "ten")]
    pub max: usize,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Slots {
    pub slots: Vec<[String; 2]>,
}

fn fifteen() -> u32 {
    15
}

fn ten() -> usize {
    10
}

fn minutes(hhmm: &str) -> Result<u32, String> {
    let (h, m) = hhmm
        .split_once(':')
        .ok_or_else(|| format!("{hhmm} is not HH:MM"))?;
    let (h, m): (u32, u32) = (
        h.parse().map_err(|_| format!("{hhmm} is not HH:MM"))?,
        m.parse().map_err(|_| format!("{hhmm} is not HH:MM"))?,
    );
    if h > 24 || m > 59 || (h == 24 && m > 0) {
        return Err(format!("{hhmm} is not a time of day"));
    }
    Ok(h * 60 + m)
}

fn hhmm(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

pub fn find_slots(req: SlotRequest) -> Result<Slots, String> {
    let (start, end) = (minutes(&req.day_start)?, minutes(&req.day_end)?);
    if start >= end {
        return Err("the day ends before it starts".into());
    }
    if req.duration == 0 || req.step == 0 {
        return Err("the duration and the step must be at least a minute".into());
    }
    let mut busy = Vec::with_capacity(req.busy.len());
    for [from, to] in &req.busy {
        let (from, to) = (minutes(from)?, minutes(to)?);
        if from < to {
            busy.push((from.max(start), to.min(end)));
        }
    }
    busy.sort_unstable();
    // Merge overlapping and touching busy intervals.
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(busy.len());
    for (from, to) in busy {
        match merged.last_mut() {
            Some(last) if from <= last.1 => last.1 = last.1.max(to),
            _ => merged.push((from, to)),
        }
    }
    let mut slots = Vec::new();
    let mut free_from = start;
    for (from, to) in merged.into_iter().chain([(end, end)]) {
        // Candidate starts sit on the step grid from the start of the day.
        let mut at = start + (free_from - start).div_ceil(req.step) * req.step;
        while at + req.duration <= from && slots.len() < req.max {
            slots.push([hhmm(at), hhmm(at + req.duration)]);
            at += req.step;
        }
        free_from = free_from.max(to);
    }
    Ok(Slots { slots })
}
octosense_guest::export_json!(find_slots);

// ---- Fuzzy ranking -----------------------------------------------------------

/// The items that match `query`, best first: a prefix beats a substring, a
/// substring beats letters in order, and a near miss still ranks (typos).
#[derive(Deserialize)]
pub struct RankRequest {
    pub query: String,
    pub items: Vec<String>,
    #[serde(default = "ten")]
    pub limit: usize,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Ranked {
    pub item: String,
    pub score: f64,
}

/// An object, not a bare list, so an agent tool can return it.
#[derive(Serialize, Debug, PartialEq)]
pub struct Ranking {
    pub ranked: Vec<Ranked>,
}

fn score(query: &str, item: &str) -> f64 {
    let (q, it) = (query.to_lowercase(), item.to_lowercase());
    if q.is_empty() {
        return 0.0;
    }
    if it.starts_with(&q) {
        return 1.0 - (it.chars().count() - q.chars().count()) as f64 / 200.0;
    }
    if let Some(at) = it.find(&q) {
        // A match at a word start is nearly as good as a prefix.
        let word_start = it[..at].ends_with([' ', '-', '_', '.', '/']);
        return if word_start { 0.95 } else { 0.85 } - at as f64 / 200.0;
    }
    // Letters in order: tighter spans score higher.
    let mut chars = it.char_indices();
    let mut first = None;
    let mut last = 0;
    let mut in_order = true;
    for c in q.chars() {
        match chars.find(|(_, x)| *x == c) {
            Some((i, _)) => {
                first.get_or_insert(i);
                last = i;
            }
            None => {
                in_order = false;
                break;
            }
        }
    }
    if in_order {
        let span = (last - first.unwrap_or(0) + 1) as f64;
        return 0.5 + 0.3 * (q.len() as f64 / span).min(1.0);
    }
    strsim::jaro_winkler(&q, &it) * 0.6
}

pub fn fuzzy_rank(req: RankRequest) -> Result<Ranking, String> {
    let mut ranked: Vec<Ranked> = req
        .items
        .iter()
        .map(|item| Ranked {
            score: (score(&req.query, item) * 1000.0).round() / 1000.0,
            item: item.clone(),
        })
        .filter(|r| r.score >= 0.45)
        .collect();
    ranked.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.item.cmp(&b.item))
    });
    ranked.truncate(req.limit);
    Ok(Ranking { ranked })
}
octosense_guest::export_json!(fuzzy_rank);

// ---- Line diff ---------------------------------------------------------------

/// A unified diff of two texts, line by line, with `context` lines around
/// each change.
#[derive(Deserialize)]
pub struct DiffRequest {
    pub old: String,
    pub new: String,
    #[serde(default = "three")]
    pub context: usize,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Diff {
    pub unified: String,
    pub added: usize,
    pub removed: usize,
}

fn three() -> usize {
    3
}

pub fn text_diff(req: DiffRequest) -> Result<Diff, String> {
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(&req.old, &req.new);
    let (mut added, mut removed) = (0, 0);
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => added += 1,
            ChangeTag::Delete => removed += 1,
            ChangeTag::Equal => {}
        }
    }
    let unified = diff
        .unified_diff()
        .context_radius(req.context)
        .header("old", "new")
        .to_string();
    Ok(Diff {
        unified,
        added,
        removed,
    })
}
octosense_guest::export_json!(text_diff);

// ---- Misbehaviour, for the containment checks --------------------------------

#[derive(Deserialize)]
pub struct RogueRequest {
    pub mode: String,
}

/// Misbehaves on purpose: `loop` never returns, `alloc` takes memory until
/// it cannot, `panic` panics, `recurse` overflows the stack.
pub fn rogue(req: RogueRequest) -> Result<String, String> {
    match req.mode.as_str() {
        "loop" => {
            let mut n = 0u64;
            loop {
                n = std::hint::black_box(n.wrapping_add(1));
            }
        }
        "alloc" => {
            let mut hoard: Vec<Vec<u8>> = Vec::new();
            loop {
                hoard.push(vec![1; 16 << 20]);
                std::hint::black_box(&hoard);
            }
        }
        "panic" => panic!("rogue: a deliberate panic"),
        "recurse" => Ok(depth(0).to_string()),
        other => Err(format!("rogue has no mode {other}")),
    }
}
octosense_guest::export_json!(rogue);

#[allow(unconditional_recursion)]
fn depth(n: u64) -> u64 {
    std::hint::black_box(depth(std::hint::black_box(n + 1))) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(busy: &[[&str; 2]], duration: u32) -> Vec<[String; 2]> {
        let req = SlotRequest {
            day_start: "09:00".into(),
            day_end: "12:00".into(),
            duration,
            step: 30,
            busy: busy
                .iter()
                .map(|[a, b]| [a.to_string(), b.to_string()])
                .collect(),
            max: 10,
        };
        find_slots(req).unwrap().slots
    }

    #[test]
    fn free_slots_go_around_busy_time_on_the_step_grid() {
        let found = slots(
            &[["09:30", "10:15"], ["10:00", "10:40"], ["11:30", "12:30"]],
            30,
        );
        let found: Vec<String> = found.iter().map(|[a, b]| format!("{a}-{b}")).collect();
        assert_eq!(found, ["09:00-09:30", "11:00-11:30"]);
        assert_eq!(slots(&[], 60).len(), 5);
    }

    #[test]
    fn bad_times_are_refused() {
        let req = SlotRequest {
            day_start: "9am".into(),
            day_end: "12:00".into(),
            duration: 30,
            step: 15,
            busy: vec![],
            max: 10,
        };
        assert!(find_slots(req).is_err());
    }

    #[test]
    fn a_prefix_beats_a_substring_beats_letters_in_order() {
        let items = ["Calculator", "Calendar", "Mail", "Local calls", "Clock"]
            .map(String::from)
            .to_vec();
        let ranked = fuzzy_rank(RankRequest {
            query: "cal".into(),
            items,
            limit: 10,
        })
        .unwrap()
        .ranked;
        let names: Vec<&str> = ranked.iter().map(|r| r.item.as_str()).collect();
        assert_eq!(&names[..2], ["Calendar", "Calculator"]);
        assert!(names.contains(&"Local calls"));
        assert!(!names.contains(&"Mail"));
    }

    #[test]
    fn a_diff_counts_its_lines() {
        let diff = text_diff(DiffRequest {
            old: "a\nb\nc\n".into(),
            new: "a\nB\nc\nd\n".into(),
            context: 1,
        })
        .unwrap();
        assert_eq!((diff.added, diff.removed), (2, 1));
        assert!(diff.unified.contains("-b\n+B\n"));
    }

    #[test]
    fn markdown_becomes_html() {
        let html =
            String::from_utf8(md_to_html(b"# Hi\n\n- [x] done\n\n| a |\n|---|\n| 1 |\n").unwrap())
                .unwrap();
        assert!(
            html.contains("<h1>Hi</h1>") && html.contains("checkbox") && html.contains("<table>")
        );
    }
}
