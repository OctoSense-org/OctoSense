//! Quick Deck's rules: the pure functions in `apps/quickdeck/bundle/main.splash`
//! (everything between the shared interface and `start_timeout(`), run in a
//! script VM. They decide what an imported presentation is called, what the
//! library index keeps across a restart and what v1's decks become, the words
//! the status line, the notes and the outline counts show, what an outline and
//! a model's draft send to the engine, which slides a range names, where Move
//! earlier and Move later put slides, what a refusal says, and how a clip
//! plays: which shapes are clips, what deck.media's refusals and the Video
//! widget's failures say, and the clip's steps from the widget's own answers.
use makepad_widgets::*;
use serde_json::json;

const QUICK_DECK: &str = include_str!("../../../apps/quickdeck/bundle/main.splash");

/// Evaluate `expression` after Quick Deck's functions; it must end in `.to_json()`.
fn quickdeck_model(expression: &str) -> serde_json::Value {
    let source = QUICK_DECK.split_once("// END shared app interface\n").unwrap().1
        .split_once("\nstart_timeout(").unwrap().0;
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm {
        host: &mut host,
        bx: Box::new(ScriptVmBase::new()),
    };
    vm.bx.captured_errors = Some(Vec::new());
    let result = vm.with_instruction_limit(500_000, |vm| {
        vm.eval(ScriptMod {
            file: "quickdeck_model_test.splash".into(),
            // The app reaches `regex` and the math functions through the widgets prelude.
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

/// The script's math runs in single precision: compare to a ten-thousandth.
fn near(got: &serde_json::Value, want: f64, what: &str) {
    let got = got.as_f64().unwrap_or_else(|| panic!("{what}: {got} is not a number"));
    assert!((got - want).abs() < 1e-4, "{what}: {got} for {want}");
}

fn import_title(shown: &str, taken: &str) -> serde_json::Value {
    quickdeck_model(&format!("title_for_import({shown}, \"Imported presentation\", {taken}).to_json()"))
}

#[test]
fn an_imported_presentation_is_titled_with_the_chosen_files_name() {
    assert_eq!(import_title("\"Lease 2026.pptx\"", "[]"), json!("Lease 2026"));
    assert_eq!(import_title("\"Q3 Review.PPTX\"", "[]"), json!("Q3 Review"), "any case of the extension");
    assert_eq!(import_title("\" Plan.pptx \"", "[]"), json!("Plan"));
    assert_eq!(import_title("\"Board: Q3/Q4.pptx\"", "[]"), json!("Board Q3 Q4"), "no folder separators in a title");
    // Android, a name nothing was left of, or nothing but the extension.
    assert_eq!(import_title("nil", "[]"), json!("Imported presentation"));
    assert_eq!(import_title("\"\"", "[]"), json!("Imported presentation"));
    assert_eq!(import_title("\".pptx\"", "[]"), json!("Imported presentation"));
}

#[test]
fn a_title_another_deck_shows_gets_the_next_number() {
    assert_eq!(import_title("\"Lease 2026.pptx\"", "[\"Lease 2026\"]"), json!("Lease 2026 (2)"));
    assert_eq!(import_title("\"Lease 2026.pptx\"", "[\"Lease 2026 (2)\", \"Lease 2026\"]"), json!("Lease 2026 (3)"));
    assert_eq!(quickdeck_model("unique_title(\"Untitled deck\", [\"Untitled deck\"]).to_json()"), json!("Untitled deck 2"));
    assert_eq!(quickdeck_model("unique_title(\"Untitled deck\", []).to_json()"), json!("Untitled deck"));
}

#[test]
fn a_title_becomes_a_file_name_without_separators_or_control_characters() {
    assert_eq!(quickdeck_model("clean_title(\"Q3/Q4: plan\\nnow\").to_json()"), json!("Q3 Q4 plan now"));
    assert_eq!(quickdeck_model("clean_title(\"  two   spaces  \").to_json()"), json!("two spaces"));
    let long = "a".repeat(70);
    assert_eq!(quickdeck_model(&format!("clean_title(\"{long}\").to_json()")), json!("a".repeat(60)), "at most 60 characters");
}

#[test]
fn the_index_keeps_a_deck_across_a_restart() {
    // What save_index writes and load_index reads back: {version: 2 decks: index}.
    let restart = |path: &str, entry: &str| {
        quickdeck_model(&format!(
            "let kept = {{}}\nkept[\"{path}\"] = {entry}\n\
             let back = {{version: 2 decks: kept}}.to_json().parse_json()\n\
             deck_from(\"{path}\", optional(back.decks, \"{path}\", nil)).to_json()"
        ))
    };
    let deck = restart(
        "decks/Q3 2026 Board Update.deckcraft",
        "index_entry({title: \"Q3 2026 Board Update\" slides: 12 bytes: 3565159 theme: \"Harbor\" created: 100 edited: 200 \
         opened: 300 slide: 4 zoom: nil cover: \".cache/covers/q3.png\" exports: [{name: \"Q3 2026 Board Update.pdf\" bytes: 5033164}] v1: false})",
    );
    assert_eq!(deck["title"], json!("Q3 2026 Board Update"));
    assert_eq!(deck["slides"], json!(12));
    assert_eq!(deck["bytes"], json!(3565159));
    assert_eq!(deck["theme"], json!("Harbor"));
    assert_eq!(deck["slide"], json!(4), "the slide it was left at");
    assert_eq!(deck["zoom"], json!(null), "a fitted zoom fits again");
    assert_eq!(deck["exports"][0]["name"], json!("Q3 2026 Board Update.pdf"));
    assert_eq!(deck["import"], json!(false));
    // An imported presentation keeps the chosen file's name as its title; its
    // file keeps the name the app gave it.
    let deck = restart("imports/Imported presentation.pptx", "{title: \"Lease 2026\"}");
    assert_eq!(deck["title"], json!("Lease 2026"));
    assert_eq!(deck["import"], json!(true));
    assert_eq!(deck["slides"], json!(null), "the engine reads it on this launch");
    // A file the index doesn't know is titled with its name.
    let deck = quickdeck_model("deck_from(\"decks/Harbor tour.deckcraft\", nil).to_json()");
    assert_eq!(deck["title"], json!("Harbor tour"));
    assert_eq!(deck["exports"], json!([]));
}

#[test]
fn v1s_decks_join_the_library_titled_with_their_first_slide() {
    let entries = quickdeck_model(
        "v1_entries({decks: [\
            {file: \"decks/a1/g2/deck.deckcraft\" slides: [{title: \" Garden plan \"}] count: 5 made: 100 edited: 200}, \
            {file: \"decks/b2/g1/deck.deckcraft\" slides: [] count: 0}, \
            {file: \"notes.txt\"}]}).to_json()",
    );
    assert_eq!(
        entries,
        json!([
            {"path": "decks/a1/g2/deck.deckcraft", "title": "Garden plan", "v1": true, "created": 100, "edited": 200, "slides": 5},
            {"path": "decks/b2/g1/deck.deckcraft", "title": "Untitled deck", "v1": true, "created": 0, "edited": 0}
        ])
    );
    assert_eq!(quickdeck_model("v1_entries(nil).to_json()"), json!([]));
}

#[test]
fn the_library_shows_the_most_recent_deck_first() {
    let order = quickdeck_model(
        "let out = [] for d in newest_first([{title: \"a\" edited: 1}, {title: \"b\" opened: 5}, {title: \"c\" edited: 3 opened: 2}]) { out.push(d.title) } out.to_json()",
    );
    assert_eq!(order, json!(["b", "c", "a"]));
}

#[test]
fn the_status_line_and_the_cards_say_sizes_as_the_designs_do() {
    // 01-home and the status lines: "24.8 of 64 MB used", "12 slides · 3.4 MB".
    assert_eq!(quickdeck_model("used_text(26004685, 67108864).to_json()"), json!("24.8 of 64 MB used"));
    assert_eq!(quickdeck_model("used_text(92160, 67108864).to_json()"), json!("90 KB of 64 MB used"));
    assert_eq!(quickdeck_model("deck_meta(12, 3565159).to_json()"), json!("12 slides  ·  3.4 MB"));
    assert_eq!(quickdeck_model("deck_meta(1, nil).to_json()"), json!("1 slide"));
    assert_eq!(quickdeck_model("deck_meta(nil, nil).to_json()"), json!(""));
    assert_eq!(
        quickdeck_model("usage_from({used_bytes: 26004685 quota_bytes: 67108864}).to_json()"),
        json!({"used": 26004685, "quota": 67108864})
    );
    for data in ["{}", "nil", "{used_bytes: 5}", "{used_bytes: 5 quota_bytes: 0}"] {
        assert_eq!(quickdeck_model(&format!("usage_from({data}).to_json()")), json!(null), "{data}");
    }
}

#[test]
fn speaker_notes_say_how_long_they_take() {
    // 10-dark-notes: the 28 words of slide 6's notes, "about 10 seconds".
    let notes = "Total membership passed 18,000 for the first time. Northwest added 460 members, most through the referral \
                 program. Ask Jun Park to say a word about the Metro waitlist.";
    assert_eq!(quickdeck_model(&format!("notes_meta(\"{notes}\").to_json()")), json!("28 words  ·  about 10 seconds"));
    assert_eq!(quickdeck_model("notes_meta(\"Thank the board.\").to_json()"), json!("3 words  ·  about 1 second"));
    let long = "word ".repeat(300);
    assert_eq!(quickdeck_model(&format!("notes_meta(\"{long}\").to_json()")), json!("300 words  ·  about 2 minutes"));
    assert_eq!(quickdeck_model("notes_meta(\"  \").to_json()"), json!("No notes yet"));
}

#[test]
fn presents_timer_counts_minutes_and_hours() {
    assert_eq!(quickdeck_model("clock_text(402).to_json()"), json!("06:42"), "08-present");
    assert_eq!(quickdeck_model("clock_text(3725).to_json()"), json!("1:02:05"));
    assert_eq!(quickdeck_model("clock_text(-3).to_json()"), json!("00:00"));
}

const CARDS: &str = "[\
    {title: \"Q3 2026 Board Update\" points: \"Northwind Cooperative · 21 October 2026\" notes: \"\"}, \
    {title: \"Agenda\" points: \"Highlights\\nRevenue\\n\\nMembership\\nOperations\\nOutlook\" notes: \"\"}, \
    {title: \"Quarter at a glance\" points: \"Revenue up 12%\\n18,400 members, up 6%\\n96% on-time delivery\" notes: \" Lead with the headline. \"}, \
    {title: \"\" points: \"\" notes: \"\"}]";

#[test]
fn the_outline_counts_its_slides_and_points() {
    // 07-outline: "10 slides · 31 points"; empty cards and empty lines don't count.
    assert_eq!(quickdeck_model(&format!("counts_text({CARDS}).to_json()")), json!("3 slides  ·  9 points"));
    assert_eq!(quickdeck_model(&format!("outline_problem({CARDS}).to_json()")), json!(""));
}

#[test]
fn an_outline_needs_titles_before_it_becomes_a_deck() {
    let problem = |cards: &str| quickdeck_model(&format!("outline_problem({cards}).to_json()"));
    assert_eq!(problem("[{title: \"\" points: \"\"}]"), json!("Give the first slide a title to start."));
    assert_eq!(problem("[{title: \"\" points: \"A subtitle\"}]"), json!("Give the first slide a title: it's the deck's title."));
    assert_eq!(problem("[{title: \"Q3\" points: \"\"}, {title: \"\" points: \"Revenue\"}]"), json!("Slide 2 has points but no title."));
}

#[test]
fn an_outline_becomes_a_title_slide_and_title_and_content_slides() {
    // SERVICE.md deck.create's slides: the first card's points are the subtitle.
    assert_eq!(
        quickdeck_model(&format!("outline_slides({CARDS}).to_json()")),
        json!([
            {"layout": "Title Slide", "title": "Q3 2026 Board Update", "body": "Northwind Cooperative · 21 October 2026"},
            {"layout": "Title and Content", "title": "Agenda", "body": "Highlights\nRevenue\nMembership\nOperations\nOutlook"},
            {"layout": "Title and Content", "title": "Quarter at a glance", "body": "Revenue up 12%\n18,400 members, up 6%\n96% on-time delivery",
             "notes": "Lead with the headline."}
        ])
    );
}

#[test]
fn a_models_draft_becomes_outline_cards() {
    // Draft with AI: model.complete fills outline_schema; the cards take what
    // it gave, cleaned, and never more than were asked for.
    let draft = "{slides: [\
        {title: \" Q3 update \" points: [\"Revenue  up 12%\", \" \", \"Members up 6%\"] notes: \" Thank the board. \"}, \
        {title: \"\" points: [\"no title\"]}, \
        {title: \"Outlook\" points: []}]}";
    assert_eq!(
        quickdeck_model(&format!("cards_from_model({draft}, 30).to_json()")),
        json!([
            {"title": "Q3 update", "points": "Revenue up 12%\nMembers up 6%", "notes": "Thank the board."},
            {"title": "Outlook", "points": "", "notes": ""}
        ])
    );
    assert_eq!(quickdeck_model(&format!("cards_from_model({draft}, 1).len().to_json()")), json!(1));
    assert_eq!(quickdeck_model("cards_from_model({}, 30).to_json()"), json!(null));
    assert_eq!(quickdeck_model("cards_from_model({slides: []}, 30).to_json()"), json!(null));
    let schema = quickdeck_model("outline_schema(12).to_json()");
    assert_eq!(schema["properties"]["slides"]["maxItems"], json!(12));
    assert_eq!(schema["properties"]["slides"]["items"]["required"], json!(["title", "points"]));
}

#[test]
fn a_slide_range_names_the_slides_it_takes() {
    let slides = |text: &str, total: u32| quickdeck_model(&format!("range_numbers(\"{text}\", {total}).to_json()"));
    assert_eq!(slides("", 3), json!([1, 2, 3]), "empty is every slide");
    assert_eq!(slides("All", 2), json!([1, 2]));
    assert_eq!(slides("2-6", 12), json!([2, 3, 4, 5, 6]), "09-export's example");
    assert_eq!(slides("1, 3, 5–7", 12), json!([1, 3, 5, 6, 7]), "a typed dash");
    assert_eq!(slides("3,3", 5), json!([3]));
    for bad in ["x", "13", "4-2", "0", "1-2-3", "2.5"] {
        assert_eq!(slides(bad, 12), json!(null), "{bad} names no slide this deck has");
    }
}

#[test]
fn a_refusal_is_read_by_its_code() {
    // SERVICE.md "Errors": `code: words`, perhaps after the method's name.
    assert_eq!(quickdeck_model("err_code(\"deck.open: protected: The file is encrypted\").to_json()"), json!("protected"));
    assert_eq!(quickdeck_model("err_words(\"deck.open: protected: The file is encrypted\").to_json()"), json!("The file is encrypted"));
    assert_eq!(quickdeck_model("err_code(\"unsaved: save or discard first\").to_json()"), json!("unsaved"));
    assert_eq!(quickdeck_model("err_code(\"no service answers deck.open\").to_json()"), json!(""));
    assert_eq!(quickdeck_model("err_words(\"no service answers deck.open\").to_json()"), json!("no service answers deck.open"));
    let state = |code: &str| quickdeck_model(&format!("open_state(\"{code}\").to_json()"));
    assert_eq!(state("protected"), json!("protected"));
    assert_eq!(state("held"), json!("held"));
    assert_eq!(state("unsupported"), json!("unsupported"));
    assert_eq!(state("too_many_open"), json!("full"));
    assert_eq!(state("something new"), json!("damaged"));
    assert_eq!(
        quickdeck_model("open_head(\"protected\").to_json()"),
        json!("This presentation is protected with a password")
    );
    assert_eq!(quickdeck_model("open_head(\"full\").to_json()"), json!("Quick Deck has 8 decks open"));
}

#[test]
fn a_refused_import_says_what_to_do() {
    let words = |error: &str, what: &str, limit: &str| {
        quickdeck_model(&format!("import_refusal(\"{error}\", \"{what}\", \"{limit}\").to_json()"))
    };
    assert_eq!(
        words("foreground_required: the window is not in front", "presentation", "64 MB"),
        json!("Bring Quick Deck to the front, then choose the file again.")
    );
    assert_eq!(words("too large", "presentation", "64 MB"), json!("That file is too big to add. 64 MB can be added."));
    assert_eq!(
        words("storage_full: 3 KB left", "picture", ""),
        json!("Quick Deck's storage is full: remove a deck or an export to make room, then try again.")
    );
    assert_eq!(words("something else", "picture", ""), json!("OctoSense couldn't copy the picture in."));
}

#[test]
fn the_engines_names_read_as_the_designs_do() {
    assert_eq!(quickdeck_model("theme_label(\"DeckCraft\").to_json()"), json!("Default"));
    assert_eq!(quickdeck_model("theme_label(\"Harbor\").to_json()"), json!("Harbor"));
    let align = |a: &str| quickdeck_model(&format!("align_name(\"{a}\").to_json()"));
    assert_eq!(align("l"), json!("left"));
    assert_eq!(align("ctr"), json!("center"));
    assert_eq!(align("r"), json!("right"));
    assert_eq!(align("just"), json!("justify"));
    assert_eq!(align("dist"), json!("justify"));
    assert_eq!(align(""), json!(""));
    near(&quickdeck_model("spacing_of({pct: 1.2}).to_json()"), 1.2, "the engine's {pct}");
    near(&quickdeck_model("spacing_of(1.5).to_json()"), 1.5, "a number");
    near(&quickdeck_model("spacing_of(nil).to_json()"), 0.0, "nothing said");
    let transition = |kind: &str| quickdeck_model(&format!("transition_label({kind}).to_json()"));
    assert_eq!(transition("\"fade\""), json!("Fade"));
    assert_eq!(transition("\"\""), json!("None"));
    assert_eq!(transition("nil"), json!("None"));
    assert_eq!(transition("\"dissolve\""), json!("dissolve"), "one the build has no word for");
    let chart = |kind: &str| quickdeck_model(&format!("chart_id(\"{kind}\").to_json()"));
    assert_eq!(chart("Clustered Column"), json!("column"));
    assert_eq!(chart("column"), json!("column"));
    assert_eq!(chart("Clustered Bar"), json!("bar"));
    assert_eq!(chart("Line with Markers"), json!("line"));
    assert_eq!(chart("donut"), json!("doughnut"));
    assert_eq!(chart("XY"), json!("scatter"));
    assert_eq!(quickdeck_model("hex_of(\"1C2D3F\").to_json()"), json!("#1c2d3f"));
    assert_eq!(quickdeck_model("hex_of(\"#FFAA00\").to_json()"), json!("#ffaa00"));
    assert_eq!(quickdeck_model("hex_of(\"#abc\").to_json()"), json!(""));
}

#[test]
fn the_offered_choices_are_the_engines() {
    // deckcraft model/src/anim.rs TRANSITIONS, insert.chart's types, and the
    // handouts' slides per page (SERVICE.md "Export").
    let ids = |list: &str| {
        let value = quickdeck_model(&format!("{list}.to_json()"));
        value.as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect::<Vec<_>>()
    };
    assert_eq!(ids("TRANSITIONS"), ["none", "fade", "push", "wipe", "reveal", "morph"]);
    assert_eq!(ids("CHARTS"), ["column", "bar", "line", "area", "pie", "doughnut", "scatter"]);
    assert_eq!(quickdeck_model("PER_PAGE.to_json()"), json!([1, 2, 3, 4, 6, 9]));
}

#[test]
fn the_slide_fits_the_canvas_as_the_designs_draw_it() {
    // At 1536 x 1024 the canvas is the window less the navigator (280), the
    // inspector (344), the bars and their lines; a 960 x 540 slide fits at 88%.
    assert_eq!(quickdeck_model("canvas_size(1536, 1024, 280, 344, 0).to_json()"), json!([912, 871]));
    near(&quickdeck_model("fit_zoom(960, 540, 912, 871).to_json()"), 0.88, "the pill's Fit");
    assert_eq!(quickdeck_model("zoom_text(0.88).to_json()"), json!("88%"));
    near(&quickdeck_model("zoom_step(0.88, 1).to_json()"), 1.0, "zoom in");
    near(&quickdeck_model("zoom_step(0.88, -1).to_json()"), 0.75, "zoom out");
    near(&quickdeck_model("zoom_step(3.0, 1).to_json()"), 3.0, "at most 300%");
    near(&quickdeck_model("zoom_step(0.25, -1).to_json()"), 0.25, "at least 25%");
    // The navigator's and the sorter's window of slides around the current one.
    assert_eq!(quickdeck_model("window_start(12, 9, 10).to_json()"), json!(2));
    assert_eq!(quickdeck_model("window_start(100, 50, 40).to_json()"), json!(46));
    assert_eq!(quickdeck_model("window_start(5, 3, 40).to_json()"), json!(0));
}

#[test]
fn a_drag_on_the_selection_moves_resizes_and_rotates_its_box() {
    assert_eq!(quickdeck_model("resized([60, 132, 400, 300], \"se\", 10, 20).to_json()"), json!([60, 132, 410, 320]));
    assert_eq!(quickdeck_model("resized([60, 132, 400, 300], \"nw\", 500, 0).to_json()"), json!([456, 132, 4, 300]), "never under 4 points");
    assert_eq!(quickdeck_model("moved([60, 132, 400, 300], -10, 5.2).to_json()")[1], json!(137.2));
    // The rotate handle, 24 points above the top edge, dragged round the centre.
    let rotation = |dx: i32, dy: i32| quickdeck_model(&format!("rotation_to([0, 0, 100, 100], 24, {dx}, {dy}).to_json()"));
    assert_eq!(rotation(0, 0), json!(0));
    assert_eq!(rotation(100, 74), json!(90));
    assert_eq!(rotation(0, 148), json!(180));
    assert_eq!(rotation(-100, 74), json!(270));
}

#[test]
fn the_charts_grid_sends_numbers_and_leaves_out_what_isnt_one() {
    assert_eq!(
        quickdeck_model("grid_of({categories: [\"Northwest\", \"Coast\"] series: [{name: \"2026\" values: [12.4, nil]}]}).to_json()"),
        json!({"categories": ["Northwest", "Coast"], "series": [{"name": "2026", "values": ["12.4", ""]}]})
    );
    let args = quickdeck_model(
        "chart_args({categories: [\"Q1\", \"Q2\", \"Q3\"] series: [{name: \"Revenue\" values: [\"4.2\", \"x\", \"\"]}]}).to_json()",
    );
    assert_eq!(args["categories"], json!(["Q1", "Q2", "Q3"]));
    near(&args["series"][0]["values"][0], 4.2, "a number");
    assert_eq!(args["series"][0]["values"][1], json!(null), "not a number");
    assert_eq!(args["series"][0]["values"][2], json!(null), "empty");
}

#[test]
fn move_earlier_and_later_put_the_chosen_slides_next_to_their_neighbours() {
    let slides = "[{id: \"a\"}, {id: \"b\"}, {id: \"c\"}, {id: \"d\"}]";
    assert_eq!(quickdeck_model(&format!("positions_of({slides}, [\"c\", \"a\"]).to_json()")), json!([0, 2]));
    let target = |ids: &str, dir: i32| quickdeck_model(&format!("move_target({slides}, {ids}, {dir}).to_json()"));
    assert_eq!(target("[\"b\", \"c\"]", -1), json!(0));
    assert_eq!(target("[\"b\", \"c\"]", 1), json!(2), "after d, once b and c are taken out");
    assert_eq!(target("[\"a\"]", -1), json!(null), "already first");
    assert_eq!(target("[\"d\"]", 1), json!(null), "already last");
    assert_eq!(target("[]", 1), json!(null));
    assert_eq!(quickdeck_model(&format!("ids_at({slides}, [1, 3, 9]).to_json()")), json!(["a", "c"]));
    // 06-sorter's section headers.
    assert_eq!(quickdeck_model("section_text(\"Highlights\", 7).to_json()"), json!("Highlights  ·  7 slides"));
    assert_eq!(quickdeck_model("section_text(\"\", 1).to_json()"), json!("Untitled section  ·  1 slide"));
}

/// The designs are drawn at 1536 x 1024, and the shipped manifest asks the
/// desktop to open Quick Deck's window at that size: App Hub's `window`,
/// which needs `schema_minor` 1. The desk clamps it (`desktop_layout`).
#[cfg(any(feature = "app-hub", native_mobile))]
#[test]
fn the_shipped_manifest_asks_for_the_designs_window_size() {
    let manifest = octosense_app_contract::AppManifest::parse(include_str!("../../../apps/quickdeck/bundle/manifest.json")).unwrap();
    assert_eq!(manifest.schema_minor, 1);
    assert_eq!(manifest.window, Some(octosense_app_contract::WindowHint::new(1536, 1024)));
    assert_eq!(manifest.capabilities, ["storage", "files", "deck", "model"]);
}

#[test]
fn a_chart_shows_its_own_elements_and_a_change_on_its_way() {
    // SERVICE.md "Inspector states (#507)": deck.slide's chart says legend
    // (a position or "none"), data_labels, gridlines and palette.
    let state = |chart: &str, pending: &str| quickdeck_model(&format!("chart_state({chart}, {pending}).to_json()"));
    assert_eq!(
        state("{title: \"Revenue\" legend: \"none\" data_labels: true gridlines: true palette: \"colorful1\"}", "{}"),
        json!({"title": true, "legend": false, "data_labels": true, "gridlines": true, "palette": "colorful1", "mono": false}),
        "design 03's chart"
    );
    assert_eq!(state("{title: \"\" legend: \"r\" data_labels: false gridlines: false palette: \"monochrome3\"}", "{}")["legend"], json!(true));
    assert_eq!(state("{title: \"\" legend: \"r\" palette: \"monochrome3\"}", "{}")["mono"], json!(true));
    // An older service said none of it: the engine's defaults (a legend
    // below, gridlines, no data labels, colourful).
    assert_eq!(
        state("{title: \"Chart Title\"}", "{}"),
        json!({"title": true, "legend": true, "data_labels": false, "gridlines": true, "palette": "colorful1", "mono": false})
    );
    // A change shows at once, until deck.slide answers.
    let pending = state("{title: \"\" legend: \"b\" data_labels: false gridlines: true}", "{legend: \"none\" data_labels: true title: true}");
    assert_eq!(pending["legend"], json!(false));
    assert_eq!(pending["data_labels"], json!(true));
    assert_eq!(pending["title"], json!(true));
    assert_eq!(quickdeck_model("legend_shown(nil).to_json()"), json!(false));
}

#[test]
fn a_table_shows_its_own_options() {
    let options = |table: &str, pending: &str| quickdeck_model(&format!("table_options({table}, {pending}).to_json()"));
    // Design 10's table: a header row, banded rows, a total row.
    assert_eq!(
        options("{options: {header_row: true total_row: true banded_rows: true first_column: false last_column: false banded_columns: false}}", "{}"),
        json!({"header_row": true, "total_row": true, "banded_rows": true, "first_column": false, "last_column": false, "banded_columns": false})
    );
    // Without options (an older service): the engine's defaults.
    assert_eq!(
        options("{}", "{}"),
        json!({"header_row": true, "total_row": false, "banded_rows": true, "first_column": false, "last_column": false, "banded_columns": false})
    );
    assert_eq!(options("{options: {first_column: false}}", "{first_column: true}")["first_column"], json!(true), "a change on its way");
}

#[test]
fn a_box_shows_its_anchor_and_autofit() {
    let state = |fmt: &str| quickdeck_model(&format!("box_state({fmt}).to_json()"));
    assert_eq!(state("{anchor: \"top\" autofit: \"shrink\"}"), json!({"anchor": "top", "autofit": "shrink", "shrink": true}), "design 02's box");
    assert_eq!(state("{anchor: \"middle\" autofit: \"resize\"}"), json!({"anchor": "middle", "autofit": "resize", "shrink": false}));
    assert_eq!(state("{anchor: \"distributed\" autofit: \"none\"}")["anchor"], json!("distributed"), "an imported deck's");
    assert_eq!(state("nil"), json!({"anchor": "", "autofit": "", "shrink": false}), "before format_state answers");
}

#[test]
fn the_footer_and_the_marks_follow_the_deck() {
    let footer = |f: &str, pending: &str| quickdeck_model(&format!("footer_state({f}, {pending}).to_json()"));
    // Design 05: slide numbers on, no date.
    assert_eq!(
        footer("{slide_numbers: true footer: true footer_text: \"Northwind Cooperative\" date: false hide_on_title: false}", "{}"),
        json!({"slide_numbers": true, "date": false, "footer": true, "footer_text": "Northwind Cooperative"})
    );
    assert_eq!(footer("nil", "{}"), json!({"slide_numbers": false, "date": false, "footer": false, "footer_text": ""}));
    assert_eq!(footer("{date: false}", "{date: true}")["date"], json!(true), "a change on its way");
    // The colour set: one picked and on its way, else the deck's, else its theme's.
    let colours = |chosen: &str, scheme: &str| quickdeck_model(&format!("scheme_mark(\"{chosen}\", \"{scheme}\", \"Harbor\").to_json()"));
    assert_eq!(colours("", "Ocean"), json!("Ocean"));
    assert_eq!(colours("Berry", "Ocean"), json!("Berry"));
    assert_eq!(colours("", ""), json!("Harbor"));
    // The font pair: by name, else by the deck's families (a built-in theme's
    // own pair is named after the theme; Harbor's fonts are Classic's).
    let pairs = "[{name: \"Inter\" major: \"Inter\" minor: \"Inter\"}, {name: \"Classic\" major: \"Merriweather\" minor: \"Open Sans\"}, \
                 {name: \"Humanist\" major: \"Merriweather\" minor: \"Open Sans\"}]";
    let fonts = |chosen: &str, scheme: &str, major: &str, minor: &str| {
        quickdeck_model(&format!("font_mark(\"{chosen}\", \"{scheme}\", {pairs}, \"{major}\", \"{minor}\").to_json()"))
    };
    assert_eq!(fonts("", "Humanist", "Merriweather", "Open Sans"), json!("Humanist"), "the deck's pair by name");
    assert_eq!(fonts("", "Harbor", "Merriweather", "Open Sans"), json!("Classic"), "the theme's own pair by its families");
    assert_eq!(fonts("Inter", "Harbor", "Merriweather", "Open Sans"), json!("Inter"), "a pick on its way");
    assert_eq!(fonts("", "Office", "Calibri Light", "Calibri"), json!(""), "an imported deck's own fonts");
}

#[test]
fn a_held_deck_is_explained_without_media() {
    // #511: audio and video open now; held: is a zip inside the zip or a
    // part past the import guard's bounds.
    let detail = quickdeck_model("open_detail(\"held\").to_json()");
    let text = detail.as_str().unwrap();
    assert!(text.contains("zip") && text.contains("simpler copy") && !text.contains("video") && !text.contains("audio"), "{text}");
    // The library card's own line (`doc_meta`, after the app's start, which
    // this harness leaves out) names no media either.
    assert!(QUICK_DECK.contains("if d.state == \"held\" { return \"Can't open this file\" }"));
    assert!(!QUICK_DECK.contains("video or audio"));
}

#[test]
fn an_office_document_that_isnt_a_presentation_is_explained() {
    // #507: a .docx or .xlsx answers unsupported:.
    let detail = quickdeck_model("open_detail(\"unsupported\").to_json()");
    let text = detail.as_str().unwrap();
    assert!(text.contains("Word documents") && text.contains("Excel workbooks"), "{text}");
}

/// A deck.slide shape as the script receives it: parsed from JSON, since
/// `loop` (a key the service sends) is a keyword in a script literal.
fn shape(value: serde_json::Value) -> String {
    format!("{}.parse_json()", serde_json::to_string(&value.to_string()).unwrap())
}

#[test]
fn a_clip_is_found_by_its_shape_and_plays_by_its_own_settings() {
    // SERVICE.md "Playback": deck.slide reports a video or audio shape with
    // media: {id, video, autoplay, loop}; deck.media takes the SHAPE's id,
    // never media.id (the clip's media item).
    let clip = |value: serde_json::Value| quickdeck_model(&format!("clip_of({}).to_json()", shape(value)));
    assert_eq!(
        clip(json!({"id": 102, "kind": "video", "box": [200, 120, 560, 315], "media": {"id": 7, "video": true, "autoplay": true, "loop": false}})),
        json!({"id": 102, "kind": "video", "box": [200, 120, 560, 315], "autoplay": true, "looping": false})
    );
    let sound = clip(json!({"id": 202, "kind": "audio", "box": [440, 200, 80, 80], "media": {"id": 8, "video": false, "autoplay": false, "loop": true}}));
    assert_eq!((sound["id"].clone(), sound["kind"].clone(), sound["looping"].clone()), (json!(202), json!("audio"), json!(true)));
    // A service without the media field still names the kind.
    assert_eq!(clip(json!({"id": 5, "kind": "video", "box": [0, 0, 10, 10]}))["autoplay"], json!(false));
    assert_eq!(clip(json!({"id": 6, "kind": "picture", "box": [0, 0, 10, 10]})), json!(null));
    assert_eq!(quickdeck_model("clip_of(nil).to_json()"), json!(null));
    // A slide's clips in order, and the first one set to play by itself.
    let shapes = shape(json!([
        {"id": 1, "kind": "textBox", "box": [0, 0, 1, 1]},
        {"id": 2, "kind": "video", "box": [0, 0, 1, 1], "media": {"id": 1, "video": true, "autoplay": false, "loop": false}},
        {"id": 3, "kind": "audio", "box": [0, 0, 1, 1], "media": {"id": 2, "video": false, "autoplay": true, "loop": false}}
    ]));
    let ids = quickdeck_model(&format!("let out = [] for c in clips_in({shapes}) {{ out.push(c.id) }} out.to_json()"));
    assert_eq!(ids, json!([2, 3]));
    assert_eq!(quickdeck_model(&format!("autoplay_clip(clips_in({shapes})).id.to_json()")), json!(3));
    // A .pptx's clips never play by themselves (deckcraft's reader at
    // d0e57d7e doesn't read PowerPoint's timing): nothing to start.
    let pptx = shape(json!([{"id": 2, "kind": "video", "box": [0, 0, 1, 1], "media": {"id": 1, "video": true, "autoplay": false, "loop": false}}]));
    assert_eq!(quickdeck_model(&format!("autoplay_clip(clips_in({pptx})).to_json()")), json!(null));
}

#[test]
fn a_clip_deck_media_refuses_is_explained() {
    let refused = |error: &str| quickdeck_model(&format!("clip_refused(\"{error}\").to_json()"));
    // The contract's own words for room.
    assert_eq!(
        refused("deck.media: storage_full: There isn't room in Quick Deck's storage to play this clip."),
        json!({"phase": "storage_full", "words": "There isn't room in Quick Deck's storage to play this clip."})
    );
    // unsupported: a linked, missing or empty clip, or another kind.
    let unsupported = refused("unsupported: the clip is linked to a file outside the deck");
    assert_eq!(unsupported["phase"], json!("unsupported"));
    let words = unsupported["words"].as_str().unwrap();
    assert!(words.contains("isn't stored in the deck") && words.contains("MP4, MOV, M4V, MP3, M4A, AAC or WAV"), "{words}");
    assert_eq!(refused("unknown_id: shape 4 on slide 9 is not a video or audio clip")["phase"], json!("failed"));
    assert!(refused("unknown_id: shape 4")["words"].as_str().unwrap().contains("no longer a clip"));
    assert_eq!(refused("busy: try again")["words"], json!("Quick Deck couldn't get the clip ready: try again"));
}

#[test]
fn a_system_without_a_player_says_so_and_a_clip_that_fails_later_is_that_clip() {
    let failure = |error: &str, played: bool| quickdeck_model(&format!("clip_failure(\"{error}\", {played}).to_json()"));
    // Video's error when no player takes the file (the software fallback
    // without a media plugin, software_video.rs).
    assert_eq!(
        failure("playback session unavailable: no media plugin installed", false),
        json!({"phase": "unavailable", "words": "Playback isn't available on this system"})
    );
    assert_eq!(failure("Linux Vulkan video texture import is not implemented", false)["phase"], json!("unavailable"));
    assert_eq!(failure("", false)["words"], json!("Playback isn't available on this system"), "a clip that never got ready");
    // Once a clip has played here, a failure is that clip's.
    assert_eq!(failure("playback session unavailable: no media plugin installed", true),
        json!({"phase": "failed", "words": "This clip couldn't be played."}));
    // Never the system's: a path outside the storage, too many players.
    assert_eq!(failure("Video source is outside app storage", false)["phase"], json!("failed"));
    assert_eq!(failure("Too many open video players", false)["phase"], json!("failed"));
}

/// One look at the widget (`clip_step`): `clip` and `look` are script objects.
fn clip_step(clip: &str, look: &str, now: f64) -> serde_json::Value {
    quickdeck_model(&format!("clip_step({clip}, {look}, {now}).to_json()"))
}

#[test]
fn a_clip_gets_ready_by_a_kick_and_shows_its_picture_once_it_plays() {
    let preparing = "{phase: \"preparing\" kicks: 0 since: 100 pos: 0 dur: 0 resume_at: 0 looping: false}";
    // Made with autoplay off: it says "unprepared" until it is kicked.
    let act = clip_step(preparing, "{state: \"unprepared\" error: \"\" pos: 0 dur: 0}", 100.3);
    assert_eq!((act["phase"].clone(), act["kick"].clone(), act["reveal"].clone()), (json!("preparing"), json!(true), json!(false)));
    let act = clip_step(preparing, "{state: \"preparing\" error: \"\" pos: 0 dur: 0}", 100.6);
    assert_eq!((act["phase"].clone(), act["kick"].clone()), (json!("preparing"), json!(false)), "it is on its way");
    let act = clip_step(preparing, "{state: \"playing\" error: \"\" pos: 166 dur: 5000}", 101.0);
    assert_eq!(act, json!({"phase": "playing", "kick": false, "seek": -1, "reveal": true, "stop": false, "pos": 166, "dur": 5000, "resume_at": 0}));
    // A widget made again (its canvas drawn again) starts where it was.
    let again = "{phase: \"preparing\" kicks: 0 since: 100 pos: 2000 dur: 5000 resume_at: 2000 looping: false}";
    let act = clip_step(again, "{state: \"playing\" error: \"\" pos: 40 dur: 5000}", 101.0);
    assert_eq!((act["seek"].clone(), act["pos"].clone(), act["resume_at"].clone()), (json!(2000), json!(2000), json!(0)));
    // It gives up after three kicks, or 20 seconds.
    let kicked = "{phase: \"preparing\" kicks: 3 since: 100 pos: 0 dur: 0 resume_at: 0 looping: false}";
    let act = clip_step(kicked, "{state: \"unprepared\" error: \"\" pos: 0 dur: 0}", 101.0);
    assert_eq!((act["phase"].clone(), act["stop"].clone()), (json!("failed"), json!(true)));
    let act = clip_step(preparing, "{state: \"preparing\" error: \"\" pos: 0 dur: 0}", 121.0);
    assert_eq!(act["phase"], json!("failed"), "never ready");
    // The widget's error ends it.
    let act = clip_step(preparing, "{state: \"unprepared\" error: \"playback session unavailable: no media plugin installed\" pos: 0 dur: 0}", 100.5);
    assert_eq!((act["phase"].clone(), act["stop"].clone(), act["kick"].clone()), (json!("failed"), json!(true), json!(false)));
}

#[test]
fn a_playing_clip_pauses_ends_and_starts_again_where_it_was() {
    let playing = "{phase: \"playing\" kicks: 0 since: 100 pos: 1500 dur: 5000 resume_at: 0 looping: false}";
    assert_eq!(clip_step(playing, "{state: \"playing\" error: \"\" pos: 1750 dur: 5000}", 102.0)["pos"], json!(1750));
    assert_eq!(clip_step(playing, "{state: \"paused\" error: \"\" pos: 1600 dur: 5000}", 102.0)["phase"], json!("paused"));
    assert_eq!(clip_step(playing, "{state: \"completed\" error: \"\" pos: 5000 dur: 5000}", 102.0)["phase"], json!("ended"));
    // The Mac's player says nothing at the end: a clip that stopped moving at
    // its last frame has ended, unless it loops.
    let at_end = "{phase: \"playing\" kicks: 0 since: 100 pos: 4958 dur: 5000 resume_at: 0 looping: false}";
    let act = clip_step(at_end, "{state: \"playing\" error: \"\" pos: 4958 dur: 5000}", 106.0);
    assert_eq!((act["phase"].clone(), act["stop"].clone()), (json!("ended"), json!(true)));
    let looping = "{phase: \"playing\" kicks: 0 since: 100 pos: 4958 dur: 5000 resume_at: 0 looping: true}";
    assert_eq!(clip_step(looping, "{state: \"playing\" error: \"\" pos: 4958 dur: 5000}", 106.0)["phase"], json!("playing"));
    assert_eq!(clip_step(at_end, "{state: \"playing\" error: \"\" pos: 4990 dur: 5000}", 106.0)["phase"], json!("playing"), "still moving");
    // Its widget made again: kicked, to start again at 1.5 s.
    let act = clip_step(playing, "{state: \"unprepared\" error: \"\" pos: 0 dur: 0}", 102.0);
    assert_eq!((act["phase"].clone(), act["kick"].clone(), act["resume_at"].clone()), (json!("preparing"), json!(true), json!(1500)));
    // A paused clip waits; made again, it keeps its place for the next play.
    let paused = "{phase: \"paused\" kicks: 0 since: 100 pos: 1500 dur: 5000 resume_at: 0 looping: false}";
    assert_eq!(clip_step(paused, "{state: \"paused\" error: \"\" pos: 1500 dur: 5000}", 103.0)["phase"], json!("paused"));
    assert_eq!(clip_step(paused, "{state: \"unprepared\" error: \"\" pos: 0 dur: 0}", 103.0)["resume_at"], json!(1500));
    let act = clip_step(paused, "{state: \"playing\" error: \"\" pos: 1550 dur: 5000}", 103.0);
    assert_eq!((act["phase"].clone(), act["reveal"].clone()), (json!("playing"), json!(true)));
}

#[test]
fn a_clips_control_tells_the_time_and_stays_on_the_slide() {
    assert_eq!(quickdeck_model("clip_time(2400, 5000).to_json()"), json!("0:02 / 0:05"));
    assert_eq!(quickdeck_model("clip_time(65000, 0).to_json()"), json!("1:05"), "before the duration is known");
    assert_eq!(quickdeck_model("clip_time(-5, 3725000).to_json()"), json!("0:00 / 62:05"));
    // Under the clip at 88%: [200, 120, 560, 315] on a 960 x 540 slide.
    let at = |b: &str, bw: f64| quickdeck_model(&format!("clip_bar_at({b}, 0.88, 960, 540, {bw}, 36).to_json()"));
    assert_eq!(at("[200, 120, 560, 315]", 196.0), json!([324, 391]));
    // A clip at the foot of the slide: the control goes over it.
    assert_eq!(at("[200, 380, 560, 150]", 196.0)[1], json!(290));
    // A clip filling the slide: inside its foot.
    assert_eq!(at("[0, 0, 960, 540]", 196.0)[1], json!(431));
    // Never past the slide's sides.
    assert_eq!(at("[0, 100, 40, 40]", 300.0)[0], json!(4));
    assert_eq!(at("[900, 100, 60, 40]", 300.0)[0], json!(541));
}

/// The names the shell finds its own widgets by in the whole window
/// (`self.ui.widget(cx, ids!(bar))`), hosted apps' widgets included.
fn shell_window_lookups() -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    for source in [include_str!("lib.rs"), include_str!("desktop_app.rs"), include_str!("mobile_app.rs")] {
        for (at, _) in source.match_indices("self.ui.") {
            let rest = &source[at..];
            let line = &rest[..rest.find('\n').unwrap_or(rest.len())];
            let Some(open) = line.find("(cx, ids!(") else { continue };
            let name = &line[open + "(cx, ids!(".len()..];
            let name: String = name.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            if !name.is_empty() {
                names.insert(name);
            }
        }
    }
    names
}

/// The names Quick Deck gives its widgets (`name := Widget{...}`).
fn quick_deck_widget_names() -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    for (at, _) in QUICK_DECK.match_indices(" := ") {
        let before = &QUICK_DECK[..at];
        let name: String = before.chars().rev().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect::<Vec<_>>().into_iter().rev().collect();
        if !name.is_empty() {
            names.insert(name);
        }
    }
    names
}

#[test]
fn quick_deck_names_no_widget_the_shell_finds_by_name_in_its_window() {
    // Since #516 put the shell's own bar after the scene, the shell's
    // `self.ui.widget(cx, ids!(bar))` found Quick Deck's toolbar, then named
    // `bar`, first, and the shell panicked applying its chrome to it (#519
    // finds the bar another way). `bar` stays out of Quick Deck's names, and
    // so does every name the shell looks up in its window.
    let mut shell = shell_window_lookups();
    assert!(shell.contains("main_window"), "the lookups were read: {shell:?}");
    shell.insert("bar".to_string());
    let ours = quick_deck_widget_names();
    assert!(ours.contains("qd_toolbar") && ours.contains("qd_clip"), "the names were read: {ours:?}");
    let both: Vec<&String> = shell.intersection(&ours).collect();
    assert!(both.is_empty(), "Quick Deck names widgets the shell looks up in its window: {both:?}");
}
