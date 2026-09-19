//! What the app knows and where the person is in it, with no widget in
//! sight: the screen state machine, the search, the two ends of a route and
//! a route per mode, the requests in flight (so a superseded reply is
//! dropped and its request cancelled), the settings the app keeps, and the
//! skin.

use crate::geo::{LonLat, Units};
use crate::guidance::ActiveNav;
use crate::places::{self, Place};
use crate::routing::{self, Directions, Mode, RouteError};
use makepad_widgets::makepad_micro_serde::*;
use makepad_widgets::*;
use std::sync::atomic::{AtomicU8, Ordering};

/// A long route's reply is a few hundred kilobytes of line and steps.
pub const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
/// The name every request carries: the services' terms ask who is calling.
pub const USER_AGENT: &str = "OctosMap/0.1 (OctoSense)";
/// The storage key of `MapsModel::saved_state`.
pub const STATE_KEY: &str = "state";

/// Where the map opens before it has ever been anywhere: the Bay Area.
pub const DEFAULT_CENTER: LonLat = LonLat {
    lon: -122.0,
    lat: 37.4,
};
pub const DEFAULT_ZOOM: f64 = 9.5;

/// Which end of the route a search is picking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchTarget {
    Destination,
    Origin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Explore,
    Search {
        target: SearchTarget,
    },
    Place,
    Directions,
    Navigating,
    Arrived,
}

/// One end of a route.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum End {
    /// Wherever the device is: a position once there is a fix.
    #[default]
    MyLocation,
    Place(Place),
}

impl End {
    pub fn label(&self) -> &str {
        match self {
            End::MyLocation => "Your location",
            End::Place(place) => &place.name,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum RouteState {
    #[default]
    Idle,
    Loading,
    Ready(Directions),
    /// In words for the person.
    Failed(String),
}

impl RouteState {
    pub fn directions(&self) -> Option<&Directions> {
        match self {
            RouteState::Ready(directions) => Some(directions),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum SearchState {
    #[default]
    Idle,
    Loading,
    /// The reply landed; `results` may still be empty.
    Done,
    Failed(String),
}

/// What a request in flight is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Search,
    /// Naming a dropped pin.
    Reverse,
    Route(Mode),
    /// A new route from where the driver went astray.
    Reroute,
}

/// What the app keeps between runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// `None`: whatever the destination's country drives in.
    pub units: Option<Units>,
    /// `None`: whatever the skin is.
    pub dark_map: Option<bool>,
    pub buildings_3d: bool,
    pub labels: bool,
    /// The last camera, so the app reopens where it was.
    pub center: LonLat,
    pub zoom: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            units: None,
            dark_map: None,
            buildings_3d: true,
            labels: true,
            center: DEFAULT_CENTER,
            zoom: DEFAULT_ZOOM,
        }
    }
}

#[derive(Default)]
pub struct MapsModel {
    screen: Screen,
    /// Where Search goes back to.
    search_return: Screen,
    pub query: String,
    pub results: Vec<Place>,
    pub search_state: SearchState,
    /// The place the sheet is about.
    place: Option<Place>,
    origin: End,
    destination: End,
    routes: [RouteState; 3],
    mode: Mode,
    /// A reroute's answer, until guidance takes it.
    reroute: Option<Directions>,
    /// Navigation left the routes behind: they are asked for again.
    rerouted: bool,
    fix: Option<LonLat>,
    pub settings: Settings,
    in_flight: Vec<(LiveId, Request)>,
    cancelled: Vec<LiveId>,
    seq: u64,
}

impl MapsModel {
    pub fn screen(&self) -> Screen {
        self.screen
    }

    pub fn place(&self) -> Option<&Place> {
        self.place.as_ref()
    }

    pub fn origin(&self) -> &End {
        &self.origin
    }

    pub fn destination(&self) -> &End {
        &self.destination
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn route(&self, mode: Mode) -> &RouteState {
        &self.routes[mode.index()]
    }

    /// The selected mode's route, when it has one.
    pub fn directions(&self) -> Option<&Directions> {
        self.route(self.mode).directions()
    }

    pub fn fix(&self) -> Option<LonLat> {
        self.fix
    }

    pub fn set_fix(&mut self, fix: LonLat) {
        self.fix = Some(fix);
    }

    /// Where an end is; `None` for the device's location before a fix.
    pub fn end_pos(&self, end: &End) -> Option<LonLat> {
        match end {
            End::MyLocation => self.fix,
            End::Place(place) => Some(place.pos),
        }
    }

    /// Whether Directions is waiting on a fix to route from or to.
    pub fn needs_fix(&self) -> bool {
        self.screen == Screen::Directions && self.ends().is_none()
    }

    /// Both ends' positions, once both have one.
    fn ends(&self) -> Option<(LonLat, LonLat)> {
        Some((
            self.end_pos(&self.origin)?,
            self.end_pos(&self.destination)?,
        ))
    }

    /// Miles or kilometres: the person's choice, else the country's habit.
    pub fn units(&self) -> Units {
        let of_place = || {
            self.place
                .as_ref()
                .map(|place| Units::for_country(&place.country_code))
        };
        self.settings.units.or_else(of_place).unwrap_or_default()
    }

    fn next_id(&mut self, what: &str) -> LiveId {
        self.seq += 1;
        LiveId::from_str(&format!("maps_{what}_{}", self.seq))
    }

    fn begin(&mut self, what: &str, request: Request) -> LiveId {
        let id = self.next_id(what);
        self.in_flight.push((id, request));
        id
    }

    /// Requests of this sort are no longer wanted: their replies will be
    /// dropped, and the view cancels them (`superseded`).
    fn supersede(&mut self, unwanted: impl Fn(Request) -> bool) {
        let cancelled = &mut self.cancelled;
        self.in_flight.retain(|(id, request)| {
            let gone = unwanted(*request);
            if gone {
                cancelled.push(*id);
            }
            !gone
        });
    }

    fn is_in_flight(&self, wanted: impl Fn(Request) -> bool) -> bool {
        self.in_flight.iter().any(|(_, request)| wanted(*request))
    }

    /// An end moved: every route was for the old pair.
    fn clear_routes(&mut self) {
        self.routes = Default::default();
        self.supersede(|request| matches!(request, Request::Route(_)));
    }

    // --- search ---

    pub fn open_search(&mut self, target: SearchTarget) {
        if !matches!(self.screen, Screen::Search { .. }) {
            self.search_return = self.screen;
        }
        self.set_query("");
        self.screen = Screen::Search { target };
    }

    pub fn search_target(&self) -> Option<SearchTarget> {
        match self.screen {
            Screen::Search { target } => Some(target),
            _ => None,
        }
    }

    /// The field changed. A blank one clears the list at once.
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        if query.trim().is_empty() {
            self.results.clear();
            self.search_state = SearchState::Idle;
            self.supersede(|request| request == Request::Search);
        }
    }

    /// The request for the current query, biased to `near`; `None` for a
    /// blank one. Any search still in flight is superseded.
    pub fn begin_search(&mut self, near: LonLat) -> Option<(LiveId, String)> {
        let query = self.query.trim().to_string();
        if query.is_empty() {
            return None;
        }
        self.supersede(|request| request == Request::Search);
        self.search_state = SearchState::Loading;
        Some((
            self.begin("search", Request::Search),
            places::search_url(&query, near),
        ))
    }

    /// A row of the list was picked, for whichever end Search was opened.
    pub fn pick_result(&mut self, index: usize) {
        let (Some(target), Some(place)) = (self.search_target(), self.results.get(index).cloned())
        else {
            return;
        };
        self.supersede(|request| request == Request::Search);
        match (target, self.search_return) {
            (SearchTarget::Origin, _) => {
                self.origin = End::Place(place);
                self.clear_routes();
                self.screen = Screen::Directions;
            }
            // Directions' own destination row: the trip keeps its origin.
            (SearchTarget::Destination, Screen::Directions) => {
                self.destination = End::Place(place.clone());
                self.place = Some(place);
                self.clear_routes();
                self.screen = Screen::Directions;
            }
            (SearchTarget::Destination, _) => self.select_place(place),
        }
    }

    // --- place ---

    /// Show `place` in the sheet. A new place is a new trip: the origin is
    /// the device again and the routes are forgotten.
    pub fn select_place(&mut self, place: Place) {
        // A lookup still naming the last pin would name this place instead.
        self.supersede(|request| request == Request::Reverse);
        self.clear_routes();
        self.origin = End::MyLocation;
        self.place = Some(place);
        self.screen = Screen::Place;
    }

    /// A long press: a pin at once, and the request that will name it.
    pub fn drop_pin(&mut self, at: LonLat) -> (LiveId, String) {
        self.select_place(places::dropped_pin(at));
        (
            self.begin("reverse", Request::Reverse),
            places::reverse_url(at),
        )
    }

    // --- directions ---

    pub fn open_directions(&mut self) {
        let Some(place) = self.place.clone() else {
            return;
        };
        // Back from Directions and in again: the same trip, its routes kept.
        if self.destination != End::Place(place.clone()) {
            self.destination = End::Place(place);
            self.clear_routes();
        }
        self.screen = Screen::Directions;
    }

    pub fn swap_ends(&mut self) {
        std::mem::swap(&mut self.origin, &mut self.destination);
        self.clear_routes();
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// The next route to ask for, one at a time: the selected mode first,
    /// then the others so every tab shows its time. `None` while one is in
    /// flight, when all are asked for, off the Directions screen, or while
    /// an end has no position.
    pub fn next_route_request(&mut self) -> Option<(LiveId, Mode, String)> {
        if self.screen != Screen::Directions
            || self.is_in_flight(|request| matches!(request, Request::Route(_)))
        {
            return None;
        }
        let (from, to) = self.ends()?;
        let idle = |mode: &Mode| matches!(self.routes[mode.index()], RouteState::Idle);
        let mode = Some(self.mode)
            .filter(idle)
            .or_else(|| Mode::ALL.into_iter().find(idle))?;
        self.routes[mode.index()] = RouteState::Loading;
        Some((
            self.begin("route", Request::Route(mode)),
            mode,
            routing::route_url(mode, from, to),
        ))
    }

    /// Ask again for a mode that failed.
    pub fn retry_route(&mut self, mode: Mode) {
        if matches!(self.routes[mode.index()], RouteState::Failed(_)) {
            self.routes[mode.index()] = RouteState::Idle;
        }
    }

    // --- navigation ---

    /// Guidance over the selected mode's route; `None` without one.
    pub fn start_navigation(&mut self, simulate: bool) -> Option<ActiveNav> {
        if self.screen != Screen::Directions {
            return None;
        }
        let directions = self.directions()?.clone();
        self.rerouted = false;
        self.screen = Screen::Navigating;
        Some(ActiveNav::new(directions, simulate))
    }

    /// A new route from `from` to the destination, in the selected mode.
    /// `None` off the Navigating screen or while one is already asked for.
    pub fn begin_reroute(&mut self, from: LonLat) -> Option<(LiveId, String)> {
        if self.screen != Screen::Navigating
            || self.is_in_flight(|request| request == Request::Reroute)
        {
            return None;
        }
        let to = self.end_pos(&self.destination)?;
        Some((
            self.begin("reroute", Request::Reroute),
            routing::route_url(self.mode, from, to),
        ))
    }

    pub fn take_reroute(&mut self) -> Option<Directions> {
        self.reroute.take()
    }

    pub fn arrive(&mut self) {
        if self.screen == Screen::Navigating {
            self.screen = Screen::Arrived;
        }
    }

    /// The drive is over, by arrival or by End.
    fn end_navigation(&mut self) {
        self.supersede(|request| request == Request::Reroute);
        self.reroute = None;
        // A rerouted route starts somewhere along the way, not at the origin.
        if std::mem::take(&mut self.rerouted) {
            self.clear_routes();
        }
    }

    // --- requests ---

    pub fn owns(&self, id: LiveId) -> Option<Request> {
        self.in_flight
            .iter()
            .find(|(owned, _)| *owned == id)
            .map(|(_, request)| *request)
    }

    /// Land a reply: `Ok` is the body's text, `Err` why there is none.
    /// Returns what it was for, or `None` for a request no longer wanted.
    pub fn complete(&mut self, id: LiveId, body: Result<String, String>) -> Option<Request> {
        let at = self.in_flight.iter().position(|(owned, _)| *owned == id)?;
        let (_, request) = self.in_flight.remove(at);
        match request {
            Request::Search => match body.and_then(|text| places::parse(&text)) {
                Ok(results) => {
                    self.results = results;
                    self.search_state = SearchState::Done;
                }
                // The list keeps what it had: a failure is not an empty answer.
                Err(why) => self.search_state = SearchState::Failed(why),
            },
            Request::Reverse => {
                // Nothing found, or no answer: the pin stays a pin.
                let found = body
                    .and_then(|text| places::parse(&text))
                    .ok()
                    .and_then(|places| places.into_iter().next());
                if let (Some(found), Some(pin)) = (found, self.place.as_mut()) {
                    // Where the finger was, not where the street's own point is.
                    *pin = Place {
                        pos: pin.pos,
                        ..found
                    };
                }
            }
            Request::Route(mode) => self.routes[mode.index()] = route_state(mode, body),
            Request::Reroute => {
                if let RouteState::Ready(directions) = route_state(self.mode, body) {
                    self.reroute = Some(directions.clone());
                    self.routes[self.mode.index()] = RouteState::Ready(directions);
                    self.rerouted = true;
                }
            }
        }
        Some(request)
    }

    /// Requests that were superseded since the last call: cancel them.
    pub fn superseded(&mut self) -> Vec<LiveId> {
        std::mem::take(&mut self.cancelled)
    }

    /// Everything in flight, for a shutdown.
    pub fn cancel_all(&mut self) -> Vec<LiveId> {
        let mut ids = std::mem::take(&mut self.cancelled);
        ids.extend(self.in_flight.drain(..).map(|(id, _)| id));
        ids
    }

    /// One step back; `false` at Explore, where the host takes over.
    pub fn back(&mut self) -> bool {
        let after_place = if self.place.is_some() {
            Screen::Place
        } else {
            Screen::Explore
        };
        self.screen = match self.screen {
            Screen::Explore => return false,
            Screen::Search { .. } => {
                self.set_query("");
                self.search_return
            }
            Screen::Place => {
                self.supersede(|request| request == Request::Reverse);
                self.clear_routes();
                self.place = None;
                Screen::Explore
            }
            Screen::Directions => after_place,
            Screen::Navigating => {
                self.end_navigation();
                Screen::Directions
            }
            Screen::Arrived => {
                self.end_navigation();
                after_place
            }
        };
        true
    }

    // --- what the app keeps ---

    pub fn saved_state(&self) -> Vec<u8> {
        let settings = &self.settings;
        SavedState {
            center_lon: Some(settings.center.lon),
            center_lat: Some(settings.center.lat),
            zoom: Some(settings.zoom),
            imperial: settings.units.map(|units| units == Units::Imperial),
            dark_map: settings.dark_map,
            buildings_3d: Some(settings.buildings_3d),
            labels: Some(settings.labels),
        }
        .serialize_json()
        .into_bytes()
    }

    /// Bytes that are not a saved state leave the defaults.
    pub fn load_state(&mut self, bytes: &[u8]) {
        let Some(saved) = std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| SavedState::deserialize_json_lenient(text).ok())
        else {
            return;
        };
        let settings = &mut self.settings;
        settings.units = saved.imperial.map(|imperial| {
            if imperial {
                Units::Imperial
            } else {
                Units::Metric
            }
        });
        settings.dark_map = saved.dark_map;
        settings.buildings_3d = saved.buildings_3d.unwrap_or(settings.buildings_3d);
        settings.labels = saved.labels.unwrap_or(settings.labels);
        // A camera only when all of it is on the map.
        if let (Some(lon), Some(lat), Some(zoom)) = (saved.center_lon, saved.center_lat, saved.zoom)
        {
            if (-180.0..=180.0).contains(&lon)
                && (-85.0..=85.0).contains(&lat)
                && (1.0..=22.0).contains(&zoom)
            {
                settings.center = LonLat::new(lon, lat);
                settings.zoom = zoom;
            }
        }
    }
}

/// `Settings` on disk. Every field optional: an older or a newer file still
/// reads, and a choice never made stays unmade.
// `pub`, not private: the micro_serde derives parse only a plain `pub`.
#[derive(Clone, Debug, Default, SerJson, DeJson)]
pub struct SavedState {
    pub center_lon: Option<f64>,
    pub center_lat: Option<f64>,
    pub zoom: Option<f64>,
    pub imperial: Option<bool>,
    pub dark_map: Option<bool>,
    pub buildings_3d: Option<bool>,
    pub labels: Option<bool>,
}

/// A routing reply as the tab's state, failures in words for the person.
fn route_state(mode: Mode, body: Result<String, String>) -> RouteState {
    match body.map(|text| routing::parse(mode, &text)) {
        Ok(Ok(directions)) => RouteState::Ready(directions),
        Ok(Err(RouteError::NoRoute)) => RouteState::Failed("No route found".into()),
        Ok(Err(RouteError::Malformed(_))) => {
            RouteState::Failed("Couldn't read the directions".into())
        }
        Err(why) => RouteState::Failed(format!("Couldn't get directions: {why}")),
    }
}

/// How long the locate button waits for a first fix before giving up.
pub const LOCATION_FIX_TIMEOUT_SECONDS: f64 = 20.0;

/// The locate button's state, after the framework's route app: nothing
/// asked for, waiting for a first fix, or fixes flowing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocationState {
    #[default]
    Idle,
    Waiting,
    Active,
}

/// What a tap on the locate button (or Directions wanting a fix) means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationAsk {
    /// Start the platform's updates and the timeout.
    Start,
    /// Fixes are flowing: fly to the last one.
    Recenter,
    /// Already waiting.
    Ignore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationFix {
    First,
    Update,
    /// A fix nobody asked for: updates were stopped, one was still on its way.
    Ignore,
}

impl LocationState {
    pub fn asked(&mut self) -> LocationAsk {
        match self {
            LocationState::Idle => {
                *self = LocationState::Waiting;
                LocationAsk::Start
            }
            LocationState::Waiting => LocationAsk::Ignore,
            LocationState::Active => LocationAsk::Recenter,
        }
    }

    pub fn received_fix(&mut self) -> LocationFix {
        match self {
            LocationState::Idle => LocationFix::Ignore,
            LocationState::Waiting => {
                *self = LocationState::Active;
                LocationFix::First
            }
            LocationState::Active => LocationFix::Update,
        }
    }

    /// An error or the timeout: back to idle. `false` when nothing was
    /// wanted, so a stray error says nothing to the person.
    pub fn failed(&mut self) -> bool {
        std::mem::take(self) != LocationState::Idle
    }
}

/// The text of a fetched body, or why it is not one: the trust boundary
/// between the network and the parsers. Only a 200 whose body is present,
/// under the cap and UTF-8 gets through.
pub fn body_text(status_code: u16, body: Option<&[u8]>) -> Result<&str, String> {
    if status_code != 200 {
        return Err(format!("HTTP {status_code}"));
    }
    let body = body.ok_or_else(|| "empty response".to_string())?;
    if body.len() > MAX_BODY_BYTES {
        return Err(format!("response too large ({} bytes)", body.len()));
    }
    std::str::from_utf8(body).map_err(|_| "response is not UTF-8".to_string())
}

/// A transport error in plain words: the Android backend reports a failed
/// request as a Java exception chain.
pub fn plain_error(message: &str) -> String {
    let lower = message.to_lowercase();
    let known = [
        (
            &[
                "unknownhost",
                "unable to resolve host",
                "no address associated",
                "dns",
            ][..],
            "no internet connection",
        ),
        (
            &["sslhandshake", "certificate", "ssl", "tls"][..],
            "secure connection failed",
        ),
        (&["timed out", "timeout"][..], "timed out"),
        (
            &[
                "connectexception",
                "failed to connect",
                "connection refused",
                "unreachable",
                "econnreset",
                "connection reset",
            ][..],
            "could not connect",
        ),
    ];
    for (needles, words) in known {
        if needles.iter().any(|needle| lower.contains(needle)) {
            return words.to_string();
        }
    }
    // The last exception in a chain carries the message; drop the wrappers.
    let last = message
        .rsplit(": ")
        .find(|part| !part.trim().is_empty())
        .unwrap_or(message)
        .trim();
    if last.is_empty() {
        message.trim().to_string()
    } else {
        last.to_string()
    }
}

/// The app's colours: Google Maps' light look, or its dark counterpart when
/// the host is dark.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skin {
    pub light: bool,
    /// The search page, behind its rows.
    pub ground: Vec4f,
    /// The search bar, the sheet, the round buttons, the top card.
    pub card: Vec4f,
    pub ink: Vec4f,
    /// Addresses, captions, idle icons.
    pub secondary: Vec4f,
    pub hairline: Vec4f,
    /// A field inside a card; the sheet's handle.
    pub field: Vec4f,
    /// Directions, the selected tab, the locate button when following.
    pub accent: Vec4f,
    /// Text and icons on the accent.
    pub on_accent: Vec4f,
    /// The navigation banner.
    pub banner: Vec4f,
    /// The place pin, End, errors.
    pub alert: Vec4f,
}

impl Skin {
    pub fn for_mode(light: bool) -> Self {
        let c = Vec4f::from_u32;
        if light {
            Skin {
                light,
                ground: c(0xffffffff),
                card: c(0xffffffff),
                ink: c(0x202124ff),
                secondary: c(0x5f6368ff),
                hairline: c(0xdadce0ff),
                field: c(0xf1f3f4ff),
                accent: c(0x1a73e8ff),
                on_accent: c(0xffffffff),
                banner: c(0x0b8043ff),
                alert: c(0xd93025ff),
            }
        } else {
            Skin {
                light,
                ground: c(0x202124ff),
                card: c(0x303134ff),
                ink: c(0xe8eaedff),
                secondary: c(0x9aa0a6ff),
                hairline: c(0x3c4043ff),
                field: c(0x3c4043ff),
                accent: c(0x8ab4f8ff),
                on_accent: c(0x202124ff),
                banner: c(0x0d652dff),
                alert: c(0xf28b82ff),
            }
        }
    }

    /// The skin for this isolate: a forced one (`force_skin`, the
    /// standalone window's `--dark` and `--light`), else the host palette's
    /// light or dark mode, else light. The host re-runs the crate's
    /// `script_mod` on a style change, so the answer is re-read then.
    pub fn for_vm(vm: &mut ScriptVm) -> Self {
        let light = forced_light()
            .or_else(|| makepad_wm_theme::current_for_vm(vm).map(|p| p.light_mode))
            .unwrap_or(true);
        LAST_SKIN.store(if light { 1 } else { 2 }, Ordering::Relaxed);
        Self::for_mode(light)
    }

    /// The skin the last `for_vm` picked: what the Rust side tints with
    /// between DSL applies. Light before any.
    pub fn current() -> Self {
        Self::for_mode(LAST_SKIN.load(Ordering::Relaxed) != 2)
    }
}

/// 0: not forced; 1: light; 2: dark.
static FORCED_SKIN: AtomicU8 = AtomicU8::new(0);
/// The last skin `Skin::for_vm` resolved, the same encoding.
static LAST_SKIN: AtomicU8 = AtomicU8::new(0);

/// Force the skin for every isolate in this process, before the crate's
/// `script_mod` runs; `None` follows the palette again.
pub fn force_skin(light: Option<bool>) {
    FORCED_SKIN.store(
        match light {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        },
        Ordering::Relaxed,
    );
}

fn forced_light() -> Option<bool> {
    match FORCED_SKIN.load(Ordering::Relaxed) {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::places::PlaceKind;

    const SEARCH_REPLY: &str = include_str!("../tests/fixtures/photon-search.json");
    const REVERSE_REPLY: &str = include_str!("../tests/fixtures/photon-reverse.json");
    const EMPTY_REPLY: &str = include_str!("../tests/fixtures/photon-empty.json");
    const CAR: &str = include_str!("../tests/fixtures/osrm-car.json");
    const BIKE: &str = include_str!("../tests/fixtures/osrm-bike.json");
    const FOOT: &str = include_str!("../tests/fixtures/osrm-foot.json");
    const NO_ROUTE: &str = include_str!("../tests/fixtures/osrm-noroute.json");

    const SAN_JOSE: LonLat = LonLat {
        lon: -121.8863,
        lat: 37.3382,
    };

    fn place(name: &str, country: &str) -> Place {
        Place {
            name: name.into(),
            pos: LonLat::new(-121.9552, 37.3541),
            country_code: country.into(),
            ..Place::default()
        }
    }

    /// A model on the Directions screen, from a fix to a place.
    fn at_directions() -> MapsModel {
        let mut model = MapsModel::default();
        model.set_fix(SAN_JOSE);
        model.select_place(place("Santa Clara University", "US"));
        model.open_directions();
        model
    }

    /// Land `body` for the next route request, which must be for `mode`.
    fn land_route(model: &mut MapsModel, mode: Mode, body: &str) {
        let (id, asked, _) = model.next_route_request().expect("a route to ask for");
        assert_eq!(asked, mode);
        assert_eq!(
            model.complete(id, Ok(body.to_string())),
            Some(Request::Route(mode))
        );
    }

    #[test]
    fn a_search_fills_the_list_and_a_pick_opens_the_place() {
        let mut model = MapsModel::default();
        model.open_search(SearchTarget::Destination);
        assert_eq!(
            model.screen(),
            Screen::Search {
                target: SearchTarget::Destination
            }
        );
        // Nothing typed: nothing to ask.
        assert!(model.begin_search(SAN_JOSE).is_none());
        model.set_query("  ");
        assert!(model.begin_search(SAN_JOSE).is_none());
        model.set_query("santa clara university");
        let (id, url) = model.begin_search(SAN_JOSE).unwrap();
        assert!(
            url.contains("q=santa%20clara%20university") && url.contains("lat=37.3382"),
            "{url}"
        );
        assert_eq!(model.search_state, SearchState::Loading);
        assert_eq!(
            model.complete(id, Ok(SEARCH_REPLY.into())),
            Some(Request::Search)
        );
        assert_eq!(model.search_state, SearchState::Done);
        assert_eq!(model.results.len(), 3);
        model.pick_result(0);
        assert_eq!(model.screen(), Screen::Place);
        assert_eq!(model.place().unwrap().name, "Santa Clara University");
        // A row that is not there is not a pick.
        model.open_search(SearchTarget::Destination);
        model.pick_result(99);
        assert_eq!(
            model.screen(),
            Screen::Search {
                target: SearchTarget::Destination
            }
        );
    }

    #[test]
    fn a_newer_search_supersedes_the_older_and_its_reply_is_dropped() {
        let mut model = MapsModel::default();
        model.open_search(SearchTarget::Destination);
        model.set_query("santa");
        let (first, _) = model.begin_search(SAN_JOSE).unwrap();
        model.set_query("santa clara");
        let (second, _) = model.begin_search(SAN_JOSE).unwrap();
        assert_ne!(first, second);
        assert_eq!(model.superseded(), vec![first]);
        assert!(model.superseded().is_empty(), "handed out once");
        assert_eq!(model.owns(first), None);
        assert_eq!(model.complete(first, Ok(SEARCH_REPLY.into())), None);
        assert!(model.results.is_empty());
        assert_eq!(
            model.complete(second, Ok(EMPTY_REPLY.into())),
            Some(Request::Search)
        );
        assert_eq!(model.search_state, SearchState::Done);
        assert!(model.results.is_empty());
        // Clearing the field forgets the list and whatever was in flight.
        model.set_query("santa clara u");
        let (third, _) = model.begin_search(SAN_JOSE).unwrap();
        model.set_query("");
        assert_eq!(model.superseded(), vec![third]);
        assert_eq!(model.search_state, SearchState::Idle);
    }

    #[test]
    fn a_failed_search_says_why_and_keeps_the_last_list() {
        let mut model = MapsModel::default();
        model.open_search(SearchTarget::Destination);
        model.set_query("santa clara");
        let (id, _) = model.begin_search(SAN_JOSE).unwrap();
        model.complete(id, Ok(SEARCH_REPLY.into()));
        let (id, _) = model.begin_search(SAN_JOSE).unwrap();
        model.complete(id, Err("no internet connection".into()));
        assert_eq!(
            model.search_state,
            SearchState::Failed("no internet connection".into())
        );
        assert_eq!(model.results.len(), 3);
        // A body that is not Photon's is a failure too.
        let (id, _) = model.begin_search(SAN_JOSE).unwrap();
        model.complete(id, Ok("<html>".into()));
        assert!(matches!(model.search_state, SearchState::Failed(_)));
    }

    #[test]
    fn a_dropped_pin_opens_at_once_and_the_lookup_names_it() {
        let mut model = MapsModel::default();
        let (id, url) = model.drop_pin(SAN_JOSE);
        assert!(url.contains("/reverse?lat=37.338200"), "{url}");
        assert_eq!(model.screen(), Screen::Place);
        assert_eq!(model.place().unwrap().name, "Dropped pin");
        assert_eq!(
            model.complete(id, Ok(REVERSE_REPLY.into())),
            Some(Request::Reverse)
        );
        let named = model.place().unwrap();
        assert_eq!(named.name, "East Santa Clara Street");
        assert_eq!(named.kind, PlaceKind::Street);
        assert_eq!(named.country_code, "US");
        // The pin stays where the finger was, not where the street's point is.
        assert_eq!(named.pos, SAN_JOSE);
        // A lookup that finds nothing leaves the pin a pin; one that lands
        // after the person moved on names nothing.
        let (id, _) = model.drop_pin(SAN_JOSE);
        model.complete(id, Ok(EMPTY_REPLY.into()));
        assert_eq!(model.place().unwrap().name, "Dropped pin");
        let (stale, _) = model.drop_pin(SAN_JOSE);
        model.select_place(place("Elsewhere", "US"));
        assert_eq!(model.superseded(), vec![stale]);
        assert_eq!(model.complete(stale, Ok(REVERSE_REPLY.into())), None);
        assert_eq!(model.place().unwrap().name, "Elsewhere");
    }

    #[test]
    fn routes_are_asked_for_one_at_a_time_the_selected_mode_first() {
        let mut model = at_directions();
        assert_eq!(model.screen(), Screen::Directions);
        assert_eq!(model.origin(), &End::MyLocation);
        assert_eq!(model.destination().label(), "Santa Clara University");
        model.set_mode(Mode::Walk);
        let (id, mode, url) = model.next_route_request().unwrap();
        assert_eq!(mode, Mode::Walk);
        assert!(
            url.contains("/routed-foot/")
                && url.contains("-121.886300,37.338200;-121.955200,37.354100"),
            "{url}"
        );
        assert!(matches!(model.route(Mode::Walk), RouteState::Loading));
        // One in flight: nothing more to ask.
        assert!(model.next_route_request().is_none());
        assert_eq!(
            model.complete(id, Ok(FOOT.into())),
            Some(Request::Route(Mode::Walk))
        );
        assert!(model.directions().is_some());
        // Then the other tabs, in their order.
        land_route(&mut model, Mode::Car, CAR);
        land_route(&mut model, Mode::Bike, BIKE);
        assert!(model.next_route_request().is_none());
        for mode in Mode::ALL {
            assert!(model.route(mode).directions().is_some(), "{mode:?}");
        }
    }

    #[test]
    fn no_route_fails_its_own_tab_only_and_can_be_retried() {
        let mut model = at_directions();
        land_route(&mut model, Mode::Car, NO_ROUTE);
        assert!(
            matches!(model.route(Mode::Car), RouteState::Failed(why) if why == "No route found")
        );
        assert!(model.directions().is_none());
        land_route(&mut model, Mode::Walk, FOOT);
        let (id, mode, _) = model.next_route_request().unwrap();
        assert_eq!(mode, Mode::Bike);
        model.complete(id, Err("timed out".into()));
        assert!(
            matches!(model.route(Mode::Bike), RouteState::Failed(why) if why.contains("timed out"))
        );
        assert!(model.route(Mode::Walk).directions().is_some());
        assert!(model.next_route_request().is_none());
        model.retry_route(Mode::Bike);
        land_route(&mut model, Mode::Bike, BIKE);
        assert!(model.route(Mode::Bike).directions().is_some());
    }

    #[test]
    fn no_fix_no_route_until_there_is_one_or_a_start_is_chosen() {
        let mut model = MapsModel::default();
        model.select_place(place("Santa Clara University", "US"));
        model.open_directions();
        assert!(model.needs_fix());
        assert!(model.next_route_request().is_none());
        // The person picks a starting point instead.
        model.open_search(SearchTarget::Origin);
        model.set_query("santa clara");
        let (id, _) = model.begin_search(SAN_JOSE).unwrap();
        model.complete(id, Ok(SEARCH_REPLY.into()));
        model.pick_result(1);
        assert_eq!(model.screen(), Screen::Directions);
        assert_eq!(model.origin().label(), "Santa Clara University Library");
        assert_eq!(model.destination().label(), "Santa Clara University");
        assert!(!model.needs_fix());
        assert!(model.next_route_request().is_some());
        // Or a fix arrives.
        let mut model = MapsModel::default();
        model.select_place(place("Santa Clara University", "US"));
        model.open_directions();
        model.set_fix(SAN_JOSE);
        assert!(!model.needs_fix());
        assert!(model.next_route_request().is_some());
    }

    #[test]
    fn changing_an_end_forgets_the_routes() {
        let mut model = at_directions();
        land_route(&mut model, Mode::Car, CAR);
        let (in_flight, _, _) = model.next_route_request().unwrap();
        model.swap_ends();
        assert_eq!(model.origin().label(), "Santa Clara University");
        assert_eq!(model.destination(), &End::MyLocation);
        assert!(matches!(model.route(Mode::Car), RouteState::Idle));
        // The route that was being fetched was for the old ends.
        assert_eq!(model.superseded(), vec![in_flight]);
        assert_eq!(model.complete(in_flight, Ok(FOOT.into())), None);
        let (_, mode, url) = model.next_route_request().unwrap();
        assert_eq!(mode, Mode::Car);
        assert!(
            url.contains("-121.955200,37.354100;-121.886300,37.338200"),
            "{url}"
        );
        // A new destination from Directions' own search does the same.
        let mut model = at_directions();
        land_route(&mut model, Mode::Car, CAR);
        model.open_search(SearchTarget::Destination);
        model.set_query("library");
        let (id, _) = model.begin_search(SAN_JOSE).unwrap();
        model.complete(id, Ok(SEARCH_REPLY.into()));
        model.pick_result(1);
        assert_eq!(model.screen(), Screen::Directions);
        assert_eq!(
            model.destination().label(),
            "Santa Clara University Library"
        );
        assert!(matches!(model.route(Mode::Car), RouteState::Idle));
    }

    #[test]
    fn back_walks_every_screen_down_to_explore_and_then_declines() {
        let mut model = at_directions();
        land_route(&mut model, Mode::Car, CAR);
        assert!(model.start_navigation(true).is_some());
        assert_eq!(model.screen(), Screen::Navigating);
        assert!(model.back());
        assert_eq!(model.screen(), Screen::Directions);
        // Search from Directions goes back to Directions.
        model.open_search(SearchTarget::Origin);
        assert!(model.back());
        assert_eq!(model.screen(), Screen::Directions);
        assert!(model.back());
        assert_eq!(model.screen(), Screen::Place);
        // Search from Place goes back to Place.
        model.open_search(SearchTarget::Destination);
        assert!(model.back());
        assert_eq!(model.screen(), Screen::Place);
        assert!(model.back());
        assert_eq!(model.screen(), Screen::Explore);
        assert!(model.place().is_none());
        model.open_search(SearchTarget::Destination);
        assert!(model.back());
        assert_eq!(model.screen(), Screen::Explore);
        assert!(!model.back(), "the host's turn");
    }

    #[test]
    fn navigation_needs_a_route_and_arrival_ends_at_the_place() {
        let mut model = at_directions();
        assert!(model.start_navigation(false).is_none(), "no route yet");
        assert_eq!(model.screen(), Screen::Directions);
        land_route(&mut model, Mode::Car, CAR);
        let nav = model.start_navigation(false).unwrap();
        assert!(!nav.simulate());
        model.arrive();
        assert_eq!(model.screen(), Screen::Arrived);
        assert!(model.back());
        assert_eq!(model.screen(), Screen::Place);
        assert_eq!(model.place().unwrap().name, "Santa Clara University");
        // Arriving off the Navigating screen is nothing.
        model.arrive();
        assert_eq!(model.screen(), Screen::Place);
    }

    #[test]
    fn a_reroute_is_asked_for_once_and_its_answer_waits_for_guidance() {
        let mut model = at_directions();
        assert!(model.begin_reroute(SAN_JOSE).is_none(), "not navigating");
        land_route(&mut model, Mode::Car, CAR);
        model.start_navigation(false).unwrap();
        let astray = LonLat::new(-121.9, 37.34);
        let (id, url) = model.begin_reroute(astray).unwrap();
        assert!(
            url.contains("/routed-car/")
                && url.contains("-121.900000,37.340000;-121.955200,37.354100"),
            "{url}"
        );
        assert!(model.begin_reroute(astray).is_none(), "one at a time");
        assert_eq!(model.complete(id, Ok(CAR.into())), Some(Request::Reroute));
        assert!(model.take_reroute().is_some());
        assert!(model.take_reroute().is_none());
        // A failed one leaves nothing to take, and the next may be asked.
        let (id, _) = model.begin_reroute(astray).unwrap();
        assert_eq!(
            model.complete(id, Err("timed out".into())),
            Some(Request::Reroute)
        );
        assert!(model.take_reroute().is_none());
        assert!(model.begin_reroute(astray).is_some());
        // Ending the drive forgets routes that no longer start where it did.
        model.back();
        assert_eq!(model.screen(), Screen::Directions);
        assert!(matches!(model.route(Mode::Car), RouteState::Idle));
    }

    #[test]
    fn units_follow_the_destination_until_the_person_chooses() {
        let mut model = MapsModel::default();
        assert_eq!(model.units(), Units::Metric);
        model.select_place(place("Santa Clara University", "US"));
        assert_eq!(model.units(), Units::Imperial);
        model.select_place(place("Rijksmuseum", "NL"));
        assert_eq!(model.units(), Units::Metric);
        model.settings.units = Some(Units::Imperial);
        assert_eq!(model.units(), Units::Imperial);
    }

    #[test]
    fn the_settings_survive_a_restart_and_garbage_does_not_touch_them() {
        let mut model = MapsModel::default();
        model.settings = Settings {
            units: Some(Units::Imperial),
            dark_map: Some(true),
            buildings_3d: false,
            labels: true,
            center: SAN_JOSE,
            zoom: 14.5,
        };
        let bytes = model.saved_state();
        let mut restored = MapsModel::default();
        restored.load_state(&bytes);
        assert_eq!(restored.settings, model.settings);
        // Choices never made stay unmade.
        let mut fresh = MapsModel::default();
        fresh.load_state(&MapsModel::default().saved_state());
        assert_eq!(fresh.settings, Settings::default());
        for garbage in [
            &b"not json"[..],
            &b""[..],
            &br#"{"zoom":"high","center_lat":1e400}"#[..],
            &[0xff, 0xfe][..],
        ] {
            let mut model = MapsModel::default();
            model.load_state(garbage);
            assert_eq!(model.settings, Settings::default());
        }
        // A camera off the map is not restored.
        let mut model = MapsModel::default();
        model.load_state(br#"{"center_lon":500.0,"center_lat":95.0,"zoom":99.0}"#);
        assert_eq!(model.settings, Settings::default());
    }

    #[test]
    fn the_locate_button_asks_once_then_recentres() {
        let mut location = LocationState::default();
        // A fix or an error nobody asked for is nothing.
        assert_eq!(location.received_fix(), LocationFix::Ignore);
        assert!(!location.failed());
        assert_eq!(location.asked(), LocationAsk::Start);
        assert_eq!(location.asked(), LocationAsk::Ignore, "already waiting");
        assert_eq!(location.received_fix(), LocationFix::First);
        assert_eq!(location.received_fix(), LocationFix::Update);
        assert_eq!(location.asked(), LocationAsk::Recenter);
        // Denied, unavailable or timed out: the next tap starts over.
        assert!(location.failed());
        assert_eq!(location, LocationState::Idle);
        assert_eq!(location.asked(), LocationAsk::Start);
        assert!(location.failed());
        assert!(!location.failed());
    }

    #[test]
    fn shutting_down_cancels_whatever_is_in_flight() {
        let mut model = at_directions();
        let (route, _, _) = model.next_route_request().unwrap();
        model.open_search(SearchTarget::Origin);
        model.set_query("library");
        let (search, _) = model.begin_search(SAN_JOSE).unwrap();
        let mut ids = model.cancel_all();
        ids.sort();
        let mut expected = vec![route, search];
        expected.sort();
        assert_eq!(ids, expected);
        assert_eq!(model.owns(route), None);
    }

    #[test]
    fn only_a_utf8_200_under_the_cap_gets_through() {
        assert_eq!(body_text(200, Some(b"{}")), Ok("{}"));
        assert_eq!(body_text(404, Some(b"{}")), Err("HTTP 404".to_string()));
        assert_eq!(body_text(200, None), Err("empty response".to_string()));
        assert!(body_text(200, Some(&[0xff, 0xfe])).is_err());
        assert!(body_text(200, Some(&vec![b' '; MAX_BODY_BYTES + 1])).is_err());
    }

    #[test]
    fn transport_errors_in_plain_words() {
        assert_eq!(
            plain_error("java.util.concurrent.CompletionException: java.net.UnknownHostException: Unable to resolve host"),
            "no internet connection"
        );
        assert_eq!(plain_error("request timed out"), "timed out");
        assert_eq!(
            plain_error("java.io.IOException: something odd"),
            "something odd"
        );
    }
}
