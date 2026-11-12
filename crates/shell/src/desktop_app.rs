//! Desktop style orchestration. Application state stays in the existing clients;
//! shell presentation and the compositor's frozen framebuffer change together.
use crate::desk::ChromeHit;
use crate::desktop::{dark_chrome, DesktopShelf, DesktopStyle, ShelfHit, SPECS};
use crate::*;

/// Match the browser's initial page palette: light Omarchy themes, classic
/// desktops that have no dark variant, and each style's selected appearance.
pub(super) fn browser_appearance(style: DesktopStyle, dark: bool, omarchy_source: &str) -> bool {
    if style == DesktopStyle::Omarchy {
        return scan_theme_color(omarchy_source, "background")
            .map(|c| 0.2126*c.x + 0.7152*c.y + 0.0722*c.z < 0.5)
            .unwrap_or(true);
    }
    dark_chrome(style, dark)
}

/// The bar's parent strip: transparent under glass, so the material sits
/// on the wallpaper with no flat wash beneath; the theme ground otherwise.
pub(super) fn bar_strip_color(material: &shell::MaterialTokens, ground: Vec4f, mac_family: bool) -> Vec4f {
    // A Mac's menu bar is translucent over the wallpaper, glass or not.
    if material.is_glass() || mac_family { Vec4f::default() } else { ground }
}

impl App {
    /// What a style's sheet says the chrome paints with: its material and
    /// its palette roles, for `WmState` to carry and the desk and kits to
    /// read. Startup and every switch come through here, so a sheet that
    /// grows a material block or a role is honoured wherever it loads; a
    /// material line that does not read is logged and the rest still applies.
    pub(super) fn chrome_from_sheet(sheet: &desktop_style::StyleSheet) -> (shell::MaterialTokens, theme::StyleRoles) {
        let (material, problems) = theme::scan_material(&sheet.theme);
        for problem in &problems {
            log!("wm: style {} material {}", sheet.name, problem);
        }
        (material, theme::scan_style_roles(&sheet.theme))
    }

    /// Hand the material to every shell surface. Nothing is stored here —
    /// `WmState.material` already holds it — and nothing redraws: the
    /// callers redraw everything after. The bar's parent strip is the one
    /// fill outside a kit: under glass it goes transparent so the material
    /// sits on the wallpaper with no flat wash beneath; under flat it is
    /// `mod.wm_theme.background` again, as the DSL resolved it.
    pub(super) fn apply_material_to_chrome(&mut self, cx: &mut Cx, material: shell::MaterialTokens, palette: Option<shell::ShellPalette>) {
        if let Some(mut w) = self.shell_bar(cx).borrow_mut::<shell::bar::ShellBar>() {
            w.set_material(material, palette);
        }
        if let Some(mut w) = self.ui.widget(cx, ids!(shell_menu)).borrow_mut::<ShellMenu>() {
            w.set_material(material, palette);
        }
        if let Some(mut w) = self.ui.widget(cx, ids!(shell_panel)).borrow_mut::<shell::panels::ShellPanel>() {
            w.set_material(material, palette);
        }
        if let Some(mut w) = self.ui.widget(cx, ids!(shell_notes)).borrow_mut::<shell::notifications::ShellNotifications>() {
            w.set_material(material, palette);
        }
        if let Some(mut w) = self.ui.widget(cx, ids!(shell_osd)).borrow_mut::<shell::osd::ShellOsd>() {
            w.set_material(material, palette);
        }
        if let Some(mut w) = self.ui.widget(cx, ids!(shell_glance)).borrow_mut::<crate::glance_panel::ShellGlancePanel>() {
            w.set_material(material, palette);
        }
        if let Some(mut w) = self.ui.widget(cx, ids!(shell_glance_sheet)).borrow_mut::<crate::glance_sheet::ShellGlanceSheet>() {
            w.set_material(material, palette);
        }
        // The chat panes: the system chat and "Ask <app>".
        for pane in [ids!(shell_system_chat), ids!(shell_app_chat)] {
            if let Some(mut w) = self.ui.widget(cx, pane).borrow_mut::<crate::system_chat::view::ShellSystemChat>() {
                w.set_material(material, palette);
            }
        }
        // The approval sheets and the Approvals page, after the chat panes
        // in the tree: lifted with the same material, they stay over them.
        if let Some(mut w) = self.ui.widget(cx, ids!(shell_approvals)).borrow_mut::<crate::approvals::view::ShellApprovals>() {
            w.set_material(material, palette);
        }
        if let Some(mut w) = self.ui.widget(cx, ids!(shell_approvals_settings)).borrow_mut::<crate::approvals::settings_page::ShellApprovalsSettings>() {
            w.set_material(material, palette);
        }
        if let Some(mut w) = self.ui.widget(cx, ids!(shell_ai_pane)).borrow_mut::<ShellAiPane>() {
            w.set_material(material, palette);
        }
        let mac_family = self.state_mut().style.target.mac_family();
        let bar_strip = bar_strip_color(&material, self.state_mut().bar_ground, mac_family);
        let mut bar = self.bar_strip(cx);
        script_apply_eval!(cx, bar, { draw_bg +: { color: #(bar_strip) } });
    }

    pub(super) fn set_desktop_style(&mut self, cx: &mut Cx, style: DesktopStyle) {
        crate::shell::menu::MAC_KEYS.store(style == DesktopStyle::Macos, std::sync::atomic::Ordering::Relaxed);
        // The Mac's pointer and menu conventions (`desktop_style::is_macos`) reach
        // the shell's chrome and the apps it hosts in process: the shell's own heap
        // keeps its startup sheet, which would otherwise answer for them.
        desktop_style::set_process_style(cx, (style == DesktopStyle::Macos).then_some(desktop_style::DesktopStyle::Macos));
        let previous = self.state_mut().style.target;
        let changes_size = previous.mobile() != style.mobile() || (style.mobile() && previous != style);
        self.close_shell_menu(cx);
        if let Some(mut scene) = self
            .ui
            .widget(cx, ids!(scene))
            .borrow_mut::<scene::WmScene>()
        {
            if changes_size { scene.cut(cx); } else { scene.transition(cx); }
        }
        self.drag = None;
        self.div_drag = None;
        self.state_mut().snap.clear();
        cx.stop_timer(self.snap_hover_timer);
        let area = self.desk_area(cx);
        let dark = self.state_mut().style.dark;
        let phone_theme = self.state_mut().phone.theme.filter(|_| style.mobile());
        let sheet = phone_theme.map(|choice| choice.sheet(style, dark))
            .unwrap_or_else(|| octosense::style::load_sheet(style, dark));
        if let Some(mut desk) = self.desk(cx).borrow_mut::<WmDesk>() {desk.set_startup_style(cx, &sheet);}
        let sheet_name = if style == DesktopStyle::OctoSense {
            if dark { "octosense-dark" } else { "octosense" }.to_string()
        } else { sheet.name.clone() };
        // New child processes need the recognized upstream family at startup.
        // Once connected, they receive the full custom palette too.
        host::set_child_env("MAKEPAD_WIDGET_STYLE", std::ffi::OsStr::new(&sheet.name));
        app_icon::install(cx, style.framework(), &sheet.icons);
        let (material, roles) = Self::chrome_from_sheet(&sheet);
        let state = self.state_mut();
        state.material = material;
        state.roles = roles;
        state.dragging.clear();
        state.style.select(style);
        // An open updater takes the style's name for its window, or gives a Mac's back.
        if let Some(def) = clients::find_app("updater") {
            let label = clients::styled_label(style == DesktopStyle::Macos, &def.id, &def.label);
            for slot in state.clients.values_mut().filter(|slot| slot.app == def.id) {
                slot.label = label.clone();
                slot.title = label.clone();
            }
        }
        state.phone.shade.dark = dark;
        // Glance cards that name no theme take the shell's mode.
        crate::glance_card::set_dark(dark);
        // Toasts draw an app's icon as the dock does, in this style.
        if let Some(mut notes) = self.ui.widget(cx, ids!(shell_notes)).borrow_mut::<crate::shell::notifications::ShellNotifications>() {
            notes.icon_style = style;
        }
        // Extra module windows are a desktop thing: the phone shell's apps are
        // full-screen, so their windows close (the apps hear it, as from a person).
        self.module_host.set_extra_windows(!style.mobile());
        if style.mobile() {
            let windows: Vec<_> = self.module_windows.keys().copied().collect();
            for window in windows {
                self.remove_client(cx, window);
            }
        }
        let state = self.state_mut();
        if changes_size { state.style.step(1.0); }
        state.layout.desktop.enabled = style.floating();
        for c in state.layout.all_clients() {
            state.layout.desktop.ensure(c, area);
        }
        // Send the complete stylesheet, so already-running and warm applications
        // receive exactly the version selected in the WM, even across hosts.
        let json = sheet.to_json();
        for slot in state.clients.values().filter(|slot| slot.ready) {
            if let Some(sender) = &slot.sender {
                send_to_app(sender, vec![StudioToApp::Custom(json.clone())]);
            }
        }
        let palette = phone_theme.map(|choice| choice.palette(dark).shell())
            .or_else(|| style.mac_family().then(|| theme::scan_shell_palette(&sheet.theme)));
        self.apply_material_to_chrome(cx, material, palette);
        self.module_host.apply_style(cx, &sheet);
        crate::glance_style::select(cx, &sheet);
        self.stylesheet = Some(sheet);
        let omarchy_source = if style == DesktopStyle::Omarchy {
            theme::load_theme_source(&self.state_mut().theme_name)
        } else { String::new() };
        self.refresh_warm_browser_appearance(cx, browser_appearance(style, dark, &omarchy_source));
        self.style_time = 0.0;
        self.style_frame = cx.new_next_frame();
        if let Some(mut menu) = self
            .ui
            .widget(cx, ids!(shell_menu))
            .borrow_mut::<ShellMenu>()
        {
            menu.desktop_style = style;
            menu.dark = dark;
            menu.roles = roles;
        }
        // Wallpaper is part of the framebuffer crossfade.
        self.apply_desk_ground(cx, style, dark);
        self.update_bar(cx);
        self.configure_phone_mode(cx,previous,style);
        self.redraw_all(cx);
        log!(
            "wm: desktop style {} applied to {} clients and {} modules",
            sheet_name,
            self.state_mut().clients.len(),
            self.module_host.len()
        );
    }
    pub(super) fn send_desktop_style(&mut self, client: ClientId) {
        let Some(json) = self.stylesheet.as_ref().map(|s| s.to_json()) else {
            return;
        };
        if let Some(sender) = self
            .state_mut()
            .clients
            .get(&client)
            .filter(|s| s.ready)
            .and_then(|s| s.sender.as_ref())
        {
            send_to_app(sender, vec![StudioToApp::Custom(json)]);
        }
    }
    pub(super) fn refresh_warm_browser_appearance(&mut self, cx: &mut Cx, dark: bool) {
        for client in self.warm_pool.set_browser_appearance(dark) {
            log!("wm: retiring warm browser {} for {} appearance", client, if dark {"dark"} else {"light"});
            self.request_close(cx, client);
        }
        // Normal top-up runs after reaping; no new process overlaps the old
        // profile lock, and pages the user has opened stay in their process.
    }
    pub(super) fn toggle_desktop_appearance(&mut self, cx: &mut Cx) {
        self.state_mut().style.dark = !self.state_mut().style.dark;
        let style = self.state_mut().style.target;
        self.set_desktop_style(cx, style);
    }
    #[cfg(not(mobile_only))]
    pub(super) fn open_style_menu(&mut self, cx: &mut Cx) {
        let anchor = if self.state_mut().style.target.mobile() {
            self.bar_strip(cx).widget(cx, ids!(phone_controls)).borrow::<crate::mobile_surface::PhoneSurface>()
                .and_then(|bar| bar.hit_rect(&crate::mobile::PhoneHit::Style))
        } else {
            self.shell_bar(cx).borrow::<crate::shell::bar::ShellBar>()
                .and_then(|bar| bar.module_rect(BarModule::Style))
        };
        self.open_shell_menu(cx, "desktop", MenuSkin::Menu);
        let active = format!("desktop.{}", self.state_mut().style.target.id());
        if let Some(mut menu) = self.ui.widget(cx, ids!(shell_menu)).borrow_mut::<ShellMenu>() {
            menu.anchor = anchor;
            if let Some(index) = menu.model.rows.iter().position(|row| row.target == active) {
                menu.model.sel = index;
            }
        }
    }
    /// A Mac-family bar's system mark: everything the shell offers, as a menu
    /// under the mark, like the Apple menu. Apps open from the Dock's first icon.
    pub(super) fn toggle_system_mark_menu(&mut self, cx: &mut Cx) {
        let open = self.ui.widget(cx, ids!(shell_menu)).borrow::<ShellMenu>().is_some_and(|m| m.is_open());
        if open {
            self.close_shell_menu(cx);
            return;
        }
        let anchor = self.shell_bar(cx).borrow::<crate::shell::bar::ShellBar>().and_then(|bar| bar.module_rect(BarModule::Menu));
        self.open_shell_menu(cx, "", MenuSkin::Menu);
        if let Some(mut menu) = self.ui.widget(cx, ids!(shell_menu)).borrow_mut::<ShellMenu>() {
            menu.anchor = anchor;
        }
    }
    pub(super) fn toggle_launcher(&mut self, cx: &mut Cx) {
        let open = self
            .ui
            .widget(cx, ids!(shell_menu))
            .borrow::<ShellMenu>()
            .is_some_and(|m| m.is_open());
        if open {
            self.close_shell_menu(cx);
        } else {
            let path = match self.state_mut().style.target {
                DesktopStyle::Omarchy => "",
                DesktopStyle::Windows2000 => "start",
                DesktopStyle::NextStep => "workspace",
                _ => "apps",
            };
            self.open_shell_menu(cx, path, MenuSkin::Launcher);
        }
    }
    pub(super) fn minimize_desktop(&mut self, cx: &mut Cx, client: ClientId) {
        let layout = &mut self.state_mut().layout;
        if let Some(w) = layout.desktop.get_mut(client) {
            w.minimized = true;
            w.hidden = false;
        }
        let next = layout
            .desktop
            .windows
            .iter()
            .rev()
            .find(|w| !w.minimized && layout.workspace_of(w.client) == Some(layout.active))
            .map(|w| w.client);
        layout.workspaces[layout.active].focus = next;
        if let Some(next) = next {
            self.focus_client(cx, next);
        }
        self.update_bar(cx);
        self.redraw_all(cx);
    }
    /// A Mac's Hide: the window goes with its app, leaving nothing in the Dock but the
    /// app's own icon. One already minimized stays as it is, and Show All leaves it there.
    pub(super) fn hide_desktop(&mut self, cx: &mut Cx, client: ClientId) {
        if self.state_mut().layout.desktop.minimized(client) {
            return;
        }
        self.minimize_desktop(cx, client);
        if let Some(w) = self.state_mut().layout.desktop.get_mut(client) {
            w.hidden = true;
        }
    }
    /// A window in the macOS style whose app doesn't zoom (`apps::mac_window_fixed`).
    /// One that is large already, from another style, can still come back.
    fn zoom_refused(&mut self, client: ClientId) -> bool {
        let state = self.state_mut();
        state.style.target == desktop::DesktopStyle::Macos
            && state.clients.get(&client).is_some_and(|slot| crate::apps::mac_window_fixed(&slot.app))
            && !state.layout.desktop.get_mut(client).is_some_and(|w| w.fullscreen || w.maximized)
    }
    /// A fixed window whose side edges still size its width (`apps::mac_window_widens`).
    fn widens(&mut self, client: ClientId) -> bool {
        self.state_mut().clients.get(&client).is_some_and(|slot| crate::apps::mac_window_widens(&slot.app))
    }
    /// A Mac's Zoom on a window that only widens: as wide as the room above the Dock,
    /// its height where it was. The same press brings back the frame it had, unless it
    /// was moved or sized meanwhile: then it zooms again, as AppKit's does.
    fn zoom_width(&mut self, cx: &mut Cx, client: ClientId) {
        let area = self.desk_area(cx);
        let room = crate::desktop_layout::room(area, self.dock_overlay(cx, area));
        if let Some(w) = self.state_mut().layout.desktop.get_mut(client) {
            let zoomed = (w.rect.x - room.x).abs() < 0.5 && (w.rect.w - room.w).abs() < 0.5;
            match w.zoomed_from.take() {
                Some(before) if zoomed => w.rect = before,
                _ => {
                    w.zoomed_from = Some(w.rect);
                    let y = w.rect.y.min(room.y + room.h - w.rect.h).max(room.y);
                    w.rect = crate::layout::LRect::new(room.x, y, room.w, w.rect.h);
                }
            }
            w.minimized = false;
        }
        self.focus_client(cx, client);
        self.redraw_all(cx);
    }
    /// macOS's green button: the window covers the desk, the top bar and a
    /// floating dock hide until the pointer comes to their edge, and the
    /// same press brings it back (`DesktopWindow::fullscreen`).
    pub(super) fn fullscreen_desktop(&mut self, cx: &mut Cx, client: ClientId) {
        // A Quick Look panel its ⤢ took full screen goes back to its panel, as its ⤡ puts it.
        if self.preview_cache.is_full_screen(client) {
            self.toggle_preview_full_screen(cx, client);
            return;
        }
        if self.zoom_refused(client) {
            // One that only widens can't fill the screen: its green light zooms its width.
            if self.widens(client) {
                self.zoom_width(cx, client);
            }
            return;
        }
        if let Some(w) = self.state_mut().layout.desktop.get_mut(client) {
            w.fullscreen = !w.fullscreen;
            w.minimized = false;
        }
        self.focus_client(cx, client);
        self.redraw_all(cx);
    }
    pub(super) fn maximize_desktop(&mut self, cx: &mut Cx, client: ClientId) {
        if self.zoom_refused(client) {
            if self.widens(client) {
                self.zoom_width(cx, client);
            }
            return;
        }
        if let Some(w) = self.state_mut().layout.desktop.get_mut(client) {
            w.maximized = !w.maximized;
            w.minimized = false;
        }
        self.focus_client(cx, client);
        self.redraw_all(cx);
    }
    pub(super) fn apply_snap(&mut self, cx: &mut Cx, client: ClientId, zone: crate::snap::Zone) {
        if let Some(w)=self.state_mut().layout.desktop.get_mut(client) {
            w.maximized=zone==crate::snap::Zone::Maximize;
            w.snap=(!w.maximized).then_some(zone);
            w.minimized=false;
        }
        self.state_mut().snap.clear();
        self.focus_client(cx,client);
        self.redraw_all(cx);
        log!("wm: snapped client {} to {:?}",client,zone);
    }
    pub(super) fn snap_event(&mut self, cx: &mut Cx, event: &Event) -> bool {
        if self.state_mut().style.target!=DesktopStyle::Windows || self.drag.is_some() || self.div_drag.is_some() {return false;}
        if let Some(client)=self.state_mut().snap.picker.as_ref().map(|p|p.client) {
            let state=self.state_mut();
            if state.layout.workspace_of(client)!=Some(state.layout.active) || state.layout.desktop.minimized(client) {
                state.snap.clear();cx.stop_timer(self.snap_hover_timer);self.redraw_all(cx);
            }
        }
        if self.snap_hover_timer.is_event(event).is_some() {
            if let Some((client,anchor))=self.state_mut().snap.hovered {
                let area=self.desk_area(cx);
                self.state_mut().snap.picker=Some(crate::snap::Picker::new(client,area,Some(anchor)));
                self.state_mut().snap.area=Some(area);
                self.redraw_all(cx);
            }
            return false;
        }
        if matches!(event,Event::KeyDown(e) if e.key_code==KeyCode::Escape) && self.state_mut().snap.picker.is_some() {
            self.state_mut().snap.clear();cx.stop_timer(self.snap_hover_timer);self.redraw_all(cx);return true;
        }
        let p=match event {Event::MouseMove(e)=>e.abs,Event::MouseDown(e)=>e.abs,Event::MouseUp(e)=>e.abs,_=>return false};
        if matches!(event,Event::MouseDown(_)) {cx.stop_timer(self.snap_hover_timer);}
        let picker=self.state_mut().snap.picker.clone();
        if let Some(picker)=picker {
            if picker.bounds.contains(p) {
                let hit=picker.hit(p);
                if self.state_mut().snap.preview!=hit {self.state_mut().snap.preview=hit;self.redraw_all(cx);}
                if matches!(event,Event::MouseDown(e) if e.button.contains(MouseButton::PRIMARY)) {
                    if let Some(zone)=hit {self.apply_snap(cx,picker.client,zone);}
                }
                return true;
            }
            let on_anchor=self.state_mut().snap.hovered.is_some_and(|(client,r)|client==picker.client && r.contains(p));
            if (!picker.near(p) && !on_anchor) || matches!(event,Event::MouseDown(_)) {
                self.state_mut().snap.clear();self.redraw_all(cx);
            }
        }
        let hovered=self.desk(cx).borrow::<WmDesk>().and_then(|desk| {
            let (client,hit)=desk.chrome_hit(p)?;
            (hit==ChromeHit::Maximize).then(|| desk.chrome_button_rect(client,hit).map(|r|(client,r))).flatten()
        });
        if self.state_mut().snap.hovered!=hovered {
            cx.stop_timer(self.snap_hover_timer);
            self.state_mut().snap.hovered=hovered;
            if hovered.is_some() && matches!(event,Event::MouseMove(_)) {self.snap_hover_timer=cx.start_timeout(0.45);}
        }
        false
    }
    /// The Dock's Trash: the window it opened if that's still there, else a Files window on it.
    fn open_trash(&mut self, cx: &mut Cx) {
        let open = self.trash_window.filter(|client| self.state_mut().clients.contains_key(client));
        if let Some(client) = open {
            self.restore_desktop(cx, client);
            return;
        }
        self.new_trash_window(cx, &["--trash".to_string()]);
    }

    /// The Dock's Empty Trash. Files is what can ask, and it asks in a window of its own.
    pub(super) fn empty_trash(&mut self, cx: &mut Cx) {
        self.close_focused_menu(cx);
        if self.ui.widget(cx, ids!(shell_menu)).borrow::<ShellMenu>().is_some_and(|m| m.is_open()) {
            self.close_shell_menu(cx);
        }
        self.new_trash_window(cx, &["--trash".to_string(), "--empty-trash".to_string()]);
    }

    fn new_trash_window(&mut self, cx: &mut Cx, args: &[String]) {
        self.launch_app_with_args(cx, "files", args);
        let s = self.state_mut();
        self.trash_window = s.clients.iter().filter(|(_, slot)| slot.app == "files").map(|(client, _)| *client).max();
    }

    pub(super) fn activate_shelf(&mut self, cx: &mut Cx, hit: ShelfHit) {
        self.close_focused_menu(cx);
        // The Mac Dock stays live over Launchpad: anything but Launchpad's own
        // icon leaves it first.
        if self.state_mut().style.target == DesktopStyle::Macos && !matches!(hit, ShelfHit::Launcher) {
            if self.ui.widget(cx, ids!(shell_menu)).borrow::<ShellMenu>().is_some_and(|m| m.is_open()) {
                self.close_shell_menu(cx);
            }
        }
        match hit {
            ShelfHit::Launcher => self.toggle_launcher(cx),
            // The system chat, as F8 opens it (#143).
            ShelfHit::Assistant => {
                crate::system_chat::toggle();
                crate::app_chat::focus(!crate::system_chat::is_open());
                self.system_chat_changed(cx);
            }
            ShelfHit::App(app) => {
                let existing = {
                    let s = self.state_mut();
                    let windows = || s.layout.desktop.windows.iter().rev()
                        .filter(|w| s.clients.get(&w.client).is_some_and(|c| c.app == app));
                    // On a Mac the icon brings up a window that is out, then a
                    // hidden app; a minimized one only when there is no other.
                    let open = (s.style.target == DesktopStyle::Macos)
                        .then(|| windows().find(|w| !w.minimized || w.hidden))
                        .flatten();
                    open.or_else(|| windows().next()).map(|w| w.client)
                };
                if let Some(c) = existing {
                    self.restore_desktop(cx, c);
                } else {
                    self.launch_app(cx, &app);
                }
            }
            ShelfHit::Trash => self.open_trash(cx),
            ShelfHit::Window(c) => {
                if self.state_mut().layout.focused_client() == Some(c)
                    && !self.state_mut().layout.desktop.minimized(c)
                {
                    self.minimize_desktop(cx, c);
                } else {
                    self.restore_desktop(cx, c);
                }
            }
            ShelfHit::ShowDesktop => {
                let layout = &mut self.state_mut().layout;
                let clients = layout.clients_on(layout.active);
                let hide = clients.iter().any(|c| !layout.desktop.minimized(*c));
                for c in clients {
                    if let Some(w) = layout.desktop.get_mut(c) {
                        w.minimized = hide;
                    }
                }
                self.redraw_all(cx);
            }
        }
    }
    /// The windows a Dock menu's Show All Windows shows for `app`: its windows
    /// on this desktop that are out, oldest first, and apart from them those
    /// in the Dock (minimized), which a Mac's App Exposé has in a row at its
    /// foot. Hidden and full screen ones stay where they are.
    pub(super) fn expose_windows(&mut self, app: &str) -> (Vec<ClientId>, Vec<ClientId>) {
        let state = self.state_mut();
        let here = state.layout.clients_on(state.layout.active);
        let windows: Vec<(ClientId, bool)> = state
            .layout
            .desktop
            .windows
            .iter()
            .filter(|w| !w.fullscreen && !w.hidden && here.contains(&w.client))
            .filter(|w| state.clients.get(&w.client).is_some_and(|s| s.app == app && !s.warm && !s.pane && s.closing.is_none()))
            .map(|w| (w.client, w.minimized))
            .collect();
        let pick = |minimized: bool| -> Vec<ClientId> { windows.iter().filter(|(_, m)| *m == minimized).map(|(c, _)| *c).collect() };
        (pick(false), pick(true))
    }

    /// A Mac's App Exposé, from a Dock menu's Show All Windows: `app`'s windows
    /// side by side over the dimmed desk (desk.rs `draw_expose`) until a click
    /// or Return picks one, or Esc or a click on the desk leaves.
    pub(super) fn open_expose(&mut self, cx: &mut Cx, app: &str) {
        let (windows, minimized) = self.expose_windows(app);
        let docked = if minimized.is_empty() { String::new() } else { format!(" and {} minimized", minimized.len()) };
        log!("wm: show all windows of {app}: {}{docked}", windows.len());
        if windows.is_empty() && minimized.is_empty() {
            return;
        }
        let since = cx.seconds_since_app_start();
        self.state_mut().expose = Some(desk::Expose { app: app.to_string(), clients: windows, minimized, cells: Vec::new(), hover: None, since });
        self.redraw_all(cx);
    }

    pub(super) fn close_expose(&mut self, cx: &mut Cx, why: &str) {
        if self.state_mut().expose.take().is_some() {
            log!("wm: show all windows closed ({why})");
            self.redraw_all(cx);
        }
    }

    fn set_expose_hover(&mut self, cx: &mut Cx, to: Option<ClientId>) {
        let changed = match self.state_mut().expose.as_mut() {
            Some(expose) if expose.hover != to => {
                expose.hover = to;
                true
            }
            _ => false,
        };
        if changed {
            self.redraw_all(cx);
        }
    }

    /// The window App Exposé chose comes to the front with the focus.
    fn expose_pick(&mut self, cx: &mut Cx, client: ClientId) {
        self.restore_desktop(cx, client);
        let focused = self.state_mut().layout.focused_client() == Some(client);
        log!("wm: show all windows brings {client} to the front{}", if focused { "" } else { " (without the focus)" });
    }

    fn on_dock_or_bar(&self, cx: &mut Cx, p: Vec2d) -> bool {
        self.ui.widget(cx, ids!(desktop_shelf)).borrow::<DesktopShelf>().is_some_and(|s| s.contains(p)) || self.shell_bar_claims(cx, p)
    }

    /// App Exposé while it is up: the arrow keys and the pointer choose a
    /// window, Return or a click brings it to the front, Esc or a click on
    /// the desk leaves. The Dock and the bar stay live: a press there leaves
    /// it and goes on to them, and so does a shortcut.
    pub(super) fn expose_event(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let Some(expose) = self.state.as_ref().and_then(|s| s.expose.as_ref()) else {
            return false;
        };
        let (cells, hover) = (expose.cells.clone(), expose.hover);
        let under = |p: Vec2d| cells.iter().find(|(_, r)| r.contains(p)).map(|(c, _)| *c);
        match event {
            Event::KeyDown(e) => {
                let dir = match e.key_code {
                    KeyCode::Escape => {
                        self.close_expose(cx, "Esc");
                        return true;
                    }
                    KeyCode::ReturnKey | KeyCode::NumpadEnter | KeyCode::Space => {
                        let pick = hover.or_else(|| cells.first().map(|(c, _)| *c));
                        self.close_expose(cx, "chosen");
                        if let Some(client) = pick {
                            self.expose_pick(cx, client);
                        }
                        return true;
                    }
                    _ if e.modifiers.logo || e.modifiers.control || e.modifiers.alt => {
                        self.close_expose(cx, "a shortcut");
                        return false;
                    }
                    KeyCode::ArrowLeft => (-1.0, 0.0),
                    KeyCode::ArrowRight => (1.0, 0.0),
                    KeyCode::ArrowUp => (0.0, -1.0),
                    KeyCode::ArrowDown => (0.0, 1.0),
                    KeyCode::Tab if !cells.is_empty() => {
                        let n = cells.len();
                        let next = match hover.and_then(|h| cells.iter().position(|(c, _)| *c == h)) {
                            Some(i) if e.modifiers.shift => (i + n - 1) % n,
                            Some(i) => (i + 1) % n,
                            None => 0,
                        };
                        self.set_expose_hover(cx, Some(cells[next].0));
                        return true;
                    }
                    _ => return true,
                };
                if let Some(to) = desk::expose_step(&cells, hover, dir) {
                    self.set_expose_hover(cx, Some(to));
                }
                true
            }
            Event::KeyUp(_) | Event::TextInput(_) => true,
            Event::MouseMove(e) => {
                if self.on_dock_or_bar(cx, e.abs) {
                    return false;
                }
                self.set_expose_hover(cx, under(e.abs));
                true
            }
            Event::MouseDown(e) => {
                if self.on_dock_or_bar(cx, e.abs) {
                    self.close_expose(cx, "the Dock or the bar");
                    return false;
                }
                let pick = under(e.abs);
                self.close_expose(cx, if pick.is_some() { "chosen" } else { "a click on the desk" });
                if let Some(client) = pick {
                    self.expose_pick(cx, client);
                }
                true
            }
            Event::MouseUp(e) => !self.on_dock_or_bar(cx, e.abs),
            Event::Scroll(_) => true,
            _ => false,
        }
    }

    /// Open at Login: each app the person ticked in a Dock menu's Options, as
    /// the shell starts on a desktop. One no longer in the catalog is skipped.
    #[cfg_attr(mobile_only, allow(dead_code))]
    pub(super) fn open_login_apps(&mut self, cx: &mut Cx) {
        if self.state_mut().style.target.mobile() {
            return;
        }
        for app in self.login_apps.clone() {
            if !Self::launchable(&app) {
                log!("wm: {app} is not here to open at login");
                continue;
            }
            log!("wm: opens {app} at login");
            self.launch_app(cx, &app);
        }
    }

    pub(super) fn restore_desktop(&mut self, cx: &mut Cx, client: ClientId) {
        // A hidden app comes back whole; this window ends in front of the others.
        let state = self.state_mut();
        if state.layout.desktop.hidden(client) {
            let app = state.clients.get(&client).map(|slot| slot.app.clone());
            let others: Vec<ClientId> = state.layout.desktop.windows.iter()
                .filter(|w| w.minimized && w.hidden && w.client != client)
                .map(|w| w.client)
                .filter(|other| state.clients.get(other).map(|slot| &slot.app) == app.as_ref())
                .collect();
            for other in others {
                if let Some(w) = state.layout.desktop.get_mut(other) {
                    w.minimized = false;
                    w.hidden = false;
                }
                state.layout.raise_float(other);
            }
        }
        if let Some(w) = self.state_mut().layout.desktop.get_mut(client) {
            w.minimized = false;
            w.hidden = false;
        }
        self.state_mut().layout.raise_float(client);
        self.focus_client(cx, client);
        self.redraw_all(cx);
    }
    pub(super) fn desktop_key(&mut self, cx: &mut Cx, e: &KeyEvent) -> bool {
        let style = self.state_mut().style.target;
        if !style.floating() {
            return false;
        }
        if e.key_code == KeyCode::Escape && e.modifiers.control {
            self.toggle_launcher(cx);
            return true;
        }
        if e.key_code == KeyCode::F4 && e.modifiers.alt {
            if let Some(c) = self.state_mut().layout.focused_client() {
                self.request_close(cx, c);
            }
            return true;
        }
        if !super_chord(&e.modifiers) {
            return false;
        }
        // The macOS family has no show-desktop: OctoSense shares the dock.
        if e.key_code == KeyCode::KeyD && !style.mac_family() {
            self.activate_shelf(cx, ShelfHit::ShowDesktop);
            return true;
        }
        let Some(client) = self.state_mut().layout.focused_client() else {
            return false;
        };
        if e.key_code == KeyCode::KeyM {
            self.minimize_desktop(cx, client);
            return true;
        }
        if style == DesktopStyle::Windows {
            match e.key_code {
                KeyCode::KeyZ => {
                    let area=self.desk_area(cx);
                    let anchor=self.desk(cx).borrow::<WmDesk>().and_then(|d|d.chrome_button_rect(client,ChromeHit::Maximize));
                    self.state_mut().snap.picker=Some(crate::snap::Picker::new(client,area,anchor));
                    self.state_mut().snap.area=Some(area);
                    self.redraw_all(cx);
                }
                KeyCode::ArrowUp => self.maximize_desktop(cx, client),
                KeyCode::ArrowDown => {
                    if self
                        .state_mut()
                        .layout
                        .desktop
                        .get(client)
                        .is_some_and(|w| w.maximized)
                    {
                        self.maximize_desktop(cx, client);
                    } else {
                        self.minimize_desktop(cx, client);
                    }
                }
                KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                    self.apply_snap(cx,client,if e.key_code==KeyCode::ArrowRight {crate::snap::Zone::Right}else{crate::snap::Zone::Left});
                }
                _ => return false,
            }
            return true;
        }
        false
    }
    /// Called before child input dispatch; coordinates are from the last drawn
    /// frame, including the interpolated title bar and shelf rectangles.
    /// A press on a window's title bar, or on the band of its unified toolbar
    /// that no control took: a drag moves the window, a double press zooms it.
    pub(super) fn press_title(&mut self, cx: &mut Cx, client: ClientId, p: Vec2d, at: f64) {
        let double = self.title_press.is_some_and(|(c, time, pos)| {
            c == client && at - time < 0.35 && (pos - p).length() < 5.0
        });
        self.title_press = Some((client, at, p));
        if self.focus_seen == Some(client) {
            self.close_focused_menu(cx);
        }
        if double {
            self.maximize_desktop(cx, client);
        } else {
            self.begin_drag_on(cx, client, p, false, false);
        }
    }

    pub(super) fn desktop_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        if self.drag.is_some() || self.div_drag.is_some() {
            return false;
        }
        let p = match event {
            Event::MouseDown(e) => e.abs,
            Event::MouseMove(e) => e.abs,
            Event::MouseUp(e) => e.abs,
            Event::Scroll(e) => e.abs,
            _ => return false,
        };
        // A menu a hiding strip opened keeps that strip out: the launcher the
        // dock, a bar menu the top of a full-screen window.
        let menu = self.ui.widget(cx, ids!(shell_menu)).borrow::<ShellMenu>().map(|m| (m.model.open, m.model.skin));
        let held = matches!(menu, Some((true, MenuSkin::Launcher)));
        let bar_menu = matches!(menu, Some((true, MenuSkin::Menu)));
        // While an app presents (`WmState::presentation`), no edge brings
        // anything out: Esc and the app's own exit are the ways out.
        let presenting = self.state.as_ref().is_some_and(|s| s.presentation.is_some());
        let shelf = self.ui.widget(cx, ids!(desktop_shelf));
        let (hit, inside) = if let Some(mut s) = shelf.borrow_mut::<DesktopShelf>() {
            s.hover_at(cx, p);
            s.auto_hide_pointer(cx, (!presenting).then_some(p), held && !presenting);
            (s.hit(p), s.contains(p))
        } else {
            (None, false)
        };
        // The Mac Dock opens on the release over the icon pressed; the press
        // only pushes it down. A release elsewhere changes nothing.
        let mac_dock = self.state_mut().style.target == DesktopStyle::Macos;
        if let (true, Event::MouseUp(e)) = (mac_dock, event) {
            if e.button.contains(MouseButton::PRIMARY) {
                let pressed = shelf.borrow_mut::<DesktopShelf>().and_then(|mut s| s.take_press(cx));
                if let Some(pressed) = pressed {
                    if hit.as_ref() == Some(&pressed) {
                        self.activate_shelf(cx, pressed);
                    }
                    return true;
                }
            }
        }
        // The top of a full-screen window comes out at the top edge: its
        // title bar, as macOS shows it with the menu bar.
        if self.front_fullscreen {
            let screen = self.desk(cx).borrow::<WmDesk>().map(|d| d.desk_rect).unwrap_or_default();
            // The bar coming out with it, then the title bar.
            let depth = self.state_mut().top_bar + self.state_mut().style.target.title_height();
            let wants = bar_menu || crate::desktop::top_pointer_wants(p, screen, depth, self.top_edge.out);
            self.top_edge.want(cx, wants && !presenting);
        }
        let chrome_hit = self.desk(cx).borrow_mut::<WmDesk>()
            .and_then(|mut d| d.chrome_pointer(cx, (!inside).then_some(p)));
        // A Mac window's edge that doesn't size it (below) is its frame all the same.
        let fixed_edge = chrome_hit
            .filter(|(client, hit)| matches!(hit, ChromeHit::Resize(..)) && self.zoom_refused(*client))
            .map(|(client, _)| client);
        // A window that doesn't zoom doesn't size from its edges either, as the gray green
        // light says, unless it widens: then its side edges and corners size its width
        // alone. Its own controls still ask for room (`WindowRequest::Resize`).
        let chrome_hit = chrome_hit.and_then(|(client, hit)| match hit {
            ChromeHit::Resize(x, _) if self.zoom_refused(client) => (x != 0 && self.widens(client)).then_some((client, ChromeHit::Resize(x, 0))),
            _ => Some((client, hit)),
        });
        if let Event::MouseUp(e) = event {
            if e.button.contains(MouseButton::PRIMARY) {
                let pressed = self.desk(cx).borrow_mut::<WmDesk>()
                    .and_then(|mut d| d.release_chrome(cx));
                if let Some((client, control)) = pressed {
                    if chrome_hit == Some((client, control)) {
                        match control {
                            ChromeHit::Close => self.request_close(cx, client),
                            ChromeHit::Minimize => self.minimize_desktop(cx, client),
                            // macOS's green button makes the window full
                            // screen; with Option it zooms (maximizes).
                            ChromeHit::Maximize
                                if self.state_mut().style.target.mac_family() && !e.modifiers.alt =>
                            {
                                self.fullscreen_desktop(cx, client)
                            }
                            ChromeHit::Maximize => self.maximize_desktop(cx, client),
                            _ => {}
                        }
                    }
                    return true;
                }
            }
        }
        if inside {
            if let Event::MouseDown(e) = event {
                if e.button.contains(MouseButton::PRIMARY) {
                    if let Some(hit) = hit {
                        if mac_dock {
                            if let Some(mut s) = shelf.borrow_mut::<DesktopShelf>() {
                                s.press(cx, hit);
                            }
                        } else {
                            self.activate_shelf(cx, hit);
                        }
                    }
                } else if mac_dock && e.button.contains(MouseButton::SECONDARY) {
                    let column = shelf.borrow::<DesktopShelf>().and_then(|s| s.hit_column(p));
                    if let (Some(hit), Some(column)) = (hit, column) {
                        self.open_dock_menu(cx, hit, column);
                    }
                }
            }
            return true;
        }
        if !self.state_mut().style.target.floating() {
            return false;
        }
        if self.state_mut().style.target == DesktopStyle::NextStep {
            if let Event::MouseDown(e) = event {
                let (on_desktop, over_window) = self.desk(cx).borrow::<WmDesk>()
                    .map(|d| (d.desk_rect.contains(p), d.window_at(p).is_some()))
                    .unwrap_or((false, false));
                if e.button.contains(MouseButton::SECONDARY)
                    && on_desktop
                    && (chrome_hit.is_some() || !over_window)
                {
                    if let Some(mut menu) = self.ui.widget(cx, ids!(shell_menu)).borrow_mut::<ShellMenu>() {
                        menu.open_next_popup(cx, p);
                    }
                    self.redraw_all(cx);
                    return true;
                }
            }
        }
        let hit = chrome_hit;
        if let Event::MouseMove(_) = event {
            if let Some((_, hit)) = hit {
                cx.set_cursor(match hit {
                    ChromeHit::Resize(0, _) => MouseCursor::NsResize,
                    ChromeHit::Resize(_, 0) => MouseCursor::EwResize,
                    ChromeHit::Resize(x, y) if x == y => MouseCursor::NwseResize,
                    ChromeHit::Resize(_, _) => MouseCursor::NeswResize,
                    _ => MouseCursor::Default,
                });
            }
        }
        if let Event::MouseDown(e) = event {
            if e.button.contains(MouseButton::PRIMARY) {
                if let Some((client, hit)) = hit {
                    match hit {
                        ChromeHit::Close | ChromeHit::Minimize | ChromeHit::Maximize => {
                            if let Some(mut desk) = self.desk(cx).borrow_mut::<WmDesk>() {
                                desk.press_chrome(cx, (client, hit));
                            }
                        }
                        // A full-screen window's title bar neither moves nor
                        // zooms it; its buttons still work.
                        ChromeHit::Title if self.state_mut().layout.is_fullscreen(client) => {}
                        ChromeHit::Title => self.press_title(cx, client, p, e.time),
                        ChromeHit::Resize(x, y) => {
                            self.begin_drag_on(cx, client, p, true, false);
                            if let Some(d) = &mut self.drag {
                                d.resize_x = x != 0;
                                d.resize_y = y != 0;
                                d.grab_left = x < 0;
                                d.grab_top = y < 0;
                            }
                        }
                    }
                    return true;
                }
                // A press on a Mac window's edge that doesn't size it brings the
                // window forward and goes no further: the app's field keeps its
                // focus, as AppKit keeps the first responder through a press on
                // the frame.
                if let Some(client) = fixed_edge.filter(|_| self.state_mut().style.target == DesktopStyle::Macos) {
                    if self.state_mut().layout.is_float(client) {
                        self.state_mut().layout.raise_float(client);
                    }
                    self.focus_client(cx, client);
                    return true;
                }
            }
        }
        hit.is_some()
    }
}

impl App {
    /// The desk's picture and the ground beneath it. Omarchy retains the
    /// selected wallpaper and OctoSense loads its bundled image into the same
    /// slot, the spec gradient beneath as the fallback. A Mac shows the
    /// person's desktop picture, and a colour fills the ground. The other
    /// desktop identities have their own ground.
    pub(crate) fn apply_desk_ground(&mut self, cx: &mut Cx, style: DesktopStyle, dark: bool) {
        use crate::mac_appearance::Picture;
        let picture = if style == DesktopStyle::Macos { self.mac_picture } else { Picture::default() };
        let has_wallpaper = match style {
            DesktopStyle::Omarchy => {
                // Restore the selected Omarchy raster when returning from
                // another style (a cache hit when it is already decoded).
                self.apply_background(cx, self.background_index)
            }
            DesktopStyle::Macos if picture.colour().is_some() => false,
            // Omarchy's default picture, under the same cache key.
            DesktopStyle::Macos if picture == Picture::TokyoNight => self.ui.image(cx, ids!(bg_image)).load_image_from_data_async(
                cx, std::path::Path::new("octosense-bundled/tokyo-night.webp"), std::sync::Arc::new(theme::BUNDLED_TOKYO_NIGHT_WALLPAPER),
            ).is_ok(),
            DesktopStyle::OctoSense | DesktopStyle::Macos => {
                // Separate cache keys prevent a decoded dark image being reused
                // for the light appearance (or vice versa).
                let (path, bytes) = if dark {
                    ("octosense-bundled/abyssal-currents.png", theme::BUNDLED_OCTOSENSE_WALLPAPER)
                } else {
                    ("octosense-bundled/abyssal-currents-light.png", theme::BUNDLED_OCTOSENSE_LIGHT_WALLPAPER)
                };
                self.ui.image(cx, ids!(bg_image)).load_image_from_data_async(
                    cx, std::path::Path::new(path), std::sync::Arc::new(bytes),
                ).is_ok()
            }
            _ => false,
        };
        self.ui.widget(cx, ids!(bg_image)).set_visible(cx, has_wallpaper);
        let spec = &SPECS[style as usize];
        let pair = match picture.colour() {
            Some(colour) => (colour, colour),
            None if dark && style.supports_dark() => spec.ground_dark,
            None => spec.ground,
        };
        let to = |(r, g, b)| shell::rgb(r, g, b);
        let (top, bottom) = (to(pair.0), to(pair.1));
        let mut bg = self.ui.widget(cx, ids!(bg_fill));
        script_apply_eval!(cx,bg,{draw_bg +: {color_top: #(top) color_bottom: #(bottom)}});
    }
}

#[cfg(test)]
mod appearance_tests {
    use super::*;
    #[test]
    fn the_bar_strip_is_transparent_only_under_glass() {
        let ground = shell::rgb(26, 27, 38);
        let flat = shell::MaterialTokens::default();
        assert_eq!(bar_strip_color(&flat, ground, false), ground);
        assert_eq!(bar_strip_color(&flat, ground, true), Vec4f::default(), "a Mac's bar is translucent over the wallpaper");
        let glass = shell::MaterialTokens { glass: 1.0, ..flat };
        assert_eq!(bar_strip_color(&glass, ground, false), Vec4f::default());
        let below = shell::MaterialTokens { glass: 0.49, ..flat };
        assert_eq!(bar_strip_color(&below, ground, false), ground, "under the glass threshold reads flat");
    }

    #[test]
    fn warm_browser_appearance_matches_classic_and_omarchy_palettes() {
        for style in [DesktopStyle::Windows2000,DesktopStyle::NextStep] {
            assert!(!browser_appearance(style,true,""));
        }
        for style in [DesktopStyle::Macos,DesktopStyle::Windows,DesktopStyle::Ios,DesktopStyle::Android] {
            assert!(browser_appearance(style,true,""));
            assert!(!browser_appearance(style,false,""));
        }
        assert!(browser_appearance(DesktopStyle::Omarchy,false,"background: #121212"));
        assert!(!browser_appearance(DesktopStyle::Omarchy,true,"background: #eeeeee"));
        assert!(!browser_appearance(DesktopStyle::OctoSense,false,""));
        assert!(browser_appearance(DesktopStyle::OctoSense,true,""));
    }
}
