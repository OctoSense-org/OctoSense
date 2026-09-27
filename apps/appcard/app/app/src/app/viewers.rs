//! W04 / M2 — File viewers (overlay pane).
//!
//! `workstreams/W04-sessions-tasks-files.md` § "File viewers". Four
//! viewers — image album, markdown, audio, video. Consume `FileMeta`
//! from `AppState.files`; URLs via `RestClient::file_url`
//! (octos-app-transport rest/mod.rs:266). Audio / video defer to OS via
//! `robius_open`; markdown fetches body once with `reqwest` and renders
//! via Makepad's `Markdown`. Overlay toggles via `set_visible` mirroring
//! `app/src/main.rs:1433`'s `login_overlay`. Web ref:
//! `octos-web/src/components/viewers/*.tsx`.

use std::sync::{LazyLock, RwLock};

use makepad_widgets::*;
use octos_app_store::files::{FileHandle, FileKind};
use octos_app_transport::rest::RestClient;
use octos_app_transport::FileHandle as TransportFileHandle;

use crate::app::sessions::APP_STATE;

/// Bridge store FileHandle (files.rs:12) → transport FileHandle (lib.rs:64).
/// Same opaque string; duplicate type exists to break store ↔ transport cycle.
fn to_transport_handle(h: &FileHandle) -> TransportFileHandle {
    TransportFileHandle(h.as_str().to_owned())
}

/// What's currently open in the overlay.
#[derive(Debug, Clone, Default)]
pub enum OpenViewer {
    #[default]
    Closed,
    ImageAlbum { handles: Vec<FileHandle>, active: usize },
    Markdown { handle: FileHandle },
    Audio { handle: FileHandle },
    Video { handle: FileHandle },
    /// PDF / Other / unknown — generic OS-handoff card.
    Generic { handle: FileHandle },
}

impl OpenViewer {
    pub fn is_open(&self) -> bool { !matches!(self, Self::Closed) }
    pub fn focus_handle(&self) -> Option<&FileHandle> {
        match self {
            Self::Closed => None,
            Self::ImageAlbum { handles, active } => handles.get(*active),
            Self::Markdown { handle }
            | Self::Audio { handle }
            | Self::Video { handle }
            | Self::Generic { handle } => Some(handle),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ViewerState {
    pub open: OpenViewer,
    pub markdown_cache: std::collections::HashMap<FileHandle, String>,
    pub last_error: Option<String>,
}

pub static VIEWER_STATE: LazyLock<RwLock<ViewerState>> =
    LazyLock::new(|| RwLock::new(ViewerState::default()));

#[derive(Debug)]
pub enum ViewerAction {
    MarkdownLoaded { handle: FileHandle, body: String },
    MarkdownFailed { handle: FileHandle, error: String },
    Close,
    AlbumStep(i32),
    OpenInOs(FileHandle),
}

/// Pick the right viewer for `handle` based on `FileMeta.kind`.
pub fn viewer_for(handle: &FileHandle) -> OpenViewer {
    let kind = display_kind(handle).unwrap_or(FileKind::Other);
    let h = handle.clone();
    match kind {
        FileKind::Image => {
            let handles = collect_image_handles();
            let active = handles.iter().position(|x| x == handle).unwrap_or(0);
            if handles.is_empty() {
                OpenViewer::ImageAlbum { handles: vec![h], active: 0 }
            } else {
                OpenViewer::ImageAlbum { handles, active }
            }
        }
        FileKind::Markdown => OpenViewer::Markdown { handle: h },
        FileKind::Audio => OpenViewer::Audio { handle: h },
        FileKind::Video => OpenViewer::Video { handle: h },
        FileKind::Pdf | FileKind::Other => OpenViewer::Generic { handle: h },
    }
}

fn collect_image_handles() -> Vec<FileHandle> {
    let Ok(state) = APP_STATE.read() else { return Vec::new() };
    let mut h: Vec<FileHandle> = state.files.values()
        .filter(|m| matches!(m.kind, FileKind::Image))
        .map(|m| m.handle.clone()).collect();
    h.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    h
}

fn display_name(handle: &FileHandle) -> String {
    APP_STATE.read().ok()
        .and_then(|s| s.files.get(handle).map(|m| m.name.clone()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| handle.as_str().to_owned())
}

fn display_kind(handle: &FileHandle) -> Option<FileKind> {
    APP_STATE.read().ok().and_then(|s| s.files.get(handle).map(|m| m.kind))
}

/// Build a token-bearing URL for the OS handoff button.
pub fn url_for(client: &RestClient, handle: &FileHandle) -> Option<url::Url> {
    client.file_url(&to_transport_handle(handle)).ok().map(|r| r.with_token)
}

/// Fetch markdown body off-thread; post `ViewerAction::MarkdownLoaded` /
/// `MarkdownFailed` back to the UI.
pub fn fetch_markdown(client: RestClient, handle: FileHandle) {
    let _ = std::thread::Builder::new()
        .name("octos-viewer-markdown".into())
        .spawn(move || {
            let fail = |h: FileHandle, error: String| {
                Cx::post_action(ViewerAction::MarkdownFailed { handle: h, error });
            };
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => return fail(handle, format!("tokio runtime: {e}")),
            };
            let url = match client.file_url(&to_transport_handle(&handle)) {
                Ok(r) => r.bare,
                Err(e) => return fail(handle, format!("file_url: {e}")),
            };
            let token = client.token.expose().to_owned();
            let res: Result<String, String> = rt.block_on(async move {
                reqwest::Client::new().get(url).bearer_auth(token).send().await
                    .map_err(|e| format!("send: {e}"))?
                    .text().await.map_err(|e| format!("text: {e}"))
            });
            match res {
                Ok(body) => Cx::post_action(ViewerAction::MarkdownLoaded { handle, body }),
                Err(error) => fail(handle, error),
            }
        });
}

/// Outer overlay widget — picks an inner pane via `VIEWER_STATE.open`.
#[derive(Script, ScriptHook, Widget)]
pub struct ViewerOverlay {
    #[deref]
    view: View,
}

struct OverlaySnapshot {
    open: OpenViewer,
    markdown_body: Option<String>,
    last_error: Option<String>,
}

fn snapshot_overlay() -> OverlaySnapshot {
    let Ok(s) = VIEWER_STATE.read() else {
        return OverlaySnapshot { open: OpenViewer::Closed, markdown_body: None, last_error: None };
    };
    let markdown_body = if let OpenViewer::Markdown { handle } = &s.open {
        s.markdown_cache.get(handle).cloned()
    } else { None };
    OverlaySnapshot { open: s.open.clone(), markdown_body, last_error: s.last_error.clone() }
}

impl Widget for ViewerOverlay {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let snap = snapshot_overlay();
        let open = snap.open.is_open();
        self.view.set_visible(cx, open);
        if !open {
            return self.view.draw_walk(cx, scope, walk);
        }

        let panes = [
            (ids!(viewer_image_pane), matches!(snap.open, OpenViewer::ImageAlbum { .. })),
            (ids!(viewer_markdown_pane), matches!(snap.open, OpenViewer::Markdown { .. })),
            (ids!(viewer_audio_pane),
             matches!(snap.open, OpenViewer::Audio { .. } | OpenViewer::Generic { .. })),
            (ids!(viewer_video_pane), matches!(snap.open, OpenViewer::Video { .. })),
        ];
        for (id, on) in panes {
            self.view.view(cx, id).set_visible(cx, on);
        }

        match &snap.open {
            OpenViewer::ImageAlbum { handles, active } => {
                let title = handles.get(*active).map(display_name).unwrap_or_default();
                self.view.label(cx, ids!(viewer_image_title)).set_text(cx, &title);
                self.view.label(cx, ids!(viewer_image_counter))
                    .set_text(cx, &format!("{} / {}", active + 1, handles.len()));
                self.view.label(cx, ids!(viewer_image_caption)).set_text(cx,
                    "Open in OS to view full-resolution. Native fetch lands later.");
            }
            OpenViewer::Markdown { handle } => {
                self.view.label(cx, ids!(viewer_markdown_title)).set_text(cx, &display_name(handle));
                let body = snap.markdown_body.clone().unwrap_or_else(|| {
                    if let Some(e) = snap.last_error.as_deref() { format!("> Failed to load: {e}") }
                    else { "_Loading…_".to_owned() }
                });
                self.view.markdown(cx, ids!(viewer_markdown_body)).set_text(cx, &body);
            }
            OpenViewer::Audio { handle } | OpenViewer::Generic { handle } => {
                let kind_label = match display_kind(handle).unwrap_or(FileKind::Other) {
                    FileKind::Audio => "Audio", FileKind::Pdf => "PDF",
                    FileKind::Video => "Video", _ => "File",
                };
                self.view.label(cx, ids!(viewer_audio_title)).set_text(cx, &display_name(handle));
                self.view.label(cx, ids!(viewer_audio_caption)).set_text(cx,
                    &format!("{kind_label}. Click \"Open in OS\" to launch your default app."));
            }
            OpenViewer::Video { handle } => {
                self.view.label(cx, ids!(viewer_video_title)).set_text(cx, &display_name(handle));
                self.view.label(cx, ids!(viewer_video_caption)).set_text(cx,
                    "Video preview. Click \"Open in OS\" to play (no native H.264 in Makepad).");
            }
            OpenViewer::Closed => {}
        }
        self.view.draw_walk(cx, scope, walk)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        let Event::Actions(actions) = event else { return };
        let click = |id| self.view.button(cx, id).clicked(actions);
        if click(ids!(viewer_close_button)) { Cx::post_action(ViewerAction::Close); }
        if click(ids!(viewer_prev_button)) { Cx::post_action(ViewerAction::AlbumStep(-1)); }
        if click(ids!(viewer_next_button)) { Cx::post_action(ViewerAction::AlbumStep(1)); }
        let open_clicked = click(ids!(viewer_image_open_button))
            || click(ids!(viewer_audio_open_button))
            || click(ids!(viewer_video_open_button))
            || click(ids!(viewer_open_in_os_button));
        if open_clicked {
            if let Some(h) = VIEWER_STATE.read().ok().and_then(|s| s.open.focus_handle().cloned()) {
                Cx::post_action(ViewerAction::OpenInOs(h));
            }
        }
    }
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    let OverlayButton = ButtonFlat {
        height: 34 padding: Inset{left: 16 right: 16 top: 0 bottom: 0}
        draw_text +: { color: #xF3E3C7 text_style +: { font_size: 12 } }
        draw_bg +: {
            color: #x08251EC8 color_hover: #x123B31DD
            border_color: #xEAD8B83A border_size: 1.0 border_radius: 10.0
        }
    }
    let TitleLabel = Label {
        width: Fill height: Fit text: ""
        draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 14
    }
    let SubLabel = Label {
        width: Fill height: Fit text: ""
        draw_text.color: #xCDBF9FAA draw_text.text_style.font_size: 11
    }
    let RowSpread = View {
        width: Fill height: Fit flow: Right align: Align{y: 0.5}
        spacing: 8 margin: Inset{top: 12}
    }

    mod.widgets.ViewerOverlay = #(crate::app::viewers::ViewerOverlay::register_widget(vm)) {
        width: Fill height: Fill flow: Overlay visible: false
        show_bg: true
        draw_bg +: { color: #x040A08D8 }

        viewer_panel := GlassPanel {
            width: Fill{min: 480 max: 1100}
            height: Fill{min: 360 max: 800}
            margin: Inset{left: 40 top: 40 right: 40 bottom: 40}
            new_batch: true flow: Down
            padding: Inset{left: 22 top: 18 right: 22 bottom: 18} spacing: 12
            draw_bg +: {
                tint_color: #x0B3B31 tint_alpha: 0.94
                border_color: #x72E4FF border_alpha: 0.32 border_width: 1.0
                corner_radius: 18.0
                halo_color: #x72E4FF halo_strength: 0.10 halo_radius: 6.0
                highlight_strength: 0.22 highlight_band_height: 50.0
                noise_strength: 0.004
            }

            viewer_header := View {
                width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 8
                Label {
                    text: "Viewer"
                    draw_text.color: #x72E4FF
                    draw_text.text_style.font_size: 11
                }
                View { width: Fill height: 1 }
                viewer_close_button := OverlayButton { text: "Close" }
            }

            viewer_image_pane := View {
                width: Fill height: Fill flow: Down spacing: 10 visible: false
                viewer_image_title := TitleLabel {}
                viewer_image_counter := SubLabel {}
                viewer_image_caption := SubLabel {}
                RowSpread {
                    viewer_prev_button := OverlayButton { text: "← Prev" }
                    viewer_next_button := OverlayButton { text: "Next →" }
                    View { width: Fill height: 1 }
                    viewer_image_open_button := OverlayButton { text: "Open in OS" }
                }
            }

            viewer_markdown_pane := View {
                width: Fill height: Fill flow: Down spacing: 8 visible: false
                viewer_markdown_title := TitleLabel {}
                viewer_markdown_scroll := ScrollYView {
                    width: Fill height: Fill
                    viewer_markdown_body := Markdown {
                        width: Fill height: Fit body: "_Loading…_"
                        selectable: true use_code_block_widget: true
                    }
                }
            }

            viewer_audio_pane := View {
                width: Fill height: Fit flow: Down spacing: 8 visible: false
                viewer_audio_title := TitleLabel {}
                viewer_audio_caption := SubLabel {}
                RowSpread {
                    viewer_audio_open_button := OverlayButton { text: "Open in OS" }
                    View { width: Fill height: 1 }
                    viewer_open_in_os_button := OverlayButton { text: "Download" }
                }
            }

            viewer_video_pane := View {
                width: Fill height: Fit flow: Down spacing: 8 visible: false
                viewer_video_title := TitleLabel {}
                viewer_video_caption := SubLabel {}
                RowSpread {
                    viewer_video_open_button := OverlayButton { text: "Open in OS" }
                }
            }
        }
    }
}
