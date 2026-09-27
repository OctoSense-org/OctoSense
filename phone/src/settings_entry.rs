//! Public Android Settings entries carry navigation only. No caller-supplied
//! setting, value or mutation can enter the privileged widget path. A package
//! selector on the notification route requires a correlated fresh Apps read.
use makepad_strict_json::Value;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum EntryRoute {Overview,Search,Appearance,Display,DisplayOptions,Sound,SoundFeedback,Wifi,Bluetooth,Apps,DefaultApps,Accounts,Notifications,Dnd,Privacy,Location,Battery,BatteryPolicy,Storage,DateTime,About,System,Updates,AdvancedNetwork,AccessibilityVision,AccessibilityHearing,AccessibilityTextInteraction,CaptionCustom,CaptionLanguage,SystemLanguages,Keyboards}
impl EntryRoute {
    pub const ALL:[Self;31]=[Self::Overview,Self::Search,Self::Appearance,Self::Display,Self::DisplayOptions,Self::Sound,Self::SoundFeedback,Self::Wifi,Self::Bluetooth,Self::Apps,Self::DefaultApps,Self::Accounts,Self::Notifications,Self::Dnd,Self::Privacy,Self::Location,Self::Battery,Self::BatteryPolicy,Self::Storage,Self::DateTime,Self::About,Self::System,Self::Updates,Self::AdvancedNetwork,Self::AccessibilityVision,Self::AccessibilityHearing,Self::AccessibilityTextInteraction,Self::CaptionCustom,Self::CaptionLanguage,Self::SystemLanguages,Self::Keyboards];
    pub fn wire(self)->&'static str {match self {Self::Keyboards=>"keyboards",Self::SystemLanguages=>"system_languages",Self::AccessibilityTextInteraction=>"accessibility_text_interaction",Self::CaptionLanguage=>"caption_language",Self::CaptionCustom=>"caption_custom",Self::Overview=>"overview",Self::Search=>"search",Self::Appearance=>"appearance",Self::Display=>"display",Self::DisplayOptions=>"display_options",Self::Sound=>"sound",Self::SoundFeedback=>"sound_feedback",Self::Wifi=>"wifi",Self::Bluetooth=>"bluetooth",Self::Apps=>"apps",Self::DefaultApps=>"default_apps",Self::Accounts=>"accounts",Self::Notifications=>"notifications",Self::Dnd=>"dnd",Self::Privacy=>"privacy",Self::Location=>"location",Self::Battery=>"battery",Self::BatteryPolicy=>"battery_policy",Self::Storage=>"storage",Self::DateTime=>"date_time",Self::About=>"about",Self::System=>"system",Self::Updates=>"updates",Self::AccessibilityHearing=>"accessibility_hearing",Self::AccessibilityVision=>"accessibility_vision",Self::AdvancedNetwork=>"advanced_network"}}

}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct SettingsEntry {pub id:i64,pub route:EntryRoute,pub package:Option<String>}
impl SettingsEntry {
    pub fn decode(value:&Value)->Option<Self> {
        let Value::Obj(fields)=value else{return None;};
        if value.get("schema")?.as_i64()?!=1{return None;}
        let id=value.get("id")?.as_i64().filter(|id|*id>0)?;let wire=value.get("route")?.as_str()?;
        let package=if wire=="app_notifications" {Some(value.get("package")?.as_str().filter(|name|crate::settings_apps::valid_package(name))?.to_owned())}else{None};
        let keys=if package.is_some(){&["schema","id","route","package"][..]}else{&["schema","id","route"][..]};
        if fields.len()!=keys.len()||keys.iter().any(|expected|fields.iter().filter(|(key,_)|key==expected).count()!=1){return None;}
        Some(Self{id,route:if package.is_some(){EntryRoute::Apps}else{EntryRoute::ALL.into_iter().find(|route|route.wire()==wire)?},package})
    }
}
#[derive(Default)]
pub struct EntryQueue {last_id:i64,pending:Option<SettingsEntry>,resolution:Option<(i64,String)>}
impl EntryQueue {
    pub fn receive(&mut self,value:&Value)->bool {
        let Some(entry)=SettingsEntry::decode(value)else{return false;};if entry.id<=self.last_id{return false;}
        self.last_id=entry.id;self.resolution=entry.package.clone().map(|package|(entry.id,package));self.pending=Some(entry);true
    }
    pub fn pending(&self)->Option<SettingsEntry>{self.pending.clone()}
    pub fn complete(&mut self,id:i64){if self.pending.as_ref().is_some_and(|entry|entry.id==id){self.pending=None;}}
    pub fn permits_resolution(&self,id:i64,package:&str)->bool {self.resolution.as_ref().is_some_and(|(observed_id,observed)|*observed_id==id&&observed==package)}
    pub fn cancel(&mut self){self.pending=None;self.resolution=None;}
}

#[cfg(test)]
mod tests {
    use super::*;use makepad_strict_json::{obj,s};
    fn entry(id:i64,route:&str)->Value{obj(vec![("schema",Value::Int(1)),("id",Value::Int(id)),("route",s(route))])}
    #[test]
    fn public_entry_schema_carries_only_finite_navigation() {
        for route in EntryRoute::ALL {assert_eq!(SettingsEntry::decode(&entry(1,route.wire())),Some(SettingsEntry{id:1,route,package:None}));}
        for route in ["shell","android.settings.WIFI_SETTINGS","wifi?ssid=private","updates_install","","WiFi"] {assert!(SettingsEntry::decode(&entry(1,route)).is_none());}
        assert!(SettingsEntry::decode(&entry(0,"overview")).is_none());assert!(SettingsEntry::decode(&entry(-1,"overview")).is_none());
        for extra in ["query","package","value","intent","operation"] {let Value::Obj(mut fields)=entry(1,"apps")else{unreachable!()};fields.push((extra.into(),s("external")));assert!(SettingsEntry::decode(&Value::Obj(fields)).is_none());}
        assert!(SettingsEntry::decode(&obj(vec![("schema",Value::Int(2)),("id",Value::Int(1)),("route",s("wifi"))])).is_none());
    }
    #[test]
    fn cold_entries_keep_only_latest_and_cannot_replay_after_navigation_or_home() {
        let mut queue=EntryQueue::default();assert!(queue.receive(&entry(1,"wifi")));assert_eq!(queue.pending().unwrap().route,EntryRoute::Wifi);
        assert!(queue.receive(&entry(3,"display")));assert!(!queue.receive(&entry(2,"sound")));assert!(!queue.receive(&entry(4,"invalid")));
        queue.complete(1);assert_eq!(queue.pending().unwrap().route,EntryRoute::Display);queue.complete(3);assert!(queue.pending().is_none());assert!(!queue.receive(&entry(3,"display")));
        assert!(queue.receive(&entry(4,"updates")));queue.cancel();assert!(queue.pending().is_none());assert!(!queue.receive(&entry(4,"updates")));assert!(queue.receive(&entry(5,"overview")));
    }
    #[test]
    fn notification_package_is_only_a_correlated_navigation_selector(){
        let packet=|id,package:&str|obj(vec![("schema",Value::Int(1)),("id",Value::Int(id)),("route",s("app_notifications")),("package",s(package))]);
        let mut queue=EntryQueue::default();assert!(queue.receive(&packet(1,"com.example.first")));
        assert!(queue.permits_resolution(1,"com.example.first"));assert!(!queue.permits_resolution(1,"com.example.other"));assert!(!queue.permits_resolution(2,"com.example.first"));
        queue.complete(1);assert!(queue.pending().is_none());assert!(queue.permits_resolution(1,"com.example.first"),"lookup may finish after receipt ACK");
        assert!(queue.receive(&packet(2,"com.example.second")));assert!(!queue.permits_resolution(1,"com.example.first"));
        queue.cancel();assert!(!queue.permits_resolution(2,"com.example.second"));assert!(!queue.receive(&packet(2,"com.example.second")));
        for package in ["", "no_dot", "com.example/intent", "com..bad", "com.example\n", "中文.app"]{assert!(SettingsEntry::decode(&packet(3,package)).is_none());}
        for key in ["uid","channel","operation","value"]{let Value::Obj(mut fields)=packet(3,"com.example.first")else{unreachable!()};fields.push((key.into(),s("ignored")));assert!(SettingsEntry::decode(&Value::Obj(fields)).is_none());}
        assert!(SettingsEntry::decode(&entry(3,"app_notifications")).is_none());
        assert!(queue.receive(&entry(4,"display")));assert!(!queue.permits_resolution(2,"com.example.second"));
    }
}
