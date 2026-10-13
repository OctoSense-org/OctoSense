//! Sample PDFs for testing PDF Tools (`apps/pdftools`).
//!
//! PDF Tools reads the PDFs in its own storage, and the shell's `pdf` engine
//! works on them there (ADR 0013). This example writes four sample PDFs and
//! a damaged file into the app's storage under an apps root (an OctoSense
//! home's `apps/`, or a `card-host --app-data` folder), before the app
//! starts:
//!
//! ```text
//! <apps root>/os.pdftools/accounts/device/library/*.pdf
//! ```
//!
//! With `--import-sample <file>` it writes one more PDF, a three-page garden
//! plan, to `<file>`: what a test places in the library as `Imported PDF.pdf`,
//! the name PDF Tools gives the first PDF it opens from the device, to drive
//! the path an imported PDF takes without the host's file dialog. With
//! `--edit-sample <file>` it writes a page as a web browser prints it, whose
//! fonts break text editing the way real-world PDFs do (`printed_flyer`
//! below), for PDF Tools' Edit. The PDFs are written by hand below, so they
//! are the same on every run and need no files of their own. Nothing here
//! calls the engine.
//!
//! ```sh
//! cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- <apps root>
//! cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- --import-sample <file>
//! cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- --edit-sample <file>
//! ```

use std::path::{Path, PathBuf};

/// The app the samples are for.
const APP: &str = "os.pdftools";
/// Where PDF Tools keeps its PDFs, relative to its storage (ADR 0004 §11).
const LIBRARY: &str = "accounts/device/library";
/// A file in the library that is not a PDF (a failed download).
const DAMAGED: &str = "Damaged scan.pdf";
const DAMAGED_BYTES: &[u8] = b"%PDF-1.7\n% this download stopped after a few bytes\n";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let written = match args.as_slice() {
        [flag, file] if flag == "--import-sample" => write(Path::new(file), &garden_plan()).map(|_| PathBuf::from(file)),
        [flag, file] if flag == "--edit-sample" => write(Path::new(file), &printed_flyer()).map(|_| PathBuf::from(file)),
        [root] if !root.starts_with('-') => samples(Path::new(root)),
        _ => Err("usage: pdftools_fixture <apps root> | --import-sample <file> | --edit-sample <file>".into()),
    };
    match written {
        Ok(path) => println!("Wrote {}", path.display()),
        Err(e) => {
            eprintln!("pdftools_fixture: {e}");
            std::process::exit(2);
        }
    }
}

/// The samples: file name and bytes. (The service's own tests include
/// this file as a module and use them too.)
pub(crate) fn documents() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("Quarterly report.pdf", quarterly_report()),
        ("Board minutes.pdf", board_minutes()),
        ("Field guide.pdf", field_guide()),
        ("Apartment lease.pdf", apartment_lease()),
        // Not a PDF at all: the app's "Can't open this file" state, with
        // the engine's own answer.
        (DAMAGED, DAMAGED_BYTES.to_vec()),
    ]
}

/// The samples in PDF Tools' library under `root`, into a new or empty
/// storage: nothing of a person's is ever overwritten.
fn samples(root: &Path) -> Result<PathBuf, String> {
    let jail = root.join(APP);
    if jail.exists() && std::fs::read_dir(&jail).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(format!("{} already holds data; give an apps root without PDF Tools' storage", jail.display()));
    }
    let library = jail.join(LIBRARY);
    for (name, bytes) in documents() {
        write(&library.join(name), &bytes)?;
    }
    Ok(library)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

// ---------------------------------------------------------------- the PDFs
//
// Small hand-written PDF 1.4 files: the standard Type 1 fonts (no embedded
// font), filled shapes and plain text, uncompressed, so the samples are
// deterministic and need nothing outside this file.

const W: f64 = 595.0;
const H: f64 = 842.0;
const M: f64 = 56.0;
const NAVY: u32 = 0x1f3a5f;
const INK: u32 = 0x1f2328;
const GREY: u32 = 0x667085;
const RULE: u32 = 0xd0d5dd;
const SOFT: u32 = 0xf2f4f7;

/// Helvetica, Helvetica-Bold, Times-Roman, Times-Italic.
#[derive(Clone, Copy)]
enum Font {
    Sans,
    Bold,
    Serif,
    Italic,
}

impl Font {
    fn tag(self) -> &'static str {
        match self {
            Font::Sans => "F1",
            Font::Bold => "F2",
            Font::Serif => "F3",
            Font::Italic => "F4",
        }
    }
    /// A rough average glyph width, in ems, for line breaking.
    fn em(self) -> f64 {
        match self {
            Font::Sans => 0.52,
            Font::Bold => 0.56,
            Font::Serif | Font::Italic => 0.46,
        }
    }
}

#[derive(Default)]
struct Page {
    ops: String,
}

fn rgb(c: u32) -> String {
    let part = |shift: u32| f64::from((c >> shift) & 0xff) / 255.0;
    format!("{:.3} {:.3} {:.3}", part(16), part(8), part(0))
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
}

impl Page {
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, c: u32) {
        self.ops.push_str(&format!("{} rg {x:.1} {y:.1} {w:.1} {h:.1} re f\n", rgb(c)));
    }
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, width: f64, c: u32) {
        self.ops.push_str(&format!("{} RG {width:.1} w {x1:.1} {y1:.1} m {x2:.1} {y2:.1} l S\n", rgb(c)));
    }
    fn circle(&mut self, cx: f64, cy: f64, r: f64, c: u32) {
        let k = 0.5523 * r;
        self.ops.push_str(&format!(
            "{} rg {:.1} {cy:.1} m {:.1} {:.1} {:.1} {:.1} {cx:.1} {:.1} c {:.1} {:.1} {:.1} {:.1} {:.1} {cy:.1} c \
             {:.1} {:.1} {:.1} {:.1} {cx:.1} {:.1} c {:.1} {:.1} {:.1} {:.1} {:.1} {cy:.1} c f\n",
            rgb(c),
            cx + r,
            cx + r, cy + k, cx + k, cy + r, cy + r,
            cx - k, cy + r, cx - r, cy + k, cx - r,
            cx - r, cy - k, cx - k, cy - r, cy - r,
            cx + k, cy - r, cx + r, cy - k, cx + r,
        ));
    }
    fn triangle(&mut self, a: (f64, f64), b: (f64, f64), c: (f64, f64), color: u32) {
        self.ops.push_str(&format!(
            "{} rg {:.1} {:.1} m {:.1} {:.1} l {:.1} {:.1} l h f\n",
            rgb(color), a.0, a.1, b.0, b.1, c.0, c.1
        ));
    }
    fn text(&mut self, font: Font, size: f64, x: f64, y: f64, c: u32, s: &str) {
        self.ops.push_str(&format!("BT {} rg /{} {size:.1} Tf {x:.1} {y:.1} Td ({}) Tj ET\n", rgb(c), font.tag(), escape(s)));
    }
    /// Text broken into lines that fit `width`; returns the baseline after
    /// the last line.
    fn para(&mut self, font: Font, size: f64, (x, y): (f64, f64), width: f64, c: u32, s: &str) -> f64 {
        let per_line = (width / (size * font.em())).floor().max(8.0) as usize;
        let mut y = y;
        let mut line = String::new();
        for word in s.split_whitespace() {
            if !line.is_empty() && line.len() + 1 + word.len() > per_line {
                self.text(font, size, x, y, c, &line);
                y -= size * 1.38;
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            self.text(font, size, x, y, c, &line);
            y -= size * 1.38;
        }
        y
    }
    fn running(&mut self, title: &str, number: usize) {
        self.text(Font::Sans, 9.0, M, H - 36.0, GREY, title);
        self.line(M, H - 44.0, W - M, H - 44.0, 0.6, RULE);
        self.text(Font::Sans, 9.0, W - M - 10.0, 34.0, GREY, &number.to_string());
    }
}

/// A PDF of `pages` with the document information `info`.
fn pdf(info: &[(&str, &str)], pages: Vec<Page>) -> Vec<u8> {
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        String::new(), // the page tree, once the page objects are numbered
        format!(
            "<< {} /CreationDate (D:20261009090000Z) >>",
            info.iter().map(|(k, v)| format!("/{k} ({})", escape(v))).collect::<Vec<_>>().join(" ")
        ),
    ];
    for base in ["Helvetica", "Helvetica-Bold", "Times-Roman", "Times-Italic"] {
        objects.push(format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base} /Encoding /WinAnsiEncoding >>"));
    }
    let mut kids = Vec::new();
    for page in &pages {
        let page_id = objects.len() + 1;
        kids.push(format!("{page_id} 0 R"));
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {W} {H}] \
             /Resources << /Font << /F1 4 0 R /F2 5 0 R /F3 6 0 R /F4 7 0 R >> >> /Contents {} 0 R >>",
            page_id + 1
        ));
        objects.push(format!("<< /Length {} >>\nstream\n{}endstream", page.ops.len(), page.ops));
    }
    objects[1] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len());
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{body}\nendobj\n", i + 1).bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes());
    out
}

fn info<'a>(title: &'a str, author: &'a str, subject: &'a str, keywords: &'a str) -> [(&'a str, &'a str); 6] {
    [
        ("Title", title),
        ("Author", author),
        ("Subject", subject),
        ("Keywords", keywords),
        ("Creator", "PDF Tools sample generator"),
        ("Producer", "octosense-pdf-service pdftools_fixture"),
    ]
}

fn quarterly_report() -> Vec<u8> {
    let title = "Quarterly report - third quarter 2026";
    let mut cover = Page::default();
    cover.rect(0.0, 500.0, W, H - 500.0, NAVY);
    cover.text(Font::Bold, 11.0, M, 770.0, 0xf2a541, "QUARTERLY REPORT");
    cover.text(Font::Bold, 38.0, M, 690.0, 0xffffff, "Third quarter 2026");
    cover.para(Font::Sans, 15.0, (M, 650.0), 420.0, 0xd9e2ec, "Revenue, costs and the outlook for July to September.");
    cover.rect(M, 560.0, 72.0, 6.0, 0xf2a541);
    let cards = [("Revenue", "$4.2M", "+12% on Q2"), ("Gross margin", "61%", "+3 points"), ("Customers", "1,840", "+215 new")];
    for (i, (label, value, note)) in cards.iter().enumerate() {
        let x = M + i as f64 * 166.0;
        cover.rect(x, 330.0, 150.0, 120.0, SOFT);
        cover.text(Font::Sans, 10.0, x + 14.0, 422.0, GREY, label);
        cover.text(Font::Bold, 26.0, x + 14.0, 380.0, NAVY, value);
        cover.text(Font::Sans, 10.0, x + 14.0, 350.0, 0x2e8540, note);
    }
    cover.para(Font::Serif, 12.0, (M, 270.0), W - 2.0 * M, INK,
        "The quarter closed ahead of plan. Subscription revenue grew in every region, the new support tier \
         paid for itself in eight weeks, and costs stayed flat while the team grew by four people.");
    cover.line(M, 80.0, W - M, 80.0, 0.6, RULE);
    cover.text(Font::Sans, 9.0, M, 62.0, GREY, "Prepared by the finance team, 9 October 2026");

    let mut summary = Page::default();
    summary.running(title, 2);
    summary.text(Font::Bold, 24.0, M, 740.0, INK, "Summary");
    let mut y = summary.para(Font::Serif, 12.0, (M, 710.0), W - 2.0 * M, INK,
        "Revenue reached 4.2 million dollars, twelve per cent more than in the second quarter and thirty-five \
         per cent more than a year ago. Most of the growth came from existing customers moving to annual plans.");
    y = summary.para(Font::Serif, 12.0, (M, y - 8.0), W - 2.0 * M, INK,
        "Operating costs were 2.6 million dollars, almost unchanged. Hosting costs fell after the move to the new \
         region, which offset the cost of four new hires in support and engineering.");
    summary.para(Font::Serif, 12.0, (M, y - 8.0), W - 2.0 * M, INK,
        "Cash at the end of the quarter was 9.8 million dollars. No new debt was taken on.");
    let base = 250.0;
    summary.line(M + 20.0, base, W - M, base, 0.8, GREY);
    let bars = [("Q4 2025", 3.1), ("Q1 2026", 3.4), ("Q2 2026", 3.75), ("Q3 2026", 4.2)];
    for (i, (label, value)) in bars.iter().enumerate() {
        let x = M + 50.0 + i as f64 * 110.0;
        let h = value * 52.0;
        summary.rect(x, base, 60.0, h, if i == 3 { 0xf2a541 } else { NAVY });
        summary.text(Font::Bold, 11.0, x + 14.0, base + h + 8.0, INK, &format!("{value:.2}"));
        summary.text(Font::Sans, 10.0, x + 6.0, base - 18.0, GREY, label);
    }
    summary.text(Font::Italic, 11.0, M, 190.0, GREY, "Revenue by quarter, in millions of dollars.");

    let mut regions = Page::default();
    regions.running(title, 3);
    regions.text(Font::Bold, 24.0, M, 740.0, INK, "Regional results");
    regions.para(Font::Serif, 12.0, (M, 710.0), W - 2.0 * M, INK,
        "Every region grew. The South grew fastest after the partnership with two local resellers; the North \
         remains the largest region by revenue.");
    let rows = [
        ["Region", "Revenue", "Change", "Customers"],
        ["North", "$1.32M", "+9%", "520"],
        ["South", "$0.98M", "+15%", "410"],
        ["East", "$1.14M", "+11%", "470"],
        ["West", "$0.76M", "+14%", "440"],
        ["Total", "$4.20M", "+12%", "1,840"],
    ];
    let mut y = 620.0;
    for (r, row) in rows.iter().enumerate() {
        let fill = if r == 0 { NAVY } else if r % 2 == 0 { SOFT } else { 0xffffff };
        regions.rect(M, y - 10.0, W - 2.0 * M, 30.0, fill);
        for (c, cell) in row.iter().enumerate() {
            let font = if r == 0 || r == rows.len() - 1 { Font::Bold } else { Font::Sans };
            let color = if r == 0 { 0xffffff } else { INK };
            regions.text(font, 11.0, M + 12.0 + c as f64 * 120.0, y, color, cell);
        }
        y -= 30.0;
    }
    regions.line(M, y + 20.0, W - M, y + 20.0, 0.8, GREY);
    regions.para(Font::Serif, 12.0, (M, y - 20.0), W - 2.0 * M, INK,
        "Customer numbers count paying organisations at the end of the quarter. Revenue is recognised monthly.");

    let mut outlook = Page::default();
    outlook.running(title, 4);
    outlook.text(Font::Bold, 24.0, M, 740.0, INK, "Outlook");
    let y = outlook.para(Font::Serif, 12.0, (M, 710.0), W - 2.0 * M, INK,
        "We expect fourth-quarter revenue between 4.4 and 4.6 million dollars. Annual plans renew mostly in \
         January, so the first quarter of 2027 should start strongly. Hiring continues at the current pace.");
    outlook.rect(M, y - 150.0, W - 2.0 * M, 120.0, 0xfff4d6);
    outlook.rect(M, y - 150.0, 5.0, 120.0, 0xf2a541);
    outlook.text(Font::Bold, 13.0, M + 20.0, y - 56.0, INK, "Next steps");
    for (i, step) in ["Open the second support office in November.", "Finish the move of the remaining services to the new region.", "Review prices for the small-team plan before January."].iter().enumerate() {
        outlook.text(Font::Sans, 11.0, M + 20.0, y - 80.0 - i as f64 * 18.0, INK, &format!("-  {step}"));
    }
    pdf(&info(title, "Finance team", "Revenue, costs and outlook for July to September 2026", "quarterly, finance, report"),
        vec![cover, summary, regions, outlook])
}

fn board_minutes() -> Vec<u8> {
    let title = "Board meeting minutes - 2 October 2026";
    let mut first = Page::default();
    first.text(Font::Bold, 26.0, M, 760.0, INK, "Board meeting minutes");
    first.text(Font::Sans, 12.0, M, 736.0, GREY, "Riverside Community Garden  -  Thursday 2 October 2026, 7 pm");
    first.line(M, 722.0, W - M, 722.0, 1.0, 0x3a7d44);
    first.text(Font::Bold, 12.0, M, 696.0, INK, "Present");
    first.para(Font::Serif, 12.0, (M + 70.0, 696.0), W - 2.0 * M - 70.0, INK,
        "Ana Lima (chair), Tom Becker (treasurer), Priya Nair (secretary), Joe Okafor, Mei Chen.");
    let items = [
        ("1. Minutes of the last meeting", "The minutes of 4 September were read and approved without changes."),
        ("2. Treasurer's report", "The garden holds 3,420 dollars. Plot fees for the new season are due by 1 November. \
          The water bill was higher than usual because of the dry August; a rain tank would pay for itself in two seasons."),
        ("3. Autumn work day", "The work day is set for Saturday 18 October. Volunteers will mulch the paths, \
          repair the north fence and plant the bulb bed by the entrance. Tools and lunch are provided."),
        ("4. New plot holders", "Six families are on the waiting list. Two plots will be free after the season; \
          they go to the first two families on the list."),
    ];
    let mut y = 650.0;
    for (heading, body) in items {
        first.text(Font::Bold, 13.0, M, y, INK, heading);
        y = first.para(Font::Serif, 12.0, (M, y - 20.0), W - 2.0 * M, INK, body) - 14.0;
    }
    first.text(Font::Sans, 9.0, W - M - 10.0, 34.0, GREY, "1");

    let mut second = Page::default();
    second.running(title, 2);
    second.text(Font::Bold, 13.0, M, 740.0, INK, "5. Rain tank");
    let y = second.para(Font::Serif, 12.0, (M, 720.0), W - 2.0 * M, INK,
        "Tom presented two quotes for a 2,000 litre rain tank. The board agreed the cheaper quote, \
         including installation, for 640 dollars.");
    second.rect(M, y - 110.0, W - 2.0 * M, 90.0, 0xeaf4ec);
    second.text(Font::Bold, 12.0, M + 16.0, y - 44.0, 0x2c5f34, "Decisions");
    second.text(Font::Sans, 11.0, M + 16.0, y - 64.0, INK, "-  Buy and install the rain tank (640 dollars).");
    second.text(Font::Sans, 11.0, M + 16.0, y - 82.0, INK, "-  Hold the autumn work day on 18 October.");
    let mut row = y - 150.0;
    second.text(Font::Bold, 12.0, M, row, INK, "Actions");
    for (who, what, when) in [("Tom", "Order the rain tank", "10 Oct"), ("Priya", "Email the work-day plan", "6 Oct"),
                              ("Mei", "Contact the waiting list", "15 Oct"), ("Joe", "Borrow a trailer for mulch", "17 Oct")] {
        row -= 24.0;
        second.line(M, row - 8.0, W - M, row - 8.0, 0.5, RULE);
        second.text(Font::Bold, 11.0, M, row, INK, who);
        second.text(Font::Sans, 11.0, M + 80.0, row, INK, what);
        second.text(Font::Sans, 11.0, W - M - 60.0, row, GREY, when);
    }
    second.para(Font::Serif, 12.0, (M, row - 50.0), W - 2.0 * M, INK, "The meeting closed at 8.40 pm. Next meeting: 6 November.");
    second.line(M, 140.0, M + 200.0, 140.0, 0.8, INK);
    second.text(Font::Sans, 10.0, M, 124.0, GREY, "Approved by the chair");
    pdf(&info(title, "Priya Nair", "Minutes of the October board meeting", "minutes, board, garden"), vec![first, second])
}

fn field_guide() -> Vec<u8> {
    let title = "A field guide to garden birds";
    let mut cover = Page::default();
    cover.rect(0.0, 0.0, W, H, 0xf4efe1);
    cover.rect(0.0, 430.0, W, H - 430.0, 0x2f5d3a);
    cover.circle(440.0, 690.0, 70.0, 0xf4c95d);
    cover.text(Font::Bold, 12.0, M, 780.0, 0xcfe3c9, "A POCKET GUIDE");
    cover.text(Font::Bold, 34.0, M, 600.0, 0xffffff, "A field guide to");
    cover.text(Font::Bold, 34.0, M, 556.0, 0xffffff, "garden birds");
    cover.para(Font::Sans, 14.0, (M, 510.0), 360.0, 0xe4efe0, "Six common visitors, how to recognise them and where to look.");
    for (i, c) in [0xd9572b, 0x2f6db5, 0xe8b730, 0x2b2b2b, 0x8a5a3c, 0x4a3f6b].iter().enumerate() {
        cover.circle(M + 40.0 + i as f64 * 82.0, 300.0, 30.0, *c);
    }
    cover.text(Font::Italic, 12.0, M, 220.0, 0x5b5b4c, "Robin, blue tit, goldfinch, blackbird, wren and starling.");
    cover.text(Font::Sans, 9.0, M, 60.0, 0x5b5b4c, "Greenway Nature Club");

    let birds = [
        ("Robin", 0xd9572bu32, "14 cm", "Lawns, hedges, the edge of the vegetable bed",
         "A round bird with an orange-red face and breast and a brown back. Robins defend a territory all year and \
          sing in winter too. They follow gardeners to pick up worms from freshly turned soil.",
         "Both males and females sing; the winter song is thinner and sadder than the spring song."),
        ("Blue tit", 0x2f6db5, "12 cm", "Feeders, trees, nest boxes",
         "Small and busy, with a blue cap, a white face and a yellow belly. Blue tits hang upside down to reach \
          insects at the tips of branches and come to seed feeders in groups.",
         "A pair can raise ten or more chicks, and the parents bring caterpillars hundreds of times a day."),
        ("Goldfinch", 0xe8b730, "12 cm", "Thistles, teasels, nyjer feeders",
         "A red face, black and white head and a broad gold bar on each wing. Goldfinches travel in chattering \
          flocks and pick seeds from seed heads with their fine beaks.",
         "Leave some seed heads standing over winter and goldfinches will visit until spring."),
        ("Blackbird", 0x2b2b2b, "25 cm", "Lawns, shrubs, under hedges",
         "Males are black with a yellow beak and eye ring; females are dark brown. Blackbirds toss leaves aside \
          looking for worms and eat fallen fruit in autumn.",
         "The rich, fluting song is often heard at dusk from a rooftop or a high branch."),
        ("Wren", 0x8a5a3c, "10 cm", "Low cover, log piles, ivy",
         "A tiny brown bird with a short tail often held upright. Wrens creep through low cover like mice, \
          but their song is astonishingly loud for their size.",
         "In cold weather many wrens may roost together in one nest box to keep warm."),
        ("Starling", 0x4a3f6b, "21 cm", "Lawns, rooftops, aerials",
         "From a distance starlings look black; close up they shine purple and green, spotted with white in winter. \
          They walk rather than hop and probe lawns for grubs.",
         "Starlings are fine mimics and copy other birds, phones and car alarms."),
    ];
    let mut pages = vec![cover];
    for (i, (name, color, size, where_, about, fact)) in birds.iter().enumerate() {
        let mut page = Page::default();
        page.running(title, i + 2);
        page.text(Font::Bold, 30.0, M, 740.0, *color, name);
        page.text(Font::Sans, 11.0, M, 714.0, GREY, &format!("Length {size}   -   Where to look: {where_}"));
        page.rect(M, 440.0, W - 2.0 * M, 240.0, 0xf4efe1);
        page.circle(300.0, 540.0, 70.0, *color);
        page.circle(372.0, 600.0, 38.0, *color);
        page.triangle((404.0, 608.0), (440.0, 598.0), (404.0, 590.0), 0xe0a030);
        page.circle(382.0, 612.0, 6.0, 0xffffff);
        page.circle(383.0, 612.0, 3.0, 0x111111);
        page.triangle((236.0, 520.0), (170.0, 470.0), (250.0, 500.0), *color);
        let y = page.para(Font::Serif, 13.0, (M, 400.0), W - 2.0 * M, INK, about);
        page.rect(M, y - 90.0, W - 2.0 * M, 70.0, 0xeaf4ec);
        page.text(Font::Bold, 11.0, M + 14.0, y - 42.0, 0x2c5f34, "Did you know?");
        page.para(Font::Sans, 11.0, (M + 14.0, y - 60.0), W - 2.0 * M - 28.0, INK, fact);
        pages.push(page);
    }
    pdf(&info(title, "Greenway Nature Club", "Six common garden birds and how to recognise them", "birds, nature, guide"), pages)
}

fn apartment_lease() -> Vec<u8> {
    let title = "Residential lease agreement";
    let clauses = [
        ("1. Parties", "This agreement is made between Harbour Lane Homes (the landlord) and Sam Rivera (the tenant) for \
          the apartment at 14 Harbour Lane, flat 3B (the premises)."),
        ("2. Term", "The lease begins on 1 November 2026 and ends on 31 October 2027. It continues month to month after that \
          unless either party gives written notice at least thirty days before the end of the term."),
        ("3. Rent", "The rent is 1,450 dollars a month, due on the first day of each month by bank transfer. Rent paid more \
          than five days late carries a fee of 40 dollars."),
        ("4. Deposit", "The tenant pays a deposit of 1,450 dollars before moving in. The landlord returns it within \
          twenty-one days after the tenant moves out, less the cost of any damage beyond normal wear."),
        ("5. Utilities", "The tenant pays for electricity, internet and gas. The landlord pays for water, rubbish \
          collection and the upkeep of shared areas."),
        ("6. Use of the premises", "The premises are a private home for the tenant and the people named in this agreement. \
          No business that brings customers to the building may be run from the premises."),
        ("7. Repairs", "The tenant reports repairs promptly in writing. The landlord makes urgent repairs within \
          forty-eight hours and other repairs within fourteen days."),
        ("8. Access", "The landlord may enter the premises for inspections or repairs with at least twenty-four hours' notice, \
          or at any time in an emergency."),
        ("9. Pets", "One cat or one small dog is allowed with the landlord's written consent. The tenant is responsible \
          for any damage the animal causes."),
        ("10. Alterations", "The tenant may hang pictures and shelves. Painting, new locks or other changes need the \
          landlord's written consent."),
        ("11. Ending the lease early", "The tenant may end the lease early with sixty days' written notice and a fee equal to \
          one month's rent."),
        ("12. Whole agreement", "This document is the whole agreement between the parties. Changes are valid only in writing \
          and signed by both parties."),
    ];
    let mut pages = Vec::new();
    let mut page = Page::default();
    page.text(Font::Bold, 22.0, M, 760.0, INK, title);
    page.text(Font::Sans, 11.0, M, 738.0, GREY, "14 Harbour Lane, flat 3B  -  1 November 2026 to 31 October 2027");
    page.line(M, 724.0, W - M, 724.0, 1.0, INK);
    let mut y = 696.0;
    for (heading, body) in clauses {
        if y < 150.0 {
            pages.push(std::mem::take(&mut page));
            page.running(title, pages.len() + 1);
            y = 740.0;
        }
        page.text(Font::Bold, 12.0, M, y, INK, heading);
        y = page.para(Font::Serif, 11.0, (M, y - 18.0), W - 2.0 * M, INK, body) - 12.0;
    }
    if y < 260.0 {
        pages.push(std::mem::take(&mut page));
        page.running(title, pages.len() + 1);
        y = 740.0;
    }
    page.text(Font::Bold, 12.0, M, y - 20.0, INK, "Signatures");
    for (i, who) in ["Landlord: Harbour Lane Homes", "Tenant: Sam Rivera"].iter().enumerate() {
        let x = M + i as f64 * 250.0;
        page.line(x, y - 100.0, x + 210.0, y - 100.0, 0.8, INK);
        page.text(Font::Sans, 10.0, x, y - 116.0, GREY, who);
        page.text(Font::Sans, 10.0, x, y - 132.0, GREY, "Date:");
    }
    pages.push(page);
    pdf(&info(title, "Harbour Lane Homes", "Lease for 14 Harbour Lane, flat 3B", "lease, rental, agreement"), pages)
}

pub(crate) fn garden_plan() -> Vec<u8> {
    let title = "Garden plan - spring 2027";
    let beds = [
        ("Bed 1: salad", 0x6a994e, "Lettuce, rocket and radishes, sown every three weeks from March for a steady supply."),
        ("Bed 2: beans", 0xa7c957, "Runner beans on a frame along the north side, with French beans below them."),
        ("Bed 3: roots", 0xbc6c25, "Carrots and beetroot in rows a hand's width apart, thinned in May."),
        ("Bed 4: herbs", 0x386641, "Parsley, chives, thyme and a pot of mint sunk in the soil to keep it in place."),
    ];
    let mut pages = Vec::new();
    let mut cover = Page::default();
    cover.rect(0.0, 0.0, W, H, 0xf1f5e9);
    cover.text(Font::Bold, 11.0, M, 770.0, 0x386641, "ALLOTMENT 14");
    cover.text(Font::Bold, 34.0, M, 700.0, INK, "Garden plan");
    cover.text(Font::Sans, 16.0, M, 670.0, GREY, "Spring 2027: four beds and a calendar");
    for (i, (_, color, _)) in beds.iter().enumerate() {
        let x = M + (i % 2) as f64 * 245.0;
        let y = 380.0 - (i / 2) as f64 * 200.0;
        cover.rect(x, y, 228.0, 180.0, *color);
        cover.text(Font::Bold, 14.0, x + 14.0, y + 154.0, 0xffffff, &format!("Bed {}", i + 1));
    }
    pages.push(cover);
    let mut detail = Page::default();
    detail.running(title, 2);
    detail.text(Font::Bold, 20.0, M, 750.0, INK, "The beds");
    let mut y = 715.0;
    for (name, color, text) in beds {
        detail.rect(M, y - 4.0, 12.0, 12.0, color);
        detail.text(Font::Bold, 13.0, M + 22.0, y, INK, name);
        y = detail.para(Font::Serif, 12.0, (M + 22.0, y - 20.0), W - 2.0 * M - 22.0, INK, text) - 18.0;
    }
    pages.push(detail);
    let mut calendar = Page::default();
    calendar.running(title, 3);
    calendar.text(Font::Bold, 20.0, M, 750.0, INK, "Calendar");
    let months = [("March", "Sow salad under fleece; dig compost into beds 2 and 3."), ("April", "Sow carrots and beetroot; plant out herbs."), ("May", "Thin the roots; put up the bean frame and sow beans."), ("June", "Water in the evening; pick salad and the first beans.")];
    let mut y = 712.0;
    for (month, task) in months {
        calendar.text(Font::Bold, 13.0, M, y, 0x386641, month);
        y = calendar.para(Font::Sans, 12.0, (M + 90.0, y), W - 2.0 * M - 90.0, INK, task) - 14.0;
        calendar.line(M, y + 6.0, W - M, y + 6.0, 0.5, RULE);
    }
    pages.push(calendar);
    pdf(&info(title, "Allotment 14", "What grows where, and when to sow it", "garden, plan, allotment"), pages)
}

// ---------------------------------------------------- a page printed from the web
//
// Real-world PDFs break text editing in ways the samples above never do (a
// page a web browser printed, 11 Oct 2026). This sample has all three, in
// fonts made here, so the service's tests and PDF Tools' Edit can be held to
// honest answers:
//
// - A designed title in pieces. "Studio2026" is in an unnamed Type 3 font,
//   then 春, 季, 发布 and 会 alternate between two more, each piece in its own
//   text object, as a browser prints a title whose glyphs span several fonts.
//   The engine reads five paragraphs where a person sees one title, and a
//   longer "Studio2026" runs into the pieces after it.
// - Chinese text in an embedded subset font: a CID-keyed TrueType program
//   with only the glyphs its paragraph uses (Identity-H, FontFile2 and a
//   ToUnicode map), as office programs and browsers embed one. A Chinese
//   character it lacks needs a Chinese face the engine doesn't have.
// - Subsets everywhere: a character outside a piece's or the paragraph's font
//   makes the engine draw the edit in a stand-in (Helvetica), with other
//   widths than the original.
//
// The glyphs are blocky placeholders drawn from bitmaps here (no one's font
// is copied); the text layer, the widths and the font dictionaries are what a
// browser writes.

/// The page `--edit-sample` writes (the notes above): US Letter, as a
/// browser prints one.
pub(crate) fn printed_flyer() -> Vec<u8> {
    const TITLE: f64 = 27.8;
    const BODY: f64 = 12.65;
    let mut objs: Vec<Vec<u8>> = vec![Vec::new(), Vec::new(), Vec::new()]; // the catalog, the pages, the page: last
    let helvetica = add(&mut objs, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    let latin = ['S', 't', 'u', 'd', 'i', 'o', '2', '0', '6'];
    let first = ['春', '发', '布'];
    let second = ['季', '会'];
    let t1 = type3(&mut objs, &latin, latin_glyph);
    let t2 = type3(&mut objs, &first, cjk_glyph);
    let t3 = type3(&mut objs, &second, cjk_glyph);
    let lines = ["春季发布会展示新的应用和工具", "报名截止 2026 年 4 月 30 日"];
    let mut body: Vec<char> = Vec::new();
    for c in lines.iter().flat_map(|l| l.chars()) {
        if !body.contains(&c) {
            body.push(c);
        }
    }
    let c1 = cid_subset(&mut objs, "ABCDEF+FixtureSans-SC", &body);

    let mut ops = String::new();
    ops.push_str("BT /F1 9.14 Tf 0.35 0.37 0.4 rg 1 0 0 1 44.62 708 Tm (OctoSense sample - a page as a web browser prints it) Tj ET\n");
    // The title, a piece at a time: each in its own text object and font.
    let mut x = 44.62;
    for (font, chars, text) in [("T1", &latin[..], "Studio2026"), ("T2", &first[..], "春"), ("T3", &second[..], "季"), ("T2", &first[..], "发布"), ("T3", &second[..], "会")] {
        let codes: String = text.chars().map(|c| format!("{:02X}", chars.iter().position(|k| *k == c).unwrap() + 1)).collect();
        ops.push_str(&format!("BT /{font} {TITLE} Tf 0.07 0.09 0.15 rg 1 0 0 1 {x:.2} 670 Tm <{codes}> Tj ET\n"));
        let em: i32 = text.chars().map(|c| i32::from(if c.is_ascii() { latin_glyph(c) } else { cjk_glyph(c) }.advance)).sum();
        x += f64::from(em) / 1000.0 * TITLE;
    }
    // The Chinese paragraph: two lines in the embedded subset.
    let cids = |line: &str| -> String { line.chars().map(|c| format!("{:04X}", body.iter().position(|k| *k == c).unwrap() + 1)).collect() };
    ops.push_str(&format!("BT /C1 {BODY} Tf 0.12 0.14 0.16 rg 1 0 0 1 44.62 630 Tm <{}> Tj 0 -22.5 Td <{}> Tj ET\n", cids(lines[0]), cids(lines[1])));
    // A paragraph in Helvetica just below it.
    ops.push_str("BT /F1 12 Tf 0.12 0.14 0.16 rg 1 0 0 1 44.62 588 Tm (Doors open at nine in the main hall. Bring a) Tj 0 -16.5 Td (laptop and the badge we send you by email.) Tj ET\n");

    let contents = add(&mut objs, pdf_stream(ops.as_bytes(), ""));
    let info = add(
        &mut objs,
        b"<< /Title (Studio2026 spring launch) /Creator (PDF Tools sample generator: a page as a web browser prints it) \
          /Producer (octosense-pdf-service pdftools_fixture) /CreationDate (D:20261011090000Z) >>"
            .to_vec(),
    );
    objs[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    objs[1] = b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec();
    objs[2] = format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 {helvetica} 0 R /T1 {t1} 0 R /T2 {t2} 0 R \
         /T3 {t3} 0 R /C1 {c1} 0 R >> >> /Contents {contents} 0 R >>"
    )
    .into_bytes();
    let mut out = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).bytes());
    out
}

/// Add an object: its number.
fn add(objs: &mut Vec<Vec<u8>>, body: Vec<u8>) -> usize {
    objs.push(body);
    objs.len()
}

/// A stream object of `data`, with `extra` entries in its dictionary.
fn pdf_stream(data: &[u8], extra: &str) -> Vec<u8> {
    let mut out = format!("<< /Length {}{extra} >>\nstream\n", data.len()).into_bytes();
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream");
    out
}

/// A glyph: its advance in thousandths of an em, and the filled boxes
/// `[x, y, w, h]` that draw it, y up from the baseline.
struct Glyph {
    advance: i16,
    boxes: Vec<[i16; 4]>,
}

impl Glyph {
    /// Its bounding box `[x0, y0, x1, y1]` (all zero for a blank glyph).
    fn bounds(&self) -> [i16; 4] {
        self.boxes.iter().fold(None, |b: Option<[i16; 4]>, r| {
            let r = [r[0], r[1], r[0] + r[2], r[1] + r[3]];
            Some(b.map_or(r, |b| [b[0].min(r[0]), b[1].min(r[1]), b[2].max(r[2]), b[3].max(r[3])]))
        })
        .unwrap_or([0; 4])
    }
}

/// A 5 × 7 bitmap (top row first, the leftmost pixel the high bit) of each
/// Latin character the sample draws in its own fonts.
fn latin_bitmap(c: char) -> [u8; 7] {
    match c {
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        't' => [0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00100, 0b00011],
        'u' => [0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101],
        'd' => [0b00001, 0b00001, 0b01101, 0b10011, 0b10001, 0b10011, 0b01101],
        'i' => [0b00100, 0b00000, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110],
        'o' => [0b00000, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        _ => [0; 7],
    }
}

/// A Latin character in 100-unit pixels from its bitmap, each row's runs one
/// box; a space is blank. Advances: 600, a space 300.
fn latin_glyph(c: char) -> Glyph {
    if c == ' ' {
        return Glyph { advance: 300, boxes: Vec::new() };
    }
    let mut boxes = Vec::new();
    for (row, bits) in latin_bitmap(c).iter().enumerate() {
        let y = (6 - row as i16) * 100;
        let mut col = 0;
        while col < 5 {
            if bits >> (4 - col) & 1 == 0 {
                col += 1;
                continue;
            }
            let start = col;
            while col < 5 && bits >> (4 - col) & 1 == 1 {
                col += 1;
            }
            boxes.push([50 + start as i16 * 100, y, (col - start) as i16 * 100, 100]);
        }
    }
    Glyph { advance: 600, boxes }
}

/// A Chinese character's placeholder: a top stroke and a spine, and strokes
/// picked by its code point, so the sample's characters look different.
fn cjk_glyph(c: char) -> Glyph {
    let n = c as u32;
    let mut boxes = vec![[120, 700, 760, 70], [465, -40, 70, 800]];
    let strokes = [[120, 470, 760, 60], [120, 240, 760, 60], [120, 20, 760, 60], [200, 120, 60, 520], [740, 120, 60, 520], [300, 560, 400, 50]];
    for (bit, stroke) in strokes.iter().enumerate() {
        if n >> bit & 1 == 1 {
            boxes.push(*stroke);
        }
    }
    Glyph { advance: 1000, boxes }
}

/// A ToUnicode CMap: code `i + 1` (`bytes` long) is `chars[i]`.
fn to_unicode(chars: &[char], bytes: usize) -> String {
    let digits = bytes * 2;
    let (lo, hi) = if bytes == 1 { ("00", "FF") } else { ("0000", "FFFF") };
    let mut s = format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<{lo}> <{hi}>\nendcodespacerange\n{} beginbfchar\n",
        chars.len()
    );
    for (i, c) in chars.iter().enumerate() {
        let mut units = [0u16; 2];
        let utf16: String = c.encode_utf16(&mut units).iter().map(|u| format!("{u:04X}")).collect();
        s.push_str(&format!("<{:0digits$X}> <{utf16}>\n", i + 1));
    }
    s.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

/// An unnamed Type 3 font as a browser embeds one: `chars` get the codes 1,
/// 2, … in order, each drawn by `glyph`, with a ToUnicode map. Its number.
fn type3(objs: &mut Vec<Vec<u8>>, chars: &[char], glyph: fn(char) -> Glyph) -> usize {
    let (mut procs, mut widths) = (Vec::new(), Vec::new());
    for (i, &c) in chars.iter().enumerate() {
        let g = glyph(c);
        let [x0, y0, x1, y1] = g.bounds();
        let mut draw = format!("{} 0 {x0} {y0} {x1} {y1} d1\n", g.advance);
        for [x, y, w, h] in &g.boxes {
            draw.push_str(&format!("{x} {y} {w} {h} re\n"));
        }
        draw.push_str("f\n");
        procs.push(format!("/g{} {} 0 R", i + 1, add(objs, pdf_stream(draw.as_bytes(), ""))));
        widths.push(g.advance.to_string());
    }
    let cmap = add(objs, pdf_stream(to_unicode(chars, 1).as_bytes(), ""));
    let names: Vec<String> = (1..=chars.len()).map(|i| format!("/g{i}")).collect();
    add(
        objs,
        format!(
            "<< /Type /Font /Subtype /Type3 /FontBBox [0 -120 1000 880] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << {} >> \
             /Encoding << /Type /Encoding /Differences [1 {}] >> /FirstChar 1 /LastChar {} /Widths [{}] /ToUnicode {cmap} 0 R /Resources << >> >>",
            procs.join(" "),
            names.join(" "),
            chars.len(),
            widths.join(" ")
        )
        .into_bytes(),
    )
}

/// A CID-keyed TrueType subset (Type0, Identity-H) named `name`, with a glyph
/// for each of `chars` and no other: CID `i + 1` is `chars[i]`, its program
/// embedded (FontFile2) and its widths in `/W`. Its number.
fn cid_subset(objs: &mut Vec<Vec<u8>>, name: &str, chars: &[char]) -> usize {
    let glyphs: Vec<Glyph> = chars.iter().map(|&c| if c.is_ascii() { latin_glyph(c) } else { cjk_glyph(c) }).collect();
    let program = truetype(&glyphs);
    let file = add(objs, pdf_stream(&program, &format!(" /Length1 {}", program.len())));
    let descriptor = add(
        objs,
        format!(
            "<< /Type /FontDescriptor /FontName /{name} /Flags 4 /FontBBox [0 -120 1000 880] /ItalicAngle 0 /Ascent 880 /Descent -120 \
             /CapHeight 700 /StemV 80 /FontFile2 {file} 0 R >>"
        )
        .into_bytes(),
    );
    let widths: Vec<String> = glyphs.iter().map(|g| g.advance.to_string()).collect();
    let cid = add(
        objs,
        format!(
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
             /FontDescriptor {descriptor} 0 R /DW 1000 /W [1 [{}]] /CIDToGIDMap /Identity >>",
            widths.join(" ")
        )
        .into_bytes(),
    );
    let cmap = add(objs, pdf_stream(to_unicode(chars, 2).as_bytes(), ""));
    add(objs, format!("<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {cmap} 0 R >>").into_bytes())
}

/// A TrueType program whose glyph 0 is blank (.notdef) and glyph `i + 1`
/// draws `glyphs[i]`'s boxes, one contour each: the tables a renderer needs
/// to draw outlines by glyph id (`glyf`, `head`, `hhea`, `hmtx`, `loca`,
/// `maxp`), which is how a CID-keyed font with an identity CIDToGIDMap reaches
/// its glyphs.
fn truetype(glyphs: &[Glyph]) -> Vec<u8> {
    let blank = Glyph { advance: 500, boxes: Vec::new() };
    let all: Vec<&Glyph> = std::iter::once(&blank).chain(glyphs).collect();
    let push16 = |v: &mut Vec<u8>, x: i32| v.extend_from_slice(&(x as u16).to_be_bytes());
    let (mut glyf, mut loca, mut hmtx) = (Vec::new(), Vec::new(), Vec::new());
    let (mut most_points, mut most_contours) = (0, 0);
    for g in &all {
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        let [x0, y0, x1, y1] = g.bounds();
        push16(&mut hmtx, i32::from(g.advance));
        push16(&mut hmtx, i32::from(x0));
        if g.boxes.is_empty() {
            continue;
        }
        let n = g.boxes.len() as i32;
        most_points = most_points.max(n * 4);
        most_contours = most_contours.max(n);
        for v in [n, i32::from(x0), i32::from(y0), i32::from(x1), i32::from(y1)] {
            push16(&mut glyf, v);
        }
        for k in 0..n {
            push16(&mut glyf, k * 4 + 3);
        }
        push16(&mut glyf, 0); // no instructions
        glyf.extend(std::iter::repeat_n(1u8, (n * 4) as usize)); // every point on the curve
        // Each box clockwise from its lower left, as deltas from the point before.
        let points: Vec<(i32, i32)> = g
            .boxes
            .iter()
            .flat_map(|&[x, y, w, h]| {
                let (x, y, w, h) = (i32::from(x), i32::from(y), i32::from(w), i32::from(h));
                [(x, y), (x, y + h), (x + w, y + h), (x + w, y)]
            })
            .collect();
        for axis in 0..2 {
            let mut last = 0;
            for p in &points {
                let v = if axis == 0 { p.0 } else { p.1 };
                push16(&mut glyf, v - last);
                last = v;
            }
        }
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
    }
    loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
    let count = all.len() as i32;
    let mut head = Vec::new();
    for v in [1, 0] {
        push16(&mut head, v); // version 1.0
    }
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // font revision
    head.extend_from_slice(&0u32.to_be_bytes()); // checksum adjustment
    head.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magic number
    push16(&mut head, 3); // flags: baseline and left side bearing at 0
    push16(&mut head, 1000); // units per em
    head.extend_from_slice(&[0; 16]); // created, modified
    for v in [0, -120, 1000, 880, 0, 8, 2, 1, 0] {
        push16(&mut head, v); // bounds, mac style, lowest ppem, direction, long loca, glyph format
    }
    let mut hhea = 0x0001_0000u32.to_be_bytes().to_vec();
    for v in [880, -120, 0, 1000, 0, 0, 1000, 1, 0, 0, 0, 0, 0, 0, 0, count] {
        push16(&mut hhea, v); // ascender … caret, reserved, metric format, metrics count
    }
    let mut maxp = 0x0001_0000u32.to_be_bytes().to_vec();
    for v in [count, most_points, most_contours, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0] {
        push16(&mut maxp, v);
    }
    let tables: [(&[u8; 4], Vec<u8>); 6] = [(b"glyf", glyf), (b"head", head), (b"hhea", hhea), (b"hmtx", hmtx), (b"loca", loca), (b"maxp", maxp)];
    let mut out = 0x0001_0000u32.to_be_bytes().to_vec();
    for v in [6, 64, 2, 32] {
        push16(&mut out, v); // tables, search range, entry selector, range shift
    }
    let mut offset = 12 + 16 * tables.len();
    let mut data = Vec::new();
    for (tag, table) in &tables {
        let mut padded = table.clone();
        while padded.len() % 4 != 0 {
            padded.push(0);
        }
        let sum = padded.chunks(4).fold(0u32, |s, w| s.wrapping_add(u32::from_be_bytes([w[0], w[1], w[2], w[3]])));
        out.extend_from_slice(*tag);
        out.extend_from_slice(&sum.to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(table.len() as u32).to_be_bytes());
        offset += padded.len();
        data.extend(padded);
    }
    out.extend(data);
    out
}
