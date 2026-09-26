//! Compiled Settings descriptors, never backend-provided UI or SettingsProvider keys.
use makepad_strict_json::Value;
use crate::settings_text_interaction::{TextControl,TextValue};
use crate::settings_hearing::{NativeFloat,CAPTION_SCALES};
use crate::settings_caption_custom::{CaptionField,CaptionValue};

#[derive(serde::Serialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ControlsPage { #[default] Location, Privacy, Notifications, BatteryPolicy, SoundFeedback, AccessibilityVision, AccessibilityHearing, AccessibilityTextInteraction, CaptionCustom }
impl ControlsPage {
    pub const ALL: [Self;9] = [Self::Location,Self::Privacy,Self::Notifications,Self::BatteryPolicy,Self::SoundFeedback,Self::AccessibilityVision,Self::AccessibilityHearing,Self::AccessibilityTextInteraction,Self::CaptionCustom];
    pub fn wire(self)->&'static str {match self {Self::AccessibilityTextInteraction=>"accessibility_text_interaction",Self::CaptionCustom=>"caption_custom",Self::Location=>"location",Self::Privacy=>"privacy",Self::Notifications=>"notifications",Self::SoundFeedback=>"sound_feedback",Self::AccessibilityHearing=>"accessibility_hearing",Self::AccessibilityVision=>"accessibility_vision",Self::BatteryPolicy=>"battery_policy"}}
    pub fn decode(value:&Value)->Option<Self> {Self::ALL.into_iter().find(|page|Some(page.wire())==value.as_str())}
    pub fn title(self)->&'static str {match self {Self::AccessibilityTextInteraction=>"Accessibility: text and interaction",Self::CaptionCustom=>"Caption appearance",Self::Location=>"Location",Self::Privacy=>"Privacy",Self::Notifications=>"Notifications",Self::SoundFeedback=>"Sound feedback and haptics",Self::AccessibilityHearing=>"Accessibility: hearing",Self::AccessibilityVision=>"Accessibility: colors",Self::BatteryPolicy=>"Battery policy"}}
    pub fn description(self)->&'static str {match self {
        Self::AccessibilityTextInteraction=>"Adjust text, animation and interaction preferences. Automatic clicks require a mouse. Timeouts apply to apps that support Android accessibility recommendations.",
        Self::CaptionCustom=>"Custom caption colors, opacity, edges and typeface. Choosing an appearance option also turns captions on. Apps may render captions differently.",
        Self::AccessibilityHearing=>"Adjust mono audio and left/right balance. Choosing a caption size or style also turns captions on. Preferences apply in apps that support Android captions. Choose caption language and custom appearance below. Hearing devices remain in Android settings.",
        Self::AccessibilityVision=>"Color changes apply across apps. Choose a correction mode independently of turning correction on. Other accessibility features remain in Android settings.",
        Self::Location=>"Location and background scanning for this Android user. Apps still need their own location permission.",
        Self::Privacy=>"Control camera and microphone access across apps. Individual app permissions remain separate.",
        Self::Notifications=>"Notification behavior for this Android user. Priority only uses your current allowed interruptions.",
        Self::SoundFeedback=>"Saved sound and vibration preferences. Silent mode and Do Not Disturb can suppress playback. Charging vibration also depends on charging sounds. Options follow the device’s supported vibration levels.",
        Self::BatteryPolicy=>"Power saving and background battery policy. Availability depends on the device and current charging state.",
    }}
    pub fn controls(self)->&'static [ControlId] {use ControlId::*;match self {
        Self::AccessibilityTextInteraction=>&[TextInteraction(TextControl::HighContrast),TextInteraction(TextControl::BoldText),TextInteraction(TextControl::RemoveAnimations),TextInteraction(TextControl::TouchHold),TextInteraction(TextControl::ActionTimeout),TextInteraction(TextControl::Autoclick),TextInteraction(TextControl::LargePointer)],
        Self::CaptionCustom=>&[CaptionCustom(CaptionField::Typeface),CaptionCustom(CaptionField::Foreground),CaptionCustom(CaptionField::ForegroundOpacity),CaptionCustom(CaptionField::EdgeType),CaptionCustom(CaptionField::EdgeColor),CaptionCustom(CaptionField::Background),CaptionCustom(CaptionField::BackgroundOpacity),CaptionCustom(CaptionField::Window),CaptionCustom(CaptionField::WindowOpacity)],
        Self::Location=>&[LocationEnabled,WifiScanning,BluetoothScanning], Self::Privacy=>&[CameraAccess,MicrophoneAccess],
        Self::Notifications=>&[LockscreenNotifications,LockscreenSensitive,NotificationHistory,NotificationBubbles,DndMode],
        Self::SoundFeedback=>&[ChargingSounds,ChargingVibration,LockSounds,DialpadTones,VibrationEnabled,KeyboardVibration,RingVibration,NotificationVibration,AlarmVibration,MediaVibration,TouchVibration],
        Self::AccessibilityHearing=>&[MonoAudio,AudioBalance,CaptionsEnabled,CaptionsFontScale,CaptionsPreset],
        Self::AccessibilityVision=>&[ColorInversion,ColorCorrection,ColorCorrectionMode],
        Self::BatteryPolicy=>&[BatterySaver,BatteryThreshold,BatteryDisableAt90,AdaptiveBattery],
    }}
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlId { TextInteraction(TextControl), CaptionCustom(CaptionField), LocationEnabled,WifiScanning,BluetoothScanning,CameraAccess,MicrophoneAccess,
    LockscreenNotifications,LockscreenSensitive,NotificationHistory,NotificationBubbles,DndMode,
    BatterySaver,BatteryThreshold,BatteryDisableAt90,AdaptiveBattery,ChargingSounds,ChargingVibration,LockSounds,DialpadTones,VibrationEnabled,KeyboardVibration,RingVibration,NotificationVibration,AlarmVibration,MediaVibration,TouchVibration,ColorInversion,ColorCorrection,ColorCorrectionMode,MonoAudio,AudioBalance,CaptionsEnabled,CaptionsFontScale,CaptionsPreset }
impl ControlId {
    pub fn wire(self)->&'static str {match self {
        Self::TextInteraction(field)=>field.wire(),
        Self::CaptionCustom(field)=>field.wire(),
        Self::MonoAudio=>"mono_audio",Self::AudioBalance=>"audio_balance",Self::CaptionsEnabled=>"captions_enabled",Self::CaptionsFontScale=>"captions_font_scale",Self::CaptionsPreset=>"captions_preset",
        Self::ColorInversion=>"color_inversion",Self::ColorCorrection=>"color_correction",Self::ColorCorrectionMode=>"color_correction_mode",
        Self::LocationEnabled=>"location_enabled",Self::WifiScanning=>"wifi_scanning",Self::BluetoothScanning=>"bluetooth_scanning",
        Self::CameraAccess=>"camera_access",Self::MicrophoneAccess=>"microphone_access",
        Self::LockscreenNotifications=>"lockscreen_notifications",Self::LockscreenSensitive=>"lockscreen_sensitive",
        Self::NotificationHistory=>"notification_history",Self::NotificationBubbles=>"notification_bubbles",Self::DndMode=>"dnd_mode",
        Self::ChargingSounds=>"charging_sounds",Self::ChargingVibration=>"charging_vibration",Self::LockSounds=>"lock_sounds",Self::DialpadTones=>"dialpad_tones",Self::VibrationEnabled=>"vibration_enabled",Self::KeyboardVibration=>"keyboard_vibration",Self::RingVibration=>"ring_vibration",Self::NotificationVibration=>"notification_vibration",Self::AlarmVibration=>"alarm_vibration",Self::MediaVibration=>"media_vibration",Self::TouchVibration=>"touch_vibration",
        Self::BatterySaver=>"battery_saver",Self::BatteryThreshold=>"battery_threshold",Self::BatteryDisableAt90=>"battery_disable_at_90",Self::AdaptiveBattery=>"adaptive_battery",
    }}
    pub fn label(self)->&'static str {match self {
        Self::TextInteraction(field)=>field.label(),
        Self::CaptionCustom(field)=>field.label(),
        Self::MonoAudio=>"Mono audio",Self::AudioBalance=>"Audio balance",Self::CaptionsEnabled=>"Use captions",Self::CaptionsFontScale=>"Caption text size",Self::CaptionsPreset=>"Caption style",
        Self::ColorInversion=>"Color inversion",Self::ColorCorrection=>"Use color correction",Self::ColorCorrectionMode=>"Color correction mode",
        Self::LocationEnabled=>"Use location",Self::WifiScanning=>"Wi-Fi scanning",Self::BluetoothScanning=>"Bluetooth scanning",
        Self::CameraAccess=>"Camera access",Self::MicrophoneAccess=>"Microphone access",
        Self::LockscreenNotifications=>"Show lock screen notifications",Self::LockscreenSensitive=>"Show sensitive content on lock screen",
        Self::NotificationHistory=>"Notification history",Self::NotificationBubbles=>"Allow notification bubbles",Self::DndMode=>"Do Not Disturb",
        Self::ChargingSounds=>"Charging sounds",Self::ChargingVibration=>"Charging vibration",Self::LockSounds=>"Screen lock sounds",Self::DialpadTones=>"Dial pad tones",Self::VibrationEnabled=>"Vibration and haptics",Self::KeyboardVibration=>"Keyboard vibration",Self::RingVibration=>"Ring vibration intensity",Self::NotificationVibration=>"Notification vibration intensity",Self::AlarmVibration=>"Alarm vibration intensity",Self::MediaVibration=>"Media vibration intensity",Self::TouchVibration=>"Touch vibration intensity",
        Self::BatterySaver=>"Battery saver",Self::BatteryThreshold=>"Start battery saver at",Self::BatteryDisableAt90=>"Turn saver off at 90%",Self::AdaptiveBattery=>"Adaptive battery",
    }}
    fn decode(page:ControlsPage,value:&Value)->Option<Self> {page.controls().iter().copied().find(|id|Some(id.wire())==value.as_str())}
    pub fn intensity(self)->bool {matches!(self,Self::RingVibration|Self::NotificationVibration|Self::AlarmVibration|Self::MediaVibration|Self::TouchVibration)}
    pub fn max_options(self)->usize{match self{Self::TextInteraction(field)=>field.options().len(),Self::CaptionCustom(field)=>field.choices().len(),Self::AudioBalance=>201,Self::CaptionsPreset=>6,Self::CaptionsFontScale=>5,Self::BatteryThreshold=>THRESHOLDS.len(),id if id.choice_buttons()=>4,_=>4}}
    pub fn choice_buttons(self)->bool {self.intensity()||matches!(self,Self::TextInteraction(TextControl::TouchHold|TextControl::ActionTimeout))||matches!(self,Self::ColorCorrectionMode|Self::CaptionsFontScale|Self::CaptionsPreset)}
    pub fn chooser(self)->bool{matches!(self,Self::TextInteraction(TextControl::Autoclick))}
    pub fn accepts(self,value:ControlValue,mutation:bool)->bool {
        if let Self::TextInteraction(field)=self{return field.accepts(value,mutation);}
        if let Self::CaptionCustom(field)=self{return matches!(value,ControlValue::CaptionDetail(v) if field.accepts(v,mutation));}
        if self==Self::AudioBalance{return match value{ControlValue::Balance(_)=>!mutation,ControlValue::BalancePercent(n)=>(-100..=100).contains(&n)&&mutation,_=>false};}
        if self==Self::CaptionsFontScale{return matches!(value,ControlValue::CaptionScale(scale) if !mutation||CAPTION_SCALES.contains(&scale.get()));}
        if self==Self::CaptionsPreset{return CAPTION_PRESETS.contains(&value);}
if self==Self::ColorCorrectionMode{return COLOR_CORRECTION_MODES.contains(&value);}if self.intensity(){return matches!(value,ControlValue::Off|ControlValue::DeviceDefault|ControlValue::Low|ControlValue::Medium|ControlValue::High);}match (self,value) {
        (Self::BatteryThreshold,ControlValue::Percent(percent))=>percent<=100&&(!mutation||THRESHOLDS.contains(&percent)),
        (Self::DndMode,value)=>DND_MODES.contains(&value),
        (Self::BatteryThreshold,_)|(_,ControlValue::Percent(_))=>false,
        (_,ControlValue::Off|ControlValue::On)=>true, _=>false,
    }}
}
pub const MAX_CONTROLS:usize=11;
pub const THRESHOLDS: [u8;10] = [0,5,10,15,20,25,30,40,50,75];
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlValue { TextInteraction(TextValue), CaptionDetail(CaptionValue), Balance(NativeFloat),BalancePercent(i16),CaptionScale(NativeFloat),CaptionApp,CaptionWhiteBlack,CaptionBlackWhite,CaptionYellowBlack,CaptionYellowBlue,CaptionCustom, Off, On, Percent(u8), Priority, Alarms, Silence, DeviceDefault, Low, Medium, High, Deuteranomaly, Protanomaly, Tritanomaly, Grayscale }
pub const CAPTION_PRESETS:[ControlValue;6]=[ControlValue::CaptionApp,ControlValue::CaptionWhiteBlack,ControlValue::CaptionBlackWhite,ControlValue::CaptionYellowBlack,ControlValue::CaptionYellowBlue,ControlValue::CaptionCustom];
pub const COLOR_CORRECTION_MODES:[ControlValue;4]=[ControlValue::Deuteranomaly,ControlValue::Protanomaly,ControlValue::Tritanomaly,ControlValue::Grayscale];
pub const DND_MODES:[ControlValue;4]=[ControlValue::Off,ControlValue::Priority,ControlValue::Alarms,ControlValue::Silence];
impl ControlValue {
    pub fn wire(self)->String {match self {Self::TextInteraction(v)=>v.wire(),Self::CaptionDetail(v)=>v.wire(),Self::Balance(value)=>format!("balance:{}",value.wire()),Self::BalancePercent(value)=>format!("balance_percent:{value}"),Self::CaptionScale(value)=>format!("caption_scale:{}",value.wire()),Self::CaptionApp=>"caption_app".into(),Self::CaptionWhiteBlack=>"caption_white_black".into(),Self::CaptionBlackWhite=>"caption_black_white".into(),Self::CaptionYellowBlack=>"caption_yellow_black".into(),Self::CaptionYellowBlue=>"caption_yellow_blue".into(),Self::CaptionCustom=>"caption_custom".into(),Self::Deuteranomaly=>"deuteranomaly".into(),Self::Protanomaly=>"protanomaly".into(),Self::Tritanomaly=>"tritanomaly".into(),Self::Grayscale=>"grayscale".into(),Self::DeviceDefault=>"default".into(),Self::Low=>"low".into(),Self::Medium=>"medium".into(),Self::High=>"high".into(),Self::Off=>"off".into(),Self::On=>"on".into(),Self::Priority=>"priority".into(),Self::Alarms=>"alarms".into(),Self::Silence=>"silence".into(),Self::Percent(value)=>value.to_string()}}
    pub fn label(self)->String {match self {Self::TextInteraction(v)=>v.label(),Self::CaptionDetail(v)=>v.label(),Self::Balance(value)=>value.balance_label(),Self::BalancePercent(0)=>"Center".into(),Self::BalancePercent(value)=>format!("{}% {}",value.abs(),if value<0{"left"}else{"right"}),Self::CaptionScale(value)=>format!("{}%",value.get()*100.),Self::CaptionApp=>"Set by app".into(),Self::CaptionWhiteBlack=>"White on black".into(),Self::CaptionBlackWhite=>"Black on white".into(),Self::CaptionYellowBlack=>"Yellow on black".into(),Self::CaptionYellowBlue=>"Yellow on blue".into(),Self::CaptionCustom=>"Custom".into(),Self::Deuteranomaly=>"Red-green, green weak".into(),Self::Protanomaly=>"Red-green, red weak".into(),Self::Tritanomaly=>"Blue-yellow".into(),Self::Grayscale=>"Grayscale".into(),Self::DeviceDefault=>"Device default".into(),Self::Low=>"Low".into(),Self::Medium=>"Medium".into(),Self::High=>"High".into(),Self::Off=>"Off".into(),Self::On=>"On".into(),Self::Priority=>"Priority only".into(),Self::Alarms=>"Alarms only".into(),Self::Silence=>"Total silence".into(),Self::Percent(0)=>"Never".into(),Self::Percent(value)=>format!("{value}%")}}
    pub fn selected_by(self,observed:Self)->bool{match(self,observed){(Self::TextInteraction(a),Self::TextInteraction(b))=>a.selected_by(b),(Self::Off,Self::TextInteraction(TextValue::Autoclick(false,_)))=>true,_=>self==observed}}
    fn decode(value:&Value)->Option<Self> {let text=value.as_str()?;
        if let Some(v)=TextValue::decode(text){return Some(Self::TextInteraction(v));}
        if let Some(v)=CaptionValue::decode(text){return Some(Self::CaptionDetail(v));}
        if let Some(raw)=text.strip_prefix("balance:"){return NativeFloat::decode(raw,true).map(Self::Balance);}
        if let Some(raw)=text.strip_prefix("balance_percent:"){let n=raw.parse::<i16>().ok()?;return ((-100..=100).contains(&n)&&raw==n.to_string()).then_some(Self::BalancePercent(n));}
        if let Some(raw)=text.strip_prefix("caption_scale:"){return NativeFloat::decode(raw,false).map(Self::CaptionScale);}
        match text {"caption_app"=>Some(Self::CaptionApp),"caption_white_black"=>Some(Self::CaptionWhiteBlack),"caption_black_white"=>Some(Self::CaptionBlackWhite),"caption_yellow_black"=>Some(Self::CaptionYellowBlack),"caption_yellow_blue"=>Some(Self::CaptionYellowBlue),"caption_custom"=>Some(Self::CaptionCustom),"deuteranomaly"=>Some(Self::Deuteranomaly),"protanomaly"=>Some(Self::Protanomaly),"tritanomaly"=>Some(Self::Tritanomaly),"grayscale"=>Some(Self::Grayscale),"default"=>Some(Self::DeviceDefault),"low"=>Some(Self::Low),"medium"=>Some(Self::Medium),"high"=>Some(Self::High),"on"=>Some(Self::On),"off"=>Some(Self::Off),"priority"=>Some(Self::Priority),"alarms"=>Some(Self::Alarms),"silence"=>Some(Self::Silence),text=>{
        let percent=text.parse::<u8>().ok()?; (percent<=100&&text==percent.to_string()).then_some(Self::Percent(percent))
    }}}
}
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability { Available, Restricted, Unsupported, Unavailable }
impl Availability {
    fn decode(value:&Value)->Option<Self> {Some(match value.as_str()? {"available"=>Self::Available,"restricted"=>Self::Restricted,"unsupported"=>Self::Unsupported,"unavailable"=>Self::Unavailable,_=>return None})}
    pub fn label(self)->&'static str {match self {Self::Available=>"Read only",Self::Restricted=>"Restricted by device policy or screen lock",Self::Unsupported=>"Not supported on this device",Self::Unavailable=>"Unavailable"}}
}
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub enum ControlsRequest { Snapshot(ControlsPage), Set {page:ControlsPage,control:ControlId,value:ControlValue} }
impl ControlsRequest {
    pub fn valid(&self)->bool {match self {Self::Snapshot(_)=>true,Self::Set{page,control,value}=>*page!=ControlsPage::CaptionCustom&&page.controls().contains(control)&&control.accepts(*value,true)}}
    pub fn page(&self)->ControlsPage {match self {Self::Snapshot(page)|Self::Set{page,..}=>*page}}
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct ControlObservation {pub id:ControlId,pub value:Option<ControlValue>,pub options:Vec<ControlValue>,pub availability:Availability}
impl ControlObservation {
    /// Buttons target an explicit observed choice. Reading a custom threshold
    /// never coerces it to a preset or invents a default.
    pub fn choice(&self,higher:bool)->Option<ControlValue> {
        if self.availability!=Availability::Available||(self.id==ControlId::DndMode||self.id.choice_buttons()||self.id.chooser()) {return None;}
        let current=self.value?;
        let choice=match current {ControlValue::Balance(value)=>self.options.iter().filter_map(|option|match option{ControlValue::BalancePercent(n) if if higher{(*n as f32)/100.>value.get()}else{(*n as f32)/100.<value.get()}=>Some(*n),_=>None}).reduce(|a,b|if higher{a.min(b)}else{a.max(b)}).map(ControlValue::BalancePercent)?,ControlValue::Percent(value)=>self.options.iter().filter_map(|option|match option {
            ControlValue::Percent(candidate) if if higher {*candidate>value}else{*candidate<value}=>Some(*candidate),_=>None,
        }).reduce(|a,b|if higher {a.min(b)}else{a.max(b)}).map(ControlValue::Percent)?,
            _=>if higher {ControlValue::On}else{ControlValue::Off}};
        (choice!=current&&self.options.contains(&choice)).then_some(choice)
    }
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct ControlsSnapshot {pub request_id:i64,pub page:ControlsPage,pub controls:Vec<ControlObservation>}
impl ControlsSnapshot {
    pub fn decode(value:&Value)->Option<Self> {
        if value.get("schema")?.as_i64()?!=1 {return None;}
        let request_id=value.get("request_id")?.as_i64().filter(|id|*id>0)?;
        let page=ControlsPage::decode(value.get("page")?)?;
        let rows=value.get("controls")?.as_arr()?;
        if rows.len()!=page.controls().len() {return None;}
        let mut controls:Vec<ControlObservation>=Vec::new();
        for row in rows {
            let id=ControlId::decode(page,row.get("id")?)?;
            if controls.iter().any(|row|row.id==id) {return None;}
            let observed=match row.get("value")? {Value::Null=>None,value=>Some(ControlValue::decode(value)?)};
            if observed.is_some_and(|value|!id.accepts(value,false)) {return None;}
            let options=row.get("options")?.as_arr()?;
            if options.len()>id.max_options() {return None;}
            let options=options.iter().map(ControlValue::decode).collect::<Option<Vec<_>>>()?;
            if options.iter().enumerate().any(|(index,value)|!id.accepts(*value,true)||options[..index].contains(value)) {return None;}
            let availability=Availability::decode(row.get("availability")?)?;
            if (availability==Availability::Available&&observed.is_none())
                ||(!options.is_empty()&&(availability!=Availability::Available||observed.is_none())) {return None;}
            controls.push(ControlObservation{id,value:observed,options,availability});
        }
        Some(Self{request_id,page,controls})
    }
    pub fn control(&self,id:ControlId)->Option<&ControlObservation> {self.controls.iter().find(|row|row.id==id)}
    pub fn permits(&self,request:&ControlsRequest)->bool {match request {
        ControlsRequest::Snapshot(page)=>*page==self.page,
        ControlsRequest::Set{page,control,value}=>request.valid()&&*page==self.page&&self.control(*control).is_some_and(|row|
            row.availability==Availability::Available&&row.value.is_some()&&row.options.contains(value)),
    }}
    pub fn clear_actions(&mut self) {for row in &mut self.controls {row.options.clear();}}
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;use makepad_strict_json::{obj,s};
    pub fn snapshot(id:i64,page:ControlsPage)->Value {obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(id)),("page",s(page.wire())),
        ("controls",Value::Arr(page.controls().iter().map(|id|obj(vec![("id",s(id.wire())),("value",s(if let ControlId::TextInteraction(field)=*id {return obj(vec![("id",s(id.wire())),("value",s(field.sample().wire())),("options",Value::Arr(field.options().into_iter().map(|v|s(v.wire())).collect())),("availability",s("available"))]);}else if let ControlId::CaptionCustom(field)=*id {return obj(vec![("id",s(id.wire())),("value",s(field.sample().wire())),("options",Value::Arr(field.choices().into_iter().map(|v|s(v.wire())).collect())),("availability",s("available"))]);}else if *id==ControlId::AudioBalance {"balance:0.23125"}else if *id==ControlId::CaptionsFontScale{"caption_scale:1.0"}else if *id==ControlId::CaptionsPreset{"caption_white_black"}else if *id==ControlId::BatteryThreshold {"23"}else if *id==ControlId::ColorCorrectionMode {"deuteranomaly"}else{"off"})),
            ("options",Value::Arr(if *id==ControlId::AudioBalance {(-100..=100).map(|n|s(format!("balance_percent:{n}"))).collect()}else if *id==ControlId::CaptionsFontScale{CAPTION_SCALES.iter().map(|n|s(ControlValue::CaptionScale(NativeFloat::preset(*n)).wire())).collect()}else if *id==ControlId::CaptionsPreset{CAPTION_PRESETS.iter().map(|v|s(v.wire())).collect()}else if *id==ControlId::BatteryThreshold {THRESHOLDS.iter().map(|value|s(value.to_string())).collect()}else if *id==ControlId::ColorCorrectionMode {COLOR_CORRECTION_MODES.iter().map(|value|s(value.wire())).collect()}else if id.intensity(){vec![s("off"),s("default")]}else if *id==ControlId::DndMode {DND_MODES.iter().map(|value|s(value.wire())).collect()}else{vec![s("on"),s("off")]})),("availability",s("available"))])).collect()))])}
    fn field(value:&mut Value,key:&str,replacement:Value) {let Value::Obj(fields)=value else {panic!()};fields.iter_mut().find(|(name,_)|name==key).unwrap().1=replacement;}
    #[test]
    fn controls_are_a_finite_page_vocabulary_with_authoritative_options() {
        for page in ControlsPage::ALL {assert!(ControlsSnapshot::decode(&snapshot(1,page)).is_some());}
        let mut value=snapshot(1,ControlsPage::Location);
        field(&mut value,"page",s("privacy"));assert!(ControlsSnapshot::decode(&value).is_none());
        let mut value=snapshot(1,ControlsPage::Privacy);
        let Value::Arr(mut rows)=value.get("controls").unwrap().clone() else {panic!()};
        field(&mut rows[0],"id",s("microphone_access"));field(&mut value,"controls",Value::Arr(rows));assert!(ControlsSnapshot::decode(&value).is_none());
        let mut value=snapshot(1,ControlsPage::Location);
        let Value::Arr(mut rows)=value.get("controls").unwrap().clone() else {panic!()};
        field(&mut rows[0],"options",Value::Arr(vec![s("15")]));field(&mut value,"controls",Value::Arr(rows));assert!(ControlsSnapshot::decode(&value).is_none());
        assert!(!ControlsRequest::Set{page:ControlsPage::Privacy,control:ControlId::LocationEnabled,value:ControlValue::On}.valid());
        assert!(!ControlsRequest::Set{page:ControlsPage::BatteryPolicy,control:ControlId::BatteryThreshold,value:ControlValue::Percent(23)}.valid());
    }
    #[test]
    fn dnd_modes_are_finite_observed_choices_not_boolean_or_other_page_controls() {
        let mut state=ControlsSnapshot::decode(&snapshot(1,ControlsPage::Notifications)).unwrap();
        for value in DND_MODES {
            assert!(state.permits(&ControlsRequest::Set{page:ControlsPage::Notifications,control:ControlId::DndMode,value}));
            assert!(!ControlsRequest::Set{page:ControlsPage::Privacy,control:ControlId::DndMode,value}.valid());
        }
        assert!(!ControlId::DndMode.accepts(ControlValue::On,true));
        assert!(!ControlId::CameraAccess.accepts(ControlValue::Priority,true));
        state.clear_actions();
        assert_eq!(state.control(ControlId::DndMode).unwrap().value,Some(ControlValue::Off));
        assert!(!state.permits(&ControlsRequest::Set{page:ControlsPage::Notifications,control:ControlId::DndMode,value:ControlValue::Silence}));
    }
    #[test]
    fn vision_modes_are_independent_finite_choices_with_read_only_and_unknown_states() {
        let page=ControlsPage::AccessibilityVision;
        let mut state=ControlsSnapshot::decode(&snapshot(1,page)).unwrap();
        assert_eq!(state.control(ControlId::ColorCorrection).unwrap().value,Some(ControlValue::Off));
        let mode=state.control(ControlId::ColorCorrectionMode).unwrap();
        assert_eq!(mode.value,Some(ControlValue::Deuteranomaly));assert!(mode.choice(true).is_none());
        assert!(mode.id.choice_buttons());assert!(!mode.id.intensity());
        for value in COLOR_CORRECTION_MODES {
            assert!(state.permits(&ControlsRequest::Set{page,control:ControlId::ColorCorrectionMode,value}));
            assert!(!ControlId::ColorCorrection.accepts(value,true));
            assert!(!ControlsRequest::Set{page:ControlsPage::Privacy,control:ControlId::ColorCorrectionMode,value}.valid());
        }
        assert!(!ControlId::ColorCorrectionMode.accepts(ControlValue::On,true));
        state.clear_actions();assert_eq!(state.control(ControlId::ColorCorrectionMode).unwrap().value,Some(ControlValue::Deuteranomaly));
        assert!(!state.permits(&ControlsRequest::Set{page,control:ControlId::ColorCorrectionMode,value:ControlValue::Grayscale}));
        let mut value=snapshot(2,page);let Value::Arr(mut rows)=value.get("controls").unwrap().clone()else{panic!()};
        field(&mut rows[2],"value",s("12"));field(&mut value,"controls",Value::Arr(rows.clone()));assert!(ControlsSnapshot::decode(&value).is_none());
        field(&mut rows[2],"value",Value::Null);field(&mut rows[2],"options",Value::Arr(vec![]));field(&mut rows[2],"availability",s("unavailable"));
        field(&mut value,"controls",Value::Arr(rows.clone()));assert!(ControlsSnapshot::decode(&value).unwrap().control(ControlId::ColorCorrectionMode).unwrap().value.is_none());
        field(&mut rows[2],"options",Value::Arr(vec![s("grayscale")]));field(&mut value,"controls",Value::Arr(rows));assert!(ControlsSnapshot::decode(&value).is_none());
    }
    #[test]
    fn sound_intensity_preserves_custom_values_and_only_offers_device_authorized_levels() {
        let mut value=snapshot(1,ControlsPage::SoundFeedback);
        let Value::Arr(mut rows)=value.get("controls").unwrap().clone()else{panic!()};
        let row=rows.iter_mut().find(|row|row.get("id").unwrap().as_str()==Some("ring_vibration")).unwrap();
        field(row,"value",s("high"));field(&mut value,"controls",Value::Arr(rows.clone()));
        let mut state=ControlsSnapshot::decode(&value).unwrap();let ring=state.control(ControlId::RingVibration).unwrap();
        assert_eq!(ring.value,Some(ControlValue::High));assert_eq!(ring.options,vec![ControlValue::Off,ControlValue::DeviceDefault]);
        assert!(ring.choice(true).is_none());
        let request=|value|ControlsRequest::Set{page:ControlsPage::SoundFeedback,control:ControlId::RingVibration,value};
        assert!(!state.permits(&request(ControlValue::High)));assert!(state.permits(&request(ControlValue::DeviceDefault)));
        state.clear_actions();assert_eq!(state.control(ControlId::RingVibration).unwrap().value,Some(ControlValue::High));assert!(!state.permits(&request(ControlValue::Off)));
        assert!(!ControlId::ChargingSounds.accepts(ControlValue::DeviceDefault,true));assert!(!ControlId::RingVibration.accepts(ControlValue::On,true));
        let row=rows.iter_mut().find(|row|row.get("id").unwrap().as_str()==Some("ring_vibration")).unwrap();field(row,"value",s("3"));
        field(&mut value,"controls",Value::Arr(rows));assert!(ControlsSnapshot::decode(&value).is_none());
    }
    #[test]
    fn custom_thresholds_stay_observed_and_revocation_preserves_state() {
        let mut state=ControlsSnapshot::decode(&snapshot(2,ControlsPage::BatteryPolicy)).unwrap();
        let row=state.control(ControlId::BatteryThreshold).unwrap();assert_eq!(row.value,Some(ControlValue::Percent(23)));
        assert_eq!(row.choice(false),Some(ControlValue::Percent(20)));assert_eq!(row.choice(true),Some(ControlValue::Percent(25)));
        let request=ControlsRequest::Set{page:state.page,control:ControlId::BatteryThreshold,value:ControlValue::Percent(25)};
        assert!(state.permits(&request));state.clear_actions();assert!(!state.permits(&request));
        assert_eq!(state.control(ControlId::BatteryThreshold).unwrap().value,Some(ControlValue::Percent(23)));
    }
    #[test]
    fn missing_values_never_become_available_or_mutable() {
        let mut value=snapshot(1,ControlsPage::Privacy);let Value::Arr(mut rows)=value.get("controls").unwrap().clone() else {panic!()};
        field(&mut rows[0],"value",Value::Null);field(&mut value,"controls",Value::Arr(rows.clone()));assert!(ControlsSnapshot::decode(&value).is_none());
        field(&mut rows[0],"availability",s("unavailable"));field(&mut value,"controls",Value::Arr(rows.clone()));assert!(ControlsSnapshot::decode(&value).is_none());
        field(&mut rows[0],"options",Value::Arr(vec![]));field(&mut value,"controls",Value::Arr(rows));
        let state=ControlsSnapshot::decode(&value).unwrap();assert!(state.control(ControlId::CameraAccess).unwrap().value.is_none());
        assert!(!state.permits(&ControlsRequest::Set{page:state.page,control:ControlId::CameraAccess,value:ControlValue::On}));
    }
}
