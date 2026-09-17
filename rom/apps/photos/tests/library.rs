use octosense_photos::model::{memories, search, Photo, Store};

fn catalog() -> Vec<Photo> {
    serde_json::from_str(r#"[
        {"id":"a","file":"a.jpg","title":"A day together","date":"2026-09-15","location":"Home","people":["Alex"],"tags":["family"],"moment":"Family"},
        {"id":"b","file":"b.jpg","title":"Sunny afternoon","date":"2026-09-15","location":"Home","people":["Lily"],"tags":["family"],"moment":"Family"},
        {"id":"c","file":"c.jpg","title":"Coastal walk","date":"2026-08-08","location":"Big Sur","people":[],"tags":["ocean"],"moment":"Coast"}
    ]"#).unwrap()
}

#[test]
fn album_create_rename_membership_and_delete_leave_photos_intact() {
    let photos = catalog();
    let mut store = Store::default();
    let id = store
        .save_album(
            None,
            "  Our favorites  ",
            vec!["b".into(), "a".into(), "b".into(), "missing".into()],
            &photos,
        )
        .unwrap();
    assert_eq!(store.albums[0].title, "Our favorites");
    assert_eq!(store.albums[0].photos, ["b", "a"]);
    store
        .save_album(Some(id), "Family", vec!["a".into()], &photos)
        .unwrap();
    assert_eq!(store.albums.len(), 1);
    assert_eq!(store.albums[0].title, "Family");
    assert!(store.delete_album(id));
    assert!(store.albums.is_empty());
    assert_eq!(photos.len(), 3);
}

#[test]
fn blank_name_and_unknown_album_do_not_modify_store() {
    let mut store = Store::default();
    let before = store.clone();
    assert!(store.save_album(None, " \n ", vec![], &catalog()).is_err());
    assert!(store
        .save_album(Some(55), "Lost", vec![], &catalog())
        .is_err());
    assert_eq!(store, before);
}

#[test]
fn favorite_and_album_state_survive_serialization() {
    let photos = catalog();
    let mut store = Store::default();
    store
        .save_album(None, "Empty album", vec![], &photos)
        .unwrap();
    store.toggle_favorite("a");
    let saved = serde_json::to_string(&store).unwrap();
    let mut restored = Store::decode(&saved, &photos).unwrap();
    assert_eq!(restored, store);
    restored.toggle_favorite("a");
    assert!(restored.favorites.is_empty());
}

#[test]
fn catalog_removal_drops_stale_memberships_but_preserves_albums() {
    let source = r#"{"version":1,"next_id":1,"albums":[{"id":7,"title":"Trip","photos":["a","removed"]}],"favorites":["b","removed"]}"#;
    let mut restored = Store::decode(source, &catalog()).unwrap();
    assert_eq!(restored.albums[0].photos, ["a"]);
    assert_eq!(
        restored
            .favorites
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["b"]
    );
    let id = restored
        .save_album(None, "Next", vec![], &catalog())
        .unwrap();
    assert!(id > 7);
}

#[test]
fn corrupt_or_newer_state_is_reported_instead_of_reset() {
    assert!(Store::decode("invalid", &catalog()).is_err());
    assert!(Store::decode(
        r#"{"version":99,"next_id":1,"albums":[],"favorites":[]}"#,
        &catalog()
    )
    .is_err());
}

#[test]
fn memories_are_stable_grouped_and_exclude_single_photo_groups() {
    let photos = catalog();
    let result = memories(&photos);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].title, "Family");
    assert_eq!(result[0].photos, ["a", "b"]);
    assert_eq!(result, memories(&photos));
}

#[test]
fn search_finds_people_places_dates_and_tags_case_insensitively() {
    let photos = catalog();
    assert_eq!(search(&photos, " alex "), ["a"]);
    assert_eq!(search(&photos, "BIG SUR"), ["c"]);
    assert_eq!(search(&photos, "family"), ["a", "b"]);
    assert_eq!(search(&photos, "2026-09"), ["a", "b"]);
    assert!(search(&photos, "snow").is_empty());
    assert_eq!(search(&photos, "").len(), 3);
}

#[test]
fn file_save_replaces_previous_state_and_reopens_with_latest_edits() {
    let folder = std::env::temp_dir().join(format!("octosense-photos-test-{}", std::process::id()));
    let path = folder.join("library.json");
    let photos = catalog();
    let mut store = Store::default();
    let id = store
        .save_album(None, "Original", vec!["a".into()], &photos)
        .unwrap();
    store.write(&path).unwrap();
    store
        .save_album(Some(id), "Renamed", vec!["b".into()], &photos)
        .unwrap();
    store.toggle_favorite("b");
    store.write(&path).unwrap();
    assert_eq!(
        Store::decode(&std::fs::read_to_string(&path).unwrap(), &photos).unwrap(),
        store
    );
    assert!(!path.with_extension("json.tmp").exists());
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(folder).unwrap();
}
