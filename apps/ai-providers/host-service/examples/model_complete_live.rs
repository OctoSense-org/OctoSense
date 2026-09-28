//! A live `model.complete` against DeepSeek: one fast call, one strong call,
//! each validated against a schema, with the usage the provider reports.
//! Costs a fraction of a US cent.
//!
//! `DEEPSEEK_API_KEY=… cargo run --locked -p octosense-llm-service --example model_complete_live`
//!
//! The key is read from the environment and sent only in the request's
//! header; nothing here prints it.
use octosense_llm_config::Provider;
use octosense_llm_service::complete::{self, Candidate, Class, ModelHost, Providers, Request};
use serde_json::json;
use std::sync::Arc;

struct DeepSeek(String);
impl Providers for DeepSeek {
    fn candidates(&self) -> Result<Vec<Candidate>, String> {
        Ok(["deepseek-v4-flash", "deepseek-v4-pro"]
            .iter()
            .map(|m| Candidate { provider: Provider::new("deepseek", Some(m.to_string())), key: Some(self.0.clone()) })
            .collect())
    }
}

fn main() {
    let key = std::env::var("DEEPSEEK_API_KEY").expect("set DEEPSEEK_API_KEY");
    let host = ModelHost::new(&complete::Options::default().providers(Arc::new(DeepSeek(key))));
    let schema = json!({
        "type": "object", "additionalProperties": false, "required": ["title", "tags", "urgent"],
        "properties": {
            "title": {"type": "string", "maxLength": 40},
            "tags": {"type": "array", "maxItems": 3, "items": {"type": "string", "maxLength": 20}},
            "urgent": {"type": "boolean"}
        }
    });
    for class in [Class::Fast, Class::Strong] {
        let request = Request {
            class,
            task: "Give the note a short title, up to three lowercase tags, and say whether it is urgent.".into(),
            input: json!({"note": "The boiler is leaking into the kitchen. Call the plumber today, and buy oat milk."}),
            schema: schema.clone(),
            allow_urls: false,
            system: None,
        };
        let started = std::time::Instant::now();
        match host.complete("live.check", request) {
            Ok(done) => println!("{} in {} ms: {}", class.as_str(), started.elapsed().as_millis(), done.to_reply()),
            Err(refusal) => println!("{} refused: {refusal} ({:?})", class.as_str(), refusal.detail),
        }
    }
    let budget = host.budget("live.check");
    println!("ledger: {} calls, {} tokens today", budget.calls_today, budget.tokens_today);
}
