use crate::view::PhotosView;
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.PhotosLabel = Label {
        padding: 0 align: Align{y: 0.5}
        draw_text.color: #202124
        draw_text.text_style: theme.font_regular{font_size: 14}
    }
    mod.widgets.PhotosButton = Button {
        align: Align{x: 0.5 y: 0.5} spacing: 0
        height: 44 width: Fit margin: 0 padding: Inset{left: 14 right: 14 top: 8 bottom: 8}
        draw_text +: { color: #007aff color_hover: #007aff color_down: #005bc4 color_focus: #007aff
            text_style: theme.font_regular{font_size: 14} }
        draw_bg +: {
            color: #f0f2f5 border_size: 0 border_radius: 22
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0.0,0.0,self.rect_size.x,self.rect_size.y,22.0)
                sdf.fill(mix(#f0f2f5,#dce6f2,self.down))
                return sdf.result
            }
        }
    }
    mod.widgets.PhotosIconButton = mod.widgets.PhotosButton {
        width: 44 height: 44 padding: 0 text: ""
        label_walk: Walk{width: 0 height: 0}
        icon_walk: Walk{width: 22 height: 22}
        draw_icon.color: #007aff
        draw_icon.preserve_viewbox: true
    }
    mod.widgets.PhotosRoundImage = Image {
        width: Fill height: Fill fit: ImageFit.CropToFill
        draw_bg +: {
            radius: 14.0
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0.0,0.0,self.rect_size.x,self.rect_size.y,self.radius)
                sdf.fill(self.get_color_scale_pan(self.fit_scale,vec2(self.fit_pan.x,0.0)))
                return sdf.result
            }
        }
    }
    mod.widgets.PhotosCell = View {
        width: Fill height: Fill flow: Overlay cursor: MouseCursor.Hand grab_key_focus: false
        image := Image { width: Fill height: Fill fit: ImageFit.CropToFill }
        marker := View {
            width: Fill height: Fill padding: 8 align: Align{x: 1 y: 1}
            mark := Label {padding: 0 text: "" draw_text.color: #fff draw_text.text_style: theme.font_bold{font_size: 20}}
        }
    }
    mod.widgets.PhotosAlbumTile = View {
        width: Fill height: Fill flow: Down spacing: 5 cursor: MouseCursor.Hand grab_key_focus: false
        cover := mod.widgets.PhotosRoundImage {height: Fill}
        title := mod.widgets.PhotosLabel {width: Fill height: 22 max_lines: 1 text_overflow: Ellipsis draw_text.text_style: theme.font_bold{font_size: 14}}
        count := mod.widgets.PhotosLabel {height: 18 draw_text.color: #85858b draw_text.text_style.font_size: 12}
    }
    mod.widgets.PhotosView = set_type_default() do #(PhotosView::register_widget(vm)) {
        ..RectView
        width: Fill height: Fill flow: Down draw_bg.color: #fff
        compact := View {
            visible: false width: Fill height: Fill flow: Overlay
            photos := View {width: Fill height: Fill flow: Right spacing: 2
                preview_first := mod.widgets.PhotosRoundImage {draw_bg.radius: 0.0}
                preview_second := mod.widgets.PhotosRoundImage {draw_bg.radius: 0.0}
                preview_third := mod.widgets.PhotosRoundImage {draw_bg.radius: 0.0}
            }
            shade := View {width: Fill height: Fill show_bg: true
                draw_bg +: {pixel: fn() {return vec4(0.0,0.0,0.0,pow(self.pos.y,3.0)*0.6)}}
            }
            caption := View {width: Fill height: Fill padding: 14 align: Align{y: 1}
                label := mod.widgets.PhotosLabel {text: "Photos" draw_text.color: #fff draw_text.text_style: theme.font_bold{font_size: 16}}
            }
        }
        header := View {
            width: Fill height: 82 flow: Right padding: Inset{left: 20 right: 16 top: 12 bottom: 8} spacing: 10 align: Align{y: 0.5}
            back := mod.widgets.PhotosIconButton {visible: false}
            titles := View {width: Fill height: Fit flow: Down spacing: 4
                title := mod.widgets.PhotosLabel {width: Fill max_lines: 1 text_overflow: Ellipsis text: "Collections" draw_text.text_style: theme.font_bold{font_size: 30}}
                subtitle := mod.widgets.PhotosLabel {width: Fill max_lines: 1 text_overflow: Ellipsis text: "Your favorite moments, together" draw_text.color: #85858b draw_text.text_style.font_size: 12}
            }
            create := mod.widgets.PhotosIconButton {}
            edit := mod.widgets.PhotosButton {visible: false text: "Edit"}
            save := mod.widgets.PhotosButton {visible: false text: "Save"}
        }
        search_bar := View {visible: false width: Fill height: 54 padding: Inset{left: 18 right: 18 top: 2 bottom: 8}
            search_input := TextInput {
                width: Fill height: 44 empty_text: "People, places, dates…"
                draw_text.color: #222 draw_text.text_style.font_size: 14
                draw_bg +: {pixel: fn() {let sdf=Sdf2d.viewport(self.pos*self.rect_size); sdf.box(0.0,0.0,self.rect_size.x,self.rect_size.y,14.0); sdf.fill(#f0f2f5); return sdf.result}}
            }
        }
        editor_bar := View {visible: false width: Fill height: 100 flow: Down padding: Inset{left: 18 right: 18 top: 0 bottom: 8} spacing: 10
            name_input := TextInput {
                width: Fill height: 44 empty_text: "Album name"
                draw_text.color: #222 draw_text.text_style.font_size: 16
                draw_bg +: {pixel: fn() {let sdf=Sdf2d.viewport(self.pos*self.rect_size); sdf.box(0.0,0.0,self.rect_size.x,self.rect_size.y,12.0); sdf.fill(#f0f2f5); return sdf.result}}
            }
            editor_caption := mod.widgets.PhotosLabel {text: "Select photos for this album" draw_text.color: #77777f draw_text.text_style.font_size: 13}
        }
        message := mod.widgets.PhotosLabel {visible: false width: Fill margin: Inset{left: 18 right: 18 bottom: 8} draw_text.wrap: Words draw_text.color: #c14444 text: ""}
        // Desktop zoom: the same density the pinch reaches, on a knob.
        zoom_bar := View {visible: false width: Fill height: 40 flow: Right spacing: 12 padding: Inset{left: 20 right: 20 top: 0 bottom: 6} align: Align{y: 0.5}
            zoom_caption := mod.widgets.PhotosLabel {text: "Zoom" draw_text.color: #85858b draw_text.text_style.font_size: 12}
            zoom_slider := Slider {width: Fill height: 28 min: 0.0 max: 1.0 default: 0.5 text: ""}
        }
        body := View {width: Fill height: Fill flow: Overlay
            list := PortalList {
                width: Fill height: Fill
                Heading := View {width: Fill height: 48 flow: Right align: Align{y: 0.5} padding: Inset{left: 20 right: 20 top: 12 bottom: 8}
                    title := mod.widgets.PhotosLabel {width: Fill text: "Memories" draw_text.text_style: theme.font_bold{font_size: 21}}
                    detail := mod.widgets.PhotosLabel {text: "" draw_text.color: #8a8a91 draw_text.text_style.font_size: 12}
                }
                Memory := View {width: Fill height: 280 padding: Inset{left: 18 right: 18 bottom: 6}
                    card := View {width: Fill height: Fill flow: Overlay cursor: MouseCursor.Hand grab_key_focus: false
                        image := mod.widgets.PhotosRoundImage {}
                        shade := View {width: Fill height: Fill show_bg: true
                            draw_bg +: {pixel: fn() {let sdf=Sdf2d.viewport(self.pos*self.rect_size); sdf.box(0.0,0.0,self.rect_size.x,self.rect_size.y,14.0); sdf.fill(vec4(0.0,0.0,0.0,pow(self.pos.y,3.0)*0.7)); return sdf.result}}
                        }
                        text_box := View {width: Fill height: Fill flow: Down padding: 18 align: Align{y: 1} spacing: 6
                            eyebrow := Label {padding: 0 text: "MEMORY" draw_text.color: #ffffffb0 draw_text.text_style: theme.font_bold{font_size: 10}}
                            title := Label {padding: 0 width: Fill draw_text.wrap: Words text: "The people we love" draw_text.color: #fff draw_text.text_style: theme.font_bold{font_size: 25}}
                            date := Label {padding: 0 text: "SEPTEMBER 2026" draw_text.color: #ffffffd0 draw_text.text_style.font_size: 11}
                        }
                        play_box := View {width: Fill height: Fill padding: 14 align: Align{x: 1 y: 0}
                            play := Label {padding: 0 text: "▶" draw_text.color: #fff draw_text.text_style.font_size: 22}
                        }
                    }
                }
                Albums := View {width: Fill height: 204 flow: Right spacing: 12 padding: Inset{left: 18 right: 18 top: 4 bottom: 10}
                    first := mod.widgets.PhotosAlbumTile {}
                    second := mod.widgets.PhotosAlbumTile {}
                }
                People := View {width: Fill height: 156 flow: Right spacing: 10 padding: Inset{left: 18 right: 18 top: 2 bottom: 12}
                    first := mod.widgets.PhotosAlbumTile {count.visible: false title.align: Align{x: 0.5 y: 0.5}}
                    second := mod.widgets.PhotosAlbumTile {count.visible: false title.align: Align{x: 0.5 y: 0.5}}
                    third := mod.widgets.PhotosAlbumTile {count.visible: false title.align: Align{x: 0.5 y: 0.5}}
                }
                // Nine reusable cells; the view shows as many as the zoom asks for.
                Grid := View {width: Fill height: 132 flow: Right spacing: 2 padding: Inset{bottom: 2}
                    first := mod.widgets.PhotosCell {}
                    second := mod.widgets.PhotosCell {}
                    third := mod.widgets.PhotosCell {}
                    fourth := mod.widgets.PhotosCell {visible: false}
                    fifth := mod.widgets.PhotosCell {visible: false}
                    sixth := mod.widgets.PhotosCell {visible: false}
                    seventh := mod.widgets.PhotosCell {visible: false}
                    eighth := mod.widgets.PhotosCell {visible: false}
                    ninth := mod.widgets.PhotosCell {visible: false}
                }
                Utility := View {width: Fill height: 64 padding: Inset{left: 18 right: 18 top: 4 bottom: 8}
                    favorite_collection := mod.widgets.PhotosButton {width: Fill height: Fill text: "Favorites"}
                }
                Empty := View {width: Fill height: 190 padding: 28 align: Align{x: 0.5 y: 0.5}
                    title := mod.widgets.PhotosLabel {width: Fill align: Align{x: 0.5 y: 0.5} draw_text.wrap: Words draw_text.color: #85858b text: "No photos yet"}
                }
                End := View {width: Fill height: 34 align: Align{x: 0.5 y: 0.5}
                    title := mod.widgets.PhotosLabel {draw_text.color: #98989f draw_text.text_style.font_size: 12 text: ""}
                }
            }
            viewer := RectView {visible: false width: Fill height: Fill flow: Down draw_bg.color: #0b0c0f
                stage := View {width: Fill height: Fill align: Align{x: 0.5 y: 0.5} cursor: MouseCursor.Hand grab_key_focus: false
                    full_image := Image {width: Fill height: Fill fit: ImageFit.Smallest}
                }
                memory_caption := View {visible: false width: Fill height: 96 flow: Down padding: 18 spacing: 6 align: Align{x: 0.5 y: 0.5}
                    memory_title := Label {padding: 0 width: Fill align: Align{x: 0.5 y: 0.5} draw_text.wrap: Words text: "" draw_text.color: #fff draw_text.text_style: theme.font_bold{font_size: 24}}
                    memory_date := Label {padding: 0 text: "" draw_text.color: #bbbbc0 draw_text.text_style.font_size: 12}
                }
                viewer_controls := View {width: Fill height: 64 flow: Right padding: 10 spacing: 10 align: Align{x: 0.5 y: 0.5}
                    previous := mod.widgets.PhotosIconButton {}
                    favorite := mod.widgets.PhotosIconButton {}
                    play_pause := mod.widgets.PhotosButton {visible: false text: "Pause"}
                    position := Label {padding: 0 text: "1 / 19" draw_text.color: #ccc draw_text.text_style.font_size: 12}
                    next := mod.widgets.PhotosIconButton {}
                }
            }
        }
        editor_footer := View {visible: false width: Fill height: 64 flow: Right padding: Inset{left: 18 right: 18 top: 8 bottom: 12} spacing: 12
            cancel := mod.widgets.PhotosButton {text: "Cancel" width: Fill}
            delete := mod.widgets.PhotosButton {text: "Delete album" width: Fill draw_text.color: #df4242}
        }
        footer := View {width: Fill height: 80 flow: Right padding: Inset{left: 18 right: 18 top: 10 bottom: 14} spacing: 14 align: Align{x: 0.5 y: 0.5}
            navigation := KitBottomNavigation {
                width: Fill height: 54 flow: Right padding: 3 spacing: 2
                show_bg: true
                draw_bg +: {pixel: fn() {let sdf=Sdf2d.viewport(self.pos*self.rect_size); sdf.box(0.0,0.0,self.rect_size.x,self.rect_size.y,27.0); sdf.fill(#f0f2f5); return sdf.result}}
                contract: "{\"selected_index\":1,\"source_selected_index\":-1,\"active_color\":4278221567,\"inactive_color\":4285032558,\"active_surface\":4294967295,\"inactive_surface\":4293980917,\"items\":[{\"root\":\"library_root\",\"control\":\"library_tab\",\"paint\":[\"library_label\"],\"surfaces\":[\"library_root\"]},{\"root\":\"collections_root\",\"control\":\"collections_tab\",\"paint\":[\"collections_label\"],\"surfaces\":[\"collections_root\"]}]}"
                library_root := View {
                    width: Fill height: Fill flow: Overlay show_bg: true align: Align{x: 0.5 y: 0.5}
                    draw_bg +: {color: instance(#fff) pixel: fn() {let sdf=Sdf2d.viewport(self.pos*self.rect_size); sdf.box(0.0,0.0,self.rect_size.x,self.rect_size.y,24.0); sdf.fill(self.color); return sdf.result}}
                    library_tab := mod.widgets.PhotosButton {width: Fill height: Fill text: "" draw_bg +: {pixel: fn() {return vec4(0.0)}}}
                    library_label := mod.widgets.PhotosLabel {text: "Library" draw_text.text_style: theme.font_bold{font_size: 13}}
                }
                collections_root := View {
                    width: Fill height: Fill flow: Overlay show_bg: true align: Align{x: 0.5 y: 0.5}
                    draw_bg +: {color: instance(#fff) pixel: fn() {let sdf=Sdf2d.viewport(self.pos*self.rect_size); sdf.box(0.0,0.0,self.rect_size.x,self.rect_size.y,24.0); sdf.fill(self.color); return sdf.result}}
                    collections_tab := mod.widgets.PhotosButton {width: Fill height: Fill text: "" draw_bg +: {pixel: fn() {return vec4(0.0)}}}
                    collections_label := mod.widgets.PhotosLabel {text: "Collections" draw_text.text_style: theme.font_bold{font_size: 13}}
                }
            }
            search := mod.widgets.PhotosIconButton {width: 52 height: 52}
        }
    }
}
