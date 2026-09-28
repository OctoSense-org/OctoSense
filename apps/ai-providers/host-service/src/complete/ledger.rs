//! The `model` service's per-app budget: a rate limit and a daily call and
//! token budget, kept by the host.
//!
//! The day is the UTC day: counts reset at 00:00 UTC. Counts persist in
//! `<host dir>/model/ledger.json` (`{"day", "apps": {id: {calls, tokens}},
//! "limits": {id: {per_minute, calls_per_day, tokens_per_day}}}`), outside
//! every app's jail, so closing and reopening an app, or restarting the
//! shell, does not refill its budget. `limits` holds per-app overrides: the
//! place a Settings page will write them (a follow-up); the service only
//! reads them. The per-minute window is kept in memory.
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;

const DAY_MS: u64 = 24 * 3600 * 1000;
const MINUTE_MS: u64 = 60_000;

/// One app's limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Calls in any 60 s.
    pub per_minute: u32,
    pub calls_per_day: u32,
    /// Input plus output tokens (as the provider reports them) per day.
    pub tokens_per_day: u64,
}

impl Default for Limits {
    /// Enough for a handful of summaries, classifications or extractions an
    /// hour; not enough for an app to run up a real bill. At DeepSeek V4
    /// Flash prices 100k tokens are about US$0.03; at the priciest strong
    /// model in the catalog, a few dollars.
    fn default() -> Self {
        Limits { per_minute: 6, calls_per_day: 100, tokens_per_day: 100_000 }
    }
}

impl Limits {
    fn to_json(self) -> Value {
        json!({"per_minute": self.per_minute, "calls_per_day": self.calls_per_day, "tokens_per_day": self.tokens_per_day})
    }
    fn from_json(v: &Value, base: Limits) -> Limits {
        let n = |k: &str| v.get(k).and_then(Value::as_u64);
        Limits {
            per_minute: n("per_minute").map(|x| x.min(u32::MAX as u64) as u32).unwrap_or(base.per_minute),
            calls_per_day: n("calls_per_day").map(|x| x.min(u32::MAX as u64) as u32).unwrap_or(base.calls_per_day),
            tokens_per_day: n("tokens_per_day").unwrap_or(base.tokens_per_day),
        }
    }
}

/// Why the ledger refused a call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Too many calls in the last minute; retry after this many seconds.
    Rate { retry_after_s: u64 },
    /// Today's calls are used up.
    Calls,
    /// Today's tokens are used up, or this call would pass them.
    Tokens,
}

/// What an app (and later Settings) is shown of its budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budget {
    pub calls_today: u32,
    pub calls_per_day: u32,
    pub tokens_today: u64,
    pub tokens_per_day: u64,
    pub per_minute: u32,
    /// When today's counts reset: seconds since the Unix epoch.
    pub resets_at: u64,
}

impl Budget {
    pub fn to_json(&self) -> Value {
        json!({
            "calls_today": self.calls_today, "calls_per_day": self.calls_per_day,
            "tokens_today": self.tokens_today, "tokens_per_day": self.tokens_per_day,
            "tokens_left": self.tokens_per_day.saturating_sub(self.tokens_today),
            "per_minute": self.per_minute, "resets_at": self.resets_at,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Day {
    calls: u32,
    tokens: u64,
}

/// The ledger for every app.
#[derive(Debug, Default)]
pub struct Ledger {
    path: Option<PathBuf>,
    defaults: Limits,
    overrides: BTreeMap<String, Limits>,
    day: u64,
    apps: BTreeMap<String, Day>,
    recent: BTreeMap<String, VecDeque<u64>>,
}

impl Ledger {
    /// An in-memory ledger with these defaults.
    pub fn new(defaults: Limits) -> Ledger {
        Ledger { defaults, ..Ledger::default() }
    }

    /// Keep counts in `path` from now on, reading what it already holds
    /// (once: a later call with the same path changes nothing).
    pub fn attach(&mut self, path: PathBuf) {
        if self.path.as_ref() == Some(&path) {
            return;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                let day = v["day"].as_u64().unwrap_or(0);
                if day >= self.day {
                    self.day = day;
                    if let Some(apps) = v["apps"].as_object() {
                        for (id, d) in apps {
                            let slot = self.apps.entry(id.clone()).or_default();
                            slot.calls = slot.calls.max(d["calls"].as_u64().unwrap_or(0).min(u32::MAX as u64) as u32);
                            slot.tokens = slot.tokens.max(d["tokens"].as_u64().unwrap_or(0));
                        }
                    }
                }
                if let Some(limits) = v["limits"].as_object() {
                    for (id, l) in limits {
                        self.overrides.insert(id.clone(), Limits::from_json(l, self.defaults));
                    }
                }
            }
        }
        self.path = Some(path);
    }

    /// Set one app's limits (tests; Settings later).
    pub fn set_limits(&mut self, app: &str, limits: Limits) {
        self.overrides.insert(app.to_string(), limits);
        self.save();
    }

    pub fn limits(&self, app: &str) -> Limits {
        self.overrides.get(app).copied().unwrap_or(self.defaults)
    }

    fn roll(&mut self, now_ms: u64) {
        let day = now_ms / DAY_MS;
        if day != self.day {
            self.day = day;
            self.apps.clear();
        }
    }

    /// Admit one call by `app` that is expected to use about `estimate`
    /// tokens, and count it; or say why not.
    pub fn admit(&mut self, app: &str, now_ms: u64, estimate: u64) -> Result<(), Refused> {
        self.roll(now_ms);
        let limits = self.limits(app);
        let recent = self.recent.entry(app.to_string()).or_default();
        while recent.front().is_some_and(|t| now_ms.saturating_sub(*t) >= MINUTE_MS) {
            recent.pop_front();
        }
        if recent.len() as u32 >= limits.per_minute {
            let oldest = recent.front().copied().unwrap_or(now_ms);
            return Err(Refused::Rate { retry_after_s: (MINUTE_MS - now_ms.saturating_sub(oldest)).div_ceil(1000).max(1) });
        }
        let today = self.apps.get(app).copied().unwrap_or_default();
        if today.calls >= limits.calls_per_day {
            return Err(Refused::Calls);
        }
        if today.tokens.saturating_add(estimate) > limits.tokens_per_day {
            return Err(Refused::Tokens);
        }
        recent.push_back(now_ms);
        self.apps.entry(app.to_string()).or_default().calls += 1;
        self.save();
        Ok(())
    }

    /// Charge `tokens` to `app`'s day.
    pub fn charge(&mut self, app: &str, now_ms: u64, tokens: u64) {
        self.roll(now_ms);
        let slot = self.apps.entry(app.to_string()).or_default();
        slot.tokens = slot.tokens.saturating_add(tokens);
        self.save();
    }

    pub fn budget(&mut self, app: &str, now_ms: u64) -> Budget {
        self.roll(now_ms);
        let limits = self.limits(app);
        let today = self.apps.get(app).copied().unwrap_or_default();
        Budget {
            calls_today: today.calls,
            calls_per_day: limits.calls_per_day,
            tokens_today: today.tokens,
            tokens_per_day: limits.tokens_per_day,
            per_minute: limits.per_minute,
            resets_at: (self.day + 1) * DAY_MS / 1000,
        }
    }

    /// Every app with counts today, for a Settings page.
    pub fn all(&mut self, now_ms: u64) -> Vec<(String, Budget)> {
        self.roll(now_ms);
        let ids: Vec<String> = self.apps.keys().chain(self.overrides.keys()).cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        ids.into_iter().map(|id| (id.clone(), self.budget(&id, now_ms))).collect()
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        let apps: serde_json::Map<String, Value> =
            self.apps.iter().map(|(id, d)| (id.clone(), json!({"calls": d.calls, "tokens": d.tokens}))).collect();
        let limits: serde_json::Map<String, Value> = self.overrides.iter().map(|(id, l)| (id.clone(), l.to_json())).collect();
        let text = json!({"day": self.day, "apps": apps, "limits": limits}).to_string();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_790_000_000_000;

    #[test]
    fn rate_then_day_then_tokens() {
        let mut l = Ledger::new(Limits { per_minute: 2, calls_per_day: 3, tokens_per_day: 1000 });
        assert!(l.admit("a", T0, 10).is_ok());
        assert!(l.admit("a", T0 + 1, 10).is_ok());
        assert!(matches!(l.admit("a", T0 + 2, 10), Err(Refused::Rate { retry_after_s: 60 })));
        // Another app has its own budget.
        assert!(l.admit("b", T0 + 2, 10).is_ok());
        assert!(l.admit("a", T0 + MINUTE_MS, 10).is_ok());
        assert_eq!(l.admit("a", T0 + 3 * MINUTE_MS, 10), Err(Refused::Calls));
        l.charge("b", T0, 995);
        assert_eq!(l.admit("b", T0 + 3 * MINUTE_MS, 10), Err(Refused::Tokens));
        // A new UTC day starts again.
        assert!(l.admit("a", T0 + DAY_MS, 10).is_ok());
        assert_eq!(l.budget("a", T0 + DAY_MS).calls_today, 1);
    }

    #[test]
    fn counts_and_overrides_survive_a_restart() {
        let path = std::env::temp_dir().join(format!("model-ledger-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut l = Ledger::new(Limits::default());
        l.attach(path.clone());
        l.admit("a", T0, 0).unwrap();
        l.charge("a", T0, 1234);
        l.set_limits("a", Limits { per_minute: 1, calls_per_day: 5, tokens_per_day: 9999 });
        let mut again = Ledger::new(Limits::default());
        again.attach(path.clone());
        let b = again.budget("a", T0);
        assert_eq!((b.calls_today, b.tokens_today, b.tokens_per_day, b.per_minute), (1, 1234, 9999, 1));
        assert_eq!(b.resets_at % 86_400, 0);
        let _ = std::fs::remove_file(&path);
    }
}
