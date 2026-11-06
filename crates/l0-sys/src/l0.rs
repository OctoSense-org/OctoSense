//! Pure helpers used by generated L0/L1 code. Keep the native adapters in
//! aichat/widgets and splash-render in sync; the review harness checks both.
use makepad_widgets::*;

pub fn install(vm: &mut ScriptVm, sys: ScriptObject) {
    vm.add_method(sys, id_lut!(l0_ratio), script_args_def!(value = NIL), |vm, args| {
        let value = script_value!(vm, args.value);
        let mut text = String::new();
        text = vm.cast_to_string(value);
        let number = value.as_number().or_else(|| text.trim().parse::<f64>().ok())
            .filter(|n| n.is_finite());
        let formatted = match number {
            Some(number) => format!("{number:.1}"),
            None if text == "n/a" => text,
            None => "—".into(),
        };
        vm.new_string(&formatted)
    });
    vm.add_method(
        sys,
        id_lut!(convert),
        script_args_def!(amount = NIL, from = NIL, to = NIL, direction = NIL, field = NIL),
        |vm, args| {
            let amount = script_value!(vm, args.amount).as_number();
            let from_value = script_value!(vm, args.from);
            let to_value = script_value!(vm, args.to);
            let direction_value = script_value!(vm, args.direction);
            let field_value = script_value!(vm, args.field);
            let mut field = String::new();
            if field_value.is_nil() { field.push_str("value"); }
            else { field = vm.cast_to_string(field_value); }
            let mut from = String::new();
            let mut to = String::new();
            let mut direction = String::new();
            from = vm.cast_to_string(from_value);
            to = vm.cast_to_string(to_value);
            direction = vm.cast_to_string(direction_value);
            let (from, to) = (from.trim().to_ascii_lowercase(), to.trim().to_ascii_lowercase());
            let pair = match direction.as_str() {
                "fwd" => Some((from.as_str(), to.as_str())),
                "rev" => Some((to.as_str(), from.as_str())),
                _ => None,
            };
            let result = amount.zip(pair)
                .and_then(|(amount, (from, to))| octoscript_node::units::convert(amount, from, to));
            match result.zip(amount) {
                Some((value, _)) if field == "value" => ScriptValue::from_f64(value),
                Some((_, amount)) if field == "amount" => ScriptValue::from_f64(amount),
                _ => vm.new_string("n/a"),
            }
        },
    );
    vm.add_method(sys, id_lut!(num), script_args_def!(v = NIL), |vm, args| {
        let v = script_value!(vm, args.v);
        if let Some(n) = v.as_number() {
            return ScriptValue::from_f64(n);
        }
        let mut s = String::new();
        s = vm.cast_to_string(v);
        ScriptValue::from_f64(s.trim().parse::<f64>().unwrap_or(f64::NAN))
    });
    vm.add_method(
        sys,
        id_lut!(json_string),
        script_args_def!(v = NIL),
        |vm, args| {
            let value = script_value!(vm, args.v);
            let mut text = String::new();
            text = vm.cast_to_string(value);
            // Serializing a Rust string cannot fail.
            let encoded = serde_json::to_string(&text).expect("JSON string");
            vm.new_string(&encoded)
        },
    );
    vm.add_method(
        sys,
        id_lut!(l0_math),
        script_args_def!(op = NIL, a = NIL, b = NIL),
        |vm, args| {
            let op = script_value!(vm, args.op);
            let mut operator = String::new();
            operator = vm.cast_to_string(op);
            let a = script_value!(vm, args.a)
                .as_number()
                .filter(|v| v.is_finite());
            let b = script_value!(vm, args.b)
                .as_number()
                .filter(|v| v.is_finite());
            let result = a.zip(b).and_then(|(a, b)| {
                let value = match operator.as_str() {
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    "/" if b != 0.0 => a / b,
                    "%" if b != 0.0 => a % b,
                    _ => return None,
                };
                value.is_finite().then_some(value)
            });
            match result {
                Some(n) => ScriptValue::from_f64(n),
                None => vm.new_string("—"),
            }
        },
    );
}
