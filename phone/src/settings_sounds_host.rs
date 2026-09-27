//! Sound browsing/preview/save are bound to the current trusted Settings root.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_sounds::{SoundType,SoundsRead,SoundsRequest,SoundsSnapshot,SoundsStatus}};
use makepad_strict_json::Value;
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone)]
struct Read{id:i64,owner:Owner,request:SoundsRead,deadline:f64}
#[derive(Clone)]
struct Mutation{id:i64,owner:Owner,request:SoundsRequest,deadline:f64}
#[derive(Default)]
pub(crate) struct SettingsSoundsRuntime {
    stop_ids:Vec<i64>,active:Option<(Owner,SoundType)>,read:Option<Read>,mutation:Option<Mutation>,next_read:f64,
    pub(crate) snapshot:Option<SoundsSnapshot>,pub(crate) error:String,
}
impl SettingsSoundsRuntime {
    pub(crate) fn loading(&self)->bool{self.read.is_some()||self.mutation.is_some()}
    fn accepts(&self,id:i64,owner:Owner,kind:SoundType)->bool{self.active==Some((owner,kind))&&self.read.as_ref().is_some_and(|read|read.id==id&&read.owner==owner&&read.request.kind==kind)}
    fn retire(&mut self){self.read=None;self.mutation=None;self.snapshot=None;self.error.clear();self.next_read=0.;}
}
impl App {
    fn sounds_current_read(&self,owner:Owner)->Option<SoundsRead>{
        if !self.settings_is_foreground(owner){return None;}
        self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>().and_then(|view|view.sounds_read()))
    }
    pub(crate) fn settings_sounds_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool {
        let SettingsRequest::Sounds(request)=request else{return false;};
        if !cfg!(target_os="android"){return true;}
        if matches!(request,SoundsRequest::Stop){self.stop_settings_sound(cx);return true;}
        let Some(current)=self.sounds_current_read(owner)else{return true;};
        if let SoundsRequest::Snapshot(read)=request {
            if read.valid()&&read.kind==current.kind&&(read.key.is_none()||self.settings_runtime.sounds.snapshot.as_ref().is_some_and(|state|state.key==read.key&&state.kind==read.kind)) {
                self.start_sound_read(cx,owner,read.clone());
            }
            return true;
        }
        let runtime=&self.settings_runtime.sounds;
        if runtime.loading()||runtime.active!=Some((owner,current.kind))||!runtime.snapshot.as_ref().is_some_and(|state|state.permits(request)){return true;}
        let (operation,fields)=request.command();let id=self.android_command_id(cx,"launcher",operation,fields);
        self.settings_runtime.sounds.mutation=Some(Mutation{id,owner,request:request.clone(),deadline:crate::host::now()+20.});
        self.settings_runtime.sounds.error.clear();self.refresh_settings_app(cx);true
    }
    fn stop_settings_sound(&mut self,cx:&mut Cx){let id=self.android_command_id(cx,"launcher","sound_stop",vec![]);let ids=&mut self.settings_runtime.sounds.stop_ids;ids.push(id);if ids.len()>32{ids.remove(0);}}
    fn start_sound_read(&mut self,cx:&mut Cx,owner:Owner,request:SoundsRead){
        if self.settings_runtime.sounds.read.as_ref().is_some_and(|read|read.owner==owner&&read.request==request){return;}
        let (operation,fields)=SoundsRequest::Snapshot(request.clone()).command();let id=self.android_command_id(cx,"launcher",operation,fields);
        let runtime=&mut self.settings_runtime.sounds;
        if runtime.active!=Some((owner,request.kind)){runtime.retire();}
        runtime.active=Some((owner,request.kind));runtime.read=Some(Read{id,owner,request,deadline:crate::host::now()+20.});runtime.error.clear();runtime.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_sounds_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64){
        let current=visible.and_then(|owner|self.sounds_current_read(owner).map(|read|(owner,read)));
        let active=current.as_ref().map(|(owner,read)|(*owner,read.kind));
        if self.settings_runtime.sounds.active!=active {
            if self.settings_runtime.sounds.active.is_some(){self.stop_settings_sound(cx);}
            self.settings_runtime.sounds.retire();self.settings_runtime.sounds.active=active;self.refresh_settings_app(cx);
        }
        let Some((owner,read))=current else{return;};
        if self.settings_runtime.sounds.read.as_ref().is_some_and(|read|now>=read.deadline)||self.settings_runtime.sounds.mutation.as_ref().is_some_and(|op|now>=op.deadline){
            self.stop_settings_sound(cx);self.settings_runtime.sounds.retire();self.settings_runtime.sounds.active=active;
            self.settings_runtime.sounds.error="Sound information did not arrive. Refresh to try again.".into();self.settings_runtime.sounds.next_read=now+5.;self.refresh_settings_app(cx);return;
        }
        let runtime=&self.settings_runtime.sounds;
        if !runtime.loading()&&now>=runtime.next_read&&!runtime.snapshot.as_ref().is_some_and(|state|state.status==SoundsStatus::Expired){self.start_sound_read(cx,owner,read);}
    }
    pub(crate) fn settings_sounds_observe(&mut self,cx:&mut Cx,value:&Value){
        let Some(read)=self.settings_runtime.sounds.read.clone()else{return;};
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)||!self.settings_runtime.sounds.accepts(read.id,read.owner,read.request.kind)||self.sounds_current_read(read.owner).is_none_or(|current|current.kind!=read.request.kind){return;}
        let state=SoundsSnapshot::decode(value).filter(|state|state.kind==read.request.kind&&(state.status!=SoundsStatus::Ready||state.offset==read.request.offset&&(read.request.key.is_none()||read.request.key==state.key)));
        let runtime=&mut self.settings_runtime.sounds;runtime.read=None;runtime.next_read=crate::host::now()+5.;
        if let Some(state)=state{runtime.snapshot=Some(state);runtime.error.clear();}else{runtime.snapshot=None;runtime.error="Android returned invalid sound information. Refresh to try again.".into();}
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_sounds_result(&mut self,cx:&mut Cx,value:&Value)->bool{
        let id=value.get("id").and_then(Value::as_i64);let status=value.get("status").and_then(Value::as_i64);
        if let Some(index)=self.settings_runtime.sounds.stop_ids.iter().position(|old|Some(*old)==id){if status!=Some(0){self.settings_runtime.sounds.stop_ids.remove(index);}return true;}
        if self.settings_runtime.sounds.read.as_ref().is_some_and(|read|Some(read.id)==id){
            if status!=Some(0){let runtime=&mut self.settings_runtime.sounds;runtime.read=None;runtime.snapshot=None;runtime.error="Sounds could not be read. Refresh to try again.".into();runtime.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);}return true;
        }
        let Some(mutation)=self.settings_runtime.sounds.mutation.clone().filter(|op|Some(op.id)==id)else{return false;};
        if status==Some(0){return true;}
        self.settings_runtime.sounds.mutation=None;
        if self.sounds_current_read(mutation.owner).is_none(){return true;}
        let reason=value.get("reason").and_then(Value::as_str).unwrap_or("");
        self.settings_runtime.sounds.error=match reason {
            "sound_applied"=>"Sound saved.","sound_preview_started"|"sound_preview_requested"=>"Preview requested for up to five seconds. Volume and Do Not Disturb still apply.","sound_silent"=>"Silent has no preview.","sound_stopped"|"settings_opened"=>"",
            "sound_target_changed"=>"The sound selection changed. Refresh and choose again.","sound_restricted"=>"Sound controls are restricted or the owner account is locked.","sound_unconfirmed"=>"Android did not confirm that the sound was saved. Refresh to check.",_=>"This sound is unavailable. Refresh to try again.",
        }.into();
        if matches!(mutation.request,SoundsRequest::Save{..}|SoundsRequest::Access){self.settings_runtime.sounds.snapshot=None;self.settings_runtime.sounds.next_read=0.;}
        self.refresh_settings_app(cx);true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn sound_responses_cannot_cross_editor_roots_types_or_lifetimes() {
        let owner=(1,WidgetUid(5));let kind=SoundType::Ringtone;
        let mut runtime=SettingsSoundsRuntime{active:Some((owner,kind)),read:Some(Read{id:4,owner,request:SoundsRead::default(),deadline:20.}),snapshot:SoundsSnapshot::decode(&crate::settings_sounds::tests::snapshot(3,20,0)),stop_ids:vec![2],..Default::default()};
        assert!(runtime.accepts(4,owner,kind));assert!(!runtime.accepts(3,owner,kind));assert!(!runtime.accepts(4,(1,WidgetUid(6)),kind));assert!(!runtime.accepts(4,owner,SoundType::Alarm));
        runtime.retire();assert!(!runtime.loading());assert!(!runtime.accepts(4,owner,kind));assert!(runtime.snapshot.is_none());assert_eq!(runtime.stop_ids,vec![2],"cleanup acknowledgements remain correlated after leaving Settings");
    }
}
