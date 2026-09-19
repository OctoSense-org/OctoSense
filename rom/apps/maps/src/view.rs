//! The maps surface, after Google Maps: one `MapView` that is never
//! rebuilt, full-bleed, and over it a layer per screen that the model shows
//! or hides — Explore's search pill and round buttons, Search's field and
//! results, Place's pill and sheet; Directions and Navigation join them.
//! Everything is drawn in the app's skin (`Skin`): Google Maps' light look,
//! or its dark counterpart when the host is dark. The requests, the location
//! updates, the timers and the map's camera are the view's.
//! `ensure_started` seeds everything on the first event or draw, whichever
//! the host gives it first: a window has a `Startup` event, a module
//! instance does not.

use crate::geo::{
    arrival_text, distance_text, duration_text, fit_camera, haversine_m, Bounds, Insets, LonLat,
    Units,
};
use crate::guidance::{ActiveNav, NavTick};
use crate::model::{
    body_text, plain_error, LocationAsk, LocationFix, LocationState, MapsModel, Request,
    RouteState, Screen, SearchState, SearchTarget, Skin, LOCATION_FIX_TIMEOUT_SECONDS,
    MAX_BODY_BYTES, USER_AGENT,
};
use crate::places::{self, coordinates_text, Place, PlaceKind};
use crate::routing::{Arrow, Mode};
use crate::sheet::{Detent, Sheet};
use crate::HOSTED_TILES;
use makepad_map_nav::nav::NavState;
use makepad_widgets::makepad_platform::event::TouchState;
use makepad_widgets::*;

/// How long a status line (`Locating…`, an error) stays up.
const STATUS_SECONDS: f64 = 4.0;
/// The zoom the locate button flies to: streets and their names.
const LOCATE_ZOOM: f64 = 15.5;
/// The closest a place is shown from: its block.
const PLACE_ZOOM: f64 = 16.0;
/// Rotation or tilt, in degrees, under which the map counts as north-up
/// and flat: the widget's own snap leaves a fraction of a degree.
const LEVEL_DEGREES: f64 = 1.0;
/// `MapView::set_theme`: its light and its night style.
const MAP_THEME_LIGHT: u32 = 0;
const MAP_THEME_NIGHT: u32 = 1;
/// Typing pauses this long before the search goes out: the service's terms
/// ask for no request per keystroke.
const SEARCH_DEBOUNCE_SECONDS: f64 = 0.3;
/// What the floating pill and its margin take of the top of the viewport.
const TOP_BAR_HEIGHT: f64 = 70.0;
/// How much of the rest of the way the sheet's height goes each frame.
const SHEET_EASE: f64 = 0.28;
/// The selected place's marker, and a route's two ends.
const PLACE_MARKER: u64 = 1;
const ORIGIN_MARKER: u64 = 2;
const DESTINATION_MARKER: u64 = 3;
/// What the directions card and its margin take of the top of the viewport.
const DIRECTIONS_CARD_HEIGHT: f64 = 176.0;
/// The room a fitted route keeps from the viewport's sides and the panels.
const ROUTE_MARGIN: f64 = 36.0;
/// The chase camera: close enough to read the next street, tilted to see
/// down it.
const NAV_ZOOM: f64 = 17.0;
const NAV_TILT: f64 = 55.0;
/// How far ahead of the puck the chase camera looks, so the puck sits below
/// the middle and the road ahead fills the screen.
const NAV_LEAD_M: f64 = 60.0;
/// The accuracy ring guidance draws: the puck is on the line, not a guess.
const NAV_PUCK_ACCURACY_M: f64 = 8.0;
/// A press on the map that moves further than this many pixels is a pan,
/// not a tap: the chase pauses.
const PAN_PIXELS: f64 = 8.0;
/// Between two live fixes when their clocks say nothing useful.
const DEFAULT_FIX_INTERVAL_S: f64 = 1.0;
/// A step shorter than this shows no distance: it would read `0 ft`.
const MIN_STEP_DISTANCE_M: f64 = 5.0;
/// The closest a short route is shown from.
const ROUTE_MAX_ZOOM: f64 = 17.0;
/// The sheet's detail rows: what fits at half height.
const DETAIL_ROWS: usize = 4;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // The skin's roles, resolved when this module is evaluated: the host
    // re-runs it on a light/dark change and re-applies the root.
    let c_ground = #(Skin::for_vm(vm).ground)
    let c_card = #(Skin::for_vm(vm).card)
    let c_ink = #(Skin::for_vm(vm).ink)
    let c_secondary = #(Skin::for_vm(vm).secondary)
    let c_hairline = #(Skin::for_vm(vm).hairline)
    let c_field = #(Skin::for_vm(vm).field)
    let c_accent = #(Skin::for_vm(vm).accent)
    let c_on_accent = #(Skin::for_vm(vm).on_accent)
    let c_alert = #(Skin::for_vm(vm).alert)
    let c_banner = #(Skin::for_vm(vm).banner)
    // On the banner's green, in either skin.
    let c_on_banner = #ffffff
    let c_clear = #00000000

    // Text without `Label`'s own padding, so rows are as tall as their text.
    let Text = Label{padding: 0 draw_text +: {color: c_ink text_style: theme.font_regular{font_size: 14}}}
    let Caption = Text{draw_text +: {color: c_secondary text_style: theme.font_regular{font_size: 12}}}
    let Hairline = SolidView{width: Fill height: 0.5 draw_bg.color: c_hairline}

    // A button with no face of its own: an icon, or a line of text.
    let Plain = ButtonFlat{padding: 0 margin: 0 text: "" spacing: 0 align: Center
        draw_bg +: {
            border_size: uniform(0.0)
            color: uniform(c_clear) color_hover: uniform(c_clear) color_down: uniform(#00000018) color_focus: uniform(c_clear)
            border_color: uniform(c_clear) border_color_hover: uniform(c_clear) border_color_down: uniform(c_clear) border_color_focus: uniform(c_clear)
        }
    }
    // An icon button inside a bar: back, clear.
    let BarButton = Plain{width: 40 height: 40 icon_walk: Walk{width: 16 height: 16} draw_icon +: {color: c_secondary}}
    // A round button floating over the map: a disc in the card colour with
    // a hairline rim, its icon in the ink.
    let Fab = Plain{width: 48 height: 48 icon_walk: Walk{width: 20 height: 20}
        draw_bg +: {
            border_size: uniform(1.0) border_radius: uniform(24.0)
            color: uniform(c_card) color_hover: uniform(c_card) color_down: uniform(c_field) color_focus: uniform(c_card)
            border_color: uniform(c_hairline) border_color_hover: uniform(c_hairline) border_color_down: uniform(c_hairline) border_color_focus: uniform(c_hairline)
        }
        draw_icon +: {color: c_ink}
    }
    // The sheet's primary action: a pill in the accent.
    let PrimaryButton = Plain{width: Fit height: 40 spacing: 8 padding: Inset{left: 18 right: 20}
        icon_walk: Walk{width: 16 height: 16}
        draw_bg +: {
            border_radius: uniform(20.0)
            color: uniform(c_accent) color_hover: uniform(c_accent) color_down: uniform(c_accent) color_focus: uniform(c_accent)
        }
        draw_icon +: {color: c_on_accent}
        draw_text +: {color: c_on_accent color_hover: c_on_accent color_down: c_on_accent color_focus: c_on_accent text_style: theme.font_bold{font_size: 14}}
    }
    // Beside the primary action: an outline in the accent.
    let SecondaryButton = Plain{width: Fit height: 40 padding: Inset{left: 20 right: 20}
        draw_bg +: {
            border_size: uniform(1.0) border_radius: uniform(20.0)
            border_color: uniform(c_hairline) border_color_hover: uniform(c_hairline) border_color_down: uniform(c_hairline) border_color_focus: uniform(c_hairline)
        }
        draw_text +: {color: c_accent color_hover: c_accent color_down: c_accent color_focus: c_accent text_style: theme.font_bold{font_size: 14}}
    }
    // Ending the drive.
    let DangerButton = PrimaryButton{
        draw_bg +: {color: uniform(c_alert) color_hover: uniform(c_alert) color_down: uniform(c_alert) color_focus: uniform(c_alert)}
    }
    // A line of text in the accent: Retry.
    let TextButton = Plain{width: Fit height: 36 padding: Inset{left: 10 right: 10}
        draw_text +: {color: c_accent color_hover: c_accent color_down: c_accent color_focus: c_accent text_style: theme.font_bold{font_size: 14}}}
    // Anything that floats over the map on a card: the pills, the status
    // line, the sheets.
    let Floating = RoundedShadowView{width: Fill height: Fit
        draw_bg +: {color: c_card border_radius: uniform(24.0) shadow_color: #00000040 shadow_radius: uniform(8.0) shadow_offset: uniform(vec2(0.0, 2.0))}
    }
    // The search field inside its pill: no face of its own.
    let Field = TextInputFlat{width: Fill height: 40 margin: 0 padding: Inset{left: 4 right: 4 top: 10 bottom: 10} empty_text: "Search here"
        draw_bg +: {
            border_radius: 8.0 border_size: 0.0
            color: c_clear color_hover: c_clear color_focus: c_clear color_empty: c_clear
            border_color: c_clear border_color_hover: c_clear border_color_focus: c_clear border_color_empty: c_clear
        }
        draw_text +: {
            color: c_ink color_hover: c_ink color_focus: c_ink
            color_empty: c_secondary color_empty_hover: c_secondary color_empty_focus: c_secondary
            text_style: theme.font_regular{font_size: 16}
        }
    }
    // A switch of the layers sheet: a pill in the accent when on.
    let Switch = Toggle{text: "" margin: 0 padding: 0 width: 42 height: 26
        draw_bg +: {
            size: uniform(26.0)
            border_size: uniform(0.0)
            color: uniform(c_field) color_hover: uniform(c_field) color_down: uniform(c_field) color_focus: uniform(c_field)
            color_active: uniform(c_accent) color_disabled: uniform(c_field)
            border_color: uniform(c_clear) border_color_hover: uniform(c_clear) border_color_active: uniform(c_clear) border_color_focus: uniform(c_clear)
            mark_color: uniform(#ffffff) mark_color_hover: uniform(#ffffff) mark_color_down: uniform(#ffffff)
            mark_color_active: uniform(#ffffff) mark_color_active_hover: uniform(#ffffff)
        }
    }
    // A row of the layers sheet: its label fills the width, so the switch
    // after it sits at the right.
    let SwitchRow = View{width: Fill height: 52 flow: Right align: Align{y: 0.5} padding: Inset{left: 20 right: 16}}
    let SwitchLabel = Text{width: Fill draw_text.text_style: theme.font_regular{font_size: 16}}

    // A result's mark: one icon per kind of place, the row showing its own
    // (`show_kind`).
    let KindMark = View{visible: false width: Fit height: Fit}
    let KindIcon = Icon{icon_walk: Walk{width: 18 height: 18} draw_icon +: {color: c_secondary}}
    // One search result: its mark on a disc, its name over its address, and
    // how far it is once there is a fix to measure from.
    let ResultRow = View{width: Fill height: Fit flow: Down
        row := View{width: Fill height: Fit flow: Right spacing: 14 align: Align{y: 0.5} padding: Inset{left: 16 right: 16 top: 11 bottom: 11} cursor: MouseCursor.Hand
            RoundedView{width: 38 height: 38 align: Center draw_bg +: {color: c_field border_radius: uniform(19.0)}
                place := KindMark{KindIcon{draw_icon.svg: crate_resource("self:resources/icons/place.svg")}}
                food := KindMark{KindIcon{draw_icon.svg: crate_resource("self:resources/icons/food.svg")}}
                shop := KindMark{KindIcon{draw_icon.svg: crate_resource("self:resources/icons/shop.svg")}}
                transit := KindMark{KindIcon{draw_icon.svg: crate_resource("self:resources/icons/transit.svg")}}
                lodging := KindMark{KindIcon{draw_icon.svg: crate_resource("self:resources/icons/lodging.svg")}}
                street := KindMark{KindIcon{draw_icon.svg: crate_resource("self:resources/icons/street.svg")}}
            }
            View{width: Fill height: Fit flow: Down spacing: 3
                name := Text{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_regular{font_size: 16}}
                address := Caption{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_regular{font_size: 13}}
            }
            distance := Caption{draw_text.text_style: theme.font_regular{font_size: 13}}
        }
        View{width: Fill height: Fit padding: Inset{left: 68} Hairline{}}
    }
    // What the list says when it has no rows: a hint, `Searching…`, `No
    // results`, or what went wrong and a way to try again.
    let StatusRow = View{width: Fill height: Fit flow: Down spacing: 6 align: Align{x: 0.5} padding: Inset{left: 32 right: 32 top: 48 bottom: 24}
        text := Caption{max_lines: 3 align: Align{x: 0.5} draw_text.text_style: theme.font_regular{font_size: 15}}
        retry := TextButton{text: "Retry"}
    }
    // One fact on the sheet: what it is, over what it says.
    let DetailRow = View{visible: false width: Fill height: Fit flow: Down spacing: 2 padding: Inset{left: 20 right: 20 top: 10 bottom: 10}
        label := Caption{}
        value := Text{width: Fill max_lines: 2 text_overflow: Ellipsis draw_text.text_style: theme.font_regular{font_size: 15}}
    }

    // One end of the route on the directions card: a dot in its colour and
    // where it is, on a field of its own that opens Search for it.
    let EndRow = RoundedView{width: Fill height: 40 flow: Right spacing: 10 align: Align{y: 0.5} padding: Inset{left: 12 right: 12} cursor: MouseCursor.Hand
        draw_bg +: {color: c_field border_radius: uniform(10.0)}
    }
    let EndDot = RoundedView{width: 10 height: 10 draw_bg +: {color: c_accent border_radius: uniform(5.0)}}
    let EndText = Text{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_regular{font_size: 15}}
    // A travel mode's tab: its icon and how long that way takes. Each mode
    // has the two faces, and `render` shows the one that fits.
    let ModeTab = Plain{width: Fill height: 36 spacing: 6 icon_walk: Walk{width: 18 height: 18}
        draw_bg +: {border_radius: uniform(18.0)}
        draw_icon +: {color: c_secondary}
        draw_text +: {color: c_secondary color_hover: c_secondary color_down: c_secondary color_focus: c_secondary text_style: theme.font_bold{font_size: 13}}
    }
    let ModeTabOn = ModeTab{visible: false
        draw_bg +: {color: uniform(c_field) color_hover: uniform(c_field) color_down: uniform(c_field) color_focus: uniform(c_field)}
        draw_icon +: {color: c_accent}
        draw_text +: {color: c_accent color_hover: c_accent color_down: c_accent color_focus: c_accent}
    }
    let ModeSlot = View{width: Fill height: Fit flow: Overlay}
    // A step's arrow: one icon per way to go, the row showing its own
    // (`show_arrow`).
    let ArrowMark = View{visible: false width: Fit height: Fit}
    let ArrowIcon = Icon{icon_walk: Walk{width: 22 height: 22} draw_icon +: {color: c_ink}}
    // One line of the directions: its arrow, what to do, and for how far.
    let StepRow = View{width: Fill height: Fit flow: Down
        View{width: Fill height: Fit flow: Right spacing: 14 align: Align{y: 0.5} padding: Inset{left: 20 right: 20 top: 12 bottom: 12}
            View{width: 26 height: 26 align: Center
                depart := ArrowMark{ArrowIcon{draw_icon.svg: crate_resource("self:resources/icons/depart.svg")}}
                arrive := ArrowMark{ArrowIcon{draw_icon.svg: crate_resource("self:resources/icons/arrive.svg")}}
                straight := ArrowMark{ArrowIcon{draw_icon.svg: crate_resource("self:resources/icons/straight.svg")}}
                slight_left := ArrowMark{ArrowIcon{draw_icon.svg: crate_resource("self:resources/icons/slight-left.svg")}}
                left := ArrowMark{ArrowIcon{draw_icon.svg: crate_resource("self:resources/icons/turn-left.svg")}}
                sharp_left := ArrowMark{ArrowIcon{draw_icon.svg: crate_resource("self:resources/icons/sharp-left.svg")}}
                slight_right := ArrowMark{ArrowIcon{draw_icon.svg: crate_resource("self:resources/icons/slight-right.svg")}}
                right := ArrowMark{ArrowIcon{draw_icon.svg: crate_resource("self:resources/icons/turn-right.svg")}}
                sharp_right := ArrowMark{ArrowIcon{draw_icon.svg: crate_resource("self:resources/icons/sharp-right.svg")}}
                uturn := ArrowMark{ArrowIcon{draw_icon.svg: crate_resource("self:resources/icons/uturn.svg")}}
                roundabout := ArrowMark{ArrowIcon{draw_icon.svg: crate_resource("self:resources/icons/roundabout.svg")}}
            }
            View{width: Fill height: Fit flow: Down spacing: 3
                text := Text{width: Fill max_lines: 2 text_overflow: Ellipsis draw_text.text_style: theme.font_regular{font_size: 15}}
                distance := Caption{draw_text.text_style: theme.font_regular{font_size: 13}}
            }
        }
        View{width: Fill height: Fit padding: Inset{left: 60} Hairline{}}
    }

    // The banner's arrow: the step's, larger and on the green.
    let BannerArrow = Icon{icon_walk: Walk{width: 36 height: 36} draw_icon +: {color: c_on_banner}}

    mod.widgets.MapsViewBase = #(MapsView::register_widget(vm))
    mod.widgets.MapsView = set_type_default() do mod.widgets.MapsViewBase{
        width: Fill height: Fill flow: Overlay

        // The one map, under everything, for as long as the app lives.
        // `debug_cam`: the camera readout is a workbench tool.
        map := MapView{width: Fill height: Fill min_zoom: 3.0 max_zoom: 20.0 buildings_3d: true debug_cam: false}

        // Explore: no ground of its own, so the map beneath keeps its touches.
        explore := View{width: Fill height: Fill flow: Down padding: Inset{left: 12 right: 12 top: 10 bottom: 14}
            pill := Floating{height: 48 flow: Right spacing: 12 align: Align{y: 0.5} padding: Inset{left: 16 right: 16} cursor: MouseCursor.Hand
                Icon{icon_walk: Walk{width: 18 height: 18} draw_icon +: {svg: crate_resource("self:resources/icons/search.svg") color: c_secondary}}
                Text{width: Fill max_lines: 1 text: "Search here" draw_text +: {color: c_secondary text_style: theme.font_regular{font_size: 16}}}
            }
            View{width: Fill height: Fit flow: Right padding: Inset{top: 12}
                View{width: Fill height: Fit}
                layers := Fab{width: 44 height: 44 draw_bg +: {border_radius: uniform(22.0)} draw_icon.svg: crate_resource("self:resources/icons/layers.svg")}
            }
            View{width: Fill height: Fill}
            View{width: Fill height: Fit flow: Right align: Align{y: 1.0}
                // The licence's line, where the data's readers look for it,
                // on a backing so it reads over any map.
                View{width: Fill height: Fit
                    RoundedView{width: Fit height: Fit padding: Inset{left: 6 right: 6 top: 3 bottom: 3}
                        draw_bg +: {color: c_card border_radius: uniform(6.0)}
                        attribution := Caption{text: "© OpenStreetMap contributors" draw_text.text_style: theme.font_regular{font_size: 10}}
                    }
                }
                View{width: Fit height: Fit flow: Down spacing: 12
                    compass := Fab{visible: false draw_icon +: {svg: crate_resource("self:resources/icons/compass.svg") color: c_accent}}
                    locate := Fab{draw_icon.svg: crate_resource("self:resources/icons/locate.svg")}
                    locate_active := Fab{visible: false draw_icon +: {svg: crate_resource("self:resources/icons/locate-active.svg") color: c_accent}}
                }
            }
        }

        // Place: the pill names the place, the sheet tells about it. No
        // ground of its own either: the map is still the map.
        place_layer := View{visible: false width: Fill height: Fill flow: Down
            View{width: Fill height: Fit padding: Inset{left: 12 right: 12 top: 10}
                Floating{height: 48 flow: Right align: Align{y: 0.5} padding: Inset{left: 4 right: 4}
                    place_back := BarButton{draw_icon.svg: crate_resource("self:resources/icons/back.svg")}
                    place_pill := View{width: Fill height: Fill align: Align{y: 0.5} padding: Inset{left: 4 right: 4} cursor: MouseCursor.Hand
                        place_pill_text := Text{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_regular{font_size: 16}}
                    }
                    place_close := BarButton{draw_icon.svg: crate_resource("self:resources/icons/close.svg")}
                }
            }
            View{width: Fill height: Fill}
            // Its height is the sheet's (`Sheet`), set as it is dragged and
            // eased; what does not fit is clipped, so half shows more than
            // peek and full more than half.
            sheet := Floating{height: 132 flow: Down
                draw_bg +: {border_radius: uniform(20.0)}
                // The part a finger takes hold of: the handle and the title.
                sheet_head := View{width: Fill height: Fit flow: Down cursor: MouseCursor.Hand
                    View{width: Fill height: 18 align: Center
                        RoundedView{width: 36 height: 4 draw_bg +: {color: c_hairline border_radius: uniform(2.0)}}
                    }
                    View{width: Fill height: Fit flow: Down spacing: 4 padding: Inset{left: 20 right: 20 bottom: 12}
                        sheet_title := Text{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_bold{font_size: 20}}
                        sheet_subtitle := Caption{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_regular{font_size: 14}}
                    }
                }
                View{width: Fill height: Fit padding: Inset{left: 20 right: 20 bottom: 14}
                    directions := PrimaryButton{text: "Directions" draw_icon.svg: crate_resource("self:resources/icons/directions.svg")}
                }
                Hairline{}
                address_row := DetailRow{label := Caption{text: "Address"}}
                coordinates_row := DetailRow{visible: true label := Caption{text: "Coordinates"}}
                detail_0 := DetailRow{}
                detail_1 := DetailRow{}
                detail_2 := DetailRow{}
                detail_3 := DetailRow{}
            }
        }

        // Directions: the two ends and the modes on a card, the route's
        // summary and its steps on a sheet, the map between them.
        directions_layer := View{visible: false width: Fill height: Fill flow: Down
            View{width: Fill height: Fit padding: Inset{left: 10 right: 10 top: 10}
                Floating{flow: Down spacing: 8 padding: Inset{left: 4 right: 4 top: 8 bottom: 8}
                    draw_bg +: {border_radius: uniform(16.0)}
                    View{width: Fill height: Fit flow: Right align: Align{y: 0.5}
                        directions_back := BarButton{draw_icon.svg: crate_resource("self:resources/icons/back.svg")}
                        View{width: Fill height: Fit flow: Down spacing: 6
                            origin_row := EndRow{EndDot{} origin_text := EndText{}}
                            destination_row := EndRow{EndDot{draw_bg +: {color: c_alert}} destination_text := EndText{}}
                        }
                        swap := BarButton{draw_icon.svg: crate_resource("self:resources/icons/swap.svg")}
                    }
                    View{width: Fill height: Fit flow: Right spacing: 6 padding: Inset{left: 8 right: 8}
                        ModeSlot{
                            mode_car := ModeTab{draw_icon.svg: crate_resource("self:resources/icons/car.svg")}
                            mode_car_on := ModeTabOn{draw_icon.svg: crate_resource("self:resources/icons/car.svg")}
                        }
                        ModeSlot{
                            mode_walk := ModeTab{draw_icon.svg: crate_resource("self:resources/icons/walk.svg")}
                            mode_walk_on := ModeTabOn{draw_icon.svg: crate_resource("self:resources/icons/walk.svg")}
                        }
                        ModeSlot{
                            mode_bike := ModeTab{draw_icon.svg: crate_resource("self:resources/icons/bike.svg")}
                            mode_bike_on := ModeTabOn{draw_icon.svg: crate_resource("self:resources/icons/bike.svg")}
                        }
                    }
                }
            }
            View{width: Fill height: Fill}
            // The same sheet as a place's, about the route.
            route_sheet := Floating{height: 132 flow: Down
                draw_bg +: {border_radius: uniform(20.0)}
                route_head := View{width: Fill height: Fit flow: Down cursor: MouseCursor.Hand
                    View{width: Fill height: 18 align: Center
                        RoundedView{width: 36 height: 4 draw_bg +: {color: c_hairline border_radius: uniform(2.0)}}
                    }
                    View{width: Fill height: Fit flow: Down spacing: 4 padding: Inset{left: 20 right: 20 bottom: 12}
                        route_title := Text{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_bold{font_size: 20}}
                        route_subtitle := Caption{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_regular{font_size: 14}}
                    }
                }
                route_actions := View{width: Fill height: Fit flow: Right spacing: 10 padding: Inset{left: 20 right: 20 bottom: 14}
                    start := PrimaryButton{visible: false text: "Start" draw_icon.svg: crate_resource("self:resources/icons/recenter.svg")}
                    preview := SecondaryButton{visible: false text: "Preview"}
                    route_retry := TextButton{visible: false text: "Retry"}
                }
                Hairline{}
                steps := PortalList{width: Fill height: Fill
                    Step := StepRow{}
                }
            }
        }

        // Navigation: the next turn on a banner, what is left on a bar, the
        // map chasing the puck between them.
        nav_layer := View{visible: false width: Fill height: Fill flow: Down
            View{width: Fill height: Fit padding: Inset{left: 10 right: 10 top: 10}
                banner := RoundedShadowView{width: Fill height: Fit flow: Right spacing: 14 align: Align{y: 0.5} padding: Inset{left: 18 right: 18 top: 14 bottom: 14}
                    draw_bg +: {color: c_banner border_radius: uniform(16.0) shadow_color: #00000040 shadow_radius: uniform(8.0) shadow_offset: uniform(vec2(0.0, 2.0))}
                    View{width: 44 height: 44 align: Center
                        depart := ArrowMark{BannerArrow{draw_icon.svg: crate_resource("self:resources/icons/depart.svg")}}
                        arrive := ArrowMark{BannerArrow{draw_icon.svg: crate_resource("self:resources/icons/arrive.svg")}}
                        straight := ArrowMark{BannerArrow{draw_icon.svg: crate_resource("self:resources/icons/straight.svg")}}
                        slight_left := ArrowMark{BannerArrow{draw_icon.svg: crate_resource("self:resources/icons/slight-left.svg")}}
                        left := ArrowMark{BannerArrow{draw_icon.svg: crate_resource("self:resources/icons/turn-left.svg")}}
                        sharp_left := ArrowMark{BannerArrow{draw_icon.svg: crate_resource("self:resources/icons/sharp-left.svg")}}
                        slight_right := ArrowMark{BannerArrow{draw_icon.svg: crate_resource("self:resources/icons/slight-right.svg")}}
                        right := ArrowMark{BannerArrow{draw_icon.svg: crate_resource("self:resources/icons/turn-right.svg")}}
                        sharp_right := ArrowMark{BannerArrow{draw_icon.svg: crate_resource("self:resources/icons/sharp-right.svg")}}
                        uturn := ArrowMark{BannerArrow{draw_icon.svg: crate_resource("self:resources/icons/uturn.svg")}}
                        roundabout := ArrowMark{BannerArrow{draw_icon.svg: crate_resource("self:resources/icons/roundabout.svg")}}
                    }
                    View{width: Fill height: Fit flow: Down spacing: 2
                        banner_distance := Text{width: Fill max_lines: 1 draw_text +: {color: c_on_banner text_style: theme.font_bold{font_size: 24}}}
                        banner_text := Text{width: Fill max_lines: 2 text_overflow: Ellipsis draw_text +: {color: c_on_banner text_style: theme.font_regular{font_size: 16}}}
                    }
                }
            }
            // No ground of its own: the map keeps its touches.
            View{width: Fill height: Fill flow: Down align: Align{x: 1.0 y: 1.0} padding: Inset{right: 12 bottom: 12}
                recenter := Fab{visible: false draw_icon +: {svg: crate_resource("self:resources/icons/recenter.svg") color: c_accent}}
            }
            View{width: Fill height: Fit padding: Inset{left: 10 right: 10 bottom: 12}
                nav_bar := Floating{flow: Right align: Align{y: 0.5} padding: Inset{left: 20 right: 14 top: 14 bottom: 14}
                    draw_bg +: {border_radius: uniform(20.0)}
                    View{width: Fill height: Fit flow: Down spacing: 3
                        nav_time := Text{width: Fill max_lines: 1 draw_text +: {color: c_banner text_style: theme.font_bold{font_size: 22}}}
                        nav_rest := Caption{width: Fill max_lines: 1 draw_text.text_style: theme.font_regular{font_size: 14}}
                    }
                    end := DangerButton{text: "End"}
                    done := PrimaryButton{visible: false text: "Done"}
                }
            }
        }

        // Search: a page of its own over the map.
        search_layer := View{visible: false width: Fill height: Fill flow: Overlay
            // A `SolidView`: a plain view's ground has no shader at this
            // revision and would draw nothing.
            SolidView{width: Fill height: Fill draw_bg.color: c_ground cursor: MouseCursor.Default}
            View{width: Fill height: Fill flow: Down
                View{width: Fill height: Fit padding: Inset{left: 12 right: 12 top: 10 bottom: 8}
                    Floating{height: 48 flow: Right align: Align{y: 0.5} padding: Inset{left: 4 right: 4}
                        search_back := BarButton{draw_icon.svg: crate_resource("self:resources/icons/back.svg")}
                        search := Field{}
                        search_clear := BarButton{visible: false draw_icon.svg: crate_resource("self:resources/icons/close.svg")}
                    }
                }
                results := PortalList{width: Fill height: Fill
                    Result := ResultRow{}
                    Status := StatusRow{}
                }
            }
        }

        // What the app has to say in passing, over any screen.
        View{width: Fill height: Fit align: Align{x: 0.5} padding: Inset{top: 68}
            status := Floating{visible: false width: Fit padding: Inset{left: 16 right: 16 top: 9 bottom: 9}
                draw_bg +: {border_radius: uniform(18.0)}
                status_text := Text{draw_text.text_style: theme.font_regular{font_size: 13}}
            }
        }

        // The layers sheet: a scrim and a card at the bottom.
        layers_layer := View{visible: false width: Fill height: Fill flow: Overlay
            scrim := SolidView{width: Fill height: Fill draw_bg.color: #00000055 cursor: MouseCursor.Default}
            View{width: Fill height: Fill flow: Down align: Align{y: 1.0} padding: Inset{left: 10 right: 10 bottom: 12}
                Floating{flow: Down padding: Inset{top: 8 bottom: 8}
                    View{width: Fill height: 48 flow: Right align: Align{y: 0.5} padding: Inset{left: 20 right: 8}
                        Text{width: Fill text: "Map details" draw_text.text_style: theme.font_bold{font_size: 17}}
                        layers_close := BarButton{draw_icon.svg: crate_resource("self:resources/icons/close.svg")}
                    }
                    Hairline{}
                    SwitchRow{SwitchLabel{text: "Dark map"} dark_map := Switch{}}
                    SwitchRow{SwitchLabel{text: "3D buildings"} buildings := Switch{}}
                    SwitchRow{SwitchLabel{text: "Labels"} labels := Switch{}}
                    SwitchRow{SwitchLabel{text: "Distances in miles"} miles := Switch{}}
                }
            }
        }
    }
}

/// One entry of the results list: a place, or what the list says instead.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResultItem {
    Place(usize),
    Status { text: String, retry: bool },
}

impl ResultItem {
    fn template(&self) -> LiveId {
        match self {
            ResultItem::Place(_) => live_id!(Result),
            ResultItem::Status { .. } => live_id!(Status),
        }
    }
}

/// The results list for a search in this state.
pub(crate) fn result_items(state: &SearchState, results: usize, query: &str) -> Vec<ResultItem> {
    let status = |text: &str, retry| {
        vec![ResultItem::Status {
            text: text.to_string(),
            retry,
        }]
    };
    match state {
        // A failure keeps the last list in the model, but says so instead
        // of showing rows that may not match what is typed now.
        SearchState::Failed(why) => status(&format!("Couldn't search: {why}"), true),
        _ if results > 0 => (0..results).map(ResultItem::Place).collect(),
        SearchState::Loading => status("Searching…", false),
        SearchState::Done => status(&format!("No results for “{}”", query.trim()), false),
        SearchState::Idle => status("Search for a place or an address", false),
    }
}

/// The map, the screens over it and what drives them: one widget that owns
/// everything the standalone window's `App` would otherwise own, so a module
/// host gets the same maps a window does.
#[derive(Script, ScriptHook, Widget)]
pub struct MapsView {
    #[deref]
    view: View,
    /// Set once, on the first event or draw.
    #[rust]
    started: bool,
    #[rust]
    model: MapsModel,
    #[rust]
    location: LocationState,
    /// Runs while the locate button waits for its first fix.
    #[rust]
    location_timeout: Option<Timer>,
    /// Takes the status line down again.
    #[rust]
    status_timer: Option<Timer>,
    /// Runs from the last keystroke to the search it sends.
    #[rust]
    search_timer: Option<Timer>,
    #[rust]
    layers_open: bool,
    /// The results list as drawn, recomputed by `render`.
    #[rust]
    items: Vec<ResultItem>,
    /// Search was just opened: its field takes the key focus (and raises
    /// the keyboard) after its first draw, when it has an area to focus.
    #[rust]
    focus_search: bool,
    #[rust]
    sheet: Sheet,
    /// The sheet's height as drawn, on its way to `sheet`'s.
    #[rust]
    sheet_height: f64,
    /// Runs while the sheet's height is easing.
    #[rust]
    sheet_frame: NextFrame,
    /// The finger on the sheet a frame ago, for the speed it lifts at.
    #[rust]
    sheet_last_move: Option<(f64, f64)>,
    /// Pixels a second, downwards, of the sheet's last drag.
    #[rust]
    sheet_velocity: f64,
    /// The viewport's size at the last draw: the sheet's heights and the
    /// camera's room are fractions of it.
    #[rust]
    viewport: (f64, f64),
    /// Guidance, while navigating or previewing.
    #[rust]
    nav: Option<ActiveNav>,
    /// Runs while a preview drives.
    #[rust]
    nav_frame: NextFrame,
    /// The clock of the preview's last frame, or of the last live fix.
    #[rust]
    nav_clock: Option<f64>,
    /// Whether the camera follows the puck: a pan pauses it.
    #[rust]
    chasing: bool,
    /// Where guidance last put the camera: where Recentre goes back to.
    #[rust]
    chase_center: Option<LonLat>,
    /// Where a press on the bare map began, while the camera is chasing.
    #[rust]
    map_press: Option<Vec2d>,
    /// A new route is on its way: the banner says so.
    #[rust]
    rerouting: bool,
    /// The map theme last applied, so a skin change re-applies it.
    #[rust]
    map_theme: Option<u32>,
}

impl MapsView {
    pub fn model(&self) -> &MapsModel {
        &self.model
    }

    pub fn location(&self) -> LocationState {
        self.location
    }

    #[cfg(test)]
    pub(crate) fn layers_open(&self) -> bool {
        self.layers_open
    }

    #[cfg(test)]
    pub(crate) fn items(&self) -> &[ResultItem] {
        &self.items
    }

    #[cfg(test)]
    pub(crate) fn navigating(&self) -> bool {
        self.nav.is_some()
    }

    #[cfg(test)]
    pub(crate) fn sheet_detent(&self) -> Detent {
        self.sheet.detent()
    }

    #[cfg(test)]
    pub(crate) fn status_text(&self, cx: &Cx) -> Option<String> {
        self.view
            .widget(cx, ids!(status))
            .visible()
            .then(|| self.view.label(cx, ids!(status_text)).text())
    }

    fn map(&self, cx: &Cx) -> MapViewRef {
        self.view.map_view(cx, ids!(map))
    }

    /// The timers stop, every request in flight is cancelled and the
    /// platform's location updates end; nothing else this widget owns
    /// outlives its isolate.
    pub fn shutdown(&mut self, cx: &mut Cx) {
        let timers = [
            self.location_timeout.take(),
            self.status_timer.take(),
            self.search_timer.take(),
        ];
        for timer in timers.into_iter().flatten() {
            cx.stop_timer(timer);
        }
        for id in self.model.cancel_all() {
            cx.cancel_http_request(id);
        }
        // Its frames stop with it.
        self.nav = None;
        if self.location.failed() {
            cx.stop_location_updates();
        }
    }

    /// Start on the first event or draw, whichever comes first.
    fn ensure_started(&mut self, cx: &mut Cx) {
        if self.started {
            return;
        }
        self.started = true;
        let map = self.map(cx);
        map.set_source_config(cx, TileSourceConfig::http_archive(HOSTED_TILES));
        self.apply_settings(cx);
        let settings = &self.model.settings;
        map.set_center(cx, settings.center.lon, settings.center.lat);
        map.set_map_zoom(cx, settings.zoom);
        self.render(cx);
    }

    /// Open the layers sheet, as its button does.
    pub fn open_layers(&mut self, cx: &mut Cx) {
        self.layers_open = true;
        self.render(cx);
    }

    /// The camera the app was given to open on (`--at`), before it starts.
    pub fn set_initial_camera(&mut self, center: LonLat, zoom: f64) {
        self.model.settings.center = center;
        self.model.settings.zoom = zoom;
    }

    /// The layer switches as the map's own state.
    fn apply_settings(&mut self, cx: &mut Cx) {
        let map = self.map(cx);
        let settings = &self.model.settings;
        map.set_buildings_3d(cx, settings.buildings_3d);
        map.set_labels_visible(cx, settings.labels);
        self.apply_map_theme(cx);
    }

    /// The map is dark when the person said so, else when the skin is.
    fn apply_map_theme(&mut self, cx: &mut Cx) {
        let dark = self
            .model
            .settings
            .dark_map
            .unwrap_or(!Skin::current().light);
        let theme = if dark {
            MAP_THEME_NIGHT
        } else {
            MAP_THEME_LIGHT
        };
        if self.map_theme != Some(theme) {
            self.map_theme = Some(theme);
            self.map(cx).set_theme(cx, theme);
        }
    }

    // ---- Requests ----

    /// One GET to a service, named as its terms ask, its body capped.
    fn send(&mut self, cx: &mut Cx, id: LiveId, url: String) {
        let mut request = HttpRequest::new(url, HttpMethod::GET);
        request.set_header("Accept".into(), "application/json".into());
        request.set_header("User-Agent".into(), USER_AGENT.into());
        // The backend stops allocating at the cap while bytes arrive. The
        // Android Java backend has no transport cap, so the check on the
        // landed body (`body_text`) is the line that holds there.
        request.max_response_body_bytes = MAX_BODY_BYTES as u64;
        cx.http_request(id, request);
    }

    /// Whatever the model no longer wants an answer to.
    fn cancel_superseded(&mut self, cx: &mut Cx) {
        for id in self.model.superseded() {
            cx.cancel_http_request(id);
        }
    }

    /// A reply landed, or failed to. The map's own tile requests, and any
    /// the model has superseded, are not ours: `complete` drops them.
    pub(crate) fn handle_reply(&mut self, cx: &mut Cx, id: LiveId, body: Result<String, String>) {
        match self.model.complete(id, body) {
            Some(Request::Search) => self.render(cx),
            // The dropped pin has a name now.
            Some(Request::Reverse) => self.render(cx),
            Some(Request::Route(mode)) => {
                // The shown tab's route goes on the map; then the next tab's
                // is asked for, one at a time.
                if mode == self.model.mode() {
                    self.show_route(cx);
                }
                self.pump_routes(cx);
                self.render(cx);
            }
            Some(Request::Reroute) => self.on_reroute(cx),
            None => {}
        }
    }

    // ---- Search ----

    /// The search page, for one end of a route or for a place to show.
    pub fn open_search(&mut self, cx: &mut Cx, target: SearchTarget) {
        self.model.open_search(target);
        self.cancel_superseded(cx);
        self.view.text_input(cx, ids!(search)).set_text(cx, "");
        self.focus_search = true;
        self.render(cx);
    }

    /// The field changed: the list follows at once for a blank field, and
    /// after a pause in the typing otherwise.
    fn set_query(&mut self, cx: &mut Cx, query: &str) {
        self.model.set_query(query);
        self.cancel_superseded(cx);
        if let Some(timer) = self.search_timer.take() {
            cx.stop_timer(timer);
        }
        if !query.trim().is_empty() {
            self.search_timer = Some(cx.start_timeout(SEARCH_DEBOUNCE_SECONDS));
        }
        self.render(cx);
    }

    /// Search for `query` as if it had been typed and the pause had passed.
    pub fn search_for(&mut self, cx: &mut Cx, query: &str) {
        self.open_search(cx, SearchTarget::Destination);
        self.view.text_input(cx, ids!(search)).set_text(cx, query);
        self.model.set_query(query);
        self.search_now(cx);
    }

    /// Send the search for what is typed, near where the map is looking.
    pub(crate) fn search_now(&mut self, cx: &mut Cx) {
        if let Some(timer) = self.search_timer.take() {
            cx.stop_timer(timer);
        }
        let near = self.model.settings.center;
        if let Some((id, url)) = self.model.begin_search(near) {
            self.cancel_superseded(cx);
            self.send(cx, id, url);
        }
        self.render(cx);
    }

    /// A row of the results was picked.
    pub fn pick_result(&mut self, cx: &mut Cx, index: usize) {
        self.model.pick_result(index);
        self.cancel_superseded(cx);
        self.leave_search(cx);
        match self.model.screen() {
            Screen::Place => self.show_place(cx),
            // One of the route's ends was picked.
            Screen::Directions => self.enter_directions(cx),
            _ => {}
        }
        self.render(cx);
    }

    /// The field lets go of the keyboard.
    fn leave_search(&mut self, cx: &mut Cx) {
        if let Some(timer) = self.search_timer.take() {
            cx.stop_timer(timer);
        }
        self.focus_search = false;
        cx.set_key_focus(Area::Empty);
        cx.hide_text_ime();
    }

    // ---- Place ----

    /// The model has a place: pin it, look at it, and bring the sheet up
    /// at peek.
    fn show_place(&mut self, cx: &mut Cx) {
        let Some(place) = self.model.place().cloned() else {
            return;
        };
        self.sheet.set(Detent::Peek);
        self.sheet_height = self.sheet.height(self.viewport.1);
        let map = self.map(cx);
        let marker = MapMarker::new(
            PLACE_MARKER,
            place.pos.lon,
            place.pos.lat,
            Skin::current().alert,
        );
        map.set_markers(cx, vec![marker]);
        // The place in the middle of the map the pill and the sheet leave.
        let insets = Insets {
            top: TOP_BAR_HEIGHT,
            bottom: self.sheet_height,
            ..Insets::default()
        };
        let point = Bounds {
            west: place.pos.lon,
            east: place.pos.lon,
            south: place.pos.lat,
            north: place.pos.lat,
        };
        // Never closer than the block, never further out than the map is.
        let zoom = self.model.settings.zoom.max(PLACE_ZOOM);
        let camera = fit_camera(&point, self.viewport, &insets, zoom);
        map.fly_to(cx, camera.center.lon, camera.center.lat, camera.zoom);
    }

    /// A long press on the map: a pin there, and the lookup that names it.
    pub(crate) fn drop_pin(&mut self, cx: &mut Cx, at: LonLat) {
        let (id, url) = self.model.drop_pin(at);
        self.cancel_superseded(cx);
        self.send(cx, id, url);
        self.show_place(cx);
        self.render(cx);
    }

    /// A pin of one of the map's own layers was tapped.
    fn open_pin(&mut self, cx: &mut Cx, lon: f64, lat: f64, info: &[(String, String)]) {
        self.model
            .select_place(places::from_pin_info(lon, lat, info));
        self.cancel_superseded(cx);
        self.show_place(cx);
        self.render(cx);
    }

    /// The sheet's texts, from the place it is about.
    fn fill_sheet(&mut self, cx: &mut Cx, place: &Place) {
        self.view
            .label(cx, ids!(place_pill_text))
            .set_text(cx, &place.name);
        self.view
            .label(cx, ids!(sheet_title))
            .set_text(cx, &place.name);
        let distance = self.distance_to(place.pos);
        let subtitle = [place.category.as_str(), distance.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        self.view
            .label(cx, ids!(sheet_subtitle))
            .set_text(cx, &subtitle);
        let address_row = self.view.widget(cx, ids!(address_row));
        address_row.set_visible(cx, !place.address.is_empty());
        address_row
            .label(cx, ids!(value))
            .set_text(cx, &place.address);
        self.view
            .widget(cx, ids!(coordinates_row))
            .label(cx, ids!(value))
            .set_text(cx, &coordinates_text(place.pos));
        let rows = [
            ids!(detail_0),
            ids!(detail_1),
            ids!(detail_2),
            ids!(detail_3),
        ];
        for (index, id) in rows.into_iter().enumerate().take(DETAIL_ROWS) {
            let row = self.view.widget(cx, id);
            let detail = place.details.get(index);
            row.set_visible(cx, detail.is_some());
            if let Some((label, value)) = detail {
                row.label(cx, ids!(label)).set_text(cx, label);
                row.label(cx, ids!(value)).set_text(cx, value);
            }
        }
    }

    /// How far a point is from the last fix, in the person's units; nothing
    /// before there is a fix to measure from.
    fn distance_to(&self, pos: LonLat) -> String {
        self.model
            .fix()
            .map(|fix| distance_text(haversine_m(fix, pos), self.model.units()))
            .unwrap_or_default()
    }

    // ---- Directions ----

    /// The Directions screen came up, or one of its ends changed: a fix is
    /// asked for if an end needs one, the routes are asked for, and the map
    /// shows what there is.
    fn enter_directions(&mut self, cx: &mut Cx) {
        self.sheet.set(Detent::Peek);
        self.sheet_height = self.sheet.height(self.viewport.1);
        self.apply_sheet_height(cx);
        if self.model.needs_fix() && self.location == LocationState::Idle {
            self.ask_location(cx);
        }
        self.pump_routes(cx);
        self.show_route(cx);
        self.render(cx);
    }

    /// The place's Directions button.
    pub fn open_directions(&mut self, cx: &mut Cx) {
        self.model.open_directions();
        if self.model.screen() == Screen::Directions {
            self.enter_directions(cx);
        }
    }

    /// Ask for the next route the model wants, if it wants one now.
    fn pump_routes(&mut self, cx: &mut Cx) {
        self.cancel_superseded(cx);
        if let Some((id, _mode, url)) = self.model.next_route_request() {
            self.send(cx, id, url);
        }
    }

    /// The map as Directions wants it: the ends pinned, the shown tab's
    /// route drawn and fitted between the card and the sheet.
    fn show_route(&mut self, cx: &mut Cx) {
        let map = self.map(cx);
        let skin = Skin::current();
        let ends = [
            (
                ORIGIN_MARKER,
                self.model.end_pos(self.model.origin()),
                skin.accent,
            ),
            (
                DESTINATION_MARKER,
                self.model.end_pos(self.model.destination()),
                skin.alert,
            ),
        ];
        let markers = ends
            .into_iter()
            .filter_map(|(id, pos, color)| Some(MapMarker::new(id, pos?.lon, pos?.lat, color)))
            .collect();
        map.set_markers(cx, markers);
        let Some(directions) = self.model.directions() else {
            map.clear_route(cx);
            return;
        };
        let points = &directions.route.points;
        let line: Vec<(f64, f64)> = points.iter().map(|p| (p.lon, p.lat)).collect();
        map.set_route(cx, &line);
        if let Some(bounds) = Bounds::of(points) {
            let insets = Insets {
                top: DIRECTIONS_CARD_HEIGHT + ROUTE_MARGIN,
                bottom: self.sheet.height(self.viewport.1) + ROUTE_MARGIN,
                left: ROUTE_MARGIN,
                right: ROUTE_MARGIN,
            };
            let camera = fit_camera(&bounds, self.viewport, &insets, ROUTE_MAX_ZOOM);
            // The fit is a flat, north-up one.
            map.set_rotation(cx, 0.0);
            map.set_tilt(cx, 0.0);
            map.fly_to(cx, camera.center.lon, camera.center.lat, camera.zoom);
        }
    }

    /// A mode's tab was picked.
    pub fn set_mode(&mut self, cx: &mut Cx, mode: Mode) {
        self.model.set_mode(mode);
        self.pump_routes(cx);
        self.show_route(cx);
        self.render(cx);
    }

    /// The card's swap button.
    pub(crate) fn swap_ends(&mut self, cx: &mut Cx) {
        self.model.swap_ends();
        self.enter_directions(cx);
    }

    /// The card's texts, the tabs' times and the sheet's summary.
    fn fill_directions(&mut self, cx: &mut Cx) {
        let origin = self.model.origin().label().to_string();
        let destination = self.model.destination().label().to_string();
        self.view.label(cx, ids!(origin_text)).set_text(cx, &origin);
        self.view
            .label(cx, ids!(destination_text))
            .set_text(cx, &destination);
        let tabs = [
            (Mode::Car, ids!(mode_car), ids!(mode_car_on)),
            (Mode::Walk, ids!(mode_walk), ids!(mode_walk_on)),
            (Mode::Bike, ids!(mode_bike), ids!(mode_bike_on)),
        ];
        let selected = self.model.mode();
        for (mode, idle, on) in tabs {
            let time = match self.model.route(mode) {
                RouteState::Ready(directions) => duration_text(directions.route.duration_s),
                RouteState::Loading => "…".to_string(),
                RouteState::Failed(_) => "—".to_string(),
                RouteState::Idle => String::new(),
            };
            for (id, shown) in [(idle, mode != selected), (on, mode == selected)] {
                let tab = self.view.button(cx, id);
                tab.set_text(cx, &time);
                tab.set_visible(cx, shown);
            }
        }
        let (title, subtitle, retry) = self.route_summary();
        self.view.label(cx, ids!(route_title)).set_text(cx, &title);
        self.view
            .label(cx, ids!(route_subtitle))
            .set_text(cx, &subtitle);
        self.view
            .widget(cx, ids!(route_retry))
            .set_visible(cx, retry);
        let ready = self.model.directions().is_some();
        self.view.widget(cx, ids!(start)).set_visible(cx, ready);
        self.view.widget(cx, ids!(preview)).set_visible(cx, ready);
        self.view.portal_list(cx, ids!(steps)).redraw(cx);
    }

    /// The sheet's two lines for the shown tab, and whether Retry belongs.
    fn route_summary(&self) -> (String, String, bool) {
        let mode = self.model.mode();
        if self.model.needs_fix() {
            // An end is the device's location, and there is no fix.
            return if self.location == LocationState::Waiting {
                (
                    "Finding your location…".into(),
                    "Or choose a starting point above".into(),
                    false,
                )
            } else {
                (
                    "Choose a starting point".into(),
                    "Your location isn't available".into(),
                    false,
                )
            };
        }
        match self.model.route(mode) {
            RouteState::Ready(directions) => (
                duration_text(directions.route.duration_s),
                format!(
                    "{} · {}",
                    distance_text(directions.route.length_m, self.model.units()),
                    mode.label()
                ),
                false,
            ),
            RouteState::Failed(why) => (why.clone(), mode.label().to_string(), true),
            RouteState::Idle | RouteState::Loading => (
                "Finding the best route…".into(),
                mode.label().to_string(),
                false,
            ),
        }
    }

    fn draw_steps(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        let steps = self
            .model
            .directions()
            .map(|d| d.steps.clone())
            .unwrap_or_default();
        let units = self.model.units();
        list.set_item_range(cx, 0, steps.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(step) = steps.get(index) else {
                continue;
            };
            let item = list.item(cx, index, live_id!(Step));
            item.label(cx, ids!(text)).set_text(cx, &step.text);
            // The arrival goes nowhere further, and a departure of a few
            // steps rounds to nothing worth printing.
            let distance = if step.distance_m >= MIN_STEP_DISTANCE_M {
                distance_text(step.distance_m, units)
            } else {
                String::new()
            };
            item.label(cx, ids!(distance)).set_text(cx, &distance);
            show_arrow(cx, &item, step.arrow);
            item.draw_all(cx, &mut Scope::empty());
        }
    }

    fn handle_directions_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.view.button(cx, ids!(directions_back)).clicked(actions) {
            self.back(cx);
            return;
        }
        if self.view.button(cx, ids!(swap)).clicked(actions) {
            self.swap_ends(cx);
            return;
        }
        let layer = self.view.widget(cx, ids!(directions_layer));
        if tapped(cx, &layer, ids!(origin_row), actions) {
            self.open_search(cx, SearchTarget::Origin);
            return;
        }
        if tapped(cx, &layer, ids!(destination_row), actions) {
            self.open_search(cx, SearchTarget::Destination);
            return;
        }
        let tabs = [
            (Mode::Car, ids!(mode_car), ids!(mode_car_on)),
            (Mode::Walk, ids!(mode_walk), ids!(mode_walk_on)),
            (Mode::Bike, ids!(mode_bike), ids!(mode_bike_on)),
        ];
        for (mode, idle, on) in tabs {
            if self.view.button(cx, idle).clicked(actions)
                || self.view.button(cx, on).clicked(actions)
            {
                self.set_mode(cx, mode);
            }
        }
        if self.view.button(cx, ids!(start)).clicked(actions) {
            self.start_navigation(cx, false);
            return;
        }
        if self.view.button(cx, ids!(preview)).clicked(actions) {
            self.start_navigation(cx, true);
            return;
        }
        if self.view.button(cx, ids!(route_retry)).clicked(actions) {
            self.model.retry_route(self.model.mode());
            self.pump_routes(cx);
            self.render(cx);
        }
        self.handle_sheet_drag(cx, actions);
    }

    // ---- Navigation ----

    /// Start, over live fixes, or Preview, over the simulated drive.
    pub fn start_navigation(&mut self, cx: &mut Cx, simulate: bool) {
        let Some(nav) = self.model.start_navigation(simulate) else {
            return;
        };
        let map = self.map(cx);
        // The destination stays pinned; the origin is where the puck is.
        if let Some(to) = self.model.end_pos(self.model.destination()) {
            let pin = MapMarker::new(DESTINATION_MARKER, to.lon, to.lat, Skin::current().alert);
            map.set_markers(cx, vec![pin]);
        }
        let start = nav.directions().route.points.first().copied();
        self.nav = Some(nav);
        self.nav_clock = None;
        self.rerouting = false;
        self.chasing = true;
        // Set, not flown to: a flight would read as a finger's pan.
        if let Some(start) = start {
            map.set_center(cx, start.lon, start.lat);
            self.chase_center = Some(start);
        }
        map.set_map_zoom(cx, NAV_ZOOM);
        map.set_tilt(cx, NAV_TILT);
        if simulate {
            self.nav_frame = cx.new_next_frame();
        } else if self.location == LocationState::Idle {
            self.ask_location(cx);
        }
        // The first banner, before the first fix or frame.
        let tick = match (self.nav.as_mut(), simulate, self.model.fix()) {
            (Some(nav), true, _) => Some(nav.tick_sim(0.0)),
            (Some(nav), false, Some(fix)) => Some(nav.feed(fix, None, 0.0)),
            _ => None,
        };
        if let Some(tick) = tick {
            self.apply_tick(cx, tick);
        }
        self.render(cx);
    }

    /// One frame of the preview.
    fn tick_preview(&mut self, cx: &mut Cx, time: f64) {
        let dt = self.nav_clock.map_or(0.0, |last| time - last);
        self.nav_clock = Some(time);
        let Some(nav) = self.nav.as_mut().filter(|nav| nav.simulate()) else {
            return;
        };
        let tick = nav.tick_sim(dt);
        let arrived = tick.state == NavState::Arrived;
        self.apply_tick(cx, tick);
        if !arrived {
            self.nav_frame = cx.new_next_frame();
        }
    }

    /// What one position means on screen: the puck, the chase, the route
    /// behind it dimmed, the banner and the bar; a reroute asked for when
    /// guidance wants one; arrival.
    fn apply_tick(&mut self, cx: &mut Cx, tick: NavTick) {
        let map = self.map(cx);
        let at = tick.position;
        map.set_puck(
            cx,
            Some(MapPuck::new(
                at.lon,
                at.lat,
                tick.heading,
                NAV_PUCK_ACCURACY_M,
            )),
        );
        map.set_route_progress(cx, tick.progress_index);
        if self.chasing {
            let ahead = lead(at, tick.rotation, NAV_LEAD_M);
            map.set_center(cx, ahead.lon, ahead.lat);
            map.set_rotation(cx, tick.rotation);
            self.chase_center = Some(ahead);
        }
        if tick.needs_reroute {
            if let Some((id, url)) = self.model.begin_reroute(at) {
                self.send(cx, id, url);
                self.rerouting = true;
            }
        }
        let units = self.model.units();
        let banner = self.view.widget(cx, ids!(banner));
        if self.rerouting {
            self.view.label(cx, ids!(banner_distance)).set_text(cx, "");
            self.view
                .label(cx, ids!(banner_text))
                .set_text(cx, "Rerouting…");
        } else {
            let distance = if tick.state == NavState::Arrived {
                String::new()
            } else {
                distance_text(tick.banner_distance_m, units)
            };
            self.view
                .label(cx, ids!(banner_distance))
                .set_text(cx, &distance);
            self.view
                .label(cx, ids!(banner_text))
                .set_text(cx, &tick.banner);
        }
        if let Some(arrow) = tick.arrow {
            show_arrow(cx, &banner, arrow);
        }
        self.view
            .label(cx, ids!(nav_time))
            .set_text(cx, &duration_text(tick.remaining_s));
        let mut rest = distance_text(tick.remaining_m, units);
        if let Some(now) = local_minutes_of_day() {
            rest = format!("{rest} · {}", arrival_text(now, tick.remaining_s));
        }
        self.view.label(cx, ids!(nav_rest)).set_text(cx, &rest);
        if tick.state == NavState::Arrived && self.model.screen() == Screen::Navigating {
            self.model.arrive();
            self.view.label(cx, ids!(nav_time)).set_text(cx, "Arrived");
            self.view.label(cx, ids!(nav_rest)).set_text(cx, "");
            self.render(cx);
        }
        self.view.redraw(cx);
    }

    /// A live fix while navigating: guidance takes it.
    fn feed_nav(&mut self, cx: &mut Cx, fix: &LocationUpdateEvent) {
        let Some(nav) = self.nav.as_mut().filter(|nav| !nav.simulate()) else {
            return;
        };
        // The fixes' own clock; a platform that sends none, or the same
        // twice, counts a second.
        let dt = self
            .nav_clock
            .map(|last| fix.time - last)
            .filter(|dt| *dt > 0.0 && *dt < 60.0)
            .unwrap_or(DEFAULT_FIX_INTERVAL_S);
        self.nav_clock = Some(fix.time);
        let tick = nav.feed(LonLat::new(fix.lon, fix.lat), fix.heading_deg, dt);
        self.apply_tick(cx, tick);
    }

    /// The reroute's reply: guidance starts over on the new route, or the
    /// old one stands and guidance will ask again if the driver stays off it.
    fn on_reroute(&mut self, cx: &mut Cx) {
        self.rerouting = false;
        match (self.model.take_reroute(), self.nav.as_mut()) {
            (Some(directions), Some(nav)) => {
                let line: Vec<(f64, f64)> = directions
                    .route
                    .points
                    .iter()
                    .map(|p| (p.lon, p.lat))
                    .collect();
                nav.replace_route(directions);
                self.map(cx).set_route(cx, &line);
            }
            (None, Some(_)) => self.show_status(cx, "Couldn't find a new route"),
            _ => {}
        }
    }

    /// The drive is over, by End, by Done or by the back gesture: the
    /// camera lies flat again and a preview's puck goes back to the fix.
    fn end_navigation(&mut self, cx: &mut Cx) {
        let Some(nav) = self.nav.take() else {
            return;
        };
        self.rerouting = false;
        self.chasing = false;
        self.chase_center = None;
        let map = self.map(cx);
        map.set_rotation(cx, 0.0);
        map.set_tilt(cx, 0.0);
        map.set_route_progress(cx, 0);
        if nav.simulate() {
            let puck = self
                .model
                .fix()
                .map(|fix| MapPuck::new(fix.lon, fix.lat, None, NAV_PUCK_ACCURACY_M));
            map.set_puck(cx, puck);
        }
    }

    /// The camera is back on the puck.
    fn recenter(&mut self, cx: &mut Cx) {
        self.chasing = true;
        let map = self.map(cx);
        map.set_map_zoom(cx, NAV_ZOOM);
        map.set_tilt(cx, NAV_TILT);
        if let Some(at) = self.chase_center {
            map.set_center(cx, at.lon, at.lat);
        }
        self.render(cx);
    }

    /// While the camera chases, a finger that takes hold of the bare map and
    /// moves it pauses the chase. Watched on the raw pointer events, without
    /// hit-testing: the map keeps its own capture. (The map reports a
    /// viewport change for guidance's own moves too, a pass late, so its
    /// reports cannot tell the two apart.)
    fn watch_pointer(&mut self, cx: &mut Cx, event: &Event) {
        if self.nav.is_none() || !self.chasing {
            self.map_press = None;
            return;
        }
        match event {
            Event::MouseDown(down) => self.pointer_down(cx, down.abs),
            Event::MouseMove(moved) => self.pointer_moved(cx, moved.abs),
            Event::MouseUp(_) => self.map_press = None,
            Event::TouchUpdate(update) => {
                for touch in &update.touches {
                    match touch.state {
                        TouchState::Start => self.pointer_down(cx, touch.abs),
                        TouchState::Move => self.pointer_moved(cx, touch.abs),
                        TouchState::Stop => self.map_press = None,
                        TouchState::Stable => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn pointer_down(&mut self, cx: &mut Cx, abs: Vec2d) {
        let inside = |id: &[LiveId]| {
            let area = self.view.widget(cx, id).area();
            area.is_valid(cx) && area.rect(cx).contains(abs)
        };
        // On the map, and not on what floats over it.
        if inside(ids!(map)) && !inside(ids!(banner)) && !inside(ids!(nav_bar)) {
            self.map_press = Some(abs);
        }
    }

    fn pointer_moved(&mut self, cx: &mut Cx, abs: Vec2d) {
        let Some(start) = self.map_press else {
            return;
        };
        if (abs - start).length() > PAN_PIXELS {
            self.map_press = None;
            self.chasing = false;
            self.render(cx);
        }
    }

    fn handle_nav_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.view.button(cx, ids!(end)).clicked(actions)
            || self.view.button(cx, ids!(done)).clicked(actions)
        {
            self.back(cx);
            return;
        }
        if self.view.button(cx, ids!(recenter)).clicked(actions) {
            self.recenter(cx);
        }
    }

    // ---- The sheet ----

    /// The sheet on screen: the place's, or on Directions the route's.
    fn sheet_widget(&self, cx: &Cx) -> WidgetRef {
        if self.model.screen() == Screen::Directions {
            self.view.widget(cx, ids!(route_sheet))
        } else {
            self.view.widget(cx, ids!(sheet))
        }
    }

    /// Its height as a property, without asking for a draw.
    fn set_sheet_height(&mut self, cx: &mut Cx) {
        let height = self.sheet_height;
        let mut sheet = self.sheet_widget(cx);
        script_apply_eval!(cx, sheet, { height: #(height) });
    }

    fn apply_sheet_height(&mut self, cx: &mut Cx) {
        self.set_sheet_height(cx);
        self.view.redraw(cx);
    }

    /// A finger on the sheet's head: it follows, and snaps on release.
    fn handle_sheet_drag(&mut self, cx: &mut Cx, actions: &Actions) {
        let head = if self.model.screen() == Screen::Directions {
            self.view.view(cx, ids!(route_head))
        } else {
            self.view.view(cx, ids!(sheet_head))
        };
        let viewport_h = self.viewport.1;
        if head.finger_down(actions).is_some() {
            self.sheet.drag_start(viewport_h);
            self.sheet_last_move = None;
            self.sheet_velocity = 0.0;
        }
        if let Some(moved) = head.finger_move(actions) {
            self.sheet
                .drag_by(moved.abs.y - moved.abs_start.y, viewport_h);
            if let Some((y, time)) = self.sheet_last_move {
                let dt = moved.time - time;
                if dt > 0.0 {
                    self.sheet_velocity = (moved.abs.y - y) / dt;
                }
            }
            self.sheet_last_move = Some((moved.abs.y, moved.time));
            self.sheet_height = self.sheet.height(viewport_h);
            self.apply_sheet_height(cx);
        }
        if let Some(up) = head.finger_up(actions) {
            if up.was_tap() {
                // A tap on the head steps the sheet up, and from full down.
                let next = match self.sheet.detent() {
                    Detent::Peek => Detent::Half,
                    Detent::Half => Detent::Full,
                    Detent::Full => Detent::Peek,
                };
                self.sheet.set(next);
            } else {
                self.sheet.release(self.sheet_velocity, viewport_h);
            }
            self.sheet_frame = cx.new_next_frame();
        }
    }

    /// One frame of the sheet's way to its detent.
    fn ease_sheet(&mut self, cx: &mut Cx) {
        let target = self.sheet.height(self.viewport.1);
        let rest = target - self.sheet_height;
        if rest.abs() < 0.5 {
            self.sheet_height = target;
        } else {
            self.sheet_height += rest * SHEET_EASE;
            self.sheet_frame = cx.new_next_frame();
        }
        self.apply_sheet_height(cx);
    }

    // ---- Location ----

    /// The locate button, or a screen that wants a fix.
    pub fn ask_location(&mut self, cx: &mut Cx) {
        match self.location.asked() {
            LocationAsk::Start => {
                self.show_status(cx, "Locating…");
                cx.start_location_updates();
                self.location_timeout = Some(cx.start_timeout(LOCATION_FIX_TIMEOUT_SECONDS));
            }
            LocationAsk::Recenter => {
                if let Some(fix) = self.model.fix() {
                    self.map(cx).fly_to(cx, fix.lon, fix.lat, LOCATE_ZOOM);
                }
            }
            LocationAsk::Ignore => {}
        }
        self.render(cx);
    }

    fn on_location_update(&mut self, cx: &mut Cx, fix: &LocationUpdateEvent) {
        let kind = self.location.received_fix();
        if kind == LocationFix::Ignore {
            return;
        }
        self.model.set_fix(LonLat::new(fix.lon, fix.lat));
        if self.nav.as_ref().is_some_and(|nav| !nav.simulate()) {
            // Guidance draws the puck, on the line.
            if kind == LocationFix::First {
                self.stop_location_timeout(cx);
                self.hide_status(cx);
            }
            self.feed_nav(cx, fix);
            return;
        }
        let map = self.map(cx);
        // A preview's puck is the preview's.
        if self.nav.is_none() {
            map.set_puck(
                cx,
                Some(MapPuck::new(
                    fix.lon,
                    fix.lat,
                    fix.heading_deg,
                    fix.accuracy_m,
                )),
            );
        }
        let routing = self.model.screen() == Screen::Directions;
        if kind == LocationFix::First {
            self.stop_location_timeout(cx);
            self.hide_status(cx);
            // Directions fits the route instead, once it has one.
            if !routing {
                map.fly_to(cx, fix.lon, fix.lat, LOCATE_ZOOM);
            }
            self.render(cx);
        }
        if routing {
            // The fix an end was waiting for; nothing to ask otherwise.
            self.pump_routes(cx);
            self.render(cx);
        }
    }

    /// An error, or the timeout: say why, once, and let the next tap retry.
    fn fail_location(&mut self, cx: &mut Cx, why: &str) {
        if !self.location.failed() {
            return;
        }
        self.stop_location_timeout(cx);
        cx.stop_location_updates();
        self.show_status(cx, why);
        self.render(cx);
    }

    fn stop_location_timeout(&mut self, cx: &mut Cx) {
        if let Some(timer) = self.location_timeout.take() {
            cx.stop_timer(timer);
        }
    }

    // ---- The status line ----

    fn show_status(&mut self, cx: &mut Cx, text: &str) {
        self.view.label(cx, ids!(status_text)).set_text(cx, text);
        self.view.widget(cx, ids!(status)).set_visible(cx, true);
        if let Some(timer) = self.status_timer.take() {
            cx.stop_timer(timer);
        }
        self.status_timer = Some(cx.start_timeout(STATUS_SECONDS));
        self.view.redraw(cx);
    }

    fn hide_status(&mut self, cx: &mut Cx) {
        if let Some(timer) = self.status_timer.take() {
            cx.stop_timer(timer);
        }
        self.view.widget(cx, ids!(status)).set_visible(cx, false);
        self.view.redraw(cx);
    }

    // ---- The map's camera ----

    /// Whether the map is north-up and flat, give or take the snap.
    fn is_level(&self, cx: &Cx) -> bool {
        let map = self.map(cx);
        let rotation = map.rotation().rem_euclid(360.0);
        rotation.min(360.0 - rotation) < LEVEL_DEGREES && map.tilt().abs() < LEVEL_DEGREES
    }

    /// The compass: north-up and flat again.
    fn level_map(&mut self, cx: &mut Cx) {
        let map = self.map(cx);
        map.set_rotation(cx, 0.0);
        map.set_tilt(cx, 0.0);
        self.render(cx);
    }

    // ---- Back ----

    /// One step back: the layers sheet, then the model's screens. `false`
    /// when nothing was left, so the host handles it.
    pub fn back(&mut self, cx: &mut Cx) -> bool {
        if self.layers_open {
            self.layers_open = false;
            self.render(cx);
            return true;
        }
        let from = self.model.screen();
        if !self.model.back() {
            return false;
        }
        self.cancel_superseded(cx);
        if matches!(from, Screen::Search { .. }) {
            self.leave_search(cx);
        }
        if matches!(from, Screen::Navigating | Screen::Arrived) {
            self.end_navigation(cx);
        }
        match (from, self.model.screen()) {
            // End: the routes again, fitted; asked for again if the drive
            // was rerouted away from them.
            (Screen::Navigating, Screen::Directions) => self.enter_directions(cx),
            // Done: the place the drive was to.
            (Screen::Arrived, Screen::Place) => {
                self.map(cx).clear_route(cx);
                self.show_place(cx);
            }
            (_, Screen::Explore) => {
                let map = self.map(cx);
                map.set_markers(cx, Vec::new());
                map.clear_route(cx);
            }
            // The route goes, the place's pin and sheet come back.
            (Screen::Directions, Screen::Place) => {
                self.map(cx).clear_route(cx);
                self.show_place(cx);
            }
            // Search left without a pick: the trip is as it was.
            (Screen::Search { .. }, Screen::Directions) => self.enter_directions(cx),
            _ => {}
        }
        self.render(cx);
        true
    }

    // ---- Showing the state ----

    /// Show the state: which layers are up, the locate button's face, the
    /// compass, the results, the sheet's texts.
    fn render(&mut self, cx: &mut Cx) {
        let screen = self.model.screen();
        let searching = matches!(screen, Screen::Search { .. });
        self.view
            .widget(cx, ids!(explore))
            .set_visible(cx, screen == Screen::Explore);
        self.view
            .widget(cx, ids!(search_layer))
            .set_visible(cx, searching);
        self.view
            .widget(cx, ids!(place_layer))
            .set_visible(cx, screen == Screen::Place);
        self.view
            .widget(cx, ids!(directions_layer))
            .set_visible(cx, screen == Screen::Directions);
        let driving = matches!(screen, Screen::Navigating | Screen::Arrived);
        self.view
            .widget(cx, ids!(nav_layer))
            .set_visible(cx, driving);
        if driving {
            let arrived = screen == Screen::Arrived;
            self.view.widget(cx, ids!(end)).set_visible(cx, !arrived);
            self.view.widget(cx, ids!(done)).set_visible(cx, arrived);
            self.view
                .widget(cx, ids!(recenter))
                .set_visible(cx, !self.chasing && !arrived);
        }
        self.view
            .widget(cx, ids!(layers_layer))
            .set_visible(cx, self.layers_open);
        let following = self.location == LocationState::Active;
        self.view
            .widget(cx, ids!(locate))
            .set_visible(cx, !following);
        self.view
            .widget(cx, ids!(locate_active))
            .set_visible(cx, following);
        let level = self.is_level(cx);
        self.view.widget(cx, ids!(compass)).set_visible(cx, !level);
        if searching {
            self.items = result_items(
                &self.model.search_state,
                self.model.results.len(),
                &self.model.query,
            );
            self.view
                .widget(cx, ids!(search_clear))
                .set_visible(cx, !self.model.query.is_empty());
            let hint = match self.model.search_target() {
                Some(SearchTarget::Origin) => "Choose a starting point",
                _ => "Search here",
            };
            let mut field = self.view.widget(cx, ids!(search));
            script_apply_eval!(cx, field, { empty_text: #(hint) });
        }
        if screen == Screen::Place {
            if let Some(place) = self.model.place().cloned() {
                self.fill_sheet(cx, &place);
            }
        }
        if screen == Screen::Directions {
            self.fill_directions(cx);
        }
        self.view.redraw(cx);
    }

    fn draw_results(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        list.set_item_range(cx, 0, self.items.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(entry) = self.items.get(index).cloned() else {
                continue;
            };
            let item = list.item(cx, index, entry.template());
            match &entry {
                ResultItem::Place(result) => {
                    let Some(place) = self.model.results.get(*result) else {
                        continue;
                    };
                    item.label(cx, ids!(name)).set_text(cx, &place.name);
                    // A place with no address still says what it is.
                    let line = if place.address.is_empty() {
                        &place.category
                    } else {
                        &place.address
                    };
                    item.label(cx, ids!(address)).set_text(cx, line);
                    item.label(cx, ids!(distance))
                        .set_text(cx, &self.distance_to(place.pos));
                    show_kind(cx, &item, place.kind);
                }
                ResultItem::Status { text, retry } => {
                    item.label(cx, ids!(text)).set_text(cx, text);
                    item.widget(cx, ids!(retry)).set_visible(cx, *retry);
                }
            }
            item.draw_all(cx, &mut Scope::empty());
        }
    }

    fn handle_result_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let list = self.view.portal_list(cx, ids!(results));
        let mut pick = None;
        let mut retry = false;
        for (index, item) in list.items_with_actions(actions) {
            match self.items.get(index) {
                Some(ResultItem::Place(result)) if tapped(cx, &item, ids!(row), actions) => {
                    pick = Some(*result);
                }
                Some(ResultItem::Status { retry: true, .. }) => {
                    retry |= item.button(cx, ids!(retry)).clicked(actions);
                }
                _ => {}
            }
        }
        if let Some(index) = pick {
            self.pick_result(cx, index);
        } else if retry {
            self.search_now(cx);
        }
    }

    /// The layers sheet's switches, as the settings stand.
    fn sync_switches(&mut self, cx: &mut Cx) {
        let settings = &self.model.settings;
        let dark = settings.dark_map.unwrap_or(!Skin::current().light);
        let miles = self.model.units() == Units::Imperial;
        let states = [
            (ids!(dark_map), dark),
            (ids!(buildings), settings.buildings_3d),
            (ids!(labels), settings.labels),
            (ids!(miles), miles),
        ];
        for (id, on) in states {
            let switch = self.view.check_box(cx, id);
            if switch.active(cx) != on {
                // A cut, not a play: the sheet opens showing what is.
                switch.set_active(cx, on, Animate::No);
                self.view.redraw(cx);
            }
        }
    }

    fn handle_layers_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let layer = self.view.widget(cx, ids!(layers_layer));
        if self.view.button(cx, ids!(layers_close)).clicked(actions)
            || tapped(cx, &layer, ids!(scrim), actions)
        {
            self.layers_open = false;
            self.render(cx);
            return;
        }
        let mut changed = false;
        if let Some(on) = self.view.check_box(cx, ids!(dark_map)).changed(actions) {
            self.model.settings.dark_map = Some(on);
            changed = true;
        }
        if let Some(on) = self.view.check_box(cx, ids!(buildings)).changed(actions) {
            self.model.settings.buildings_3d = on;
            changed = true;
        }
        if let Some(on) = self.view.check_box(cx, ids!(labels)).changed(actions) {
            self.model.settings.labels = on;
            changed = true;
        }
        if let Some(on) = self.view.check_box(cx, ids!(miles)).changed(actions) {
            self.model.settings.units = Some(if on { Units::Imperial } else { Units::Metric });
            changed = true;
        }
        if changed {
            self.apply_settings(cx);
        }
    }

    fn handle_explore_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.view.button(cx, ids!(layers)).clicked(actions) {
            self.open_layers(cx);
        }
        if self.view.button(cx, ids!(locate)).clicked(actions)
            || self.view.button(cx, ids!(locate_active)).clicked(actions)
        {
            self.ask_location(cx);
        }
        if self.view.button(cx, ids!(compass)).clicked(actions) {
            self.level_map(cx);
        }
        if tapped(
            cx,
            &self.view.widget(cx, ids!(explore)),
            ids!(pill),
            actions,
        ) {
            self.open_search(cx, SearchTarget::Destination);
        }
    }

    fn handle_search_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.view.button(cx, ids!(search_back)).clicked(actions) {
            self.back(cx);
            return;
        }
        let field = self.view.text_input(cx, ids!(search));
        if self.view.button(cx, ids!(search_clear)).clicked(actions) {
            field.set_text(cx, "");
            self.set_query(cx, "");
            self.focus_search = true;
        }
        if let Some(query) = field.changed(actions) {
            self.set_query(cx, &query);
        }
        if field.returned(actions).is_some() {
            self.search_now(cx);
        }
        self.handle_result_actions(cx, actions);
    }

    fn handle_place_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.view.button(cx, ids!(place_back)).clicked(actions)
            || self.view.button(cx, ids!(place_close)).clicked(actions)
        {
            self.back(cx);
            return;
        }
        let layer = self.view.widget(cx, ids!(place_layer));
        if tapped(cx, &layer, ids!(place_pill), actions) {
            self.open_search(cx, SearchTarget::Destination);
            return;
        }
        if self.view.button(cx, ids!(directions)).clicked(actions) {
            self.open_directions(cx);
            return;
        }
        self.handle_sheet_drag(cx, actions);
    }

    /// What the map itself reports: a long press drops a pin, a layer's pin
    /// opens, a tap on the bare map lets go of the place.
    fn handle_map_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let map = self.map(cx);
        let screen = self.model.screen();
        if matches!(screen, Screen::Explore | Screen::Place) {
            if let Some((lon, lat)) = map.long_pressed(actions) {
                self.drop_pin(cx, LonLat::new(lon, lat));
            } else if let Some((lon, lat, info)) = map.pin_tapped(actions) {
                self.open_pin(cx, lon, lat, &info);
            } else if screen == Screen::Place && map.tapped(actions).is_some() {
                self.back(cx);
            }
        }
        // The camera moved, by a finger or by the app: the settings keep
        // where it is, and the compass shows when it is not level.
        if let Some((lon, lat, zoom)) = map.viewport_changed(actions) {
            // Where the app reopens: not halfway down a drive.
            if self.nav.is_none() {
                self.model.settings.center = LonLat::new(lon, lat);
                self.model.settings.zoom = zoom;
                self.render(cx);
            }
        }
        if map.tilt_changed(actions).is_some() {
            self.render(cx);
        }
    }
}

/// A tap (not the end of a drag) on the child `id` of `parent`.
fn tapped(cx: &Cx, parent: &WidgetRef, id: &[LiveId], actions: &Actions) -> bool {
    parent
        .widget(cx, id)
        .as_view()
        .finger_up(actions)
        .is_some_and(|up| up.was_tap())
}

/// Shows the mark of `kind` among a result row's marks and hides the rest.
fn show_kind(cx: &mut Cx, item: &WidgetRef, kind: PlaceKind) {
    let marks = [
        (ids!(place), PlaceKind::Place),
        (ids!(food), PlaceKind::Food),
        (ids!(shop), PlaceKind::Shop),
        (ids!(transit), PlaceKind::Transit),
        (ids!(lodging), PlaceKind::Lodging),
        (ids!(street), PlaceKind::Street),
    ];
    for (id, mark) in marks {
        item.widget(cx, id).set_visible(cx, mark == kind);
    }
}

/// The point `meters` from `from` along `bearing_deg`: near enough on a
/// plane, at the distances a camera leads by.
fn lead(from: LonLat, bearing_deg: f64, meters: f64) -> LonLat {
    const M_PER_DEGREE: f64 = 111_320.0;
    let bearing = bearing_deg.to_radians();
    let north = meters * bearing.cos() / M_PER_DEGREE;
    let east = meters * bearing.sin() / (M_PER_DEGREE * from.lat.to_radians().cos().max(0.01));
    LonLat::new(from.lon + east, from.lat + north)
}

/// The local time of day in minutes, from the C library's local time
/// (`localtime_r`, the tz data the shell's clock reads); `None` where there
/// is none, and the bar then shows no arrival time.
fn local_minutes_of_day() -> Option<u32> {
    #[cfg(unix)]
    {
        // SAFETY: `time` accepts a null out-pointer, and `localtime_r`
        // writes only into the zeroed `tm` it is handed.
        let tm = unsafe {
            let now = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&now, &mut tm).is_null() {
                return None;
            }
            tm
        };
        Some((tm.tm_hour * 60 + tm.tm_min).clamp(0, 24 * 60 - 1) as u32)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Shows the mark of `arrow` among a step row's marks and hides the rest.
fn show_arrow(cx: &mut Cx, item: &WidgetRef, arrow: Arrow) {
    let marks = [
        (ids!(depart), Arrow::Depart),
        (ids!(arrive), Arrow::Arrive),
        (ids!(straight), Arrow::Straight),
        (ids!(slight_left), Arrow::SlightLeft),
        (ids!(left), Arrow::Left),
        (ids!(sharp_left), Arrow::SharpLeft),
        (ids!(slight_right), Arrow::SlightRight),
        (ids!(right), Arrow::Right),
        (ids!(sharp_right), Arrow::SharpRight),
        (ids!(uturn), Arrow::UTurn),
        (ids!(roundabout), Arrow::Roundabout),
    ];
    for (id, mark) in marks {
        item.widget(cx, id).set_visible(cx, mark == arrow);
    }
}

impl Widget for MapsView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.ensure_started(cx);
        self.watch_pointer(cx, event);
        match event {
            Event::BackPressed { handled } => {
                if !handled.get() && self.back(cx) {
                    handled.set(true);
                }
            }
            Event::LocationUpdate(fix) => self.on_location_update(cx, fix),
            Event::LocationError(error) => {
                let why = match error {
                    LocationErrorEvent::PermissionDenied => "Location is off for OctosMap",
                    LocationErrorEvent::Unavailable(_) => "Could not find your location",
                };
                self.fail_location(cx, why);
            }
            Event::NetworkResponses(responses) => {
                for response in responses {
                    match response {
                        NetworkResponse::HttpResponse {
                            request_id,
                            response,
                        } => {
                            let body = body_text(response.status_code, response.body.as_deref())
                                .map(str::to_string);
                            self.handle_reply(cx, *request_id, body);
                        }
                        NetworkResponse::HttpError { request_id, error } => {
                            self.handle_reply(cx, *request_id, Err(plain_error(&error.message)));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if self
            .location_timeout
            .as_ref()
            .is_some_and(|t| t.is_event(event).is_some())
        {
            self.location_timeout = None;
            self.fail_location(cx, "Could not find your location");
        }
        if self
            .status_timer
            .as_ref()
            .is_some_and(|t| t.is_event(event).is_some())
        {
            self.status_timer = None;
            self.hide_status(cx);
        }
        if self
            .search_timer
            .as_ref()
            .is_some_and(|t| t.is_event(event).is_some())
        {
            self.search_timer = None;
            self.search_now(cx);
        }
        if self.sheet_frame.is_event(event).is_some() {
            self.ease_sheet(cx);
        }
        if let Some(frame) = self.nav_frame.is_event(event) {
            self.tick_preview(cx, frame.time);
        }
        if let Event::Actions(actions) = event {
            if self.layers_open {
                self.handle_layers_actions(cx, actions);
            } else {
                match self.model.screen() {
                    Screen::Explore => self.handle_explore_actions(cx, actions),
                    Screen::Search { .. } => self.handle_search_actions(cx, actions),
                    Screen::Place => self.handle_place_actions(cx, actions),
                    Screen::Directions => self.handle_directions_actions(cx, actions),
                    Screen::Navigating | Screen::Arrived => self.handle_nav_actions(cx, actions),
                }
            }
            self.handle_map_actions(cx, actions);
        }
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.ensure_started(cx);
        // The host re-ran the module for a new skin: the map follows it,
        // unless the person chose.
        self.apply_map_theme(cx);
        let size = cx.turtle().rect().size;
        // A zero-size first frame says nothing about the room.
        if size.x >= 1.0 && size.y >= 1.0 && self.viewport != (size.x, size.y) {
            self.viewport = (size.x, size.y);
            if !self.sheet.is_dragging() {
                self.sheet_height = self.sheet.height(size.y);
                self.set_sheet_height(cx);
            }
        }
        let results = self.view.portal_list(cx, ids!(results)).widget_uid();
        let steps = self.view.portal_list(cx, ids!(steps)).widget_uid();
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            let uid = step.widget_uid();
            if let Some(mut list) = step.as_portal_list().borrow_mut() {
                if uid == results {
                    self.draw_results(cx, &mut list);
                } else if uid == steps {
                    self.draw_steps(cx, &mut list);
                }
            }
        }
        if self.layers_open {
            // After the draw: a toggle's animator comes up in its default
            // state on its first draw, which would undo a state set before.
            self.sync_switches(cx);
        }
        if self.focus_search && matches!(self.model.screen(), Screen::Search { .. }) {
            // Drawn now: the field has an area for the focus to land on.
            self.focus_search = false;
            self.view.text_input(cx, ids!(search)).take_key_focus(cx);
        }
        DrawStep::done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::isolate_root;

    const SEARCH_REPLY: &str = include_str!("../tests/fixtures/photon-search.json");
    const REVERSE_REPLY: &str = include_str!("../tests/fixtures/photon-reverse.json");
    const CAR: &str = include_str!("../tests/fixtures/osrm-car.json");
    const BIKE: &str = include_str!("../tests/fixtures/osrm-bike.json");
    const FOOT: &str = include_str!("../tests/fixtures/osrm-foot.json");
    const NO_ROUTE: &str = include_str!("../tests/fixtures/osrm-noroute.json");
    const SAN_JOSE: LonLat = LonLat {
        lon: -121.8863,
        lat: 37.3382,
    };

    /// A `MapsView` in an isolate of its own, started, handed to `test`
    /// inside the isolate; torn down in the host's order afterwards. No
    /// network: nothing lands but what a test injects.
    fn with_view(test: impl FnOnce(&mut Cx, &WidgetRef)) {
        let (mut iso, root) = isolate_root();
        iso.entered(|cx| {
            root.handle_event(cx, &Event::Custom(String::new()), &mut Scope::empty());
            test(cx, &root);
            root.borrow_mut::<MapsView>().unwrap().shutdown(cx);
        });
        iso.teardown(root);
    }

    fn back_pressed(cx: &mut Cx, root: &WidgetRef) -> bool {
        // The answer is the event's own cell: a clone would be a copy.
        let event = Event::BackPressed {
            handled: std::cell::Cell::new(false),
        };
        root.handle_event(cx, &event, &mut Scope::empty());
        matches!(event, Event::BackPressed { handled } if handled.get())
    }

    fn fix_at(pos: LonLat) -> Event {
        Event::LocationUpdate(LocationUpdateEvent {
            lon: pos.lon,
            lat: pos.lat,
            accuracy_m: 12.0,
            altitude_m: None,
            speed_mps: None,
            heading_deg: None,
            time: 0.0,
        })
    }

    /// The one request of this sort in flight.
    fn in_flight(view: &MapsView, wanted: Request) -> LiveId {
        let ids: Vec<LiveId> = view
            .model()
            .in_flight()
            .into_iter()
            .filter(|(_, request)| *request == wanted)
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids.len(), 1, "{wanted:?} in flight");
        ids[0]
    }

    /// Type a query and land `reply` for it.
    fn search(cx: &mut Cx, view: &mut MapsView, query: &str, reply: Result<String, String>) {
        view.open_search(cx, SearchTarget::Destination);
        view.set_query(cx, query);
        view.search_now(cx);
        let id = in_flight(view, Request::Search);
        view.handle_reply(cx, id, reply);
    }

    #[test]
    fn the_view_starts_on_explore_with_its_map() {
        with_view(|cx, root| {
            let view = root.borrow::<MapsView>().unwrap();
            assert_eq!(view.model().screen(), Screen::Explore);
            assert!(view.view.widget(cx, ids!(explore)).visible());
            for hidden in [
                ids!(layers_layer),
                ids!(search_layer),
                ids!(place_layer),
                ids!(compass),
                ids!(locate_active),
            ] {
                assert!(!view.view.widget(cx, hidden).visible());
            }
            assert!(view.view.widget(cx, ids!(locate)).visible());
            assert_eq!(
                view.view.label(cx, ids!(attribution)).text(),
                "© OpenStreetMap contributors"
            );
            assert_eq!(view.status_text(cx), None);
        });
    }

    #[test]
    fn back_at_explore_is_the_hosts_and_closes_the_layers_sheet_first() {
        with_view(|cx, root| {
            assert!(!back_pressed(cx, root), "nothing to go back from");
            root.borrow_mut::<MapsView>().unwrap().open_layers(cx);
            assert!(root
                .borrow::<MapsView>()
                .unwrap()
                .view
                .widget(cx, ids!(layers_layer))
                .visible());
            assert!(back_pressed(cx, root));
            let view = root.borrow::<MapsView>().unwrap();
            assert!(!view.layers_open());
            assert!(!view.view.widget(cx, ids!(layers_layer)).visible());
        });
    }

    #[test]
    fn the_locate_button_waits_then_follows_the_fix() {
        with_view(|cx, root| {
            root.borrow_mut::<MapsView>().unwrap().ask_location(cx);
            {
                let view = root.borrow::<MapsView>().unwrap();
                assert_eq!(view.location(), LocationState::Waiting);
                assert_eq!(view.status_text(cx).as_deref(), Some("Locating…"));
            }
            root.handle_event(cx, &fix_at(SAN_JOSE), &mut Scope::empty());
            let view = root.borrow::<MapsView>().unwrap();
            assert_eq!(view.location(), LocationState::Active);
            assert_eq!(view.model().fix(), Some(SAN_JOSE));
            assert_eq!(view.status_text(cx), None, "found: nothing more to say");
            assert!(view.view.widget(cx, ids!(locate_active)).visible());
            assert!(!view.view.widget(cx, ids!(locate)).visible());
        });
    }

    #[test]
    fn a_location_error_says_why_once_and_lets_the_next_tap_retry() {
        with_view(|cx, root| {
            // An error nobody asked for says nothing.
            let denied = Event::LocationError(LocationErrorEvent::PermissionDenied);
            root.handle_event(cx, &denied, &mut Scope::empty());
            assert_eq!(root.borrow::<MapsView>().unwrap().status_text(cx), None);
            root.borrow_mut::<MapsView>().unwrap().ask_location(cx);
            root.handle_event(cx, &denied, &mut Scope::empty());
            {
                let view = root.borrow::<MapsView>().unwrap();
                assert_eq!(view.location(), LocationState::Idle);
                assert_eq!(
                    view.status_text(cx).as_deref(),
                    Some("Location is off for OctosMap")
                );
            }
            root.borrow_mut::<MapsView>().unwrap().ask_location(cx);
            assert_eq!(
                root.borrow::<MapsView>().unwrap().location(),
                LocationState::Waiting
            );
        });
    }

    #[test]
    fn a_fix_nobody_asked_for_moves_nothing() {
        with_view(|cx, root| {
            root.handle_event(cx, &fix_at(SAN_JOSE), &mut Scope::empty());
            let view = root.borrow::<MapsView>().unwrap();
            assert_eq!(view.location(), LocationState::Idle);
            assert_eq!(view.model().fix(), None);
        });
    }

    #[test]
    fn the_list_says_what_the_search_is_doing() {
        let place = |i| ResultItem::Place(i);
        let status = |text: &str, retry| ResultItem::Status {
            text: text.into(),
            retry,
        };
        assert_eq!(
            result_items(&SearchState::Idle, 0, ""),
            vec![status("Search for a place or an address", false)]
        );
        assert_eq!(
            result_items(&SearchState::Loading, 0, "santa"),
            vec![status("Searching…", false)]
        );
        // The last list stays up while the next one loads.
        assert_eq!(
            result_items(&SearchState::Loading, 2, "santa"),
            vec![place(0), place(1)]
        );
        assert_eq!(
            result_items(&SearchState::Done, 3, "santa"),
            vec![place(0), place(1), place(2)]
        );
        assert_eq!(
            result_items(&SearchState::Done, 0, " zzzz "),
            vec![status("No results for “zzzz”", false)]
        );
        assert_eq!(
            result_items(&SearchState::Failed("timed out".into()), 3, "santa"),
            vec![status("Couldn't search: timed out", true)]
        );
    }

    #[test]
    fn a_search_fills_the_list_and_a_pick_opens_the_place_with_its_sheet() {
        with_view(|cx, root| {
            let mut view = root.borrow_mut::<MapsView>().unwrap();
            search(
                cx,
                &mut view,
                "santa clara university",
                Ok(SEARCH_REPLY.into()),
            );
            assert!(view.view.widget(cx, ids!(search_layer)).visible());
            assert!(!view.view.widget(cx, ids!(explore)).visible());
            assert!(view.view.widget(cx, ids!(search_clear)).visible());
            assert_eq!(
                view.items(),
                &[
                    ResultItem::Place(0),
                    ResultItem::Place(1),
                    ResultItem::Place(2)
                ]
            );
            view.pick_result(cx, 0);
            assert_eq!(view.model().screen(), Screen::Place);
            assert!(view.view.widget(cx, ids!(place_layer)).visible());
            assert!(!view.view.widget(cx, ids!(search_layer)).visible());
            assert_eq!(view.sheet_detent(), Detent::Peek);
            assert_eq!(
                view.view.label(cx, ids!(sheet_title)).text(),
                "Santa Clara University"
            );
            assert_eq!(
                view.view.label(cx, ids!(place_pill_text)).text(),
                "Santa Clara University"
            );
            // No fix yet: the category alone, no distance.
            assert_eq!(
                view.view.label(cx, ids!(sheet_subtitle)).text(),
                "University"
            );
            let address = view.view.widget(cx, ids!(address_row));
            assert!(address.visible());
            assert_eq!(
                address.label(cx, ids!(value)).text(),
                "500 El Camino Real, Santa Clara, California"
            );
            assert!(!view.view.widget(cx, ids!(detail_0)).visible());
        });
    }

    #[test]
    fn a_fix_puts_the_distance_on_the_sheet_in_the_places_units() {
        with_view(|cx, root| {
            root.borrow_mut::<MapsView>().unwrap().ask_location(cx);
            root.handle_event(cx, &fix_at(SAN_JOSE), &mut Scope::empty());
            let mut view = root.borrow_mut::<MapsView>().unwrap();
            search(
                cx,
                &mut view,
                "santa clara university",
                Ok(SEARCH_REPLY.into()),
            );
            view.pick_result(cx, 0);
            // 4.6 km as the crow flies, in a country that reads miles.
            assert_eq!(
                view.view.label(cx, ids!(sheet_subtitle)).text(),
                "University · 2.9 mi"
            );
        });
    }

    #[test]
    fn a_failed_search_offers_a_retry_that_asks_again() {
        with_view(|cx, root| {
            let mut view = root.borrow_mut::<MapsView>().unwrap();
            search(cx, &mut view, "santa clara", Err("timed out".into()));
            assert_eq!(
                view.items(),
                &[ResultItem::Status {
                    text: "Couldn't search: timed out".into(),
                    retry: true
                }]
            );
            view.search_now(cx);
            let id = in_flight(&view, Request::Search);
            view.handle_reply(cx, id, Ok(SEARCH_REPLY.into()));
            assert_eq!(view.items().len(), 3);
        });
    }

    #[test]
    fn a_long_press_drops_a_pin_that_the_lookup_names() {
        with_view(|cx, root| {
            let mut view = root.borrow_mut::<MapsView>().unwrap();
            view.drop_pin(cx, SAN_JOSE);
            assert_eq!(view.model().screen(), Screen::Place);
            assert_eq!(view.view.label(cx, ids!(sheet_title)).text(), "Dropped pin");
            let id = in_flight(&view, Request::Reverse);
            view.handle_reply(cx, id, Ok(REVERSE_REPLY.into()));
            assert_eq!(
                view.view.label(cx, ids!(sheet_title)).text(),
                "East Santa Clara Street"
            );
            assert_eq!(view.view.label(cx, ids!(sheet_subtitle)).text(), "Street");
        });
    }

    /// A view on Directions to the university, with a fix or without.
    fn to_directions(cx: &mut Cx, root: &WidgetRef, fix: bool) {
        if fix {
            root.borrow_mut::<MapsView>().unwrap().ask_location(cx);
            root.handle_event(cx, &fix_at(SAN_JOSE), &mut Scope::empty());
        }
        let mut view = root.borrow_mut::<MapsView>().unwrap();
        search(
            cx,
            &mut view,
            "santa clara university",
            Ok(SEARCH_REPLY.into()),
        );
        view.pick_result(cx, 0);
        view.open_directions(cx);
    }

    /// The route requests in flight, by mode.
    fn routes_in_flight(view: &MapsView) -> Vec<Mode> {
        view.model()
            .in_flight()
            .into_iter()
            .filter_map(|(_, request)| match request {
                Request::Route(mode) => Some(mode),
                _ => None,
            })
            .collect()
    }

    fn land_route(cx: &mut Cx, view: &mut MapsView, mode: Mode, body: &str) {
        let id = in_flight(view, Request::Route(mode));
        view.handle_reply(cx, id, Ok(body.to_string()));
    }

    #[test]
    fn directions_ask_for_one_route_at_a_time_and_show_each_tabs_time() {
        with_view(|cx, root| {
            to_directions(cx, root, true);
            let mut view = root.borrow_mut::<MapsView>().unwrap();
            assert_eq!(view.model().screen(), Screen::Directions);
            assert!(view.view.widget(cx, ids!(directions_layer)).visible());
            assert!(!view.view.widget(cx, ids!(place_layer)).visible());
            assert_eq!(
                view.view.label(cx, ids!(origin_text)).text(),
                "Your location"
            );
            assert_eq!(
                view.view.label(cx, ids!(destination_text)).text(),
                "Santa Clara University"
            );
            // The shown tab's route first, and only it.
            assert_eq!(routes_in_flight(&view), vec![Mode::Car]);
            assert_eq!(
                view.view.label(cx, ids!(route_title)).text(),
                "Finding the best route…"
            );
            assert!(view.view.widget(cx, ids!(mode_car_on)).visible());
            assert!(!view.view.widget(cx, ids!(mode_car)).visible());
            assert!(view.view.widget(cx, ids!(mode_walk)).visible());
            land_route(cx, &mut view, Mode::Car, CAR);
            assert_eq!(view.view.label(cx, ids!(route_title)).text(), "10 min");
            assert_eq!(
                view.view.label(cx, ids!(route_subtitle)).text(),
                "4.8 mi · Drive"
            );
            assert_eq!(view.view.button(cx, ids!(mode_car_on)).text(), "10 min");
            // Its reply sent the next tab's.
            assert_eq!(routes_in_flight(&view), vec![Mode::Walk]);
            assert_eq!(view.view.button(cx, ids!(mode_walk)).text(), "…");
            land_route(cx, &mut view, Mode::Walk, FOOT);
            land_route(cx, &mut view, Mode::Bike, BIKE);
            assert!(routes_in_flight(&view).is_empty());
            assert_eq!(view.view.button(cx, ids!(mode_bike)).text(), "35 min");
            // Another tab shows its own route, without asking again.
            view.set_mode(cx, Mode::Walk);
            assert_eq!(view.view.label(cx, ids!(route_title)).text(), "1 hr 44 min");
            assert!(view.view.widget(cx, ids!(mode_walk_on)).visible());
            assert!(routes_in_flight(&view).is_empty());
        });
    }

    #[test]
    fn no_route_marks_its_tab_only_and_offers_a_retry() {
        with_view(|cx, root| {
            to_directions(cx, root, true);
            let mut view = root.borrow_mut::<MapsView>().unwrap();
            land_route(cx, &mut view, Mode::Car, NO_ROUTE);
            assert_eq!(
                view.view.label(cx, ids!(route_title)).text(),
                "No route found"
            );
            assert!(view.view.widget(cx, ids!(route_retry)).visible());
            assert_eq!(view.view.button(cx, ids!(mode_car_on)).text(), "—");
            land_route(cx, &mut view, Mode::Walk, FOOT);
            assert_eq!(view.view.button(cx, ids!(mode_walk)).text(), "1 hr 44 min");
            view.set_mode(cx, Mode::Walk);
            assert!(!view.view.widget(cx, ids!(route_retry)).visible());
        });
    }

    #[test]
    fn without_a_fix_directions_wait_for_one_or_for_a_chosen_start() {
        with_view(|cx, root| {
            to_directions(cx, root, false);
            {
                let view = root.borrow::<MapsView>().unwrap();
                // Directions asked for the location itself.
                assert_eq!(view.location(), LocationState::Waiting);
                assert!(routes_in_flight(&view).is_empty());
                assert_eq!(
                    view.view.label(cx, ids!(route_title)).text(),
                    "Finding your location…"
                );
            }
            root.handle_event(cx, &fix_at(SAN_JOSE), &mut Scope::empty());
            assert_eq!(
                routes_in_flight(&root.borrow::<MapsView>().unwrap()),
                vec![Mode::Car]
            );
        });
        with_view(|cx, root| {
            to_directions(cx, root, false);
            let denied = Event::LocationError(LocationErrorEvent::PermissionDenied);
            root.handle_event(cx, &denied, &mut Scope::empty());
            let mut view = root.borrow_mut::<MapsView>().unwrap();
            assert_eq!(
                view.view.label(cx, ids!(route_title)).text(),
                "Choose a starting point"
            );
            // The origin row's search picks one.
            view.open_search(cx, SearchTarget::Origin);
            view.set_query(cx, "library");
            view.search_now(cx);
            let id = in_flight(&view, Request::Search);
            view.handle_reply(cx, id, Ok(SEARCH_REPLY.into()));
            view.pick_result(cx, 1);
            assert_eq!(view.model().screen(), Screen::Directions);
            assert_eq!(
                view.view.label(cx, ids!(origin_text)).text(),
                "Santa Clara University Library"
            );
            assert_eq!(routes_in_flight(&view), vec![Mode::Car]);
        });
    }

    #[test]
    fn swapping_the_ends_asks_again_the_other_way() {
        with_view(|cx, root| {
            to_directions(cx, root, true);
            let mut view = root.borrow_mut::<MapsView>().unwrap();
            land_route(cx, &mut view, Mode::Car, CAR);
            view.swap_ends(cx);
            assert_eq!(
                view.view.label(cx, ids!(origin_text)).text(),
                "Santa Clara University"
            );
            assert_eq!(
                view.view.label(cx, ids!(destination_text)).text(),
                "Your location"
            );
            // The walk that was in flight was for the old ends.
            assert_eq!(routes_in_flight(&view), vec![Mode::Car]);
            assert_eq!(
                view.view.label(cx, ids!(route_title)).text(),
                "Finding the best route…"
            );
        });
    }

    #[test]
    fn back_from_directions_is_the_place_again() {
        with_view(|cx, root| {
            to_directions(cx, root, true);
            assert!(back_pressed(cx, root));
            let view = root.borrow::<MapsView>().unwrap();
            assert_eq!(view.model().screen(), Screen::Place);
            assert!(view.view.widget(cx, ids!(place_layer)).visible());
            assert!(!view.view.widget(cx, ids!(directions_layer)).visible());
        });
    }

    /// Directions with the car's route landed and nothing else in flight.
    fn with_a_route(cx: &mut Cx, root: &WidgetRef) {
        to_directions(cx, root, true);
        let mut view = root.borrow_mut::<MapsView>().unwrap();
        land_route(cx, &mut view, Mode::Car, CAR);
        land_route(cx, &mut view, Mode::Walk, FOOT);
        land_route(cx, &mut view, Mode::Bike, BIKE);
    }

    #[test]
    fn start_and_preview_need_a_route() {
        with_view(|cx, root| {
            to_directions(cx, root, true);
            let mut view = root.borrow_mut::<MapsView>().unwrap();
            assert!(!view.view.widget(cx, ids!(start)).visible());
            view.start_navigation(cx, true);
            assert_eq!(
                view.model().screen(),
                Screen::Directions,
                "nothing to drive yet"
            );
            land_route(cx, &mut view, Mode::Car, CAR);
            assert!(view.view.widget(cx, ids!(start)).visible());
            assert!(view.view.widget(cx, ids!(preview)).visible());
        });
    }

    #[test]
    fn a_preview_drives_the_banner_to_the_arrival_and_done_is_the_place() {
        with_view(|cx, root| {
            with_a_route(cx, root);
            {
                let mut view = root.borrow_mut::<MapsView>().unwrap();
                view.start_navigation(cx, true);
                assert_eq!(view.model().screen(), Screen::Navigating);
                assert!(view.navigating());
                assert!(view.view.widget(cx, ids!(nav_layer)).visible());
                assert!(!view.view.widget(cx, ids!(directions_layer)).visible());
                assert!(view.view.widget(cx, ids!(end)).visible());
                assert!(!view.view.widget(cx, ids!(done)).visible());
                assert!(!view.view.widget(cx, ids!(recenter)).visible(), "chasing");
                // The first turn, and how far it is.
                assert_eq!(
                    view.view.label(cx, ids!(banner_text)).text(),
                    "Turn right onto North 5th Street"
                );
                assert_eq!(view.view.label(cx, ids!(nav_time)).text(), "10 min");
                let banner = view.view.widget(cx, ids!(banner));
                assert!(banner.widget(cx, ids!(right)).visible());
                assert!(!banner.widget(cx, ids!(left)).visible());
                // Half-second frames to the end: ninety seconds at the most.
                let mut time = 0.0;
                for _ in 0..400 {
                    time += 0.5;
                    view.tick_preview(cx, time);
                    if view.model().screen() == Screen::Arrived {
                        break;
                    }
                }
                assert_eq!(view.model().screen(), Screen::Arrived);
                assert_eq!(
                    view.view.label(cx, ids!(banner_text)).text(),
                    "You have arrived"
                );
                assert_eq!(view.view.label(cx, ids!(nav_time)).text(), "Arrived");
                assert!(view.view.widget(cx, ids!(done)).visible());
                assert!(!view.view.widget(cx, ids!(end)).visible());
            }
            assert!(back_pressed(cx, root), "Done, or the back gesture");
            let view = root.borrow::<MapsView>().unwrap();
            assert_eq!(view.model().screen(), Screen::Place);
            assert!(!view.navigating());
            assert!(view.view.widget(cx, ids!(place_layer)).visible());
            assert!(!view.view.widget(cx, ids!(nav_layer)).visible());
        });
    }

    #[test]
    fn end_goes_back_to_the_routes() {
        with_view(|cx, root| {
            with_a_route(cx, root);
            root.borrow_mut::<MapsView>()
                .unwrap()
                .start_navigation(cx, true);
            assert!(back_pressed(cx, root));
            let view = root.borrow::<MapsView>().unwrap();
            assert_eq!(view.model().screen(), Screen::Directions);
            assert!(!view.navigating());
            assert!(view.view.widget(cx, ids!(directions_layer)).visible());
            // Nothing was rerouted: the routes are still there.
            assert_eq!(view.view.label(cx, ids!(route_title)).text(), "10 min");
            assert!(routes_in_flight(&view).is_empty());
        });
    }

    #[test]
    fn the_chase_camera_leads_the_puck_the_way_it_is_going() {
        let at = LonLat::new(-121.9, 37.3);
        let north = lead(at, 0.0, 60.0);
        assert!((haversine_m(at, north) - 60.0).abs() < 0.5);
        assert!(north.lat > at.lat && (north.lon - at.lon).abs() < 1e-9);
        let east = lead(at, 90.0, 60.0);
        assert!((haversine_m(at, east) - 60.0).abs() < 0.5);
        assert!(east.lon > at.lon && (east.lat - at.lat).abs() < 1e-9);
        assert_eq!(lead(at, 200.0, 0.0), at);
    }

    #[test]
    fn a_pan_pauses_the_chase_and_recenter_resumes_it() {
        with_view(|cx, root| {
            with_a_route(cx, root);
            let mut view = root.borrow_mut::<MapsView>().unwrap();
            view.start_navigation(cx, true);
            assert!(view.chasing);
            // A finger on the bare map that barely moves is a tap.
            view.map_press = Some(dvec2(200.0, 400.0));
            view.pointer_moved(cx, dvec2(203.0, 402.0));
            assert!(view.chasing);
            // One that drags is a pan.
            view.pointer_moved(cx, dvec2(240.0, 400.0));
            assert!(!view.chasing);
            assert!(view.view.widget(cx, ids!(recenter)).visible());
            // No press, no pan: a move over the map is not a drag.
            view.recenter(cx);
            assert!(view.chasing);
            assert!(!view.view.widget(cx, ids!(recenter)).visible());
            view.pointer_moved(cx, dvec2(300.0, 300.0));
            assert!(view.chasing);
        });
    }

    #[test]
    fn a_live_drive_off_the_route_asks_for_a_new_one_once_and_takes_it() {
        with_view(|cx, root| {
            with_a_route(cx, root);
            root.borrow_mut::<MapsView>()
                .unwrap()
                .start_navigation(cx, false);
            // A kilometre south of the route, second after second.
            let astray = LonLat::new(SAN_JOSE.lon, SAN_JOSE.lat - 0.01);
            for second in 1..=8 {
                let mut event = fix_at(astray);
                if let Event::LocationUpdate(fix) = &mut event {
                    fix.time = 1_000.0 + second as f64;
                }
                root.handle_event(cx, &event, &mut Scope::empty());
            }
            let mut view = root.borrow_mut::<MapsView>().unwrap();
            let id = in_flight(&view, Request::Reroute);
            assert_eq!(view.view.label(cx, ids!(banner_text)).text(), "Rerouting…");
            view.handle_reply(cx, id, Ok(CAR.into()));
            assert!(!view.rerouting);
            assert!(view.model().in_flight().is_empty());
            assert!(view.navigating());
        });
    }

    #[test]
    fn back_walks_from_the_place_to_explore_and_cancels_what_it_left() {
        with_view(|cx, root| {
            root.borrow_mut::<MapsView>()
                .unwrap()
                .drop_pin(cx, SAN_JOSE);
            // Into Search from the place's pill, and back out to the place.
            root.borrow_mut::<MapsView>()
                .unwrap()
                .open_search(cx, SearchTarget::Destination);
            assert!(back_pressed(cx, root));
            assert_eq!(
                root.borrow::<MapsView>().unwrap().model().screen(),
                Screen::Place
            );
            assert!(back_pressed(cx, root));
            {
                let view = root.borrow::<MapsView>().unwrap();
                assert_eq!(view.model().screen(), Screen::Explore);
                assert!(view.view.widget(cx, ids!(explore)).visible());
                assert!(!view.view.widget(cx, ids!(place_layer)).visible());
                assert!(
                    view.model().in_flight().is_empty(),
                    "the lookup went with its pin"
                );
            }
            assert!(!back_pressed(cx, root), "the host's turn");
        });
    }
}
