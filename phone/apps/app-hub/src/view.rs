//! Native App Hub. The artwork is the only raster part of these screens.
use crate::catalog::{
    self, Backend, CatalogKind, CatalogSnapshot, Entry, EntryStatus, InstallConsent,
};
use makepad_widgets::image_cache::handle_image_cache_network_responses;
use makepad_widgets::makepad_platform::thread::{SignalToUI, ThreadOptions};
use makepad_widgets::*;
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

fn skin(vm: &mut ScriptVm, light: u32, dark: u32) -> Vec4f {
    Vec4f::from_u32(
        if makepad_wm_theme::current_for_vm(vm).is_none_or(|p| p.light_mode) {
            light
        } else {
            dark
        },
    )
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    let ground = #(skin(vm, 0xf5f6f9ff, 0x11151dff))
    let card = #(skin(vm, 0xffffffff, 0x1d2430ff))
    let ink = #(skin(vm, 0x111b2bff, 0xf5f7fbff))
    let secondary = #(skin(vm, 0x67758aff, 0xadb9cdff))
    let field = #(skin(vm, 0xe9eef6ff, 0x2c3545ff))
    let accent = #087cf0
    let clear = #00000000
    let Heavy = theme.font_bold{font_family +: {latin := FontMember{res: crate_resource("makepad_widgets:resources/Inter.ttf") weight: 800.0 asc: 0.0 desc: 0.0}}}
    let Text = Label{width: Fill padding: 0 draw_text +: {color: ink max_lines: 0 text_overflow: TextOverflow.Clip text_style: theme.font_regular{font_size: 14 line_spacing: 1.3}}}
    let Heading = Text{draw_text.text_style: Heavy{font_size: 22}}
    let Meta = Text{draw_text +: {color: secondary text_style: theme.font_regular{font_size: 12 line_spacing: 1.3}}}
    let Plain = ButtonFlat{height: 44 padding: Inset{left: 10 right: 10} margin: 0 text: "" align: Center
        draw_bg +: {border_size: uniform(0.0) color: uniform(clear) color_hover: uniform(clear) color_down: uniform(#087cf020) color_focus: uniform(clear)
            border_color: uniform(clear) border_color_hover: uniform(clear) border_color_down: uniform(clear) border_color_focus: uniform(clear)}
        draw_text +: {color: accent color_hover: accent color_down: accent color_focus: accent text_style: theme.font_bold{font_size: 13}}
    }
    let Pill = Plain{width: Fit height: 36 padding: Inset{left: 16 right: 16} draw_bg +: {border_radius: uniform(18.0) color: uniform(field) color_hover: uniform(field) color_focus: uniform(field)}}
    let Primary = Plain{width: Fill height: 46 draw_bg +: {border_radius: uniform(14.0) color: uniform(accent) color_hover: uniform(accent) color_down: uniform(#0069d5) color_focus: uniform(accent)}
        draw_text +: {color: #fff color_hover: #fff color_down: #fff color_focus: #fff}}
    let Card = RoundedAllView{width: Fill height: Fit flow: Down padding: 16 spacing: 10 draw_bg +: {color: card border_radius: vec4(16.0)}}
    let Item = View{width: Fill height: Fit flow: Down padding: Inset{left: 20 right: 20}}
    let AppMark = RoundedAllView{width: 54 height: 54 flow: Overlay align: Center draw_bg +: {color: #078be8 border_radius: vec4(12.0)}
        mark := View{width: Fit height: Fit Icon{icon_walk: Walk{width: 28 height: 28} draw_icon +: {svg: crate_resource("self:resources/icons/apps.svg") color: #fff}}}
        builtin := View{visible: false width: Fill height: Fill
            art := AppIcon{width: Fill height: Fill}
        }
        image := Image{visible: false width: Fill height: Fill fit: ImageFit.Smallest}
    }
    let AppRow = Item{
        row := Card{flow: Right padding: Inset{left: 12 right: 12 top: 12 bottom: 12} spacing: 12 align: Align{y: 0.5} cursor: MouseCursor.Hand
            icon := AppMark{}
            View{width: Fill height: Fit flow: Down spacing: 4
                name := Text{max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_bold{font_size: 15}}
                subtitle := Meta{max_lines: 2 text_overflow: Ellipsis}
            }
            action := Pill{text: "Get"}
        }
    }
    let Tab = Plain{width: Fill height: 58 flow: Down spacing: 4 padding: 4 icon_walk: Walk{width: 22 height: 22}
        draw_icon.color: secondary draw_text +: {color: secondary color_hover: accent color_down: accent color_focus: accent text_style: theme.font_bold{font_size: 10}}}

    mod.widgets.AppHubView = set_type_default() do #(AppHubView::register_widget(vm)) {
        width: Fill height: Fill flow: Overlay
        SolidView{width: Fill height: Fill draw_bg.color: ground}
        content := View{width: Fill height: Fill flow: Down
        brand_bar := View{width: Fill height: 50 flow: Right align: Align{y: 0.5} padding: Inset{left: 20 right: 16} spacing: 8
            AppIcon{width: 26 height: 26 name: "apphub"}
            Text{width: Fill text: "App Hub" draw_text.text_style: theme.font_bold{font_size: 13}}
            source := Pill{width: Fit text: "Live catalog" padding: Inset{left: 10 right: 10} draw_text.text_style.font_size: 11}
            refresh := Plain{width: 28 padding: 0 icon_walk: Walk{width: 17 height: 17} draw_icon +: {svg: crate_resource("self:resources/icons/refresh.svg") color: accent}}
        }
        back_bar := View{visible: false width: Fill height: 44 padding: Inset{left: 10 right: 20} flow: Right
            back := Plain{text: "‹ Back"}
            View{width: Fill height: Fit}
        }
        search_bar := View{visible: false width: Fill height: Fit flow: Down spacing: 14 padding: Inset{left: 20 right: 20 top: 6 bottom: 10}
            Heading{text: "Search" draw_text.text_style: Heavy{font_size: 32}}
            search := TextInputFlat{width: Fill height: 44 empty_text: "Search apps" margin: 0 padding: Inset{left: 14 right: 14 top: 12 bottom: 12}
                draw_bg +: {border_radius: 12.0 color: field color_hover: field color_focus: field color_empty: field border_size: 0.0}
                draw_text +: {color: ink color_hover: ink color_focus: ink color_empty: secondary color_empty_hover: secondary color_empty_focus: secondary text_style: theme.font_regular{font_size: 15}}}
        }
        list := PortalList{width: Fill height: Fill
            Title := Item{padding: Inset{left: 20 right: 20 top: 6 bottom: 18}
                title := Heading{draw_text.text_style: Heavy{font_size: 32}}
                caption := Meta{margin: Inset{top: 5}}
            }
            Section := Item{padding: Inset{left: 20 right: 20 top: 22 bottom: 12}
                title := Heading{}
            }
            Row := AppRow{}
            Hero := Item{padding: Inset{left: 20 right: 20 bottom: 8}
                row := Card{padding: 0 spacing: 0 cursor: MouseCursor.Hand
                    art := Image{width: Fill height: 194 fit: ImageFit.CropToFill src: crate_resource("self:resources/coast.jpg")}
                    View{width: Fill height: Fit flow: Down padding: 18 spacing: 7
                        eyebrow := Meta{text: "EXPLORE OCTOSENSE" draw_text +: {color: #009b84 text_style: theme.font_bold{font_size: 10}}}
                        title := Heading{text: "A little more discovery." draw_text.text_style: Heavy{font_size: 25}}
                        caption := Meta{text: "Find your next favorite place with Maps."}
                        action := Plain{text: "Explore Maps  ›" width: Fit padding: 0}
                    }
                }
            }
            Categories := Item{padding: Inset{left: 16 right: 16 bottom: 8} flow: Right spacing: 2
                all := Plain{text: "All" width: Fill}
                travel := Plain{text: "Explore" width: Fill}
                photo := Plain{text: "Create" width: Fill}
                work := Plain{text: "Work" width: Fill}
            }
            Empty := Item{padding: Inset{left: 24 right: 24 top: 48 bottom: 30}
                Card{padding: 24 spacing: 20 align: Align{x: 0.5}
                    Icon{icon_walk: Walk{width: 68 height: 68} draw_icon +: {svg: crate_resource("self:resources/icons/apps.svg") color: #00b9a2}}
                    title := Heading{draw_text.text_style: Heavy{font_size: 25}}
                    caption := Text{draw_text.color: secondary}
                    retry := Primary{text: "Refresh"}
                    preview := Plain{text: "Explore preview catalog" width: Fill}
                }
            }
            Notice := Item{padding: Inset{left: 20 right: 20 top: 8 bottom: 8}
                caption := Meta{max_lines: 5}
            }
            Detail := Item{
                Card{spacing: 14
                    View{width: Fill height: Fit flow: Right spacing: 16 align: Align{y: 0.5}
                        icon := AppMark{width: 78 height: 78 draw_bg.border_radius: vec4(18.0)}
                        View{width: Fill height: Fit flow: Down spacing: 6
                            name := Heading{}
                            subtitle := Meta{}
                        }
                    }
                    action := Primary{text: "Get"}
                    caption := Meta{}
                }
            }
            Copy := Item{padding: Inset{left: 20 right: 20 top: 14 bottom: 8} spacing: 10
                title := Heading{}
                body := Text{}
            }
            Screenshot := Item{padding: Inset{left: 20 right: 20 top: 10 bottom: 10}
                art := Image{width: Fill height: 280 fit: ImageFit.Smallest}
            }
            PreviewArt := Item{padding: Inset{left: 20 right: 20 top: 14 bottom: 8} spacing: 8
                Image{width: Fill height: 210 fit: ImageFit.CropToFill src: crate_resource("self:resources/mountain.jpg")}
                Meta{text: "A glimpse of OctoSense • Preview artwork"}
            }
            Consent := Item{padding: Inset{left: 20 right: 20 top: 12 bottom: 24}
                Card{spacing: 14
                    title := Heading{}
                    permissions := Text{}
                    privacy := Meta{}
                    confirm := Primary{text: "Install"}
                    cancel := Plain{text: "Cancel" width: Fill}
                }
            }
            Space := View{width: Fill height: 20}
        }
        nav := SolidView{width: Fill height: 70 flow: Right padding: Inset{left: 12 right: 12 top: 4 bottom: 8} show_bg: true draw_bg.color: card
            today := Tab{text: "Today" draw_icon.svg: crate_resource("self:resources/icons/today.svg")}
            apps := Tab{text: "Apps" draw_icon.svg: crate_resource("self:resources/icons/apps.svg")}
            search_tab := Tab{text: "Search" draw_icon.svg: crate_resource("self:resources/icons/search.svg")}
            library := Tab{text: "Library" draw_icon.svg: crate_resource("self:resources/icons/library.svg")}
        }
        }
    }
}

#[derive(Clone, Default, Debug)]
pub enum AppHubAction {
    Launch(String),
    OpenInstalled(String),
    #[default]
    None,
}
#[derive(Clone, Copy, Default, PartialEq)]
enum Page {
    #[default]
    Today,
    Apps,
    Search,
    Library,
}
#[derive(Clone)]
enum Row {
    Title(String, String),
    Section(String),
    App(Entry),
    Hero(Entry),
    Categories,
    Empty(String, String),
    Notice(String),
    Detail(Entry),
    Copy(String, String),
    Screenshot(String),
    PreviewArt,
    Consent(Entry),
    Space,
}
impl Row {
    fn template(&self) -> LiveId {
        match self {
            Self::Title(..) => id!(Title),
            Self::Section(..) => id!(Section),
            Self::App(..) => id!(Row),
            Self::Hero(..) => id!(Hero),
            Self::Categories => id!(Categories),
            Self::Empty(..) => id!(Empty),
            Self::Notice(..) => id!(Notice),
            Self::Detail(..) => id!(Detail),
            Self::Copy(..) => id!(Copy),
            Self::Screenshot(..) => id!(Screenshot),
            Self::PreviewArt => id!(PreviewArt),
            Self::Consent(..) => id!(Consent),
            Self::Space => id!(Space),
        }
    }
}
enum Command {
    Refresh,
    Install(InstallConsent),
    Open(String),
}
enum Reply {
    Catalog(CatalogSnapshot),
    Installed {
        app_id: String,
        snapshot: CatalogSnapshot,
        error: Option<String>,
    },
    Open(String, Result<(), String>),
}

// Owned by the host process, not an App Hub view: downloads can finish
// after the user closes the view and drops its reply receiver.
static COMPLETED_INSTALLS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The shell drains this on UI signals, even when no App Hub view is open.
pub fn take_completed_installs() -> Vec<String> {
    std::mem::take(&mut *COMPLETED_INSTALLS.lock().unwrap_or_else(|p| p.into_inner()))
}

fn publish_reply(replies: &Sender<Reply>, reply: Reply) -> bool {
    if let Reply::Installed { app_id, error: None, .. } = &reply {
        COMPLETED_INSTALLS.lock().unwrap_or_else(|p| p.into_inner()).push(app_id.clone());
    }
    let delivered = replies.send(reply).is_ok();
    SignalToUI::set_ui_signal();
    delivered
}

#[derive(Script, Widget)]
pub struct AppHubView {
    #[deref]
    view: View,
    #[rust]
    started: bool,
    #[rust]
    style_changed: bool,
    #[rust]
    page: Page,
    #[rust]
    preview: bool,
    #[rust]
    query: String,
    #[rust]
    category: Option<String>,
    #[rust]
    selected: Option<Entry>,
    #[rust]
    confirming: bool,
    #[rust]
    busy: bool,
    #[rust]
    installing: bool,
    #[rust]
    notice: String,
    #[rust]
    snapshot: Option<CatalogSnapshot>,
    #[rust]
    rows: Vec<Row>,
    #[rust]
    tx: Option<Sender<Command>>,
    #[rust]
    rx: Option<Receiver<Reply>>,
    #[rust]
    image_bound: HashMap<WidgetUid, String>,
    #[rust]
    svg_requests: HashMap<LiveId, String>,
    #[rust]
    svg_data: HashMap<String, Option<Arc<[u8]>>>,
}
impl ScriptHook for AppHubView {
    fn on_after_apply(&mut self, _: &mut ScriptVm, apply: &Apply, _: &mut Scope, _: ScriptValue) {
        if matches!(apply, Apply::ScriptReapply) {
            self.style_changed = true;
        }
    }
}
impl AppHubView {
    fn start(&mut self, cx: &mut Cx) {
        if self.started {
            return;
        }
        self.started = true;
        let root = crate::data_root(cx);
        let (tx, commands) = mpsc::channel();
        let (replies, rx) = mpsc::channel();
        match cx
            .thread_spawner()
            .spawn_worker(ThreadOptions::default(), move || {
                let mut backend = Backend::from_environment(root);
                while let Ok(command) = commands.recv() {
                    let reply = match command {
                        Command::Refresh => Reply::Catalog(backend.refresh()),
                        Command::Install(consent) => {
                            let error = backend.install(&consent).err();
                            Reply::Installed {
                                app_id: consent.app_id,
                                snapshot: backend.snapshot(),
                                error,
                            }
                        }
                        Command::Open(id) => {
                            let result = backend.may_open(&id);
                            Reply::Open(id, result)
                        }
                    };
                    if !publish_reply(&replies, reply) {
                        break;
                    }
                }
            }) {
            Ok(handle) => {
                handle.detach();
                self.tx = Some(tx);
                self.rx = Some(rx);
                self.refresh(cx);
            }
            Err(e) => self.notice = format!("Could not start App Hub: {e:?}"),
        }
        self.render(cx);
    }
    pub fn shutdown(&mut self, cx: &mut Cx) {
        for (request, _) in self.svg_requests.drain() {
            cx.cancel_http_request(request);
        }
        self.tx = None;
        self.rx = None;
    }
    fn refresh(&mut self, cx: &mut Cx) {
        if self.busy {
            return;
        }
        self.image_bound.clear();
        for (request, _) in self.svg_requests.drain() {
            cx.cancel_http_request(request);
        }
        self.svg_data.clear();
        self.send(Command::Refresh);
    }
    fn send(&mut self, command: Command) {
        if self.tx.as_ref().is_some_and(|tx| tx.send(command).is_ok()) {
            self.busy = true;
        } else {
            self.notice = "App Hub is unavailable. Close it and try again.".into();
        }
    }
    fn drain(&mut self, cx: &mut Cx, _scope: &mut Scope) {
        let replies: Vec<_> = self
            .rx
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for reply in replies {
            self.busy = false;
            match reply {
                Reply::Catalog(snapshot) => {
                    self.notice.clear();
                    self.snapshot = Some(snapshot);
                }
                Reply::Installed { snapshot, error, .. } => {
                    self.installing = false;
                    self.confirming = false;
                    self.snapshot = Some(snapshot);
                    if let Some(error) = error {
                        self.notice = format!("Installation didn’t finish. {error}");
                    } else {
                        self.notice = "Installed. Your app is ready to open.".into();
                    }
                }
                Reply::Open(id, result) => match result {
                    Ok(()) => cx.widget_action(self.widget_uid(), AppHubAction::OpenInstalled(id)),
                    Err(error) => self.notice = format!("Could not open this app. {error}"),
                },
            }
            if let Some(entry) = self
                .selected
                .as_mut()
                .filter(|entry| entry.kind == CatalogKind::Live)
            {
                if let Some(current) = self.snapshot.as_ref().and_then(|s| {
                    s.entries
                        .iter()
                        .chain(&s.library)
                        .find(|e| e.id == entry.id)
                }) {
                    *entry = current.clone();
                } else {
                    self.selected = None;
                    self.confirming = false;
                }
            }
            self.render(cx);
        }
    }
    fn entries(&self) -> Vec<Entry> {
        if self.preview {
            catalog::preview_entries()
        } else {
            self.snapshot
                .as_ref()
                .map(|s| {
                    if self.page == Page::Library {
                        s.library.clone()
                    } else {
                        s.entries.clone()
                    }
                })
                .unwrap_or_default()
        }
    }
    fn render(&mut self, cx: &mut Cx) {
        self.style_changed = false;
        self.view.button(cx, ids!(source)).set_text(
            cx,
            if self.preview {
                "Preview catalog"
            } else {
                "Live catalog"
            },
        );
        self.view
            .widget(cx, ids!(back_bar))
            .set_visible(cx, self.selected.is_some());
        self.view
            .widget(cx, ids!(search_bar))
            .set_visible(cx, self.page == Page::Search && self.selected.is_none());
        self.view
            .widget(cx, ids!(nav))
            .set_visible(cx, !self.confirming);
        self.view
            .button(cx, ids!(source))
            .set_enabled(cx, !self.installing);
        self.view
            .button(cx, ids!(refresh))
            .set_enabled(cx, !self.busy);
        for (id, page) in [
            (ids!(today), Page::Today),
            (ids!(apps), Page::Apps),
            (ids!(search_tab), Page::Search),
            (ids!(library), Page::Library),
        ] {
            let color = Vec4f::from_u32(if self.page == page {
                0x087cf0ff
            } else {
                0x8190a7ff
            });
            let mut tab = self.view.widget(cx, id);
            script_apply_eval!(cx,tab,{draw_icon.color: #(color) draw_text.color: #(color)});
        }
        self.rows.clear();
        if !self.notice.is_empty() {
            self.rows.push(Row::Notice(self.notice.clone()));
        }
        if let Some(entry) = self.selected.clone() {
            if entry.kind == CatalogKind::Live {
                if let Some(warning) = self.snapshot.as_ref().and_then(|s| s.warning.clone()) {
                    self.rows.push(Row::Notice(warning));
                }
            }
            self.rows.push(Row::Detail(entry.clone()));
            if self.confirming {
                self.rows.push(Row::Consent(entry));
            } else {
                if entry.kind == CatalogKind::Preview {
                    self.rows.push(Row::PreviewArt);
                }
                for url in &entry.screenshots {
                    self.rows.push(Row::Screenshot(url.clone()));
                }
                self.rows
                    .push(Row::Copy("About".into(), entry.description.clone()));
                self.rows.push(Row::Copy(
                    "Information".into(),
                    format!(
                        "{}\n{}\nVersion {}",
                        entry.publisher, entry.category, entry.version
                    ),
                ));
                if !entry.release_notes.is_empty() {
                    self.rows
                        .push(Row::Copy("What’s new".into(), entry.release_notes.clone()));
                }
                if entry.kind == CatalogKind::Live {
                    self.rows.push(Row::Copy(
                        "Permissions".into(),
                        lines_or(&entry.permissions, "No additional permissions."),
                    ));
                    self.rows.push(Row::Copy(
                        "Privacy".into(),
                        lines_or(&entry.privacy, "No additional privacy information."),
                    ));
                }
            }
        } else {
            let title = match self.page {
                Page::Today => "Today",
                Page::Apps => "Apps",
                Page::Search => "Search",
                Page::Library => "Library",
            };
            let caption = if self.preview {
                "Preview • Apps included with OctoSense"
            } else if self.page == Page::Library {
                "Your installed apps"
            } else {
                "Discover apps for your OctoSense"
            };
            if self.page != Page::Search {
                self.rows.push(Row::Title(title.into(), caption.into()));
            }
            if !self.preview {
                if let Some(warning) = self.snapshot.as_ref().and_then(|s| s.warning.as_ref()) {
                    self.rows.push(Row::Notice(warning.clone()));
                }
            }
            if matches!(self.page, Page::Apps | Page::Search) {
                self.rows.push(Row::Categories);
            }
            let all = self.entries();
            let entries = catalog::filter_entries(
                &all,
                if self.page == Page::Search {
                    &self.query
                } else {
                    ""
                },
                self.category.as_deref(),
            );
            if entries.is_empty() {
                let (title, caption) = if self.busy && self.snapshot.is_none() {
                    ("Connecting to App Hub", "Checking the latest catalog…")
                } else if (self.page == Page::Search && !self.query.is_empty())
                    || self.category.is_some()
                {
                    ("No matching apps", "Try another search or choose All.")
                } else if self.page == Page::Library {
                    (
                        "Make yourself at home",
                        "Apps you install from the Hub will appear here.",
                    )
                } else if self.snapshot.as_ref().is_some_and(|s| !s.verified) {
                    (
                        "Couldn’t reach App Hub",
                        "Check your connection and try again.",
                    )
                } else {
                    (
                        "Good things are on the way",
                        "Apps published to the OctoSense Hub will appear here.",
                    )
                };
                self.rows.push(Row::Empty(title.into(), caption.into()));
            } else {
                if self.page == Page::Today && self.preview {
                    if let Some(entry) = all.iter().find(|e| e.id == "maps") {
                        self.rows.push(Row::Hero(entry.clone()));
                    }
                }
                if self.page == Page::Today {
                    self.rows.push(Row::Section(
                        if self.preview {
                            "Made for OctoSense"
                        } else {
                            "Explore the Hub"
                        }
                        .into(),
                    ));
                }
                for entry in entries {
                    self.rows.push(Row::App(entry));
                }
            }
            if !self.preview {
                self.rows.push(Row::Notice(
                    if let Some(p) = self.snapshot.as_ref().and_then(|s| s.published.as_ref()) {
                        format!("OctoSense App Hub • Catalog published {p}")
                    } else {
                        "OctoSense App Hub".into()
                    },
                ));
            }
        }
        self.rows.push(Row::Space);
        self.view.redraw(cx);
    }
    fn navigate(&mut self, cx: &mut Cx, page: Page) {
        self.page = page;
        self.selected = None;
        self.confirming = false;
        self.category = None;
        self.reset_scroll(cx);
        self.render(cx);
    }
    fn reset_scroll(&mut self, cx: &mut Cx) {
        self.view
            .portal_list(cx, ids!(list))
            .set_first_id_and_scroll(0, 0.0);
    }
    fn activate(&mut self, cx: &mut Cx, _scope: &mut Scope, entry: Entry) {
        if self.busy && entry.status != EntryStatus::BuiltIn {
            return;
        }
        match entry.status {
            EntryStatus::BuiltIn => {
                cx.widget_action(self.widget_uid(), AppHubAction::Launch(entry.id))
            }
            EntryStatus::Installed => self.send(Command::Open(entry.id)),
            EntryStatus::Available | EntryStatus::UpdateAvailable => {
                if entry.consent.is_some() {
                    self.selected = Some(entry);
                    self.confirming = true;
                    self.reset_scroll(cx);
                } else {
                    self.notice = "Refresh the catalog before installing this app.".into();
                }
            }
            EntryStatus::Unavailable(reason) => self.notice = reason,
        }
        self.render(cx);
    }
    fn bind_image(&mut self, cx: &mut Cx, image: ImageRef, url: Option<&str>) {
        let uid = image.widget_uid();
        let Some(url) = url else {
            image.set_visible(cx, false);
            self.image_bound.remove(&uid);
            return;
        };
        if self.image_bound.get(&uid).is_none_or(|held| held != url) {
            self.image_bound.remove(&uid);
            image.set_texture(cx, None);
            let svg = url
                .split(['?', '#'])
                .next()
                .unwrap_or(url)
                .to_ascii_lowercase()
                .ends_with(".svg");
            let remote = url.starts_with("https://") || url.starts_with("http://");
            // The framework's async texture cache decodes rasters only. SVGs
            // use Image's vector parser, after an asynchronous HTTP request.
            let result = if svg {
                if !self.svg_data.contains_key(url) {
                    if remote {
                        let request = LiveId::unique();
                        self.svg_data.insert(url.into(), None);
                        self.svg_requests.insert(request, url.into());
                        let mut download = HttpRequest::new(url.into(), HttpMethod::GET);
                        download.set_max_response_body_bytes(1024 * 1024);
                        cx.http_request(request, download);
                    } else {
                        use std::io::Read;
                        let bytes = std::fs::File::open(url).ok().and_then(|file| {
                            let mut bytes = Vec::new();
                            file.take(1024 * 1024 + 1).read_to_end(&mut bytes).ok()?;
                            (bytes.len() <= 1024 * 1024).then(|| Arc::from(bytes))
                        });
                        self.svg_data.insert(url.into(), bytes);
                    }
                }
                let Some(Some(data)) = self.svg_data.get(url) else {
                    image.set_visible(cx, false);
                    return;
                };
                image.load_svg_from_shared_data(cx, data.clone())
            } else if remote {
                image.load_image_http_by_url_async(cx, url)
            } else {
                image.load_image_file_by_path_async(cx, std::path::Path::new(url))
            };
            if result.is_err() {
                image.set_visible(cx, false);
                return;
            }
            self.image_bound.insert(uid, url.into());
        }
        image.set_visible(cx, true);
    }
    fn receive_svg(&mut self, cx: &mut Cx, responses: &NetworkResponsesEvent) {
        for response in responses {
            let (request, body) = match response {
                NetworkResponse::HttpResponse {
                    request_id,
                    response,
                }
                | NetworkResponse::HttpStreamComplete {
                    request_id,
                    response,
                } => (
                    request_id,
                    response
                        .body
                        .as_ref()
                        .filter(|body| {
                            (200..300).contains(&response.status_code) && body.len() <= 1024 * 1024
                        })
                        .cloned(),
                ),
                NetworkResponse::HttpError { request_id, .. } => (request_id, None),
                _ => continue,
            };
            if let Some(url) = self.svg_requests.remove(request) {
                self.svg_data.insert(url, body);
                self.view.redraw(cx);
            }
        }
    }
    fn fill_entry(&mut self, cx: &mut Cx, item: &WidgetRef, entry: &Entry) {
        item.label(cx, ids!(name)).set_text(cx, &entry.name);
        item.label(cx, ids!(subtitle)).set_text(cx, &entry.subtitle);
        item.button(cx, ids!(action)).set_enabled(
            cx,
            (!self.busy || entry.status == EntryStatus::BuiltIn) && can_activate(entry),
        );
        item.button(cx, ids!(action)).set_text(
            cx,
            if self.installing && self.selected.as_ref().is_some_and(|e| e.id == entry.id) {
                "Installing…"
            } else {
                button_text(entry)
            },
        );
        let known = entry.kind == CatalogKind::Preview;
        let color = Vec4f::from_u32(if known || entry.icon.is_some() { 0 } else { match entry.category.as_str() {
            "travel" => 0x00ac96ff,
            "news" => 0xf44863ff,
            "photo-video" => 0x8c68dfff,
            "productivity" => 0x2587e8ff,
            _ => 0x00a78fff,
        }});
        let mut icon = item.widget(cx, ids!(icon));
        script_apply_eval!(cx,icon,{draw_bg.color: #(color)});
        self.bind_image(cx, item.image(cx, ids!(icon.image)), entry.icon.as_deref());
        let artwork = item.image(cx, ids!(icon.image));
        let loaded = self
            .image_bound
            .get(&artwork.widget_uid())
            .map(String::as_str)
            == entry.icon.as_deref()
            && entry.icon.is_some()
            && artwork.has_content();
        item.widget(cx, ids!(icon.mark))
            .set_visible(cx, !known && !loaded);
        item.widget(cx, ids!(icon.builtin)).set_visible(cx, known);
        if known {
            if let Some(mut art) = item.widget(cx, ids!(icon.builtin.art)).borrow_mut::<app_icon::AppIcon>() {
                art.set_name(cx, &entry.id);
            }
        }
    }
    fn draw_rows(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        list.set_item_range(cx, 0, self.rows.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(row) = self.rows.get(index).cloned() else {
                continue;
            };
            let item = list.item(cx, index, row.template());
            if matches!(row, Row::App(_)) {
                let top = if index > 0 && matches!(self.rows.get(index - 1), Some(Row::App(_))) {
                    0.5
                } else {
                    16.0
                };
                let bottom = if matches!(self.rows.get(index + 1), Some(Row::App(_))) {
                    0.5
                } else {
                    16.0
                };
                let radius = vec4(top, top, bottom, bottom);
                let mut segment = item.widget(cx, ids!(row));
                script_apply_eval!(cx,segment,{draw_bg.border_radius: #(radius)});
            }
            match row {
                Row::Title(title, caption) | Row::Empty(title, caption) => {
                    item.label(cx, ids!(title)).set_text(cx, &title);
                    item.label(cx, ids!(caption)).set_text(cx, &caption);
                    item.widget(cx, ids!(preview))
                        .set_visible(cx, !self.preview);
                    item.button(cx, ids!(retry)).set_text(
                        cx,
                        if (self.page == Page::Search && !self.query.is_empty())
                            || self.category.is_some()
                        {
                            "Clear filters"
                        } else if self.busy {
                            "Connecting…"
                        } else {
                            "Refresh"
                        },
                    );
                }
                Row::Section(title) => item.label(cx, ids!(title)).set_text(cx, &title),
                Row::App(entry) | Row::Detail(entry) => {
                    self.fill_entry(cx, &item, &entry);
                    item.label(cx, ids!(caption)).set_text(
                        cx,
                        if entry.kind == CatalogKind::Preview {
                            "Included with OctoSense • Preview"
                        } else {
                            &entry.publisher
                        },
                    );
                }
                Row::Notice(text) => item.label(cx, ids!(caption)).set_text(cx, &text),
                Row::Copy(title, body) => {
                    item.label(cx, ids!(title)).set_text(cx, &title);
                    item.label(cx, ids!(body)).set_text(cx, &body);
                }
                Row::Screenshot(url) => {
                    let image = item.image(cx, ids!(art));
                    self.bind_image(cx, image, Some(&url));
                }
                Row::Consent(entry) => {
                    item.label(cx, ids!(title))
                        .set_text(cx, &format!("Install {}?", entry.name));
                    item.label(cx, ids!(permissions)).set_text(
                        cx,
                        &format!(
                            "This app will have access to:\n\n{}",
                            lines_or(&entry.permissions, "No additional permissions.")
                        ),
                    );
                    item.label(cx, ids!(privacy))
                        .set_text(cx, &lines_or(&entry.privacy, ""));
                    item.button(cx, ids!(confirm))
                        .set_enabled(cx, !self.busy && entry.consent.is_some());
                    item.button(cx, ids!(cancel))
                        .set_enabled(cx, !self.installing);
                    item.button(cx, ids!(confirm)).set_text(
                        cx,
                        if self.installing {
                            "Installing…"
                        } else {
                            "Install"
                        },
                    );
                }
                Row::Categories => {
                    for (id, category) in [
                        (ids!(all), None),
                        (ids!(travel), Some("travel")),
                        (ids!(photo), Some("photo-video")),
                        (ids!(work), Some("productivity")),
                    ] {
                        let color = Vec4f::from_u32(if self.category.as_deref() == category {
                            0x009b84ff
                        } else {
                            0x687b97ff
                        });
                        let mut button = item.widget(cx, id);
                        script_apply_eval!(cx,button,{draw_text.color: #(color)});
                    }
                }
                _ => {}
            }
            item.draw_all(cx, &mut Scope::empty());
        }
    }
    fn actions(&mut self, cx: &mut Cx, actions: &Actions, scope: &mut Scope) {
        if self.view.button(cx, ids!(source)).clicked(actions) && !self.installing {
            self.preview = !self.preview;
            self.selected = None;
            self.confirming = false;
            self.category = None;
            self.notice.clear();
            self.reset_scroll(cx);
            self.render(cx);
        }
        for (id, page) in [
            (ids!(today), Page::Today),
            (ids!(apps), Page::Apps),
            (ids!(search_tab), Page::Search),
            (ids!(library), Page::Library),
        ] {
            if self.view.button(cx, id).clicked(actions) {
                self.navigate(cx, page);
            }
        }
        if self.view.button(cx, ids!(back)).clicked(actions) {
            self.back(cx);
        }
        if self.view.button(cx, ids!(refresh)).clicked(actions) {
            self.refresh(cx);
            self.render(cx);
        }
        if let Some(query) = self.view.text_input(cx, ids!(search)).changed(actions) {
            self.query = query;
            self.reset_scroll(cx);
            self.render(cx);
        }
        let items = self
            .view
            .portal_list(cx, ids!(list))
            .items_with_actions(actions);
        for (index, item) in items {
            let Some(row) = self.rows.get(index).cloned() else {
                continue;
            };
            match row {
                Row::App(entry) | Row::Detail(entry) | Row::Hero(entry) => {
                    if item.button(cx, ids!(action)).clicked(actions) {
                        self.activate(cx, scope, entry);
                    } else if item
                        .widget(cx, ids!(row))
                        .as_view()
                        .finger_up(actions)
                        .is_some_and(|up| up.was_tap())
                    {
                        self.selected = Some(entry);
                        self.confirming = false;
                        self.reset_scroll(cx);
                        self.render(cx);
                    }
                }
                Row::Empty(..) => {
                    if item.button(cx, ids!(retry)).clicked(actions) {
                        if (self.page == Page::Search && !self.query.is_empty())
                            || self.category.is_some()
                        {
                            self.query.clear();
                            self.category = None;
                            self.view.text_input(cx, ids!(search)).set_text(cx, "");
                        } else {
                            self.refresh(cx);
                        }
                        self.render(cx);
                    }
                    if item.button(cx, ids!(preview)).clicked(actions) {
                        self.preview = true;
                        self.navigate(cx, Page::Today);
                    }
                }
                Row::Categories => {
                    for (id, category) in [
                        (ids!(all), None),
                        (ids!(travel), Some("travel")),
                        (ids!(photo), Some("photo-video")),
                        (ids!(work), Some("productivity")),
                    ] {
                        if item.button(cx, id).clicked(actions) {
                            self.category = category.map(str::to_owned);
                            self.render(cx);
                        }
                    }
                }
                Row::Consent(entry) => {
                    if item.button(cx, ids!(confirm)).clicked(actions) && !self.busy {
                        if let Some(consent) = entry.consent {
                            self.installing = true;
                            self.send(Command::Install(consent));
                            self.render(cx);
                        }
                    }
                    if item.button(cx, ids!(cancel)).clicked(actions) && !self.installing {
                        self.confirming = false;
                        self.render(cx);
                    }
                }
                _ => {}
            }
        }
    }
    fn back(&mut self, cx: &mut Cx) -> bool {
        if self.installing {
            return true;
        }
        if self.confirming {
            self.confirming = false;
        } else if self.selected.take().is_none() {
            return false;
        }
        self.reset_scroll(cx);
        self.render(cx);
        true
    }
}
fn can_activate(entry: &Entry) -> bool {
    match entry.status {
        EntryStatus::Available | EntryStatus::UpdateAvailable => entry.consent.is_some(),
        EntryStatus::Unavailable(_) => false,
        EntryStatus::BuiltIn | EntryStatus::Installed => true,
    }
}
fn button_text(entry: &Entry) -> &str {
    if matches!(
        entry.status,
        EntryStatus::Available | EntryStatus::UpdateAvailable
    ) && entry.consent.is_none()
    {
        return "Refresh";
    }
    match entry.status {
        EntryStatus::BuiltIn | EntryStatus::Installed => "Open",
        EntryStatus::UpdateAvailable => "Update",
        EntryStatus::Available => "Get",
        EntryStatus::Unavailable(_) => "Unavailable",
    }
}
fn lines_or(lines: &[String], empty: &str) -> String {
    if lines.is_empty() {
        empty.into()
    } else {
        lines.join("\n\n")
    }
}
impl Widget for AppHubView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.start(cx);
        self.drain(cx, scope);
        if let Event::NetworkResponses(responses) = event {
            self.receive_svg(cx, responses);
            handle_image_cache_network_responses(cx, responses);
        }
        if let Event::BackPressed { handled } = event {
            if !handled.get() && self.back(cx) {
                handled.set(true);
            }
        }
        if let Event::Actions(actions) = event {
            self.actions(cx, actions, scope);
        }
        self.view.handle_event(cx, event, scope);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.start(cx);
        if self.style_changed {
            self.render(cx);
        }
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.as_portal_list().borrow_mut() {
                self.draw_rows(cx, &mut list);
            }
        }
        DrawStep::done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_app_module::AppModule;

    #[test]
    fn completed_install_survives_closed_store_view() {
        let (sender, receiver) = mpsc::channel();
        drop(receiver); // AppHubView::shutdown drops this while work is in flight.
        assert!(!publish_reply(&sender, Reply::Installed {
            app_id: "org.example.failed".into(),
            snapshot: Default::default(),
            error: Some("Download failed".into()),
        }));
        assert!(take_completed_installs().is_empty());
        assert!(!publish_reply(&sender, Reply::Installed {
            app_id: "org.example.updated".into(),
            snapshot: Default::default(),
            error: None,
        }));
        assert_eq!(take_completed_installs(), vec!["org.example.updated"]);
        assert!(take_completed_installs().is_empty(), "each completion is consumed once");
    }
    #[test]
    fn native_module_and_all_presentation_states_evaluate() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let vm_id = cx.alloc_splash_vm_with_network(false);
        let root = cx.with_script_vm_id_trusted(vm_id, |vm| {
            crate::APP_HUB_MODULE.register(vm);
            let value = script_eval!(vm,{use mod.widgets.* AppHubView{}});
            let root = WidgetRef::script_from_value(vm, value);
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "native App Hub errors: {errors:?}");
            root
        });
        let entered = makepad_widgets::widget_async::enter_isolate(&mut cx, vm_id);
        let artwork = cx.with_vm(|vm| {
            let value = script_eval!(vm, {use mod.widgets.* Image{}});
            WidgetRef::script_from_value(vm, value).as_image()
        });
        {
            let mut view = root.borrow_mut::<AppHubView>().unwrap();
            view.started = true; // Presentation tests must not fetch a real catalog.
            // Both store surfaces must render the shared launcher identity,
            // not a second set of toolbar glyphs or category-colored artwork.
            for template in [id!(Row), id!(Detail)] {
                let item = view.view.portal_list(&mut cx, ids!(list)).item(&mut cx, 0, template);
                for entry in catalog::preview_entries() {
                    view.fill_entry(&mut cx, &item, &entry);
                    assert!(item.widget(&mut cx, ids!(icon.builtin.art))
                        .borrow::<app_icon::AppIcon>().is_some(), "preview must use the shared AppIcon");
                    assert!(item.widget(&mut cx, ids!(icon.builtin)).visible());
                    let mut live = entry.clone();
                    live.kind = CatalogKind::Live;
                    view.fill_entry(&mut cx, &item, &live);
                    assert!(!item.widget(&mut cx, ids!(icon.builtin)).visible(),
                        "a recycled live listing must not retain the builtin icon");
                }
            }
            let art_dir = tempfile::tempdir().unwrap();
            let svg = art_dir.path().join("icon.svg");
            std::fs::write(&svg, br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2H22V22H2Z" fill="#00ac96"/></svg>"##).unwrap();
            view.bind_image(&mut cx, artwork.clone(), svg.to_str());
            assert!(artwork.has_content(), "catalog SVG icons must render");
            let remote_svg = "https://example.com/icon.svg?revision=2";
            view.bind_image(&mut cx, artwork.clone(), Some(remote_svg));
            assert!(
                !artwork.has_content(),
                "a recycled icon must not show old artwork"
            );
            let request_id = *view.svg_requests.keys().next().unwrap();
            view.receive_svg(
                &mut cx,
                &vec![NetworkResponse::HttpResponse {
                    request_id,
                    response: HttpResponse::new(
                        LiveId(0),
                        200,
                        Default::default(),
                        Some(std::fs::read(&svg).unwrap()),
                    ),
                }],
            );
            view.bind_image(&mut cx, artwork.clone(), Some(remote_svg));
            assert!(
                artwork.has_content(),
                "downloaded SVGs must use the vector parser"
            );
            view.snapshot = Some(CatalogSnapshot {
                verified: true,
                ..Default::default()
            });
            view.render(&mut cx);
            assert!(view.rows.iter().any(|row| matches!(row, Row::Empty(..))));
            view.preview = true;
            for page in [Page::Today, Page::Apps, Page::Search, Page::Library] {
                view.navigate(&mut cx, page);
                assert!(view.rows.iter().any(|row| matches!(row, Row::App(_))));
            }
            view.query = "NO MATCH".into();
            view.page = Page::Search;
            view.render(&mut cx);
            assert!(!view.rows.iter().any(|row| matches!(row, Row::App(_))));
            let entry = catalog::preview_entries().remove(0);
            view.selected = Some(entry);
            view.render(&mut cx);
            assert!(view.rows.iter().any(|row| matches!(row, Row::Detail(_))));
            assert!(view.back(&mut cx));
            assert!(view.selected.is_none());
            view.preview = false;
            view.page = Page::Library;
            view.render(&mut cx);
            assert!(
                view.rows
                    .iter()
                    .any(|row| matches!(row,Row::Empty(title,_) if title=="Make yourself at home")),
                "saved search must not filter Library"
            );
            let mut old = catalog::preview_entries().remove(0);
            old.kind = CatalogKind::Live;
            old.status = EntryStatus::Available;
            view.selected = Some(old.clone());
            view.confirming = true;
            view.installing = true;
            let mut changed = old;
            changed.status = EntryStatus::Unavailable("Withdrawn".into());
            let (sender, receiver) = mpsc::channel();
            view.rx = Some(receiver);
            sender
                .send(Reply::Installed {
                    app_id: changed.id.clone(),
                    snapshot: CatalogSnapshot {
                        entries: vec![changed],
                        verified: true,
                        ..Default::default()
                    },
                    error: Some("Withdrawn".into()),
                })
                .unwrap();
            view.drain(&mut cx, &mut Scope::empty());
            assert!(matches!(
                view.selected.as_ref().unwrap().status,
                EntryStatus::Unavailable(_)
            ));
            assert!(!view.confirming && !view.installing);
        }
        makepad_widgets::widget_async::leave_isolate(&mut cx, entered);
        drop(root);
        cx.free_splash_vm(vm_id);
    }
    #[test]
    fn missing_consent_and_unavailable_apps_cannot_activate() {
        let mut entry = catalog::preview_entries().remove(0);
        assert!(can_activate(&entry));
        entry.kind = CatalogKind::Live;
        entry.status = EntryStatus::Available;
        assert!(!can_activate(&entry));
        assert_eq!(button_text(&entry), "Refresh");
        entry.status = EntryStatus::Unavailable("Withdrawn".into());
        assert!(!can_activate(&entry));
    }
}
