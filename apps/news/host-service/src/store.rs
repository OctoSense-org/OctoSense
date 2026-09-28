//! What the service keeps under `<host_dir>/news`: the stories
//! (`items.json`), the seen-items ledger (`ledger.json`), each source's fetch
//! state (`sources.json`), the followed topics (`topics.json`) and imported
//! feeds (`feeds.json`). Each file is written whole and renamed into place.
use crate::item::{item_id, near_duplicate, title_key, Draft, Item};
use crate::sources::{Source, Topic};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// How much is kept.
#[derive(Clone, Debug)]
pub struct Retention {
    /// Stories kept, newest first.
    pub max_items: usize,
    /// Stories older than this (by when they were fetched) are dropped.
    pub max_age_secs: i64,
    /// Ledger entries: a story is remembered as seen this long after it was
    /// dropped, so it does not come back as new.
    pub ledger_secs: i64,
    pub ledger_max: usize,
    /// How far back a new title is compared with stored ones.
    pub near_duplicate_secs: i64,
}

impl Default for Retention {
    fn default() -> Self {
        Retention { max_items: 3000, max_age_secs: 14 * 86_400, ledger_secs: 60 * 86_400, ledger_max: 50_000, near_duplicate_secs: 3 * 86_400 }
    }
}

/// Seen stories: by id (canonical URL) and by title key, with when first
/// seen, and the id a title key belongs to.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub urls: HashMap<String, i64>,
    #[serde(default)]
    pub titles: HashMap<String, (String, i64)>,
}

/// One source's fetch state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub last_attempt: i64,
    #[serde(default)]
    pub last_success: i64,
    #[serde(default)]
    pub failures: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Not before this (Unix seconds): the interval, or a back-off.
    #[serde(default)]
    pub next_due: i64,
    /// The ids of its latest successful fetch, in the source's order.
    #[serde(default)]
    pub current: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Data {
    pub items: Vec<Item>,
    pub ledger: Ledger,
    pub state: HashMap<String, SourceState>,
    pub topics: Vec<Topic>,
    /// Feeds imported from OPML, beyond the built-in ones.
    pub feeds: Vec<Source>,
    /// When the last run finished.
    pub updated: i64,
}

#[derive(Serialize, Deserialize)]
struct StateFile {
    #[serde(default)]
    updated: i64,
    #[serde(default)]
    sources: HashMap<String, SourceState>,
}

pub struct Store {
    pub dir: PathBuf,
}

fn read<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot store news: {e}"))?;
    }
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, bytes).map_err(|e| format!("Cannot store news: {e}"))?;
    std::fs::rename(&temp, path).map_err(|e| format!("Cannot store news: {e}"))
}

fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    write_atomic(path, &serde_json::to_vec(value).map_err(|e| e.to_string())?)
}

impl Store {
    /// The service's folder under a host directory.
    pub fn at(host_dir: &Path) -> Store {
        Store { dir: host_dir.join("news") }
    }

    pub fn load(&self) -> Data {
        let state: Option<StateFile> = std::fs::read(self.dir.join("sources.json")).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let (updated, state) = state.map(|s| (s.updated, s.sources)).unwrap_or_default();
        Data {
            items: read(&self.dir.join("items.json")),
            ledger: read(&self.dir.join("ledger.json")),
            state,
            topics: read(&self.dir.join("topics.json")),
            feeds: read(&self.dir.join("feeds.json")),
            updated,
        }
    }

    pub fn save(&self, data: &Data) -> Result<(), String> {
        write(&self.dir.join("items.json"), &data.items)?;
        write(&self.dir.join("ledger.json"), &data.ledger)?;
        write(&self.dir.join("sources.json"), &StateFile { updated: data.updated, sources: data.state.clone() })
    }

    pub fn save_topics(&self, topics: &[Topic]) -> Result<(), String> {
        write(&self.dir.join("topics.json"), &topics)
    }

    pub fn save_feeds(&self, feeds: &[Source]) -> Result<(), String> {
        write(&self.dir.join("feeds.json"), &feeds)
    }
}

impl Data {
    fn index_of(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|i| i.id == id)
    }

    /// File one fetch of `source`: new stories are added and entered in the
    /// ledger; a story seen before (same canonical URL, or a near-identical
    /// title among recent ones) is not added again, and the stored copy
    /// takes the source's fresher counts. Returns the ids new this time; the
    /// source's `current` list becomes what it carried, in its order.
    pub fn ingest(&mut self, source: &Source, drafts: Vec<Draft>, now: i64, retention: &Retention) -> Vec<String> {
        let mut new_ids = Vec::new();
        let mut current = Vec::new();
        for draft in drafts {
            let id = item_id(&draft.url);
            let key = title_key(&draft.title);
            // The stored story this one is, if any.
            let seen = if self.index_of(&id).is_some() {
                Seen::Stored(id.clone())
            } else if self.ledger.urls.contains_key(&id) {
                Seen::Dropped
            } else if let Some((owner, _)) = self.ledger.titles.get(&key).filter(|_| !key.is_empty()) {
                Seen::Stored(owner.clone())
            } else {
                let since = now - retention.near_duplicate_secs;
                match self.items.iter().find(|i| i.fetched >= since && near_duplicate(&title_key(&i.title), &key)) {
                    Some(i) => Seen::Stored(i.id.clone()),
                    None => Seen::New,
                }
            };
            match seen {
                // Seen before and since dropped: not news again.
                Seen::Dropped => {}
                Seen::Stored(owner) => {
                    self.ledger.urls.entry(id.clone()).or_insert(now);
                    if let Some(at) = self.index_of(&owner) {
                        let item = &mut self.items[at];
                        if item.feed == source.id || owner == id {
                            // The same feed again: its counts move.
                            item.points = draft.points.or(item.points);
                            item.comments = draft.comments.or(item.comments);
                        }
                        if item.image.is_none() {
                            item.image = draft.image;
                        }
                        if item.summary.is_empty() {
                            item.summary = draft.summary;
                        }
                        if item.feed != source.id && !item.also.contains(&source.id) {
                            item.also.push(source.id.clone());
                        }
                        for topic in &source.topics {
                            if !item.topics.contains(topic) {
                                item.topics.push(topic.clone());
                            }
                        }
                        if !current.contains(&owner) {
                            current.push(owner);
                        }
                    }
                }
                Seen::New => {
                    let lang = draft.lang.clone().unwrap_or_else(|| source.lang.clone());
                    let item = Item {
                        id: id.clone(),
                        title: draft.title,
                        url: draft.url,
                        source: if draft.source.is_empty() { source.label.clone() } else { draft.source },
                        feed: source.id.clone(),
                        lang,
                        published: draft.published.unwrap_or(0),
                        fetched: now,
                        summary: draft.summary,
                        image: draft.image,
                        topics: source.topics.clone(),
                        discussion: draft.discussion,
                        points: draft.points,
                        comments: draft.comments,
                        also: Vec::new(),
                    };
                    self.ledger.urls.insert(id.clone(), now);
                    if !key.is_empty() {
                        self.ledger.titles.entry(key).or_insert((id.clone(), now));
                    }
                    self.items.push(item);
                    current.push(id.clone());
                    new_ids.push(id);
                }
            }
        }
        self.state.entry(source.id.clone()).or_default().current = current;
        new_ids
    }

    /// Drop what is past the retention: stories by age and count, ledger
    /// entries by age and count.
    pub fn prune(&mut self, now: i64, retention: &Retention) {
        self.items.retain(|i| now - i.fetched <= retention.max_age_secs);
        self.items.sort_by(|a, b| b.when().cmp(&a.when()).then_with(|| b.fetched.cmp(&a.fetched)));
        self.items.truncate(retention.max_items);
        let cutoff = now - retention.ledger_secs;
        self.ledger.urls.retain(|_, seen| *seen >= cutoff);
        self.ledger.titles.retain(|_, (_, seen)| *seen >= cutoff);
        if self.ledger.urls.len() > retention.ledger_max {
            let mut seen: Vec<i64> = self.ledger.urls.values().copied().collect();
            seen.sort_unstable_by(|a, b| b.cmp(a));
            let keep_from = seen[retention.ledger_max - 1];
            self.ledger.urls.retain(|_, s| *s >= keep_from);
            self.ledger.titles.retain(|_, (_, s)| *s >= keep_from);
        }
    }
}

/// `news.list`'s filters.
#[derive(Clone, Debug, Default)]
pub struct Query {
    /// Unix seconds: only stories published (or, undated, fetched) since.
    pub since: Option<i64>,
    /// A topic tag the story carries (case-insensitive).
    pub topic: Option<String>,
    pub lang: Option<String>,
    /// Only stories this source carried; with `current`, exactly its latest
    /// fetch, in the source's order.
    pub feed: Option<String>,
    pub current: bool,
    pub limit: usize,
    pub offset: usize,
}

pub const MAX_LIMIT: usize = 500;

impl Data {
    /// The stories matching `query`, newest first (or in the source's order
    /// for `current`), and how many matched before `offset`/`limit`.
    pub fn query(&self, query: &Query) -> (usize, Vec<&Item>) {
        let by_id: HashMap<&str, &Item> = self.items.iter().map(|i| (i.id.as_str(), i)).collect();
        let mut rows: Vec<&Item> = match (&query.feed, query.current) {
            (Some(feed), true) => self
                .state
                .get(feed)
                .map(|s| s.current.iter().filter_map(|id| by_id.get(id.as_str()).copied()).collect())
                .unwrap_or_default(),
            (Some(feed), false) => {
                let mut rows: Vec<&Item> = self.items.iter().filter(|i| &i.feed == feed || i.also.contains(feed)).collect();
                rows.sort_by_key(|i| std::cmp::Reverse(i.when()));
                rows
            }
            (None, _) => {
                let mut rows: Vec<&Item> = self.items.iter().collect();
                rows.sort_by_key(|i| std::cmp::Reverse(i.when()));
                rows
            }
        };
        if let Some(since) = query.since {
            rows.retain(|i| i.when() >= since);
        }
        if let Some(topic) = query.topic.as_deref().map(str::to_lowercase) {
            rows.retain(|i| i.topics.iter().any(|t| t.to_lowercase() == topic));
        }
        if let Some(lang) = query.lang.as_deref().map(str::to_lowercase) {
            rows.retain(|i| i.lang == lang);
        }
        let total = rows.len();
        let rows = rows.into_iter().skip(query.offset).take(query.limit.min(MAX_LIMIT)).collect();
        (total, rows)
    }

    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }
}

enum Seen {
    /// A stored story (by URL, title key or a near-identical title).
    Stored(String),
    /// In the ledger, but dropped from the store.
    Dropped,
    New,
}
