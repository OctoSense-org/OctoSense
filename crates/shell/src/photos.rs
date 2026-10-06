//! Photos' business API and Glance projection. UI and agents read one bundled
//! sample catalog; albums/favorites remain in Photos' existing account folder.
//! This is not Android MediaStore access or image recognition.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once, OnceLock};

const APP: &str = "os.photos";
const CATALOG: &str = include_str!("../../../apps/photos/catalog.json");
const CARD: &str = include_str!("../../../apps/photos/resources/selection.card");
const MAX_SELECTION: usize = 12;
const FILE_LIMIT: u64 = 512 * 1024;
static FOCUS: Mutex<BTreeMap<PathBuf, Vec<String>>> = Mutex::new(BTreeMap::new());
static PUBLICATIONS: Mutex<()> = Mutex::new(());
static ASSETS: Mutex<Option<octosense_app_policy::AssetServer>> = Mutex::new(None);

fn catalog() -> &'static Vec<Value> {
    static DATA: OnceLock<Vec<Value>> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str::<Value>(CATALOG).expect("Photos catalog JSON")["photos"]
            .as_array()
            .expect("Photos catalog records")
            .clone()
    })
}
fn photo(id: &str) -> Result<&'static Value, String> {
    catalog()
        .iter()
        .find(|p| p["id"].as_str() == Some(id))
        .ok_or_else(|| "That photo is not in the sample library.".into())
}
fn text<'a>(args: &'a Value, key: &str) -> &'a str {
    args[key].as_str().unwrap_or("")
}
fn account_dir(host: &Path) -> Result<PathBuf, String> {
    Ok(host
        .parent()
        .ok_or("Photos has no app data root")?
        .join(APP)
        .join("accounts/device"))
}
fn read_json(path: &Path) -> Result<Option<Value>, String> {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read Photos data: {e}")),
    };
    if metadata.len() > FILE_LIMIT {
        return Err("Photos data exceeds its read limit.".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "Photos data is damaged; it was not overwritten.".into())
}
fn valid_ids(value: &Value, max: usize) -> Result<Vec<String>, String> {
    let values = value.as_array().ok_or("Provide photo IDs as an array.")?;
    if values.is_empty() || values.len() > max {
        return Err(format!("Choose between 1 and {max} photos."));
    }
    let mut ids = Vec::new();
    for value in values {
        let id = value.as_str().ok_or("Photo IDs must be strings.")?;
        photo(id)?;
        if ids.iter().any(|v| v == id) {
            return Err("Choose each photo once.".into());
        }
        ids.push(id.to_string());
    }
    Ok(ids)
}
fn library(host: &Path) -> Result<Value, String> {
    let account = account_dir(host)?;
    // Match Splash's existing migration: an old file remains authoritative
    // until its verified account-folder copy exists.
    let mut path = account.join("library.json");
    if !path.exists() {
        path = account
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("library.json");
    }
    if let Some(value) = read_json(&path)? {
        if value["version"] != 1 || !value["albums"].is_array() || !value["favorites"].is_array() {
            return Err("Photos library has an unsupported format.".into());
        }
        return Ok(value);
    }
    let family: Vec<_> = catalog()
        .iter()
        .filter(|p| !p["people"].as_array().unwrap().is_empty())
        .map(|p| p["id"].clone())
        .collect();
    let travel: Vec<_> = catalog()
        .iter()
        .filter(|p| p["tags"].as_array().unwrap().iter().any(|t| t == "travel"))
        .map(|p| p["id"].clone())
        .collect();
    Ok(
        json!({"version":1,"next_id":3,"albums":[{"id":1,"title":"Family","photos":family},{"id":2,"title":"Weekends away","photos":travel}],"favorites":["family","alpine","beach"]}),
    )
}
fn decorated(record: &Value, state: &Value) -> Value {
    let mut out = record.clone();
    out["sample"] = json!(true);
    out["favorite"] = json!(state["favorites"]
        .as_array()
        .is_some_and(|ids| ids.contains(&record["id"])));
    out
}

/// Read-only, bounded metadata query. These are sample photos, never a scan of
/// the person's Android gallery. Favorite flags come from the app's saved data.
pub fn list(host: &Path, args: &Value) -> Result<Value, String> {
    let query = text(args, "query").trim().to_lowercase();
    if query.chars().count() > 160 {
        return Err("Keep the search under 160 characters.".into());
    }
    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let limit = args["limit"].as_u64().unwrap_or(30).clamp(1, 100) as usize;
    let state = library(host)?;
    let matches: Vec<_> = catalog()
        .iter()
        .filter(|p| {
            let haystack = ["title", "date", "location", "moment", "people", "tags"]
                .iter()
                .map(|key| p[key].to_string())
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            haystack.contains(&query)
                && (!args["favorites"].as_bool().unwrap_or(false)
                    || state["favorites"]
                        .as_array()
                        .is_some_and(|ids| ids.contains(&p["id"])))
        })
        .collect();
    let items: Vec<_> = matches
        .iter()
        .skip(offset)
        .take(limit)
        .map(|p| decorated(p, &state))
        .collect();
    Ok(
        json!({"source":"sample","sample_library":true,"items":items,"total":matches.len(),"offset":offset,"limit":limit}),
    )
}
pub fn read(host: &Path, args: &Value) -> Result<Value, String> {
    Ok(json!({"source":"sample","photo":decorated(photo(text(args,"id"))?, &library(host)?)}))
}
pub fn collections(host: &Path) -> Result<Value, String> {
    let state = library(host)?;
    let albums: Vec<Value> = state["albums"]
        .as_array()
        .unwrap()
        .iter()
        .map(|album| {
            let ids: Vec<Value> = album["photos"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|id| id.as_str().is_some_and(|id| photo(id).is_ok()))
                .cloned()
                .collect();
            json!({"id":album["id"],"title":album["title"],"photos":ids})
        })
        .collect();
    let favorites: Vec<_> = catalog()
        .iter()
        .filter(|p| state["favorites"].as_array().unwrap().contains(&p["id"]))
        .map(|p| p["id"].clone())
        .collect();
    Ok(json!({"source":"sample","sample_library":true,"albums":albums,"favorites":favorites}))
}

/// Exact server created by this module, not an app/model-supplied endpoint.
/// Glance's Photos-only isolate may append this single loopback host.
pub fn asset_host() -> Option<String> {
    ASSETS.lock().unwrap().as_ref().map(|s| s.allowlist_entry())
}
fn asset_origin(host: &Path) -> Result<String, String> {
    let mut assets = ASSETS.lock().unwrap();
    if let Some(server) = assets.as_ref() {
        return Ok(server.origin().trim_end_matches('/').to_string());
    }
    let app = octosense_appstore::system::system_app(APP)
        .ok_or("Photos is not installed in this shell.")?;
    let root = host.parent().ok_or("Photos has no app data root")?;
    let (bundle, _) = octosense_appstore::system::prepare(root, &app)?;
    let server = octosense_app_policy::AssetServer::start(&bundle).map_err(|e| e.to_string())?;
    let origin = server.origin().trim_end_matches('/').to_string();
    *assets = Some(server);
    Ok(origin)
}

fn selection_key(ids: &[String]) -> String {
    format!(
        "selection-{:x}",
        Sha256::digest(serde_json::to_vec(ids).unwrap())
    )[..42]
        .to_string()
}
fn card_args(ids: &[String], origin: &str, notify: bool) -> Result<Value, String> {
    let records: Vec<_> = ids.iter().map(|id| photo(id)).collect::<Result<_, _>>()?;
    let first = records[0];
    let id = selection_key(ids);
    let title = if records.len() == 1 {
        text(first, "title").to_string()
    } else {
        format!("{} selected photos", records.len())
    };
    let count = format!(
        "{} photo{}",
        records.len(),
        if records.len() == 1 { "" } else { "s" }
    );
    let mut data = json!({"title":title,"subtitle":"Sample library · metadata only","summary":"","coverage":format!("{count} selected"),"url1":format!("app://photos/selection/{id}")});
    let mut remainder = Vec::new();
    for (index, p) in records.iter().enumerate() {
        let caption = format!("{} · {}", text(p, "date"), text(p, "location"));
        if index < 3 {
            data[format!("pick{}_title", index + 1)] = p["title"].clone();
            data[format!("pick{}_body", index + 1)] = json!(caption);
            data[format!("pick{}_source", index + 1)] =
                json!(format!("{origin}/thumbs/{}.jpg", text(p, "id")));
        } else {
            remainder.push(format!("{} — {caption}", text(p, "title")));
        }
    }
    for index in records.len().min(3)..3 {
        for suffix in ["title", "body", "source"] {
            data[format!("pick{}_{suffix}", index + 1)] = json!("");
        }
    }
    data["summary"] = json!(remainder.join("\n"));
    Ok(
        json!({"card_id":id,"title":title,"summary":format!("Sample library · {count} · {}",text(first,"location")),"source":CARD,"data":{"selection":data,"photo_context":{"source":"sample","ids":ids}},"open":{"app":"photos","route":format!("selection/{id}")},"priority":30,"notify":notify}),
    )
}
fn cards_path(host: &Path) -> PathBuf {
    host.join("photos/cards.json")
}
fn cards(host: &Path) -> Result<Vec<Value>, String> {
    let Some(value) = read_json(&cards_path(host))? else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .cloned()
        .ok_or_else(|| "Photos cards have an unsupported format.".into())
}
fn write_cards(host: &Path, cards: &[Value]) -> Result<(), String> {
    let path = cards_path(host);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let pending = path.with_extension("tmp");
    std::fs::write(&pending, serde_json::to_vec(cards).unwrap()).map_err(|e| e.to_string())?;
    std::fs::rename(pending, path).map_err(|e| e.to_string())
}
pub fn publish_card(host: &Path, args: &Value) -> Result<Value, String> {
    let ids = valid_ids(&args["ids"], MAX_SELECTION)?;
    let now = crate::glance::now_ms();
    let origin = asset_origin(host)?;
    let publish = card_args(&ids, &origin, args["notify"].as_bool().unwrap_or(false))?;
    let receipt = crate::glance::publish_for(APP, &publish)?;
    let _guard = PUBLICATIONS.lock().unwrap();
    let mut saved = cards(host)?;
    saved.retain(|p| {
        p["expires_at"].as_u64().unwrap_or(0) > now && p["card_id"] != publish["card_id"]
    });
    saved.push(json!({"card_id":publish["card_id"],"ids":ids,"dismissed":false,"expires_at":receipt["expires_at"]}));
    if saved.len() > 32 {
        saved.drain(..saved.len() - 32);
    }
    write_cards(host, &saved)?;
    Ok(receipt)
}
fn remaining_lifetime(card: &Value, now_ms: u64) -> Option<u64> {
    let seconds = card["expires_at"].as_u64()?.checked_sub(now_ms)? / 1_000;
    // Never extend a near-expired card to satisfy Glance's minimum lifetime.
    (seconds >= 60).then_some(seconds)
}

/// Quietly reproject live cards with this process's asset URLs. Call after
/// Glance and Photos grants are ready; do not turn restoration into a notice.
pub fn restore(host: &Path) -> Result<usize, String> {
    let saved = {
        let _guard = PUBLICATIONS.lock().unwrap();
        cards(host)?
    };
    let now = crate::glance::now_ms();
    let mut restored = 0;
    for card in saved {
        if card["dismissed"] == true {
            continue;
        }
        let Some(lifetime) = remaining_lifetime(&card, now) else {
            continue;
        };
        if crate::glance::card(&format!("{APP}/{}", text(&card, "card_id"))).is_some() {
            continue;
        }
        let ids = valid_ids(&card["ids"], MAX_SELECTION)?;
        let mut args = card_args(&ids, &asset_origin(host)?, false)?;
        args["expires"] = json!(lifetime);
        crate::glance::publish_for(APP, &args)?;
        restored += 1;
    }
    Ok(restored)
}
pub fn set_dismissed(host: &Path, id: &str, dismissed: bool) -> Result<(), String> {
    let _guard = PUBLICATIONS.lock().unwrap();
    let mut saved = cards(host)?;
    if let Some(card) = saved.iter_mut().find(|p| p["card_id"].as_str() == Some(id)) {
        card["dismissed"] = json!(dismissed);
        write_cards(host, &saved)?;
    }
    Ok(())
}
/// Trusted own-app navigation, consumed once by Photos' `view` poll. Routes
/// cannot select file paths or photos outside the authoritative catalog.
pub fn focus_route(host: &Path, route: &str) -> Result<(), String> {
    let ids = if let Some(id) = route.strip_prefix("photo/") {
        photo(id)?;
        vec![id.to_string()]
    } else if let Some(id) = route.strip_prefix("selection/") {
        let _guard = PUBLICATIONS.lock().unwrap();
        let saved = cards(host)?;
        let record = saved
            .iter()
            .find(|p| p["card_id"].as_str() == Some(id))
            .ok_or("This photo selection is no longer saved.")?;
        valid_ids(&record["ids"], MAX_SELECTION)?
    } else {
        return Err("Photos cannot open that route.".into());
    };
    FOCUS.lock().unwrap().insert(host.to_path_buf(), ids);
    Ok(())
}
fn view(host: &Path, args: &Value) -> Value {
    let focus = if args["take_focus"].as_bool().unwrap_or(true) {
        FOCUS.lock().unwrap().remove(host)
    } else {
        None
    };
    json!({"focus_ids":focus.unwrap_or_default()})
}

struct PhotosService;
impl octosense_appstore::services::HostService for PhotosService {
    fn family(&self) -> &'static str {
        "photos"
    }
    fn call(
        &mut self,
        call: octosense_appstore::services::ServiceCall,
        reply: octosense_appstore::services::Replier,
        _host: &mut dyn octosense_appstore::services::ServiceHost,
    ) {
        // Cross-app agent calls reach this executor as the owner only AFTER
        // the relay has checked Photos' shareability and the caller's grant.
        if call.app_id != APP {
            return reply.send(Err("photos serves os.photos only".into()));
        }
        let result = match call.method() {
            "list" => list(&call.host_dir, &call.args),
            "read" => read(&call.host_dir, &call.args),
            "collections" => collections(&call.host_dir),
            "publish_card" => publish_card(&call.host_dir, &call.args),
            "view" => Ok(view(&call.host_dir, &call.args)),
            "notify" => crate::glance_notice::notify(APP, &call.args),
            _ => Err("Photos has no such method.".into()),
        };
        reply.send(result);
    }
}
pub fn register() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| octosense_appstore::services::register_host_service(Box::new(PhotosService)));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("photos-service-{name}-{}", std::process::id()))
            .join(".host");
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
        root
    }
    #[test]
    fn sample_catalog_query_and_detail_share_favorites_with_the_app() {
        let root = root("library");
        let rows = list(&root, &json!({"query":"coffee"})).unwrap();
        assert!(rows["total"].as_u64().unwrap() > 1);
        assert_eq!(rows["sample_library"], true);
        let account = account_dir(&root).unwrap();
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join("library.json"),r#"{"version":1,"albums":[{"id":7,"title":"My set","photos":["coast","missing"]}],"favorites":["coast"]}"#).unwrap();
        let rows = list(&root, &json!({"favorites":true})).unwrap();
        assert_eq!(rows["items"].as_array().unwrap().len(), 1);
        assert_eq!(rows["items"][0]["id"], "coast");
        assert_eq!(
            read(&root, &json!({"id":"coast"})).unwrap()["photo"]["favorite"],
            true
        );
        assert_eq!(
            collections(&root).unwrap()["albums"][0]["photos"],
            json!(["coast"])
        );
        assert!(read(&root, &json!({"id":"../../private"})).is_err());
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }
    #[test]
    fn damaged_state_is_reported_without_replacing_user_albums() {
        let root = root("damaged");
        let account = account_dir(&root).unwrap();
        std::fs::create_dir_all(&account).unwrap();
        let path = account.join("library.json");
        std::fs::write(&path, "{broken").unwrap();
        assert!(list(&root, &json!({})).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{broken");
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }
    #[test]
    fn selection_card_is_valid_l0_and_binds_only_catalog_thumbnails_and_own_navigation() {
        let ids = valid_ids(&json!(["coast", "beach", "alpine", "family"]), 12).unwrap();
        let card = card_args(&ids, "http://127.0.0.1:23456", false).unwrap();
        crate::glance::check_level(CARD).expect("Photos card must pass the pinned L0 contract");
        assert!(card["data"]["selection"]["summary"]
            .as_str()
            .unwrap()
            .contains("All together"));
        assert_eq!(
            card["data"]["selection"]["pick1_source"],
            "http://127.0.0.1:23456/thumbs/coast.jpg"
        );
        assert_eq!(card["notify"], false);
        assert_eq!(
            card["data"]["selection"]["url1"],
            format!("app://photos/{}", card["open"]["route"].as_str().unwrap())
        );
        assert!(valid_ids(&json!(["coast", "coast"]), 12).is_err());
        assert!(valid_ids(&json!([]), 12).is_err());
        assert!(valid_ids(&json!(["http://127.0.0.1/private"]), 12).is_err());
    }
    #[test]
    fn restoration_converts_millisecond_receipts_to_seconds_without_extending_expiry() {
        assert_eq!(
            remaining_lifetime(&json!({"expires_at": 4_601_999}), 1_001_000),
            Some(3600)
        );
        assert_eq!(
            remaining_lifetime(&json!({"expires_at": 60_999}), 1_000),
            None
        );
        assert_eq!(
            remaining_lifetime(&json!({"expires_at": 1_000}), 2_000),
            None
        );
        assert_eq!(remaining_lifetime(&json!({}), 0), None);
    }
    #[test]
    fn navigation_uses_saved_selection_and_is_consumed_once_without_interrupting_editing() {
        let root = root("route");
        let ids = vec!["coast".into(), "beach".into()];
        let key = selection_key(&ids);
        write_cards(
            &root,
            &[json!({"card_id":key,"ids":ids,"expires_at":u64::MAX,"dismissed":false})],
        )
        .unwrap();
        focus_route(&root, &format!("selection/{key}")).unwrap();
        assert_eq!(
            view(&root, &json!({"take_focus":false}))["focus_ids"],
            json!([])
        );
        assert_eq!(
            view(&root, &json!({}))["focus_ids"],
            json!(["coast", "beach"])
        );
        assert_eq!(view(&root, &json!({}))["focus_ids"], json!([]));
        set_dismissed(&root, &key, true).unwrap();
        assert_eq!(cards(&root).unwrap()[0]["dismissed"], true);
        assert!(focus_route(&root, "photo/../../private").is_err());
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }
}
