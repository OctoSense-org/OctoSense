//! PDF Tools' library rules: the pure functions in
//! `apps/pdftools/bundle/main.splash` (everything between the shared
//! interface and `start_timeout(`), run in a script VM. They decide what an
//! imported PDF is called (`files.import` reports the chosen file's name; the
//! file keeps the name the app gave it), what the library index keeps
//! across a restart, and what the Edit panel says an edit would do before
//! Apply (from `pdf.edit_check`).
use makepad_widgets::*;
use serde_json::json;

const PDF_TOOLS: &str = include_str!("../../../apps/pdftools/bundle/main.splash");

/// Evaluate `expression` after PDF Tools' functions; it must end in `.to_json()`.
fn pdftools_model(expression: &str) -> serde_json::Value {
    let source = PDF_TOOLS.split_once("// END shared app interface\n").unwrap().1
        .split_once("\nstart_timeout(").unwrap().0;
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm {
        host: &mut host,
        bx: Box::new(ScriptVmBase::new()),
    };
    vm.bx.captured_errors = Some(Vec::new());
    let result = vm.with_instruction_limit(500_000, |vm| {
        vm.eval(ScriptMod {
            file: "pdftools_model_test.splash".into(),
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

fn import_title(shown: &str, name: &str, taken: &str) -> serde_json::Value {
    pdftools_model(&format!("title_for_import({shown}, \"{name}\", {taken}).to_json()"))
}

#[test]
fn an_imported_pdf_is_titled_with_the_chosen_files_name() {
    assert_eq!(import_title("\"Lease 2026.pdf\"", "Imported PDF.pdf", "[]"), json!("Lease 2026"));
    assert_eq!(import_title("\"Scan 7.PDF\"", "Imported PDF 2.pdf", "[]"), json!("Scan 7"), "any case of the extension");
    assert_eq!(import_title("\"notes\"", "Imported PDF.pdf", "[]"), json!("notes"), "a name without one is kept whole");
    assert_eq!(import_title("\" Plan.pdf \"", "Imported PDF.pdf", "[]"), json!("Plan"));
    assert_eq!(import_title("\"Lease 2026.pdf\"", "Imported PDF.pdf", "[\"Field guide\", \"Lease\"]"), json!("Lease 2026"),
        "other titles leave it alone");
}

#[test]
fn without_a_name_an_imported_pdf_is_titled_with_its_stored_name() {
    // Android, and a name nothing was left of after the host cleaned it.
    assert_eq!(import_title("nil", "Imported PDF 2.pdf", "[]"), json!("Imported PDF 2"));
    assert_eq!(import_title("\"\"", "Imported PDF 3.pdf", "[]"), json!("Imported PDF 3"));
    assert_eq!(import_title("\".pdf\"", "Imported PDF.pdf", "[]"), json!("Imported PDF"), "nothing but the extension");
}

#[test]
fn a_name_another_pdf_shows_gets_the_next_number() {
    assert_eq!(import_title("\"Lease 2026.pdf\"", "Imported PDF 2.pdf", "[\"Lease 2026\"]"), json!("Lease 2026 (2)"));
    assert_eq!(
        import_title("\"Lease 2026.pdf\"", "Imported PDF 3.pdf", "[\"Lease 2026 (2)\", \"Lease 2026\"]"),
        json!("Lease 2026 (3)")
    );
}

#[test]
fn a_part_split_from_a_titled_pdf_is_titled_after_it() {
    let part = |title: &str, name: &str, part: &str| {
        pdftools_model(&format!("title_for_part(\"{title}\", \"{name}\", \"{part}\").to_json()"))
    };
    // The engine names parts after the file; the title follows the PDF's.
    assert_eq!(part("Lease 2026", "Imported PDF.pdf", "Imported PDF-part2.pdf"), json!("Lease 2026-part2"));
    assert_eq!(part("Lease 2026 (2)", "Imported PDF 3.pdf", "Imported PDF 3-part1.pdf"), json!("Lease 2026 (2)-part1"));
    // A PDF that shows its file name gives its parts theirs.
    assert_eq!(part("Field guide", "Field guide.pdf", "Field guide-part2.pdf"), json!(null));
}

#[test]
fn the_index_keeps_a_title_across_a_restart() {
    // What save_index writes and load_index reads back: {version: 1 docs: index}.
    let restart = |entry: &str| {
        pdftools_model(&format!(
            "let kept = {{}}\nkept[\"Imported PDF.pdf\"] = {entry}\n\
             let back = {{version: 1 docs: kept}}.to_json().parse_json()\n\
             doc_from(\"Imported PDF.pdf\", optional(back.docs, \"Imported PDF.pdf\", nil)).to_json()"
        ))
    };
    let doc = restart("index_entry(3, 5053, \"Garden plan 2027\", \"Imported PDF.pdf\")");
    assert_eq!(doc["title"], json!("Garden plan 2027"));
    assert_eq!(doc["pages"], json!(3));
    assert_eq!(doc["bytes"], json!(5053));
    assert_eq!(doc["path"], json!("accounts/device/library/Imported PDF.pdf"), "the file keeps its stored name");
    // Written as the import lands, before the engine has read the file.
    let doc = restart("{title: \"Garden plan 2027\"}");
    assert_eq!(doc["title"], json!("Garden plan 2027"));
    assert_eq!(doc["pages"], json!(null), "the engine reads it on this launch");
}

#[test]
fn the_index_keeps_no_title_for_a_pdf_that_shows_its_name() {
    assert_eq!(
        pdftools_model("index_entry(4, 9551, \"Quarterly report\", \"Quarterly report.pdf\").to_json()"),
        json!({"pages": 4, "bytes": 9551})
    );
    let doc = pdftools_model("doc_from(\"Quarterly report.pdf\", nil).to_json()");
    assert_eq!(doc["title"], json!("Quarterly report"));
    assert_eq!(doc["pages"], json!(null));
}

#[test]
fn a_page_range_names_the_pages_it_takes() {
    let pages = |text: &str, total: u32| pdftools_model(&format!("range_pages(\"{text}\", {total}).to_json()"));
    assert_eq!(pages("", 3), json!([1, 2, 3]), "empty is every page");
    assert_eq!(pages("All", 2), json!([1, 2]));
    assert_eq!(pages("1-4, 9", 9), json!([1, 2, 3, 4, 9]), "Combine's example (06-combine)");
    assert_eq!(pages("1–3", 9), json!([1, 2, 3]), "a typed dash");
    assert_eq!(pages("2,", 9), json!([2]), "an empty part is left out");
    for bad in ["x", "12", "4-2", "0", "1-2-3", "2.5"] {
        assert_eq!(pages(bad, 9), json!(null), "{bad} names no page this PDF has");
    }
}

#[test]
fn a_find_snippet_marks_its_match_in_bold() {
    assert_eq!(
        pdftools_model("snippet_html(\"… strong [[revenue]] <up> & more\").to_json()"),
        json!("… strong <b>revenue</b> &lt;up&gt; &amp; more")
    );
}

#[test]
fn an_avatar_shows_the_first_letters_of_the_first_and_last_names() {
    let initials = |name: &str| pdftools_model(&format!("initials_of(\"{name}\").to_json()"));
    assert_eq!(initials("Jun Park"), json!("JP"));
    assert_eq!(initials("Ana Ruiz"), json!("AR"));
    assert_eq!(initials("Ana María Ruiz"), json!("AR"));
    assert_eq!(initials(" Maya "), json!("M"));
    assert_eq!(initials(""), json!("?"));
}

#[test]
fn a_comment_date_is_read_as_the_service_writes_it() {
    // SERVICE.md: local time without a zone or seconds.
    assert_eq!(
        pdftools_model("comment_time(\"2026-10-10T22:21\").to_json()"),
        json!({"year": 2026, "month": 10, "day": 10, "hour": 22, "minute": 21})
    );
    assert_eq!(pdftools_model("comment_time(\"later\").to_json()"), json!(null));
}

/// The designs are drawn at 1536 x 1024, and the shipped manifest asks the
/// desktop to open PDF Tools' window at that size: App Hub's `window`, which
/// needs `schema_minor` 1. The desk clamps it (`desktop_layout`).
#[cfg(any(feature = "app-hub", native_mobile))]
#[test]
fn the_shipped_manifest_asks_for_the_designs_window_size() {
    let manifest = octosense_app_contract::AppManifest::parse(include_str!("../../../apps/pdftools/bundle/manifest.json")).unwrap();
    assert_eq!(manifest.schema_minor, 1);
    assert_eq!(manifest.window, Some(octosense_app_contract::WindowHint::new(1536, 1024)));
}

#[test]
fn the_storage_line_reads_what_files_status_says() {
    // Design 08's status line, from files.status's used_bytes and quota_bytes.
    assert_eq!(pdftools_model("used_text(15309209, 67108864).to_json()"), json!("14.6 of 64 MB used"));
    assert_eq!(pdftools_model("used_text(92160, 67108864).to_json()"), json!("90 KB of 64 MB used"));
    assert_eq!(pdftools_model("used_text(4002, 8388608).to_json()"), json!("4 KB of 8 MB used"));
    assert_eq!(
        pdftools_model("usage_from({used_bytes: 15309209 quota_bytes: 67108864}).to_json()"),
        json!({"used": 15309209, "quota": 67108864})
    );
    // No storage, or a shell from before files.status said: nothing to read.
    for data in ["{}", "nil", "{used_bytes: 5}", "{used_bytes: 5 quota_bytes: 0}"] {
        assert_eq!(pdftools_model(&format!("usage_from({data}).to_json()")), json!(null), "{data}");
    }
}

#[test]
fn the_edit_panel_reads_a_paragraphs_alignment_from_its_lines() {
    let align = |boxes: &str| pdftools_model(&format!("guess_align({boxes}).to_json()"));
    // Design 08's paragraph: flush left, ragged right.
    assert_eq!(align("[[56, 100, 300, 14], [56, 116, 280, 14], [56, 132, 120, 14]]"), json!("left"));
    assert_eq!(align("[[56, 100, 300, 14], [56, 116, 300, 14], [56, 132, 120, 14]]"), json!("justify"));
    assert_eq!(align("[[56, 100, 300, 14], [56, 116, 300.8, 14]]"), json!("left"), "two lines may just happen to end together");
    assert_eq!(align("[[106, 100, 200, 14], [126, 116, 160, 14], [156, 132, 100, 14]]"), json!("center"));
    assert_eq!(align("[[100, 100, 256, 14], [150, 116, 206, 14]]"), json!("right"));
    assert_eq!(align("[[70, 100, 286, 14], [56, 116, 290, 14], [56, 132, 120, 14]]"), json!("left"), "an indented first line");
    assert_eq!(align("[[56, 100, 300, 14]]"), json!("left"), "one line shows nothing");
    assert_eq!(align("[]"), json!("left"));
}

#[test]
fn an_edit_sends_its_text_and_alignment_and_a_chosen_colour() {
    // SERVICE.md "Edit": the engine sets a paragraph flush left unless told,
    // so the alignment always goes; the colour only once one is chosen.
    let edit = |draft: &str, align: &str, color: &str| {
        format!("{{page: 2 n: 3 text: \"Old\" draft: \"{draft}\" align: \"{align}\" align0: \"left\" color: {color}}}")
    };
    assert_eq!(
        pdftools_model(&format!("edit_args(\"d1\", {}).to_json()", edit("New", "center", "nil"))),
        json!({"doc": "d1", "page": 2, "paragraph": 3, "text": "New", "align": "center"})
    );
    assert_eq!(
        pdftools_model(&format!("edit_args(\"d1\", {}).to_json()", edit("Old", "left", "\"#c01c28\""))),
        json!({"doc": "d1", "page": 2, "paragraph": 3, "text": "Old", "align": "left", "color": "#c01c28"})
    );
    let changes = |e: String| pdftools_model(&format!("edit_changes({e}).to_json()"));
    assert_eq!(changes(edit("Old", "left", "nil")), json!(false), "nothing to apply");
    assert_eq!(changes(edit("New", "left", "nil")), json!(true));
    assert_eq!(changes(edit("Old", "right", "nil")), json!(true), "an alignment alone");
    assert_eq!(changes(edit("Old", "left", "\"#000000\"")), json!(true), "a colour alone");
    assert_eq!(pdftools_model("colour_name(nil).to_json()"), json!("Its own colour"));
    assert_eq!(pdftools_model("colour_name(\"#c01c28\").to_json()"), json!("Red"));
    // What the panel offers is what pdf.edit_text takes.
    assert_eq!(pdftools_model("ALIGNS.to_json()"), json!(["left", "center", "right", "justify"]));
    for c in pdftools_model("TEXT_COLOURS.to_json()").as_array().unwrap() {
        let hex = c["hex"].as_str().unwrap();
        assert!(hex.len() == 7 && hex.starts_with('#') && hex[1..].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{c}");
    }
}

#[test]
fn the_combine_card_scrolls_its_list_only_when_the_window_is_too_short() {
    // Each PDF's row and line (114), Add files (60), the box's border (2);
    // the list of PDFs to add while it is open (10, and 40 a row).
    let list = |items: u32, adding: u32| pdftools_model(&format!("combine_list_h({items}, {adding}).to_json()"));
    assert_eq!(list(2, 0), json!(290));
    assert_eq!(list(1, 5), json!(386));
    let fits = |list_h: u32, height: u32, error: bool| pdftools_model(&format!("combine_fits({list_h}, {height}, {error}).to_json()"));
    // Design 06's size (1536 x 1024): two PDFs and the card's own button fit
    // as designed.
    assert_eq!(fits(290, 1024, false), json!(true));
    // The desktop's window above the dock (1292 x 662) and its default
    // (990 x 603): the list scrolls inside the card, its footer in view.
    assert_eq!(fits(290, 662, false), json!(false));
    assert_eq!(fits(290, 603, false), json!(false));
    // Where it starts to fit: the bars (151), the card's margins (82) and the
    // rest of the card (279), and a refusal's line (30) takes room.
    assert_eq!(fits(290, 802, false), json!(true));
    assert_eq!(fits(290, 801, false), json!(false));
    assert_eq!(fits(290, 802, true), json!(false));
}

#[test]
fn search_is_the_top_bars_and_the_rail_offers_pages_outline_and_comments() {
    // The user: "side search and top search are duplicated". Search in
    // document is the one way to search; the rail has no Search of its own.
    let rail = pdftools_model("RAIL.to_json()");
    let ids: Vec<&str> = rail.as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["pages", "outline", "comments"]);
    let labels: Vec<&str> = rail.as_array().unwrap().iter().map(|r| r["label"].as_str().unwrap()).collect();
    assert_eq!(labels, ["Pages", "Outline", "Comments"]);
    // While the left panel shows search results, no rail item is lit.
    let lit = |id: &str, mode: &str, panel: &str, open: bool| pdftools_model(&format!("rail_lit(\"{id}\", \"{mode}\", \"{panel}\", {open}).to_json()"));
    for id in ids {
        assert_eq!(lit(id, "read", "search", true), json!(false), "{id} while results show");
        assert_eq!(lit(id, "read", id, true), json!(true), "{id} while its own panel shows");
        assert_eq!(lit(id, "read", id, false), json!(false), "{id} while the left side is folded");
    }
    assert_eq!(lit("pages", "pages", "search", false), json!(true), "the Pages mode lights Pages");
    assert_eq!(lit("comments", "pages", "comments", true), json!(false));
}

#[test]
fn a_search_gives_the_left_side_back_when_it_ends() {
    let back = |from: &str| pdftools_model(&format!("panel_after_find({from}).to_json()"));
    // What the left side showed before the results took it.
    assert_eq!(back("\"outline\""), json!("outline"));
    assert_eq!(back("\"comments\""), json!("comments"));
    assert_eq!(back("\"pages\""), json!("pages"));
    assert_eq!(back("\"\""), json!(""), "folded away before, folded away after");
    // Unknown: Pages.
    assert_eq!(back("nil"), json!("pages"));
    assert_eq!(back("\"search\""), json!("pages"));
}

#[test]
fn read_mode_shows_the_pages_alone() {
    // The user: "there is no full screen read mode?". In Read mode only the
    // pages, the floating bar and the corner control that leaves it show.
    assert_eq!(
        pdftools_model("chrome_for(true).to_json()"),
        json!({"tabs": false, "modebar": false, "rail": false, "panels": false, "right": false, "toolrow": false, "status": false, "exit": true})
    );
    assert_eq!(
        pdftools_model("chrome_for(false).to_json()"),
        json!({"tabs": true, "modebar": true, "rail": true, "panels": true, "right": true, "toolrow": true, "status": true, "exit": false})
    );
    // The canvas: the window less the rail (84 and its line), the panels and
    // the bars (tab row 53, mode bar 54, status line 44, two lines); in Read
    // mode, the whole window.
    let canvas = |w: u32, h: u32, reading: bool, left: u32, right: u32| {
        pdftools_model(&format!("canvas_size({w}, {h}, {reading}, {left}, {right}).to_json()"))
    };
    assert_eq!(canvas(1292, 662, false, 205, 0), json!([1002, 509]), "the window's own size, Pages open");
    assert_eq!(canvas(1292, 662, true, 205, 425), json!([1292, 662]), "Read mode: the panels' room is the pages'");
    assert_eq!(canvas(990, 603, false, 205, 321), json!([379, 450]));
    assert_eq!(canvas(300, 200, false, 205, 321), json!([240, 240]), "never smaller than 240");
}

#[test]
fn read_mode_keeps_the_zoom_the_person_chose() {
    // Entering or leaving Read mode: a chosen zoom stays, a fit fits the new
    // canvas, the automatic zoom is worked out again. (The script's numbers
    // are single precision: compared to a ten-thousandth.)
    let kept = |zoom: f64, auto: bool, fit: &str, fit_w: f64, fit_p: f64, start: f64| {
        pdftools_model(&format!("kept_zoom({zoom}, {auto}, \"{fit}\", {fit_w}, {fit_p}, {start}).to_json()")).as_f64().unwrap()
    };
    let near = |got: f64, want: f64, what: &str| assert!((got - want).abs() < 1e-4, "{what}: {got} for {want}");
    near(kept(1.25, false, "", 2.0907, 0.6766, 1.0), 1.25, "the zoom the person chose");
    near(kept(0.8, true, "", 2.0907, 0.6766, 1.0), 1.0, "the automatic zoom, worked out again");
    near(kept(1.6, false, "width", 2.0907, 0.6766, 1.0), 2.09, "fit width, on the new canvas");
    near(kept(1.6, false, "page", 2.0907, 0.6766, 1.0), 0.67, "fit page, on the new canvas");
    near(kept(1.0, false, "width", 5.5, 0.1, 1.0), 4.0, "at most 400%");
    near(kept(1.0, false, "page", 5.5, 0.1, 1.0), 0.25, "at least 25%");
}

#[test]
fn a_fonts_name_reads_as_a_person_would_say_it() {
    let label = |font: &str| pdftools_model(&format!("font_label(\"{font}\").to_json()"));
    // A subset's tag goes, and so do the engine's hyphens; a browser's Type 3
    // fonts have no name at all.
    assert_eq!(label("ABCDEF+Menlo-Regular"), json!("Menlo Regular"));
    assert_eq!(label("Helvetica-Bold"), json!("Helvetica Bold"));
    assert_eq!(label("Times-Roman"), json!("Times Roman"));
    assert_eq!(label("Shippori Mincho Type3"), json!("Shippori Mincho"));
    assert_eq!(label(""), json!("Unnamed font"));
    assert_eq!(label("ABCDE+Odd"), json!("ABCDE+Odd"), "five letters are no subset tag");
    // The Font box: the name and the size (whole sizes here: the script's
    // numbers are single precision).
    let font = |name: &str, size: u32| pdftools_model(&format!("font_text({{font: \"{name}\" size: {size}}}).to_json()"));
    assert_eq!(font("", 28), json!("Unnamed font  ·  28 pt"));
    assert_eq!(font("ABCDEF+FixtureSans-SC", 0), json!("FixtureSans SC"));
}

#[test]
fn the_edit_panel_says_what_an_edit_would_change_before_apply() {
    // What pdf.edit_check answers, as the panel puts it (the service's own
    // tests hold the answers to what the engine then does).
    let risks = |check: &str| pdftools_model(&format!("edit_risks({check}).to_json()"));
    assert_eq!(risks("nil"), json!([]), "before the check answers, or an engine without it");
    assert_eq!(risks("{keeps_font: true font: \"\" box: [44.62, 97.54, 166.8, 27.8] covers: 0 off_page: false}"), json!([]));
    assert_eq!(
        risks("{keeps_font: false font: \"Helvetica-Bold\" covers: 3 off_page: false}"),
        json!([
            "Set in Helvetica Bold: this paragraph's font can't show all of this text.",
            "Longer than its box: it runs into 3 lines of text nearby."
        ])
    );
    assert_eq!(risks("{keeps_font: true font: \"X\" covers: 1 off_page: true}"), json!(["Longer than its box: it runs into 1 line of text nearby.", "Part of it falls off the page."]));
    // Apply says so while there is something to say, but not for a refusal
    // (Apply would only be refused again).
    let label = |check: &str, error: &str| pdftools_model(&format!("apply_label({{check: {check} check_error: \"{error}\"}}).to_json()"));
    assert_eq!(label("nil", ""), json!("Apply"));
    assert_eq!(label("{keeps_font: true covers: 0 off_page: false}", ""), json!("Apply"));
    assert_eq!(label("{keeps_font: false font: \"Helvetica\" covers: 0 off_page: false}", ""), json!("Apply anyway"));
    assert_eq!(label("nil", "PDF Tools can't edit Chinese, Japanese or Korean text yet."), json!("Apply"));
    // A check is for the edit as it stands: its text, alignment and colour.
    let key = |draft: &str, align: &str, color: &str| pdftools_model(&format!("edit_key({{draft: \"{draft}\" align: \"{align}\" color: {color}}}).to_json()"));
    assert_eq!(key("Studio", "left", "nil"), json!("Studio\nleft\n"));
    assert_ne!(key("Studio", "left", "nil"), key("Studio", "center", "nil"));
    assert_ne!(key("Studio", "left", "nil"), key("Studio", "left", "\"#c01c28\""));
}

#[test]
fn a_title_printed_in_pieces_is_counted_as_pieces() {
    // pdf.lines' paragraphs on the page `--edit-sample` writes (a page as a
    // web browser prints it): the title in five pieces, one per font, then a
    // Chinese paragraph and an English one below.
    let page = r#"[
        {n: 1 box: [44.62, 75.77, 219.58, 10.51] size: 9.14},
        {n: 2 box: [44.62, 97.54, 166.8, 27.8] size: 27.8},
        {n: 3 box: [211.42, 97.54, 27.8, 27.8] size: 27.8},
        {n: 4 box: [239.22, 97.54, 27.8, 27.8] size: 27.8},
        {n: 5 box: [267.02, 97.54, 55.6, 27.8] size: 27.8},
        {n: 6 box: [322.62, 97.54, 27.8, 27.8] size: 27.8},
        {n: 7 box: [44.62, 150.87, 177.1, 35.15] size: 12.65},
        {n: 8 box: [44.62, 193.2, 235.55, 30.3] size: 12.0}
    ]"#;
    let pieces = |n: u32| pdftools_model(&format!("line_pieces({page}, {n}).to_json()"));
    for n in 2..=6 {
        assert_eq!(pieces(n), json!(5), "piece {n} of the title");
    }
    for n in [1, 7, 8] {
        assert_eq!(pieces(n), json!(1), "paragraph {n} is whole");
    }
    assert_eq!(pieces(99), json!(1), "a paragraph that isn't there");
    // Two columns side by side are not one line in pieces.
    let columns = "[{n: 1 box: [50, 100, 200, 12] size: 10}, {n: 2 box: [300, 100, 200, 12] size: 10}]";
    assert_eq!(pdftools_model(&format!("line_pieces({columns}, 1).to_json()")), json!(1));
    assert_eq!(pdftools_model("pieces_note(5).to_json()"), json!("The PDF stores this line in 5 pieces: only this one changes."));
    assert_eq!(pdftools_model("pieces_note(1).to_json()"), json!(""));
}

/// The errors the script's parser and tokenizer report for `code`, run in a
/// bare VM: what runs after them fails there (no widgets, no host), and is
/// not counted.
fn parse_errors(code: &str) -> Vec<String> {
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm {
        host: &mut host,
        bx: Box::new(ScriptVmBase::new()),
    };
    vm.bx.captured_errors = Some(Vec::new());
    vm.with_instruction_limit(200_000, |vm| {
        vm.eval(ScriptMod {
            file: "pdftools_parse_test.splash".into(),
            code: format!("use mod.math.*\nuse mod.std.regex\n{code}\n;"),
            ..Default::default()
        })
    });
    vm.take_errors().into_iter().filter(|e| e.contains("parser.rs") || e.contains("tokenizer.rs")).collect()
}

#[test]
fn the_whole_bundle_parses() {
    // The model tests above run only the functions before `start_timeout(`;
    // the views and handlers after it are parsed when the app loads, and one
    // parse error there fails the whole app. The parser reads all of it.
    assert_eq!(parse_errors(PDF_TOOLS), Vec::<String>::new());
    // The check itself finds a parse error (a reserved name, as `me` is).
    assert!(!parse_errors(&format!("{PDF_TOOLS}\nfn broken(){{ let me = 1 }}")).is_empty());
}
