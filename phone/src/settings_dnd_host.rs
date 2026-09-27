//! Foreground-root, stable page and request correlation for DND policy and schedules.
use crate::{
    hub::ClientId,
    settings_app::{SettingsRequest, SettingsView},
    settings_dnd::{DndRead, DndRequest, DndSnapshot},
    App,
};
use makepad_strict_json::{s, Value};
use makepad_widgets::*;
type Owner = (ClientId, WidgetUid);
#[derive(Clone)]
struct Read {
    id: i64,
    owner: Owner,
    key: DndRead,
    deadline: f64,
}
#[derive(Default)]
pub(crate) struct SettingsDndRuntime {
    active: Option<Owner>,
    desired: Option<DndRead>,
    read: Option<Read>,
    next_read: f64,
    fresh: bool,
    focused: bool,
    snapshot: Option<DndSnapshot>,
    pub(crate) error: String,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dnd_focus_and_owner_retire_pending_reads_and_write_authority() {
        let owner = (1, WidgetUid(4));
        let state = DndSnapshot::decode(&crate::settings_dnd::tests::snapshot(1)).unwrap();
        let key = DndRead {
            offset: 0,
            generation: Some(state.generation.clone()),
        };
        let mut runtime = SettingsDndRuntime {
            active: Some(owner),
            read: Some(Read {
                id: 2,
                owner,
                key,
                deadline: 20.,
            }),
            fresh: true,
            focused: true,
            snapshot: Some(state),
            next_read: 50.,
            ..Default::default()
        };
        assert!(runtime.accepts(2, owner));
        assert!(!runtime.accepts(2, (1, WidgetUid(5))));
        assert!(runtime.focus(Some(false)));
        assert!(!runtime.accepts(2, owner));
        assert!(runtime
            .snapshot()
            .unwrap()
            .policy
            .iter()
            .all(|r| !r.can_set));
        assert!(runtime.focus(Some(true)));
        assert_eq!(runtime.next_read, 0.);
        assert!(!runtime.fresh);
    }
}
impl SettingsDndRuntime {
    pub(crate) fn snapshot(&self) -> Option<DndSnapshot> {
        let mut s = self.snapshot.clone()?;
        if !self.fresh {
            s.clear_actions();
        }
        Some(s)
    }
    fn focus(&mut self, observed: Option<bool>) -> bool {
        let Some(focused) = observed else {
            return false;
        };
        if self.focused == focused {
            return false;
        }
        self.focused = focused;
        self.retire();
        true
    }
    fn retire(&mut self) {
        self.read = None;
        self.fresh = false;
        self.next_read = 0.;
    }
    fn accepts(&self, id: i64, owner: Owner) -> bool {
        self.active == Some(owner)
            && self
                .read
                .as_ref()
                .is_some_and(|r| r.id == id && r.owner == owner)
    }
}
impl App {
    pub(crate) fn settings_dnd_focus(&mut self, cx: &mut Cx, observed: Option<bool>) {
        if self.settings_runtime.dnd.focus(observed) {
            self.refresh_settings_app(cx);
        }
    }
    fn current_settings_dnd_read(&self) -> Option<DndRead> {
        self.module_host
            .settings_instance()?
            .root
            .borrow::<SettingsView>()?
            .dnd_read()
    }
    pub(crate) fn settings_dnd_read_request(
        &mut self,
        cx: &mut Cx,
        owner: Owner,
        request: &SettingsRequest,
    ) -> bool {
        let SettingsRequest::Dnd(DndRequest::Snapshot(key)) = request else {
            return false;
        };
        if cfg!(target_os = "android")
            && self.settings_runtime.dnd.focused
            && self.settings_is_foreground(owner)
            && self.current_settings_dnd_read().as_ref() == Some(key)
        {
            self.start_settings_dnd_read(cx, owner, key.clone(), true);
        }
        true
    }
    fn start_settings_dnd_read(&mut self, cx: &mut Cx, owner: Owner, key: DndRead, manual: bool) {
        if self.module_host.settings_client(owner.1) != Some(owner.0) || !key.valid() {
            return;
        }
        if self
            .settings_runtime
            .dnd
            .read
            .as_ref()
            .is_some_and(|r| r.owner == owner && r.key == key)
        {
            return;
        }
        let id = self.android_command_id(
            cx,
            "launcher",
            "dnd_snapshot",
            vec![
                ("offset", Value::Int(key.offset as i64)),
                (
                    "generation",
                    key.generation
                        .as_ref()
                        .map(|g| s(g.wire()))
                        .unwrap_or(Value::Null),
                ),
            ],
        );
        let runtime = &mut self.settings_runtime.dnd;
        if runtime.active != Some(owner) || runtime.desired.as_ref() != Some(&key) {
            runtime.retire();
        }
        runtime.active = Some(owner);
        runtime.desired = Some(key.clone());
        runtime.read = Some(Read {
            id,
            owner,
            key,
            deadline: crate::host::now() + 20.,
        });
        runtime.next_read = crate::host::now() + 5.;
        if manual {
            runtime.error.clear();
            self.refresh_settings_app(cx);
        }
    }
    pub(crate) fn settings_dnd_tick(&mut self, cx: &mut Cx, visible: Option<Owner>, now: f64) {
        let visible = visible.filter(|_| self.settings_runtime.dnd.focused);
        let desired = visible.and_then(|_| self.current_settings_dnd_read());
        let active = visible.filter(|_| desired.is_some());
        let runtime = &mut self.settings_runtime.dnd;
        if runtime.active != active || runtime.desired != desired {
            runtime.retire();
            runtime.active = active;
            runtime.desired = desired.clone();
            runtime.error.clear();
            self.refresh_settings_app(cx);
        }
        let (Some(owner), Some(key)) = (active, desired) else {
            return;
        };
        if self
            .settings_runtime
            .dnd
            .read
            .as_ref()
            .is_some_and(|r| now >= r.deadline)
        {
            self.fail_settings_dnd_read(
                cx,
                "Do Not Disturb settings did not arrive. Refresh to try again.",
            );
        }
        if self.settings_runtime.dnd.read.is_none() && now >= self.settings_runtime.dnd.next_read {
            self.start_settings_dnd_read(cx, owner, key, false);
        }
    }
    fn fail_settings_dnd_read(&mut self, cx: &mut Cx, message: &str) {
        let r = &mut self.settings_runtime.dnd;
        r.retire();
        r.error = message.into();
        r.next_read = crate::host::now() + 5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_dnd_result(&mut self, cx: &mut Cx, v: &Value) -> bool {
        let Some(read) = self.settings_runtime.dnd.read.as_ref() else {
            return false;
        };
        if v.get("id").and_then(Value::as_i64) != Some(read.id) {
            return false;
        }
        if v.get("status").and_then(Value::as_i64) != Some(0) {
            self.fail_settings_dnd_read(
                cx,
                "Android could not read DND settings. Refresh to try again.",
            );
        }
        true
    }
    pub(crate) fn settings_dnd_observe(&mut self, cx: &mut Cx, v: &Value) {
        let Some(read) = self.settings_runtime.dnd.read.clone() else {
            return;
        };
        if v.get("request_id").and_then(Value::as_i64) != Some(read.id)
            || !self.settings_runtime.dnd.accepts(read.id, read.owner)
            || self.module_host.settings_client(read.owner.1) != Some(read.owner.0)
            || !self.settings_is_foreground(read.owner)
            || self.current_settings_dnd_read().as_ref() != Some(&read.key)
        {
            return;
        }
        let Some(snapshot) = DndSnapshot::decode(v).filter(|s| read.key.accepts(s)) else {
            self.fail_settings_dnd_read(
                cx,
                "Android returned invalid DND settings. Refresh to try again.",
            );
            return;
        };
        let r = &mut self.settings_runtime.dnd;
        r.snapshot = Some(snapshot);
        r.read = None;
        r.fresh = true;
        r.error.clear();
        r.next_read = crate::host::now() + 5.;
        self.refresh_settings_app(cx);
        self.settings_runtime.dnd.desired = self.current_settings_dnd_read();
    }
    pub(crate) fn settings_dnd_permits(&self, owner: Owner, request: &DndRequest) -> bool {
        let r = &self.settings_runtime.dnd;
        self.settings_is_foreground(owner)
            && r.focused
            && r.active == Some(owner)
            && r.fresh
            && self.current_settings_dnd_read().as_ref() == r.desired.as_ref()
            && r.snapshot.as_ref().is_some_and(|s| s.permits(request))
    }
    pub(crate) fn settings_dnd_resync(&mut self, cx: &mut Cx) {
        self.settings_runtime.dnd.retire();
        self.refresh_settings_app(cx);
    }
}
