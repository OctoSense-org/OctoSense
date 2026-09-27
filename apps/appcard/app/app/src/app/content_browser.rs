//! W04 / M2 — ContentBrowser screen.
//!
//! `workstreams/W04-sessions-tasks-files.md` § "Content browser":
//! `PortalList` grid + filter bar over `GET /api/my/content`
//! (octos-cli `auth_handlers.rs:1122`, **Locked**). Reads
//! `AppState.files`; click on a card emits `ContentAction::Open(FileHandle)`
//! consumed by `crate::app::viewers`. Mirrors
//! `octos-web/src/components/content-browser.tsx`. Plumbing pattern
//! lifted from `app/src/app/sessions.rs`. Wire row shape per
//! `octos-app-transport::rest::mod.rs:84`.

use std::sync::{LazyLock, RwLock};

use makepad_widgets::*;
use octos_app_store::files::{FileHandle, FileKind, FileMeta};
use octos_app_store::state::{AppState, SnapshotEvent};
use octos_app_transport::rest::{MyContentQuery, MyContentRow, RestClient};

use crate::app::sessions::APP_STATE;

/// Cross-thread action posted by `hydrate_content`. App folds into store.
#[derive(Debug)]
pub enum ContentAction {
    Hydrated(Vec<FileMeta>),
    Failed(String),
    Open(FileHandle),
}

/// Local UI state — filter, search, last error. Read by `draw_walk`,
/// written from `App::handle_actions`. Mirrors `sessions.rs:39`.
#[derive(Debug, Default)]
pub struct ContentBrowserState {
    pub filter: ContentFilter,
    pub search: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ContentFilter {
    #[default]
    All,
    Image,
    Audio,
    Video,
    Markdown,
    Pdf,
    Other,
}

impl ContentFilter {
    /// Map dropdown index → variant. Order matches DSL `labels:`.
    pub fn from_dropdown_index(i: usize) -> Self {
        match i {
            1 => Self::Image,
            2 => Self::Audio,
            3 => Self::Video,
            4 => Self::Markdown,
            5 => Self::Pdf,
            6 => Self::Other,
            _ => Self::All,
        }
    }
    fn allows(&self, kind: FileKind) -> bool {
        match self {
            Self::All => true,
            Self::Image => matches!(kind, FileKind::Image),
            Self::Audio => matches!(kind, FileKind::Audio),
            Self::Video => matches!(kind, FileKind::Video),
            Self::Markdown => matches!(kind, FileKind::Markdown),
            Self::Pdf => matches!(kind, FileKind::Pdf),
            Self::Other => matches!(kind, FileKind::Other),
        }
    }
    /// Maps to server `category` query param (auth_handlers.rs ContentQuery).
    pub fn server_kind(&self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Image => Some("image"),
            Self::Audio => Some("audio"),
            Self::Video => Some("video"),
            Self::Markdown => Some("report"),
            Self::Pdf => Some("other"),
            Self::Other => Some("other"),
        }
    }
}

pub static CONTENT_STATE: LazyLock<RwLock<ContentBrowserState>> =
    LazyLock::new(|| RwLock::new(ContentBrowserState::default()));

/// Project a wire row into `FileMeta`. Server `category` maps to `FileKind`.
pub fn project_row(row: MyContentRow) -> FileMeta {
    let name = row.title.clone().unwrap_or_else(|| row.id.clone());
    let (content_type, kind_override) = match row.kind.as_str() {
        "image" => ("image/*", Some(FileKind::Image)),
        "audio" => ("audio/*", Some(FileKind::Audio)),
        "video" => ("video/*", Some(FileKind::Video)),
        "report" => ("text/markdown", Some(FileKind::Markdown)),
        "other" => ("application/octet-stream", Some(FileKind::Other)),
        _ => ("application/octet-stream", None),
    };
    let mut meta = FileMeta::new(
        FileHandle::from(row.id), content_type, 0, name,
    );
    if let Some(k) = kind_override { meta.kind = k; }
    meta
}

/// Spawn a thread + tokio runtime to hit `RestClient::my_content`.
/// Mirrors `sessions.rs::hydrate_sessions`.
pub fn hydrate_content(client: RestClient, query: MyContentQuery) {
    let _ = std::thread::Builder::new()
        .name("octos-content-hydrate".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    Cx::post_action(ContentAction::Failed(format!("spawn tokio runtime: {e}")));
                    return;
                }
            };
            match rt.block_on(async { client.my_content(query).await }) {
                Ok(envelope) => Cx::post_action(ContentAction::Hydrated(
                    envelope.entries.into_iter().map(project_row).collect()
                )),
                Err(e) => Cx::post_action(ContentAction::Failed(format!("{e}"))),
            }
        });
}

struct CardRow {
    handle: FileHandle,
    title: String,
    kind: FileKind,
    size: String,
}

impl CardRow {
    fn from_meta(meta: &FileMeta) -> Self {
        let title = if meta.name.is_empty() {
            meta.handle.as_str().to_owned()
        } else {
            meta.name.clone()
        };
        Self {
            handle: meta.handle.clone(),
            title,
            kind: meta.kind,
            size: format_size(meta.size_bytes),
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes == 0 { return String::new(); }
    if bytes < 1024 { return format!("{bytes} B"); }
    let kb = bytes as f64 / 1024.0;
    if kb < 1024.0 { return format!("{kb:.1} KB"); }
    format!("{:.1} MB", kb / 1024.0)
}

fn kind_glyph(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Image => "🖼",  FileKind::Audio => "🎵",
        FileKind::Video => "🎬",  FileKind::Markdown => "📄",
        FileKind::Pdf => "📕",    FileKind::Other => "📎",
    }
}

fn kind_label(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Image => "image",       FileKind::Audio => "audio",
        FileKind::Video => "video",       FileKind::Markdown => "markdown",
        FileKind::Pdf => "pdf",           FileKind::Other => "other",
    }
}

fn collect_rows(state: &AppState) -> (Vec<CardRow>, Option<String>) {
    let cs = match CONTENT_STATE.read() {
        Ok(g) => g,
        Err(_) => return (Vec::new(), None),
    };
    let q = cs.search.trim().to_ascii_lowercase();
    let mut rows: Vec<CardRow> = state
        .files
        .values()
        .filter(|m| cs.filter.allows(m.kind))
        .filter(|m| q.is_empty() || m.name.to_ascii_lowercase().contains(&q))
        .map(CardRow::from_meta)
        .collect();
    rows.sort_by(|a, b| {
        a.title
            .cmp(&b.title)
            .then(a.handle.as_str().cmp(b.handle.as_str()))
    });
    (rows, cs.last_error.clone())
}

/// Replace `state.files` with a fresh REST snapshot. App calls on
/// `ContentAction::Hydrated` so a re-query produces a clean view.
pub fn fold_hydrated(state: &mut AppState, metas: Vec<FileMeta>) {
    state.files.clear();
    for m in metas {
        octos_app_store::state::reduce(
            state,
            octos_app_store::state::Event::Snapshot(SnapshotEvent::FileMetaHydrated(m)),
        );
    }
}

/// `ContentBrowser` widget. Read-only `View` wrapper; filter / search live
/// in `CONTENT_STATE`; APP_STATE.files carries the row data.
#[derive(Script, ScriptHook, Widget)]
pub struct ContentBrowser {
    #[deref]
    view: View,
}

impl Widget for ContentBrowser {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let (rows, err) = {
            let state = match APP_STATE.read() {
                Ok(g) => g,
                Err(_) => return DrawStep::done(),
            };
            collect_rows(&state)
        };

        let empty_label = self.view.label(cx, ids!(empty_state_label));
        let empty_body = if let Some(e) = err.as_deref() {
            format!("Failed to load content: {e}")
        } else {
            "No files yet. Generated artifacts will appear here.".to_owned()
        };
        empty_label.set_text(cx, &empty_body);
        self.view
            .view(cx, ids!(empty_state))
            .set_visible(cx, rows.is_empty());

        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.as_portal_list().borrow_mut() {
                list.set_item_range(cx, 0, rows.len());
                while let Some(item_id) = list.next_visible_item(cx) {
                    let Some(row) = rows.get(item_id) else { continue };
                    let card = list.item(cx, item_id, id!(ContentCard));
                    card.label(cx, ids!(card_icon))
                        .set_text(cx, kind_glyph(row.kind));
                    card.label(cx, ids!(card_title)).set_text(cx, &row.title);
                    card.label(cx, ids!(card_kind))
                        .set_text(cx, kind_label(row.kind));
                    let size_label = card.label(cx, ids!(card_size));
                    size_label.set_text(cx, &row.size);
                    size_label.set_visible(cx, !row.size.is_empty());
                    card.draw_all_unscoped(cx);
                }
            }
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            let list = self.view.portal_list(cx, ids!(grid));
            if !list.any_items_with_actions(actions) {
                return;
            }
            let ordered: Vec<FileHandle> = {
                let state = match APP_STATE.read() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                let (rows, _) = collect_rows(&state);
                rows.into_iter().map(|r| r.handle).collect()
            };
            for (item_id, item) in list.items_with_actions(actions) {
                let Some(handle) = ordered.get(item_id).cloned() else { continue };
                if item.button(cx, ids!(card_click)).clicked(actions) {
                    Cx::post_action(ContentAction::Open(handle));
                }
            }
        }
    }
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.ContentBrowser = #(crate::app::content_browser::ContentBrowser::register_widget(vm)) {
        width: Fill height: Fill flow: Down spacing: 12
        padding: Inset{left: 8 right: 8 top: 4 bottom: 4}

        toolbar := View {
            width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 8
            Label {
                text: "Content" margin: Inset{right: 12}
                draw_text.color: #xF3E3C7
                draw_text.text_style.font_size: 16
            }
            content_filter_dropdown := DropDown {
                width: 160 height: 32
                popup_menu_position: PopupMenuPosition.BelowInput
                labels: ["All", "Images", "Audio", "Video", "Markdown", "PDF", "Other"]
                draw_text +: { color: #xF3E3C7 text_style +: { font_size: 11 } }
                draw_bg +: {
                    color: #x08251ED8 color_hover: #x12382FEE
                    border_color: #xEAD8B832 border_size: 1.0 border_radius: 10.0
                    arrow_color: #xF3E3C7
                }
            }
            content_search_input := TextInput {
                width: Fill height: 32 empty_text: "Search filename…"
                draw_bg +: {
                    color: #x06241DCC color_hover: #x0A2D24DD color_focus: #x0F362DEE
                    color_empty: #x06241DCC border_color: #x72E4FF44
                    border_size: 1.0 border_radius: 10.0
                }
                draw_text +: {
                    color: #xF3E3C7 color_empty: #xF3E3C766
                    text_style +: { font_size: 12 }
                }
            }
            content_refresh_button := ButtonFlat {
                width: Fit height: 32 text: "Refresh"
                padding: Inset{left: 12 right: 12}
                draw_text +: { color: #xF3E3C7 text_style +: { font_size: 11 } }
                draw_bg +: {
                    color: #x08251EC8 color_hover: #x123B31DD
                    border_color: #xEAD8B83A border_size: 1.0 border_radius: 10.0
                }
            }
        }

        grid := PortalList {
            width: Fill height: Fill flow: Down
            drag_scrolling: true auto_tail: false selectable: true

            ContentCard := RoundedView {
                width: Fill height: Fit flow: Right
                margin: Inset{top: 2 bottom: 2}
                padding: Inset{left: 10 top: 10 right: 10 bottom: 10}
                spacing: 10 align: Align{y: 0.5}
                show_bg: true
                draw_bg +: { color: #x0B2A22A0 color_hover: #x123B31DD radius: 10.0 }

                card_icon := Label {
                    width: 24 height: Fit text: "📎"
                    draw_text.color: #xF6BE63
                    draw_text.text_style.font_size: 16
                }
                card_click := ButtonFlat {
                    width: Fill height: Fit
                    align: Align{x: 0.0 y: 0.5}
                    flow: Down spacing: 2 padding: 0 text: ""
                    draw_text +: { color: #00000000 }
                    draw_bg +: {
                        color: #00000000 color_hover: #00000000
                        border_size: 0.0 border_radius: 0.0
                    }
                    card_title := Label {
                        width: Fill height: Fit text: ""
                        draw_text.color: #xF3E3C7
                        draw_text.text_style.font_size: 12
                    }
                    card_meta_row := View {
                        width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                        card_kind := Label {
                            width: Fit height: Fit text: ""
                            margin: Inset{right: 6}
                            draw_text.color: #x72E4FF
                            draw_text.text_style.font_size: 10
                        }
                        card_size := Label {
                            width: Fit height: Fit text: ""
                            draw_text.color: #xCDBF9F88
                            draw_text.text_style.font_size: 10
                        }
                    }
                }
            }
        }

        empty_state := View {
            width: Fill height: Fit flow: Down align: Align{x: 0.5 y: 0.5}
            margin: Inset{top: 24}
            empty_state_label := Label {
                width: Fit text: ""
                draw_text.color: #xCDBF9FAA
                draw_text.text_style.font_size: 12
            }
        }
    }
}
