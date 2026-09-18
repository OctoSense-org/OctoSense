//! Public Android launcher identities and asynchronous system state.
use crate::{
    mobile::PhoneScreen,
    mobile_shade::{ShadeHit, Toggle},
    App,
};
use makepad_strict_json::{obj, s, Value};
use makepad_widgets::{
    image_cache::{decode_image_from_data, ImageBuffer, ImageError},
    makepad_platform::thread::Lane,
    *,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    io::Read,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct AndroidApp {
    pub id: String,
    pub label: String,
    pub component: String,
    pub user: i64,
    pub icon: String,
    pub locked: bool,
    pub suspended: bool,
    pub shortcut: bool,
    pub disabled_message: String,
}
#[derive(Clone, Debug)]
pub struct NativeNoticeAction {
    pub handle: String,
    pub reply: bool,
    pub open: bool,
}
#[derive(Clone, Debug)]
pub struct NativeNotice {
    pub identity: String,
    pub handle: String,
    pub actions: Vec<NativeNoticeAction>,
    pub dismissible: bool,
}
#[derive(Clone, Debug)]
pub struct AndroidWidget {
    pub id: i32,
    pub label: String,
    pub available: bool,
}
#[derive(Clone, Default)]
pub struct AndroidState {
    pub apps: Arc<Vec<AndroidApp>>,
    pub rows: Arc<Vec<(String, String)>>,
    pub icons: Arc<HashMap<String, Texture>>,
    pub favorites: Arc<Vec<String>>,
    pub dock: Arc<Vec<String>>,
    pub hidden_hosted: Arc<Vec<String>>,
    pub widgets: Arc<Vec<AndroidWidget>>,
    pub widget_epoch: String,
    pub widget_revision: u64,
    pub home_layout_generation: u64,
    pub home_transition_id: u64,
    pub home_transition_progress: f64,
    pub catalog_revision: u64,
    pub quickstep_connected: bool,
    pub quickstep_reason: String,
    pub notices: Arc<HashMap<u64, NativeNotice>>,
    pub capabilities: Arc<HashSet<String>>,
    pub connected: bool,
    pub connection_reason: String,
}
#[derive(Default)]
pub struct AndroidRuntime {
    epoch: String,
    revision: u64,
    next_chunk: u64,
    chunks: u64,
    staging: Vec<AndroidApp>,
    placement_epoch: String,
    placement_revision: u64,
    pending_icons: VecDeque<String>,
    active_icons: HashSet<String>,
    bridge_epoch: String,
    bridge_revision: u64,
    home_transition_epoch: String,
    home_transition_revision: u64,
    next_command: i64,
}
#[derive(Debug)]
struct IconLoaded {
    path: String,
    result: RefCell<Option<Result<ImageBuffer, ImageError>>>,
}
fn string(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_owned()
}
fn boolean(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}
fn unit(v: &Value, k: &str) -> Option<f64> {
    match v.get(k)? {
        Value::F64(v) => Some(v.clamp(0.0, 1.0)),
        Value::Int(v) => Some((*v as f64).clamp(0.0, 1.0)),
        _ => None,
    }
}
fn hosted_identity(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-'))
}
#[cfg(test)]
mod placement_tests {
    use super::*;

    fn decode(text: &str) -> Option<(Vec<String>, Vec<String>, Vec<String>)> {
        decode_placements(&makepad_strict_json::parse(text.as_bytes()).unwrap())
    }
    #[test]
    fn legacy_placements_keep_native_identity_and_default_hosted_visibility() {
        let (favorites, dock, hidden) = decode(r#"{"version":1,"favorites":["android:10:example/.Main","android-shortcut:0:example:with:colon"],"dock":["browser","files","photos","terminal"]}"#).unwrap();
        assert_eq!(
            favorites,
            [
                "android:10:example/.Main",
                "android-shortcut:0:example:with:colon"
            ]
        );
        assert_eq!(dock, crate::mobile_surface::PINNED);
        assert!(hidden.is_empty());
    }
    #[test]
    fn hosted_visibility_and_empty_dock_slots_decode_without_duplicate_icons() {
        let (_, dock, hidden) = decode(r#"{"version":2,"favorites":[],"dock":["finance","","","android:0:example/.Main"],"hidden_hosted":["finance"]}"#).unwrap();
        assert_eq!(dock, ["finance", "", "", "android:0:example/.Main"]);
        assert_eq!(hidden, ["finance"]);
        for invalid in [
            r#"{"version":3,"favorites":[],"dock":["","","",""],"hidden_hosted":[]}"#,
            r#"{"version":2,"favorites":[],"dock":["finance","finance","",""],"hidden_hosted":[]}"#,
            r#"{"version":2,"favorites":[],"dock":["","","",""],"hidden_hosted":["android:0:example/.Main"]}"#,
            r#"{"version":2,"favorites":[],"dock":["","","",""],"hidden_hosted":[""]}"#,
            r#"{"version":2,"favorites":[],"dock":["apps.finance","","",""],"hidden_hosted":[]}"#,
            r#"{"version":2,"favorites":["finance"],"dock":["","","",""],"hidden_hosted":[]}"#,
            r#"{"version":2,"favorites":[],"dock":["","","",""]}"#,
            r#"{"version":1,"favorites":[],"dock":["finance","files","photos","terminal"]}"#,
        ] {
            assert!(decode(invalid).is_none(), "accepted {invalid}");
        }
    }
}
fn decode_placements(value: &Value) -> Option<(Vec<String>, Vec<String>, Vec<String>)> {
    let version = value.get("version")?.as_u64()?;
    if version != 1 && version != 2 {
        return None;
    }
    let read = |key: &str, limit: usize| -> Option<Vec<String>> {
        let entries = value.get(key)?.as_arr()?;
        if entries.len() > limit {
            return None;
        }
        let mut seen = HashSet::new();
        entries
            .iter()
            .map(|entry| {
                let id = entry.as_str()?;
                let native = id.starts_with("android:") || id.starts_with("android-shortcut:");
                let valid = match key {
                    "hidden_hosted" => hosted_identity(id),
                    "dock" => {
                        native
                            || if version == 1 {
                                crate::mobile_surface::PINNED.contains(&id)
                            } else {
                                id.is_empty() || hosted_identity(id)
                            }
                    }
                    _ => native,
                };
                if !valid || id.len() > 4096 || (!id.is_empty() && !seen.insert(id)) {
                    return None;
                }
                Some(id.to_owned())
            })
            .collect()
    };
    let favorites = read("favorites", 128)?;
    let dock = read("dock", 4)?;
    if dock.len() != 4 {
        return None;
    }
    let hidden = if version == 1 {
        Vec::new()
    } else {
        read("hidden_hosted", 128)?
    };
    Some((favorites, dock, hidden))
}
impl App {
    pub(crate) fn android_command(
        &mut self,
        cx: &mut Cx,
        channel: &str,
        operation: &str,
        mut fields: Vec<(&str, Value)>,
    ) {
        self.android_runtime.next_command += 1;
        fields.push(("id", Value::Int(self.android_runtime.next_command)));
        fields.push(("operation", s(operation)));
        cx.android_integration(channel, &obj(fields).to_json());
    }
    pub(crate) fn android_launch(&mut self, cx: &mut Cx, id: &str) -> bool {
        let Some(app) = self
            .state
            .as_ref()
            .and_then(|state| state.phone.android.apps.iter().find(|app| app.id == id))
            .cloned()
        else {
            if id.starts_with("android:") || id.starts_with("android-shortcut:") {
                self.notify(
                    cx,
                    "App unavailable",
                    "This app or shortcut is unavailable in its Android profile.",
                );
                return true;
            }
            return false;
        };
        if app.shortcut && app.suspended && !app.locked {
            self.notify(
                cx,
                "Shortcut unavailable",
                if app.disabled_message.is_empty() {
                    "This shortcut was disabled by its app."
                } else {
                    &app.disabled_message
                },
            );
        } else if app.locked || app.suspended {
            self.notify(
                cx,
                "App unavailable",
                "Unlock the profile or enable this app in Android settings.",
            );
        } else {
            self.android_command(
                cx,
                "launcher",
                if app.shortcut { "shortcut" } else { "launch" },
                vec![("app", s(id))],
            );
            self.state_mut().phone.navigate(PhoneScreen::Home);
        }
        true
    }
    pub(crate) fn android_event(&mut self, cx: &mut Cx, event: &Event) -> bool {
        if let Event::Actions(actions) = event {
            let mut changed = false;
            for action in actions {
                if let Some(icon) = action.downcast_ref::<IconLoaded>() {
                    self.android_runtime.active_icons.remove(&icon.path);
                    if let Some(Ok(image)) = icon.result.borrow_mut().take() {
                        if self.state.as_ref().is_some_and(|state| {
                            state
                                .phone
                                .android
                                .apps
                                .iter()
                                .any(|app| app.icon == icon.path)
                                || state
                                    .phone
                                    .shade
                                    .notifications
                                    .iter()
                                    .any(|note| note.app_icon == icon.path)
                        }) {
                            let texture = image.into_new_texture(cx);
                            Arc::make_mut(&mut self.state_mut().phone.android.icons)
                                .insert(icon.path.clone(), texture);
                            changed = true;
                        }
                    }
                }
            }
            if changed {
                self.redraw_all(cx);
            }
            self.android_pump_icons(cx);
        }
        let Event::AndroidIntegration { channel, payload } = event else {
            return false;
        };
        if self.state.is_none() {
            return true;
        }
        let Ok(value) = makepad_strict_json::parse_depth(payload.as_bytes(), 12) else {
            log!("Android: invalid {} packet", channel);
            return true;
        };
        match channel.as_str() {
            "home.layout.request" => {
                if let Some(generation) = value
                    .get("generation")
                    .and_then(Value::as_i64)
                    .filter(|v| *v >= 0)
                {
                    self.state_mut().phone.android.home_layout_generation = generation as u64;
                    self.animate_phone(cx);
                }
            }
            "home.extension" => {
                let android = &mut self.state_mut().phone.android;
                android.quickstep_connected = string(&value, "connection") == "connected";
                android.quickstep_reason = string(&value, "reason");
                if !android.quickstep_connected || android.quickstep_reason == "subscribed" {
                    android.home_transition_id = 0;
                    android.home_transition_progress = 0.0;
                }
            }
            "home.transition" => {
                let epoch = string(&value, "epoch");
                let revision = value
                    .get("event_revision")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let id = value.get("id").and_then(Value::as_u64).unwrap_or(0);
                let phase = value.get("phase").and_then(Value::as_i64).unwrap_or(-1);
                let progress = match value.get("progress") {
                    Some(Value::F64(value)) => *value,
                    Some(Value::Int(value)) => *value as f64,
                    _ => f64::NAN,
                };
                if epoch.is_empty()
                    || id == 0
                    || revision == 0
                    || !(0..=3).contains(&phase)
                    || !progress.is_finite()
                    || !(0.0..=1.0).contains(&progress)
                    || (epoch == self.android_runtime.home_transition_epoch
                        && revision <= self.android_runtime.home_transition_revision)
                {
                    return true;
                }
                self.android_runtime.home_transition_epoch = epoch;
                self.android_runtime.home_transition_revision = revision;
                let android = &mut self.state_mut().phone.android;
                if phase == 0 {
                    android.home_transition_id = id;
                    android.home_transition_progress = 0.0;
                } else if android.home_transition_id == id {
                    android.home_transition_progress = progress;
                    if phase >= 2 {
                        android.home_transition_id = 0;
                    }
                } else {
                    return true;
                }
                // Native SurfaceControl owns animation frames. Only lifecycle
                // boundaries request fresh renderer geometry; progress is state.
                if phase != 1 {
                    self.animate_phone(cx);
                }
            }
            // Only the explicitly enabled Android validation remote publishes
            // this local event. Normal manifests cannot start that endpoint.
            "validation.notification_probe" => {
                let hit = crate::mobile::PhoneHit::Shade(ShadeHit::Open(
                    crate::mobile_gestures::ShadeSide::Notifications,
                ));
                let bounds = self
                    .desk(cx)
                    .borrow::<crate::desk::WmDesk>()
                    .and_then(|desk| desk.phone_hit_rect(&hit));
                let desk_ref = self.desk(cx);
                let desk = desk_ref.borrow::<crate::desk::WmDesk>();
                let hit_bounds = |hit| {
                    desk.as_ref().and_then(|desk| desk.phone_hit_rect(&crate::mobile::PhoneHit::Shade(hit)))
                        .map(|r| Value::Arr(vec![Value::F64(r.pos.x), Value::F64(r.pos.y),
                            Value::F64(r.size.x), Value::F64(r.size.y)]))
                        .unwrap_or(Value::Null)
                };
                let phone = &self.state.as_ref().unwrap().phone;
                let notes = phone
                    .shade
                    .notifications
                    .iter()
                    .map(|note| {
                        obj(vec![
                            ("id", Value::Int(note.id as i64)),
                            ("title", s(&note.title)),
                            ("app_label", s(&note.app_label)),
                            ("body", s(&note.body)),
                            ("card_bounds", hit_bounds(ShadeHit::Note(note.id))),
                            ("revealed", Value::Bool(note.revealed)),
                            ("actions", Value::Arr(note.actions.iter().enumerate().map(|(index,label)|
                                obj(vec![("label",s(label)),("bounds",hit_bounds(ShadeHit::Action(note.id,index)))])
                            ).collect())),
                            ("dismiss_bounds", hit_bounds(ShadeHit::Action(note.id,note.actions.len()))),
                            (
                                "icon_loaded",
                                Value::Bool(phone.android.icons.contains_key(&note.app_icon)),
                            ),
                        ])
                    })
                    .collect();
                let bounds = bounds
                    .map(|r| {
                        Value::Arr(vec![
                            Value::F64(r.pos.x),
                            Value::F64(r.pos.y),
                            Value::F64(r.size.x),
                            Value::F64(r.size.y),
                        ])
                    })
                    .unwrap_or(Value::Null);
                let model = obj(vec![
                    ("token", s(string(&value, "token"))),
                    ("shade_open", Value::F64(phone.shade.open)),
                    ("open_bounds", bounds),
                    (
                        "logical_width",
                        Value::F64(cx.display_context.screen_size.x),
                    ),
                    ("notes", Value::Arr(notes)),
                    ("controls_bounds",hit_bounds(ShadeHit::Open(crate::mobile_gestures::ShadeSide::Controls))),
                    ("connected",Value::Bool(phone.shade.bridge_connected)),
                    ("notification_access",Value::Bool(phone.shade.notification_access)),
                    ("network_summary",s(&phone.shade.network_summary)),
                    ("settings",Value::Arr(["notifications","internet","bluetooth","hotspot","vpn","battery","display","sound","accessibility"].iter().map(|destination|
                        obj(vec![("destination",s(*destination)),("bounds",hit_bounds(ShadeHit::Settings(destination)))])).collect())),
                    ("setup_bounds",hit_bounds(ShadeHit::SystemAccess)),
                    ("toggles",Value::Arr(Toggle::ALL.iter().map(|toggle|obj(vec![("name",s(format!("{toggle:?}"))),("bounds",hit_bounds(ShadeHit::Toggle(*toggle)))])).collect())),
                ]);
                cx.android_integration("validation.ui", &model.to_json());
            }
            "validation.launcher_probe" => {
                let identity = string(&value, "identity");
                let hit = crate::mobile::PhoneHit::App(identity.clone());
                let bounds = self
                    .desk(cx)
                    .borrow::<crate::desk::WmDesk>()
                    .and_then(|desk| desk.phone_hit_rect(&hit))
                    .map(|r| {
                        Value::Arr(vec![
                            Value::F64(r.pos.x),
                            Value::F64(r.pos.y),
                            Value::F64(r.size.x),
                            Value::F64(r.size.y),
                        ])
                    })
                    .unwrap_or(Value::Null);
                let phone = &self.state.as_ref().unwrap().phone;
                let app = phone
                    .android
                    .apps
                    .iter()
                    .find(|app| app.id == identity)
                    .map(|app| {
                        obj(vec![
                            ("label", s(&app.label)),
                            ("icon", s(&app.icon)),
                            (
                                "icon_loaded",
                                Value::Bool(phone.android.icons.contains_key(&app.icon)),
                            ),
                            ("locked", Value::Bool(app.locked)),
                            ("suspended", Value::Bool(app.suspended)),
                            ("disabled_message", s(&app.disabled_message)),
                        ])
                    })
                    .unwrap_or(Value::Null);
                let notice = phone
                    .shade
                    .notifications
                    .iter()
                    .filter(|note| note.app == "wm")
                    .max_by_key(|note| note.id)
                    .map(|note| {
                        obj(vec![
                            ("id", Value::Int(note.id as i64)),
                            ("title", s(&note.title)),
                            ("body", s(&note.body)),
                        ])
                    })
                    .unwrap_or(Value::Null);
                let model = obj(vec![
                    ("token", s(string(&value, "token"))),
                    ("identity", s(&identity)),
                    ("app", app),
                    ("bounds", bounds),
                    (
                        "logical_width",
                        Value::F64(cx.display_context.screen_size.x),
                    ),
                    ("last_local_notice", notice),
                ]);
                cx.android_integration("validation.launcher_ui", &model.to_json());
            }
            "validation.navigation" => {
                if let Some(page) = value
                    .get("page")
                    .and_then(Value::as_i64)
                    .filter(|page| *page >= 0 && *page <= 32)
                {
                    self.state_mut().phone.navigate(PhoneScreen::Home);
                    self.state_mut().phone.pages.jump(page);
                    self.animate_phone(cx);
                }
            }
            "launcher.catalog" => self.android_catalog(cx, &value),
            "launcher.placements" => self.android_placements(&value),
            "launcher.widgets" => self.android_widgets(&value),
            "bridge.connection" => {
                let android = &mut self.state_mut().phone.android;
                android.connected = string(&value, "state") == "connected";
                android.connection_reason = string(&value, "reason");
                if !android.connected {
                    android.capabilities = Arc::default();
                }
                let connected=android.connected;
                self.state_mut().phone.shade.bridge_connected=connected;
                if !connected {
                    let phone=&mut self.state_mut().phone;
                    phone.shade.notifications.retain(|note| !phone.android.notices.contains_key(&note.id));
                    phone.android.notices=Arc::default();
                    let shade = &mut self.state_mut().phone.shade;
                    shade.control_enabled = [false, false, false, false, false, true];
                    shade.slider_enabled = [false; 2];
                    shade.notification_access=false;
                    shade.network_summary="Android reconnecting · Network settings".into();
                }
            }
            "bridge.snapshot" => self.android_snapshot(cx, &value),
            "notification.reply.submit" => {
                self.android_command(
                    cx,
                    "bridge",
                    "reply_send",
                    vec![
                        ("token", s(string(&value, "token"))),
                        ("handle", s(string(&value, "handle"))),
                        ("reply", s(string(&value, "reply"))),
                    ],
                );
            }
            "integration.resync" => {
                self.android_command(cx, "launcher", "catalog", vec![]);
                self.android_command(cx, "launcher", "widgets_snapshot", vec![]);
                self.android_command(cx, "bridge", "snapshot", vec![]);
                if boolean(&value, "command_outcome_uncertain") {
                    self.notify(cx,"Android connection interrupted","Some command results are unavailable. Check the observed device state before retrying.");
                }
            }
            "bridge.result" | "launcher.result" => {
                let status = value.get("status").and_then(Value::as_i64).unwrap_or(9);
                if status > 1 {
                    self.notify(
                        cx,
                        "Android operation unavailable",
                        &string(&value, "reason"),
                    );
                }
            }
            _ => {}
        }
        self.redraw_all(cx);
        true
    }
    fn android_placements(&mut self, value: &Value) {
        let epoch = string(value, "epoch");
        let revision = value.get("revision").and_then(Value::as_u64).unwrap_or(0);
        if epoch.is_empty()
            || revision == 0
            || (epoch == self.android_runtime.placement_epoch
                && revision <= self.android_runtime.placement_revision)
        {
            return;
        }
        let Some((favorites, dock, hidden_hosted)) = decode_placements(value) else {
            return;
        };
        self.android_runtime.placement_epoch = epoch;
        self.android_runtime.placement_revision = revision;
        let android = &mut self.state_mut().phone.android;
        android.favorites = Arc::new(favorites);
        android.dock = Arc::new(dock);
        android.hidden_hosted = Arc::new(hidden_hosted);
    }
    fn android_widgets(&mut self, value: &Value) {
        let epoch = string(value, "epoch");
        let revision = value.get("revision").and_then(Value::as_u64).unwrap_or(0);
        let Some(entries) = value.get("widgets").and_then(Value::as_arr) else {
            return;
        };
        let android = &mut self.state_mut().phone.android;
        if epoch.is_empty()
            || revision == 0
            || entries.len() > 16
            || (epoch == android.widget_epoch && revision <= android.widget_revision)
        {
            return;
        }
        let mut widgets = Vec::new();
        let mut seen = HashSet::new();
        for entry in entries {
            let Some(id) = entry
                .get("id")
                .and_then(Value::as_i64)
                .and_then(|id| i32::try_from(id).ok())
                .filter(|id| *id > 0)
            else {
                return;
            };
            if !seen.insert(id) {
                return;
            }
            widgets.push(AndroidWidget {
                id,
                label: string(entry, "label"),
                available: boolean(entry, "available"),
            });
        }
        android.widgets = Arc::new(widgets);
        android.widget_epoch = epoch;
        android.widget_revision = revision;
    }
    fn android_catalog(&mut self, cx: &mut Cx, value: &Value) {
        let epoch = string(value, "epoch");
        let revision = value.get("revision").and_then(Value::as_u64).unwrap_or(0);
        let chunk = value
            .get("chunk")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        let chunks = value.get("chunks").and_then(Value::as_u64).unwrap_or(0);
        if epoch.is_empty() || revision == 0 || chunks == 0 || chunks > 64 || chunk >= chunks {
            return;
        }
        let runtime = &mut self.android_runtime;
        if chunk == 0 {
            if epoch == runtime.epoch && revision <= runtime.revision {
                return;
            }
            runtime.epoch = epoch.clone();
            runtime.revision = revision;
            runtime.next_chunk = 0;
            runtime.chunks = chunks;
            runtime.staging.clear();
        }
        if epoch != runtime.epoch
            || revision != runtime.revision
            || chunk != runtime.next_chunk
            || chunks != runtime.chunks
        {
            runtime.staging.clear();
            self.android_command(cx, "launcher", "catalog", vec![]);
            return;
        }
        let Some(entries) = value.get("apps").and_then(Value::as_arr) else {
            return;
        };
        if entries.len() > 64 {
            return;
        }
        for entry in entries {
            let id = string(entry, "id");
            if !(id.starts_with("android:") || id.starts_with("android-shortcut:"))
                || id.len() > 1024
            {
                continue;
            }
            runtime.staging.push(AndroidApp {
                id,
                label: string(entry, "label"),
                component: string(entry, "component"),
                user: entry.get("user").and_then(Value::as_i64).unwrap_or(-1),
                icon: string(entry, "icon"),
                locked: boolean(entry, "locked"),
                suspended: boolean(entry, "suspended"),
                shortcut: boolean(entry, "shortcut"),
                disabled_message: string(entry, "disabled_message"),
            });
        }
        runtime.next_chunk += 1;
        if runtime.next_chunk != chunks {
            return;
        }
        let apps = std::mem::take(&mut runtime.staging);
        let mut rows: Vec<_> = crate::shell::launcher::apps()
            .iter()
            .map(|app| {
                (
                    app.id.trim_start_matches("apps.").to_owned(),
                    app.label.clone(),
                )
            })
            .collect();
        rows.extend(apps.iter().map(|app| (app.id.clone(), app.label.clone())));
        rows.sort_by_cached_key(|(_, label)| label.to_lowercase());
        let android = &mut self.state_mut().phone.android;
        android.apps = Arc::new(apps);
        android.catalog_revision = revision;
        android.rows = Arc::new(rows);
        self.android_sync_icons(cx);
    }
    fn android_sync_icons(&mut self, cx: &mut Cx) {
        let phone = &mut self.state_mut().phone;
        let paths: HashSet<_> = phone
            .android
            .apps
            .iter()
            .map(|app| app.icon.clone())
            .chain(
                phone
                    .shade
                    .notifications
                    .iter()
                    .map(|note| note.app_icon.clone()),
            )
            .filter(|path| !path.is_empty())
            .collect();
        Arc::make_mut(&mut phone.android.icons).retain(|path, _| paths.contains(path));
        let missing: Vec<_> = paths
            .into_iter()
            .filter(|path| !phone.android.icons.contains_key(path))
            .collect();
        self.android_runtime.pending_icons = missing
            .into_iter()
            .filter(|path| !self.android_runtime.active_icons.contains(path))
            .collect();
        self.android_pump_icons(cx);
    }
    fn android_pump_icons(&mut self, cx: &mut Cx) {
        while self.android_runtime.active_icons.len() < 4 {
            let Some(path) = self.android_runtime.pending_icons.pop_front() else {
                break;
            };
            let owned_path = path.clone();
            let task =
                cx.task_pool()
                    .submit_named(Lane::Light, "android_launcher_icon", move || {
                        let result = (|| {
                            let file = std::fs::File::open(&owned_path)
                                .map_err(|_| ImageError::PathNotFound(owned_path.clone().into()))?;
                            let mut bytes = Vec::new();
                            file.take(256 * 1024 + 1)
                                .read_to_end(&mut bytes)
                                .map_err(|_| ImageError::PathNotFound(owned_path.clone().into()))?;
                            if bytes.len() > 256 * 1024 {
                                return Err(ImageError::DataTooLarge {
                                    bytes: bytes.len(),
                                    limit: 256 * 1024,
                                });
                            }
                            decode_image_from_data(&bytes)
                        })();
                        Cx::post_action(IconLoaded {
                            path: owned_path,
                            result: RefCell::new(Some(result)),
                        });
                    });
            match task {
                Ok(task) => {
                    task.detach();
                    self.android_runtime.active_icons.insert(path);
                }
                Err(_) => {
                    self.android_runtime.pending_icons.push_front(path);
                    break;
                }
            }
        }
    }
    fn android_snapshot(&mut self, cx: &mut Cx, value: &Value) {
        let epoch = string(value, "epoch");
        let revision = value.get("revision").and_then(Value::as_u64).unwrap_or(0);
        if epoch.is_empty()
            || (epoch == self.android_runtime.bridge_epoch
                && revision < self.android_runtime.bridge_revision)
        {
            return;
        }
        let Some(state) = value.get("state") else {
            return;
        };
        self.android_runtime.bridge_epoch = epoch;
        self.android_runtime.bridge_revision = revision;
        let phone = &mut self.state_mut().phone;
        let caps = state.get("capabilities");
        let mut accessible = HashSet::new();
        for op in [
            "wifi",
            "bluetooth",
            "torch",
            "rotation",
            "brightness",
            "volume",
            "dnd",
            "notifications",
            "battery_saver",
        ] {
            if caps
                .and_then(|caps| caps.get(op))
                .is_some_and(|cap| boolean(cap, "accessible"))
            {
                accessible.insert(op.to_owned());
            }
        }
        phone.android.capabilities = Arc::new(accessible);
        phone.shade.bridge_connected=true;
        phone.shade.notification_access=phone.android.capabilities.contains("notifications");
        phone.shade.network_summary=string(state,"network_summary");
        if phone.shade.network_summary.is_empty() {phone.shade.network_summary="Network settings".into();}
        phone.shade.battery_saver=boolean(state,"battery_saver");
        phone.shade.control_enabled = ["wifi", "bluetooth", "torch", "rotation", "dnd", "dark"]
            .map(|op| op == "dark" || phone.android.capabilities.contains(op));
        phone.shade.slider_enabled =
            ["brightness", "volume"].map(|op| phone.android.capabilities.contains(op));
        if let Some(v) = unit(state, "brightness") {
            phone.shade.brightness = v;
        }
        if let Some(v) = unit(state, "volume") {
            phone.shade.volume = v;
        }
        phone.shade.wifi = boolean(state, "wifi");
        phone.shade.bluetooth = boolean(state, "bluetooth");
        phone.shade.torch = boolean(state, "torch");
        phone.shade.rotation_lock = boolean(state, "rotation_locked");
        phone.shade.do_not_disturb = state
            .get("interruption_filter")
            .and_then(Value::as_i64)
            .is_some_and(|v| v != 1);
        let previous = phone.android.notices.clone();
        let mut notices = HashMap::new();
        if let Some(entries) = state.get("notifications").and_then(Value::as_arr) {
            for entry in entries.iter().take(100) {
                let handle = string(entry, "handle");
                if handle.is_empty() {
                    continue;
                }
                let identity=string(entry,"identity");
                let existing = previous
                    .iter()
                    .find(|(_, notice)| notice.handle == handle || (!identity.is_empty() && notice.identity == identity))
                    .map(|(id, _)| *id)
                    .filter(|id| phone.shade.notifications.iter().any(|note| note.id == *id));
                let models = entry.get("actions").and_then(Value::as_arr).unwrap_or(&[]);
                let labels: Vec<_> = models.iter().map(|model| string(model, "label")).collect();
                let id = existing.unwrap_or_else(|| {
                    phone.shade.post(
                        &string(entry, "package"),
                        &string(entry, "title"),
                        &string(entry, "text"),
                        phone.shade.now,
                        labels.clone(),
                    )
                });
                if let Some(note) = phone
                    .shade
                    .notifications
                    .iter_mut()
                    .find(|note| note.id == id)
                {
                    note.app = string(entry, "package");
                    note.app_label = string(entry, "app_label");
                    note.app_icon = string(entry, "app_icon");
                    note.title = string(entry, "title");
                    note.body = string(entry, "text");
                    note.actions = labels;
                    note.dismissible=boolean(entry,"dismissible");
                }
                notices.insert(
                    id,
                    NativeNotice {
                        identity,
                        handle,
                        actions: models
                            .iter()
                            .map(|model| NativeNoticeAction {
                                handle: string(model, "handle"),
                                reply: boolean(model, "reply"),
                                open: boolean(model, "open"),
                            })
                            .collect(),
                        dismissible: boolean(entry, "dismissible"),
                    },
                );
            }
        }
        phone
            .shade
            .notifications
            .retain(|note| !previous.contains_key(&note.id) || notices.contains_key(&note.id));
        phone.android.notices = Arc::new(notices);
        self.android_sync_icons(cx);
    }
    pub(crate) fn android_shade_action(&mut self, cx: &mut Cx, hit: &ShadeHit) -> bool {
        if !cfg!(target_os = "android") {
            return false;
        }
        let phone = &self.state.as_ref().unwrap().phone;
        if matches!(hit, ShadeHit::SystemAccess) {
            self.android_command(cx, "launcher", "bridge_settings", vec![]);
            return true;
        }
        if let ShadeHit::Settings(destination)=hit {
            self.android_command(cx,"launcher","system_settings",vec![("destination",s(*destination))]);
            return true;
        }
        if let ShadeHit::Note(id)=hit {
            if let Some(notice)=phone.android.notices.get(id) {
                let revealed=phone.shade.notifications.iter().any(|note| note.id==*id && note.revealed);
                if !revealed {
                    if let Some(action)=notice.actions.iter().find(|action| action.open).cloned() {
                        self.android_command(cx,"bridge","action",vec![("handle",s(action.handle))]);
                        return true;
                    }
                }
            }
        }
        if matches!(hit, ShadeHit::Brightness | ShadeHit::Volume) {
            self.android_shade_release(cx, hit, dvec2(0.0, 0.0), 1.0);
            return true;
        }
        let operation = match hit {
            ShadeHit::Toggle(t) => match t {
                Toggle::Wifi => Some(("wifi", !phone.shade.wifi)),
                Toggle::Bluetooth => Some(("bluetooth", !phone.shade.bluetooth)),
                Toggle::Torch => Some(("torch", !phone.shade.torch)),
                Toggle::RotationLock => Some(("rotation", !phone.shade.rotation_lock)),
                Toggle::DoNotDisturb => Some(("dnd", !phone.shade.do_not_disturb)),
                Toggle::DarkMode => None,
            },
            _ => None,
        };
        if let Some((op, enabled)) = operation {
            if phone.android.connected && phone.android.capabilities.contains(op) {
                self.android_command(cx, "bridge", op, vec![("enabled", Value::Bool(enabled))]);
            } else {
                self.android_command(cx,"launcher","system_settings",vec![("destination",s(op))]);
            }
            return true;
        }
        if let ShadeHit::Action(id, index) = hit {
            if let Some(notice) = phone.android.notices.get(id).cloned() {
                if let Some(action) = notice.actions.get(*index) {
                    self.android_command(
                        cx,
                        "bridge",
                        if action.reply { "reply" } else { "action" },
                        vec![("handle", s(&action.handle))],
                    );
                } else if notice.dismissible {
                    self.android_command(
                        cx,
                        "bridge",
                        "dismiss",
                        vec![("handle", s(notice.handle))],
                    );
                }
                return true;
            }
        }
        if matches!(hit, ShadeHit::ClearAll) {
            self.android_command(cx, "bridge", "dismiss_all", vec![]);
            return true;
        }
        false
    }
    pub(crate) fn android_shade_release(
        &mut self,
        cx: &mut Cx,
        hit: &ShadeHit,
        delta: Vec2d,
        dt: f64,
    ) {
        if !cfg!(target_os = "android") {
            return;
        }
        match hit {
            ShadeHit::Brightness | ShadeHit::Volume => {
                let phone = &self.state.as_ref().unwrap().phone;
                let op = if matches!(hit, ShadeHit::Brightness) {
                    "brightness"
                } else {
                    "volume"
                };
                let value = if op == "brightness" {
                    phone.shade.brightness
                } else {
                    phone.shade.volume
                };
                if phone.android.connected && phone.android.capabilities.contains(op) {
                    self.android_command(cx, "bridge", op, vec![("value", Value::F64(value))]);
                }
            }
            ShadeHit::Note(id) if delta.x > 96.0 || (dt < 0.3 && delta.x > 40.0) => {
                if let Some(notice) = self
                    .state
                    .as_ref()
                    .unwrap()
                    .phone
                    .android
                    .notices
                    .get(id)
                    .cloned()
                {
                    if notice.dismissible {
                        self.android_command(
                            cx,
                            "bridge",
                            "dismiss",
                            vec![("handle", s(notice.handle))],
                        );
                    }
                }
            }
            _ => {}
        }
    }
}
