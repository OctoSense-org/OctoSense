//! Optional host-configured news workflow. All fields share one asynchronous
//! Cx fetch, so the VM never blocks on article retrieval or inference.
use crate::sys::*;
use makepad_widgets::makepad_platform::script::vm::ScriptVmCx;
use makepad_widgets::*;

#[derive(Default)]
struct NewsCache {
    requests: std::collections::BTreeMap<String, std::time::Instant>,
}

pub(crate) fn install(vm: &mut ScriptVm, sys: ScriptObject) {
    vm.add_method(sys, id_lut!(news_digest),
        script_args_def!(query = NIL, language = NIL, field = NIL), |vm, args| {
            let query_v = script_value!(vm, args.query);
            let language_v = script_value!(vm, args.language);
            let field_v = script_value!(vm, args.field);
            let mut query = String::new();
            let mut language = String::new();
            let mut field = String::new();
            query = vm.cast_to_string(query_v);
            language = vm.cast_to_string(language_v);
            field = vm.cast_to_string(field_v);
            let path_ok = matches!(field.as_str(), "status" | "message" | "count") || {
                let parts: Vec<_> = field.split('.').collect();
                parts.len() == 3 && parts[0] == "items"
                    && parts[1].parse::<usize>().is_ok_and(|i| i < 3)
                    && matches!(parts[2], "id" | "title" | "summary" | "publisher" | "url" | "published_at")
            };
            let base = std::env::var("OCTOS_NEWS_PIPELINE_URL").unwrap_or_default();
            let out = if !path_ok || query.trim().is_empty() || query.chars().count() > 160
                || !matches!(language.as_str(), "en" | "zh-CN") || !base.starts_with("http://127.0.0.1:") {
                "n/a".to_owned()
            } else {
                let url = format!("{}/news?q={}&language={}", base.trim_end_matches('/'),
                    percent_encode_query(query.trim()), percent_encode_query(&language));
                let now = std::time::Instant::now();
                let mut expired = Vec::new();
                {
                    let cache = vm.host.cx_mut().global::<NewsCache>();
                    if cache.requests.get(&url).is_some_and(|at| at.elapsed().as_secs() >= 60) {
                        expired.push(url.clone());
                        cache.requests.remove(&url);
                    }
                    if !cache.requests.contains_key(&url) && cache.requests.len() >= 32 {
                        if let Some(oldest) = cache.requests.iter().min_by_key(|(_, at)| **at).map(|(key, _)| key.clone()) {
                            cache.requests.remove(&oldest);
                            expired.push(oldest);
                        }
                    }
                    cache.requests.entry(url.clone()).or_insert(now);
                }
                for old in expired {
                    crate::fetch::forget(&old);
                }
                let heap_key = vm.heap_key();
                match crate::fetch::fetch_cx(vm.host.cx_mut(), heap_key, &url) {
                    Some(bytes) => json_pluck(&bytes, &field).unwrap_or_default(),
                    None => crate::fetch::placeholder(&url),
                }
            };
            vm.new_string(&out)
        });
}
