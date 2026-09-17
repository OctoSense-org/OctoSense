use makepad_widgets::*;

#[test]
fn photos_ui_registers_and_instantiates_without_script_errors() {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        octosense_photos::script_mod(vm);
        let value = script_eval!(vm, {use mod.widgets.* PhotosView{}});
        let root = WidgetRef::script_from_value(vm, value);
        assert!(!root.is_empty());
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Photos UI errors: {errors:?}");
    });
}

#[test]
fn bundled_catalog_has_unique_ids_and_real_image_assets() {
    let catalog: Vec<octosense_photos::model::Photo> =
        serde_json::from_str(include_str!("../resources/catalog.json")).unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for photo in &catalog {
        assert!(ids.insert(&photo.id), "duplicate photo {}", photo.id);
        let bytes = octosense_photos::view::photo_bytes(&photo.file).expect("embedded photo");
        assert!(bytes.starts_with(b"\x89PNG") || bytes.starts_with(b"\xff\xd8\xff"));
    }
    assert_eq!(catalog.iter().filter(|p| p.people.len() == 1).count(), 6);
    assert!(octosense_photos::model::memories(&catalog).len() >= 3);
}
