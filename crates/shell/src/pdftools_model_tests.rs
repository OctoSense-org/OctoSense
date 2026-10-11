//! PDF Tools' library rules: the pure functions in
//! `apps/pdftools/bundle/main.splash` (everything between the shared
//! interface and `start_timeout(`), run in a script VM. They decide what an
//! imported PDF is called (`files.import` reports the chosen file's name; the
//! file keeps the name the app gave it) and what the library index keeps
//! across a restart.
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
