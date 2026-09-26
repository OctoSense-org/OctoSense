//! Account observations and commands stay within a foreground trusted Settings root.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_accounts::{AccountDetails,AccountsRead,AccountsRequest,AccountsSnapshot}};
use makepad_strict_json::{s,Value};
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone)]
struct Read {id:i64,owner:Owner,key:AccountsRead,deadline:f64}
#[derive(Default)]
pub(crate) struct SettingsAccountsRuntime {
    active:Option<(Owner,AccountsRead)>,read:Option<Read>,next_read:f64,fresh:bool,
    snapshot:Option<AccountsSnapshot>,details:Option<AccountDetails>,pub(crate) error:String,
}
impl SettingsAccountsRuntime {
    pub(crate) fn snapshot(&self)->Option<AccountsSnapshot> {
        let mut state=self.snapshot.clone()?;if !self.fresh||!matches!(self.active,Some((_,AccountsRead::Snapshot))) {state.clear_actions();}Some(state)
    }
    pub(crate) fn details(&self)->Option<AccountDetails> {
        let mut state=self.details.clone()?;if !self.fresh||!self.active.as_ref().is_some_and(|(_,key)|*key==AccountsRead::Details(state.target.clone())) {state.clear_actions();}Some(state)
    }
    fn retire(&mut self) {self.read=None;self.fresh=false;self.next_read=0.;}
    fn accepts(&self,id:i64,owner:Owner,key:&AccountsRead)->bool {
        self.active.as_ref()==Some(&(owner,key.clone()))&&self.read.as_ref().is_some_and(|read|read.id==id&&read.owner==owner&&&read.key==key)
    }
}
impl App {
    fn settings_accounts_visible(&self)->Option<AccountsRead> {
        self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>().and_then(|view|view.accounts_read()))
    }
    pub(crate) fn settings_accounts_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool {
        let SettingsRequest::Accounts(AccountsRequest::Read(key))=request else {return false;};
        if cfg!(target_os="android")&&self.settings_is_foreground(owner)&&self.settings_accounts_visible().as_ref()==Some(key) {self.start_settings_accounts_read(cx,owner,key.clone(),true);}
        true
    }
    fn start_settings_accounts_read(&mut self,cx:&mut Cx,owner:Owner,key:AccountsRead,manual:bool) {
        if self.module_host.settings_client(owner.1)!=Some(owner.0) {return;}
        if self.settings_runtime.accounts.read.as_ref().is_some_and(|read|read.owner==owner&&read.key==key) {return;}
        if self.settings_runtime.accounts.active.as_ref()!=Some(&(owner,key.clone())) {self.settings_runtime.accounts.retire();}
        let (operation,fields)=match &key {AccountsRead::Snapshot=>("accounts_snapshot",vec![]),AccountsRead::Details(target)=>("account_details",vec![("key",s(target.key()))])};
        let id=self.android_command_id(cx,"launcher",operation,fields);
        let state=&mut self.settings_runtime.accounts;state.active=Some((owner,key.clone()));state.read=Some(Read{id,owner,key,deadline:crate::host::now()+20.});state.next_read=crate::host::now()+5.;
        if manual {state.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_accounts_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64) {
        let active=visible.and_then(|owner|self.settings_accounts_visible().map(|key|(owner,key)));
        if self.settings_runtime.accounts.active!=active {let state=&mut self.settings_runtime.accounts;state.retire();state.active=active.clone();state.error.clear();self.refresh_settings_app(cx);}
        let Some((owner,key))=active else {return;};
        if self.settings_runtime.accounts.read.as_ref().is_some_and(|read|now>=read.deadline) {self.fail_settings_accounts_read(cx,"Account information did not arrive. Refresh to try again.");}
        if self.settings_runtime.accounts.read.is_none()&&now>=self.settings_runtime.accounts.next_read {self.start_settings_accounts_read(cx,owner,key,false);}
    }
    fn fail_settings_accounts_read(&mut self,cx:&mut Cx,message:&str) {
        let state=&mut self.settings_runtime.accounts;state.read=None;state.fresh=false;state.error=message.into();state.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_accounts_result(&mut self,cx:&mut Cx,value:&Value)->bool {
        let Some(read)=self.settings_runtime.accounts.read.as_ref() else {return false;};
        if value.get("id").and_then(Value::as_i64)!=Some(read.id) {return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0) {self.fail_settings_accounts_read(cx,"Android could not read accounts. Refresh to try again.");}true
    }
    pub(crate) fn settings_accounts_observe(&mut self,cx:&mut Cx,value:&Value,details:bool) {
        let Some(read)=self.settings_runtime.accounts.read.clone() else {return;};
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)||!self.settings_runtime.accounts.accepts(read.id,read.owner,&read.key)
            ||self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)||!self.settings_is_foreground(read.owner)||self.settings_accounts_visible().as_ref()!=Some(&read.key) {return;}
        let valid=match (&read.key,details) {
            (AccountsRead::Snapshot,false)=>AccountsSnapshot::decode(value).map(|state|self.settings_runtime.accounts.snapshot=Some(state)).is_some(),
            (AccountsRead::Details(target),true)=>AccountDetails::decode(value).filter(|state|&state.target==target).map(|state|self.settings_runtime.accounts.details=Some(state)).is_some(),_=>false,
        };
        if !valid {self.fail_settings_accounts_read(cx,"Android returned invalid account information. Refresh to try again.");return;}
        let state=&mut self.settings_runtime.accounts;state.read=None;state.fresh=true;state.error.clear();state.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_accounts_permits(&self,owner:Owner,request:&AccountsRequest)->bool {
        if !self.settings_is_foreground(owner) {return false;}
        let state=&self.settings_runtime.accounts;let Some(key)=self.settings_accounts_visible() else {return false;};
        if !state.fresh||state.active.as_ref()!=Some(&(owner,key.clone())) {return false;}
        let authorized=match key {AccountsRead::Snapshot=>state.snapshot.as_ref().is_some_and(|state|state.permits(request)),AccountsRead::Details(_)=>state.details.as_ref().is_some_and(|state|state.permits(request))};
        authorized&&self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>().map(|view|view.accounts_context_permits(request))).unwrap_or(false)
    }
    pub(crate) fn settings_accounts_resync(&mut self,cx:&mut Cx) {self.settings_runtime.accounts.retire();self.refresh_settings_app(cx);}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn account_read_authority_cannot_cross_page_root_or_retired_lifetime() {
        let owner=(1,WidgetUid(10));let key=AccountsRead::Snapshot;
        let mut state=SettingsAccountsRuntime{active:Some((owner,key.clone())),read:Some(Read{id:3,owner,key:key.clone(),deadline:20.}),fresh:true,
            snapshot:AccountsSnapshot::decode(&crate::settings_accounts::tests::snapshot(3,1)),..Default::default()};
        assert!(state.accepts(3,owner,&key));assert!(!state.accepts(2,owner,&key));assert!(!state.accepts(3,(1,WidgetUid(11)),&key));
        let detail=AccountsRead::Details(state.snapshot.as_ref().unwrap().accounts[0].target.clone());assert!(!state.accepts(3,owner,&detail));
        state.retire();assert!(!state.accepts(3,owner,&key));let cached=state.snapshot().unwrap();assert!(!cached.can_set_master_sync);assert!(!cached.providers[0].can_add);assert_eq!(cached.accounts[0].name,"Account 0");
    }
}
