//! A cheap, deterministic check that a digest's summary only restates its
//! points.
//!
//! The model writes the points with citations, and the host checks those
//! (see `validate_digest`). The summary cites nothing, so it is where a model
//! adds statements no source backs: validation run 2 (27 Sep 2026) found
//! summaries that added a date, a forecast or a framing that no point
//! carried, while every point was clean. The digest prompt forbids that; this
//! check holds the line on the host, with no model call.
//!
//! The summary is split into sentences (`.`, `!`, `?` before a capital, and
//! `。！？`). Each sentence's **key terms** are its numbers (`945`, `3.5`,
//! `1,032` = `1032`), its names (words written with a capital or a digit,
//! other than an ordinary capitalized first word) and, in scripts written
//! without spaces, its two-character sequences without a function character
//! such as 和 or 的 (folded to Simplified Chinese). A sentence with no key term is judged on its other words of
//! four letters or more.
//!
//! - A sentence none of whose key terms appears in any point has no backing
//!   point: it is **dropped** from the summary. This is deliberately
//!   conservative: one shared name or number keeps a sentence.
//! - A kept sentence that carries a number, a name, or a run of five or more
//!   unspaced characters none of whose content pairs (pairs without a
//!   function character such as 和 or 的) is in a point, is **flagged**: kept,
//!   and counted, since the check cannot tell a new fact from a paraphrase.
//!
//! On the 24 digests of validation run 2 (81 summary sentences), it drops
//! none and flags 10; 5 of those carry a statement the judge found no point
//! backs (an invented date, an event, a place, a warning), and 5 are
//! paraphrases (`Saudi Arabia` for `Saudi`, `Q1` for `first-quarter`).
//!
//! What it cannot see: a sentence built from the points' own words that says
//! something they do not (a forecast moved to another day, a quote given to
//! the wrong speaker, a framing such as "enforcement is expanding"). Those
//! are left to the prompt.

use super::relevance::{is_unspaced, stem, tokens, Token, STOP_WORDS};
use std::collections::BTreeSet;

/// What the check found in one summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SummaryCheck {
    /// The summary with the dropped sentences removed.
    pub summary: String,
    /// Sentences in the model's summary.
    pub sentences: usize,
    /// 1-based numbers of the sentences dropped: no key term appears in any
    /// point.
    pub dropped: Vec<usize>,
    /// 1-based numbers of kept sentences carrying a number, name or phrase
    /// no point carries.
    pub flagged: Vec<usize>,
}

/// Checks `summary` against the texts of the digest's kept `points`.
pub fn check_summary(summary: &str, points: &[&str]) -> SummaryCheck {
    let index = Terms::of(&points.join("\n"), false);
    let mut check = SummaryCheck::default();
    let mut kept = String::new();
    for (n, sentence) in sentences(summary).into_iter().enumerate() {
        let n = n + 1;
        check.sentences += 1;
        let terms = Terms::of(sentence, true);
        let backed_number = terms.numbers.iter().any(|x| index.numbers.contains(x));
        let backed_name = terms
            .names
            .iter()
            .chain(&terms.first)
            .any(|w| index.has_word(w));
        let backed_run = terms.bigrams().any(|b| index.runs.contains(&b));
        let has_key_terms = !terms.numbers.is_empty()
            || !terms.names.is_empty()
            || terms.bigrams().next().is_some();
        let backed = if has_key_terms {
            backed_number || backed_name || backed_run
        } else {
            // Nothing specific to check: judged on its other words, and kept
            // when it has none (it cannot carry a fact).
            (terms.words.is_empty() && terms.first.is_none())
                || terms
                    .words
                    .iter()
                    .chain(&terms.first)
                    .any(|w| index.has_word(w))
        };
        if !backed {
            check.dropped.push(n);
            continue;
        }
        let new_number = terms.numbers.iter().any(|x| !index.numbers.contains(x));
        let new_name = terms.names.iter().any(|w| !index.has_word(w));
        let new_phrase = terms
            .run_list
            .iter()
            .any(|r| r.chars().count() >= 5 && !content_pairs(r).any(|b| index.runs.contains(&b)));
        if new_number || new_name || new_phrase {
            check.flagged.push(n);
        }
        kept.push_str(sentence);
        if !sentence.ends_with(unspaced_end) {
            kept.push(' ');
        }
    }
    check.summary = kept.trim_end().to_owned();
    check
}

fn unspaced_end(c: char) -> bool {
    matches!(c, '。' | '！' | '？' | '”' | '」' | '』' | '）')
}

/// The key terms and other words of a text.
#[derive(Default)]
struct Terms {
    /// Numbers, without thousands separators.
    numbers: BTreeSet<String>,
    /// Stemmed words written with a capital or a digit (not numbers alone).
    names: BTreeSet<String>,
    /// A capitalized first word: it backs the sentence when a point has it.
    first: Option<String>,
    /// Other stemmed words of four letters or more, not stop words.
    words: BTreeSet<String>,
    /// Runs of unspaced script, folded to Simplified, joined by spaces.
    runs: String,
    /// The same runs, one by one.
    run_list: Vec<String>,
}

impl Terms {
    /// `sentence`: the first word's capital is grammar, not a name, unless
    /// the word has two capitals or a digit (`EU`, `GB300`).
    fn of(text: &str, sentence: bool) -> Terms {
        let mut terms = Terms::default();
        for number in numbers(text) {
            terms.numbers.insert(number);
        }
        let mut first = sentence;
        for token in tokens(text) {
            match token {
                Token::Word(word, name) => {
                    let ordinary_first = first && !word_is_marked(text, &word);
                    first = false;
                    if word.chars().all(|c| c.is_ascii_digit()) {
                        continue;
                    }
                    let stemmed = stem(&word);
                    if !sentence {
                        // The points' index keeps every word.
                        terms.words.insert(stemmed);
                        continue;
                    }
                    if word.chars().count() < 2 || STOP_WORDS.contains(&word.as_str()) {
                        continue;
                    }
                    if word.starts_with(|c: char| c.is_ascii_digit()) {
                        // A number with its unit (`68b`, `20m`): the number
                        // is already a key term.
                        continue;
                    }
                    if name && ordinary_first {
                        // `Iran is …`: a name or grammar. It can back the
                        // sentence, but is never counted as new.
                        terms.first = Some(stemmed);
                    } else if name {
                        terms.names.insert(stemmed);
                    } else if word.chars().count() >= 4 {
                        terms.words.insert(stemmed);
                    }
                }
                Token::Run(run) => {
                    first = false;
                    terms.runs.push_str(&run);
                    terms.runs.push(' ');
                    terms.run_list.push(run);
                }
            }
        }
        terms.runs = terms.runs.trim_end().to_owned();
        terms
    }

    /// The content pairs of every run of two or more characters.
    fn bigrams(&self) -> impl Iterator<Item = String> + '_ {
        self.run_list.iter().flat_map(|r| content_pairs(r))
    }

    /// Whether the index has `word`, or a word one of them begins (at least
    /// four letters shared: `serbia` ~ `serbian`, `iran` ~ `iranian`).
    fn has_word(&self, word: &str) -> bool {
        if self.words.contains(word) {
            return true;
        }
        if word.chars().count() < 4 {
            return false;
        }
        self.words.iter().any(|w| {
            w.chars().count() >= 4 && (w.starts_with(word) || word.starts_with(w.as_str()))
        })
    }
}

/// Whether the first word, as written in `text`, has two capitals or a digit.
fn word_is_marked(text: &str, folded: &str) -> bool {
    let written: String = text
        .chars()
        .skip_while(|c| !c.is_alphanumeric())
        .take_while(|c| c.is_alphanumeric())
        .collect();
    let marked = written.chars().filter(|c| c.is_uppercase()).count() >= 2
        || written.chars().any(|c| c.is_numeric());
    marked && written.to_lowercase().chars().count() == folded.chars().count()
}

/// The two-character sequences of `run` without a function character.
fn content_pairs(run: &str) -> impl Iterator<Item = String> + '_ {
    let chars: Vec<char> = run.chars().collect();
    (0..chars.len().saturating_sub(1))
        .map(move |i| chars[i..i + 2].iter().collect::<String>())
        .filter(|pair: &String| !pair.chars().any(|c| FUNCTION_CHARS.contains(c)))
}

/// The numbers in `text`: digit runs with `.` or `,` between digits, the
/// commas removed (`1,032` = `1032`, `65.62`).
fn numbers(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut number = String::new();
        while i < chars.len() {
            let c = chars[i];
            if c.is_ascii_digit() {
                number.push(c);
            } else if matches!(c, '.' | ',')
                && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit())
                && !number.is_empty()
            {
                if c == '.' {
                    number.push('.');
                }
            } else {
                break;
            }
            i += 1;
        }
        let trimmed = number.trim_start_matches('0');
        let number = if trimmed.is_empty() || trimmed.starts_with('.') {
            format!("0{trimmed}")
        } else {
            trimmed.to_owned()
        };
        out.push(number);
    }
    out
}

/// Chinese function characters: a two-character sequence with one of them
/// (`浪和`, `的大`) says nothing about a phrase's content.
const FUNCTION_CHARS: &str = "的了和与及并或在是将也等对从向为被把其之而于以所着过个这那就都又还但";

/// Abbreviations after which a `.` does not end a sentence.
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "st", "jr", "sr", "gen", "gov", "sen", "rep", "prof", "inc", "corp",
    "co", "ltd", "vs", "no", "jan", "feb", "mar", "apr", "jun", "jul", "aug", "sep", "sept", "oct",
    "nov", "dec", "approx", "est",
];

/// Splits `text` into sentences, each with its closing quotes and its
/// trailing space.
pub fn sentences(text: &str) -> Vec<&str> {
    let text = text.trim();
    let mut out = Vec::new();
    let mut start = 0;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        let terminal = matches!(c, '。' | '！' | '？' | '.' | '!' | '?');
        if !terminal {
            i += 1;
            continue;
        }
        // Closing quotes and brackets belong to the sentence.
        let mut j = i + 1;
        while j < chars.len()
            && matches!(
                chars[j].1,
                '"' | '\'' | '”' | '’' | '」' | '』' | ')' | '）'
            )
        {
            j += 1;
        }
        let ends = if matches!(c, '。' | '！' | '？') {
            true
        } else {
            let mut k = j;
            while k < chars.len() && chars[k].1.is_whitespace() {
                k += 1;
            }
            let spaced = k > j;
            let next = chars.get(k).map(|&(_, n)| n);
            let starts_sentence = next.is_none_or(|n| {
                n.is_uppercase() || is_unspaced(n) || matches!(n, '"' | '“' | '\'' | '‘')
            });
            let abbreviation = c == '.' && {
                let word: String = text[start..at]
                    .chars()
                    .rev()
                    .take_while(|w| w.is_alphanumeric() || *w == '.')
                    .collect::<Vec<char>>()
                    .into_iter()
                    .rev()
                    .collect();
                word.chars().any(char::is_alphabetic)
                    && (word.contains('.')
                        || word.chars().count() == 1
                        || ABBREVIATIONS.contains(&word.to_lowercase().as_str()))
            };
            (spaced || next.is_none()) && starts_sentence && !abbreviation
        };
        if ends {
            let mut k = j;
            while k < chars.len() && chars[k].1.is_whitespace() {
                k += 1;
            }
            let end = chars.get(j).map_or(text.len(), |&(b, _)| b);
            let next_start = chars.get(k).map_or(text.len(), |&(b, _)| b);
            let sentence = text[start..end].trim();
            if !sentence.is_empty() {
                out.push(sentence);
            }
            start = next_start;
            i = k;
        } else {
            i = j;
        }
    }
    let rest = text[start..].trim();
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences_split_at_terminal_punctuation_not_abbreviations() {
        assert_eq!(
            sentences("U.S. crude rose 1.3% to $93.62. Brent rose. Talks begin Feb. 28 in Doha."),
            [
                "U.S. crude rose 1.3% to $93.62.",
                "Brent rose.",
                "Talks begin Feb. 28 in Doha."
            ]
        );
        assert_eq!(
            sentences("He said \"no deal.\" Oil rose 3.5x"),
            ["He said \"no deal.\"", "Oil rose 3.5x"]
        );
        assert_eq!(
            sentences("台风减弱。日本气象厅提醒警惕大浪！路径仍不确定"),
            ["台风减弱。", "日本气象厅提醒警惕大浪！", "路径仍不确定"]
        );
        assert_eq!(
            sentences("Mr. Waltz spoke. Then"),
            ["Mr. Waltz spoke.", "Then"]
        );
    }

    #[test]
    fn numbers_ignore_thousands_separators() {
        assert_eq!(
            numbers("€1,032 and 65.62, 3.5x, 08:00, 2026."),
            ["1032", "65.62", "3.5", "8", "0", "2026"]
        );
    }

    #[test]
    fn a_sentence_with_no_backing_point_is_dropped() {
        let points = [
            "Trump rejected Iran's proposal to reopen the Strait of Hormuz.",
            "Oil rose 1.3% to $93.62.",
        ];
        let check = check_summary(
            "Trump rejected Iran's proposal. Analysts at Goldman Sachs expect a recession in 2027. Oil rose 1.3%.",
            &points,
        );
        assert_eq!(check.sentences, 3);
        assert_eq!(check.dropped, [2]);
        assert!(check.flagged.is_empty(), "{check:?}");
        assert_eq!(
            check.summary,
            "Trump rejected Iran's proposal. Oil rose 1.3%."
        );
        // In Chinese: no two-character sequence of the sentence is in a point.
        let check = check_summary(
            "台风“舒力基”逼近冲绳。股市全面上涨。",
            &["台风“舒力基”28日接近冲绳和奄美地区。"],
        );
        assert_eq!(check.dropped, [2]);
        assert_eq!(check.summary, "台风“舒力基”逼近冲绳。");
    }

    #[test]
    fn restatements_are_kept_and_paraphrased_names_match() {
        let points = [
            "Serbian President Aleksandar Vucic resigned on Sunday ahead of the October 25 parliamentary election.",
            "Iran's Revolutionary Guards said they seized an American submersible drone.",
        ];
        let check = check_summary(
            "Serbia's president Vucic resigned before the 25 October vote. The Iranian Revolutionary Guards seized a US drone. Overall, it was a busy week.",
            &points,
        );
        // "US" is in no point: flagged, kept. A sentence of filler with no
        // key term and no word any point has is dropped.
        assert_eq!(check.flagged, [2]);
        assert_eq!(check.dropped, [3], "{check:?}");
        assert_eq!(check.sentences, 3);
        // A capitalized first word backs the sentence when a point has it,
        // and is never counted as a new name.
        let check = check_summary("Iran seized a drone. Iranian guards acted.", &points);
        assert!(
            check.dropped.is_empty() && check.flagged.is_empty(),
            "{check:?}"
        );
    }

    /// The summary drift validation run 2 found (27 Sep 2026,
    /// `validation2/judged/oe-tb-typhoon.json`), with the digest's points.
    const TYPHOON_POINTS: &[&str] = &[
        "今年第26号超强台风“舒力基”28日在维持现有强度的同时接近冲绳和奄美地区，中心气压945百帕，中心附近最大风速50米/秒，最大瞬间风速70米/秒。",
        "截至28日上午9点，台风伴随暴风圈在冲绳南大东岛以北约100公里处以15公里时速向东北偏东方向移动；另一报道称上午5时其位于南大东岛西北约130公里处，以15公里时速向东北移动。",
        "卫星图像显示台风中心有清晰的风眼，表明其仍维持极强强度；预计28日将经冲绳本岛东南海域向东北移动并保持强度，29日仍维持极强。",
        "大东群岛面临最直接威胁，28日清晨起强降雨云系逼近，当地可能于下午3时前后进入暴风圈，当局呼吁防范道路积水、低洼地区淹水、飞散物、树木倒伏和停电。",
        "日本气象厅提醒警惕伴有涌浪的大浪和强风，预计28日浪高冲绳7米、奄美4米，29日冲绳5米；28日最大风速冲绳23米/秒、29日15米/秒。",
        "预计10月1日台风将从八丈岛以南的伊豆群岛南部附近经过，10月2日下午抵达三陆海岸外海，10月3日在北海道以东变性为温带气旋。",
        "中国方面消息称，28日8时台风“舒力基”距琉球群岛那霸市偏东方向约325公里，以每小时15公里左右速度向东北移动，逐渐趋向日本以南洋面，强度将逐渐减弱，并将于30日在日本东南洋面变性为温带气旋。",
        "台风经过大东群岛后的预测路径仍有较大不确定性，视其走向也可能影响本州部分地区；同时因路径原因，风力恐进一步增强，需持续关注最新预报。",
    ];

    #[test]
    fn real_drift_typhoon() {
        let summary = "超强台风“舒力基”（今年第26号）9月28日继续向东北方向移动，维持极强强度，逼近冲绳、奄美及大东群岛，日本气象厅呼吁防范狂风、巨浪和暴雨。台风中心气压945百帕，最大风速50米/秒，阵风70米/秒；预计9月30日减弱，10月初经过伊豆群岛南部附近，随后东移，10月3日在北海道以东变性为温带气旋。其后续路径仍存在不确定性，可能影响日本本州等地。";
        let check = check_summary(summary, TYPHOON_POINTS);
        assert_eq!(check.sentences, 3);
        // Every sentence restates points, so none is dropped.
        assert!(check.dropped.is_empty(), "{check:?}");
        // Sentence 1 adds "狂风、巨浪和暴雨" ("gales, huge waves and
        // torrential rain"): the run 巨浪和暴雨 shares no two characters with
        // any point, so the sentence is flagged. Sentence 2's "预计9月30日
        // 减弱" moves the points' "30日 … 变性" and "逐渐减弱" together:
        // built from the points' own words, it is beyond this check.
        assert_eq!(check.flagged, [1]);
        assert_eq!(check.summary, summary);
        // The unbacked clause on its own has a backing name (日本气象厅) and
        // is kept, flagged; one with no backing at all is dropped.
        let check = check_summary("日本气象厅呼吁防范狂风、巨浪和暴雨。", TYPHOON_POINTS);
        assert_eq!(
            (check.dropped.len(), check.flagged.as_slice()),
            (0, &[1][..])
        );
        let check = check_summary("狂风、巨浪和暴雨袭击全境。", TYPHOON_POINTS);
        assert_eq!(check.dropped, [1]);
        assert_eq!(check.summary, "");
    }

    #[test]
    fn real_drift_invented_date() {
        // validation2/judged/oe-tb-vucic.json: "resigned on 27 September
        // 2026"; no point gives the date.
        let points = [
            "Vučić announced in Belgrade that he was resigning the office of president and would hand over his duties to the parliament speaker the next morning. His second term would otherwise have run until May 2027.",
            "Banned by the constitution from a third term, Vučić said he was resigning to lead his right-wing Serbian Progressive Party (SNS) in the 25 October early parliamentary election.",
            "Protests began after a concrete canopy at Novi Sad's newly reconstructed railway station collapsed on 1 November 2024, killing 16 people; demonstrators demanded accountability for the disaster.",
            "Analysts say the student group and Vučić are polling closely in a referendum-like atmosphere; also running are Vučić's Socialist allies, a pro-EU alliance and smaller groups.",
        ];
        let summary = "Serbia's President Aleksandar Vučić resigned on 27 September 2026 to run for prime minister in a 25 October snap parliamentary election, after nearly two years of student-led protests triggered by the November 2024 collapse of a railway station canopy in Novi Sad that killed 16 people. Barred from a third presidential term, he will lead his Serbian Progressive Party against a student-backed movement that analysts say is polling closely with him.";
        let check = check_summary(summary, &points);
        assert!(check.dropped.is_empty(), "{check:?}");
        assert_eq!(check.flagged, [1]);
    }

    #[test]
    fn real_drift_invented_event() {
        // validation2/judged/oe-tb-openai.json: "a resident AI assistant,
        // 'o', … at Dev Day": no point mentions Dev Day.
        let points = [
            "OpenAI said it has paused training of its latest artificial intelligence models as reports mount of AI agents going rogue.",
            "It is the second time in three months OpenAI has halted development of its models; the first came in July after disclosure of a cyberattack targeting AI startup Hugging Face.",
            "OpenAI removed usage multipliers from its ChatGPT Pro pricing page, renaming tiers Pro Standard and Pro More, while a code change adds a \"Pro Max\" plan reportedly priced at $500 a month.",
        ];
        let summary = "OpenAI paused training of its latest AI models, its second halt in three months. Separately, OpenAI is overhauling ChatGPT Pro pricing and is expected to launch a resident AI assistant, 'o', and a $500-a-month Pro Max tier at Dev Day.";
        let check = check_summary(summary, &points);
        assert!(check.dropped.is_empty(), "{check:?}");
        assert_eq!(check.flagged, [2]);
    }
}
