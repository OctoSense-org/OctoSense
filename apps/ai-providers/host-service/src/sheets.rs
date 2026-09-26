//! The host's sheets: Splash programs run in a host-owned isolate over the
//! app. What is typed on them (a key, a PIN, a pasted code) and what they
//! show (the QR, its PIN) reaches the service or comes from it, never the
//! app. Values from the profile are embedded as string literals, so they are
//! stripped of anything that could end a literal first.
use octosense_llm_config::{registry, ApiType, Provider};

/// A Splash string literal's contents: no quote, backslash or control
/// character can end it early.
fn lit(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '"' => '\'',
            '\\' => '/',
            c => c,
        })
        .collect()
}

const STYLES: &str = r##"let ink = #x1c1c1e
let secondary = #x8e8e93
let accent = #x007aff
let Field = TextInput{width: Fill height: 40
    draw_bg +: {color: #xf2f2f7 color_hover: #xf2f2f7 color_focus: #xf2f2f7 color_empty: #xf2f2f7
        border_color: #x00000000 border_color_hover: #x00000000 border_color_focus: #x007aff border_color_empty: #x00000000 border_radius: 10.0}
    draw_text +: {color: #x1c1c1e color_hover: #x1c1c1e color_focus: #x1c1c1e color_empty: #x8e8e93 color_empty_hover: #x8e8e93}
}
let Caption = Label{text: "" draw_text.color: #x8e8e93 draw_text.text_style.font_size: 11}
let Note = Label{width: Fill text: "" draw_text.color: #x3a3a3c draw_text.text_style.font_size: 12}
let Choice = ButtonFlat{height: 32 width: Fill
    draw_bg +: {border_radius: 8.0 color: #xf2f2f7 color_hover: #xe5e5ea color_down: #xd1d1d6 border_size: 0.0}
    draw_text +: {color: #x3a3a3c color_hover: #x3a3a3c color_down: #x3a3a3c text_style +: {font_size: 13}}}
let Plain = ButtonFlat{height: 40
    draw_bg +: {color: #x00000000 color_hover: #x0000000a color_down: #x00000014 border_size: 0.0}
    draw_text +: {color: #x007aff color_hover: #x007aff color_down: #x007aff text_style +: {font_size: 15}}}
let Primary = ButtonFlat{height: 40 padding: Inset{left: 20 right: 20}
    draw_bg +: {border_radius: 20.0 color: #x007aff color_hover: #x0a84ff color_down: #x0062cc border_size: 0.0}
    draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff text_style +: {font_size: 15}}}
let Title = Label{width: Fill text: "" draw_text.color: #x1c1c1e draw_text.text_style: theme.font_bold{font_size: 17}}
let Status = Label{width: Fill text: "" draw_text.color: #xff3b30 draw_text.text_style.font_size: 12}
"##;

/// A card over a dimmed backdrop, scrolling, with `actions` at the top so
/// the keyboard never hides them.
fn frame(actions: &str, content: &str) -> String {
    format!(
        r##"SolidView{{width: Fill height: Fill flow: Down draw_bg.color: #x000000aa new_batch: true
    ScrollYView{{width: Fill height: Fill flow: Down padding: Inset{{left: 12 right: 12 top: 24 bottom: 24}}
    RoundedView{{width: Fill height: Fit flow: Down spacing: 8 padding: 16 new_batch: true show_bg: true draw_bg.color: #xffffff draw_bg.border_radius: 18.0
        View{{width: Fill height: Fit flow: Right spacing: 8 align: Align{{y: 0.5}}
{actions}
        }}
{content}
    }}
    }}
}}
"##
    )
}

fn api_type_name(t: Option<ApiType>) -> &'static str {
    t.map(|t| t.as_str()).unwrap_or("default")
}

/// Add (`None`) or edit a provider: the family, its model, the advanced
/// route, and the key, which is typed here and nowhere else.
pub fn edit(existing: Option<&Provider>) -> String {
    let (family, label) = match existing.and_then(|p| registry::lookup(&p.family)) {
        Some(f) => (f.id, f.label),
        None => ("", "Choose one below"),
    };
    let model = existing.and_then(|p| p.model.clone()).unwrap_or_default();
    let base = existing.and_then(|p| p.base_url.clone()).unwrap_or_default();
    let api = api_type_name(existing.and_then(|p| p.api_type));
    let title = if existing.is_some() { "Edit provider" } else { "Add a provider" };
    let key_note = if existing.is_some() { "Leave empty to keep the saved key." } else { "" };
    let mut script = format!(
        r##"let family = "{family}"
let protocol = "{api}"
fn pick(id, label, model, base, keyed){{
    family = id
    ui.family.set_text(label)
    if model == "" {{ ui.model_note.set_text("This provider needs a model.") }} else {{ ui.model_note.set_text("Default: " + model) }}
    if base == "" {{ ui.base_note.set_text("This provider needs a base URL.") }} else {{ ui.base_note.set_text("Default: " + base) }}
    if keyed {{ ui.key_note.set_text("{key_note}") }} else {{ ui.key_note.set_text("Optional for this provider.") }}
}}
fn choose(p){{
    protocol = p
    ui.protocol.set_text("Protocol: " + p)
}}
fn submit(){{
    ui.status.set_text("Saving…")
    host.request("llm.sheet.submit", {{family: family model: ui.model.text() base_url: ui.base_url.text() api_type: protocol key: ui.key.text()}},
        fn(r){{ if r.is_ok {{ ui.status.set_text("Saved") }} else {{ ui.status.set_text(r.error) }} }})
}}
fn cancel(){{ host.request("llm.sheet.cancel", {{}}, fn(r){{}}) }}
{STYLES}"##,
        family = lit(family),
        api = api,
        key_note = key_note,
    );
    let mut grid = String::new();
    for pair in registry::all().chunks(2) {
        grid.push_str("        View{width: Fill height: Fit flow: Right spacing: 6\n");
        for f in pair {
            // A long name loses its aside on the button, not on the label.
            let short = match f.label.split_once(" (") {
                Some((head, _)) if f.label.chars().count() > 20 => head,
                _ => f.label,
            };
            grid.push_str(&format!(
                "            Choice{{text: \"{short}\" on_click: || pick(\"{id}\", \"{label}\", \"{model}\", \"{base}\", {keyed})}}\n",
                short = lit(short),
                label = lit(f.label),
                id = lit(f.id),
                model = lit(f.default_model.unwrap_or("")),
                base = lit(f.default_base_url.unwrap_or("")),
                keyed = f.key_required,
            ));
        }
        grid.push_str("        }\n");
    }
    let actions = r#"            Plain{text: "Cancel" on_click: || cancel()}
            View{width: Fill height: 1}
            Primary{text: "Save" on_click: || submit()}"#;
    let content = format!(
        r#"        Title{{text: "OctoSense · {title}"}}
        Note{{text: "The key stays with OctoSense. The app that asked sees only that a key is set."}}
        status := Status{{}}
        Caption{{text: "Provider"}}
        family := Label{{width: Fill text: "{label}" draw_text.color: ink draw_text.text_style: theme.font_bold{{font_size: 15}}}}
{grid}        Caption{{text: "API key"}}
        key := Field{{empty_text: "key" is_password: true}}
        key_note := Caption{{text: "{key_note}"}}
        Caption{{text: "Model"}}
        model := Field{{text: "{model}" empty_text: "the provider's default"}}
        model_note := Caption{{text: ""}}
        Caption{{text: "Advanced: base URL"}}
        base_url := Field{{text: "{base}" empty_text: "the provider's default"}}
        base_note := Caption{{text: ""}}
        protocol := Caption{{text: "Protocol: {api}"}}
        View{{width: Fill height: Fit flow: Right spacing: 6
            Choice{{text: "Default" on_click: || choose("default")}}
            Choice{{text: "OpenAI" on_click: || choose("openai")}}
            Choice{{text: "Anthropic" on_click: || choose("anthropic")}}
            Choice{{text: "Responses" on_click: || choose("responses")}}
        }}"#,
        label = lit(label),
        model = lit(&model),
        base = lit(&base),
    );
    script.push_str(&frame(actions, &content));
    script
}

/// Shown while the code is sealed (Argon2id takes a moment): it asks for
/// the finished sheet until the service swaps it in.
pub fn export_waiting() -> String {
    let mut script = format!(
        r##"let poll_timer = nil
fn poll(){{
    host.request("llm.sheet.qr", {{}}, fn(r){{
        if !r.is_ok {{
            ui.status.set_text(r.error)
            if poll_timer != nil {{ stop_timer(poll_timer); poll_timer = nil }}
        }}
    }})
}}
fn cancel(){{ host.request("llm.sheet.cancel", {{}}, fn(r){{}}) }}
poll_timer = start_interval(0.3, || poll())
{STYLES}"##
    );
    script.push_str(&frame(
        r#"            Plain{text: "Cancel" on_click: || cancel()}
            View{width: Fill height: 1}"#,
        r#"        Title{text: "OctoSense · Code for your phone"}
        Note{text: "Preparing the code…"}
        status := Status{}"#,
    ));
    script
}

/// The dark and light runs of one QR row (`true` = dark), without the
/// light run that ends it.
fn runs(row: &[bool]) -> Vec<(bool, usize)> {
    let mut out: Vec<(bool, usize)> = Vec::new();
    for &dark in row {
        match out.last_mut() {
            Some((d, n)) if *d == dark => *n += 1,
            _ => out.push((dark, 1)),
        }
    }
    if out.last().is_some_and(|(d, _)| !d) {
        out.pop();
    }
    out
}

/// The QR as views: one row per run of identical module rows, each a
/// sequence of dark and light spans `px` wide per module, on a white card
/// with a four-module quiet zone. The lines between `// qr-begin` and
/// `// qr-end` are exactly these rows.
pub fn qr_views(size: usize, modules: &[bool], px: usize) -> String {
    let mut out = format!(
        "        SolidView{{width: Fit height: Fit flow: Down padding: {pad} draw_bg.color: #xffffff new_batch: true\n        // qr-begin {px}\n",
        pad = 4 * px
    );
    let rows: Vec<&[bool]> = modules.chunks(size).collect();
    let mut y = 0;
    while y < rows.len() {
        let mut repeat = 1;
        while y + repeat < rows.len() && rows[y + repeat] == rows[y] {
            repeat += 1;
        }
        out.push_str(&format!("            QrRow{{height: {}", repeat * px));
        for (dark, n) in runs(rows[y]) {
            out.push_str(&format!(" {}{{width: {}}}", if dark { "Dark" } else { "Light" }, n * px));
        }
        out.push_str("}\n");
        y += repeat;
    }
    out.push_str("        // qr-end\n        }\n");
    out
}

/// Pixels per module: the code about 280 wide, never under 2.
pub fn module_px(size: usize) -> usize {
    (280 / size.max(1)).max(2)
}

/// The phone QR, its PIN beside it, and a countdown after which the sheet
/// closes itself.
pub fn export(size: usize, modules: &[bool], pin: &str, labels: &[String], lifetime_secs: u64) -> String {
    let mut script = format!(
        r##"let left = {lifetime_secs}
let tick_timer = nil
fn close(){{
    if tick_timer != nil {{ stop_timer(tick_timer); tick_timer = nil }}
    host.request("llm.sheet.cancel", {{}}, fn(r){{}})
}}
fn tick(){{
    left = left - 1
    if left <= 0 {{ close() return }}
    let m = floor(left / 60)
    let s = left - m * 60
    if s < 10 {{ ui.countdown.set_text("Expires in " + m + ":0" + s) }} else {{ ui.countdown.set_text("Expires in " + m + ":" + s) }}
}}
tick_timer = start_interval(1, || tick())
let Dark = SolidView{{height: Fill draw_bg.color: #x000000}}
let Light = View{{height: Fill}}
let QrRow = View{{width: Fit flow: Right}}
{STYLES}"##
    );
    let minutes = lifetime_secs / 60;
    let content = format!(
        r#"        Title{{text: "OctoSense · Code for your phone"}}
        Note{{text: "On the phone, open AI providers and choose Scan QR from desktop, then type the PIN. The code carries your keys: close it when you are done."}}
        View{{width: Fill height: Fit flow: Down align: Align{{x: 0.5}}
{qr}        }}
        Caption{{text: "PIN"}}
        pin := Label{{width: Fill align: Align{{x: 0.5}} text: "{pin}" draw_text.color: ink draw_text.text_style: theme.font_bold{{font_size: 28}}}}
        countdown := Caption{{text: "Expires in {minutes}:00"}}
        Note{{text: "Providers: {providers}"}}"#,
        qr = qr_views(size, modules, module_px(size)),
        pin = lit(pin),
        providers = lit(&labels.join(", ")),
    );
    script.push_str(&frame(
        r#"            View{width: Fill height: 1}
            Primary{text: "Close" on_click: || close()}"#,
        &content,
    ));
    script
}

/// Import a code: scan it (where the host has a scanner) or paste it, then
/// type its PIN.
pub fn import(can_scan: bool) -> String {
    let mut script = format!(
        r##"let can_scan = {can_scan}
fn scan(){{
    ui.status.set_text("")
    ui.note.set_text("Point the camera at the code on your computer…")
    host.request("llm.sheet.scan", {{}}, fn(r){{
        if r.is_ok {{
            if r.data.needs_pin == true {{ ui.note.set_text("Code scanned. Type the PIN shown beside it.") }} else {{ ui.note.set_text("Code scanned. Tap Import.") }}
        }} else {{ ui.note.set_text("") ui.status.set_text(r.error) }}
    }})
}}
fn submit(){{
    ui.status.set_text("")
    ui.note.set_text("Checking the code…")
    host.request("llm.sheet.import", {{text: ui.code.text() pin: ui.pin.text()}}, fn(r){{
        if r.is_ok {{ ui.note.set_text("Imported") }} else {{ ui.note.set_text("") ui.status.set_text(r.error) }}
    }})
}}
fn cancel(){{ host.request("llm.sheet.cancel", {{}}, fn(r){{}}) }}
if can_scan {{ start_timeout(0.1, || scan()) }}
{STYLES}"##
    );
    let (scan_button, paste) = if can_scan {
        ("\n        Choice{text: \"Scan again\" on_click: || scan()}", "Or paste the code")
    } else {
        ("", "Paste the code")
    };
    let content = format!(
        r#"        Title{{text: "OctoSense · Import providers"}}
        Note{{text: "Show the code on your computer: AI providers, Show QR for phone. The keys it carries go to OctoSense, not to the app that asked."}}
        status := Status{{}}
        note := Note{{}}{scan_button}
        Caption{{text: "{paste} (OCTOS1E:…)"}}
        code := Field{{empty_text: "OCTOS1E:…"}}
        Caption{{text: "PIN"}}
        pin := Field{{empty_text: "XXXX-XXXX" is_password: true}}"#
    );
    script.push_str(&frame(
        r#"            Plain{text: "Cancel" on_click: || cancel()}
            View{width: Fill height: 1}
            Primary{text: "Import" on_click: || submit()}"#,
        &content,
    ));
    script
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_cannot_be_ended_early() {
        assert_eq!(lit("a\"b\\c\nd"), "a'b/cd");
    }

    #[test]
    fn rows_drop_their_trailing_light_run() {
        assert_eq!(runs(&[false, true, true, false, false]), [(false, 1), (true, 2)]);
        assert_eq!(runs(&[false, false]), []);
    }

    #[test]
    fn the_edit_sheet_offers_every_family_and_prefills_a_route() {
        let mut p = Provider::new("deepseek", Some("deepseek-chat".into()));
        p.base_url = Some("https://example.test/v1".into());
        let body = edit(Some(&p));
        for f in registry::all() {
            assert!(body.contains(&format!("pick(\"{}\"", f.id)), "{}", f.id);
        }
        assert!(body.contains("let family = \"deepseek\""));
        assert!(body.contains("text: \"deepseek-chat\""));
        assert!(body.contains("is_password: true"));
        assert!(body.contains("llm.sheet.submit"));
    }
}
