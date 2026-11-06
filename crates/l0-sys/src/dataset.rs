//! Host-bound snapshot for the composition study. Cards select an ID, never a path.
use crate::sys::*;
use makepad_widgets::makepad_platform::script::vm::ScriptVmCx;
use makepad_widgets::*;
#[derive(Default)]
struct DatasetSnapshot { loaded: bool, json: String }
pub(crate) fn install(vm: &mut ScriptVm, sys: ScriptObject) {
    vm.add_method(sys, id_lut!(dataset), script_args_def!(id = NIL, field = NIL), |vm, args| {
        let id_v = script_value!(vm, args.id);
        let field_v = script_value!(vm, args.field);
        let mut id = String::new(); let mut field = String::new();
        id = vm.cast_to_string(id_v);
        field = vm.cast_to_string(field_v);
        let snapshot = vm.host.cx_mut().global::<DatasetSnapshot>();
        if !snapshot.loaded {
            snapshot.loaded = true;
            if let Ok(path) = std::env::var("OCTOS_DATASET_FILE") {
                if std::fs::metadata(&path).is_ok_and(|m| m.len() <= 131072) {
                    snapshot.json = std::fs::read_to_string(path).unwrap_or_default();
                }
            }
        }
        let out = if !field.is_empty() && field.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && json_pluck(snapshot.json.as_bytes(), "id").as_deref() == Some(id.as_str()) {
            json_pluck(snapshot.json.as_bytes(), &format!("data.{field}")).unwrap_or_default()
        } else { String::new() };
        vm.new_string(&out)
    });
}
