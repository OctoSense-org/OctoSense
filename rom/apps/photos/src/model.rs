use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Photo {
    pub id: String,
    pub file: String,
    pub title: String,
    pub date: String,
    pub location: String,
    #[serde(default)]
    pub people: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub moment: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Album {
    pub id: u64,
    pub title: String,
    pub photos: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Memory {
    pub title: String,
    pub date: String,
    pub photos: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Store {
    pub version: u32,
    pub next_id: u64,
    pub albums: Vec<Album>,
    pub favorites: BTreeSet<String>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            version: 1,
            next_id: 1,
            albums: Vec::new(),
            favorites: BTreeSet::new(),
        }
    }
}

impl Store {
    pub fn save_album(
        &mut self,
        id: Option<u64>,
        title: &str,
        photos: Vec<String>,
        catalog: &[Photo],
    ) -> Result<u64, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("Give your album a name.".into());
        }
        if title.chars().count() > 80 {
            return Err("Use a name of 80 characters or fewer.".into());
        }
        let mut seen = BTreeSet::new();
        let photos = photos
            .into_iter()
            .filter(|id| catalog.iter().any(|p| p.id == *id) && seen.insert(id.clone()))
            .collect();
        if let Some(id) = id {
            let album = self
                .albums
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or("This album no longer exists.")?;
            album.title = title.into();
            album.photos = photos;
            Ok(id)
        } else {
            let id = self.next_id.max(1);
            let next = id.checked_add(1).ok_or("Could not create another album.")?;
            self.albums.push(Album {
                id,
                title: title.into(),
                photos,
            });
            self.next_id = next;
            Ok(id)
        }
    }
    pub fn delete_album(&mut self, id: u64) -> bool {
        let before = self.albums.len();
        self.albums.retain(|a| a.id != id);
        self.albums.len() != before
    }
    pub fn toggle_favorite(&mut self, id: &str) {
        if !self.favorites.remove(id) {
            self.favorites.insert(id.into());
        }
    }
    pub fn decode(source: &str, catalog: &[Photo]) -> Result<Self, String> {
        let mut store: Self =
            serde_json::from_str(source).map_err(|e| format!("Couldn't read saved albums: {e}"))?;
        if store.version != 1 {
            return Err("Saved albums use an unsupported version.".into());
        }
        let valid: BTreeSet<_> = catalog.iter().map(|p| p.id.as_str()).collect();
        let mut album_ids = BTreeSet::new();
        for album in &mut store.albums {
            if !album_ids.insert(album.id) {
                return Err("Saved albums contain duplicate identifiers.".into());
            }
            let mut seen = BTreeSet::new();
            album
                .photos
                .retain(|id| valid.contains(id.as_str()) && seen.insert(id.clone()));
        }
        store.favorites.retain(|id| valid.contains(id.as_str()));
        let next = store
            .albums
            .iter()
            .map(|a| a.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("Saved album identifiers are invalid.")?;
        store.next_id = store.next_id.max(next);
        Ok(store)
    }
    pub fn write(&self, path: &Path) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or("Album storage location is unavailable.")?;
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Couldn't create album storage: {e}"))?;
        let temp = path.with_extension("json.tmp");
        let content = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&temp, content).map_err(|e| format!("Couldn't save albums: {e}"))?;
        std::fs::rename(temp, path).map_err(|e| format!("Couldn't finish saving albums: {e}"))
    }
}

/// A stable home-card selection: favorites first, then other library photos.
pub fn preview_photos(
    catalog: &[Photo],
    favorites: &BTreeSet<String>,
    limit: usize,
) -> Vec<String> {
    catalog
        .iter()
        .filter(|photo| favorites.contains(&photo.id))
        .chain(
            catalog
                .iter()
                .filter(|photo| !favorites.contains(&photo.id)),
        )
        .take(limit)
        .map(|photo| photo.id.clone())
        .collect()
}

pub fn memories(catalog: &[Photo]) -> Vec<Memory> {
    let mut groups: BTreeMap<String, Vec<&Photo>> = BTreeMap::new();
    for photo in catalog {
        let title = if photo.moment.trim().is_empty() {
            format!(
                "{} · {}",
                photo.location,
                photo.date.get(..7).unwrap_or(&photo.date)
            )
        } else {
            photo.moment.clone()
        };
        groups.entry(title).or_default().push(photo);
    }
    let mut result: Vec<_> = groups
        .into_iter()
        .filter(|(_, photos)| photos.len() >= 2)
        .map(|(title, mut photos)| {
            photos.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
            Memory {
                title,
                date: photos.last().unwrap().date.clone(),
                photos: photos.iter().map(|p| p.id.clone()).collect(),
            }
        })
        .collect();
    result.sort_by(|a, b| b.date.cmp(&a.date).then(a.title.cmp(&b.title)));
    result
}

pub fn search(catalog: &[Photo], query: &str) -> Vec<String> {
    let query = query.trim().to_lowercase();
    catalog
        .iter()
        .filter(|p| {
            format!(
                "{} {} {} {} {} {}",
                p.title,
                p.location,
                p.date,
                p.moment,
                p.people.join(" "),
                p.tags.join(" ")
            )
            .to_lowercase()
            .contains(&query)
        })
        .map(|p| p.id.clone())
        .collect()
}
