//! Phone navigation/input, sharing the WM's real clients and launch paths.
use crate::{mobile::*, mobile_surface::PhoneSurface, mobile_tiles::{self, Face, TILE_APPS}, *};
use crate::mobile_gestures::{Dir, FingerPhase, GestureContext, GestureKind, SafeInsets, ShellGesture};
use makepad_widgets::makepad_platform::ime::{HostedKeyboard, InputMode};
use makepad_widgets::widget_async::{enter_isolate, leave_isolate};

#[derive(Clone, Copy, PartialEq, Eq)]
enum PhonePointerPhase { Down, Move, Up, Scroll }

impl App {
    pub(super) fn animate_phone(&mut self,cx:&mut Cx) {
        self.phone_frame=cx.new_next_frame();
        self.redraw_all(cx);
    }

    // --------------------------------------------------------------
    // Home tiles (mobile_tiles.rs)
    //
    // Clock, Weather and Photos live on the home page as compact faces of
    // the SAME client that opens full screen: one process (or one module
    // isolate), one model, one window. The home page launches a missing
    // one through the ordinary cargo path, keeps it out of the layout until
    // the person opens it, and tells every tile client which face to show.
    // --------------------------------------------------------------

    /// The compact viewport of `app`'s tile on the current phone screen.
    fn tile_viewport(&mut self, app: &str) -> Option<Vec2d> {
        let state = self.state_mut();
        let screen = state.phone.viewport;
        if screen.size.x < 1.0 || screen.size.y < 1.0 { return None; }
        PhoneSurface::home_layout(state.style.target, screen).tiles.into_iter().find(|s| s.app == app).map(|s| s.rect.size)
    }
    /// The full viewport an open app gets on this phone screen.
    fn full_viewport(&mut self) -> Vec2d {
        let screen = self.state_mut().phone.viewport;
        if screen.size.x < 1.0 { return dvec2(0.0, 0.0); }
        app_rect(screen).size
    }
    /// Bind or launch a client per tile app, then send every tile client
    /// the face it should be showing. Cheap when nothing changed; called
    /// from the phone's animation frames and after every phone action.
    pub(super) fn sync_home_tiles(&mut self, cx: &mut Cx) {
        if !self.state.as_ref().is_some_and(|s| s.style.target.mobile()) { return; }
        let alive: Vec<ClientId> = self.state_mut().clients.keys().copied().collect();
        self.state_mut().phone.tiles.retain_clients(|c| alive.contains(&c));
        let now = host::now();
        let home_visible = self.state_mut().phone.home_visible();
        for (app, _) in TILE_APPS {
            if self.state_mut().phone.tiles.client_of(app).is_some() { continue; }
            // A window of this app the person already has: its tile is one
            // more face of that window, never a second instance.
            let existing = self.state_mut().clients.iter()
                .filter(|(_, s)| s.app == app && !s.warm && !s.pane && !s.is_preview && s.closing.is_none())
                .map(|(c, _)| *c).min();
            if let Some(client) = existing {
                self.state_mut().phone.tiles.bind(app, client, false);
            } else if self.warm_pool.enabled() && home_visible && self.state_mut().phone.tiles.may_launch(app, now) {
                self.launch_tile_client(cx, app);
            }
        }
        let (foreground, settled) = {
            let phone = &self.state_mut().phone;
            (phone.foreground(), phone.home_settled())
        };
        let full = self.full_viewport();
        let plan: Vec<(ClientId, Face, Vec2d)> = self.state_mut().phone.tiles.clients()
            .map(|t| (t.client, t.app.clone(), t.background))
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|(client, app, background)| {
                let face = mobile_tiles::wanted_face(client, foreground, settled, background)?;
                let viewport = match face { Face::Full => full, Face::Tile => self.tile_viewport(&app)? };
                if viewport.x < 1.0 || viewport.y < 1.0 { return None; }
                self.state_mut().phone.tiles.pending(client, face, viewport).then_some((client, face, viewport))
            })
            .collect();
        for (client, face, viewport) in plan {
            self.send_face(cx, client, face, viewport);
        }
    }
    /// Tell one client which face to show, with the viewport that face
    /// gets, in ONE batch so the client never draws the new face at the
    /// old size. A process that has not announced its window yet is left
    /// for `replay_tile_face`; a module hears it in its own isolate.
    fn send_face(&mut self, cx: &mut Cx, client: ClientId, face: Face, viewport: Vec2d) -> bool {
        let json = face.mode().to_json();
        if self.module_host.is_module(client) {
            if let Some((root, vm_id)) = self.module_host.get(client).map(|i| (i.root.clone(), i.vm_id)) {
                let entry = enter_isolate(cx, vm_id);
                root.handle_event(cx, &Event::Custom(json), &mut Scope::empty());
                leave_isolate(cx, entry);
            }
            // A module draws inside the desk's own capture at exactly the
            // rect asked for: the face is confirmed the moment it is sent.
            let tiles = &mut self.state_mut().phone.tiles;
            tiles.note_sent(client, face, viewport);
            tiles.note_frame(client, viewport);
            self.animate_phone(cx);
            return true;
        }
        let dpi = if self.dpi_factor > 0.0 { self.dpi_factor } else { 1.0 };
        let Some((sender, window_id)) = self.state_mut().clients.get(&client)
            .filter(|s| s.ready)
            .and_then(|s| s.sender.clone().map(|sender| (sender, s.window_id)))
        else { return false };
        let mut msgs = Vec::new();
        if viewport.x >= 1.0 && viewport.y >= 1.0 {
            msgs.push(StudioToApp::WindowGeomChange {
                window_id, dpi_factor: dpi, left: 0.0, top: 0.0, width: viewport.x, height: viewport.y,
            });
        }
        msgs.push(StudioToApp::Custom(json));
        send_to_app(&sender, msgs);
        self.state_mut().phone.tiles.note_sent(client, face, viewport);
        self.animate_phone(cx);
        true
    }
    /// CreateWindow arrived for a tile client: it gets its face before its
    /// first frame, so a tile never opens on a full-screen layout.
    pub(super) fn replay_tile_face(&mut self, cx: &mut Cx, client: ClientId) {
        let Some((app, background)) = self.state_mut().phone.tiles.get(client).map(|t| (t.app.clone(), t.background)) else { return };
        if !self.state_mut().style.target.mobile() {
            // On the desktop every window is its full self; the desk's
            // own tile hands it its geometry.
            if self.state_mut().phone.tiles.face_of(client) != Some(Face::Full) {
                self.send_face(cx, client, Face::Full, dvec2(0.0, 0.0));
            }
            return;
        }
        let (foreground, settled) = {
            let phone = &self.state_mut().phone;
            (phone.foreground(), phone.home_settled())
        };
        let face = mobile_tiles::wanted_face(client, foreground, settled, background).unwrap_or(Face::Full);
        let viewport = match face {
            Face::Full => self.full_viewport(),
            Face::Tile => self.tile_viewport(&app).unwrap_or(dvec2(0.0, 0.0)),
        };
        self.send_face(cx, client, face, viewport);
    }
    /// A frame from `client` landed: which face does it belong to? Tile
    /// clients answer by size (a stale frame from the previous viewport
    /// belongs to neither capture); every other client is in its full face.
    pub(super) fn note_client_frame_face(&mut self, client: ClientId, width: u32, height: u32) -> Option<Face> {
        let tiles = &mut self.state_mut().phone.tiles;
        if !tiles.is_tile_client(client) { return Some(Face::Full); }
        let dpi = if self.dpi_factor > 0.0 { self.dpi_factor } else { 1.0 };
        let size = dvec2(width as f64 / dpi, height as f64 / dpi);
        let tiles = &mut self.state_mut().phone.tiles;
        let face = tiles.note_frame(client, size);
        if face.is_some() {
            if let Some(app) = tiles.get(client).map(|t| t.app.clone()) { tiles.note_healthy(&app); }
        }
        face
    }
    /// The person opened a tile client that was only ever a tile: seat it
    /// in the layout like any launch. Its face follows through
    /// `sync_home_tiles` once the phone state says it is in front.
    pub(super) fn promote_tile_client(&mut self, cx: &mut Cx, client: ClientId) {
        if !self.state_mut().phone.tiles.promote(client) { return; }
        let area = self.desk_area(cx);
        let state = self.state_mut();
        let gap = state.gap;
        state.layout.insert(client, area, gap);
        log!("wm: home tile client {} opened as a window", client);
        self.update_bar(cx);
    }
    /// Leaving the phone: every tile client goes back to its full face;
    /// the tile it drew for is gone with the home page.
    pub(super) fn restore_tile_faces(&mut self, cx: &mut Cx) {
        let clients: Vec<ClientId> = self.state_mut().phone.tiles.clients().filter(|t| t.in_tile_face()).map(|t| t.client).collect();
        for client in clients {
            self.send_face(cx, client, Face::Full, dvec2(0.0, 0.0));
        }
    }
    /// Start `app_id` for its home tile: the same cargo launch (or module
    /// isolate) an ordinary open uses, minus the layout seat and the focus.
    fn launch_tile_client(&mut self, cx: &mut Cx, app_id: &str) {
        self.state_mut().phone.tiles.note_launch(app_id, host::now());
        let Some(app) = crate::clients::find_app(app_id) else { return };
        if self.apps.hosting(app_id) == Hosting::Module {
            if let Some(module) = self.apps.module(app_id) { self.launch_tile_module(cx, module); }
            return;
        }
        if !host::processes_available() { return; }
        let hub_port = self.state_mut().hub_port;
        if hub_port == 0 { return; }
        let id = self.next_id;
        self.next_id += 1;
        let lines = self.line_sender();
        let pool = cx.task_pool();
        match spawn_client(&pool, &cx.thread_spawner(), &app, id, hub_port, None, None, &[], false, lines) {
            Ok(slot) => {
                self.state_mut().clients.insert(id, slot);
                // The tile widget exists from now on: its ticks drive the
                // child and its captures show the build on the home page.
                self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| d.with_run_view(cx, id, |cx, v| v.set_run_target(cx, id, 0, hub_port)));
                self.state_mut().phone.tiles.bind(app_id, id, true);
                log!("wm: launched {} as client {} for its home tile", app.id, id);
                self.animate_phone(cx);
            }
            Err(err) => log!("wm: home tile launch of {} failed: {}", app.id, err),
        }
    }
    fn launch_tile_module(&mut self, cx: &mut Cx, module: &'static dyn AppModule) {
        let open = match module.open_schema().empty_open() {
            Ok(open) => open,
            Err(e) => { log!("wm: {} cannot open without arguments: {}", module.id(), e); return; }
        };
        let id = self.next_id;
        self.next_id += 1;
        let viewport = self.tile_viewport(module.id()).unwrap_or_else(|| { let a = self.desk_area(cx); dvec2(a.w, a.h) });
        if let Err(e) = self.module_host.create(cx, id, module, open, viewport) {
            log!("wm: module {} failed to start for its home tile: {}", module.id(), e);
            return;
        }
        let Some((manifest, root, vm_id)) = self.module_host.get(id).map(|i| (i.manifest(), i.root.clone(), i.vm_id)) else { return };
        self.state_mut().clients.insert(id, clients::ClientSlot::module(id, module.id(), module.label()));
        self.desk(cx).borrow_mut::<WmDesk>().map(|mut d| {
            d.mark_module(id);
            d.with_module_view(cx, id, |cx, v| v.set_root(cx, id, vm_id, root));
        });
        // On the bus like any instance: opening it later changes nothing
        // the assistant can see except the window.
        if self.apps.pane_in_process() {
            self.pane_links.open_instance(cx, id, manifest);
        } else {
            let frame = self.ai_bus.register_local(id, manifest);
            self.send_to_pane(frame);
        }
        self.state_mut().phone.tiles.bind(module.id(), id, true);
        log!("wm: launched {} as client {} for its home tile (in-process)", module.id(), id);
        self.animate_phone(cx);
    }
    pub(super) fn configure_phone_mode(&mut self,cx:&mut Cx,previous:desktop::DesktopStyle,style:desktop::DesktopStyle) {
        let window=self.ui.window(cx,ids!(main_window));
        if style.mobile() && !previous.mobile() {
            let size=window.get_inner_size(cx);
            let focused=self.state_mut().layout.focused_client();
            let desktop_clients=self.state_mut().layout.all_clients();
            let phone=&mut self.state_mut().phone;
            phone.desktop_size=Some(size);phone.desktop_style=previous;
            phone.desktop_clients=desktop_clients;
            phone.client=focused;phone.navigate(PhoneScreen::Home);
            phone.openness=0.0;phone.overview=0.0;
            if !cfg!(any(target_os="ios",target_os="android")) {window.resize(cx,phone_size(style));}
        }else if !style.mobile() && previous.mobile() {
            self.dismiss_phone_keyboard(cx);
            self.restore_tile_faces(cx);
            let size=self.state_mut().phone.desktop_size.take().unwrap_or(dvec2(1400.0,900.0));
            if !cfg!(any(target_os="ios",target_os="android")) {window.resize(cx,size);}
            // Apps first opened on a phone have no desktop restore geometry.
            // Give them normal desktop windows; retain pre-phone user geometry.
            let state=self.state_mut();
            let retained=std::mem::take(&mut state.phone.desktop_clients);
            state.layout.desktop.windows.retain(|w|retained.contains(&w.client));
            let area=LRect::new(0.0,36.0,size.x,(size.y-90.0).max(1.0));
            for client in state.layout.all_clients() {state.layout.desktop.ensure(client,area);}
        }else if style.mobile() && previous!=style {
            let current=window.get_inner_size(cx);
            let size=phone_size(style);
            if !cfg!(any(target_os="ios",target_os="android")) {
                window.resize(cx,if current.x>current.y {dvec2(size.y,size.x)}else{size});
            }
        }
        self.ui.widget(cx,ids!(desktop_controls)).set_visible(cx,!style.mobile());
        self.ui.widget(cx,ids!(phone_controls)).set_visible(cx,style.mobile());
        self.ui.widget(cx,ids!(shell_ai_pane)).set_visible(cx,!style.mobile());
        self.phone_time=0.0;
        self.animate_phone(cx);
    }
    pub(super) fn phone_toolbar_hit(&self,cx:&Cx,p:Vec2d)->Option<PhoneHit> {
        if !self.state.as_ref().is_some_and(|s|s.style.target.mobile()) {return None;}
        self.ui.widget(cx,ids!(phone_controls)).borrow::<PhoneSurface>().and_then(|s|s.hit(p))
    }
    pub(super) fn phone_animation_event(&mut self,cx:&mut Cx,event:&Event) {
        if let Some(frame)=self.phone_frame.is_event(event) {
            if self.state.as_ref().is_some_and(|s|s.style.target.mobile()) {
                let dt=if self.phone_time==0.0 {1.0/60.0}else{(frame.time-self.phone_time).clamp(0.001,0.05)};
                self.phone_time=frame.time;
                let Some(state) = self.state.as_mut() else { return };
                let phone = &mut state.phone;
                phone.wallpaper_time = frame.time;
                let moving = phone.step(dt);
                // A finger resting near the top of a home swipe becomes the
                // switcher without moving; the frame keeps running while
                // the recognizer owns a finger so the hold can land.
                let mut tracking = false;
                if self.phone_gestures.active() {
                    tracking = true;
                    if let Some(out) = self.phone_gestures.tick(frame.time) {
                        log!("wm: gesture {:?}", out);
                        let from = phone.gesture.as_ref().map(|g| g.screen).unwrap_or(phone.screen);
                        phone.gesture_out = Some(out);
                        Self::drive_gesture(phone, out, from);
                    }
                } else if matches!(phone.gesture_out, Some(ShellGesture::Commit(_) | ShellGesture::Cancel(_))) {
                    // Commit/Cancel stay for exactly one drawn frame.
                    if self.gesture_out_age >= 1 { phone.gesture_out = None; } else { self.gesture_out_age += 1; tracking = true; }
                }
                let wallpaper_visible = phone.screen != PhoneScreen::App || phone.openness < 0.999 || phone.overview > 0.001;
                if moving || wallpaper_visible || tracking {self.phone_frame=cx.new_next_frame();}
                // The tiles follow the phone state every frame: a window
                // takes its compact face only once its dismissal settled.
                self.sync_home_tiles(cx);
                self.redraw_all(cx);
            }
        }
    }
    pub(super) fn sync_phone_keyboard(&mut self,cx:&mut Cx) {
        if !self.state.as_ref().is_some_and(|s|s.style.target.mobile()) {return;}
        let client=self.state_mut().phone.foreground();
        if let Some(client)=client.filter(|c|self.module_host.is_module(*c)) {
            self.state_mut().phone.ime.insert(client,cx.hosted_ime_state());
        }
        let phone=&mut self.state_mut().phone;
        let visible=!cfg!(any(target_os="ios",target_os="android"))
            && ((phone.screen==PhoneScreen::Drawer && phone.search_focused)
                || client.and_then(|c|phone.ime.get(&c)).is_some_and(|ime|ime.visible));
        let height=if visible {phone.keyboard_height()}else{0.0};
        if height==phone.keyboard_sent_height && (height==0.0 || phone.keyboard_client==client) {return;}
        let old=phone.keyboard_client;
        phone.keyboard_target=height;phone.keyboard_sent_height=height;phone.keyboard_client=client;
        if visible {
            let ime=client.and_then(|c|phone.ime.get(&c)).copied().unwrap_or_default();
            phone.symbols=matches!(ime.input_mode,InputMode::Numeric|InputMode::Decimal|InputMode::Tel);
        }
        if old!=client {if let Some(old)=old {self.send_phone_keyboard(cx,old,0.0,false);}}
        if let Some(client)=client {self.send_phone_keyboard(cx,client,height,false);}
        self.animate_phone(cx);
    }
    fn send_phone_keyboard(&mut self,cx:&mut Cx,client:ClientId,height:f64,dismiss:bool) {
        let keyboard=HostedKeyboard {height,dismiss};
        if self.module_host.is_module(client) {
            if dismiss {cx.text_ime_was_dismissed();}
            if let Some((root,vm_id))=self.module_host.get(client).map(|i|(i.root.clone(),i.vm_id)) {
                let time=cx.seconds_since_app_start();
                let event=if height>0.0 {VirtualKeyboardEvent::WillShow{time,height,duration:0.22,ease:makepad_platform::event::Ease::OutCubic}}
                    else{VirtualKeyboardEvent::WillHide{time,height:0.0,duration:0.22,ease:makepad_platform::event::Ease::OutCubic}};
                let entry=enter_isolate(cx,vm_id);
                root.handle_event(cx,&Event::VirtualKeyboard(event),&mut Scope::empty());
                leave_isolate(cx,entry);
            }
        }else if let Some(sender)=self.state_mut().clients.get(&client).and_then(|s|s.sender.as_ref()) {
            send_to_app(sender,vec![StudioToApp::Custom(keyboard.to_json())]);
        }
    }
    fn dismiss_phone_keyboard(&mut self,cx:&mut Cx) {
        if self.state_mut().phone.search_focused {
            if let Some(mut desk)=self.desk(cx).borrow_mut::<WmDesk>() {
                desk.dismiss_phone_search(cx,&mut self.state_mut().phone,false);
            }
        }
        if let Some(client)=self.state_mut().phone.keyboard_client {
            self.send_phone_keyboard(cx,client,0.0,true);
            if let Some(ime)=self.state_mut().phone.ime.get_mut(&client) {ime.visible=false;}
        }
        self.state_mut().phone.keyboard_target=0.0;
        self.animate_phone(cx);
    }
    fn phone_action(&mut self,cx:&mut Cx,hit:PhoneHit) {
        match hit {
            PhoneHit::App(app)=>{
                // A running window of the app, a home tile's own client
                // included: the same client opens, never a second one.
                let existing=self.state_mut().clients.iter().filter(|(_,slot)|slot.app==app && !slot.warm && !slot.pane && !slot.is_preview && slot.closing.is_none()).map(|(c,_)|*c).min();
                if let Some(client)=existing {self.activate_client(cx,client);}
                else {self.launch_app(cx,&app);}
            },
            PhoneHit::Card(client)=>self.activate_client(cx,client),
            PhoneHit::Home=>self.state_mut().phone.navigate(PhoneScreen::Home),
            PhoneHit::Recents=>self.state_mut().phone.navigate(PhoneScreen::Recents),
            PhoneHit::Drawer=>self.state_mut().phone.navigate(PhoneScreen::Drawer),
            PhoneHit::Rotate=>{
                let window=self.ui.window(cx,ids!(main_window));let size=window.get_inner_size(cx);
                self.state_mut().phone.gesture=None;
                self.state_mut().phone.touch=None;
                self.phone_gestures.cancel();
                self.state_mut().phone.gesture_out=None;
                window.resize(cx,dvec2(size.y,size.x));
            }
            PhoneHit::Style=>self.open_style_menu(cx),
            PhoneHit::Appearance=>{
                let focused=self.state_mut().phone.search_focused;
                self.toggle_desktop_appearance(cx);
                if focused {
                    if let Some(mut desk)=self.desk(cx).borrow_mut::<WmDesk>() {desk.focus_phone_search(cx,&mut self.state_mut().phone);}
                }
            }
            PhoneHit::Desktop=>{let style=self.state_mut().phone.desktop_style;self.set_desktop_style(cx,style);}
            PhoneHit::HideKeyboard=>self.dismiss_phone_keyboard(cx),
            PhoneHit::ClearSearch=>{
                if let Some(mut desk)=self.desk(cx).borrow_mut::<WmDesk>() {desk.clear_phone_search(cx,&mut self.state_mut().phone);}
            }
            PhoneHit::CancelSearch=>{
                if let Some(mut desk)=self.desk(cx).borrow_mut::<WmDesk>() {desk.dismiss_phone_search(cx,&mut self.state_mut().phone,true);}
            }
            PhoneHit::Shift=>{let p=&mut self.state_mut().phone;p.shift=!p.shift;}
            PhoneHit::Symbols=>{let p=&mut self.state_mut().phone;p.symbols=!p.symbols;}
            PhoneHit::Key(key)=>self.type_phone_key(cx,&key),
            PhoneHit::Back=>{
                if self.state_mut().phone.keyboard_target>0.0 {self.dismiss_phone_keyboard(cx);}
                else {self.phone_back(cx);}
            }
            PhoneHit::Shade(hit)=>self.state_mut().phone.shade.tap(hit),
        }
        self.sync_phone_keyboard(cx);
        self.sync_home_tiles(cx);
        self.animate_phone(cx);
    }
    fn phone_back(&mut self,cx:&mut Cx) {
        if self.state_mut().phone.screen != PhoneScreen::App {
            self.state_mut().phone.navigate(PhoneScreen::Home);
            return;
        }
        let Some(client)=self.state_mut().phone.client else{return};
        if let Some((root,vm_id))=self.module_host.get(client).map(|i|(i.root.clone(),i.vm_id)) {
            let event=Event::BackPressed{handled:std::cell::Cell::new(false)};
            let entry=enter_isolate(cx,vm_id);
            root.handle_event(cx,&event,&mut Scope::empty());
            leave_isolate(cx,entry);
            if matches!(event,Event::BackPressed{handled} if !handled.get()) {self.state_mut().phone.navigate(PhoneScreen::Home);}
        }else if let Some(sender)=self.state_mut().clients.get(&client).and_then(|s|s.sender.as_ref()) {
            send_to_app(sender,vec![StudioToApp::Custom(makepad_platform::ime::HostedBack::default().to_json())]);
        }
    }
    fn type_phone_key(&mut self,cx:&mut Cx,key:&str) {
        let event=match key {
            "backspace"=>Event::KeyDown(KeyEvent{key_code:KeyCode::Backspace,..Default::default()}),
            "return"=>Event::KeyDown(KeyEvent{key_code:KeyCode::ReturnKey,..Default::default()}),
            _=>Event::TextInput(TextInputEvent{input:key.into(),..Default::default()}),
        };
        if self.state_mut().phone.search_focused {
            self.phone_search_event(cx,&event);
            if let Event::KeyDown(key)=event {self.phone_search_event(cx,&Event::KeyUp(key));}
            if key.chars().count()==1 {self.state_mut().phone.shift=false;}
            return;
        }
        let Some(client)=self.state_mut().phone.foreground() else{return};
        if self.module_host.is_module(client) {
            if let Some((root,vm_id))=self.module_host.get(client).map(|i|(i.root.clone(),i.vm_id)) {
                let entry=enter_isolate(cx,vm_id);
                root.handle_event(cx,&event,&mut Scope::empty());
                if let Event::KeyDown(key)=event {root.handle_event(cx,&Event::KeyUp(key),&mut Scope::empty());}
                leave_isolate(cx,entry);
            }
        }else if let Some(sender)=self.state_mut().clients.get(&client).and_then(|s|s.sender.as_ref()) {
            match event {
                Event::KeyDown(key)=>send_to_app(sender,vec![StudioToApp::KeyDown(key.clone()),StudioToApp::KeyUp(key)]),
                Event::TextInput(text)=>send_to_app(sender,vec![StudioToApp::TextInput(text)]),
                _=>{}
            }
        }
        if key.chars().count()==1 {self.state_mut().phone.shift=false;}
    }
    pub(super) fn phone_key(&mut self,cx:&mut Cx,e:&KeyEvent)->bool {
        if !self.state_mut().style.target.mobile() {return false;}
        if e.key_code==KeyCode::Escape {self.phone_action(cx,PhoneHit::Back);return true;}
        if e.key_code==KeyCode::Tab && e.modifiers.alt {self.phone_action(cx,PhoneHit::Recents);return true;}
        !self.state_mut().phone.accepts_app_input()
    }
    pub(super) fn phone_search_event(&mut self,cx:&mut Cx,event:&Event)->bool {
        let Some(state)=self.state.as_ref() else{return false};
        if matches!(event,Event::KeyDown(_)|Event::KeyUp(_)|Event::TextInput(_)|Event::TextCopy(_)|Event::TextCut(_))
            && self.ui.widget(cx,ids!(shell_menu)).borrow::<ShellMenu>().is_some_and(|menu|menu.is_open()) {return false;}
        let old=(state.phone.search_focused,state.phone.search_query.clone());
        let handled=if let Some(mut desk)=self.desk(cx).borrow_mut::<WmDesk>() {
            desk.phone_search_event(cx,event,self.state_mut())
        }else{false};
        let phone=&self.state_mut().phone;
        if old!=(phone.search_focused,phone.search_query.clone()) {
            self.sync_phone_keyboard(cx);
            self.animate_phone(cx);
        }
        handled
    }
    pub(super) fn phone_pointer(&mut self,cx:&mut Cx,event:&Event)->bool {
        if !self.state_mut().style.target.mobile() {return false;}
        if let Event::TouchUpdate(update) = event {
            use makepad_platform::event::TouchState;
            let owned = self.state_mut().phone.touch;
            let point = if let Some(uid) = owned {
                update.touches.iter().find(|p| p.uid == uid)
            } else {
                update.touches.iter().find(|p| p.state == TouchState::Start)
            };
            let Some(point) = point else { return owned.is_some() || !self.state_mut().phone.accepts_app_input(); };
            let phase = match point.state {
                TouchState::Start => PhonePointerPhase::Down,
                TouchState::Move => PhonePointerPhase::Move,
                TouchState::Stop => PhonePointerPhase::Up,
                TouchState::Stable => return owned.is_some() || !self.state_mut().phone.accepts_app_input(),
            };
            let handled = self.phone_pointer_at(cx, phase, point.abs, point.time, true, 0.0);
            if point.state == TouchState::Start && handled && self.state_mut().phone.gesture.is_some() {
                self.state_mut().phone.touch = Some(point.uid);
            }
            if point.state == TouchState::Stop { self.state_mut().phone.touch = None; }
            return handled || owned.is_some();
        }
        let (phase, p, time, primary, scroll) = match event {
            Event::MouseDown(e) => (PhonePointerPhase::Down, e.abs, e.time, e.button.contains(MouseButton::PRIMARY), 0.0),
            Event::MouseMove(e) => (PhonePointerPhase::Move, e.abs, e.time, true, 0.0),
            Event::MouseUp(e) => (PhonePointerPhase::Up, e.abs, e.time, e.button.contains(MouseButton::PRIMARY), 0.0),
            Event::Scroll(e) => (PhonePointerPhase::Scroll, e.abs, e.time, true, e.scroll.y),
            _ => return false,
        };
        self.phone_pointer_at(cx, phase, p, time, primary, scroll)
    }
    /// What the recognizer needs to know about the screen right now.
    fn gesture_context(&mut self, cx: &Cx) -> GestureContext {
        let phone = &self.state_mut().phone;
        let i = cx.display_context.safe_area_insets;
        GestureContext {
            screen: phone.viewport,
            insets: SafeInsets { top: i.top, right: i.right, bottom: i.bottom, left: i.left },
            phone: phone.screen,
        }
    }
    /// The recognizer's in-progress gesture moves what the shell draws
    /// itself: the window pulling back on a home swipe, the predictive
    /// back preview. Every other surface reads `gesture_out` on its own.
    fn drive_gesture(phone: &mut PhoneState, out: ShellGesture, from: PhoneScreen) {
        match out {
            ShellGesture::HomeUp { progress, held } if from != PhoneScreen::Home => {
                phone.openness = 1.0;
                phone.overview = if held { 1.0 } else { progress * 0.6 };
            }
            ShellGesture::Back { progress, .. } if from == PhoneScreen::App => {
                phone.openness = (1.0 - progress * 0.18).clamp(0.4, 1.0);
            }
            _ => {}
        }
    }
    /// A committed gesture becomes the navigation the shell already has.
    /// ShadePull, PageSwipe and HomeSearch commits only reach `gesture_out`:
    /// the shade, the pages and search are their own surfaces' work.
    fn commit_gesture(&mut self, cx: &mut Cx, kind: GestureKind, from: PhoneScreen) {
        match kind {
            GestureKind::HomeUp => {
                let android = self.state_mut().style.target == desktop::DesktopStyle::Android;
                let target = if from == PhoneScreen::Home && android { PhoneHit::Drawer } else { PhoneHit::Home };
                self.phone_action(cx, target);
            }
            GestureKind::Switcher => self.phone_action(cx, PhoneHit::Recents),
            GestureKind::QuickSwitch(dir) => {
                // `order` is most-recent first: right brings back the app
                // used before this one, left cycles the other way round.
                let order = &self.state_mut().phone.order;
                let next = match dir {
                    Dir::Right => order.get(1).copied(),
                    Dir::Left => order.last().copied().filter(|_| order.len() > 1),
                };
                if let Some(client) = next { self.phone_action(cx, PhoneHit::Card(client)); }
            }
            // The app sees the back press first (BackPressed / HostedBack)
            // and the shell goes home only when it declines.
            GestureKind::Back => self.phone_action(cx, PhoneHit::Back),
            GestureKind::Shade(_) | GestureKind::Page(_) | GestureKind::HomeSearch => {}
        }
    }
    fn phone_pointer_at(&mut self, cx: &mut Cx, phase: PhonePointerPhase, p: Vec2d, time: f64, primary: bool, scroll: f64) -> bool {
        if self.state_mut().phone.gesture.is_none() {
            if let Some(hit) = self.phone_toolbar_hit(cx, p) {
                if phase == PhonePointerPhase::Down && primary { self.phone_action(cx, hit); }
                return true;
            }
        }
        let (hit,search_scroll_max)=self.desk(cx).borrow::<WmDesk>().map(|d|(d.phone_hit(p),d.phone_search_scroll_max())).unwrap_or_default();
        let ctx=self.gesture_context(cx);
        let Some(state)=self.state.as_mut() else {return false};
        let phone=&mut state.phone;
        let screen=phone.viewport;
        match phase {
            PhonePointerPhase::Down=>{
                if !primary {return phone.screen!=PhoneScreen::App;}
                let old=phone.screen;
                // The recognizer claims a finger in a band (or on the home
                // page body); an excluded edge is left to the app.
                self.phone_gestures.feed(FingerPhase::Down,p,time,&ctx,&phone.exclusions);
                let shell=self.phone_gestures.active();
                if !shell && !screen.contains(p) {return false;}
                if shell || hit.is_some() || old!=PhoneScreen::App {
                    phone.gesture=Some(PhoneGesture{start:p,last:p,time,hit,shell,screen:old});
                    phone.gesture_out=None;
                    self.redraw_all(cx);
                    return true;
                }
                false
            }
            PhonePointerPhase::Move=>{
                let Some(g)=phone.gesture.as_mut() else{return phone.screen!=PhoneScreen::App;};
                let delta=p-g.start;let last=p-g.last;g.last=p;
                let (shell,from)=(g.shell,g.screen);
                let shade_hit=if let Some(PhoneHit::Shade(h))=&g.hit {Some(h.clone())} else {None};
                if let Some(h)=shade_hit {phone.shade.drag(&h,p,delta,screen);self.animate_phone(cx);return true;}
                let out=if shell {self.phone_gestures.feed(FingerPhase::Move,p,time,&ctx,&phone.exclusions)} else {None};
                phone.gesture_out=out;
                if let Some(out)=out {
                    Self::drive_gesture(phone,out,from);
                }else if from==PhoneScreen::Drawer && (phone.search_focused || !phone.search_query.is_empty()) {
                    phone.search_scroll=(phone.search_scroll.min(search_scroll_max)-last.y).clamp(0.0,search_scroll_max);
                }else if from==PhoneScreen::Recents && !shell {
                    if delta.y.abs()>delta.x.abs()*1.2 {phone.dismiss_y=delta.y.min(0.0);}
                    else {let width=card_rect(screen,0.0,0.0).size.x+22.0;phone.page=(phone.page-last.x/width).clamp(-0.25,phone.order.len().saturating_sub(1)as f64+0.25);}
                }
                self.animate_phone(cx);true
            }
            PhonePointerPhase::Up=>{
                let Some(g)=phone.gesture.take() else{return phone.screen!=PhoneScreen::App;};
                let delta=p-g.start;
                if let (Some(PhoneHit::Shade(h)),true)=(&g.hit,delta.length()>=12.0) {phone.shade.release(h,delta,time-g.time);self.animate_phone(cx);return true;}
                let out=if g.shell {self.phone_gestures.feed(FingerPhase::Up,p,time,&ctx,&phone.exclusions)} else {None};
                phone.gesture_out=out;
                self.gesture_out_age=0;
                match out {
                    Some(ShellGesture::Commit(kind))=>{
                        log!("wm: gesture commit {:?} from {:?}",kind,g.screen);
                        self.commit_gesture(cx,kind,g.screen);
                    }
                    Some(ShellGesture::Cancel(kind))=>{
                        log!("wm: gesture cancel {:?}",kind);
                    }
                    _=>{
                        if g.screen==PhoneScreen::Recents && !g.shell && delta.y < -90.0 && delta.y.abs()>delta.x.abs()*1.2 {
                            if let Some(PhoneHit::Card(client))=g.hit {self.request_close(cx,client);self.state_mut().phone.navigate(PhoneScreen::Recents);}
                        }else if delta.length()<12.0 {
                            if let Some(hit)=g.hit.filter(|h|Some(h)==hit.as_ref()) {self.phone_action(cx,hit);}
                        }else if g.screen==PhoneScreen::Home && delta.y < -55.0 && delta.y.abs()>delta.x.abs() {self.phone_action(cx,PhoneHit::Drawer);}
                    }
                }
                self.state_mut().phone.dismiss_y=0.0;
                self.animate_phone(cx);true
            }
            PhonePointerPhase::Scroll if phone.screen==PhoneScreen::Recents=>{
                phone.page=(phone.page+scroll.signum()).clamp(0.0,phone.order.len().saturating_sub(1)as f64);
                self.animate_phone(cx);true
            }
            PhonePointerPhase::Scroll if phone.searching()=>{
                phone.search_scroll=(phone.search_scroll.min(search_scroll_max)+scroll).clamp(0.0,search_scroll_max);
                self.animate_phone(cx);true
            }
            _=>phone.screen!=PhoneScreen::App,
        }
    }
}
