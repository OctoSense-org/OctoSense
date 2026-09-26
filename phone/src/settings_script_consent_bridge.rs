//! Finite transport decoding for script-owned consent and language flows.
//! Native models and services retain authority; this layer makes no UI decisions.
use crate::settings_app::{SettingsRequest, SettingsSnapshot};
use crate::settings_apps::AppTarget;
use crate::settings_roles::{RoleId, RoleKey, RolesRead, RolesRequest};
use crate::settings_permissions::{PermissionGroup, PermissionKey, PermissionsRead, PermissionsRequest};
use crate::settings_app_language::{LanguageKey, LanguageRead, AppLanguageRequest};
use crate::settings_caption_language::{CaptionLanguageKey, CaptionLanguageRead, CaptionLanguageRequest};
use crate::settings_system_language::{SystemLanguageKey, SystemLanguageRead, SystemLanguageRequest, SystemLanguageAvailability};
use crate::settings_keyboards::{KeyboardKey, KeyboardRead, KeyboardOperation, KeyboardRequest};
use makepad_strict_json::Value;

fn exact(value:&Value,keys:&[&str])->Option<()> {
    let Value::Obj(fields)=value else{return None};
    (fields.len()==keys.len()&&keys.iter().all(|key|value.get(key).is_some())).then_some(())
}
fn text<'a>(value:&'a Value,key:&str)->Option<&'a str>{value.get(key)?.as_str()}
fn offset(value:&Value)->Option<u32>{u32::try_from(value.get("offset")?.as_i64()?).ok()}
fn optional<T>(value:&Value,key:&str,decode:impl FnOnce(&Value)->Option<T>)->Option<Option<T>> {
    match value.get(key)?{Value::Null=>Some(None),value=>decode(value).map(Some)}
}
fn role(value:&Value)->Option<RoleId>{RoleId::ALL.into_iter().find(|r|Some(r.wire())==value.as_str())}
fn group(value:&Value)->Option<PermissionGroup>{PermissionGroup::ALL.into_iter().find(|g|Some(g.wire())==value.as_str())}
fn app(value:&Value,key:&str,observed:&SettingsSnapshot)->Option<AppTarget>{
    let target=AppTarget::decode(value.get(key)?)?;
    let details=observed.app_details.as_ref()?;
    (details.exists&&details.target==target).then_some(target)
}

fn read_app(value:&Value,key:&str,observed:Option<&SettingsSnapshot>)->Option<AppTarget>{
    match observed{Some(observed)=>app(value,key,observed),None=>AppTarget::decode(value.get(key)?)}
}
/// Syntax-only read metadata. This does not authorize a native call or mutation;
/// existing hosts recheck owner, current package, focus and request correlation.
pub(crate) fn subscription(value:&Value)->Option<SettingsRequest>{
    if text(value,"kind")? != "read" {return None} read(value,None)
}

fn read(value:&Value,observed:Option<&SettingsSnapshot>)->Option<SettingsRequest>{
    Some(match text(value,"domain")? {
        "roles"=>{
            exact(value,&["kind","domain","role","offset","generation"])?;
            let read=RolesRead{role:optional(value,"role",role)?,offset:offset(value)?,generation:optional(value,"generation",RoleKey::decode)?};
            if !read.valid(){return None} SettingsRequest::Roles(RolesRequest::Snapshot(read))
        }
        "permissions"=>{
            exact(value,&["kind","domain","package","group","offset","generation"])?;
            let read=PermissionsRead{package:read_app(value,"package",observed)?,group:optional(value,"group",group)?,offset:offset(value)?,generation:optional(value,"generation",PermissionKey::decode)?};
            if !read.valid(){return None} SettingsRequest::Permissions(PermissionsRequest::Snapshot(read))
        }
        "app_language"=>{
            exact(value,&["kind","domain","target","key","parent","query","offset"])?;
            let read=LanguageRead{target:read_app(value,"target",observed)?,key:optional(value,"key",LanguageKey::decode)?,parent:optional(value,"parent",LanguageKey::decode)?,query:text(value,"query")?.into(),offset:offset(value)?};
            if !read.valid(){return None} SettingsRequest::AppLanguage(AppLanguageRequest::Snapshot(read))
        }
        "caption_language"=>{
            exact(value,&["kind","domain","key","query","offset"])?;
            let read=CaptionLanguageRead{key:optional(value,"key",CaptionLanguageKey::decode)?,query:text(value,"query")?.into(),offset:offset(value)?};
            if !read.valid(){return None} SettingsRequest::CaptionLanguage(CaptionLanguageRequest::Snapshot(read))
        }
        "system_languages"=>{
            exact(value,&["kind","domain","key","parent","query","offset"])?;
            let read=SystemLanguageRead{key:optional(value,"key",SystemLanguageKey::decode)?,parent:optional(value,"parent",SystemLanguageKey::decode)?,query:text(value,"query")?.into(),offset:offset(value)?};
            if !read.valid(){return None} SettingsRequest::SystemLanguages(SystemLanguageRequest::Snapshot(read))
        }
        "keyboards"=>{
            exact(value,&["kind","domain","query","offset"])?;
            let read=KeyboardRead{query:text(value,"query")?.into(),offset:offset(value)?};
            if !read.valid(){return None} SettingsRequest::Keyboard(KeyboardRequest::Snapshot(read))
        }
        _=>return None,
    })
}

pub fn request(value:&Value,observed:&SettingsSnapshot)->Option<SettingsRequest>{
    if !observed.android{return None}
    Some(match text(value,"kind")? {
        "read"=>return read(value,Some(observed)),
        "roles"=>{
            exact(value,&["kind","role","key","target"])?;
            let snapshot=observed.roles.as_ref()?;
            let request=RolesRequest::Confirm{role:role(value.get("role")?)?,key:RoleKey::decode(value.get("key")?)?,target:RoleKey::decode(value.get("target")?)?};
            if !snapshot.permits(&request){return None} SettingsRequest::Roles(request)
        }
        "permission"=>{
            exact(value,&["kind","package","group","key","target"])?;
            let snapshot=observed.permissions.as_ref()?;
            let request=PermissionsRequest::Choose{package:app(value,"package",observed)?,group:group(value.get("group")?)?,key:PermissionKey::decode(value.get("key")?)?,target:PermissionKey::decode(value.get("target")?)?};
            if !snapshot.permits(&request){return None} SettingsRequest::Permissions(request)
        }
        "app_language"=>{
            exact(value,&["kind","target","key","choice"])?;
            let snapshot=observed.app_language.as_ref()?;
            let request=AppLanguageRequest::Select{target:app(value,"target",observed)?,key:LanguageKey::decode(value.get("key")?)?,choice:LanguageKey::decode(value.get("choice")?)?};
            if !snapshot.permits(&request){return None} SettingsRequest::AppLanguage(request)
        }
        "caption_language"=>{
            exact(value,&["kind","key","choice"])?;
            let snapshot=observed.caption_language.as_ref()?;
            let request=CaptionLanguageRequest::Select{key:CaptionLanguageKey::decode(value.get("key")?)?,choice:CaptionLanguageKey::decode(value.get("choice")?)?};
            if !snapshot.permits(&request){return None} SettingsRequest::CaptionLanguage(request)
        }
        "system_languages"=>{
            exact(value,&["kind","key","order"])?;
            let snapshot=observed.system_language.as_ref()?;
            let key=SystemLanguageKey::decode(value.get("key")?)?;
            if snapshot.availability!=SystemLanguageAvailability::Available||!snapshot.can_apply||snapshot.key.as_ref()!=Some(&key){return None}
            let order=value.get("order")?.as_arr()?.iter().map(SystemLanguageKey::decode).collect::<Option<Vec<_>>>()?;
            // A reviewed list can contain leaves from earlier catalog pages.
            // The broker validates every opaque target against this exact lease.
            let request=SystemLanguageRequest::Apply{key,order};
            if !request.valid(){return None} SettingsRequest::SystemLanguages(request)
        }
        "keyboards"=>{
            exact(value,&["kind","key","target","operation"])?;
            let operation=[KeyboardOperation::Enable,KeyboardOperation::Disable,KeyboardOperation::Settings,KeyboardOperation::Subtypes,KeyboardOperation::ChooseDefault].into_iter().find(|op|Some(op.wire())==text(value,"operation"))?;
            let request=KeyboardRequest::Flow{key:KeyboardKey::decode(value.get("key")?)?,target:optional(value,"target",KeyboardKey::decode)?,operation};
            if !request.valid()||!observed.keyboards.as_ref()?.permits(&request){return None} SettingsRequest::Keyboard(request)
        }
        _=>return None,
    })
}
