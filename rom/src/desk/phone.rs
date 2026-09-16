use super::*;
use crate::mobile::{self, PhoneHit, PhoneScreen};
use crate::mobile_tiles::{Face, TILE_RADIUS};

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    set_type_default() do #(DrawPhoneApp::script_shader(vm)) {
        ..mod.draw.DrawQuad
        image: texture_2d(float)
        opacity: 1.0 radius: 0.0 y_flip: 0.0
        // The part of the image this quad shows: a band of a cached scene
        // can be drawn without the rest.
        uv_pos: vec2(0.0, 0.0) uv_size: vec2(1.0, 1.0)
        pixel: fn() {
            let sdf=Sdf2d.viewport(self.pos*self.rect_size)
            sdf.box(0.0,0.0,self.rect_size.x,self.rect_size.y,self.radius)
            let uv=self.uv_pos+vec2(self.pos.x,mix(self.pos.y,1.0-self.pos.y,self.y_flip))*self.uv_size
            sdf.fill(self.image.sample(uv)*self.opacity)
            return sdf.result
        }
    }
}
#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawPhoneApp {
    #[deref] draw_super: DrawQuad,
    #[live] opacity: f32,
    #[live] radius: f32,
    #[live] y_flip: f32,
    #[live] uv_pos: Vec2f,
    #[live] uv_size: Vec2f,
}

/// One off-screen capture of a client's tile widget at one viewport.
struct Capture {
    frame: WindowFrame,
    dirty: bool,
    size: Vec2d,
    style: crate::desktop::DesktopStyle,
    dark: bool,
}
impl Capture {
    fn new(cx: &mut Cx, style: crate::desktop::DesktopStyle, dark: bool) -> Self {
        Self { frame: WindowFrame::new_with_name(cx, "wm_phone_capture"), dirty: true, size: dvec2(0.0, 0.0), style, dark }
    }
    fn stale(&self, size: Vec2d, style: crate::desktop::DesktopStyle, dark: bool) -> bool {
        self.dirty || self.size != size || self.style != style || self.dark != dark
    }
    fn settle(&mut self, size: Vec2d, style: crate::desktop::DesktopStyle, dark: bool) {
        self.dirty = false;
        self.size = size;
        self.style = style;
        self.dark = dark;
    }
}

/// A client's captures on the phone. The full-screen face (Recents cards,
/// the open/close warp) and the compact home-tile face are kept apart: a
/// tile-face frame never lands in a card, and a window frame never lands in
/// a tile, whatever the client happens to present while it switches.
#[derive(Default)]
pub(super) struct PhoneFrame {
    full: Option<Capture>,
    tile: Option<Capture>,
}

/// The still home scene under a frosted overlay — the shade's sheet, Recents'
/// overview glass, a tile group's window — and its blur pyramid, kept for the
/// overlay's whole transition. Recorded on idle home frames too, so the first
/// moving frame already finds it. `key` is None whenever the scene may differ.
pub(super) struct PhoneSceneBackdrop {
    frame: WindowFrame,
    key: Option<(Rect, Rect, f64, crate::desktop::DesktopStyle, bool)>,
    blur: Option<GaussBlurSnapshot>,
}

impl WmDesk {
    pub(super) fn draw_window_surface(&mut self, cx: &mut Cx2d, frame: &WindowFrame, rect: Rect, radius: f32) {
        self.draw_window_surface_band(cx, frame, rect, rect, radius);
    }
    /// `band` of the frame recorded over `rect`, drawn in place: the rows a
    /// cached scene still shows beside an opaque sheet.
    fn draw_window_surface_band(&mut self, cx: &mut Cx2d, frame: &WindowFrame, rect: Rect, band: Rect, radius: f32) {
        if band.size.x < 0.5 || band.size.y < 0.5 { return; }
        self.draw_phone.draw_vars.set_texture(0, frame.texture());
        self.draw_phone.opacity = 1.0;
        // Sdf2d.box uses half the visible corner radius.
        self.draw_phone.radius = radius * 0.5;
        // WindowFrame render targets already have top-left rows, including
        // the pinned Android GL backend. Keep pixels aligned with input.
        self.draw_phone.y_flip = 0.0;
        let size = dvec2(rect.size.x.max(1.0), rect.size.y.max(1.0));
        self.draw_phone.uv_pos = vec2f(((band.pos.x - rect.pos.x) / size.x) as f32, ((band.pos.y - rect.pos.y) / size.y) as f32);
        self.draw_phone.uv_size = vec2f((band.size.x / size.x) as f32, (band.size.y / size.y) as f32);
        self.draw_phone.draw_abs(cx, band);
        self.draw_phone.uv_pos = vec2f(0.0, 0.0);
        self.draw_phone.uv_size = vec2f(1.0, 1.0);
    }
    fn client_arriving(&self, client: ClientId) -> bool {
        self.items.get(&client).and_then(|item| item.borrow::<MpRunView>())
            .is_some_and(|view| view.arrival_fade() < 1.0)
    }
    pub fn phone_hit(&self,p:Vec2d)->Option<PhoneHit> {self.phone_ui.hit(p)}
    pub fn phone_search_event(&mut self,cx:&mut Cx,event:&Event,state:&mut WmState)->bool {
        let enabled=state.style.target.mobile() && state.phone.screen==PhoneScreen::Drawer;
        self.phone_ui.search_event(cx,event,&mut state.phone,enabled)
    }
    pub fn dismiss_phone_search(&mut self,cx:&mut Cx,phone:&mut crate::mobile::PhoneState,clear:bool) {
        self.phone_ui.dismiss_search(cx,phone,clear);
    }
    pub fn clear_phone_search(&mut self,cx:&mut Cx,phone:&mut crate::mobile::PhoneState) {
        self.phone_ui.clear_search(cx,phone);
    }
    pub fn focus_phone_search(&mut self,cx:&mut Cx,phone:&mut crate::mobile::PhoneState) {
        self.phone_ui.focus_search(cx,phone);
    }
    pub fn phone_search_scroll_max(&self)->f64 {self.phone_ui.search_scroll_max}
    /// A frame from `client` landed in the given face: that face's capture
    /// re-records on the next draw. A frame that belongs to neither (a stale
    /// size while the client switches faces) is left out of both.
    pub fn note_client_frame(&mut self,client:ClientId,face:Option<Face>) {
        let Some(frame)=self.phone_frames.get_mut(&client) else {return};
        match face {
            Some(Face::Full)=>{if let Some(c)=frame.full.as_mut() {c.dirty=true;}}
            Some(Face::Tile)=>{if let Some(c)=frame.tile.as_mut() {c.dirty=true;}}
            None=>{}
        }
    }
    /// Record `client`'s tile widget into `capture` at `rect`, configuring
    /// the child for exactly that viewport.
    fn record_capture(&mut self,cx:&mut Cx2d,scope:&mut Scope,client:ClientId,capture:&mut Capture,rect:Rect,wash:bool) {
        capture.frame.begin(cx,rect);
        if wash {
            self.draw_panel.alpha=1.0;
            self.draw_panel.draw_abs(cx,rect);
        }
        if let Some(item)=self.item(cx,client) {
            with_tile_host(&item,|tile| {tile.set_target_size(Some(rect.size));tile.set_close_crop(None);tile.set_fade(1.0);});
            let t=std::time::Instant::now();
            item.draw_walk_all(cx,scope,Walk::abs_rect(rect));
            if crate::mobile_perf::enabled() {let ch=crate::mobile_perf::channels(cx.cx);crate::mobile_perf::span(cx.cx,ch.module,t);}
        }
        capture.frame.end(cx);
        if self.phone_compose {self.compositor.as_mut().unwrap().content_pass(capture.frame.pass_id());}
    }
    /// A composited object covered `rect` this frame: the compositor's
    /// backdrop reuse needs to know, on the frames that compose.
    pub(crate) fn phone_content(&mut self,rect:Rect) {
        if self.phone_compose {if let Some(c)=self.compositor.as_mut() {c.content(rect);}}
    }
    /// A group member's live look (mobile_groups.rs): its compact capture,
    /// else its full one, fitted into `cell` by aspect. False without one.
    pub(crate) fn present_member_capture(&mut self,cx:&mut Cx2d,client:ClientId,cell:Rect,opacity:f32,radius:f32)->bool {
        let Some(stored)=self.phone_frames.remove(&client) else {return false};
        let shown=stored.tile.as_ref().or(stored.full.as_ref()).filter(|c|c.size.x>=1.0).map(|c|(c.size,c.frame.texture().clone()));
        if let Some((size,texture))=&shown {
            let rect=crate::mobile_groups::fit(cell,*size);
            self.draw_phone.draw_vars.set_texture(0,texture);
            self.draw_phone.opacity=opacity;
            self.draw_phone.radius=radius*0.5;
            self.draw_phone.y_flip=0.0;
            self.draw_phone.draw_abs(cx,rect);
        }
        self.phone_frames.insert(client,stored);
        shown.is_some()
    }
    fn present_capture(&mut self,cx:&mut Cx2d,capture:&Capture,rect:Rect,opacity:f32,radius:f32) {
        self.draw_phone.draw_vars.set_texture(0,capture.frame.texture());
        self.draw_phone.opacity=opacity;
        self.draw_phone.radius=radius;
        self.draw_phone.y_flip=0.0;
        self.draw_phone.draw_abs(cx,rect);
        self.phone_content(rect);
    }
    /// The live tiles on the home page: each tile client's compact capture,
    /// or a placeholder card while it builds, starts, or has not confirmed
    /// its compact face yet.
    fn draw_home_tiles(&mut self,cx:&mut Cx2d,scope:&mut Scope,screen:Rect) {
        let state=scope.data.get_mut::<WmState>().unwrap();
        let phone=state.phone.clone();
        let style=state.style.target;
        let dark=state.style.dark;
        let opacity=(1.0-phone.openness*0.85) as f32;
        if opacity<0.01 || phone.screen==PhoneScreen::Drawer {return;}
        // The tiles ride page 0 of the home pager (mobile_pages.rs).
        let dx=phone.pages.page_offset(0,screen.size.x);
        if !phone.pages.page_visible(0,screen.size.x) {return;}
        let layout=PhoneSurface::home_layout(style,screen);
        // Everything the placeholders need, read before any tile draws.
        let slots:Vec<(crate::mobile_tiles::TileSlot,Option<ClientId>,String,bool)>=layout.tiles.iter().map(|slot| {
            let client=phone.tiles.client_of(slot.app);
            let (status,connected)=client.and_then(|c|state.clients.get(&c)).map(|s|(s.status.clone(),s.sender.is_some())).unwrap_or_default();
            (*slot,client,status,connected)
        }).collect();
        for (slot,client,status,connected) in slots {
            if matches!(slot.kind,crate::mobile_tiles::TileKind::Group(_)) {continue;}
            let shown_rect=Rect{pos:slot.rect.pos+dvec2(dx,0.0),size:slot.rect.size};
            let gave_up=phone.tiles.gave_up(slot.app);
            let entry=client.and_then(|c|phone.tiles.get(c));
            let mut shown=false;
            if let (Some(client),Some(entry))=(client,entry) {
                let mut stored=self.phone_frames.remove(&client).unwrap_or_default();
                if entry.in_tile_face() {
                    let ready=entry.tile_ready();
                    let mut capture=stored.tile.take().unwrap_or_else(||Capture::new(cx,style,dark));
                    // Not confirmed yet: keep the child driven at the tile
                    // viewport every frame; confirmed: only when it drew.
                    if !ready || self.client_arriving(client) || capture.stale(slot.rect.size,style,dark) {
                        self.record_capture(cx,scope,client,&mut capture,slot.rect,false);
                        capture.settle(slot.rect.size,style,dark);
                    } else {
                        capture.frame.freeze(cx);
                    }
                    if ready {
                        self.present_capture(cx,&capture,shown_rect,opacity,TILE_RADIUS as f32);
                        shown=true;
                    }
                    stored.tile=Some(capture);
                } else if let Some(capture)=stored.tile.as_mut() {
                    // Open, or still animating home: the last compact face
                    // stands in until the client is back in it.
                    capture.frame.freeze(cx);
                    if capture.size==slot.rect.size {
                        self.present_capture(cx,capture,shown_rect,opacity,TILE_RADIUS as f32);
                        shown=true;
                    }
                }
                self.phone_frames.insert(client,stored);
            }
            if !shown {
                let (headline,detail)=if client.is_none() && !gave_up {
                    ("Tap to open", String::new())
                } else { crate::mobile_tiles::placeholder_text(&status,connected,gave_up) };
                self.phone_ui.draw_tile_placeholder(cx,crate::mobile_tiles::TileSlot{rect:shown_rect,..slot},style,dark,opacity,headline,&detail);
                self.phone_content(shown_rect);
            }
        }
    }
    pub(super) fn draw_phone_scene(&mut self,cx:&mut Cx2d,scope:&mut Scope,full:Rect) {
        crate::mobile_perf::frame_boundary(cx.cx);
        let state=scope.data.get_mut::<WmState>().unwrap();
        // The wallpaper fills the desk; the shell lays out inside the
        // platform's safe area (the notch, the system bars): the status bar
        // under the notch, the navigation band above Android's gesture bar.
        let screen=state.phone.insets.inset(full);
        state.phone.viewport=screen;
        // The frame's exclusion zones are rebuilt from what is drawn: cleared
        // once here, then every surface adds its own (the shade's sheet, the
        // split divider, the apps that own their edges, the keyboard).
        state.phone.exclusions.clear();
        if let Some(z)=state.phone.shade.exclusion(screen) {state.phone.exclusions.add(z,[true;4]);}
        state.phone.groups.add_exclusions(state.phone.screen,crate::mobile::app_rect(screen),&mut state.phone.exclusions);
        let owns_edges:Vec<ClientId>=state.clients.iter().filter(|(_,s)|s.owns_edges).map(|(c,_)|*c).collect();
        crate::mobile_pages::sync(&mut state.phone,state.style.target,screen);
        state.phone.order.retain(|c|state.clients.contains_key(c));
        if state.phone.client.is_some_and(|c|!state.clients.contains_key(&c)) {
            state.phone.client=state.phone.order.first().copied();
            if state.phone.client.is_none() {state.phone.navigate(PhoneScreen::Home);}
        }
        // Only windows in the layout join Recents: a tile client launched by
        // the home page stays out until the person opens it.
        for c in state.layout.clients_on(state.layout.active) {
            if !state.phone.order.contains(&c) {state.phone.order.push(c);}
        }
        self.style=state.style.clone();
        self.title_hits.clear();self.zorder.clear();self.minimized.clear();
        self.dock_warps.clear();
        self.phone_frames.retain(|c,_|state.clients.contains_key(c));
        let phone=state.phone.clone();
        // Consume the final settling frame's activity flag. No next-frame
        // callback follows it, so later idle redraws must not inherit it.
        state.phone.draw_active = false;
        let style=state.style.target;
        let dark=state.style.dark;
        let app=mobile::app_rect(screen);
        crate::mobile_perf::trace_phone_frame(&phone);
        // What this frame needs (mobile.rs): the compositor only when a
        // frosted surface samples the scene, the wallpaper and the home
        // page only while an open app does not cover them.
        let plan=phone.scene_plan(style==crate::desktop::DesktopStyle::Ios);
        let perf=crate::mobile_perf::enabled();
        let ch=crate::mobile_perf::channels(cx.cx);
        let mut clock=std::time::Instant::now();
        self.phone_ui.begin();
        // The overlays that frost a still home scene — the shade's sheet,
        // Recents' overview glass, a tile group's window — sample that same
        // scene on every frame of their transition. Keep the scene and its
        // mip textures for the transition: a moving overlay frame is then one
        // quad of the scene plus the overlay itself, and an idle home frame
        // records it so the first moving frame already finds it. On the
        // OnePlus 6 the GPU clock sits at its floor for the first ~120 ms of a
        // gesture, and there a live scene plus its pyramid did not fit a
        // refresh. Other navigation, an open app, a keyboard and the drawer
        // draw live; geometry and appearance changes re-record.
        let overlay=phone.shade.open>0.001 || phone.overview>0.001 || phone.groups.window_visible();
        // With a window open (`openness`) the page is recorded at full
        // opacity and dimmed over the kept scene, so a card zooming under
        // the overview glass still reads the same scene every frame; an
        // app opening or closing with no overlay draws live, as before.
        // The home-up drag on an open app (its window pulling back over the
        // overview glass) is the same still scene, so it is kept as well;
        // its first frame records, since a settled app draws no home page.
        let cache_scene=(matches!(phone.screen,PhoneScreen::Home|PhoneScreen::Recents)
                && (phone.openness<0.001 || phone.overview>0.001)
            || phone.screen==PhoneScreen::App && phone.overview>0.001)
            && phone.keyboard<0.5
            && !(phone.shade.open>0.001 && phone.groups.window_visible())
            && phone.pages.position()==phone.pages.current() as f64;
        let key=(full,screen,cx.current_dpi_factor(),style,dark);
        let moving=phone.draw_active || phone.gesture.is_some();
        let mut cache=if cache_scene {
            Some(self.phone_scene_backdrop.take().unwrap_or_else(|| PhoneSceneBackdrop {
                frame: WindowFrame::new_with_name(cx,"wm_phone_scene_backdrop"),
                key: None, blur: None,
            }))
        } else {
            if let Some(cached)=self.phone_scene_backdrop.as_mut() {cached.key=None;}
            None
        };
        let hit=overlay && moving && cache.as_ref().is_some_and(|c|c.key==Some(key) && c.blur.is_some());
        // Record on idle frames and under an overlay the cache does not fit;
        // a home animation without an overlay (the island, a settling tile)
        // draws live and may change the scene, so it drops the key.
        let record=!hit && cache.is_some() && (!moving || overlay);
        if let Some(cached)=cache.as_mut() {if !hit && !record {cached.key=None;}}
        if perf || crate::mobile_perf::trace_on() {
            crate::mobile_perf::trace_phone_scene(if hit {"hit"} else if record {"record"} else {"live"},
                &format!("cacheable={} overlay={} moving={} keyed={} blur={} screen={:?} openness={:.3} overview={:.3} shade={:.3} group={} pages={:.3}/{}",
                    cache_scene, overlay, moving, cache.as_ref().is_some_and(|c|c.key==Some(key)), cache.as_ref().is_some_and(|c|c.blur.is_some()),
                    phone.screen, phone.openness, phone.overview, phone.shade.open, phone.groups.window_visible(), phone.pages.position(), phone.pages.current()));
        }
        let compose=(plan.compose && !hit) || record;
        self.phone_compose=compose;
        let mut scene_backdrop:Option<GaussBlurSnapshot>=None;
        if hit {
            let cached=cache.as_ref().unwrap();
            scene_backdrop=cached.blur.clone();
            cached.frame.attach(cx);
            // The sheet is opaque: only the rows beside it show the scene.
            // Overlap its edges by its rounded corners and shadow.
            let sheet=(phone.shade.open>0.001 && phone.overview<0.001 && !phone.groups.window_visible())
                .then(||phone.shade.sheet_rect(screen));
            // A settled overview glass is opaque over the whole screen:
            // nothing of the scene under it reaches the display.
            let covered=phone.overview>=0.999 && phone.shade.open<0.001 && !phone.groups.window_visible();
            match sheet {
                _ if covered => {}
                Some(sheet) if sheet.size.y>128.0 => {
                    let top=Rect{pos:full.pos,size:dvec2(full.size.x,(sheet.pos.y+48.0-full.pos.y).max(0.0))};
                    let from=sheet.pos.y+sheet.size.y-48.0;
                    let bottom=Rect{pos:dvec2(full.pos.x,from),size:dvec2(full.size.x,(full.pos.y+full.size.y-from).max(0.0))};
                    self.draw_window_surface_band(cx,&cached.frame,full,top,0.0);
                    self.draw_window_surface_band(cx,&cached.frame,full,bottom,0.0);
                }
                _ => self.draw_window_surface(cx,&cached.frame,full,0.0),
            }
            if phone.openness>0.001 {self.phone_ui.d.solid(cx,screen,crate::shell::alpha(crate::shell::rgb(0,0,0),(0.35*phone.openness) as f32));}
        }
        if record {cache.as_mut().unwrap().frame.begin(cx,full);}
        if compose {self.compositor.get_or_insert_with(||BackdropCompositor::new(cx)).begin(cx);}
        if hit {
            // The scene is the cached quad above.
        } else if plan.wallpaper {
            self.phone_ui.draw_wallpaper(cx,full,style,dark,phone.wallpaper_phase);
        } else {
            // Under an open app only the system-bar strips can show: the
            // status bar's own colour, one flat fill. Under Android's opaque
            // drawer only the strips outside the safe area can: a full-screen
            // fill there was a second full-screen layer under the drawer's own,
            // and with Recents' glass over both the frame missed its vsync
            // (a hold from the drawer ran 38-40 fps, 48-49 without the two).
            let bars=if dark {crate::shell::rgb(24,24,28)}else{crate::shell::rgb(248,248,252)};
            if phone.screen==PhoneScreen::Drawer && style!=crate::desktop::DesktopStyle::Ios {
                let bottom=screen.pos.y+screen.size.y;
                self.phone_ui.d.solid(cx,Rect{pos:full.pos,size:dvec2(full.size.x,(screen.pos.y-full.pos.y).max(0.0))},bars);
                self.phone_ui.d.solid(cx,Rect{pos:dvec2(full.pos.x,bottom),size:dvec2(full.size.x,(full.pos.y+full.size.y-bottom).max(0.0))},bars);
            } else {
                self.phone_ui.d.solid(cx,full,bars);
            }
        }
        if !hit {self.phone_content(full);}
        let home_backdrop=if compose && !hit && style==crate::desktop::DesktopStyle::Ios && phone.openness<0.999 {
            let t=std::time::Instant::now();
            let b=self.compositor.as_mut().unwrap().backdrop(cx,PhoneSurface::home_dock(screen),4.0);
            if perf {crate::mobile_perf::span(cx.cx,ch.glass,t);}
            Some(b)
        }else{None};
        if plan.home && !hit {
            self.phone_ui.draw_home(cx,state,screen,home_backdrop,record);
            self.phone_content(screen);
        }
        if plan.home && !hit && phone.home_visible() {self.draw_home_tiles(cx,scope,screen);}
        if record {
            // The scene is complete: its pyramid, to the deepest level an
            // overlay reads (the group window's 4), then the frame ends and
            // shows in the window. Overlays draw after it, in the window.
            let mut cached=cache.take().unwrap();
            let blur=self.compositor.as_mut().unwrap().finish(cx,screen,Some((screen,4.0))).0;
            cached.frame.end(cx);
            self.draw_window_surface(cx,&cached.frame,full,0.0);
            if phone.openness>0.001 {self.phone_ui.d.solid(cx,screen,crate::shell::alpha(crate::shell::rgb(0,0,0),(0.35*phone.openness) as f32));}
            cached.key=Some(key);
            cached.blur=blur.clone();
            scene_backdrop=blur;
            self.phone_compose=false;
            cache=Some(cached);
        }
        // Kept across frames the scene is not cacheable on: its frame and
        // textures are reused, not re-created, at the next overlay.
        if cache.is_some() {self.phone_scene_backdrop=cache;}
        if perf {crate::mobile_perf::span(cx.cx,ch.home,clock);clock=std::time::Instant::now();}
        if phone.groups.window_visible() {let state=scope.data.get_mut::<WmState>().unwrap();self.draw_group_window(cx,state,screen,scene_backdrop.clone());}
        if perf {crate::mobile_perf::span(cx.cx,ch.groups,clock);clock=std::time::Instant::now();}
        if phone.overview>0.001 {
            // The glass samples level 3 whatever `overview` is; only its
            // opacity follows the transition (mobile_surface.rs).
            self.phone_ui.overview_glass.set_blurriness(cx, 3.0);
            let backdrop=match scene_backdrop.clone() {
                Some(b)=>b,
                None=>self.compositor.as_mut().unwrap().backdrop(cx,screen,3.0),
            };
            self.phone_ui.overview_glass.draw_surface_with_backdrop(cx,screen,Some(backdrop),phone.overview as f32);
            self.phone_content(screen);
        }
        if perf {crate::mobile_perf::span(cx.cx,ch.glass,clock);clock=std::time::Instant::now();}
        let mut excluded:Vec<Rect>=Vec::new();
        let mut order=phone.order.clone();
        order.reverse();
        // Foreground paints last during launch/return transitions.
        if phone.overview<0.001 {
            if let Some(c)=phone.client {order.retain(|i|*i!=c);order.push(c);}
        }
        for client in order {
            let foreground=phone.client==Some(client) || (phone.screen==PhoneScreen::App && phone.groups.in_split(client));
            // In a split each client gets its pane, so it lays out for it.
            let app=phone.groups.pane(client,app);
            if !foreground && phone.overview<0.001 {continue;}
            if foreground && phone.openness<0.001 {continue;}
            let index=phone.order.iter().position(|c|*c==client).unwrap_or(0);
            let card=mobile::card_rect(screen,index as f64,phone.page);
            let mut display=if foreground {
                let icon=PhoneSurface::launch_origin(style,screen,phone.tiles.get(client).map(|t|t.app.as_str()));
                mobile::mix_rect(mobile::mix_rect(icon,app,phone.openness),card,phone.overview)
            }else{card};
            if phone.gesture.as_ref().is_some_and(|g|g.hit==Some(PhoneHit::Card(client))) {display.pos.y+=phone.dismiss_y;}
            if display.pos.x+display.size.x<screen.pos.x || display.pos.x>screen.pos.x+screen.size.x {continue;}
            // A tile client's window frames are trusted only in its full
            // face: while it shows (or switches to) the compact face the
            // card keeps the last full-screen capture, whatever arrives.
            let full_ready=phone.tiles.get(client).map_or(true,|t|t.full_ready());
            let mut stored=self.phone_frames.remove(&client).unwrap_or_default();
            let opacity=if foreground {phone.openness as f32}else{phone.overview as f32};
            let radius=((1.0-phone.openness).max(phone.overview)*26.0)as f32;
            match stored.full.take() {
                Some(mut capture)=>{
                    // The foreground app is re-recorded every scene frame once it is
                    // settled on screen. While it zooms open the last capture is the
                    // animation's source: re-recording a full-screen module (and its
                    // own glass pyramid) under a moving quad cost a whole GPU frame
                    // per animation frame on the phone.
                    let settled=phone.openness>=0.999 && phone.overview<=0.001;
                    let fresh=self.client_arriving(client) || capture.stale(app.size,style,dark);
                    let refresh=full_ready && (foreground && phone.screen==PhoneScreen::App && settled || fresh);
                    // The zoom's last frame is the first settled one. Recording
                    // the module there (67-84 ms on the phone) was the one hitch
                    // of every app opening. Show the zoom's capture at full size
                    // instead, pixel for pixel what a fresh record would show
                    // unless the app changed, and record on the extra idle frame
                    // asked for here, where a long frame moves nothing.
                    if refresh && !fresh && (phone.draw_active || phone.gesture.is_some()) {
                        capture.frame.freeze(cx);
                        cx.redraw_all();
                    } else if refresh {
                        self.record_capture(cx,scope,client,&mut capture,app,true);
                        capture.settle(app.size,style,dark);
                    }else{capture.frame.freeze(cx);}
                    self.present_capture(cx,&capture,display,opacity,radius);
                    stored.full=Some(capture);
                }
                None if full_ready=>{
                    let mut capture=Capture::new(cx,style,dark);
                    self.record_capture(cx,scope,client,&mut capture,app,true);
                    capture.settle(app.size,style,dark);
                    self.present_capture(cx,&capture,display,opacity,radius);
                    stored.full=Some(capture);
                }
                None=>{
                    // Opened straight from its tile and no full-size frame
                    // yet: a plain launch card, never the squeezed tile.
                    let app_id=phone.tiles.get(client).map(|t|t.app.clone()).unwrap_or_default();
                    self.phone_ui.draw_launch_card(cx,display,&app_id,style,dark,opacity,radius);
                    self.phone_content(display);
                }
            }
            self.phone_frames.insert(client,stored);
            if foreground {
                self.zorder.push(client);
                // An app that owns its edges keeps them while it is the one
                // full-screen window the finger can reach.
                if phone.screen==PhoneScreen::App && owns_edges.contains(&client) {excluded.push(display);}
            }
        }
        // The captures' own draws are the `module` channel (record_capture).
        if perf {clock=std::time::Instant::now();}
        let shade_glass=plan.compose && phone.shade.open>0.001;
        let keyboard_glass=if plan.compose && phone.keyboard>0.5 {
            Some((Rect {pos:screen.pos+dvec2(0.0,screen.size.y-phone.keyboard-24.0),size:dvec2(screen.size.x,phone.keyboard)},4.0))
        }else{None};
        // The shade's sheet samples the finished scene, so it is the
        // compositor's final glass: a mid-scene `backdrop` here opened a second
        // segment holding nothing but a full-screen copy of the first, one
        // more full-resolution scene pass per frame of the pull. With the
        // keyboard up (its own final glass) the shade keeps the checkpoint.
        let live=scene_backdrop.is_none();
        let shade_backdrop=if live && shade_glass && keyboard_glass.is_some() {Some(self.compositor.as_mut().unwrap().backdrop(cx,screen,3.0))}else{None};
        let final_glass=if shade_glass && keyboard_glass.is_none() {Some((screen,3.0))} else {keyboard_glass};
        let backdrop=if live && plan.compose {self.compositor.as_mut().unwrap().finish(cx,screen,final_glass).0} else {scene_backdrop};
        if perf {crate::mobile_perf::span(cx.cx,ch.glass,clock);}
        let state=scope.data.get_mut::<WmState>().unwrap();
        for r in excluded {state.phone.exclusions.add(r,[false,false,true,true]);}
        if phone.keyboard>0.5 {
            // The keyboard and the navigation bar under it: a key at the
            // bottom row is a key, never the start of a home swipe.
            let mut kb=PhoneSurface::keyboard_rect(&phone,screen);
            kb.size.y=screen.pos.y+screen.size.y-kb.pos.y;
            state.phone.exclusions.add(kb,[false,true,false,false]);
        }
        // The sheet's recorded content is shown through the desk's own
        // texture quad (mobile_shade.rs keeps the frame, the desk the draw).
        let quad=&mut self.draw_phone;
        self.phone_ui.draw_overlay(cx,state,screen,shade_backdrop.or(backdrop),&mut |cx,texture,r| {
            quad.draw_vars.set_texture(0,texture);
            quad.opacity=1.0; quad.radius=0.0; quad.y_flip=0.0;
            quad.draw_abs(cx,r);
        });
    }
    pub(super) fn handle_phone_event(&mut self,cx:&mut Cx,event:&Event,scope:&mut Scope) {
        let state=scope.data.get_mut::<WmState>().unwrap();
        let input=matches!(event,Event::TouchUpdate(_)|Event::MouseDown(_)|Event::MouseUp(_)|Event::MouseMove(_)|Event::Scroll(_)|Event::KeyDown(_)|Event::KeyUp(_)|Event::TextInput(_));
        let client=state.phone.client;
        if input && !state.phone.accepts_app_input() {return;}
        let items:Vec<_>=self.items.iter().filter(|(c,_)|!input || Some(**c)==client || state.phone.groups.in_split(**c)).map(|(_,w)|w.clone()).collect();
        for item in items {item.handle_event(cx,event,scope);}
    }
}
