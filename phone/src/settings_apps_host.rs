//! Correlated Apps reads and authorization tied to the live trusted Settings root.
use crate::{App, hub::ClientId, settings_app::{SettingsRequest, SettingsView}, settings_apps::{AppAction, AppDetails, AppTarget, AppsCatalog}};
use makepad_strict_json::{s, Value};
use makepad_widgets::*;

#[derive(Clone, Debug, PartialEq, Eq)]
enum ReadKey {
    Catalog { query: String, include_system: bool, offset: u32, generation: Option<String> },
    Details { target: AppTarget, permission_offset: u32 },
    EntryDetails { entry_id:i64,package:String },
}
impl ReadKey {
    fn from_request(request: &SettingsRequest) -> Option<Self> { match request {
        SettingsRequest::AppsCatalog { query, include_system, offset, generation } => Some(Self::Catalog { query: query.clone(), include_system: *include_system, offset: *offset, generation: generation.clone() }),
        SettingsRequest::AppDetails { target, permission_offset } => Some(Self::Details { target: target.clone(), permission_offset: *permission_offset }),
        SettingsRequest::AppEntryDetails {entry_id,package}=>Some(Self::EntryDetails{entry_id:*entry_id,package:package.clone()}),
        _ => None,
    } }
    fn command(&self) -> (&'static str, Vec<(&'static str, Value)>) { match self {
        Self::Catalog { query, include_system, offset, generation } => ("apps_catalog", vec![("query",s(query)),("include_system",Value::Bool(*include_system)),
            ("offset",Value::Int(*offset as i64)),("generation",generation.as_ref().map(|value| s(value)).unwrap_or(Value::Null))]),
        Self::Details { target, permission_offset } => ("app_details", vec![("package",s(target.package())),("permission_offset",Value::Int(*permission_offset as i64))]),
        Self::EntryDetails {package,..}=>("app_details",vec![("package",s(package)),("permission_offset",Value::Int(0))]),
    } }
    fn accepts_catalog(&self, value: &AppsCatalog) -> bool {
        let Self::Catalog { query, include_system, offset, generation } = self else {return false;};
        query == &value.query && *include_system == value.include_system && if value.stale {
            generation.is_some() && value.offset == 0
        } else { value.offset == *offset && generation.as_ref().is_none_or(|expected| expected == &value.generation) }
    }
    fn accepts_details(&self, value: &AppDetails) -> bool {
        if let Self::EntryDetails {package,..}=self{return package==value.target.package()&&value.permissions_offset==0;}
        let Self::Details { target, permission_offset } = self else {return false;};
        target == &value.target && (value.permissions_offset == *permission_offset
            || (value.permissions_offset == 0 && (!value.exists || *permission_offset >= value.permissions_total)))
    }
}
#[derive(Clone)]
struct Read { id: i64, owner: (ClientId, WidgetUid), key: ReadKey, deadline: f64 }

#[derive(Default)]
pub(crate) struct SettingsAppsRuntime {
    active: Option<(ClientId, WidgetUid)>,
    desired: Option<ReadKey>,
    read: Option<Read>,
    next_read: f64,
    fresh: bool,
    pub(crate) catalog: Option<AppsCatalog>,
    details: Option<AppDetails>,
    pub(crate) error: String,
}
impl SettingsAppsRuntime {
    pub(crate) fn details(&self) -> Option<AppDetails> {
        let mut value = self.details.clone()?;
        if !self.fresh { value.actions.clear(); value.storage.can_request_usage_access = false; }
        Some(value)
    }
    fn observed_target(&self, target: &AppTarget) -> bool {
        self.catalog.as_ref().is_some_and(|catalog| catalog.apps.iter().any(|app| &app.target == target))
            || self.details.as_ref().is_some_and(|details| &details.target == target)
    }
    fn permits(&self, target: &AppTarget, action: AppAction) -> bool {
        self.fresh && self.details.as_ref().is_some_and(|details| details.exists && &details.target == target && details.actions.contains(&action))
            && matches!(&self.desired, Some(ReadKey::Details { target: current, .. }) if current == target)
    }
    fn retire(&mut self) {self.read = None;self.fresh = false;self.next_read = 0.0;}
    pub(crate) fn clear_entry_observations(&mut self){self.retire();self.details=None;self.error.clear();}
}

impl App {
    fn current_settings_apps_read(&self) -> Option<ReadKey> {
        let instance = self.module_host.settings_instance()?;
        let view = instance.root.borrow::<SettingsView>()?;
        ReadKey::from_request(&view.apps_read()?)
    }
    pub(crate) fn settings_apps_read_request(&mut self, cx: &mut Cx, owner: (ClientId, WidgetUid), request: &SettingsRequest) -> bool {
        let Some(key) = ReadKey::from_request(request) else {return false;};
        if !cfg!(target_os="android") {return true;}
        if self.current_settings_apps_read().as_ref() != Some(&key) {return true;}
        if !self.settings_entry_read_permitted(&key){return true;}
        if let ReadKey::Details { target, .. } = &key {
            if !self.settings_runtime.apps.observed_target(target) {
                self.settings_runtime.apps.error = "Refresh the app list before opening this app.".into();
                self.refresh_settings_app(cx);return true;
            }
        }
        self.start_settings_apps_read(cx, owner, key, true);
        true
    }
    fn start_settings_apps_read(&mut self, cx: &mut Cx, owner: (ClientId, WidgetUid), key: ReadKey, manual: bool) {
        if self.module_host.settings_client(owner.1) != Some(owner.0)||!self.settings_entry_read_permitted(&key) {return;}
        let (operation, fields) = key.command();
        let id = self.android_command_id(cx, "launcher", operation, fields);
        let apps = &mut self.settings_runtime.apps;
        if apps.desired.as_ref() != Some(&key) {apps.fresh = false;}
        apps.desired = Some(key.clone());
        apps.read = Some(Read { id, owner, key, deadline: crate::host::now()+20.0 });
        apps.next_read = crate::host::now()+5.0;
        if manual {apps.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_apps_tick(&mut self, cx: &mut Cx, visible: Option<(ClientId, WidgetUid)>, now: f64) {
        let desired = visible.and_then(|_| self.current_settings_apps_read());
        let changed = self.settings_runtime.apps.active != visible || self.settings_runtime.apps.desired != desired;
        if changed {
            let apps = &mut self.settings_runtime.apps;
            apps.retire();apps.active = visible;apps.desired = desired.clone();apps.error.clear();
            self.refresh_settings_app(cx);
        }
        let (Some(owner), Some(key)) = (visible, desired) else {return;};
        if !self.settings_entry_read_permitted(&key){return;}
        if let ReadKey::Details {target,..} = &key {
            if !self.settings_runtime.apps.observed_target(target) {return;}
        }
        if self.settings_runtime.apps.read.as_ref().is_some_and(|read| now >= read.deadline) {
            self.fail_settings_apps_read(cx, "App information did not arrive. Refresh to try again.");
        }
        let apps = &self.settings_runtime.apps;
        if apps.read.is_none() && now >= apps.next_read {self.start_settings_apps_read(cx, owner, key, false);}
    }
    fn fail_settings_apps_read(&mut self, cx: &mut Cx, message: &str) {
        let apps = &mut self.settings_runtime.apps;
        apps.read = None;apps.fresh = false;apps.error = message.into();apps.next_read = crate::host::now()+5.0;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_apps_result(&mut self, cx: &mut Cx, value: &Value) -> bool {
        let Some(read) = &self.settings_runtime.apps.read else {return false;};
        if value.get("id").and_then(Value::as_i64) != Some(read.id) {return false;}
        if value.get("status").and_then(Value::as_i64) != Some(0) {
            self.fail_settings_apps_read(cx, "Android could not read app information. Refresh to try again.");
        }
        true
    }
    pub(crate) fn settings_apps_observe(&mut self, cx: &mut Cx, channel: &str, value: &Value) {
        let Some(read) = self.settings_runtime.apps.read.clone() else {return;};
        if value.get("request_id").and_then(Value::as_i64) != Some(read.id)
            || self.module_host.settings_client(read.owner.1) != Some(read.owner.0)
            || self.current_settings_apps_read().as_ref() != Some(&read.key)||!self.settings_entry_read_permitted(&read.key) {return;}
        if channel == "launcher.apps_catalog" {
            let Some(catalog) = AppsCatalog::decode(value).filter(|catalog| read.key.accepts_catalog(catalog)) else {
                self.fail_settings_apps_read(cx,"Android returned an invalid app list. Refresh to try again.");return;
            };
            self.settings_runtime.apps.catalog = Some(catalog);
        } else {
            let Some(details) = AppDetails::decode(value).filter(|details| read.key.accepts_details(details)) else {
                self.fail_settings_apps_read(cx,"Android returned invalid app details. Refresh to try again.");return;
            };
            if let ReadKey::EntryDetails {entry_id,..}=&read.key {
                let Some(instance)=self.module_host.settings_instance()else{return;};
                let(root,vm_id)=(instance.root.clone(),instance.vm_id);
                let isolate=makepad_widgets::widget_async::enter_isolate(cx,vm_id);
                let adopted=root.borrow_mut::<SettingsView>().is_some_and(|mut view|view.resolve_notification_entry(cx,*entry_id,&details));
                makepad_widgets::widget_async::leave_isolate(cx,isolate);
                if !adopted{return;}
            }
            self.settings_runtime.apps.details = Some(details);
        }
        let apps = &mut self.settings_runtime.apps;
        apps.read = None;apps.fresh = true;apps.error.clear();apps.next_read = crate::host::now()+5.0;
        self.refresh_settings_app(cx);
        // The view may adopt a new catalog generation or reset an invalidated
        // page. Treat that as this read's result, not another immediate request.
        self.settings_runtime.apps.desired = self.current_settings_apps_read();
    }
    pub(crate) fn settings_apps_permits(&self, target: &AppTarget, action: AppAction) -> bool {
        self.settings_runtime.apps.permits(target,action)
            && matches!(self.current_settings_apps_read(),Some(ReadKey::Details {target:current,..}) if current == *target)
    }
    pub(crate) fn settings_apps_usage_permitted(&self) -> bool {
        let apps = &self.settings_runtime.apps;
        apps.fresh && apps.details.as_ref().is_some_and(|details| details.exists && details.storage.can_request_usage_access
            && matches!(self.current_settings_apps_read(),Some(ReadKey::Details {target,..}) if target == details.target))
    }
    pub(crate) fn settings_apps_resync(&mut self, cx: &mut Cx) {
        self.settings_runtime.apps.retire();self.refresh_settings_app(cx);
    }
    fn settings_entry_read_permitted(&self,key:&ReadKey)->bool{
        match key{ReadKey::EntryDetails{entry_id,package}=>self.settings_runtime.entries.permits_resolution(*entry_id,package),_=>true}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn catalog(query: &str, generation: &str, offset: u32, stale: bool) -> AppsCatalog {
        AppsCatalog { request_id:7, query:query.into(),include_system:false,offset,page_size:20,total:40,generation:generation.into(),stale,apps:vec![] }
    }
    #[test]
    fn a_late_app_page_cannot_replace_a_different_filter_or_generation() {
        let key=ReadKey::Catalog {query:"mail".into(),include_system:false,offset:20,generation:Some("a".repeat(64))};
        assert!(!key.accepts_catalog(&catalog("camera",&"a".repeat(64),20,false)));
        assert!(!key.accepts_catalog(&catalog("mail",&"b".repeat(64),20,false)));
        assert!(key.accepts_catalog(&catalog("mail",&"a".repeat(64),20,false)));
        assert!(key.accepts_catalog(&catalog("mail",&"b".repeat(64),0,true)));
        assert!(!key.accepts_catalog(&catalog("mail",&"b".repeat(64),20,true)));
    }
    #[test]
    fn changing_details_target_cannot_accept_another_apps_removal() {
        let decode=|name:&str| AppDetails::decode(&makepad_strict_json::parse(format!(r#"{{"schema":1,"request_id":7,"package":"{name}","exists":false}}"#).as_bytes()).unwrap()).unwrap();
        let first=decode("com.example.first");let second=decode("com.example.second");
        let key=ReadKey::Details {target:first.target.clone(),permission_offset:20};
        assert!(key.accepts_details(&first));assert!(!key.accepts_details(&second));
        let mut runtime=SettingsAppsRuntime {fresh:true,desired:Some(key),details:Some(first),..Default::default()};
        assert!(!runtime.permits(&second.target,AppAction::Uninstall));
        runtime.retire();assert!(!runtime.fresh);
    }
    #[test]
    fn external_package_lookup_accepts_only_its_observation_and_never_supplies_action_authority(){
        let details=AppDetails::decode(&crate::settings_apps::tests::detail("com.example.first",0,20)).unwrap();
        let other=AppDetails::decode(&crate::settings_apps::tests::detail("com.example.other",0,20)).unwrap();
        let key=ReadKey::EntryDetails{entry_id:3,package:"com.example.first".into()};
        assert!(key.accepts_details(&details));assert!(!key.accepts_details(&other));
        let mut runtime=SettingsAppsRuntime{fresh:true,desired:Some(key),details:Some(details.clone()),..Default::default()};
        assert!(!runtime.permits(&details.target,AppAction::Notifications),"entry lookup itself is not an actionable details view");
        runtime.clear_entry_observations();assert!(runtime.details().is_none());assert!(!runtime.observed_target(&details.target));
    }
}
