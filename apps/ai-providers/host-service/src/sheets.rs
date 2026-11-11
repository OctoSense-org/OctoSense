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

const STYLES: &str = r##"// A desktop sheet follows the appearance at Mac metrics.
let desk = try { theme.ui_pointer == true } catch { false }
let follow_touch_appearance = false
let dark_appearance = if desk || follow_touch_appearance { try { dot(mod.theme.color_text, #x36b61200) >= 0.5 } catch { false } } else { false }
fn by(touch, pointer){ if desk { pointer } else { touch } }
fn tone(touch, light, dark){
    if dark_appearance && (desk || follow_touch_appearance) { dark }
    else if desk { light } else { touch }
}
let sh_card = tone(#xffffff, #xf6f6f8, #x2c2c2e)
let sh_ink = tone(#x1c1c1e, #x1d1d1f, #xf5f5f7)
let sh_ink2 = tone(#x3a3a3c, #x3a3a3c, #xd1d1d6)
let sh_ink3 = tone(#x6e6e73, #x636368, #xa6a6ab)
let sh_muted = tone(#x8e8e93, #x636368, #xa6a6ab)
let sh_faint = tone(#xaeaeb2, #x8e8e93, #x8e8e93)
let sh_accent = tone(#x007aff, #x0071e3, #x0071e3)
let sh_accent_hover = tone(#x0a84ff, #x006edb, #x006edb)
let sh_accent_down = tone(#x0062cc, #x0060c0, #x0060c0)
let sh_link = tone(#x007aff, #x0066cc, #x2997ff)
let sh_field = tone(#xf2f2f7, #xffffff, #x3a3a3c)
let sh_field_line = tone(#x00000000, #xc9c9ce, #x545458)
let sh_line = tone(#xe5e5ea, #xe3e3e8, #x48484a)
let sh_line2 = tone(#xd1d1d6, #xd1d1d6, #x545458)
let sh_danger = tone(#xff3b30, #xd70015, #xff7b72)
let sh_ok = tone(#x248a3d, #x1f7f36, #x30d158)
let sh_wash = tone(#x0000000a, #x0000000a, #xffffff14)
let sh_wash2 = tone(#x00000014, #x00000014, #xffffff24)
// A sheet's dim, the shell's for every app on a desktop: 14% black in the light, 32% in the dark.
let sh_dim = if desk { tone(#x000000aa, #x00000024, #x00000052) } else { #x000000aa }
fn sh_fs(n){
    if !desk { return n }
    if n <= 11 { return 8.25 }
    if n <= 12 { return 9 }
    if n <= 16 { return 9.75 }
    if n <= 17 { return 11.25 }
    if n <= 20 { return 12.75 }
    16.5
}
fn sh_h(n){
    if !desk { return n }
    if n >= 50 { return 32 }
    if n >= 40 { return 28 }
    if n >= 32 { return 26 }
    n
}
let ink = sh_ink
let secondary = sh_muted
let accent = sh_accent
let Field = TextInput{width: Fill height: sh_h(40)
    draw_bg +: {color: sh_field color_hover: sh_field color_focus: sh_field color_empty: sh_field
        border_color: sh_field_line border_color_hover: sh_field_line border_color_focus: sh_accent border_color_empty: sh_field_line border_radius: by(10.0, 3.0)}
    draw_text +: {color: sh_ink color_hover: sh_ink color_focus: sh_ink color_empty: sh_muted color_empty_hover: sh_muted}
}
// A sheet has no icon files: a search field's magnifier draws itself.
let DeskSearchGlyph = View{width: 14 height: 14 show_bg: true draw_bg +: {color: sh_muted
    pixel: fn() {
        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
        sdf.circle(6.0, 6.0, 4.5)
        sdf.stroke(self.color, 1.2)
        sdf.move_to(9.3, 9.3)
        sdf.line_to(12.8, 12.8)
        return sdf.stroke(self.color, 1.3)
    }
}}
// On a desktop a sheet's buttons are a Mac's: a push face, the default in blue, and
// what can't be pressed yet dimmed on that same face.
let DeskPush = ButtonFlat{height: 28 margin: 0 padding: Inset{left: 12 right: 12}
    draw_bg +: {border_radius: 3.0 border_size: 1.0 border_color: sh_field_line color: tone(#xffffff, #xffffff, #x48484a) color_focus: tone(#xffffff, #xffffff, #x48484a) color_hover: tone(#xf5f5f7, #xf5f5f7, #x545458) color_down: tone(#xe8e8ed, #xe8e8ed, #x636366)}
    draw_text +: {color: sh_ink color_focus: sh_ink color_hover: sh_ink color_down: sh_ink text_style: theme.font_regular{font_size: 9.75}}}
let DeskDefault = DeskPush{draw_bg +: {border_size: 0.0 color: sh_accent color_focus: sh_accent color_hover: sh_accent_hover color_down: sh_accent_down}
    draw_text +: {color: #xffffff color_focus: #xffffff color_hover: #xffffff color_down: #xffffff}}
let DeskOff = DeskPush{enabled: false draw_bg +: {color_hover: tone(#xffffff, #xffffff, #x48484a) color_down: tone(#xffffff, #xffffff, #x48484a)}
    draw_text +: {color: sh_muted color_focus: sh_muted color_hover: sh_muted color_down: sh_muted}}
// A form's label, right-aligned to the left of its control.
let DeskKey = Label{clip_y: false padding: 0 width: 90 align: Align{x: 1.0} text: "" draw_text.color: sh_ink draw_text.text_style: theme.font_regular{font_size: 9.75}}
let Caption = if desk { Label{clip_y: false padding: 0 width: Fill height: Fit flow: Flow.Right{wrap: true} text: "" draw_text.color: sh_muted draw_text.text_style: theme.font_regular{font_size: 8.25 line_spacing: 1.3}} } else { Label{width: Fill height: Fit flow: Right{wrap: true} text: "" draw_text.color: sh_muted draw_text.text_style.font_size: sh_fs(11)} }
let Note = if desk { Label{clip_y: false padding: 0 width: Fill height: Fit flow: Flow.Right{wrap: true} text: "" draw_text.color: sh_ink3 draw_text.text_style: theme.font_regular{font_size: 9 line_spacing: 1.3}} } else { Label{width: Fill text: "" draw_text.color: sh_ink2 draw_text.text_style.font_size: sh_fs(12)} }
let Choice = if desk { DeskPush{} } else { ButtonFlat{height: sh_h(32) width: Fill
    draw_bg +: {border_radius: by(8.0, 3.0) color: sh_field color_focus: sh_field color_hover: sh_line color_down: sh_line2 border_size: 0.0}
    draw_text +: {color: sh_ink2 color_focus: sh_ink2 color_hover: sh_ink2 color_down: sh_ink2 text_style +: {font_size: sh_fs(13)}}} }
let Plain = if desk { DeskPush{} } else { ButtonFlat{height: sh_h(40)
    draw_bg +: {color: #x00000000 color_focus: #x00000000 color_hover: sh_wash color_down: sh_wash2 border_size: 0.0}
    draw_text +: {color: sh_link color_focus: sh_link color_hover: sh_link color_down: sh_link text_style +: {font_size: sh_fs(15)}}} }
let Primary = if desk { DeskDefault{} } else { ButtonFlat{height: sh_h(40) padding: Inset{left: 20 right: 20}
    draw_bg +: {border_radius: by(20.0, 3.0) color: sh_accent color_focus: sh_accent color_hover: sh_accent_hover color_down: sh_accent_down border_size: 0.0}
    draw_text +: {color: #xffffff color_focus: #xffffff color_hover: #xffffff color_down: #xffffff text_style +: {font_size: sh_fs(15)}}} }
let Title = if desk { Label{clip_y: false padding: 0 width: Fill text: "" draw_text.color: sh_ink draw_text.text_style: theme.font_bold{font_size: 9.75}} } else { Label{width: Fill text: "" draw_text.color: sh_ink draw_text.text_style: theme.font_bold{font_size: sh_fs(17)}} }
// The Talk sheet's way in: blue on a phone; on a Mac the sheet's default is Done, at its corner.
let BodyMain = if desk { DeskPush{} } else { Primary{} }
let FootMain = if desk { DeskDefault{} } else { Plain{} }
let Status = if desk { Label{clip_y: false padding: 0 width: Fill height: Fit flow: Flow.Right{wrap: true} text: "" draw_text.color: sh_danger draw_text.text_style: theme.font_regular{font_size: 9 line_spacing: 1.3}} } else { Label{width: Fill text: "" draw_text.color: sh_danger draw_text.text_style.font_size: sh_fs(12)} }
"##;

/// A card over a dimmed backdrop, scrolling. On a phone `actions` are at the
/// top, so the keyboard never hides them; on a desktop the card is a dialog
/// with its actions at the bottom right, as a Mac's sheets have them.
fn frame(actions: &str, content: &str) -> String {
    let desk_actions = actions.replace("View{width: Fill height: 1}", "");
    format!(
        r##"let SheetTouch = SolidView{{width: Fill height: Fill flow: Down draw_bg.color: sh_dim new_batch: true
    ScrollYView{{width: Fill height: Fill flow: Down padding: Inset{{left: 12 right: 12 top: 24 bottom: 24}}
    RoundedView{{width: Fill height: Fit flow: Down spacing: 8 padding: 16 new_batch: true show_bg: true draw_bg.color: sh_card draw_bg.border_radius: 18.0
        View{{width: Fill height: Fit flow: Right spacing: 8 align: Align{{y: 0.5}}
{actions}
        }}
{content}
    }}
    }}
}}
let SheetDesk = SolidView{{width: Fill height: Fill flow: Down draw_bg.color: sh_dim new_batch: true
    ScrollYView{{width: Fill height: Fill flow: Down align: Align{{x: 0.5}} padding: Inset{{left: 12 right: 12 top: 53 bottom: 24}}
    RoundedView{{width: Fill max_width: 480 height: Fit flow: Down spacing: 8 padding: 18 show_bg: true draw_bg +: {{color: sh_card border_radius: 5.0 border_size: 1.0 border_color: sh_line}}
{content}
        View{{width: Fill height: Fit flow: Right spacing: 8 align: Align{{x: 1.0 y: 0.5}} margin: Inset{{top: 6}}
{desk_actions}
        }}
    }}
    }}
}}
if desk {{ SheetDesk{{}} }} else {{ SheetTouch{{}} }}
"##
    )
}

/// Talk to Octos, on the trusted sheet: off by default. Turning it on starts
/// the kernel's loopback server for external clients and mints their token;
/// a web client pairs with a one-time code (`pair_client`). No token is ever
/// shown, copied or sent to this script.
pub fn connect_client() -> String {
    let mut script = String::from(r#"
// On a desktop these words take room when they come, and only a failure is red.
fn status(text){
    ui.status.set_text(text)
    if desk {
        ui.status.set_visible(text != "")
        ui.fail_note.set_visible(false)
    }
}
fn failed(text){
    if desk {
        status("")
        ui.fail_note.set_text(text)
        ui.fail_note.set_visible(true)
    } else {
        status(text)
    }
}
let talk_on = false
fn show(d){
    talk_on = d.enabled == true
    ui.on_box.set_visible(d.enabled == true)
    if desk {
        ui.talk_row.render()
        // A view drawn by on_render is empty until asked: the Advanced row with it.
        ui.adv_row.render()
        ui.off_note.set_visible(d.enabled == false)
    } else {
        if d.enabled { ui.state.set_text("On") } else { ui.state.set_text("Off") }
        ui.turn_on.set_visible(d.enabled == false)
    }
    ui.origin.set_text(d.origin)
    ui.endpoint.set_text(d.endpoint)
    ui.descriptor.set_text(d.descriptor)
    ui.session.set_text(d.session_id)
    ui.web_origin.set_text(d.web_origin)
}
fn answer(r){
    if !r.is_ok {
        failed(r.error)
        // A switch that couldn't change goes back.
        if desk { ui.talk_row.render() }
        return
    }
    show(r.data)
    status("")
}
fn refresh(){
    if !desk { status("Checking…") }
    host.request("llm.sheet.client_info", {}, fn(r){ answer(r) })
}
fn enable(on){
    if on { status(by("Starting the Talk to Octos server…", "Turning on web clients…")) } else { status(by("Stopping external access…", "Turning off web clients…")) }
    host.request("llm.sheet.client_enable", {on: on}, fn(r){ answer(r) })
}
fn rotate(){
    status("Revoking…")
    host.request("llm.sheet.client_rotate", {}, fn(r){
        answer(r)
        if r.is_ok { status("Every paired client must pair again.") }
    })
}
fn save_origin(){ status("Saving…") host.request("llm.sheet.client_origin", {origin: ui.web_origin.text()}, fn(r){ answer(r) }) }
fn open_web(){ host.request("llm.sheet.client_open", {}, fn(r){ if !r.is_ok { failed(r.error) } }) }
fn poll_pair(){
    host.request("llm.sheet.client_pair_ready", {}, fn(r){
        if !r.is_ok { failed(r.error) return }
        if r.data.ready == true { host.request("llm.sheet.client_pair_show", {}, nil) return }
        start_timeout(0.3, || poll_pair())
    })
}
fn pair(){ status("Preparing a pairing code…") host.request("llm.sheet.client_pair", {}, fn(r){ if r.is_ok { poll_pair() } else { failed(r.error) } }) }
fn close(){ host.request("llm.sheet.cancel", {}, nil) }
// A desktop's Advanced disclosure: what a terminal client needs, out of the way.
let details_open = false
fn toggle_details(){
    details_open = !details_open
    ui.details_box.set_visible(details_open)
    ui.adv_row.render()
}
// The host's Back (phone) calls cancel(): closing ends any pairing code.
fn cancel(){ close() }
start_timeout(0.1, || refresh())
"#);
    script.push_str(STYLES);
    script.push_str(r#"// A desktop's form: its labels to the left of what they name, its notes under the controls.
let TalkRow = View{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}}
let TalkKey = DeskKey{width: 116}
let TalkNote = View{width: Fill height: Fit padding: Inset{left: 124}}
// A disclosure's chevron: right while shut, down while open (a sheet has no icon files).
let DiscloseShut = View{width: 12 height: 12 show_bg: true draw_bg +: {color: sh_ink3
    pixel: fn() {
        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
        sdf.move_to(4.5, 2.5)
        sdf.line_to(8.0, 6.0)
        sdf.line_to(4.5, 9.5)
        return sdf.stroke(self.color, 1.4)
    }
}}
let DiscloseOpen = View{width: 12 height: 12 show_bg: true draw_bg +: {color: sh_ink3
    pixel: fn() {
        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
        sdf.move_to(2.5, 4.5)
        sdf.line_to(6.0, 8.0)
        sdf.line_to(9.5, 4.5)
        return sdf.stroke(self.color, 1.4)
    }
}}
let DeskSwitch = Toggle{text: "" padding: 0 margin: 0 width: 28 height: 18
    draw_bg +: {size: 16.0 border_size: 0.0 color: sh_line2 color_hover: sh_line2 color_down: sh_line2 color_focus: sh_line2 color_active: sh_accent
        mark_color: #xffffff mark_color_hover: #xffffff mark_color_active: #xffffff mark_color_active_hover: #xffffff
        pixel: fn(){
            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
            let h = self.size
            let w = h * 1.625
            let top = (self.rect_size.y - h) * 0.5
            sdf.box(0.0, top, w, h, h * 0.25)
            sdf.fill(self.color.mix(self.color_active, self.active))
            sdf.circle(h * 0.5 + (w - h) * self.active, top + h * 0.5, h * 0.5 - 1.5)
            sdf.fill(self.mark_color)
            return sdf.result
        }
    }
}
"#);
    script.push_str(&frame(
        r#"FootMain{text: "Done" on_click: || close()}
           View{width: Fill height: 1}"#,
        r#"if desk {
        Title{text: "Web Clients"}
        TalkRow{
            Label{clip_y: false padding: 0 width: Fill text: "Web Clients and Terminals" draw_text.color: sh_ink draw_text.text_style: theme.font_regular{font_size: 9.75}}
            talk_row := View{width: Fit height: Fit on_render: || {
                talk_switch := DeskSwitch{active: talk_on on_click: |active| enable(active)}
            }}
        }
        Caption{text: "They can talk to your assistant from this device. Turning this on or off restarts the assistant, and work in progress stops."}
        off_note := View{width: Fill height: Fit
            Caption{text: "A paired client talks only in the system conversation. It can't change models, keys or skills, run commands or stop the assistant."}
        }
        on_box := View{visible: false width: Fill height: Fit flow: Down spacing: 8
            TalkRow{
                TalkKey{text: "Web Client:"}
                web_origin := Field{empty_text: "https://web.example"}
                DeskPush{text: "Save" on_click: || save_origin()}
            }
            TalkNote{Caption{text: "https, or http only for localhost. Saving a new address restarts the assistant."}}
            TalkRow{
                View{width: 116 height: 1}
                DeskPush{text: "Pair a Web Client…" on_click: || pair()}
                DeskPush{text: "Open Web Client" on_click: || open_web()}
            }
            TalkRow{
                TalkKey{text: "Paired Clients:"}
                DeskPush{text: "Revoke All Clients" on_click: || rotate()}
            }
            TalkNote{Caption{text: "Revoking restarts the assistant. Every client pairs again."}}
            // Advanced: the server and the address a terminal client connects to.
            adv_row := View{width: Fill height: Fit padding: Inset{top: 4} on_render: || {
                GestureView{width: Fit height: 24 flow: Right spacing: 4 align: Align{y: 0.5} cursor: MouseCursor.Default on_tap: |x, y| toggle_details()
                    if details_open { DiscloseOpen{} } else { DiscloseShut{} }
                    Label{clip_y: false padding: 0 text: "Advanced" draw_text.color: sh_ink draw_text.text_style: theme.font_regular{font_size: 9.75}}
                }
            }}
            details_box := View{visible: false width: Fill height: Fit flow: Down spacing: 8
                TalkRow{TalkKey{text: "Server:"} origin := Field{is_read_only: true}}
                TalkRow{TalkKey{text: "Terminal Address:"} endpoint := Field{is_read_only: true}}
                TalkNote{Caption{text: "A terminal client on this device finds these by itself."}}
            }
            // The connection file is the terminal client's to read, and it names the
            // conversation: a person has no use for its path or the conversation's id.
            View{visible: false descriptor := Field{is_read_only: true} session := Field{is_read_only: true}}
        }
        status := Caption{visible: false text: ""}
        fail_note := Status{visible: false}
    } else {
        Title{text: "Talk to Octos"}
        Note{text: "Let a web client or a terminal on this device talk to your assistant. Off by default; only this device's apps use it then."}
        View{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
            if desk { Label{clip_y: false padding: 0 text: "Status:" draw_text.color: sh_ink draw_text.text_style: theme.font_regular{font_size: 9.75}} } else { Caption{text: "Talk to Octos"} }
            state := Label{text: "" draw_text.color: sh_ink draw_text.text_style: theme.font_bold{font_size: sh_fs(13)}}
        }
        turn_on := BodyMain{text: by("Turn on", "Turn On") on_click: || enable(true)}
        Note{text: "Turning on restarts the assistant: work in progress, in apps too, stops."}
        on_box := View{visible: false width: Fill height: Fit flow: Down spacing: 8
            BodyMain{text: by("Pair a web client", "Pair a Web Client…") on_click: || pair()}
            Note{text: "Shows a one-time code and a QR of the web link. A code works once, for five minutes, only while its sheet is open."}
            Caption{text: "Web client origin · https, or http only for localhost"}
            web_origin := Field{empty_text: "https://web.example"}
            Plain{text: by("Save origin", "Save Origin") on_click: || save_origin()}
            Note{text: "Saving a changed origin restarts the assistant: work in progress, in apps too, stops."}
            Plain{text: by("Open web client", "Open Web Client") on_click: || open_web()}
            Caption{text: "Server · the web client pairs with it"}
            origin := Field{is_read_only: true}
            Caption{text: "WebSocket endpoint · for a terminal client"}
            endpoint := Field{is_read_only: true}
            Caption{text: "Connection file · a terminal client of this user reads it"}
            descriptor := Field{is_read_only: true}
            Caption{text: by("System-agent conversation · profile _main", "The assistant's conversation")}
            session := Field{is_read_only: true}
            Plain{text: by("Revoke all clients", "Revoke All Clients") on_click: || rotate()}
            Note{text: "Revoking disconnects every client and makes each pair again. It restarts the assistant, and Turn off does too: work in progress, in apps too, stops."}
            Plain{text: by("Turn off", "Turn Off") on_click: || enable(false)}
        }
        Note{text: "A paired client talks with the assistant in the system conversation and answers only its own requests' questions. It cannot see or drive the apps' assistants, touch this device's turns, change models, keys or skills, run commands or git, or stop the assistant. A computer reaches this device through a tunnel that keeps the port number."}
        status := Status{}
    }"#,
    ));
    script
}

/// The pairing code's sheet: the code in large type and, when a web origin
/// is saved, a QR of the web client's pairing link. It counts down and ends
/// at zero; leaving it (Back, Done) turns pairing off.
pub fn pair_client(code: &str, server: &str, qr: Option<(usize, &[bool])>, lifetime_secs: u64) -> String {
    let mut script = format!(
        r##"let left = {lifetime_secs}
let closing = false
fn back(){{
    if closing {{ return }}
    closing = true
    host.request("llm.sheet.client_back", {{}}, nil)
}}
fn say(text){{
    ui.status.set_text(text)
    ui.status.set_visible(true)
}}
fn again(){{
    if closing {{ return }}
    closing = true
    host.request("llm.sheet.client_pair", {{}}, fn(r){{ if r.is_ok {{ poll() }} else {{ say(r.error) }} }})
}}
fn poll(){{
    host.request("llm.sheet.client_pair_ready", {{}}, fn(r){{
        if !r.is_ok {{ say(r.error) return }}
        if r.data.ready == true {{ host.request("llm.sheet.client_pair_show", {{}}, nil) return }}
        start_timeout(0.3, || poll())
    }})
}}
fn close(){{ host.request("llm.sheet.cancel", {{}}, nil) }}
fn cancel(){{ close() }}
fn count_down(){{
    if closing {{ return }}
    left = left - 1
    if left <= 0 {{ back() return }}
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
    let qr = match qr {
        Some((size, modules)) => format!(
            "        Note{{text: \"Scan it with the device where the web client runs, or open the web client and type the code.\"}}\n        View{{width: Fill height: Fit flow: Down align: Align{{x: 0.5}}\n{}        }}\n",
            qr_views(size, modules, module_px(size))
        ),
        None => "        Note{text: by(\"Save the web client's origin on the previous page to get a QR of its link.\", \"Save the web client's address in Web Clients to get a QR of its link.\")}\n".to_string(),
    };
    let content = format!(
        r#"        Title{{text: by("Pair a web client", "Pair a Web Client")}}
        Note{{text: "Open the web client, enter this device's server and type the code. The code gives that client access once."}}
{qr}        Caption{{align: Align{{x: by(0.0, 0.5)}} text: "Code"}}
        code := Label{{width: Fill align: Align{{x: 0.5}} text: "{code}" draw_text.color: ink draw_text.text_style: theme.font_bold{{font_size: sh_fs(30)}}}}
        countdown := Caption{{align: Align{{x: by(0.0, 0.5)}} text: "Expires in {expires}"}}
        if desk {{ View{{width: Fill height: Fit flow: Right spacing: 8 align: Align{{y: 0.5}}
            DeskKey{{width: Fit text: "Server:"}}
            Field{{is_read_only: true text: "{server}"}}
        }} }} else {{
            Caption{{text: "Server"}}
            Note{{text: "{server}"}}
        }}
        status := Status{{visible: !desk}}"#,
        code = lit(code),
        server = lit(server),
    );
    script.push_str(&frame(
        r#"            Plain{text: "Back" on_click: || back()}
            if desk { View{width: Fill height: 0} }
            View{width: Fill height: 1}
            Plain{text: by("New code", "New Code") on_click: || again()}
            Primary{text: "Done" on_click: || close()}"#,
        &content,
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
    // Other host sheets keep their existing phone palette; the add wizard
    // follows the phone's appearance in both its light and dark states.
    let wizard_styles = STYLES.replacen("let follow_touch_appearance = false", "let follow_touch_appearance = true", 1);
    ADD_SHEET
        .replace("@STYLES@", &wizard_styles)
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
/// handles too (scripts/ai_providers_remote.py in the desktop shell).
const PICKERS: &str = r##"let Drop = RoundedView{width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 10
    padding: Inset{left: 14 right: 16 top: 10 bottom: 10} show_bg: true draw_bg.color: tone(#xedf2fc, #xffffff, #x3a3a3c) draw_bg.border_radius: by(12.0, 3.0) draw_bg.border_size: by(0.0, 1.0) draw_bg.border_color: sh_field_line}
let Opt = RoundedView{width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 10
    padding: Inset{left: 14 right: 16 top: 8 bottom: 8} show_bg: true draw_bg.color: tone(#xf6f8fc, #xffffff, #x3a3a3c) draw_bg.border_radius: by(10.0, 3.0)}
let OptOn = Opt{draw_bg.color: tone(#xe8f0fe, #xdce9fb, #x1f3b5c)}
let Glyph = Label{text: "" draw_text.color: sh_muted draw_text.text_style: theme.font_icons{font_size: sh_fs(10)}}
let Tick = Label{text: "" draw_text.color: sh_link draw_text.text_style: theme.font_icons{font_size: sh_fs(12)}}
let Tag = RoundedView{width: Fit height: Fit padding: Inset{left: 6 right: 6 top: 2 bottom: 2} show_bg: true draw_bg.color: tone(#xe2f4e6, #xdff3e4, #x1f3a27) draw_bg.border_radius: by(6.0, 3.0)}
let TagText = Label{padding: 0 text: "" draw_text.color: tone(#x1f7a3a, #x1f7a3a, #x5fd38a) draw_text.text_style: theme.font_bold{font_size: sh_fs(10)}}
let OptTitle = Label{padding: 0 width: Fit text: "" draw_text.color: sh_ink draw_text.text_style.font_size: sh_fs(14)}
let OptTitleOn = Label{padding: 0 width: Fit text: "" draw_text.color: sh_ink draw_text.text_style: if desk { theme.font_regular{font_size: 9.75} } else { theme.font_bold{font_size: sh_fs(14)} }}
let OptNote = Label{padding: 0 width: Fill text: "" draw_text.color: sh_muted draw_text.text_style.font_size: sh_fs(11)}
"##;

const ADD_SHEET: &str = r##"let editing = @EDITING@
let edit_opening = editing
let want_family = "@FAMILY@"
let want_model = "@MODEL@"
let want_route = "@ROUTE@"
let families = []
let families_request = 0
let fam = nil
let models = []
let model_requests = 0
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
let save_issue = ""
// A desktop edits a saved model on one form; adding one stays an assistant.
let edit_form = false
let save_after_test = false
let wizard_run = try { mod.provider_wizard_run } catch { 0 }
if wizard_run == nil { wizard_run = 0 }
wizard_run = wizard_run + 1
mod.provider_wizard_run = wizard_run

fn wizard_current(){
    if desk { return true }
    let current = try { mod.provider_wizard_run } catch { nil }
    current == wizard_run
}
// The host sheet keeps this state only in its own isolate. The key and other
// typed fields stay in their existing widgets and never enter this snapshot.
fn remember_touch(){
    if desk || !wizard_current() { return }
    mod.provider_touch_wizard = {editing: editing want_family: want_family want_model: want_model want_route: want_route
        families: families fam: fam models: models family_routes: family_routes fetched: fetched model: model
        custom: custom route: route protocol: protocol step: step tested: tested result: result reason: reason result_ms: result_ms
        busy: busy save_issue: save_issue}
}

fn show(label, text){
    label.set_text(text)
    label.set_visible(text != "")
}
fn load_families(q){
    families_request = families_request + 1
    let request = families_request
    host.request("llm.families", {query: q}, fn(r){
        if !wizard_current() || request != families_request { return }
        if !r.is_ok { show(ui.status, r.error) return }
        show(ui.status, "")
        families = r.data
        remember_touch()
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
    heading_touch()
    desk_heading()
}
fn heading_touch(){
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
// The preselected model may be far down a long list.
fn bring_model_into_view(){
    let row = 0
    let at = 0
    for m in models {
        if !custom && model != nil && model.id == m.id { at = row }
        row = row + 1
    }
    if at > 3 { ui.model_scroll.set_scroll_y((at - 2) * 48) } else { ui.model_scroll.set_scroll_y(0.0) }
}
fn go(n){
    step = n
    open = ""
    save_issue = ""
    if desk { ui.wizard_scroll.set_scroll_y(0.0) }
    show(ui.status, "")
    ui.page1.set_visible(n == 1 && !(desk && editing))
    ui.page2.set_visible(n == 2 || edit_form)
    ui.page3.set_visible(n == 3 || edit_form)
    ui.page4.set_visible(n == 4 || (edit_form && fam.has_key))
    ui.page5.set_visible(n == 5)
    heading()
    ui.progress.render()
    // The touch header's Cancel is the only way out after step 1.
    if !desk { ui.header_action.render() }
    if n == 2 {
        ui.model_list.render()
        if by(false, true) { bring_model_into_view() }
    }
    if n == 3 { ui.route_list.render() }
    if n == 5 { ui.test_box.render() }
    ui.footer.render()
    remember_touch()
}
fn valid(){
    if busy { return false }
    if edit_form {
        if tested == "testing" || route == nil { return false }
        if custom && ui.custom_model.text().trim() == "" { return false }
        return route.id != "custom" || ui.base_url.text().trim() != ""
    }
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
fn next_title(){
    let t = next_text()
    if t == "Next" { return by(t, "Continue") }
    if t == "Save as primary" { return by(t, "Save as Primary") }
    if t == "Add as fallback" { return by(t, "Add as Fallback") }
    return t
}
fn next(){
    if !valid() { return }
    // Save tests what hasn't been tested, and saves if it passes.
    if edit_form {
        if tested == "ok" { save() return }
        save_after_test = true
        test()
        return
    }
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
    if busy { return }
    if step == 5 && !fam.has_key { go(3) return }
    if step > 1 { go(step - 1) }
}
fn changed(){
    tested = ""
    ui.footer.render()
    form_sync()
    remember_touch()
}
// The form's pop-ups say the choice; everything else about it is a menu away.
fn form_open(){
    edit_form = true
    ui.provider_row.set_visible(true)
    ui.form_provider.set_text(fam.label)
    ui.model_frame.set_visible(false)
    ui.model_row.set_visible(true)
    ui.route_frame.set_visible(false)
    ui.route_row.set_visible(true)
    ui.key_label.set_visible(true)
    ui.key_gap.set_visible(true)
    ui.summary.set_visible(false)
    go(5)
    form_sync()
    if fam.has_key { ui.key.focus() }
}
fn form_sync(){
    if !edit_form { return }
    ui.model_pop.set_text(model_title())
    ui.form_model_note.set_text(model_note())
    ui.route_pop.set_text(route_title())
    ui.form_route_note.set_text(route_note())
    ui.test_box.render()
}
fn model_menu_text(m){
    if m.default == true { return m.label + " (Recommended)" }
    m.label
}
fn model_menu(){
    let titles = []
    let current = ""
    for m in models { titles.push(model_menu_text(m)) }
    for f in fetched { titles.push(f.id) }
    titles.push("-")
    titles.push("Custom Model ID…")
    titles.push("Fetch Models from Provider…")
    if custom { current = "Custom Model ID…" } else if model != nil { current = model_menu_text(model) }
    ui.model_pick.open_menu(titles, current)
}
fn model_chosen(title){
    if title == "Custom Model ID…" { use_custom("") return }
    if title == "Fetch Models from Provider…" { fetch_models() return }
    for m in models { if model_menu_text(m) == title { select_model(m) return } }
    for f in fetched { if f.id == title { use_custom(f.id) return } }
}
fn route_menu(){
    let titles = []
    let current = ""
    for r in route_list() { titles.push(r.label) }
    titles.push("-")
    titles.push("Custom Endpoint…")
    if route != nil { current = if route.id == "custom" { "Custom Endpoint…" } else { route.label } }
    ui.route_pick.open_menu(titles, current)
}
fn route_chosen(title){
    if title == "Custom Endpoint…" { custom_route() return }
    for r in route_list() { if r.label == title { select_route(r) return } }
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
    remember_touch()
    model_requests = model_requests + 1
    let request = model_requests
    host.request("llm.models", {family: f.id}, fn(r){
        if !wizard_current() || request != model_requests || fam == nil || fam.id != f.id { return }
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
        if desk && edit_opening {
            edit_opening = false
            keep_route()
            form_open()
        }
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
    form_sync()
    remember_touch()
}
fn use_custom(id){
    custom = true
    open = ""
    tested = ""
    ui.custom_box.set_visible(true)
    if id != "" { ui.custom_model.set_text(id) }
    ui.model_list.render()
    ui.footer.render()
    form_sync()
    remember_touch()
}
fn select_route(r){
    route = r
    open = ""
    tested = ""
    ui.endpoint_box.set_visible(r.id == "custom")
    ui.protocol_row.render()
    ui.route_list.render()
    ui.footer.render()
    form_sync()
    remember_touch()
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
    ui.footer.render()
    form_sync()
    remember_touch()
}
// A pull-down's button shows the choice while closed; open, the list below
// shows every option once (the chosen one ticked) and the button asks.
fn model_title(){
    if open == "model" { return by("Choose a model", "Choose a Model") }
    if custom { return by("Custom model ID", "Custom Model ID") }
    if model == nil { return "Loading models…" }
    return model.label
}
fn model_note(){
    if open == "model" { return fam.label + " · " + fam.models_text + " in the catalog" }
    if custom { return by("Type the model's id below", "Type the model's ID below") }
    if model == nil { return "" }
    return by(model.id + " · " + model.detail, model.detail)
}
fn route_title(){
    if open == "route" { return by("Choose a route", "Choose a Route") }
    if route == nil { return "Official API" }
    if route.id == "custom" { return by(route.label, "Custom Endpoint") }
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
fn apply_test_result(r){
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
    remember_touch()
    if save_after_test {
        save_after_test = false
        if tested == "ok" { save() }
    }
}
// Re-evaluating the sheet for a phone appearance change creates a new run.
// The old callback leaves only its result here; the current run consumes it.
fn follow_touch_test(id){
    if !wizard_current() || tested != "testing" { return }
    let pending = try { mod.provider_touch_test } catch { nil }
    if pending == nil || pending.id != id { return }
    if pending.response == nil { start_timeout(0.25, || follow_touch_test(id)) return }
    mod.provider_touch_test = nil
    apply_test_result(pending.response)
}
fn test(){
    if busy || tested == "testing" { return }
    tested = "testing"
    ui.test_box.render()
    ui.footer.render()
    if !desk {
        let id = try { mod.provider_touch_test_serial } catch { 0 }
        if id == nil { id = 0 }
        id = id + 1
        mod.provider_touch_test_serial = id
        mod.provider_touch_test = {id: id response: nil}
        remember_touch()
        host.request("llm.sheet.test", form(false), fn(r){
            let pending = try { mod.provider_touch_test } catch { nil }
            if pending == nil || pending.id != id { return }
            mod.provider_touch_test = {id: id response: r}
            if wizard_current() { follow_touch_test(id) }
        })
        return
    }
    host.request("llm.sheet.test", form(false), fn(r){ apply_test_result(r) })
}
fn fetch_models(){
    show(ui.fetch_note, "Asking the provider for its models…")
    host.request("llm.sheet.fetch_models", form(false), fn(r){
        if !r.is_ok { show(ui.fetch_note, r.error) return }
        fetched = r.data.models
        open = "model"
        show(ui.fetch_note, r.data.message)
        ui.model_list.render()
        remember_touch()
    })
}
fn save_message(error){
    if error.search("invalid: parse ") == 0 || error.search(" is not a JSON object") >= 0 || error.search("i/o:") == 0 {
        return "Couldn't save the model because the saved provider settings are unavailable. Try again."
    }
    error
}
fn save_controls(){
    if !desk { ui.header_action.render() }
    ui.footer.render()
}
fn apply_save_result(r){
    busy = false
    save_controls()
    if !r.is_ok {
        save_issue = save_message(r.error)
        show(ui.status, save_issue)
        remember_touch()
        return
    }
    if !desk { mod.provider_touch_wizard = nil }
}
fn follow_touch_save(id){
    if !wizard_current() || !busy { return }
    let pending = try { mod.provider_touch_save } catch { nil }
    if pending == nil || pending.id != id { return }
    if pending.response == nil { start_timeout(0.25, || follow_touch_save(id)) return }
    mod.provider_touch_save = nil
    apply_save_result(pending.response)
}
fn save(){
    if busy || fam == nil { return }
    busy = true
    save_issue = ""
    show(ui.status, "")
    save_controls()
    if !desk {
        let id = try { mod.provider_touch_save_serial } catch { 0 }
        if id == nil { id = 0 }
        id = id + 1
        mod.provider_touch_save_serial = id
        mod.provider_touch_save = {id: id response: nil}
        remember_touch()
        host.request("llm.sheet.submit", form(false), fn(r){
            let pending = try { mod.provider_touch_save } catch { nil }
            if pending == nil || pending.id != id { return }
            mod.provider_touch_save = {id: id response: r}
            if wizard_current() { follow_touch_save(id) }
        })
        return
    }
    host.request("llm.sheet.submit", form(false), fn(r){ apply_save_result(r) })
}
fn cancel(){
    if busy { return }
    if !desk { mod.provider_touch_wizard = nil mod.provider_touch_test = nil mod.provider_touch_save = nil }
    host.request("llm.sheet.cancel", {}, nil)
}
fn restore_touch(){
    if desk { return false }
    let saved = try { mod.provider_touch_wizard } catch { nil }
    if saved == nil || saved.editing != editing { return false }
    if saved.step > 1 && saved.fam == nil { return false }
    want_family = saved.want_family
    want_model = saved.want_model
    want_route = saved.want_route
    families = saved.families
    fam = saved.fam
    models = saved.models
    family_routes = saved.family_routes
    fetched = saved.fetched
    model = saved.model
    custom = saved.custom
    route = saved.route
    protocol = saved.protocol
    step = saved.step
    tested = saved.tested
    result = saved.result
    reason = saved.reason
    result_ms = saved.result_ms
    busy = saved.busy
    let saved_issue = saved.save_issue
    ui.custom_box.set_visible(custom)
    ui.endpoint_box.set_visible(route != nil && route.id == "custom")
    if fam != nil { show(ui.key_note, key_note()) }
    ui.protocol_row.render()
    go(step)
    save_issue = saved_issue
    show(ui.status, save_issue)
    save_controls()
    remember_touch()
    if fam != nil && model == nil && !custom {
        let chosen = fam
        fam = nil
        choose_family(chosen)
    }
    if step == 1 { load_families(ui.search.text()) } else { ui.fam_list.render() }
    if tested == "testing" {
        let pending = try { mod.provider_touch_test } catch { nil }
        if pending == nil { tested = "" ui.test_box.render() ui.footer.render() remember_touch() }
        else { follow_touch_test(pending.id) }
    }
    if busy {
        let pending = try { mod.provider_touch_save } catch { nil }
        if pending == nil { busy = false save_controls() remember_touch() }
        else { follow_touch_save(pending.id) }
    }
    true
}
fn start(){
    // A style reapply restores the previous mod snapshot after evaluating this
    // run, so claim the new generation only after that restoration completes.
    if !desk { mod.provider_wizard_run = wizard_run }
    if restore_touch() { return }
    families_request = families_request + 1
    let request = families_request
    host.request("llm.families", {query: ""}, fn(r){
        if !wizard_current() || request != families_request { return }
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
let Section = Label{text: "" draw_text.color: sh_ink3 draw_text.text_style: theme.font_bold{font_size: sh_fs(11)}}
let Link = ButtonFlat{height: sh_h(32) padding: Inset{left: 4 right: 4}
    draw_bg +: {color: #x00000000 color_focus: #x00000000 color_hover: #x00000000 color_down: #x00000000 border_size: 0.0}
    draw_text +: {color: sh_link color_focus: sh_link color_hover: sh_accent_hover color_down: sh_accent_down text_style +: {font_size: sh_fs(13)}}}
let LinkOff = Link{enabled: false draw_text +: {color: sh_faint color_focus: sh_faint color_hover: sh_faint color_down: sh_faint}}
let Back = if desk { DeskPush{} } else { ButtonFlat{height: sh_h(44) padding: Inset{left: 18 right: 18}
    draw_bg +: {border_radius: by(12.0, 3.0) color: sh_field color_focus: sh_field color_hover: sh_line color_down: sh_line2 border_size: 0.0}
    draw_text +: {color: sh_ink color_focus: sh_ink color_hover: sh_ink color_down: sh_ink text_style +: {font_size: sh_fs(15)}}} }
let BackOff = if desk { DeskOff{} } else { Back{enabled: false draw_text +: {color: sh_faint color_focus: sh_faint color_hover: sh_faint color_down: sh_faint}} }
let Next = if desk { DeskDefault{} } else { ButtonFlat{height: sh_h(44) padding: Inset{left: 24 right: 24}
    draw_bg +: {border_radius: by(12.0, 3.0) color: sh_accent color_focus: sh_accent color_hover: sh_accent_hover color_down: sh_accent_down border_size: 0.0}
    draw_text +: {color: #xffffff color_focus: #xffffff color_hover: #xffffff color_down: #xffffff text_style: theme.font_bold{font_size: sh_fs(15)}}} }
let NextOff = if desk { DeskOff{} } else { ButtonFlat{enabled: false height: sh_h(44) padding: Inset{left: 24 right: 24}
    draw_bg +: {border_radius: by(12.0, 3.0) color: sh_line color_focus: sh_line color_hover: sh_line color_down: sh_line border_size: 0.0}
    draw_text +: {color: sh_faint color_focus: sh_faint color_hover: sh_faint color_down: sh_faint text_style: theme.font_bold{font_size: sh_fs(15)}}} }
let Bypass = ButtonFlat{height: sh_h(34) padding: Inset{left: 14 right: 14}
    draw_bg +: {border_radius: by(10.0, 3.0) color: tone(#xfff1e0, #xfff1e0, #x3a2a18) color_focus: tone(#xfff1e0, #xfff1e0, #x3a2a18) color_hover: tone(#xffe6c7, #xffe6c7, #x4a3520) color_down: tone(#xffd9ad, #xffd9ad, #x5a4028) border_size: 0.0}
    draw_text +: {color: tone(#xb25000, #xb25000, #xffb340) color_focus: tone(#xb25000, #xb25000, #xffb340) color_hover: tone(#xb25000, #xb25000, #xffb340) color_down: tone(#xb25000, #xb25000, #xffb340) text_style +: {font_size: sh_fs(13)}}}
let test_link = if !desk && follow_touch_appearance && dark_appearance { #x75baff } else { sh_link }
let TestButton = ButtonFlat{width: Fill height: sh_h(52)
    draw_bg +: {border_radius: by(14.0, 3.0) color: tone(#xe3edff, #xe3edff, #x1f3b5c) color_focus: tone(#xe3edff, #xe3edff, #x1f3b5c) color_hover: tone(#xd6e5ff, #xd6e5ff, #x26466c) color_down: tone(#xc7dbff, #xc7dbff, #x2b4f7a) border_size: 0.0}
    draw_text +: {color: test_link color_focus: test_link color_hover: test_link color_down: test_link text_style: theme.font_bold{font_size: sh_fs(16)}}}
let TestOff = ButtonFlat{enabled: false width: Fill height: sh_h(52)
    draw_bg +: {border_radius: by(14.0, 3.0) color: sh_field color_focus: sh_field color_hover: sh_field color_down: sh_field border_size: 0.0}
    draw_text +: {color: sh_muted color_focus: sh_muted color_hover: sh_muted color_down: sh_muted text_style: theme.font_bold{font_size: sh_fs(16)}}}
let ResultOk = RoundedView{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5} padding: Inset{left: 14 right: 14 top: 12 bottom: 12}
    show_bg: true draw_bg.color: tone(#xeaf7ee, #xeaf7ee, #x1f3a27) draw_bg.border_radius: by(12.0, 3.0)}
let ResultBad = RoundedView{width: Fill height: Fit flow: Down spacing: 8 padding: Inset{left: 14 right: 14 top: 12 bottom: 12}
    show_bg: true draw_bg.color: tone(#xfdeeec, #xfdeeec, #x4a2320) draw_bg.border_radius: by(12.0, 3.0)}
let Seg = ButtonFlat{height: sh_h(38) width: Fill
    draw_bg +: {border_radius: by(10.0, 3.0) color: sh_field color_focus: sh_field color_hover: sh_line color_down: sh_line2 border_size: 0.0}
    draw_text +: {color: sh_ink2 color_focus: sh_ink2 color_hover: sh_ink2 color_down: sh_ink2 text_style +: {font_size: sh_fs(13)}}}
let SegOn = ButtonFlat{height: sh_h(38) width: Fill
    draw_bg +: {border_radius: by(10.0, 3.0) color: sh_accent color_focus: sh_accent color_hover: sh_accent_hover color_down: sh_accent_down border_size: 0.0}
    draw_text +: {color: #xffffff color_focus: #xffffff color_hover: #xffffff color_down: #xffffff text_style +: {font_size: sh_fs(13)}}}
let Bar = RoundedView{width: Fill height: 4 show_bg: true draw_bg.color: sh_line draw_bg.border_radius: by(2.0, 3.0)}
let BarOn = Bar{draw_bg.color: sh_accent}
let SumKey = Label{width: 80 text: "" draw_text.color: sh_muted draw_text.text_style.font_size: sh_fs(12)}
let SumValue = Label{width: Fill text: "" draw_text.color: sh_ink draw_text.text_style.font_size: sh_fs(13)}
let Icon = Label{text: "" draw_text.text_style: theme.font_icons{font_size: sh_fs(14)}}
// A desktop's list of choices: one group, its rows parted by a line inset both sides,
// the chosen row inset and rounded.
let DeskGroup = RoundedView{width: Fill height: Fit flow: Down padding: 4 show_bg: true draw_bg +: {color: tone(#xffffff, #xffffff, #x3a3a3c) border_size: 1.0 border_color: sh_field_line border_radius: 4.0}}
let DeskLine = View{width: Fill height: 1 padding: Inset{left: 10 right: 10} SolidView{width: Fill height: 1 draw_bg.color: sh_line}}
let ListBox = if desk { DeskGroup{height: 344 padding: 1} } else { View{width: Fill height: Fit flow: Down} }
let ListScroll = if desk { ScrollYView{width: Fill height: Fill flow: Down padding: 3} } else { View{width: Fill height: Fit flow: Down} }
let OptBox = if desk { View{width: Fill height: Fit flow: Down} } else { View{width: Fill height: Fit flow: Down spacing: 4} }
// A provider can list dozens of models: give the list room before it scrolls.
let ListFrame = if desk { DeskGroup{padding: 1} } else { View{width: Fill height: Fit flow: Down} }
let ListFit = if desk { ScrollYView{width: Fill height: Fit max_height: 380 flow: Down} } else { View{width: Fill height: Fit flow: Down} }
// A pop-up button, with a sheet's own up-down mark at its trailing edge.
let DeskPopup = DeskPush{padding: Inset{left: 10 right: 26} align: Align{x: 0.0 y: 0.5}}
let DeskPopMark = View{width: 16 height: 16 margin: Inset{right: 6} new_batch: true show_bg: true draw_bg +: {color: sh_muted
    pixel: fn() {
        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
        sdf.move_to(4.5, 6.5)
        sdf.line_to(8.0, 3.5)
        sdf.line_to(11.5, 6.5)
        sdf.stroke(self.color, 1.2)
        sdf.move_to(4.5, 9.5)
        sdf.line_to(8.0, 12.5)
        sdf.line_to(11.5, 9.5)
        return sdf.stroke(self.color, 1.2)
    }
}}
// A recommended model, said as a Mac says it: in the secondary words, not a badge.
let DeskRec = Label{clip_y: false padding: 0 width: Fit text: "Recommended" draw_text.color: sh_muted draw_text.text_style: theme.font_regular{font_size: 9}}
let DeskSeg = ButtonFlat{height: 24 margin: 0 padding: Inset{left: 12 right: 12}
    draw_bg +: {border_radius: 2.5 border_size: 0.0 color: #x00000000 color_focus: #x00000000 color_hover: sh_wash color_down: sh_wash2}
    draw_text +: {color: sh_ink color_focus: sh_ink color_hover: sh_ink color_down: sh_ink text_style: theme.font_regular{font_size: 9.75}}}
let DeskSegOn = DeskSeg{draw_bg +: {border_size: 1.0 border_color: sh_field_line color: tone(#xffffff, #xffffff, #x636366) color_focus: tone(#xffffff, #xffffff, #x636366) color_hover: tone(#xffffff, #xffffff, #x636366) color_down: tone(#xffffff, #xffffff, #x636366)}}
let DeskTrack = RoundedView{width: Fit height: 28 flow: Right align: Align{y: 0.5} spacing: 2 padding: 2 show_bg: true draw_bg +: {color: sh_wash2 border_radius: 3.5}}
let DeskSumKey = DeskKey{}
let DeskSumValue = Label{clip_y: false padding: 0 width: Fill max_lines: 1 text_overflow: Ellipsis text: "" draw_text.color: sh_ink draw_text.text_style: theme.font_regular{font_size: 9.75}}
let DeskMark = Label{clip_y: false padding: 0 text: "" draw_text.text_style: theme.font_icons{font_size: 9.75}}
let DeskSaid = Label{clip_y: false padding: 0 width: Fill max_lines: 1 text_overflow: Ellipsis text: "" draw_text.color: sh_ink draw_text.text_style: theme.font_regular{font_size: 9.75}}
// What a desktop's sheet says otherwise: a plain title for each step in a Mac's case, as
// Internet Accounts has them, with no count of steps.
fn desk_heading(){
    if !desk { return }
    if editing {
        ui.sub.set_visible(false)
        ui.title.set_text("Edit Model")
        return
    }
    if step == 1 { ui.title.set_text("Choose a Provider") }
    if step == 2 { ui.title.set_text("Choose a Model") }
    if step == 3 { ui.title.set_text("Choose a Route") }
    if step == 4 {
        ui.title.set_text("API Key")
        ui.sub.set_text("The key for " + fam.label + " stays with OctoSense. Apps only see that one is set.")
    }
    if step == 5 {
        ui.title.set_text("Test Connection")
        ui.sub.set_text("One short request checks the route and the key before anything is saved.")
    }
}
// A desktop's test: a push button that hugs its words, and the result beside it, in words.
fn key_refused(){ reason.search("invalid key") >= 0 }
fn desk_test_view(){
    View{width: Fill height: Fit flow: Down spacing: 8
        View{width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5}
            if !edit_form {
                if tested == "testing" { DeskOff{text: "Testing…"} } else { DeskPush{text: "Test Connection" on_click: || test()} }
            }
            if tested == "ok" {
                DeskMark{text: "@CHECK@" draw_text.color: sh_ok}
                DeskSaid{text: "Connected"}
            }
            if tested == "fail" || tested == "network" {
                DeskMark{text: "@CROSS@" draw_text.color: sh_danger}
                DeskSaid{text: if key_refused() { "The provider refused this API key." } else { "Couldn't connect: " + reason }}
            }
            if tested == "testing" { DeskSaid{text: "Sending one short request…" draw_text.color: sh_muted} }
            if tested == "" && !edit_form { DeskSaid{text: "You can save once the test passes." draw_text.color: sh_muted} }
        }
        if tested == "fail" || tested == "network" {
            if !key_refused() { Note{text: result} }
            if tested == "network" {
                Note{text: "The provider could not be reached from here. You can save without testing and test it later from the list."}
                DeskPush{text: "Save Without Testing" on_click: || save()}
            } else {
                Note{text: if edit_form { "Fix the key or the route, then try again." } else { "Fix the key or the route (Back), then test again." }}
            }
        }
    }
}
let WizardCard = RoundedView{width: Fill max_width: by(9999, 560) height: by(Fill, Fit) max_height: by(9999, 570) flow: Down padding: Inset{left: 20 right: 20 top: 16 bottom: 16} new_batch: by(true, false) show_bg: true draw_bg.color: sh_card draw_bg.border_radius: by(20.0, 5.0) draw_bg.border_size: by(0.0, 1.0) draw_bg.border_color: sh_line
        View{width: Fill height: Fit flow: Down spacing: 6
            View{visible: !desk width: Fill height: Fit flow: Right align: Align{y: 0.5}
                kicker := Caption{width: Fill padding: 0 text: "OctoSense · @TITLE@"}
                header_action := View{width: Fit height: Fit on_render: || {
                    if busy { LinkOff{text: "Cancel"} } else { Link{text: "Cancel" on_click: || cancel()} }
                }}
            }
            progress := View{visible: !desk width: Fill height: Fit flow: Right spacing: 6 on_render: || {
                if step >= 1 { BarOn{} } else { Bar{} }
                if step >= 2 { BarOn{} } else { Bar{} }
                if step >= 3 { BarOn{} } else { Bar{} }
                if step >= 4 { BarOn{} } else { Bar{} }
                if step >= 5 { BarOn{} } else { Bar{} }
            }}
            step := Label{visible: !desk width: Fill text: "" padding: Inset{top: 6} draw_text.color: sh_link draw_text.text_style: theme.font_bold{font_size: sh_fs(12)}}
            title := Label{width: Fill padding: 0 text: "" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: by(sh_fs(20), 11.25)}}
            sub := Label{width: Fill padding: 0 text: "" draw_text.color: sh_ink3 draw_text.text_style.font_size: by(sh_fs(13), 9)}
            status := Status{visible: false}
        }
        ScrollYView{width: Fill height: by(Fill, Fit) flow: Down spacing: by(0, 8) padding: Inset{top: 14 bottom: 8}
            provider_row := View{visible: false width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                DeskKey{text: "Provider:"}
                form_provider := DeskSumValue{}
            }
            page1 := View{visible: false width: Fill height: Fit flow: Down spacing: 8
                if desk {
                    View{width: Fill height: Fit flow: Overlay align: Align{y: 0.5}
                        search := Field{padding: Inset{left: 28 right: 8 top: 6 bottom: 6} empty_text: "Search" on_change: |text| load_families(text)}
                        DeskSearchGlyph{margin: Inset{left: 8}}
                    }
                } else {
                    View{width: Fill height: Fit
                        search := Field{empty_text: "Search families or models" on_change: |text| load_families(text)}
                    }
                }
                ListBox{ListScroll{
                fam_list := View{width: Fill height: Fit flow: Down spacing: by(6, 0) on_render: || {
                    View{width: Fill height: by(1, 0)}
                    if families.len() == 0 { Note{text: by("No family or model matches.", "No provider or model matches.")} }
                    for i f in families {
                        if desk && i > 0 { DeskLine{} }
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
                                    if !desk { Glyph{text: "@CHEVRON_RIGHT@"} }
                                }
                            }
                        }
                    }
                }}
                }}
            }
            // Keep these intermediate pages in separate containers so their
            // named controls remain reachable in the touch widget tree.
            page2_host := View{width: Fill height: Fit flow: Down
            page2 := View{visible: false width: Fill height: Fit flow: Down spacing: 8
                model_row := View{visible: false width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                    DeskKey{text: "Model:"}
                    View{width: Fit height: 28 flow: Overlay align: Align{x: 1.0 y: 0.5}
                        model_pick := GestureView{width: Fit height: 28 on_menu: |title| model_chosen(title)
                            model_pop := DeskPopup{reset_hover_on_click: true text: "" on_click: || model_menu()}
                        }
                        DeskPopMark{}
                    }
                    form_model_note := Caption{}
                }
                model_frame := ListFrame{model_scroll := ListFit{
                model_list := View{width: Fill height: Fit flow: Down spacing: 4 on_render: || {
                    if !desk {
                    GestureView{width: Fill height: Fit on_tap: |x, y| toggle("model")
                        Drop{
                            View{width: Fill height: Fit flow: Down spacing: 2
                                OptTitleOn{text: model_title()}
                                OptNote{text: model_note()}
                            }
                            if open == "model" { Glyph{text: "@CHEVRON_UP@"} } else { Glyph{text: "@CHEVRON_DOWN@"} }
                        }
                    }
                    }
                    if open == "model" || desk {
                        OptBox{
                        for i m in models {
                            if desk && i > 0 { DeskLine{} }
                            GestureView{width: Fill height: Fit on_tap: |x, y| select_model(m)
                                Opt{
                                    View{width: Fill height: Fit flow: Down spacing: 2
                                        View{width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                            if !custom && model != nil && model.id == m.id { OptTitleOn{text: m.label} } else { OptTitle{text: m.label} }
                                            if m.default == true { if desk { DeskRec{} } else { Tag{TagText{text: "Recommended"}} } }
                                        }
                                        OptNote{text: m.detail}
                                    }
                                    if !custom && model != nil && model.id == m.id { Tick{text: "@CHECK@"} }
                                }
                            }
                        }
                        for f in fetched {
                            if desk { DeskLine{} }
                            GestureView{width: Fill height: Fit on_tap: |x, y| use_custom(f.id)
                                Opt{
                                    View{width: Fill height: Fit flow: Down spacing: 2
                                        OptTitle{text: f.label}
                                        OptNote{text: f.id + " · listed by the provider"}
                                    }
                                }
                            }
                        }
                        if desk { DeskLine{} }
                        GestureView{width: Fill height: Fit on_tap: |x, y| use_custom("")
                            Opt{
                                View{width: Fill height: Fit flow: Down spacing: 2
                                    OptTitle{text: by("Custom model ID…", "Custom Model ID…")}
                                    OptNote{text: "Any model the route serves"}
                                }
                                if custom { Tick{text: "@CHECK@"} }
                            }
                        }
                        if desk {
                            DeskLine{}
                            GestureView{width: Fill height: Fit on_tap: |x, y| fetch_models()
                                Opt{
                                    View{width: Fill height: Fit flow: Down spacing: 2
                                        OptTitle{text: "Fetch Models from Provider…"}
                                        OptNote{text: "Ask the provider which models it serves"}
                                    }
                                }
                            }
                        }
                        }
                    }
                }}
                }}
                custom_box := View{visible: false width: Fill height: Fit flow: by(Flow.Down, Flow.Right) spacing: by(4, 8) align: Align{y: by(0.0, 0.5)} padding: Inset{top: 4}
                    if desk { DeskKey{text: "Model ID:"} } else { Section{text: "MODEL ID"} }
                    custom_model := Field{empty_text: "e.g. deepseek-v4-pro" on_change: |text| changed()}
                }
                if !desk {
                    View{width: Fill height: Fit flow: Right align: Align{y: 0.5} padding: Inset{top: 4}
                        Link{text: "Fetch models from provider" on_click: || fetch_models()}
                    }
                }
                fetch_note := Caption{visible: false width: Fill}
            }
            }
            page3_host := View{width: Fill height: Fit flow: Down
            page3 := View{visible: false width: Fill height: Fit flow: Down spacing: 8
                route_row := View{visible: false width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                    DeskKey{text: "Route:"}
                    View{width: Fit height: 28 flow: Overlay align: Align{x: 1.0 y: 0.5}
                        route_pick := GestureView{width: Fit height: 28 on_menu: |title| route_chosen(title)
                            route_pop := DeskPopup{reset_hover_on_click: true text: "" on_click: || route_menu()}
                        }
                        DeskPopMark{}
                    }
                    form_route_note := Caption{}
                }
                route_frame := ListFrame{route_scroll := ListFit{
                route_list := View{width: Fill height: Fit flow: Down spacing: 4 on_render: || {
                    if !desk {
                    GestureView{width: Fill height: Fit on_tap: |x, y| toggle("route")
                        Drop{
                            View{width: Fill height: Fit flow: Down spacing: 2
                                OptTitleOn{text: route_title()}
                                OptNote{text: route_note()}
                            }
                            if open == "route" { Glyph{text: "@CHEVRON_UP@"} } else { Glyph{text: "@CHEVRON_DOWN@"} }
                        }
                    }
                    }
                    if open == "route" || desk {
                        OptBox{
                        for i r in route_list() {
                            if desk && i > 0 { DeskLine{} }
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
                        if desk { DeskLine{} }
                        GestureView{width: Fill height: Fit on_tap: |x, y| custom_route()
                            Opt{
                                View{width: Fill height: Fit flow: Down spacing: 2
                                    OptTitle{text: by("Custom endpoint…", "Custom Endpoint…")}
                                    OptNote{text: "Your own base URL, OpenAI- or Anthropic-compatible"}
                                }
                                if route != nil && route.id == "custom" { Tick{text: "@CHECK@"} }
                            }
                        }
                        }
                    }
                }}
                }}
                endpoint_box := View{visible: false width: Fill height: Fit flow: Down spacing: 6 padding: Inset{top: 6}
                    View{width: Fill height: Fit flow: by(Flow.Down, Flow.Right) spacing: by(6, 8) align: Align{y: by(0.0, 0.5)}
                        if desk { DeskKey{text: "Base URL:"} } else { Section{text: "BASE URL"} }
                        base_url := Field{text: "@BASE@" empty_text: "https://…/v1" on_change: |text| changed()}
                    }
                    View{width: Fill height: Fit flow: by(Flow.Down, Flow.Right) spacing: by(6, 8) align: Align{y: by(0.0, 0.5)}
                        if desk { DeskKey{text: "API Protocol:"} } else { Section{text: "API PROTOCOL · COMPATIBLE WITH" padding: Inset{top: 6}} }
                        protocol_row := View{width: Fill height: Fit flow: Right spacing: 6 on_render: || {
                            if desk {
                                DeskTrack{
                                    if protocol == "anthropic" { DeskSeg{text: "OpenAI" on_click: || set_protocol("openai")} } else { DeskSegOn{text: "OpenAI" on_click: || set_protocol("openai")} }
                                    if protocol == "anthropic" { DeskSegOn{text: "Anthropic" on_click: || set_protocol("anthropic")} } else { DeskSeg{text: "Anthropic" on_click: || set_protocol("anthropic")} }
                                }
                            } else {
                                if protocol == "anthropic" { Seg{text: "OpenAI" on_click: || set_protocol("openai")} } else { SegOn{text: "OpenAI" on_click: || set_protocol("openai")} }
                                if protocol == "anthropic" { SegOn{text: "Anthropic" on_click: || set_protocol("anthropic")} } else { Seg{text: "Anthropic" on_click: || set_protocol("anthropic")} }
                            }
                        }}
                    }
                }
            }
            }
            page4_host := View{width: Fill height: Fit flow: Down
            page4 := View{visible: false width: Fill height: Fit flow: Down spacing: 8
                View{width: Fill height: Fit flow: Right spacing: by(0, 8) align: Align{y: 0.5}
                    key_label := DeskKey{visible: false text: "API Key:"}
                    key := Field{empty_text: "Paste the key" is_password: true on_change: |text| changed() on_return: |text| next()}
                }
                View{width: Fill height: Fit flow: Right spacing: by(0, 8)
                    key_gap := View{visible: false width: 90 height: 1}
                    key_note := Caption{visible: false width: Fill}
                }
            }
            }
            page5 := View{visible: false width: Fill height: Fit flow: Down spacing: 14
                test_box := View{width: Fill height: Fit flow: Down spacing: 10 on_render: || {
                    if desk { desk_test_view() }
                    if !desk {
                    if tested == "testing" { TestOff{text: "Testing…"} } else { TestButton{text: "Test connection" on_click: || test()} }
                    if tested == "ok" {
                        ResultOk{
                            Icon{text: "@CHECK@" draw_text.color: sh_ok}
                            Label{width: Fill padding: 0 text: "Connected · " + result_ms + " ms" draw_text.color: sh_ok draw_text.text_style: theme.font_bold{font_size: sh_fs(14)}}
                        }
                    }
                    if tested == "fail" || tested == "network" {
                        ResultBad{
                            View{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                                Icon{text: "@CROSS@" draw_text.color: sh_danger}
                                Label{width: Fill padding: 0 text: "Couldn't connect: " + reason draw_text.color: tone(#xc4281c, #xc4281c, #xff7b72) draw_text.text_style: theme.font_bold{font_size: sh_fs(14)}}
                            }
                            Label{width: Fill padding: 0 text: result draw_text.color: tone(#x6e2a24, #x6e2a24, #xffb4ad) draw_text.text_style.font_size: sh_fs(12)}
                            if tested == "network" {
                                Label{width: Fill padding: 0 text: "The provider could not be reached from here. You can save without testing and test it later from the list." draw_text.color: sh_ink3 draw_text.text_style.font_size: sh_fs(12)}
                                Bypass{text: "Save without testing" on_click: || save()}
                            } else {
                                Label{width: Fill padding: 0 text: "Fix the key or the route (Back), then test again." draw_text.color: sh_ink3 draw_text.text_style.font_size: sh_fs(12)}
                            }
                        }
                    }
                    if tested == "" { Caption{width: Fill text: "Save is available once the test passes."} }
                    if tested == "testing" { Caption{width: Fill text: "Sending one tiny request…"} }
                    }
                }}
                summary := RoundedView{width: Fill height: Fit flow: Down spacing: by(4, 6) padding: Inset{left: by(14, 4) right: 14 top: 10 bottom: 10} show_bg: true draw_bg.color: tone(#xf9f9fb, #xffffff, #x3a3a3c) draw_bg.border_radius: by(12.0, 4.0) draw_bg.border_size: by(0.0, 1.0) draw_bg.border_color: sh_field_line
                    View{width: Fill height: Fit flow: Right spacing: by(0, 8)
                        if desk { DeskSumKey{text: "Provider:"} sum_family := DeskSumValue{} } else { SumKey{text: "Family"} sum_family := SumValue{} }
                    }
                    View{width: Fill height: Fit flow: Right spacing: by(0, 8)
                        if desk { DeskSumKey{text: "Model:"} sum_model := DeskSumValue{} } else { SumKey{text: "Model"} sum_model := SumValue{} }
                    }
                    View{width: Fill height: Fit flow: Right spacing: by(0, 8)
                        if desk { DeskSumKey{text: "Route:"} sum_route := DeskSumValue{} } else { SumKey{text: "Route"} sum_route := SumValue{} }
                    }
                    View{width: Fill height: Fit flow: Right spacing: by(0, 8)
                        if desk { DeskSumKey{text: "API Key:"} sum_key := DeskSumValue{} } else { SumKey{text: "API key"} sum_key := SumValue{} }
                    }
                }
            }
        }
        footer := View{width: Fill height: Fit flow: Right spacing: by(10, 8) align: Align{y: 0.5} padding: Inset{top: 12} on_render: || {
            if desk {
                if edit_form {
                    if tested == "testing" { DeskOff{text: "Testing…"} } else if busy { BackOff{text: "Test Connection"} } else { Back{text: "Test Connection" on_click: || test()} }
                } else if step > 1 { if busy { BackOff{text: "Back"} } else { Back{text: "Back" on_click: || back()} } }
                View{width: Fill height: 1}
                if busy { BackOff{text: "Cancel"} NextOff{text: "Saving…"} } else {
                    Back{text: "Cancel" on_click: || cancel()}
                    if valid() { Next{text: next_title() on_click: || next()} } else { NextOff{text: next_title()} }
                }
            } else {
                if step == 1 { Back{text: "Cancel" on_click: || cancel()} } else if busy { BackOff{text: "Back"} } else { Back{text: "Back" on_click: || back()} }
                View{width: Fill height: 1}
                if busy { NextOff{text: "Saving…"} } else if valid() { Next{text: next_text() on_click: || next()} } else { NextOff{text: next_text()} }
            }
        }}
}
SolidView{width: Fill height: Fill flow: Down align: by(Align{x: 0.0 y: 0.0}, Align{x: 0.5 y: 0.0}) draw_bg.color: sh_dim new_batch: true padding: Inset{left: 14 right: 14 top: by(18, 53) bottom: 18}
    if desk { wizard_scroll := ScrollYView{width: Fill height: Fill flow: Down align: Align{x: 0.5} WizardCard{} } } else { WizardCard{} }
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
        r#"        Title{text: by("OctoSense · Code for your phone", "QR Code for Your Phone")}
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
        "        SolidView{{width: Fit height: Fit flow: Down padding: {pad} draw_bg.color: by(sh_card, #xffffff) new_batch: by(true, false)\n        // qr-begin {px}\n",
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
fn cancel(){{ close() }}
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
        r#"        Title{{text: by("OctoSense · Code for your phone", "QR Code for Your Phone")}}
        Note{{text: by("On the phone, open AI Providers and choose Scan QR from desktop, then type the PIN. The code carries your keys: close it when you are done.", "On the phone, open AI Providers and choose Scan QR from desktop, then type the PIN. The code carries your keys, so close it when you are done.")}}
        View{{width: Fill height: Fit flow: Down align: Align{{x: 0.5}}
{qr}        }}
        Caption{{align: Align{{x: by(0.0, 0.5)}} text: "PIN"}}
        pin := Label{{width: Fill align: Align{{x: 0.5}} text: "{pin}" draw_text.color: ink draw_text.text_style: theme.font_bold{{font_size: sh_fs(28)}}}}
        countdown := Caption{{align: Align{{x: by(0.0, 0.5)}} text: "Expires in {expires}"}}
        if desk {{ Caption{{align: Align{{x: 0.5}} text: "{carries}"}} }} else {{ Note{{text: "Providers: {providers}"}} }}"#,
        qr = qr_views(size, modules, module_px(size)),
        pin = lit(pin),
        providers = lit(&labels.join(", ")),
        carries = if labels.len() == 1 { "It brings 1 model to the phone.".to_string() } else { format!("It brings {} models to the phone.", labels.len()) },
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
let code_read = false
// A desktop's sheet has no empty line waiting for words: they take room when they come.
fn say_status(text){{
    ui.status.set_text(text)
    if by(false, true) {{ ui.status.set_visible(text != "") }}
}}
fn say_note(text){{
    ui.note.set_text(text)
    if by(false, true) {{ ui.note.set_visible(text != "") }}
}}
// On a desktop Import stays dimmed until there is a code to import.
fn sync_import(){{
    if !by(false, true) {{ return }}
    let on = code_read || ui.code.text().trim() != ""
    ui.import_btn.set_visible(on)
    ui.import_off.set_visible(!on)
}}
fn scan(){{
    say_status("")
    say_note("Point the camera at the code on your computer…")
    host.request("llm.sheet.scan", {{}}, fn(r){{
        if r.is_ok {{
            code_read = true
            sync_import()
            if r.data.needs_pin == true {{ say_note("Code scanned. Type the PIN shown beside it.") }} else {{ say_note("Code scanned. Tap Import.") }}
        }} else {{ say_note("") say_status(r.error) }}
    }})
}}
fn read_image(r){{
    if !r.is_ok {{ say_note("") say_status(r.error) return }}
    if r.data.cancelled == true {{ say_note("") return }}
    if r.data.error != nil {{ say_note("") say_status(r.data.error) return }}
    say_status("")
    code_read = true
    sync_import()
    if r.data.needs_pin == true {{ say_note("Code read from the image. Type the PIN shown beside it.") }} else {{ say_note(by("Code read from the image. Tap Import.", "Code read from the image. Click Import.")) }}
}}
fn pick(){{
    say_status("")
    say_note("Choose a screenshot or photo of the code…")
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
    say_status("")
    say_note("Checking the code…")
    host.request("llm.sheet.import", {{text: ui.code.text() pin: ui.pin.text()}}, fn(r){{
        if r.is_ok {{ say_note(r.data.message) }} else {{ say_note("") say_status(r.error) }}
    }})
}}
fn cancel(){{ host.request("llm.sheet.cancel", {{}}, nil) }}
if can_scan {{ start_timeout(0.1, || scan()) }}
if can_drop {{ await_drop() }}
start_timeout(0.1, || sync_import())
{STYLES}"##
    );
    let mut buttons = String::new();
    if can_scan {
        buttons.push_str("\n            Choice{text: by(\"Scan again\", \"Scan Again\") on_click: || scan()}");
    }
    if can_pick {
        buttons.push_str("\n            pick_image := Choice{text: by(\"Choose image\", \"Choose Image…\") on_click: || pick()}");
    }
    let buttons = if buttons.is_empty() {
        String::new()
    } else {
        format!("\n        View{{width: Fill height: Fit flow: Right spacing: 6{buttons}\n        }}")
    };
    let image_note = match (can_pick, can_drop) {
        (_, true) => "\n        Caption{text: \"Or drop a screenshot of the code on this sheet.\"}",
        (true, false) => "\n        Caption{text: by(\"Choose image reads the code from a screenshot or photo.\", \"Choose Image reads the code from a screenshot or photo.\")}",
        _ => "",
    };
    let paste = if can_scan || can_pick { "Or paste the code" } else { "Paste the code" };
    let content = format!(
        r#"        Title{{text: by("OctoSense · Import providers", "Import Models")}}
        Note{{text: by("Show the code on your computer: AI Providers, Show QR for phone. Its providers are added after yours, as fallbacks; the keys it carries go to OctoSense, not to the app that asked.", "Show the code on the other device: AI Providers, Show QR Code. Its models are added after yours, as fallbacks; the keys it carries go to OctoSense, not to the app that asked.")}}
        status := Status{{visible: by(true, false)}}
        note := Note{{visible: by(true, false)}}{buttons}{image_note}
        View{{width: Fill height: Fit flow: by(Flow.Down, Flow.Right) spacing: 8 align: Align{{y: by(0.0, 0.5)}}
            if desk {{ DeskKey{{text: "Code:"}} }} else {{ Caption{{text: "{paste} (OCTOS1E:…)"}} }}
            code := Field{{empty_text: by("OCTOS1E:…", "Paste the code") on_change: |text| sync_import()}}
        }}
        View{{width: Fill height: Fit flow: by(Flow.Down, Flow.Right) spacing: 8 align: Align{{y: by(0.0, 0.5)}}
            if desk {{ DeskKey{{text: "PIN:"}} }} else {{ Caption{{text: "PIN"}} }}
            pin := Field{{empty_text: "XXXX-XXXX" is_password: true}}
        }}"#
    );
    script.push_str(&frame(
        r#"            Plain{text: "Cancel" on_click: || cancel()}
            View{width: Fill height: 1}
            import_btn := Primary{visible: by(true, false) text: "Import" on_click: || submit()}
            import_off := DeskOff{visible: by(false, true) text: "Import"}"#,
        &content,
    ));
    script
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pairing_sheet_shows_the_code_and_a_qr_of_the_link_only() {
        let (size, modules) = octosense_llm_config::qr::render_matrix("https://web.example/?octos=http%3A%2F%2F127.0.0.1%3A4000&pair=ABCD2345").unwrap();
        let sheet = pair_client("ABCD2345", "http://127.0.0.1:4000", Some((size, &modules)), 300);
        assert!(sheet.contains("ABCD2345") && sheet.contains("// qr-begin") && sheet.contains("Expires in 5:00"));
        assert!(sheet.contains("llm.sheet.client_back"), "leaving the code's sheet turns pairing off");
        let without = pair_client("AB\"CD", "http://127.0.0.1:4000", None, 300);
        assert!(!without.contains("// qr-begin") && !without.contains("AB\"CD"), "literals are sanitised");
    }

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

    #[test]
    fn a_desktop_wizard_has_a_plain_title_per_step_and_continue() {
        let add = edit(None, false);
        // A phone keeps its step line and its Next; a Mac counts no steps and goes on with Continue.
        assert!(add.contains("if t == \"Next\" { return by(t, \"Continue\") }"));
        assert!(!add.contains("\"Step \" + step + \" of 5\""), "a Mac's sheet counts no steps");
        assert!(add.contains("View{visible: !desk width: Fill height: Fit flow: Right"), "the kicker is the phone's");
        assert!(add.contains("if desk { DeskRec{} } else { Tag{TagText{text: \"Recommended\"}} }"));
    }

    #[test]
    fn a_desktop_web_clients_sheet_keeps_the_terminal_address_behind_advanced() {
        let sheet = connect_client();
        assert!(sheet.contains("Title{text: \"Web Clients\"}") && sheet.contains("text: \"Advanced\""));
        assert!(!sheet.contains("Show Connection Details"));
        // The phone's sheet keeps its name.
        assert!(sheet.contains("Title{text: \"Talk to Octos\"}"));
    }
}
