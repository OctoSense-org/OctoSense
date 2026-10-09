//! Writer's text rules: the pure functions in `apps/writer/bundle/main.splash`
//! (its "text rules" section; everything between the shared interface and the
//! first top-level `start_timeout(` is evaluated), run in a script VM. They
//! decide what the document list shows for a draft, how a draft becomes the
//! Markdown the `word` engine imports, what the saved and exported files are
//! called, how the preview reads `word.inspect`'s blocks, and what a refused
//! engine call tells the person.
use makepad_widgets::*;
use serde_json::json;

const WRITER: &str = include_str!("../../../apps/writer/bundle/main.splash");

/// Evaluate `expression` after Writer's functions; it must end in `.to_json()`.
fn writer_model(expression: &str) -> serde_json::Value {
    let source = WRITER.split_once("// END shared app interface\n").unwrap().1
        .split_once("\nstart_timeout(").unwrap().0;
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm {
        host: &mut host,
        bx: Box::new(ScriptVmBase::new()),
    };
    vm.bx.captured_errors = Some(Vec::new());
    let result = vm.with_instruction_limit(1_000_000, |vm| {
        vm.eval(ScriptMod {
            file: "writer_model_test.splash".into(),
            // The app reaches `regex` through the widgets prelude.
            code: format!("use mod.math.*\nuse mod.std.regex\n{source}\n{expression}\n;"),
            ..Default::default()
        })
    });
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    let json = vm
        .bx
        .heap
        .string_with(result, |_, value| value.to_string())
        .unwrap();
    serde_json::from_str(&json).unwrap()
}

/// `text` as a script string literal (the script reads JSON's escapes).
fn lit(text: &str) -> String {
    serde_json::to_string(text).unwrap()
}

fn call(function: &str, text: &str) -> serde_json::Value {
    writer_model(&format!("{function}({}).to_json()", lit(text)))
}

#[test]
fn writer_counts_words_as_the_engine_does() {
    assert_eq!(call("count_words", ""), json!(0));
    // Heading, bullet and quote markers are not words.
    assert_eq!(call("count_words", "# Field notes\n- Clean as you go.\n> A quote here."), json!(9));
    // A numbered item's "1." becomes the list's numbering in the document.
    assert_eq!(call("count_words", "1. Switch on the breaker.\n2) Open the skylights.\n  10. Start the kettle."), json!(10));
    assert_eq!(call("count_words", "Version 2.0 is out"), json!(4), "a number inside a line is a word");
}

#[test]
fn the_list_shows_a_title_and_a_flattened_snippet() {
    assert_eq!(call("title_of", "\n\n## Field notes from the *Alder* studio\nBody"), json!("Field notes from the Alder studio"));
    assert_eq!(call("title_of", "  \n"), json!(""));
    // Lines that end without punctuation get a dot between them.
    assert_eq!(call("snippet_of", "Title\n- one\n- two\nA sentence.\nNext"), json!("one · two · A sentence. Next"));
    assert_eq!(call("snippet_of", "Only a title"), json!(""));
    assert_eq!(writer_model(r#"clip("abcdef", 3).to_json()"#), json!("abc…"));
    assert_eq!(writer_model(r#"clip("abc", 3).to_json()"#), json!("abc"));
    let long = "A sentence that goes on. ".repeat(12);
    let snippet = call("snippet_of", &format!("Title\n{long}"));
    let snippet = snippet.as_str().unwrap();
    assert!(snippet.ends_with('…') && snippet.chars().count() <= 161, "{snippet}");
}

#[test]
fn a_draft_becomes_markdown_with_a_paragraph_per_line() {
    let md = |text: &str| call("markdown_of", text);
    assert_eq!(md("# Title\nFirst line.\nSecond line."), json!("# Title\n\nFirst line.\n\nSecond line."));
    // List items and table rows stay together.
    assert_eq!(md("Intro\n- a\n- b\nAfter"), json!("Intro\n\n- a\n- b\n\nAfter"));
    assert_eq!(md("1. one\n2. two"), json!("1. one\n2. two"));
    assert_eq!(md("| a | b |\n| - | - |\n| 1 | 2 |"), json!("| a | b |\n| - | - |\n| 1 | 2 |"));
    // A blank line stays one blank line; Windows line ends are dropped.
    assert_eq!(md("A\n\nB"), json!("A\n\nB"));
    assert_eq!(md("A\r\nB"), json!("A\n\nB"));
}

#[test]
fn indentation_never_turns_a_line_into_code() {
    let md = |text: &str| call("markdown_of", text);
    assert_eq!(md("- item\n        - deep item"), json!("- item\n   - deep item"));
    assert_eq!(md("Text\n\tTabbed text"), json!("Text\n\nTabbed text"));
    assert_eq!(md("Text\n    Four spaces"), json!("Text\n\nFour spaces"));
    assert_eq!(md("- a\n  - nested"), json!("- a\n  - nested"), "shallow indentation stays");
}

#[test]
fn a_long_draft_converts_across_slices() {
    // The app converts 240 lines per turn; the joins must not change at the seams.
    // Items come in pairs (10 and 11, …), so pairs straddle both seams (240|241, 480|481).
    let lines: Vec<String> = (1..=600).map(|n| if n % 10 <= 1 { format!("- item {n}") } else { format!("Line {n}.") }).collect();
    let mut want = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            let both_items = line.starts_with("- ") && lines[i - 1].starts_with("- ");
            want.push_str(if both_items { "\n" } else { "\n\n" });
        }
        want.push_str(line);
    }
    assert_eq!(call("markdown_of", &lines.join("\n")), json!(want));
}

#[test]
fn files_are_named_after_the_title() {
    assert_eq!(call("stem_of", "Q3: plan / budget?"), json!("Q3 plan budget"));
    assert_eq!(call("stem_of", "  "), json!("Untitled"));
    let stem = call("stem_of", &"word ".repeat(30));
    let stem = stem.as_str().unwrap();
    assert!(stem.chars().count() <= 60 && !stem.ends_with('…'), "{stem}");
    let docs = r#"[{id: "b" docx: "documents/Notes.docx"} {id: "c" docx: "documents/Notes 2.docx"}]"#;
    assert_eq!(writer_model(&format!(r#"docx_name({{id: "a" title: "Notes"}}, {docs}).to_json()"#)), json!("documents/Notes 3.docx"));
    assert_eq!(
        writer_model(&format!(r#"docx_name({{id: "b" title: "Notes"}}, {docs}).to_json()"#)),
        json!("documents/Notes.docx"),
        "a document keeps its own name when saved again"
    );
    assert_eq!(writer_model(r#"export_path("documents/Notes 2.docx", "pdf").to_json()"#), json!("exports/Notes 2.pdf"));
    assert_eq!(writer_model(r#"export_path("documents/Notes.docx", "odt").to_json()"#), json!("exports/Notes.odt"));
}

#[test]
fn an_edited_document_moves_to_the_front_once() {
    assert_eq!(
        writer_model(r#"front({id: "b" n: 2}, [{id: "a" n: 1} {id: "b" n: 0} {id: "c" n: 3}]).to_json()"#),
        json!([{"id": "b", "n": 2}, {"id": "a", "n": 1}, {"id": "c", "n": 3}])
    );
}

/// Blocks shaped like `word.inspect`'s: type, style and text always; list and
/// props only when set.
const BLOCKS: &str = r#"[
    {index: 0 type: "paragraph" style: "Title" text: "Field notes"}
    {index: 1 type: "paragraph" style: "Normal" text: "Every Thursday."}
    {index: 2 type: "paragraph" style: "Heading1" text: "What we agreed "}
    {index: 3 type: "paragraph" style: "ListParagraph" text: "Clean as you go." list: {num: 1 level: 0}}
    {index: 4 type: "paragraph" style: "Heading2" text: "Details"}
    {index: 5 type: "paragraph" style: "Quote" text: "A studio is a place."}
    {index: 6 type: "paragraph" style: "Heading1" text: "Next steps"}
    {index: 7 type: "paragraph" style: "Heading4" text: "Small print"}
    {index: 8 type: "table" style: "TableGrid" rows: 1 cols: 2 cells: [["a" "b"]]}
    {index: 9 type: "paragraph" style: "Heading2" text: "  "}
    {index: 10 type: "paragraph" style: "Normal" text: ""}
    {index: 11 type: "paragraph" style: "Normal" text: "" props: {borders: {bottom: {}}}}
]"#;

#[test]
fn the_outline_lists_the_headings_with_text() {
    assert_eq!(
        writer_model(&format!("outline_of({BLOCKS}).to_json()")),
        json!([
            {"level": 1, "text": "Field notes", "index": 0},
            {"level": 1, "text": "What we agreed", "index": 2},
            {"level": 2, "text": "Details", "index": 4},
            {"level": 1, "text": "Next steps", "index": 6}
        ])
    );
}

#[test]
fn a_section_runs_to_the_next_heading_of_its_level() {
    let section = |k: usize| writer_model(&format!("section_of(outline_of({BLOCKS}), {k}, 12).to_json()"));
    assert_eq!(section(0), json!({"start": 0, "stop": 2}));
    assert_eq!(section(1), json!({"start": 2, "stop": 6}), "a level-1 section holds its level-2 heading");
    assert_eq!(section(2), json!({"start": 4, "stop": 6}));
    assert_eq!(section(3), json!({"start": 6, "stop": 12}), "the last one runs to the end");
}

#[test]
fn the_preview_draws_each_block_by_its_style() {
    let mut kinds = Vec::new();
    for i in 0..12 {
        kinds.push(writer_model(&format!("let blocks = {BLOCKS}\nkind_of(blocks[{i}]).to_json()")));
    }
    assert_eq!(
        kinds,
        vec![
            json!("h1"), json!("para"), json!("h1"), json!("item"), json!("h2"), json!("quote"),
            json!("h1"), json!("h3"), json!("table"), json!("h2"), json!("gap"), json!("rule")
        ]
    );
    assert_eq!(writer_model(r#"list_indent({type: "paragraph" style: "ListParagraph" text: "x" list: {num: 2 level: 2}}).to_json()"#), json!(36));
    assert_eq!(writer_model(r#"list_indent({type: "paragraph" style: "Normal" text: "x"}).to_json()"#), json!(0));
}

#[test]
fn times_read_as_how_long_ago() {
    for (seconds, text) in [
        (0, json!("just now")),
        (59, json!("just now")),
        (60, json!("1 min ago")),
        (3599, json!("59 min ago")),
        (3600, json!("1 h ago")),
        (86399, json!("23 h ago")),
        (86400, json!("1 day ago")),
        (172799, json!("1 day ago")),
        (172800, json!("2 days ago")),
        (44 * 86400, json!("44 days ago")),
        // Older ones name the month (when(), with the clock).
        (45 * 86400, json!(null)),
    ] {
        assert_eq!(writer_model(&format!("ago({seconds}).to_json()")), text, "{seconds} s");
    }
    assert_eq!(writer_model("size_text(512).to_json()"), json!("512 bytes"));
    assert_eq!(writer_model("size_text(26361).to_json()"), json!("25 KB"));
    assert_eq!(writer_model("size_text(5500000).to_json()"), json!("5.2 MB"));
}

#[test]
fn a_refused_engine_call_says_what_to_do() {
    let message = |what: &str, error: &str| writer_model(&format!("engine_message({}, {}).to_json()", lit(what), lit(error)));
    // The runtime's refusal when a manifest lacks the capability.
    assert_eq!(
        message("save Word documents", r#"this app was not granted "word", which "word.convert" needs"#),
        json!("Writer can't save Word documents yet: the word engine on this device isn't open to apps. Your draft is safe in Writer.")
    );
    // App Hub's when no service answers (card-host has none).
    assert_eq!(
        message("show a preview", r#"no service answers "word" on this device"#),
        json!("Writer can't show a preview here: this device has no word engine. Your draft is safe in Writer.")
    );
    // Anything else is the engine's own words.
    assert_eq!(
        message("show a preview", "word.info: not a valid zip package: invalid Zip archive: Could not find EOCD"),
        json!("Writer couldn't show a preview. The engine said: word.info: not a valid zip package: invalid Zip archive: Could not find EOCD")
    );
}

#[test]
fn optional_fields_read_with_a_fallback() {
    assert_eq!(writer_model(r#"field("{\"docs\": [1]}".parse_json(), "docs", []).to_json()"#), json!([1]));
    assert_eq!(writer_model(r#"field({a: 1}, "a", 0).to_json()"#), json!(1));
    assert_eq!(writer_model(r#"field({a: 1}, "b", 0).to_json()"#), json!(0));
}
