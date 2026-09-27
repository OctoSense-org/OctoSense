use crate::model::{self, Memory, Photo, Store};
use crate::zoom::{LibraryZoom, PinchTracker};
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
/// The Library's density at zoom 1.0, and the layout every other collection
/// keeps. `Grid` rows carry one cell per slot; extra slots stay hidden.
const DEFAULT_GRID_COLUMNS: usize = 3;

/// The zoom slider is for pointer platforms; a touch device pinches instead.
fn shows_zoom_slider(os: &OsType) -> bool {
    matches!(
        os,
        OsType::Windows | OsType::Macos | OsType::LinuxWindow(_) | OsType::LinuxDirect
    )
}

/// The Grid row's cell slots, in draw order.
fn grid_slots() -> [LiveId; 9] {
    [
        id!(first),
        id!(second),
        id!(third),
        id!(fourth),
        id!(fifth),
        id!(sixth),
        id!(seventh),
        id!(eighth),
        id!(ninth),
    ]
}

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
    #[rust]
    zoom: LibraryZoom,
    #[rust]
    pinch: PinchTracker,
    /// The photo a running zoom holds, with the screen offset of its row.
    #[rust]
    zoom_anchor: Option<(String, f64)>,
    /// Grid geometry from the last draw, so a gesture knows what it grabbed.
    #[rust]
    grid_rect: Rect,
    #[rust]
    row_rects: Vec<(usize, Rect)>,
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

    /// Photos per grid row. Only the Library zooms; the other collections
    /// keep the layout their tiles were designed for.
    fn grid_columns(&self) -> usize {
        if self.route == Route::Library {
            self.zoom.columns()
        } else {
            DEFAULT_GRID_COLUMNS
        }
    }

    fn rebuild_rows(&mut self) {
        let columns = self.grid_columns();
        self.rows.clear();
        // The drawn geometry belongs to the rows we are replacing.
        self.row_rects.clear();
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
                    for chunk in ids.chunks(columns) {
                        self.rows.push(Row::Grid(chunk.to_vec()));
                    }
                }
            } else {
                for chunk in self.photo_ids.chunks(columns) {
                    self.rows.push(Row::Grid(chunk.to_vec()));
                }
            }
            self.rows
                .push(Row::End(format!("{} photos", self.photo_ids.len())));
        }
    }

    fn rebuild(&mut self, cx: &mut Cx) {
        self.rebuild_rows();
        self.view
            .portal_list(cx, ids!(list))
            .set_first_id_and_scroll(0, 0.0);
        self.sync_chrome(cx);
        self.view.redraw(cx);
    }

    /// Reflows the grid at the current zoom, leaving the anchored photo where
    /// the gesture found it instead of jumping back to the top of the library.
    fn apply_zoom(&mut self, cx: &mut Cx) {
        self.rebuild_rows();
        if let Some((id, offset)) = self.zoom_anchor.clone() {
            if let Some(row) = self
                .rows
                .iter()
                .position(|row| matches!(row, Row::Grid(ids) if ids.iter().any(|i| *i == id)))
            {
                self.view
                    .portal_list(cx, ids!(list))
                    .set_first_id_and_scroll(row, offset);
            }
        }
        self.view
            .slider(cx, ids!(zoom_slider))
            .set_value(cx, self.zoom.normalized());
        self.view.redraw(cx);
    }

    /// The desktop knob, driving the state the gestures drive. The photo at the
    /// top of the grid stands in for the pointer the knob does not have.
    fn set_zoom_position(&mut self, cx: &mut Cx, position: f64) {
        if let Some(anchor) = self.anchor_at(self.grid_rect.pos + dvec2(1.0, 1.0)) {
            self.zoom_anchor = Some(anchor);
        }
        self.zoom.set_normalized(position);
        self.apply_zoom(cx);
    }

    /// The photo drawn under `pos`, with the screen offset of its row, so a
    /// reflow can put that photo back under the fingers.
    fn anchor_at(&self, pos: DVec2) -> Option<(String, f64)> {
        let columns = self.grid_columns() as f64;
        self.row_rects.iter().find_map(|(index, rect)| {
            if pos.y < rect.pos.y || pos.y > rect.pos.y + rect.size.y {
                return None;
            }
            let Some(Row::Grid(ids)) = self.rows.get(*index) else {
                return None;
            };
            let column = ((pos.x - rect.pos.x) / rect.size.x.max(1.0) * columns).floor();
            let column = (column.max(0.0) as usize).min(ids.len().checked_sub(1)?);
            Some((ids[column].clone(), rect.pos.y - self.grid_rect.pos.y))
        })
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
                id!(zoom_bar),
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
        // The knob mirrors the pinch, where there is a pointer to drag it with.
        let zooming = self.route == Route::Library && shows_zoom_slider(cx.os_type());
        self.view.view(cx, ids!(zoom_bar)).set_visible(cx, zooming);
        self.view
            .slider(cx, ids!(zoom_slider))
            .set_value(cx, self.zoom.normalized());
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
                let columns = self.grid_columns();
                // Square cells: the row's own bottom padding keeps the gutter.
                let side = ((width - 2.0 * (columns - 1) as f64) / columns as f64).max(24.0);
                if let Some(mut row) = item.borrow_mut::<View>() {
                    row.walk.height = Size::Fixed(side + 2.0);
                }
                for (i, slot) in grid_slots().into_iter().enumerate() {
                    let cell = item.widget(cx, &[slot]);
                    cell.set_visible(cx, i < columns);
                    if i >= columns {
                        continue;
                    }
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
        if let Some(position) = self.view.slider(cx, ids!(zoom_slider)).slided(actions) {
            self.set_zoom_position(cx, position);
        }
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
                    for (i, slot) in grid_slots().into_iter().enumerate() {
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
                self.grid_rect = cx.turtle().rect();
                let width = self.grid_rect.size.x.max(1.0);
                self.row_rects.clear();
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
                    if matches!(row, Row::Grid(_)) {
                        self.row_rects.push((index, item.area().rect(cx)));
                    }
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
        // Zoom belongs to the Library grid: a pinch or a wheel notch over the
        // photos resizes them instead of scrolling the list or opening a photo.
        if self.route == Route::Library {
            match event {
                Event::TouchUpdate(touch) => {
                    let pinch = self.pinch.update(touch, self.grid_rect, self.zoom.scale());
                    if pinch.began {
                        self.zoom_anchor = self.anchor_at(pinch.center);
                    }
                    if let Some(scale) = pinch.scale {
                        self.zoom.set_scale(scale);
                        self.apply_zoom(cx);
                    }
                    if pinch.consumed {
                        return;
                    }
                }
                Event::Scroll(scroll) if self.grid_rect.contains(scroll.abs) => {
                    if let Some(anchor) = self.anchor_at(scroll.abs) {
                        self.zoom_anchor = Some(anchor);
                    }
                    self.zoom.wheel(scroll.scroll.y);
                    self.apply_zoom(cx);
                    return;
                }
                _ => {}
            }
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
    use makepad_widgets::makepad_platform::event::{
        ScrollEvent, ScrollPhase, TouchPoint, TouchState, TouchUpdateEvent,
    };
    use makepad_widgets::makepad_platform::LinuxWindowParams;
    use std::cell::Cell;

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

    /// The photo count of every grid row, so density changes read at a glance.
    fn grid_widths(rows: &[Row]) -> Vec<usize> {
        rows.iter()
            .filter_map(|row| match row {
                Row::Grid(ids) => Some(ids.len()),
                _ => None,
            })
            .collect()
    }

    /// A drawn Library of two dozen photos from one month, without the
    /// on-disk storage or image decoding a real one would load.
    fn library(cx: &mut Cx) -> WidgetRef {
        let mut root = WidgetRef::empty();
        cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            crate::script_mod(vm);
            let value = script_eval!(vm, {use mod.widgets.* PhotosView{}});
            root = WidgetRef::script_from_value(vm, value);
            assert!(vm.take_errors().is_empty());
        });
        {
            let mut view = root.borrow_mut::<PhotosView>().unwrap();
            view.initialized = true;
            view.catalog = (0..24)
                .map(|i| Photo {
                    id: format!("p{i:02}"),
                    file: String::new(),
                    title: format!("Photo {i}"),
                    date: "2026-09-04".into(),
                    location: "Home".into(),
                    people: Vec::new(),
                    tags: Vec::new(),
                    moment: "Morning".into(),
                })
                .collect();
            view.route = Route::Library;
            view.rebuild(cx);
            // Stand in for the last draw: the grid viewport, and its first row
            // of photos sitting at the top of it.
            view.grid_rect = Rect {
                pos: dvec2(0.0, 100.0),
                size: dvec2(300.0, 400.0),
            };
            view.row_rects = vec![(
                1,
                Rect {
                    pos: dvec2(0.0, 100.0),
                    size: dvec2(300.0, 100.0),
                },
            )];
        }
        root
    }

    fn scroll(abs: DVec2, delta_y: f64) -> Event {
        Event::Scroll(ScrollEvent {
            window_id: WindowId(0, 0),
            scroll: dvec2(0.0, delta_y),
            abs,
            modifiers: KeyModifiers::default(),
            handled_x: Cell::new(false),
            handled_y: Cell::new(false),
            is_mouse: true,
            time: 1.0,
            phase: ScrollPhase::None,
        })
    }

    fn touch(points: &[(u64, TouchState, f64, f64)]) -> Event {
        Event::TouchUpdate(TouchUpdateEvent {
            time: 1.0,
            window_id: WindowId(0, 0),
            modifiers: KeyModifiers::default(),
            touches: points
                .iter()
                .map(|&(uid, state, x, y)| TouchPoint {
                    uid,
                    state,
                    abs: dvec2(x, y),
                    time: 1.0,
                    rotation_angle: 0.0,
                    force: 1.0,
                    radius: dvec2(1.0, 1.0),
                    handled: Cell::new(Area::Empty),
                    sweep_lock: Cell::new(Area::Empty),
                })
                .collect(),
        })
    }

    #[test]
    fn library_zoom_reflows_rows_and_holds_the_focal_photo() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let root = library(&mut cx);
        let mut view = root.borrow_mut::<PhotosView>().unwrap();
        assert_eq!(grid_widths(&view.rows), vec![3; 8]);

        // Zooming out reflows the same photos into denser rows and keeps the
        // photo the gesture started on in view instead of jumping to the top.
        view.zoom_anchor = Some(("p12".into(), 0.0));
        view.zoom.set_scale(0.5);
        view.apply_zoom(&mut cx);
        assert_eq!(grid_widths(&view.rows), vec![6; 4]);
        let first = view.view.portal_list(&mut cx, ids!(list)).first_id();
        assert!(
            matches!(view.rows.get(first), Some(Row::Grid(ids)) if ids.iter().any(|id| id == "p12")),
            "row {first} should hold the focal photo"
        );

        // Zooming all the way in keeps that same photo anchored.
        view.zoom.set_scale(3.0);
        view.apply_zoom(&mut cx);
        assert_eq!(grid_widths(&view.rows), vec![1; 24]);
        let first = view.view.portal_list(&mut cx, ids!(list)).first_id();
        assert!(
            matches!(view.rows.get(first), Some(Row::Grid(ids)) if ids.iter().any(|id| id == "p12")),
            "row {first} should hold the focal photo"
        );

        // Other collections keep the fixed three-photo layout.
        view.route = Route::Favorites;
        view.store.favorites = view.catalog.iter().map(|p| p.id.clone()).collect();
        view.rebuild(&mut cx);
        assert_eq!(grid_widths(&view.rows), vec![3; 8]);
    }

    #[test]
    fn only_a_wheel_over_the_library_grid_zooms_it() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let root = library(&mut cx);
        let mut view = root.borrow_mut::<PhotosView>().unwrap();

        // A notch over the photos shows more of them, anchored on the photo
        // under the pointer rather than snapping back to the top.
        view.handle_event(
            &mut cx,
            &scroll(dvec2(150.0, 150.0), 200.0),
            &mut Scope::empty(),
        );
        let zoomed = view.zoom.columns();
        assert!(zoomed > DEFAULT_GRID_COLUMNS);
        assert_eq!(
            view.zoom_anchor.as_ref().map(|(id, _)| id.as_str()),
            Some("p01")
        );
        assert_eq!(grid_widths(&view.rows).first().copied(), Some(zoomed));

        // Above the grid the wheel still belongs to the page.
        view.handle_event(
            &mut cx,
            &scroll(dvec2(150.0, 20.0), 200.0),
            &mut Scope::empty(),
        );
        assert_eq!(view.zoom.columns(), zoomed);

        // Collections never zoom, wherever the pointer is.
        view.route = Route::Collections;
        view.handle_event(
            &mut cx,
            &scroll(dvec2(150.0, 150.0), 200.0),
            &mut Scope::empty(),
        );
        assert_eq!(view.zoom.columns(), zoomed);
    }

    #[test]
    fn the_zoom_slider_is_desktop_only_and_drives_the_same_density() {
        // A phone zooms with its fingers; the knob is for pointer platforms.
        // Android and iOS take the same default-false arm as Unknown, which is
        // the only non-desktop OsType constructible outside the platform crate.
        assert!(shows_zoom_slider(&OsType::Macos));
        assert!(shows_zoom_slider(&OsType::Windows));
        assert!(shows_zoom_slider(&OsType::LinuxDirect));
        assert!(shows_zoom_slider(&OsType::LinuxWindow(
            LinuxWindowParams::default()
        )));
        assert!(!shows_zoom_slider(&OsType::Unknown));

        let mut cx = Cx::new(Box::new(|_, _| {}));
        let root = library(&mut cx);
        let mut view = root.borrow_mut::<PhotosView>().unwrap();
        view.set_zoom_position(&mut cx, 0.0);
        assert_eq!(view.zoom.columns(), 9);
        assert_eq!(grid_widths(&view.rows).first().copied(), Some(9));

        // A gesture moves the knob too, so the two never disagree.
        view.handle_event(
            &mut cx,
            &scroll(dvec2(150.0, 150.0), -10_000.0),
            &mut Scope::empty(),
        );
        assert_eq!(view.zoom.columns(), 1);
        assert_eq!(
            view.view.slider(&mut cx, ids!(zoom_slider)).value(),
            Some(1.0)
        );
    }

    #[test]
    fn a_library_pinch_resizes_the_grid_around_its_midpoint() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let root = library(&mut cx);
        let mut view = root.borrow_mut::<PhotosView>().unwrap();

        for points in [
            vec![(1, TouchState::Start, 100.0, 150.0)],
            vec![
                (1, TouchState::Stable, 100.0, 150.0),
                (2, TouchState::Start, 200.0, 150.0),
            ],
        ] {
            view.handle_event(&mut cx, &touch(&points), &mut Scope::empty());
        }
        // The gesture holds the photo under its midpoint.
        assert_eq!(
            view.zoom_anchor.as_ref().map(|(id, _)| id.as_str()),
            Some("p01")
        );
        assert_eq!(grid_widths(&view.rows), vec![3; 8]);

        // Spreading the fingers makes the photos bigger and the rows shorter.
        view.handle_event(
            &mut cx,
            &touch(&[
                (1, TouchState::Move, 50.0, 150.0),
                (2, TouchState::Move, 250.0, 150.0),
            ]),
            &mut Scope::empty(),
        );
        let zoomed = view.zoom.columns();
        assert!(zoomed < DEFAULT_GRID_COLUMNS);
        assert_eq!(grid_widths(&view.rows).first().copied(), Some(zoomed));
        let first = view.view.portal_list(&mut cx, ids!(list)).first_id();
        assert!(
            matches!(view.rows.get(first), Some(Row::Grid(ids)) if ids.iter().any(|id| id == "p01")),
            "row {first} should hold the focal photo"
        );
    }
}
