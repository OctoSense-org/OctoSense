//! The news surface, after Apple News: one `PortalList` that shows the
//! current page — Today (a section per followed source), Following (the
//! sources, on or off, and the add-a-feed form), Saved, Search, or one
//! source's own page — under a floating glass bottom bar, with the reader
//! over everything when a story is open. Both faces (the full page and the
//! wide home tile the host switches to with `HostedViewMode` over
//! `Event::Custom`; the `HostedView` child reads it and swaps faces on its
//! own) draw in the app's skin: Apple News' light look, or its dark
//! counterpart when the host is dark (`Skin`). The fetches, the tick timer,
//! the storage jail (the feeds file, the hidden ids, the saved stories, the
//! per-source cache) and the opener chain (the reader first, then the
//! bundled Browser through the host, the system browser, a notification:
//! `OpenPolicy`) are the view's too. `ensure_started` seeds everything on
//! the first event or draw, whichever the host gives it first: a window has
//! a `Startup` event, a module instance does not.

use crate::model::{
    body_text, builtin_count, date_line, deck_text, host_of, initial, is_http_url, meta_line, parse_body, parse_user_feeds,
    plain_error, section_caption, source_color, tile_line, Headline, Hosting, Nav, NewsModel, OpenPolicy, OpenTier, Page, Pushed, Root, Skin, WmUnavailable,
    FEEDS_KEY, HIDDEN_KEY, MAX_BODY_BYTES, SAVED_KEY,
};
use crate::reader::{ArticleReader, ReaderAction};
use makepad_widgets::image_cache::{handle_image_cache_network_responses, ImageCache};
use makepad_widgets::makepad_platform::storage::{StorageHandle, StorageRequestId, StorageResponse, StorageResult};
use makepad_widgets::*;
use makepad_wm_api::WmRequest;
use std::collections::HashMap;
use std::path::Path;

/// Below this height the tile drops its heading to keep three headlines.
/// Below roughly 70 pt (the 60 pt banner floor on the smallest Android homes
/// with five tile apps) the third headline clips; no shipping phone size
/// reaches it.
const SHORT_TILE: f64 = 96.0;
/// How often a picture that the cache forgot (a failed fetch, an eviction)
/// is asked for again before the row gives up on it.
const IMAGE_ATTEMPTS: u8 = 2;

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
    let c_clear = #00000000
    // Fixed amber for errors, readable on both grounds.
    let c_warn = #e08a2c

    // Apple News sets its titles heavy; Inter's variable weight gives it.
    let Heavy = theme.font_bold{
        font_family +: {latin := FontMember{res: crate_resource("makepad_widgets:resources/Inter.ttf") weight: 800.0 asc: 0.0 desc: 0.0}}
    }

    // Text roles, all without `Label`'s own padding so rows are as tall as
    // their text.
    let Text = Label{padding: 0 draw_text +: {color: c_ink text_style: theme.font_regular{font_size: 13}}}
    let PageTitleText = Text{width: Fill draw_text.text_style: Heavy{font_size: 30}}
    let SectionText = Text{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.text_style: Heavy{font_size: 20}}
    let HeroHeadline = Text{width: Fill max_lines: 3 text_overflow: Ellipsis draw_text.text_style: Heavy{font_size: 20 line_spacing: 1.15}}
    let Headline = Text{width: Fill max_lines: 3 text_overflow: Ellipsis draw_text.text_style: theme.font_bold{font_size: 15 line_spacing: 1.2}}
    let Publisher = Text{max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_bold{font_size: 12}}
    let Deck = Text{width: Fill max_lines: 2 text_overflow: Ellipsis draw_text +: {color: c_secondary text_style: theme.font_regular{font_size: 13 line_spacing: 1.25}}}
    let Meta = Text{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text +: {color: c_secondary text_style: theme.font_regular{font_size: 11}}}
    let Caption = Text{width: Fill max_lines: 2 draw_text +: {color: c_secondary text_style: theme.font_bold{font_size: 13}}}
    let Hairline = SolidView{width: Fill height: 0.5 draw_bg.color: c_hairline}
    // A source's dot: slate until `render` colours it.
    let Dot = RoundedView{width: 8 height: 8 draw_bg +: {color: #7d8aa5 border_radius: 4.0}}

    // A group of stories is one card; each row is a segment of it. The
    // first segment rounds the top corners, the last the bottom ones. A
    // square corner is a hair's radius, never zero: the per-corner box
    // distance field misfills with a zero radius.
    let Segment = RoundedAllView{width: Fill height: Fit flow: Down show_bg: true draw_bg +: {color: c_card border_radius: vec4(0.5)}}
    let SegmentTop = Segment{draw_bg.border_radius: vec4(14.0, 14.0, 0.5, 0.5)}
    let SegmentBottom = Segment{draw_bg.border_radius: vec4(0.5, 0.5, 14.0, 14.0)}
    let Card = Segment{draw_bg.border_radius: vec4(14.0)}

    // A button with no face of its own: an icon, or a line of text.
    let Plain = ButtonFlat{padding: 0 margin: 0 text: "" spacing: 0 align: Center
        draw_bg +: {
            border_size: uniform(0.0)
            color: uniform(c_clear) color_hover: uniform(c_clear) color_down: uniform(#00000018) color_focus: uniform(c_clear)
            border_color: uniform(c_clear) border_color_hover: uniform(c_clear) border_color_down: uniform(c_clear) border_color_focus: uniform(c_clear)
        }
    }
    let MoreButton = Plain{width: 32 height: 32 icon_walk: Walk{width: 18 height: 18}
        draw_icon +: {svg: crate_resource("self:resources/icons/more.svg") color: c_secondary}}
    let ChevronButton = Plain{width: 30 height: 30 icon_walk: Walk{width: 14 height: 14}
        draw_icon +: {svg: crate_resource("self:resources/icons/chevron-right.svg") color: c_secondary}}
    let RemoveButton = Plain{width: 32 height: 32 icon_walk: Walk{width: 13 height: 13}
        draw_icon +: {svg: crate_resource("self:resources/icons/close.svg") color: c_secondary}}
    // A line of text in the accent: Cancel, the sheet's rows.
    let TextButton = Plain{width: Fit height: 40 padding: Inset{left: 8 right: 8}
        draw_text +: {color: c_accent color_hover: c_accent color_down: c_accent color_focus: c_accent text_style: theme.font_regular{font_size: 15}}}
    let SheetButton = Plain{width: Fill height: 54
        draw_text +: {color: c_ink color_hover: c_ink color_down: c_ink color_focus: c_ink text_style: theme.font_regular{font_size: 17}}}
    // The round glass buttons that float over a page: a 44 pt lens with
    // its icon centred, in the skin's ink.
    let RoundButton = glass.IconButton{width: 44 height: 44 text: "" spacing: 0 padding: 0 align: Center
        icon_walk: Walk{width: 18 height: 18}
        draw_bg +: {border_radius: uniform(22.0)}
        draw_icon +: {color: c_ink}}
    let BackButton = RoundButton{draw_icon.svg: crate_resource("self:resources/icons/chevron-left.svg")}
    let RefreshButton = RoundButton{draw_icon.svg: crate_resource("self:resources/icons/refresh.svg")}
    let SearchButton = RoundButton{width: 52 height: 52 icon_walk: Walk{width: 20 height: 20}
        draw_bg +: {border_radius: uniform(26.0)}
        draw_icon.svg: crate_resource("self:resources/icons/search.svg")}
    // A tab of the bottom bar: its icon over its label, both centred, both
    // tinted by `render` (the accent when selected).
    let TabButton = Plain{width: Fill height: Fill flow: Down spacing: 3 align: Center
        icon_walk: Walk{width: 22 height: 22}
        draw_icon +: {color: c_secondary}
        draw_text +: {color: c_secondary color_hover: c_secondary color_down: c_secondary color_focus: c_secondary text_style: theme.font_bold{font_size: 10}}}
    // The follow switch: a pill in the accent when on, the field colour
    // when off, a white knob, big enough for a thumb.
    let FollowToggle = Toggle{text: "" margin: 0 padding: 0 width: 42 height: 26
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
    // A text field on a card: the skin's field colour, the accent ring when
    // focused.
    let Field = TextInputFlat{width: Fill height: 40 margin: 0 padding: Inset{left: 12 right: 12 top: 10 bottom: 10} empty_text: "https://example.com/feed.xml"
        draw_bg +: {
            border_radius: 10.0 border_size: 1.0
            color: c_field color_hover: c_field color_focus: c_field color_empty: c_field
            border_color: c_clear border_color_hover: c_clear border_color_focus: c_accent border_color_empty: c_clear
        }
        draw_text +: {
            color: c_ink color_hover: c_ink color_focus: c_ink
            color_empty: c_secondary color_empty_hover: c_secondary color_empty_focus: c_secondary
            text_style: theme.font_regular{font_size: 14}
        }
    }

    // The first line of a story: the source's dot and the publisher.
    let PublisherLine = View{width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
        dot := Dot{}
        publisher := Publisher{}
    }
    // The last line: the meta text and the `•••` button, centred on it.
    let MetaLine = View{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
        meta := Meta{}
        more := MoreButton{}
    }
    // One list item's frame: the page's 16 pt gutters, no vertical margin,
    // so the segments of a group touch.
    let Item = View{width: Fill height: Fit flow: Down padding: Inset{left: 16 right: 16}}

    let TitleRow = Item{padding: Inset{left: 16 right: 16 top: 14 bottom: 6} spacing: 2
        title := PageTitleText{}
        caption := Caption{}
    }
    // The title of a pushed page starts under the floating back button.
    let TitleBelowBarRow = TitleRow{padding: Inset{left: 16 right: 16 top: 62 bottom: 6}}
    let SectionRow = Item{padding: Inset{left: 16 right: 16 top: 22 bottom: 10}
        header := View{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5} cursor: MouseCursor.Hand
            View{width: Fill height: Fit flow: Down spacing: 2
                name := SectionText{}
                caption := Caption{draw_text.text_style: theme.font_regular{font_size: 12}}
            }
            open := ChevronButton{}
        }
    }
    // A section's first story: the picture when the feed gives one, the
    // publisher, a heavy headline, the deck, the meta line.
    let HeroRow = Item{
        card := SegmentTop{cursor: MouseCursor.Hand padding: Inset{left: 14 right: 14 top: 14 bottom: 10} spacing: 8
            image := Image{width: Fill height: 190 fit: ImageFit.CropToFill margin: Inset{bottom: 4}}
            PublisherLine{}
            headline := HeroHeadline{}
            deck := Deck{}
            MetaLine{}
        }
    }
    // The stories after the hero: a hairline above, a thumbnail at the
    // right when the feed gives one.
    let CompactRow = Item{
        card := Segment{cursor: MouseCursor.Hand padding: Inset{left: 14 right: 14 top: 0 bottom: 10}
            hairline := Hairline{}
            View{width: Fill height: Fit flow: Right spacing: 12 padding: Inset{top: 12}
                View{width: Fill height: Fit flow: Down spacing: 6
                    PublisherLine{}
                    headline := Headline{}
                    MetaLine{}
                }
                thumb := Image{width: 72 height: 72 fit: ImageFit.CropToFill}
            }
        }
    }
    // `More from …`, closing a Today section's group.
    let FooterRow = Item{
        card := SegmentBottom{cursor: MouseCursor.Hand padding: Inset{left: 14 right: 14 top: 0 bottom: 14}
            Hairline{}
            View{width: Fill height: Fit flow: Right align: Align{y: 0.5} padding: Inset{top: 12}
                label := Text{width: Fill draw_text +: {color: c_accent text_style: theme.font_bold{font_size: 14}}}
                Icon{width: 14 height: 14 icon_walk: Walk{width: 14 height: 14}
                    draw_icon +: {svg: crate_resource("self:resources/icons/chevron-right.svg") color: c_accent}}
            }
        }
    }
    // The caps of a group that has no hero or footer of its own.
    let CapTopRow = Item{SegmentTop{height: 12}}
    let CapBottomRow = Item{SegmentBottom{height: 12}}
    let StatusRow = Item{padding: Inset{left: 16 right: 16 top: 4 bottom: 8}
        text := Caption{draw_text +: {color: c_warn text_style: theme.font_regular{font_size: 12}}}
    }
    let EmptyRow = Item{padding: Inset{left: 32 right: 32 top: 70 bottom: 40}
        text := Caption{max_lines: 3 align: Align{x: 0.5} draw_text +: {text_style: theme.font_regular{font_size: 15}}}
    }
    // Room under the last row for the floating bar.
    let SpacerRow = View{width: Fill height: 112}
    // A source on the Following page: its monogram, its name and host, a
    // remove button on the person's own feeds, and the follow toggle.
    let SourceRow = Item{
        card := Segment{padding: Inset{left: 14 right: 10}
            hairline := Hairline{}
            row := View{width: Fill height: Fit flow: Right spacing: 12 align: Align{y: 0.5} padding: Inset{top: 10 bottom: 10} cursor: MouseCursor.Hand
                mono := RoundedView{width: 40 height: 40 align: Center draw_bg +: {color: #7d8aa5 border_radius: 20.0}
                    initial := Label{padding: 0 draw_text +: {color: #ffffff text_style: theme.font_bold{font_size: 17}}}
                }
                View{width: Fill height: Fit flow: Down spacing: 2
                    label := Text{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_bold{font_size: 15}}
                    host := Meta{}
                }
                remove := RemoveButton{}
                follow := FollowToggle{}
            }
        }
    }
    let AddFeedRow = Item{padding: Inset{left: 16 right: 16 top: 22 bottom: 10} spacing: 4
        SectionText{text: "Add a feed"}
        Caption{text: "RSS or Atom. Up to four feeds." draw_text.text_style: theme.font_regular{font_size: 12}}
        card := Card{padding: 14 spacing: 10 margin: Inset{top: 8}
            url := Field{}
            name := Field{empty_text: "Name (optional)"}
            View{width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5}
                error := Meta{max_lines: 2 draw_text.color: c_warn}
                add := Plain{width: Fit height: 40 text: "Add feed" padding: Inset{left: 18 right: 18}
                    draw_bg +: {color: uniform(c_accent) color_hover: uniform(c_accent) color_down: uniform(c_accent) color_focus: uniform(c_accent) border_radius: uniform(10.0)}
                    draw_text +: {color: #ffffff color_hover: #ffffff color_down: #ffffff color_focus: #ffffff text_style: theme.font_bold{font_size: 14}}}
            }
        }
    }

    // One tile line: the source's dot (slate until `render` colours it)
    // and the headline.
    let TileLine = View{width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
        dot := Dot{width: 7 height: 7 draw_bg.border_radius: 3.5}
        line := Text{width: Fill max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_bold{font_size: 11.25}}
    }

    mod.widgets.NewsViewBase = #(NewsView::register_widget(vm))
    mod.widgets.NewsView = set_type_default() do mod.widgets.NewsViewBase{
        width: Fill height: Fill
        app_view := HostedView{
            full: View{width: Fill height: Fill flow: Overlay
                SolidView{width: Fill height: Fill draw_bg.color: c_ground}
                body := View{width: Fill height: Fill flow: Down
                    // Search's field row, above the list, only on that page.
                    search_bar := View{visible: false width: Fill height: Fit flow: Right spacing: 4 align: Align{y: 0.5} padding: Inset{left: 16 right: 8 top: 10 bottom: 6}
                        search := Field{empty_text: "Search headlines"}
                        cancel := TextButton{text: "Cancel"}
                    }
                    list := PortalList{width: Fill height: Fill
                        Title := TitleRow{}
                        TitleBelowBar := TitleBelowBarRow{}
                        Section := SectionRow{}
                        Hero := HeroRow{}
                        Compact := CompactRow{}
                        Footer := FooterRow{}
                        CapTop := CapTopRow{}
                        CapBottom := CapBottomRow{}
                        Source := SourceRow{}
                        AddFeed := AddFeedRow{}
                        Status := StatusRow{}
                        Empty := EmptyRow{}
                        Spacer := SpacerRow{}
                    }
                }
                // The floating controls: back and refresh at the top, the
                // bar and the search button at the bottom. No ground of its
                // own, so the page beneath keeps its touches.
                chrome := View{width: Fill height: Fill flow: Down
                    View{width: Fill height: Fit flow: Right align: Align{y: 0.5} padding: Inset{left: 12 right: 12 top: 8}
                        back := BackButton{visible: false}
                        View{width: Fill height: Fit}
                        refresh := RefreshButton{}
                    }
                    View{width: Fill height: Fill}
                    bar_row := View{width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5} padding: Inset{left: 16 right: 16 bottom: 12}
                        bar := glass.TabBar{width: Fill height: 64 spacing: 0 padding: Inset{left: 6 right: 6 top: 6 bottom: 6}
                            today_tab := TabButton{text: "Today" draw_icon.svg: crate_resource("self:resources/icons/today.svg")}
                            following_tab := TabButton{text: "Following" draw_icon.svg: crate_resource("self:resources/icons/following.svg")}
                            saved_tab := TabButton{text: "Saved" draw_icon.svg: crate_resource("self:resources/icons/saved.svg")}
                        }
                        search_button := SearchButton{}
                    }
                }
                // The story's actions: a scrim and a sheet at the bottom.
                sheet_layer := View{visible: false width: Fill height: Fill flow: Overlay
                    // A `SolidView`: a plain view's ground has no shader at
                    // this revision and would draw nothing.
                    scrim := SolidView{width: Fill height: Fill draw_bg.color: #00000066 cursor: MouseCursor.Default}
                    View{width: Fill height: Fill flow: Down align: Align{y: 1.0} padding: Inset{left: 10 right: 10 bottom: 12} spacing: 8
                        sheet := Card{
                            sheet_title := Caption{max_lines: 2 padding: Inset{left: 16 right: 16 top: 12 bottom: 10} draw_text.text_style: theme.font_bold{font_size: 12}}
                            Hairline{}
                            save := SheetButton{text: "Save"}
                            discussion_line := Hairline{}
                            discussion := SheetButton{text: "Discussion on Hacker News"}
                            Hairline{}
                            browser := SheetButton{text: "Open in Browser"}
                            Hairline{}
                            copy := SheetButton{text: "Copy link"}
                        }
                        Card{
                            cancel_sheet := SheetButton{text: "Cancel" draw_text.text_style: theme.font_bold{font_size: 17}}
                        }
                    }
                }
                // Over everything, including the bar, while a story is open.
                reader := ArticleReader{visible: false}
            }
            tile: View{width: Fill height: Fill flow: Overlay
                SolidView{width: Fill height: Fill draw_bg.color: c_ground}
                RoundedView{width: Fill height: Fill margin: 5 flow: Down spacing: 3 padding: Inset{left: 12 right: 12 top: 6 bottom: 6}
                    draw_bg +: {color: c_card border_radius: 12.0}
                    tile_title := Text{text: "NEWS" draw_text +: {color: c_accent text_style: theme.font_bold{font_size: 9.5}}}
                    tile_0 := TileLine{}
                    tile_1 := TileLine{}
                    tile_2 := TileLine{}
                }
            }
        }
    }
}

/// One entry of the list, whatever the page: the template it draws with
/// and what it shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Item {
    /// A page's title and caption; `below_bar` on a pushed page, whose
    /// back button floats above.
    Title { text: String, caption: String, below_bar: bool },
    /// A Today section's header: the source's name and caption.
    Section { source: usize },
    /// A section's (or a page's) first story.
    Hero { row: Headline },
    /// One of the stories after it; `hairline` above all but a group's first.
    Compact { row: Headline, hairline: bool },
    /// `More from …`, closing a Today section.
    Footer { source: usize },
    /// The rounded caps of a group with no hero or footer.
    CapTop,
    CapBottom,
    /// One source on the Following page.
    Source { source: usize },
    /// The add-a-feed form.
    AddFeed,
    /// An error line under a title.
    Status { text: String },
    /// A centred caption where a list would be.
    Empty { text: String },
    /// Room for the floating bar.
    Spacer,
}

impl Item {
    fn template(&self) -> LiveId {
        match self {
            Item::Title { below_bar: false, .. } => live_id!(Title),
            Item::Title { below_bar: true, .. } => live_id!(TitleBelowBar),
            Item::Section { .. } => live_id!(Section),
            Item::Hero { .. } => live_id!(Hero),
            Item::Compact { .. } => live_id!(Compact),
            Item::Footer { .. } => live_id!(Footer),
            Item::CapTop => live_id!(CapTop),
            Item::CapBottom => live_id!(CapBottom),
            Item::Source { .. } => live_id!(Source),
            Item::AddFeed => live_id!(AddFeed),
            Item::Status { .. } => live_id!(Status),
            Item::Empty { .. } => live_id!(Empty),
            Item::Spacer => live_id!(Spacer),
        }
    }

    /// The story an item is about, if it is one.
    #[cfg(test)]
    fn row(&self) -> Option<&Headline> {
        match self {
            Item::Hero { row } | Item::Compact { row, .. } => Some(row),
            _ => None,
        }
    }
}

/// A storage read in flight and what it is for.
#[derive(Clone, Debug, PartialEq)]
enum Load {
    Feeds,
    Hidden,
    Saved,
    /// A source's cache, by the source's id: the source may have moved (or
    /// gone) by the time the bytes land.
    Cache(String),
}

/// The pages, the bar and the tile: one widget with two resident faces
/// (`app_view`, a `HostedView`). Owns everything the standalone window's
/// `App` would otherwise own, so a module host gets the same news a window
/// does.
#[derive(Script, ScriptHook, Widget)]
pub struct NewsView {
    #[deref]
    view: View,
    /// Set once, on the first event or draw.
    #[rust]
    started: bool,
    #[rust]
    model: NewsModel,
    /// Where the person is.
    #[rust]
    nav: Nav,
    /// The current page's list, recomputed by `render`, read by the list.
    #[rust]
    items: Vec<Item>,
    /// Search's query, as typed.
    #[rust]
    query: String,
    /// A slow tick that re-checks the refresh budget and ages the times.
    #[rust]
    tick: Option<Timer>,
    /// The instance's storage jail (module) or its own namespace
    /// (standalone): the feeds file, the hidden ids, the saved stories and
    /// the per-source cache live there.
    #[rust]
    storage: Option<StorageHandle>,
    /// Reads in flight, with what each is for.
    #[rust]
    loads: Vec<(StorageRequestId, Load)>,
    /// Writes in flight, with the key each is for, so a failure is logged
    /// by name.
    #[rust]
    writes: Vec<(StorageRequestId, String)>,
    /// Whether the tile last drew without its heading, so the visibility
    /// flips once per size change rather than every frame.
    #[rust]
    short_tile: Option<bool>,
    /// How this view is hosted (`main.rs` and the module's `create` say):
    /// the opener policy and the request channel read it.
    #[rust]
    hosting: Hosting,
    /// The link being opened and the tiers still to try for it: the host's
    /// `wm_unavailable` reply for that link moves on to the next. A Browser
    /// that launched sends no reply, so after a Browser request the entry
    /// lingers until the next open or a reply, which is what lets a process
    /// child's asynchronous `wm_unavailable` land over its socket. A
    /// module's host answers while it handles the actions pass that carries
    /// the posted request, so there the entry is dropped on the pass after
    /// that one (`asked_host`): by then the answer has come or never will.
    #[rust]
    pending_open: Option<(String, Vec<OpenTier>)>,
    /// A module posted a Browser request the host has not had its actions
    /// pass for yet.
    #[rust]
    asked_host: bool,
    /// The story the action sheet is about, while it is up.
    #[rust]
    sheet: Option<Headline>,
    /// The story the reader shows, for its `•••`.
    #[rust]
    reader_row: Option<Headline>,
    /// The last add-a-feed problem, shown on the form.
    #[rust]
    feed_error: Option<String>,
    /// The URL each row's `Image` was last asked to show, by the widget's
    /// uid: a recycled item is re-pointed only when its story changes.
    #[rust]
    image_bound: HashMap<u64, String>,
    /// How often each picture was asked for: a URL the cache forgot is
    /// asked for again `IMAGE_ATTEMPTS` times, then the row gives up on it
    /// rather than refetching a dead link every frame.
    #[rust]
    image_attempts: HashMap<String, u8>,
    /// The bar's last tint: the selected root (none while a page is
    /// pushed) and whether Search is up.
    #[rust]
    tinted: Option<(Option<Root>, bool)>,
    /// Search was just opened: its field takes the key focus (and raises
    /// the keyboard) after its first draw, when it has an area to focus.
    #[rust]
    focus_search: bool,
}

impl NewsView {
    /// A host handed this instance its storage: everything loads from and
    /// saves to it from now on. Call once: a second handle is ignored, so
    /// the feeds file is read from one jail only. Before the start
    /// `ensure_started` issues the loads; after it they go out at once, so
    /// a handle that arrives late is never a silent no-op.
    pub fn set_storage(&mut self, cx: &mut Cx, storage: StorageHandle) {
        if self.storage.is_some() {
            log!("news: set_storage called twice; keeping the first jail");
            return;
        }
        self.storage = Some(storage.clone());
        if self.started {
            self.load_from(cx, &storage);
        }
    }

    #[cfg(test)]
    pub(crate) fn has_pending_loads(&self) -> bool {
        !self.loads.is_empty()
    }

    /// The keys of the writes in flight.
    #[cfg(test)]
    pub(crate) fn pending_writes(&self) -> Vec<String> {
        self.writes.iter().map(|(_, key)| key.clone()).collect()
    }

    /// The link an opener tier was asked for and not yet answered.
    #[cfg(test)]
    pub(crate) fn pending_open_url(&self) -> Option<&str> {
        self.pending_open.as_ref().map(|(url, _)| url.as_str())
    }

    #[cfg(test)]
    pub(crate) fn items(&self) -> &[Item] {
        &self.items
    }

    #[cfg(test)]
    pub(crate) fn sheet_row(&self) -> Option<&Headline> {
        self.sheet.as_ref()
    }

    /// Where this view runs; the default is a window of its own.
    pub fn set_hosting(&mut self, hosting: Hosting) {
        self.hosting = hosting;
    }

    /// Whether the reader is showing an article over the page.
    pub fn reader_open(&self, cx: &Cx) -> bool {
        self.view.widget(cx, ids!(reader)).borrow::<ArticleReader>().is_some_and(|r| r.is_open())
    }

    pub fn model(&self) -> &NewsModel {
        &self.model
    }

    pub fn nav(&self) -> Nav {
        self.nav
    }

    /// The face the host asked for, read off the embedded `HostedView`.
    pub fn face(&self, cx: &Cx) -> HostedViewMode {
        self.view.widget(cx, ids!(app_view)).borrow::<HostedView>().map(|h| h.mode()).unwrap_or_default()
    }

    /// One line for the assistant's volatile context.
    pub fn ai_summary(&self) -> String {
        crate::ai::context_line(&self.model)
    }

    /// The timer stops, every fetch in flight is cancelled and the reader's
    /// web view is released; nothing else this widget owns outlives its
    /// isolate.
    pub fn shutdown(&mut self, cx: &mut Cx) {
        if let Some(t) = self.tick.take() {
            cx.stop_timer(t);
        }
        for id in self.model.cancel_all() {
            cx.cancel_http_request(id);
        }
        if let Some(mut reader) = self.view.widget(cx, ids!(reader)).borrow_mut::<ArticleReader>() {
            reader.shutdown(cx);
        }
    }

    /// Start on the first event or draw, whichever comes first.
    fn ensure_started(&mut self, cx: &mut Cx) {
        if self.started {
            return;
        }
        self.started = true;
        if let Some(storage) = self.storage.clone() {
            self.load_from(cx, &storage);
        }
        self.tick = Some(cx.start_interval(30.0));
        self.fetch_tab(cx, self.model.tab, false);
        self.render(cx);
    }

    /// The feeds file, the hidden ids, the saved stories and every source's
    /// cache, from the jail.
    fn load_from(&mut self, cx: &mut Cx, storage: &StorageHandle) {
        self.loads.push((storage.get(cx, FEEDS_KEY), Load::Feeds));
        self.loads.push((storage.get(cx, HIDDEN_KEY), Load::Hidden));
        self.loads.push((storage.get(cx, SAVED_KEY), Load::Saved));
        self.load_caches(cx, storage, 0..self.model.sources.len());
    }

    fn load_caches(&mut self, cx: &mut Cx, storage: &StorageHandle, sources: std::ops::Range<usize>) {
        for i in sources {
            let key = self.model.cache_key(i);
            let id = self.model.sources[i].def.id.clone();
            self.loads.push((storage.get(cx, &key), Load::Cache(id)));
        }
    }

    /// Write one value to the jail, if there is one.
    fn save(&mut self, cx: &mut Cx, key: &str, bytes: Vec<u8>) {
        if let Some(storage) = self.storage.as_ref() {
            let id = storage.set(cx, key, bytes);
            self.writes.push((id, key.to_string()));
        }
    }

    /// Fetch the tab's stale sources (`force`: every source it shows, the
    /// manual refresh — `due` would hold a fresh or failed one for the
    /// budget).
    fn fetch_tab(&mut self, cx: &mut Cx, tab: usize, force: bool) {
        for i in self.model.sources_for_tab(tab) {
            if force || self.model.due(i) {
                self.fetch_source(cx, i);
            }
        }
    }

    fn fetch_source(&mut self, cx: &mut Cx, source: usize) {
        if let Some(old) = self.model.sources[source].pending.take() {
            cx.cancel_http_request(old);
        }
        let (id, url) = self.model.begin_fetch(source);
        let mut request = HttpRequest::new(url, HttpMethod::GET);
        request.set_header(
            "Accept".into(),
            "application/rss+xml, application/atom+xml, application/xml, text/xml, application/json;q=0.9, */*;q=0.8".into(),
        );
        request.set_header("User-Agent".into(), "OctoSense-News/0.1".into());
        // A user feed may point anywhere: the backend stops allocating at
        // the cap while bytes arrive. The Android Java backend has no
        // transport cap, so the check on the landed body (`body_text`) is
        // the line that holds there.
        request.max_response_body_bytes = MAX_BODY_BYTES as u64;
        cx.http_request(id, request);
    }

    fn handle_response(&mut self, cx: &mut Cx, id: LiveId, result: Result<HttpResponse, String>) {
        // A reply for a request we already superseded (or a picture's, which
        // the image cache owns): drop it untouched.
        let Some(source) = self.model.owns(id) else { return };
        let kind = self.model.sources[source].def.kind;
        let parsed = result
            .and_then(|response| body_text(response.status_code, response.body.as_deref()).and_then(|text| parse_body(kind, text)));
        let good = parsed.is_ok();
        self.model.complete(id, parsed);
        if good {
            // Only rows from the network are worth saving: a failure keeps
            // whatever the source had, which is already in the jail.
            let key = self.model.cache_key(source);
            let bytes = self.model.cache_bytes(source);
            self.save(cx, &key, bytes);
        }
        self.render(cx);
    }

    fn on_storage(&mut self, cx: &mut Cx, responses: &[StorageResponse]) {
        for response in responses {
            if let Some(pos) = self.loads.iter().position(|(id, _)| *id == response.request_id) {
                let (_, load) = self.loads.remove(pos);
                let bytes = match &response.result {
                    Ok(StorageResult::Value(Some(bytes))) => bytes,
                    // No value yet: the defaults stand.
                    Ok(_) => continue,
                    Err(e) => {
                        log!("news: could not read {load:?}: {e}");
                        continue;
                    }
                };
                match load {
                    Load::Feeds => self.apply_feeds_file(cx, bytes),
                    Load::Hidden => {
                        self.model.load_hidden(bytes);
                        // What Today shows just changed: its sources are due.
                        self.fetch_tab(cx, self.model.tab, false);
                    }
                    Load::Saved => {
                        self.model.load_saved(bytes);
                    }
                    Load::Cache(source_id) => {
                        if let Some(i) = self.model.sources.iter().position(|s| s.def.id == source_id) {
                            self.model.load_cache(i, bytes);
                        }
                    }
                }
                self.render(cx);
            } else if let Some(pos) = self.writes.iter().position(|(id, _)| *id == response.request_id) {
                let (_, key) = self.writes.remove(pos);
                if let Err(e) = &response.result {
                    log!("news: could not save {key}: {e}");
                }
            }
        }
    }

    /// The person's feeds file landed: the user sources follow it, their
    /// caches load, and whatever the current tab now shows is fetched.
    fn apply_feeds_file(&mut self, cx: &mut Cx, bytes: &[u8]) {
        let parsed = std::str::from_utf8(bytes).map_err(|_| "not UTF-8".to_string()).and_then(parse_user_feeds);
        let feeds = match parsed {
            Ok(feeds) => feeds,
            Err(e) => {
                log!("news: {FEEDS_KEY} is not readable: {e}");
                return;
            }
        };
        let (added, cancelled) = self.model.set_user_feeds(feeds);
        for id in cancelled {
            cx.cancel_http_request(id);
        }
        if let Some(storage) = self.storage.clone() {
            self.load_caches(cx, &storage, added);
        }
        self.fetch_tab(cx, self.model.tab, false);
    }

    // ---- Navigation ----

    /// The bar picked a root page. Picking the page already showing
    /// scrolls it to the top.
    pub(crate) fn pick_root(&mut self, cx: &mut Cx, root: Root) {
        self.dismiss_sheet();
        if self.nav.pick(root) {
            self.after_nav(cx);
        } else {
            self.view.portal_list(cx, ids!(list)).set_first_id_and_scroll(0, 0.0);
            self.render(cx);
        }
    }

    /// A source's own page, from a section header, a footer or Following.
    pub(crate) fn push_source(&mut self, cx: &mut Cx, source: usize) {
        if source >= self.model.sources.len() {
            return;
        }
        self.dismiss_sheet();
        self.nav.push(Pushed::Source(source));
        self.after_nav(cx);
    }

    /// The search page, from the round button.
    pub(crate) fn open_search(&mut self, cx: &mut Cx) {
        self.dismiss_sheet();
        self.nav.push(Pushed::Search);
        self.focus_search = true;
        self.after_nav(cx);
    }

    /// Search's query changed.
    pub(crate) fn set_query(&mut self, cx: &mut Cx, query: &str) {
        self.query = query.to_string();
        self.view.portal_list(cx, ids!(list)).set_first_id_and_scroll(0, 0.0);
        self.render(cx);
    }

    /// Back: the sheet goes, else the reader, else the pushed page. False
    /// when nothing was open, so the host may take the press.
    pub fn back(&mut self, cx: &mut Cx) -> bool {
        if self.sheet.is_some() {
            self.dismiss_sheet();
            self.render(cx);
            return true;
        }
        if self.reader_open(cx) {
            self.close_reader(cx);
            return true;
        }
        if self.nav.pop() {
            self.after_nav(cx);
            return true;
        }
        false
    }

    /// The page changed: the tick follows it, its stale sources are
    /// fetched, the list starts at the top, Search's query is dropped when
    /// Search is left.
    fn after_nav(&mut self, cx: &mut Cx) {
        if self.nav.page() != Page::Search && !self.query.is_empty() {
            self.query.clear();
            self.view.text_input(cx, ids!(search)).set_text(cx, "");
        }
        self.model.select_tab(self.nav.fetch_tab());
        self.fetch_tab(cx, self.model.tab, false);
        self.view.portal_list(cx, ids!(list)).set_first_id_and_scroll(0, 0.0);
        self.render(cx);
    }

    // ---- Following and Saved ----

    /// Follow or hide a source from the Following page.
    pub(crate) fn set_followed(&mut self, cx: &mut Cx, source: usize, followed: bool) {
        if !self.model.set_followed(source, followed) {
            return;
        }
        let bytes = self.model.hidden_bytes();
        self.save(cx, HIDDEN_KEY, bytes);
        if followed {
            // Back on Today: fetch it if it is stale.
            self.fetch_tab(cx, 0, false);
        }
        self.render(cx);
    }

    /// Add a feed from the form; the problem, if any, shows on it.
    pub(crate) fn add_feed(&mut self, cx: &mut Cx, label: &str, url: &str) -> bool {
        match self.model.add_user_feed(label, url) {
            Ok(source) => {
                self.feed_error = None;
                let bytes = self.model.feeds_bytes();
                self.save(cx, FEEDS_KEY, bytes);
                if let Some(storage) = self.storage.clone() {
                    self.load_caches(cx, &storage, source..source + 1);
                }
                self.fetch_source(cx, source);
                self.render(cx);
                true
            }
            Err(e) => {
                self.feed_error = Some(e);
                self.render(cx);
                false
            }
        }
    }

    /// Remove one of the person's feeds; its page, if showing, goes.
    pub(crate) fn remove_feed(&mut self, cx: &mut Cx, source: usize) {
        match self.model.remove_user_feed(source) {
            Ok(cancelled) => {
                for id in cancelled {
                    cx.cancel_http_request(id);
                }
                let bytes = self.model.feeds_bytes();
                self.save(cx, FEEDS_KEY, bytes);
                let hidden = self.model.hidden_bytes();
                self.save(cx, HIDDEN_KEY, hidden);
                if matches!(self.nav.pushed, Some(Pushed::Source(i)) if i >= self.model.sources.len()) {
                    self.nav.pop();
                }
                self.after_nav(cx);
            }
            Err(e) => log!("news: {e}"),
        }
    }

    /// Save a story, or unsave it: the sheet's first row.
    pub(crate) fn toggle_saved(&mut self, cx: &mut Cx, row: &Headline) -> bool {
        let saved = self.model.toggle_saved(row);
        let bytes = self.model.saved_bytes();
        self.save(cx, SAVED_KEY, bytes);
        self.render(cx);
        saved
    }

    // ---- The sheet ----

    /// The story's actions, from its `•••` or the reader's.
    pub(crate) fn open_sheet(&mut self, cx: &mut Cx, row: Headline) {
        self.sheet = Some(row);
        self.render(cx);
    }

    fn dismiss_sheet(&mut self) {
        self.sheet = None;
    }

    // ---- Opening a link ----

    /// Open a story: the reader first where there is one, then the bundled
    /// Browser through the host, the system browser, a notification
    /// (`OpenPolicy`). The one place a link leaves the list, and only an
    /// http(s) link gets past it. Nothing on the tile face: a tap there is
    /// the host's, and opens the full app.
    pub fn open_link(&mut self, cx: &mut Cx, url: &str) {
        let tiers = OpenPolicy::for_platform(self.hosting).tiers();
        self.start_open(cx, url, tiers);
    }

    /// A story, from its card: the reader remembers it for its `•••`.
    fn open_row(&mut self, cx: &mut Cx, row: &Headline) {
        self.reader_row = Some(row.clone());
        self.open_link(cx, &row.link);
    }

    /// `Open in Browser`: past the reader, the bundled Browser through the
    /// host, else the system browser, else a notification.
    pub(crate) fn open_in_browser(&mut self, cx: &mut Cx, url: &str) {
        let tiers = OpenPolicy::for_platform(self.hosting).browser_tiers();
        self.start_open(cx, url, tiers);
    }

    fn start_open(&mut self, cx: &mut Cx, url: &str, tiers: Vec<OpenTier>) {
        if !is_http_url(url) || self.face(cx) == HostedViewMode::Tile {
            return;
        }
        self.pending_open = Some((url.to_string(), tiers));
        self.try_next_tier(cx);
    }

    /// The next tier for the pending link, and the one after when a tier
    /// cannot even be asked. A tier that was asked (the Browser request)
    /// leaves `pending_open` in place for the host's answer; the others
    /// settle the link here.
    fn try_next_tier(&mut self, cx: &mut Cx) {
        self.asked_host = false;
        loop {
            let Some((url, tiers)) = self.pending_open.as_mut() else { return };
            if tiers.is_empty() {
                self.pending_open = None;
                return;
            }
            let tier = tiers.remove(0);
            let url = url.clone();
            match tier {
                OpenTier::Reader => {
                    self.pending_open = None;
                    self.dismiss_sheet();
                    if let Some(mut reader) = self.view.widget(cx, ids!(reader)).borrow_mut::<ArticleReader>() {
                        reader.open(cx, &url);
                    }
                    self.render(cx);
                    return;
                }
                OpenTier::Browser => {
                    let req = WmRequest::Open { app: Some("browser".into()), path: url };
                    if self.send_request(cx, req) {
                        self.asked_host = self.hosting == Hosting::Module;
                        return;
                    }
                }
                OpenTier::System => {
                    self.pending_open = None;
                    cx.open_url(&url, OpenUrlInPlace::No);
                    return;
                }
                OpenTier::Notify => {
                    self.pending_open = None;
                    if !self.send_request(cx, browser_failed_notice(&url)) {
                        log!("news: nothing here can open {url}");
                    }
                    return;
                }
            }
        }
    }

    /// A request to the window manager by the channel this hosting has: a
    /// widget action from the root for a module (the host attributes it by
    /// the root's uid), the studio socket for a process. False when there
    /// is no window manager to ask.
    fn send_request(&mut self, cx: &mut Cx, req: WmRequest) -> bool {
        match self.hosting {
            Hosting::Module => {
                cx.widget_action(self.widget_uid(), req);
                true
            }
            Hosting::Process => makepad_wm_api::send(cx, &req),
            Hosting::Standalone => false,
        }
    }

    /// Whether this actions pass is the one carrying a request this root
    /// posted: the host answers during it, so it settles nothing yet.
    fn carries_own_request(&self, actions: &Actions) -> bool {
        let uid = self.widget_uid();
        actions
            .iter()
            .filter_map(|a| a.as_widget_action())
            .any(|wa| wa.widget_uid == uid && wa.action.downcast_ref::<WmRequest>().is_some())
    }

    /// Close the reader, if open, and show the page again.
    pub fn close_reader(&mut self, cx: &mut Cx) {
        if let Some(mut reader) = self.view.widget(cx, ids!(reader)).borrow_mut::<ArticleReader>() {
            reader.close(cx);
        }
        self.render(cx);
    }

    // ---- Rendering ----

    /// The current page as list items.
    fn build_items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        match self.nav.page() {
            Page::Today => {
                let caption = format!("{} · {}", date_line(now_unix()), self.model.status_text(0));
                items.push(Item::Title { text: "Today".into(), caption, below_bar: false });
                if let Some(text) = self.model.error_text(0) {
                    items.push(Item::Status { text });
                }
                let sections = self.model.today_sections();
                if sections.is_empty() {
                    let text = if self.model.loading(0) {
                        "Loading the day's stories…"
                    } else if self.model.followed_sources().is_empty() {
                        "Follow a source to see its stories here."
                    } else {
                        "No headlines yet."
                    };
                    items.push(Item::Empty { text: text.into() });
                }
                for section in sections {
                    items.push(Item::Section { source: section.source });
                    let mut rows = section.rows.into_iter();
                    if let Some(hero) = rows.next() {
                        items.push(Item::Hero { row: hero });
                    }
                    items.extend(rows.map(|row| Item::Compact { row, hairline: true }));
                    items.push(Item::Footer { source: section.source });
                }
            }
            Page::Source(source) => {
                let def = &self.model.sources[source].def;
                let caption = format!("{} · {}", section_caption(def), self.model.status_text(source + 1));
                items.push(Item::Title { text: def.label.clone(), caption, below_bar: true });
                if let Some(text) = self.model.error_text(source + 1) {
                    items.push(Item::Status { text });
                }
                let rows = self.model.rows_for_tab(source + 1);
                if rows.is_empty() {
                    let text = if self.model.loading(source + 1) { "Loading…" } else { "No headlines yet." };
                    items.push(Item::Empty { text: text.into() });
                } else {
                    push_group(&mut items, rows, true);
                }
            }
            Page::Following => {
                let n = self.model.followed_sources().len();
                let caption = format!("{} of {} sources on Today", n, self.model.sources.len());
                items.push(Item::Title { text: "Following".into(), caption, below_bar: false });
                items.push(Item::CapTop);
                items.extend((0..self.model.sources.len()).map(|source| Item::Source { source }));
                items.push(Item::CapBottom);
                items.push(Item::AddFeed);
            }
            Page::Saved => {
                let n = self.model.saved.len();
                let caption = if n == 1 { "1 story".to_string() } else { format!("{n} stories") };
                items.push(Item::Title { text: "Saved".into(), caption, below_bar: false });
                if n == 0 {
                    items.push(Item::Empty { text: "Stories you save appear here.".into() });
                } else {
                    push_group(&mut items, self.model.saved.clone(), false);
                }
            }
            Page::Search => {
                if self.query.trim().is_empty() {
                    items.push(Item::Empty { text: "Search the headlines of the sources you follow.".into() });
                } else {
                    let rows = self.model.search(&self.query);
                    if rows.is_empty() {
                        items.push(Item::Empty { text: format!("No results for \u{201c}{}\u{201d}", self.query.trim()) });
                    } else {
                        push_group(&mut items, rows, false);
                    }
                }
            }
        }
        items.push(Item::Spacer);
        items
    }

    /// Push the model and the navigation into both faces.
    fn render(&mut self, cx: &mut Cx) {
        let page = self.nav.page();
        let reader_open = self.reader_open(cx);
        let sheet_up = self.sheet.is_some();
        self.items = self.build_items();
        let list = self.view.portal_list(cx, ids!(list));
        // A refresh into a shorter list: back to the top rather than a
        // blank page past the end.
        let first = list.first_id();
        if first > 0 && self.items.len() <= first {
            list.set_first_id_and_scroll(0, 0.0);
        }

        // The chrome: the page beneath the reader is hidden with it, so a
        // stray touch never reaches it.
        self.view.widget(cx, ids!(body)).set_visible(cx, !reader_open);
        self.view.widget(cx, ids!(chrome)).set_visible(cx, !reader_open);
        self.view.widget(cx, ids!(search_bar)).set_visible(cx, page == Page::Search);
        self.view.widget(cx, ids!(back)).set_visible(cx, matches!(page, Page::Source(_)));
        self.view.widget(cx, ids!(refresh)).set_visible(cx, matches!(page, Page::Today | Page::Source(_)));
        // The glass bar composites above ordinary content, so it steps
        // aside for the sheet; Search's keyboard wants the room too.
        self.view.widget(cx, ids!(bar_row)).set_visible(cx, page != Page::Search && !sheet_up);
        // The bar's tint follows the selection; applied only when it moves,
        // since an apply re-evaluates the button and reloads its icon.
        let selected = if self.nav.pushed.is_none() { Some(self.nav.root) } else { None };
        if self.tinted != Some((selected, page == Page::Search)) {
            self.tinted = Some((selected, page == Page::Search));
            let skin = Skin::current();
            for (id, root) in [(ids!(today_tab), Root::Today), (ids!(following_tab), Root::Following), (ids!(saved_tab), Root::Saved)] {
                let color = if selected == Some(root) { skin.accent } else { skin.secondary };
                let mut button = self.view.widget(cx, id);
                script_apply_eval!(cx, button, { draw_icon.color: #(color) draw_text.color: #(color) });
            }
            let color = if page == Page::Search { skin.accent } else { skin.ink };
            let mut button = self.view.widget(cx, ids!(search_button));
            script_apply_eval!(cx, button, { draw_icon.color: #(color) });
        }

        // The sheet.
        self.view.widget(cx, ids!(sheet_layer)).set_visible(cx, sheet_up && !reader_open || sheet_up && self.reader_row.is_some());
        if let Some(row) = self.sheet.clone() {
            let saved = self.model.is_saved(&row.link);
            self.view.label(cx, ids!(sheet_title)).set_text(cx, &row.title);
            self.view.button(cx, ids!(save)).set_text(cx, if saved { "Unsave" } else { "Save" });
            let has_discussion = row.discussion.is_some();
            self.view.widget(cx, ids!(discussion)).set_visible(cx, has_discussion);
            self.view.widget(cx, ids!(discussion_line)).set_visible(cx, has_discussion);
        }

        // The tile.
        let tile = self.model.tile_rows();
        for (i, id) in [ids!(tile_0), ids!(tile_1), ids!(tile_2)].iter().enumerate() {
            let line = self.view.widget(cx, *id);
            let row = tile.get(i);
            let text = row.map(tile_line).unwrap_or_else(|| if i == 0 { self.model.status_text(0) } else { String::new() });
            line.label(cx, ids!(line)).set_text(cx, &text);
            // The status line, or an empty one, has no source to mark.
            line.widget(cx, ids!(dot)).set_visible(cx, row.is_some());
            if let Some(row) = row {
                let dot = line.widget(cx, ids!(dot));
                tint(cx, dot, source_color(&row.source_id));
            }
        }
        self.view.redraw(cx);
    }

    /// One story's shared parts: the dot, the publisher, the headline, the
    /// meta line.
    fn fill_story(&self, cx: &mut Cx, item: &WidgetRef, row: &Headline, now: i64) {
        let dot = item.widget(cx, ids!(dot));
        tint(cx, dot, source_color(&row.source_id));
        item.label(cx, ids!(publisher)).set_text(cx, &self.model.publisher(row));
        item.label(cx, ids!(headline)).set_text(cx, &row.title);
        item.label(cx, ids!(meta)).set_text(cx, &meta_line(row, now));
    }

    /// Point a row's `Image` at the story's picture, or hide it. A picture
    /// loads through the framework's image cache; a URL the cache forgot
    /// is retried `IMAGE_ATTEMPTS` times, then the row goes without.
    fn show_image(&mut self, cx: &mut Cx, image: ImageRef, url: Option<&str>) {
        let Some(url) = url else {
            image.set_visible(cx, false);
            return;
        };
        let uid = image.widget_uid().0;
        let bound = self.image_bound.get(&uid).map(String::as_str) == Some(url);
        let cached = cx.has_global::<ImageCache>() && cx.get_global::<ImageCache>().map.contains_key(Path::new(url));
        if !bound || !cached {
            let attempts = self.image_attempts.entry(url.to_string()).or_insert(0);
            if !cached && *attempts >= IMAGE_ATTEMPTS {
                image.set_visible(cx, false);
                return;
            }
            if !cached {
                *attempts += 1;
            }
            if let Err(e) = image.load_image_http_by_url_async(cx, url) {
                log!("news: could not load {url}: {e:?}");
                image.set_visible(cx, false);
                return;
            }
            self.image_bound.insert(uid, url.to_string());
        }
        image.set_visible(cx, true);
    }

    fn draw_rows(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        let now = now_unix();
        list.set_item_range(cx, 0, self.items.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(entry) = self.items.get(index).cloned() else { continue };
            let item = list.item(cx, index, entry.template());
            match &entry {
                Item::Title { text, caption, .. } => {
                    item.label(cx, ids!(title)).set_text(cx, text);
                    item.label(cx, ids!(caption)).set_text(cx, caption);
                }
                Item::Section { source } => {
                    let def = &self.model.sources[*source].def;
                    item.label(cx, ids!(name)).set_text(cx, &def.label);
                    item.label(cx, ids!(caption)).set_text(cx, &section_caption(def));
                }
                Item::Hero { row } => {
                    self.fill_story(cx, &item, row, now);
                    let deck = deck_text(row);
                    item.widget(cx, ids!(deck)).set_visible(cx, deck.is_some());
                    item.label(cx, ids!(deck)).set_text(cx, deck.unwrap_or_default());
                    let image = item.image(cx, ids!(image));
                    self.show_image(cx, image, row.image.as_deref());
                }
                Item::Compact { row, hairline } => {
                    self.fill_story(cx, &item, row, now);
                    item.widget(cx, ids!(hairline)).set_visible(cx, *hairline);
                    let thumb = item.image(cx, ids!(thumb));
                    self.show_image(cx, thumb, row.image.as_deref());
                }
                Item::Footer { source } => {
                    let label = format!("More from {}", self.model.sources[*source].def.label);
                    item.label(cx, ids!(label)).set_text(cx, &label);
                }
                Item::Source { source } => {
                    let def = self.model.sources[*source].def.clone();
                    let user = *source >= builtin_count();
                    let mono = item.widget(cx, ids!(mono));
                    tint(cx, mono, source_color(&def.id));
                    item.label(cx, ids!(initial)).set_text(cx, &initial(&def.label));
                    item.label(cx, ids!(label)).set_text(cx, &def.label);
                    item.label(cx, ids!(host)).set_text(cx, &host_of(&def.url));
                    item.widget(cx, ids!(hairline)).set_visible(cx, *source > 0);
                    item.widget(cx, ids!(remove)).set_visible(cx, user);
                }
                Item::AddFeed => {
                    let error = self.feed_error.clone().unwrap_or_default();
                    item.widget(cx, ids!(error)).set_visible(cx, !error.is_empty());
                    item.label(cx, ids!(error)).set_text(cx, &error);
                }
                Item::Status { text } | Item::Empty { text } => {
                    item.label(cx, ids!(text)).set_text(cx, text);
                }
                Item::CapTop | Item::CapBottom | Item::Spacer => {}
            }
            item.draw_all(cx, &mut Scope::empty());
            if let Item::Source { source } = &entry {
                // After the draw: a toggle's animator comes up in its
                // default state on its first draw, which would undo a state
                // set before it. A cut, not a play, so a recycled row shows
                // its source's state at once.
                let follow = item.check_box(cx, ids!(follow));
                let followed = self.model.is_followed(*source);
                if follow.active(cx) != followed {
                    follow.set_active(cx, followed, Animate::No);
                    item.redraw(cx);
                }
            }
        }
    }

    /// The actions of the list's items: taps on stories, sections and
    /// footers, the sheet buttons, the Following toggles, the form.
    fn handle_item_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let list = self.view.portal_list(cx, ids!(list));
        let mut open: Option<Headline> = None;
        let mut sheet: Option<Headline> = None;
        let mut push: Option<usize> = None;
        let mut follow: Option<(usize, bool)> = None;
        let mut remove: Option<usize> = None;
        let mut add: Option<(String, String)> = None;
        for (index, item) in list.items_with_actions(actions) {
            // Beyond `items`: a row removed by a refresh in the same frame.
            let Some(entry) = self.items.get(index) else { continue };
            match entry {
                Item::Hero { row } | Item::Compact { row, .. } => {
                    if item.button(cx, ids!(more)).clicked(actions) {
                        sheet = Some(row.clone());
                    } else if tapped(cx, &item, ids!(card), actions) {
                        open = Some(row.clone());
                    }
                }
                Item::Section { source } => {
                    if item.button(cx, ids!(open)).clicked(actions) || tapped(cx, &item, ids!(header), actions) {
                        push = Some(*source);
                    }
                }
                Item::Footer { source } => {
                    if tapped(cx, &item, ids!(card), actions) {
                        push = Some(*source);
                    }
                }
                Item::Source { source } => {
                    if let Some(on) = item.check_box(cx, ids!(follow)).changed(actions) {
                        follow = Some((*source, on));
                    } else if item.button(cx, ids!(remove)).clicked(actions) {
                        remove = Some(*source);
                    } else if tapped(cx, &item, ids!(row), actions) {
                        push = Some(*source);
                    }
                }
                Item::AddFeed => {
                    let submitted = item.button(cx, ids!(add)).clicked(actions)
                        || item.text_input(cx, ids!(url)).returned(actions).is_some()
                        || item.text_input(cx, ids!(name)).returned(actions).is_some();
                    if submitted {
                        add = Some((item.text_input(cx, ids!(name)).text(), item.text_input(cx, ids!(url)).text()));
                    }
                }
                _ => {}
            }
        }
        if let Some(row) = sheet {
            self.open_sheet(cx, row);
        } else if let Some(row) = open {
            self.open_row(cx, &row);
        } else if let Some(source) = push {
            self.push_source(cx, source);
        } else if let Some((source, on)) = follow {
            self.set_followed(cx, source, on);
        } else if let Some(source) = remove {
            self.remove_feed(cx, source);
        } else if let Some((name, url)) = add {
            if self.add_feed(cx, &name, &url) {
                // The form is the list's last item; a fresh one greets the
                // next feed.
                if let Some((_, item)) = list.get_item(self.items.len().saturating_sub(2)) {
                    item.text_input(cx, ids!(url)).set_text(cx, "");
                    item.text_input(cx, ids!(name)).set_text(cx, "");
                }
            }
        }
    }

    /// The sheet's buttons and its scrim.
    fn handle_sheet_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let Some(row) = self.sheet.clone() else { return };
        if self.view.button(cx, ids!(save)).clicked(actions) {
            self.dismiss_sheet();
            self.toggle_saved(cx, &row);
        } else if self.view.button(cx, ids!(discussion)).clicked(actions) {
            self.dismiss_sheet();
            if let Some(discussion) = row.discussion.as_deref() {
                self.reader_row = Some(row.clone());
                self.open_link(cx, discussion);
            }
            self.render(cx);
        } else if self.view.button(cx, ids!(browser)).clicked(actions) {
            self.dismiss_sheet();
            self.open_in_browser(cx, &row.link);
            self.render(cx);
        } else if self.view.button(cx, ids!(copy)).clicked(actions) {
            self.dismiss_sheet();
            cx.copy_to_clipboard(&row.link);
            self.render(cx);
        } else if self.view.button(cx, ids!(cancel_sheet)).clicked(actions) || tapped(cx, &self.view.widget(cx, ids!(sheet_layer)), ids!(scrim), actions) {
            self.dismiss_sheet();
            self.render(cx);
        }
    }
}

/// A tap (not the end of a scroll drag) on the child `id` of `item`.
fn tapped(cx: &Cx, item: &WidgetRef, id: &[LiveId], actions: &Actions) -> bool {
    item.widget(cx, id).as_view().finger_up(actions).is_some_and(|up| up.was_tap())
}

/// Colour a view's ground (a source's dot or monogram).
fn tint(cx: &mut Cx, mut widget: WidgetRef, rgba: u32) {
    let color = Vec4f::from_u32(rgba);
    script_apply_eval!(cx, widget, { draw_bg.color: #(color) });
}

/// A page's rows as one card group: a hero first when `hero`, compacts
/// after, the caps where the group has no hero or footer.
fn push_group(items: &mut Vec<Item>, rows: Vec<Headline>, hero: bool) {
    let mut rows = rows.into_iter();
    if hero {
        if let Some(first) = rows.next() {
            items.push(Item::Hero { row: first });
        }
    } else {
        items.push(Item::CapTop);
    }
    for (i, row) in rows.enumerate() {
        items.push(Item::Compact { row, hairline: hero || i > 0 });
    }
    items.push(Item::CapBottom);
}

fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The last tier's notice: the Browser could not be launched here, or was
/// and failed to start, and nothing else on this platform opens a link.
pub(crate) fn browser_failed_notice(url: &str) -> WmRequest {
    WmRequest::Notify { title: "Could not open in the Browser".into(), body: host_of(url) }
}

impl Widget for NewsView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.ensure_started(cx);
        match event {
            Event::Custom(json) => {
                if let Some(reply) = WmUnavailable::parse(json) {
                    // The host cannot launch the Browser for this link: the
                    // next tier. An answer for some other link is not ours.
                    if self.pending_open.as_ref().is_some_and(|(url, _)| *url == reply.path) {
                        self.try_next_tier(cx);
                    }
                } else if HostedViewMode::parse(json) == Some(HostedViewMode::Tile) {
                    // The full face goes away under an open article: the
                    // overlay would stay on the window on its own.
                    self.dismiss_sheet();
                    self.close_reader(cx);
                }
            }
            Event::BackPressed { handled } => {
                if !handled.get() && self.back(cx) {
                    handled.set(true);
                }
            }
            Event::Storage(responses) => self.on_storage(cx, responses),
            Event::NetworkResponses(responses) => {
                // The pictures' replies first: the image cache owns them,
                // and a reply that lands while no row is on screen would
                // otherwise be lost.
                handle_image_cache_network_responses(cx, responses);
                for response in responses {
                    match response {
                        NetworkResponse::HttpResponse { request_id, response } => {
                            self.handle_response(cx, *request_id, Ok(response.clone()));
                        }
                        NetworkResponse::HttpError { request_id, error } => {
                            self.handle_response(cx, *request_id, Err(plain_error(&error.message)));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if self.tick.as_ref().is_some_and(|t| t.is_event(event).is_some()) {
            self.fetch_tab(cx, self.model.tab, false);
            self.render(cx);
        }
        if let Event::Actions(actions) = event {
            if self.asked_host && !self.carries_own_request(actions) {
                // The host had its pass for the request: it answered (and
                // the link moved on, above) or launched the Browser, which
                // never answers. Either way nothing waits any more.
                self.asked_host = false;
                self.pending_open = None;
            }
            if self.view.button(cx, ids!(refresh)).clicked(actions) {
                self.fetch_tab(cx, self.model.tab, true);
                self.render(cx);
            }
            if self.view.button(cx, ids!(back)).clicked(actions) {
                self.back(cx);
            }
            if self.view.button(cx, ids!(today_tab)).clicked(actions) {
                self.pick_root(cx, Root::Today);
            }
            if self.view.button(cx, ids!(following_tab)).clicked(actions) {
                self.pick_root(cx, Root::Following);
            }
            if self.view.button(cx, ids!(saved_tab)).clicked(actions) {
                self.pick_root(cx, Root::Saved);
            }
            if self.view.button(cx, ids!(search_button)).clicked(actions) {
                self.open_search(cx);
            }
            if self.view.button(cx, ids!(cancel)).clicked(actions) {
                self.back(cx);
            }
            if let Some(query) = self.view.text_input(cx, ids!(search)).changed(actions) {
                self.set_query(cx, &query);
            }
            if self.sheet.is_some() {
                self.handle_sheet_actions(cx, actions);
            } else {
                self.handle_item_actions(cx, actions);
            }
            let reader_uid = self.view.widget(cx, ids!(reader)).widget_uid();
            match actions.find_widget_action(reader_uid).cast::<ReaderAction>() {
                ReaderAction::Closed => {
                    self.reader_row = None;
                    self.render(cx);
                }
                ReaderAction::More => {
                    if let Some(row) = self.reader_row.clone() {
                        self.open_sheet(cx, row);
                    }
                }
                ReaderAction::None => {}
            }
        }
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.ensure_started(cx);
        let height = cx.turtle().rect().size.y;
        // A zero-height first frame says nothing about the room.
        if height >= 1.0 {
            let short = height < SHORT_TILE;
            if self.short_tile != Some(short) {
                self.short_tile = Some(short);
                self.view.widget(cx, ids!(tile_title)).set_visible(cx, !short);
            }
        }
        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.as_portal_list().borrow_mut() {
                self.draw_rows(cx, &mut list);
            }
        }
        if self.focus_search && self.nav.page() == Page::Search {
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

    /// A `NewsView` in an isolate of its own, started on the full face and
    /// hosted as `hosting`, handed to `test` inside the isolate; torn down
    /// in the host's order afterwards. No network: nothing lands.
    fn with_view(hosting: Hosting, test: impl FnOnce(&mut Cx, &WidgetRef)) {
        let (mut iso, root) = isolate_root();
        iso.entered(|cx| {
            root.handle_event(cx, &Event::Custom(HostedViewMode::Full.to_json()), &mut Scope::empty());
            root.borrow_mut::<NewsView>().unwrap().set_hosting(hosting);
            test(cx, &root);
            root.borrow_mut::<NewsView>().unwrap().shutdown(cx);
        });
        iso.teardown(root);
    }

    /// Whether a view hosted as `hosting` gets the reader here: the
    /// policy's own answer for this platform, not a guess at the OS.
    fn reader_here(hosting: Hosting) -> bool {
        OpenPolicy::for_platform(hosting).has_webview
    }

    fn unavailable(path: &str) -> Event {
        Event::Custom(WmUnavailable { app: "browser".into(), path: path.into() }.to_json())
    }

    /// The actions the root posted while `f` ran.
    fn posted_actions(cx: &mut Cx, root: &WidgetRef, f: impl FnOnce(&mut Cx, &mut NewsView)) -> ActionsBuf {
        cx.capture_actions(|cx| f(cx, &mut root.borrow_mut::<NewsView>().unwrap()))
    }

    /// The `WmRequest` the root posted while `f` ran, if one.
    fn posted_request(cx: &mut Cx, root: &WidgetRef, f: impl FnOnce(&mut Cx, &mut NewsView)) -> Option<WmRequest> {
        let uid = root.widget_uid();
        posted_actions(cx, root, f)
            .iter()
            .filter_map(|a| a.as_widget_action())
            .find(|wa| wa.widget_uid == uid)
            .and_then(|wa| wa.action.downcast_ref::<WmRequest>().cloned())
    }

    fn visible(cx: &Cx, root: &WidgetRef, id: &[LiveId]) -> bool {
        root.borrow::<NewsView>().unwrap().view.widget(cx, id).visible()
    }

    fn with<R>(root: &WidgetRef, f: impl FnOnce(&mut NewsView) -> R) -> R {
        f(&mut root.borrow_mut::<NewsView>().unwrap())
    }

    /// Land three rows in a built-in source, as a fetch would.
    fn land(cx: &mut Cx, root: &WidgetRef, source: usize, prefix: &str) {
        with(root, |view| {
            let (id, _) = view.model.begin_fetch(source);
            let rows = (1..=3)
                .map(|i| Headline { title: format!("{prefix}{i}"), link: format!("https://x/{prefix}{i}"), ..Default::default() })
                .collect();
            view.model.complete(id, Ok(rows));
            view.render(cx);
        });
    }

    fn row(title: &str) -> Headline {
        Headline { title: title.into(), link: format!("https://x/{title}"), ..Default::default() }
    }

    #[test]
    fn the_view_starts_on_today_with_the_bar_and_no_reader() {
        with_view(Hosting::Standalone, |cx, root| {
            let view = root.borrow::<NewsView>().unwrap();
            assert!(!view.reader_open(cx));
            assert_eq!(view.pending_open_url(), None);
            assert_eq!(view.nav().page(), Page::Today);
            assert!(matches!(view.items().first(), Some(Item::Title { text, below_bar: false, .. }) if text == "Today"));
            assert!(view.items().iter().any(|i| matches!(i, Item::Empty { .. })), "nothing landed: the placeholder shows");
            assert!(matches!(view.items().last(), Some(Item::Spacer)), "room for the bar");
            drop(view);
            assert!(!visible(cx, root, ids!(reader)));
            assert!(visible(cx, root, ids!(bar_row)));
            assert!(visible(cx, root, ids!(refresh)));
            assert!(!visible(cx, root, ids!(back)));
            assert!(!visible(cx, root, ids!(search_bar)));
            assert!(!visible(cx, root, ids!(sheet_layer)));
        });
    }

    #[test]
    fn today_lists_a_section_per_source_with_rows_a_hero_first_and_a_footer_last() {
        with_view(Hosting::Standalone, |cx, root| {
            land(cx, root, 0, "h");
            land(cx, root, 2, "g");
            let view = root.borrow::<NewsView>().unwrap();
            let kinds: Vec<String> = view
                .items()
                .iter()
                .map(|i| match i {
                    Item::Title { .. } => "title".into(),
                    Item::Section { source } => format!("section{source}"),
                    Item::Hero { row } => format!("hero:{}", row.title),
                    Item::Compact { row, hairline } => format!("compact:{}:{hairline}", row.title),
                    Item::Footer { source } => format!("footer{source}"),
                    Item::Spacer => "spacer".into(),
                    other => format!("{other:?}"),
                })
                .collect();
            assert_eq!(
                kinds,
                [
                    "title", "section0", "hero:h1", "compact:h2:true", "compact:h3:true", "footer0", "section2", "hero:g1", "compact:g2:true",
                    "compact:g3:true", "footer2", "spacer"
                ]
            );
        });
    }

    #[test]
    fn the_bar_switches_pages_and_picking_the_same_page_again_keeps_it() {
        with_view(Hosting::Standalone, |cx, root| {
            with(root, |v| v.pick_root(cx, Root::Saved));
            {
                let view = root.borrow::<NewsView>().unwrap();
                assert_eq!(view.nav().page(), Page::Saved);
                assert!(matches!(&view.items()[1], Item::Empty { text } if text.contains("save")));
            }
            with(root, |v| v.pick_root(cx, Root::Following));
            {
                let view = root.borrow::<NewsView>().unwrap();
                assert_eq!(view.nav().page(), Page::Following);
                assert_eq!(view.items().iter().filter(|i| matches!(i, Item::Source { .. })).count(), 3, "the built-in three");
                assert!(view.items().iter().any(|i| matches!(i, Item::AddFeed)));
            }
            with(root, |v| v.pick_root(cx, Root::Following));
            assert_eq!(root.borrow::<NewsView>().unwrap().nav().page(), Page::Following);
            with(root, |v| v.pick_root(cx, Root::Today));
            assert_eq!(root.borrow::<NewsView>().unwrap().nav().page(), Page::Today);
        });
    }

    #[test]
    fn a_source_page_pushes_over_the_root_and_back_pops_it() {
        with_view(Hosting::Standalone, |cx, root| {
            land(cx, root, 1, "t");
            with(root, |v| v.push_source(cx, 1));
            {
                let view = root.borrow::<NewsView>().unwrap();
                assert_eq!(view.nav().page(), Page::Source(1));
                assert_eq!(view.model().tab, 2, "the tick refreshes the page's source");
                assert!(matches!(view.items().first(), Some(Item::Title { text, below_bar: true, .. }) if text == "TechMeme"));
                assert!(matches!(&view.items()[1], Item::Hero { row } if row.title == "t1"));
                assert!(matches!(view.items().iter().rev().nth(1), Some(Item::CapBottom)));
            }
            assert!(visible(cx, root, ids!(back)));
            assert!(with(root, |v| v.back(cx)), "back pops the page");
            let view = root.borrow::<NewsView>().unwrap();
            assert_eq!((view.nav().page(), view.model().tab), (Page::Today, 0));
            drop(view);
            assert!(!with(root, |v| v.back(cx)), "nothing left to pop: the host's press");
            with(root, |v| v.push_source(cx, 99));
            assert_eq!(root.borrow::<NewsView>().unwrap().nav().page(), Page::Today, "no such source");
        });
    }

    #[test]
    fn the_back_press_event_is_handled_only_when_something_closes() {
        with_view(Hosting::Standalone, |cx, root| {
            let press = || Event::BackPressed { handled: std::cell::Cell::new(false) };
            let event = press();
            root.handle_event(cx, &event, &mut Scope::empty());
            assert!(matches!(&event, Event::BackPressed { handled } if !handled.get()), "Today: left to the host");
            with(root, |v| v.push_source(cx, 0));
            let event = press();
            root.handle_event(cx, &event, &mut Scope::empty());
            assert!(matches!(&event, Event::BackPressed { handled } if handled.get()));
            assert_eq!(root.borrow::<NewsView>().unwrap().nav().page(), Page::Today);
        });
    }

    #[test]
    fn search_filters_the_followed_sources_rows_as_typed() {
        with_view(Hosting::Standalone, |cx, root| {
            land(cx, root, 0, "h");
            land(cx, root, 1, "t");
            with(root, |v| v.open_search(cx));
            assert!(visible(cx, root, ids!(search_bar)));
            assert!(!visible(cx, root, ids!(bar_row)), "the bar makes room for the keyboard");
            {
                let view = root.borrow::<NewsView>().unwrap();
                assert_eq!(view.nav().page(), Page::Search);
                assert!(matches!(view.items().first(), Some(Item::Empty { .. })), "no query yet");
            }
            with(root, |v| v.set_query(cx, "T2"));
            {
                let view = root.borrow::<NewsView>().unwrap();
                let titles: Vec<&str> = view.items().iter().filter_map(Item::row).map(|r| r.title.as_str()).collect();
                assert_eq!(titles, ["t2"]);
                assert!(matches!(view.items().first(), Some(Item::CapTop)));
            }
            with(root, |v| v.set_followed(cx, 1, false));
            assert!(root.borrow::<NewsView>().unwrap().items().iter().any(|i| matches!(i, Item::Empty { text } if text.contains("No results"))), "a hidden source is not searched");
            with(root, |v| v.set_query(cx, "zzz"));
            assert!(with(root, |v| v.back(cx)));
            let view = root.borrow::<NewsView>().unwrap();
            assert_eq!(view.nav().page(), Page::Today);
            assert!(view.query.is_empty(), "leaving Search drops the query");
        });
    }

    /// Following writes the hidden ids and Saved the stories to the jail;
    /// the form adds a feed and writes the feeds file.
    #[test]
    fn following_saved_and_the_form_write_their_files() {
        with_view(Hosting::Standalone, |cx, root| {
            let storage = cx.storage("news.view.test");
            with(root, |v| v.set_storage(cx, storage));
            with(root, |v| v.writes.clear());
            with(root, |v| v.set_followed(cx, 2, false));
            assert_eq!(with(root, |v| v.pending_writes()), [HIDDEN_KEY]);
            assert!(!with(root, |v| v.model().is_followed(2)));
            with(root, |v| v.writes.clear());
            let saved = with(root, |v| v.toggle_saved(cx, &row("a")));
            assert!(saved);
            assert_eq!(with(root, |v| v.pending_writes()), [SAVED_KEY]);
            with(root, |v| v.pick_root(cx, Root::Saved));
            assert!(with(root, |v| v.items().iter().any(|i| matches!(i, Item::Compact { row, hairline: false } if row.title == "a"))));
            with(root, |v| v.writes.clear());
            assert!(!with(root, |v| v.add_feed(cx, "", "nope")));
            assert_eq!(with(root, |v| v.feed_error.clone()).as_deref().map(|e| e.starts_with("Enter")), Some(true));
            assert!(with(root, |v| v.pending_writes()).is_empty());
            assert!(with(root, |v| v.add_feed(cx, "Blog", "https://blog.example/feed")));
            assert_eq!(with(root, |v| v.pending_writes()), [FEEDS_KEY]);
            assert_eq!(with(root, |v| (v.model().sources.len(), v.feed_error.clone())), (4, None));
            assert!(with(root, |v| v.model().sources[3].pending.is_some()), "the new feed is fetched at once");
            with(root, |v| v.push_source(cx, 3));
            with(root, |v| v.remove_feed(cx, 3));
            let view = root.borrow::<NewsView>().unwrap();
            assert_eq!(view.model().sources.len(), 3);
            assert_eq!(view.nav().page(), Page::Saved, "the removed feed's page went with it; the root beneath shows");
        });
    }

    #[test]
    fn the_sheet_opens_from_a_story_and_saves_or_dismisses() {
        with_view(Hosting::Standalone, |cx, root| {
            let story = row("s");
            with(root, |v| v.open_sheet(cx, story.clone()));
            assert!(visible(cx, root, ids!(sheet_layer)));
            assert_eq!(with(root, |v| v.sheet_row().cloned()), Some(story.clone()));
            assert!(with(root, |v| v.back(cx)), "back dismisses the sheet first");
            assert!(!visible(cx, root, ids!(sheet_layer)));
            assert_eq!(with(root, |v| v.sheet_row().cloned()), None);
        });
    }

    /// A story opens in the reader first, standalone and as a module: the
    /// host is not asked for the Browser.
    #[test]
    fn a_tap_opens_the_reader_first_wherever_there_is_one() {
        for hosting in [Hosting::Standalone, Hosting::Module] {
            with_view(hosting, |cx, root| {
                let req = posted_request(cx, root, |cx, view| view.open_link(cx, "https://x/a"));
                let view = root.borrow::<NewsView>().unwrap();
                if reader_here(hosting) {
                    assert_eq!(req, None, "{hosting:?}: the reader, not the host");
                    assert!(view.reader_open(cx));
                    assert_eq!(view.pending_open_url(), None);
                    drop(view);
                    assert!(visible(cx, root, ids!(reader)));
                    assert!(!visible(cx, root, ids!(body)), "the page waits behind the article");
                    assert!(!visible(cx, root, ids!(chrome)), "and so does the bar");
                    root.borrow_mut::<NewsView>().unwrap().close_reader(cx);
                    assert!(!root.borrow::<NewsView>().unwrap().reader_open(cx));
                    assert!(visible(cx, root, ids!(body)), "the page is back");
                } else if hosting == Hosting::Module {
                    assert_eq!(req, Some(WmRequest::Open { app: Some("browser".into()), path: "https://x/a".into() }), "no web view: the Browser");
                }
            });
        }
    }

    /// `Open in Browser` asks the host past the reader; the host's
    /// `wm_unavailable` for that link moves on to the next tier.
    #[test]
    fn open_in_browser_asks_the_host_and_falls_back_on_the_reply() {
        with_view(Hosting::Module, |cx, root| {
            let req = posted_request(cx, root, |cx, view| view.open_in_browser(cx, "https://x/a"));
            assert_eq!(req, Some(WmRequest::Open { app: Some("browser".into()), path: "https://x/a".into() }));
            {
                let view = root.borrow::<NewsView>().unwrap();
                assert!(!view.reader_open(cx), "the person asked for the Browser, not the reader");
                assert_eq!(view.pending_open_url(), Some("https://x/a"), "the link waits for the answer");
            }
            root.handle_event(cx, &unavailable("https://x/b"), &mut Scope::empty());
            assert_eq!(root.borrow::<NewsView>().unwrap().pending_open_url(), Some("https://x/a"), "a reply for another link is not ours");
            root.handle_event(cx, &unavailable("https://x/a"), &mut Scope::empty());
            let view = root.borrow::<NewsView>().unwrap();
            assert_eq!(view.pending_open_url(), None, "the reply settled the link");
            assert!(!view.reader_open(cx), "the reader is not in the Browser's chain");
        });
    }

    /// The host answers a module's request during the actions pass that
    /// carries it, and a Browser that launched never answers: the link
    /// waits through that pass and is forgotten on the next, so a late
    /// reply cannot act on a tap long settled.
    #[test]
    fn a_module_forgets_an_unanswered_browser_request_after_the_hosts_pass() {
        with_view(Hosting::Module, |cx, root| {
            let carrying = posted_actions(cx, root, |cx, view| view.open_in_browser(cx, "https://x/a"));
            assert!(carrying.iter().filter_map(|a| a.as_widget_action()).any(|wa| wa.action.downcast_ref::<WmRequest>().is_some()));
            root.handle_event(cx, &Event::Actions(carrying), &mut Scope::empty());
            assert_eq!(root.borrow::<NewsView>().unwrap().pending_open_url(), Some("https://x/a"), "the host may still answer during this pass");
            root.handle_event(cx, &Event::Actions(ActionsBuf::new()), &mut Scope::empty());
            assert_eq!(root.borrow::<NewsView>().unwrap().pending_open_url(), None, "no answer came: the Browser has the link");
        });
    }

    #[test]
    fn the_last_tier_names_the_browser_and_the_links_host() {
        let notice = browser_failed_notice("https://www.example.com/a/b");
        assert_eq!(notice, WmRequest::Notify { title: "Could not open in the Browser".into(), body: "example.com".into() });
    }

    /// A tap on the tile is the host's (it opens the full app); the host
    /// switching to the tile takes an open article and the sheet with it.
    #[test]
    fn the_tile_face_never_opens_a_link_and_closes_an_open_one() {
        with_view(Hosting::Standalone, |cx, root| {
            with(root, |v| v.open_sheet(cx, row("s")));
            if reader_here(Hosting::Standalone) {
                root.borrow_mut::<NewsView>().unwrap().open_link(cx, "https://x/a");
                assert!(root.borrow::<NewsView>().unwrap().reader_open(cx));
                assert_eq!(with(root, |v| v.sheet_row().cloned()), None, "opening a story takes the sheet down");
                with(root, |v| v.open_sheet(cx, row("s")));
            }
            root.handle_event(cx, &Event::Custom(HostedViewMode::Tile.to_json()), &mut Scope::empty());
            assert!(!root.borrow::<NewsView>().unwrap().reader_open(cx), "the overlay would outlive the face");
            assert_eq!(with(root, |v| v.sheet_row().cloned()), None);
            let req = posted_request(cx, root, |cx, view| view.open_link(cx, "https://x/b"));
            assert_eq!(req, None);
            let view = root.borrow::<NewsView>().unwrap();
            assert_eq!(view.face(cx), HostedViewMode::Tile);
            assert!(!view.reader_open(cx));
            assert_eq!(view.pending_open_url(), None, "a tile tap is not an open");
        });
    }

    /// One tick of the reader's own watchdog timer.
    fn tick_for(timer: Timer) -> Event {
        Event::Timer(TimerEvent { timer_id: timer.0, time: None })
    }

    /// The overlay is the window's, not the tile's: when the tile stops
    /// being drawn (a hidden workspace, another tile gone fullscreen) the
    /// reader's watchdog takes the web view off the window. The pane stays
    /// open, the watch keeps running, and the next draw puts the overlay
    /// back.
    #[test]
    fn an_open_reader_that_is_not_drawn_hides_its_overlay() {
        with_view(Hosting::Standalone, |cx, root| {
            if !reader_here(Hosting::Standalone) {
                return;
            }
            root.borrow_mut::<NewsView>().unwrap().open_link(cx, "https://x/a");
            let reader = root.borrow::<NewsView>().unwrap().view.widget(cx, ids!(reader));
            let state = |cx: &Cx| {
                let r = reader.borrow::<ArticleReader>().unwrap();
                (r.overlay_visible(), r.watchdog_timer().map(|t| t.0), root.borrow::<NewsView>().unwrap().reader_open(cx))
            };
            let (visible, timer, open) = state(cx);
            assert!(visible && open, "opening puts the overlay on the window");
            let timer = Timer(timer.expect("the watchdog runs while the pane is open"));
            root.handle_event(cx, &tick_for(timer), &mut Scope::empty());
            let (visible, running, open) = state(cx);
            assert!(visible && open);
            assert_eq!(running, Some(timer.0), "the same timer keeps running");
            root.handle_event(cx, &tick_for(timer), &mut Scope::empty());
            let (visible, running, open) = state(cx);
            assert!(!visible, "the overlay is taken off the window");
            assert!(open, "the pane itself stays open");
            assert_eq!(running, Some(timer.0), "and the watchdog keeps looking for the tile's return");
            root.borrow_mut::<NewsView>().unwrap().close_reader(cx);
            let (visible, running, open) = state(cx);
            assert!(!visible && !open && running.is_none());
        });
    }

    #[test]
    fn a_closed_reader_runs_no_timer() {
        with_view(Hosting::Standalone, |cx, root| {
            let reader = root.borrow::<NewsView>().unwrap().view.widget(cx, ids!(reader));
            let r = reader.borrow::<ArticleReader>().unwrap();
            assert!(r.watchdog_timer().is_none());
            assert!(!r.overlay_visible());
        });
    }

    /// Only http(s) links leave the list: the one seam where a row's link
    /// reaches a browser refuses anything else.
    #[test]
    fn open_link_refuses_anything_but_http_links() {
        with_view(Hosting::Module, |cx, root| {
            for bad in ["javascript:alert(1)", "file:///etc/passwd", "not a url", ""] {
                let req = posted_request(cx, root, |cx, view| view.open_in_browser(cx, bad));
                assert_eq!(req, None, "{bad:?} asks nobody");
                let view = root.borrow::<NewsView>().unwrap();
                assert!(!view.reader_open(cx), "{bad:?} opens nothing");
                assert_eq!(view.pending_open_url(), None, "{bad:?} is not pending");
            }
            let req = posted_request(cx, root, |cx, view| view.open_in_browser(cx, "HTTP://x/a"));
            assert!(matches!(req, Some(WmRequest::Open { .. })), "the scheme check is case-insensitive");
        });
    }
}
