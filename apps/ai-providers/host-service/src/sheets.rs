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
    draw_bg +: {border_radius: 8.0 color: #xf2f2f7 color_focus: #xf2f2f7 color_hover: #xe5e5ea color_down: #xd1d1d6 border_size: 0.0}
    draw_text +: {color: #x3a3a3c color_focus: #x3a3a3c color_hover: #x3a3a3c color_down: #x3a3a3c text_style +: {font_size: 13}}}
let Plain = ButtonFlat{height: 40
    draw_bg +: {color: #x00000000 color_focus: #x00000000 color_hover: #x0000000a color_down: #x00000014 border_size: 0.0}
    draw_text +: {color: #x007aff color_focus: #x007aff color_hover: #x007aff color_down: #x007aff text_style +: {font_size: 15}}}
let Primary = ButtonFlat{height: 40 padding: Inset{left: 20 right: 20}
    draw_bg +: {border_radius: 20.0 color: #x007aff color_focus: #x007aff color_hover: #x0a84ff color_down: #x0062cc border_size: 0.0}
    draw_text +: {color: #xffffff color_focus: #xffffff color_hover: #xffffff color_down: #xffffff text_style +: {font_size: 15}}}
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

/// Connection controls live on the trusted sheet. The access token is
/// copied by native code and is never present in this script or its replies.
pub fn connect_client() -> String {
    let mut script = String::from(r#"
fn status(text){ ui.status.set_text(text) }
fn refresh(){
    status("Starting Octos…")
    host.request("llm.sheet.client_info", {}, fn(r){
        if !r.is_ok { status(r.error) return }
        ui.origin.set_text(r.data.origin)
        ui.endpoint.set_text(r.data.endpoint)
        ui.session.set_text(r.data.session_id)
        ui.web_origin.set_text(r.data.web_origin)
        ui.web_url.set_text(r.data.web_url)
        ui.copy.set_visible(r.data.can_copy)
        status("Connected. Native apps and external clients share this agent runtime.")
    })
}
fn save_origin(){
    status("Restarting Octos…")
    host.request("llm.sheet.client_origin", {origin: ui.web_origin.text()}, fn(r){
        if !r.is_ok { status(r.error) return }
        refresh()
    })
}
fn copy_token(){
    host.request("llm.sheet.client_copy_token", {}, fn(r){
        if r.is_ok { status("Access token copied. Paste it into your client.") } else { status(r.error) }
    })
}
fn open_web(){
    host.request("llm.sheet.client_open", {}, fn(r){ if !r.is_ok { status(r.error) } })
}
fn close(){ host.request("llm.sheet.cancel", {}, nil) }
start_timeout(0.1, || refresh())
"#);
    script.push_str(STYLES);
    script.push_str(&frame(
        r#"Plain{text: "Done" on_click: || close()}
           View{width: Fill height: 1}
           Plain{text: "Refresh" on_click: || refresh()}"#,
        r#"Title{text: "Talk to Octos"}
        Note{text: "Connect OctosCode Web or OctosCode in a terminal to this device's assistant."}
        Caption{text: "Server origin · for the web client"}
        origin := Field{is_read_only: true}
        Caption{text: "WebSocket endpoint · for OctosCode --endpoint"}
        endpoint := Field{is_read_only: true}
        Caption{text: "System-agent conversation · profile _main"}
        session := Field{is_read_only: true}
        copy := Primary{text: "Copy access token" on_click: || copy_token()}
        Caption{text: "Web client origin · for example http://localhost:4173"}
        web_origin := Field{empty_text: "http://localhost:4173"}
        Note{text: "Use the address where your OctosCode Web client is hosted. Saving a changed origin restarts Octos and interrupts active work."}
        Plain{text: "Save origin and restart" on_click: || save_origin()}
        Caption{text: "System conversation link · open on either device"}
        web_url := Field{is_read_only: true}
        Primary{text: "Open web client" on_click: || open_web()}
        Note{text: "Enter the server origin and paste the access token in the web client. The link selects the system conversation. A computer connects through a tunnel. Closing the browser can interrupt work started there."}
        status := Status{}"#,
    ));
    script
}

/// The add and edit sheet: a five-step wizard in Octoscode's `/model` order,
/// one step per page, a step indicator and progress bar at the top, and Back
/// / Next pinned at the bottom (Next is greyed out until the step is valid):
///
/// 1. Choose model family: a search field and the families (`llm.families`),
///    each with its model count and whether a key is saved.
/// 2. Choose model: a pull-down of the family's catalog models
///    (`llm.models`: display name, context, price), the recommended one
///    preselected, "Custom model ID…", and "Fetch models from provider" (the
///    endpoint's own list, with the saved key).
/// 3. Choose provider route: a pull-down of the model's catalog endpoints
///    (Official API first) and "Custom endpoint…" (base URL and API protocol).
/// 4. API key: write only; skipped for a keyless family; "Leave blank to keep
///    the saved key" when editing or when the family has one.
/// 5. Test connection & save: a summary, a big "Test connection" button with
///    its live result, and the save ("Save as primary" into an empty list,
///    "Add as fallback" after it, "Save" when editing), enabled once a test
///    passes; after a network failure a small "Save without testing" saves
///    anyway.
///
/// Pull-downs are drawn in the script (Makepad's DropDown has no Splash
/// binding): a tinted button with a chevron that opens its options below.
/// The typed fields are static widgets, so a re-render never loses them.
pub fn edit(existing: Option<&Provider>, has_primary: bool) -> String {
    let family = existing.and_then(|p| registry::lookup(&p.family)).map(|f| f.id).unwrap_or("");
    let model = existing.and_then(crate::model::effective_model).unwrap_or_default();
    let route = existing.map(crate::model::route_choice).unwrap_or_else(|| "official".into());
    let base = existing.filter(|_| route == "custom").and_then(|p| p.base_url.clone()).unwrap_or_default();
    let api = match existing.and_then(|p| p.api_type) {
        Some(ApiType::Anthropic) => "anthropic",
        _ => "openai",
    };
    let (title, action) = match (existing.is_some(), has_primary) {
        (true, _) => ("Edit model", "Save"),
        (false, false) => ("Add a model", "Save as primary"),
        (false, true) => ("Add a model", "Add as fallback"),
    };
    ADD_SHEET
        .replace("@STYLES@", STYLES)
        .replace("@PICKERS@", PICKERS)
        .replace("@EDITING@", if existing.is_some() { "true" } else { "false" })
        .replace("@FAMILY@", &lit(family))
        .replace("@MODEL@", &lit(&model))
        .replace("@ROUTE@", &lit(&route))
        .replace("@BASE@", &lit(&base))
        .replace("@API@", api)
        .replace("@TITLE@", title)
        .replace("@ACTION@", action)
        .replace("@CHEVRON_DOWN@", "\u{f078}")
        .replace("@CHEVRON_UP@", "\u{f077}")
        .replace("@CHEVRON_RIGHT@", "\u{f054}")
        .replace("@CHECK@", "\u{f00c}")
        .replace("@CROSS@", "\u{f00d}")
        .replace("@WARN@", "\u{f071}")
}

/// The pull-down look, shared by the sheet and (copied) the app: a tinted
/// button, option rows a shade lighter, the chosen row tinted, a chevron and
/// a check from the icon font. The colours are the end-to-end test's
/// handles too (scripts/ai_providers_remote.sh in the desktop shell).
const PICKERS: &str = r##"let Drop = RoundedView{width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 10
    padding: Inset{left: 14 right: 16 top: 10 bottom: 10} show_bg: true draw_bg.color: #xedf2fc draw_bg.border_radius: 12.0}
let Opt = RoundedView{width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 10
    padding: Inset{left: 14 right: 16 top: 8 bottom: 8} show_bg: true draw_bg.color: #xf6f8fc draw_bg.border_radius: 10.0}
let OptOn = Opt{draw_bg.color: #xe8f0fe}
let Glyph = Label{text: "" draw_text.color: #x8e8e93 draw_text.text_style: theme.font_icons{font_size: 10}}
let Tick = Label{text: "" draw_text.color: #x007aff draw_text.text_style: theme.font_icons{font_size: 12}}
let Tag = RoundedView{width: Fit height: Fit padding: Inset{left: 6 right: 6 top: 2 bottom: 2} show_bg: true draw_bg.color: #xe2f4e6 draw_bg.border_radius: 6.0}
let TagText = Label{padding: 0 text: "" draw_text.color: #x1f7a3a draw_text.text_style: theme.font_bold{font_size: 10}}
let OptTitle = Label{padding: 0 width: Fit text: "" draw_text.color: #x1c1c1e draw_text.text_style.font_size: 14}
let OptTitleOn = Label{padding: 0 width: Fit text: "" draw_text.color: #x1c1c1e draw_text.text_style: theme.font_bold{font_size: 14}}
let OptNote = Label{padding: 0 width: Fill text: "" draw_text.color: #x8e8e93 draw_text.text_style.font_size: 11}
"##;

const ADD_SHEET: &str = r##"let editing = @EDITING@
let want_family = "@FAMILY@"
let want_model = "@MODEL@"
let want_route = "@ROUTE@"
let families = []
let fam = nil
let models = []
let family_routes = []
let fetched = []
let model = nil
let custom = false
let route = nil
let protocol = "@API@"
let open = ""
let step = 1
let tested = ""
let result = ""
let reason = ""
let result_ms = ""
let busy = false

fn show(label, text){
    label.set_text(text)
    label.set_visible(text != "")
}
fn load_families(q){
    host.request("llm.families", {query: q}, fn(r){
        if !r.is_ok { show(ui.status, r.error) return }
        families = r.data
        ui.fam_list.render()
    })
}
fn model_label(){
    if custom { return ui.custom_model.text().trim() }
    if model == nil { return "" }
    return model.label
}
fn route_label(){
    if route == nil { return "" }
    if route.id == "custom" { return "Custom endpoint · " + ui.base_url.text().trim() }
    return route.label + " · " + route.detail
}
fn key_summary(){
    if !fam.has_key { return "Not needed" }
    if ui.key.text().trim() != "" { return "New key typed" }
    if editing || fam.configured { return "Saved key" }
    return "Missing"
}
fn heading(){
    if step == 1 {
        ui.step.set_text("Step 1 of 5 · Choose model family")
        ui.title.set_text("Choose a model family")
        ui.sub.set_text("The provider that serves the model. More can be added later as fallbacks.")
    }
    if step == 2 {
        ui.step.set_text("Step 2 of 5 · Choose model")
        ui.title.set_text("Choose a model")
        ui.sub.set_text(fam.label + " · " + fam.models_text + ". The recommended one is preselected.")
    }
    if step == 3 {
        ui.step.set_text("Step 3 of 5 · Choose provider route")
        ui.title.set_text("Choose a provider route")
        ui.sub.set_text("Where requests for " + model_label() + " go.")
    }
    if step == 4 {
        ui.step.set_text("Step 4 of 5 · API key")
        ui.title.set_text("API key")
        ui.sub.set_text("For " + fam.label + ". Typed here, kept by OctoSense; the app sees only that a key is set.")
    }
    if step == 5 {
        ui.step.set_text("Step 5 of 5 · Test connection & save")
        ui.title.set_text("Test connection & save")
        ui.sub.set_text("One tiny request checks the route and the key before anything is saved.")
        ui.sum_family.set_text(fam.label)
        ui.sum_model.set_text(model_label())
        ui.sum_route.set_text(route_label())
        ui.sum_key.set_text(key_summary())
    }
}
fn go(n){
    step = n
    open = ""
    show(ui.status, "")
    ui.page1.set_visible(n == 1)
    ui.page2.set_visible(n == 2)
    ui.page3.set_visible(n == 3)
    ui.page4.set_visible(n == 4)
    ui.page5.set_visible(n == 5)
    heading()
    ui.progress.render()
    if n == 2 { ui.model_list.render() }
    if n == 3 { ui.route_list.render() }
    if n == 5 { ui.test_box.render() }
    ui.footer.render()
}
fn valid(){
    if busy { return false }
    if step == 1 { return fam != nil }
    if step == 2 {
        if custom { return ui.custom_model.text().trim() != "" }
        return model != nil
    }
    if step == 3 {
        if route == nil { return false }
        if route.id == "custom" { return ui.base_url.text().trim() != "" }
        return true
    }
    if step == 4 { return ui.key.text().trim() != "" || !fam.key_required || fam.configured || editing }
    return tested == "ok"
}
fn next_text(){
    if step == 5 { return "@ACTION@" }
    return "Next"
}
fn next(){
    if !valid() { return }
    if step == 1 { go(2) return }
    if step == 2 { keep_route() go(3) return }
    if step == 3 {
        if fam.has_key { go(4) } else { go(5) }
        return
    }
    if step == 4 { go(5) return }
    save()
}
fn back(){
    if step == 5 && !fam.has_key { go(3) return }
    if step > 1 { go(step - 1) }
}
fn changed(){
    tested = ""
    ui.footer.render()
}
fn key_note(){
    if !fam.key_required { return "Optional for this provider." }
    if editing { return "Leave blank to keep the saved key." }
    if fam.configured { return "A key for " + fam.label + " is saved. Leave blank to keep it." }
    return "Required. Paste it from the provider's console."
}
fn choose_family(f){
    if fam != nil && fam.id == f.id { ui.fam_list.render() ui.footer.render() return }
    fam = f
    fetched = []
    custom = false
    model = nil
    route = nil
    tested = ""
    ui.custom_box.set_visible(false)
    show(ui.fetch_note, "")
    show(ui.key_note, key_note())
    ui.fam_list.render()
    ui.footer.render()
    host.request("llm.models", {family: f.id}, fn(r){
        if !r.is_ok { show(ui.status, r.error) return }
        models = r.data.models
        family_routes = r.data.routes
        let pick = nil
        let fallback = nil
        for m in models {
            if m.id == want_model { pick = m }
            if m.default == true { fallback = m }
        }
        if pick == nil && fallback == nil { for m in models { if fallback == nil { fallback = m } } }
        if pick == nil && want_model != "" && want_family == f.id {
            use_custom(want_model)
        } else {
            if pick == nil { pick = fallback }
            select_model(pick)
        }
        want_model = ""
        ui.footer.render()
    })
}
fn route_list(){
    if custom || model == nil { return family_routes }
    return model.routes
}
fn keep_route(){
    let found = nil
    let first = nil
    for r in route_list() {
        if first == nil { first = r }
        if route != nil && r.id == route.id { found = r }
        if want_route != "" && r.id == want_route { found = r }
    }
    if want_route == "custom" || (route != nil && route.id == "custom") { found = {id: "custom" label: "Custom endpoint" detail: "Your own base URL"} }
    want_route = ""
    if found == nil { found = first }
    select_route(found)
}
fn select_model(m){
    model = m
    custom = false
    open = ""
    tested = ""
    ui.custom_box.set_visible(false)
    ui.model_list.render()
    ui.footer.render()
}
fn use_custom(id){
    custom = true
    open = ""
    tested = ""
    ui.custom_box.set_visible(true)
    if id != "" { ui.custom_model.set_text(id) }
    ui.model_list.render()
    ui.footer.render()
}
fn select_route(r){
    route = r
    open = ""
    tested = ""
    ui.endpoint_box.set_visible(r.id == "custom")
    ui.protocol_row.render()
    ui.route_list.render()
    ui.footer.render()
}
fn custom_route(){ select_route({id: "custom" label: "Custom endpoint" detail: "Your own base URL"}) }
fn toggle(which){
    if open == which { open = "" } else { open = which }
    ui.model_list.render()
    ui.route_list.render()
}
fn set_protocol(p){
    protocol = p
    tested = ""
    ui.protocol_row.render()
}
// A pull-down's button shows the choice while closed; open, the list below
// shows every option once (the chosen one ticked) and the button asks.
fn model_title(){
    if open == "model" { return "Choose a model" }
    if custom { return "Custom model ID" }
    if model == nil { return "Loading models…" }
    return model.label
}
fn model_note(){
    if open == "model" { return fam.label + " · " + fam.models_text + " in the catalog" }
    if custom { return "Type the model's id below" }
    if model == nil { return "" }
    return model.id + " · " + model.detail
}
fn route_title(){
    if open == "route" { return "Choose a route" }
    if route == nil { return "Official API" }
    return route.label
}
fn route_note(){
    if open == "route" { return "Where requests for " + model_label() + " go" }
    if route == nil { return "" }
    return route.detail
}
fn form(check){
    let mid = ""
    if custom { mid = ui.custom_model.text() } else { if model != nil { mid = model.id } }
    let rid = "official"
    if route != nil { rid = route.id }
    let args = {family: fam.id model: mid route: rid base_url: ui.base_url.text() api_type: protocol key: ui.key.text() check: check}
    return args
}
fn test(){
    if busy { return }
    tested = "testing"
    ui.test_box.render()
    ui.footer.render()
    host.request("llm.sheet.test", form(false), fn(r){
        if !r.is_ok { tested = "fail" result = r.error reason = "not tested" }
        else {
            if r.data.error == nil { tested = "ok" result_ms = "" + r.data.ms } else {
                result = r.data.error
                reason = r.data.reason
                if r.data.network == true { tested = "network" } else { tested = "fail" }
            }
        }
        ui.test_box.render()
        ui.footer.render()
    })
}
fn fetch_models(){
    show(ui.fetch_note, "Asking the provider for its models…")
    host.request("llm.sheet.fetch_models", form(false), fn(r){
        if !r.is_ok { show(ui.fetch_note, r.error) return }
        fetched = r.data.models
        open = "model"
        show(ui.fetch_note, r.data.message)
        ui.model_list.render()
    })
}
fn save(){
    if busy || fam == nil { return }
    busy = true
    show(ui.status, "")
    ui.footer.render()
    host.request("llm.sheet.submit", form(false), fn(r){
        busy = false
        if !r.is_ok { show(ui.status, r.error) ui.footer.render() return }
        ui.footer.render()
    })
}
fn cancel(){ host.request("llm.sheet.cancel", {}, nil) }
fn start(){
    host.request("llm.families", {query: ""}, fn(r){
        if !r.is_ok { show(ui.status, r.error) return }
        families = r.data
        let found = nil
        for f in families { if f.id == want_family { found = f } }
        if found == nil { go(1) ui.fam_list.render() return }
        choose_family(found)
        go(1)
    })
}
start_timeout(0.05, || start())
@STYLES@
@PICKERS@
let Section = Label{text: "" draw_text.color: #x6e6e73 draw_text.text_style: theme.font_bold{font_size: 11}}
let Link = ButtonFlat{height: 32 padding: Inset{left: 4 right: 4}
    draw_bg +: {color: #x00000000 color_focus: #x00000000 color_hover: #x00000000 color_down: #x00000000 border_size: 0.0}
    draw_text +: {color: #x007aff color_focus: #x007aff color_hover: #x0a84ff color_down: #x0062cc text_style +: {font_size: 13}}}
let Back = ButtonFlat{height: 44 padding: Inset{left: 18 right: 18}
    draw_bg +: {border_radius: 12.0 color: #xf2f2f7 color_focus: #xf2f2f7 color_hover: #xe5e5ea color_down: #xd1d1d6 border_size: 0.0}
    draw_text +: {color: #x1c1c1e color_focus: #x1c1c1e color_hover: #x1c1c1e color_down: #x1c1c1e text_style +: {font_size: 15}}}
let Next = ButtonFlat{height: 44 padding: Inset{left: 24 right: 24}
    draw_bg +: {border_radius: 12.0 color: #x007aff color_focus: #x007aff color_hover: #x0a84ff color_down: #x0062cc border_size: 0.0}
    draw_text +: {color: #xffffff color_focus: #xffffff color_hover: #xffffff color_down: #xffffff text_style: theme.font_bold{font_size: 15}}}
let NextOff = ButtonFlat{height: 44 padding: Inset{left: 24 right: 24}
    draw_bg +: {border_radius: 12.0 color: #xe5e5ea color_focus: #xe5e5ea color_hover: #xe5e5ea color_down: #xe5e5ea border_size: 0.0}
    draw_text +: {color: #xaeaeb2 color_focus: #xaeaeb2 color_hover: #xaeaeb2 color_down: #xaeaeb2 text_style: theme.font_bold{font_size: 15}}}
let Bypass = ButtonFlat{height: 34 padding: Inset{left: 14 right: 14}
    draw_bg +: {border_radius: 10.0 color: #xfff1e0 color_focus: #xfff1e0 color_hover: #xffe6c7 color_down: #xffd9ad border_size: 0.0}
    draw_text +: {color: #xb25000 color_focus: #xb25000 color_hover: #xb25000 color_down: #xb25000 text_style +: {font_size: 13}}}
let TestButton = ButtonFlat{width: Fill height: 52
    draw_bg +: {border_radius: 14.0 color: #xe3edff color_focus: #xe3edff color_hover: #xd6e5ff color_down: #xc7dbff border_size: 0.0}
    draw_text +: {color: #x007aff color_focus: #x007aff color_hover: #x007aff color_down: #x007aff text_style: theme.font_bold{font_size: 16}}}
let TestOff = ButtonFlat{width: Fill height: 52
    draw_bg +: {border_radius: 14.0 color: #xf2f2f7 color_focus: #xf2f2f7 color_hover: #xf2f2f7 color_down: #xf2f2f7 border_size: 0.0}
    draw_text +: {color: #x8e8e93 color_focus: #x8e8e93 color_hover: #x8e8e93 color_down: #x8e8e93 text_style: theme.font_bold{font_size: 16}}}
let ResultOk = RoundedView{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5} padding: Inset{left: 14 right: 14 top: 12 bottom: 12}
    show_bg: true draw_bg.color: #xeaf7ee draw_bg.border_radius: 12.0}
let ResultBad = RoundedView{width: Fill height: Fit flow: Down spacing: 8 padding: Inset{left: 14 right: 14 top: 12 bottom: 12}
    show_bg: true draw_bg.color: #xfdeeec draw_bg.border_radius: 12.0}
let Seg = ButtonFlat{height: 38 width: Fill
    draw_bg +: {border_radius: 10.0 color: #xf2f2f7 color_focus: #xf2f2f7 color_hover: #xe5e5ea color_down: #xd1d1d6 border_size: 0.0}
    draw_text +: {color: #x3a3a3c color_focus: #x3a3a3c color_hover: #x3a3a3c color_down: #x3a3a3c text_style +: {font_size: 13}}}
let SegOn = ButtonFlat{height: 38 width: Fill
    draw_bg +: {border_radius: 10.0 color: #x007aff color_focus: #x007aff color_hover: #x0a84ff color_down: #x0062cc border_size: 0.0}
    draw_text +: {color: #xffffff color_focus: #xffffff color_hover: #xffffff color_down: #xffffff text_style +: {font_size: 13}}}
let Bar = RoundedView{width: Fill height: 4 show_bg: true draw_bg.color: #xe5e5ea draw_bg.border_radius: 2.0}
let BarOn = Bar{draw_bg.color: #x007aff}
let SumKey = Label{width: 80 text: "" draw_text.color: #x8e8e93 draw_text.text_style.font_size: 12}
let SumValue = Label{width: Fill text: "" draw_text.color: #x1c1c1e draw_text.text_style.font_size: 13}
let Icon = Label{text: "" draw_text.text_style: theme.font_icons{font_size: 14}}
SolidView{width: Fill height: Fill flow: Down draw_bg.color: #x000000aa new_batch: true padding: Inset{left: 14 right: 14 top: 18 bottom: 18}
    RoundedView{width: Fill height: Fill flow: Down padding: Inset{left: 20 right: 20 top: 16 bottom: 16} new_batch: true show_bg: true draw_bg.color: #xffffff draw_bg.border_radius: 20.0
        View{width: Fill height: Fit flow: Down spacing: 6
            View{width: Fill height: Fit flow: Right align: Align{y: 0.5}
                Caption{width: Fill padding: 0 text: "OctoSense · @TITLE@"}
                Link{text: "Cancel" on_click: || cancel()}
            }
            progress := View{width: Fill height: Fit flow: Right spacing: 6 on_render: || {
                if step >= 1 { BarOn{} } else { Bar{} }
                if step >= 2 { BarOn{} } else { Bar{} }
                if step >= 3 { BarOn{} } else { Bar{} }
                if step >= 4 { BarOn{} } else { Bar{} }
                if step >= 5 { BarOn{} } else { Bar{} }
            }}
            step := Label{width: Fill text: "" padding: Inset{top: 6} draw_text.color: #x007aff draw_text.text_style: theme.font_bold{font_size: 12}}
            title := Label{width: Fill padding: 0 text: "" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 20}}
            sub := Label{width: Fill padding: 0 text: "" draw_text.color: #x6e6e73 draw_text.text_style.font_size: 13}
            status := Status{visible: false}
        }
        ScrollYView{width: Fill height: Fill flow: Down padding: Inset{top: 14 bottom: 8}
            page1 := View{visible: false width: Fill height: Fit flow: Down spacing: 8
                search := Field{empty_text: "Search families or models" on_change: |text| load_families(text)}
                fam_list := View{width: Fill height: Fit flow: Down spacing: 6 on_render: || {
                    View{width: Fill height: 1}
                    if families.len() == 0 { Note{text: "No family or model matches."} }
                    for f in families {
                        GestureView{width: Fill height: Fit on_tap: |x, y| choose_family(f)
                            if fam != nil && fam.id == f.id {
                                OptOn{padding: Inset{left: 14 right: 16 top: 10 bottom: 10}
                                    View{width: Fill height: Fit flow: Down spacing: 2
                                        OptTitleOn{text: f.label}
                                        OptNote{text: f.models_text + " · " + f.key_text}
                                    }
                                    Tick{text: "@CHECK@"}
                                }
                            } else {
                                Opt{padding: Inset{left: 14 right: 16 top: 10 bottom: 10}
                                    View{width: Fill height: Fit flow: Down spacing: 2
                                        OptTitleOn{text: f.label}
                                        OptNote{text: f.models_text + " · " + f.key_text}
                                    }
                                    Glyph{text: "@CHEVRON_RIGHT@"}
                                }
                            }
                        }
                    }
                }}
            }
            page2 := View{visible: false width: Fill height: Fit flow: Down spacing: 8
                model_list := View{width: Fill height: Fit flow: Down spacing: 4 on_render: || {
                    GestureView{width: Fill height: Fit on_tap: |x, y| toggle("model")
                        Drop{
                            View{width: Fill height: Fit flow: Down spacing: 2
                                OptTitleOn{text: model_title()}
                                OptNote{text: model_note()}
                            }
                            if open == "model" { Glyph{text: "@CHEVRON_UP@"} } else { Glyph{text: "@CHEVRON_DOWN@"} }
                        }
                    }
                    if open == "model" {
                        for m in models {
                            GestureView{width: Fill height: Fit on_tap: |x, y| select_model(m)
                                Opt{
                                    View{width: Fill height: Fit flow: Down spacing: 2
                                        View{width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                            if !custom && model != nil && model.id == m.id { OptTitleOn{text: m.label} } else { OptTitle{text: m.label} }
                                            if m.default == true { Tag{TagText{text: "Recommended"}} }
                                        }
                                        OptNote{text: m.detail}
                                    }
                                    if !custom && model != nil && model.id == m.id { Tick{text: "@CHECK@"} }
                                }
                            }
                        }
                        for f in fetched {
                            GestureView{width: Fill height: Fit on_tap: |x, y| use_custom(f.id)
                                Opt{
                                    View{width: Fill height: Fit flow: Down spacing: 2
                                        OptTitle{text: f.label}
                                        OptNote{text: f.id + " · listed by the provider"}
                                    }
                                }
                            }
                        }
                        GestureView{width: Fill height: Fit on_tap: |x, y| use_custom("")
                            Opt{
                                View{width: Fill height: Fit flow: Down spacing: 2
                                    OptTitle{text: "Custom model ID…"}
                                    OptNote{text: "Any model the route serves"}
                                }
                                if custom { Tick{text: "@CHECK@"} }
                            }
                        }
                    }
                }}
                custom_box := View{visible: false width: Fill height: Fit flow: Down spacing: 4 padding: Inset{top: 4}
                    Section{text: "MODEL ID"}
                    custom_model := Field{empty_text: "e.g. deepseek-v4-pro" on_change: |text| changed()}
                }
                View{width: Fill height: Fit flow: Right align: Align{y: 0.5} padding: Inset{top: 4}
                    Link{text: "Fetch models from provider" on_click: || fetch_models()}
                }
                fetch_note := Caption{visible: false width: Fill}
            }
            page3 := View{visible: false width: Fill height: Fit flow: Down spacing: 8
                route_list := View{width: Fill height: Fit flow: Down spacing: 4 on_render: || {
                    GestureView{width: Fill height: Fit on_tap: |x, y| toggle("route")
                        Drop{
                            View{width: Fill height: Fit flow: Down spacing: 2
                                OptTitleOn{text: route_title()}
                                OptNote{text: route_note()}
                            }
                            if open == "route" { Glyph{text: "@CHEVRON_UP@"} } else { Glyph{text: "@CHEVRON_DOWN@"} }
                        }
                    }
                    if open == "route" {
                        for r in route_list() {
                            GestureView{width: Fill height: Fit on_tap: |x, y| select_route(r)
                                Opt{
                                    View{width: Fill height: Fit flow: Down spacing: 2
                                        OptTitle{text: r.label}
                                        OptNote{text: r.detail}
                                    }
                                    if route != nil && route.id == r.id { Tick{text: "@CHECK@"} }
                                }
                            }
                        }
                        GestureView{width: Fill height: Fit on_tap: |x, y| custom_route()
                            Opt{
                                View{width: Fill height: Fit flow: Down spacing: 2
                                    OptTitle{text: "Custom endpoint…"}
                                    OptNote{text: "Your own base URL, OpenAI- or Anthropic-compatible"}
                                }
                                if route != nil && route.id == "custom" { Tick{text: "@CHECK@"} }
                            }
                        }
                    }
                }}
                endpoint_box := View{visible: false width: Fill height: Fit flow: Down spacing: 6 padding: Inset{top: 6}
                    Section{text: "BASE URL"}
                    base_url := Field{text: "@BASE@" empty_text: "https://…/v1" on_change: |text| changed()}
                    Section{text: "API PROTOCOL · COMPATIBLE WITH" padding: Inset{top: 6}}
                    protocol_row := View{width: Fill height: Fit flow: Right spacing: 6 on_render: || {
                        if protocol == "anthropic" { Seg{text: "OpenAI" on_click: || set_protocol("openai")} } else { SegOn{text: "OpenAI" on_click: || set_protocol("openai")} }
                        if protocol == "anthropic" { SegOn{text: "Anthropic" on_click: || set_protocol("anthropic")} } else { Seg{text: "Anthropic" on_click: || set_protocol("anthropic")} }
                    }}
                }
            }
            page4 := View{visible: false width: Fill height: Fit flow: Down spacing: 8
                key := Field{empty_text: "Paste the key" is_password: true on_change: |text| changed() on_return: |text| next()}
                key_note := Caption{visible: false width: Fill}
            }
            page5 := View{visible: false width: Fill height: Fit flow: Down spacing: 14
                test_box := View{width: Fill height: Fit flow: Down spacing: 10 on_render: || {
                    if tested == "testing" { TestOff{text: "Testing…"} } else { TestButton{text: "Test connection" on_click: || test()} }
                    if tested == "ok" {
                        ResultOk{
                            Icon{text: "@CHECK@" draw_text.color: #x248a3d}
                            Label{width: Fill padding: 0 text: "Connected · " + result_ms + " ms" draw_text.color: #x248a3d draw_text.text_style: theme.font_bold{font_size: 14}}
                        }
                    }
                    if tested == "fail" || tested == "network" {
                        ResultBad{
                            View{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                                Icon{text: "@CROSS@" draw_text.color: #xff3b30}
                                Label{width: Fill padding: 0 text: "Couldn't connect: " + reason draw_text.color: #xc4281c draw_text.text_style: theme.font_bold{font_size: 14}}
                            }
                            Label{width: Fill padding: 0 text: result draw_text.color: #x6e2a24 draw_text.text_style.font_size: 12}
                            if tested == "network" {
                                Label{width: Fill padding: 0 text: "The provider could not be reached from here. You can save without testing and test it later from the list." draw_text.color: #x6e6e73 draw_text.text_style.font_size: 12}
                                Bypass{text: "Save without testing" on_click: || save()}
                            } else {
                                Label{width: Fill padding: 0 text: "Fix the key or the route (Back), then test again." draw_text.color: #x6e6e73 draw_text.text_style.font_size: 12}
                            }
                        }
                    }
                    if tested == "" { Caption{width: Fill text: "Save is available once the test passes."} }
                    if tested == "testing" { Caption{width: Fill text: "Sending one tiny request…"} }
                }}
                RoundedView{width: Fill height: Fit flow: Down spacing: 4 padding: Inset{left: 14 right: 14 top: 10 bottom: 10} show_bg: true draw_bg.color: #xf9f9fb draw_bg.border_radius: 12.0
                    View{width: Fill height: Fit flow: Right SumKey{text: "Family"} sum_family := SumValue{}}
                    View{width: Fill height: Fit flow: Right SumKey{text: "Model"} sum_model := SumValue{}}
                    View{width: Fill height: Fit flow: Right SumKey{text: "Route"} sum_route := SumValue{}}
                    View{width: Fill height: Fit flow: Right SumKey{text: "API key"} sum_key := SumValue{}}
                }
            }
        }
        footer := View{width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5} padding: Inset{top: 12} on_render: || {
            if step == 1 { Back{text: "Cancel" on_click: || cancel()} } else { Back{text: "Back" on_click: || back()} }
            View{width: Fill height: 1}
            if valid() { Next{text: next_text() on_click: || next()} } else { NextOff{text: next_text()} }
        }}
    }
}
"##;

/// Shown while the code is sealed (Argon2id takes a moment): it asks whether
/// the finished sheet is ready and then asks the service to swap it in.
///
/// A sheet swap re-runs the new program in the same isolate, and the timers
/// the old program armed keep firing into code that is gone (Splash logs
/// `pop_stack_resolved on empty stack` for each). So no sheet arms a
/// repeating timer: each poll is a one-shot re-armed from its answer, and the
/// swap itself (`llm.sheet.show`) is asked for without a callback, since the
/// answer would arrive after this program is replaced.
pub fn export_waiting() -> String {
    let mut script = format!(
        r##"fn poll(){{
    host.request("llm.sheet.qr", {{}}, fn(r){{
        if !r.is_ok {{ ui.status.set_text(r.error) return }}
        if r.data.ready == true {{ host.request("llm.sheet.show", {{}}, nil) return }}
        start_timeout(0.3, || poll())
    }})
}}
fn cancel(){{ host.request("llm.sheet.cancel", {{}}, nil) }}
start_timeout(0.1, || poll())
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
/// closes itself (the service closes it too, a moment later, should this
/// program stop). The countdown is a chain of one-shot timers under a name
/// other than `tick`: Splash calls a body's `fn tick()` once a second by
/// itself, which would count down twice as fast.
pub fn export(size: usize, modules: &[bool], pin: &str, labels: &[String], lifetime_secs: u64) -> String {
    let mut script = format!(
        r##"let left = {lifetime_secs}
let closing = false
fn close(){{
    if closing {{ return }}
    closing = true
    host.request("llm.sheet.cancel", {{}}, nil)
}}
fn count_down(){{
    if closing {{ return }}
    left = left - 1
    if left <= 0 {{ close() return }}
    let m = floor(left / 60)
    let s = left - m * 60
    if s < 10 {{ ui.countdown.set_text("Expires in " + m + ":0" + s) }} else {{ ui.countdown.set_text("Expires in " + m + ":" + s) }}
    start_timeout(1, || count_down())
}}
start_timeout(1, || count_down())
let Dark = SolidView{{height: Fill draw_bg.color: #x000000}}
let Light = View{{height: Fill}}
let QrRow = View{{width: Fit flow: Right}}
{STYLES}"##
    );
    let expires = format!("{}:{:02}", lifetime_secs / 60, lifetime_secs % 60);
    let content = format!(
        r#"        Title{{text: "OctoSense · Code for your phone"}}
        Note{{text: "On the phone, open AI providers and choose Scan QR from desktop, then type the PIN. The code carries your keys: close it when you are done."}}
        View{{width: Fill height: Fit flow: Down align: Align{{x: 0.5}}
{qr}        }}
        Caption{{text: "PIN"}}
        pin := Label{{width: Fill align: Align{{x: 0.5}} text: "{pin}" draw_text.color: ink draw_text.text_style: theme.font_bold{{font_size: 28}}}}
        countdown := Caption{{text: "Expires in {expires}"}}
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

/// Import a code: scan it (where the host has a scanner), read it out of an
/// image (chosen with the host's picker, or dropped on the app where the host
/// passes drops on), or paste it, then type its PIN. An import only adds
/// (the service keeps what is saved and appends the code's providers as
/// fallbacks), so there is nothing to confirm: the sheet shows what the
/// service did and closes.
pub fn import(can_scan: bool, can_pick: bool, can_drop: bool) -> String {
    let mut script = format!(
        r##"let can_scan = {can_scan}
let can_pick = {can_pick}
let can_drop = {can_drop}
fn scan(){{
    ui.status.set_text("")
    ui.note.set_text("Point the camera at the code on your computer…")
    host.request("llm.sheet.scan", {{}}, fn(r){{
        if r.is_ok {{
            if r.data.needs_pin == true {{ ui.note.set_text("Code scanned. Type the PIN shown beside it.") }} else {{ ui.note.set_text("Code scanned. Tap Import.") }}
        }} else {{ ui.note.set_text("") ui.status.set_text(r.error) }}
    }})
}}
fn read_image(r){{
    if !r.is_ok {{ ui.note.set_text("") ui.status.set_text(r.error) return }}
    if r.data.cancelled == true {{ ui.note.set_text("") return }}
    if r.data.error != nil {{ ui.note.set_text("") ui.status.set_text(r.data.error) return }}
    ui.status.set_text("")
    if r.data.needs_pin == true {{ ui.note.set_text("Code read from the image. Type the PIN shown beside it.") }} else {{ ui.note.set_text("Code read from the image. Tap Import.") }}
}}
fn pick(){{
    ui.status.set_text("")
    ui.note.set_text("Choose a screenshot or photo of the code…")
    host.request("llm.sheet.pick", {{}}, fn(r){{ read_image(r) }})
}}
fn await_drop(){{
    host.request("llm.sheet.image", {{}}, fn(r){{
        if r.is_ok {{
            read_image(r)
            await_drop()
        }}
    }})
}}
fn submit(){{
    ui.status.set_text("")
    ui.note.set_text("Checking the code…")
    host.request("llm.sheet.import", {{text: ui.code.text() pin: ui.pin.text()}}, fn(r){{
        if r.is_ok {{ ui.note.set_text(r.data.message) }} else {{ ui.note.set_text("") ui.status.set_text(r.error) }}
    }})
}}
fn cancel(){{ host.request("llm.sheet.cancel", {{}}, nil) }}
if can_scan {{ start_timeout(0.1, || scan()) }}
if can_drop {{ await_drop() }}
{STYLES}"##
    );
    let mut buttons = String::new();
    if can_scan {
        buttons.push_str("\n            Choice{text: \"Scan again\" on_click: || scan()}");
    }
    if can_pick {
        buttons.push_str("\n            pick_image := Choice{text: \"Choose image\" on_click: || pick()}");
    }
    let buttons = if buttons.is_empty() {
        String::new()
    } else {
        format!("\n        View{{width: Fill height: Fit flow: Right spacing: 6{buttons}\n        }}")
    };
    let image_note = match (can_pick, can_drop) {
        (_, true) => "\n        Caption{text: \"Or drop a screenshot of the code on this sheet.\"}",
        (true, false) => "\n        Caption{text: \"Choose image reads the code from a screenshot or photo.\"}",
        _ => "",
    };
    let paste = if can_scan || can_pick { "Or paste the code" } else { "Paste the code" };
    let content = format!(
        r#"        Title{{text: "OctoSense · Import providers"}}
        Note{{text: "Show the code on your computer: AI providers, Show QR for phone. Its providers are added after yours, as fallbacks; the keys it carries go to OctoSense, not to the app that asked."}}
        status := Status{{}}
        note := Note{{}}{buttons}{image_note}
        Caption{{text: "{paste} (OCTOS1E:…)"}}
        code := Field{{empty_text: "OCTOS1E:…"}}
        Caption{{text: "PIN"}}
        pin := Field{{empty_text: "XXXX-XXXX" is_password: true}}"#
    );
    script.push_str(&frame(
        r#"            Plain{text: "Cancel" on_click: || cancel()}
            View{width: Fill height: 1}
            import_btn := Primary{text: "Import" on_click: || submit()}"#,
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
    fn the_edit_sheet_prefills_the_route_and_takes_the_key_only_here() {
        let mut p = Provider::new("deepseek", Some("deepseek-v4-pro".into()));
        p.route_id = Some("autodl".into());
        let body = edit(Some(&p), true);
        assert!(body.contains("let want_family = \"deepseek\""));
        assert!(body.contains("let want_model = \"deepseek-v4-pro\""));
        assert!(body.contains("let want_route = \"autodl\""));
        assert!(body.contains("let editing = true"));
        assert!(body.contains("is_password: true"));
        assert!(body.contains("llm.sheet.submit") && body.contains("llm.sheet.test") && body.contains("llm.sheet.fetch_models"));
        assert!(!body.contains('@'), "every placeholder filled");
        let add = edit(None, false);
        assert!(add.contains("return \"Save as primary\"") && add.contains("let want_family = \"\""));
        assert!(edit(None, true).contains("return \"Add as fallback\""));
        for step in ["Step 1 of 5 · Choose model family", "Step 2 of 5 · Choose model", "Step 3 of 5 · Choose provider route", "Step 4 of 5 · API key", "Step 5 of 5 · Test connection & save"] {
            assert!(add.contains(step), "{step}");
        }
    }
}
