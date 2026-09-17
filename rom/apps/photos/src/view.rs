use crate::model::{self, Memory, Photo, Store};
use makepad_widgets::kit::*;
use makepad_widgets::*;
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::Arc,
};

include!(concat!(env!("OUT_DIR"), "/photo_assets.rs"));

const PLUS_ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 5v14M5 12h14" fill="none" stroke="#007aff" stroke-width="2" stroke-linecap="round"/></svg>"##;
const HEART_ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M20.8 4.6a5.5 5.5 0 0 0-7.8 0l-1 1-1-1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8z" fill="none" stroke="#007aff" stroke-width="1.8" stroke-linejoin="round"/></svg>"##;
const HEART_FILLED_ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M20.8 4.6a5.5 5.5 0 0 0-7.8 0l-1 1-1-1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8z" fill="#007aff"/></svg>"##;

#[derive(Clone, Default, PartialEq)]
enum Route {
    #[default]
    Collections,
    Library,
    Album(u64),
    Person(String),
    Favorites,
    Editor,
    Viewer,
}

#[derive(Clone)]
enum Row {
    Heading(String, String),
    Memory(usize),
    Albums(Vec<u64>),
    People(Vec<String>),
    Grid(Vec<String>),
    Utility,
    Empty(String),
    End(String),
}

#[derive(Script, ScriptHook, Widget)]
pub struct PhotosView {
    #[deref]
    view: View,
    #[rust]
    initialized: bool,
    #[rust]
    mode: HostedViewMode,
    #[rust]
    catalog: Vec<Photo>,
    #[rust]
    store: Store,
    #[rust]
    memories: Vec<Memory>,
    #[rust]
    state_path: Option<PathBuf>,
    #[rust]
    storage_error: Option<String>,
    #[rust]
    route: Route,
    #[rust]
    return_route: Route,
    #[rust]
    editor_return: Route,
    #[rust]
    rows: Vec<Row>,
    #[rust]
    photo_ids: Vec<String>,
    #[rust]
    viewer_ids: Vec<String>,
    #[rust]
    viewer_index: usize,
    #[rust]
    memory_index: Option<usize>,
    #[rust]
    playing: bool,
    #[rust]
    slide_timer: Timer,
    #[rust]
    query: String,
    #[rust]
    searching: bool,
    #[rust]
    editor_id: Option<u64>,
    #[rust]
    selected: BTreeSet<String>,
    #[rust]
    confirming_delete: bool,
    #[rust]
    image_keys: HashMap<WidgetUid, String>,
}

impl PhotosView {
    fn initialize(&mut self, cx: &mut Cx) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        for (id, svg) in [
            (
                id!(back),
                include_str!("../../../resources/icons/chevron-left.svg"),
            ),
            (
                id!(previous),
                include_str!("../../../resources/icons/chevron-left.svg"),
            ),
            (
                id!(next),
                include_str!("../../../resources/icons/chevron-right.svg"),
            ),
            (
                id!(search),
                include_str!("../../../resources/icons/search.svg"),
            ),
            (id!(create), PLUS_ICON),
        ] {
            if let Some(mut button) = self.view.widget(cx, &[id]).borrow_mut::<Button>() {
                button.draw_icon.load_from_str(svg);
            }
        }
        self.catalog = serde_json::from_str(include_str!("../resources/catalog.json"))
            .expect("bundled photo catalog");
        self.catalog.sort_by(|a, b| b.date.cmp(&a.date));
        self.memories = model::memories(&self.catalog);
        self.state_path = std::env::var_os("OCTOSENSE_PHOTOS_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                if cfg!(any(target_os = "android", target_os = "ios")) {
                    cx.get_data_dir().map(|p| PathBuf::from(p).join("photos"))
                } else {
                    std::env::var_os("OCTOSENSE_HOME")
                        .map(PathBuf::from)
                        .or_else(|| {
                            std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".octosense"))
                        })
                        .map(|p| p.join("photos"))
                }
            })
            .map(|p| p.join("library.json"));
        if let Some(path) = &self.state_path {
            match std::fs::read_to_string(path) {
                Ok(source) => match Store::decode(&source, &self.catalog) {
                    Ok(store) => self.store = store,
                    Err(error) => self.storage_error = Some(error),
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => self.seed_albums(),
                Err(error) => {
                    self.storage_error = Some(format!("Couldn't open saved albums: {error}"))
                }
            }
        } else {
            self.storage_error = Some("Album storage isn't available on this device.".into());
        }
        self.rebuild(cx);
        if let Some(error) = self.storage_error.clone() {
            self.message(cx, &error);
        }
    }

    fn seed_albums(&mut self) {
        let family = self
            .catalog
            .iter()
            .filter(|p| !p.people.is_empty())
            .map(|p| p.id.clone())
            .collect();
        let travel = self
            .catalog
            .iter()
            .filter(|p| p.tags.iter().any(|t| t == "travel"))
            .map(|p| p.id.clone())
            .collect();
        let _ = self.store.save_album(None, "Family", family, &self.catalog);
        let _ = self
            .store
            .save_album(None, "Weekends away", travel, &self.catalog);
        self.store
            .favorites
            .extend(["family".into(), "alpine".into(), "beach".into()]);
    }

    fn message(&self, cx: &mut Cx, text: &str) {
        let label = self.view.label(cx, ids!(message));
        label.set_text(cx, text);
        label.set_visible(cx, self.mode == HostedViewMode::Full && !text.is_empty());
    }

    fn persist(&mut self, cx: &mut Cx, previous: Store) -> bool {
        let result = if let Some(error) = &self.storage_error {
            Err(error.clone())
        } else if let Some(path) = &self.state_path {
            self.store.write(path)
        } else {
            Err("Album storage isn't available.".into())
        };
        if let Err(error) = result {
            self.store = previous;
            self.message(cx, &error);
            false
        } else {
            self.message(cx, "");
            true
        }
    }

    fn visible_ids(&self) -> Vec<String> {
        match &self.route {
            Route::Album(id) => self
                .store
                .albums
                .iter()
                .find(|a| a.id == *id)
                .map(|a| a.photos.clone())
                .unwrap_or_default(),
            Route::Person(name) => self
                .catalog
                .iter()
                .filter(|p| p.people.contains(name))
                .map(|p| p.id.clone())
                .collect(),
            Route::Favorites => self
                .catalog
                .iter()
                .filter(|p| self.store.favorites.contains(&p.id))
                .map(|p| p.id.clone())
                .collect(),
            Route::Library if self.searching => model::search(&self.catalog, &self.query),
            _ => self.catalog.iter().map(|p| p.id.clone()).collect(),
        }
    }

    fn rebuild(&mut self, cx: &mut Cx) {
        self.rows.clear();
        if self.route == Route::Collections {
            if !self.memories.is_empty() {
                self.rows
                    .push(Row::Heading("Memories".into(), "Made for you".into()));
                self.rows.push(Row::Memory(0));
            }
            self.rows.push(Row::Heading(
                "Albums".into(),
                format!("{} albums", self.store.albums.len()),
            ));
            let albums: Vec<_> = self.store.albums.iter().map(|a| a.id).collect();
            if albums.is_empty() {
                self.rows.push(Row::Empty(
                    "Your albums start here. Tap + to create one.".into(),
                ));
            }
            for chunk in albums.chunks(2) {
                self.rows.push(Row::Albums(chunk.to_vec()));
            }
            self.rows
                .push(Row::Heading("People".into(), "Your familiar faces".into()));
            let people: BTreeSet<_> = self
                .catalog
                .iter()
                .flat_map(|p| p.people.iter().cloned())
                .collect();
            for chunk in people.into_iter().collect::<Vec<_>>().chunks(3) {
                self.rows.push(Row::People(chunk.to_vec()));
            }
            self.rows
                .push(Row::Heading("Pinned collections".into(), String::new()));
            self.rows.push(Row::Utility);
            if self.memories.len() > 1 {
                self.rows
                    .push(Row::Heading("More memories".into(), String::new()));
            }
            for i in 1..self.memories.len() {
                self.rows.push(Row::Memory(i));
            }
            self.rows.push(Row::End(format!(
                "{} photos · On this device",
                self.catalog.len()
            )));
        } else if self.route != Route::Viewer {
            self.photo_ids = self.visible_ids();
            if self.photo_ids.is_empty() {
                let message = if self.searching && !self.query.is_empty() {
                    "No matching photos. Try a person, place, or date."
                } else if self.route == Route::Favorites {
                    "Your favorite photos will appear here. Tap the heart when viewing a photo."
                } else {
                    "This album is empty. Tap Edit to add photos."
                };
                self.rows.push(Row::Empty(message.into()));
            }
            if self.route == Route::Library && !self.searching {
                let mut groups: Vec<(String, Vec<String>)> = Vec::new();
                for id in &self.photo_ids {
                    let photo = self.photo(id).unwrap();
                    let month = photo.date.get(..7).unwrap_or(&photo.date).to_string();
                    if groups.last().is_none_or(|g| g.0 != month) {
                        groups.push((month, Vec::new()));
                    }
                    groups.last_mut().unwrap().1.push(id.clone());
                }
                for (month, ids) in groups {
                    self.rows.push(Row::Heading(
                        month_title(&month),
                        format!("{} photos", ids.len()),
                    ));
                    for chunk in ids.chunks(3) {
                        self.rows.push(Row::Grid(chunk.to_vec()));
                    }
                }
            } else {
                for chunk in self.photo_ids.chunks(3) {
                    self.rows.push(Row::Grid(chunk.to_vec()));
                }
            }
            self.rows
                .push(Row::End(format!("{} photos", self.photo_ids.len())));
        }
        self.view
            .portal_list(cx, ids!(list))
            .set_first_id_and_scroll(0, 0.0);
        self.sync_chrome(cx);
        self.view.redraw(cx);
    }

    fn sync_chrome(&mut self, cx: &mut Cx) {
        let compact = self.mode == HostedViewMode::Tile;
        self.view.view(cx, ids!(compact)).set_visible(cx, compact);
        self.view.view(cx, ids!(header)).set_visible(cx, !compact);
        self.view.view(cx, ids!(body)).set_visible(cx, !compact);
        let message = self.view.label(cx, ids!(message));
        message.set_visible(cx, !compact && !message.text().is_empty());
        if compact {
            for id in [
                id!(search_bar),
                id!(editor_bar),
                id!(editor_footer),
                id!(footer),
            ] {
                self.view.widget(cx, &[id]).set_visible(cx, false);
            }
            let photos = model::preview_photos(&self.catalog, &self.store.favorites, 3);
            for (index, id) in [id!(preview_first), id!(preview_second), id!(preview_third)]
                .into_iter()
                .enumerate()
            {
                let image = self.view.image(cx, &[id]);
                self.set_image(cx, image, photos.get(index).map(String::as_str));
            }
            return;
        }
        let viewer = self.route == Route::Viewer;
        let editor = self.route == Route::Editor;
        let root = matches!(self.route, Route::Library | Route::Collections);
        let title = match &self.route {
            Route::Collections => "Collections".into(),
            Route::Library => {
                if self.searching {
                    "Search".into()
                } else {
                    "Library".into()
                }
            }
            Route::Album(id) => self
                .store
                .albums
                .iter()
                .find(|a| a.id == *id)
                .map(|a| a.title.clone())
                .unwrap_or("Album".into()),
            Route::Person(name) => name.clone(),
            Route::Favorites => "Favorites".into(),
            Route::Editor => {
                if self.editor_id.is_some() {
                    "Edit album".into()
                } else {
                    "New album".into()
                }
            }
            Route::Viewer => self
                .current_photo()
                .map(|p| p.title.clone())
                .unwrap_or_default(),
        };
        let subtitle = match &self.route {
            Route::Collections => "Your favorite moments, together".into(),
            Route::Viewer => self
                .current_photo()
                .map(|p| format!("{} · {}", display_date(&p.date), p.location))
                .unwrap_or_default(),
            Route::Editor => "A little collection of your own".into(),
            _ => format!("{} photos", self.visible_ids().len()),
        };
        self.view.label(cx, ids!(title)).set_text(cx, &title);
        self.view.label(cx, ids!(subtitle)).set_text(cx, &subtitle);
        if let Some(mut label) = self.view.label(cx, ids!(title)).borrow_mut() {
            label.draw_text.text_style.font_size = if root { 30.0 } else { 20.0 };
        }
        self.view.button(cx, ids!(back)).set_visible(cx, !root);
        self.view
            .button(cx, ids!(create))
            .set_visible(cx, self.route == Route::Collections);
        self.view
            .button(cx, ids!(edit))
            .set_visible(cx, matches!(self.route, Route::Album(_)));
        self.view.button(cx, ids!(save)).set_visible(cx, editor);
        self.view
            .view(cx, ids!(search_bar))
            .set_visible(cx, self.searching && self.route == Route::Library);
        self.view.view(cx, ids!(editor_bar)).set_visible(cx, editor);
        self.view
            .view(cx, ids!(editor_footer))
            .set_visible(cx, editor);
        self.view
            .button(cx, ids!(delete))
            .set_visible(cx, editor && self.editor_id.is_some());
        self.view.button(cx, ids!(delete)).set_text(
            cx,
            if self.confirming_delete {
                "Confirm delete"
            } else {
                "Delete album"
            },
        );
        self.view.label(cx, ids!(editor_caption)).set_text(
            cx,
            &format!(
                "{} selected · Tap photos to include them",
                self.selected.len()
            ),
        );
        self.view.widget(cx, ids!(list)).set_visible(cx, !viewer);
        self.view.view(cx, ids!(viewer)).set_visible(cx, viewer);
        self.view
            .view(cx, ids!(footer))
            .set_visible(cx, !viewer && !editor);
        let active_library = self.route == Route::Library;
        self.view
            .kit_bottom_navigation(cx, ids!(navigation))
            .select(cx, if active_library { 0 } else { 1 });
        if viewer {
            self.refresh_viewer(cx);
        }
    }

    fn photo(&self, id: &str) -> Option<&Photo> {
        self.catalog.iter().find(|p| p.id == id)
    }
    fn current_photo(&self) -> Option<&Photo> {
        self.viewer_ids
            .get(self.viewer_index)
            .and_then(|id| self.photo(id))
    }

    fn set_image(&mut self, cx: &mut Cx, image: ImageRef, id: Option<&str>) {
        image.set_visible(cx, id.is_some());
        let Some(photo) = id.and_then(|id| self.photo(id)) else {
            image.set_texture(cx, None);
            self.image_keys.remove(&image.widget_uid());
            return;
        };
        let file = photo.file.clone();
        if self.image_keys.get(&image.widget_uid()) == Some(&file) {
            return;
        }
        if let Some(bytes) = photo_bytes(&file) {
            let key = PathBuf::from(format!("octosense-photos/{file}"));
            if let Err(error) = image.load_image_from_data_async(cx, &key, Arc::new(bytes)) {
                self.message(cx, &format!("Couldn't load this photo: {error:?}"));
            }
            self.image_keys.insert(image.widget_uid(), file);
        }
    }

    fn fill_row(&mut self, cx: &mut Cx, item: &WidgetRef, row: &Row, width: f64) {
        match row {
            Row::Heading(title, detail) => {
                item.label(cx, ids!(title)).set_text(cx, title);
                item.label(cx, ids!(detail)).set_text(cx, detail);
            }
            Row::Memory(index) => {
                let memory = self.memories[*index].clone();
                let cover = if *index == 0 && memory.photos.iter().any(|id| id == "family") {
                    Some("family")
                } else {
                    memory.photos.first().map(String::as_str)
                };
                self.set_image(cx, item.image(cx, ids!(image)), cover);
                item.label(cx, ids!(title)).set_text(cx, &memory.title);
                item.label(cx, ids!(date)).set_text(
                    cx,
                    &format!(
                        "{} · {} photos",
                        display_date(&memory.date),
                        memory.photos.len()
                    ),
                );
            }
            Row::Albums(ids) => {
                for (slot, id) in [id!(first), id!(second)]
                    .into_iter()
                    .zip((0..2).map(|i| ids.get(i)))
                {
                    let cell = item.widget(cx, &[slot]);
                    if let Some(album) = id
                        .and_then(|id| self.store.albums.iter().find(|a| a.id == *id))
                        .cloned()
                    {
                        self.set_image(
                            cx,
                            cell.image(cx, ids!(cover)),
                            album.photos.first().map(String::as_str),
                        );
                        cell.label(cx, ids!(title)).set_text(cx, &album.title);
                        cell.label(cx, ids!(count))
                            .set_text(cx, &format!("{} photos", album.photos.len()));
                    } else {
                        self.set_image(cx, cell.image(cx, ids!(cover)), None);
                        cell.label(cx, ids!(title)).set_text(cx, "");
                        cell.label(cx, ids!(count)).set_text(cx, "");
                    }
                }
            }
            Row::People(names) => {
                for (i, slot) in [id!(first), id!(second), id!(third)]
                    .into_iter()
                    .enumerate()
                {
                    let cell = item.widget(cx, &[slot]);
                    let name = names.get(i).cloned().unwrap_or_default();
                    let photo = self
                        .catalog
                        .iter()
                        .find(|p| p.people.len() == 1 && p.people[0] == name)
                        .map(|p| p.id.clone());
                    self.set_image(cx, cell.image(cx, ids!(cover)), photo.as_deref());
                    cell.label(cx, ids!(title)).set_text(cx, &name);
                }
            }
            Row::Grid(ids) => {
                if let Some(mut row) = item.borrow_mut::<View>() {
                    row.walk.height = Size::Fixed(((width - 4.0) / 3.0).max(76.0));
                }
                for (i, slot) in [id!(first), id!(second), id!(third)]
                    .into_iter()
                    .enumerate()
                {
                    let cell = item.widget(cx, &[slot]);
                    let id = ids.get(i);
                    self.set_image(cx, cell.image(cx, ids!(image)), id.map(String::as_str));
                    let mark = if self.route == Route::Editor {
                        if id.is_some_and(|id| self.selected.contains(id)) {
                            "●"
                        } else {
                            "○"
                        }
                    } else if id.is_some_and(|id| self.store.favorites.contains(id)) {
                        "♥"
                    } else {
                        ""
                    };
                    cell.label(cx, ids!(mark))
                        .set_text(cx, if id.is_some() { mark } else { "" });
                }
            }
            Row::Utility => item.button(cx, ids!(favorite_collection)).set_text(
                cx,
                &format!("Favorites · {} photos", self.store.favorites.len()),
            ),
            Row::Empty(text) | Row::End(text) => item.label(cx, ids!(title)).set_text(cx, text),
        }
    }

    fn navigate(&mut self, cx: &mut Cx, route: Route) {
        self.stop_playback(cx);
        self.route = route;
        self.message(cx, "");
        cx.hide_text_ime();
        self.rebuild(cx);
    }

    fn edit_album(&mut self, cx: &mut Cx, id: Option<u64>) {
        self.editor_return = self.route.clone();
        self.editor_id = id;
        self.confirming_delete = false;
        let album = id
            .and_then(|id| self.store.albums.iter().find(|a| a.id == id))
            .cloned();
        self.selected = album
            .as_ref()
            .map(|a| a.photos.iter().cloned().collect())
            .unwrap_or_default();
        self.view
            .text_input(cx, ids!(name_input))
            .set_text(cx, &album.map(|a| a.title).unwrap_or_default());
        self.navigate(cx, Route::Editor);
    }

    fn open_photo(&mut self, cx: &mut Cx, id: &str) {
        self.return_route = self.route.clone();
        self.viewer_ids = self.photo_ids.clone();
        self.viewer_index = self.viewer_ids.iter().position(|p| p == id).unwrap_or(0);
        self.memory_index = None;
        self.navigate(cx, Route::Viewer);
    }

    fn open_memory(&mut self, cx: &mut Cx, index: usize) {
        self.return_route = Route::Collections;
        self.viewer_ids = self.memories[index].photos.clone();
        self.viewer_index = 0;
        self.memory_index = Some(index);
        self.navigate(cx, Route::Viewer);
        self.playing = true;
        self.slide_timer = cx.start_interval(3.0);
        self.sync_chrome(cx);
    }

    pub fn stop_playback(&mut self, cx: &mut Cx) {
        cx.stop_timer(self.slide_timer);
        self.slide_timer = Timer::default();
        self.playing = false;
    }

    fn advance(&mut self, cx: &mut Cx, offset: isize) {
        if self.viewer_ids.is_empty() {
            return;
        }
        self.viewer_index = (self.viewer_index as isize + offset)
            .rem_euclid(self.viewer_ids.len() as isize) as usize;
        self.sync_chrome(cx);
        self.view.redraw(cx);
    }

    fn refresh_viewer(&mut self, cx: &mut Cx) {
        let id = self.viewer_ids.get(self.viewer_index).cloned();
        let image = self.view.image(cx, ids!(full_image));
        self.set_image(cx, image, id.as_deref());
        if let Some(mut button) = self.view.button(cx, ids!(favorite)).borrow_mut() {
            button.draw_icon.load_from_str(
                if id
                    .as_ref()
                    .is_some_and(|id| self.store.favorites.contains(id))
                {
                    HEART_FILLED_ICON
                } else {
                    HEART_ICON
                },
            );
        }
        self.view.label(cx, ids!(position)).set_text(
            cx,
            &format!("{} / {}", self.viewer_index + 1, self.viewer_ids.len()),
        );
        self.view
            .button(cx, ids!(play_pause))
            .set_visible(cx, self.memory_index.is_some());
        self.view
            .button(cx, ids!(play_pause))
            .set_text(cx, if self.playing { "Pause" } else { "Play" });
        self.view
            .view(cx, ids!(memory_caption))
            .set_visible(cx, self.memory_index.is_some());
        if let Some(memory) = self.memory_index.and_then(|i| self.memories.get(i)) {
            self.view
                .label(cx, ids!(memory_title))
                .set_text(cx, &memory.title);
            self.view
                .label(cx, ids!(memory_date))
                .set_text(cx, &display_date(&memory.date));
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if let Some(index) = self
            .view
            .kit_bottom_navigation(cx, ids!(navigation))
            .selected(actions)
        {
            self.searching = false;
            self.navigate(
                cx,
                if index == 0 {
                    Route::Library
                } else {
                    Route::Collections
                },
            );
        }
        if self.view.button(cx, ids!(search)).clicked(actions) {
            self.searching = !self.searching;
            self.query.clear();
            self.view
                .text_input(cx, ids!(search_input))
                .set_text(cx, "");
            self.navigate(cx, Route::Library);
        }
        if let Some(query) = self
            .view
            .text_input(cx, ids!(search_input))
            .changed(actions)
        {
            self.query = query;
            self.rebuild(cx);
        }
        if self.view.button(cx, ids!(create)).clicked(actions) {
            self.edit_album(cx, None);
        }
        if self.view.button(cx, ids!(edit)).clicked(actions) {
            if let Route::Album(id) = self.route {
                self.edit_album(cx, Some(id));
            }
        }
        if self.view.button(cx, ids!(back)).clicked(actions)
            || self.view.button(cx, ids!(cancel)).clicked(actions)
        {
            self.go_back(cx);
        }
        if self.view.button(cx, ids!(save)).clicked(actions) && self.route == Route::Editor {
            let name = self.view.text_input(cx, ids!(name_input)).text();
            let photos = self
                .catalog
                .iter()
                .filter(|p| self.selected.contains(&p.id))
                .map(|p| p.id.clone())
                .collect();
            let previous = self.store.clone();
            match self
                .store
                .save_album(self.editor_id, &name, photos, &self.catalog)
            {
                Ok(id) => {
                    if self.persist(cx, previous) {
                        self.navigate(cx, Route::Album(id));
                    }
                }
                Err(error) => self.message(cx, &error),
            }
        }
        if self.view.button(cx, ids!(delete)).clicked(actions) && self.route == Route::Editor {
            if self.confirming_delete {
                let previous = self.store.clone();
                if let Some(id) = self.editor_id {
                    self.store.delete_album(id);
                }
                if self.persist(cx, previous) {
                    self.navigate(cx, Route::Collections);
                }
            } else {
                self.confirming_delete = true;
                self.sync_chrome(cx);
            }
        }
        if self.route == Route::Viewer {
            if self.view.button(cx, ids!(previous)).clicked(actions) {
                self.advance(cx, -1);
            }
            if self.view.button(cx, ids!(next)).clicked(actions) {
                self.advance(cx, 1);
            }
            if let Some(up) = self.view.view(cx, ids!(stage)).finger_up(actions) {
                let delta = up.abs.x - up.abs_start.x;
                if delta.abs() > 45.0 && (up.abs.y - up.abs_start.y).abs() < 80.0 {
                    self.advance(cx, if delta < 0.0 { 1 } else { -1 });
                }
            }
            if self.view.button(cx, ids!(favorite)).clicked(actions) {
                if let Some(id) = self.viewer_ids.get(self.viewer_index).cloned() {
                    let previous = self.store.clone();
                    self.store.toggle_favorite(&id);
                    self.persist(cx, previous);
                    self.refresh_viewer(cx);
                }
            }
            if self.view.button(cx, ids!(play_pause)).clicked(actions) {
                if self.playing {
                    self.stop_playback(cx);
                } else {
                    self.playing = true;
                    self.slide_timer = cx.start_interval(3.0);
                }
                self.refresh_viewer(cx);
            }
            return;
        }
        let list = self.view.portal_list(cx, ids!(list));
        for (index, item) in list.items_with_actions(actions) {
            let Some(row) = self.rows.get(index).cloned() else {
                continue;
            };
            match row {
                Row::Memory(index) => {
                    if tapped(&item, cx, ids!(card), actions) {
                        self.open_memory(cx, index);
                        break;
                    }
                }
                Row::Albums(ids) => {
                    for (i, slot) in [id!(first), id!(second)].into_iter().enumerate() {
                        if tapped(&item, cx, &[slot], actions) {
                            if let Some(id) = ids.get(i) {
                                self.navigate(cx, Route::Album(*id));
                            }
                            break;
                        }
                    }
                }
                Row::People(names) => {
                    for (i, slot) in [id!(first), id!(second), id!(third)]
                        .into_iter()
                        .enumerate()
                    {
                        if tapped(&item, cx, &[slot], actions) {
                            if let Some(name) = names.get(i) {
                                self.navigate(cx, Route::Person(name.clone()));
                            }
                            break;
                        }
                    }
                }
                Row::Grid(ids) => {
                    for (i, slot) in [id!(first), id!(second), id!(third)]
                        .into_iter()
                        .enumerate()
                    {
                        if tapped(&item, cx, &[slot], actions) {
                            if let Some(id) = ids.get(i) {
                                if self.route == Route::Editor {
                                    if !self.selected.remove(id) {
                                        self.selected.insert(id.clone());
                                    }
                                    self.sync_chrome(cx);
                                    self.view.redraw(cx);
                                } else {
                                    self.open_photo(cx, id);
                                }
                                break;
                            }
                        }
                    }
                }
                Row::Utility => {
                    if item.button(cx, ids!(favorite_collection)).clicked(actions) {
                        self.navigate(cx, Route::Favorites);
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    fn go_back(&mut self, cx: &mut Cx) {
        let route = match self.route {
            Route::Viewer => self.return_route.clone(),
            Route::Editor => self.editor_return.clone(),
            _ => Route::Collections,
        };
        self.navigate(cx, route);
    }
}

fn tapped(item: &WidgetRef, cx: &mut Cx, path: &[LiveId], actions: &Actions) -> bool {
    item.view(cx, path)
        .finger_up(actions)
        .is_some_and(|e| e.is_over && e.was_tap())
}

fn month_title(date: &str) -> String {
    let months = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let month = date
        .get(5..7)
        .and_then(|s| s.parse::<usize>().ok())
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| months.get(i))
        .copied()
        .unwrap_or("");
    format!("{} {}", month, date.get(..4).unwrap_or(date))
}
fn display_date(date: &str) -> String {
    let title = month_title(date);
    let day = date.get(8..10).and_then(|s| s.parse::<u32>().ok());
    if let Some(day) = day {
        format!(
            "{} {}, {}",
            title.split(' ').next().unwrap_or(""),
            day,
            date.get(..4).unwrap_or("")
        )
    } else {
        title
    }
}

impl Widget for PhotosView {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.initialize(cx);
        while let Some(widget) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = widget.borrow_mut::<PortalList>() {
                let width = cx.turtle().rect().size.x.max(1.0);
                list.set_item_range(cx, 0, self.rows.len());
                while let Some(index) = list.next_visible_item(cx) {
                    let Some(row) = self.rows.get(index).cloned() else {
                        continue;
                    };
                    let template = match &row {
                        Row::Heading(..) => id!(Heading),
                        Row::Memory(_) => id!(Memory),
                        Row::Albums(_) => id!(Albums),
                        Row::People(_) => id!(People),
                        Row::Grid(_) => id!(Grid),
                        Row::Utility => id!(Utility),
                        Row::Empty(_) => id!(Empty),
                        Row::End(_) => id!(End),
                    };
                    let item = list.item(cx, index, template);
                    self.fill_row(cx, &item, &row, width);
                    item.draw_all(cx, &mut Scope::empty());
                }
            }
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.initialize(cx);
        if let Event::Custom(json) = event {
            if let Some(mode) = HostedViewMode::parse(json) {
                if self.mode != mode {
                    self.mode = mode;
                    if mode == HostedViewMode::Tile {
                        self.stop_playback(cx);
                    }
                    self.sync_chrome(cx);
                    self.view.redraw(cx);
                }
            }
        }
        if matches!(event, Event::Pause | Event::Background | Event::Shutdown) {
            self.stop_playback(cx);
        }
        if self.mode == HostedViewMode::Tile {
            // Keep async image loading alive; the shell owns tapping the card.
            self.view.handle_event(cx, event, scope);
            return;
        }
        if !matches!(self.route, Route::Library | Route::Collections) && event.back_pressed() {
            self.go_back(cx);
        }
        if self.slide_timer.is_event(event).is_some() && self.playing && self.route == Route::Viewer
        {
            self.advance(cx, 1);
        }
        if let Event::KeyDown(key) = event {
            match key.key_code {
                KeyCode::Escape if !matches!(self.route, Route::Library | Route::Collections) => {
                    self.go_back(cx)
                }
                KeyCode::ArrowRight if self.route == Route::Viewer => self.advance(cx, 1),
                KeyCode::ArrowLeft if self.route == Route::Viewer => self.advance(cx, -1),
                _ => {}
            }
        }
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        self.handle_actions(cx, &actions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_card_round_trip_preserves_the_album_draft() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut root = WidgetRef::empty();
        cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            crate::script_mod(vm);
            let value = script_eval!(vm, {use mod.widgets.* PhotosView{}});
            root = WidgetRef::script_from_value(vm, value);
            assert!(vm.take_errors().is_empty());
        });
        let mut view = root.borrow_mut::<PhotosView>().unwrap();
        // Isolate the transition from on-disk storage and image decoding.
        view.initialized = true;
        view.route = Route::Editor;
        view.editor_return = Route::Album(7);
        view.editor_id = Some(7);
        view.selected.insert("family".into());
        view.view
            .text_input(&mut cx, ids!(name_input))
            .set_text(&mut cx, "Weekend draft");
        view.message(&mut cx, "A saved error message");
        view.sync_chrome(&mut cx);

        view.handle_event(
            &mut cx,
            &Event::Custom(HostedViewMode::Tile.to_json()),
            &mut Scope::empty(),
        );
        assert!(view.view.widget(&mut cx, ids!(compact)).visible());
        for id in [
            id!(header),
            id!(body),
            id!(editor_bar),
            id!(editor_footer),
            id!(footer),
            id!(message),
        ] {
            assert!(!view.view.widget(&mut cx, &[id]).visible());
        }

        view.handle_event(
            &mut cx,
            &Event::Custom(HostedViewMode::Full.to_json()),
            &mut Scope::empty(),
        );
        assert!(!view.view.widget(&mut cx, ids!(compact)).visible());
        for id in [
            id!(header),
            id!(body),
            id!(editor_bar),
            id!(editor_footer),
            id!(message),
        ] {
            assert!(view.view.widget(&mut cx, &[id]).visible());
        }
        assert!(matches!(view.route, Route::Editor));
        assert!(matches!(view.editor_return, Route::Album(7)));
        assert_eq!(view.editor_id, Some(7));
        assert_eq!(view.selected, BTreeSet::from(["family".into()]));
        assert_eq!(
            view.view.text_input(&mut cx, ids!(name_input)).text(),
            "Weekend draft"
        );

        // A playing Memory pauses on the home card and retains its position.
        view.route = Route::Viewer;
        view.viewer_ids = vec!["family".into(), "alpine".into()];
        view.viewer_index = 1;
        view.playing = true;
        view.handle_event(
            &mut cx,
            &Event::Custom(HostedViewMode::Tile.to_json()),
            &mut Scope::empty(),
        );
        assert!(!view.playing);
        view.handle_event(
            &mut cx,
            &Event::Custom(HostedViewMode::Full.to_json()),
            &mut Scope::empty(),
        );
        assert!(matches!(view.route, Route::Viewer));
        assert_eq!(view.viewer_index, 1);
        assert!(!view.playing);
        assert!(view.view.widget(&mut cx, ids!(viewer)).visible());
    }
}
